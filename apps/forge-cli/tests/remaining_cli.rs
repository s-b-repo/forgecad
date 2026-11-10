use forge_core::{Document, Geometry, Point, scene::Scene, terrain::Terrain};
use serde_json::{Value, json};
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
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/feature-cli-tests")
            .join(format!(
                "{}-{}",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(&dir).unwrap();
        Self(dir.canonicalize().unwrap())
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
fn p(path: &Path) -> &str {
    path.to_str().unwrap()
}
fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_forge"))
        .args(args)
        .output()
        .unwrap()
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
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-8, "{a} != {b}");
}

#[test]
fn project_scripts_author_parented_animation_pockets_and_drilling_without_modifying_source() {
    let f = Files::new();
    let input = f.file("input.forge.json");
    let output = f.file("edited.forge.json");
    let script = f.file("edits.json");
    let mut document = Document::default();
    document.scene = Scene::example();
    document.building = forge_core::building::Building::example();
    document.review = forge_core::review::ProjectReview::example();
    document.save_json(&input).unwrap();
    let source = fs::read(&input).unwrap();
    let edits = json!([
        {"type":"scene_parent","object_id":2,"parent_id":1},
        {"type":"cam_setup","settings":{"depth":2.5,"step_down":1,"step_over":1.2}},
        {"type":"cam_operation","profile":{"id":1,"name":"Pocket","operation":"pocket","compensation":"inside","geometry":{"type":"circle","center":{"x":20,"y":20},"radius":8},"islands":[{"type":"circle","center":{"x":20,"y":20},"radius":2}]}} ,
        {"type":"cam_operation","profile":{"id":2,"name":"Drill","operation":"drill","compensation":"center","geometry":{"type":"point","position":{"x":50,"y":40}}}}
    ]);
    fs::write(&script, serde_json::to_vec(&edits).unwrap()).unwrap();
    success(run(&[
        "workbench",
        "apply",
        p(&input),
        p(&output),
        "--script",
        p(&script),
    ]));
    let saved = Document::load_json(&output).unwrap();
    assert_eq!(saved.building, document.building);
    assert_eq!(saved.review, document.review);
    assert_eq!(saved.scene.objects[1].parent_id, Some(1));
    assert_eq!(saved.manufacturing.profiles[0].islands.len(), 1);
    let info: Value = serde_json::from_slice(
        &success(run(&["workbench", "inspect", p(&output), "--json"])).stdout,
    )
    .unwrap();
    assert_eq!(info["manufacturing"]["pockets"], 1);
    assert_eq!(info["manufacturing"]["drills"], 1);
    assert_eq!(info["manufacturing"]["islands"], 1);
    assert_eq!(info["scene"]["parented_objects"], 1);
    let program = f.file("machining.nc");
    success(run(&[
        "workbench",
        "export",
        p(&output),
        p(&program),
        "--area",
        "make",
    ]));
    assert!(
        fs::read_to_string(program)
            .unwrap()
            .contains("G1 X50 Y40 Z-2.5")
    );
    let frame = f.file("frame.obj");
    success(run(&[
        "workbench",
        "export",
        p(&output),
        p(&frame),
        "--area",
        "motion",
        "--time",
        "2.5",
    ]));
    assert!(
        !forge_core::mesh::TriangleMesh::from_obj(&fs::read_to_string(frame).unwrap())
            .unwrap()
            .triangles
            .is_empty()
    );
    assert_eq!(fs::read(&input).unwrap(), source);
    let edited = fs::read(&output).unwrap();
    assert!(
        !run(&[
            "workbench",
            "apply",
            p(&input),
            p(&output),
            "--script",
            p(&script)
        ])
        .status
        .success()
    );
    assert_eq!(fs::read(&output).unwrap(), edited);
}

#[test]
fn invalid_project_script_is_atomic_even_with_overwrite_authorization() {
    let f = Files::new();
    let input = f.file("input.forge.json");
    let output = f.file("protected.forge.json");
    let script = f.file("invalid.json");
    let mut document = Document::default();
    document.scene = Scene::example();
    document.save_json(&input).unwrap();
    let source = fs::read(&input).unwrap();
    fs::write(&output, b"protected").unwrap();
    fs::write(&script,r#"[{"type":"scene_parent","object_id":2,"parent_id":1},{"type":"scene_parent","object_id":1,"parent_id":2}]"#).unwrap();
    let failed = run(&[
        "workbench",
        "apply",
        p(&input),
        p(&output),
        "--script",
        p(&script),
        "--overwrite",
    ]);
    assert!(!failed.status.success());
    assert!(String::from_utf8_lossy(&failed.stderr).contains("edit 2"));
    assert_eq!(fs::read(&output).unwrap(), b"protected");
    assert_eq!(fs::read(&input).unwrap(), source);
    let edits = fs::read(&script).unwrap();
    assert!(
        !run(&[
            "workbench",
            "apply",
            p(&input),
            p(&script),
            "--script",
            p(&script),
            "--overwrite"
        ])
        .status
        .success()
    );
    assert_eq!(fs::read(&script).unwrap(), edits);
    fs::write(
        &script,
        r#"[{"type":"scene_parent","object_id":2,"parent_id":1,"unexpected":1}]"#,
    )
    .unwrap();
    let absent = f.file("absent.forge.json");
    assert!(
        !run(&[
            "workbench",
            "apply",
            p(&input),
            p(&absent),
            "--script",
            p(&script)
        ])
        .status
        .success()
    );
    assert!(!absent.exists());
}

#[test]
fn civil_queries_and_csv_reports_keep_units_stationing_and_exact_quantities() {
    let f = Files::new();
    let input = f.file("terrain.forge.json");
    let output = f.file("aligned.forge.json");
    let script = f.file("alignment.json");
    let mut document = Document::default();
    document.units = 6;
    document.terrain = Terrain::triangulate(vec![
        [0.0, 0.0, 0.0],
        [10.0, 0.0, 10.0],
        [0.0, 10.0, 20.0],
        [10.0, 10.0, 30.0],
    ])
    .unwrap();
    document.save_json(&input).unwrap();
    fs::write(&script,r#"[{"type":"alignment","alignment":{"id":1,"name":"Road","start_station":1000,"vertices":[{"x":0,"y":5},{"x":10,"y":5}]}}]"#).unwrap();
    success(run(&[
        "workbench",
        "apply",
        p(&input),
        p(&output),
        "--script",
        p(&script),
    ]));
    let before = fs::read(&output).unwrap();
    let report: Value = serde_json::from_slice(
        &success(run(&[
            "workbench",
            "station",
            p(&output),
            "--alignment",
            "1",
            "--point",
            "5",
            "7",
            "--json",
        ]))
        .stdout,
    )
    .unwrap();
    assert_eq!(report["unit_code"], 6);
    near(report["projection"]["station"].as_f64().unwrap(), 1005.0);
    near(report["projection"]["offset"].as_f64().unwrap(), 2.0);
    let quantities: Value = serde_json::from_slice(
        &success(run(&[
            "workbench",
            "earthwork",
            p(&output),
            "--datum",
            "0",
            "--json",
        ]))
        .stdout,
    )
    .unwrap();
    near(quantities["quantities"]["area"].as_f64().unwrap(), 100.0);
    near(quantities["quantities"]["cut"].as_f64().unwrap(), 1500.0);
    near(quantities["quantities"]["fill"].as_f64().unwrap(), 0.0);
    let profile = f.file("profile.csv");
    success(run(&[
        "workbench",
        "export",
        p(&output),
        p(&profile),
        "--area",
        "terrain",
        "--alignment",
        "1",
        "--samples",
        "3",
        "--offset",
        "1",
    ]));
    assert!(fs::read_to_string(profile).unwrap().contains("1005,5,6,17"));
    let earthwork = f.file("earthwork.csv");
    success(run(&[
        "workbench",
        "export",
        p(&output),
        p(&earthwork),
        "--area",
        "terrain",
        "--datum",
        "0",
    ]));
    assert!(
        fs::read_to_string(earthwork)
            .unwrap()
            .contains("6,0,100,1500,0,1500")
    );
    let invalid = f.file("invalid.csv");
    assert!(
        !run(&[
            "workbench",
            "export",
            p(&output),
            p(&invalid),
            "--area",
            "terrain",
            "--datum",
            "0",
            "--alignment",
            "1"
        ])
        .status
        .success()
    );
    assert!(!invalid.exists());
    assert_eq!(fs::read(&output).unwrap(), before);
}

#[test]
fn layout_authoring_plotting_and_explicit_exchange_loss_work_end_to_end() {
    let f = Files::new();
    let input = f.file("drawing.forge.json");
    let sheet = f.file("sheet.forge.json");
    let detail = f.file("detail.forge.json");
    let annotated = f.file("notes.forge.json");
    let mut document = Document::default();
    document.add_geometry(Geometry::Rectangle {
        a: Point::default(),
        b: Point::new(100.0, 100.0),
    });
    document.save_json(&input).unwrap();
    success(run(&[
        "layout",
        "create",
        p(&input),
        p(&sheet),
        "--name",
        "Plan A",
        "--page",
        "297",
        "210",
    ]));
    success(run(&[
        "layout",
        "viewport",
        p(&sheet),
        p(&detail),
        "--layout",
        "Plan A",
        "--name",
        "Detail",
        "--paper",
        "150",
        "20",
        "100",
        "100",
        "--center",
        "10",
        "10",
        "--scale",
        "2",
        "--rotation",
        "90",
        "--hide-layer",
        "1",
    ]));
    success(run(&[
        "layout",
        "note",
        p(&detail),
        p(&annotated),
        "--layout",
        "Plan A",
        "--position",
        "20",
        "190",
        "--text",
        "Plan <A & B>",
        "--height",
        "4",
    ]));
    let list: Value =
        serde_json::from_slice(&success(run(&["layout", "list", p(&annotated), "--json"])).stdout)
            .unwrap();
    assert_eq!(list[0]["viewports"].as_array().unwrap().len(), 2);
    assert_eq!(list[0]["viewports"][1]["rotation_deg"], 90.0);
    assert_eq!(list[0]["notes"][0]["text"], "Plan <A & B>");
    let before = fs::read(&annotated).unwrap();
    let svg = f.file("plan.svg");
    success(run(&["plot", p(&annotated), p(&svg), "--layout", "Plan A"]));
    let svg = fs::read_to_string(svg).unwrap();
    assert!(svg.contains("Plan &lt;A &amp; B&gt;"));
    assert!(svg.contains("clip-path=\"url(#viewport-1)\""));
    let matrices: Vec<Vec<f64>> = svg
        .split("matrix(")
        .skip(1)
        .map(|part| {
            part.split(')')
                .next()
                .unwrap()
                .split_whitespace()
                .map(|value| value.parse().unwrap())
                .collect()
        })
        .collect();
    assert!(matrices.iter().any(|matrix| {
        matrix.len() == 6
            && matrix[0].abs() < 1e-12
            && (matrix[1] - 2.0).abs() < 1e-12
            && (matrix[2] - 2.0).abs() < 1e-12
            && matrix[3].abs() < 1e-12
    }));
    let dxf = f.file("drawing.dxf");
    assert!(!run(&["convert", p(&annotated), p(&dxf)]).status.success());
    assert!(!dxf.exists());
    let converted = success(run(&["convert", p(&annotated), p(&dxf), "--allow-lossy"]));
    assert!(
        String::from_utf8_lossy(&converted.stderr).contains("print layouts")
            || String::from_utf8_lossy(&converted.stdout).contains("print layouts")
    );
    assert_eq!(Document::import_dxf(&dxf).unwrap().entities.len(), 1);
    let invalid = f.file("invalid.svg");
    assert!(
        !run(&[
            "plot",
            p(&annotated),
            p(&invalid),
            "--layout",
            "Plan A",
            "--scale",
            "1"
        ])
        .status
        .success()
    );
    assert!(!invalid.exists());
    let invalid = f.file("invalid.forge.json");
    assert!(
        !run(&[
            "layout",
            "viewport",
            p(&annotated),
            p(&invalid),
            "--layout",
            "Plan A",
            "--name",
            "Bad",
            "--paper",
            "9999",
            "0",
            "100",
            "100",
            "--center",
            "0",
            "0",
            "--scale",
            "1"
        ])
        .status
        .success()
    );
    assert!(!invalid.exists());
    assert_eq!(fs::read(&annotated).unwrap(), before);
}
