//! Reusable planar block definitions and nested INSERT expansion.
//! Definitions retain their native entities; expansion is an explicit derived view.
use crate::{Document, Entity, Geometry, Point, edit::Transform2d};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
};

pub const MAX_BLOCK_DEPTH: usize = 32;
pub const MAX_EXPANDED_ENTITIES: usize = 100_000;
pub const MAX_BLOCK_DEFINITIONS: usize = 10_000;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BlockDefinition {
    pub name: String,
    pub base_point: Point,
    pub entities: Vec<Entity>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExpandedEntity {
    /// ID of the top-level selectable entity, including the outermost INSERT.
    pub source_id: u64,
    pub definition_entity_id: u64,
    pub layer_id: u64,
    pub geometry: Geometry,
    /// Combines the visibility of the INSERT chain and the resolved leaf layer.
    pub visible: bool,
}

impl Document {
    pub fn block(&self, name: &str) -> Option<&BlockDefinition> {
        self.blocks
            .iter()
            .find(|block| block.name.eq_ignore_ascii_case(name))
    }

    /// Adds a definition transactionally and assigns fresh globally unique entity IDs.
    pub fn add_block(
        &mut self,
        name: impl Into<String>,
        base_point: Point,
        entities: Vec<Entity>,
    ) -> io::Result<()> {
        let mut candidate = self.clone();
        let mut definition = BlockDefinition {
            name: name.into(),
            base_point,
            entities,
        };
        for entity in &mut definition.entities {
            entity.id = candidate.try_allocate_id()?;
        }
        candidate.blocks.push(definition);
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }

    /// Replaces selected model-space entities with a reusable block at its base point.
    pub fn define_block_from_selection(
        &mut self,
        name: impl Into<String>,
        base_point: Point,
        ids: &[u64],
    ) -> io::Result<u64> {
        let ids: BTreeSet<u64> = ids.iter().copied().collect();
        if ids.is_empty() {
            return Err(invalid("Select at least one object to define a block"));
        }
        let entities: Vec<Entity> = self
            .entities
            .iter()
            .filter(|entity| ids.contains(&entity.id))
            .cloned()
            .collect();
        if entities.len() != ids.len() {
            return Err(invalid("Block selection refers to missing objects"));
        }
        let name = name.into();
        let mut candidate = self.clone();
        candidate.add_block(name.clone(), base_point, entities)?;
        candidate
            .entities
            .retain(|entity| !ids.contains(&entity.id));
        let id = candidate.add_geometry(Geometry::Insert {
            block_name: name,
            position: base_point,
            scale: 1.0,
            rotation_deg: 0.0,
        })?;
        candidate.validate()?;
        *self = candidate;
        Ok(id)
    }

    pub fn expanded_entities(&self) -> io::Result<Vec<ExpandedEntity>> {
        expand_document(self)
    }

    /// Resolves one model-space INSERT into ordinary entities, retaining effective layers.
    pub fn explode_insert(&mut self, id: u64) -> io::Result<Vec<u64>> {
        if !self
            .entities
            .iter()
            .any(|entity| entity.id == id && matches!(entity.geometry, Geometry::Insert { .. }))
        {
            return Err(invalid("The selected object is not a block reference"));
        }
        let leaves: Vec<_> = expand_document(self)?
            .into_iter()
            .filter(|entity| entity.source_id == id)
            .collect();
        let mut candidate = self.clone();
        candidate.entities.retain(|entity| entity.id != id);
        let mut inserted = Vec::new();
        for leaf in leaves {
            let old_layer = candidate.active_layer_id;
            candidate.active_layer_id = leaf.layer_id;
            inserted.push(candidate.add_geometry(leaf.geometry)?);
            candidate.active_layer_id = old_layer;
        }
        candidate.validate()?;
        *self = candidate;
        Ok(inserted)
    }
}

pub fn validate_definitions(document: &Document) -> io::Result<()> {
    if document.blocks.len() > MAX_BLOCK_DEFINITIONS {
        return Err(invalid("Too many block definitions"));
    }
    let mut names = BTreeMap::new();
    for (index, block) in document.blocks.iter().enumerate() {
        if block.name.trim().is_empty()
            || block.name.chars().count() > 255
            || block.name.contains(['\n', '\r', '\0'])
            || is_layout_block(&block.name)
        {
            return Err(invalid("Invalid or reserved block name"));
        }
        if !block.base_point.is_finite() {
            return Err(invalid("Block base point must be finite"));
        }
        if names
            .insert(block.name.to_ascii_lowercase(), index)
            .is_some()
        {
            return Err(invalid("Block names must be unique, ignoring case"));
        }
    }
    let stored = document.blocks.iter().try_fold(0usize, |sum, block| {
        sum.checked_add(block.entities.len())
            .ok_or_else(|| invalid("Block entity count overflow"))
    })?;
    if stored > MAX_EXPANDED_ENTITIES {
        return Err(invalid("Too many stored block entities"));
    }
    let mut state = vec![0u8; document.blocks.len()];
    let mut memo = vec![(0usize, 0usize); document.blocks.len()];
    for index in 0..document.blocks.len() {
        block_cost(index, document, &names, &mut state, &mut memo, 1)?;
    }
    let mut total = 0usize;
    for entity in &document.entities {
        let count = if let Geometry::Insert { block_name, .. } = &entity.geometry {
            let index = *names
                .get(&block_name.to_ascii_lowercase())
                .ok_or_else(|| invalid(format!("Missing block definition: {block_name}")))?;
            memo[index].0
        } else {
            1
        };
        total = total
            .checked_add(count)
            .ok_or_else(|| invalid("Expanded entity count overflow"))?;
        if total > MAX_EXPANDED_ENTITIES {
            return Err(invalid("Block expansion exceeds the entity resource limit"));
        }
    }
    Ok(())
}
fn block_cost(
    index: usize,
    document: &Document,
    names: &BTreeMap<String, usize>,
    state: &mut [u8],
    memo: &mut [(usize, usize)],
    current_depth: usize,
) -> io::Result<(usize, usize)> {
    if current_depth > MAX_BLOCK_DEPTH {
        return Err(invalid("Block nesting exceeds the depth resource limit"));
    }
    if state[index] == 1 {
        return Err(invalid("Cyclic block references are not supported"));
    }
    if state[index] == 2 {
        if current_depth + memo[index].1 - 1 > MAX_BLOCK_DEPTH {
            return Err(invalid("Block nesting exceeds the depth resource limit"));
        }
        return Ok(memo[index]);
    }
    state[index] = 1;
    let mut count = 0usize;
    let mut depth = 1usize;
    for entity in &document.blocks[index].entities {
        let (leaves, child_depth) = if let Geometry::Insert { block_name, .. } = &entity.geometry {
            let child = *names
                .get(&block_name.to_ascii_lowercase())
                .ok_or_else(|| invalid(format!("Missing block definition: {block_name}")))?;
            let (leaves, depth) =
                block_cost(child, document, names, state, memo, current_depth + 1)?;
            (leaves, depth + 1)
        } else {
            (1, 1)
        };
        count = count
            .checked_add(leaves)
            .ok_or_else(|| invalid("Expanded block count overflow"))?;
        depth = depth.max(child_depth);
        if depth > MAX_BLOCK_DEPTH {
            return Err(invalid("Block nesting exceeds the depth resource limit"));
        }
        if count > MAX_EXPANDED_ENTITIES {
            return Err(invalid("Block expansion exceeds the entity resource limit"));
        }
    }
    state[index] = 2;
    memo[index] = (count, depth);
    Ok(memo[index])
}

pub fn is_layout_block(name: &str) -> bool {
    name.eq_ignore_ascii_case("*Model_Space")
        || name.to_ascii_lowercase().starts_with("*paper_space")
}

pub fn insertion_transform(
    block: &BlockDefinition,
    position: Point,
    scale: f64,
    rotation_deg: f64,
) -> io::Result<Transform2d> {
    Geometry::Insert {
        block_name: block.name.clone(),
        position,
        scale,
        rotation_deg,
    }
    .validate()?;
    let transform = Transform2d::translation(-block.base_point.x, -block.base_point.y)
        .then(Transform2d::uniform_scale(scale, Point::default()))
        .then(Transform2d::rotation(rotation_deg, Point::default()))
        .then(Transform2d::translation(position.x, position.y));
    if transform
        .matrix
        .iter()
        .flatten()
        .chain([&transform.offset.x, &transform.offset.y])
        .any(|value| !value.is_finite())
    {
        return Err(invalid("Block transform overflows finite coordinates"));
    }
    Ok(transform)
}

pub fn expand_document(document: &Document) -> io::Result<Vec<ExpandedEntity>> {
    document.validate()?;
    let mut result = Vec::new();
    for entity in &document.entities {
        expand_entity(
            document,
            entity,
            entity.id,
            Transform2d::identity(),
            entity.layer_id,
            true,
            0,
            &mut result,
        )?;
    }
    Ok(result)
}
#[allow(clippy::too_many_arguments)]
fn expand_entity(
    document: &Document,
    entity: &Entity,
    source_id: u64,
    parent: Transform2d,
    inherited_layer: u64,
    parent_visible: bool,
    depth: usize,
    output: &mut Vec<ExpandedEntity>,
) -> io::Result<()> {
    if depth > MAX_BLOCK_DEPTH {
        return Err(invalid("Block nesting exceeds the depth resource limit"));
    }
    let layer = document
        .layer(entity.layer_id)
        .ok_or_else(|| invalid("Missing block entity layer"))?;
    let effective_layer = if layer.name == "0" {
        inherited_layer
    } else {
        entity.layer_id
    };
    let visible = parent_visible
        && document
            .layer(effective_layer)
            .is_some_and(|layer| layer.visible);
    if let Geometry::Insert {
        block_name,
        position,
        scale,
        rotation_deg,
    } = &entity.geometry
    {
        let definition = document
            .block(block_name)
            .ok_or_else(|| invalid(format!("Missing block definition: {block_name}")))?;
        let transform =
            insertion_transform(definition, *position, *scale, *rotation_deg)?.then(parent);
        for child in &definition.entities {
            expand_entity(
                document,
                child,
                source_id,
                transform,
                effective_layer,
                visible,
                depth + 1,
                output,
            )?;
        }
    } else {
        if output.len() >= MAX_EXPANDED_ENTITIES {
            return Err(invalid("Block expansion exceeds the entity resource limit"));
        }
        output.push(ExpandedEntity {
            source_id,
            definition_entity_id: entity.id,
            layer_id: effective_layer,
            geometry: entity.geometry.transformed(parent)?,
            visible,
        });
    }
    Ok(())
}
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn line(id: u64, layer: u64, a: Point, b: Point) -> Entity {
        Entity {
            id,
            layer_id: layer,
            geometry: Geometry::Line { start: a, end: b },
        }
    }
    fn insert(
        id: u64,
        layer: u64,
        name: &str,
        position: Point,
        scale: f64,
        rotation: f64,
    ) -> Entity {
        Entity {
            id,
            layer_id: layer,
            geometry: Geometry::Insert {
                block_name: name.into(),
                position,
                scale,
                rotation_deg: rotation,
            },
        }
    }
    #[test]
    fn nested_transforms_honor_base_points_layers_and_outer_selection_id() {
        let mut document = Document::default();
        let layer = document.add_layer("Instance", [1, 2, 3]).unwrap();
        document
            .add_block(
                "Leaf",
                Point::new(1.0, 0.0),
                vec![line(0, 1, Point::new(1.0, 0.0), Point::new(3.0, 0.0))],
            )
            .unwrap();
        document
            .add_block(
                "Nested",
                Point::new(5.0, 0.0),
                vec![insert(0, 1, "leaf", Point::new(5.0, 0.0), 2.0, 90.0)],
            )
            .unwrap();
        let id = document
            .add_geometry(Geometry::Insert {
                block_name: "Nested".into(),
                position: Point::new(10.0, 20.0),
                scale: 3.0,
                rotation_deg: 90.0,
            })
            .unwrap();
        let leaves = document.expanded_entities().unwrap();
        assert_eq!(leaves.len(), 1);
        assert_eq!(leaves[0].source_id, id);
        assert_eq!(leaves[0].layer_id, layer);
        if let Geometry::Line { start, end } = leaves[0].geometry {
            assert!(start.distance(Point::new(10.0, 20.0)) < 1e-10);
            assert!(end.distance(Point::new(-2.0, 20.0)) < 1e-10);
        } else {
            panic!("expected line");
        }
        let native = document.to_json().unwrap();
        assert_eq!(Document::from_json(&native).unwrap(), document);
    }
    #[test]
    fn explicit_child_layers_and_hidden_insert_chains_are_retained() {
        let mut document = Document::default();
        let explicit = document.add_layer("Explicit", [5, 6, 7]).unwrap();
        let insertion = document.add_layer("Insertion", [1, 2, 3]).unwrap();
        document
            .add_block(
                "Pair",
                Point::default(),
                vec![
                    line(0, 1, Point::default(), Point::new(1.0, 0.0)),
                    line(0, explicit, Point::default(), Point::new(0.0, 1.0)),
                ],
            )
            .unwrap();
        document
            .add_geometry(Geometry::Insert {
                block_name: "Pair".into(),
                position: Point::default(),
                scale: 1.0,
                rotation_deg: 0.0,
            })
            .unwrap();
        let leaves = document.expanded_entities().unwrap();
        assert_eq!(leaves[0].layer_id, insertion);
        assert_eq!(leaves[1].layer_id, explicit);
        document
            .layers
            .iter_mut()
            .find(|layer| layer.id == insertion)
            .unwrap()
            .visible = false;
        assert!(
            document
                .expanded_entities()
                .unwrap()
                .iter()
                .all(|leaf| !leaf.visible)
        );
    }
    #[test]
    fn missing_cyclic_deep_and_explosive_references_are_rejected() {
        let mut missing = Document::default();
        missing
            .add_geometry(Geometry::Insert {
                block_name: "Missing".into(),
                position: Point::default(),
                scale: 1.0,
                rotation_deg: 0.0,
            })
            .unwrap();
        assert!(missing.validate().is_err());
        let mut cycle = Document::default();
        cycle.blocks.push(BlockDefinition {
            name: "Cycle".into(),
            base_point: Point::default(),
            entities: vec![insert(2, 1, "Cycle", Point::default(), 1.0, 0.0)],
        });
        assert!(cycle.validate().is_err());
        let mut deep = Document::default();
        let mut id = 2;
        for index in 0..=MAX_BLOCK_DEPTH {
            let entity = if index == MAX_BLOCK_DEPTH {
                line(id, 1, Point::default(), Point::new(1.0, 0.0))
            } else {
                insert(
                    id,
                    1,
                    &format!("B{}", index + 1),
                    Point::default(),
                    1.0,
                    0.0,
                )
            };
            id += 1;
            deep.blocks.push(BlockDefinition {
                name: format!("B{index}"),
                base_point: Point::default(),
                entities: vec![entity],
            });
        }
        assert!(deep.validate().is_err());
        let mut explosive = Document::default();
        explosive
            .add_block(
                "B0",
                Point::default(),
                vec![line(0, 1, Point::default(), Point::new(1.0, 0.0))],
            )
            .unwrap();
        for index in 1..18 {
            let children =
                vec![insert(0, 1, &format!("B{}", index - 1), Point::default(), 1.0, 0.0); 2];
            let result = explosive.add_block(format!("B{index}"), Point::default(), children);
            if index == 17 {
                assert!(result.is_err());
            } else {
                result.unwrap();
            }
        }
    }
    #[test]
    fn definition_and_explode_operations_are_transactional() {
        let mut document = Document::default();
        let a = document
            .add_geometry(Geometry::Line {
                start: Point::default(),
                end: Point::new(4.0, 0.0),
            })
            .unwrap();
        let b = document
            .add_geometry(Geometry::Circle {
                center: Point::new(2.0, 2.0),
                radius: 1.0,
            })
            .unwrap();
        let instance = document
            .define_block_from_selection("Part", Point::new(2.0, 0.0), &[a, b])
            .unwrap();
        assert_eq!(document.entities.len(), 1);
        assert_eq!(document.blocks[0].entities.len(), 2);
        let snapshot = document.clone();
        assert!(
            document
                .add_block("part", Point::default(), vec![])
                .is_err()
        );
        assert_eq!(document, snapshot);
        let new_ids = document.explode_insert(instance).unwrap();
        assert_eq!(new_ids.len(), 2);
        assert_eq!(document.entities.len(), 2);
        assert!(
            document
                .entities
                .iter()
                .all(|entity| entity.id != a && entity.id != b)
        );
        document.validate().unwrap();
    }

    /// Defining a block and exploding an instance both need several fresh IDs.
    /// At a nearly exhausted allocator they must refuse with an error and leave
    /// the saved document byte-identical.
    #[test]
    fn block_authoring_and_explosion_refuse_to_panic_at_the_id_limit() {
        let mut document = Document::default();
        let a = document
            .add_geometry(Geometry::Line {
                start: Point::default(),
                end: Point::new(4.0, 0.0),
            })
            .unwrap();
        let b = document
            .add_geometry(Geometry::Line {
                start: Point::new(0.0, 1.0),
                end: Point::new(4.0, 1.0),
            })
            .unwrap();
        let instance = document
            .define_block_from_selection("Part", Point::default(), &[a, b])
            .unwrap();
        // Two fresh IDs are required by each operation; only one remains.
        document.next_id = u64::MAX - 1;
        let before = document.to_json().unwrap();
        assert!(
            document
                .define_block_from_selection("Second", Point::default(), &[instance])
                .is_err()
        );
        assert!(document.explode_insert(instance).is_err());
        assert_eq!(document.to_json().unwrap(), before);
    }
}
