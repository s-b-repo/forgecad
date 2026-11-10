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
        if self.id == 0
            || self.name.trim().is_empty()
            || self.name.contains(['\r', '\n', '\0'])
            || !self.start_station.is_finite()
            || !(2..=2048).contains(&self.vertices.len())
            || self.vertices.iter().any(|p| !p.is_finite())
        {
            return Err(invalid(
                "Alignments require a nonzero ID, single-line name, finite start station and 2..2048 vertices",
            ));
        }
        let mut length = 0.0;
        for pair in self.vertices.windows(2) {
            let segment = pair[0].distance(pair[1]);
            if !segment.is_finite() || segment <= 0.0 || length + segment <= length {
                return Err(invalid(
                    "Alignment segments must have finite positive representable lengths",
                ));
            }
            length += segment;
        }
        let end = self.start_station + length;
        if !length.is_finite() || !end.is_finite() || end <= self.start_station {
            return Err(invalid("Alignment station range exceeds numeric precision"));
        }
        Ok(())
    }

    pub fn length(&self) -> io::Result<f64> {
        self.validate()?;
        Ok(self.vertices.windows(2).map(|p| p[0].distance(p[1])).sum())
    }

    /// At an internal vertex offsets follow the outgoing segment. Station must
    /// be within the authored range; no extrapolation or corner fillet is made.
    pub fn point_at(&self, station: f64, offset: f64) -> io::Result<Point> {
        let length = self.length()?;
        if !station.is_finite()
            || !offset.is_finite()
            || station < self.start_station
            || station > self.start_station + length
        {
            return Err(invalid(
                "Station must lie on the alignment and offset must be finite",
            ));
        }
        let mut distance = (station - self.start_station).min(length);
        for (index, pair) in self.vertices.windows(2).enumerate() {
            let segment = pair[0].distance(pair[1]);
            if distance < segment || index + 2 == self.vertices.len() {
                let (dx, dy) = (
                    (pair[1].x - pair[0].x) / segment,
                    (pair[1].y - pair[0].y) / segment,
                );
                let distance = distance.clamp(0.0, segment);
                let result = Point::new(
                    pair[0].x + dx * distance - dy * offset,
                    pair[0].y + dy * distance + dx * offset,
                );
                if !result.is_finite() {
                    return Err(invalid("Alignment offset exceeds numeric range"));
                }
                return Ok(result);
            }
            distance -= segment;
        }
        unreachable!("validated alignment")
    }

    /// Nearest segment projection; ties select the earliest segment. Station
    /// clamps to endpoints and offset is the signed perpendicular component.
    pub fn station_offset(&self, query: Point) -> io::Result<StationOffset> {
        self.validate()?;
        if !query.is_finite() {
            return Err(invalid("Alignment query must be finite"));
        }
        let mut best: Option<StationOffset> = None;
        let mut prefix = 0.0;
        for pair in self.vertices.windows(2) {
            let length = pair[0].distance(pair[1]);
            let (dx, dy) = (
                (pair[1].x - pair[0].x) / length,
                (pair[1].y - pair[0].y) / length,
            );
            let (qx, qy) = (query.x - pair[0].x, query.y - pair[0].y);
            let distance_along = (qx * dx + qy * dy).clamp(0.0, length);
            let position = Point::new(
                pair[0].x + dx * distance_along,
                pair[0].y + dy * distance_along,
            );
            let candidate = StationOffset {
                station: self.start_station + prefix + distance_along,
                offset: -qx * dy + qy * dx,
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
            prefix += length;
        }
        Ok(best.expect("validated segments"))
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
        let length = alignment.length()?;
        (0..samples)
            .map(|i| {
                let station = alignment.start_station + length * (i as f64 / (samples - 1) as f64);
                let position = alignment.point_at(station, offset)?;
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
            .unwrap();
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
