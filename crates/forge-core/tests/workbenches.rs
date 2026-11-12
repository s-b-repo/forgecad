use forge_core::{
    Document, Geometry, PROJECT_FORMAT_VERSION, Point,
    building::Building,
    cam::CamJob,
    dxf,
    plot::{SvgOptions, export_svg},
    scene::Scene,
    terrain::Terrain,
};
use serde_json::Value;

fn suite() -> Document {
    let mut document = Document::new("Semantic suite regression");
    document.units = 4;
    document.building = Building::example();
    document.terrain = Terrain::example();
    document.scene = Scene::example();
    document.manufacturing = CamJob::example();
    document.review = forge_core::review::ProjectReview::example();
    document
        .add_geometry(Geometry::Line {
            start: Point::new(1.0, 2.0),
            end: Point::new(3.0, 4.0),
        })
        .unwrap();
    document
}

#[test]
fn native_v7_preserves_all_semantic_models_and_derived_results() {
    let original = suite();
    assert_eq!(PROJECT_FORMAT_VERSION, 7);
    let source = original.to_json().unwrap();
    let restored = Document::from_json(&source).unwrap();
    assert_eq!(restored, original);
    assert!(restored.has_workbench_data());
    assert_eq!(
        restored.building.schedule_csv().unwrap(),
        original.building.schedule_csv().unwrap()
    );
    assert_eq!(
        restored.terrain.contours(5.0).unwrap(),
        original.terrain.contours(5.0).unwrap()
    );
    assert_eq!(
        restored.scene.meshes_at(2.5).unwrap(),
        original.scene.meshes_at(2.5).unwrap()
    );
    assert_eq!(
        restored.manufacturing.gcode().unwrap(),
        original.manufacturing.gcode().unwrap()
    );
    assert_eq!(
        restored.review.csv().unwrap(),
        original.review.csv().unwrap()
    );
}

#[test]
fn v3_drawing_migration_supplies_empty_semantic_defaults_without_geometry_changes() {
    let mut document = Document::new("Legacy drawing");
    document
        .add_geometry(Geometry::Circle {
            center: Point::new(8.0, 9.0),
            radius: 5.0,
        })
        .unwrap();
    let mut old: Value = serde_json::from_str(&document.to_json().unwrap()).unwrap();
    old["format_version"] = Value::from(3);
    let fields = old["document"].as_object_mut().unwrap();
    for field in ["building", "terrain", "scene", "manufacturing", "review"] {
        fields.remove(field);
    }
    let migrated = Document::from_json(&serde_json::to_string(&old).unwrap()).unwrap();
    assert_eq!(migrated, document);
    assert!(!migrated.has_workbench_data());
    assert_eq!(migrated.scene, Scene::default());
    assert_eq!(migrated.manufacturing, CamJob::default());
}

#[test]
fn local_review_lifecycle_round_trips_and_duplicate_ids_are_rejected() {
    use forge_core::review::{ReviewItem, ReviewPriority, ReviewStatus};
    let mut document = Document::new("Local project review");
    let mut item = ReviewItem::new(1, "Inspect doorway");
    item.description = "Initial finding\nFollow-up notes".into();
    item.element_reference = "Building wall #2".into();
    item.priority = ReviewPriority::High;
    document.review.items.push(item);
    for status in [
        ReviewStatus::Open,
        ReviewStatus::InProgress,
        ReviewStatus::Resolved,
    ] {
        document.review.items[0].status = status;
        let restored = Document::from_json(&document.to_json().unwrap()).unwrap();
        assert_eq!(restored.review, document.review);
        assert!(restored.has_workbench_data());
        assert!(restored.review.csv().unwrap().contains(status.label()));
        assert!(dxf::write(&restored).is_err());
    }
    let before = document.clone();
    document.review.items.push(document.review.items[0].clone());
    assert!(document.validate().is_err());
    assert!(document.to_json().is_err());
    let restored = Document::from_json(&before.to_json().unwrap()).unwrap();
    assert_eq!(restored.review.items.len(), 1);
    assert_eq!(restored.review.items[0].status, ReviewStatus::Resolved);
}

#[test]
fn generic_derived_exports_refuse_silent_semantic_data_loss() {
    let source = suite();
    assert!(dxf::write(&source).is_err());
    assert!(export_svg(&source, SvgOptions::default()).is_err());
    assert!(
        source
            .export_box_stl("target/should-not-be-published.stl")
            .is_err()
    );
    let mut drawing = source.clone();
    drawing.clear_workbench_data();
    assert!(!drawing.has_workbench_data());
    assert_eq!(drawing.entities, source.entities);
    assert!(dxf::write(&drawing).is_ok());
    assert!(export_svg(&drawing, SvgOptions::default()).is_ok());
    assert!(source.has_workbench_data());
}

#[test]
fn persisted_validation_checks_domain_references_and_numeric_parameters() {
    let valid = suite();
    let mut building = valid.clone();
    building.building.walls[0].storey_id = u64::MAX;
    assert!(building.validate().is_err());
    assert!(building.to_json().is_err());
    let mut terrain = valid.clone();
    terrain.terrain.triangles[0][0] = usize::MAX;
    assert!(terrain.validate().is_err());
    let mut scene = valid.clone();
    scene.scene.objects[0].keyframes[0].time = scene.scene.duration + 1.0;
    assert!(scene.validate().is_err());
    let mut cam = valid.clone();
    cam.manufacturing.feed = -1.0;
    assert!(cam.validate().is_err());
    let mut encoded: Value = serde_json::from_str(&valid.to_json().unwrap()).unwrap();
    encoded["document"]["building"]["openings"][0]["wall_id"] = Value::from(999_999);
    assert!(Document::from_json(&serde_json::to_string(&encoded).unwrap()).is_err());
}
