//! Persistent extrusion authoring; neutral export stays behind the loss policy.
use super::*;
use forge_core::Geometry;
const HELP:&str="Parametric profile extrusions

  forge model extrude INPUT.forge.json|- OUTPUT.forge.json --height H [--base-z Z] [--name NAME] PROFILE [--overwrite]
  forge model set INPUT OUTPUT.forge.json --id ID [--height H] [--base-z Z] [--name NAME] [PROFILE] [--overwrite]
  forge model remove INPUT OUTPUT.forge.json --id ID [--overwrite]
  forge model inspect INPUT.forge.json [--json]

PROFILE is exactly one of --entity ID, --rectangle X1 Y1 X2 Y2,
or --circle CX CY RADIUS. Entity profiles may also be closed straight polylines.
Coordinates/heights use project units. Native saves retain editable profiles,
base elevations and heights; OBJ/STL export tessellates them with explicit loss
authorization through forge convert. Existing outputs require --overwrite/--force.
";
pub(super) fn run(args: &mut Arguments<'_>) -> io::Result<()> {
    let command = args.next().unwrap_or("--help");
    if matches!(command, "help" | "--help" | "-h") {
        print!("{HELP}");
        return Ok(());
    }
    if !matches!(command, "extrude" | "set" | "remove" | "inspect") {
        return Err(input("Unknown model command; use forge model --help"));
    }
    let source = args.required("native input or -")?;
    let mut doc = if source == "-" {
        Document::default()
    } else {
        Document::load_json(source)?
    };
    if command == "inspect" {
        let json_output = match args.next() {
            None => false,
            Some("--json") => true,
            _ => return Err(input("Only --json is supported")),
        };
        if args.next().is_some() {
            return Err(input("Unexpected model inspect arguments"));
        }
        let mut parts = Vec::new();
        for part in &doc.extrusions {
            let mesh = part.mesh()?;
            parts.push(json!({"id":part.id,"name":part.name,"profile":part.profile,"base_z":part.base_z,"height":part.height,"triangles":mesh.triangles.len(),"volume":mesh.signed_volume()?}));
        }
        if !json_output {
            println!("Model: {} (unit code {})", doc.name, doc.units);
        }
        println!(
            "{}",
            serde_json::to_string_pretty(
                &json!({"units":doc.units,"extrusions":parts,"primitives":doc.solids.len(),"meshes":doc.meshes.len()})
            )?
        );
        return Ok(());
    }
    let destination = Path::new(args.required("native output")?);
    if !matches!(Format::from_path(destination)?, Format::Native) {
        return Err(input("Editable extrusions require .forge.json output"));
    }
    let mut overwrite = false;
    let mut id = None;
    let mut name = None;
    let mut height = None;
    let mut base_z = None;
    let mut profile = None;
    while let Some(flag) = args.next() {
        match (command, flag) {
            (_, "--overwrite" | "--force") => overwrite = true,
            ("set" | "remove", "--id") => {
                id = Some(
                    args.required("feature ID")?
                        .parse::<u64>()
                        .map_err(|_| input("Feature ID must be an integer"))?,
                )
            }
            ("set" | "extrude", "--name") => name = Some(args.required("part name")?.to_owned()),
            ("set" | "extrude", "--height") => height = Some(args.number("height")?),
            ("set" | "extrude", "--base-z") => base_z = Some(args.number("base elevation")?),
            ("set" | "extrude", "--entity" | "--rectangle" | "--circle") => {
                if profile.is_some() {
                    return Err(input("Supply exactly one profile"));
                }
                profile = Some(match flag {
                    "--rectangle" => Geometry::Rectangle {
                        a: Point::new(args.number("X1")?, args.number("Y1")?),
                        b: Point::new(args.number("X2")?, args.number("Y2")?),
                    },
                    "--circle" => Geometry::Circle {
                        center: Point::new(args.number("CX")?, args.number("CY")?),
                        radius: args.number("radius")?,
                    },
                    _ => {
                        let id = args
                            .required("entity ID")?
                            .parse::<u64>()
                            .map_err(|_| input("Entity ID must be an integer"))?;
                        doc.entities
                            .iter()
                            .find(|e| e.id == id)
                            .ok_or_else(|| input("Profile entity does not exist"))?
                            .geometry
                            .clone()
                    }
                });
            }
            _ => return Err(input(format!("Unknown {command} option {flag}"))),
        }
    }
    if source != "-" {
        reject_same_path(Path::new(source), destination, overwrite)?;
    }
    match command {
        "extrude" => {
            doc.add_extrusion(
                name.unwrap_or("Extrusion".into()),
                profile.ok_or_else(|| input("A profile is required"))?,
                base_z.unwrap_or(0.0),
                height.ok_or_else(|| input("--height is required"))?,
            )?;
        }
        "set" => {
            let id = id.ok_or_else(|| input("--id is required"))?;
            let part = doc
                .extrusions
                .iter_mut()
                .find(|p| p.id == id)
                .ok_or_else(|| input("Extrusion does not exist"))?;
            if let Some(v) = name {
                part.name = v;
            }
            if let Some(v) = height {
                part.height = v;
            }
            if let Some(v) = base_z {
                part.base_z = v;
            }
            if let Some(v) = profile {
                part.profile = v;
            }
        }
        "remove" => {
            let id = id.ok_or_else(|| input("--id is required"))?;
            if !doc.extrusions.iter().any(|p| p.id == id) {
                return Err(input("Extrusion does not exist"));
            }
            doc.extrusions.retain(|p| p.id != id);
        }
        _ => return Err(input("Unknown model command")),
    }
    write_output(destination, doc.to_json()?.as_bytes(), overwrite)?;
    report_written(destination, &[]);
    Ok(())
}
