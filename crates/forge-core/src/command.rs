//! Numeric drafting commands shared by the desktop and automation clients.
//! Commands execute as one transaction. A parse or geometry error leaves the
//! document (including its ID allocator) unchanged.
use crate::{
    Document, Geometry, Point, PolylineVertex,
    constraints::{PointHandle, PointRef, Relation, SketchConstraint},
    edit::Transform2d,
};
use std::{
    collections::{BTreeSet, VecDeque},
    io,
};

pub const HELP: &str = "LINE x1 y1 x2 y2 · RECTANGLE x1 y1 x2 y2 · CIRCLE cx cy radius\n\
ARC cx cy radius start-deg end-deg · POINT x y · PLINE x y x y … [CLOSE]\n\
TEXT x y height rotation text… · DIM H|V|A x1 y1 x2 y2 line-x line-y\n\
SELECT ALL|NONE|id… · MOVE dx dy · COPY dx dy · ROTATE deg [px py]\n\
SCALE factor [px py] · MIRROR x1 y1 x2 y2 [COPY] · OFFSET distance\n\
ARRAYRECT columns rows dx dy · ARRAYPOLAR count cx cy sweep [NOROTATE]\n\
TRIM target-id boundary-id pick-x pick-y · EXTEND target-id boundary-id pick-x pick-y\n\
ERASE · BLOCK name · EXPLODE · LAYER name · UNITS code · LENGTH · DIST x1 y1 x2 y2\n\
RELATION HORIZONTAL|VERTICAL id · COINCIDENT id handle id handle\n\
RELATION FIXPOINT id handle x y · DISTX|DISTY id handle id handle value\n\
RELATION RADIUS id value · EQUALRAD id id · REMOVE constraint-id · CLEAR\n\
CONSTRAINT REPORT|SOLVE · handles START END CENTER POSITION CORNERA\n\
CORNERB VERTEX n (case-insensitive); each relation solves as one transaction.\n\
Offsets: left for lines/open polylines, outward for closed shapes.\n\
Arrays include the originals in their counts. TRIM/EXTEND support LINE entities.";

#[derive(Clone, Debug)]
pub struct CommandResult {
    pub selection: Vec<u64>,
    pub changed: bool,
    pub message: String,
}

pub fn execute(
    document: &mut Document,
    selection: &[u64],
    input: &str,
) -> io::Result<CommandResult> {
    if input.contains(['\r', '\n', '\0']) {
        return Err(invalid(
            "A drafting command must be a single line without null characters",
        ));
    }
    document.validate()?;
    let mut args = Args(input.split_whitespace().collect());
    let name = args.take("command")?.to_ascii_uppercase();
    let mut candidate = document.clone();
    let mut selected: Vec<u64> = selection
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut changed = true;
    let mut message = format!("{name} complete");
    let mut geometry = None;
    match name.as_str() {
        "LINE" | "L" => {
            geometry = Some(Geometry::Line {
                start: args.point()?,
                end: args.point()?,
            })
        }
        "RECTANGLE" | "RECTANG" | "REC" => {
            geometry = Some(Geometry::Rectangle {
                a: args.point()?,
                b: args.point()?,
            })
        }
        "CIRCLE" | "C" => {
            geometry = Some(Geometry::Circle {
                center: args.point()?,
                radius: args.number()?,
            })
        }
        "ARC" => {
            geometry = Some(Geometry::Arc {
                center: args.point()?,
                radius: args.number()?,
                start_angle_deg: args.number()?,
                end_angle_deg: args.number()?,
            })
        }
        "POINT" => {
            geometry = Some(Geometry::Point {
                position: args.point()?,
            })
        }
        "PLINE" | "POLYLINE" => {
            let closed = args
                .0
                .back()
                .is_some_and(|word| word.eq_ignore_ascii_case("CLOSE"));
            if closed {
                args.0.pop_back();
            }
            if args.0.len() > 20_000 {
                return Err(invalid("Polyline command exceeds 10,000 vertices"));
            }
            let mut vertices = Vec::new();
            while !args.0.is_empty() {
                vertices.push(PolylineVertex::new(args.point()?));
            }
            geometry = Some(Geometry::Polyline { vertices, closed });
        }
        "TEXT" => {
            let position = args.point()?;
            let height = args.number()?;
            let rotation_deg = args.number()?;
            if args.0.is_empty() {
                return Err(invalid("TEXT requires its contents after the rotation"));
            }
            let text = args.remaining_name()?;
            geometry = Some(Geometry::Text {
                position,
                text,
                height,
                rotation_deg,
            });
        }
        "DIM" => {
            use crate::dimension::{
                Dimension, DimensionStyle, LinearDimension, LinearDimensionKind,
            };
            let kind = match args.take("H, V, or A")?.to_ascii_uppercase().as_str() {
                "H" => LinearDimensionKind::Horizontal,
                "V" => LinearDimensionKind::Vertical,
                "A" => LinearDimensionKind::Aligned,
                _ => {
                    return Err(invalid(
                        "Dimension type must be H (horizontal), V (vertical), or A (aligned)",
                    ));
                }
            };
            geometry = Some(Geometry::Dimension {
                dimension: Dimension::Linear(LinearDimension {
                    kind,
                    a: args.point()?,
                    b: args.point()?,
                    line_point: args.point()?,
                    style: DimensionStyle::default(),
                }),
            });
        }
        "SELECT" => {
            changed = false;
            if args.0.len() == 1 && args.0[0].eq_ignore_ascii_case("ALL") {
                selected = candidate
                    .entities
                    .iter()
                    .filter(|entity| {
                        candidate
                            .layer(entity.layer_id)
                            .is_some_and(|layer| layer.visible)
                    })
                    .map(|entity| entity.id)
                    .collect();
                args.0.clear();
            } else if args.0.len() == 1 && args.0[0].eq_ignore_ascii_case("NONE") {
                selected.clear();
                args.0.clear();
            } else {
                selected.clear();
                while !args.0.is_empty() {
                    selected.push(args.id()?);
                }
                selected.sort_unstable();
                selected.dedup();
                require_selection(&candidate, &selected)?;
            }
            message = format!("{} object(s) selected", selected.len());
        }
        "MOVE" | "COPY" | "ROTATE" | "SCALE" | "MIRROR" => {
            require_selection(&candidate, &selected)?;
            let transform = match name.as_str() {
                "MOVE" | "COPY" => {
                    let p = args.point()?;
                    Transform2d::translation(p.x, p.y)
                }
                "ROTATE" => {
                    let value = args.number()?;
                    Transform2d::rotation(value, args.optional_pivot()?)
                }
                "SCALE" => {
                    let value = args.number()?;
                    Transform2d::uniform_scale(value, args.optional_pivot()?)
                }
                _ => Transform2d::reflection(args.point()?, args.point()?)?,
            };
            let copy = name == "COPY" || (name == "MIRROR" && args.flag("COPY"));
            if copy {
                selected = candidate.duplicate_entities(&selected, transform)?;
            } else {
                candidate.transform_entities(&selected, transform)?;
            }
        }
        "OFFSET" => {
            require_selection(&candidate, &selected)?;
            selected = candidate.offset_entities(&selected, args.number()?)?;
        }
        "ARRAYRECT" => {
            require_selection(&candidate, &selected)?;
            let created = candidate.rectangular_array_entities(
                &selected,
                args.count()?,
                args.count()?,
                args.number()?,
                args.number()?,
            )?;
            if !created.is_empty() {
                selected = created;
            }
        }
        "ARRAYPOLAR" => {
            require_selection(&candidate, &selected)?;
            let count = args.count()?;
            let center = args.point()?;
            let sweep = args.number()?;
            let rotate = !args.flag("NOROTATE");
            let created =
                candidate.polar_array_entities(&selected, count, center, sweep, rotate)?;
            if !created.is_empty() {
                selected = created;
            }
        }
        "TRIM" | "EXTEND" => {
            let id = args.id()?;
            let boundary = args.id()?;
            let pick = args.point()?;
            if name == "TRIM" {
                candidate.trim_line_to_boundary(id, boundary, pick)?;
            } else {
                candidate.extend_line_to_boundary(id, boundary, pick)?;
            }
            selected = vec![id];
        }
        "ERASE" | "DELETE" => {
            require_selection(&candidate, &selected)?;
            candidate.delete_entities(&selected)?;
            selected.clear();
        }
        "RELATION" => {
            let kind = args.take("relation kind")?.to_ascii_uppercase();
            match kind.as_str() {
                "HORIZONTAL" => {
                    let relation = Relation::Horizontal { entity: args.id()? };
                    message = apply_relation(&mut candidate, relation)?;
                }
                "VERTICAL" => {
                    let relation = Relation::Vertical { entity: args.id()? };
                    message = apply_relation(&mut candidate, relation)?;
                }
                "COINCIDENT" => {
                    let a = args.point_ref()?;
                    let b = args.point_ref()?;
                    message = apply_relation(&mut candidate, Relation::Coincident { a, b })?;
                }
                "FIXPOINT" => {
                    let point = args.point_ref()?;
                    let position = args.point()?;
                    message =
                        apply_relation(&mut candidate, Relation::FixPoint { point, position })?;
                }
                "DISTX" => {
                    let a = args.point_ref()?;
                    let b = args.point_ref()?;
                    let value = args.number()?;
                    message = apply_relation(&mut candidate, Relation::DistanceX { a, b, value })?;
                }
                "DISTY" => {
                    let a = args.point_ref()?;
                    let b = args.point_ref()?;
                    let value = args.number()?;
                    message = apply_relation(&mut candidate, Relation::DistanceY { a, b, value })?;
                }
                "RADIUS" => {
                    let entity = args.id()?;
                    let value = args.number()?;
                    message = apply_relation(&mut candidate, Relation::Radius { entity, value })?;
                }
                "EQUALRAD" => {
                    let first = args.id()?;
                    let second = args.id()?;
                    message =
                        apply_relation(&mut candidate, Relation::EqualRadius { first, second })?;
                }
                "REMOVE" => {
                    let id = args.constraint_id()?;
                    let removed = candidate
                        .constraints
                        .iter()
                        .find(|constraint| constraint.id == id)
                        .map(|constraint| constraint.relation.describe())
                        .ok_or_else(|| invalid(format!("Constraint #{id} does not exist")))?;
                    let kept: Vec<_> = candidate
                        .constraints
                        .iter()
                        .filter(|constraint| constraint.id != id)
                        .cloned()
                        .collect();
                    let report = candidate.set_constraints(kept)?;
                    if !report.solved() {
                        return Err(invalid(format!(
                            "Removing constraint #{id} rejected: {}",
                            report.summary()
                        )));
                    }
                    message = format!("Removed relation #{id} {removed} · {}", report.summary());
                }
                "CLEAR" => {
                    let report = candidate.set_constraints(Vec::new())?;
                    if !report.solved() {
                        return Err(invalid(format!(
                            "Clearing constraints rejected: {}",
                            report.summary()
                        )));
                    }
                    message = format!("Constraints cleared · {}", report.summary());
                }
                _ => {
                    return Err(invalid(format!(
                        "Unknown relation {kind}; enter HELP for command syntax"
                    )));
                }
            }
        }
        "CONSTRAINT" => {
            let action = args.take("constraint action")?.to_ascii_uppercase();
            match action.as_str() {
                "REPORT" => {
                    changed = false;
                    message = candidate.constraint_report()?.summary();
                }
                "SOLVE" => {
                    message = candidate.solve_constraints()?.summary();
                }
                _ => {
                    return Err(invalid(format!(
                        "Unknown constraint action {action}; enter HELP for command syntax"
                    )));
                }
            }
        }
        "BLOCK" => {
            require_selection(&candidate, &selected)?;
            let block_name = args.remaining_name()?;
            selected = vec![candidate.define_block_from_selection(
                block_name,
                Point::default(),
                &selected,
            )?];
        }
        "EXPLODE" => {
            require_selection(&candidate, &selected)?;
            candidate.expanded_entities()?;
            let mut created = Vec::new();
            for id in selected {
                created.extend(candidate.explode_insert(id)?);
            }
            selected = created;
        }
        "LAYER" => {
            let layer_name = args.remaining_name()?;
            let layer = candidate
                .layers
                .iter()
                .find(|layer| layer.name.to_lowercase() == layer_name.to_lowercase())
                .map(|layer| layer.id);
            candidate.active_layer_id = if let Some(layer) = layer {
                layer
            } else {
                candidate.add_layer(layer_name, [215, 222, 234])?
            };
        }
        "LENGTH" => {
            require_selection(&candidate, &selected)?;
            let expanded = candidate.expanded_entities()?;
            let length: f64 = expanded
                .iter()
                .filter(|entity| selected.contains(&entity.source_id))
                .map(|entity| entity.geometry.length())
                .sum();
            if !length.is_finite() {
                return Err(invalid("Measured length exceeds numeric range"));
            }
            message = format!("Selected curve length: {length:.6} model units");
            changed = false;
        }
        "UNITS" => {
            candidate.units = args
                .take("INSUNITS code (0..24)")?
                .parse()
                .map_err(|_| invalid("Units must be an integer code from 0 to 24"))?;
            message = format!(
                "Unit code {} set; coordinates retain their numeric values",
                candidate.units
            );
        }
        "DIST" => {
            let length = args.point()?.distance(args.point()?);
            if !length.is_finite() {
                return Err(invalid("Measured distance exceeds numeric range"));
            }
            message = format!("Distance: {length:.6} model units");
            changed = false;
        }
        "HELP" | "?" => {
            message = HELP.into();
            changed = false;
        }
        _ => {
            return Err(invalid(format!(
                "Unknown command {name}; enter HELP to see drafting commands"
            )));
        }
    }
    args.finish()?;
    if let Some(geometry) = geometry {
        validate_authored_geometry(&geometry)?;
        selected = vec![candidate.add_geometry(geometry)?];
    }
    candidate.validate()?;
    // Commands that create a one-item array or select the current layer are no-ops.
    changed &= candidate != *document;
    if changed {
        *document = candidate;
    }
    Ok(CommandResult {
        selection: selected,
        changed,
        message,
    })
}

/// Authoring commands reject empty curves and geometry that cannot be measured
/// or bounded in f64. Imported legacy entities retain their existing rules.
fn validate_authored_geometry(geometry: &Geometry) -> io::Result<()> {
    geometry.validate()?;
    match geometry {
        Geometry::Line { start, end } if start == end => {
            return Err(invalid("LINE endpoints must be distinct"));
        }
        Geometry::Rectangle { a, b } if a.x == b.x || a.y == b.y => {
            return Err(invalid("RECTANGLE requires nonzero width and height"));
        }
        Geometry::Arc {
            start_angle_deg,
            end_angle_deg,
            ..
        } if (end_angle_deg - start_angle_deg).rem_euclid(360.0) == 0.0 => {
            return Err(invalid(
                "ARC requires a nonzero sweep; use CIRCLE for a complete circle",
            ));
        }
        Geometry::Polyline { vertices, closed } => {
            if *closed && vertices.len() < 3 {
                return Err(invalid(
                    "A closed polyline requires at least three vertices",
                ));
            }
            if vertices
                .windows(2)
                .any(|pair| pair[0].point == pair[1].point)
                || (*closed
                    && vertices.first().map(|v| v.point) == vertices.last().map(|v| v.point))
            {
                return Err(invalid(
                    "Polyline segments require distinct adjacent vertices; CLOSE supplies the final edge",
                ));
            }
        }
        Geometry::Text { text, height, .. }
            if !(height * 0.6 * text.chars().count() as f64).is_finite() =>
        {
            return Err(invalid("Text width exceeds numeric range"));
        }
        _ => {}
    }
    if !geometry.length().is_finite()
        || geometry
            .bounds()
            .is_none_or(|bounds| !bounds.min.is_finite() || !bounds.max.is_finite())
    {
        return Err(invalid("Authored geometry exceeds numeric range"));
    }
    Ok(())
}

/// Sketches are changed on a cloned candidate: a rejected or conflicting
/// relation leaves the real document and its constraint set untouched.
fn apply_relation(candidate: &mut Document, relation: Relation) -> io::Result<String> {
    let id = candidate.next_constraint_id();
    let described = relation.describe();
    let report = candidate.set_constraint(SketchConstraint { id, relation })?;
    if !report.solved() {
        return Err(invalid(format!("Relation rejected: {}", report.summary())));
    }
    Ok(format!("Relation #{id} {described} · {}", report.summary()))
}

fn require_selection(document: &Document, ids: &[u64]) -> io::Result<()> {
    if ids.is_empty() {
        return Err(invalid("Select objects first (SELECT ALL or SELECT id…)"));
    }
    for id in ids {
        if !document.entities.iter().any(|entity| entity.id == *id) {
            return Err(invalid(format!("Entity {id} does not exist")));
        }
    }
    Ok(())
}

struct Args<'a>(VecDeque<&'a str>);
impl<'a> Args<'a> {
    fn take(&mut self, label: &str) -> io::Result<&'a str> {
        self.0
            .pop_front()
            .ok_or_else(|| invalid(format!("Missing {label}; enter HELP for command syntax")))
    }
    fn number(&mut self) -> io::Result<f64> {
        let raw = self.take("numeric coordinate or value")?;
        let value = raw
            .parse::<f64>()
            .map_err(|_| invalid(format!("Invalid number: {raw}")))?;
        if value.is_finite() {
            Ok(value)
        } else {
            Err(invalid("Numbers must be finite"))
        }
    }
    fn point(&mut self) -> io::Result<Point> {
        Ok(Point::new(self.number()?, self.number()?))
    }
    fn id(&mut self) -> io::Result<u64> {
        self.take("entity ID")?
            .parse()
            .map_err(|_| invalid("Entity IDs must be positive integers"))
    }
    fn constraint_id(&mut self) -> io::Result<u64> {
        self.take("constraint ID")?
            .parse()
            .map_err(|_| invalid("Constraint IDs must be positive integers"))
    }
    fn handle(&mut self) -> io::Result<PointHandle> {
        let raw = self.take("point handle")?;
        match raw.to_ascii_uppercase().as_str() {
            "START" => Ok(PointHandle::Start),
            "END" => Ok(PointHandle::End),
            "CENTER" => Ok(PointHandle::Center),
            "POSITION" => Ok(PointHandle::Position),
            "CORNERA" => Ok(PointHandle::CornerA),
            "CORNERB" => Ok(PointHandle::CornerB),
            "VERTEX" => {
                let index = self
                    .take("vertex index")?
                    .parse::<usize>()
                    .map_err(|_| invalid("VERTEX requires a nonnegative integer index"))?;
                Ok(PointHandle::Vertex(index))
            }
            _ => Err(invalid(format!(
                "Point handle must be START, END, CENTER, POSITION, CORNERA, CORNERB or VERTEX n; got {raw}"
            ))),
        }
    }
    fn point_ref(&mut self) -> io::Result<PointRef> {
        Ok(PointRef {
            entity: self.id()?,
            handle: self.handle()?,
        })
    }
    fn count(&mut self) -> io::Result<usize> {
        self.take("item count")?
            .parse()
            .map_err(|_| invalid("Counts must be positive integers"))
    }
    fn optional_pivot(&mut self) -> io::Result<Point> {
        if self.0.is_empty() {
            Ok(Point::default())
        } else {
            self.point()
        }
    }
    fn flag(&mut self, value: &str) -> bool {
        if self
            .0
            .front()
            .is_some_and(|word| word.eq_ignore_ascii_case(value))
        {
            self.0.pop_front();
            true
        } else {
            false
        }
    }
    fn remaining_name(&mut self) -> io::Result<String> {
        if self.0.is_empty() {
            return Err(invalid("A name is required"));
        }
        let name = self.0.iter().copied().collect::<Vec<_>>().join(" ");
        self.0.clear();
        Ok(name)
    }
    fn finish(&self) -> io::Result<()> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(invalid(format!("Unexpected argument: {}", self.0[0])))
        }
    }
}
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_metadata_round_trips_without_rescaling_geometry() {
        let mut doc = Document::default();
        execute(&mut doc, &[], "LINE 0 0 100 20").unwrap();
        let geometry = doc.entities[0].geometry.clone();
        execute(&mut doc, &[], "UNITS 4").unwrap();
        assert_eq!(doc.units, 4);
        assert_eq!(doc.entities[0].geometry, geometry);
        let dxf = crate::dxf::write(&doc).unwrap();
        assert_eq!(crate::dxf::parse(&dxf).unwrap().document.units, 4);
        let before = doc.clone();
        assert!(execute(&mut doc, &[], "UNITS 25").is_err());
        assert_eq!(doc, before);
        assert!(!execute(&mut doc, &[], "UNITS 4").unwrap().changed);
    }

    #[test]
    fn exhausted_ids_fail_creation_without_panicking_or_partial_changes() {
        let mut doc = Document::default();
        let first = execute(&mut doc, &[], "LINE 0 0 3 4").unwrap().selection[0];
        let second = execute(&mut doc, &[], "LINE 5 0 8 4").unwrap().selection[0];
        let ids = [first, second];
        // One ID remains. COPY and BLOCK both require at least two IDs.
        doc.next_id = u64::MAX - 1;
        let before = doc.clone();
        for command in ["COPY 1 2", "MIRROR 0 0 0 1 COPY", "BLOCK Detail"] {
            assert!(execute(&mut doc, &ids, command).is_err(), "{command}");
            assert_eq!(doc, before, "{command}");
        }
        // A single allocation is still valid at the final usable ID.
        let result = execute(&mut doc, &[], "POINT 0 0").unwrap();
        assert_eq!(result.selection, [u64::MAX - 1]);
        let exhausted = doc.clone();
        for command in [
            "LINE 0 0 1 1",
            "POINT 1 1",
            "LAYER New layer",
            "COPY 1 2",
            "OFFSET 2",
        ] {
            assert!(execute(&mut doc, &ids, command).is_err(), "{command}");
            assert_eq!(doc, exhausted, "{command}");
        }
    }

    #[test]
    fn block_explosion_preflights_all_fresh_leaf_ids() {
        let mut doc = Document::default();
        let first = execute(&mut doc, &[], "LINE 0 0 3 4").unwrap().selection[0];
        let second = execute(&mut doc, &[], "LINE 5 0 8 4").unwrap().selection[0];
        let block = execute(&mut doc, &[first, second], "BLOCK Pair")
            .unwrap()
            .selection;
        doc.next_id = u64::MAX - 1;
        let before = doc.clone();
        assert!(execute(&mut doc, &block, "EXPLODE").is_err());
        assert_eq!(doc, before);
    }

    #[test]
    fn exhausted_allocator_allows_reading_editing_and_true_noops() {
        let mut doc = Document::default();
        let ids = execute(&mut doc, &[], "LINE 0 0 3 4").unwrap().selection;
        doc.next_id = u64::MAX;
        let before = doc.clone();
        for command in [
            "SELECT ALL",
            "LENGTH",
            "DIST 0 0 3 4",
            "HELP",
            "MOVE 0 0",
            "LAYER 0",
            "ARRAYRECT 1 1 0 0",
            "ARRAYPOLAR 1 0 0 360",
        ] {
            let result = execute(&mut doc, &ids, command).unwrap();
            assert!(!result.changed, "{command}");
            assert_eq!(result.selection, ids, "{command}");
            assert_eq!(doc, before, "{command}");
        }
        assert!(execute(&mut doc, &ids, "MOVE 1 2").unwrap().changed);
        assert!(execute(&mut doc, &ids, "ERASE").unwrap().changed);
        assert!(doc.entities.is_empty());
    }

    #[test]
    fn malformed_multiline_degenerate_and_overflowed_commands_are_atomic() {
        let mut doc = Document::default();
        let before = doc.clone();
        for command in [
            "",
            "   ",
            "POINT 0 0\n",
            "TEXT 0 0 1 0 first\rsecond",
            "LAYER null\0name",
            "LINE 0 0 0 0",
            "RECTANGLE 0 0 0 3",
            "ARC 0 0 1 0 360",
            "PLINE 0 0 0 0",
            "PLINE 0 0 1 1 CLOSE",
            "PLINE 0 0 2 0 0 0 CLOSE",
            "LINE -1e308 0 1e308 0",
            "CIRCLE 1e308 0 1e308",
            "ARC 0 0 1 -1e308 1e308",
            "TEXT 0 0 1e308 0 huge text",
        ] {
            assert!(execute(&mut doc, &[], command).is_err(), "{command:?}");
            assert_eq!(doc, before, "{command:?}");
        }
    }

    #[test]
    fn large_polyline_parses_all_vertices_in_order_with_a_bounded_limit() {
        let mut command = String::from("PLINE");
        for index in 0..10_000 {
            command.push_str(&format!(" {index} {}", index % 2));
        }
        let mut doc = Document::default();
        execute(&mut doc, &[], &command).unwrap();
        match &doc.entities[0].geometry {
            Geometry::Polyline { vertices, closed } => {
                assert_eq!(vertices.len(), 10_000);
                assert_eq!(vertices[9_999].point, Point::new(9_999.0, 1.0));
                assert!(!closed);
            }
            _ => panic!("expected authored polyline"),
        }
        let before = doc.clone();
        command.push_str(" 10000 0");
        assert!(execute(&mut doc, &[], &command).is_err());
        assert_eq!(doc, before);
    }

    #[test]
    fn selecting_an_existing_layer_uses_the_document_case_rules() {
        let mut doc = Document::default();
        execute(&mut doc, &[], "LAYER Éléments").unwrap();
        let before = doc.clone();
        assert!(!execute(&mut doc, &[], "LAYER ÉLÉMENTS").unwrap().changed);
        assert_eq!(doc, before);
    }

    #[test]
    fn exact_authoring_selection_and_offsets_form_a_reopenable_drawing() {
        let mut doc = Document::default();
        execute(&mut doc, &[], "LAYER Walls").unwrap();
        let result = execute(&mut doc, &[], "RECTANGLE 0 0 120 80").unwrap();
        let result = execute(&mut doc, &result.selection, "OFFSET -5").unwrap();
        execute(&mut doc, &result.selection, "ARRAYRECT 3 2 150 100").unwrap();
        assert_eq!(doc.entities.len(), 7);
        assert!(
            doc.entities
                .iter()
                .all(|entity| entity.layer_id == doc.active_layer_id)
        );
        assert_eq!(Document::from_json(&doc.to_json().unwrap()).unwrap(), doc);
        assert!(crate::dxf::write(&doc).unwrap().contains("LWPOLYLINE"));
    }
    #[test]
    fn errors_preserve_document_and_ids_even_after_partial_operation() {
        let mut doc = Document::default();
        let ids = execute(&mut doc, &[], "CIRCLE 5 6 10").unwrap().selection;
        let before = doc.clone();
        for input in [
            "MOVE 1 2 typo",
            "CIRCLE NaN 0 1",
            "OFFSET -20",
            "ARRAYRECT 999999 999999 2 3",
            "MIRROR 0 0 0 0",
            "LINE 0 0 1",
            "SELECT 9999",
            "BOGUS",
            "TEXT 0 0 1 0",
        ] {
            assert!(execute(&mut doc, &ids, input).is_err(), "{input}");
            assert_eq!(doc, before, "{input}");
        }
    }
    #[test]
    fn selection_measurements_and_noops_do_not_dirty_document() {
        let mut doc = Document::default();
        let ids = execute(&mut doc, &[], "LINE 0 0 3 4").unwrap().selection;
        let before = doc.clone();
        for input in [
            "SELECT ALL",
            "SELECT NONE",
            "DIST 0 0 3 4",
            "LENGTH",
            "HELP",
            "ARRAYRECT 1 1 0 0",
        ] {
            assert!(!execute(&mut doc, &ids, input).unwrap().changed, "{input}");
            assert_eq!(doc, before);
        }
        assert!(
            execute(&mut doc, &ids, "LENGTH")
                .unwrap()
                .message
                .contains("5.000000")
        );
    }
    #[test]
    fn dimensions_blocks_mirror_and_explode_keep_semantics() {
        let mut doc = Document::default();
        let ids = execute(&mut doc, &[], "DIM A 0 0 3 4 7 7")
            .unwrap()
            .selection;
        let ids = execute(&mut doc, &ids, "BLOCK Detail A").unwrap().selection;
        assert_eq!(doc.blocks.len(), 1);
        assert!(execute(&mut doc, &ids, "MIRROR 0 0 1 0").is_err());
        let ids = execute(&mut doc, &ids, "EXPLODE").unwrap().selection;
        execute(&mut doc, &ids, "MIRROR 0 0 1 0 COPY").unwrap();
        assert_eq!(doc.entities.len(), 2);
        assert!(
            doc.entities
                .iter()
                .all(|entity| matches!(entity.geometry, Geometry::Dimension { .. }))
        );
    }

    fn near(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} != {b}");
    }
    fn author(document: &mut Document, command: &str) -> u64 {
        execute(document, &[], command).unwrap().selection[0]
    }
    fn line_of(document: &Document, id: u64) -> (Point, Point) {
        match &document
            .entities
            .iter()
            .find(|entity| entity.id == id)
            .unwrap()
            .geometry
        {
            Geometry::Line { start, end } => (*start, *end),
            other => panic!("entity #{id} is not a line: {other:?}"),
        }
    }
    fn circle_of(document: &Document, id: u64) -> (Point, f64) {
        match &document
            .entities
            .iter()
            .find(|entity| entity.id == id)
            .unwrap()
            .geometry
        {
            Geometry::Circle { center, radius } => (*center, *radius),
            other => panic!("entity #{id} is not a circle: {other:?}"),
        }
    }
    fn constraint_ids(document: &Document) -> Vec<u64> {
        document
            .constraints
            .iter()
            .map(|constraint| constraint.id)
            .collect()
    }

    #[test]
    fn help_lists_the_new_constraint_command_syntax() {
        let mut doc = Document::default();
        let result = execute(&mut doc, &[], "HELP").unwrap();
        assert!(!result.changed);
        assert_eq!(result.message, HELP);
        for syntax in [
            "RELATION HORIZONTAL|VERTICAL id",
            "COINCIDENT id handle id handle",
            "RELATION FIXPOINT id handle x y",
            "DISTX|DISTY id handle id handle value",
            "RELATION RADIUS id value",
            "EQUALRAD id id",
            "REMOVE constraint-id · CLEAR",
            "CONSTRAINT REPORT|SOLVE",
            "VERTEX n",
        ] {
            assert!(HELP.contains(syntax), "{syntax}");
        }
        // The desktop help window and the CLI print these lines verbatim.
        for line in HELP.lines() {
            assert!(line.chars().count() <= 100, "{line}");
        }
    }

    #[test]
    fn relation_horizontal_solves_a_sloped_line_and_persists() {
        let mut doc = Document::default();
        let line = author(&mut doc, "LINE 0 0 100 20");
        let before = doc.to_json().unwrap();
        // Lowercase spellings, kinds and handles are all accepted.
        let result = execute(&mut doc, &[line], &format!("relation horizontal {line}")).unwrap();
        assert!(result.changed);
        assert_eq!(result.selection, [line]);
        assert!(result.message.starts_with("Relation #1 Horizontal #2 · "));
        assert!(result.message.contains(
            "Satisfied: 1 constraints, 3 remaining degrees of freedom, maximum error 0.000000; conflicting IDs []"
        ));
        let (start, end) = line_of(&doc, line);
        near(start.x, 0.0);
        near(end.x, 100.0);
        near(start.y, 10.0);
        near(end.y, 10.0);
        // Minimum displacement shares the correction between both handles.
        assert_eq!(doc.constraints[0].id, 1);
        assert_eq!(
            doc.constraints[0].relation,
            Relation::Horizontal { entity: line }
        );
        let loaded = Document::from_json(&doc.to_json().unwrap()).unwrap();
        assert_eq!(loaded, doc);
        assert_eq!(loaded.constraints.len(), 1);
        assert_eq!(line_of(&loaded, line), (start, end));
        loaded.validate().unwrap();
        assert_ne!(loaded.to_json().unwrap(), before);
        // Re-solving sharpens the shared coordinate to its exact midpoint.
        assert!(execute(&mut doc, &[], "CONSTRAINT SOLVE").unwrap().changed);
        let (start, end) = line_of(&doc, line);
        assert_eq!(start.y, 10.0);
        assert_eq!(end.y, 10.0);
        // A converged sketch re-solves as a verified no-op.
        let solved = execute(&mut doc, &[], "CONSTRAINT SOLVE").unwrap();
        assert!(!solved.changed);
        assert!(solved.message.contains("Satisfied: 1 constraints"));
    }

    #[test]
    fn constraint_report_and_solve_describe_free_and_joined_geometry() {
        let mut doc = Document::default();
        let selected = [2u64];
        let report = execute(&mut doc, &selected, "CONSTRAINT REPORT").unwrap();
        assert!(!report.changed);
        assert_eq!(report.selection, selected);
        assert_eq!(
            report.message,
            "Satisfied: 0 constraints, 0 remaining degrees of freedom, maximum error 0.000000; conflicting IDs []"
        );
        let first = author(&mut doc, "LINE 0 0 10 0");
        let second = author(&mut doc, "LINE 10 5 20 5");
        // Unconstrained geometry is not counted as a participating variable.
        assert_eq!(
            execute(&mut doc, &[], "CONSTRAINT REPORT").unwrap().message,
            "Satisfied: 0 constraints, 0 remaining degrees of freedom, maximum error 0.000000; conflicting IDs []"
        );
        let joined = execute(
            &mut doc,
            &[],
            &format!("RELATION COINCIDENT {first} eNd {second} START"),
        )
        .unwrap();
        assert!(joined.changed);
        assert!(
            joined
                .message
                .starts_with("Relation #1 Coincident #2.end→#3.start · ")
        );
        let (_, first_end) = line_of(&doc, first);
        let (second_start, _) = line_of(&doc, second);
        near(first_end.x, 10.0);
        near(first_end.y, 2.5);
        near(second_start.x, 10.0);
        near(second_start.y, 2.5);
        let report = execute(&mut doc, &[], "CONSTRAINT REPORT").unwrap();
        assert_eq!(
            report.message,
            "Satisfied: 1 constraints, 6 remaining degrees of freedom, maximum error 0.000000; conflicting IDs []"
        );
        // One more pass lands both handles on the exact join point.
        assert!(execute(&mut doc, &[], "CONSTRAINT SOLVE").unwrap().changed);
        let (_, first_end) = line_of(&doc, first);
        let (second_start, _) = line_of(&doc, second);
        assert_eq!(first_end.y, 2.5);
        assert_eq!(second_start.y, 2.5);
        assert_eq!(first_end, second_start);
        let solved = execute(&mut doc, &[], "CONSTRAINT SOLVE").unwrap();
        assert!(!solved.changed);
        assert_eq!(solved.message, report.message);
        let (start, end) = line_of(&doc, first);
        near(start.y, 0.0);
        near(end.y, 2.5);
        doc.validate().unwrap();
    }

    #[test]
    fn relation_radius_and_equal_radius_commands_solve_circles() {
        let mut doc = Document::default();
        let circle = author(&mut doc, "CIRCLE 5 6 10");
        let result = execute(&mut doc, &[], &format!("RELATION RADIUS {circle} 25")).unwrap();
        assert!(result.message.starts_with("Relation #1 Radius 25 #2 · "));
        let (center, radius) = circle_of(&doc, circle);
        near(radius, 25.0);
        assert_eq!(center, Point::new(5.0, 6.0));

        let mut doc = Document::default();
        let first = author(&mut doc, "CIRCLE 0 0 5");
        let second = author(&mut doc, "CIRCLE 20 0 10");
        let result = execute(
            &mut doc,
            &[],
            &format!("RELATION EQUALRAD {first} {second}"),
        )
        .unwrap();
        assert!(
            result
                .message
                .starts_with("Relation #1 EqualRadius #2=#3 · ")
        );
        assert!(result.message.contains("5 remaining degrees of freedom"));
        let (first_center, first_radius) = circle_of(&doc, first);
        let (second_center, second_radius) = circle_of(&doc, second);
        near(first_radius, 7.5);
        near(second_radius, 7.5);
        assert_eq!(first_center, Point::new(0.0, 0.0));
        assert_eq!(second_center, Point::new(20.0, 0.0));
        // A converged equality re-solves as a verified no-op.
        assert!(!execute(&mut doc, &[], "CONSTRAINT SOLVE").unwrap().changed);
        // Two radii for one circle contradict each other, so the second is refused.
        execute(&mut doc, &[], &format!("RELATION RADIUS {first} 7.5")).unwrap();
        let before = doc.to_json().unwrap();
        let error = execute(&mut doc, &[], &format!("RELATION RADIUS {first} 30")).unwrap_err();
        assert!(error.to_string().contains("Conflict:"), "{error}");
        // The conflicting set includes the equality tying the two circles together.
        assert!(
            error.to_string().contains("conflicting IDs [1, 2, 3]"),
            "{error}"
        );
        assert_eq!(doc.to_json().unwrap(), before);
        assert_eq!(constraint_ids(&doc), [1, 2]);
        // Clearing the contradictions reopens the sketch.
        assert!(execute(&mut doc, &[], "RELATION CLEAR").unwrap().changed);
        assert!(doc.constraints.is_empty());
    }

    #[test]
    fn relation_fixpoint_pins_a_handle_with_vertex_indexes() {
        let mut doc = Document::default();
        let line = author(&mut doc, "LINE 0 0 10 20");
        let result = execute(
            &mut doc,
            &[],
            &format!("RELATION FIXPOINT {line} END 30 40"),
        )
        .unwrap();
        assert!(
            result
                .message
                .starts_with("Relation #1 FixPoint #2.end@(30,40) · ")
        );
        let (start, end) = line_of(&doc, line);
        assert_eq!(start, Point::new(0.0, 0.0));
        assert_eq!(end, Point::new(30.0, 40.0));

        let mut doc = Document::default();
        let pline = author(&mut doc, "PLINE 0 0 10 0 10 10");
        execute(
            &mut doc,
            &[],
            &format!("RELATION FIXPOINT {pline} VERTEX 2 5 5"),
        )
        .unwrap();
        match &doc.entities[0].geometry {
            Geometry::Polyline { vertices, .. } => {
                assert_eq!(vertices[0].point, Point::new(0.0, 0.0));
                assert_eq!(vertices[1].point, Point::new(10.0, 0.0));
                assert_eq!(vertices[2].point, Point::new(5.0, 5.0));
            }
            other => panic!("expected a polyline: {other:?}"),
        }
        let before = doc.to_json().unwrap();
        assert!(
            execute(
                &mut doc,
                &[],
                &format!("RELATION FIXPOINT {pline} VERTEX 9 0 0")
            )
            .is_err()
        );
        assert!(
            execute(
                &mut doc,
                &[],
                &format!("RELATION FIXPOINT {pline} START 0 0")
            )
            .is_err()
        );
        assert_eq!(doc.to_json().unwrap(), before);
    }

    #[test]
    fn relation_distance_commands_solve_numeric_cases() {
        let mut doc = Document::default();
        let first = author(&mut doc, "LINE 0 0 10 0");
        let second = author(&mut doc, "LINE 20 0 30 0");
        let result = execute(
            &mut doc,
            &[],
            &format!("RELATION DISTX {first} start {second} START 50"),
        )
        .unwrap();
        assert!(
            result
                .message
                .starts_with("Relation #1 DistanceX 50 #2.start→#3.start · ")
        );
        let (first_start, _) = line_of(&doc, first);
        let (second_start, _) = line_of(&doc, second);
        near(first_start.x, -15.0);
        near(second_start.x, 35.0);
        near(second_start.x - first_start.x, 50.0);

        let mut doc = Document::default();
        let first = author(&mut doc, "LINE 0 0 10 0");
        let second = author(&mut doc, "LINE 20 10 30 10");
        execute(
            &mut doc,
            &[],
            &format!("RELATION DISTY {first} START {second} START 30"),
        )
        .unwrap();
        let (first_start, _) = line_of(&doc, first);
        let (second_start, _) = line_of(&doc, second);
        near(first_start.y, -10.0);
        near(second_start.y, 20.0);
        near(second_start.y - first_start.y, 30.0);

        let mut doc = Document::default();
        let rect = author(&mut doc, "RECTANGLE 0 0 40 20");
        execute(
            &mut doc,
            &[],
            &format!("RELATION FIXPOINT {rect} CORNERA 0 0"),
        )
        .unwrap();
        let result = execute(
            &mut doc,
            &[],
            &format!("RELATION DISTX {rect} cornera {rect} CORNERB 40"),
        )
        .unwrap();
        assert!(
            result
                .message
                .starts_with("Relation #2 DistanceX 40 #2.cornera→#2.cornerb · ")
        );
        match &doc.entities[0].geometry {
            Geometry::Rectangle { a, b } => {
                assert_eq!(*a, Point::new(0.0, 0.0));
                assert_eq!(*b, Point::new(40.0, 20.0));
            }
            other => panic!("expected a rectangle: {other:?}"),
        }
    }

    #[test]
    fn relation_remove_and_clear_manage_the_constraint_set() {
        let mut doc = Document::default();
        let first = author(&mut doc, "LINE 0 0 10 0");
        let second = author(&mut doc, "LINE 20 0 30 0");
        let before = doc.to_json().unwrap();
        for input in [
            "RELATION REMOVE 1",
            "RELATION REMOVE 9",
            "RELATION REMOVE zero",
        ] {
            let error = execute(&mut doc, &[], input).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "{input}");
            assert_eq!(doc.to_json().unwrap(), before, "{input}");
        }
        execute(&mut doc, &[], &format!("RELATION HORIZONTAL {first}")).unwrap();
        execute(
            &mut doc,
            &[],
            &format!("RELATION DISTX {first} START {second} START 50"),
        )
        .unwrap();
        assert_eq!(constraint_ids(&doc), [1, 2]);
        let removed = execute(&mut doc, &[], "RELATION REMOVE 1").unwrap();
        assert!(removed.changed);
        assert!(
            removed
                .message
                .starts_with("Removed relation #1 Horizontal #2 · ")
        );
        assert!(removed.selection.is_empty());
        assert_eq!(constraint_ids(&doc), [2]);
        doc.validate().unwrap();
        let (first_start, _) = line_of(&doc, first);
        let (second_start, _) = line_of(&doc, second);
        near(second_start.x - first_start.x, 50.0);
        let cleared = execute(&mut doc, &[], "RELATION CLEAR").unwrap();
        assert!(cleared.changed);
        assert!(cleared.message.starts_with("Constraints cleared · "));
        assert!(doc.constraints.is_empty());
        assert_eq!(doc.entities.len(), 2);
        assert!(!execute(&mut doc, &[], "RELATION CLEAR").unwrap().changed);
        assert!(doc.constraints.is_empty());
        // Removing from an empty set is still a clear error.
        assert!(execute(&mut doc, &[], "RELATION REMOVE 1").is_err());
        doc.validate().unwrap();
    }

    #[test]
    fn conflicting_relations_are_rejected_and_leave_identical_bytes() {
        let mut doc = Document::default();
        let first = author(&mut doc, "LINE 0 0 10 0");
        let second = author(&mut doc, "LINE 20 0 30 0");
        execute(
            &mut doc,
            &[],
            &format!("RELATION DISTX {first} START {second} START 50"),
        )
        .unwrap();
        let before = doc.to_json().unwrap();
        let error = execute(
            &mut doc,
            &[],
            &format!("RELATION DISTX {first} START {second} START 70"),
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(error.to_string().contains("Conflict:"), "{error}");
        assert!(
            error.to_string().contains("conflicting IDs [1, 2]"),
            "{error}"
        );
        assert_eq!(doc.to_json().unwrap(), before);
        assert_eq!(constraint_ids(&doc), [1]);
        // Releasing one side of the contradiction lets the new value commit.
        execute(&mut doc, &[], "RELATION REMOVE 1").unwrap();
        assert!(
            execute(
                &mut doc,
                &[],
                &format!("RELATION DISTX {first} START {second} START 70")
            )
            .unwrap()
            .changed
        );
        assert_eq!(doc.constraints[0].relation.value(), Some(70.0));
    }

    #[test]
    fn invalid_relation_input_leaves_document_and_constraints_unchanged() {
        let mut doc = Document::default();
        let line = author(&mut doc, "LINE 0 0 10 0");
        let before = doc.to_json().unwrap();
        for input in [
            "RELATION HORIZONTAL 999".to_owned(),
            format!("RELATION COINCIDENT {line} BOGUS {line} END"),
            format!("RELATION RADIUS {line}"),
            "RELATION 2".to_owned(),
            "CONSTRAINT BOGUS".to_owned(),
            "RELATION VERTICAL".to_owned(),
            "CONSTRAINT".to_owned(),
            format!("RELATION DISTX {line} START {line} END"),
            format!("RELATION EQUALRAD {line}"),
            format!("RELATION HORIZONTAL {line} EXTRA"),
            format!("RELATION COINCIDENT {line} START {line}"),
            format!("RELATION COINCIDENT {line} END {line} VERTEX"),
            "RELATION DISTX 1 NOPE 1 END 5".to_owned(),
        ] {
            let error = execute(&mut doc, &[], &input).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "{input}");
            assert!(!doc.constraints.iter().any(|c| c.id == 0), "{input}");
            assert_eq!(doc.to_json().unwrap(), before, "{input}");
            assert!(doc.constraints.is_empty(), "{input}");
            assert_eq!(doc, Document::from_json(&before).unwrap(), "{input}");
        }
        assert!(
            execute(&mut doc, &[], "CONSTRAINT REPORT")
                .unwrap()
                .message
                .contains("0 constraints")
        );
    }

    #[test]
    fn constraint_ids_auto_increment_without_colliding_after_removal() {
        let mut doc = Document::default();
        let first = author(&mut doc, "LINE 0 0 10 0");
        let second = author(&mut doc, "LINE 20 0 30 0");
        let third = author(&mut doc, "LINE 0 9 10 9");
        // Constraint IDs are their own namespace, independent of drawing IDs.
        execute(&mut doc, &[], &format!("RELATION HORIZONTAL {first}")).unwrap();
        execute(&mut doc, &[], &format!("RELATION HORIZONTAL {second}")).unwrap();
        assert_eq!(constraint_ids(&doc), [1, 2]);
        assert_eq!(doc.next_constraint_id(), 3);
        execute(&mut doc, &[], "RELATION REMOVE 1").unwrap();
        assert_eq!(constraint_ids(&doc), [2]);
        // The allocator never rewinds over a live ID.
        assert_eq!(doc.next_constraint_id(), 3);
        let added = execute(&mut doc, &[], &format!("RELATION HORIZONTAL {third}")).unwrap();
        assert!(added.message.starts_with("Relation #3 Horizontal #4 · "));
        assert_eq!(constraint_ids(&doc), [2, 3]);
        // Freeing the highest ID reuses exactly that number, deterministically.
        execute(&mut doc, &[], "RELATION REMOVE 3").unwrap();
        assert_eq!(doc.next_constraint_id(), 3);
        execute(&mut doc, &[], &format!("RELATION HORIZONTAL {third}")).unwrap();
        assert_eq!(constraint_ids(&doc), [2, 3]);
        assert_eq!(
            doc.constraints[1].relation,
            Relation::Horizontal { entity: third }
        );
    }

    #[test]
    fn scripted_constraint_flow_reports_and_reopens_a_valid_drawing() {
        let mut doc = Document::new("Constraint script");
        execute(&mut doc, &[], "LAYER Sketch").unwrap();
        let first = author(&mut doc, "LINE 0 0 40 15");
        let second = author(&mut doc, "LINE 40 15 80 0");
        let circle = author(&mut doc, "CIRCLE 40 -10 5");
        let script = [
            format!("RELATION COINCIDENT {first} END {second} START"),
            format!("RELATION HORIZONTAL {first}"),
            format!("RELATION HORIZONTAL {second}"),
            format!("RELATION RADIUS {circle} 8"),
        ];
        let selection = [first, circle];
        for command in &script {
            let result = execute(&mut doc, &selection, command).unwrap();
            assert!(result.changed, "{command}");
            // Relation commands never touch the selection.
            assert_eq!(result.selection, selection, "{command}");
        }
        doc.validate().unwrap();
        let (first_start, first_end) = line_of(&doc, first);
        let (second_start, second_end) = line_of(&doc, second);
        near(first_start.y, 7.5);
        near(first_end.y, 7.5);
        near(second_start.y, 7.5);
        near(second_end.y, 7.5);
        near(first_end.x, 40.0);
        near(second_start.x, 40.0);
        near(second_end.x, 80.0);
        near(circle_of(&doc, circle).1, 8.0);
        let report = execute(&mut doc, &[], "CONSTRAINT REPORT").unwrap();
        assert!(
            report
                .message
                .starts_with("Satisfied: 4 constraints, 6 remaining degrees of freedom"),
            "{}",
            report.message
        );
        let solved = execute(&mut doc, &[], "CONSTRAINT SOLVE").unwrap();
        // One refinement pass sharpens the shared coordinates…
        assert!(solved.changed);
        near(first_end.x, 40.0);
        near(second_start.x, 40.0);
        near(first_start.y, 7.5);
        near(first_end.y, 7.5);
        near(second_start.y, 7.5);
        near(second_end.y, 7.5);
        near(circle_of(&doc, circle).1, 8.0);
        // …leaving the converged sketch a verified no-op.
        let again = execute(&mut doc, &[], "CONSTRAINT SOLVE").unwrap();
        assert!(!again.changed);
        assert_eq!(again.message, solved.message);
        let (first_start, first_end) = line_of(&doc, first);
        let (second_start, second_end) = line_of(&doc, second);
        near(first_start.y, 7.5);
        near(second_end.y, 7.5);
        let reopened = Document::from_json(&doc.to_json().unwrap()).unwrap();
        assert_eq!(reopened.constraints.len(), 4);
        reopened.validate().unwrap();
        assert_eq!(line_of(&reopened, first), (first_start, first_end));
        assert_eq!(line_of(&reopened, second), (second_start, second_end));
        assert_eq!(circle_of(&reopened, circle), circle_of(&doc, circle));
        // The whole script replays deterministically on a fresh document.
        let mut replay = vec![
            "LAYER Sketch".to_owned(),
            "LINE 0 0 40 15".to_owned(),
            "LINE 40 15 80 0".to_owned(),
            "CIRCLE 40 -10 5".to_owned(),
        ];
        replay.extend(script.iter().cloned());
        let mut replayed = Document::new("Constraint script");
        let mut authored = Vec::new();
        for command in &replay {
            let result = execute(&mut replayed, &[], command).unwrap();
            if !result.selection.is_empty() {
                authored.push(result.selection[0]);
            }
        }
        assert_eq!(authored, [first, second, circle]);
        execute(&mut replayed, &[], "CONSTRAINT SOLVE").unwrap();
        replayed.validate().unwrap();
        assert_eq!(replayed.to_json().unwrap(), doc.to_json().unwrap());
    }
}
