//! Solver semantics for persistent sketch relations: each relation type,
//! conflict and redundancy reporting, minimum-displacement solving, geometry
//! and numeric limits, deletion guards, serialization and bounds.
use forge_core::PROJECT_FORMAT_VERSION;
use forge_core::constraints::{ConstraintState, PointHandle, PointRef, Relation, SketchConstraint};
use forge_core::{Document, Geometry, Point, PolylineVertex};
use serde_json::{Value, json};

fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}
fn entity(document: &Document, id: u64) -> &Geometry {
    &document
        .entities
        .iter()
        .find(|entity| entity.id == id)
        .expect("entity exists")
        .geometry
}
fn line(document: &mut Document, start: Point, end: Point) -> u64 {
    document
        .add_geometry(Geometry::Line { start, end })
        .expect("line")
}
fn rectangle(document: &mut Document, a: Point, b: Point) -> u64 {
    document
        .add_geometry(Geometry::Rectangle { a, b })
        .expect("rectangle")
}
fn circle(document: &mut Document, center: Point, radius: f64) -> u64 {
    document
        .add_geometry(Geometry::Circle { center, radius })
        .expect("circle")
}
fn point_entity(document: &mut Document, position: Point) -> u64 {
    document
        .add_geometry(Geometry::Point { position })
        .expect("point")
}
fn polyline(document: &mut Document, vertices: Vec<Point>) -> u64 {
    document
        .add_geometry(Geometry::Polyline {
            vertices: vertices.into_iter().map(PolylineVertex::new).collect(),
            closed: false,
        })
        .expect("polyline")
}
fn relate(document: &mut Document, relation: Relation) -> u64 {
    let id = document.next_constraint_id();
    let report = document
        .set_constraint(SketchConstraint { id, relation })
        .expect("relation is accepted");
    assert!(report.solved(), "{report:?}");
    id
}
fn reference(entity: u64, handle: PointHandle) -> PointRef {
    PointRef { entity, handle }
}
fn start(entity: u64) -> PointRef {
    reference(entity, PointHandle::Start)
}
fn end(entity: u64) -> PointRef {
    reference(entity, PointHandle::End)
}
fn center(entity: u64) -> PointRef {
    reference(entity, PointHandle::Center)
}
fn corner_a(entity: u64) -> PointRef {
    reference(entity, PointHandle::CornerA)
}
fn corner_b(entity: u64) -> PointRef {
    reference(entity, PointHandle::CornerB)
}

#[test]
fn relation_descriptions_are_stable_one_liners() {
    let cases: Vec<(Relation, &str)> = vec![
        (Relation::Horizontal { entity: 2 }, "Horizontal #2"),
        (Relation::Vertical { entity: 7 }, "Vertical #7"),
        (
            Relation::Coincident {
                a: start(3),
                b: end(4),
            },
            "Coincident #3.start→#4.end",
        ),
        (
            Relation::FixPoint {
                point: start(3),
                position: Point::new(10.0, 20.0),
            },
            "FixPoint #3.start@(10,20)",
        ),
        (
            Relation::DistanceX {
                a: end(3),
                b: start(4),
                value: 50.0,
            },
            "DistanceX 50 #3.end→#4.start",
        ),
        (
            Relation::DistanceY {
                a: end(3),
                b: start(4),
                value: 12.5,
            },
            "DistanceY 12.5 #3.end→#4.start",
        ),
        (
            Relation::Radius {
                entity: 5,
                value: 25.0,
            },
            "Radius 25 #5",
        ),
        (
            Relation::EqualRadius {
                first: 5,
                second: 6,
            },
            "EqualRadius #5=#6",
        ),
        (
            Relation::Coincident {
                a: reference(9, PointHandle::Vertex(2)),
                b: reference(9, PointHandle::CornerA),
            },
            "Coincident #9.vertex2→#9.cornera",
        ),
    ];
    for (relation, expected) in cases {
        assert_eq!(relation.describe(), expected);
    }
}

#[test]
fn geometric_relations_solve_to_their_expected_geometry() {
    // Horizontal shares its correction between both handles.
    let mut document = Document::default();
    let sloped = line(&mut document, Point::new(0.0, 0.0), Point::new(100.0, 20.0));
    relate(&mut document, Relation::Horizontal { entity: sloped });
    match entity(&document, sloped) {
        Geometry::Line { start, end } => {
            near(start.x, 0.0);
            near(start.y, 10.0);
            near(end.x, 100.0);
            near(end.y, 10.0);
        }
        other => panic!("expected a line: {other:?}"),
    }

    // Vertical makes the shared coordinate exact as well.
    let mut document = Document::default();
    let leaning = line(&mut document, Point::new(10.0, 0.0), Point::new(30.0, 40.0));
    relate(&mut document, Relation::Vertical { entity: leaning });
    match entity(&document, leaning) {
        Geometry::Line { start, end } => {
            near(start.x, 20.0);
            near(end.x, 20.0);
            near(start.y, 0.0);
            near(end.y, 40.0);
        }
        other => panic!("expected a line: {other:?}"),
    }

    // Coincident joins two endpoints exactly, moving both towards each other.
    let mut document = Document::default();
    let first = line(&mut document, Point::new(0.0, 0.0), Point::new(10.0, 0.0));
    let second = line(&mut document, Point::new(10.0, 5.0), Point::new(20.0, 5.0));
    relate(
        &mut document,
        Relation::Coincident {
            a: end(first),
            b: start(second),
        },
    );
    match entity(&document, first) {
        Geometry::Line { end: first_end, .. } => near(first_end.y, 2.5),
        other => panic!("expected a line: {other:?}"),
    }
    match entity(&document, second) {
        Geometry::Line {
            start: second_start,
            ..
        } => near(second_start.y, 2.5),
        other => panic!("expected a line: {other:?}"),
    }

    // Rectangle corners and polyline vertices are valid handles.
    let mut document = Document::default();
    let rect = rectangle(&mut document, Point::new(0.0, 0.0), Point::new(40.0, 20.0));
    relate(
        &mut document,
        Relation::Coincident {
            a: corner_a(rect),
            b: corner_b(rect),
        },
    );
    match entity(&document, rect) {
        Geometry::Rectangle { a, b } => {
            near(a.x, 20.0);
            near(a.y, 10.0);
            near(b.x, 20.0);
            near(b.y, 10.0);
            assert!(a.distance(*b) < 1e-9);
        }
        other => panic!("expected a rectangle: {other:?}"),
    }

    let mut document = Document::default();
    let poly = polyline(
        &mut document,
        vec![
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
        ],
    );
    relate(
        &mut document,
        Relation::Coincident {
            a: reference(poly, PointHandle::Vertex(0)),
            b: reference(poly, PointHandle::Vertex(2)),
        },
    );
    match entity(&document, poly) {
        Geometry::Polyline { vertices, .. } => {
            near(vertices[0].point.x, 5.0);
            near(vertices[0].point.y, 5.0);
            near(vertices[2].point.x, 5.0);
            near(vertices[2].point.y, 5.0);
        }
        other => panic!("expected a polyline: {other:?}"),
    }

    // A point entity is pinned by its position handle.
    let mut document = Document::default();
    let marker = point_entity(&mut document, Point::new(3.0, 4.0));
    let target = point_entity(&mut document, Point::new(30.0, 4.0));
    relate(
        &mut document,
        Relation::Coincident {
            a: reference(marker, PointHandle::Position),
            b: reference(target, PointHandle::Position),
        },
    );
    for id in [marker, target] {
        match entity(&document, id) {
            Geometry::Point { position } => {
                near(position.x, 16.5);
                near(position.y, 4.0);
            }
            other => panic!("expected a point: {other:?}"),
        }
    }
}

#[test]
fn dimensional_relations_solve_to_their_expected_values() {
    // FixPoint pins a single handle without dragging the rest of the curve.
    let mut document = Document::default();
    let line_id = line(&mut document, Point::new(0.0, 0.0), Point::new(10.0, 20.0));
    relate(
        &mut document,
        Relation::FixPoint {
            point: end(line_id),
            position: Point::new(30.0, 40.0),
        },
    );
    assert_eq!(
        entity(&document, line_id),
        &Geometry::Line {
            start: Point::new(0.0, 0.0),
            end: Point::new(30.0, 40.0),
        }
    );

    // DistanceX/DistanceY share the correction between the two handles.
    let mut document = Document::default();
    let left = line(&mut document, Point::new(0.0, 0.0), Point::new(10.0, 0.0));
    let right = line(&mut document, Point::new(20.0, 0.0), Point::new(30.0, 0.0));
    relate(
        &mut document,
        Relation::DistanceX {
            a: start(left),
            b: start(right),
            value: 50.0,
        },
    );
    match entity(&document, left) {
        Geometry::Line { start, .. } => near(start.x, -15.0),
        other => panic!("expected a line: {other:?}"),
    }
    match entity(&document, right) {
        Geometry::Line { start, .. } => near(start.x, 35.0),
        other => panic!("expected a line: {other:?}"),
    }

    let mut document = Document::default();
    let low = line(&mut document, Point::new(0.0, 0.0), Point::new(10.0, 0.0));
    let high = line(
        &mut document,
        Point::new(20.0, 10.0),
        Point::new(30.0, 10.0),
    );
    relate(
        &mut document,
        Relation::DistanceY {
            a: start(low),
            b: start(high),
            value: 30.0,
        },
    );
    match entity(&document, low) {
        Geometry::Line { start, .. } => near(start.y, -10.0),
        other => panic!("expected a line: {other:?}"),
    }
    match entity(&document, high) {
        Geometry::Line { start, .. } => near(start.y, 20.0),
        other => panic!("expected a line: {other:?}"),
    }

    // Rectangle dimensions measure between the two corners.
    let mut document = Document::default();
    let rect = rectangle(&mut document, Point::new(0.0, 0.0), Point::new(40.0, 20.0));
    relate(
        &mut document,
        Relation::DistanceX {
            a: corner_a(rect),
            b: corner_b(rect),
            value: 40.0,
        },
    );
    relate(
        &mut document,
        Relation::DistanceY {
            a: corner_a(rect),
            b: corner_b(rect),
            value: 20.0,
        },
    );
    assert_eq!(
        entity(&document, rect),
        &Geometry::Rectangle {
            a: Point::new(0.0, 0.0),
            b: Point::new(40.0, 20.0),
        }
    );

    // Radius replaces the dimension; EqualRadius meets the two circles halfway.
    let mut document = Document::default();
    let round = circle(&mut document, Point::new(5.0, 6.0), 10.0);
    relate(
        &mut document,
        Relation::Radius {
            entity: round,
            value: 25.0,
        },
    );
    assert_eq!(
        entity(&document, round),
        &Geometry::Circle {
            center: Point::new(5.0, 6.0),
            radius: 25.0,
        }
    );

    let mut document = Document::default();
    let small = circle(&mut document, Point::new(0.0, 0.0), 5.0);
    let large = circle(&mut document, Point::new(20.0, 0.0), 10.0);
    relate(
        &mut document,
        Relation::EqualRadius {
            first: small,
            second: large,
        },
    );
    match entity(&document, small) {
        Geometry::Circle { center, radius } => {
            assert_eq!(*center, Point::new(0.0, 0.0));
            near(*radius, 7.5);
        }
        other => panic!("expected a circle: {other:?}"),
    }
    match entity(&document, large) {
        Geometry::Circle { center, radius } => {
            assert_eq!(*center, Point::new(20.0, 0.0));
            near(*radius, 7.5);
        }
        other => panic!("expected a circle: {other:?}"),
    }

    // A degenerate radius and an unsatisfiable dimension are refused.
    let mut document = Document::default();
    let round = circle(&mut document, Point::new(0.0, 0.0), 5.0);
    let line_id = line(&mut document, Point::new(0.0, 20.0), Point::new(10.0, 20.0));
    assert!(
        document
            .set_constraint(SketchConstraint {
                id: 1,
                relation: Relation::Radius {
                    entity: round,
                    value: 0.0
                },
            })
            .is_err()
    );
    assert!(
        document
            .set_constraint(SketchConstraint {
                id: 1,
                relation: Relation::Radius {
                    entity: round,
                    value: 1e-7
                },
            })
            .is_err()
    );
    assert!(
        document
            .set_constraint(SketchConstraint {
                id: 1,
                relation: Relation::DistanceX {
                    a: start(line_id),
                    b: end(line_id),
                    value: 1e12,
                },
            })
            .is_err()
    );
    assert!(document.constraints.is_empty());
    assert_eq!(
        entity(&document, round),
        &Geometry::Circle {
            center: Point::new(0.0, 0.0),
            radius: 5.0,
        }
    );
}

#[test]
fn contradictory_systems_report_conflicts_and_leave_the_document_unchanged() {
    let mut document = Document::default();
    let left = line(&mut document, Point::new(0.0, 0.0), Point::new(10.0, 0.0));
    let right = line(&mut document, Point::new(20.0, 0.0), Point::new(30.0, 0.0));
    relate(
        &mut document,
        Relation::DistanceX {
            a: start(left),
            b: start(right),
            value: 50.0,
        },
    );
    let before = document.to_json().unwrap();
    // A second, incompatible dimension is reported as a conflict and never commits.
    let report = document
        .set_constraints(vec![
            SketchConstraint {
                id: 1,
                relation: Relation::DistanceX {
                    a: start(left),
                    b: start(right),
                    value: 50.0,
                },
            },
            SketchConstraint {
                id: 2,
                relation: Relation::DistanceX {
                    a: start(left),
                    b: start(right),
                    value: 70.0,
                },
            },
        ])
        .unwrap();
    assert_eq!(report.state, ConstraintState::Conflict);
    assert_eq!(report.conflicts, [1, 2]);
    assert!(!report.solved());
    assert_eq!(document.to_json().unwrap(), before);
    assert_eq!(document.constraints.len(), 1);
    // Solving the same conflicting set leaves the geometry untouched.
    document.constraints = vec![
        SketchConstraint {
            id: 1,
            relation: Relation::DistanceX {
                a: start(left),
                b: start(right),
                value: 50.0,
            },
        },
        SketchConstraint {
            id: 2,
            relation: Relation::DistanceX {
                a: start(left),
                b: start(right),
                value: 70.0,
            },
        },
    ];
    let geometry = document.entities.clone();
    let report = document.solve_constraints().unwrap();
    assert_eq!(report.state, ConstraintState::Conflict);
    assert_eq!(report.conflicts, [1, 2]);
    assert_eq!(report.max_error, 70.0 - 50.0);
    assert_eq!(document.entities, geometry);
    // A fixed point that contradicts a horizontal relation also conflicts.
    document.constraints.clear();
    relate(&mut document, Relation::Horizontal { entity: left });
    let report = document
        .set_constraints(vec![
            SketchConstraint {
                id: 1,
                relation: Relation::Horizontal { entity: left },
            },
            SketchConstraint {
                id: 2,
                relation: Relation::Vertical { entity: left },
            },
            SketchConstraint {
                id: 3,
                relation: Relation::FixPoint {
                    point: start(left),
                    position: Point::new(0.0, 5.0),
                },
            },
            SketchConstraint {
                id: 4,
                relation: Relation::FixPoint {
                    point: end(left),
                    position: Point::new(10.0, 9.0),
                },
            },
        ])
        .unwrap();
    assert_eq!(report.state, ConstraintState::Conflict);
    assert!(!report.conflicts.is_empty());
}

#[test]
fn underconstrained_sketches_stay_satisfied_and_move_minimally() {
    let mut document = Document::default();
    let sloped = line(&mut document, Point::new(0.0, 0.0), Point::new(100.0, 20.0));
    let free = line(
        &mut document,
        Point::new(200.0, 30.0),
        Point::new(260.0, 90.0),
    );
    let report = document.constraint_report().unwrap();
    assert_eq!(report.state, ConstraintState::Satisfied);
    assert_eq!(report.constraints, 0);
    assert_eq!(report.degrees_of_freedom, 0);
    relate(&mut document, Relation::Horizontal { entity: sloped });
    let report = document.constraint_report().unwrap();
    assert_eq!(report.state, ConstraintState::Satisfied);
    assert_eq!(report.constraints, 1);
    assert_eq!(report.variables, 4);
    assert!(report.degrees_of_freedom > 0, "{report:?}");
    match entity(&document, sloped) {
        Geometry::Line { start, end } => {
            // The minimum-displacement solution splits the correction in half.
            near(start.y, 10.0);
            near(end.y, 10.0);
            near(start.x, 0.0);
            near(end.x, 100.0);
        }
        other => panic!("expected a line: {other:?}"),
    }
    // An entity outside the constraint system never moves.
    assert_eq!(
        entity(&document, free),
        &Geometry::Line {
            start: Point::new(200.0, 30.0),
            end: Point::new(260.0, 90.0),
        }
    );
    // An already satisfied relation moves nothing at all.
    let mut document = Document::default();
    let flat = line(&mut document, Point::new(0.0, 0.0), Point::new(10.0, 0.0));
    relate(&mut document, Relation::Horizontal { entity: flat });
    let report = document.constraint_report().unwrap();
    assert_eq!(report.state, ConstraintState::Satisfied);
    assert_eq!(report.max_error, 0.0);
    assert_eq!(
        entity(&document, flat),
        &Geometry::Line {
            start: Point::new(0.0, 0.0),
            end: Point::new(10.0, 0.0),
        }
    );
}

#[test]
fn duplicate_relations_are_redundant_rather_than_conflicting() {
    let mut document = Document::default();
    let line_id = line(&mut document, Point::new(0.0, 0.0), Point::new(10.0, 4.0));
    let report = document
        .set_constraints(vec![
            SketchConstraint {
                id: 1,
                relation: Relation::Horizontal { entity: line_id },
            },
            SketchConstraint {
                id: 2,
                relation: Relation::Horizontal { entity: line_id },
            },
            SketchConstraint {
                id: 3,
                relation: Relation::Horizontal { entity: line_id },
            },
        ])
        .unwrap();
    assert_eq!(report.state, ConstraintState::Satisfied);
    assert!(report.conflicts.is_empty());
    assert!(report.solved());
    assert_eq!(report.redundant_equations, 2);
    assert_eq!(report.constraints, 3);
    assert_eq!(report.rank, 1);
    assert_eq!(document.constraints.len(), 3);
    match entity(&document, line_id) {
        Geometry::Line { start, end } => {
            near(start.y, 2.0);
            near(end.y, 2.0);
        }
        other => panic!("expected a line: {other:?}"),
    }
}

#[test]
fn unsupported_geometry_and_handles_are_rejected() {
    let mut document = Document::default();
    let arc = document
        .add_geometry(Geometry::Arc {
            center: Point::new(0.0, 0.0),
            radius: 5.0,
            start_angle_deg: 0.0,
            end_angle_deg: 90.0,
        })
        .unwrap();
    let text = document
        .add_geometry(Geometry::Text {
            position: Point::new(10.0, 0.0),
            text: "note".into(),
            height: 2.0,
            rotation_deg: 0.0,
        })
        .unwrap();
    let insert = document
        .add_geometry(Geometry::Insert {
            block_name: "Missing block".into(),
            position: Point::new(20.0, 0.0),
            scale: 1.0,
            rotation_deg: 0.0,
        })
        .unwrap();
    let dimension = document
        .add_geometry(Geometry::Dimension {
            dimension: forge_core::dimension::Dimension::Linear(
                forge_core::dimension::LinearDimension {
                    kind: forge_core::dimension::LinearDimensionKind::Horizontal,
                    a: Point::new(0.0, 0.0),
                    b: Point::new(3.0, 4.0),
                    line_point: Point::new(7.0, 7.0),
                    style: forge_core::dimension::DimensionStyle::default(),
                },
            ),
        })
        .unwrap();
    // These kinds expose no sketch handles at all.
    for geometry in [
        entity(&document, arc),
        entity(&document, text),
        entity(&document, insert),
    ] {
        assert!(
            forge_core::constraints::handles(geometry).is_empty(),
            "{geometry:?}"
        );
    }
    for relation in [
        Relation::Horizontal { entity: arc },
        Relation::Vertical { entity: text },
        Relation::Horizontal { entity: insert },
        Relation::Vertical { entity: dimension },
    ] {
        let error = document
            .set_constraint(SketchConstraint { id: 1, relation })
            .unwrap_err();
        assert!(
            error.to_string().contains("Sketch constraints support"),
            "{error}"
        );
    }
    assert!(document.constraints.is_empty());

    // polylines beyond 64 vertices or with bulges cannot be solved
    let mut document = Document::default();
    let long = polyline(
        &mut document,
        (0..65).map(|index| Point::new(index as f64, 0.0)).collect(),
    );
    assert!(forge_core::constraints::handles(entity(&document, long)).is_empty());
    let error = document
        .set_constraint(SketchConstraint {
            id: 1,
            relation: Relation::Horizontal { entity: long },
        })
        .unwrap_err();
    assert!(error.to_string().contains("at most 64 vertices"), "{error}");

    let mut document = Document::default();
    let curved = document
        .add_geometry(Geometry::Polyline {
            vertices: vec![
                PolylineVertex {
                    point: Point::new(0.0, 0.0),
                    bulge: 1.0,
                },
                PolylineVertex::new(Point::new(10.0, 0.0)),
            ],
            closed: false,
        })
        .unwrap();
    let error = document
        .set_constraint(SketchConstraint {
            id: 1,
            relation: Relation::Horizontal { entity: curved },
        })
        .unwrap_err();
    assert!(error.to_string().contains("straight polylines"), "{error}");

    // Handles must exist on the entity they name.
    let mut document = Document::default();
    let line_id = line(&mut document, Point::new(0.0, 0.0), Point::new(10.0, 0.0));
    let round = circle(&mut document, Point::new(30.0, 0.0), 5.0);
    let poly = polyline(
        &mut document,
        vec![Point::new(0.0, 20.0), Point::new(10.0, 20.0)],
    );
    for relation in [
        Relation::Coincident {
            a: end(line_id),
            b: center(line_id),
        },
        Relation::Coincident {
            a: end(line_id),
            b: end(round),
        },
        Relation::Coincident {
            a: end(line_id),
            b: reference(poly, PointHandle::Vertex(4)),
        },
        Relation::EqualRadius {
            first: line_id,
            second: round,
        },
        Relation::Radius {
            entity: line_id,
            value: 4.0,
        },
    ] {
        let error = document
            .set_constraint(SketchConstraint { id: 1, relation })
            .unwrap_err();
        assert!(
            error.to_string().contains("handle")
                || error.to_string().contains("require a LINE")
                || error.to_string().contains("require a CIRCLE"),
            "{error}"
        );
    }
    assert!(document.constraints.is_empty());

    // A relation may only name entities of its own document.
    let mut source = Document::default();
    let foreign = line(&mut source, Point::new(0.0, 0.0), Point::new(10.0, 0.0));
    let mut document = Document::default();
    let error = document
        .set_constraint(SketchConstraint {
            id: 1,
            relation: Relation::Horizontal { entity: foreign },
        })
        .unwrap_err();
    assert!(error.to_string().contains("missing entity"), "{error}");
    // Duplicate and zero constraint IDs are refused as well.
    let line_id = line(&mut document, Point::new(0.0, 0.0), Point::new(10.0, 0.0));
    let error = document
        .set_constraints(vec![
            SketchConstraint {
                id: 1,
                relation: Relation::Horizontal { entity: line_id },
            },
            SketchConstraint {
                id: 1,
                relation: Relation::Vertical { entity: line_id },
            },
        ])
        .unwrap_err();
    assert!(error.to_string().contains("unique and nonzero"), "{error}");
    assert!(document.constraints.is_empty());
}

#[test]
fn constraint_limits_are_enforced() {
    // 256 constraints is the documented ceiling.
    let mut document = Document::default();
    let line_id = line(&mut document, Point::new(0.0, 0.0), Point::new(10.0, 20.0));
    let report = document
        .set_constraints(
            (1..=256)
                .map(|id| SketchConstraint {
                    id,
                    relation: Relation::Horizontal { entity: line_id },
                })
                .collect(),
        )
        .unwrap();
    assert!(report.solved());
    assert_eq!(report.constraints, 256);
    assert_eq!(report.redundant_equations, 255);
    let before = document.to_json().unwrap();
    let error = document
        .set_constraints(
            (1..=257)
                .map(|id| SketchConstraint {
                    id,
                    relation: Relation::Horizontal { entity: line_id },
                })
                .collect(),
        )
        .unwrap_err();
    assert!(error.to_string().contains("256 constraints"), "{error}");
    assert_eq!(document.to_json().unwrap(), before);

    // 128 participating variables is the documented ceiling.
    let mut document = Document::default();
    let circles: Vec<u64> = (0..42)
        .map(|index| circle(&mut document, Point::new(index as f64 * 30.0, 0.0), 5.0))
        .collect();
    let constraints: Vec<_> = circles
        .iter()
        .enumerate()
        .map(|(index, &entity)| SketchConstraint {
            id: index as u64 + 1,
            relation: Relation::Radius { entity, value: 5.0 },
        })
        .collect();
    let report = document.set_constraints(constraints).unwrap();
    assert!(report.solved());
    assert_eq!(report.variables, 126);
    assert_eq!(report.constraints, 42);
    let extra = circle(&mut document, Point::new(999.0, 0.0), 5.0);
    let mut constraints = document.constraints.clone();
    constraints.push(SketchConstraint {
        id: 43,
        relation: Relation::Radius {
            entity: extra,
            value: 5.0,
        },
    });
    let before = document.to_json().unwrap();
    let error = document.set_constraints(constraints).unwrap_err();
    assert!(error.to_string().contains("128 participating"), "{error}");
    assert_eq!(document.to_json().unwrap(), before);
    assert_eq!(document.constraints.len(), 42);
}

#[test]
fn constrained_entities_are_protected_until_their_relations_are_cleared() {
    let mut document = Document::default();
    let first = line(&mut document, Point::new(0.0, 0.0), Point::new(10.0, 0.0));
    let second = line(&mut document, Point::new(0.0, 20.0), Point::new(10.0, 20.0));
    let round = circle(&mut document, Point::new(30.0, 5.0), 4.0);
    relate(
        &mut document,
        Relation::Coincident {
            a: end(first),
            b: start(second),
        },
    );
    relate(
        &mut document,
        Relation::Radius {
            entity: round,
            value: 6.0,
        },
    );
    assert!(document.entity_has_dependents(first));
    assert!(document.entity_has_dependents(round));
    assert!(!document.entity_has_dependents(u64::MAX - 1));
    let before = document.to_json().unwrap();
    // A single constrained ID blocks its own removal.
    let error = document.delete_entities(&[first]).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("referenced by sketch constraints"),
        "{error}"
    );
    assert_eq!(document.to_json().unwrap(), before);
    // A batch with one constrained ID removes nothing at all.
    assert!(document.delete_entities(&[round, first]).is_err());
    assert!(document.delete_entities(&[first, second]).is_err());
    assert_eq!(document.entities.len(), 3);
    assert_eq!(document.constraints.len(), 2);
    // Removing the relations reopens both deletions.
    document.set_constraints(Vec::new()).unwrap();
    assert_eq!(
        document.delete_entities(&[first, second, round]).unwrap(),
        3
    );
    assert!(document.entities.is_empty());
    assert!(document.constraints.is_empty());
    document.validate().unwrap();
}

#[test]
fn constraints_round_trip_through_project_files() {
    let mut document = Document::new("Constraints");
    let line_id = line(&mut document, Point::new(0.0, 0.0), Point::new(40.0, 12.0));
    let round = circle(&mut document, Point::new(70.0, 5.0), 6.0);
    relate(&mut document, Relation::Horizontal { entity: line_id });
    relate(
        &mut document,
        Relation::Radius {
            entity: round,
            value: 9.0,
        },
    );
    document.validate().unwrap();
    let json = document.to_json().unwrap();
    assert!(json.contains("\"constraints\""), "{json}");
    let loaded = Document::from_json(&json).unwrap();
    assert_eq!(loaded, document);
    assert_eq!(loaded.constraints.len(), 2);
    assert_eq!(loaded.constraints[0].relation.describe(), "Horizontal #2");
    loaded.validate().unwrap();

    // A v6 file without the constraints key loads empty and re-saves at v7.
    let mut value: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["format_version"], json!(PROJECT_FORMAT_VERSION));
    value["format_version"] = json!(6);
    value["document"]
        .as_object_mut()
        .unwrap()
        .remove("constraints");
    let legacy = Document::from_json(&value.to_string()).unwrap();
    assert!(legacy.constraints.is_empty());
    assert_eq!(legacy.entities, document.entities);
    assert_eq!(legacy.next_constraint_id(), 1);
    let resaved: Value = serde_json::from_str(&legacy.to_json().unwrap()).unwrap();
    assert_eq!(resaved["format_version"], json!(PROJECT_FORMAT_VERSION));
    assert_eq!(resaved["document"]["constraints"], json!([]));
}

#[test]
fn constraint_json_rejects_unknown_fields_and_relation_tags() {
    let constraint: SketchConstraint =
        serde_json::from_str(r#"{"id":1,"relation":{"type":"horizontal","entity":2}}"#).unwrap();
    assert_eq!(constraint.relation, Relation::Horizontal { entity: 2 });
    let constraint: SketchConstraint = serde_json::from_str(
        r#"{"id":4,"relation":{"type":"distance_x","a":{"entity":2,"handle":"start"},"b":{"entity":3,"handle":{"vertex":2}},"value":12.5}}"#,
    )
    .unwrap();
    assert_eq!(constraint.relation.value(), Some(12.5));
    for source in [
        r#"{"id":1,"relation":{"type":"horizontal","entity":2},"note":"x"}"#,
        r#"{"id":1,"relation":{"type":"spiral","entity":2}}"#,
        r#"{"id":1,"relation":{"type":"horizontal","entity":2,"axis":"x"}}"#,
        r#"{"id":1,"relation":{"type":"horizontal"}}"#,
        r#"{"id":1,"relation":{"type":"coincident","a":{"entity":2,"handle":"start"},"b":{"entity":3,"handle":"end"},"weight":1}}"#,
        r#"{"id":1,"relation":{"type":"coincident","a":{"entity":2,"handle":"middle"},"b":{"entity":3,"handle":"end"}}}"#,
        r#"{"relation":{"type":"horizontal","entity":2}}"#,
    ] {
        assert!(
            serde_json::from_str::<SketchConstraint>(source).is_err(),
            "{source}"
        );
    }
}

#[test]
fn unbounded_values_are_refused() {
    let mut document = Document::default();
    let line_id = line(&mut document, Point::new(0.0, 0.0), Point::new(10.0, 0.0));
    let round = circle(&mut document, Point::new(40.0, 0.0), 5.0);
    let geometry = document.entities.clone();
    for relation in [
        Relation::FixPoint {
            point: start(line_id),
            position: Point::new(2e9, 0.0),
        },
        Relation::FixPoint {
            point: start(line_id),
            position: Point::new(f64::NAN, 0.0),
        },
        Relation::FixPoint {
            point: start(line_id),
            position: Point::new(-1e9 - 1.0, 0.0),
        },
        Relation::DistanceX {
            a: start(line_id),
            b: end(line_id),
            value: f64::NAN,
        },
        Relation::DistanceX {
            a: start(line_id),
            b: end(line_id),
            value: -1e12,
        },
        Relation::DistanceY {
            a: start(line_id),
            b: end(line_id),
            value: f64::INFINITY,
        },
        Relation::Radius {
            entity: round,
            value: 1e10,
        },
    ] {
        let error = document
            .set_constraint(SketchConstraint { id: 1, relation })
            .unwrap_err();
        assert!(
            error.to_string().contains("finite and within +/-1e9"),
            "{error}"
        );
    }
    assert!(document.constraints.is_empty());
    assert_eq!(document.entities, geometry);

    // Non-finite geometry cannot join a constraint system at all.
    let mut document = Document::default();
    let broken = document
        .add_geometry(Geometry::Line {
            start: Point::new(f64::NAN, 0.0),
            end: Point::new(1.0, 0.0),
        })
        .unwrap();
    let error = document
        .set_constraint(SketchConstraint {
            id: 1,
            relation: Relation::Horizontal { entity: broken },
        })
        .unwrap_err();
    assert!(error.to_string().contains("finite"), "{error}");
    assert!(document.constraints.is_empty());
    assert!(document.to_json().is_err());
}
