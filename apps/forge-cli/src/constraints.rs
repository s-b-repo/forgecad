//! Sketch constraint reporting, solving and point handle discovery.
use super::*;
use forge_core::{
    Geometry,
    constraints::{PointHandle, PointRef},
};

const HELP: &str = "Sketch constraint tools

  forge constraints report INPUT [--json]
  forge constraints solve INPUT|- OUTPUT [--overwrite]
  forge constraints handles INPUT ENTITY-ID [--json]

report and handles read a saved .forge.json project. solve writes a solved
project and refuses a conflicting sketch without touching the output. Input '-'
solves a new drawing. In-place editing and replacing an existing output require
--overwrite (alias --force).

Relations are authored through the shared drafting language, which solves each
relation as one transaction and rejects a conflicting set:

  forge draft in out --command \"RELATION HORIZONTAL 2\" --command \"CONSTRAINT SOLVE\"

report exits 0 for a conflicting sketch: the summary is a successful query.
Use forge constraints handles to list an entity's point handles before writing
a RELATION command; handles are START END CENTER POSITION CORNERA CORNERB
VERTEX n. Only lines, rectangles, circles, points and straight polylines carry
constraint variables.
";

pub(super) fn run(args: &mut Arguments<'_>) -> io::Result<()> {
    match args.next().unwrap_or("--help") {
        "--help" | "help" | "-h" => {
            print!("{HELP}");
            Ok(())
        }
        "report" => report(args),
        "solve" => solve(args),
        "handles" => handles(args),
        command => Err(input(format!(
            "unknown constraints command {command}; run forge constraints --help"
        ))),
    }
}

/// Constraints persist in Forge projects only; neutral formats drop them.
fn native_document(path: &Path) -> io::Result<Document> {
    let loaded = load(path)?;
    if !matches!(loaded.format, Format::Native) {
        return Err(unsupported(format!(
            "sketch constraints are stored in Forge projects; {} is not a .forge.json project",
            path.display()
        )));
    }
    Ok(loaded.document)
}

fn require_native_output(path: &Path) -> io::Result<()> {
    if matches!(Format::from_path(path)?, Format::Native) {
        Ok(())
    } else {
        Err(unsupported(
            "constraint solving writes .forge.json projects; constraints are not retained in neutral formats",
        ))
    }
}

fn report(args: &mut Arguments<'_>) -> io::Result<()> {
    let path = Path::new(args.required("input path")?);
    let mut as_json = false;
    while let Some(flag) = args.next() {
        if flag == "--json" {
            as_json = true;
        } else {
            return Err(input(format!("unknown constraints report option {flag}")));
        }
    }
    let document = native_document(path)?;
    let report = document.constraint_report()?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("{}", report.summary());
        for constraint in &document.constraints {
            println!("  #{} {}", constraint.id, constraint.relation.describe());
        }
    }
    Ok(())
}

fn solve(args: &mut Arguments<'_>) -> io::Result<()> {
    let source_arg = args.required("input path or '-' for a new drawing")?;
    let destination = Path::new(args.required("native project output")?);
    require_native_output(destination)?;
    let mut overwrite = false;
    while let Some(flag) = args.next() {
        if matches!(flag, "--overwrite" | "--force") {
            overwrite = true;
        } else {
            return Err(input(format!("unknown constraints solve option {flag}")));
        }
    }
    let mut document = if source_arg == "-" {
        Document::default()
    } else {
        let source = Path::new(source_arg);
        reject_same_path(source, destination, overwrite)?;
        native_document(source)?
    };
    let report = document.solve_constraints()?;
    if !report.solved() {
        return Err(input(format!(
            "constraints not solved: {}; no output written",
            report.summary()
        )));
    }
    write_output(destination, document.to_json()?.as_bytes(), overwrite)?;
    println!("{}", report.summary());
    report_written(destination, &[]);
    Ok(())
}

fn handles(args: &mut Arguments<'_>) -> io::Result<()> {
    let path = Path::new(args.required("input path")?);
    let id = args
        .required("entity ID")?
        .parse::<u64>()
        .map_err(|_| input("entity ID must be a positive integer"))?;
    let mut as_json = false;
    while let Some(flag) = args.next() {
        if flag == "--json" {
            as_json = true;
        } else {
            return Err(input(format!("unknown constraints handles option {flag}")));
        }
    }
    let document = native_document(path)?;
    let entity = document
        .entities
        .iter()
        .find(|entity| entity.id == id)
        .ok_or_else(|| input(format!("entity #{id} does not exist in {}", path.display())))?;
    let handles = forge_core::constraints::handles(&entity.geometry);
    if handles.is_empty() {
        return Err(input(format!(
            "entity #{id} is a {} with no constraint point handles; constraints support lines, rectangles, circles, points and straight polylines",
            geometry_name(&entity.geometry)
        )));
    }
    let positions: Vec<(PointHandle, Option<Point>)> = handles
        .into_iter()
        .map(|handle| {
            let position =
                forge_core::constraints::point_position(&document, PointRef { entity: id, handle })
                    .ok();
            (handle, position)
        })
        .collect();
    if as_json {
        let entries: Vec<_> = positions
            .iter()
            .map(|(handle, position)| {
                json!({
                    "name": handle_name(*handle),
                    "x": position.map(|point| point.x),
                    "y": position.map(|point| point.y),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&entries)?);
    } else {
        println!(
            "Entity #{id} {} point handles",
            geometry_name(&entity.geometry)
        );
        for (handle, position) in &positions {
            match position {
                Some(point) => println!("  {} ({}, {})", handle_name(*handle), point.x, point.y),
                None => println!("  {}", handle_name(*handle)),
            }
        }
    }
    Ok(())
}

fn geometry_name(geometry: &Geometry) -> &'static str {
    match geometry {
        Geometry::Line { .. } => "line",
        Geometry::Rectangle { .. } => "rectangle",
        Geometry::Circle { .. } => "circle",
        Geometry::Arc { .. } => "arc",
        Geometry::Point { .. } => "point",
        Geometry::Polyline { .. } => "polyline",
        Geometry::Text { .. } => "text",
        Geometry::Insert { .. } => "block insert",
        Geometry::Dimension { .. } => "dimension",
    }
}

fn handle_name(handle: PointHandle) -> String {
    match handle {
        PointHandle::Start => "start".to_owned(),
        PointHandle::End => "end".to_owned(),
        PointHandle::Center => "center".to_owned(),
        PointHandle::Position => "position".to_owned(),
        PointHandle::CornerA => "cornera".to_owned(),
        PointHandle::CornerB => "cornerb".to_owned(),
        PointHandle::Vertex(index) => format!("vertex{index}"),
    }
}
