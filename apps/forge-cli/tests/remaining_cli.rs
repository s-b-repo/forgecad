use forge_core::{
    Document, Geometry, Point,
    constraints::{PointHandle, PointRef, Relation, SketchConstraint},
    scene::Scene,
    terrain::Terrain,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[test]
fn native_extrusion_cli_edits_parameters_and_requires_loss_authorization_for_meshes() {
    let files = Files::new();
    let input = files.file("part.forge.json");
    let edited = files.file("edited.forge.json");
    let obj = files.file("part.obj");
    success(run(&[
        "model",
        "extrude",
        "-",
        p(&input),
        "--rectangle",
        "0",
        "0",
        "8",
        "4",
        "--height",
        "3",
        "--name",
        "Plate",
    ]));
    let doc = Document::load_json(&input).unwrap();
    let id = doc.extrusions[0].id.to_string();
    success(run(&[
        "model",
        "set",
        p(&input),
        p(&edited),
        "--id",
        &id,
        "--height",
        "5",
    ]));
    let info: Value =
        serde_json::from_slice(&success(run(&["model", "inspect", p(&edited), "--json"])).stdout)
            .unwrap();
    near(info["extrusions"][0]["volume"].as_f64().unwrap(), 160.0);
    assert!(!run(&["convert", p(&edited), p(&obj)]).status.success());
    assert!(!obj.exists());
    success(run(&["convert", p(&edited), p(&obj), "--allow-lossy"]));
    let mesh =
        forge_core::mesh::TriangleMesh::from_obj(&fs::read_to_string(&obj).unwrap()).unwrap();
    near(mesh.signed_volume().unwrap(), 160.0);
    let before = fs::read(&edited).unwrap();
    assert!(
        !run(&[
            "model",
            "set",
            p(&input),
            p(&edited),
            "--id",
            &id,
            "--height",
            "0",
            "--overwrite"
        ])
        .status
        .success()
    );
    assert_eq!(fs::read(&edited).unwrap(), before);
    let native = Document::load_json(&input).unwrap();
    near(native.extrusions[0].height, 3.0);
}
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
    document
        .add_geometry(Geometry::Rectangle {
            a: Point::default(),
            b: Point::new(100.0, 100.0),
        })
        .unwrap();
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

/// A native project with one sloped line and one circle, authored through the
/// shared drafting language as a horizontal line and a 25 unit radius.
fn authored(files: &Files) -> (PathBuf, u64, u64) {
    let input = files.file("sketch.forge.json");
    let output = files.file("constrained.forge.json");
    let mut document = Document::default();
    document.units = 4;
    let line = document
        .add_geometry(Geometry::Line {
            start: Point::new(0.0, 0.0),
            end: Point::new(30.0, 12.0),
        })
        .unwrap();
    let circle = document
        .add_geometry(Geometry::Circle {
            center: Point::new(60.0, 10.0),
            radius: 8.0,
        })
        .unwrap();
    document.save_json(&input).unwrap();
    success(run(&[
        "draft",
        p(&input),
        p(&output),
        "--command",
        &format!("RELATION HORIZONTAL {line}"),
        "--command",
        &format!("RELATION RADIUS {circle} 25"),
        "--command",
        "CONSTRAINT SOLVE",
    ]));
    (output, line, circle)
}

fn conflicting_sketch(files: &Files) -> (PathBuf, u64, u64) {
    let mut document = Document::default();
    let first = document
        .add_geometry(Geometry::Line {
            start: Point::new(0.0, 0.0),
            end: Point::new(10.0, 0.0),
        })
        .unwrap();
    let second = document
        .add_geometry(Geometry::Line {
            start: Point::new(20.0, 0.0),
            end: Point::new(30.0, 0.0),
        })
        .unwrap();
    let distance_x = |id, value| SketchConstraint {
        id,
        relation: Relation::DistanceX {
            a: PointRef {
                entity: first,
                handle: PointHandle::Start,
            },
            b: PointRef {
                entity: second,
                handle: PointHandle::Start,
            },
            value,
        },
    };
    let report = document.set_constraints(vec![distance_x(1, 50.0)]).unwrap();
    assert!(report.solved());
    let path = files.file("conflict.forge.json");
    document.save_json(&path).unwrap();
    let mut project: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    project["document"]["constraints"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id": 2,
            "relation": {
                "type": "distance_x",
                "a": {"entity": first, "handle": "start"},
                "b": {"entity": second, "handle": "start"},
                "value": 70.0
            }
        }));
    fs::write(&path, serde_json::to_vec_pretty(&project).unwrap()).unwrap();
    (path, first, second)
}

#[test]
fn scripted_relations_author_solve_and_persist_as_a_native_project() {
    let f = Files::new();
    let (project, _, _) = authored(&f);
    let saved = Document::load_json(&project).unwrap();
    saved.validate().unwrap();
    assert_eq!(saved.constraints.len(), 2);
    assert_eq!(saved.next_constraint_id(), 3);
    match &saved.entities[0].geometry {
        Geometry::Line { start, end } => {
            near(start.y, end.y);
            near(start.y, 6.0);
            near(end.x, 30.0);
        }
        geometry => panic!("expected a line, found {geometry:?}"),
    }
    match &saved.entities[1].geometry {
        Geometry::Circle { center, radius } => {
            near(*radius, 25.0);
            near(center.x, 60.0);
            near(center.y, 10.0);
        }
        geometry => panic!("expected a circle, found {geometry:?}"),
    }
}

#[test]
fn constraints_report_text_and_json_modes_and_inspect_expose_the_same_state() {
    let f = Files::new();
    let (project, _, _) = authored(&f);
    let report: Value = serde_json::from_slice(
        &success(run(&["constraints", "report", p(&project), "--json"])).stdout,
    )
    .unwrap();
    assert_eq!(report["state"], "satisfied");
    assert_eq!(report["constraints"], 2);
    assert_eq!(report["variables"], 7);
    assert_eq!(report["equations"], 2);
    assert_eq!(report["rank"], 2);
    assert_eq!(report["redundant_equations"], 0);
    assert_eq!(report["degrees_of_freedom"], 5);
    near(report["max_error"].as_f64().unwrap(), 0.0);
    assert!(report["conflicts"].as_array().unwrap().is_empty());
    assert_eq!(report["errors"].as_array().unwrap().len(), 2);
    assert_eq!(report["errors"][0]["id"], 1);
    near(report["errors"][0]["error"].as_f64().unwrap(), 0.0);
    let text = success(run(&["constraints", "report", p(&project)]));
    let text = String::from_utf8_lossy(&text.stdout);
    assert!(text.contains("Satisfied: 2 constraints"), "{text}");
    assert!(text.contains("5 remaining degrees of freedom"), "{text}");
    assert!(text.contains("#1 Horizontal"), "{text}");
    assert!(text.contains("#2 Radius 25"), "{text}");
    let info: Value =
        serde_json::from_slice(&success(run(&["inspect", p(&project), "--json"])).stdout).unwrap();
    assert_eq!(info["workbenches"]["sketch_constraints"]["count"], 2);
    assert_eq!(
        info["workbenches"]["sketch_constraints"]["degrees_of_freedom"],
        5
    );
    let free = f.file("free.forge.json");
    let mut document = Document::default();
    document
        .add_geometry(Geometry::Line {
            start: Point::default(),
            end: Point::new(10.0, 5.0),
        })
        .unwrap();
    document.save_json(&free).unwrap();
    let free_report: Value = serde_json::from_slice(
        &success(run(&["constraints", "report", p(&free), "--json"])).stdout,
    )
    .unwrap();
    assert_eq!(free_report["constraints"], 0);
    assert_eq!(free_report["degrees_of_freedom"], 0);
    assert_eq!(free_report["state"], "satisfied");
    let info: Value =
        serde_json::from_slice(&success(run(&["inspect", p(&free), "--json"])).stdout).unwrap();
    assert_eq!(info["workbenches"]["sketch_constraints"]["count"], 0);
    assert_eq!(
        info["workbenches"]["sketch_constraints"]["degrees_of_freedom"],
        0
    );
}

#[test]
fn constraints_solve_writes_a_solved_project_and_a_conflict_writes_nothing() {
    let f = Files::new();
    let (project, _, circle) = authored(&f);
    let solved = f.file("solved.forge.json");
    let output = success(run(&["constraints", "solve", p(&project), p(&solved)]));
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("Satisfied: 2 constraints"),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let saved = Document::load_json(&solved).unwrap();
    saved.validate().unwrap();
    assert_eq!(saved.constraints.len(), 2);
    match &saved
        .entities
        .iter()
        .find(|entity| entity.id == circle)
        .unwrap()
        .geometry
    {
        Geometry::Circle { radius, .. } => near(*radius, 25.0),
        geometry => panic!("expected a circle, found {geometry:?}"),
    }
    let before = fs::read(&project).unwrap();
    assert!(
        !run(&["constraints", "solve", p(&project), p(&project)])
            .status
            .success()
    );
    assert_eq!(fs::read(&project).unwrap(), before);
    let (conflict, _, _) = conflicting_sketch(&f);
    let source = fs::read(&conflict).unwrap();
    let report: Value = serde_json::from_slice(
        &success(run(&["constraints", "report", p(&conflict), "--json"])).stdout,
    )
    .unwrap();
    assert_eq!(report["state"], "conflict");
    assert_eq!(report["max_error"].as_f64().unwrap(), 20.0);
    assert_eq!(report["conflicts"].as_array().unwrap().len(), 2);
    let sentinel = f.file("protected.forge.json");
    fs::write(&sentinel, b"protected").unwrap();
    for extra in [Vec::new(), vec!["--overwrite"]] {
        let mut command = vec!["constraints", "solve", p(&conflict), p(&sentinel)];
        command.extend(extra);
        let failed = run(&command);
        assert!(!failed.status.success());
        let stderr = String::from_utf8_lossy(&failed.stderr);
        assert!(stderr.contains("Conflict"), "{stderr}");
        assert!(stderr.contains("no output written"), "{stderr}");
        assert_eq!(fs::read(&sentinel).unwrap(), b"protected");
    }
    assert_eq!(fs::read(&conflict).unwrap(), source);
}

#[test]
fn constraints_handles_list_point_handles_for_lines_and_circles() {
    let f = Files::new();
    let (project, line, circle) = authored(&f);
    let line_handles: Value = serde_json::from_slice(
        &success(run(&[
            "constraints",
            "handles",
            p(&project),
            &line.to_string(),
            "--json",
        ]))
        .stdout,
    )
    .unwrap();
    let line_handles = line_handles.as_array().unwrap();
    assert_eq!(line_handles.len(), 2);
    assert_eq!(line_handles[0]["name"], "start");
    assert_eq!(line_handles[1]["name"], "end");
    near(line_handles[0]["x"].as_f64().unwrap(), 0.0);
    near(line_handles[0]["y"].as_f64().unwrap(), 6.0);
    near(line_handles[1]["x"].as_f64().unwrap(), 30.0);
    near(line_handles[1]["y"].as_f64().unwrap(), 6.0);
    let circle_handles: Value = serde_json::from_slice(
        &success(run(&[
            "constraints",
            "handles",
            p(&project),
            &circle.to_string(),
            "--json",
        ]))
        .stdout,
    )
    .unwrap();
    assert_eq!(circle_handles.as_array().unwrap().len(), 1);
    assert_eq!(circle_handles[0]["name"], "center");
    near(circle_handles[0]["x"].as_f64().unwrap(), 60.0);
    near(circle_handles[0]["y"].as_f64().unwrap(), 10.0);
    let text = success(run(&[
        "constraints",
        "handles",
        p(&project),
        &line.to_string(),
    ]));
    let text = String::from_utf8_lossy(&text.stdout);
    assert!(text.contains("start (0, 6)"), "{text}");
    assert!(text.contains("end (30, 6)"), "{text}");
    let missing = run(&["constraints", "handles", p(&project), "999"]);
    assert!(!missing.status.success());
    let stderr = String::from_utf8_lossy(&missing.stderr);
    assert!(stderr.contains("entity #999"), "{stderr}");
    assert!(stderr.contains("does not exist"), "{stderr}");
    let notes = f.file("note.forge.json");
    let mut document = Document::default();
    document
        .add_geometry(Geometry::Text {
            position: Point::new(1.0, 2.0),
            text: "note".to_owned(),
            height: 2.0,
            rotation_deg: 0.0,
        })
        .unwrap();
    document.save_json(&notes).unwrap();
    let failed = run(&["constraints", "handles", p(&notes), "2"]);
    assert!(!failed.status.success());
    assert!(
        String::from_utf8_lossy(&failed.stderr).contains("no constraint point handles"),
        "{}",
        String::from_utf8_lossy(&failed.stderr)
    );
}

#[test]
fn constraints_failures_are_rejected_before_any_output_is_written() {
    let f = Files::new();
    let (project, _, _) = authored(&f);
    let absent = f.file("absent.forge.json");
    let unknown = run(&["constraints", "bogus", p(&project)]);
    assert!(!unknown.status.success());
    assert!(
        String::from_utf8_lossy(&unknown.stderr).contains("unknown constraints command bogus"),
        "{}",
        String::from_utf8_lossy(&unknown.stderr)
    );
    assert!(
        !run(&["constraints", "report", p(&project), "--verbose"])
            .status
            .success()
    );
    assert!(!run(&["constraints", "report", p(&absent)]).status.success());
    assert!(!absent.exists());
    let dxf = f.file("sheet.dxf");
    assert!(
        !run(&["constraints", "solve", p(&project), p(&dxf)])
            .status
            .success()
    );
    assert!(!dxf.exists());
    let neutral = f.file("sheet2.dxf");
    success(run(&["convert", p(&project), p(&neutral), "--overwrite"]));
    let refused = run(&["constraints", "report", p(&neutral)]);
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains(".forge.json"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
}
