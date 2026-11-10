//! Numeric drafting commands shared by the desktop and automation clients.
//! Commands execute as one transaction. A parse or geometry error leaves the
//! document (including its ID allocator) unchanged.
use crate::{Document, Geometry, Point, PolylineVertex, edit::Transform2d};
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
                require_id_capacity(&candidate, selected.len())?;
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
            candidate
                .entities
                .retain(|entity| !selected.contains(&entity.id));
            selected.clear();
        }
        "BLOCK" => {
            require_selection(&candidate, &selected)?;
            let block_name = args.remaining_name()?;
            require_id_capacity(
                &candidate,
                selected
                    .len()
                    .checked_add(1)
                    .ok_or_else(|| invalid("Block entity count overflow"))?,
            )?;
            selected = vec![candidate.define_block_from_selection(
                block_name,
                Point::default(),
                &selected,
            )?];
        }
        "EXPLODE" => {
            require_selection(&candidate, &selected)?;
            let expanded = candidate.expanded_entities()?;
            let copies = expanded
                .iter()
                .filter(|entity| selected.contains(&entity.source_id))
                .count();
            require_id_capacity(&candidate, copies)?;
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
                require_id_capacity(&candidate, 1)?;
                candidate.add_layer(layer_name, [215, 222, 234])
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
        require_id_capacity(&candidate, 1)?;
        selected = vec![candidate.add_geometry(geometry)];
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

/// The legacy allocation helpers are infallible and panic on exhaustion. Check
/// their full transaction budget before calling them, including the fresh IDs
/// assigned to block definitions and to exploded leaves. Read-only operations
/// need no allocator capacity, even when next_id is already exhausted.
fn require_id_capacity(document: &Document, count: usize) -> io::Result<()> {
    if count == 0 {
        return Ok(());
    }
    let highest = document
        .layers
        .iter()
        .map(|item| item.id)
        .chain(document.entities.iter().map(|item| item.id))
        .chain(
            document
                .blocks
                .iter()
                .flat_map(|block| block.entities.iter().map(|entity| entity.id)),
        )
        .chain(document.solids.iter().map(|item| item.id))
        .chain(document.extrusions.iter().map(|item| item.id))
        .chain(document.meshes.iter().map(|item| item.id))
        .max()
        .unwrap_or(0);
    let first = document.next_id.max(highest.saturating_add(1)).max(1);
    let count = u64::try_from(count).map_err(|_| invalid("Object ID space is exhausted"))?;
    first
        .checked_add(count)
        .ok_or_else(|| invalid("Object ID space is exhausted"))?;
    Ok(())
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
        if self.0.is_empty() {
            Err(invalid(format!(
                "Missing {label}; enter HELP for command syntax"
            )))
        } else {
            Ok(self.0.pop_front().expect("checked argument"))
        }
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
}
