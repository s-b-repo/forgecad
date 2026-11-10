//! Analytic boundary representation modeling and checked STEP exchange.
//!
//! Primitives retain parametric curves and surfaces, not tessellated triangles.
//! The checked reader currently accepts one closed manifold solid per STEP
//! exchange file. It audits the upstream reader's topology counts because that
//! reader can otherwise skip individual failed edge and face conversions.
//! Original STEP source is retained verbatim for unchanged document exchange.

use std::{
    collections::BTreeSet,
    io,
    panic::{AssertUnwindSafe, catch_unwind},
};
use truck_modeling::{
    BoundedCurve, EuclideanSpace, MetricSpace, ParametricCurve, Point3, Rad, Solid, Vector3,
    builder,
};
use truck_stepio::{
    r#in::{
        self as step_in, alias,
        ruststep::{
            self,
            ast::{EntityInstance, Name, Parameter},
            tables::EntityTable,
        },
    },
    out::{CompleteStepDisplay, StepHeaderDescriptor, StepModel},
};
use truck_topology::compress::{CompressedShell, CompressedSolid};

pub use truck_modeling as kernel;

pub type StepSolid = truck_topology::Solid<Point3, alias::Curve3D, alias::Surface>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TopologyStats {
    pub shells: usize,
    pub faces: usize,
    pub edges: usize,
    pub vertices: usize,
    pub boundary_loops: usize,
    pub curved_edges: usize,
    pub nonplanar_faces: usize,
}

/// A validated analytic B-rep and the source from which it was extracted.
#[derive(Clone, Debug)]
pub struct BrepModel {
    source: String,
    solid: StepSolid,
    stats: TopologyStats,
}
impl BrepModel {
    /// Parse one closed solid without silently discarding failed topology.
    /// Assemblies, open shells, multiple bodies, and external references are
    /// rejected by this entry point; their metadata is not flattened.
    pub fn from_step(source: &str) -> io::Result<Self> {
        checked_kernel(|| parse_step(source))
    }

    /// Original Part 21 bytes, retained so unchanged exchange loses no records.
    pub fn step_source(&self) -> &str {
        &self.source
    }
    pub fn solid(&self) -> &StepSolid {
        &self.solid
    }
    pub fn stats(&self) -> TopologyStats {
        self.stats
    }

    /// Author a new geometry-only STEP exchange from the parsed B-rep.
    /// The caller must explicitly opt into regenerating metadata: use
    /// `step_source()` for an unchanged, record-preserving exchange.
    pub fn regenerated_geometry_step(&self) -> io::Result<String> {
        checked_kernel(|| {
            let compressed = self.solid.compress();
            let text = format_step(&compressed, "Forge Craft B-rep");
            let round_trip = Self::from_step(&text)?;
            if round_trip.stats != self.stats {
                return Err(invalid("STEP re-export changed the topology counts"));
            }
            Ok(text)
        })
    }
}

/// Construct an exact planar box; all dimensions are in millimetres.
pub fn make_box(center: [f64; 3], size: [f64; 3]) -> io::Result<BrepModel> {
    validate_dimensions(center, size)?;
    checked_kernel(|| {
        let vertex = builder::vertex(Point3::new(
            center[0] - size[0] / 2.0,
            center[1] - size[1] / 2.0,
            center[2] - size[2] / 2.0,
        ));
        let edge = builder::tsweep(&vertex, Vector3::new(size[0], 0.0, 0.0));
        let face = builder::tsweep(&edge, Vector3::new(0.0, size[1], 0.0));
        let solid: Solid = builder::tsweep(&face, Vector3::new(0.0, 0.0, size[2]));
        authored(solid, "Forge Craft box")
    })
}

/// Construct an exact circular cylinder on the Z axis, in millimetres.
pub fn make_cylinder(center: [f64; 3], radius: f64, height: f64) -> io::Result<BrepModel> {
    make_elliptic_cylinder(center, [2.0 * radius, 2.0 * radius, height])
}

/// Exact rational ellipse boundary and extrusion; size gives X/Y diameters and Z height.
pub fn make_elliptic_cylinder(center: [f64; 3], size: [f64; 3]) -> io::Result<BrepModel> {
    validate_dimensions(center, size)?;
    checked_kernel(|| {
        let origin = Point3::new(0.0, 0.0, -size[2] / 2.0);
        let vertex = builder::vertex(Point3::new(size[0] / 2.0, 0.0, -size[2] / 2.0));
        let circle = builder::rsweep(
            &vertex,
            origin,
            Vector3::unit_z(),
            Rad(std::f64::consts::TAU),
        );
        let circle = builder::scaled(
            &circle,
            Point3::origin(),
            Vector3::new(1.0, size[1] / size[0], 1.0),
        );
        let disk = builder::try_attach_plane(&[circle])
            .map_err(|error| invalid(format!("cannot create disk: {error}")))?;
        let solid: Solid = builder::tsweep(&disk, Vector3::new(0.0, 0.0, size[2]));
        let solid = builder::translated(&solid, Vector3::new(center[0], center[1], center[2]));
        authored(solid, "Forge Craft elliptic cylinder")
    })
}

fn authored(solid: Solid, name: &str) -> io::Result<BrepModel> {
    // try_new checks connectedness, closed edge incidence, and manifold vertices.
    Solid::try_new(solid.boundaries().clone())
        .map_err(|error| invalid(format!("kernel rejected primitive topology: {error}")))?;
    if !solid.is_geometric_consistent() {
        return Err(invalid("primitive edge/surface geometry is inconsistent"));
    }
    let compressed = solid.compress();
    BrepModel::from_step(&format_step(&compressed, name))
}

fn format_step<C, S>(solid: &CompressedSolid<Point3, C, S>, name: &str) -> String
where
    C: truck_stepio::out::DisplayByStep
        + truck_stepio::out::StepLength
        + truck_stepio::out::StepCurve,
    S: truck_stepio::out::DisplayByStep
        + truck_stepio::out::StepLength
        + truck_stepio::out::StepSurface,
{
    CompleteStepDisplay::new(
        StepModel::from(solid),
        StepHeaderDescriptor {
            file_name: name.to_owned(),
            organization_system: "Forge Craft / Truck analytic B-rep".into(),
            ..Default::default()
        },
    )
    .to_string()
    .replace(
        "FILE_SCHEMA(('ISO-10303-042'));",
        "FILE_SCHEMA(('AUTOMOTIVE_DESIGN'));",
    )
}

fn parse_step(source: &str) -> io::Result<BrepModel> {
    if source.len() > 64 * 1024 * 1024 {
        return Err(unsupported(
            "STEP exchange exceeds the checked-reader size limit",
        ));
    }
    let exchange = ruststep::parser::parse(source)
        .map_err(|error| invalid(format!("invalid STEP Part 21 source: {error}")))?;
    if exchange.data.len() != 1 || !exchange.anchor.is_empty() || !exchange.reference.is_empty() {
        return Err(unsupported(
            "checked STEP import requires one data section without external references",
        ));
    }
    let entities = &exchange.data[0].entities;
    let mut ids = BTreeSet::new();
    for entity in entities {
        let id = match entity {
            EntityInstance::Simple { id, .. } | EntityInstance::Complex { id, .. } => *id,
        };
        if id == 0 || !ids.insert(id) {
            return Err(invalid("STEP entity IDs must be unique and nonzero"));
        }
    }
    let mut table = step_in::Table::default();
    let mut solid_shells = Vec::new();
    let mut open_shell = false;
    for entity in entities {
        let records = match entity {
            EntityInstance::Simple { record, .. } => std::slice::from_ref(record),
            EntityInstance::Complex { subsuper, .. } => subsuper.0.as_slice(),
        };
        for record in records {
            audit_parameter(&record.parameter, &ids, 0)?;
            match record.name.as_str() {
                "MANIFOLD_SOLID_BREP" => {
                    if let Parameter::List(values) = &record.parameter {
                        if let Some(Parameter::Ref(Name::Entity(shell_id))) = values.get(1) {
                            solid_shells.push(*shell_id);
                        } else {
                            return Err(invalid("solid shell reference is missing"));
                        }
                    } else {
                        return Err(invalid("invalid solid parameters"));
                    }
                }
                "OPEN_SHELL" => open_shell = true,
                "BREP_WITH_VOIDS"
                | "NEXT_ASSEMBLY_USAGE_OCCURRENCE"
                | "CONTEXT_DEPENDENT_SHAPE_REPRESENTATION"
                | "REPRESENTATION_RELATIONSHIP_WITH_TRANSFORMATION"
                | "MAPPED_ITEM"
                | "SHELL_BASED_SURFACE_MODEL"
                | "FACETED_BREP" => {
                    return Err(unsupported(format!(
                        "checked STEP import does not yet support {}",
                        record.name
                    )));
                }
                _ => {}
            }
        }
        // Avoid Table::from_data_section: upstream prints errors and continues.
        table.push_instance(entity).map_err(|error| {
            invalid(format!(
                "STEP entity cannot be decoded without loss: {error}"
            ))
        })?;
    }
    if open_shell
        || solid_shells.len() != 1
        || table.shell.len() != 1
        || !table.shell.contains_key(&solid_shells[0])
    {
        return Err(unsupported(
            "checked STEP import requires exactly one closed manifold solid with one shell",
        ));
    }
    // Resolve and convert all declared curves up front. This prevents the
    // upstream filter_map path from removing a failed edge and renumbering it.
    for id in table.edge_curve.keys() {
        let edge = EntityTable::<step_in::EdgeCurveHolder>::get_owned(&table, *id)
            .map_err(|error| invalid(format!("STEP edge #{id}: {error}")))?;
        edge.parse_curve3d()
            .map_err(|error| invalid(format!("STEP edge #{id}: {error}")))?;
    }
    for id in table.face_surface.keys() {
        let face = EntityTable::<step_in::FaceSurfaceHolder>::get_owned(&table, *id)
            .map_err(|error| invalid(format!("STEP face #{id}: {error}")))?;
        alias::Surface::try_from(&face.face_geometry)
            .map_err(|error| invalid(format!("STEP face #{id}: {error}")))?;
    }
    let shell = table
        .to_compressed_shell(&table.shell[&solid_shells[0]])
        .map_err(|error| invalid(format!("cannot extract STEP shell: {error}")))?;
    let boundary_loops = shell
        .faces
        .iter()
        .map(|face| face.boundaries.len())
        .sum::<usize>();
    let boundary_edges = shell
        .faces
        .iter()
        .flat_map(|face| &face.boundaries)
        .map(Vec::len)
        .sum::<usize>();
    let expected_boundary_edges = table
        .edge_loop
        .values()
        .map(|wire| wire.edge_list.len())
        .sum::<usize>();
    if shell.faces.len() != table.face_surface.len()
        || shell.edges.len() != table.edge_curve.len()
        || shell.vertices.len() != table.vertex_point.len()
        || boundary_loops != table.face_bound.len()
        || boundary_loops != table.edge_loop.len()
        || boundary_edges != expected_boundary_edges
    {
        return Err(invalid(
            "STEP topology extraction dropped or omitted declared faces, edges, vertices, or boundaries",
        ));
    }
    audit_geometry(&shell)?;
    let stats=TopologyStats {
        shells:1,faces:shell.faces.len(),edges:shell.edges.len(),vertices:shell.vertices.len(),boundary_loops,
        curved_edges:shell.edges.iter().filter(|edge|!matches!(edge.curve,alias::Curve3D::Line(_))).count(),
        nonplanar_faces:shell.faces.iter().filter(|face|!matches!(&face.surface,alias::Surface::ElementarySurface(surface) if matches!(surface.as_ref(),alias::ElementarySurface::Plane(_)))).count(),
    };
    let solid = StepSolid::extract(CompressedSolid {
        boundaries: vec![shell],
    })
    .map_err(|error| invalid(format!("STEP is not a closed manifold B-rep: {error}")))?;
    Ok(BrepModel {
        source: source.to_owned(),
        solid,
        stats,
    })
}

fn audit_parameter(parameter: &Parameter, ids: &BTreeSet<u64>, depth: usize) -> io::Result<()> {
    if depth > 128 {
        return Err(invalid("STEP parameter nesting exceeds limit"));
    }
    match parameter {
        Parameter::Real(value) if !value.is_finite() => {
            return Err(invalid("STEP contains a non-finite coordinate"));
        }
        Parameter::Ref(Name::Entity(id)) if !ids.contains(id) => {
            return Err(invalid(format!("STEP references missing entity #{id}")));
        }
        Parameter::List(values) => {
            for value in values {
                audit_parameter(value, ids, depth + 1)?;
            }
        }
        Parameter::Typed { parameter, .. } => audit_parameter(parameter, ids, depth + 1)?,
        _ => {}
    }
    Ok(())
}

fn audit_geometry(
    shell: &CompressedShell<Point3, alias::Curve3D, alias::Surface>,
) -> io::Result<()> {
    for point in &shell.vertices {
        if ![point.x, point.y, point.z]
            .iter()
            .all(|value| value.is_finite())
        {
            return Err(invalid("B-rep contains non-finite vertex geometry"));
        }
    }
    for edge in &shell.edges {
        let (first, last) = edge.vertices;
        let a = *shell
            .vertices
            .get(first)
            .ok_or_else(|| invalid("B-rep edge vertex index is invalid"))?;
        let b = *shell
            .vertices
            .get(last)
            .ok_or_else(|| invalid("B-rep edge vertex index is invalid"))?;
        let (start, end) = edge.curve.range_tuple();
        if !start.is_finite() || !end.is_finite() || end <= start {
            return Err(invalid("B-rep curve parameter range is invalid"));
        }
        let tolerance = 1e-6_f64.max(a.distance(b) * 1e-8);
        if edge.curve.subs(start).distance(a) > tolerance
            || edge.curve.subs(end).distance(b) > tolerance
        {
            return Err(invalid(
                "STEP curve endpoints disagree with topological vertices",
            ));
        }
        for fraction in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let p = edge.curve.subs(start + (end - start) * fraction);
            if ![p.x, p.y, p.z].iter().all(|value| value.is_finite()) {
                return Err(invalid("STEP curve evaluates to non-finite geometry"));
            }
        }
    }
    Ok(())
}

fn validate_dimensions(center: [f64; 3], size: [f64; 3]) -> io::Result<()> {
    if !center.iter().all(|value| value.is_finite())
        || !size.iter().all(|value| value.is_finite() && *value > 1e-5)
    {
        return Err(invalid(
            "primitive center must be finite and dimensions must exceed kernel tolerance (0.00001 mm)",
        ));
    }
    if center.iter().zip(size).any(|(center, size)| {
        !(*center - size / 2.0).is_finite()
            || !(*center + size / 2.0).is_finite()
            || *center - size / 2.0 == *center + size / 2.0
    }) {
        return Err(invalid(
            "primitive dimensions cannot be represented at this coordinate magnitude",
        ));
    }
    Ok(())
}
fn checked_kernel<T>(operation: impl FnOnce() -> io::Result<T>) -> io::Result<T> {
    catch_unwind(AssertUnwindSafe(operation)).unwrap_or_else(|_| {
        Err(invalid(
            "CAD kernel rejected this geometry; no partially converted B-rep was returned",
        ))
    })
}
fn invalid(message: impl ToString) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_string())
}
fn unsupported(message: impl ToString) -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, message.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn box_is_analytic_and_round_trips_without_topology_loss() {
        let model = make_box([4.0, -3.0, 2.0], [20.0, 12.0, 8.0]).unwrap();
        assert_eq!(
            model.stats(),
            TopologyStats {
                shells: 1,
                faces: 6,
                edges: 12,
                vertices: 8,
                boundary_loops: 6,
                curved_edges: 0,
                nonplanar_faces: 0
            }
        );
        assert!(model.step_source().contains("MANIFOLD_SOLID_BREP"));
        assert!(
            model
                .step_source()
                .contains("FILE_SCHEMA(('AUTOMOTIVE_DESIGN'))")
        );
        assert!(!model.step_source().contains("TRIANGULATED_FACE_SET"));
        let reparsed = BrepModel::from_step(&model.regenerated_geometry_step().unwrap()).unwrap();
        assert_eq!(reparsed.stats(), model.stats());
    }
    #[test]
    fn elliptic_cylinder_retains_rational_curves_and_surfaces() {
        let model = make_elliptic_cylinder([3.0, 2.0, -1.0], [20.0, 12.0, 16.0]).unwrap();
        assert!(model.stats().curved_edges > 0);
        assert!(model.stats().nonplanar_faces > 0);
        assert!(model.step_source().contains("RATIONAL_B_SPLINE_CURVE"));
        assert!(model.step_source().contains("RATIONAL_B_SPLINE_SURFACE"));
        let regenerated = model.regenerated_geometry_step().unwrap();
        assert_eq!(
            BrepModel::from_step(&regenerated).unwrap().stats(),
            model.stats()
        );
    }
    #[test]
    fn invalid_primitive_parameters_fail() {
        for size in [
            [0.0, 1.0, 1.0],
            [-1.0, 2.0, 2.0],
            [f64::NAN, 1.0, 1.0],
            [1e-9, 1.0, 1.0],
        ] {
            assert!(make_box([0.0; 3], size).is_err());
        }
        assert!(make_cylinder([0.0; 3], -2.0, 4.0).is_err());
        assert!(make_box([1e30, 0.0, 0.0], [1.0; 3]).is_err());
    }
    #[test]
    fn corrupted_missing_geometry_and_open_shells_are_rejected() {
        let model = make_box([0.0; 3], [10.0; 3]).unwrap();
        let source = model.step_source();
        assert!(BrepModel::from_step(&source.replace("CLOSED_SHELL", "OPEN_SHELL")).is_err());
        assert!(BrepModel::from_step(&source.replace("EDGE_CURVE", "UNSUPPORTED_CURVE")).is_err());
        let mut text = source.to_owned();
        let line = text
            .lines()
            .find(|line| line.contains("CARTESIAN_POINT"))
            .unwrap()
            .to_owned();
        text = text.replace(&line, "");
        assert!(BrepModel::from_step(&text).is_err());
        assert!(BrepModel::from_step("not a STEP file").is_err());
    }
    #[test]
    fn unchanged_source_retains_all_metadata_bytes() {
        let source = make_box([0.0; 3], [5.0; 3])
            .unwrap()
            .step_source()
            .replace("Forge Craft box", "Document metadata preserved");
        assert_eq!(BrepModel::from_step(&source).unwrap().step_source(), source);
    }
}
