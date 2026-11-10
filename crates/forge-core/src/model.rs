//! Editable profile extrusions with checked derived, outward-facing meshes.
//! This supplies a bounded feature model, not general B-rep history or booleans.
use crate::{
    Document, Geometry, Point,
    mesh::{TessellationOptions, TriangleMesh, tessellate_solids, triangulate_polygon},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, io};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Extrusion {
    pub id: u64,
    pub name: String,
    /// Profile XY coordinates use the project units; the source remains editable.
    pub profile: Geometry,
    pub base_z: f64,
    pub height: f64,
}
impl Extrusion {
    pub fn complexity(&self) -> io::Result<usize> {
        match &self.profile {
            Geometry::Rectangle { .. } => Ok(4),
            Geometry::Circle { .. } => Ok(96),
            Geometry::Polyline {
                vertices,
                closed: true,
            } if (3..=256).contains(&vertices.len()) && vertices.iter().all(|v| v.bulge == 0.0) => {
                Ok(vertices.len())
            }
            _ => Err(invalid(
                "Extrusion requires a rectangle, circle or closed straight polyline with 3..256 vertices",
            )),
        }
    }
    pub fn validate(&self) -> io::Result<()> {
        self.mesh().map(|_| ())
    }
    pub fn mesh(&self) -> io::Result<TriangleMesh> {
        self.complexity()?;
        self.profile.validate()?;
        if self.id == 0
            || self.name.trim().is_empty()
            || self.name.len() > 1024
            || self.name.contains(['\r', '\n', '\0'])
            || !self.height.is_finite()
            || !(0.000001..=1e9).contains(&self.height)
            || !self.base_z.is_finite()
            || self.base_z.abs() > 1e9
            || self.base_z + self.height == self.base_z
        {
            return Err(invalid(
                "Extrusions require a nonzero ID, single-line name, finite base within +/-1e9 and representable height from 0.000001 to 1e9 model units",
            ));
        }
        let mut points = match &self.profile {
            Geometry::Rectangle { a, b } => {
                vec![*a, Point::new(b.x, a.y), *b, Point::new(a.x, b.y)]
            }
            Geometry::Circle { center, radius } => (0..96)
                .map(|i| {
                    let a = std::f64::consts::TAU * i as f64 / 96.0;
                    Point::new(center.x + radius * a.cos(), center.y + radius * a.sin())
                })
                .collect(),
            Geometry::Polyline { vertices, .. } => vertices.iter().map(|v| v.point).collect(),
            _ => return Err(invalid("Unsupported extrusion profile")),
        };
        if points.first() == points.last() {
            points.pop();
        }
        if points.len() < 3
            || points
                .iter()
                .any(|p| !p.is_finite() || p.x.abs() > 1e9 || p.y.abs() > 1e9)
        {
            return Err(invalid(
                "Extrusion profile requires finite distinct polygon vertices within +/-1e9 model units",
            ));
        }
        let origin = points[0];
        let area: f64 = (0..points.len())
            .map(|i| {
                let (a, b) = (points[i], points[(i + 1) % points.len()]);
                (a.x - origin.x) * (b.y - origin.y) - (a.y - origin.y) * (b.x - origin.x)
            })
            .sum();
        if area < 0.0 {
            points.reverse();
        }
        let base: Vec<_> = points.iter().map(|p| [p.x, p.y, self.base_z]).collect();
        let indices: Vec<_> = (0..base.len() as u32).collect();
        let caps = triangulate_polygon(&base, &indices).map_err(invalid)?;
        // The shared triangulator removes redundant collinear boundary vertices.
        // Use the same reduced boundary for walls to avoid cap/wall T-junctions.
        let used: BTreeSet<_> = caps.iter().flatten().copied().collect();
        let mut remap = vec![0u32; base.len()];
        let mut vertices = Vec::with_capacity(used.len() * 2);
        for (index, p) in base.iter().enumerate() {
            if used.contains(&(index as u32)) {
                remap[index] = vertices.len() as u32;
                vertices.push(*p);
            }
        }
        let n = vertices.len() as u32;
        for index in 0..n as usize {
            let p = vertices[index];
            vertices.push([p[0], p[1], self.base_z + self.height]);
        }
        let mut triangles = Vec::with_capacity(caps.len() * 2 + n as usize * 2);
        for t in caps {
            let [a, b, c] = t.map(|i| remap[i as usize]);
            triangles.extend([[a, c, b], [a + n, b + n, c + n]]);
        }
        for a in 0..n {
            let b = (a + 1) % n;
            triangles.extend([[a, b, b + n], [a, b + n, a + n]]);
        }
        let mesh = TriangleMesh {
            vertices,
            triangles,
        };
        mesh.validate()?;
        if mesh.signed_volume()? <= 0.0 {
            return Err(invalid(
                "Extrusion does not enclose positive representable volume",
            ));
        }
        Ok(mesh)
    }
}

impl Document {
    /// Copy a supported profile into an independently editable feature.
    pub fn add_extrusion(
        &mut self,
        name: impl Into<String>,
        profile: Geometry,
        base_z: f64,
        height: f64,
    ) -> io::Result<u64> {
        if self.next_id == u64::MAX {
            return Err(invalid("Object ID space is exhausted"));
        }
        let mut candidate = self.clone();
        let id = candidate.try_allocate_id()?;
        candidate.extrusions.push(Extrusion {
            id,
            name: name.into(),
            profile,
            base_z,
            height,
        });
        candidate.validate()?;
        *self = candidate;
        Ok(id)
    }
    /// Derived model-area output. Native saves retain parameters and identities.
    pub fn model_mesh(&self, options: TessellationOptions) -> io::Result<TriangleMesh> {
        self.validate()?;
        let mut mesh = tessellate_solids(&self.solids, options)?;
        for part in &self.extrusions {
            mesh.append(&part.mesh()?)?;
        }
        for object in &self.meshes {
            mesh.append(&object.mesh)?;
        }
        Ok(mesh)
    }
    pub fn has_model_geometry(&self) -> bool {
        !self.solids.is_empty() || !self.meshes.is_empty() || !self.extrusions.is_empty()
    }
}
pub(crate) fn validate_features(extrusions: &[Extrusion]) -> io::Result<()> {
    if extrusions.len() > 1000 {
        return Err(invalid("Model exceeds 1000 extrusion features"));
    }
    let mut work = 0usize;
    let mut vertices = 0usize;
    for part in extrusions {
        let n = part.complexity()?;
        work = work
            .checked_add(n * n * n)
            .ok_or_else(|| invalid("Extrusion work budget overflow"))?;
        vertices += n;
        if work > 16_777_216 || vertices > 8192 {
            return Err(invalid(
                "Extrusions exceed the bounded triangulation budget (8192 total profile vertices / 16777216 cubic work units)",
            ));
        }
    }
    Ok(())
}
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}
