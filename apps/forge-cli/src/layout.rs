//! Saved sheet authoring; all writes retain the complete native project.
use super::*;
use forge_core::plot::{PaperText, PrintLayout, Viewport};

const HELP: &str = "Saved print layouts

  forge layout list INPUT.forge.json [--json]
  forge layout create INPUT OUTPUT.forge.json --name NAME [--page WIDTH HEIGHT] [--margin MM] [--scale FACTOR] [--color] [--overwrite]
  forge layout viewport INPUT OUTPUT.forge.json --layout NAME --name VIEW --paper X Y WIDTH HEIGHT --center X Y --scale FACTOR [--rotation DEG] [--hide-layer ID] [--no-border] [--overwrite]
  forge layout note INPUT OUTPUT.forge.json --layout NAME --text TEXT --position X Y [--height MM] [--overwrite]
  forge layout remove INPUT OUTPUT.forge.json --name NAME [--overwrite]
  forge plot INPUT OUTPUT.svg --layout NAME [--overwrite]

Create adds a fitted sheet. Viewport adds or updates a named frame. Note appends
paper text. Paper positions/sizes are mm from the top left; model Y points up.
Scales are paper mm per model unit; rotation turns the model view clockwise
inside its axis-aligned frame. Frames clip intentionally. Hidden layers
are specific to their viewport. Existing output requires --overwrite/--force.
";

pub(super) fn run(args: &mut Arguments<'_>) -> io::Result<()> {
    let command = args.next().unwrap_or("--help");
    if matches!(command, "--help" | "help" | "-h") {
        print!("{HELP}");
        return Ok(());
    }
    if !matches!(command, "list" | "create" | "viewport" | "note" | "remove") {
        return Err(input("Unknown layout command; use forge layout --help"));
    }
    let source = Path::new(args.required("native input")?);
    let mut document = Document::load_json(source)?;
    if command == "list" {
        let json_output = match args.next() {
            None => false,
            Some("--json") => true,
            _ => return Err(input("Only --json is supported for layout list")),
        };
        if args.next().is_some() {
            return Err(input("Unexpected layout list arguments"));
        }
        if json_output {
            println!("{}", serde_json::to_string_pretty(&document.layouts)?);
        } else {
            for layout in &document.layouts {
                println!(
                    "{}: {} × {} mm, {} viewports, {} notes",
                    layout.name,
                    layout.options.page_width_mm,
                    layout.options.page_height_mm,
                    layout.viewports.len(),
                    layout.notes.len()
                );
            }
        }
        return Ok(());
    }
    let destination = Path::new(args.required("native output")?);
    if !matches!(Format::from_path(destination)?, Format::Native) {
        return Err(input("Layouts require .forge.json output"));
    }
    let mut name = None;
    let mut layout_name = None;
    let mut options = SvgOptions::default();
    let mut paper = None;
    let mut center = None;
    let mut border = true;
    let mut rotation = 0.0;
    let mut hidden = Vec::new();
    let mut text = None;
    let mut position = None;
    let mut height = 3.5;
    let mut overwrite = false;
    while let Some(flag) = args.next() {
        match (command, flag) {
            (_, "--overwrite" | "--force") => overwrite = true,
            ("create" | "remove" | "viewport", "--name") => {
                name = Some(args.required("name")?.to_string())
            }
            ("viewport" | "note", "--layout") => layout_name = Some(args.required("layout name")?),
            ("create", "--page") => {
                options.page_width_mm = args.number("page width")?;
                options.page_height_mm = args.number("page height")?;
            }
            ("create", "--margin") => options.margin_mm = args.number("page margin")?,
            ("create", "--color") => options.monochrome = false,
            ("create" | "viewport", "--scale") => {
                options.scale = Some(args.number("paper mm/model unit")?)
            }
            ("viewport", "--paper") => {
                paper = Some([
                    args.number("paper x")?,
                    args.number("paper y")?,
                    args.number("paper width")?,
                    args.number("paper height")?,
                ])
            }
            ("viewport", "--center") => {
                center = Some(Point::new(
                    args.number("model center x")?,
                    args.number("model center y")?,
                ))
            }
            ("viewport", "--rotation") => rotation = args.number("view rotation degrees")?,
            ("viewport", "--hide-layer") => hidden.push(
                args.required("layer ID")?
                    .parse::<u64>()
                    .map_err(|_| input("Layer ID must be a positive integer"))?,
            ),
            ("viewport", "--no-border") => border = false,
            ("note", "--text") => text = Some(args.required("note text")?.to_string()),
            ("note", "--position") => {
                position = Some(Point::new(args.number("paper x")?, args.number("paper y")?))
            }
            ("note", "--height") => height = args.number("text height mm")?,
            _ => return Err(input(format!("Unsupported {command} option {flag}"))),
        }
    }
    reject_same_path(source, destination, overwrite)?;
    match command {
        "create" => document.layouts.push(PrintLayout::fitted(
            name.ok_or_else(|| input("--name is required"))?,
            &document,
            options,
        )?),
        "remove" => {
            let name = name.ok_or_else(|| input("--name is required"))?;
            let before = document.layouts.len();
            document
                .layouts
                .retain(|l| !l.name.eq_ignore_ascii_case(&name));
            if document.layouts.len() == before {
                return Err(input("Print layout does not exist"));
            }
        }
        "viewport" | "note" => {
            let name_of_layout = layout_name.ok_or_else(|| input("--layout is required"))?;
            let layout = document
                .layouts
                .iter_mut()
                .find(|l| l.name.eq_ignore_ascii_case(name_of_layout))
                .ok_or_else(|| input("Print layout does not exist"))?;
            if command == "viewport" {
                let name = name.ok_or_else(|| input("--name is required"))?;
                let [x, y, w, h] =
                    paper.ok_or_else(|| input("--paper X Y WIDTH HEIGHT is required"))?;
                hidden.sort_unstable();
                hidden.dedup();
                let viewport = Viewport {
                    name: name.clone(),
                    paper_position: Point::new(x, y),
                    paper_size: [w, h],
                    model_center: center.ok_or_else(|| input("--center X Y is required"))?,
                    scale: options.scale.ok_or_else(|| input("--scale is required"))?,
                    rotation_deg: rotation,
                    border,
                    hidden_layer_ids: hidden,
                };
                if let Some(saved) = layout
                    .viewports
                    .iter_mut()
                    .find(|v| v.name.eq_ignore_ascii_case(&name))
                {
                    *saved = viewport;
                } else {
                    layout.viewports.push(viewport);
                }
            } else {
                layout.notes.push(PaperText {
                    position: position.ok_or_else(|| input("--position is required"))?,
                    text: text.ok_or_else(|| input("--text is required"))?,
                    height_mm: height,
                });
            }
        }
        _ => unreachable!(),
    }
    write_output(destination, document.to_json()?.as_bytes(), overwrite)?;
    report_written(destination, &[]);
    Ok(())
}
