//! Polyline horizontal alignments and exact TIN quantities against a level datum.
use crate::{
    Point,
    terrain::{ProfileSample, Terrain},
};
use serde::{Deserialize, Serialize};
use std::{fmt::Write, io};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Alignment {
    pub id: u64,
    pub name: String,
    pub start_station: f64,
    pub vertices: Vec<Point>,
    /// tan(signed sweep / 4) for each segment; empty means all straight.
    #[serde(default)]
    pub bulges: Vec<f64>,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct StationOffset {
    pub station: f64,
    /// Positive to the left of the selected segment, negative to its right.
    pub offset: f64,
    pub position: Point,
    /// Distance to the clamped alignment, including any endpoint overrun.
    pub distance: f64,
}

impl Alignment {
    pub fn validate(&self) -> io::Result<()> {
        self.segments().map(|_| ())
    }

    fn segments(&self) -> io::Result<Vec<AlignmentSegment>> {
        if self.id == 0
            || self.name.trim().is_empty()
            || self.name.contains(['\r', '\n', '\0'])
            || !self.start_station.is_finite()
            || !(2..=2048).contains(&self.vertices.len())
            || self.vertices.iter().any(|p| !p.is_finite())
            || (!self.bulges.is_empty() && self.bulges.len() != self.vertices.len() - 1)
        {
            return Err(invalid(
                "Alignments require a nonzero ID, single-line name, finite start station and 2..2048 vertices",
            ));
        }
        let mut length = 0.0;
        let mut output = Vec::with_capacity(self.vertices.len() - 1);
        for (index, pair) in self.vertices.windows(2).enumerate() {
            let bulge = self.bulges.get(index).copied().unwrap_or(0.0);
            if !bulge.is_finite() {
                return Err(invalid("Alignment bulges must be finite"));
            }
            let arc = if bulge == 0.0 {
                None
            } else {
                Some(
                    crate::bulge_arc(pair[0], pair[1], bulge)
                        .ok_or_else(|| invalid("Invalid circular alignment segment"))?,
                )
            };
            let segment = arc
                .map(|(_, r, _, s)| r * s.to_radians().abs())
                .unwrap_or_else(|| pair[0].distance(pair[1]));
            if !segment.is_finite() || segment <= 0.0 || length + segment <= length {
                return Err(invalid(
                    "Alignment segments must have finite positive representable lengths",
                ));
            }
            length += segment;
            if let Some((center, radius, _, _)) = arc {
                if [
                    center.x - radius,
                    center.x + radius,
                    center.y - radius,
                    center.y + radius,
                ]
                .iter()
                .any(|v| !v.is_finite())
                {
                    return Err(invalid("Circular alignment extents exceed numeric range"));
                }
            }
            output.push(AlignmentSegment {
                a: pair[0],
                b: pair[1],
                length: segment,
                arc,
            });
        }
        let end = self.start_station + length;
        if !length.is_finite() || !end.is_finite() || end <= self.start_station {
            return Err(invalid("Alignment station range exceeds numeric precision"));
        }
        Ok(output)
    }

    pub fn length(&self) -> io::Result<f64> {
        Ok(self.segments()?.iter().map(|s| s.length).sum())
    }

    /// At an internal vertex offsets follow the outgoing segment. Station must
    /// be within the authored range; no extrapolation or corner fillet is made.
    pub fn point_at(&self, station: f64, offset: f64) -> io::Result<Point> {
        let segments = self.segments()?;
        let length: f64 = segments.iter().map(|s| s.length).sum();
        self.point_on_segments(station, offset, &segments, length)
    }

    fn point_on_segments(
        &self,
        station: f64,
        offset: f64,
        segments: &[AlignmentSegment],
        length: f64,
    ) -> io::Result<Point> {
        if !station.is_finite()
            || !offset.is_finite()
            || station < self.start_station
            || station > self.start_station + length
        {
            return Err(invalid(
                "Station must lie on the alignment and offset must be finite",
            ));
        }
        if station == self.start_station + length {
            let last = segments
                .last()
                .ok_or_else(|| invalid("Alignment has no segments"))?;
            return last.position(last.length, offset);
        }
        let mut prefix = 0.0;
        for (index, segment) in segments.iter().enumerate() {
            if station < self.start_station + (prefix + segment.length)
                || index + 1 == segments.len()
            {
                return segment.position(
                    (station - (self.start_station + prefix)).clamp(0.0, segment.length),
                    offset,
                );
            }
            prefix += segment.length;
        }
        Err(invalid("Station could not be located on the alignment"))
    }

    /// Nearest segment projection; ties select the earliest segment. Station
    /// clamps to endpoints and offset is the signed perpendicular component.
    pub fn station_offset(&self, query: Point) -> io::Result<StationOffset> {
        let segments = self.segments()?;
        if !query.is_finite() {
            return Err(invalid("Alignment query must be finite"));
        }
        let mut best: Option<StationOffset> = None;
        let mut prefix = 0.0;
        for segment in &segments {
            let (distance_along, position, offset) = segment.project(query)?;
            let candidate = StationOffset {
                station: self.start_station + prefix + distance_along,
                offset,
                distance: query.distance(position),
                position,
            };
            if !candidate.station.is_finite()
                || !candidate.offset.is_finite()
                || !candidate.distance.is_finite()
                || !position.is_finite()
            {
                return Err(invalid("Alignment projection exceeds numeric range"));
            }
            if best.is_none_or(|b| candidate.distance < b.distance) {
                best = Some(candidate);
            }
            prefix += segment.length;
        }
        best.ok_or_else(|| invalid("Alignment has no projectable segments"))
    }

    /// Preview tessellation only; stationing and projection stay analytic.
    pub fn display_lines(&self) -> io::Result<Vec<(Point, Point)>> {
        let segments = self.segments()?;
        let mut output = Vec::new();
        for s in segments {
            let count = s
                .arc
                .map(|(_, _, _, sweep)| (sweep.abs() / 3.0).ceil() as usize)
                .unwrap_or(1)
                .max(1);
            if output.len() + count > 100_000 {
                return Err(invalid("Alignment preview exceeds 100000 segments"));
            }
            for i in 0..count {
                output.push((
                    s.position(s.length * (i as f64 / count as f64), 0.0)?,
                    s.position(s.length * ((i + 1) as f64 / count as f64), 0.0)?,
                ));
            }
        }
        Ok(output)
    }
}

struct AlignmentSegment {
    a: Point,
    b: Point,
    length: f64,
    arc: Option<(Point, f64, f64, f64)>,
}
impl AlignmentSegment {
    fn tangent(&self, distance: f64) -> (f64, f64) {
        if let Some((_, _, start, sweep)) = self.arc {
            let angle = (start + sweep * (distance / self.length)).to_radians();
            (-angle.sin() * sweep.signum(), angle.cos() * sweep.signum())
        } else {
            (
                (self.b.x - self.a.x) / self.length,
                (self.b.y - self.a.y) / self.length,
            )
        }
    }
    fn position(&self, distance: f64, offset: f64) -> io::Result<Point> {
        let result = if let Some((center, radius, start, sweep)) = self.arc {
            let r = radius - sweep.signum() * offset;
            if !r.is_finite() || r <= radius * f64::EPSILON * 64.0 {
                return Err(invalid(
                    "Alignment offset collapses or reverses a circular segment",
                ));
            }
            if offset == 0.0 && distance == 0.0 {
                self.a
            } else if offset == 0.0 && distance == self.length {
                self.b
            } else {
                crate::arc_point(center, r, start + sweep * (distance / self.length))
            }
        } else {
            let (dx, dy) = self.tangent(distance);
            Point::new(
                self.a.x + dx * distance - dy * offset,
                self.a.y + dy * distance + dx * offset,
            )
        };
        if !result.is_finite() {
            return Err(invalid("Alignment offset exceeds numeric range"));
        }
        Ok(result)
    }
    fn project(&self, query: Point) -> io::Result<(f64, Point, f64)> {
        let along = if let Some((center, _, start, sweep)) = self.arc {
            let (qx, qy) = (query.x - center.x, query.y - center.y);
            if qx == 0.0 && qy == 0.0 {
                0.0
            } else {
                let angle = qy.atan2(qx).to_degrees();
                let delta = if sweep > 0.0 {
                    (angle - start).rem_euclid(360.0)
                } else {
                    (start - angle).rem_euclid(360.0)
                };
                if delta <= sweep.abs() + 1e-10 {
                    self.length * (delta / sweep.abs()).min(1.0)
                } else if query.distance(self.a) <= query.distance(self.b) {
                    0.0
                } else {
                    self.length
                }
            }
        } else {
            let (dx, dy) = self.tangent(0.0);
            ((query.x - self.a.x) * dx + (query.y - self.a.y) * dy).clamp(0.0, self.length)
        };
        let point = self.position(along, 0.0)?;
        let (dx, dy) = self.tangent(along);
        Ok((
            along,
            point,
            -(query.x - point.x) * dy + (query.y - point.y) * dx,
        ))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct Earthwork {
    pub datum: f64,
    pub area: f64,
    pub cut: f64,
    pub fill: f64,
    /// Positive means material to remove, negative means material to add.
    pub net_cut: f64,
}

impl Terrain {
    pub fn alignment_profile(
        &self,
        id: u64,
        samples: usize,
        offset: f64,
    ) -> io::Result<Vec<ProfileSample>> {
        self.validate()?;
        if !(2..=10_000).contains(&samples) {
            return Err(invalid("Alignment profiles require 2..10000 samples"));
        }
        let alignment = self
            .alignments
            .iter()
            .find(|a| a.id == id)
            .ok_or_else(|| invalid("Alignment does not exist"))?;
        let segments = alignment.segments()?;
        let length: f64 = segments.iter().map(|s| s.length).sum();
        (0..samples)
            .map(|i| {
                let station = alignment.start_station + length * (i as f64 / (samples - 1) as f64);
                let position = alignment.point_on_segments(station, offset, &segments, length)?;
                Ok(ProfileSample {
                    distance: station,
                    position,
                    elevation: self.elevation_unchecked(position)?,
                })
            })
            .collect()
    }

    pub fn alignment_csv(&self, id: u64, samples: usize, offset: f64) -> io::Result<String> {
        let mut out = String::from("station,x,y,elevation\n");
        for sample in self.alignment_profile(id, samples, offset)? {
            let z = sample.elevation.map(|z| z.to_string()).unwrap_or_default();
            writeln!(
                out,
                "{},{},{},{}",
                sample.distance, sample.position.x, sample.position.y, z
            )
            .map_err(io::Error::other)?;
        }
        Ok(out)
    }

    /// Integrate the piecewise linear height exactly over every TIN triangle,
    /// clipping triangles at zero height before summing positive/negative volume.
    /// Quantities are in project units squared/cubed; voids are excluded.
    pub fn earthwork(&self, datum: f64) -> io::Result<Earthwork> {
        self.validate()?;
        if !datum.is_finite() || self.triangles.is_empty() {
            return Err(invalid(
                "Earthwork requires a finite datum and a triangulated surface",
            ));
        }
        let mut result = Earthwork {
            datum,
            ..Earthwork::default()
        };
        for face in &self.triangles {
            let vertices = face.map(|i| {
                let [x, y, z] = self.points[i];
                [x, y, z - datum]
            });
            if vertices.iter().flatten().any(|v| !v.is_finite()) {
                return Err(invalid("Earthwork heights exceed numeric range"));
            }
            result.area += triangle_area(vertices);
            result.cut += clipped_volume(&vertices, 1.0);
            result.fill += clipped_volume(&vertices, -1.0);
        }
        result.net_cut = result.cut - result.fill;
        if [result.area, result.cut, result.fill, result.net_cut]
            .iter()
            .any(|v| !v.is_finite())
        {
            return Err(invalid("Earthwork quantities exceed numeric range"));
        }
        Ok(result)
    }
}

fn triangle_area(v: [[f64; 3]; 3]) -> f64 {
    ((v[1][0] - v[0][0]) * (v[2][1] - v[0][1]) - (v[1][1] - v[0][1]) * (v[2][0] - v[0][0])).abs()
        / 2.0
}
fn clipped_volume(vertices: &[[f64; 3]; 3], sign: f64) -> f64 {
    let mut polygon = Vec::with_capacity(4);
    for index in 0..3 {
        let a = vertices[index];
        let b = vertices[(index + 1) % 3];
        let inside_a = a[2] * sign >= 0.0;
        let inside_b = b[2] * sign >= 0.0;
        if inside_a {
            polygon.push(a);
        }
        if inside_a != inside_b {
            let scale = a[2].abs().max(b[2].abs());
            let t = (a[2].abs() / scale) / (a[2].abs() / scale + b[2].abs() / scale);
            polygon.push([
                a[0] * (1.0 - t) + b[0] * t,
                a[1] * (1.0 - t) + b[1] * t,
                0.0,
            ]);
        }
    }
    (1..polygon.len().saturating_sub(1))
        .map(|i| {
            let v = [polygon[0], polygon[i], polygon[i + 1]];
            triangle_area(v) * (v[0][2] / 3.0 + v[1][2] / 3.0 + v[2][2] / 3.0) * sign
        })
        .sum()
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}
