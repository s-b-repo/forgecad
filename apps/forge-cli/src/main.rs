use forge_core::{
    Document, Point, SolidKind, catalog, dxf,
    edit::Transform2d,
    mesh::{StlEncoding, TessellationOptions, TriangleMesh},
    plot::{SvgOptions, export_svg},
};
use forge_dwg::{NativeDrawing, NativeVersion};
use forge_modeling::BrepModel;
use serde_json::json;
use std::{collections::BTreeMap, env, fs, io, path::Path};

mod constraints;
mod layout;
mod model;
mod workbench;

const HELP: &str = "Forge Craft headless tools

Usage:
  forge inspect INPUT [--json]
  forge convert INPUT OUTPUT [OPTIONS]
  forge transform INPUT OUTPUT [TRANSFORMS] [OPTIONS]
  forge draft INPUT|- OUTPUT [--command \"COMMAND\"] [--script FILE] [OPTIONS]
  forge plot INPUT OUTPUT.svg [--page WIDTH HEIGHT] [--margin MM] [--scale FACTOR] [--color] [--overwrite]
  forge plot INPUT OUTPUT.svg --layout NAME [--overwrite]
  forge primitive box|cylinder|sphere OUTPUT [--size X Y Z] [--center X Y Z] [OPTIONS]
  forge brep inspect INPUT.step [--json]
  forge workbench --help
  forge layout --help
  forge model --help
  forge brep primitive box|cylinder OUTPUT.step [--size X Y Z] [--center X Y Z] [--overwrite]
  forge brep convert INPUT.step OUTPUT.step [--regenerate --allow-lossy] [--overwrite]
  forge native inspect INPUT [--json]
  forge native convert INPUT OUTPUT [--version r2000|r2018] [--overwrite]
  forge native transform INPUT OUTPUT --handle H --translate DX DY DZ [--overwrite]
  forge products [QUERY] [--json]
  forge constraints report INPUT [--json]
  forge constraints solve INPUT|- OUTPUT [--overwrite]
  forge constraints handles INPUT ENTITY-ID [--json]

Transforms (applied in the order supplied):
  --translate DX DY     --rotate DEGREES     --scale FACTOR
  --mirror X1 Y1 X2 Y2  --pivot X Y          --copy

Options:
  --overwrite          Explicitly replace an existing output
  --allow-lossy        Explicitly allow reported unsupported data/metadata loss
  --ascii | --binary   STL encoding (default binary)
  --segments N         Primitive radial resolution (default 48)
  --latitudes N        Ellipsoid latitude resolution (default 24)
  --name NAME          Primitive document and solid name
  --units CODE         Primitive DXF unit code (0 unitless, 4 mm, 6 m)

Supported exchange: validated .forge.json projects, a documented 2D ASCII DXF
subset, geometry-only Wavefront OBJ, ASCII/binary STL triangle surfaces.
The constraints commands report, solve and list the point handles of persistent
sketch relations; authoring goes through forge draft --command \"RELATION HORIZONTAL 3\".
The brep commands create or inspect analytic STEP solids; run forge brep --help
for checked-reader limits. STEP exchange is separate from Forge project files.
The native commands read and write DWG through the acadrust engine; RVT, IPT,
MAX, MA/MB and FBX are not implemented. No output is written when a conversion
would discard supported data without --allow-lossy.
";

const NATIVE_HELP: &str = "Native DWG commands (acadrust engine)

Usage:
  forge native inspect INPUT [--json]
  forge native convert INPUT OUTPUT [--version r2000|r2018] [--overwrite]
  forge native transform INPUT OUTPUT --handle H [--handle ...] --translate DX DY DZ [--overwrite]

Reads .dwg drawings and ASCII .dxf input. Output is .dwg (write_dwg performs a
read-back inventory comparison) or .dxf. Saves are refused when the engine
reports unsupported or recovered content, so nothing silently degrades a
drawing. Handles accept decimal or 0x-prefixed hexadecimal values; list the
entities of a drawing with forge native inspect --json.
";

const BREP_HELP: &str = "Checked analytic B-rep / STEP tools

Usage:
  forge brep inspect INPUT.step [--json]
  forge brep primitive box|cylinder OUTPUT.step [--size X Y Z] [--center X Y Z] [--overwrite]
  forge brep convert INPUT.step OUTPUT.step [--regenerate --allow-lossy] [--overwrite]

Primitive sizes and centers are in millimetres. For cylinders X/Y are the two
diameters (unequal values create an exact elliptic cylinder); Z is the height.
Box defaults: 80 x 60 x 48 mm. Cylinder defaults: 60 x 60 x 80 mm.
Primitives retain analytic B-rep curves and surfaces rather than triangles.

Reads and writes .step or .stp files. The checked reader accepts exactly one
closed manifold solid with one shell, rejects incomplete topology, assemblies,
multiple bodies, open shells and external references, and limits input to 64 MiB.
It does not establish compatibility with every STEP schema or Autodesk product.

Conversion validates the input and copies its original STEP source byte for byte.
--regenerate authors a geometry-only STEP file and requires --allow-lossy because
names, units, placements and other metadata are regenerated rather than retained.
Existing output and in-place conversion require --overwrite. Aliases: --force for
--overwrite; --allow-loss for --allow-lossy.
";

const DRAFT_HELP: &str = "Transactional numeric drafting

Usage:
  forge draft INPUT|- OUTPUT [--command \"COMMAND\"] [--script FILE] [--overwrite] [--allow-lossy]

INPUT '-' starts a new drawing. OUTPUT must be a Forge project or ASCII DXF.
--command and --script may repeat and run in the order supplied. Scripts have
one command per line; empty lines and lines starting with # are ignored. A
newly authored or copied entity becomes the selection for the next command.
SELECT ALL selects visible drawing entities. Script errors leave the input
and output files untouched; the complete result is written only once.
Existing outputs and in-place editing require --overwrite (alias --force).

Commands:
";

const PLOT_HELP: &str = "SVG drawing sheets

Usage:
  forge plot INPUT OUTPUT.svg [--page WIDTH HEIGHT] [--margin MM] [--scale FACTOR] [--color] [--overwrite] [--allow-lossy]
  forge plot INPUT OUTPUT.svg --layout NAME [--overwrite]

Exports visible 2D drawing entities, expanded blocks and dimension annotation.
The default sheet is A4 landscape, fit to page, with monochrome strokes.
--page and --margin use millimetres; --scale sets paper mm per model unit.
--color uses layer colors. The SVG can be printed from a browser or vector editor.
--layout selects a saved multi-viewport sheet; its page/scale/color settings
come from the native layout. Use forge layout --help to create or edit sheets.
3D model geometry is refused rather than projected or discarded.
An existing output requires --overwrite (alias --force). Unsupported input
content requires explicit --allow-lossy before the supported subset is plotted.
";

fn main() {
    if let Err(error) = run(&env::args().skip(1).collect::<Vec<_>>()) {
        eprintln!("forge: {error}");
        std::process::exit(2);
    }
}

fn run(values: &[String]) -> io::Result<()> {
    let mut args = Arguments::new(values);
    match args.next().unwrap_or("help") {
        "help" | "--help" | "-h" => print!("{HELP}"),
        "--version" | "version" => println!("forge 0.1.0"),
        "inspect" => inspect_command(&mut args)?,
        "convert" => convert_command(&mut args)?,
        "transform" => transform_command(&mut args)?,
        "draft" => draft_command(&mut args)?,
        "plot" => plot_command(&mut args)?,
        "primitive" => primitive_command(&mut args)?,
        "products" => products_command(&mut args)?,
        "constraints" => constraints::run(&mut args)?,
        "native" => native_command(&mut args)?,
        "brep" => brep_command(&mut args)?,
        "workbench" => workbench::run(&mut args)?,
        "layout" => layout::run(&mut args)?,
        "model" => model::run(&mut args)?,
        command => {
            return Err(input(format!(
                "unknown command {command}; run forge --help"
            )));
        }
    }
    Ok(())
}

struct Arguments<'a> {
    values: &'a [String],
    cursor: usize,
}
impl<'a> Arguments<'a> {
    fn new(values: &'a [String]) -> Self {
        Self { values, cursor: 0 }
    }
    fn next(&mut self) -> Option<&'a str> {
        let value = self.values.get(self.cursor)?;
        self.cursor += 1;
        Some(value)
    }
    fn required(&mut self, name: &str) -> io::Result<&'a str> {
        self.next()
            .ok_or_else(|| input(format!("missing {name}; run forge --help")))
    }
    fn number(&mut self, name: &str) -> io::Result<f64> {
        let value = self.required(name)?;
        let value = value
            .parse::<f64>()
            .map_err(|_| input(format!("{name} must be a number")))?;
        if !value.is_finite() {
            return Err(input(format!("{name} must be finite")));
        }
        Ok(value)
    }
    fn count(&mut self, name: &str) -> io::Result<usize> {
        self.required(name)?
            .parse()
            .map_err(|_| input(format!("{name} must be a positive integer")))
    }
    fn vector(&mut self, name: &str) -> io::Result<[f64; 3]> {
        Ok([self.number(name)?, self.number(name)?, self.number(name)?])
    }
}

#[derive(Clone, Copy)]
enum Format {
    Native,
    Dxf,
    Obj,
    Stl,
}
impl Format {
    fn from_path(path: &Path) -> io::Result<Self> {
        match path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "json" | "forge" => Ok(Self::Native),
            "dxf" => Ok(Self::Dxf),
            "obj" => Ok(Self::Obj),
            "stl" => Ok(Self::Stl),
            "step" | "stp" => Err(unsupported(
                "analytic STEP exchange uses forge brep inspect/convert; STEP solids are separate from Forge project documents",
            )),
            "dwg" | "dwt" | "dws" => Err(unsupported(format!(
                "native format {} is not implemented by this command; use forge native inspect/convert/transform",
                path.extension().unwrap().to_string_lossy()
            ))),
            "rvt" | "rfa" | "rte" | "rft" | "ipt" | "iam" | "idw" | "ipn" | "max" | "ma" | "mb"
            | "fbx" | "nwd" | "nwc" | "dwf" | "dwfx" => Err(unsupported(format!(
                "native format {} is not implemented; changing an extension does not convert a file",
                path.extension().unwrap().to_string_lossy()
            ))),
            extension => Err(unsupported(format!(
                "unsupported file extension {extension:?}; supported: .forge.json, .dxf, .obj, .stl"
            ))),
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Native => "forge_json",
            Self::Dxf => "ascii_dxf_2d",
            Self::Obj => "obj_geometry",
            Self::Stl => "stl_mesh",
        }
    }
}

struct Loaded {
    document: Document,
    format: Format,
    losses: Vec<String>,
}

fn load(path: &Path) -> io::Result<Loaded> {
    let format = Format::from_path(path)?;
    let name = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let mut losses = Vec::new();
    let document = match format {
        Format::Native => Document::load_json(path)?,
        Format::Dxf => {
            let report = dxf::parse(&fs::read_to_string(path)?)?;
            if !report.unsupported_entities.is_empty() || !report.unsupported_features.is_empty() {
                losses.push(format!("DXF unsupported content: {}", report.summary()));
            }
            report.document
        }
        Format::Obj => {
            let source = fs::read_to_string(path)?;
            let mut attributes = BTreeMap::<&str, usize>::new();
            for line in source.lines() {
                if let Some(keyword) = line
                    .split('#')
                    .next()
                    .unwrap_or("")
                    .split_whitespace()
                    .next()
                {
                    if [
                        "vt",
                        "vn",
                        "mtllib",
                        "usemtl",
                        "g",
                        "o",
                        "s",
                        "bevel",
                        "c_interp",
                        "d_interp",
                        "lod",
                        "shadow_obj",
                        "trace_obj",
                        "ctech",
                        "stech",
                    ]
                    .contains(&keyword)
                    {
                        *attributes.entry(keyword).or_default() += 1;
                    }
                }
            }
            if !attributes.is_empty() {
                losses.push(format!(
                    "OBJ texture/material/normal/group metadata is not retained ({})",
                    attributes
                        .into_iter()
                        .map(|(key, count)| format!("{key}: {count}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            let mut document = Document::new(&name);
            document.add_mesh(&name, TriangleMesh::from_obj(&source)?)?;
            document
        }
        Format::Stl => {
            let bytes = fs::read(path)?;
            if bytes.len() >= 84 {
                let count = u32::from_le_bytes(bytes[80..84].try_into().unwrap()) as usize;
                if count
                    .checked_mul(50)
                    .and_then(|length| length.checked_add(84))
                    == Some(bytes.len())
                    && bytes[84..]
                        .chunks_exact(50)
                        .any(|triangle| triangle[48] != 0 || triangle[49] != 0)
                {
                    losses.push("STL attribute/color fields are not retained".to_owned());
                }
            }
            if let Ok(text) = std::str::from_utf8(&bytes) {
                let solids = text
                    .lines()
                    .filter(|line| line.split_whitespace().next() == Some("solid"))
                    .count();
                if solids > 1 {
                    losses.push(
                        "multiple ASCII STL solid groups are merged into one mesh".to_owned(),
                    );
                }
            }
            let mut document = Document::new(&name);
            document.add_mesh(&name, TriangleMesh::from_stl(&bytes)?)?;
            document
        }
    };
    document.validate()?;
    Ok(Loaded {
        document,
        format,
        losses,
    })
}

struct Options {
    overwrite: bool,
    allow_lossy: bool,
    encoding: StlEncoding,
    tessellation: TessellationOptions,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            overwrite: false,
            allow_lossy: false,
            encoding: StlEncoding::Binary,
            tessellation: TessellationOptions::default(),
        }
    }
}
impl Options {
    fn parse(&mut self, flag: &str, args: &mut Arguments<'_>) -> io::Result<bool> {
        match flag {
            "--overwrite" => self.overwrite = true,
            "--allow-lossy" => self.allow_lossy = true,
            "--ascii" => self.encoding = StlEncoding::Ascii,
            "--binary" => self.encoding = StlEncoding::Binary,
            "--segments" => self.tessellation.radial_segments = args.count("segment count")?,
            "--latitudes" => self.tessellation.latitude_segments = args.count("latitude count")?,
            _ => return Ok(false),
        }
        Ok(true)
    }
}

fn inspect_command(args: &mut Arguments<'_>) -> io::Result<()> {
    let path = Path::new(args.required("input path")?);
    let mut as_json = false;
    while let Some(flag) = args.next() {
        if flag == "--json" {
            as_json = true;
        } else {
            return Err(input(format!("unknown inspect option {flag}")));
        }
    }
    let loaded = load(path)?;
    let doc = &loaded.document;
    let vertices: usize = doc
        .meshes
        .iter()
        .map(|object| object.mesh.vertices.len())
        .sum();
    let triangles: usize = doc
        .meshes
        .iter()
        .map(|object| object.mesh.triangles.len())
        .sum();
    let bounds = doc
        .drawing_bounds(false)
        .map(|bounds| json!({"min":[bounds.min.x,bounds.min.y],"max":[bounds.max.x,bounds.max.y]}));
    let degrees_of_freedom = doc
        .constraint_report()
        .ok()
        .map(|report| report.degrees_of_freedom);
    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "format":loaded.format.name(),"name":doc.name,"entities":doc.entities.len(),
                "solids":doc.solids.len(),"extrusions":doc.extrusions.len(),"meshes":doc.meshes.len(),"vertices":vertices,"triangles":triangles,
                "layers":doc.layers,"layouts":doc.layouts.len(),"units":doc.units,"drawing_bounds":bounds,"conversion_losses":loaded.losses,
                "workbenches":{"storeys":doc.building.storeys.len(),"walls":doc.building.walls.len(),"slabs":doc.building.slabs.len(),"openings":doc.building.openings.len(),"survey_points":doc.terrain.points.len(),"terrain_triangles":doc.terrain.triangles.len(),"terrain_alignments":doc.terrain.alignments.len(),"scene_objects":doc.scene.objects.len(),"parented_objects":doc.scene.objects.iter().filter(|o| o.parent_id.is_some()).count(),"cam_profiles":doc.manufacturing.profiles.len(),"cam_pockets":doc.manufacturing.profiles.iter().filter(|p| p.operation==forge_core::cam::Operation::Pocket).count(),"cam_drills":doc.manufacturing.profiles.iter().filter(|p| p.operation==forge_core::cam::Operation::Drill).count(),"cam_islands":doc.manufacturing.profiles.iter().map(|p| p.islands.len()).sum::<usize>(),"review_items":doc.review.items.len(),"sketch_constraints":{"count":doc.constraints.len(),"degrees_of_freedom":degrees_of_freedom}},
            }))?
        );
    } else {
        println!(
            "{} ({})\n{} drawing entities, {} solids, {} meshes\n{} stored vertices, {} stored triangles\n{} layers; unit code {}",
            doc.name,
            loaded.format.name(),
            doc.entities.len(),
            doc.solids.len(),
            doc.meshes.len(),
            vertices,
            triangles,
            doc.layers.len(),
            doc.units
        );
        for warning in &loaded.losses {
            println!("Conversion loss: {warning}");
        }
    }
    Ok(())
}

fn convert_command(args: &mut Arguments<'_>) -> io::Result<()> {
    let source = Path::new(args.required("input path")?);
    let destination = Path::new(args.required("output path")?);
    let mut options = Options::default();
    while let Some(flag) = args.next() {
        if !options.parse(flag, args)? {
            return Err(input(format!("unknown convert option {flag}")));
        }
    }
    reject_same_path(source, destination, options.overwrite)?;
    let loaded = load(source)?;
    let mut warnings = Vec::new();
    for loss in loaded.losses {
        authorize_loss(&options, &mut warnings, loss)?;
    }
    let bytes = encode(
        &loaded.document,
        Format::from_path(destination)?,
        &options,
        &mut warnings,
    )?;
    write_output(destination, &bytes, options.overwrite)?;
    report_written(destination, &warnings);
    Ok(())
}

fn encode(
    document: &Document,
    format: Format,
    options: &Options,
    warnings: &mut Vec<String>,
) -> io::Result<Vec<u8>> {
    document.validate()?;
    if !matches!(format, Format::Native) && !document.layouts.is_empty() {
        authorize_loss(
            options,
            warnings,
            "Neutral conversion omits saved print layouts and paper notes; use forge plot --layout NAME for a derived SVG sheet",
        )?;
        let mut drawing = document.clone();
        drawing.layouts.clear();
        return encode(&drawing, format, options, warnings);
    }
    if !matches!(format, Format::Native) && document.has_workbench_data() {
        authorize_loss(
            options,
            warnings,
            "Neutral conversion omits semantic building, terrain, animation, CAM and project review data; use forge workbench export for their derived geometry/reports",
        )?;
        let mut drawing = document.clone();
        drawing.clear_workbench_data();
        return encode(&drawing, format, options, warnings);
    }
    match format {
        Format::Native => Ok(document.to_json()?.into_bytes()),
        Format::Dxf => {
            let mut drawing = document.clone();
            if drawing.has_model_geometry() {
                authorize_loss(
                    options,
                    warnings,
                    "DXF subset cannot retain 3D solids or triangle meshes",
                )?;
                drawing.solids.clear();
                drawing.meshes.clear();
                drawing.extrusions.clear();
            }
            if drawing.entities.is_empty() && document.has_model_geometry() {
                return Err(unsupported(
                    "conversion would produce an empty drawing; no output written",
                ));
            }
            Ok(dxf::write(&drawing)?.into_bytes())
        }
        Format::Stl | Format::Obj => {
            if !document.entities.is_empty() {
                authorize_loss(
                    options,
                    warnings,
                    "mesh exchange cannot retain 2D drawing entities or their layers",
                )?;
            }
            if !document.solids.is_empty() || !document.extrusions.is_empty() {
                authorize_loss(
                    options,
                    warnings,
                    "parametric solids and extrusions become tessellated triangle surfaces without editable parameters",
                )?;
            }
            if document.meshes.len() + document.solids.len() + document.extrusions.len() > 1 {
                authorize_loss(
                    options,
                    warnings,
                    "mesh exchange merges object identity and group names",
                )?;
            }
            if document.units != 0 {
                authorize_loss(
                    options,
                    warnings,
                    "mesh exchange does not encode the project unit code",
                )?;
            }
            let mesh = document.model_mesh(options.tessellation)?;
            if mesh.triangles.is_empty() {
                return Err(unsupported("no 3D triangle geometry to export"));
            }
            let name = if document.meshes.len() == 1
                && document.solids.is_empty()
                && document.extrusions.is_empty()
            {
                &document.meshes[0].name
            } else {
                &document.name
            };
            match format {
                Format::Obj => Ok(mesh.to_obj(name)?.into_bytes()),
                Format::Stl if options.encoding == StlEncoding::Ascii => {
                    Ok(mesh.to_stl_ascii(name)?.into_bytes())
                }
                _ => {
                    if mesh
                        .vertices
                        .iter()
                        .flatten()
                        .any(|value| (*value as f32) as f64 != *value)
                    {
                        authorize_loss(
                            options,
                            warnings,
                            "binary STL float32 coordinates round the original float64 positions; use --ascii to retain their decimal values",
                        )?;
                    }
                    mesh.to_stl_binary()
                }
            }
        }
    }
}

enum Operation {
    Translate(f64, f64),
    Rotate(f64),
    Scale(f64),
    Mirror(Point, Point),
}

fn transform_command(args: &mut Arguments<'_>) -> io::Result<()> {
    let source = Path::new(args.required("input path")?);
    let destination = Path::new(args.required("output path")?);
    let mut options = Options::default();
    let mut operations = Vec::new();
    let mut pivot = Point::default();
    let mut copy = false;
    while let Some(flag) = args.next() {
        match flag {
            "--translate" => {
                operations.push(Operation::Translate(args.number("dx")?, args.number("dy")?))
            }
            "--rotate" => operations.push(Operation::Rotate(args.number("rotation degrees")?)),
            "--scale" => operations.push(Operation::Scale(args.number("scale factor")?)),
            "--mirror" => operations.push(Operation::Mirror(
                Point::new(args.number("mirror x1")?, args.number("mirror y1")?),
                Point::new(args.number("mirror x2")?, args.number("mirror y2")?),
            )),
            "--pivot" => pivot = Point::new(args.number("pivot x")?, args.number("pivot y")?),
            "--copy" => copy = true,
            _ => {
                if !options.parse(flag, args)? {
                    return Err(input(format!("unknown transform option {flag}")));
                }
            }
        }
    }
    if operations.is_empty() {
        return Err(input("supply at least one transform"));
    }
    reject_same_path(source, destination, options.overwrite)?;
    let loaded = load(source)?;
    let mut warnings = Vec::new();
    for loss in loaded.losses {
        authorize_loss(&options, &mut warnings, loss)?;
    }
    let mut document = loaded.document;
    if document.has_model_geometry() || document.has_workbench_data() {
        return Err(unsupported(
            "transform currently edits 2D drafting only; files containing model geometry are refused so model objects cannot be left behind or discarded",
        ));
    }
    if document.entities.is_empty() {
        return Err(input("document has no drawing entities to transform"));
    }
    let mut transform = Transform2d::identity();
    for operation in operations {
        let next = match operation {
            Operation::Translate(dx, dy) => Transform2d::translation(dx, dy),
            Operation::Rotate(degrees) => Transform2d::rotation(degrees, pivot),
            Operation::Scale(factor) => Transform2d::uniform_scale(factor, pivot),
            Operation::Mirror(start, end) => Transform2d::reflection(start, end)?,
        };
        transform = transform.then(next);
    }
    let ids = document
        .entities
        .iter()
        .map(|entity| entity.id)
        .collect::<Vec<_>>();
    if copy {
        document.duplicate_entities(&ids, transform)?;
    } else {
        document.transform_entities(&ids, transform)?;
    }
    let bytes = encode(
        &document,
        Format::from_path(destination)?,
        &options,
        &mut warnings,
    )?;
    write_output(destination, &bytes, options.overwrite)?;
    report_written(destination, &warnings);
    Ok(())
}

fn draft_command(args: &mut Arguments<'_>) -> io::Result<()> {
    let source_arg = args.required("input path or '-' for a new drawing")?;
    if matches!(source_arg, "help" | "--help" | "-h") {
        print!("{DRAFT_HELP}{}\n", forge_core::command::HELP);
        return Ok(());
    }
    let destination = Path::new(args.required("output path")?);
    let format = Format::from_path(destination)?;
    if !matches!(format, Format::Native | Format::Dxf) {
        return Err(unsupported(
            "draft output must be a .forge.json project or .dxf drawing",
        ));
    }
    let mut options = Options::default();
    let mut commands = Vec::<(String, String)>::new();
    while let Some(flag) = args.next() {
        match flag {
            "--command" => {
                let command = args.required("draft command")?;
                if command.trim().is_empty() {
                    return Err(input("--command requires a nonempty drafting command"));
                }
                commands.push((
                    command.to_owned(),
                    format!("command {}", commands.len() + 1),
                ));
            }
            "--script" => {
                let script = Path::new(args.required("script path")?);
                if fs::metadata(script)?.len() > 1024 * 1024 {
                    return Err(input("draft scripts may not exceed 1 MiB"));
                }
                for (line_number, line) in fs::read_to_string(script)?.lines().enumerate() {
                    let line = line.trim();
                    if !line.is_empty() && !line.starts_with('#') {
                        commands.push((
                            line.to_owned(),
                            format!("{}:{}", script.display(), line_number + 1),
                        ));
                    }
                }
            }
            "--overwrite" | "--force" => options.overwrite = true,
            "--allow-lossy" => options.allow_lossy = true,
            _ => return Err(input(format!("unknown draft option {flag}"))),
        }
        if commands.len() > 10_000 {
            return Err(input("a draft batch may not exceed 10,000 commands"));
        }
    }
    if commands.is_empty() {
        return Err(input("supply at least one --command or nonempty --script"));
    }
    let mut warnings = Vec::new();
    let mut document = if source_arg == "-" {
        Document::default()
    } else {
        let source = Path::new(source_arg);
        reject_same_path(source, destination, options.overwrite)?;
        let loaded = load(source)?;
        for loss in loaded.losses {
            authorize_loss(&options, &mut warnings, loss)?;
        }
        loaded.document
    };
    let mut selection = Vec::new();
    let mut messages = Vec::new();
    for (command, origin) in commands {
        let result = forge_core::command::execute(&mut document, &selection, &command)
            .map_err(|error| io::Error::new(error.kind(), format!("{origin}: {error}")))?;
        selection = result.selection;
        messages.push(result.message);
    }
    let bytes = encode(&document, format, &options, &mut warnings)?;
    write_output(destination, &bytes, options.overwrite)?;
    for message in messages {
        println!("{message}");
    }
    report_written(destination, &warnings);
    Ok(())
}

fn plot_command(args: &mut Arguments<'_>) -> io::Result<()> {
    let source_arg = args.required("input path")?;
    if matches!(source_arg, "help" | "--help" | "-h") {
        print!("{PLOT_HELP}");
        return Ok(());
    }
    let source = Path::new(source_arg);
    let destination = Path::new(args.required("SVG output path")?);
    if !destination
        .extension()
        .is_some_and(|value| value.eq_ignore_ascii_case("svg"))
    {
        return Err(unsupported("plot output must use the .svg extension"));
    }
    let mut options = Options::default();
    let mut sheet = SvgOptions::default();
    let mut layout_name = None;
    let mut page_override = false;
    while let Some(flag) = args.next() {
        match flag {
            "--page" => {
                page_override = true;
                sheet.page_width_mm = args.number("page width in mm")?;
                sheet.page_height_mm = args.number("page height in mm")?;
            }
            "--margin" => {
                page_override = true;
                sheet.margin_mm = args.number("margin in mm")?;
            }
            "--scale" => {
                page_override = true;
                sheet.scale = Some(args.number("paper mm per model unit")?);
            }
            "--color" => {
                page_override = true;
                sheet.monochrome = false;
            }
            "--layout" => layout_name = Some(args.required("saved layout name")?),
            "--overwrite" | "--force" => options.overwrite = true,
            "--allow-lossy" => options.allow_lossy = true,
            _ => return Err(input(format!("unknown plot option {flag}"))),
        }
    }
    reject_same_path(source, destination, options.overwrite)?;
    let loaded = load(source)?;
    let mut warnings = Vec::new();
    for loss in loaded.losses {
        authorize_loss(&options, &mut warnings, loss)?;
    }
    if layout_name.is_some() && page_override {
        return Err(input(
            "Saved layouts define their own page, scale and color settings; edit the layout before plotting",
        ));
    }
    let svg = if let Some(name) = layout_name {
        forge_core::plot::export_layout_svg(&loaded.document, name)?
    } else {
        export_svg(&loaded.document, sheet)?
    };
    write_output(destination, svg.as_bytes(), options.overwrite)?;
    report_written(destination, &warnings);
    Ok(())
}

fn primitive_command(args: &mut Arguments<'_>) -> io::Result<()> {
    let kind = match args.required("primitive kind")? {
        "box" => SolidKind::Box,
        "cylinder" => SolidKind::Cylinder,
        "sphere" => SolidKind::Sphere,
        value => {
            return Err(input(format!(
                "unknown primitive {value}; expected box, cylinder or sphere"
            )));
        }
    };
    let destination = Path::new(args.required("output path")?);
    let mut options = Options::default();
    let (mut size, mut center, mut name, mut units) =
        ([80.0, 60.0, 48.0], [0.0; 3], "Primitive".to_owned(), 0_u16);
    while let Some(flag) = args.next() {
        match flag {
            "--size" => size = args.vector("size")?,
            "--center" => center = args.vector("center")?,
            "--name" => name = args.required("primitive name")?.to_owned(),
            "--units" => {
                units = args
                    .required("unit code")?
                    .parse()
                    .map_err(|_| input("unit code must be an integer from 0 through 24"))?
            }
            _ => {
                if !options.parse(flag, args)? {
                    return Err(input(format!("unknown primitive option {flag}")));
                }
            }
        }
    }
    let mut document = Document::new(&name);
    let id = document.add_solid(&name, kind)?;
    let solid = document
        .solids
        .iter_mut()
        .find(|solid| solid.id == id)
        .ok_or_else(|| input(format!("solid {name} could not be created")))?;
    solid.size = size;
    solid.center = center;
    document.units = units;
    let mut warnings = Vec::new();
    let bytes = encode(
        &document,
        Format::from_path(destination)?,
        &options,
        &mut warnings,
    )?;
    write_output(destination, &bytes, options.overwrite)?;
    report_written(destination, &warnings);
    Ok(())
}

fn products_command(args: &mut Arguments<'_>) -> io::Result<()> {
    let mut query = None;
    let mut as_json = false;
    while let Some(value) = args.next() {
        if value == "--json" {
            as_json = true;
        } else if value.starts_with('-') {
            return Err(input(format!("unknown products option {value}")));
        } else if query.replace(value).is_some() {
            return Err(input("products accepts one optional search query"));
        }
    }
    let products = catalog::search(query.unwrap_or(""));
    if as_json {
        println!("{}", serde_json::to_string_pretty(&products)?);
    } else {
        println!("{} product targets", products.len());
        for product in products {
            println!(
                "{}\t{}\t{:?}\t{:?}",
                product.id, product.name, product.area, product.status
            );
        }
    }
    Ok(())
}

fn native_command(args: &mut Arguments<'_>) -> io::Result<()> {
    match args.next().unwrap_or("help") {
        "inspect" => native_inspect_command(args)?,
        "convert" => native_convert_command(args)?,
        "transform" => native_transform_command(args)?,
        "help" | "--help" | "-h" => print!("{NATIVE_HELP}"),
        command => {
            return Err(input(format!(
                "unknown native command {command}; run forge native --help"
            )));
        }
    }
    Ok(())
}

/// Opens a drawing through the native engine. DWG is read as binary native
/// data; DXF goes through the engine's own reader instead of the strict subset.
fn open_native(path: &Path) -> io::Result<NativeDrawing> {
    let bytes = fs::read(path)?;
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match extension.as_str() {
        "dxf" => NativeDrawing::from_dxf(&bytes),
        "dwg" | "dwt" | "dws" => NativeDrawing::from_dwg(&bytes),
        _ => Err(unsupported(format!(
            "native commands read .dwg and .dxf files, not {extension:?}"
        ))),
    }
}

fn parse_version(value: &str) -> io::Result<NativeVersion> {
    match value.trim().to_ascii_lowercase().as_str() {
        "r2000" | "2000" => Ok(NativeVersion::R2000),
        "r2018" | "2018" => Ok(NativeVersion::R2018),
        _ => Err(input("native version must be r2000 or r2018")),
    }
}

fn parse_handle(value: &str) -> io::Result<u64> {
    let value = value.trim();
    let radix = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"));
    let parsed = match radix {
        Some(hex) => u64::from_str_radix(hex, 16),
        None => value.parse::<u64>(),
    };
    parsed.map_err(|_| input(format!("invalid entity handle {value:?}")))
}

fn native_inspect_command(args: &mut Arguments<'_>) -> io::Result<()> {
    let path = Path::new(args.required("input path")?);
    let mut as_json = false;
    while let Some(flag) = args.next() {
        if flag == "--json" {
            as_json = true;
        } else {
            return Err(input(format!("unknown native inspect option {flag}")));
        }
    }
    let drawing = open_native(path)?;
    let summary = drawing.summary();
    if as_json {
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else {
        println!("{} ({})", path.display(), summary.version);
        println!(
            "{} entities; {} layers; {} block records",
            summary.entity_count,
            summary.layers.len(),
            summary.blocks.len()
        );
        for (kind, count) in &summary.entity_types {
            println!("  {kind}: {count}");
        }
        for entity in &summary.entities {
            println!(
                "  {:#X}  {:<12} {}",
                entity.handle, entity.kind, entity.layer
            );
        }
        for diagnostic in &summary.diagnostics {
            println!("Engine diagnostic: {diagnostic}");
        }
    }
    Ok(())
}

fn native_convert_command(args: &mut Arguments<'_>) -> io::Result<()> {
    let source = Path::new(args.required("input path")?).to_path_buf();
    let destination = Path::new(args.required("output path")?).to_path_buf();
    let mut version = None;
    let mut overwrite = false;
    while let Some(flag) = args.next() {
        match flag {
            "--version" => version = Some(parse_version(args.required("version")?)?),
            "--overwrite" => overwrite = true,
            _ => return Err(input(format!("unknown native convert option {flag}"))),
        }
    }
    reject_same_path(&source, &destination, overwrite)?;
    let drawing = open_native(&source)?;
    let extension = destination
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let bytes = match extension.as_str() {
        "dwg" | "dwt" | "dws" => drawing.write_dwg(version)?,
        "dxf" => {
            if version.is_some() {
                return Err(input("--version applies to DWG output only"));
            }
            drawing.write_dxf()?
        }
        _ => {
            return Err(unsupported(
                "native convert writes .dwg or .dxf output only",
            ));
        }
    };
    write_output(&destination, &bytes, overwrite)?;
    println!("Wrote {}", destination.display());
    Ok(())
}

fn native_transform_command(args: &mut Arguments<'_>) -> io::Result<()> {
    let source = Path::new(args.required("input path")?).to_path_buf();
    let destination = Path::new(args.required("output path")?).to_path_buf();
    let mut handles: Vec<u64> = Vec::new();
    let mut delta = [0.0; 3];
    let mut translated = false;
    let mut overwrite = false;
    while let Some(flag) = args.next() {
        match flag {
            "--handle" => handles.push(parse_handle(args.required("handle")?)?),
            "--translate" => {
                delta = [args.number("dx")?, args.number("dy")?, args.number("dz")?];
                translated = true;
            }
            "--overwrite" => overwrite = true,
            _ => {
                return Err(input(format!("unknown native transform option {flag}")));
            }
        }
    }
    if handles.is_empty() {
        return Err(input("supply at least one --handle to translate"));
    }
    if !translated {
        return Err(input("supply --translate DX DY DZ"));
    }
    reject_same_path(&source, &destination, overwrite)?;
    let mut drawing = open_native(&source)?;
    let moved = drawing.translate_primitives(&handles, delta)?;
    let bytes = drawing.write_dwg(None)?;
    write_output(&destination, &bytes, overwrite)?;
    println!(
        "Translated {moved} native entities; wrote {}",
        destination.display()
    );
    Ok(())
}

fn brep_command(args: &mut Arguments<'_>) -> io::Result<()> {
    match args.next().unwrap_or("help") {
        "inspect" => brep_inspect_command(args)?,
        "primitive" => brep_primitive_command(args)?,
        "convert" => brep_convert_command(args)?,
        "help" | "--help" | "-h" => print!("{BREP_HELP}"),
        command => {
            return Err(input(format!(
                "unknown brep command {command}; run forge brep --help"
            )));
        }
    }
    Ok(())
}

fn require_step_path(path: &Path) -> io::Result<()> {
    if matches!(
        path.extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        "step" | "stp"
    ) {
        Ok(())
    } else {
        Err(unsupported(
            "brep commands read and write .step or .stp files only",
        ))
    }
}

fn open_brep(path: &Path) -> io::Result<BrepModel> {
    require_step_path(path)?;
    if fs::metadata(path)?.len() > 64 * 1024 * 1024 {
        return Err(unsupported("STEP input exceeds the 64 MiB reader limit"));
    }
    BrepModel::from_step(&fs::read_to_string(path)?)
}

fn brep_inspect_command(args: &mut Arguments<'_>) -> io::Result<()> {
    let path = Path::new(args.required("STEP input path")?);
    let mut as_json = false;
    while let Some(flag) = args.next() {
        if flag == "--json" {
            as_json = true;
        } else {
            return Err(input(format!("unknown brep inspect option {flag}")));
        }
    }
    let model = open_brep(path)?;
    let stats = model.stats();
    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "format":"step_brep",
                "source_bytes":model.step_source().len(),
                "solids":1,
                "shells":stats.shells,
                "faces":stats.faces,
                "edges":stats.edges,
                "vertices":stats.vertices,
                "boundary_loops":stats.boundary_loops,
                "curved_edges":stats.curved_edges,
                "nonplanar_faces":stats.nonplanar_faces,
            }))?
        );
    } else {
        println!(
            "{} (checked STEP B-rep)\n1 closed solid; {} shells; {} faces; {} edges; {} vertices\n{} boundary loops; {} curved edges; {} nonplanar faces\n{} original source bytes retained",
            path.display(),
            stats.shells,
            stats.faces,
            stats.edges,
            stats.vertices,
            stats.boundary_loops,
            stats.curved_edges,
            stats.nonplanar_faces,
            model.step_source().len(),
        );
    }
    Ok(())
}

fn brep_primitive_command(args: &mut Arguments<'_>) -> io::Result<()> {
    let kind = args.required("B-rep primitive kind")?;
    let mut size = match kind {
        "box" => [80.0, 60.0, 48.0],
        "cylinder" => [60.0, 60.0, 80.0],
        value => {
            return Err(input(format!(
                "unknown B-rep primitive {value}; expected box or cylinder"
            )));
        }
    };
    let destination = Path::new(args.required("STEP output path")?);
    require_step_path(destination)?;
    let mut center = [0.0; 3];
    let mut overwrite = false;
    while let Some(flag) = args.next() {
        match flag {
            "--size" => size = args.vector("size")?,
            "--center" => center = args.vector("center")?,
            "--overwrite" | "--force" => overwrite = true,
            _ => return Err(input(format!("unknown brep primitive option {flag}"))),
        }
    }
    let model = match kind {
        "box" => forge_modeling::make_box(center, size)?,
        _ => forge_modeling::make_elliptic_cylinder(center, size)?,
    };
    write_output(destination, model.step_source().as_bytes(), overwrite)?;
    println!(
        "Wrote checked analytic STEP solid {}",
        destination.display()
    );
    Ok(())
}

fn brep_convert_command(args: &mut Arguments<'_>) -> io::Result<()> {
    let source = Path::new(args.required("STEP input path")?);
    let destination = Path::new(args.required("STEP output path")?);
    require_step_path(destination)?;
    let mut options = Options::default();
    let mut regenerate = false;
    while let Some(flag) = args.next() {
        match flag {
            "--regenerate" => regenerate = true,
            "--overwrite" | "--force" => options.overwrite = true,
            "--allow-lossy" | "--allow-loss" => options.allow_lossy = true,
            _ => return Err(input(format!("unknown brep convert option {flag}"))),
        }
    }
    reject_same_path(source, destination, options.overwrite)?;
    let mut warnings = Vec::new();
    if regenerate {
        authorize_loss(
            &options,
            &mut warnings,
            "STEP geometry regeneration replaces source metadata, names and units with generated exchange records",
        )?;
    }
    let model = open_brep(source)?;
    let regenerated;
    let result = if regenerate {
        regenerated = model.regenerated_geometry_step()?;
        regenerated.as_str()
    } else {
        model.step_source()
    };
    write_output(destination, result.as_bytes(), options.overwrite)?;
    report_written(destination, &warnings);
    Ok(())
}

fn authorize_loss(
    options: &Options,
    warnings: &mut Vec<String>,
    message: impl Into<String>,
) -> io::Result<()> {
    let message = message.into();
    if !options.allow_lossy {
        return Err(unsupported(format!(
            "{message}; pass --allow-lossy only if that loss is acceptable"
        )));
    }
    warnings.push(message);
    Ok(())
}
fn reject_same_path(input_path: &Path, output_path: &Path, overwrite: bool) -> io::Result<()> {
    if !overwrite
        && input_path
            .canonicalize()
            .ok()
            .zip(output_path.canonicalize().ok())
            .is_some_and(|(input, output)| input == output)
    {
        return Err(input(
            "input and output are the same file; --overwrite is required",
        ));
    }
    Ok(())
}

fn write_output(path: &Path, bytes: &[u8], overwrite: bool) -> io::Result<()> {
    forge_core::storage::write_file(path, bytes, overwrite)
}
fn report_written(path: &Path, warnings: &[String]) {
    for warning in warnings {
        eprintln!("Loss explicitly allowed: {warning}");
    }
    println!("Wrote {}", path.display());
}
fn input(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}
fn unsupported(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, message.into())
}
