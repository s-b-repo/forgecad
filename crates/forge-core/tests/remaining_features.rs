use forge_core::{
    Document, Geometry, Point, SolidKind,
    automation::{ProjectEdit, apply_edits},
    cam::{CamJob, Compensation, Operation, Profile},
    civil::Alignment,
    plot::{PaperText, PrintLayout, SvgOptions, export_layout_svg},
    scene::{Keyframe, Scene, SceneObject},
    terrain::Terrain,
};

fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-8, "{a} != {b}");
}
fn alignment() -> Alignment {
    Alignment {
        id: 1,
        name: "Road".into(),
        start_station: 1000.0,
        bulges: Vec::new(),
        vertices: vec![
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
        ],
    }
}
fn pocket(geometry: Geometry) -> CamJob {
    let mut job = CamJob::default();
    job.profiles.push(Profile {
        id: 1,
        name: "Pocket".into(),
        geometry,
        compensation: Compensation::Inside,
        operation: Operation::Pocket,
        islands: Vec::new(),
    });
    job
}

#[test]
fn nested_scene_animation_composes_parent_rotation_nonuniform_scale_and_child_translation() {
    let mut root = SceneObject::new(1, "Root", SolidKind::Box);
    root.keyframes[0].translation = [10.0, 20.0, 30.0];
    root.keyframes[0].rotation_deg = [0.0, 0.0, 90.0];
    root.keyframes[0].scale = [2.0, 3.0, 4.0];
    let mut child = SceneObject::new(2, "Child", SolidKind::Box);
    child.parent_id = Some(1);
    child.size = [2.0; 3];
    child.keyframes[0].translation = [5.0, 0.0, 0.0];
    child.keyframes.push(Keyframe {
        time: 5.0,
        translation: [15.0, 0.0, 0.0],
        ..Keyframe::default()
    });
    let mut leaf = SceneObject::new(3, "Leaf", SolidKind::Box);
    leaf.parent_id = Some(2);
    leaf.size = [2.0; 3];
    leaf.keyframes[0].translation = [0.0, 2.0, 0.0];
    // Unsorted node order must not affect evaluation.
    let scene = Scene {
        objects: vec![leaf, child, root],
        ..Scene::default()
    };
    let before = scene.clone();
    let meshes = scene.meshes_at(2.5).unwrap();
    let (min, max) = meshes[0].3.bounds().unwrap();
    // Child local x=10, leaf local y=2 -> world center (4,40,30).
    for (v, e) in min.into_iter().zip([1.0, 38.0, 26.0]) {
        near(v, e);
    }
    for (v, e) in max.into_iter().zip([7.0, 42.0, 34.0]) {
        near(v, e);
    }
    near(meshes[0].3.signed_volume().unwrap(), 192.0);
    assert_eq!(scene, before);
}

#[test]
fn hierarchy_rejects_dangling_parents_cycles_and_excessive_depth_atomically() {
    let mut scene = Scene::example();
    scene.set_parent(2, Some(1)).unwrap();
    let before = scene.clone();
    for (id, parent) in [(1, Some(2)), (1, Some(1)), (2, Some(999)), (999, None)] {
        assert!(scene.set_parent(id, parent).is_err());
        assert_eq!(scene, before);
    }
    let removed = scene.remove_subtree(1).unwrap();
    assert_eq!(removed, vec![1, 2]);
    assert!(scene.objects.is_empty());
    let mut scene = Scene::default();
    for id in 1..=65 {
        let mut object = SceneObject::new(id, "Node", SolidKind::Box);
        object.parent_id = if id == 1 { None } else { Some(id - 1) };
        scene.objects.push(object);
    }
    assert!(scene.validate().is_err());
}

#[test]
fn alignment_station_offset_vertex_rules_and_outside_projection_are_deterministic() {
    let alignment = alignment();
    near(alignment.length().unwrap(), 20.0);
    assert_eq!(
        alignment.point_at(1005.0, 2.0).unwrap(),
        Point::new(5.0, 2.0)
    );
    assert_eq!(
        alignment.point_at(1010.0, 2.0).unwrap(),
        Point::new(8.0, 0.0)
    );
    assert_eq!(
        alignment.point_at(1020.0, -2.0).unwrap(),
        Point::new(12.0, 10.0)
    );
    let p = alignment.station_offset(Point::new(5.0, -3.0)).unwrap();
    near(p.station, 1005.0);
    near(p.offset, -3.0);
    near(p.distance, 3.0);
    let p = alignment.station_offset(Point::new(-5.0, 2.0)).unwrap();
    near(p.station, 1000.0);
    near(p.offset, 2.0);
    near(p.distance, 29.0_f64.sqrt());
    for station in [999.0, 1021.0, f64::NAN] {
        assert!(alignment.point_at(station, 0.0).is_err());
    }
    assert!(alignment.point_at(1000.0, f64::INFINITY).is_err());
    let mut bad = alignment.clone();
    bad.vertices[1] = bad.vertices[0];
    assert!(bad.validate().is_err());
    let mut bad = alignment;
    bad.start_station = 1e308;
    assert!(bad.validate().is_err());
}

#[test]
fn alignment_profiles_keep_stations_offsets_and_surface_gaps() {
    let mut terrain = Terrain::triangulate(vec![
        [0.0, 0.0, 0.0],
        [10.0, 0.0, 10.0],
        [0.0, 10.0, 20.0],
        [10.0, 10.0, 30.0],
    ])
    .unwrap();
    terrain.alignments.push(Alignment {
        vertices: vec![Point::new(-5.0, 5.0), Point::new(15.0, 5.0)],
        ..alignment()
    });
    let samples = terrain.alignment_profile(1, 5, 0.0).unwrap();
    assert_eq!(
        samples.iter().map(|s| s.distance).collect::<Vec<_>>(),
        vec![1000.0, 1005.0, 1010.0, 1015.0, 1020.0]
    );
    assert_eq!(
        samples.iter().map(|s| s.elevation).collect::<Vec<_>>(),
        vec![None, Some(10.0), Some(15.0), Some(20.0), None]
    );
    let offset = terrain.alignment_profile(1, 5, 1.0).unwrap();
    near(offset[2].elevation.unwrap(), 17.0);
    assert!(
        terrain
            .alignment_csv(1, 5, 0.0)
            .unwrap()
            .contains("1000,-5,5,\n")
    );
    assert!(terrain.alignment_profile(999, 5, 0.0).is_err());
    assert!(terrain.alignment_profile(1, 10001, 0.0).is_err());
    let mut empty = Terrain::default();
    empty.alignments.push(Alignment {
        id: 1,
        name: "Large".into(),
        start_station: 0.0,
        vertices: vec![Point::default(), Point::new(1e308, 0.0)],
        bulges: Vec::new(),
    });
    let samples = empty.alignment_profile(1, 3, 0.0).unwrap();
    assert_eq!(samples[1].position, Point::new(5e307, 0.0));
    assert_eq!(samples[2].position, Point::new(1e308, 0.0));
}

#[test]
fn earthwork_splits_mixed_triangles_and_conserves_signed_prism_volume() {
    // Height x-1 over a 2×2 square. Cut and fill are each a triangular
    // prism of volume 1; integrating unsplit triangle means would be wrong.
    let terrain = Terrain::triangulate(vec![
        [0.0, 0.0, -1.0],
        [2.0, 0.0, 1.0],
        [2.0, 2.0, 1.0],
        [0.0, 2.0, -1.0],
    ])
    .unwrap();
    let before = terrain.clone();
    let q = terrain.earthwork(0.0).unwrap();
    near(q.area, 4.0);
    near(q.cut, 1.0);
    near(q.fill, 1.0);
    near(q.net_cut, 0.0);
    let q = terrain.earthwork(-2.0).unwrap();
    near(q.cut, 8.0);
    near(q.fill, 0.0);
    near(q.net_cut, 8.0);
    let q = terrain.earthwork(2.0).unwrap();
    near(q.fill, 8.0);
    near(q.cut, 0.0);
    near(q.net_cut, -8.0);
    assert_eq!(terrain, before);
    assert!(Terrain::default().earthwork(0.0).is_err());
    assert!(terrain.earthwork(f64::NAN).is_err());
    let flat = Terrain::triangulate(vec![[0.0, 0.0, 2.0], [2.0, 0.0, 2.0], [0.0, 2.0, 2.0]])
        .unwrap()
        .earthwork(2.0)
        .unwrap();
    near(flat.cut, 0.0);
    near(flat.fill, 0.0);
    near(flat.area, 2.0);
}

fn distance_segment(p: Point, a: Point, b: Point) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    p.distance(Point::new(a.x + t * dx, a.y + t * dy))
}

#[test]
fn pocket_paths_clear_interior_keep_tool_inside_boundary_and_retract_vertically() {
    for geometry in [
        Geometry::Rectangle {
            a: Point::new(0.0, 0.0),
            b: Point::new(20.0, 16.0),
        },
        Geometry::Circle {
            center: Point::new(10.0, 10.0),
            radius: 9.0,
        },
    ] {
        let job = pocket(geometry.clone());
        let before = job.clone();
        let path = job.plan().unwrap();
        assert_eq!(path.passes, 3);
        let r = job.tool_diameter / 2.0;
        for motion in path.motions.iter().filter(|m| !m.rapid) {
            let [x, y, _] = motion.position;
            match geometry {
                Geometry::Rectangle { .. } => {
                    assert!(x >= r - 1e-9 && x <= 20.0 - r + 1e-9);
                    assert!(y >= r - 1e-9 && y <= 16.0 - r + 1e-9);
                }
                Geometry::Circle { center, radius } => {
                    assert!(Point::new(x, y).distance(center) <= radius - r + 1e-9)
                }
                _ => unreachable!(),
            }
        }
        for pair in path.motions.windows(2) {
            if pair[1].rapid && pair[0].position[2] < job.safe_z {
                assert_eq!(
                    pair[0].position[..2],
                    pair[1].position[..2],
                    "retract must be vertical"
                );
            }
            if pair[1].rapid && pair[0].position[..2] != pair[1].position[..2] {
                near(pair[0].position[2], job.safe_z);
                near(pair[1].position[2], job.safe_z);
            }
        }
        for x in 2..19 {
            for y in 2..15 {
                let p = Point::new(x as f64, y as f64);
                if let Geometry::Circle { center, radius } = geometry {
                    if p.distance(center) > radius - 0.1 {
                        continue;
                    }
                }
                let cleared = path.motions.windows(2).any(|pair| {
                    if pair.iter().any(|m| m.rapid || m.position[2] != -job.depth) {
                        return false;
                    }
                    distance_segment(
                        p,
                        Point::new(pair[0].position[0], pair[0].position[1]),
                        Point::new(pair[1].position[0], pair[1].position[1]),
                    ) <= r + 1e-8
                });
                assert!(cleared, "uncleared interior {p:?} in {geometry:?}");
            }
        }
        assert!(job.gcode().unwrap().contains("G17 G21 G90 G94"));
        assert_eq!(job, before);
    }
}

fn boundary_distance(point: Point, contour: &[Point]) -> f64 {
    (0..contour.len())
        .map(|index| distance_segment(point, contour[index], contour[(index + 1) % contour.len()]))
        .fold(f64::INFINITY, f64::min)
}

fn inside_contour(point: Point, contour: &[Point]) -> bool {
    let mut inside = false;
    for index in 0..contour.len() {
        let a = contour[index];
        let b = contour[(index + 1) % contour.len()];
        if (a.y > point.y) != (b.y > point.y) {
            let x = a.x + (point.y - a.y) * (b.x - a.x) / (b.y - a.y);
            if x > point.x {
                inside = !inside;
            }
        }
    }
    inside
}

fn cleared_by_final_depth(
    path: &forge_core::cam::Toolpath,
    depth: f64,
    point: Point,
    r: f64,
) -> bool {
    path.motions.windows(2).any(|pair| {
        if pair
            .iter()
            .any(|motion| motion.rapid || motion.position[2] != depth)
        {
            return false;
        }
        distance_segment(
            point,
            Point::new(pair[0].position[0], pair[0].position[1]),
            Point::new(pair[1].position[0], pair[1].position[1]),
        ) <= r + 1e-8
    })
}

#[test]
fn polygon_pockets_raster_clear_concave_interiors_and_finish_walls() {
    let contour = vec![
        Point::new(0.0, 0.0),
        Point::new(30.0, 0.0),
        Point::new(30.0, 10.0),
        Point::new(10.0, 10.0),
        Point::new(10.0, 30.0),
        Point::new(0.0, 30.0),
    ];
    let job = pocket(Geometry::Polyline {
        vertices: contour
            .iter()
            .copied()
            .map(forge_core::PolylineVertex::new)
            .collect(),
        closed: true,
    });
    let path = job.plan().unwrap();
    assert!(path.passes > 3, "disconnected raster rows are passes");
    let r = job.tool_diameter / 2.0;
    for motion in path.motions.iter().filter(|motion| !motion.rapid) {
        let point = Point::new(motion.position[0], motion.position[1]);
        assert!(boundary_distance(point, &contour) >= r - 1e-8);
    }
    for x in 1..30 {
        for y in 1..30 {
            let point = Point::new(x as f64, y as f64);
            if !inside_contour(point, &contour) || boundary_distance(point, &contour) < r + 0.2 {
                continue;
            }
            assert!(
                cleared_by_final_depth(&path, -job.depth, point, r),
                "polygon interior not cleared at {point:?}"
            );
        }
    }
    assert!(job.gcode().unwrap().contains("G17 G21 G90 G94"));
}

#[test]
fn pockets_with_islands_leave_islands_uncut_and_clear_remaining_material() {
    let island_center = Point::new(20.0, 15.0);
    let island_radius = 5.0;
    let outer = Geometry::Rectangle {
        a: Point::new(0.0, 0.0),
        b: Point::new(40.0, 30.0),
    };
    let mut job = pocket(outer.clone());
    job.profiles[0].islands.push(Geometry::Circle {
        center: island_center,
        radius: island_radius,
    });
    let before = job.clone();
    let path = job.plan().unwrap();
    let r = job.tool_diameter / 2.0;
    assert!(path.passes > 3);
    for pair in path.motions.windows(2) {
        if pair
            .iter()
            .any(|motion| motion.rapid || motion.position[2] != -job.depth)
        {
            continue;
        }
        let start = Point::new(pair[0].position[0], pair[0].position[1]);
        let end = Point::new(pair[1].position[0], pair[1].position[1]);
        assert!(
            distance_segment(island_center, start, end) >= island_radius + r - 1e-8,
            "cutting move {start:?} -> {end:?} entered the island at distance {}",
            distance_segment(island_center, start, end)
        );
    }
    for x in 1..40 {
        for y in 1..30 {
            let point = Point::new(x as f64, y as f64);
            if point.distance(island_center) < island_radius + r + 0.2
                || point.x < r + 0.2
                || point.x > 40.0 - r - 0.2
                || point.y < r + 0.2
                || point.y > 30.0 - r - 0.2
            {
                continue;
            }
            assert!(
                cleared_by_final_depth(&path, -job.depth, point, r),
                "material around island not cleared at {point:?}"
            );
        }
    }
    assert_eq!(job, before);
    assert!(job.gcode().unwrap().contains("G1"));
}

#[test]
fn pocket_island_validation_rejects_invalid_topology_and_unsupported_shapes() {
    let mut job = pocket(Geometry::Rectangle {
        a: Point::new(0.0, 0.0),
        b: Point::new(40.0, 30.0),
    });
    let valid_island = Geometry::Circle {
        center: Point::new(20.0, 15.0),
        radius: 5.0,
    };
    job.profiles[0].islands = vec![valid_island.clone()];
    assert!(job.plan().is_ok());

    job.profiles[0].operation = Operation::Profile;
    assert!(job.plan().is_err());
    job.profiles[0].operation = Operation::Pocket;

    job.profiles[0].islands[0].clone_from(&Geometry::Circle {
        center: Point::new(50.0, 15.0),
        radius: 5.0,
    });
    assert!(job.plan().unwrap_err().to_string().contains("inside"));
    job.profiles[0].islands[0].clone_from(&Geometry::Circle {
        center: Point::new(20.0, 15.0),
        radius: 20.0,
    });
    assert!(job.plan().is_err());
    job.profiles[0].islands = vec![
        Geometry::Circle {
            center: Point::new(10.0, 15.0),
            radius: 5.0,
        },
        Geometry::Circle {
            center: Point::new(10.0, 15.0),
            radius: 5.0,
        },
    ];
    assert!(job.plan().unwrap_err().to_string().contains("overlap"));
    job.profiles[0].islands = (0..65)
        .map(|index| Geometry::Circle {
            center: Point::new(
                2.0 + (index % 13) as f64 * 2.5,
                2.0 + (index / 13) as f64 * 5.0,
            ),
            radius: 0.2,
        })
        .collect();
    assert!(job.plan().unwrap_err().to_string().contains("64"));
    job.profiles[0].islands = vec![Geometry::Polyline {
        vertices: vec![
            forge_core::PolylineVertex::new(Point::new(15.0, 10.0)),
            forge_core::PolylineVertex::new(Point::new(25.0, 10.0)),
        ],
        closed: false,
    }];
    assert!(job.plan().is_err());
    job.profiles[0].islands = vec![Geometry::Polyline {
        vertices: vec![
            forge_core::PolylineVertex::new(Point::new(15.0, 10.0)),
            forge_core::PolylineVertex {
                point: Point::new(25.0, 10.0),
                bulge: 1.0,
            },
            forge_core::PolylineVertex::new(Point::new(25.0, 20.0)),
        ],
        closed: true,
    }];
    assert!(job.plan().is_err());
}

#[test]
fn machining_islands_round_trip_and_old_profiles_default_to_none() {
    let mut job = pocket(Geometry::Rectangle {
        a: Point::new(0.0, 0.0),
        b: Point::new(40.0, 30.0),
    });
    job.profiles[0].islands = vec![Geometry::Circle {
        center: Point::new(20.0, 15.0),
        radius: 5.0,
    }];
    let json = serde_json::to_string(&job).unwrap();
    assert_eq!(serde_json::from_str::<CamJob>(&json).unwrap(), job);
    let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(
        value["profiles"][0]
            .as_object_mut()
            .unwrap()
            .remove("islands")
            .is_some()
    );
    let legacy: CamJob = serde_json::from_value(value).unwrap();
    assert!(legacy.profiles[0].islands.is_empty());
}

#[test]
fn peck_drilling_uses_fixed_xy_incremental_depths_and_clearance_between_pecks() {
    let mut job = CamJob::default();
    job.depth = 2.5;
    job.step_down = 1.0;
    job.profiles.push(Profile {
        id: 1,
        name: "Drill".into(),
        geometry: Geometry::Point {
            position: Point::new(20.0, 30.0),
        },
        operation: Operation::Drill,
        compensation: Compensation::Center,
        islands: Vec::new(),
    });
    let path = job.plan().unwrap();
    let cuts: Vec<_> = path.motions.iter().filter(|m| !m.rapid).collect();
    assert_eq!(
        cuts.iter().map(|m| m.position[2]).collect::<Vec<_>>(),
        vec![-1.0, -2.0, -2.5]
    );
    for m in cuts {
        assert_eq!(m.position[..2], [20.0, 30.0]);
        near(m.feed, job.plunge_feed);
    }
    assert_eq!(path.passes, 3);
    let program = job.gcode().unwrap();
    assert!(program.contains("G1 X20 Y30 Z-2.5"));
    assert!(program.ends_with("M5\nM2\n"));
}

#[test]
fn unsupported_or_excessive_machining_is_rejected_before_output() {
    let mut job = pocket(Geometry::Rectangle {
        a: Point::default(),
        b: Point::new(2.0, 20.0),
    });
    assert!(job.plan().is_err());
    job.profiles[0].geometry = Geometry::Rectangle {
        a: Point::default(),
        b: Point::new(20.0, 20.0),
    };
    job.step_over = job.tool_diameter + 0.1;
    assert!(job.plan().is_err());
    job.step_over = 0.000001;
    assert!(job.plan().is_err());
    job.step_over = 1.0;
    job.profiles[0].operation = Operation::Drill;
    assert!(job.gcode().is_err());
    job.profiles[0].geometry = Geometry::Point {
        position: Point::new(110.0, 90.0),
    };
    job.profiles[0].compensation = Compensation::Center;
    assert!(job.plan().is_err());
}

#[test]
fn layouts_preserve_multiple_clipped_scales_paper_notes_and_viewport_layer_filters() {
    let mut document = Document::new("Layout <drawing>");
    document.add_geometry(Geometry::Line {
        start: Point::default(),
        end: Point::new(100.0, 100.0),
    });
    let hidden = document.add_layer("Detail & hatch", [255, 0, 0]);
    let hidden_entity = document.add_geometry(Geometry::Circle {
        center: Point::default(),
        radius: 5.0,
    });
    let mut layout = PrintLayout::fitted("Two views", &document, SvgOptions::default()).unwrap();
    layout.viewports[0].paper_size = [100.0, 100.0];
    layout.viewports[0].scale = 1.0;
    layout.viewports[0].hidden_layer_ids.push(hidden);
    let mut detail = layout.viewports[0].clone();
    detail.name = "Detail".into();
    detail.paper_position = Point::new(150.0, 10.0);
    detail.scale = 2.0;
    detail.hidden_layer_ids.clear();
    layout.viewports.push(detail);
    layout.notes.push(PaperText {
        position: Point::new(20.0, 190.0),
        text: "Title <A & B>".into(),
        height_mm: 4.0,
    });
    document.layouts.push(layout);
    let before = document.clone();
    let output = export_layout_svg(&document, "two VIEWS").unwrap();
    assert!(output.contains("matrix(1 0 0 -1"));
    assert!(output.contains("matrix(2 0 0 -2"));
    assert!(output.contains("clip-path=\"url(#viewport-0)\""));
    assert!(output.contains("clip-path=\"url(#viewport-1)\""));
    assert_eq!(
        output
            .matches(&format!("data-source-id=\"{hidden_entity}\""))
            .count(),
        1
    );
    assert!(output.contains("Title &lt;A &amp; B&gt;"));
    let restored = Document::from_json(&document.to_json().unwrap()).unwrap();
    assert_eq!(restored, document);
    assert_eq!(export_layout_svg(&restored, "Two views").unwrap(), output);
    assert_eq!(document, before);
    assert!(forge_core::dxf::write(&document).is_err());
    let mut invalid = document.clone();
    invalid.layouts[0].viewports[0].scale = 0.0;
    assert!(invalid.to_json().is_err());
    let mut invalid = document.clone();
    invalid.layouts[0].viewports[0].paper_position.x = 10000.0;
    assert!(invalid.to_json().is_err());
    let mut invalid = document.clone();
    invalid.layouts[0].viewports[0].hidden_layer_ids = vec![999];
    assert!(invalid.to_json().is_err());
    let mut invalid = document;
    invalid.layouts.push(invalid.layouts[0].clone());
    assert!(invalid.validate().is_err());
}

#[test]
fn project_edits_are_atomic_and_v4_files_receive_compatible_defaults() {
    let mut document = Document::default();
    document.scene = Scene::example();
    document.manufacturing = CamJob::example();
    document.terrain = Terrain::example();
    let before = document.clone();
    let edits = vec![
        ProjectEdit::Alignment {
            alignment: alignment(),
        },
        ProjectEdit::SceneParent {
            object_id: 2,
            parent_id: Some(1),
        },
        ProjectEdit::SceneParent {
            object_id: 1,
            parent_id: Some(2),
        },
    ];
    assert!(apply_edits(&mut document, &edits).is_err());
    assert_eq!(document, before);
    apply_edits(&mut document, &edits[..2]).unwrap();
    assert_eq!(document.terrain.alignments.len(), 1);
    assert_eq!(document.scene.objects[1].parent_id, Some(1));
    let saved = Document::from_json(&document.to_json().unwrap()).unwrap();
    assert_eq!(saved, document);
    let mut legacy = serde_json::to_value(
        serde_json::from_str::<serde_json::Value>(&before.to_json().unwrap()).unwrap(),
    )
    .unwrap();
    legacy["format_version"] = 4.into();
    let doc = legacy["document"].as_object_mut().unwrap();
    doc.remove("layouts");
    doc["terrain"].as_object_mut().unwrap().remove("alignments");
    for object in doc["scene"]["objects"].as_array_mut().unwrap() {
        object.as_object_mut().unwrap().remove("parent_id");
    }
    doc["manufacturing"]
        .as_object_mut()
        .unwrap()
        .remove("step_over");
    for profile in doc["manufacturing"]["profiles"].as_array_mut().unwrap() {
        profile.as_object_mut().unwrap().remove("operation");
    }
    let migrated = Document::from_json(&serde_json::to_string(&legacy).unwrap()).unwrap();
    assert_eq!(migrated, before);
    assert!(
        serde_json::from_str::<ProjectEdit>(
            "{\"type\":\"scene_parent\",\"object_id\":1,\"parent_id\":null,\"typo\":true}"
        )
        .is_err()
    );
}
