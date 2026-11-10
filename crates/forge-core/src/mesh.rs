//! Geometry-only triangle mesh exchange and primitive tessellation.
//!
//! STL attributes and OBJ texture/material/normal records are not retained. OBJ
//! polygon positions are triangulated without changing their winding. Freeform
//! curves and surfaces are rejected rather than silently discarded.

use crate::{Solid, SolidKind};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, io, path::Path};

const MAX_ELEMENTS: usize = 10_000_000;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct TriangleMesh {
    pub vertices: Vec<[f64; 3]>,
    pub triangles: Vec<[u32; 3]>,
}

#[derive(Clone, Copy, Debug)]
pub struct TessellationOptions {
    pub radial_segments: usize,
    pub latitude_segments: usize,
}

impl Default for TessellationOptions {
    fn default() -> Self {
        Self {
            radial_segments: 48,
            latitude_segments: 24,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StlEncoding {
    Ascii,
    Binary,
}

impl TriangleMesh {
    /// Checks finite positions, valid indices, and nondegenerate triangles.
    /// Watertightness is not required: open imported surfaces are valid meshes.
    pub fn validate(&self) -> io::Result<()> {
        if self.vertices.len() > MAX_ELEMENTS || self.triangles.len() > MAX_ELEMENTS {
            return Err(invalid("mesh exceeds the supported element limit"));
        }
        for vertex in &self.vertices {
            finite_vector(*vertex)?;
        }
        for triangle in &self.triangles {
            let points = self.triangle_points(*triangle)?;
            normal(points)?;
        }
        Ok(())
    }

    pub fn bounds(&self) -> Option<([f64; 3], [f64; 3])> {
        let first = *self.vertices.first()?;
        let (mut min, mut max) = (first, first);
        for point in &self.vertices {
            for axis in 0..3 {
                min[axis] = min[axis].min(point[axis]);
                max[axis] = max[axis].max(point[axis]);
            }
        }
        Some((min, max))
    }

    /// Positive volume indicates outward winding for a closed surface.
    pub fn signed_volume(&self) -> io::Result<f64> {
        self.validate()?;
        let origin = self.vertices.first().copied().unwrap_or([0.0; 3]);
        let mut volume = 0.0;
        for triangle in &self.triangles {
            let [a, b, c] = self.triangle_points(*triangle)?;
            let (a, b, c) = (sub(a, origin), sub(b, origin), sub(c, origin));
            volume += dot(a, cross(b, c)) / 6.0;
        }
        if !volume.is_finite() {
            return Err(invalid("mesh volume overflow"));
        }
        Ok(volume)
    }

    pub fn append(&mut self, other: &Self) -> io::Result<()> {
        self.validate()?;
        other.validate()?;
        let offset =
            u32::try_from(self.vertices.len()).map_err(|_| invalid("too many vertices"))?;
        if self.vertices.len().saturating_add(other.vertices.len()) > MAX_ELEMENTS
            || self.triangles.len().saturating_add(other.triangles.len()) > MAX_ELEMENTS
        {
            return Err(invalid("mesh exceeds the supported element limit"));
        }
        self.vertices.extend_from_slice(&other.vertices);
        self.triangles.extend(
            other
                .triangles
                .iter()
                .map(|t| [t[0] + offset, t[1] + offset, t[2] + offset]),
        );
        Ok(())
    }

    pub fn from_stl(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() >= 84 {
            let count = u32::from_le_bytes(bytes[80..84].try_into().unwrap()) as usize;
            let expected = count.checked_mul(50).and_then(|n| n.checked_add(84));
            // Length, not the first word of the header, distinguishes binary
            // files. Valid binary headers can begin with the word `solid`.
            if expected == Some(bytes.len()) {
                return parse_binary_stl(bytes, count);
            }
            let start = bytes
                .iter()
                .position(|byte| !byte.is_ascii_whitespace())
                .unwrap_or(bytes.len());
            if !bytes[start..].starts_with(b"solid") {
                return Err(invalid(
                    "binary STL triangle count does not match its length",
                ));
            }
        }
        let text = std::str::from_utf8(bytes)
            .map_err(|_| invalid("STL is neither valid binary nor UTF-8 ASCII"))?;
        parse_ascii_stl(text)
    }

    pub fn to_stl_ascii(&self, name: &str) -> io::Result<String> {
        self.validate()?;
        let name = safe_name(name);
        let mut result = format!("solid {name}\n");
        for triangle in &self.triangles {
            let points = self.triangle_points(*triangle)?;
            let n = normal(points)?;
            result.push_str(&format!(
                "  facet normal {} {} {}\n    outer loop\n",
                n[0], n[1], n[2]
            ));
            for p in points {
                result.push_str(&format!("      vertex {} {} {}\n", p[0], p[1], p[2]));
            }
            result.push_str("    endloop\n  endfacet\n");
        }
        result.push_str(&format!("endsolid {name}\n"));
        Ok(result)
    }

    pub fn to_stl_binary(&self) -> io::Result<Vec<u8>> {
        self.validate()?;
        let count =
            u32::try_from(self.triangles.len()).map_err(|_| invalid("too many STL triangles"))?;
        let mut result = vec![0; 80];
        let label = b"Forge Craft geometry-only binary STL";
        result[..label.len()].copy_from_slice(label);
        result.extend_from_slice(&count.to_le_bytes());
        for triangle in &self.triangles {
            let points = self.triangle_points(*triangle)?;
            let mut quantized = [[0.0; 3]; 3];
            for (point_index, point) in points.into_iter().enumerate() {
                for (axis, value) in point.into_iter().enumerate() {
                    let value = value as f32;
                    if !value.is_finite() {
                        return Err(invalid(
                            "coordinate cannot be represented by binary STL float32",
                        ));
                    }
                    quantized[point_index][axis] = value as f64;
                }
            }
            let n = normal(quantized)
                .map_err(|_| invalid("triangle collapses at binary STL float32 precision"))?;
            for point in [n, quantized[0], quantized[1], quantized[2]] {
                for value in point {
                    let value = value as f32;
                    if !value.is_finite() {
                        return Err(invalid(
                            "coordinate cannot be represented by binary STL float32",
                        ));
                    }
                    result.extend_from_slice(&value.to_le_bytes());
                }
            }
            result.extend_from_slice(&0_u16.to_le_bytes());
        }
        Ok(result)
    }

    pub fn from_obj(text: &str) -> io::Result<Self> {
        parse_obj(text)
    }

    pub fn to_obj(&self, name: &str) -> io::Result<String> {
        self.validate()?;
        let mut result = format!("# Forge Craft geometry-only OBJ\no {}\n", safe_name(name));
        for p in &self.vertices {
            result.push_str(&format!("v {} {} {}\n", p[0], p[1], p[2]));
        }
        for t in &self.triangles {
            result.push_str(&format!("f {} {} {}\n", t[0] + 1, t[1] + 1, t[2] + 1));
        }
        Ok(result)
    }

    pub fn read_stl(path: impl AsRef<Path>) -> io::Result<Self> {
        Self::from_stl(&fs::read(path)?)
    }
    pub fn read_obj(path: impl AsRef<Path>) -> io::Result<Self> {
        Self::from_obj(&fs::read_to_string(path)?)
    }
    pub fn write_stl(
        &self,
        path: impl AsRef<Path>,
        encoding: StlEncoding,
        name: &str,
    ) -> io::Result<()> {
        match encoding {
            StlEncoding::Ascii => fs::write(path, self.to_stl_ascii(name)?),
            StlEncoding::Binary => fs::write(path, self.to_stl_binary()?),
        }
    }
    pub fn write_obj(&self, path: impl AsRef<Path>, name: &str) -> io::Result<()> {
        fs::write(path, self.to_obj(name)?)
    }

    fn triangle_points(&self, triangle: [u32; 3]) -> io::Result<[[f64; 3]; 3]> {
        if triangle[0] == triangle[1] || triangle[1] == triangle[2] || triangle[0] == triangle[2] {
            return Err(invalid("triangle repeats a vertex index"));
        }
        let point = |index: u32| {
            self.vertices
                .get(index as usize)
                .copied()
                .ok_or_else(|| invalid("triangle vertex index is out of range"))
        };
        Ok([
            point(triangle[0])?,
            point(triangle[1])?,
            point(triangle[2])?,
        ])
    }
}

pub fn tessellate_solids(
    solids: &[Solid],
    options: TessellationOptions,
) -> io::Result<TriangleMesh> {
    let mut result = TriangleMesh::default();
    for solid in solids {
        result.append(&tessellate_solid(solid, options)?)?;
    }
    Ok(result)
}

pub fn tessellate_solid(solid: &Solid, options: TessellationOptions) -> io::Result<TriangleMesh> {
    finite_vector(solid.center)?;
    finite_vector(solid.size)?;
    if solid.size.iter().any(|dimension| *dimension <= 0.0) {
        return Err(invalid("solid dimensions must be positive"));
    }
    if !(3..=4096).contains(&options.radial_segments)
        || !(2..=2048).contains(&options.latitude_segments)
    {
        return Err(invalid(
            "tessellation segments are outside supported bounds",
        ));
    }
    if matches!(solid.kind, SolidKind::Sphere) {
        let rings = options.latitude_segments - 1;
        let vertices = rings
            .checked_mul(options.radial_segments)
            .and_then(|count| count.checked_add(2));
        let triangles = rings
            .checked_mul(options.radial_segments)
            .and_then(|count| count.checked_mul(2));
        if vertices.is_none_or(|count| count > MAX_ELEMENTS)
            || triangles.is_none_or(|count| count > MAX_ELEMENTS)
        {
            return Err(invalid("tessellation exceeds the supported element limit"));
        }
    }
    let mesh = match solid.kind {
        SolidKind::Box => box_mesh(solid),
        SolidKind::Cylinder => cylinder_mesh(solid, options.radial_segments),
        SolidKind::Sphere => sphere_mesh(solid, options.radial_segments, options.latitude_segments),
    };
    mesh.validate()?;
    Ok(mesh)
}

fn box_mesh(solid: &Solid) -> TriangleMesh {
    let [x, y, z] = solid.center;
    let [a, b, c] = solid.size.map(|size| size / 2.0);
    let vertices = vec![
        [x - a, y - b, z - c],
        [x + a, y - b, z - c],
        [x + a, y + b, z - c],
        [x - a, y + b, z - c],
        [x - a, y - b, z + c],
        [x + a, y - b, z + c],
        [x + a, y + b, z + c],
        [x - a, y + b, z + c],
    ];
    let faces = [
        [0, 3, 2, 1],
        [4, 5, 6, 7],
        [0, 1, 5, 4],
        [1, 2, 6, 5],
        [2, 3, 7, 6],
        [3, 0, 4, 7],
    ];
    let triangles = faces
        .into_iter()
        .flat_map(|[a, b, c, d]| [[a, b, c], [a, c, d]])
        .collect();
    TriangleMesh {
        vertices,
        triangles,
    }
}

fn cylinder_mesh(solid: &Solid, segments: usize) -> TriangleMesh {
    let [x, y, z] = solid.center;
    let [rx, ry, h] = solid.size.map(|size| size / 2.0);
    let mut mesh = TriangleMesh::default();
    for height in [z - h, z + h] {
        for i in 0..segments {
            let angle = std::f64::consts::TAU * i as f64 / segments as f64;
            mesh.vertices
                .push([x + rx * angle.cos(), y + ry * angle.sin(), height]);
        }
    }
    let bottom = mesh.vertices.len() as u32;
    mesh.vertices.push([x, y, z - h]);
    let top = mesh.vertices.len() as u32;
    mesh.vertices.push([x, y, z + h]);
    let n = segments as u32;
    for i in 0..n {
        let j = (i + 1) % n;
        mesh.triangles.extend([
            [i, j, j + n],
            [i, j + n, i + n],
            [bottom, j, i],
            [top, i + n, j + n],
        ]);
    }
    mesh
}

fn sphere_mesh(solid: &Solid, segments: usize, latitudes: usize) -> TriangleMesh {
    let [x, y, z] = solid.center;
    let [rx, ry, rz] = solid.size.map(|size| size / 2.0);
    let mut mesh = TriangleMesh {
        vertices: vec![[x, y, z + rz]],
        triangles: Vec::new(),
    };
    for ring in 1..latitudes {
        let phi = std::f64::consts::PI * ring as f64 / latitudes as f64;
        for i in 0..segments {
            let theta = std::f64::consts::TAU * i as f64 / segments as f64;
            mesh.vertices.push([
                x + rx * phi.sin() * theta.cos(),
                y + ry * phi.sin() * theta.sin(),
                z + rz * phi.cos(),
            ]);
        }
    }
    let south = mesh.vertices.len() as u32;
    mesh.vertices.push([x, y, z - rz]);
    let n = segments as u32;
    for i in 0..n {
        let j = (i + 1) % n;
        mesh.triangles.push([0, 1 + i, 1 + j]);
        for ring in 0..latitudes - 2 {
            let a = 1 + ring as u32 * n + i;
            let b = 1 + ring as u32 * n + j;
            let c = a + n;
            let d = b + n;
            mesh.triangles.extend([[a, c, d], [a, d, b]]);
        }
        let last = 1 + (latitudes as u32 - 2) * n;
        mesh.triangles.push([last + i, south, last + j]);
    }
    mesh
}

fn parse_binary_stl(bytes: &[u8], count: usize) -> io::Result<TriangleMesh> {
    if count > MAX_ELEMENTS {
        return Err(invalid("STL exceeds the supported triangle limit"));
    }
    let mut builder = MeshBuilder::default();
    for triangle in bytes[84..].chunks_exact(50) {
        let mut values = [0.0; 12];
        for (i, chunk) in triangle[..48].chunks_exact(4).enumerate() {
            values[i] = f32::from_le_bytes(chunk.try_into().unwrap()) as f64;
        }
        if values.iter().any(|value| !value.is_finite()) {
            return Err(invalid("STL contains non-finite coordinates or normals"));
        }
        builder.push([
            [values[3], values[4], values[5]],
            [values[6], values[7], values[8]],
            [values[9], values[10], values[11]],
        ])?;
    }
    builder.finish()
}

fn parse_ascii_stl(text: &str) -> io::Result<TriangleMesh> {
    let lines: Vec<(usize, &str)> = text
        .lines()
        .enumerate()
        .map(|(i, line)| (i + 1, line.trim()))
        .filter(|(_, line)| !line.is_empty())
        .collect();
    let mut cursor = 0;
    let mut builder = MeshBuilder::default();
    if lines.is_empty() {
        return Err(invalid("empty STL"));
    }
    while cursor < lines.len() {
        let (line, head) = lines[cursor];
        if head.split_whitespace().next() != Some("solid") {
            return Err(at(line, "expected solid"));
        }
        cursor += 1;
        loop {
            let (line, facet) = *lines
                .get(cursor)
                .ok_or_else(|| invalid("missing endsolid"))?;
            if facet.split_whitespace().next() == Some("endsolid") {
                cursor += 1;
                break;
            }
            let tokens: Vec<_> = facet.split_whitespace().collect();
            if tokens.len() != 5 || tokens[..2] != ["facet", "normal"] {
                return Err(at(line, "expected facet normal"));
            }
            parse_vector(&tokens[2..], line)?;
            cursor += 1;
            expect_line(&lines, &mut cursor, "outer loop")?;
            let mut points = [[0.0; 3]; 3];
            for point in &mut points {
                let (line, vertex) = *lines
                    .get(cursor)
                    .ok_or_else(|| invalid("truncated STL facet"))?;
                let tokens: Vec<_> = vertex.split_whitespace().collect();
                if tokens.len() != 4 || tokens[0] != "vertex" {
                    return Err(at(line, "expected vertex x y z"));
                }
                *point = parse_vector(&tokens[1..], line)?;
                cursor += 1;
            }
            expect_line(&lines, &mut cursor, "endloop")?;
            expect_line(&lines, &mut cursor, "endfacet")?;
            builder.push(points)?;
        }
    }
    builder.finish()
}

fn expect_line(lines: &[(usize, &str)], cursor: &mut usize, expected: &str) -> io::Result<()> {
    let (line, text) = *lines
        .get(*cursor)
        .ok_or_else(|| invalid(format!("expected {expected} before end of STL")))?;
    if text.split_whitespace().collect::<Vec<_>>()
        != expected.split_whitespace().collect::<Vec<_>>()
    {
        return Err(at(line, format!("expected {expected}")));
    }
    *cursor += 1;
    Ok(())
}

#[derive(Default)]
struct MeshBuilder {
    mesh: TriangleMesh,
    indices: HashMap<[u64; 3], u32>,
}
impl MeshBuilder {
    fn push(&mut self, points: [[f64; 3]; 3]) -> io::Result<()> {
        normal(points)?;
        if self.mesh.triangles.len() >= MAX_ELEMENTS {
            return Err(invalid("mesh exceeds the supported triangle limit"));
        }
        let mut triangle = [0; 3];
        for (i, point) in points.into_iter().enumerate() {
            finite_vector(point)?;
            let key = point.map(|value| if value == 0.0 { 0 } else { value.to_bits() });
            triangle[i] = if let Some(index) = self.indices.get(&key) {
                *index
            } else {
                if self.mesh.vertices.len() >= MAX_ELEMENTS {
                    return Err(invalid("mesh exceeds the supported vertex limit"));
                }
                let index = self.mesh.vertices.len() as u32;
                self.mesh.vertices.push(point);
                self.indices.insert(key, index);
                index
            };
        }
        self.mesh.triangles.push(triangle);
        Ok(())
    }
    fn finish(self) -> io::Result<TriangleMesh> {
        self.mesh.validate()?;
        Ok(self.mesh)
    }
}

fn parse_obj(text: &str) -> io::Result<TriangleMesh> {
    let mut mesh = TriangleMesh::default();
    let (mut textures, mut normals) = (0_usize, 0_usize);
    let mut logical = String::new();
    let mut first_line = 1;
    for (line_index, line) in text.lines().enumerate() {
        if logical.is_empty() {
            first_line = line_index + 1;
        }
        let line = line.split('#').next().unwrap_or("").trim_end();
        let continued = line.ends_with('\\');
        logical.push_str(if continued {
            line[..line.len() - 1].trim_end()
        } else {
            line
        });
        if continued {
            logical.push(' ');
            continue;
        }
        parse_obj_line(&logical, first_line, &mut mesh, &mut textures, &mut normals)?;
        logical.clear();
    }
    if !logical.is_empty() {
        return Err(at(first_line, "unterminated OBJ line continuation"));
    }
    mesh.validate()?;
    Ok(mesh)
}

fn parse_obj_line(
    line: &str,
    line_number: usize,
    mesh: &mut TriangleMesh,
    textures: &mut usize,
    normals: &mut usize,
) -> io::Result<()> {
    let mut tokens = line.split_whitespace();
    let Some(keyword) = tokens.next() else {
        return Ok(());
    };
    let values: Vec<_> = tokens.collect();
    match keyword {
        "v" => {
            if values.len() != 3 && values.len() != 4 {
                return Err(at(
                    line_number,
                    "vertex requires x y z and optional homogeneous w; color extensions are unsupported",
                ));
            }
            let mut point = parse_vector(&values[..3], line_number)?;
            if values.len() == 4 {
                let w = parse_number(values[3], line_number)?;
                if w == 0.0 {
                    return Err(at(line_number, "homogeneous vertex w cannot be zero"));
                }
                point = point.map(|value| value / w);
                finite_vector(point)?;
            }
            if mesh.vertices.len() >= MAX_ELEMENTS {
                return Err(invalid("OBJ exceeds the supported vertex limit"));
            }
            mesh.vertices.push(point);
        }
        "vt" => {
            if values.is_empty() || values.len() > 3 {
                return Err(at(
                    line_number,
                    "texture coordinate requires one to three values",
                ));
            }
            for value in values {
                parse_number(value, line_number)?;
            }
            *textures += 1;
        }
        "vn" => {
            parse_vector(&values, line_number)?;
            *normals += 1;
        }
        "f" => {
            if values.len() < 3 {
                return Err(at(line_number, "face requires at least three vertices"));
            }
            let mut polygon = Vec::with_capacity(values.len());
            for value in values {
                let parts: Vec<_> = value.split('/').collect();
                if parts.len() > 3 || parts[0].is_empty() {
                    return Err(at(line_number, "invalid face index"));
                }
                let vertex = obj_index(parts[0], mesh.vertices.len(), line_number)?;
                if parts.len() >= 2 && !parts[1].is_empty() {
                    obj_index(parts[1], *textures, line_number)?;
                }
                if parts.len() == 3 && !parts[2].is_empty() {
                    obj_index(parts[2], *normals, line_number)?;
                }
                if parts.len() == 2 && parts[1].is_empty()
                    || parts.len() == 3 && parts[2].is_empty()
                {
                    return Err(at(line_number, "incomplete face index"));
                }
                polygon.push(vertex as u32);
            }
            let triangles = triangulate_polygon(&mesh.vertices, &polygon)
                .map_err(|error| at(line_number, error))?;
            if mesh.triangles.len().saturating_add(triangles.len()) > MAX_ELEMENTS {
                return Err(invalid("OBJ exceeds the supported triangle limit"));
            }
            mesh.triangles.extend(triangles);
        }
        // These records affect display/grouping, not position topology. The
        // geometry-only API explicitly does not preserve their attributes.
        "o" | "g" | "s" | "mtllib" | "usemtl" | "bevel" | "c_interp" | "d_interp" | "lod"
        | "shadow_obj" | "trace_obj" | "ctech" | "stech" => {}
        _ => return Err(at(line_number, format!("unsupported OBJ record {keyword}"))),
    }
    Ok(())
}

fn obj_index(value: &str, count: usize, line: usize) -> io::Result<usize> {
    let index = value
        .parse::<i64>()
        .map_err(|_| at(line, "invalid OBJ index"))?;
    let resolved = if index > 0 {
        index - 1
    } else if index < 0 {
        (count as i64)
            .checked_add(index)
            .ok_or_else(|| at(line, "OBJ index overflow"))?
    } else {
        return Err(at(line, "OBJ indices cannot be zero"));
    };
    if resolved < 0 || resolved as usize >= count {
        return Err(at(line, "OBJ index references an undeclared value"));
    }
    Ok(resolved as usize)
}

pub(crate) fn triangulate_polygon(
    vertices: &[[f64; 3]],
    indices: &[u32],
) -> Result<Vec<[u32; 3]>, String> {
    let mut polygon = indices.to_vec();
    if polygon.first() == polygon.last() {
        polygon.pop();
    }
    if polygon.len() < 3 {
        return Err("face has fewer than three distinct vertices".into());
    }
    let mut seen = std::collections::HashSet::new();
    if polygon.iter().any(|i| !seen.insert(*i)) {
        return Err("face repeats a non-closing vertex".into());
    }
    if polygon.len() == 3 {
        normal([
            vertices[polygon[0] as usize],
            vertices[polygon[1] as usize],
            vertices[polygon[2] as usize],
        ])
        .map_err(|e| e.to_string())?;
        return Ok(vec![[polygon[0], polygon[1], polygon[2]]]);
    }
    let origin = vertices[polygon[0] as usize];
    let mut n = [0.0; 3];
    for i in 1..polygon.len() - 1 {
        let contribution = cross(
            sub(vertices[polygon[i] as usize], origin),
            sub(vertices[polygon[i + 1] as usize], origin),
        );
        for axis in 0..3 {
            n[axis] += contribution[axis];
        }
    }
    let length = n[0].hypot(n[1]).hypot(n[2]);
    if !length.is_finite() || length == 0.0 {
        return Err("face is degenerate or self-intersecting".into());
    }
    n = n.map(|value| value / length);
    let mut extent = 0.0_f64;
    for index in &polygon {
        let p = sub(vertices[*index as usize], origin);
        extent = extent.max(p[0].hypot(p[1]).hypot(p[2]));
    }
    for index in &polygon {
        if dot(sub(vertices[*index as usize], origin), n).abs() > extent * 1e-7 {
            return Err(
                "non-planar OBJ polygon cannot be triangulated without changing its surface".into(),
            );
        }
    }
    let drop_axis = (0..3)
        .max_by(|a, b| n[*a].abs().total_cmp(&n[*b].abs()))
        .unwrap();
    let axes: Vec<usize> = (0..3).filter(|axis| *axis != drop_axis).collect();
    let points: HashMap<u32, [f64; 2]> = polygon
        .iter()
        .map(|index| {
            let p = sub(vertices[*index as usize], origin);
            (*index, [p[axes[0]] / extent, p[axes[1]] / extent])
        })
        .collect();
    const EPS: f64 = 1e-12;
    for i in 0..polygon.len() {
        let j = (i + 1) % polygon.len();
        for k in i + 1..polygon.len() {
            let l = (k + 1) % polygon.len();
            if i == k || i == l || j == k || j == l {
                continue;
            }
            if segments_intersect(
                points[&polygon[i]],
                points[&polygon[j]],
                points[&polygon[k]],
                points[&polygon[l]],
                EPS,
            ) {
                return Err("self-intersecting OBJ polygon".into());
            }
        }
    }
    // Removing an intermediate collinear position preserves the boundary and
    // prevents zero-area ears in CAD exports with redundant contour vertices.
    for i in 0..polygon.len() {
        let a = points[&polygon[(i + polygon.len() - 1) % polygon.len()]];
        let b = points[&polygon[i]];
        let c = points[&polygon[(i + 1) % polygon.len()]];
        if orient(a, b, c).abs() <= EPS
            && (b[0] - a[0]) * (c[0] - b[0]) + (b[1] - a[1]) * (c[1] - b[1]) < -EPS
        {
            return Err("OBJ polygon has a backtracking boundary".into());
        }
    }
    loop {
        let removable = (0..polygon.len()).find(|i| {
            let previous = polygon[(*i + polygon.len() - 1) % polygon.len()];
            let current = polygon[*i];
            let next = polygon[(*i + 1) % polygon.len()];
            orient(points[&previous], points[&current], points[&next]).abs() <= EPS
        });
        if polygon.len() <= 3 || removable.is_none() {
            break;
        }
        polygon.remove(removable.unwrap());
    }
    let area = (0..polygon.len())
        .map(|i| {
            let a = points[&polygon[i]];
            let b = points[&polygon[(i + 1) % polygon.len()]];
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>();
    if area.abs() <= EPS {
        return Err("zero-area OBJ polygon".into());
    }
    let sign = area.signum();
    let mut triangles = Vec::new();
    while polygon.len() > 3 {
        let ear = (0..polygon.len())
            .find(|i| {
                let a = polygon[(*i + polygon.len() - 1) % polygon.len()];
                let b = polygon[*i];
                let c = polygon[(*i + 1) % polygon.len()];
                if orient(points[&a], points[&b], points[&c]) * sign <= EPS {
                    return false;
                }
                !polygon
                    .iter()
                    .filter(|p| **p != a && **p != b && **p != c)
                    .any(|p| {
                        point_in_triangle(points[p], points[&a], points[&b], points[&c], sign, EPS)
                    })
            })
            .ok_or_else(|| "OBJ polygon cannot be triangulated".to_owned())?;
        triangles.push([
            polygon[(ear + polygon.len() - 1) % polygon.len()],
            polygon[ear],
            polygon[(ear + 1) % polygon.len()],
        ]);
        polygon.remove(ear);
    }
    triangles.push([polygon[0], polygon[1], polygon[2]]);
    Ok(triangles)
}

fn orient(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}
fn point_in_triangle(
    p: [f64; 2],
    a: [f64; 2],
    b: [f64; 2],
    c: [f64; 2],
    sign: f64,
    eps: f64,
) -> bool {
    orient(a, b, p) * sign >= -eps
        && orient(b, c, p) * sign >= -eps
        && orient(c, a, p) * sign >= -eps
}
fn segments_intersect(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2], eps: f64) -> bool {
    let (o1, o2, o3, o4) = (
        orient(a, b, c),
        orient(a, b, d),
        orient(c, d, a),
        orient(c, d, b),
    );
    let between = |p: [f64; 2], x: [f64; 2], y: [f64; 2]| {
        (0..2).all(|i| p[i] >= x[i].min(y[i]) - eps && p[i] <= x[i].max(y[i]) + eps)
    };
    ((o1 > eps && o2 < -eps) || (o1 < -eps && o2 > eps))
        && ((o3 > eps && o4 < -eps) || (o3 < -eps && o4 > eps))
        || o1.abs() <= eps && between(c, a, b)
        || o2.abs() <= eps && between(d, a, b)
        || o3.abs() <= eps && between(a, c, d)
        || o4.abs() <= eps && between(b, c, d)
}

fn parse_number(value: &str, line: usize) -> io::Result<f64> {
    let number = value
        .parse::<f64>()
        .map_err(|_| at(line, format!("invalid number {value}")))?;
    if !number.is_finite() {
        return Err(at(line, "non-finite number"));
    }
    Ok(number)
}
fn parse_vector(values: &[&str], line: usize) -> io::Result<[f64; 3]> {
    if values.len() != 3 {
        return Err(at(line, "expected three coordinates"));
    }
    Ok([
        parse_number(values[0], line)?,
        parse_number(values[1], line)?,
        parse_number(values[2], line)?,
    ])
}
fn finite_vector(point: [f64; 3]) -> io::Result<()> {
    if point.iter().any(|v| !v.is_finite()) {
        return Err(invalid("mesh contains a non-finite coordinate"));
    }
    Ok(())
}
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn normal(points: [[f64; 3]; 3]) -> io::Result<[f64; 3]> {
    for p in points {
        finite_vector(p)?;
    }
    let n = cross(sub(points[1], points[0]), sub(points[2], points[0]));
    let length = n[0].hypot(n[1]).hypot(n[2]);
    if !length.is_finite() || length == 0.0 {
        return Err(invalid(
            "degenerate or numerically unrepresentable triangle",
        ));
    }
    Ok(n.map(|v| v / length))
}
fn safe_name(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}
fn invalid(message: impl ToString) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_string())
}
fn at(line: usize, message: impl ToString) -> io::Error {
    invalid(format!("line {line}: {}", message.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn independently_authored_stl_fixtures_are_read() {
        let tetrahedron =
            TriangleMesh::from_stl(include_bytes!("../tests/mesh/tetrahedron-ascii.stl")).unwrap();
        assert_eq!(tetrahedron.vertices.len(), 4);
        assert_eq!(tetrahedron.triangles.len(), 4);
        assert!(closed_edges(&tetrahedron));
        assert!((tetrahedron.signed_volume().unwrap() - 1.0 / 6.0).abs() < 1e-12);
        let triangle =
            TriangleMesh::from_stl(include_bytes!("../tests/mesh/triangle-binary.stl")).unwrap();
        assert_eq!(
            triangle.vertices,
            vec![[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 3.0, 0.0]]
        );
        assert_eq!(triangle.triangles, vec![[0, 1, 2]]);
    }

    #[test]
    fn leading_whitespace_in_ascii_stl_is_accepted() {
        let source = format!(
            "  \n\t{}",
            include_str!("../tests/mesh/tetrahedron-ascii.stl")
        );
        assert_eq!(
            TriangleMesh::from_stl(source.as_bytes())
                .unwrap()
                .triangles
                .len(),
            4
        );
    }

    #[test]
    fn binary_stl_rejects_float32_triangle_collapse() {
        let mesh = TriangleMesh {
            vertices: vec![[1e10, 0.0, 0.0], [1e10 + 1.0, 0.0, 0.0], [1e10, 1.0, 0.0]],
            triangles: vec![[0, 1, 2]],
        };
        mesh.validate().unwrap();
        assert!(
            mesh.to_stl_binary()
                .unwrap_err()
                .to_string()
                .contains("collapses")
        );
    }

    #[test]
    fn oversized_tessellation_is_rejected_before_allocation() {
        assert!(
            tessellate_solid(
                &solid(SolidKind::Sphere),
                TessellationOptions {
                    radial_segments: 4096,
                    latitude_segments: 2048
                }
            )
            .is_err()
        );
    }

    #[test]
    fn clockwise_and_collinear_obj_boundaries_preserve_area() {
        let source = "v 0 0 0\nv 0 2 0\nv 2 2 0\nv 2 1 0\nv 2 0 0\nf 1 2 3 4 5\n";
        let mesh = TriangleMesh::from_obj(source).unwrap();
        let signed_area = mesh
            .triangles
            .iter()
            .map(|t| {
                let [a, b, c] = mesh.triangle_points(*t).unwrap();
                cross(sub(b, a), sub(c, a))[2] / 2.0
            })
            .sum::<f64>();
        assert!((signed_area + 4.0).abs() < 1e-12);
        assert_eq!(mesh.triangles.len(), 2);
    }

    fn solid(kind: SolidKind) -> Solid {
        Solid {
            id: 1,
            name: "test".into(),
            kind,
            center: [5.0, -2.0, 3.0],
            size: [20.0, 12.0, 8.0],
        }
    }
    fn closed_edges(mesh: &TriangleMesh) -> bool {
        let mut edges: HashMap<(u32, u32), (usize, i32)> = HashMap::new();
        for [a, b, c] in &mesh.triangles {
            for (x, y) in [(*a, *b), (*b, *c), (*c, *a)] {
                let key = (x.min(y), x.max(y));
                let e = edges.entry(key).or_default();
                e.0 += 1;
                e.1 += if x < y { 1 } else { -1 };
            }
        }
        edges
            .values()
            .all(|(count, balance)| *count == 2 && *balance == 0)
    }
    #[test]
    fn primitives_are_closed_and_have_outward_winding() {
        for kind in [SolidKind::Box, SolidKind::Cylinder, SolidKind::Sphere] {
            let body = solid(kind);
            let mesh = tessellate_solid(&body, TessellationOptions::default()).unwrap();
            assert!(closed_edges(&mesh), "{kind:?}");
            assert!(mesh.signed_volume().unwrap() > 0.0, "{kind:?}");
            let expected = match kind {
                SolidKind::Box => 20.0 * 12.0 * 8.0,
                SolidKind::Cylinder => std::f64::consts::PI * 10.0 * 6.0 * 8.0,
                SolidKind::Sphere => 4.0 / 3.0 * std::f64::consts::PI * 10.0 * 6.0 * 4.0,
            };
            assert!(
                (mesh.signed_volume().unwrap() / expected - 1.0).abs() < 0.02,
                "{kind:?}"
            );
        }
    }
    #[test]
    fn box_bounds_dimensions_and_exact_volume() {
        let mesh =
            tessellate_solid(&solid(SolidKind::Box), TessellationOptions::default()).unwrap();
        assert_eq!(mesh.bounds(), Some(([-5.0, -8.0, -1.0], [15.0, 4.0, 7.0])));
        assert!((mesh.signed_volume().unwrap() - 1920.0).abs() < 1e-10);
    }
    #[test]
    fn ascii_and_binary_stl_round_trip_all_primitives() {
        for kind in [SolidKind::Box, SolidKind::Cylinder, SolidKind::Sphere] {
            let original = tessellate_solid(
                &solid(kind),
                TessellationOptions {
                    radial_segments: 12,
                    latitude_segments: 6,
                },
            )
            .unwrap();
            for bytes in [
                original.to_stl_ascii("shape").unwrap().into_bytes(),
                original.to_stl_binary().unwrap(),
            ] {
                let read = TriangleMesh::from_stl(&bytes).unwrap();
                assert_eq!(read.triangles.len(), original.triangles.len());
                assert!(
                    (read.signed_volume().unwrap() / original.signed_volume().unwrap() - 1.0).abs()
                        < 1e-6
                );
                assert!(closed_edges(&read));
            }
        }
    }
    #[test]
    fn binary_solid_header_is_not_misidentified() {
        let mesh =
            tessellate_solid(&solid(SolidKind::Box), TessellationOptions::default()).unwrap();
        let mut bytes = mesh.to_stl_binary().unwrap();
        bytes[..5].copy_from_slice(b"solid");
        assert_eq!(TriangleMesh::from_stl(&bytes).unwrap().triangles.len(), 12);
    }
    #[test]
    fn truncated_stl_and_nonfinite_coordinates_fail() {
        let mesh =
            tessellate_solid(&solid(SolidKind::Box), TessellationOptions::default()).unwrap();
        let mut bytes = mesh.to_stl_binary().unwrap();
        bytes.pop();
        assert!(TriangleMesh::from_stl(&bytes).is_err());
        let mut bytes = mesh.to_stl_binary().unwrap();
        bytes[96..100].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(TriangleMesh::from_stl(&bytes).is_err());
        assert!(
            TriangleMesh::from_stl(
                b"solid x\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nendsolid"
            )
            .is_err()
        );
    }
    #[test]
    fn obj_quad_negative_indices_and_attributes() {
        let obj =
            "v 0 0 0\nv 2 0 0\nv 2 2 0\nv 0 2 0\nvt 0 0\nvn 0 0 1\nf -4/1/1 -3/1/1 -2/1/1 -1/1/1\n";
        let mesh = TriangleMesh::from_obj(obj).unwrap();
        assert_eq!(mesh.triangles.len(), 2);
        assert_eq!(
            TriangleMesh::from_obj(&mesh.to_obj("quad").unwrap()).unwrap(),
            mesh
        );
    }
    #[test]
    fn concave_polygon_area_is_preserved() {
        let mesh=TriangleMesh::from_obj("v 0 0 0\nv 3 0 0\nv 3 3 0\nv 2 3 0\nv 2 1 0\nv 1 1 0\nv 1 3 0\nv 0 3 0\nf 1 2 3 4 5 6 7 8\n").unwrap();
        let area = mesh
            .triangles
            .iter()
            .map(|t| {
                let [a, b, c] = mesh.triangle_points(*t).unwrap();
                cross(sub(b, a), sub(c, a))[2] / 2.0
            })
            .sum::<f64>();
        assert!((area - 7.0).abs() < 1e-12);
        assert_eq!(mesh.triangles.len(), 6);
    }
    #[test]
    fn invalid_obj_indices_and_polygons_fail() {
        for face in ["f 0 2 3", "f 1 2 4", "f -4 -2 -1", "f 1//1 2//1 3//1"] {
            assert!(
                TriangleMesh::from_obj(&format!("v 0 0 0\nv 1 0 0\nv 0 1 0\n{face}\n")).is_err()
            );
        }
        assert!(TriangleMesh::from_obj("v 0 0 0\nv 2 2 0\nv 0 2 0\nv 2 0 0\nf 1 2 3 4").is_err());
        assert!(TriangleMesh::from_obj("v 0 0 0\nv 2 0 0\nv 2 2 0.1\nv 0 2 0\nf 1 2 3 4").is_err());
    }
    #[test]
    fn obj_homogeneous_vertices_and_continuations() {
        let mesh =
            TriangleMesh::from_obj("v 0 0 0 2\nv 2 0 0 2\nv 0 2 0 2\nf 1 2 \\\n3 # comment\n")
                .unwrap();
        assert_eq!(mesh.vertices[1], [1.0, 0.0, 0.0]);
        assert_eq!(mesh.triangles, vec![[0, 1, 2]]);
    }
    #[test]
    fn invalid_solids_and_options_fail() {
        let mut body = solid(SolidKind::Box);
        body.size[0] = -1.0;
        assert!(tessellate_solid(&body, TessellationOptions::default()).is_err());
        body.size[0] = f64::INFINITY;
        assert!(tessellate_solid(&body, TessellationOptions::default()).is_err());
        assert!(
            tessellate_solid(
                &solid(SolidKind::Sphere),
                TessellationOptions {
                    radial_segments: 2,
                    latitude_segments: 2
                }
            )
            .is_err()
        );
    }
    #[test]
    fn binary_stl_rejects_float32_overflow() {
        let mesh = TriangleMesh {
            vertices: vec![[1e100, 0.0, 0.0], [1e100, 1.0, 0.0], [1e100, 0.0, 1.0]],
            triangles: vec![[0, 1, 2]],
        };
        assert!(mesh.to_stl_binary().is_err());
    }
    #[test]
    fn mesh_append_rebases_indices() {
        let original =
            tessellate_solid(&solid(SolidKind::Box), TessellationOptions::default()).unwrap();
        let mut both = original.clone();
        both.append(&original).unwrap();
        assert_eq!(both.vertices.len(), 16);
        assert_eq!(both.triangles.len(), 24);
        assert!(both.triangles[12].iter().all(|index| *index >= 8));
    }
}
