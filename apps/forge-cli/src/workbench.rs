//! Headless authoring and derived exports for semantic workbenches.
use super::*;

const HELP: &str = "Semantic workbench tools

  forge workbench create build|terrain|motion|make|connect|all OUTPUT.forge.json [--overwrite]
  forge workbench inspect INPUT.forge.json [--json]
  forge workbench export INPUT.forge.json OUTPUT --area build|terrain|motion|make|connect [OPTIONS]
  forge workbench survey INPUT.csv OUTPUT.forge.json [--overwrite]
  forge workbench apply INPUT.forge.json|- OUTPUT.forge.json --script EDITS.json [--overwrite]
  forge workbench station INPUT.forge.json --alignment ID --point X Y [--json]
  forge workbench earthwork INPUT.forge.json --datum Z [--json]

create writes an editable example; every workbench also has desktop authoring.
survey reads headerless X,Y,Z millimetre survey rows (optional x,y,z header).
Export options: --time SECONDS (motion), --interval STEP (terrain contours),
--alignment ID --samples COUNT --offset DISTANCE (terrain profile CSV),
--datum Z (terrain cut/fill CSV),
--overwrite or --force. Export selects one area and leaves other areas untouched.
build: .csv quantities, .obj mesh; terrain: .obj surface, .csv surveyed points,
.dxf contours; motion: .obj evaluated frame; make: .nc/.gcode profile program;
connect: .csv local review issues. Survey CSV preserves an optional '# units: CODE'
comment; coordinates otherwise default to millimetres.
These are derived outputs; save the native project to retain semantic models.
Animation materials are retained in the native project, not geometry-only OBJ.
G-code covers profiles, rectangular/circular/polygon pockets with optional
uncut islands and point-based peck drilling with stock top Z=0 and millimetre units,
not a machine-specific postprocessor or stock/collision simulation.
";

pub(super) fn run(args: &mut Arguments<'_>) -> io::Result<()> {
    match args.next().unwrap_or("--help") {
        "--help" | "help" | "-h" => {
            print!("{HELP}");
            Ok(())
        }
        "create" => create(args),
        "inspect" => inspect(args),
        "export" => export(args),
        "survey" => survey(args),
        "apply" => apply(args),
        "station" => station(args),
        "earthwork" => earthwork(args),
        _ => Err(input(
            "Unknown workbench command; use forge workbench --help",
        )),
    }
}
fn overwrite_flag(args: &mut Arguments<'_>) -> io::Result<bool> {
    let mut overwrite = false;
    while let Some(flag) = args.next() {
        match flag {
            "--force" | "--overwrite" => overwrite = true,
            _ => return Err(input(format!("Unknown option {flag}"))),
        }
    }
    Ok(overwrite)
}
fn native_output(path: &Path) -> io::Result<()> {
    if matches!(Format::from_path(path)?, Format::Native) {
        Ok(())
    } else {
        Err(input("Semantic projects require .forge.json output"))
    }
}
fn create(args: &mut Arguments<'_>) -> io::Result<()> {
    let area = args.required("workbench area")?;
    let destination = Path::new(args.required("native project output")?);
    native_output(destination)?;
    let overwrite = overwrite_flag(args)?;
    let mut doc = Document::new(format!("{} workbench example", area));
    doc.units = 4;
    match area {
        "build" => doc.building = forge_core::building::Building::example(),
        "terrain" => doc.terrain = forge_core::terrain::Terrain::example(),
        "motion" => doc.scene = forge_core::scene::Scene::example(),
        "make" => doc.manufacturing = forge_core::cam::CamJob::example(),
        "connect" => doc.review = forge_core::review::ProjectReview::example(),
        "all" => {
            doc.building = forge_core::building::Building::example();
            doc.terrain = forge_core::terrain::Terrain::example();
            doc.scene = forge_core::scene::Scene::example();
            doc.manufacturing = forge_core::cam::CamJob::example();
            doc.review = forge_core::review::ProjectReview::example();
        }
        _ => {
            return Err(input(
                "Area must be build, terrain, motion, make, connect or all",
            ));
        }
    }
    write_output(destination, doc.to_json()?.as_bytes(), overwrite)?;
    report_written(destination, &[]);
    Ok(())
}
fn inspect(args: &mut Arguments<'_>) -> io::Result<()> {
    let source = Path::new(args.required("native project input")?);
    let mut json_output = false;
    while let Some(flag) = args.next() {
        if flag == "--json" {
            json_output = true;
        } else {
            return Err(input("Only --json is supported"));
        }
    }
    let doc = Document::load_json(source)?;
    let plan = doc.manufacturing.plan()?;
    let mut info = json!({"layouts":doc.layouts.len(),"building":{"storeys":doc.building.storeys.len(),"walls":doc.building.walls.len(),"slabs":doc.building.slabs.len(),"openings":doc.building.openings.len()},"terrain":{"points":doc.terrain.points.len(),"triangles":doc.terrain.triangles.len(),"alignments":doc.terrain.alignments.len()},"scene":{"objects":doc.scene.objects.len(),"parented_objects":doc.scene.objects.iter().filter(|o| o.parent_id.is_some()).count(),"duration":doc.scene.duration,"fps":doc.scene.fps,"frames":doc.scene.frame_times()?.len()},"manufacturing":{"profiles":doc.manufacturing.profiles.len(),"pockets":doc.manufacturing.profiles.iter().filter(|p| p.operation==forge_core::cam::Operation::Pocket).count(),"drills":doc.manufacturing.profiles.iter().filter(|p| p.operation==forge_core::cam::Operation::Drill).count(),"islands":doc.manufacturing.profiles.iter().map(|p| p.islands.len()).sum::<usize>(),"passes":plan.passes,"motions":plan.motions.len(),"estimated_minutes":plan.estimated_minutes}});
    info["review"] = json!({"items":doc.review.items.len(),"resolved":doc.review.items.iter().filter(|item|item.status==forge_core::review::ReviewStatus::Resolved).count()});
    if !json_output {
        println!("Semantic project: {}", doc.name);
    }
    println!("{}", serde_json::to_string_pretty(&info)?);
    Ok(())
}
fn export(args: &mut Arguments<'_>) -> io::Result<()> {
    let source = Path::new(args.required("native input")?);
    let destination = Path::new(args.required("derived output")?);
    let mut area = "";
    let mut time = 0.0;
    let mut interval = 100.0;
    let mut overwrite = false;
    let mut alignment = None;
    let mut samples = 101;
    let mut offset = 0.0;
    let mut datum = None;
    while let Some(flag) = args.next() {
        match flag {
            "--area" => area = args.required("area")?,
            "--alignment" => alignment = Some(id_argument(args)?),
            "--samples" => samples = args.count("profile sample count")?,
            "--offset" => offset = args.number("profile offset")?,
            "--datum" => datum = Some(args.number("design elevation")?),
            "--time" => time = args.number("frame time")?,
            "--interval" => interval = args.number("contour interval")?,
            "--overwrite" | "--force" => overwrite = true,
            _ => return Err(input(format!("Unknown export option {flag}"))),
        }
    }
    reject_same_path(source, destination, false)?;
    let doc = Document::load_json(source)?;
    let ext = destination
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if (alignment.is_some() || datum.is_some()) && (area != "terrain" || ext != "csv") {
        return Err(input(
            "Alignment and earthwork reports require --area terrain and CSV output",
        ));
    }
    if alignment.is_some() && datum.is_some() {
        return Err(input(
            "Choose either an alignment profile or an earthwork report",
        ));
    }
    let bytes = match (area, ext.as_str()) {
        ("build", "csv") => doc.building.schedule_csv()?.into_bytes(),
        ("connect", "csv") => doc.review.csv()?.into_bytes(),
        ("terrain", "csv") => {
            if let Some(id) = alignment {
                doc.terrain.alignment_csv(id, samples, offset)?.into_bytes()
            } else if let Some(datum) = datum {
                let q = doc.terrain.earthwork(datum)?;
                format!(
                    "unit_code,datum,area,cut,fill,net_cut\n{},{},{},{},{},{}\n",
                    doc.units, q.datum, q.area, q.cut, q.fill, q.net_cut
                )
                .into_bytes()
            } else {
                format!("# units: {}\n{}", doc.units, doc.terrain.points_csv()?).into_bytes()
            }
        }
        ("make", "nc" | "gcode") => doc.manufacturing.gcode()?.into_bytes(),
        ("terrain", "dxf") => {
            let drawing = doc.terrain.contour_drawing(interval, doc.units)?;
            dxf::write(&drawing)?.into_bytes()
        }
        ("build" | "terrain" | "motion", "obj") => {
            let mut mesh = TriangleMesh::default();
            match area {
                "build" => {
                    for (_, _, part) in doc.building.meshes()? {
                        mesh.append(&part)?;
                    }
                }
                "terrain" => mesh = doc.terrain.mesh()?,
                _ => {
                    for (_, _, _, part) in doc.scene.meshes_at(time)? {
                        mesh.append(&part)?;
                    }
                }
            }
            if mesh.triangles.is_empty() {
                return Err(input("The selected area has no surface geometry"));
            }
            mesh.to_obj(&doc.name)?.into_bytes()
        }
        _ => {
            return Err(input(
                "Unsupported area/output pair; use forge workbench --help",
            ));
        }
    };
    write_output(destination, &bytes, overwrite)?;
    report_written(destination, &[]);
    Ok(())
}

fn id_argument(args: &mut Arguments<'_>) -> io::Result<u64> {
    let id = args
        .required("positive ID")?
        .parse::<u64>()
        .map_err(|_| input("ID must be a positive integer"))?;
    if id == 0 {
        return Err(input("ID must be a positive integer"));
    }
    Ok(id)
}

fn apply(args: &mut Arguments<'_>) -> io::Result<()> {
    let source = args.required("native input or -")?;
    let destination = Path::new(args.required("native output")?);
    native_output(destination)?;
    let mut script = None;
    let mut overwrite = false;
    while let Some(flag) = args.next() {
        match flag {
            "--script" => {
                if script.is_some() {
                    return Err(input("Supply one project script"));
                }
                script = Some(Path::new(args.required("JSON edit script")?));
            }
            "--overwrite" | "--force" => overwrite = true,
            _ => return Err(input(format!("Unknown apply option {flag}"))),
        }
    }
    let script = script.ok_or_else(|| input("--script EDITS.json is required"))?;
    reject_same_path(script, destination, false)?;
    if fs::metadata(script)?.len() > 1024 * 1024 {
        return Err(input("Project edit script exceeds 1 MiB"));
    }
    let edits: Vec<forge_core::automation::ProjectEdit> =
        serde_json::from_str(&fs::read_to_string(script)?)?;
    let mut document = if source == "-" {
        Document::default()
    } else {
        reject_same_path(Path::new(source), destination, overwrite)?;
        Document::load_json(source)?
    };
    forge_core::automation::apply_edits(&mut document, &edits)?;
    write_output(destination, document.to_json()?.as_bytes(), overwrite)?;
    report_written(destination, &[]);
    Ok(())
}

fn station(args: &mut Arguments<'_>) -> io::Result<()> {
    let doc = Document::load_json(args.required("native input")?)?;
    let mut alignment = None;
    let mut point = None;
    let mut json_output = false;
    while let Some(flag) = args.next() {
        match flag {
            "--alignment" => alignment = Some(id_argument(args)?),
            "--point" => point = Some(Point::new(args.number("query X")?, args.number("query Y")?)),
            "--json" => json_output = true,
            _ => return Err(input(format!("Unknown station option {flag}"))),
        }
    }
    let alignment = alignment.ok_or_else(|| input("--alignment ID is required"))?;
    let alignment = doc
        .terrain
        .alignments
        .iter()
        .find(|a| a.id == alignment)
        .ok_or_else(|| input("Alignment does not exist"))?;
    let projection =
        alignment.station_offset(point.ok_or_else(|| input("--point X Y is required"))?)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &json!({"unit_code":doc.units,"alignment":alignment.id,"projection":projection})
            )?
        );
    } else {
        println!(
            "Station {} · offset {} · nearest X {}, Y {} · distance {}",
            projection.station,
            projection.offset,
            projection.position.x,
            projection.position.y,
            projection.distance
        );
    }
    Ok(())
}

fn earthwork(args: &mut Arguments<'_>) -> io::Result<()> {
    let doc = Document::load_json(args.required("native input")?)?;
    let mut datum = None;
    let mut json_output = false;
    while let Some(flag) = args.next() {
        match flag {
            "--datum" => datum = Some(args.number("design elevation")?),
            "--json" => json_output = true,
            _ => return Err(input(format!("Unknown earthwork option {flag}"))),
        }
    }
    let q = doc
        .terrain
        .earthwork(datum.ok_or_else(|| input("--datum Z is required"))?)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({"unit_code":doc.units,"quantities":q}))?
        );
    } else {
        println!(
            "Datum {} · area {} · cut {} · fill {} · net cut {} (model units²/units³)",
            q.datum, q.area, q.cut, q.fill, q.net_cut
        );
    }
    Ok(())
}
fn survey(args: &mut Arguments<'_>) -> io::Result<()> {
    let source = Path::new(args.required("survey CSV input")?);
    let destination = Path::new(args.required("native output")?);
    native_output(destination)?;
    let overwrite = overwrite_flag(args)?;
    reject_same_path(source, destination, false)?;
    if fs::metadata(source)?.len() > 1024 * 1024 {
        return Err(input("Survey CSV exceeds 1 MiB"));
    }
    let mut points = Vec::new();
    let mut units = 4;
    for (index, line) in fs::read_to_string(source)?.lines().enumerate() {
        let line = line.trim();
        if let Some(code) = line.strip_prefix("# units:") {
            if !points.is_empty() {
                return Err(input("Survey units metadata must precede point rows"));
            }
            units = code
                .trim()
                .parse::<u16>()
                .map_err(|_| input("Survey units metadata must contain an integer code"))?;
            if units > 24 {
                return Err(input("Survey units metadata code must be 0..24"));
            }
            continue;
        }
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if points.is_empty()
            && matches!(
                line.to_ascii_lowercase().replace(' ', "").as_str(),
                "x,y,z" | "x,y,elevation"
            )
        {
            continue;
        }
        let values = line
            .split(',')
            .map(|v| {
                v.trim()
                    .parse::<f64>()
                    .map_err(|_| input(format!("Survey line {} requires numeric X,Y,Z", index + 1)))
            })
            .collect::<io::Result<Vec<_>>>()?;
        if values.len() != 3 {
            return Err(input(format!(
                "Survey line {} requires three columns",
                index + 1
            )));
        }
        points.push([values[0], values[1], values[2]]);
    }
    let mut doc = Document::new("Surveyed terrain");
    doc.units = units;
    doc.terrain = forge_core::terrain::Terrain::triangulate(points)?;
    write_output(destination, doc.to_json()?.as_bytes(), overwrite)?;
    report_written(destination, &[]);
    Ok(())
}
