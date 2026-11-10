//! Bounded profiles, rectangular/circular/polygon pockets, optional uncut
//! pocket islands and point-based peck drilling.
//! Work coordinates are millimetres with stock top at Z=0. Preview and generic
//! G-code do not provide stock-removal simulation or a machine-specific post.
//! Posting uses twelve decimal places without exponential notation. Coordinates
//! stay within +/-1,000,000 mm; settings are at least 0.000001 and profile
//! segments at least 0.000000001 mm. Unrepresentable programs are refused.
use crate::{Geometry, Point};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fmt::Write, io};

const MAX_COORDINATE: f64 = 1_000_000.0;
const MIN_SETTING: f64 = 0.000_001;
const MIN_SEGMENT: f64 = 0.000_000_001;
const MAX_POLYLINE_WORK: usize = 8_388_608;
const MAX_POCKET_ISLANDS: usize = 64;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Compensation {
    #[default]
    Center,
    Inside,
    Outside,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    #[default]
    Profile,
    Pocket,
    Drill,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub id: u64,
    pub name: String,
    pub geometry: Geometry,
    pub compensation: Compensation,
    #[serde(default)]
    pub operation: Operation,
    /// Closed islands removed from a Pocket operation. Each island is kept
    /// inside the pocket boundary and never cut.
    #[serde(default)]
    pub islands: Vec<Geometry>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CamJob {
    pub profiles: Vec<Profile>,
    pub tool_diameter: f64,
    pub depth: f64,
    pub step_down: f64,
    #[serde(default = "default_step_over")]
    pub step_over: f64,
    pub safe_z: f64,
    pub feed: f64,
    pub plunge_feed: f64,
    pub spindle_rpm: u32,
    pub stock_min: [f64; 3],
    pub stock_max: [f64; 3],
}
fn default_step_over() -> f64 {
    1.2
}
impl Default for CamJob {
    fn default() -> Self {
        Self {
            profiles: Vec::new(),
            tool_diameter: 3.0,
            depth: 3.0,
            step_down: 1.0,
            step_over: default_step_over(),
            safe_z: 5.0,
            feed: 300.0,
            plunge_feed: 100.0,
            spindle_rpm: 10000,
            stock_min: [-10.0, -10.0, -10.0],
            stock_max: [110.0, 90.0, 0.0],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motion {
    pub position: [f64; 3],
    pub rapid: bool,
    pub feed: f64,
}
#[derive(Clone, Debug, Default)]
pub struct Toolpath {
    pub motions: Vec<Motion>,
    pub cutting_distance: f64,
    pub total_distance: f64,
    pub estimated_minutes: f64,
    pub passes: usize,
}

impl CamJob {
    pub fn is_empty(&self) -> bool {
        self.profiles.is_empty()
    }
    pub fn validate(&self) -> io::Result<()> {
        self.validated_paths().map(|_| ())
    }
    fn validated_paths(&self) -> io::Result<Vec<Vec<Vec<Point>>>> {
        for value in [
            self.tool_diameter,
            self.depth,
            self.step_down,
            self.step_over,
            self.safe_z,
            self.feed,
            self.plunge_feed,
        ] {
            if !value.is_finite() || !(MIN_SETTING..=MAX_COORDINATE).contains(&value) {
                return Err(invalid(
                    "CAM dimensions, feeds, depth and clearance must be finite and between 0.000001 and 1000000",
                ));
            }
        }
        if self.spindle_rpm == 0
            || self.spindle_rpm > 100_000
            || self.feed > 100_000.0
            || self.plunge_feed > 100_000.0
        {
            return Err(invalid("CAM spindle/feed exceeds supported range"));
        }
        if self
            .stock_min
            .iter()
            .chain(self.stock_max.iter())
            .any(|v| !v.is_finite() || v.abs() > MAX_COORDINATE)
            || (0..3).any(|i| self.stock_min[i] >= self.stock_max[i])
            || self.stock_max[2] != 0.0
            || self.depth > -self.stock_min[2]
            || self.profiles.len() > 1000
        {
            return Err(invalid(
                "CAM requires ordered stock bounds within +/-1000000 mm, top Z=0, depth within stock and at most 1000 profiles",
            ));
        }
        if posted(self.depth)? != self.depth || posted(self.safe_z)? != self.safe_z {
            return Err(invalid(
                "CAM depth and clearance must be representable with twelve decimal places",
            ));
        }
        let passes = pass_count(self.depth, self.step_down)?;
        if self
            .profiles
            .iter()
            .any(|p| p.operation == Operation::Pocket)
            && self.step_over > self.tool_diameter
        {
            return Err(invalid("Pocket step over cannot exceed the tool diameter"));
        }
        let mut ids = BTreeSet::new();
        let mut point_count = 0usize;
        let mut path_count = 0usize;
        let mut polyline_work = 0usize;
        for profile in &self.profiles {
            if profile.id == 0
                || !ids.insert(profile.id)
                || profile.name.trim().is_empty()
                || profile.name.contains(['\r', '\n', '\0'])
            {
                return Err(invalid(
                    "CAM profiles require unique nonzero IDs and single-line names",
                ));
            }
            if profile.operation != Operation::Pocket && !profile.islands.is_empty() {
                return Err(invalid(
                    "CAM islands require a pocket operation with Inside compensation",
                ));
            }
            if profile.islands.len() > MAX_POCKET_ISLANDS {
                return Err(invalid("CAM pockets support at most 64 islands"));
            }
            let (paths, points) = match (profile.operation, &profile.geometry) {
                (Operation::Drill, Geometry::Point { .. })
                    if profile.compensation == Compensation::Center =>
                {
                    (1, 1)
                }
                (Operation::Pocket, _) if profile.compensation == Compensation::Inside => {
                    self.pocket_estimate(profile, &mut polyline_work)?
                }
                (Operation::Profile, Geometry::Rectangle { .. }) => (1, 5),
                (Operation::Profile, Geometry::Circle { .. }) => (1, 97),
                (
                    Operation::Profile,
                    Geometry::Polyline {
                        vertices,
                        closed: true,
                    },
                ) if (3..=2048).contains(&vertices.len())
                    && vertices.iter().all(|v| v.bulge == 0.0) =>
                {
                    add_polyline_work(&mut polyline_work, vertices.len())?;
                    (1, vertices.len() + 1)
                }
                _ => {
                    return Err(invalid(
                        "Profiles require closed contours; pockets require rectangles, circles or closed straight polylines with Inside compensation; drilling requires points with Center compensation",
                    ));
                }
            };
            path_count = path_count
                .checked_add(paths)
                .ok_or_else(|| invalid("CAM path count overflow"))?;
            point_count = point_count
                .checked_add(points)
                .ok_or_else(|| invalid("CAM motion count overflow"))?;
        }
        // Each cut path costs a rapid, plunge, every segment and a retract.
        // Bound path allocation and quadratic polyline work before offsets.
        if point_count
            .checked_add(
                path_count
                    .checked_mul(2)
                    .ok_or_else(|| invalid("CAM motion count overflow"))?,
            )
            .and_then(|n| n.checked_mul(passes))
            .and_then(|n| n.checked_add(1))
            .is_none_or(|n| n > 500_000)
        {
            return Err(invalid("CAM toolpath exceeds 500,000 motions"));
        }
        let mut paths = Vec::with_capacity(self.profiles.len());
        let radius = self.tool_diameter / 2.0;
        let mut actual_motions = 1usize;
        for profile in &self.profiles {
            let profile_paths = match profile.operation {
                Operation::Profile => vec![self.profile_points(profile)?],
                Operation::Pocket => self.pocket_paths(profile)?,
                Operation::Drill => {
                    let Geometry::Point { position } = profile.geometry else {
                        unreachable!("operation preflight")
                    };
                    check_point(position)?;
                    vec![vec![position]]
                }
            };
            for points in &profile_paths {
                if points.is_empty() {
                    return Err(invalid("CAM operation produced an empty path"));
                }
                for p in points {
                    self.check_footprint(*p, radius)?;
                    self.check_footprint(Point::new(posted(p.x)?, posted(p.y)?), radius)?;
                }
                for pair in points.windows(2) {
                    let a = Point::new(posted(pair[0].x)?, posted(pair[0].y)?);
                    let b = Point::new(posted(pair[1].x)?, posted(pair[1].y)?);
                    if a.distance(b) < MIN_SEGMENT {
                        return Err(invalid(
                            "CAM segment is below the representable posting resolution",
                        ));
                    }
                }
                actual_motions = actual_motions
                    .checked_add(points.len() + 2)
                    .ok_or_else(|| invalid("CAM motion count overflow"))?;
            }
            paths.push(profile_paths);
        }
        if actual_motions
            .checked_mul(passes)
            .is_none_or(|n| n > 500_000)
        {
            return Err(invalid("CAM toolpath exceeds 500,000 motions"));
        }
        Ok(paths)
    }
    fn check_footprint(&self, p: Point, radius: f64) -> io::Result<()> {
        if !p.is_finite()
            || p.x - radius < self.stock_min[0]
            || p.x + radius > self.stock_max[0]
            || p.y - radius < self.stock_min[1]
            || p.y + radius > self.stock_max[1]
        {
            Err(invalid(
                "Compensated tool footprint extends beyond stock XY bounds",
            ))
        } else {
            Ok(())
        }
    }
    fn profile_points(&self, profile: &Profile) -> io::Result<Vec<Point>> {
        profile.geometry.validate()?;
        match &profile.geometry {
            Geometry::Rectangle { a, b } if a.x != b.x && a.y != b.y => {
                check_point(*a)?;
                check_point(*b)?;
            }
            Geometry::Circle { center, radius } => {
                check_point(*center)?;
                if *radius > MAX_COORDINATE {
                    return Err(invalid("CAM circle exceeds supported coordinate range"));
                }
            }
            Geometry::Polyline {
                vertices,
                closed: true,
            } if vertices.len() >= 3
                && vertices.len() <= 2048
                && vertices.iter().all(|v| v.bulge == 0.0) =>
            {
                let points: Vec<_> = vertices.iter().map(|v| v.point).collect();
                for p in &points {
                    check_point(*p)?;
                }
                simple_polygon(&points)?;
            }
            _ => {
                return Err(invalid(
                    "CAM profiles require nondegenerate rectangles, circles or closed straight polylines",
                ));
            }
        }
        let distance = match profile.compensation {
            Compensation::Center => 0.0,
            Compensation::Inside => -self.tool_diameter / 2.0,
            Compensation::Outside => self.tool_diameter / 2.0,
        };
        let geometry = if distance == 0.0 {
            profile.geometry.clone()
        } else {
            profile.geometry.offset(distance)?
        };
        let mut points = match geometry {
            Geometry::Rectangle { a, b } => vec![a, Point::new(b.x, a.y), b, Point::new(a.x, b.y)],
            Geometry::Polyline { vertices, .. } => vertices.into_iter().map(|v| v.point).collect(),
            Geometry::Circle { center, radius } => (0..96)
                .map(|index| {
                    let angle = std::f64::consts::TAU * index as f64 / 96.0;
                    Point::new(
                        center.x + radius * angle.cos(),
                        center.y + radius * angle.sin(),
                    )
                })
                .collect(),
            _ => unreachable!("profile types checked"),
        };
        points.push(points[0]);
        if points
            .windows(2)
            .any(|p| p[0].distance(p[1]) < MIN_SEGMENT || !p[0].distance(p[1]).is_finite())
        {
            return Err(invalid(
                "CAM profile has collapsed, too short or unrepresentable segments",
            ));
        }
        Ok(points)
    }

    /// Conservative preflight for polygon/island pockets. Paths are estimated
    /// from the clearance bounds and the maximum scanline crossings before any
    /// offsets or row paths are allocated.
    fn pocket_estimate(
        &self,
        profile: &Profile,
        polyline_work: &mut usize,
    ) -> io::Result<(usize, usize)> {
        if profile.islands.is_empty()
            && matches!(
                profile.geometry,
                Geometry::Rectangle { .. } | Geometry::Circle { .. }
            )
        {
            return Ok((1, self.pocket_point_count(&profile.geometry)?));
        }
        let outer = pocket_contour(&profile.geometry)?;
        if let Geometry::Polyline { vertices, .. } = &profile.geometry {
            add_polyline_work(polyline_work, vertices.len())?;
        }
        let (mut y_min, mut y_max) = bounds_y(&outer)
            .ok_or_else(|| invalid("Pocket boundary has no finite vertical extent"))?;
        if y_min > y_max {
            std::mem::swap(&mut y_min, &mut y_max);
        }
        let mut island_points = 0usize;
        let mut intervals = 1usize;
        for island in &profile.islands {
            let points = pocket_contour(island)?;
            if let Geometry::Polyline { vertices, .. } = island {
                add_polyline_work(polyline_work, vertices.len())?;
            }
            island_points = island_points
                .checked_add(points.len())
                .ok_or_else(|| invalid("CAM island point count overflow"))?;
            intervals = intervals
                .checked_add(points.len().div_ceil(2).max(1))
                .ok_or_else(|| invalid("CAM island scanline count overflow"))?;
        }
        let height = y_max - y_min;
        if !height.is_finite() || height <= 0.0 {
            return Err(invalid("Pocket boundary has no finite vertical extent"));
        }
        let rows = (height / self.step_over).ceil().max(1.0);
        if !rows.is_finite() || rows > 1_000_000.0 {
            return Err(invalid(
                "Pocket exceeds the scanline limit; increase step over",
            ));
        }
        let row_paths = (rows as usize + 1)
            .checked_mul(intervals)
            .ok_or_else(|| invalid("CAM pocket path count overflow"))?;
        let paths = row_paths
            .checked_add(1)
            .and_then(|n| n.checked_add(profile.islands.len()))
            .ok_or_else(|| invalid("CAM pocket path count overflow"))?;
        let points = row_paths
            .checked_mul(2)
            .and_then(|n| n.checked_add(outer.len() + 1 + island_points + profile.islands.len()))
            .ok_or_else(|| invalid("CAM pocket point count overflow"))?;
        if paths > 500_000 || points > 500_000 {
            return Err(invalid(
                "Pocket exceeds the 500,000 motion limit; increase step over",
            ));
        }
        Ok((paths, points))
    }

    /// Exact motion estimate for the optimized rectangle/circle paths.
    fn pocket_point_count(&self, geometry: &Geometry) -> io::Result<usize> {
        geometry.validate()?;
        let count = match geometry {
            Geometry::Rectangle { a, b } => {
                check_point(*a)?;
                check_point(*b)?;
                if (b.x - a.x).abs() <= self.tool_diameter
                    || (b.y - a.y).abs() <= self.tool_diameter
                {
                    return Err(invalid(
                        "Pocket width and height must exceed the tool diameter",
                    ));
                }
                (((b.y - a.y).abs() - self.tool_diameter) / self.step_over)
                    .ceil()
                    .max(1.0)
                    * 2.0
                    + 7.0
            }
            Geometry::Circle { center, radius } => {
                check_point(*center)?;
                let tool_radius = self.tool_diameter / 2.0;
                if *radius <= tool_radius || *radius > MAX_COORDINATE {
                    return Err(invalid(
                        "Pocket radius must exceed the tool radius and stay within coordinate bounds",
                    ));
                }
                ((*radius - tool_radius) / self.step_over).ceil().max(1.0) * 97.0 + 1.0
            }
            _ => unreachable!("optimized pocket preflight"),
        };
        if !count.is_finite() || count > 500_000.0 {
            return Err(invalid(
                "Pocket exceeds the 500,000 motion limit; increase step over",
            ));
        }
        Ok(count as usize)
    }

    fn pocket_points(&self, geometry: &Geometry) -> io::Result<Vec<Point>> {
        let r = self.tool_diameter / 2.0;
        let mut points = Vec::new();
        match geometry {
            Geometry::Rectangle { a, b } => {
                let (x0, x1) = (a.x.min(b.x) + r, a.x.max(b.x) - r);
                let (y0, y1) = (a.y.min(b.y) + r, a.y.max(b.y) - r);
                if x1 <= x0 || y1 <= y0 {
                    return Err(invalid(
                        "Pocket width and height must exceed the tool diameter",
                    ));
                }
                let rows = ((y1 - y0) / self.step_over).ceil().max(1.0);
                if !rows.is_finite() || rows > 1_000_000.0 {
                    return Err(invalid("Pocket scanline count exceeds the resource limit"));
                }
                let rows = rows as usize;
                for row in 0..=rows {
                    let y = if row == rows {
                        y1
                    } else {
                        y0 + (y1 - y0) * row as f64 / rows as f64
                    };
                    let (start, end) = if row % 2 == 0 { (x0, x1) } else { (x1, x0) };
                    points.extend([Point::new(start, y), Point::new(end, y)]);
                }
                // Finish all four walls; zig-zag row links alone would leave
                // alternate sections of each side scalloped.
                points.extend([
                    Point::new(x0, y0),
                    Point::new(x1, y0),
                    Point::new(x1, y1),
                    Point::new(x0, y1),
                    Point::new(x0, y0),
                ]);
            }
            Geometry::Circle { center, radius } => {
                if *radius <= r {
                    return Err(invalid("Pocket radius must exceed the tool radius"));
                }
                points.push(*center);
                let rings = ((*radius - r) / self.step_over).ceil().max(1.0);
                if !rings.is_finite() || rings > 1_000_000.0 {
                    return Err(invalid("Pocket ring count exceeds the resource limit"));
                }
                let rings = rings as usize;
                for ring in 1..=rings {
                    let radius = (*radius - r) * ring as f64 / rings as f64;
                    for index in 0..=96 {
                        let angle = std::f64::consts::TAU * (index % 96) as f64 / 96.0;
                        points.push(Point::new(
                            center.x + radius * angle.cos(),
                            center.y + radius * angle.sin(),
                        ));
                    }
                }
            }
            _ => unreachable!("pocket preflight"),
        }
        Ok(points)
    }

    /// Build independently linked cut paths. Rectangle/circle pockets without
    /// islands keep their existing continuous spiral/raster path. Polygon
    /// pockets and pockets with islands raster-clear the material between the
    /// tool-radius boundaries, retracting between each disconnected interval,
    /// then finish the outer wall and every island.
    fn pocket_paths(&self, profile: &Profile) -> io::Result<Vec<Vec<Point>>> {
        if profile.islands.is_empty()
            && matches!(
                profile.geometry,
                Geometry::Rectangle { .. } | Geometry::Circle { .. }
            )
        {
            return Ok(vec![self.pocket_points(&profile.geometry)?]);
        }
        let contours = self.pocket_clearance_contours(profile)?;
        let outer = &contours[0];
        let islands = &contours[1..];
        let (mut y0, mut y1) = bounds_y(outer)
            .ok_or_else(|| invalid("Pocket boundary has no finite vertical extent"))?;
        if y0 > y1 {
            std::mem::swap(&mut y0, &mut y1);
        }
        let height = y1 - y0;
        let rows = (height / self.step_over).ceil().max(1.0);
        if !height.is_finite() || height <= 0.0 || !rows.is_finite() || rows > 1_000_000.0 {
            return Err(invalid("Pocket scanline count exceeds the resource limit"));
        }
        let rows = rows as usize;
        // Sample just below the top boundary so a horizontal top edge cannot
        // make the final raster line degenerate to a single touched vertex.
        let top_inset = (height * 1e-9).max(MIN_SEGMENT);
        let last_y = if y1 - top_inset > y0 {
            y1 - top_inset
        } else {
            y0 + height / 2.0
        };
        let mut paths = Vec::new();
        for row in 0..=rows {
            let y = if row == rows {
                last_y
            } else {
                y0 + height * row as f64 / rows as f64
            };
            let mut intervals = scanline_intervals(outer, y)?;
            for island in islands {
                let cuts = scanline_intervals(island, y)?;
                intervals = subtract_intervals(&intervals, &cuts);
            }
            if row % 2 == 1 {
                intervals.reverse();
            }
            for (start, end) in intervals {
                if end - start < MIN_SEGMENT {
                    continue;
                }
                let mut points = vec![Point::new(start, y), Point::new(end, y)];
                if row % 2 == 1 {
                    points.reverse();
                }
                paths.push(points);
            }
        }
        paths.push(closed_path(outer));
        for island in islands {
            paths.push(closed_path(island));
        }
        Ok(paths)
    }

    fn pocket_clearance_contours(&self, profile: &Profile) -> io::Result<Vec<Vec<Point>>> {
        let radius = self.tool_diameter / 2.0;
        let nominal_outer = pocket_contour(&profile.geometry)?;
        let nominal_islands = profile
            .islands
            .iter()
            .map(pocket_contour)
            .collect::<io::Result<Vec<_>>>()?;
        check_pocket_topology(&nominal_outer, &nominal_islands)?;
        let outer = pocket_contour(&profile.geometry.offset(-radius)?)?;
        let islands = profile
            .islands
            .iter()
            .map(|island| {
                let clearance = island.offset(radius)?;
                match clearance {
                    // The sampled 96-gon is inscribed in a circle. Circumscribe
                    // the island clearance circle so chord midpoints cannot
                    // encroach on the island by the sagitta.
                    Geometry::Circle { center, radius } => {
                        let sampled_radius = radius / (std::f64::consts::PI / 96.0).cos();
                        pocket_contour(&Geometry::Circle {
                            center,
                            radius: sampled_radius,
                        })
                    }
                    other => pocket_contour(&other),
                }
            })
            .collect::<io::Result<Vec<_>>>()?;
        check_pocket_topology(&outer, &islands).map_err(|_| {
            invalid(
                "Pocket walls and islands leave no room for the tool; reduce the tool diameter or increase spacing",
            )
        })?;
        let mut contours = vec![outer];
        contours.extend(islands);
        Ok(contours)
    }
    pub fn plan(&self) -> io::Result<Toolpath> {
        let paths = self.validated_paths()?;
        let mut result = Toolpath::default();
        let passes = pass_count(self.depth, self.step_down)?;
        if self.is_empty() {
            return Ok(result);
        }
        let mut previous = [0.0, 0.0, self.safe_z];
        result.motions.push(Motion {
            position: previous,
            rapid: true,
            feed: 0.0,
        });
        let mut push = |position: [f64; 3], rapid: bool, feed: f64| -> io::Result<()> {
            let distance = (position[0] - previous[0])
                .hypot(position[1] - previous[1])
                .hypot(position[2] - previous[2]);
            if !distance.is_finite() {
                return Err(invalid("CAM motion distance overflows"));
            }
            let z = posted(position[2])?;
            if z < self.stock_min[2] || z < -self.depth {
                return Err(invalid(
                    "Posted CAM depth exceeds requested depth or stock bottom",
                ));
            }
            result.total_distance += distance;
            result.estimated_minutes += distance / if rapid { 3000.0 } else { feed };
            if !rapid {
                result.cutting_distance += distance;
            }
            result.motions.push(Motion {
                position,
                rapid,
                feed,
            });
            previous = position;
            Ok(())
        };
        let mut path_count = 0usize;
        for profile_paths in &paths {
            for points in profile_paths {
                path_count += 1;
                for pass in 1..=passes {
                    let z = if pass == passes {
                        -self.depth
                    } else {
                        -(self.step_down * pass as f64).min(self.depth)
                    };
                    push([points[0].x, points[0].y, self.safe_z], true, 0.0)?;
                    push([points[0].x, points[0].y, z], false, self.plunge_feed)?;
                    for p in points.iter().skip(1) {
                        push([p.x, p.y, z], false, self.feed)?;
                    }
                    let last = points.last().expect("nonempty operation path");
                    push([last.x, last.y, self.safe_z], true, 0.0)?;
                }
            }
        }
        result.passes = passes * path_count;
        if !result.total_distance.is_finite() || !result.estimated_minutes.is_finite() {
            return Err(invalid("CAM path summary overflows"));
        }
        Ok(result)
    }
    /// G17 XY, G21 mm, G90 absolute coordinates, G94 units/minute. Clear Z
    /// before the first XY positioning move. No G28/home or tool-change motion.
    pub fn gcode(&self) -> io::Result<String> {
        let path = self.plan()?;
        if path.motions.is_empty() {
            return Err(invalid("Add a CAM profile before posting G-code"));
        }
        let mut text = String::from(
            "(Forge Craft 2.5D profile/pocket/drill; stock top Z0; estimated rapid 3000 mm/min)\nG17 G21 G90 G94 G40 G49 G80\n",
        );
        writeln!(text, "G0 Z{}", num(self.safe_z)).unwrap();
        writeln!(text, "S{} M3", self.spindle_rpm).unwrap();
        for motion in path.motions.iter().skip(1) {
            let [x, y, z] = motion.position;
            if motion.rapid {
                writeln!(text, "G0 X{} Y{} Z{}", num(x), num(y), num(z)).unwrap();
            } else {
                writeln!(
                    text,
                    "G1 X{} Y{} Z{} F{}",
                    num(x),
                    num(y),
                    num(z),
                    num(motion.feed)
                )
                .unwrap();
            }
        }
        text.push_str("M5\nM2\n");
        Ok(text)
    }
    pub fn example() -> Self {
        Self {
            profiles: vec![
                Profile {
                    id: 1,
                    name: "Outside contour".into(),
                    geometry: Geometry::Rectangle {
                        a: Point::new(0.0, 0.0),
                        b: Point::new(80.0, 60.0),
                    },
                    compensation: Compensation::Outside,
                    operation: Operation::Profile,
                    islands: Vec::new(),
                },
                Profile {
                    id: 2,
                    name: "Inner contour".into(),
                    geometry: Geometry::Circle {
                        center: Point::new(40.0, 30.0),
                        radius: 10.0,
                    },
                    compensation: Compensation::Inside,
                    operation: Operation::Profile,
                    islands: Vec::new(),
                },
            ],
            ..Default::default()
        }
    }
}
fn num(value: f64) -> String {
    let text = format!("{value:.12}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" || text.is_empty() {
        "0".into()
    } else {
        text.into()
    }
}
fn posted(value: f64) -> io::Result<f64> {
    if !value.is_finite() || value.abs() > MAX_COORDINATE {
        return Err(invalid("Posted CAM coordinate exceeds supported range"));
    }
    num(value)
        .parse()
        .map_err(|_| invalid("CAM coordinate cannot be represented in G-code"))
}
fn pass_count(depth: f64, step_down: f64) -> io::Result<usize> {
    let ratio = depth / step_down;
    let nearest = ratio.round();
    let count = if (ratio - nearest).abs() <= f64::EPSILON * ratio.abs().max(1.0) * 8.0 {
        nearest.max(1.0)
    } else {
        ratio.ceil()
    };
    if !count.is_finite() || count < 1.0 || count > 1000.0 {
        Err(invalid("CAM requires at most 1000 depth passes"))
    } else {
        Ok(count as usize)
    }
}
fn check_point(point: Point) -> io::Result<()> {
    if !point.is_finite() || point.x.abs() > MAX_COORDINATE || point.y.abs() > MAX_COORDINATE {
        Err(invalid("CAM coordinates must be within +/-1000000 mm"))
    } else {
        Ok(())
    }
}
fn simple_polygon(points: &[Point]) -> io::Result<()> {
    let cross =
        |a: Point, b: Point, c: Point| (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    let on = |p: Point, a: Point, b: Point| {
        p.x >= a.x.min(b.x) && p.x <= a.x.max(b.x) && p.y >= a.y.min(b.y) && p.y <= a.y.max(b.y)
    };
    let mut area = 0.0;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        if a.distance(b) < MIN_SEGMENT {
            return Err(invalid("CAM polygon contains a collapsed segment"));
        }
        area += cross(points[0], a, b);
        for j in i + 1..points.len() {
            if j == i + 1 || (i == 0 && j + 1 == points.len()) {
                continue;
            }
            let c = points[j];
            let d = points[(j + 1) % points.len()];
            let [p, q, r, s] = [
                cross(a, b, c),
                cross(a, b, d),
                cross(c, d, a),
                cross(c, d, b),
            ];
            if (p == 0.0 && on(c, a, b))
                || (q == 0.0 && on(d, a, b))
                || (r == 0.0 && on(a, c, d))
                || (s == 0.0 && on(b, c, d))
                || (((p < 0.0 && q > 0.0) || (p > 0.0 && q < 0.0))
                    && ((r < 0.0 && s > 0.0) || (r > 0.0 && s < 0.0)))
            {
                return Err(invalid(
                    "CAM polygon must be simple and non-self-intersecting",
                ));
            }
        }
    }
    if !area.is_finite() || area == 0.0 {
        Err(invalid("CAM polygon must enclose finite nonzero area"))
    } else {
        Ok(())
    }
}
fn add_polyline_work(work: &mut usize, vertices: usize) -> io::Result<()> {
    let cost = vertices
        .checked_mul(vertices)
        .ok_or_else(|| invalid("CAM polyline complexity overflows"))?;
    *work = work
        .checked_add(cost)
        .ok_or_else(|| invalid("CAM polyline complexity overflows"))?;
    if *work > MAX_POLYLINE_WORK {
        Err(invalid(
            "Combined CAM polyline complexity exceeds the bounded validation limit",
        ))
    } else {
        Ok(())
    }
}

/// Convert a supported closed pocket/island shape to a simple point contour.
/// Arcs and bulges are intentionally refused so scanline and offset operations
/// can retain their exact linear semantics.
fn pocket_contour(geometry: &Geometry) -> io::Result<Vec<Point>> {
    geometry.validate()?;
    let points = match geometry {
        Geometry::Rectangle { a, b } => {
            check_point(*a)?;
            check_point(*b)?;
            if a.x == b.x || a.y == b.y {
                return Err(invalid("Pocket rectangle must be nondegenerate"));
            }
            vec![*a, Point::new(b.x, a.y), *b, Point::new(a.x, b.y)]
        }
        Geometry::Circle { center, radius } => {
            check_point(*center)?;
            if *radius > MAX_COORDINATE {
                return Err(invalid("Pocket circle exceeds the coordinate range"));
            }
            (0..96)
                .map(|index| {
                    let angle = std::f64::consts::TAU * index as f64 / 96.0;
                    Point::new(
                        center.x + radius * angle.cos(),
                        center.y + radius * angle.sin(),
                    )
                })
                .collect()
        }
        Geometry::Polyline {
            vertices,
            closed: true,
        } if (3..=2048).contains(&vertices.len())
            && vertices.iter().all(|vertex| vertex.bulge == 0.0) =>
        {
            let points: Vec<_> = vertices.iter().map(|vertex| vertex.point).collect();
            for point in &points {
                check_point(*point)?;
            }
            simple_polygon(&points)?;
            points
        }
        _ => {
            return Err(invalid(
                "Pocket boundaries and islands must be rectangles, circles or closed straight polylines",
            ));
        }
    };
    if points.len() < 3 {
        return Err(invalid("Pocket contour has too few points"));
    }
    Ok(points)
}

fn bounds_y(points: &[Point]) -> Option<(f64, f64)> {
    let first = points.first()?;
    if !first.is_finite() {
        return None;
    }
    let mut low = first.y;
    let mut high = first.y;
    for point in &points[1..] {
        if !point.is_finite() {
            return None;
        }
        low = low.min(point.y);
        high = high.max(point.y);
    }
    Some((low, high))
}

/// Intersections of a horizontal line with a simple closed contour, returned
/// as disjoint increasing spans. The half-open Y rule avoids double counting
/// shared vertices; duplicate tangent crossings are collapsed in pairs.
fn scanline_intervals(polygon: &[Point], y: f64) -> io::Result<Vec<(f64, f64)>> {
    if polygon.len() < 3 || !y.is_finite() {
        return Err(invalid("Pocket scanline requires a finite contour"));
    }
    let mut crossings = Vec::new();
    for index in 0..polygon.len() {
        let a = polygon[index];
        let b = polygon[(index + 1) % polygon.len()];
        if !a.is_finite() || !b.is_finite() {
            return Err(invalid("Pocket contour contains nonfinite coordinates"));
        }
        if (a.y > y) != (b.y > y) {
            let t = (y - a.y) / (b.y - a.y);
            let x = if t <= 0.0 {
                a.x
            } else if t >= 1.0 {
                b.x
            } else {
                a.x + (b.x - a.x) * t
            };
            if !x.is_finite() {
                return Err(invalid("Pocket scanline intersection overflows"));
            }
            crossings.push(x);
        }
    }
    crossings.sort_by(f64::total_cmp);
    let mut compact = Vec::with_capacity(crossings.len());
    let mut index = 0;
    while index < crossings.len() {
        if index + 1 < crossings.len() && crossings[index + 1] == crossings[index] {
            index += 2;
        } else {
            compact.push(crossings[index]);
            index += 1;
        }
    }
    if compact.len() % 2 != 0 {
        return Err(invalid(
            "Pocket scanline intersects the boundary at an unsupported vertex",
        ));
    }
    let mut intervals = Vec::with_capacity(compact.len() / 2);
    for index in (0..compact.len()).step_by(2) {
        if compact[index + 1] > compact[index] {
            intervals.push((compact[index], compact[index + 1]));
        }
    }
    Ok(intervals)
}

fn subtract_intervals(intervals: &[(f64, f64)], cuts: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut cuts: Vec<_> = cuts.iter().copied().filter(|(a, b)| b > a).collect();
    cuts.sort_by(|left, right| left.0.total_cmp(&right.0));
    let mut merged: Vec<(f64, f64)> = Vec::with_capacity(cuts.len());
    for (start, end) in cuts {
        if let Some(last) = merged.last_mut()
            && start <= last.1
        {
            last.1 = last.1.max(end);
            continue;
        }
        merged.push((start, end));
    }
    let mut result = Vec::new();
    for &(start, end) in intervals {
        let mut cursor = start;
        for &(cut_start, cut_end) in &merged {
            if cut_end <= cursor {
                continue;
            }
            if cut_start >= end {
                break;
            }
            if cut_start > cursor {
                result.push((cursor, cut_start.min(end)));
            }
            cursor = cursor.max(cut_end);
            if cursor >= end {
                break;
            }
        }
        if cursor < end {
            result.push((cursor, end));
        }
    }
    result
}

fn closed_path(points: &[Point]) -> Vec<Point> {
    let mut path = points.to_vec();
    if let Some(first) = path.first().copied() {
        path.push(first);
    }
    path
}

fn point_in_polygon(point: Point, polygon: &[Point]) -> bool {
    let mut inside = false;
    for index in 0..polygon.len() {
        let a = polygon[index];
        let b = polygon[(index + 1) % polygon.len()];
        if (a.y > point.y) != (b.y > point.y) {
            let x = a.x + (point.y - a.y) * (b.x - a.x) / (b.y - a.y);
            if x > point.x {
                inside = !inside;
            }
        }
    }
    inside
}

fn orientation(a: Point, b: Point, c: Point) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

fn on_segment(a: Point, b: Point, point: Point) -> bool {
    point.x >= a.x.min(b.x)
        && point.x <= a.x.max(b.x)
        && point.y >= a.y.min(b.y)
        && point.y <= a.y.max(b.y)
}

fn segments_intersect(a: Point, b: Point, c: Point, d: Point) -> bool {
    let o1 = orientation(a, b, c);
    let o2 = orientation(a, b, d);
    let o3 = orientation(c, d, a);
    let o4 = orientation(c, d, b);
    if ((o1 > 0.0 && o2 < 0.0) || (o1 < 0.0 && o2 > 0.0))
        && ((o3 > 0.0 && o4 < 0.0) || (o3 < 0.0 && o4 > 0.0))
    {
        return true;
    }
    (o1 == 0.0 && on_segment(a, b, c))
        || (o2 == 0.0 && on_segment(a, b, d))
        || (o3 == 0.0 && on_segment(c, d, a))
        || (o4 == 0.0 && on_segment(c, d, b))
}

fn contours_intersect(left: &[Point], right: &[Point]) -> bool {
    for left_index in 0..left.len() {
        let a = left[left_index];
        let b = left[(left_index + 1) % left.len()];
        for right_index in 0..right.len() {
            let c = right[right_index];
            let d = right[(right_index + 1) % right.len()];
            if segments_intersect(a, b, c, d) {
                return true;
            }
        }
    }
    false
}

fn check_pocket_topology(outer: &[Point], islands: &[Vec<Point>]) -> io::Result<()> {
    if outer.len() < 3 {
        return Err(invalid("Pocket boundary has too few points"));
    }
    simple_polygon(outer)?;
    for (index, island) in islands.iter().enumerate() {
        if island.len() < 3 {
            return Err(invalid("Pocket island has too few points"));
        }
        simple_polygon(island)?;
        if contours_intersect(outer, island) {
            return Err(invalid(
                "Pocket island intersects or touches the pocket boundary",
            ));
        }
        if !island.iter().all(|point| point_in_polygon(*point, outer)) {
            return Err(invalid("Pocket island must lie inside the pocket boundary"));
        }
        for previous in &islands[..index] {
            if contours_intersect(previous, island)
                || point_in_polygon(island[0], previous)
                || point_in_polygon(previous[0], island)
            {
                return Err(invalid("Pocket islands must not overlap or nest"));
            }
        }
    }
    Ok(())
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn posting_preserves_small_positive_values_and_never_uses_exponents() {
        let mut job = CamJob::example();
        job.profiles.truncate(1);
        job.feed = MIN_SETTING;
        job.plunge_feed = MIN_SETTING;
        job.depth = MIN_SETTING;
        job.step_down = MIN_SETTING;
        job.safe_z = MIN_SETTING;
        let program = job.gcode().unwrap();
        assert!(program.contains("G0 Z0.000001\n"));
        assert!(program.contains(" Z-0.000001 F0.000001\n"));
        for line in program.lines().filter(|line| line.starts_with('G')) {
            for word in line
                .split_whitespace()
                .filter(|w| w.starts_with(['X', 'Y', 'Z', 'F']))
            {
                assert!(!word.contains(['e', 'E']));
                if let Some(feed) = word.strip_prefix('F') {
                    assert!(feed.parse::<f64>().unwrap() > 0.0);
                }
            }
        }
        for value in [1e-7, 1e-12, 1e-300] {
            let mut bad = job.clone();
            bad.feed = value;
            assert!(bad.gcode().is_err());
        }
        assert_eq!(num(-0.0), "0");
    }
    #[test]
    fn subresolution_profiles_and_unrepresentable_depths_are_refused() {
        let mut job = CamJob::example();
        job.profiles.truncate(1);
        job.profiles[0].compensation = Compensation::Center;
        job.profiles[0].geometry = Geometry::Rectangle {
            a: Point::new(0.0, 0.0),
            b: Point::new(1e-10, 1.0),
        };
        assert!(job.plan().is_err());
        job = CamJob::example();
        job.depth = 0.1234567890126;
        assert!(job.gcode().is_err());
        job = CamJob::example();
        job.safe_z = 0.1234567890126;
        assert!(job.gcode().is_err());
        job = CamJob::example();
        job.stock_max[0] = 1e308;
        assert!(job.plan().is_err());
    }
    #[test]
    fn decimal_stepdown_has_no_duplicate_final_depth_pass() {
        let mut job = CamJob::example();
        job.profiles.truncate(1);
        job.depth = 0.07;
        job.step_down = 0.01;
        let path = job.plan().unwrap();
        assert_eq!(path.passes, 7);
        let depths: Vec<_> = path
            .motions
            .iter()
            .filter(|m| !m.rapid && m.feed == job.plunge_feed)
            .map(|m| m.position[2])
            .collect();
        assert_eq!(depths.len(), 7);
        assert_eq!(*depths.last().unwrap(), -0.07);
        assert!(depths.windows(2).all(|pair| pair[1] < pair[0]));
    }
    #[test]
    fn excessive_paths_fail_preflight_before_polygon_geometry_work() {
        let mut job = CamJob::example();
        job.depth = 1000.0;
        job.step_down = 1.0;
        job.stock_min[2] = -1000.0;
        job.profiles = vec![job.profiles[1].clone(); 6];
        for (index, p) in job.profiles.iter_mut().enumerate() {
            p.id = index as u64 + 1;
        }
        assert!(job.plan().unwrap_err().to_string().contains("500,000"));
        let vertices: Vec<_> = (0..2048)
            .map(|i| crate::PolylineVertex::new(Point::new(i as f64, 0.0)))
            .collect();
        let profile = Profile {
            id: 1,
            name: "Heavy".into(),
            geometry: Geometry::Polyline {
                vertices,
                closed: true,
            },
            compensation: Compensation::Center,
            operation: Operation::Profile,
            islands: Vec::new(),
        };
        let mut job = CamJob::default();
        job.profiles = vec![profile; 3];
        for (index, p) in job.profiles.iter_mut().enumerate() {
            p.id = index as u64 + 1;
        }
        assert!(job.plan().unwrap_err().to_string().contains("complexity"));
    }
    #[test]
    fn centered_polygons_validate_without_arbitrary_offset_probe() {
        let mut job = CamJob::default();
        job.stock_min = [900000.0, 900000.0, -10.0];
        job.stock_max = [901000.0, 901000.0, 0.0];
        job.profiles = vec![Profile {
            id: 1,
            name: "Far origin".into(),
            compensation: Compensation::Center,
            operation: Operation::Profile,
            islands: Vec::new(),
            geometry: Geometry::Polyline {
                vertices: vec![
                    Point::new(900100.0, 900100.0),
                    Point::new(900200.0, 900100.0),
                    Point::new(900200.0, 900200.0),
                    Point::new(900100.0, 900200.0),
                ]
                .into_iter()
                .map(crate::PolylineVertex::new)
                .collect(),
                closed: true,
            },
        }];
        job.tool_diameter = 0.00001;
        assert!(job.plan().is_ok());
        job.profiles[0].geometry = Geometry::Polyline {
            vertices: vec![
                Point::new(900100.0, 900100.0),
                Point::new(900200.0, 900200.0),
                Point::new(900100.0, 900200.0),
                Point::new(900200.0, 900100.0),
            ]
            .into_iter()
            .map(crate::PolylineVertex::new)
            .collect(),
            closed: true,
        };
        assert!(job.plan().is_err());
    }
    #[test]
    fn stepdowns_clamp_final_depth_and_positioning_retracts_first() {
        let mut job = CamJob::example();
        job.profiles.truncate(1);
        job.depth = 7.0;
        job.step_down = 3.0;
        let path = job.plan().unwrap();
        assert_eq!(path.passes, 3);
        let depths: Vec<_> = path
            .motions
            .iter()
            .filter(|m| !m.rapid && m.feed == job.plunge_feed)
            .map(|m| m.position[2])
            .collect();
        assert_eq!(depths, [-3.0, -6.0, -7.0]);
        for pair in path.motions.windows(2) {
            if pair[1].rapid && pair[0].position[..2] != pair[1].position[..2] {
                assert_eq!(pair[0].position[2], job.safe_z);
                assert_eq!(pair[1].position[2], job.safe_z);
            }
        }
        let code = job.gcode().unwrap();
        assert!(code.starts_with("(Forge Craft"));
        assert!(code.find("G0 Z5").unwrap() < code.find("G0 X").unwrap());
        assert!(code.ends_with("M5\nM2\n"));
    }
    #[test]
    fn tool_compensation_changes_actual_path() {
        let job = CamJob::example();
        let path = job.plan().unwrap();
        assert_eq!(path.motions[1].position, [-1.5, -1.5, 5.0]);
        let circle = job.profile_points(&job.profiles[1]).unwrap();
        assert!((circle[0].distance(Point::new(40.0, 30.0)) - 8.5).abs() < 1e-9);
        assert!(path.estimated_minutes > 0.0);
    }
    #[test]
    fn invalid_jobs_and_collapsed_compensation_are_refused() {
        let original = CamJob::example();
        for value in [0.0, -1.0, f64::INFINITY, f64::NAN] {
            let mut job = original.clone();
            job.step_down = value;
            assert!(job.plan().is_err());
        }
        let mut job = original.clone();
        job.depth = 11.0;
        assert!(job.plan().is_err());
        job = original.clone();
        job.stock_max[0] = 80.0;
        assert!(job.plan().is_err());
        job = original.clone();
        job.tool_diameter = 22.0;
        assert!(job.plan().is_err());
        job = original;
        job.profiles[0].geometry = Geometry::Line {
            start: Point::default(),
            end: Point::new(1.0, 2.0),
        };
        assert!(job.gcode().is_err());
        assert!(CamJob::default().gcode().is_err());
    }
    #[test]
    fn cam_settings_and_profiles_round_trip_semantically() {
        let job = CamJob::example();
        let json = serde_json::to_string(&job).unwrap();
        assert_eq!(serde_json::from_str::<CamJob>(&json).unwrap(), job);
    }
}
