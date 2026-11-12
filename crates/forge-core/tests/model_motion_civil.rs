use forge_core::{
    Document, Geometry, PROJECT_FORMAT_VERSION, Point, PolylineVertex, SolidKind,
    civil::Alignment,
    mesh::TessellationOptions,
    scene::{Axis, Interpolation, Keyframe, Modifier, Scene, SceneObject},
    terrain::Terrain,
};
use std::collections::BTreeMap;
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-7, "{a} != {b}");
}
fn l_profile() -> Geometry {
    Geometry::Polyline {
        closed: true,
        vertices: vec![
            (0.0, 0.0),
            (2.0, 0.0),
            (4.0, 0.0),
            (4.0, 1.0),
            (1.0, 1.0),
            (1.0, 4.0),
            (0.0, 4.0),
        ]
        .into_iter()
        .map(|(x, y)| PolylineVertex::new(Point::new(x, y)))
        .collect(),
    }
}
#[test]
fn concave_extrusions_close_caps_and_walls_with_outward_winding_and_exact_volume() {
    for reverse in [false, true] {
        let mut profile = l_profile();
        if reverse {
            if let Geometry::Polyline { vertices, .. } = &mut profile {
                vertices.reverse();
            }
        }
        let mut doc = Document::default();
        doc.add_extrusion("L bracket", profile, 5.0, 3.0).unwrap();
        let mesh = doc.extrusions[0].mesh().unwrap();
        near(mesh.signed_volume().unwrap(), 21.0);
        let (min, max) = mesh.bounds().unwrap();
        assert_eq!(min, [0.0, 0.0, 5.0]);
        assert_eq!(max, [4.0, 4.0, 8.0]);
        assert_eq!(
            mesh.vertices.len(),
            12,
            "collinear boundary point must be removed from both walls and caps"
        );
        let mut edges = BTreeMap::new();
        for [a, b, c] in mesh.triangles {
            for (a, b) in [(a, b), (b, c), (c, a)] {
                let value = edges.entry((a.min(b), a.max(b))).or_insert((0, 0));
                value.0 += 1;
                value.1 += if a < b { 1 } else { -1 };
            }
        }
        assert!(
            edges
                .values()
                .all(|(count, winding)| *count == 2 && *winding == 0),
            "closed mesh edges must be shared with opposite orientation"
        );
    }
}
#[test]
fn extrusion_parameters_round_trip_and_mesh_exchange_refuses_silent_parameter_loss() {
    let mut doc = Document::default();
    let id = doc
        .add_extrusion(
            "Plate",
            Geometry::Rectangle {
                a: Point::default(),
                b: Point::new(8.0, 4.0),
            },
            0.0,
            3.0,
        )
        .unwrap();
    near(
        doc.model_mesh(TessellationOptions::default())
            .unwrap()
            .signed_volume()
            .unwrap(),
        96.0,
    );
    doc.extrusions[0].height = 5.0;
    let restored = Document::from_json(&doc.to_json().unwrap()).unwrap();
    assert_eq!(restored, doc);
    near(
        restored
            .model_mesh(TessellationOptions::default())
            .unwrap()
            .signed_volume()
            .unwrap(),
        160.0,
    );
    assert!(forge_core::dxf::write(&doc).is_err());
    assert!(forge_core::plot::export_svg(&doc, Default::default()).is_err());
    let mut loaded = restored;
    let next = loaded
        .add_geometry(Geometry::Point {
            position: Point::default(),
        })
        .unwrap();
    assert!(next > id);
    let before = loaded.clone();
    assert!(
        loaded
            .add_extrusion(
                "Invalid",
                Geometry::Line {
                    start: Point::default(),
                    end: Point::new(1.0, 0.0)
                },
                0.0,
                1.0
            )
            .is_err()
    );
    assert_eq!(loaded, before);
    let crossed = Geometry::Polyline {
        closed: true,
        vertices: vec![(0.0, 0.0), (2.0, 2.0), (0.0, 2.0), (2.0, 0.0)]
            .into_iter()
            .map(|(x, y)| PolylineVertex::new(Point::new(x, y)))
            .collect(),
    };
    assert!(loaded.add_extrusion("Crossed", crossed, 0.0, 1.0).is_err());
    assert_eq!(loaded, before);
    assert!(loaded.add_extrusion("Flat", l_profile(), 0.0, 0.0).is_err());
    assert_eq!(loaded, before);
    let mut exhausted = Document::default();
    exhausted.layers[0].id = u64::MAX - 1;
    exhausted.active_layer_id = u64::MAX - 1;
    let before = exhausted.clone();
    assert!(
        exhausted
            .add_extrusion("No IDs", l_profile(), 0.0, 1.0)
            .is_err()
    );
    assert_eq!(exhausted, before);
    let mut high_feature_id = doc.clone();
    high_feature_id.extrusions[0].id = u64::MAX - 1;
    let before = high_feature_id.clone();
    assert!(forge_core::command::execute(&mut high_feature_id, &[], "POINT 0 0").is_err());
    assert_eq!(high_feature_id, before);
}
#[test]
fn circular_extrusion_retains_analytic_profile_and_has_documented_chord_approximation() {
    let mut doc = Document::default();
    doc.add_extrusion(
        "Circular boss",
        Geometry::Circle {
            center: Point::new(10.0, 20.0),
            radius: 4.0,
        },
        -2.0,
        10.0,
    )
    .unwrap();
    let mesh = doc.extrusions[0].mesh().unwrap();
    let analytic = std::f64::consts::PI * 16.0 * 10.0;
    let volume = mesh.signed_volume().unwrap();
    assert!(volume < analytic && volume > analytic * 0.999);
    assert!(matches!(
        doc.extrusions[0].profile,
        Geometry::Circle { radius: 4.0, .. }
    ));
    let (min, max) = mesh.bounds().unwrap();
    for (a, b) in min.into_iter().zip([6.0, 16.0, -2.0]) {
        near(a, b);
    }
    for (a, b) in max.into_iter().zip([14.0, 24.0, 8.0]) {
        near(a, b);
    }
}
fn animated() -> SceneObject {
    let mut object = SceneObject::new(1, "Animated", SolidKind::Box);
    object.size = [2.0; 3];
    object.keyframes.push(Keyframe {
        time: 4.0,
        translation: [8.0, 0.0, 0.0],
        rotation_deg: [0.0, 0.0, 90.0],
        scale: [3.0; 3],
    });
    object
}
#[test]
fn interpolation_modes_hold_keys_or_ease_all_channels_without_changing_endpoints() {
    let mut object = animated();
    near(object.pose_at(1.0).unwrap().translation[0], 2.0);
    object.interpolation = Interpolation::Step;
    let p = object.pose_at(1.0).unwrap();
    near(p.translation[0], 0.0);
    near(p.scale[0], 1.0);
    near(p.time, 1.0);
    object.interpolation = Interpolation::Smoothstep;
    let p = object.pose_at(1.0).unwrap();
    near(p.translation[0], 1.25);
    near(p.rotation_deg[2], 14.0625);
    near(p.scale[0], 1.3125);
    assert_eq!(object.pose_at(4.0).unwrap(), object.keyframes[1]);
    let before = object.clone();
    assert!(object.pose_at(f64::NAN).is_err());
    assert_eq!(object, before);
}
#[test]
fn array_and_mirror_modifiers_preserve_outward_volume_and_obey_parent_visibility() {
    let mut object = animated();
    object.keyframes.truncate(1);
    object.modifiers = vec![
        Modifier::Array {
            count: 2,
            offset: [10.0, 0.0, 0.0],
        },
        Modifier::Mirror {
            axis: Axis::X,
            offset: 0.0,
            keep_original: true,
        },
    ];
    let mut scene = Scene {
        objects: vec![object],
        ..Scene::default()
    };
    let before = scene.clone();
    let meshes = scene.meshes_at(0.0).unwrap();
    let mesh = &meshes[0].3;
    assert_eq!(mesh.triangles.len(), 48);
    near(mesh.signed_volume().unwrap(), 32.0);
    let (min, max) = mesh.bounds().unwrap();
    near(min[0], -11.0);
    near(max[0], 11.0);
    assert_eq!(scene, before);
    let mut child = SceneObject::new(2, "Child", SolidKind::Box);
    child.parent_id = Some(1);
    scene.objects.push(child);
    scene.objects[0].visible = false;
    assert!(scene.meshes_at(0.0).unwrap().is_empty());
    scene.objects[0].visible = true;
    scene.objects[0].modifiers.push(Modifier::Array {
        count: 128,
        offset: [1.0, 0.0, 0.0],
    });
    assert!(scene.validate().is_err());
}
fn arc_alignment(bulge: f64) -> Alignment {
    Alignment {
        id: 1,
        name: "Circular alignment".into(),
        start_station: 1000.0,
        vertices: vec![Point::new(10.0, 0.0), Point::new(0.0, 10.0)],
        bulges: vec![bulge],
    }
}
#[test]
fn circular_alignment_stationing_offsets_and_projection_use_true_arc_length() {
    let a = arc_alignment((std::f64::consts::PI / 8.0).tan());
    let length = 5.0 * std::f64::consts::PI;
    near(a.length().unwrap(), length);
    let p = a.point_at(1000.0 + length / 2.0, 2.0).unwrap();
    near(p.x, 8.0 / 2.0_f64.sqrt());
    near(p.y, 8.0 / 2.0_f64.sqrt());
    let q = a
        .station_offset(Point::new(12.0 / 2.0_f64.sqrt(), 12.0 / 2.0_f64.sqrt()))
        .unwrap();
    near(q.station, 1000.0 + length / 2.0);
    near(q.offset, -2.0);
    near(q.distance, 2.0);
    assert_eq!(a.point_at(1000.0, 0.0).unwrap(), a.vertices[0]);
    assert_eq!(
        a.point_at(1000.0 + a.length().unwrap(), 0.0).unwrap(),
        a.vertices[1]
    );
    assert!(a.point_at(1000.0, 10.0).is_err());
    assert!(a.display_lines().unwrap().len() >= 30);
    let mut reversed = a.clone();
    reversed.vertices.reverse();
    reversed.bulges[0] *= -1.0;
    let p = reversed.point_at(1000.0 + length / 2.0, 2.0).unwrap();
    near(p.x, 12.0 / 2.0_f64.sqrt());
    near(p.y, 12.0 / 2.0_f64.sqrt());
    let mut invalid = a;
    invalid.bulges.push(0.0);
    assert!(invalid.validate().is_err());
}
#[test]
fn major_arcs_and_terrain_profiles_keep_analytic_stationing() {
    let a = Alignment {
        id: 1,
        name: "Major".into(),
        start_station: 0.0,
        vertices: vec![Point::new(1.0, 0.0), Point::new(0.0, -1.0)],
        bulges: vec![(3.0 * std::f64::consts::PI / 8.0).tan()],
    };
    near(a.length().unwrap(), 1.5 * std::f64::consts::PI);
    let p = a.station_offset(Point::new(-2.0, 0.0)).unwrap();
    near(p.station, std::f64::consts::PI);
    near(p.offset, -1.0);
    let mut terrain = Terrain::triangulate(vec![
        [0.0, 0.0, 0.0],
        [10.0, 0.0, 10.0],
        [0.0, 10.0, 10.0],
        [10.0, 10.0, 20.0],
    ])
    .unwrap();
    terrain
        .alignments
        .push(arc_alignment((std::f64::consts::PI / 8.0).tan()));
    let samples = terrain.alignment_profile(1, 3, 0.0).unwrap();
    near(samples[1].elevation.unwrap(), 10.0 * 2.0_f64.sqrt());
}
#[test]
fn v5_projects_receive_empty_extrusions_linear_visible_unmodified_objects_and_straight_alignments()
{
    assert_eq!(PROJECT_FORMAT_VERSION, 7);
    let mut doc = Document::default();
    doc.scene = Scene::example();
    doc.terrain = Terrain::example();
    let mut a = arc_alignment(0.0);
    a.bulges.clear();
    doc.terrain.alignments.push(a);
    let mut value: serde_json::Value = serde_json::from_str(&doc.to_json().unwrap()).unwrap();
    value["format_version"] = 5.into();
    let fields = value["document"].as_object_mut().unwrap();
    fields.remove("extrusions");
    for o in fields["scene"]["objects"].as_array_mut().unwrap() {
        for name in ["interpolation", "visible", "modifiers"] {
            o.as_object_mut().unwrap().remove(name);
        }
    }
    fields["terrain"]["alignments"][0]
        .as_object_mut()
        .unwrap()
        .remove("bulges");
    let migrated = Document::from_json(&serde_json::to_string(&value).unwrap()).unwrap();
    assert_eq!(migrated, doc);
}
