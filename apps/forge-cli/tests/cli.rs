use forge_core::{Document, Geometry, Point, mesh::TriangleMesh};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
struct Files(PathBuf);
impl Files {
    fn new() -> Self {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/cli-tests")
            .join(format!(
                "{}-{}",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_forge"))
        .args(args)
        .output()
        .unwrap()
}
fn path(path: &Path) -> &str {
    path.to_str().unwrap()
}
fn success(output: Output) -> Output {
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}
fn drawing() -> &'static str {
    "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nLINE\n8\nWalls\n10\n0\n20\n0\n11\n10\n21\n0\n0\nCIRCLE\n8\nWalls\n10\n5\n20\n5\n40\n2\n0\nENDSEC\n0\nEOF\n"
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-8, "{a} != {b}");
}

#[test]
fn workbench_examples_round_trip_native_v5_with_all_semantic_domains() {
    let files = Files::new();
    let source = files.file("suite.forge.json");
    let copy = files.file("suite-copy.forge.json");
    success(run(&["workbench", "create", "all", path(&source)]));
    let source_bytes = fs::read(&source).unwrap();
    let envelope: Value = serde_json::from_slice(&source_bytes).unwrap();
    assert_eq!(envelope["format_version"], 5);
    let document = Document::load_json(&source).unwrap();
    assert!(document.has_workbench_data());
    assert!(!document.building.walls.is_empty());
    assert!(!document.building.openings.is_empty());
    assert!(!document.terrain.triangles.is_empty());
    assert!(!document.scene.objects.is_empty());
    assert!(!document.manufacturing.profiles.is_empty());
    assert!(!document.review.items.is_empty());
    success(run(&["convert", path(&source), path(&copy)]));
    assert_eq!(Document::load_json(copy).unwrap(), document);
    let inspected = success(run(&["workbench", "inspect", path(&source), "--json"]));
    let info: Value = serde_json::from_slice(&inspected.stdout).unwrap();
    assert_eq!(info["building"]["walls"], document.building.walls.len());
    assert_eq!(
        info["building"]["openings"],
        document.building.openings.len()
    );
    assert_eq!(info["terrain"]["points"], document.terrain.points.len());
    assert_eq!(info["scene"]["objects"], document.scene.objects.len());
    assert!(info["manufacturing"]["motions"].as_u64().unwrap() > 10);
    assert_eq!(fs::read(source).unwrap(), source_bytes);
}

#[test]
fn connect_review_lifecycle_and_csv_exchange_preserve_exact_native_records() {
    use forge_core::review::{ReviewPriority, ReviewStatus};
    let files = Files::new();
    let source = files.file("review.forge.json");
    success(run(&["workbench", "create", "connect", path(&source)]));
    let mut document = Document::load_json(&source).unwrap();
    assert_eq!(document.review.items.len(), 3);
    assert!(document.building.is_empty());
    assert!(document.scene.is_empty());
    document.review.items[0].status = ReviewStatus::Resolved;
    document.review.items[0].priority = ReviewPriority::High;
    document.review.items[0].description = "First review line\nDecision, with \"quotes\"".into();
    document.review.items[0].element_reference = "=A1".into();
    document.save_json(&source).unwrap();
    let before = fs::read(&source).unwrap();
    let output = files.file("review.csv");
    success(run(&[
        "workbench",
        "export",
        path(&source),
        path(&output),
        "--area",
        "connect",
    ]));
    let csv = fs::read_to_string(&output).unwrap();
    assert!(csv.starts_with("id,title,description,status,priority,element_reference\n"));
    assert!(csv.contains("\"Resolved\",\"High\""));
    assert!(csv.contains("\"First review line\nDecision, with \"\"quotes\"\"\""));
    assert!(csv.contains("\"'=A1\""));
    let inspected = success(run(&["inspect", path(&source), "--json"]));
    let info: Value = serde_json::from_slice(&inspected.stdout).unwrap();
    assert_eq!(info["workbenches"]["review_items"], 3);
    let copy = files.file("review-copy.forge.json");
    success(run(&["convert", path(&source), path(&copy)]));
    assert_eq!(Document::load_json(copy).unwrap().review, document.review);
    assert!(
        !run(&[
            "workbench",
            "export",
            path(&source),
            path(&output),
            "--area",
            "connect"
        ])
        .status
        .success()
    );
    assert_eq!(fs::read_to_string(output).unwrap(), csv);
    assert_eq!(fs::read(source).unwrap(), before);
}

#[test]
fn workbench_exports_derive_geometry_quantities_and_contours_without_changing_source() {
    let files = Files::new();
    let source = files.file("suite.forge.json");
    success(run(&["workbench", "create", "all", path(&source)]));
    let before = fs::read(&source).unwrap();
    let document = Document::load_json(&source).unwrap();
    let schedule = files.file("building.csv");
    success(run(&[
        "workbench",
        "export",
        path(&source),
        path(&schedule),
        "--area",
        "build",
    ]));
    let text = fs::read_to_string(schedule).unwrap();
    assert!(text.starts_with("id,kind,name,storey_id,host_id,gross_area_model2"));
    assert!(text.contains(",wall,"));
    assert!(text.contains(",door,") || text.contains(",window,"));
    let building = files.file("building.obj");
    success(run(&[
        "workbench",
        "export",
        path(&source),
        path(&building),
        "--area",
        "build",
    ]));
    let mesh = TriangleMesh::from_obj(&fs::read_to_string(building).unwrap()).unwrap();
    assert!(mesh.triangles.len() > 12);
    assert!(mesh.signed_volume().unwrap() > 0.0);
    let terrain = files.file("terrain.obj");
    success(run(&[
        "workbench",
        "export",
        path(&source),
        path(&terrain),
        "--area",
        "terrain",
    ]));
    assert_eq!(
        TriangleMesh::from_obj(&fs::read_to_string(terrain).unwrap())
            .unwrap()
            .triangles
            .len(),
        document.terrain.triangles.len()
    );
    let contours = files.file("contours.dxf");
    success(run(&[
        "workbench",
        "export",
        path(&source),
        path(&contours),
        "--area",
        "terrain",
        "--interval",
        "5",
    ]));
    let drawing = Document::import_dxf(contours).unwrap();
    assert!(!drawing.entities.is_empty());
    assert!(
        drawing
            .entities
            .iter()
            .all(|entity| matches!(entity.geometry, Geometry::Line { .. }))
    );
    assert_eq!(drawing.units, document.units);
    assert_eq!(fs::read(source).unwrap(), before);
}

#[test]
fn motion_frame_exports_evaluate_animation_in_actual_mesh_vertices() {
    let files = Files::new();
    let source = files.file("motion.forge.json");
    success(run(&["workbench", "create", "motion", path(&source)]));
    let mut document = Document::load_json(&source).unwrap();
    document.scene.objects.truncate(1);
    document.save_json(&source).unwrap();
    let before = fs::read(&source).unwrap();
    let first = files.file("first.obj");
    let last = files.file("last.obj");
    success(run(&[
        "workbench",
        "export",
        path(&source),
        path(&first),
        "--area",
        "motion",
        "--time",
        "0",
    ]));
    success(run(&[
        "workbench",
        "export",
        path(&source),
        path(&last),
        "--area",
        "motion",
        "--time",
        "5",
    ]));
    let first = TriangleMesh::from_obj(&fs::read_to_string(first).unwrap()).unwrap();
    let last = TriangleMesh::from_obj(&fs::read_to_string(last).unwrap()).unwrap();
    assert_eq!(first.triangles.len(), last.triangles.len());
    near(first.bounds().unwrap().0[0], -105.0);
    near(last.bounds().unwrap().0[0], 55.0);
    near(
        first.signed_volume().unwrap(),
        last.signed_volume().unwrap(),
    );
    assert_eq!(fs::read(source).unwrap(), before);
}

#[test]
fn manufacturing_program_retracts_before_xy_and_reaches_final_stepdown_depth() {
    let files = Files::new();
    let source = files.file("job.forge.json");
    let output = files.file("profile.nc");
    success(run(&["workbench", "create", "make", path(&source)]));
    let mut document = Document::load_json(&source).unwrap();
    document.manufacturing.profiles.truncate(1);
    document.manufacturing.depth = 7.0;
    document.manufacturing.step_down = 3.0;
    document.save_json(&source).unwrap();
    let before = fs::read(&source).unwrap();
    success(run(&[
        "workbench",
        "export",
        path(&source),
        path(&output),
        "--area",
        "make",
    ]));
    let program = fs::read_to_string(output).unwrap();
    assert!(program.contains("G17 G21 G90 G94 G40 G49 G80"));
    assert!(program.find("G0 Z5\n").unwrap() < program.find("G0 X").unwrap());
    assert!(program.contains(" Z-3 F100\n"));
    assert!(program.contains(" Z-6 F100\n"));
    assert!(program.contains(" Z-7 F100\n"));
    assert!(!program.contains(" Z-9 "));
    assert!(program.ends_with("M5\nM2\n"));
    let plan = document.manufacturing.plan().unwrap();
    assert_eq!(plan.passes, 3);
    for pair in plan.motions.windows(2) {
        if pair[1].rapid && pair[0].position[..2] != pair[1].position[..2] {
            assert_eq!(pair[0].position[2], document.manufacturing.safe_z);
            assert_eq!(pair[1].position[2], document.manufacturing.safe_z);
        }
    }
    assert_eq!(fs::read(source).unwrap(), before);
}

#[test]
fn survey_csv_import_interpolates_elevations_and_reimports_derived_points() {
    let files = Files::new();
    let survey = files.file("survey.csv");
    let source = files.file("surface.forge.json");
    fs::write(&survey, "x,y,z\n0,0,0\n10,0,10\n0,10,20\n10,10,30\n").unwrap();
    success(run(&["workbench", "survey", path(&survey), path(&source)]));
    let doc = Document::load_json(&source).unwrap();
    near(
        doc.terrain
            .elevation_at(Point::new(2.0, 3.0))
            .unwrap()
            .unwrap(),
        8.0,
    );
    assert_eq!(doc.terrain.triangles.len(), 2);
    assert_eq!(doc.units, 4);
    let mut doc = doc;
    doc.units = 6;
    doc.save_json(&source).unwrap();
    let exported = files.file("survey-copy.csv");
    let copy = files.file("surface-copy.forge.json");
    success(run(&[
        "workbench",
        "export",
        path(&source),
        path(&exported),
        "--area",
        "terrain",
    ]));
    success(run(&["workbench", "survey", path(&exported), path(&copy)]));
    let restored = Document::load_json(copy).unwrap();
    assert_eq!(restored.terrain, doc.terrain);
    assert_eq!(
        restored.units, 6,
        "Survey CSV metadata must retain metre units"
    );
}

#[test]
fn semantic_export_failures_do_not_write_or_replace_files_and_loss_is_explicit() {
    let files = Files::new();
    let source = files.file("source.forge.json");
    success(run(&["workbench", "create", "all", path(&source)]));
    let mut document = Document::load_json(&source).unwrap();
    document.add_geometry(Geometry::Line {
        start: Point::new(0.0, 0.0),
        end: Point::new(10.0, 5.0),
    });
    document.save_json(&source).unwrap();
    let before = fs::read(&source).unwrap();
    assert!(
        !run(&["workbench", "create", "make", path(&source)])
            .status
            .success()
    );
    assert_eq!(fs::read(&source).unwrap(), before);
    let output = files.file("contours.dxf");
    fs::write(&output, b"sentinel").unwrap();
    assert!(
        !run(&[
            "workbench",
            "export",
            path(&source),
            path(&output),
            "--area",
            "terrain",
            "--interval",
            "5"
        ])
        .status
        .success()
    );
    assert_eq!(fs::read(&output).unwrap(), b"sentinel");
    for interval in ["0", "-1", "NaN", "inf"] {
        assert!(
            !run(&[
                "workbench",
                "export",
                path(&source),
                path(&output),
                "--area",
                "terrain",
                "--interval",
                interval,
                "--force"
            ])
            .status
            .success()
        );
        assert_eq!(fs::read(&output).unwrap(), b"sentinel");
    }
    let neutral = files.file("drawing.dxf");
    assert!(
        !run(&["convert", path(&source), path(&neutral)])
            .status
            .success()
    );
    assert!(!neutral.exists());
    success(run(&[
        "convert",
        path(&source),
        path(&neutral),
        "--allow-lossy",
    ]));
    let retained = Document::import_dxf(neutral).unwrap();
    assert_eq!(retained.entities.len(), 1);
    assert!(!retained.has_workbench_data());
    let malformed = files.file("malformed.csv");
    let absent = files.file("absent.forge.json");
    fs::write(&malformed, "x,y,z\n0,0,0\n10,0,10\n0,10,bad\n").unwrap();
    assert!(
        !run(&["workbench", "survey", path(&malformed), path(&absent)])
            .status
            .success()
    );
    assert!(!absent.exists());
    assert_eq!(fs::read(source).unwrap(), before);
}

#[test]
fn drafting_script_preserves_command_order_selection_and_editable_exchange() {
    let files = Files::new();
    let script = files.file("walls.scr");
    fs::write(
        &script,
        "# Offset the selected rectangle\n\nOFFSET -5\nARRAYRECT 3 2 150 100\n",
    )
    .unwrap();
    let native = files.file("walls.forge.json");
    success(run(&[
        "draft",
        "-",
        path(&native),
        "--command",
        "LAYER Walls",
        "--command",
        "RECTANGLE 0 0 120 80",
        "--script",
        path(&script),
        "--command",
        "SELECT ALL",
        "--command",
        "LENGTH",
    ]));
    let document = Document::load_json(&native).unwrap();
    assert_eq!(document.entities.len(), 7);
    let walls = document
        .layers
        .iter()
        .find(|layer| layer.name == "Walls")
        .unwrap()
        .id;
    assert!(
        document
            .entities
            .iter()
            .all(|entity| entity.layer_id == walls)
    );
    assert!(
        matches!(document.entities[0].geometry, Geometry::Rectangle { a, b } if a == Point::new(0.0,0.0) && b == Point::new(120.0,80.0))
    );
    let dxf = files.file("walls.dxf");
    success(run(&[
        "draft",
        path(&native),
        path(&dxf),
        "--command",
        "SELECT ALL",
        "--command",
        "MOVE 10 20",
    ]));
    let reopened = Document::import_dxf(&dxf).unwrap();
    assert_eq!(reopened.entities.len(), 7);
    let bounds = reopened.drawing_bounds(false).unwrap();
    near(bounds.min.x, 10.0);
    near(bounds.min.y, 20.0);
    near(bounds.max.x, 425.0);
    near(bounds.max.y, 195.0);
    assert_eq!(Document::load_json(native).unwrap(), document);
}

#[test]
fn draft_failure_never_writes_partial_batches_or_replaces_unapproved_files() {
    let files = Files::new();
    let input = files.file("source.dxf");
    let output = files.file("existing.forge.json");
    let script = files.file("broken.scr");
    fs::write(&input, drawing()).unwrap();
    fs::write(&output, b"sentinel").unwrap();
    fs::write(&script, "SELECT ALL\nMOVE 10 20\nOFFSET NaN\n").unwrap();
    let failed = run(&[
        "draft",
        path(&input),
        path(&output),
        "--script",
        path(&script),
        "--overwrite",
    ]);
    assert!(!failed.status.success());
    assert!(String::from_utf8_lossy(&failed.stderr).contains("broken.scr:3"));
    assert_eq!(fs::read(&output).unwrap(), b"sentinel");
    assert_eq!(fs::read_to_string(&input).unwrap(), drawing());
    assert!(
        !run(&["draft", "-", path(&output), "--command", "LINE 0 0 10 0"])
            .status
            .success()
    );
    assert_eq!(fs::read(&output).unwrap(), b"sentinel");
    assert!(
        !run(&[
            "draft",
            path(&input),
            path(&input),
            "--command",
            "SELECT ALL",
            "--command",
            "ERASE"
        ])
        .status
        .success()
    );
    assert_eq!(fs::read_to_string(&input).unwrap(), drawing());
    assert!(
        !run(&["draft", "-", path(&output), "--overwrite"])
            .status
            .success()
    );
    assert!(
        !run(&[
            "draft",
            "-",
            path(&output),
            "--command",
            "   ",
            "--overwrite"
        ])
        .status
        .success()
    );
    success(run(&[
        "draft",
        "-",
        path(&output),
        "--command",
        "LINE 0 0 10 0",
        "--force",
    ]));
    assert_eq!(Document::load_json(output).unwrap().entities.len(), 1);
}

#[test]
fn plotting_produces_physical_svg_sheets_with_visible_layers_and_escaped_annotation() {
    let files = Files::new();
    let native = files.file("drawing.forge.json");
    let svg = files.file("drawing.svg");
    success(run(&[
        "draft",
        "-",
        path(&native),
        "--command",
        "LAYER Pipes",
        "--command",
        "RECTANGLE 0 0 20 10",
        "--command",
        "TEXT 1 2 2 0 <Valve & Pipe>",
    ]));
    let mut document = Document::load_json(&native).unwrap();
    let pipes = document.active_layer_id;
    document
        .layers
        .iter_mut()
        .find(|layer| layer.id == pipes)
        .unwrap()
        .color = [0x12, 0x34, 0x56];
    let hidden = document.add_layer("Hidden", [255, 0, 0]);
    document
        .layers
        .iter_mut()
        .find(|layer| layer.id == hidden)
        .unwrap()
        .visible = false;
    document.active_layer_id = hidden;
    document.add_geometry(Geometry::Circle {
        center: Point::new(10_000.0, 20_000.0),
        radius: 100.0,
    });
    document.save_json(&native).unwrap();
    success(run(&[
        "plot",
        path(&native),
        path(&svg),
        "--page",
        "200",
        "100",
        "--margin",
        "5",
        "--scale",
        "2",
        "--color",
    ]));
    let source = fs::read_to_string(&svg).unwrap();
    assert!(source.contains("xmlns=\"http://www.w3.org/2000/svg\""));
    assert!(source.contains("width=\"200mm\" height=\"100mm\""));
    assert!(source.contains("matrix(2 0 0 -2"));
    assert!(source.contains("stroke=\"#123456\""));
    assert!(source.contains("&lt;Valve &amp; Pipe&gt;"));
    assert!(source.contains("data-layer=\"Pipes\""));
    assert!(!source.contains("data-layer=\"Hidden\""));
    success(run(&["plot", path(&native), path(&svg), "--overwrite"]));
    let monochrome = fs::read_to_string(&svg).unwrap();
    assert!(monochrome.contains("width=\"297mm\" height=\"210mm\""));
    assert!(monochrome.contains("stroke=\"#111111\""));
    assert_eq!(Document::load_json(native).unwrap(), document);
}

#[test]
fn invalid_plot_settings_or_3d_input_preserve_existing_output() {
    let files = Files::new();
    let input = files.file("source.dxf");
    let svg = files.file("existing.svg");
    fs::write(&input, drawing()).unwrap();
    fs::write(&svg, b"sentinel").unwrap();
    assert!(!run(&["plot", path(&input), path(&svg)]).status.success());
    for settings in [
        vec!["--page", "0", "100"],
        vec!["--margin", "500"],
        vec!["--scale", "0"],
        vec!["--scale", "1000000"],
    ] {
        let mut args = vec!["plot", path(&input), path(&svg), "--overwrite"];
        args.extend(settings);
        assert!(!run(&args).status.success());
        assert_eq!(fs::read(&svg).unwrap(), b"sentinel");
    }
    let model = files.file("model.forge.json");
    success(run(&["primitive", "box", path(&model)]));
    let failed = run(&[
        "plot",
        path(&model),
        path(&svg),
        "--overwrite",
        "--allow-lossy",
    ]);
    assert!(!failed.status.success());
    assert!(String::from_utf8_lossy(&failed.stderr).contains("2D"));
    assert_eq!(fs::read(&svg).unwrap(), b"sentinel");
    let wrong_extension = files.file("fake.pdf");
    assert!(
        !run(&["plot", path(&input), path(&wrong_extension)])
            .status
            .success()
    );
    assert!(!wrong_extension.exists());
}

#[test]
fn analytic_step_box_is_inspectable_and_default_exchange_preserves_source_bytes() {
    let files = Files::new();
    let original = files.file("box.step");
    let copied = files.file("copy.stp");
    success(run(&[
        "brep",
        "primitive",
        "box",
        path(&original),
        "--size",
        "20",
        "12",
        "8",
        "--center",
        "4",
        "-3",
        "2",
    ]));
    let bytes = fs::read_to_string(&original).unwrap();
    assert!(bytes.contains("MANIFOLD_SOLID_BREP"));
    assert!(!bytes.contains("TRIANGULATED_FACE_SET"));
    // Preserve original header metadata and line endings, rather than merely
    // comparing tessellations or accepting a successful parse as a round trip.
    let bytes = bytes
        .replace("Forge Craft box", "Original design revision C")
        .replace('\n', "\r\n");
    fs::write(&original, &bytes).unwrap();
    let inspect = success(run(&["brep", "inspect", path(&original), "--json"]));
    let summary: Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(summary["format"], "step_brep");
    assert_eq!(summary["solids"], 1);
    assert_eq!(summary["shells"], 1);
    assert_eq!(summary["faces"], 6);
    assert_eq!(summary["edges"], 12);
    assert_eq!(summary["vertices"], 8);
    assert_eq!(summary["curved_edges"], 0);
    assert_eq!(summary["nonplanar_faces"], 0);
    assert_eq!(summary["source_bytes"], bytes.len());
    success(run(&["brep", "convert", path(&original), path(&copied)]));
    assert_eq!(fs::read(&copied).unwrap(), bytes.as_bytes());
    assert_eq!(fs::read(&original).unwrap(), bytes.as_bytes());
}

#[test]
fn analytic_step_cylinder_keeps_curves_and_requires_regeneration_loss_authorization() {
    let files = Files::new();
    let cylinder = files.file("elliptic.step");
    let regenerated = files.file("generated.step");
    success(run(&[
        "brep",
        "primitive",
        "cylinder",
        path(&cylinder),
        "--size",
        "20",
        "12",
        "16",
        "--center",
        "3",
        "2",
        "-1",
    ]));
    let bytes = fs::read_to_string(&cylinder).unwrap();
    assert!(bytes.contains("RATIONAL_B_SPLINE_CURVE"));
    assert!(bytes.contains("RATIONAL_B_SPLINE_SURFACE"));
    let inspect = success(run(&["brep", "inspect", path(&cylinder), "--json"]));
    let before: Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert!(before["curved_edges"].as_u64().unwrap() > 0);
    assert!(before["nonplanar_faces"].as_u64().unwrap() > 0);
    let failed = run(&[
        "brep",
        "convert",
        path(&cylinder),
        path(&regenerated),
        "--regenerate",
    ]);
    assert!(!failed.status.success());
    assert!(String::from_utf8_lossy(&failed.stderr).contains("--allow-lossy"));
    assert!(!regenerated.exists());
    success(run(&[
        "brep",
        "convert",
        path(&cylinder),
        path(&regenerated),
        "--regenerate",
        "--allow-loss",
    ]));
    let inspect = success(run(&["brep", "inspect", path(&regenerated), "--json"]));
    let after: Value = serde_json::from_slice(&inspect.stdout).unwrap();
    for property in [
        "shells",
        "faces",
        "edges",
        "vertices",
        "boundary_loops",
        "curved_edges",
        "nonplanar_faces",
    ] {
        assert_eq!(before[property], after[property], "changed {property}");
    }
    assert_eq!(fs::read_to_string(cylinder).unwrap(), bytes);
}

#[test]
fn step_output_is_never_replaced_without_authorization_or_after_failed_validation() {
    let files = Files::new();
    let source = files.file("box.step");
    let destination = files.file("existing.step");
    success(run(&["brep", "primitive", "box", path(&source)]));
    let original = fs::read(&source).unwrap();
    fs::write(&destination, b"sentinel").unwrap();
    assert!(
        !run(&["brep", "convert", path(&source), path(&destination)])
            .status
            .success()
    );
    assert_eq!(fs::read(&destination).unwrap(), b"sentinel");
    assert!(
        !run(&["brep", "convert", path(&source), path(&source)])
            .status
            .success()
    );
    assert_eq!(fs::read(&source).unwrap(), original);
    success(run(&[
        "brep",
        "convert",
        path(&source),
        path(&destination),
        "--force",
    ]));
    assert_eq!(fs::read(&destination).unwrap(), original);
    fs::write(&source, b"not a STEP exchange file").unwrap();
    assert!(
        !run(&[
            "brep",
            "convert",
            path(&source),
            path(&destination),
            "--overwrite"
        ])
        .status
        .success()
    );
    assert_eq!(fs::read(&destination).unwrap(), original);
    assert!(
        !run(&[
            "brep",
            "primitive",
            "box",
            path(&destination),
            "--size",
            "0",
            "1",
            "1",
            "--overwrite"
        ])
        .status
        .success()
    );
    assert_eq!(fs::read(&destination).unwrap(), original);
    assert_eq!(fs::read_dir(&files.0).unwrap().count(), 2);
}

#[test]
fn step_commands_reject_unsupported_formats_open_shells_and_invalid_parameters() {
    let files = Files::new();
    let source = files.file("box.step");
    success(run(&["brep", "primitive", "box", path(&source)]));
    let bytes = fs::read_to_string(&source).unwrap();
    fs::write(&source, bytes.replace("CLOSED_SHELL", "OPEN_SHELL")).unwrap();
    assert!(!run(&["brep", "inspect", path(&source)]).status.success());
    let fake = files.file("fake.ipt");
    assert!(
        !run(&["brep", "primitive", "box", path(&fake)])
            .status
            .success()
    );
    assert!(!fake.exists());
    let invalid = files.file("invalid.step");
    for size in ["0", "-1", "NaN", "inf", "0.000000001"] {
        assert!(
            !run(&[
                "brep",
                "primitive",
                "cylinder",
                path(&invalid),
                "--size",
                size,
                "10",
                "10"
            ])
            .status
            .success()
        );
        assert!(!invalid.exists());
    }
    assert!(
        !run(&["brep", "primitive", "sphere", path(&invalid)])
            .status
            .success()
    );
    assert!(
        !run(&["brep", "inspect", path(&source), "--unknown"])
            .status
            .success()
    );
    let help = success(run(&["brep", "--help"]));
    assert!(String::from_utf8_lossy(&help.stdout).contains("one shell"));
}

#[test]
fn all_individual_product_targets_are_available_to_automation() {
    let output = success(run(&["products", "--json"]));
    let products: Value = serde_json::from_slice(&output.stdout).unwrap();
    let products = products.as_array().unwrap();
    assert_eq!(products.len(), 140);
    for name in [
        "AutoCAD", "Revit", "Inventor", "Fusion", "Maya", "3ds Max", "Civil 3D",
    ] {
        assert!(products.iter().any(|p| p["name"] == name), "missing {name}");
    }
}

#[test]
fn drawing_edit_and_exchange_workflow_preserves_units_layers_and_coordinates() {
    let files = Files::new();
    let input = files.file("source.dxf");
    let native = files.file("saved.forge.json");
    let result = files.file("moved.dxf");
    fs::write(&input, drawing()).unwrap();
    success(run(&["convert", path(&input), path(&native)]));
    let document = Document::load_json(&native).unwrap();
    assert_eq!(document.units, 4);
    assert_eq!(document.entities.len(), 2);
    assert!(document.layers.iter().any(|l| l.name == "Walls"));
    success(run(&[
        "transform",
        path(&native),
        path(&result),
        "--translate",
        "10",
        "20",
        "--rotate",
        "90",
    ]));
    let document = Document::import_dxf(&result).unwrap();
    assert_eq!(document.units, 4);
    if let Geometry::Line { start, end } = document.entities[0].geometry {
        near(start.x, -20.0);
        near(start.y, 10.0);
        near(end.x, -20.0);
        near(end.y, 20.0);
    } else {
        panic!("line type lost");
    }
    if let Geometry::Circle { center, radius } = document.entities[1].geometry {
        near(center.x, -25.0);
        near(center.y, 15.0);
        near(radius, 2.0);
    } else {
        panic!("circle type lost");
    }
    assert_eq!(fs::read_to_string(input).unwrap(), drawing());
}

#[test]
fn native_solids_export_and_import_as_real_mesh_geometry() {
    let files = Files::new();
    let native = files.file("sphere.forge.json");
    let stl = files.file("sphere.stl");
    let imported = files.file("mesh.forge.json");
    success(run(&[
        "primitive",
        "sphere",
        path(&native),
        "--size",
        "20",
        "30",
        "40",
        "--center",
        "10",
        "20",
        "30",
    ]));
    assert!(
        !run(&["convert", path(&native), path(&stl)])
            .status
            .success()
    );
    assert!(!stl.exists());
    success(run(&[
        "convert",
        path(&native),
        path(&stl),
        "--ascii",
        "--allow-lossy",
    ]));
    let mesh = TriangleMesh::read_stl(&stl).unwrap();
    assert!(mesh.triangles.len() > 100);
    let (min, max) = mesh.bounds().unwrap();
    near(min[0], 0.0);
    near(max[0], 20.0);
    near(min[1], 5.0);
    near(max[1], 35.0);
    near(min[2], 10.0);
    near(max[2], 50.0);
    success(run(&["convert", path(&stl), path(&imported)]));
    let doc = Document::load_json(imported).unwrap();
    assert_eq!(doc.meshes.len(), 1);
    assert_eq!(doc.meshes[0].mesh.triangles.len(), mesh.triangles.len());
}

#[test]
fn destination_and_original_data_survive_failed_or_unapproved_operations() {
    let files = Files::new();
    let input = files.file("source.dxf");
    let output = files.file("existing.forge.json");
    fs::write(&input, drawing()).unwrap();
    fs::write(&output, b"sentinel").unwrap();
    assert!(
        !run(&["convert", path(&input), path(&output)])
            .status
            .success()
    );
    assert_eq!(fs::read(&output).unwrap(), b"sentinel");
    success(run(&[
        "convert",
        path(&input),
        path(&output),
        "--overwrite",
    ]));
    assert_eq!(Document::load_json(&output).unwrap().entities.len(), 2);
    let original = fs::read(&input).unwrap();
    assert!(
        !run(&[
            "transform",
            path(&input),
            path(&input),
            "--translate",
            "3",
            "4"
        ])
        .status
        .success()
    );
    assert_eq!(fs::read(&input).unwrap(), original);
    fs::write(&input, "0\nSECTION\n2\nENTITIES\n0\nCIRCLE\n10\nNaN\n").unwrap();
    let saved = fs::read(&output).unwrap();
    assert!(
        !run(&["convert", path(&input), path(&output), "--overwrite"])
            .status
            .success()
    );
    assert_eq!(fs::read(&output).unwrap(), saved);
}

#[test]
fn obj_metadata_requires_explicit_loss_authorization() {
    let files = Files::new();
    let input = files.file("textured.obj");
    let native = files.file("mesh.forge.json");
    fs::write(&input,"v 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0 0\nvt 1 0\nvt 0 1\nvn 0 0 1\nusemtl painted\nf 1/1/1 2/2/1 3/3/1\n").unwrap();
    let inspect = success(run(&["inspect", path(&input), "--json"]));
    let json: Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert!(!json["conversion_losses"].as_array().unwrap().is_empty());
    assert!(
        !run(&["convert", path(&input), path(&native)])
            .status
            .success()
    );
    assert!(!native.exists());
    success(run(&[
        "convert",
        path(&input),
        path(&native),
        "--allow-lossy",
    ]));
    assert_eq!(
        Document::load_json(native).unwrap().meshes[0]
            .mesh
            .triangles
            .len(),
        1
    );
}

#[test]
fn unsupported_native_formats_and_mixed_scene_transforms_are_refused() {
    let files = Files::new();
    let native = files.file("drawing.dwg");
    let output = files.file("fake.json");
    fs::write(&native, b"AC1032").unwrap();
    let failed = run(&["convert", path(&native), path(&output)]);
    assert!(!failed.status.success());
    assert!(String::from_utf8_lossy(&failed.stderr).contains("not implemented"));
    assert!(!output.exists());
    let scene = files.file("mixed.forge.json");
    let mut doc = Document::default();
    doc.add_geometry(Geometry::Line {
        start: Point::default(),
        end: Point::new(10.0, 0.0),
    });
    doc.add_solid("Part", forge_core::SolidKind::Box);
    doc.save_json(&scene).unwrap();
    assert!(
        !run(&[
            "transform",
            path(&scene),
            path(&output),
            "--translate",
            "2",
            "3"
        ])
        .status
        .success()
    );
    assert!(!output.exists());
}

fn native_drawing() -> &'static str {
    "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1015\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nLINE\n8\n0\n10\n1.25\n20\n2.5\n30\n3.75\n11\n10.5\n21\n20.25\n31\n30.75\n0\nCIRCLE\n8\n0\n10\n20\n20\n30\n30\n0\n40\n5\n0\nENDSEC\n0\nEOF\n"
}

#[test]
fn native_dwg_round_trip_and_translation_preserve_the_drawing_inventory() {
    let files = Files::new();
    let input = files.file("source.dxf");
    let drawing = files.file("roundtrip.dwg");
    let modern = files.file("modern.dwg");
    let moved = files.file("translated.dwg");
    fs::write(&input, native_drawing()).unwrap();
    success(run(&[
        "native",
        "convert",
        path(&input),
        path(&drawing),
        "--version",
        "r2000",
    ]));
    assert_eq!(&fs::read(&drawing).unwrap()[..6], b"AC1015");
    success(run(&[
        "native",
        "convert",
        path(&drawing),
        path(&modern),
        "--version",
        "r2018",
    ]));
    assert_eq!(&fs::read(&modern).unwrap()[..6], b"AC1032");
    let inspect = success(run(&["native", "inspect", path(&drawing), "--json"]));
    let summary: Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(summary["entity_count"], 2);
    assert_eq!(summary["entity_types"]["LINE"], 1);
    assert_eq!(summary["entity_types"]["CIRCLE"], 1);
    assert!(
        summary["layers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|layer| layer == "0")
    );
    // Read-back warnings are inherent to the engine; severe diagnostics are not,
    // because write_dwg refuses to save a drawing the engine could not recover.
    for diagnostic in summary["diagnostics"].as_array().unwrap() {
        let text = diagnostic.as_str().unwrap();
        assert!(
            !text.contains("Error")
                && !text.contains("NotImplemented")
                && !text.contains("NotSupported"),
            "severe engine diagnostic: {text}"
        );
    }
    let line = summary["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entity| entity["kind"] == "LINE")
        .unwrap();
    let handle = line["handle"].as_u64().unwrap().to_string();
    success(run(&[
        "native",
        "transform",
        path(&drawing),
        path(&moved),
        "--handle",
        &handle,
        "--translate",
        "10",
        "20",
        "30",
    ]));
    let again = success(run(&["native", "inspect", path(&moved), "--json"]));
    let reloaded: Value = serde_json::from_slice(&again.stdout).unwrap();
    assert_eq!(reloaded["entity_count"], 2);
    assert_eq!(reloaded["entity_types"], summary["entity_types"]);
    assert_eq!(reloaded["layers"], summary["layers"]);
}

#[test]
fn native_commands_refuse_unreadable_input_and_unapproved_destinations() {
    let files = Files::new();
    let broken = files.file("broken.dwg");
    let output = files.file("out.dwg");
    fs::write(&broken, b"AC1032").unwrap();
    assert!(!run(&["native", "inspect", path(&broken)]).status.success());
    assert!(!output.exists());
    let input = files.file("source.dxf");
    fs::write(&input, native_drawing()).unwrap();
    fs::write(&output, b"sentinel").unwrap();
    assert!(
        !run(&["native", "convert", path(&input), path(&output)])
            .status
            .success()
    );
    assert_eq!(fs::read(&output).unwrap(), b"sentinel");
    success(run(&[
        "native",
        "convert",
        path(&input),
        path(&output),
        "--overwrite",
    ]));
    assert!(fs::metadata(&output).unwrap().len() > 6);
    let bad = files.file("bad.dwg");
    assert!(
        !run(&[
            "native",
            "transform",
            path(&output),
            path(&bad),
            "--handle",
            "0xFFFF",
            "--translate",
            "1",
            "2",
            "3",
        ])
        .status
        .success()
    );
    assert!(!bad.exists());
    let no_translation = files.file("untranslated.dwg");
    assert!(
        !run(&[
            "native",
            "transform",
            path(&output),
            path(&no_translation),
            "--handle",
            "1",
        ])
        .status
        .success()
    );
    assert!(!no_translation.exists());
    let unsupported = files.file("fake.max");
    assert!(
        !run(&["native", "convert", path(&input), path(&unsupported)])
            .status
            .success()
    );
    assert!(!unsupported.exists());
    assert!(
        !run(&[
            "native",
            "convert",
            path(&input),
            path(&files.file("versioned.dxf")),
            "--version",
            "r2018",
        ])
        .status
        .success()
    );
}
