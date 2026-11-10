use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs, io,
    io::Write,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

pub mod automation;
pub mod blocks;
pub use blocks::BlockDefinition;
pub mod building;
pub mod cam;
pub mod catalog;
pub mod civil;
pub mod command;
pub mod dimension;
pub mod drafting;
pub mod dxf;
pub mod edit;
pub mod mesh;
pub mod plot;
pub mod review;
pub mod scene;
pub mod storage;
pub mod terrain;

pub const PROJECT_FORMAT_VERSION: u32 = 5;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}
impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
    pub fn distance(self, other: Self) -> f64 {
        (self.x - other.x).hypot(self.y - other.y)
    }
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Layer {
    pub id: u64,
    pub name: String,
    pub color: [u8; 3],
    pub visible: bool,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Entity {
    pub id: u64,
    pub layer_id: u64,
    pub geometry: Geometry,
}
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PolylineVertex {
    pub point: Point,
    #[serde(default)]
    pub bulge: f64,
}
impl PolylineVertex {
    pub fn new(point: Point) -> Self {
        Self { point, bulge: 0.0 }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Geometry {
    Line {
        start: Point,
        end: Point,
    },
    Rectangle {
        a: Point,
        b: Point,
    },
    Circle {
        center: Point,
        radius: f64,
    },
    Arc {
        center: Point,
        radius: f64,
        start_angle_deg: f64,
        end_angle_deg: f64,
    },
    Point {
        position: Point,
    },
    Polyline {
        vertices: Vec<PolylineVertex>,
        closed: bool,
    },
    Text {
        position: Point,
        text: String,
        height: f64,
        rotation_deg: f64,
    },
    Insert {
        block_name: String,
        position: Point,
        scale: f64,
        rotation_deg: f64,
    },
    Dimension {
        dimension: dimension::Dimension,
    },
}
impl Geometry {
    pub fn validate(&self) -> io::Result<()> {
        let finite_point = |point: Point| {
            if point.is_finite() {
                Ok(())
            } else {
                Err(invalid("Coordinates must be finite"))
            }
        };
        match self {
            Self::Dimension { dimension } => dimension.validate()?,
            Self::Insert {
                block_name,
                position,
                scale,
                rotation_deg,
            } => {
                finite_point(*position)?;
                if block_name.trim().is_empty()
                    || block_name.contains(['\n', '\r', '\0'])
                    || !scale.is_finite()
                    || *scale == 0.0
                    || !rotation_deg.is_finite()
                {
                    return Err(invalid(
                        "INSERT requires a block name, finite position/rotation, and nonzero finite uniform scale",
                    ));
                }
            }
            Self::Line { start, end } => {
                finite_point(*start)?;
                finite_point(*end)?;
            }
            Self::Rectangle { a, b } => {
                finite_point(*a)?;
                finite_point(*b)?;
            }
            Self::Circle { center, radius } | Self::Arc { center, radius, .. } => {
                finite_point(*center)?;
                if !radius.is_finite() || *radius <= 0.0 {
                    return Err(invalid("Radius must be finite and positive"));
                }
                if let Self::Arc {
                    start_angle_deg,
                    end_angle_deg,
                    ..
                } = self
                {
                    if !start_angle_deg.is_finite() || !end_angle_deg.is_finite() {
                        return Err(invalid("Arc angles must be finite"));
                    }
                }
            }
            Self::Point { position } => finite_point(*position)?,
            Self::Polyline { vertices, closed } => {
                if vertices.len() < 2 {
                    return Err(invalid("A polyline requires at least two vertices"));
                }
                for (index, vertex) in vertices.iter().enumerate() {
                    finite_point(vertex.point)?;
                    if !vertex.bulge.is_finite() {
                        return Err(invalid("Polyline bulges must be finite"));
                    }
                    if vertex.bulge != 0.0 && (index + 1 < vertices.len() || *closed) {
                        let end = vertices[(index + 1) % vertices.len()].point;
                        if bulge_arc(vertex.point, end, vertex.bulge).is_none() {
                            return Err(invalid("Invalid curved polyline segment"));
                        }
                    }
                }
            }
            Self::Text {
                position,
                text,
                height,
                rotation_deg,
            } => {
                finite_point(*position)?;
                if !height.is_finite() || *height <= 0.0 || !rotation_deg.is_finite() {
                    return Err(invalid("Text height must be positive and rotation finite"));
                }
                if text.contains(['\n', '\r', '\0']) {
                    return Err(invalid(
                        "Single-line text cannot contain newline or null characters",
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn distance_to(&self, point: Point) -> f64 {
        match self {
            Self::Dimension { dimension } => dimension
                .annotation_geometry()
                .map(|geometry| {
                    geometry
                        .iter()
                        .map(|item| item.distance_to(point))
                        .fold(f64::INFINITY, f64::min)
                })
                .unwrap_or(f64::INFINITY),
            Self::Line { start, end } => point_segment_distance(point, *start, *end),
            Self::Circle { center, radius } => (point.distance(*center) - radius).abs(),
            Self::Arc {
                center,
                radius,
                start_angle_deg,
                end_angle_deg,
            } => {
                let angle = (point.y - center.y).atan2(point.x - center.x).to_degrees();
                let sweep = (end_angle_deg - start_angle_deg).rem_euclid(360.0);
                if (angle - start_angle_deg).rem_euclid(360.0) <= sweep {
                    (point.distance(*center) - radius).abs()
                } else {
                    point
                        .distance(arc_point(*center, *radius, *start_angle_deg))
                        .min(point.distance(arc_point(*center, *radius, *end_angle_deg)))
                }
            }
            Self::Point { position } | Self::Insert { position, .. } => point.distance(*position),
            Self::Rectangle { .. } | Self::Polyline { .. } => self
                .layer_lines()
                .iter()
                .map(|(start, end)| point_segment_distance(point, *start, *end))
                .fold(f64::INFINITY, f64::min),
            Self::Text {
                position,
                text,
                height,
                rotation_deg,
            } => {
                let angle = -rotation_deg.to_radians();
                let (sin, cos) = angle.sin_cos();
                let dx = point.x - position.x;
                let dy = point.y - position.y;
                let local = Point::new(dx * cos - dy * sin, dx * sin + dy * cos);
                let width = *height * 0.6 * text.chars().count() as f64;
                local.distance(Point::new(
                    local.x.clamp(0.0, width),
                    local.y.clamp(0.0, *height),
                ))
            }
        }
    }

    /// Line segments for display and approximate picking. Stored arcs and bulges remain exact.
    pub fn layer_lines(&self) -> Vec<(Point, Point)> {
        match self {
            Self::Dimension { dimension } => dimension
                .annotation_geometry()
                .map(|geometry| geometry.iter().flat_map(Geometry::layer_lines).collect())
                .unwrap_or_default(),
            Self::Line { start, end } => vec![(*start, *end)],
            Self::Rectangle { a, b } => {
                let points = [*a, Point::new(b.x, a.y), *b, Point::new(a.x, b.y), *a];
                points.windows(2).map(|pair| (pair[0], pair[1])).collect()
            }
            Self::Circle { center, radius } => sample_arc(*center, *radius, 0.0, 360.0),
            Self::Arc {
                center,
                radius,
                start_angle_deg,
                end_angle_deg,
            } => sample_arc(
                *center,
                *radius,
                *start_angle_deg,
                (end_angle_deg - start_angle_deg).rem_euclid(360.0),
            ),
            Self::Polyline { vertices, closed } => {
                let count = if *closed {
                    vertices.len()
                } else {
                    vertices.len().saturating_sub(1)
                };
                let mut segments = Vec::new();
                for index in 0..count {
                    let start = vertices[index];
                    let end = vertices[(index + 1) % vertices.len()].point;
                    if start.bulge.abs() < 1e-12 {
                        segments.push((start.point, end));
                    } else if let Some((center, radius, angle, sweep)) =
                        bulge_arc(start.point, end, start.bulge)
                    {
                        segments.extend(sample_arc(center, radius, angle, sweep));
                    }
                }
                segments
            }
            Self::Point { .. } | Self::Text { .. } | Self::Insert { .. } => Vec::new(),
        }
    }
}

pub(crate) fn arc_point(center: Point, radius: f64, angle_deg: f64) -> Point {
    let (sin, cos) = angle_deg.to_radians().sin_cos();
    Point::new(center.x + radius * cos, center.y + radius * sin)
}
pub(crate) fn bulge_arc(start: Point, end: Point, bulge: f64) -> Option<(Point, f64, f64, f64)> {
    let chord = start.distance(end);
    if chord <= f64::EPSILON || !chord.is_finite() || !bulge.is_finite() || bulge == 0.0 {
        return None;
    }
    let offset = chord * (1.0 - bulge * bulge) / (4.0 * bulge);
    let center = Point::new(
        (start.x + end.x) / 2.0 - (end.y - start.y) / chord * offset,
        (start.y + end.y) / 2.0 + (end.x - start.x) / chord * offset,
    );
    let radius = chord * (1.0 + bulge * bulge) / (4.0 * bulge.abs());
    let angle = (start.y - center.y).atan2(start.x - center.x).to_degrees();
    if !center.is_finite() || !radius.is_finite() {
        return None;
    }
    Some((center, radius, angle, 4.0 * bulge.atan().to_degrees()))
}
fn sample_arc(center: Point, radius: f64, start: f64, sweep: f64) -> Vec<(Point, Point)> {
    let steps = ((sweep.abs() / 3.0).ceil() as usize).clamp(1, 720);
    (0..steps)
        .map(|step| {
            (
                arc_point(center, radius, start + sweep * step as f64 / steps as f64),
                arc_point(
                    center,
                    radius,
                    start + sweep * (step + 1) as f64 / steps as f64,
                ),
            )
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SolidKind {
    Box,
    Cylinder,
    Sphere,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Solid {
    pub id: u64,
    pub name: String,
    pub kind: SolidKind,
    pub center: [f64; 3],
    pub size: [f64; 3],
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MeshObject {
    pub id: u64,
    pub name: String,
    pub mesh: mesh::TriangleMesh,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub name: String,
    pub entities: Vec<Entity>,
    pub solids: Vec<Solid>,
    #[serde(default)]
    pub meshes: Vec<MeshObject>,
    #[serde(default)]
    pub blocks: Vec<BlockDefinition>,
    #[serde(default)]
    pub layouts: Vec<plot::PrintLayout>,
    #[serde(default)]
    pub building: building::Building,
    #[serde(default)]
    pub terrain: terrain::Terrain,
    #[serde(default)]
    pub scene: scene::Scene,
    #[serde(default)]
    pub manufacturing: cam::CamJob,
    #[serde(default)]
    pub review: review::ProjectReview,
    pub layers: Vec<Layer>,
    pub active_layer_id: u64,
    /// DXF INSUNITS code: 0 unitless, 1 inches, 4 millimetres, 6 metres, etc.
    #[serde(default)]
    pub units: u16,
    #[serde(default = "default_next_id")]
    next_id: u64,
}
fn default_next_id() -> u64 {
    2
}
impl Default for Document {
    fn default() -> Self {
        Self::new("Untitled design")
    }
}
impl Document {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            entities: Vec::new(),
            solids: Vec::new(),
            meshes: Vec::new(),
            blocks: Vec::new(),
            layouts: Vec::new(),
            building: building::Building::default(),
            terrain: terrain::Terrain::default(),
            scene: scene::Scene::default(),
            manufacturing: cam::CamJob::default(),
            review: review::ProjectReview::default(),
            layers: vec![Layer {
                id: 1,
                name: "0".to_owned(),
                color: [215, 222, 234],
                visible: true,
            }],
            active_layer_id: 1,
            units: 0,
            next_id: 2,
        }
    }
    pub fn add_geometry(&mut self, geometry: Geometry) -> u64 {
        let id = self.allocate_id();
        self.entities.push(Entity {
            id,
            layer_id: self.active_layer_id,
            geometry,
        });
        id
    }
    pub fn add_solid(&mut self, name: impl Into<String>, kind: SolidKind) -> u64 {
        let id = self.allocate_id();
        let size = match kind {
            SolidKind::Box => [80.0, 60.0, 48.0],
            SolidKind::Cylinder => [52.0, 52.0, 64.0],
            SolidKind::Sphere => [56.0, 56.0, 56.0],
        };
        self.solids.push(Solid {
            id,
            name: name.into(),
            kind,
            center: [0.0, 0.0, 0.0],
            size,
        });
        id
    }
    pub fn add_mesh(
        &mut self,
        name: impl Into<String>,
        mesh: mesh::TriangleMesh,
    ) -> io::Result<u64> {
        mesh.validate()?;
        let id = self.allocate_id();
        self.meshes.push(MeshObject {
            id,
            name: name.into(),
            mesh,
        });
        Ok(id)
    }
    pub fn add_layer(&mut self, name: impl Into<String>, color: [u8; 3]) -> u64 {
        let id = self.allocate_id();
        self.layers.push(Layer {
            id,
            name: name.into(),
            color,
            visible: true,
        });
        self.active_layer_id = id;
        id
    }
    pub fn layer(&self, id: u64) -> Option<&Layer> {
        self.layers.iter().find(|layer| layer.id == id)
    }
    pub fn remove_entity(&mut self, id: u64) -> bool {
        let before = self.entities.len();
        self.entities.retain(|entity| entity.id != id);
        before != self.entities.len()
    }
    pub fn remove_solid(&mut self, id: u64) -> bool {
        let before = self.solids.len();
        self.solids.retain(|solid| solid.id != id);
        before != self.solids.len()
    }

    pub fn validate(&self) -> io::Result<()> {
        self.building.validate()?;
        self.terrain.validate()?;
        self.scene.validate()?;
        self.manufacturing.validate()?;
        self.review.validate()?;
        if self.units > 24 {
            return Err(invalid("Unknown drawing unit code"));
        }
        if self.layers.is_empty() {
            return Err(invalid("The document must contain a layer"));
        }
        let mut ids = HashSet::new();
        let mut names = HashSet::new();
        for layer in &self.layers {
            if layer.id == 0 || !ids.insert(layer.id) {
                return Err(invalid("Object IDs must be unique and nonzero"));
            }
            if layer.name.trim().is_empty()
                || layer.name.contains(['\n', '\r', '\0'])
                || !names.insert(layer.name.to_lowercase())
            {
                return Err(invalid(
                    "Layer names must be unique and nonempty without control characters",
                ));
            }
        }
        if self.layer(self.active_layer_id).is_none() {
            return Err(invalid("The active layer does not exist"));
        }
        if self.layouts.len() > 100 {
            return Err(invalid("Project exceeds 100 print layouts"));
        }
        let mut layout_names = HashSet::new();
        for layout in &self.layouts {
            layout.validate(self)?;
            if !layout_names.insert(layout.name.to_lowercase()) {
                return Err(invalid("Print layout names must be unique"));
            }
        }
        for entity in self
            .entities
            .iter()
            .chain(self.blocks.iter().flat_map(|block| block.entities.iter()))
        {
            if entity.id == 0 || !ids.insert(entity.id) {
                return Err(invalid("Object IDs must be unique and nonzero"));
            }
            if self.layer(entity.layer_id).is_none() {
                return Err(invalid("An entity refers to a missing layer"));
            }
            entity.geometry.validate()?;
        }
        for solid in &self.solids {
            if solid.id == 0 || !ids.insert(solid.id) {
                return Err(invalid("Object IDs must be unique and nonzero"));
            }
            if !solid.center.iter().all(|value| value.is_finite())
                || !solid
                    .size
                    .iter()
                    .all(|value| value.is_finite() && *value > 0.0)
            {
                return Err(invalid(
                    "Solid coordinates must be finite and dimensions positive",
                ));
            }
        }
        for object in &self.meshes {
            if object.id == 0 || !ids.insert(object.id) {
                return Err(invalid("Object IDs must be unique and nonzero"));
            }
            object.mesh.validate()?;
        }
        blocks::validate_definitions(self)?;
        if ids.contains(&u64::MAX) {
            return Err(invalid("Object ID space is exhausted"));
        }
        Ok(())
    }
    pub fn to_json(&self) -> io::Result<String> {
        self.validate()?;
        let project = ProjectFile {
            format_version: PROJECT_FORMAT_VERSION,
            document: self.clone(),
        };
        let data = serde_json::to_string_pretty(&project)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        Ok(data)
    }
    /// Semantic workbench data cannot be silently discarded by neutral export.
    pub fn has_workbench_data(&self) -> bool {
        self.building != building::Building::default()
            || self.terrain != terrain::Terrain::default()
            || self.scene != scene::Scene::default()
            || self.manufacturing != cam::CamJob::default()
            || self.review != review::ProjectReview::default()
    }
    pub fn clear_workbench_data(&mut self) {
        self.building = building::Building::default();
        self.terrain = terrain::Terrain::default();
        self.scene = scene::Scene::default();
        self.manufacturing = cam::CamJob::default();
        self.review = review::ProjectReview::default();
    }
    pub fn save_json(&self, path: impl AsRef<Path>) -> io::Result<()> {
        write_atomic(path.as_ref(), self.to_json()?.as_bytes())
    }
    pub fn load_json(path: impl AsRef<Path>) -> io::Result<Self> {
        Self::from_json(&fs::read_to_string(path)?)
    }
    pub fn from_json(source: &str) -> io::Result<Self> {
        let project: ProjectFile = serde_json::from_str(source)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        if !(1..=PROJECT_FORMAT_VERSION).contains(&project.format_version) {
            return Err(invalid("Unsupported Forge Craft project version"));
        }
        let mut document = project.document;
        document.validate()?;
        let highest_id = document
            .entities
            .iter()
            .map(|item| item.id)
            .chain(document.solids.iter().map(|item| item.id))
            .chain(document.meshes.iter().map(|item| item.id))
            .chain(
                document
                    .blocks
                    .iter()
                    .flat_map(|block| block.entities.iter().map(|entity| entity.id)),
            )
            .chain(document.layers.iter().map(|item| item.id))
            .max()
            .unwrap_or(0);
        document.next_id = highest_id
            .checked_add(1)
            .ok_or_else(|| invalid("Object ID space is exhausted"))?;
        Ok(document)
    }
    pub fn import_dxf(path: impl AsRef<Path>) -> io::Result<Self> {
        let source = fs::read_to_string(path)?;
        let report = dxf::parse(&source)?;
        if !report.unsupported_entities.is_empty() || !report.unsupported_features.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!(
                    "DXF contains unsupported content; import refused to avoid data loss: {}",
                    report.summary()
                ),
            ));
        }
        Ok(report.document)
    }
    pub fn export_dxf(&self, path: impl AsRef<Path>) -> io::Result<()> {
        write_atomic(path.as_ref(), dxf::write(self)?.as_bytes())
    }
    pub fn export_box_stl(&self, path: impl AsRef<Path>) -> io::Result<usize> {
        self.validate()?;
        if self.has_workbench_data() || !self.layouts.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Legacy STL export cannot omit semantic workbench data; use the dedicated workbench export",
            ));
        }
        if self.solids.iter().any(|solid| solid.kind != SolidKind::Box) || !self.meshes.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Legacy box STL export cannot omit cylinders, spheres, or imported meshes",
            ));
        }
        let mut output = String::from("solid forge_craft\n");
        let mut triangle_count = 0;
        for solid in self
            .solids
            .iter()
            .filter(|solid| matches!(solid.kind, SolidKind::Box))
        {
            let [cx, cy, cz] = solid.center;
            let [sx, sy, sz] = solid.size;
            let [hx, hy, hz] = [sx / 2.0, sy / 2.0, sz / 2.0];
            let vertices = [
                [cx - hx, cy - hy, cz - hz],
                [cx + hx, cy - hy, cz - hz],
                [cx + hx, cy + hy, cz - hz],
                [cx - hx, cy + hy, cz - hz],
                [cx - hx, cy - hy, cz + hz],
                [cx + hx, cy - hy, cz + hz],
                [cx + hx, cy + hy, cz + hz],
                [cx - hx, cy + hy, cz + hz],
            ];
            for face in [
                [0, 3, 2, 1],
                [4, 5, 6, 7],
                [0, 1, 5, 4],
                [1, 2, 6, 5],
                [2, 3, 7, 6],
                [3, 0, 4, 7],
            ] {
                write_facet(
                    &mut output,
                    vertices[face[0]],
                    vertices[face[1]],
                    vertices[face[2]],
                );
                write_facet(
                    &mut output,
                    vertices[face[0]],
                    vertices[face[2]],
                    vertices[face[3]],
                );
                triangle_count += 2;
            }
        }
        output.push_str("endsolid forge_craft\n");
        write_atomic(path.as_ref(), output.as_bytes())?;
        Ok(triangle_count)
    }
    fn allocate_id(&mut self) -> u64 {
        // Public vectors may have been edited directly; never issue an existing ID.
        let highest = self
            .layers
            .iter()
            .map(|item| item.id)
            .chain(self.entities.iter().map(|item| item.id))
            .chain(
                self.blocks
                    .iter()
                    .flat_map(|block| block.entities.iter().map(|entity| entity.id)),
            )
            .chain(self.solids.iter().map(|item| item.id))
            .chain(self.meshes.iter().map(|item| item.id))
            .max()
            .unwrap_or(0);
        let id = self.next_id.max(highest.saturating_add(1)).max(1);
        assert!(id < u64::MAX, "Object ID space exhausted");
        self.next_id = id + 1;
        id
    }
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ProjectFile {
    format_version: u32,
    document: Document,
}
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
pub(crate) fn point_segment_distance(point: Point, start: Point, end: Point) -> f64 {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let length = dx * dx + dy * dy;
    if length <= f64::EPSILON {
        return point.distance(start);
    }
    let t = (((point.x - start.x) * dx + (point.y - start.y) * dy) / length).clamp(0.0, 1.0);
    point.distance(Point::new(start.x + t * dx, start.y + t * dy))
}

static SAVE_SEQUENCE: AtomicU64 = AtomicU64::new(0);
pub(crate) fn write_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "A file path is required"))?;
    let mut temporary = None;
    for _ in 0..100 {
        let mut temporary_name = name.to_os_string();
        temporary_name.push(format!(
            ".{}.{}.tmp",
            std::process::id(),
            SAVE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let candidate = parent.join(temporary_name);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => {
                temporary = Some((candidate, file));
                break;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    let (temporary_path, mut file) = temporary.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::AlreadyExists,
            "Cannot create a temporary save file",
        )
    })?;
    let result = (|| {
        if let Ok(metadata) = fs::metadata(path) {
            file.set_permissions(metadata.permissions())?;
        }
        file.write_all(data)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary_path, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    result
}
fn write_facet(output: &mut String, a: [f64; 3], b: [f64; 3], c: [f64; 3]) {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let cross = [
        ab[1] * ac[2] - ab[2] * ac[1],
        ab[2] * ac[0] - ab[0] * ac[2],
        ab[0] * ac[1] - ab[1] * ac[0],
    ];
    let length = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
    let normal = if length > f64::EPSILON {
        [cross[0] / length, cross[1] / length, cross[2] / length]
    } else {
        [0.0; 3]
    };
    output.push_str(&format!("  facet normal {} {} {}\n    outer loop\n      vertex {} {} {}\n      vertex {} {} {}\n      vertex {} {} {}\n    endloop\n  endfacet\n",normal[0],normal[1],normal[2],a[0],a[1],a[2],b[0],b[1],b[2],c[0],c[1],c[2]));
}

#[cfg(test)]
mod native_tests {
    use super::*;
    use std::path::PathBuf;
    struct TestDir(PathBuf);
    impl TestDir {
        fn new() -> Self {
            let root =
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/drawing-test-files");
            let path = root.join(format!(
                "{}-{}",
                std::process::id(),
                SAVE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn sample() -> Document {
        let mut document = Document::new("Roundtrip ±");
        document.add_geometry(Geometry::Arc {
            center: Point::new(2.0, 3.0),
            radius: 4.0,
            start_angle_deg: 350.0,
            end_angle_deg: 40.0,
        });
        document.add_solid("Box", SolidKind::Box);
        document
            .add_mesh(
                "Triangle",
                mesh::TriangleMesh {
                    vertices: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                    triangles: vec![[0, 1, 2]],
                },
            )
            .unwrap();
        document
    }
    #[test]
    fn native_roundtrip_retains_all_supported_document_types() {
        let document = sample();
        let loaded = Document::from_json(&document.to_json().unwrap()).unwrap();
        assert_eq!(loaded, document);
    }
    #[test]
    fn old_v1_projects_get_default_units_and_meshes_and_repair_stale_ids() {
        let mut document = Document::default();
        document.add_geometry(Geometry::Point {
            position: Point::new(3.0, 4.0),
        });
        let mut value: serde_json::Value =
            serde_json::from_str(&document.to_json().unwrap()).unwrap();
        value["format_version"] = serde_json::json!(1);
        let object = value["document"].as_object_mut().unwrap();
        object.remove("units");
        object.remove("meshes");
        object.insert("next_id".into(), serde_json::json!(1));
        let mut loaded = Document::from_json(&value.to_string()).unwrap();
        assert_eq!(loaded.units, 0);
        assert!(loaded.meshes.is_empty());
        let id = loaded.add_geometry(Geometry::Point {
            position: Point::default(),
        });
        assert!(id > 2);
        loaded.validate().unwrap();
    }
    #[test]
    fn invalid_ids_missing_layers_geometry_and_versions_are_rejected() {
        let mut duplicate = Document::default();
        duplicate.add_geometry(Geometry::Point {
            position: Point::default(),
        });
        duplicate.entities[0].id = 1;
        assert!(duplicate.to_json().is_err());
        let mut missing = Document::default();
        missing.active_layer_id = 99;
        assert!(missing.validate().is_err());
        let mut invalid = Document::default();
        invalid.add_geometry(Geometry::Circle {
            center: Point::default(),
            radius: -1.0,
        });
        assert!(invalid.to_json().is_err());
        let mut value: serde_json::Value =
            serde_json::from_str(&Document::default().to_json().unwrap()).unwrap();
        value["format_version"] = serde_json::json!(PROJECT_FORMAT_VERSION + 1);
        assert!(Document::from_json(&value.to_string()).is_err());
        value["format_version"] = serde_json::json!(1);
        value["document"]["unrecognized_object"] = serde_json::json!({"x":1});
        assert!(Document::from_json(&value.to_string()).is_err());
    }
    #[test]
    fn invalid_save_preserves_existing_file_and_successful_save_replaces_it() {
        let directory = TestDir::new();
        let path = directory.0.join("design.forge.json");
        fs::write(&path, b"original").unwrap();
        let mut invalid = Document::default();
        invalid.layers.clear();
        assert!(invalid.save_json(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"original");
        let document = sample();
        document.save_json(&path).unwrap();
        assert_eq!(Document::load_json(&path).unwrap(), document);
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
    }
    #[test]
    fn failed_atomic_rename_cleans_up_only_its_temporary_file() {
        let directory = TestDir::new();
        let path = directory.0.join("existing-directory");
        fs::create_dir(&path).unwrap();
        assert!(write_atomic(&path, b"new content").is_err());
        assert!(path.is_dir());
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
    }
    #[test]
    fn strict_dxf_import_refuses_unsupported_objects_and_attributes() {
        let directory = TestDir::new();
        let path = directory.0.join("source.dxf");
        fs::write(
            &path,
            "0\nSECTION\n2\nENTITIES\n0\nHATCH\n2\nPattern\n0\nENDSEC\n0\nEOF\n",
        )
        .unwrap();
        assert_eq!(
            Document::import_dxf(&path).unwrap_err().kind(),
            io::ErrorKind::Unsupported
        );
    }
    #[test]
    fn curved_polyline_sampling_and_arc_wraparound_pick_the_correct_curve() {
        let geometry = Geometry::Polyline {
            vertices: vec![
                PolylineVertex {
                    point: Point::new(0.0, 0.0),
                    bulge: 1.0,
                },
                PolylineVertex::new(Point::new(10.0, 0.0)),
            ],
            closed: false,
        };
        geometry.validate().unwrap();
        let lines = geometry.layer_lines();
        assert!(lines.len() > 10);
        assert!(lines.first().unwrap().0.distance(Point::new(0.0, 0.0)) < 1e-10);
        assert!(lines.last().unwrap().1.distance(Point::new(10.0, 0.0)) < 1e-10);
        let arc = Geometry::Arc {
            center: Point::default(),
            radius: 10.0,
            start_angle_deg: 350.0,
            end_angle_deg: 20.0,
        };
        assert!(arc.distance_to(Point::new(10.0, 0.0)) < 1e-10);
        assert!(arc.distance_to(Point::new(-10.0, 0.0)) > 10.0);
    }
}
