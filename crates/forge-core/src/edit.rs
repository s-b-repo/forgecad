//! Geometric editing and object snaps. All coordinates use the document's model units.
use crate::{Document, Entity, Geometry, Point, PolylineVertex};
use std::{collections::BTreeSet, io};

const EPS: f64 = 1e-10;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds2d {
    pub min: Point,
    pub max: Point,
}

impl Bounds2d {
    pub fn from_point(point: Point) -> Self {
        Self {
            min: point,
            max: point,
        }
    }
    pub fn include(&mut self, point: Point) {
        self.min.x = self.min.x.min(point.x);
        self.min.y = self.min.y.min(point.y);
        self.max.x = self.max.x.max(point.x);
        self.max.y = self.max.y.max(point.y);
    }
    pub fn contains(&self, point: Point) -> bool {
        point.x >= self.min.x
            && point.x <= self.max.x
            && point.y >= self.min.y
            && point.y <= self.max.y
    }
}

/// A similarity transform: translation, rotation, uniform scale, or reflection.
/// Nonuniform scale and shear are rejected because circles would become ellipses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform2d {
    pub matrix: [[f64; 2]; 2],
    pub offset: Point,
}

impl Transform2d {
    pub fn identity() -> Self {
        Self {
            matrix: [[1.0, 0.0], [0.0, 1.0]],
            offset: Point::default(),
        }
    }
    pub fn translation(dx: f64, dy: f64) -> Self {
        Self {
            offset: Point::new(dx, dy),
            ..Self::identity()
        }
    }
    pub fn rotation(degrees: f64, pivot: Point) -> Self {
        let (s, c) = degrees.to_radians().sin_cos();
        Self::around([[c, -s], [s, c]], pivot)
    }
    pub fn uniform_scale(factor: f64, pivot: Point) -> Self {
        Self::around([[factor, 0.0], [0.0, factor]], pivot)
    }
    pub fn reflection(start: Point, end: Point) -> io::Result<Self> {
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let length = dx.hypot(dy);
        if !length.is_finite() || length == 0.0 {
            return Err(invalid("Mirror axis must have finite, distinct endpoints"));
        }
        let (x, y) = (dx / length, dy / length);
        Ok(Self::around(
            [
                [2.0 * x * x - 1.0, 2.0 * x * y],
                [2.0 * x * y, 2.0 * y * y - 1.0],
            ],
            start,
        ))
    }
    fn around(matrix: [[f64; 2]; 2], pivot: Point) -> Self {
        Self {
            matrix,
            offset: Point::new(
                pivot.x - matrix[0][0] * pivot.x - matrix[0][1] * pivot.y,
                pivot.y - matrix[1][0] * pivot.x - matrix[1][1] * pivot.y,
            ),
        }
    }
    pub fn apply(self, point: Point) -> Point {
        Point::new(
            self.matrix[0][0] * point.x + self.matrix[0][1] * point.y + self.offset.x,
            self.matrix[1][0] * point.x + self.matrix[1][1] * point.y + self.offset.y,
        )
    }
    /// Compose in execution order: `a.then(b)` applies a, then b.
    pub fn then(self, next: Self) -> Self {
        let a = self.matrix;
        let b = next.matrix;
        Self {
            matrix: [
                [
                    b[0][0] * a[0][0] + b[0][1] * a[1][0],
                    b[0][0] * a[0][1] + b[0][1] * a[1][1],
                ],
                [
                    b[1][0] * a[0][0] + b[1][1] * a[1][0],
                    b[1][0] * a[0][1] + b[1][1] * a[1][1],
                ],
            ],
            offset: next.apply(self.offset),
        }
    }
    fn properties(self) -> io::Result<(f64, bool)> {
        if self
            .matrix
            .iter()
            .flatten()
            .chain([&self.offset.x, &self.offset.y])
            .any(|v| !v.is_finite())
        {
            return Err(invalid("Transform contains a nonfinite coordinate"));
        }
        let m = self.matrix;
        let x = m[0][0].hypot(m[1][0]);
        let y = m[0][1].hypot(m[1][1]);
        if !x.is_finite() || !y.is_finite() || x == 0.0 || y == 0.0 {
            return Err(invalid("Transform scale must be finite and nonzero"));
        }
        let dot = (m[0][0] / x) * (m[0][1] / y) + (m[1][0] / x) * (m[1][1] / y);
        let tolerance = 1e-9 * x.max(y);
        if (x - y).abs() > tolerance || dot.abs() > 1e-9 {
            return Err(invalid(
                "Only finite nonzero uniform scales, rotations, translations, and mirrors are supported",
            ));
        }
        Ok((
            x,
            (m[0][0] / x) * (m[1][1] / y) - (m[0][1] / y) * (m[1][0] / x) < 0.0,
        ))
    }
    fn angle(self, degrees: f64) -> f64 {
        let (s, c) = degrees.to_radians().sin_cos();
        (self.matrix[1][0] * c + self.matrix[1][1] * s)
            .atan2(self.matrix[0][0] * c + self.matrix[0][1] * s)
            .to_degrees()
            .rem_euclid(360.0)
    }
}

impl Geometry {
    pub fn transformed(&self, transform: Transform2d) -> io::Result<Self> {
        self.validate()?;
        let (scale, mirrored) = transform.properties()?;
        let result = match self {
            Self::Insert {
                block_name,
                position,
                scale: insert_scale,
                rotation_deg,
            } => {
                if mirrored {
                    return Err(invalid(
                        "Mirroring block references requires nonuniform instance support; transform was not applied",
                    ));
                }
                Self::Insert {
                    block_name: block_name.clone(),
                    position: transform.apply(*position),
                    scale: insert_scale * scale,
                    rotation_deg: transform.angle(*rotation_deg),
                }
            }
            Self::Dimension { dimension } => Self::Dimension {
                dimension: dimension.transformed(transform)?,
            },
            Self::Line { start, end } => Self::Line {
                start: transform.apply(*start),
                end: transform.apply(*end),
            },
            Self::Rectangle { a, b } => {
                let corners = [*a, Point::new(b.x, a.y), *b, Point::new(a.x, b.y)]
                    .map(|p| transform.apply(p));
                let m = transform.matrix;
                let axis_aligned = ((m[0][0] / scale).abs() <= 1e-12
                    && (m[1][1] / scale).abs() <= 1e-12)
                    || ((m[0][1] / scale).abs() <= 1e-12 && (m[1][0] / scale).abs() <= 1e-12);
                if axis_aligned {
                    Self::Rectangle {
                        a: corners[0],
                        b: corners[2],
                    }
                } else {
                    Self::Polyline {
                        vertices: corners
                            .into_iter()
                            .map(|point| PolylineVertex { point, bulge: 0.0 })
                            .collect(),
                        closed: true,
                    }
                }
            }
            Self::Circle { center, radius } => Self::Circle {
                center: transform.apply(*center),
                radius: radius * scale,
            },
            Self::Arc {
                center,
                radius,
                start_angle_deg,
                end_angle_deg,
            } => {
                let (start, end) = (
                    transform.angle(*start_angle_deg),
                    transform.angle(*end_angle_deg),
                );
                Self::Arc {
                    center: transform.apply(*center),
                    radius: radius * scale,
                    start_angle_deg: if mirrored { end } else { start },
                    end_angle_deg: if mirrored { start } else { end },
                }
            }
            Self::Point { position } => Self::Point {
                position: transform.apply(*position),
            },
            Self::Polyline { vertices, closed } => Self::Polyline {
                vertices: vertices
                    .iter()
                    .map(|v| PolylineVertex {
                        point: transform.apply(v.point),
                        bulge: if mirrored { -v.bulge } else { v.bulge },
                    })
                    .collect(),
                closed: *closed,
            },
            Self::Text {
                position,
                text,
                height,
                rotation_deg,
            } => {
                if mirrored {
                    return Err(invalid(
                        "Mirrored text requires glyph reflection support; transform was not applied",
                    ));
                }
                Self::Text {
                    position: transform.apply(*position),
                    text: text.clone(),
                    height: height * scale,
                    rotation_deg: transform.angle(*rotation_deg),
                }
            }
        };
        result.validate()?;
        if result.bounds().is_some_and(|b| {
            [b.min.x, b.min.y, b.max.x, b.max.y]
                .iter()
                .any(|v| !v.is_finite())
        }) {
            return Err(invalid(
                "Transform overflowed the geometry coordinate range",
            ));
        }
        Ok(result)
    }

    /// Model bounds. Text returns its insertion point; font extents require a text shaper.
    pub fn bounds(&self) -> Option<Bounds2d> {
        match self {
            Self::Insert { position, .. } => Some(Bounds2d::from_point(*position)),
            Self::Dimension { dimension } => {
                let mut total: Option<Bounds2d> = None;
                for geometry in dimension.annotation_geometry().ok()? {
                    if let Some(bounds) = geometry.bounds() {
                        if let Some(total) = &mut total {
                            total.include(bounds.min);
                            total.include(bounds.max);
                        } else {
                            total = Some(bounds);
                        }
                    }
                }
                total
            }
            Self::Line { start, end } | Self::Rectangle { a: start, b: end } => {
                let mut bounds = Bounds2d::from_point(*start);
                bounds.include(*end);
                Some(bounds)
            }
            Self::Circle { center, radius } => Some(Bounds2d {
                min: Point::new(center.x - radius, center.y - radius),
                max: Point::new(center.x + radius, center.y + radius),
            }),
            Self::Arc {
                center,
                radius,
                start_angle_deg,
                end_angle_deg,
            } => Some(arc_bounds(
                *center,
                *radius,
                start_angle_deg.to_radians(),
                ccw_sweep(*start_angle_deg, *end_angle_deg).to_radians(),
            )),
            Self::Point { position } | Self::Text { position, .. } => {
                Some(Bounds2d::from_point(*position))
            }
            Self::Polyline { vertices, closed } => {
                let mut bounds = Bounds2d::from_point(vertices.first()?.point);
                for v in vertices {
                    bounds.include(v.point);
                }
                for (a, b) in segments(vertices, *closed) {
                    if let Some(arc) = bulge_arc(a, b) {
                        let ab = arc_bounds(arc.center, arc.radius, arc.start, arc.sweep);
                        bounds.include(ab.min);
                        bounds.include(ab.max);
                    }
                }
                Some(bounds)
            }
        }
    }

    /// Curve length; text/point entities have no curve length.
    pub fn length(&self) -> f64 {
        match self {
            Self::Insert { .. } => 0.0,
            Self::Dimension { dimension } => dimension.measurement().unwrap_or(0.0),
            Self::Line { start, end } => distance(*start, *end),
            Self::Rectangle { a, b } => 2.0 * ((a.x - b.x).abs() + (a.y - b.y).abs()),
            Self::Circle { radius, .. } => std::f64::consts::TAU * radius,
            Self::Arc {
                radius,
                start_angle_deg,
                end_angle_deg,
                ..
            } => ccw_sweep(*start_angle_deg, *end_angle_deg).to_radians() * radius,
            Self::Polyline { vertices, closed } => segments(vertices, *closed)
                .map(|(a, b)| {
                    bulge_arc(a, b)
                        .map(|arc| arc.radius * arc.sweep.abs())
                        .unwrap_or_else(|| distance(a.point, b.point))
                })
                .sum(),
            Self::Point { .. } | Self::Text { .. } => 0.0,
        }
    }
}

impl Document {
    /// Validate the entire operation before replacing any original entity.
    pub fn transform_entities(&mut self, ids: &[u64], transform: Transform2d) -> io::Result<usize> {
        let requested: BTreeSet<u64> = ids.iter().copied().collect();
        let replacements = self.prepare_transforms(&requested, transform)?;
        let mut candidate = self.clone();
        for (id, geometry) in replacements {
            candidate
                .entities
                .iter_mut()
                .find(|e| e.id == id)
                .expect("validated id")
                .geometry = geometry;
        }
        candidate.validate()?;
        *self = candidate;
        Ok(requested.len())
    }
    pub fn duplicate_entities(
        &mut self,
        ids: &[u64],
        transform: Transform2d,
    ) -> io::Result<Vec<u64>> {
        let requested: BTreeSet<u64> = ids.iter().copied().collect();
        let replacements = self.prepare_transforms(&requested, transform)?;
        let mut candidate = self.clone();
        let mut new_ids = Vec::with_capacity(replacements.len());
        for (source_id, geometry) in replacements {
            let layer_id = self
                .entities
                .iter()
                .find(|e| e.id == source_id)
                .expect("validated id")
                .layer_id;
            let id = candidate.allocate_id();
            candidate.entities.push(Entity {
                id,
                layer_id,
                geometry,
            });
            new_ids.push(id);
        }
        candidate.validate()?;
        *self = candidate;
        Ok(new_ids)
    }
    fn prepare_transforms(
        &self,
        requested: &BTreeSet<u64>,
        transform: Transform2d,
    ) -> io::Result<Vec<(u64, Geometry)>> {
        transform.properties()?;
        requested
            .iter()
            .map(|id| {
                let entity = self
                    .entities
                    .iter()
                    .find(|entity| entity.id == *id)
                    .ok_or_else(|| invalid(format!("Entity {id} does not exist")))?;
                Ok((*id, entity.geometry.transformed(transform)?))
            })
            .collect()
    }
    pub fn drawing_bounds(&self, visible_only: bool) -> Option<Bounds2d> {
        let mut result: Option<Bounds2d> = None;
        for entity in self.expanded_entities().ok()? {
            if visible_only && !entity.visible {
                continue;
            }
            if let Some(bounds) = entity.geometry.bounds() {
                if let Some(total) = &mut result {
                    total.include(bounds.min);
                    total.include(bounds.max);
                } else {
                    result = Some(bounds);
                }
            }
        }
        result
    }
    pub fn object_snap(
        &self,
        point: Point,
        tolerance: f64,
        options: SnapOptions,
    ) -> Option<SnapResult> {
        if !point.x.is_finite() || !point.y.is_finite() || !tolerance.is_finite() || tolerance < 0.0
        {
            return None;
        }
        let mut result: Option<SnapResult> = None;
        for entity in self.expanded_entities().ok()? {
            if !entity.visible {
                continue;
            }
            for (candidate, kind) in snap_candidates(&entity.geometry, point, options) {
                let d = distance(point, candidate);
                if d <= tolerance
                    && result.as_ref().is_none_or(|best| {
                        d < best.distance - EPS
                            || ((d - best.distance).abs() <= EPS
                                && (kind as u8) < (best.kind as u8))
                    })
                {
                    result = Some(SnapResult {
                        point: candidate,
                        kind,
                        entity_id: entity.source_id,
                        distance: d,
                    });
                }
            }
        }
        result
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapKind {
    Endpoint,
    Midpoint,
    Center,
    Quadrant,
    Nearest,
}
#[derive(Clone, Copy, Debug)]
pub struct SnapOptions {
    pub endpoints: bool,
    pub midpoints: bool,
    pub centers: bool,
    pub quadrants: bool,
    pub nearest: bool,
}
impl Default for SnapOptions {
    fn default() -> Self {
        Self {
            endpoints: true,
            midpoints: true,
            centers: true,
            quadrants: true,
            nearest: false,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct SnapResult {
    pub point: Point,
    pub kind: SnapKind,
    pub entity_id: u64,
    pub distance: f64,
}

fn snap_candidates(
    geometry: &Geometry,
    cursor: Point,
    options: SnapOptions,
) -> Vec<(Point, SnapKind)> {
    let mut result = Vec::new();
    let mut line = |a: Point, b: Point| {
        if options.endpoints {
            result.extend([(a, SnapKind::Endpoint), (b, SnapKind::Endpoint)]);
        }
        if options.midpoints {
            result.push((
                Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0),
                SnapKind::Midpoint,
            ));
        }
        if options.nearest {
            result.push((nearest_line(cursor, a, b), SnapKind::Nearest));
        }
    };
    match geometry {
        Geometry::Insert { position, .. } => {
            if options.endpoints {
                result.push((*position, SnapKind::Endpoint));
            }
        }
        Geometry::Dimension { dimension } => {
            if let Ok(primitives) = dimension.annotation_geometry() {
                for primitive in primitives {
                    result.extend(snap_candidates(&primitive, cursor, options));
                }
            }
        }
        Geometry::Line { start, end } => line(*start, *end),
        Geometry::Rectangle { .. } => {
            for (a, b) in geometry.layer_lines() {
                line(a, b);
            }
        }
        Geometry::Circle { center, radius } => {
            if options.centers {
                result.push((*center, SnapKind::Center));
            }
            if options.quadrants {
                for i in 0..4 {
                    result.push((
                        circle_point(*center, *radius, i as f64 * std::f64::consts::FRAC_PI_2),
                        SnapKind::Quadrant,
                    ));
                }
            }
            if options.nearest {
                result.push((
                    circle_point(
                        *center,
                        *radius,
                        (cursor.y - center.y).atan2(cursor.x - center.x),
                    ),
                    SnapKind::Nearest,
                ));
            }
        }
        Geometry::Arc {
            center,
            radius,
            start_angle_deg,
            end_angle_deg,
        } => {
            result.extend(arc_snaps(
                Arc {
                    center: *center,
                    radius: *radius,
                    start: start_angle_deg.to_radians(),
                    sweep: ccw_sweep(*start_angle_deg, *end_angle_deg).to_radians(),
                },
                cursor,
                options,
            ));
        }
        Geometry::Point { position } | Geometry::Text { position, .. } => {
            if options.endpoints {
                result.push((*position, SnapKind::Endpoint));
            }
        }
        Geometry::Polyline { vertices, closed } => {
            for (a, b) in segments(vertices, *closed) {
                if let Some(arc) = bulge_arc(a, b) {
                    result.extend(arc_snaps(arc, cursor, options));
                } else {
                    if options.endpoints {
                        result
                            .extend([(a.point, SnapKind::Endpoint), (b.point, SnapKind::Endpoint)]);
                    }
                    if options.midpoints {
                        result.push((
                            Point::new(
                                (a.point.x + b.point.x) / 2.0,
                                (a.point.y + b.point.y) / 2.0,
                            ),
                            SnapKind::Midpoint,
                        ));
                    }
                    if options.nearest {
                        result.push((nearest_line(cursor, a.point, b.point), SnapKind::Nearest));
                    }
                }
            }
        }
    }
    result
}

#[derive(Clone, Copy)]
struct Arc {
    center: Point,
    radius: f64,
    start: f64,
    sweep: f64,
}
fn arc_snaps(arc: Arc, cursor: Point, options: SnapOptions) -> Vec<(Point, SnapKind)> {
    let mut result = Vec::new();
    let (a, b) = (
        circle_point(arc.center, arc.radius, arc.start),
        circle_point(arc.center, arc.radius, arc.start + arc.sweep),
    );
    if options.endpoints {
        result.extend([(a, SnapKind::Endpoint), (b, SnapKind::Endpoint)]);
    }
    if options.centers {
        result.push((arc.center, SnapKind::Center));
    }
    if options.midpoints {
        result.push((
            circle_point(arc.center, arc.radius, arc.start + arc.sweep / 2.0),
            SnapKind::Midpoint,
        ));
    }
    if options.quadrants {
        for i in 0..4 {
            let angle = i as f64 * std::f64::consts::FRAC_PI_2;
            if on_arc(angle, arc.start, arc.sweep) {
                result.push((
                    circle_point(arc.center, arc.radius, angle),
                    SnapKind::Quadrant,
                ));
            }
        }
    }
    if options.nearest {
        let angle = (cursor.y - arc.center.y).atan2(cursor.x - arc.center.x);
        let nearest = if on_arc(angle, arc.start, arc.sweep) {
            circle_point(arc.center, arc.radius, angle)
        } else if distance(cursor, a) < distance(cursor, b) {
            a
        } else {
            b
        };
        result.push((nearest, SnapKind::Nearest));
    }
    result
}
fn segments(
    vertices: &[PolylineVertex],
    closed: bool,
) -> impl Iterator<Item = (&PolylineVertex, &PolylineVertex)> {
    vertices
        .windows(2)
        .map(|v| (&v[0], &v[1]))
        .chain(if closed && vertices.len() > 1 {
            Some((&vertices[vertices.len() - 1], &vertices[0]))
        } else {
            None
        })
}
fn bulge_arc(a: &PolylineVertex, b: &PolylineVertex) -> Option<Arc> {
    let bulge = a.bulge;
    let chord = distance(a.point, b.point);
    if bulge.abs() < EPS || chord < EPS {
        return None;
    }
    let offset = chord / 4.0 * (1.0 / bulge - bulge);
    let center = Point::new(
        (a.point.x + b.point.x) / 2.0 - (b.point.y - a.point.y) / chord * offset,
        (a.point.y + b.point.y) / 2.0 + (b.point.x - a.point.x) / chord * offset,
    );
    Some(Arc {
        center,
        radius: chord / 4.0 * (bulge.abs() + 1.0 / bulge.abs()),
        start: (a.point.y - center.y).atan2(a.point.x - center.x),
        sweep: 4.0 * bulge.atan(),
    })
}
fn arc_bounds(center: Point, radius: f64, start: f64, sweep: f64) -> Bounds2d {
    let mut result = Bounds2d::from_point(circle_point(center, radius, start));
    result.include(circle_point(center, radius, start + sweep));
    for i in 0..4 {
        let angle = i as f64 * std::f64::consts::FRAC_PI_2;
        if on_arc(angle, start, sweep) {
            result.include(circle_point(center, radius, angle));
        }
    }
    result
}
fn ccw_sweep(start: f64, end: f64) -> f64 {
    (end - start).rem_euclid(360.0)
}
fn on_arc(angle: f64, start: f64, sweep: f64) -> bool {
    if sweep >= 0.0 {
        (angle - start).rem_euclid(std::f64::consts::TAU) <= sweep + EPS
    } else {
        (start - angle).rem_euclid(std::f64::consts::TAU) <= -sweep + EPS
    }
}
fn circle_point(center: Point, radius: f64, angle: f64) -> Point {
    Point::new(
        center.x + radius * angle.cos(),
        center.y + radius * angle.sin(),
    )
}
fn distance(a: Point, b: Point) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}
fn nearest_line(p: Point, a: Point, b: Point) -> Point {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let length_squared = dx * dx + dy * dy;
    if length_squared <= EPS {
        return a;
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / length_squared).clamp(0.0, 1.0);
    Point::new(a.x + t * dx, a.y + t * dy)
}
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn very_large_shear_cannot_pass_similarity_validation_by_overflow() {
        let geometry = Geometry::Point {
            position: Point::new(0.0, 0.0),
        };
        let transform = Transform2d {
            matrix: [[1e200, 1e200], [1e200, 1e200]],
            offset: Point::default(),
        };
        assert!(geometry.transformed(transform).is_err());
        assert!(
            geometry
                .transformed(Transform2d::uniform_scale(1e-12, Point::default()))
                .is_ok()
        );
    }
    #[test]
    fn tiny_rotated_rectangle_keeps_its_orientation() {
        let geometry = Geometry::Rectangle {
            a: Point::default(),
            b: Point::new(1e-12, 2e-12),
        };
        assert!(matches!(
            geometry
                .transformed(Transform2d::rotation(45.0, Point::default()))
                .unwrap(),
            Geometry::Polyline { .. }
        ));
    }
    fn near(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-8, "{a} != {b}");
    }
    #[test]
    fn rotation_about_pivot_and_composition() {
        let p = Point::new(2.0, 1.0);
        let t = Transform2d::rotation(90.0, Point::new(1.0, 1.0))
            .then(Transform2d::translation(5.0, 0.0));
        let q = t.apply(p);
        near(q.x, 6.0);
        near(q.y, 2.0);
    }
    #[test]
    fn rotated_rectangle_preserves_all_corners_and_length() {
        let g = Geometry::Rectangle {
            a: Point::new(0.0, 0.0),
            b: Point::new(4.0, 2.0),
        };
        let rotated = g
            .transformed(Transform2d::rotation(45.0, Point::default()))
            .unwrap();
        assert!(matches!(rotated, Geometry::Polyline { closed: true, .. }));
        near(rotated.length(), 12.0);
        let b = rotated.bounds().unwrap();
        near(b.min.x, -2.0_f64.sqrt());
        near(b.max.x, 2.0 * 2.0_f64.sqrt());
    }
    #[test]
    fn mirror_reverses_arc_angles_and_polyline_bulges() {
        let t = Transform2d::reflection(Point::default(), Point::new(1.0, 0.0)).unwrap();
        let arc = Geometry::Arc {
            center: Point::default(),
            radius: 2.0,
            start_angle_deg: 0.0,
            end_angle_deg: 90.0,
        };
        if let Geometry::Arc {
            start_angle_deg,
            end_angle_deg,
            ..
        } = arc.transformed(t).unwrap()
        {
            near(start_angle_deg, 270.0);
            near(end_angle_deg, 0.0);
        } else {
            panic!("arc changed type");
        }
        let poly = Geometry::Polyline {
            vertices: vec![
                PolylineVertex {
                    point: Point::default(),
                    bulge: 1.0,
                },
                PolylineVertex {
                    point: Point::new(2.0, 0.0),
                    bulge: 0.0,
                },
            ],
            closed: false,
        };
        if let Geometry::Polyline { vertices, .. } = poly.transformed(t).unwrap() {
            near(vertices[0].bulge, -1.0);
        }
    }
    #[test]
    fn unsupported_transform_is_transactional() {
        let mut d = Document::default();
        let id = d.add_geometry(Geometry::Circle {
            center: Point::default(),
            radius: 3.0,
        });
        let before = d.entities.clone();
        assert!(
            d.transform_entities(
                &[id],
                Transform2d {
                    matrix: [[2.0, 0.0], [0.0, 1.0]],
                    offset: Point::default()
                }
            )
            .is_err()
        );
        assert_eq!(d.entities, before);
        assert!(
            d.transform_entities(&[id, 999], Transform2d::translation(1.0, 1.0))
                .is_err()
        );
        assert_eq!(d.entities, before);
    }
    #[test]
    fn copy_preserves_layer_and_allocates_distinct_ids() {
        let mut d = Document::default();
        let layer = d.add_layer("Walls", [10, 20, 30]);
        let id = d.add_geometry(Geometry::Line {
            start: Point::default(),
            end: Point::new(5.0, 0.0),
        });
        let ids = d
            .duplicate_entities(&[id, id], Transform2d::translation(10.0, 0.0))
            .unwrap();
        assert_eq!(ids.len(), 1);
        assert_ne!(ids[0], id);
        assert_eq!(d.entities[1].layer_id, layer);
        near(d.entities[1].geometry.bounds().unwrap().min.x, 10.0);
    }
    #[test]
    fn arc_bounds_cross_zero_degrees() {
        let g = Geometry::Arc {
            center: Point::default(),
            radius: 2.0,
            start_angle_deg: 315.0,
            end_angle_deg: 45.0,
        };
        let b = g.bounds().unwrap();
        near(b.max.x, 2.0);
        near(b.min.x, 2.0_f64.sqrt());
        near(g.length(), std::f64::consts::PI);
    }
    #[test]
    fn bulged_polyline_length_bounds_and_midpoint() {
        let mut d = Document::default();
        let g = Geometry::Polyline {
            vertices: vec![
                PolylineVertex {
                    point: Point::new(-1.0, 0.0),
                    bulge: 1.0,
                },
                PolylineVertex {
                    point: Point::new(1.0, 0.0),
                    bulge: 0.0,
                },
            ],
            closed: false,
        };
        near(g.length(), std::f64::consts::PI);
        let b = g.bounds().unwrap();
        near(b.min.y, -1.0);
        near(b.max.y, 0.0);
        d.add_geometry(g);
        let s = d
            .object_snap(Point::new(0.05, -0.95), 0.1, SnapOptions::default())
            .unwrap();
        assert_eq!(s.kind, SnapKind::Midpoint);
        near(s.point.y, -1.0);
    }
    #[test]
    fn hidden_entities_do_not_snap_or_affect_visible_bounds() {
        let mut d = Document::default();
        d.layers[0].visible = false;
        d.add_geometry(Geometry::Line {
            start: Point::default(),
            end: Point::new(10.0, 0.0),
        });
        assert!(
            d.object_snap(Point::default(), 1.0, SnapOptions::default())
                .is_none()
        );
        assert!(d.drawing_bounds(true).is_none());
        assert!(d.drawing_bounds(false).is_some());
    }
    #[test]
    fn mirrored_text_is_rejected_without_mutation() {
        let mut d = Document::default();
        let id = d.add_geometry(Geometry::Text {
            position: Point::default(),
            text: "Label".into(),
            height: 3.0,
            rotation_deg: 0.0,
        });
        let before = d.entities.clone();
        assert!(
            d.transform_entities(
                &[id],
                Transform2d::reflection(Point::default(), Point::new(1.0, 0.0)).unwrap()
            )
            .is_err()
        );
        assert_eq!(d.entities, before);
    }
}
