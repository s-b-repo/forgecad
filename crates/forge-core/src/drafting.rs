//! Checked drafting operations. An operation either commits all of its entities
//! or leaves the document (including its ID allocator) unchanged.
use crate::{
    Document, Entity, Geometry, Point, PolylineVertex, arc_point, bulge_arc, edit::Transform2d,
};
use std::{collections::BTreeSet, io};

/// Bound the work and memory required by a single array operation.
pub const MAX_ARRAY_ENTITIES: usize = 10_000;
const ANGULAR_TOLERANCE: f64 = 1e-12;

impl Geometry {
    /// Create a parallel analytic entity. Positive distances go left of directed
    /// lines/open polylines and outward from circles, arcs and closed contours.
    /// Straight polyline corners use miter joins. Curved polylines retain exact
    /// arc bulges and require tangent-continuous joins; sharp curved joins,
    /// self-crossing contours, collapsing offsets, and unsupported entity types
    /// are refused.
    pub fn offset(&self, distance: f64) -> io::Result<Self> {
        self.validate()?;
        if !distance.is_finite() || distance == 0.0 {
            return Err(invalid("Offset distance must be finite and nonzero"));
        }
        let result = match self {
            Self::Line { start, end } => {
                let direction = unit_direction(*start, *end)?;
                let displacement = Point::new(-direction.y * distance, direction.x * distance);
                Self::Line {
                    start: add(*start, displacement),
                    end: add(*end, displacement),
                }
            }
            Self::Circle { center, radius } => Self::Circle {
                center: *center,
                radius: offset_radius(*radius, distance)?,
            },
            Self::Arc {
                center,
                radius,
                start_angle_deg,
                end_angle_deg,
            } => Self::Arc {
                center: *center,
                radius: offset_radius(*radius, distance)?,
                start_angle_deg: *start_angle_deg,
                end_angle_deg: *end_angle_deg,
            },
            Self::Rectangle { a, b } => {
                let min = Point::new(a.x.min(b.x) - distance, a.y.min(b.y) - distance);
                let max = Point::new(a.x.max(b.x) + distance, a.y.max(b.y) + distance);
                if a.x == b.x || a.y == b.y || max.x <= min.x || max.y <= min.y {
                    return Err(invalid("Rectangle offset collapses or reverses an edge"));
                }
                if min == Point::new(a.x.min(b.x), a.y.min(b.y))
                    && max == Point::new(a.x.max(b.x), a.y.max(b.y))
                {
                    return Err(invalid(
                        "Offset is too small to represent at these coordinates",
                    ));
                }
                Self::Rectangle { a: min, b: max }
            }
            Self::Polyline { vertices, closed } => Self::Polyline {
                vertices: offset_polyline(vertices, *closed, distance)?,
                closed: *closed,
            },
            _ => {
                return Err(unsupported(
                    "Offset supports lines, rectangles, circles, arcs, and straight or tangent-continuous curved polylines",
                ));
            }
        };
        result.validate()?;
        if result
            .bounds()
            .is_some_and(|bounds| !bounds.min.is_finite() || !bounds.max.is_finite())
        {
            return Err(invalid("Offset exceeds the geometry coordinate range"));
        }
        if result == *self {
            return Err(invalid(
                "Offset is too small to represent at these coordinates",
            ));
        }
        Ok(result)
    }
}

impl Document {
    /// Add offset copies on each source entity's layer; retain the originals.
    pub fn offset_entities(&mut self, ids: &[u64], distance: f64) -> io::Result<Vec<u64>> {
        let sources = self.drafting_sources(ids)?;
        let copies = sources
            .iter()
            .map(|entity| Ok((entity.layer_id, entity.geometry.offset(distance)?)))
            .collect::<io::Result<Vec<_>>>()?;
        self.append_drafting_copies(copies)
    }

    /// Make a rectangular array. Counts include the original row/column; return
    /// only newly created IDs. Spacing is signed and expressed in model units.
    pub fn rectangular_array_entities(
        &mut self,
        ids: &[u64],
        columns: usize,
        rows: usize,
        dx: f64,
        dy: f64,
    ) -> io::Result<Vec<u64>> {
        if columns == 0
            || rows == 0
            || !dx.is_finite()
            || !dy.is_finite()
            || (columns > 1 && dx == 0.0)
            || (rows > 1 && dy == 0.0)
        {
            return Err(invalid(
                "Array counts must be positive and active row/column spacing finite and nonzero",
            ));
        }
        let sources = self.drafting_sources(ids)?;
        let instances = columns
            .checked_mul(rows)
            .ok_or_else(|| invalid("Array count overflow"))?;
        let capacity = array_capacity(instances, sources.len())?;
        let mut copies = Vec::with_capacity(capacity);
        for row in 0..rows {
            for column in 0..columns {
                if row == 0 && column == 0 {
                    continue;
                }
                let transform = Transform2d::translation(column as f64 * dx, row as f64 * dy);
                for entity in &sources {
                    let geometry = entity.geometry.transformed(transform)?;
                    if geometry == entity.geometry {
                        return Err(invalid(
                            "Array spacing is too small to move an entity at these coordinates",
                        ));
                    }
                    copies.push((entity.layer_id, geometry));
                }
            }
        }
        self.append_drafting_copies(copies)
    }

    /// Make a polar array; `count` includes the original. A full +/-360 degree
    /// sweep has no duplicate endpoint; partial sweeps include their final angle.
    /// When rotation is disabled, the selection is translated as one rigid group:
    /// its combined bounding-box center travels around `center`.
    pub fn polar_array_entities(
        &mut self,
        ids: &[u64],
        count: usize,
        center: Point,
        sweep_deg: f64,
        rotate_items: bool,
    ) -> io::Result<Vec<u64>> {
        if count == 0
            || !center.is_finite()
            || !sweep_deg.is_finite()
            || sweep_deg == 0.0
            || sweep_deg.abs() > 360.0
        {
            return Err(invalid(
                "Polar array requires a positive count, finite center, and nonzero sweep within +/-360 degrees",
            ));
        }
        let sources = self.drafting_sources(ids)?;
        let capacity = array_capacity(count, sources.len())?;
        let mut copies = Vec::with_capacity(capacity);
        let anchor = if rotate_items {
            center
        } else {
            selection_center(self, &sources)?
        };
        let full_circle = sweep_deg.abs() == 360.0;
        let divisor = if full_circle {
            count
        } else {
            count.saturating_sub(1).max(1)
        };
        for index in 1..count {
            let rotation =
                Transform2d::rotation(sweep_deg * (index as f64 / divisor as f64), center);
            let transform = if rotate_items {
                rotation
            } else {
                let moved = rotation.apply(anchor);
                Transform2d::translation(moved.x - anchor.x, moved.y - anchor.y)
            };
            for entity in &sources {
                let geometry = entity.geometry.transformed(transform)?;
                if !rotate_items && geometry == entity.geometry {
                    return Err(invalid(
                        "Polar array translation has zero radius or is too small to move an entity",
                    ));
                }
                copies.push((entity.layer_id, geometry));
            }
        }
        self.append_drafting_copies(copies)
    }

    /// Trim a LINE at its intersection with a finite boundary LINE. The
    /// projection of `pick` onto the target chooses the side of the intersection
    /// to remove. The intersection must be strictly inside the target and on the
    /// finite boundary segment. A pick exactly on the intersection is ambiguous.
    pub fn trim_line_to_boundary(
        &mut self,
        id: u64,
        boundary_id: u64,
        pick: Point,
    ) -> io::Result<()> {
        self.edit_line_boundary(id, boundary_id, pick, false)
    }

    /// Extend the endpoint of a LINE nearest `pick` to a finite boundary LINE.
    /// Refuses parallel lines, boundaries behind the chosen endpoint, and cases
    /// that would shorten rather than extend the target.
    pub fn extend_line_to_boundary(
        &mut self,
        id: u64,
        boundary_id: u64,
        pick: Point,
    ) -> io::Result<()> {
        self.edit_line_boundary(id, boundary_id, pick, true)
    }

    fn edit_line_boundary(
        &mut self,
        id: u64,
        boundary_id: u64,
        pick: Point,
        extend: bool,
    ) -> io::Result<()> {
        self.validate()?;
        if id == boundary_id || !pick.is_finite() {
            return Err(invalid(
                "Choose distinct target/boundary lines and a finite endpoint pick",
            ));
        }
        let find = |id| {
            self.entities
                .iter()
                .find(|entity| entity.id == id)
                .ok_or_else(|| invalid(format!("Entity {id} does not exist")))
        };
        let (start, end) = line_endpoints(&find(id)?.geometry)?;
        let (a, b) = line_endpoints(&find(boundary_id)?.geometry)?;
        let (intersection, target_t, boundary_t) = line_intersection(start, end, a, b)?;
        if !(-1e-10..=1.0 + 1e-10).contains(&boundary_t) {
            return Err(invalid(
                "Intersection lies outside the finite boundary line",
            ));
        }
        let edit_start = if extend {
            let (start_distance, end_distance) = (pick.distance(start), pick.distance(end));
            if !start_distance.is_finite() || !end_distance.is_finite() {
                return Err(invalid("Endpoint pick distance overflows"));
            }
            start_distance <= end_distance
        } else {
            let direction = unit_direction(start, end)?;
            let pick_distance = dot(sub(pick, intersection), direction);
            if !pick_distance.is_finite() || pick_distance == 0.0 {
                return Err(invalid(
                    "Choose a finite point on the side to trim, away from the intersection",
                ));
            }
            pick_distance < 0.0
        };
        if extend {
            if (edit_start && target_t >= -1e-10) || (!edit_start && target_t <= 1.0 + 1e-10) {
                return Err(invalid("Boundary does not extend the chosen endpoint"));
            }
        } else if !(1e-10..1.0 - 1e-10).contains(&target_t) {
            return Err(invalid(
                "Trim intersection must be strictly inside the target line",
            ));
        }
        let replacement = Geometry::Line {
            start: if edit_start { intersection } else { start },
            end: if edit_start { end } else { intersection },
        };
        replacement.validate()?;
        let mut candidate = self.clone();
        candidate
            .entities
            .iter_mut()
            .find(|entity| entity.id == id)
            .ok_or_else(|| invalid(format!("Entity {id} does not exist")))?
            .geometry = replacement;
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }

    fn drafting_sources(&self, ids: &[u64]) -> io::Result<Vec<Entity>> {
        self.validate()?;
        let requested: BTreeSet<u64> = ids.iter().copied().collect();
        if requested.is_empty() {
            return Err(invalid("Select at least one drawing entity"));
        }
        requested
            .iter()
            .map(|id| {
                self.entities
                    .iter()
                    .find(|entity| entity.id == *id)
                    .cloned()
                    .ok_or_else(|| invalid(format!("Entity {id} does not exist")))
            })
            .collect()
    }

    fn append_drafting_copies(&mut self, copies: Vec<(u64, Geometry)>) -> io::Result<Vec<u64>> {
        if copies.len() > MAX_ARRAY_ENTITIES {
            return Err(invalid(format!(
                "Operation exceeds the {MAX_ARRAY_ENTITIES} new-entity limit"
            )));
        }
        if copies.is_empty() {
            return Ok(Vec::new());
        }
        let highest = self
            .layers
            .iter()
            .map(|entity| entity.id)
            .chain(self.entities.iter().map(|entity| entity.id))
            .chain(
                self.blocks
                    .iter()
                    .flat_map(|block| block.entities.iter().map(|entity| entity.id)),
            )
            .chain(self.solids.iter().map(|entity| entity.id))
            .chain(self.meshes.iter().map(|entity| entity.id))
            .max()
            .unwrap_or(0);
        let first_id = self.next_id.max(highest.saturating_add(1)).max(1);
        let next_id = first_id
            .checked_add(copies.len() as u64)
            .ok_or_else(|| invalid("Object ID space is exhausted"))?;
        let mut candidate = self.clone();
        let mut created = Vec::with_capacity(copies.len());
        for (index, (layer_id, geometry)) in copies.into_iter().enumerate() {
            let id = first_id + index as u64;
            candidate.entities.push(Entity {
                id,
                layer_id,
                geometry,
            });
            created.push(id);
        }
        candidate.next_id = next_id;
        candidate.validate()?;
        *self = candidate;
        Ok(created)
    }
}

fn array_capacity(instances: usize, sources: usize) -> io::Result<usize> {
    let count = instances
        .saturating_sub(1)
        .checked_mul(sources)
        .ok_or_else(|| invalid("Array count overflow"))?;
    if count > MAX_ARRAY_ENTITIES {
        return Err(invalid(format!(
            "Array exceeds the {MAX_ARRAY_ENTITIES} new-entity limit"
        )));
    }
    Ok(count)
}

fn selection_center(document: &Document, entities: &[Entity]) -> io::Result<Point> {
    let source_ids: BTreeSet<u64> = entities.iter().map(|entity| entity.id).collect();
    let mut bounds = None::<crate::edit::Bounds2d>;
    // INSERT bounds are only their insertion point. Resolve definitions and
    // nested instances so an array group is anchored to its actual contents.
    // Hidden contents still contribute: array geometry is independent of view.
    for entity in document
        .expanded_entities()?
        .into_iter()
        .filter(|entity| source_ids.contains(&entity.source_id))
    {
        let current = entity
            .geometry
            .bounds()
            .ok_or_else(|| invalid("Selection has no finite bounds"))?;
        if let Some(total) = &mut bounds {
            total.include(current.min);
            total.include(current.max);
        } else {
            bounds = Some(current);
        }
    }
    let bounds = bounds.ok_or_else(|| invalid("Selection has no finite bounds"))?;
    // Half before adding to avoid overflow for large coordinates.
    let result = Point::new(
        bounds.min.x / 2.0 + bounds.max.x / 2.0,
        bounds.min.y / 2.0 + bounds.max.y / 2.0,
    );
    if !result.is_finite() {
        return Err(invalid("Selection has no finite center"));
    }
    Ok(result)
}

fn offset_radius(radius: f64, distance: f64) -> io::Result<f64> {
    let new_radius = radius + distance;
    if !new_radius.is_finite() || new_radius <= 0.0 || new_radius == radius {
        return Err(invalid(
            "Offset radius collapses, overflows, or cannot be represented",
        ));
    }
    Ok(new_radius)
}

fn unit_direction(a: Point, b: Point) -> io::Result<Point> {
    let vector = sub(b, a);
    let length = vector.x.hypot(vector.y);
    if !length.is_finite() || length == 0.0 {
        return Err(invalid(
            "Line segments require distinct endpoints and a finite length",
        ));
    }
    Ok(Point::new(vector.x / length, vector.y / length))
}

fn offset_polyline(
    vertices: &[PolylineVertex],
    closed: bool,
    distance: f64,
) -> io::Result<Vec<PolylineVertex>> {
    if vertices.iter().any(|vertex| vertex.bulge != 0.0) {
        return offset_curved_polyline(vertices, closed, distance);
    }
    if closed && vertices.len() < 3 {
        return Err(invalid(
            "Closed offset contours require at least three vertices",
        ));
    }
    let points: Vec<Point> = vertices.iter().map(|vertex| vertex.point).collect();
    ensure_simple(&points, closed)?;
    let segment_count = if closed {
        points.len()
    } else {
        points.len() - 1
    };
    let directions = (0..segment_count)
        .map(|index| unit_direction(points[index], points[(index + 1) % points.len()]))
        .collect::<io::Result<Vec<_>>>()?;
    let area = if closed { signed_area(&points)? } else { 0.0 };
    let distance = if closed && area > 0.0 {
        -distance
    } else {
        distance
    };
    let mut result = Vec::with_capacity(points.len());
    for (index, point) in points.iter().enumerate() {
        let displacement = if !closed && index == 0 {
            let first = directions
                .first()
                .copied()
                .ok_or_else(|| invalid("Polyline offset requires at least one segment"))?;
            normal(first, distance)
        } else if !closed && index + 1 == points.len() {
            let last = directions
                .last()
                .copied()
                .ok_or_else(|| invalid("Polyline offset requires at least one segment"))?;
            normal(last, distance)
        } else {
            let previous = directions[(index + directions.len() - 1) % directions.len()];
            let next = directions[index];
            let denominator = 1.0 + dot(previous, next);
            if denominator <= ANGULAR_TOLERANCE {
                return Err(invalid(
                    "Polyline offset has a reversing or unbounded miter corner",
                ));
            }
            let normals = add(normal(previous, distance), normal(next, distance));
            Point::new(normals.x / denominator, normals.y / denominator)
        };
        let new_point = add(*point, displacement);
        if !new_point.is_finite() {
            return Err(invalid("Polyline offset overflows"));
        }
        result.push(new_point);
    }
    for index in 0..segment_count {
        if dot(
            sub(result[(index + 1) % result.len()], result[index]),
            directions[index],
        ) <= 0.0
        {
            return Err(invalid("Polyline offset collapses or reverses an edge"));
        }
    }
    ensure_simple(&result, closed)?;
    if closed && signed_area(&result)?.signum() != area.signum() {
        return Err(invalid("Polyline offset reverses its contour"));
    }
    Ok(result.into_iter().map(PolylineVertex::new).collect())
}

/// Offset a polyline whose segments are lines or circular arcs. Exact arc
/// geometry is retained by changing each arc's radius; joins must be tangent
/// so adjacent offset segments meet at the common normal point. Sharp curved
/// joins are refused rather than approximated with an invented miter.
fn offset_curved_polyline(
    vertices: &[PolylineVertex],
    closed: bool,
    distance: f64,
) -> io::Result<Vec<PolylineVertex>> {
    if vertices.len() < 2 {
        return Err(invalid(
            "Curved offset contours require at least two vertices",
        ));
    }
    let segment_count = if closed {
        vertices.len()
    } else {
        vertices.len() - 1
    };
    let mut tangents = Vec::with_capacity(segment_count);
    for index in 0..segment_count {
        let start = vertices[index].point;
        let end = vertices[(index + 1) % vertices.len()].point;
        tangents.push((
            segment_tangent(start, end, vertices[index].bulge, false)?,
            segment_tangent(start, end, vertices[index].bulge, true)?,
        ));
    }
    for index in 0..vertices.len() {
        if !closed && (index == 0 || index + 1 == vertices.len()) {
            continue;
        }
        let previous = tangents[if index == 0 {
            segment_count - 1
        } else {
            index - 1
        }]
        .1;
        let next = tangents[index % segment_count].0;
        if !same_direction(previous, next) {
            return Err(unsupported(
                "Offset of curved polylines requires tangent-continuous joins; split the polyline or add tangent arcs",
            ));
        }
    }
    let distance = if closed && curved_signed_area(vertices)? > 0.0 {
        -distance
    } else {
        distance
    };
    let mut result = Vec::with_capacity(vertices.len());
    for (index, vertex) in vertices.iter().enumerate() {
        let tangent = if !closed && index == 0 {
            tangents[0].0
        } else if !closed && index + 1 == vertices.len() {
            tangents[segment_count - 1].1
        } else {
            tangents[if index == 0 {
                segment_count - 1
            } else {
                index - 1
            }]
            .1
        };
        let point = add(vertex.point, normal(tangent, distance));
        if !point.is_finite() {
            return Err(invalid("Curved polyline offset overflows"));
        }
        result.push(PolylineVertex {
            point,
            bulge: vertex.bulge,
        });
    }
    for index in 0..segment_count {
        let vertex = vertices[index];
        if vertex.bulge == 0.0 {
            unit_direction(vertex.point, vertices[(index + 1) % vertices.len()].point)?;
            continue;
        }
        let (_, radius, _, _) = bulge_arc(
            vertex.point,
            vertices[(index + 1) % vertices.len()].point,
            vertex.bulge,
        )
        .ok_or_else(|| invalid("Invalid curved polyline segment"))?;
        let offset_radius = radius - distance;
        if !offset_radius.is_finite() || offset_radius <= 0.0 {
            return Err(invalid(
                "Curved polyline offset collapses or reverses an arc radius",
            ));
        }
    }
    // Check the sampled path for self-intersections and collapsed segments.
    // Arc bulges are still emitted analytically; this is validation only.
    let sampled = sampled_curve_vertices(&result, closed)?;
    ensure_simple(&sampled, closed)?;
    Ok(result)
}

fn segment_tangent(start: Point, end: Point, bulge: f64, at_end: bool) -> io::Result<Point> {
    if bulge == 0.0 {
        return unit_direction(start, end);
    }
    let (_, _, angle, sweep) =
        bulge_arc(start, end, bulge).ok_or_else(|| invalid("Invalid curved polyline segment"))?;
    let radial = if at_end { angle + sweep } else { angle };
    let tangent = radial + if sweep > 0.0 { 90.0 } else { -90.0 };
    let (sin, cos) = tangent.to_radians().sin_cos();
    let direction = Point::new(cos, sin);
    if !direction.is_finite() {
        return Err(invalid("Curved polyline tangent exceeds numeric range"));
    }
    Ok(direction)
}

fn same_direction(a: Point, b: Point) -> bool {
    dot(a, b) >= 1.0 - 1e-9 && cross(a, b).abs() <= 1e-9
}

fn sampled_curve_vertices(vertices: &[PolylineVertex], closed: bool) -> io::Result<Vec<Point>> {
    let count = if closed {
        vertices.len()
    } else {
        vertices.len() - 1
    };
    let mut result = Vec::with_capacity(vertices.len() * 4);
    result.push(vertices[0].point);
    for index in 0..count {
        let vertex = vertices[index];
        let end = vertices[(index + 1) % vertices.len()].point;
        if vertex.bulge == 0.0 {
            result.push(end);
        } else {
            let (center, radius, angle, sweep) = bulge_arc(vertex.point, end, vertex.bulge)
                .ok_or_else(|| invalid("Invalid curved polyline segment"))?;
            let steps = ((sweep.abs() / 10.0).ceil() as usize).clamp(1, 64);
            for step in 1..=steps {
                result.push(arc_point(
                    center,
                    radius,
                    angle + sweep * step as f64 / steps as f64,
                ));
            }
        }
        if result.len() > 2048 {
            return Err(unsupported(
                "Curved polyline offset exceeds the 2048 sampled segment limit",
            ));
        }
    }
    if closed {
        result.pop();
    }
    Ok(result)
}

/// Signed area of a closed polyline including the circular segment between
/// each chord and its arc. Two opposite semicircles therefore describe a full
/// circle even though the chord polygon alone has zero area.
fn curved_signed_area(vertices: &[PolylineVertex]) -> io::Result<f64> {
    let mut area = 0.0;
    for index in 0..vertices.len() {
        let start = vertices[index];
        let end = vertices[(index + 1) % vertices.len()];
        area += cross(start.point, end.point) / 2.0;
        if start.bulge != 0.0 {
            let (_, radius, _, sweep) = bulge_arc(start.point, end.point, start.bulge)
                .ok_or_else(|| invalid("Invalid curved polyline segment"))?;
            let theta = sweep.to_radians();
            area += 0.5 * radius * radius * (theta - theta.sin());
        }
    }
    if !area.is_finite() || area == 0.0 {
        Err(invalid(
            "Closed curved contour must enclose a finite nonzero area",
        ))
    } else {
        Ok(area)
    }
}

fn signed_area(points: &[Point]) -> io::Result<f64> {
    // Translate to the first vertex before taking cross products.
    let origin = points[0];
    let area = (1..points.len() - 1)
        .map(|index| cross(sub(points[index], origin), sub(points[index + 1], origin)) / 2.0)
        .sum::<f64>();
    if !area.is_finite() || area == 0.0 {
        return Err(invalid("Closed contour must enclose a finite nonzero area"));
    }
    Ok(area)
}

fn ensure_simple(points: &[Point], closed: bool) -> io::Result<()> {
    // Quadratic intersection checking is deliberately bounded. Larger contours
    // require an intersection index rather than unbounded interactive work.
    if points.len() > 2048 {
        return Err(unsupported("Polyline offset is limited to 2048 vertices"));
    }
    let count = if closed {
        points.len()
    } else {
        points.len() - 1
    };
    for i in 0..count {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        unit_direction(a, b)?;
        for j in i + 1..count {
            if j == i + 1 || (closed && i == 0 && j == count - 1) {
                continue;
            }
            let c = points[j];
            let d = points[(j + 1) % points.len()];
            if segments_intersect(a, b, c, d)? {
                return Err(invalid(
                    "Polyline offset requires a simple, non-self-intersecting contour",
                ));
            }
        }
    }
    Ok(())
}

fn segments_intersect(a: Point, b: Point, c: Point, d: Point) -> io::Result<bool> {
    let ab = sub(b, a);
    let cd = sub(d, c);
    let values = [
        cross(ab, sub(c, a)),
        cross(ab, sub(d, a)),
        cross(cd, sub(a, c)),
        cross(cd, sub(b, c)),
    ];
    if values.iter().any(|value| !value.is_finite()) {
        return Err(invalid("Contour intersection calculation overflows"));
    }
    if values[0] == 0.0 && on_segment(c, a, b)
        || values[1] == 0.0 && on_segment(d, a, b)
        || values[2] == 0.0 && on_segment(a, c, d)
        || values[3] == 0.0 && on_segment(b, c, d)
    {
        return Ok(true);
    }
    Ok(values[0].signum() != values[1].signum() && values[2].signum() != values[3].signum())
}
fn on_segment(point: Point, a: Point, b: Point) -> bool {
    point.x >= a.x.min(b.x)
        && point.x <= a.x.max(b.x)
        && point.y >= a.y.min(b.y)
        && point.y <= a.y.max(b.y)
}

fn line_endpoints(geometry: &Geometry) -> io::Result<(Point, Point)> {
    match geometry {
        Geometry::Line { start, end } => {
            unit_direction(*start, *end)?;
            Ok((*start, *end))
        }
        _ => Err(unsupported(
            "Trim and extend currently require LINE targets and LINE boundaries",
        )),
    }
}

fn line_intersection(a: Point, b: Point, c: Point, d: Point) -> io::Result<(Point, f64, f64)> {
    let first = unit_direction(a, b)?;
    let second = unit_direction(c, d)?;
    let denominator = cross(first, second);
    if denominator.abs() <= ANGULAR_TOLERANCE {
        return Err(invalid(
            "Parallel or coincident lines have no unique intersection",
        ));
    }
    let delta = sub(c, a);
    let along_first = cross(delta, second) / denominator;
    let along_second = cross(delta, first) / denominator;
    let target_t = along_first / a.distance(b);
    let boundary_t = along_second / c.distance(d);
    let intersection = add(a, Point::new(first.x * along_first, first.y * along_first));
    if !intersection.is_finite() || !target_t.is_finite() || !boundary_t.is_finite() {
        return Err(invalid("Line intersection calculation overflows"));
    }
    Ok((intersection, target_t, boundary_t))
}

fn normal(direction: Point, distance: f64) -> Point {
    Point::new(-direction.y * distance, direction.x * distance)
}
fn add(a: Point, b: Point) -> Point {
    Point::new(a.x + b.x, a.y + b.y)
}
fn sub(a: Point, b: Point) -> Point {
    Point::new(a.x - b.x, a.y - b.y)
}
fn dot(a: Point, b: Point) -> f64 {
    a.x * b.x + a.y * b.y
}
fn cross(a: Point, b: Point) -> f64 {
    a.x * b.y - a.y * b.x
}
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}
fn unsupported(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dimension::{Dimension, DimensionStyle, LinearDimension, LinearDimensionKind};

    fn point(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }
    fn line(a: [f64; 2], b: [f64; 2]) -> Geometry {
        Geometry::Line {
            start: point(a[0], a[1]),
            end: point(b[0], b[1]),
        }
    }
    fn near(actual: Point, expected: Point) {
        assert!(
            actual.distance(expected) < 1e-8,
            "{actual:?} != {expected:?}"
        );
    }
    // Offset vertices are derived through `hypot`-normalized directions, whose
    // last bit differs by an ulp between libm (glibc) and Miri's libm. Compare
    // bounds analytically but with a sub-nanometre tolerance so the intent
    // (outward/inward miter extent) is still enforced.
    fn assert_bounds(actual: Option<crate::edit::Bounds2d>, expected: crate::edit::Bounds2d) {
        let actual = actual.expect("offset produced no bounds");
        let close = |a: f64, b: f64| (a - b).abs() <= 1e-9;
        assert!(
            close(actual.min.x, expected.min.x)
                && close(actual.min.y, expected.min.y)
                && close(actual.max.x, expected.max.x)
                && close(actual.max.y, expected.max.y),
            "bounds {actual:?} not within 1e-9 of {expected:?}"
        );
    }
    /// True when two coordinates are equal or differ by at most a few ulps of
    /// their magnitude, i.e. the floating-point noise floor of a `hypot`
    /// normalisation rather than a real geometric difference.
    fn ulp_close(a: f64, b: f64) -> bool {
        if a == b {
            return true;
        }
        let scale = a.abs().max(b.abs());
        (a - b).abs() <= 4.0 * scale * f64::EPSILON
    }
    /// Offset vertices are derived through normalized directions, whose last bit
    /// differs by an ulp between libm (glibc) and Miri's libm. Compare offset
    /// polylines analytically: identical topology and bit-exact bulges with
    /// coordinates inside the noise floor, so direction and extent are still
    /// enforced without demanding bit equality across libms.
    fn assert_polyline_close(actual: &Geometry, expected: &Geometry, label: &str) {
        let Geometry::Polyline {
            vertices: actual_vertices,
            closed: actual_closed,
        } = actual
        else {
            panic!("{label}: offset is not a polyline: {actual:?}");
        };
        let Geometry::Polyline {
            vertices: expected_vertices,
            closed: expected_closed,
        } = expected
        else {
            panic!("{label}: expected geometry is not a polyline: {expected:?}");
        };
        assert_eq!(actual_closed, expected_closed, "{label}: closure changed");
        assert_eq!(
            actual_vertices.len(),
            expected_vertices.len(),
            "{label}: vertex count changed"
        );
        for (index, (actual, expected)) in actual_vertices
            .iter()
            .zip(expected_vertices.iter())
            .enumerate()
        {
            assert_eq!(
                actual.bulge.to_bits(),
                expected.bulge.to_bits(),
                "{label}: bulge {index} changed"
            );
            assert!(
                ulp_close(actual.point.x, expected.point.x)
                    && ulp_close(actual.point.y, expected.point.y),
                "{label}: vertex {index} {:?} != {:?}",
                actual.point,
                expected.point
            );
        }
    }
    /// Extend/trim endpoints come from a line intersection parametrisation
    /// whose last bit differs by an ulp between libm (glibc) and Miri's libm.
    /// Compare endpoint coordinates analytically so the boundary point is still
    /// pinned down to the noise floor instead of demanding bit equality.
    fn assert_line_close(actual: &Geometry, expected: &Geometry, label: &str) {
        let Geometry::Line {
            start: actual_start,
            end: actual_end,
        } = actual
        else {
            panic!("{label}: expected a line, got {actual:?}");
        };
        let Geometry::Line {
            start: expected_start,
            end: expected_end,
        } = expected
        else {
            panic!("{label}: expected geometry is not a line: {expected:?}");
        };
        for (coord, expected, name) in [
            (actual_start.x, expected_start.x, "start.x"),
            (actual_start.y, expected_start.y, "start.y"),
            (actual_end.x, expected_end.x, "end.x"),
            (actual_end.y, expected_end.y, "end.y"),
        ] {
            assert!(
                ulp_close(coord, expected),
                "{label}: {name} {coord:?} != {expected:?}"
            );
        }
    }
    fn polyline(points: &[[f64; 2]], closed: bool) -> Geometry {
        Geometry::Polyline {
            vertices: points
                .iter()
                .map(|p| PolylineVertex::new(point(p[0], p[1])))
                .collect(),
            closed,
        }
    }

    #[test]
    fn analytic_offsets_use_documented_direction_and_preserve_arc_angles() {
        assert_eq!(
            line([0.0, 0.0], [10.0, 0.0]).offset(2.0).unwrap(),
            line([0.0, 2.0], [10.0, 2.0])
        );
        assert_eq!(
            line([10.0, 0.0], [0.0, 0.0]).offset(2.0).unwrap(),
            line([10.0, -2.0], [0.0, -2.0])
        );
        assert_eq!(
            Geometry::Circle {
                center: point(4.0, 5.0),
                radius: 8.0
            }
            .offset(-3.0)
            .unwrap(),
            Geometry::Circle {
                center: point(4.0, 5.0),
                radius: 5.0
            }
        );
        assert_eq!(
            Geometry::Arc {
                center: point(4.0, 5.0),
                radius: 8.0,
                start_angle_deg: 350.0,
                end_angle_deg: 70.0
            }
            .offset(3.0)
            .unwrap(),
            Geometry::Arc {
                center: point(4.0, 5.0),
                radius: 11.0,
                start_angle_deg: 350.0,
                end_angle_deg: 70.0
            }
        );
        assert_eq!(
            Geometry::Rectangle {
                a: point(10.0, 5.0),
                b: point(0.0, 0.0)
            }
            .offset(2.0)
            .unwrap(),
            Geometry::Rectangle {
                a: point(-2.0, -2.0),
                b: point(12.0, 7.0)
            }
        );
    }

    #[test]
    fn curved_polyline_offsets_keep_arcs_and_respect_closed_orientation() {
        let circle = Geometry::Polyline {
            vertices: vec![
                PolylineVertex {
                    point: point(0.0, 0.0),
                    bulge: 1.0,
                },
                PolylineVertex {
                    point: point(4.0, 0.0),
                    bulge: 1.0,
                },
            ],
            closed: true,
        };
        let outward = circle.offset(1.0).unwrap();
        let Geometry::Polyline {
            vertices,
            closed: true,
        } = &outward
        else {
            panic!("curved offset changed the entity type");
        };
        near(vertices[0].point, point(-1.0, 0.0));
        near(vertices[1].point, point(5.0, 0.0));
        assert_eq!(vertices[0].bulge, 1.0);
        assert_eq!(vertices[1].bulge, 1.0);
        let bounds = outward.bounds().unwrap();
        near(bounds.min, point(-1.0, -3.0));
        near(bounds.max, point(5.0, 3.0));
        let inward = circle.offset(-1.0).unwrap();
        let bounds = inward.bounds().unwrap();
        near(bounds.min, point(1.0, -1.0));
        near(bounds.max, point(3.0, 1.0));
    }

    #[test]
    fn straight_polyline_offsets_miter_corners_and_preserve_outward_direction() {
        assert_polyline_close(
            &polyline(&[[0.0, 0.0], [10.0, 0.0], [10.0, 8.0]], false)
                .offset(2.0)
                .unwrap(),
            &polyline(&[[0.0, 2.0], [8.0, 2.0], [8.0, 8.0]], false),
            "open miter offset",
        );
        let ccw = polyline(&[[0.0, 0.0], [10.0, 0.0], [10.0, 8.0], [0.0, 8.0]], true);
        let cw = polyline(&[[0.0, 8.0], [10.0, 8.0], [10.0, 0.0], [0.0, 0.0]], true);
        let expected_bounds = crate::edit::Bounds2d {
            min: point(-2.0, -2.0),
            max: point(12.0, 10.0),
        };
        assert_bounds(ccw.offset(2.0).unwrap().bounds(), expected_bounds);
        assert_bounds(cw.offset(2.0).unwrap().bounds(), expected_bounds);
        assert_bounds(
            ccw.offset(-2.0).unwrap().bounds(),
            crate::edit::Bounds2d {
                min: point(2.0, 2.0),
                max: point(8.0, 6.0),
            },
        );
        assert_eq!(
            polyline(&[[0.0, 0.0], [5.0, 0.0], [10.0, 0.0]], false)
                .offset(2.0)
                .unwrap(),
            polyline(&[[0.0, 2.0], [5.0, 2.0], [10.0, 2.0]], false)
        );
    }

    #[test]
    fn offsets_refuse_collapses_crossings_and_unrepresentable_distances() {
        let circle = Geometry::Circle {
            center: point(0.0, 0.0),
            radius: 3.0,
        };
        for distance in [0.0, f64::NAN, f64::INFINITY, -3.0, -5.0, 1e-300] {
            assert!(circle.offset(distance).is_err());
        }
        for shape in [
            line([0.0, 0.0], [0.0, 0.0]),
            Geometry::Rectangle {
                a: point(0.0, 0.0),
                b: point(10.0, 4.0),
            },
            polyline(&[[0.0, 0.0], [10.0, 0.0], [10.0, 4.0], [0.0, 4.0]], true),
            polyline(&[[0.0, 0.0], [10.0, 0.0], [0.0, 0.0]], false),
            polyline(&[[0.0, 0.0], [10.0, 10.0], [0.0, 10.0], [10.0, 0.0]], false),
        ] {
            assert!(shape.offset(-3.0).is_err(), "{shape:?}");
        }
        let curved = Geometry::Polyline {
            vertices: vec![
                PolylineVertex {
                    point: point(0.0, 0.0),
                    bulge: 1.0,
                },
                PolylineVertex::new(point(5.0, 0.0)),
            ],
            closed: false,
        };
        let Geometry::Polyline {
            vertices,
            closed: false,
        } = curved.offset(1.0).unwrap()
        else {
            panic!("curved offset changed the entity type");
        };
        near(vertices[0].point, point(1.0, 0.0));
        near(vertices[1].point, point(4.0, 0.0));
        assert_eq!(vertices[0].bulge, 1.0);
        assert_eq!(vertices[1].bulge, 0.0);
        let sharp = Geometry::Polyline {
            vertices: vec![
                PolylineVertex {
                    point: point(0.0, 0.0),
                    bulge: 1.0,
                },
                PolylineVertex::new(point(4.0, 0.0)),
                PolylineVertex::new(point(8.0, 0.0)),
            ],
            closed: false,
        };
        assert_eq!(
            sharp.offset(1.0).unwrap_err().kind(),
            io::ErrorKind::Unsupported
        );
        let huge = Geometry::Rectangle {
            a: point(1e20 + 1e8, 1e20 + 1e8),
            b: point(1e20, 1e20),
        };
        assert!(huge.offset(1e-6).is_err());
        assert!(line([-1e308, 0.0], [1e308, 0.0]).offset(2.0).is_err());
    }

    #[test]
    fn offset_selection_is_atomic_preserves_layers_and_deduplicates_ids() {
        let mut document = Document::default();
        let layer = document.add_layer("Offsets", [255, 0, 0]).unwrap();
        let id = document
            .add_geometry(line([0.0, 0.0], [10.0, 0.0]))
            .unwrap();
        let unsupported_id = document
            .add_geometry(Geometry::Point {
                position: point(0.0, 0.0),
            })
            .unwrap();
        let before = document.clone();
        assert!(
            document
                .offset_entities(&[id, unsupported_id], 2.0)
                .is_err()
        );
        assert_eq!(document, before);
        let copies = document.offset_entities(&[id, id], 2.0).unwrap();
        assert_eq!(copies.len(), 1);
        assert_eq!(copies[0], before.next_id);
        assert_eq!(document.entities.last().unwrap().layer_id, layer);
        assert_eq!(
            document.entities.last().unwrap().geometry,
            line([0.0, 2.0], [10.0, 2.0])
        );
        assert_eq!(document.entities[0], before.entities[0]);
    }

    #[test]
    fn rectangular_arrays_preserve_sources_layers_and_signed_spacing() {
        let mut document = Document::default();
        let layer = document.add_layer("Array", [2, 3, 4]).unwrap();
        let id = document
            .add_geometry(Geometry::Circle {
                center: point(2.0, 3.0),
                radius: 1.0,
            })
            .unwrap();
        let ids = document
            .rectangular_array_entities(&[id, id], 3, 2, -10.0, 8.0)
            .unwrap();
        assert_eq!(ids.len(), 5);
        let expected = [
            point(2.0, 3.0),
            point(-8.0, 3.0),
            point(-18.0, 3.0),
            point(2.0, 11.0),
            point(-8.0, 11.0),
            point(-18.0, 11.0),
        ];
        for (entity, center) in document.entities.iter().zip(expected) {
            assert_eq!(entity.layer_id, layer);
            assert_eq!(
                entity.geometry,
                Geometry::Circle {
                    center,
                    radius: 1.0
                }
            );
        }
        document.validate().unwrap();
        assert_eq!(
            document
                .rectangular_array_entities(&[id], 1, 1, 0.0, 0.0)
                .unwrap(),
            Vec::<u64>::new()
        );
    }

    #[test]
    fn polar_full_circle_excludes_endpoint_and_partial_sweep_includes_it() {
        let mut document = Document::default();
        let id = document
            .add_geometry(Geometry::Circle {
                center: point(10.0, 0.0),
                radius: 1.0,
            })
            .unwrap();
        document
            .polar_array_entities(&[id], 4, Point::default(), 360.0, true)
            .unwrap();
        for (entity, expected) in document.entities.iter().zip([
            point(10.0, 0.0),
            point(0.0, 10.0),
            point(-10.0, 0.0),
            point(0.0, -10.0),
        ]) {
            if let Geometry::Circle { center, .. } = entity.geometry {
                near(center, expected);
            } else {
                panic!();
            }
        }
        let mut document = Document::default();
        let id = document
            .add_geometry(Geometry::Point {
                position: point(10.0, 0.0),
            })
            .unwrap();
        document
            .polar_array_entities(&[id], 3, Point::default(), -180.0, true)
            .unwrap();
        let Geometry::Point { position } = document.entities[2].geometry else {
            panic!()
        };
        near(position, point(-10.0, 0.0));
        let Geometry::Point { position } = document.entities[1].geometry else {
            panic!()
        };
        near(position, point(0.0, -10.0));
    }

    #[test]
    fn polar_no_rotation_keeps_whole_selection_shape_and_orientation() {
        let mut document = Document::default();
        let a = document
            .add_geometry(line([8.0, 0.0], [12.0, 0.0]))
            .unwrap();
        let b = document
            .add_geometry(line([8.0, 2.0], [12.0, 2.0]))
            .unwrap();
        document
            .polar_array_entities(&[a, b], 2, Point::default(), 90.0, false)
            .unwrap();
        let Geometry::Line { start, end } = document.entities[2].geometry else {
            panic!()
        };
        near(start, point(-3.0, 9.0));
        near(end, point(1.0, 9.0));
        let Geometry::Line { start, end } = document.entities[3].geometry else {
            panic!()
        };
        near(start, point(-3.0, 11.0));
        near(end, point(1.0, 11.0));
    }

    #[test]
    fn polar_no_rotation_anchors_nested_blocks_to_contents_even_on_hidden_layers() {
        let mut document = Document::default();
        let source = document
            .add_geometry(line([100.0, 0.0], [120.0, 0.0]))
            .unwrap();
        let inner = document
            .define_block_from_selection("Inner", Point::default(), &[source])
            .unwrap();
        let outer = document
            .define_block_from_selection("Outer", Point::default(), &[inner])
            .unwrap();
        document.layers[0].visible = false;
        let ids = document
            .polar_array_entities(&[outer], 2, Point::default(), 90.0, false)
            .unwrap();
        let expanded = document.expanded_entities().unwrap();
        let copy = expanded
            .iter()
            .find(|entity| entity.source_id == ids[0])
            .unwrap();
        let Geometry::Line { start, end } = copy.geometry else {
            panic!()
        };
        near(start, point(-10.0, 110.0));
        near(end, point(10.0, 110.0));
        assert!(!copy.visible);
    }

    #[test]
    fn copies_refuse_translations_too_small_to_represent_and_offset_extent_overflow() {
        let mut document = Document::default();
        let id = document
            .add_geometry(Geometry::Circle {
                center: point(1e20, 1e20),
                radius: 1e8,
            })
            .unwrap();
        let before = document.clone();
        assert!(
            document
                .rectangular_array_entities(&[id], 2, 1, 1e-8, 0.0)
                .is_err()
        );
        assert!(
            document
                .polar_array_entities(&[id], 2, point(1e20, 1e20), 90.0, false)
                .is_err()
        );
        assert_eq!(
            document, before,
            "refused arrays must not touch the document"
        );
        // A 1e-30 degree sweep cannot displace a 1e20-magnitude entity by more
        // than one ulp, so it is refused — unless libm's `cos` of such a tiny
        // angle is one ulp below 1.0 (Miri's libm) instead of exactly 1.0
        // (glibc), which makes the sub-ulp translation representable. Accept
        // either outcome, but require that a copy never leaves the coordinate's
        // noise floor: the tool must not silently move geometry it was asked to
        // leave in place.
        match document.polar_array_entities(&[id], 2, Point::default(), 1e-30, false) {
            Ok(_) => {
                assert_eq!(document.entities.len(), before.entities.len() + 1);
                let Geometry::Circle {
                    center: copied,
                    radius: copied_radius,
                } = document.entities.last().expect("copy appended").geometry
                else {
                    panic!("array copy kept the circle shape")
                };
                let Geometry::Circle {
                    center: source,
                    radius: source_radius,
                } = before.entities.first().expect("source entity").geometry
                else {
                    panic!("source entity kept the circle shape")
                };
                assert_eq!(copied_radius, source_radius);
                assert!(
                    ulp_close(copied.x, source.x) && ulp_close(copied.y, source.y),
                    "sub-ulp sweep moved the copy beyond the coordinate noise floor: {copied:?} vs {source:?}"
                );
            }
            Err(_) => assert_eq!(document, before),
        }
        assert!(
            Geometry::Circle {
                center: point(f64::MAX, 0.0),
                radius: f64::MAX
            }
            .offset(-f64::MAX / 2.0)
            .is_err()
        );
    }

    #[test]
    fn arrays_preserve_block_references_and_semantic_dimensions() {
        let mut document = Document::default();
        let source = document.add_geometry(line([1.0, 0.0], [5.0, 0.0])).unwrap();
        let insert = document
            .define_block_from_selection("Part", point(1.0, 0.0), &[source])
            .unwrap();
        let dimension = document
            .add_geometry(Geometry::Dimension {
                dimension: Dimension::Linear(LinearDimension {
                    kind: LinearDimensionKind::Aligned,
                    a: point(1.0, 0.0),
                    b: point(5.0, 0.0),
                    line_point: point(2.0, 10.0),
                    style: DimensionStyle::default(),
                }),
            })
            .unwrap();
        document
            .polar_array_entities(&[insert, dimension], 2, Point::default(), 90.0, true)
            .unwrap();
        let Geometry::Insert {
            ref block_name,
            position,
            rotation_deg,
            scale,
        } = document.entities[2].geometry
        else {
            panic!()
        };
        assert_eq!(block_name, "Part");
        near(position, point(0.0, 1.0));
        assert!((rotation_deg - 90.0).abs() < 1e-10);
        // The instance scale is derived as hypot(cos, sin) of the rotation
        // matrix: exactly 1.0 on glibc but 1 ulp off on Miri's libm. Compare
        // analytically (like the rotation/measurement tolerances below) while
        // still rejecting any real scaling of the block.
        assert!((scale - 1.0).abs() < 1e-10, "scale {scale} != 1.0");
        let Geometry::Dimension { ref dimension } = document.entities[3].geometry else {
            panic!()
        };
        assert!((dimension.measurement().unwrap() - 4.0).abs() < 1e-10);
        document.validate().unwrap();
        assert_eq!(
            Document::from_json(&document.to_json().unwrap()).unwrap(),
            document
        );
    }

    #[test]
    fn array_invalid_parameters_or_missing_sources_leave_document_unchanged() {
        let mut document = Document::default();
        let id = document.add_geometry(line([1.0, 0.0], [4.0, 0.0])).unwrap();
        let before = document.clone();
        for (columns, rows, dx, dy) in [
            (0, 2, 1.0, 1.0),
            (2, 0, 1.0, 1.0),
            (2, 1, 0.0, 0.0),
            (1, 2, 0.0, 0.0),
            (usize::MAX, 2, 1.0, 1.0),
            (MAX_ARRAY_ENTITIES + 2, 1, 1.0, 1.0),
            (4, 1, f64::MAX, 0.0),
        ] {
            assert!(
                document
                    .rectangular_array_entities(&[id], columns, rows, dx, dy)
                    .is_err()
            );
            assert_eq!(document, before);
        }
        for sweep in [0.0, 361.0, -361.0, f64::NAN, f64::INFINITY] {
            assert!(
                document
                    .polar_array_entities(&[id], 4, Point::default(), sweep, true)
                    .is_err()
            );
            assert_eq!(document, before);
        }
        assert!(
            document
                .polar_array_entities(&[id], 0, Point::default(), 360.0, true)
                .is_err()
        );
        assert!(
            document
                .polar_array_entities(&[id], 3, point(f64::NAN, 0.0), 360.0, true)
                .is_err()
        );
        assert!(
            document
                .rectangular_array_entities(&[999], 2, 2, 1.0, 1.0)
                .is_err()
        );
        assert!(
            document
                .rectangular_array_entities(&[], 2, 2, 1.0, 1.0)
                .is_err()
        );
        assert_eq!(document, before);
    }

    #[test]
    fn drafting_copy_allocation_checks_id_exhaustion_without_panicking() {
        let mut document = Document::default();
        let id = document
            .add_geometry(line([0.0, 0.0], [10.0, 0.0]))
            .unwrap();
        document.entities[0].id = u64::MAX - 1;
        let before = document.clone();
        document.validate().unwrap();
        assert!(
            document
                .rectangular_array_entities(&[u64::MAX - 1], 2, 1, 1.0, 0.0)
                .is_err()
        );
        assert!(document.offset_entities(&[u64::MAX - 1], 1.0).is_err());
        assert_eq!(document, before);
        assert_ne!(id, u64::MAX - 1);
    }

    #[test]
    fn trim_uses_chosen_endpoint_retains_boundary_and_can_be_serialized() {
        let mut document = Document::default();
        let target = document
            .add_geometry(line([0.0, 0.0], [10.0, 0.0]))
            .unwrap();
        let boundary = document
            .add_geometry(line([4.0, -3.0], [4.0, 3.0]))
            .unwrap();
        let original_boundary = document.entities[1].clone();
        document
            .trim_line_to_boundary(target, boundary, point(1.0, 0.0))
            .unwrap();
        assert_eq!(document.entities[0].geometry, line([4.0, 0.0], [10.0, 0.0]));
        assert_eq!(document.entities[1], original_boundary);
        let dxf = crate::dxf::write(&document).unwrap();
        assert_eq!(
            crate::dxf::parse(&dxf).unwrap().document.entities[0].geometry,
            line([4.0, 0.0], [10.0, 0.0])
        );
        let mut document = Document::default();
        let target = document
            .add_geometry(line([0.0, 0.0], [10.0, 0.0]))
            .unwrap();
        let boundary = document
            .add_geometry(line([4.0, -3.0], [4.0, 3.0]))
            .unwrap();
        document
            .trim_line_to_boundary(target, boundary, point(9.0, 0.0))
            .unwrap();
        assert_eq!(document.entities[0].geometry, line([0.0, 0.0], [4.0, 0.0]));
    }

    #[test]
    fn extend_handles_both_ends_and_oblique_boundaries() {
        let mut document = Document::default();
        let target = document
            .add_geometry(line([0.0, 0.0], [10.0, 0.0]))
            .unwrap();
        let right = document
            .add_geometry(line([12.0, -3.0], [18.0, 3.0]))
            .unwrap();
        let left = document
            .add_geometry(line([-8.0, -3.0], [-2.0, 3.0]))
            .unwrap();
        document
            .extend_line_to_boundary(target, right, point(9.0, 0.0))
            .unwrap();
        assert_line_close(
            &document.entities[0].geometry,
            &line([0.0, 0.0], [15.0, 0.0]),
            "right extension",
        );
        document
            .extend_line_to_boundary(target, left, point(0.0, 0.0))
            .unwrap();
        assert_line_close(
            &document.entities[0].geometry,
            &line([-5.0, 0.0], [15.0, 0.0]),
            "left extension",
        );
    }

    #[test]
    fn trim_picks_side_relative_to_intersection_rather_than_nearest_endpoint() {
        let mut document = Document::default();
        let target = document
            .add_geometry(line([0.0, 0.0], [10.0, 0.0]))
            .unwrap();
        let boundary = document
            .add_geometry(line([9.0, -3.0], [9.0, 3.0]))
            .unwrap();
        let before = document.clone();
        assert!(
            document
                .trim_line_to_boundary(target, boundary, point(9.0, 1.0))
                .is_err()
        );
        assert_eq!(document, before);
        document
            .trim_line_to_boundary(target, boundary, point(8.0, 1.0))
            .unwrap();
        assert_eq!(document.entities[0].geometry, line([9.0, 0.0], [10.0, 0.0]));
        let mut document = Document::default();
        let target = document
            .add_geometry(line([10.0, 0.0], [0.0, 0.0]))
            .unwrap();
        let boundary = document
            .add_geometry(line([1.0, -3.0], [1.0, 3.0]))
            .unwrap();
        document
            .trim_line_to_boundary(target, boundary, point(2.0, 0.0))
            .unwrap();
        assert_eq!(document.entities[0].geometry, line([1.0, 0.0], [0.0, 0.0]));
    }

    #[test]
    fn trim_extend_refuse_wrong_sides_parallel_missing_and_finite_boundary_misses() {
        let mut document = Document::default();
        let target = document
            .add_geometry(line([0.0, 0.0], [10.0, 0.0]))
            .unwrap();
        let inside = document
            .add_geometry(line([4.0, -3.0], [4.0, 3.0]))
            .unwrap();
        let outside = document
            .add_geometry(line([15.0, -3.0], [15.0, 3.0]))
            .unwrap();
        let parallel = document
            .add_geometry(line([0.0, 1.0], [10.0, 1.0]))
            .unwrap();
        let missed = document.add_geometry(line([4.0, 3.0], [4.0, 5.0])).unwrap();
        let before = document.clone();
        assert!(
            document
                .trim_line_to_boundary(target, outside, point(0.0, 0.0))
                .is_err()
        );
        assert!(
            document
                .extend_line_to_boundary(target, inside, point(0.0, 0.0))
                .is_err()
        );
        assert!(
            document
                .extend_line_to_boundary(target, outside, point(0.0, 0.0))
                .is_err()
        );
        assert!(
            document
                .trim_line_to_boundary(target, parallel, point(0.0, 0.0))
                .is_err()
        );
        assert!(
            document
                .trim_line_to_boundary(target, missed, point(0.0, 0.0))
                .is_err()
        );
        assert!(
            document
                .trim_line_to_boundary(target, target, point(0.0, 0.0))
                .is_err()
        );
        assert!(
            document
                .trim_line_to_boundary(target, 9999, point(0.0, 0.0))
                .is_err()
        );
        assert!(
            document
                .trim_line_to_boundary(target, inside, point(f64::NAN, 0.0))
                .is_err()
        );
        assert_eq!(document, before);
    }
}
