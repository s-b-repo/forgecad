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
