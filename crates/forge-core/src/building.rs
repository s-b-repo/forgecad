//! Local semantic building elements and derived geometry/quantity schedules.
//!
//! Coordinates and quantities use the document's model units. Storey elevations
//! define the bottom of walls and the top of slabs. Openings are measured along
//! their host wall from its start point, and above its storey's elevation.
//! This model does not implement a vendor BIM file format or design analysis.
use crate::{Point, mesh::TriangleMesh};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write,
    io,
};

const MAX_ELEMENTS: usize = 10_000;
const MAX_OPENINGS_PER_WALL: usize = 64;
const MAX_WALL_CELLS: usize = 16_384;
const MAX_TRIANGLES: usize = 1_000_000;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Building {
    #[serde(default)]
    pub storeys: Vec<Storey>,
    #[serde(default)]
    pub walls: Vec<Wall>,
    #[serde(default)]
    pub slabs: Vec<Slab>,
    #[serde(default)]
    pub openings: Vec<Opening>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Storey {
    pub id: u64,
    pub name: String,
    pub elevation: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Wall {
    pub id: u64,
    pub name: String,
    pub storey_id: u64,
    pub start: Point,
    pub end: Point,
    pub thickness: f64,
    pub height: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Slab {
    pub id: u64,
    pub name: String,
    pub storey_id: u64,
    pub min: Point,
    pub max: Point,
    pub thickness: f64,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OpeningKind {
    Door,
    Window,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Opening {
    pub id: u64,
    pub name: String,
    pub wall_id: u64,
    pub kind: OpeningKind,
    pub offset: f64,
    pub width: f64,
    pub sill: f64,
    pub height: f64,
}

impl Building {
    pub fn is_empty(&self) -> bool {
        self.storeys.is_empty()
            && self.walls.is_empty()
            && self.slabs.is_empty()
            && self.openings.is_empty()
    }

    /// Validate every element and reference before deriving any output.
    pub fn validate(&self) -> io::Result<()> {
        let total = self
            .storeys
            .len()
            .checked_add(self.walls.len())
            .and_then(|n| n.checked_add(self.slabs.len()))
            .and_then(|n| n.checked_add(self.openings.len()))
            .ok_or_else(|| invalid("Building element count overflow"))?;
        if total > MAX_ELEMENTS {
            return Err(invalid("Building exceeds the 10,000 element limit"));
        }
        let mut ids = BTreeSet::new();
        let mut identify = |id, name: &str| -> io::Result<()> {
            if id == 0 || !ids.insert(id) {
                return Err(invalid("Building element IDs must be unique and nonzero"));
            }
            if name.trim().is_empty() || name.contains(['\0', '\r', '\n']) {
                return Err(invalid(
                    "Building element names must be nonempty single lines",
                ));
            }
            Ok(())
        };
        for item in &self.storeys {
            identify(item.id, &item.name)?;
            if !item.elevation.is_finite() {
                return Err(invalid("Storey elevation must be finite"));
            }
        }
        let storeys: BTreeMap<_, _> = self.storeys.iter().map(|s| (s.id, s)).collect();
        for wall in &self.walls {
            identify(wall.id, &wall.name)?;
            let storey = storeys
                .get(&wall.storey_id)
                .ok_or_else(|| invalid("Wall refers to a missing storey"))?;
            let length = wall.start.distance(wall.end);
            if !wall.start.is_finite()
                || !wall.end.is_finite()
                || !length.is_finite()
                || length <= 0.0
            {
                return Err(invalid("Wall requires finite distinct endpoints"));
            }
            positive(wall.thickness, "Wall thickness")?;
            positive(wall.height, "Wall height")?;
            positive(length * wall.height * wall.thickness, "Wall gross volume")?;
            positive_span(storey.elevation, wall.height, "Wall elevation/height")?;
            let (dx, dy) = (
                (wall.end.x - wall.start.x) / length,
                (wall.end.y - wall.start.y) / length,
            );
            for p in [wall.start, wall.end] {
                let a = Point::new(
                    p.x - dy * wall.thickness / 2.0,
                    p.y + dx * wall.thickness / 2.0,
                );
                let b = Point::new(
                    p.x + dy * wall.thickness / 2.0,
                    p.y - dx * wall.thickness / 2.0,
                );
                if !a.is_finite() || !b.is_finite() || a == b {
                    return Err(invalid(
                        "Wall thickness cannot be represented at these coordinates",
                    ));
                }
            }
        }
        for slab in &self.slabs {
            identify(slab.id, &slab.name)?;
            let storey = storeys
                .get(&slab.storey_id)
                .ok_or_else(|| invalid("Slab refers to a missing storey"))?;
            positive(slab.thickness, "Slab thickness")?;
            if !slab.min.is_finite()
                || !slab.max.is_finite()
                || slab.max.x <= slab.min.x
                || slab.max.y <= slab.min.y
            {
                return Err(invalid(
                    "Slab requires finite increasing rectangular bounds",
                ));
            }
            positive(
                (slab.max.x - slab.min.x) * (slab.max.y - slab.min.y) * slab.thickness,
                "Slab volume",
            )?;
            positive_span(
                storey.elevation - slab.thickness,
                slab.thickness,
                "Slab elevation/thickness",
            )?;
        }
        let walls: BTreeMap<_, _> = self.walls.iter().map(|w| (w.id, w)).collect();
        let mut openings: BTreeMap<u64, Vec<&Opening>> = BTreeMap::new();
        for opening in &self.openings {
            identify(opening.id, &opening.name)?;
            let wall = walls
                .get(&opening.wall_id)
                .ok_or_else(|| invalid("Opening refers to a missing host wall"))?;
            positive(opening.width, "Opening width")?;
            positive(opening.height, "Opening height")?;
            if !opening.offset.is_finite()
                || !opening.sill.is_finite()
                || opening.offset < 0.0
                || opening.sill < 0.0
                || !((opening.offset + opening.width).is_finite()
                    && opening.offset + opening.width <= wall.start.distance(wall.end))
                || !((opening.sill + opening.height).is_finite()
                    && opening.sill + opening.height <= wall.height)
                || opening.offset + opening.width <= opening.offset
                || opening.sill + opening.height <= opening.sill
            {
                return Err(invalid("Opening bounds must fit within the host wall"));
            }
            if opening.kind == OpeningKind::Door && opening.sill != 0.0 {
                return Err(invalid(
                    "Door openings must start at the storey floor (sill 0)",
                ));
            }
            let hosts = openings.entry(wall.id).or_default();
            if hosts.len() >= MAX_OPENINGS_PER_WALL {
                return Err(invalid("A wall may contain at most 64 openings"));
            }
            for other in hosts.iter() {
                if opening.offset < other.offset + other.width
                    && other.offset < opening.offset + opening.width
                    && opening.sill < other.sill + other.height
                    && other.sill < opening.sill + opening.height
                {
                    return Err(invalid("Openings on the same wall overlap"));
                }
            }
            hosts.push(opening);
        }
        for wall in &self.walls {
            let net = wall.start.distance(wall.end) * wall.height
                - openings
                    .get(&wall.id)
                    .into_iter()
                    .flatten()
                    .map(|o| o.width * o.height)
                    .sum::<f64>();
            positive(net * wall.thickness, "Wall net volume")?;
            let count = openings.get(&wall.id).map_or(0, Vec::len);
            if (2 * count + 1).saturating_mul(2 * count + 1) > MAX_WALL_CELLS {
                return Err(invalid(
                    "Wall opening subdivision exceeds the mesh resource limit",
                ));
            }
            validate_wall_subdivision(
                wall,
                storeys[&wall.storey_id].elevation,
                openings.get(&wall.id).map_or(&[], Vec::as_slice),
            )?;
        }
        Ok(())
    }

    /// One mesh per wall/slab, carrying its semantic element ID and name.
    /// Internal subdivision faces are omitted; door/window voids pass through
    /// the complete wall thickness. Opening solids are not filled implicitly.
    pub fn meshes(&self) -> io::Result<Vec<(u64, String, TriangleMesh)>> {
        self.validate()?;
        let elevations: BTreeMap<_, _> = self.storeys.iter().map(|s| (s.id, s.elevation)).collect();
        let mut result = Vec::new();
        let mut triangles = 0usize;
        for wall in &self.walls {
            let openings: Vec<_> = self
                .openings
                .iter()
                .filter(|o| o.wall_id == wall.id)
                .collect();
            let mesh = wall_mesh(wall, elevations[&wall.storey_id], &openings)?;
            triangles = triangles
                .checked_add(mesh.triangles.len())
                .ok_or_else(|| invalid("Building mesh size overflow"))?;
            if triangles > MAX_TRIANGLES {
                return Err(invalid("Building meshes exceed one million triangles"));
            }
            result.push((wall.id, wall.name.clone(), mesh));
        }
        for slab in &self.slabs {
            let elevation = elevations[&slab.storey_id];
            let wall = Wall {
                id: slab.id,
                name: slab.name.clone(),
                storey_id: slab.storey_id,
                start: Point::new(slab.min.x, slab.min.y / 2.0 + slab.max.y / 2.0),
                end: Point::new(slab.max.x, slab.min.y / 2.0 + slab.max.y / 2.0),
                thickness: slab.max.y - slab.min.y,
                height: slab.thickness,
            };
            let mesh = wall_mesh(&wall, elevation - slab.thickness, &[])?;
            triangles = triangles
                .checked_add(mesh.triangles.len())
                .ok_or_else(|| invalid("Building mesh size overflow"))?;
            if triangles > MAX_TRIANGLES {
                return Err(invalid("Building meshes exceed one million triangles"));
            }
            result.push((slab.id, slab.name.clone(), mesh));
        }
        Ok(result)
    }

    /// Material quantities use model units squared/cubed, without assuming a
    /// particular unit system. Wall areas and volumes subtract host openings.
    /// Storeys have zero area/volume; opening volume is the removed host volume.
    pub fn schedule_csv(&self) -> io::Result<String> {
        self.validate()?;
        let mut output = String::from(
            "id,kind,name,storey_id,host_id,gross_area_model2,opening_area_model2,net_area_model2,volume_model3\n",
        );
        for storey in &self.storeys {
            writeln!(
                output,
                "{},storey,{},,,0,0,0,0",
                storey.id,
                csv(&storey.name)
            )
            .unwrap();
        }
        for wall in &self.walls {
            let gross = wall.start.distance(wall.end) * wall.height;
            let cut = self
                .openings
                .iter()
                .filter(|o| o.wall_id == wall.id)
                .map(|o| o.width * o.height)
                .sum::<f64>();
            let net = gross - cut;
            writeln!(
                output,
                "{},wall,{},{},,{},{},{},{}",
                wall.id,
                csv(&wall.name),
                wall.storey_id,
                gross,
                cut,
                net,
                net * wall.thickness
            )
            .unwrap();
        }
        for slab in &self.slabs {
            let area = (slab.max.x - slab.min.x) * (slab.max.y - slab.min.y);
            writeln!(
                output,
                "{},slab,{},{},,{},0,{},{}",
                slab.id,
                csv(&slab.name),
                slab.storey_id,
                area,
                area,
                area * slab.thickness
            )
            .unwrap();
        }
        for opening in &self.openings {
            let wall = self
                .walls
                .iter()
                .find(|w| w.id == opening.wall_id)
                .ok_or_else(|| {
                    invalid(format!(
                        "Opening {} refers to a missing host wall",
                        opening.id
                    ))
                })?;
            let kind = match opening.kind {
                OpeningKind::Door => "door",
                OpeningKind::Window => "window",
            };
            let area = opening.width * opening.height;
            writeln!(
                output,
                "{},{},{},{},{},{},{},0,{}",
                opening.id,
                kind,
                csv(&opening.name),
                wall.storey_id,
                wall.id,
                area,
                area,
                area * wall.thickness
            )
            .unwrap();
        }
        Ok(output)
    }

    /// A small model in millimetres, suitable for exercising building tools.
    pub fn example() -> Self {
        Self {
            storeys: vec![Storey {
                id: 1,
                name: "Ground floor".into(),
                elevation: 0.0,
            }],
            walls: vec![
                Wall {
                    id: 2,
                    name: "South wall".into(),
                    storey_id: 1,
                    start: Point::new(0.0, 0.0),
                    end: Point::new(6000.0, 0.0),
                    thickness: 200.0,
                    height: 2800.0,
                },
                Wall {
                    id: 3,
                    name: "East wall".into(),
                    storey_id: 1,
                    start: Point::new(6000.0, 0.0),
                    end: Point::new(6000.0, 4000.0),
                    thickness: 200.0,
                    height: 2800.0,
                },
                Wall {
                    id: 4,
                    name: "North wall".into(),
                    storey_id: 1,
                    start: Point::new(6000.0, 4000.0),
                    end: Point::new(0.0, 4000.0),
                    thickness: 200.0,
                    height: 2800.0,
                },
                Wall {
                    id: 5,
                    name: "West wall".into(),
                    storey_id: 1,
                    start: Point::new(0.0, 4000.0),
                    end: Point::new(0.0, 0.0),
                    thickness: 200.0,
                    height: 2800.0,
                },
            ],
            slabs: vec![Slab {
                id: 6,
                name: "Ground slab".into(),
                storey_id: 1,
                min: Point::new(-100.0, -100.0),
                max: Point::new(6100.0, 4100.0),
                thickness: 200.0,
            }],
            openings: vec![
                Opening {
                    id: 7,
                    name: "Entrance".into(),
                    wall_id: 2,
                    kind: OpeningKind::Door,
                    offset: 600.0,
                    width: 900.0,
                    sill: 0.0,
                    height: 2100.0,
                },
                Opening {
                    id: 8,
                    name: "South window".into(),
                    wall_id: 2,
                    kind: OpeningKind::Window,
                    offset: 3000.0,
                    width: 1600.0,
                    sill: 900.0,
                    height: 1200.0,
                },
                Opening {
                    id: 9,
                    name: "East window".into(),
                    wall_id: 3,
                    kind: OpeningKind::Window,
                    offset: 1000.0,
                    width: 1800.0,
                    sill: 900.0,
                    height: 1200.0,
                },
            ],
        }
    }
}

fn wall_mesh(wall: &Wall, elevation: f64, openings: &[&Opening]) -> io::Result<TriangleMesh> {
    let length = wall.start.distance(wall.end);
    let mut xs = vec![0.0, length];
    let mut zs = vec![0.0, wall.height];
    for opening in openings {
        xs.extend([opening.offset, opening.offset + opening.width]);
        zs.extend([opening.sill, opening.sill + opening.height]);
    }
    xs.sort_by(f64::total_cmp);
    xs.dedup();
    zs.sort_by(f64::total_cmp);
    zs.dedup();
    let nx = xs.len() - 1;
    let nz = zs.len() - 1;
    if nx.checked_mul(nz).is_none_or(|n| n > MAX_WALL_CELLS) {
        return Err(invalid("Wall subdivision exceeds mesh resource limit"));
    }
    let mut occupied = vec![true; nx * nz];
    for x in 0..nx {
        for z in 0..nz {
            // Every opening edge is a grid edge, so midpoint classification
            // cannot partially cut a cell or approximate a rectangular hole.
            let mx = xs[x] / 2.0 + xs[x + 1] / 2.0;
            let mz = zs[z] / 2.0 + zs[z + 1] / 2.0;
            occupied[x * nz + z] = !openings.iter().any(|o| {
                mx > o.offset && mx < o.offset + o.width && mz > o.sill && mz < o.sill + o.height
            });
        }
    }
    let direction = [
        (wall.end.x - wall.start.x) / length,
        (wall.end.y - wall.start.y) / length,
    ];
    let mut mesh = TriangleMesh::default();
    let mut indices = BTreeMap::<(usize, usize, bool), u32>::new();
    let mut face = |corners: [(usize, usize, bool); 4]| -> io::Result<()> {
        let mut ids = [0u32; 4];
        for (i, key) in corners.into_iter().enumerate() {
            ids[i] = if let Some(id) = indices.get(&key) {
                *id
            } else {
                let local_y = if key.2 {
                    wall.thickness / 2.0
                } else {
                    -wall.thickness / 2.0
                };
                let p = [
                    wall.start.x + direction[0] * xs[key.0] - direction[1] * local_y,
                    wall.start.y + direction[1] * xs[key.0] + direction[0] * local_y,
                    elevation + zs[key.1],
                ];
                let id = u32::try_from(mesh.vertices.len())
                    .map_err(|_| invalid("Building mesh vertex count overflow"))?;
                mesh.vertices.push(p);
                indices.insert(key, id);
                id
            };
        }
        mesh.triangles
            .extend([[ids[0], ids[1], ids[2]], [ids[0], ids[2], ids[3]]]);
        Ok(())
    };
    for x in 0..nx {
        for z in 0..nz {
            if !occupied[x * nz + z] {
                continue;
            }
            // Local axes are along-wall X, thickness Y, and vertical Z.
            face([
                (x, z, false),
                (x + 1, z, false),
                (x + 1, z + 1, false),
                (x, z + 1, false),
            ])?;
            face([
                (x, z, true),
                (x, z + 1, true),
                (x + 1, z + 1, true),
                (x + 1, z, true),
            ])?;
            if x == 0 || !occupied[(x - 1) * nz + z] {
                face([
                    (x, z, false),
                    (x, z + 1, false),
                    (x, z + 1, true),
                    (x, z, true),
                ])?;
            }
            if x + 1 == nx || !occupied[(x + 1) * nz + z] {
                face([
                    (x + 1, z, false),
                    (x + 1, z, true),
                    (x + 1, z + 1, true),
                    (x + 1, z + 1, false),
                ])?;
            }
            if z == 0 || !occupied[x * nz + z - 1] {
                face([
                    (x, z, false),
                    (x, z, true),
                    (x + 1, z, true),
                    (x + 1, z, false),
                ])?;
            }
            if z + 1 == nz || !occupied[x * nz + z + 1] {
                face([
                    (x, z + 1, false),
                    (x + 1, z + 1, false),
                    (x + 1, z + 1, true),
                    (x, z + 1, true),
                ])?;
            }
        }
    }
    mesh.validate()?;
    Ok(mesh)
}

fn validate_wall_subdivision(wall: &Wall, elevation: f64, openings: &[&Opening]) -> io::Result<()> {
    let length = wall.start.distance(wall.end);
    let direction = [
        (wall.end.x - wall.start.x) / length,
        (wall.end.y - wall.start.y) / length,
    ];
    let mut xs = vec![0.0, length];
    let mut zs = vec![0.0, wall.height];
    for opening in openings {
        xs.extend([opening.offset, opening.offset + opening.width]);
        zs.extend([opening.sill, opening.sill + opening.height]);
    }
    xs.sort_by(f64::total_cmp);
    xs.dedup();
    zs.sort_by(f64::total_cmp);
    zs.dedup();
    for side in [-1.0, 1.0] {
        let points: Vec<_> = xs
            .iter()
            .map(|x| {
                Point::new(
                    wall.start.x + direction[0] * x - direction[1] * wall.thickness / 2.0 * side,
                    wall.start.y + direction[1] * x + direction[0] * wall.thickness / 2.0 * side,
                )
            })
            .collect();
        if points.iter().any(|p| !p.is_finite()) || points.windows(2).any(|pair| pair[0] == pair[1])
        {
            return Err(invalid(
                "Wall or opening edge cannot be represented at these coordinates",
            ));
        }
    }
    if zs
        .windows(2)
        .any(|pair| elevation + pair[1] <= elevation + pair[0])
    {
        return Err(invalid(
            "Opening height cannot be represented at this storey elevation",
        ));
    }
    for x in xs.windows(2) {
        for z in zs.windows(2) {
            for area in [
                (x[1] - x[0]) * (z[1] - z[0]),
                (x[1] - x[0]) * wall.thickness,
                (z[1] - z[0]) * wall.thickness,
            ] {
                positive(area, "Building face area")?;
            }
        }
    }
    Ok(())
}

fn csv(value: &str) -> String {
    // Prefix formula-like names so opening the CSV in a spreadsheet cannot
    // interpret a user-supplied object name as a formula.
    let prefix = if value.trim_start().starts_with(['=', '+', '-', '@']) {
        "'"
    } else {
        ""
    };
    format!("\"{}{}\"", prefix, value.replace('"', "\"\""))
}
fn positive(value: f64, label: &str) -> io::Result<()> {
    if !value.is_finite() || value <= 0.0 {
        Err(invalid(format!("{label} must be finite and positive")))
    } else {
        Ok(())
    }
}
fn positive_span(start: f64, height: f64, label: &str) -> io::Result<()> {
    if !start.is_finite() || !(start + height).is_finite() || start + height <= start {
        Err(invalid(format!("{label} cannot be represented")))
    } else {
        Ok(())
    }
}
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn basic() -> Building {
        Building {
            storeys: vec![Storey {
                id: 1,
                name: "Ground".into(),
                elevation: 2.0,
            }],
            walls: vec![Wall {
                id: 2,
                name: "Wall".into(),
                storey_id: 1,
                start: Point::new(0.0, 0.0),
                end: Point::new(10.0, 0.0),
                thickness: 0.3,
                height: 3.0,
            }],
            slabs: vec![],
            openings: vec![],
        }
    }
    fn near(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-8, "{a} != {b}");
    }
    fn edge_incidence(mesh: &TriangleMesh) -> BTreeMap<(u32, u32), usize> {
        let mut counts = BTreeMap::new();
        for t in &mesh.triangles {
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                *counts.entry((a.min(b), a.max(b))).or_insert(0) += 1;
            }
        }
        counts
    }
    #[test]
    fn plain_wall_is_closed_outward_and_placed_at_storey_elevation() {
        let building = basic();
        let meshes = building.meshes().unwrap();
        let mesh = &meshes[0].2;
        assert_eq!(mesh.triangles.len(), 12);
        assert_eq!(mesh.vertices.len(), 8);
        near(mesh.signed_volume().unwrap(), 9.0);
        assert_eq!(
            mesh.bounds().unwrap(),
            ([0.0, -0.15, 2.0], [10.0, 0.15, 5.0])
        );
        assert!(edge_incidence(mesh).values().all(|n| *n == 2));
    }
    #[test]
    fn doors_and_windows_remove_actual_geometry_and_net_volume() {
        let mut building = basic();
        building.openings = vec![
            Opening {
                id: 3,
                name: "Door".into(),
                wall_id: 2,
                kind: OpeningKind::Door,
                offset: 1.0,
                width: 1.0,
                sill: 0.0,
                height: 2.0,
            },
            Opening {
                id: 4,
                name: "Window".into(),
                wall_id: 2,
                kind: OpeningKind::Window,
                offset: 5.0,
                width: 2.0,
                sill: 1.0,
                height: 1.0,
            },
        ];
        let meshes = building.meshes().unwrap();
        let mesh = &meshes[0].2;
        near(mesh.signed_volume().unwrap(), (30.0 - 2.0 - 2.0) * 0.3);
        assert!(edge_incidence(mesh).values().all(|n| *n == 2));
        // No front/back surface triangle may cover the center of either void.
        for triangle in &mesh.triangles {
            let p = triangle.map(|i| mesh.vertices[i as usize]);
            if p.iter().all(|v| v[1] == -0.15) || p.iter().all(|v| v[1] == 0.15) {
                let cx = p.iter().map(|p| p[0]).sum::<f64>() / 3.0;
                let cz = p.iter().map(|p| p[2] - 2.0).sum::<f64>() / 3.0;
                assert!(!(cx > 1.0 && cx < 2.0 && cz > 0.0 && cz < 2.0));
                assert!(!(cx > 5.0 && cx < 7.0 && cz > 1.0 && cz < 2.0));
            }
        }
        assert!(
            building
                .schedule_csv()
                .unwrap()
                .contains("2,wall,\"Wall\",1,,30,4,26,7.8")
        );
    }
    #[test]
    fn rotated_wall_and_slab_volume_match_quantities() {
        let mut building = basic();
        building.walls[0].start = Point::new(4.0, 8.0);
        building.walls[0].end = Point::new(10.0, 16.0);
        building.slabs.push(Slab {
            id: 3,
            name: "Floor".into(),
            storey_id: 1,
            min: Point::new(3.0, 7.0),
            max: Point::new(12.0, 18.0),
            thickness: 0.2,
        });
        let meshes = building.meshes().unwrap();
        near(meshes[0].2.signed_volume().unwrap(), 9.0);
        near(meshes[1].2.signed_volume().unwrap(), 19.8);
        assert_eq!(
            meshes[1].2.bounds().unwrap(),
            ([3.0, 7.0, 1.8], [12.0, 18.0, 2.0])
        );
        assert!(
            building
                .schedule_csv()
                .unwrap()
                .contains("3,slab,\"Floor\",1,,99,0,99,19.8")
        );
    }
    #[test]
    fn references_and_ids_are_checked_across_all_element_types() {
        let mut building = basic();
        building.walls[0].id = 1;
        assert!(building.validate().is_err());
        let mut building = basic();
        building.walls[0].storey_id = 999;
        assert!(building.validate().is_err());
        let mut building = basic();
        building.storeys.clear();
        assert!(building.validate().is_err());
        let mut building = basic();
        building.openings.push(Opening {
            id: 3,
            name: "Missing".into(),
            wall_id: 999,
            kind: OpeningKind::Door,
            offset: 1.0,
            width: 1.0,
            sill: 0.0,
            height: 2.0,
        });
        assert!(building.validate().is_err());
    }
    #[test]
    fn opening_bounds_overlap_and_door_sills_are_checked() {
        let source = Opening {
            id: 3,
            name: "Door".into(),
            wall_id: 2,
            kind: OpeningKind::Door,
            offset: 1.0,
            width: 1.0,
            sill: 0.0,
            height: 2.0,
        };
        for (offset, width, sill, height) in [
            (9.5, 1.0, 0.0, 2.0),
            (1.0, 1.0, 0.0, 4.0),
            (1.0, 0.0, 0.0, 2.0),
            (1.0, 1.0, 0.1, 2.0),
            (-1.0, 1.0, 0.0, 2.0),
            (1.0, 1.0, 0.0, f64::NAN),
        ] {
            let mut building = basic();
            building.openings.push(Opening {
                offset,
                width,
                sill,
                height,
                ..source.clone()
            });
            assert!(building.validate().is_err());
        }
        let mut building = basic();
        building.openings.push(source.clone());
        building.openings.push(Opening {
            id: 4,
            offset: 1.5,
            ..source.clone()
        });
        assert!(building.validate().is_err());
        building.openings[1].offset = 2.0;
        assert!(building.validate().is_ok());
        // Windows stacked at different heights are not overlapping.
        building.openings[1] = Opening {
            id: 4,
            kind: OpeningKind::Window,
            sill: 2.0,
            height: 0.5,
            ..source
        };
        assert!(building.validate().is_ok());
        near(
            building.meshes().unwrap()[0].2.signed_volume().unwrap(),
            (30.0 - 2.0 - 0.5) * 0.3,
        );
    }
    #[test]
    fn malformed_or_unrepresentable_geometry_is_refused() {
        for (thickness, height) in [
            (0.0, 3.0),
            (0.2, -1.0),
            (f64::INFINITY, 3.0),
            (0.2, f64::NAN),
            (1e308, 1e308),
        ] {
            let mut building = basic();
            building.walls[0].thickness = thickness;
            building.walls[0].height = height;
            assert!(building.validate().is_err());
        }
        let mut building = basic();
        building.walls[0].end = building.walls[0].start;
        assert!(building.validate().is_err());
        let mut building = basic();
        building.storeys[0].elevation = 1e30;
        assert!(building.validate().is_err());
        let mut building = basic();
        building.walls[0].start = Point::new(1e20, 1e20);
        building.walls[0].end = Point::new(1e20 + 1e8, 1e20);
        assert!(building.validate().is_err());
    }
    #[test]
    fn names_are_csv_escaped_and_spreadsheet_formulas_are_inert() {
        let mut building = basic();
        building.walls[0].name = "Wall, \"east\"".into();
        building.storeys[0].name = "=1+1".into();
        let text = building.schedule_csv().unwrap();
        assert!(text.contains("\"Wall, \"\"east\"\"\""));
        assert!(text.contains("\"'=1+1\""));
        building.walls[0].name = "bad\nrow".into();
        assert!(building.schedule_csv().is_err());
    }
    #[test]
    fn opening_edges_must_remain_distinct_in_world_coordinates() {
        let mut building = basic();
        building.walls[0].start = Point::new(1e16, 0.0);
        building.walls[0].end = Point::new(1e16 + 1000.0, 0.0);
        building.walls[0].thickness = 20.0;
        building.openings.push(Opening {
            id: 3,
            name: "Rounded edge".into(),
            wall_id: 2,
            kind: OpeningKind::Window,
            offset: 11.0,
            width: 1.0,
            sill: 1.0,
            height: 1.0,
        });
        assert!(building.validate().is_err());
    }
    #[test]
    fn example_retains_semantics_and_generates_valid_meshes() {
        let building = Building::example();
        building.validate().unwrap();
        assert!(!building.is_empty());
        assert_eq!(building.meshes().unwrap().len(), 5);
        assert_eq!(building.openings.len(), 3);
        let text = serde_json::to_string(&building).unwrap();
        assert_eq!(serde_json::from_str::<Building>(&text).unwrap(), building);
        assert!(Building::default().is_empty());
        assert!(Building::default().meshes().unwrap().is_empty());
    }
}
