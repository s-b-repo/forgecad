//! Survey points, a checked 2.5D triangulated surface, contours and profiles.
//!
//! Coordinates use the project's model units. The TIN is a single-valued XY
//! surface: duplicate XY positions, inverted/overlapping faces and nonmanifold
//! edges are refused. Derived contours are line segments, not smoothed curves.
use crate::{Point, mesh::TriangleMesh};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write,
    io,
};

pub const MAX_SURVEY_POINTS: usize = 2048;
pub const MAX_CONTOUR_SEGMENTS: usize = 100_000;
pub const MAX_PROFILE_SAMPLES: usize = 10_000;
const EPS: f64 = 1e-12;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Terrain {
    pub points: Vec<[f64; 3]>,
    pub triangles: Vec<[usize; 3]>,
    #[serde(default)]
    pub alignments: Vec<crate::civil::Alignment>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContourSegment {
    pub level: f64,
    pub start: Point,
    pub end: Point,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProfileSample {
    pub distance: f64,
    pub position: Point,
    pub elevation: Option<f64>,
}

impl Terrain {
    pub fn is_empty(&self) -> bool {
        self.points.is_empty() && self.triangles.is_empty() && self.alignments.is_empty()
    }

    /// Finite survey points without faces are allowed while a survey is edited.
    /// A populated TIN must use every point and contain CCW non-overlapping faces.
    pub fn validate(&self) -> io::Result<()> {
        if self.alignments.len() > 100
            || self
                .alignments
                .iter()
                .map(|a| a.vertices.len())
                .sum::<usize>()
                > 20_000
        {
            return Err(invalid(
                "Terrain exceeds 100 alignments or 20000 alignment vertices",
            ));
        }
        let mut ids = BTreeSet::new();
        for alignment in &self.alignments {
            alignment.validate()?;
            if !ids.insert(alignment.id) {
                return Err(invalid("Alignment IDs must be unique"));
            }
        }
        let xy = normalized_points(&self.points)?;
        if self.triangles.len() > MAX_SURVEY_POINTS * 2 {
            return Err(invalid("Terrain exceeds the triangle limit"));
        }
        if self.triangles.is_empty() {
            return Ok(());
        }
        let mut used = BTreeSet::<usize>::new();
        let mut unique_faces = BTreeSet::new();
        let mut edges = BTreeMap::<(usize, usize), (usize, i32)>::new();
        let mut boxes = Vec::with_capacity(self.triangles.len());
        for (index, triangle) in self.triangles.iter().enumerate() {
            if triangle.iter().any(|i| *i >= xy.len()) {
                return Err(invalid("Terrain triangle refers to a missing survey point"));
            }
            let [a, b, c] = triangle.map(|i| xy[i]);
            if cross(sub(b, a), sub(c, a)) <= EPS {
                return Err(invalid(
                    "Terrain faces must have nondegenerate counterclockwise XY winding",
                ));
            }
            let mut key = *triangle;
            key.sort_unstable();
            if !unique_faces.insert(key) {
                return Err(invalid("Terrain contains duplicate faces"));
            }
            used.extend(triangle.iter().copied());
            for edge in [
                (triangle[0], triangle[1]),
                (triangle[1], triangle[2]),
                (triangle[2], triangle[0]),
            ] {
                let key = (edge.0.min(edge.1), edge.0.max(edge.1));
                let value = edges.entry(key).or_default();
                value.0 += 1;
                value.1 += if edge.0 < edge.1 { 1 } else { -1 };
                if value.0 > 2 || (value.0 == 2 && value.1 != 0) {
                    return Err(invalid(
                        "Terrain faces overlap across an edge or form a nonmanifold edge",
                    ));
                }
            }
            boxes.push((
                a.x.min(b.x).min(c.x),
                a.x.max(b.x).max(c.x),
                a.y.min(b.y).min(c.y),
                a.y.max(b.y).max(c.y),
                index,
            ));
        }
        if used.len() != self.points.len() {
            return Err(invalid("Terrain faces omit survey points"));
        }
        // Sweep triangle bounds before checking exact edge/interior intersections.
        boxes.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.4.cmp(&b.4)));
        for (position, a) in boxes.iter().enumerate() {
            for b in &boxes[position + 1..] {
                if b.0 > a.1 + EPS {
                    break;
                }
                if b.2 > a.3 + EPS || b.3 < a.2 - EPS {
                    continue;
                }
                if faces_overlap(self.triangles[a.4], self.triangles[b.4], &xy) {
                    return Err(invalid("Terrain triangle interiors overlap or edges cross"));
                }
            }
        }
        Ok(())
    }

    /// Deterministic XY Delaunay triangulation using a bounded Bowyer-Watson
    /// insertion. Returned points are sorted by X then Y. Co-circular ties use
    /// that insertion order. Near-coincident/near-collinear geometry below the
    /// normalized numerical tolerance is refused rather than guessed.
    pub fn triangulate(mut points: Vec<[f64; 3]>) -> io::Result<Self> {
        if points.len() < 3 {
            return Err(invalid(
                "Triangulation requires at least three survey points",
            ));
        }
        points.sort_by(|a, b| {
            a[0].total_cmp(&b[0])
                .then(a[1].total_cmp(&b[1]))
                .then(a[2].total_cmp(&b[2]))
        });
        let mut xy = normalized_points(&points)?;
        let n = xy.len();
        if !(1..n - 1).any(|i| cross(sub(xy[i], xy[0]), sub(xy[n - 1], xy[0])).abs() > EPS) {
            return Err(invalid(
                "Survey points are collinear at the supported XY tolerance",
            ));
        }
        xy.extend([
            Point::new(-64.0, -32.0),
            Point::new(64.0, -32.0),
            Point::new(0.0, 64.0),
        ]);
        let mut faces = vec![[n, n + 1, n + 2]];
        for index in 0..n {
            let mut boundary = BTreeMap::<(usize, usize), (usize, (usize, usize))>::new();
            let mut kept = Vec::with_capacity(faces.len() + 2);
            for face in faces {
                if in_circle(xy[face[0]], xy[face[1]], xy[face[2]], xy[index]) >= -EPS {
                    for edge in [(face[0], face[1]), (face[1], face[2]), (face[2], face[0])] {
                        let entry = boundary
                            .entry((edge.0.min(edge.1), edge.0.max(edge.1)))
                            .or_insert((0, edge));
                        entry.0 += 1;
                    }
                } else {
                    kept.push(face);
                }
            }
            if boundary.is_empty() {
                return Err(invalid(
                    "Survey insertion could not resolve a stable triangulation",
                ));
            }
            for (_, (count, (a, b))) in boundary {
                if count != 1 {
                    continue;
                }
                let turn = cross(sub(xy[b], xy[a]), sub(xy[index], xy[a]));
                if turn.abs() <= EPS {
                    continue;
                }
                kept.push(if turn > 0.0 {
                    [a, b, index]
                } else {
                    [b, a, index]
                });
            }
            if kept.len() > MAX_SURVEY_POINTS * 2 + 8 {
                return Err(invalid("Triangulation exceeds resource limits"));
            }
            faces = kept;
        }
        faces.retain(|face| face.iter().all(|index| *index < n));
        // Normalize each cyclic start for stable serialized output.
        for face in &mut faces {
            let start = (0..3).min_by_key(|i| face[*i]).unwrap();
            face.rotate_left(start);
        }
        faces.sort_unstable();
        let terrain = Self {
            points,
            triangles: faces,
            alignments: Vec::new(),
        };
        if terrain.triangles.is_empty() {
            return Err(invalid("Survey produced no stable terrain faces"));
        }
        terrain.validate()?;
        Ok(terrain)
    }

    pub fn mesh(&self) -> io::Result<TriangleMesh> {
        self.validate()?;
        let mesh = TriangleMesh {
            vertices: self.points.clone(),
            triangles: self
                .triangles
                .iter()
                .map(|face| face.map(|i| i as u32))
                .collect(),
        };
        mesh.validate()?;
        Ok(mesh)
    }

    /// Piecewise planar elevation, including TIN boundaries. Outside points
    /// return None. No extrapolation or smoothing changes the surveyed surface.
    pub fn elevation_at(&self, position: Point) -> io::Result<Option<f64>> {
        self.validate()?;
        if !position.is_finite() {
            return Err(invalid("Elevation query must use finite XY coordinates"));
        }
        self.elevation_unchecked(position)
    }

    pub(crate) fn elevation_unchecked(&self, position: Point) -> io::Result<Option<f64>> {
        for face in &self.triangles {
            let [a, b, c] = face.map(|i| self.points[i]);
            let scale = (b[0] - a[0])
                .abs()
                .max((b[1] - a[1]).abs())
                .max((c[0] - a[0]).abs())
                .max((c[1] - a[1]).abs());
            let bxy = Point::new((b[0] - a[0]) / scale, (b[1] - a[1]) / scale);
            let cxy = Point::new((c[0] - a[0]) / scale, (c[1] - a[1]) / scale);
            let pxy = Point::new((position.x - a[0]) / scale, (position.y - a[1]) / scale);
            if !pxy.is_finite() {
                continue;
            }
            let determinant = cross(bxy, cxy);
            let wb = cross(pxy, cxy) / determinant;
            let wc = cross(bxy, pxy) / determinant;
            let wa = 1.0 - wb - wc;
            if [wa, wb, wc]
                .iter()
                .all(|value| *value >= -1e-10 && *value <= 1.0 + 1e-10)
            {
                let weights = [wa.max(0.0), wb.max(0.0), wc.max(0.0)];
                let total: f64 = weights.iter().sum();
                let elevation = (weights[0] / total) * a[2]
                    + (weights[1] / total) * b[2]
                    + (weights[2] / total) * c[2];
                if !elevation.is_finite() {
                    return Err(invalid("Terrain interpolation exceeds numeric range"));
                }
                return Ok(Some(elevation));
            }
        }
        Ok(None)
    }

    /// Return independent exact linear contour segments at multiples of interval.
    /// Flat triangles emit no arbitrary interior contour. A flat edge shared by
    /// sloping faces is emitted once. Contours are bounded to 10,000 levels and
    /// 100,000 segments per request.
    pub fn contours(&self, interval: f64) -> io::Result<Vec<ContourSegment>> {
        self.validate()?;
        if !interval.is_finite() || interval <= 0.0 {
            return Err(invalid("Contour interval must be finite and positive"));
        }
        if self.triangles.is_empty() {
            return Ok(Vec::new());
        }
        let min = self
            .points
            .iter()
            .map(|p| p[2])
            .fold(f64::INFINITY, f64::min);
        let max = self
            .points
            .iter()
            .map(|p| p[2])
            .fold(f64::NEG_INFINITY, f64::max);
        if min == max {
            return Ok(Vec::new());
        }
        let first = (min / interval).ceil();
        let last = (max / interval).floor();
        if !first.is_finite()
            || !last.is_finite()
            || first.abs() > (1u64 << 52) as f64
            || last.abs() > (1u64 << 52) as f64
        {
            return Err(invalid(
                "Contour levels cannot be represented at this interval",
            ));
        }
        if last < first {
            return Ok(Vec::new());
        }
        let count = last - first + 1.0;
        if count > 10_000.0 {
            return Err(invalid("Contour request exceeds 10,000 levels"));
        }
        let mut segments = Vec::new();
        for step in 0..count as usize {
            let level = (first + step as f64) * interval;
            if !level.is_finite() {
                return Err(invalid("Contour elevation exceeds numeric range"));
            }
            let mut flat_edges = BTreeSet::new();
            for face in &self.triangles {
                if face.iter().all(|i| self.points[*i][2] == level) {
                    continue;
                }
                let mut hits = Vec::<Point>::new();
                let mut flat = None;
                for (i, j) in [(face[0], face[1]), (face[1], face[2]), (face[2], face[0])] {
                    let a = self.points[i];
                    let b = self.points[j];
                    if a[2] == level && b[2] == level {
                        flat = Some((i.min(j), i.max(j)));
                        hits = vec![Point::new(a[0], a[1]), Point::new(b[0], b[1])];
                        break;
                    }
                    let position = if a[2] == level {
                        Some(Point::new(a[0], a[1]))
                    } else if b[2] == level {
                        Some(Point::new(b[0], b[1]))
                    } else if (a[2] < level && b[2] > level) || (b[2] < level && a[2] > level) {
                        let t = (level - a[2]) / (b[2] - a[2]);
                        Some(Point::new(
                            a[0] + (b[0] - a[0]) * t,
                            a[1] + (b[1] - a[1]) * t,
                        ))
                    } else {
                        None
                    };
                    if let Some(position) = position {
                        if !position.is_finite() {
                            return Err(invalid("Contour interpolation exceeds numeric range"));
                        }
                        if !hits.contains(&position) {
                            hits.push(position);
                        }
                    }
                }
                if flat.is_some_and(|edge| !flat_edges.insert(edge)) {
                    continue;
                }
                if hits.len() == 2 && hits[0] != hits[1] {
                    segments.push(ContourSegment {
                        level,
                        start: hits[0],
                        end: hits[1],
                    });
                    if segments.len() > MAX_CONTOUR_SEGMENTS {
                        return Err(invalid("Contour request exceeds 100,000 segments"));
                    }
                }
            }
        }
        Ok(segments)
    }

    pub fn profile(
        &self,
        start: Point,
        end: Point,
        samples: usize,
    ) -> io::Result<Vec<ProfileSample>> {
        self.validate()?;
        if !start.is_finite()
            || !end.is_finite()
            || start == end
            || !start.distance(end).is_finite()
        {
            return Err(invalid(
                "Profile requires finite distinct endpoints and a finite length",
            ));
        }
        if !(2..=MAX_PROFILE_SAMPLES).contains(&samples) {
            return Err(invalid("Profile sample count must be between 2 and 10,000"));
        }
        let length = start.distance(end);
        (0..samples)
            .map(|index| {
                let t = index as f64 / (samples - 1) as f64;
                let position = Point::new(
                    start.x + (end.x - start.x) * t,
                    start.y + (end.y - start.y) * t,
                );
                Ok(ProfileSample {
                    distance: length * t,
                    position,
                    elevation: self.elevation_unchecked(position)?,
                })
            })
            .collect()
    }

    pub fn points_csv(&self) -> io::Result<String> {
        self.validate()?;
        let mut output = String::from("x,y,elevation\n");
        for point in &self.points {
            writeln!(output, "{},{},{}", point[0], point[1], point[2]).unwrap();
        }
        Ok(output)
    }

    pub fn profile_csv(&self, start: Point, end: Point, samples: usize) -> io::Result<String> {
        let mut output = String::from("distance,x,y,elevation\n");
        for sample in self.profile(start, end, samples)? {
            writeln!(
                output,
                "{},{},{},{}",
                sample.distance,
                sample.position.x,
                sample.position.y,
                sample.elevation.map(|z| z.to_string()).unwrap_or_default()
            )
            .unwrap();
        }
        Ok(output)
    }

    /// Derive a 2D contour drawing with the exact level retained in each line's
    /// layer name (`Contour {level}`). The drafting core has no Z coordinate;
    /// this output is a plan drawing, not a 3D polyline or semantic TIN.
    pub fn contour_drawing(&self, interval: f64, units: u16) -> io::Result<crate::Document> {
        if units > 24 {
            return Err(invalid(
                "Contour drawing requires a supported unit code (0..24)",
            ));
        }
        let segments = self.contours(interval)?;
        let mut drawing = crate::Document::new("Terrain contours");
        drawing.units = units;
        let mut last_level = None;
        let mut next_id = 2u64;
        for segment in segments {
            if last_level != Some(segment.level) {
                drawing.layers.push(crate::Layer {
                    id: next_id,
                    name: format!("Contour {}", segment.level),
                    color: [105, 190, 255],
                    visible: true,
                });
                drawing.active_layer_id = next_id;
                next_id += 1;
                last_level = Some(segment.level);
            }
            // This is a new, bounded drawing; sequential allocation avoids the
            // general allocator's repeated scan for every derived segment.
            drawing.entities.push(crate::Entity {
                id: next_id,
                layer_id: drawing.active_layer_id,
                geometry: crate::Geometry::Line {
                    start: segment.start,
                    end: segment.end,
                },
            });
            next_id += 1;
        }
        drawing.next_id = next_id;
        drawing.validate()?;
        Ok(drawing)
    }

    pub fn example() -> Self {
        Self::triangulate(vec![
            [0.0, 0.0, 0.0],
            [100.0, 0.0, 8.0],
            [200.0, 0.0, 12.0],
            [0.0, 100.0, 3.0],
            [100.0, 100.0, 28.0],
            [200.0, 100.0, 16.0],
            [0.0, 200.0, 6.0],
            [100.0, 200.0, 14.0],
            [200.0, 200.0, 20.0],
        ])
        .expect("built-in survey must triangulate")
    }
}

fn normalized_points(points: &[[f64; 3]]) -> io::Result<Vec<Point>> {
    if points.len() > MAX_SURVEY_POINTS {
        return Err(invalid("Terrain exceeds the 2048 survey-point limit"));
    }
    if points.iter().flatten().any(|v| !v.is_finite()) {
        return Err(invalid("Survey coordinates must be finite"));
    }
    if points.is_empty() {
        return Ok(Vec::new());
    }
    let mut min = points[0];
    let mut max = points[0];
    for point in points {
        for axis in 0..3 {
            min[axis] = min[axis].min(point[axis]);
            max[axis] = max[axis].max(point[axis]);
        }
    }
    let spans = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];
    if spans.iter().any(|v| !v.is_finite()) {
        return Err(invalid("Survey extents exceed numeric range"));
    }
    let scale = spans[0].max(spans[1]);
    if points.len() > 1 && scale == 0.0 {
        return Err(invalid("Survey contains duplicate XY positions"));
    }
    let scale = if scale == 0.0 { 1.0 } else { scale };
    let mut result = Vec::with_capacity(points.len());
    for point in points {
        let p = Point::new((point[0] - min[0]) / scale, (point[1] - min[1]) / scale);
        if result
            .iter()
            .any(|existing: &Point| existing.distance(p) <= EPS)
        {
            return Err(invalid(
                "Survey contains duplicate or numerically indistinguishable XY positions",
            ));
        }
        result.push(p);
    }
    Ok(result)
}

fn faces_overlap(a: [usize; 3], b: [usize; 3], points: &[Point]) -> bool {
    let shared = a.iter().filter(|i| b.contains(i)).count();
    if shared >= 2 {
        return false;
    } // Edge winding/manifold checks already cover this pair.
    for (i, j) in [(a[0], a[1]), (a[1], a[2]), (a[2], a[0])] {
        for (k, l) in [(b[0], b[1]), (b[1], b[2]), (b[2], b[0])] {
            if edges_cross(i, j, k, l, points) {
                return true;
            }
        }
    }
    a.iter()
        .any(|i| !b.contains(i) && strictly_inside(points[*i], b, points))
        || b.iter()
            .any(|i| !a.contains(i) && strictly_inside(points[*i], a, points))
}

fn edges_cross(i: usize, j: usize, k: usize, l: usize, points: &[Point]) -> bool {
    let a = points[i];
    let b = points[j];
    let c = points[k];
    let d = points[l];
    let ab = sub(b, a);
    let cd = sub(d, c);
    let oa = cross(ab, sub(c, a));
    let ob = cross(ab, sub(d, a));
    let oc = cross(cd, sub(a, c));
    let od = cross(cd, sub(b, c));
    if oa * ob < -EPS * EPS && oc * od < -EPS * EPS {
        return true;
    }
    for (index, p, start, end) in [(k, c, i, j), (l, d, i, j), (i, a, k, l), (j, b, k, l)] {
        if index == start || index == end {
            continue;
        }
        let a = points[start];
        let b = points[end];
        if cross(sub(b, a), sub(p, a)).abs() <= EPS
            && p.x >= a.x.min(b.x) - EPS
            && p.x <= a.x.max(b.x) + EPS
            && p.y >= a.y.min(b.y) - EPS
            && p.y <= a.y.max(b.y) + EPS
        {
            return true;
        }
    }
    false
}
fn strictly_inside(p: Point, face: [usize; 3], points: &[Point]) -> bool {
    [(face[0], face[1]), (face[1], face[2]), (face[2], face[0])]
        .iter()
        .all(|(a, b)| cross(sub(points[*b], points[*a]), sub(p, points[*a])) > EPS)
}
fn sub(a: Point, b: Point) -> Point {
    Point::new(a.x - b.x, a.y - b.y)
}
fn cross(a: Point, b: Point) -> f64 {
    a.x * b.y - a.y * b.x
}
fn in_circle(a: Point, b: Point, c: Point, p: Point) -> f64 {
    let a = sub(a, p);
    let b = sub(b, p);
    let c = sub(c, p);
    (a.x * a.x + a.y * a.y) * cross(b, c) - (b.x * b.x + b.y * b.y) * cross(a, c)
        + (c.x * c.x + c.y * c.y) * cross(a, b)
}
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn near(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} != {b}");
    }
    fn planar() -> Terrain {
        Terrain::triangulate(vec![
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 10.0],
            [0.0, 10.0, 20.0],
            [10.0, 10.0, 30.0],
            [5.0, 5.0, 15.0],
        ])
        .unwrap()
    }

    #[test]
    fn triangulation_uses_every_survey_point_with_upward_faces() {
        let terrain = planar();
        assert_eq!(terrain.triangles.len(), 4);
        terrain.mesh().unwrap().validate().unwrap();
        let xy = normalized_points(&terrain.points).unwrap();
        for face in &terrain.triangles {
            let [a, b, c] = face.map(|i| xy[i]);
            assert!(cross(sub(b, a), sub(c, a)) > 0.0);
        }
        let mut reverse = terrain.points.clone();
        reverse.reverse();
        assert_eq!(Terrain::triangulate(reverse).unwrap(), terrain);
        let source = serde_json::to_string(&terrain).unwrap();
        assert_eq!(serde_json::from_str::<Terrain>(&source).unwrap(), terrain);
    }

    #[test]
    fn elevation_queries_follow_the_survey_plane_and_preserve_outside_gaps() {
        let terrain = planar();
        near(
            terrain.elevation_at(Point::new(2.0, 3.0)).unwrap().unwrap(),
            8.0,
        );
        near(
            terrain
                .elevation_at(Point::new(10.0, 10.0))
                .unwrap()
                .unwrap(),
            30.0,
        );
        assert_eq!(terrain.elevation_at(Point::new(11.0, 5.0)).unwrap(), None);
        assert!(terrain.elevation_at(Point::new(f64::NAN, 0.0)).is_err());
        let profile = terrain
            .profile(Point::new(-5.0, 5.0), Point::new(15.0, 5.0), 5)
            .unwrap();
        assert_eq!(profile[0].elevation, None);
        assert_eq!(profile[4].elevation, None);
        near(profile[2].elevation.unwrap(), 15.0);
        near(profile[4].distance, 20.0);
        assert!(
            terrain
                .profile_csv(Point::new(-5.0, 5.0), Point::new(15.0, 5.0), 5)
                .unwrap()
                .contains("0,-5,5,\n")
        );
    }

    #[test]
    fn contours_interpolate_exact_levels_without_duplicate_flat_edges() {
        let terrain = planar();
        let contours = terrain.contours(5.0).unwrap();
        assert!(!contours.is_empty());
        for segment in contours {
            near(segment.level % 5.0, 0.0);
            near(segment.start.x + 2.0 * segment.start.y, segment.level);
            near(segment.end.x + 2.0 * segment.end.y, segment.level);
        }
        let valley = Terrain {
            points: vec![
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 1.0],
                [0.0, 1.0, 0.0],
            ],
            triangles: vec![[0, 1, 3], [1, 2, 3]],
            alignments: Vec::new(),
        };
        let zero = valley
            .contours(1.0)
            .unwrap()
            .into_iter()
            .filter(|s| s.level == 0.0)
            .collect::<Vec<_>>();
        assert_eq!(zero.len(), 1);
        let flat =
            Terrain::triangulate(vec![[0.0, 0.0, 5.0], [1.0, 0.0, 5.0], [0.0, 1.0, 5.0]]).unwrap();
        assert!(flat.contours(1.0).unwrap().is_empty());
    }

    #[test]
    fn malformed_surveys_and_tins_are_refused() {
        for points in [
            vec![],
            vec![[0.0; 3], [1.0; 3]],
            vec![[0.0; 3], [1.0; 3], [2.0; 3]],
            vec![[0.0; 3], [0.0, 0.0, 1.0], [1.0; 3]],
            vec![[0.0; 3], [1.0; 3], [0.0, f64::NAN, 1.0]],
        ] {
            assert!(Terrain::triangulate(points).is_err());
        }
        let terrain = planar();
        let mut inverted = terrain.clone();
        inverted.triangles[0].swap(1, 2);
        assert!(inverted.validate().is_err());
        let mut duplicate = terrain.clone();
        duplicate.triangles.push(duplicate.triangles[0]);
        assert!(duplicate.validate().is_err());
        let mut missing = terrain.clone();
        missing.triangles[0][0] = 999;
        assert!(missing.validate().is_err());
        let crossing = Terrain {
            points: vec![
                [0.0, 0.0, 0.0],
                [4.0, 0.0, 0.0],
                [2.0, 4.0, 0.0],
                [0.0, 3.0, 0.0],
                [4.0, 3.0, 0.0],
                [2.0, -1.0, 0.0],
            ],
            triangles: vec![[0, 1, 2], [3, 5, 4]],
            alignments: Vec::new(),
        };
        assert!(crossing.validate().is_err());
    }

    #[test]
    fn resource_and_numeric_limits_are_checked() {
        assert!(Terrain::triangulate(vec![[0.0; 3]; MAX_SURVEY_POINTS + 1]).is_err());
        let terrain = planar();
        for interval in [0.0, -1.0, f64::NAN, f64::INFINITY, 1e-9] {
            assert!(terrain.contours(interval).is_err());
        }
        for samples in [0, 1, MAX_PROFILE_SAMPLES + 1] {
            assert!(
                terrain
                    .profile(Point::new(0.0, 0.0), Point::new(1.0, 1.0), samples)
                    .is_err()
            );
        }
        assert!(
            terrain
                .profile(Point::default(), Point::default(), 10)
                .is_err()
        );
        assert!(
            Terrain {
                points: vec![[-1e308, 0.0, 0.0], [1e308, 1.0, 1.0]],
                triangles: vec![],
                alignments: Vec::new(),
            }
            .validate()
            .is_err()
        );
        assert!(Terrain::default().is_empty());
        assert!(Terrain::default().mesh().unwrap().triangles.is_empty());
    }

    #[test]
    fn example_survey_is_nonflat_and_round_trips_geometry_exchange() {
        let terrain = Terrain::example();
        terrain.validate().unwrap();
        assert_eq!(terrain.points.len(), 9);
        assert_eq!(terrain.triangles.len(), 8);
        near(
            terrain
                .elevation_at(Point::new(100.0, 100.0))
                .unwrap()
                .unwrap(),
            28.0,
        );
        assert!(terrain.points_csv().unwrap().starts_with("x,y,elevation\n"));
        let mesh = terrain.mesh().unwrap();
        let obj = mesh.to_obj("Survey").unwrap();
        assert_eq!(
            TriangleMesh::from_obj(&obj).unwrap().triangles.len(),
            mesh.triangles.len()
        );
    }

    #[test]
    fn dense_seeded_survey_obeys_empty_circumcircles_and_exact_planar_queries() {
        let mut seed = 1234567u64;
        let mut coordinate = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            ((seed >> 32) as u32) as f64 / u32::MAX as f64 * 100.0
        };
        let mut points = vec![
            [0.0, 0.0, 0.0],
            [100.0, 0.0, 100.0],
            [100.0, 100.0, 300.0],
            [0.0, 100.0, 200.0],
        ];
        for _ in 0..200 {
            let x = coordinate();
            let y = coordinate();
            points.push([x, y, x + 2.0 * y]);
        }
        let terrain = Terrain::triangulate(points).unwrap();
        let xy = normalized_points(&terrain.points).unwrap();
        for face in &terrain.triangles {
            let [a, b, c] = face.map(|i| xy[i]);
            for (index, p) in xy.iter().enumerate() {
                if !face.contains(&index) {
                    assert!(in_circle(a, b, c, *p) <= 1e-9);
                }
            }
        }
        for point in [
            Point::new(5.0, 8.0),
            Point::new(32.0, 71.0),
            Point::new(90.0, 90.0),
        ] {
            near(
                terrain.elevation_at(point).unwrap().unwrap(),
                point.x + 2.0 * point.y,
            );
        }
    }

    #[test]
    fn contour_elevations_survive_dxf_roundtrip_in_layer_names() {
        let terrain = planar();
        let drawing = terrain.contour_drawing(5.0, 4).unwrap();
        assert!(!drawing.entities.is_empty());
        let source = crate::dxf::write(&drawing).unwrap();
        let report = crate::dxf::parse(&source).unwrap();
        assert!(report.unsupported_entities.is_empty());
        assert!(report.unsupported_features.is_empty());
        assert_eq!(report.document.units, 4);
        assert_eq!(report.document.entities.len(), drawing.entities.len());
        for entity in &report.document.entities {
            let name = &report.document.layer(entity.layer_id).unwrap().name;
            let level: f64 = name.strip_prefix("Contour ").unwrap().parse().unwrap();
            let crate::Geometry::Line { start, end } = entity.geometry else {
                panic!("expected contour LINE");
            };
            near(start.x + 2.0 * start.y, level);
            near(end.x + 2.0 * end.y, level);
        }
        assert!(terrain.contour_drawing(5.0, 25).is_err());
    }
}
