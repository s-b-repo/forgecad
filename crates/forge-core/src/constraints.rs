//! Persistent linear sketch relations with minimum-displacement solving.
//! Coordinates and radii are model units; general lengths/angles are not solved.
use crate::{Document, Geometry, Point};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
};

const MAX_VARIABLES: usize = 128;
const MAX_CONSTRAINTS: usize = 256;
const COORD_LIMIT: f64 = 1e9;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PointHandle {
    Start,
    End,
    Center,
    Position,
    CornerA,
    CornerB,
    Vertex(usize),
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PointRef {
    pub entity: u64,
    pub handle: PointHandle,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Relation {
    Horizontal {
        entity: u64,
    },
    Vertical {
        entity: u64,
    },
    Coincident {
        a: PointRef,
        b: PointRef,
    },
    FixPoint {
        point: PointRef,
        position: Point,
    },
    DistanceX {
        a: PointRef,
        b: PointRef,
        value: f64,
    },
    DistanceY {
        a: PointRef,
        b: PointRef,
        value: f64,
    },
    Radius {
        entity: u64,
        value: f64,
    },
    EqualRadius {
        first: u64,
        second: u64,
    },
}
impl Relation {
    pub fn entity_ids(&self) -> Vec<u64> {
        match self {
            Self::Horizontal { entity }
            | Self::Vertical { entity }
            | Self::Radius { entity, .. } => vec![*entity],
            Self::Coincident { a, b }
            | Self::DistanceX { a, b, .. }
            | Self::DistanceY { a, b, .. } => vec![a.entity, b.entity],
            Self::FixPoint { point, .. } => vec![point.entity],
            Self::EqualRadius { first, second } => vec![*first, *second],
        }
    }
    pub fn set_value(&mut self, value: f64) -> io::Result<()> {
        match self {
            Self::DistanceX { value: target, .. }
            | Self::DistanceY { value: target, .. }
            | Self::Radius { value: target, .. } => *target = value,
            _ => return Err(invalid("This relation has no editable dimension value")),
        }
        Ok(())
    }
    pub fn value(&self) -> Option<f64> {
        match self {
            Self::DistanceX { value, .. }
            | Self::DistanceY { value, .. }
            | Self::Radius { value, .. } => Some(*value),
            _ => None,
        }
    }
    /// One stable line for command bars, listing panels and CLI output.
    pub fn describe(&self) -> String {
        let label = |reference: &PointRef| {
            format!("#{}.{}", reference.entity, handle_name(reference.handle))
        };
        match self {
            Self::Horizontal { entity } => format!("Horizontal #{entity}"),
            Self::Vertical { entity } => format!("Vertical #{entity}"),
            Self::Coincident { a, b } => format!("Coincident {}→{}", label(a), label(b)),
            Self::FixPoint { point, position } => {
                format!("FixPoint {}@({},{})", label(point), position.x, position.y)
            }
            Self::DistanceX { a, b, value } => {
                format!("DistanceX {value} {}→{}", label(a), label(b))
            }
            Self::DistanceY { a, b, value } => {
                format!("DistanceY {value} {}→{}", label(a), label(b))
            }
            Self::Radius { entity, value } => format!("Radius {value} #{entity}"),
            Self::EqualRadius { first, second } => format!("EqualRadius #{first}=#{second}"),
        }
    }
}
fn handle_name(handle: PointHandle) -> String {
    match handle {
        PointHandle::Start => "start".to_owned(),
        PointHandle::End => "end".to_owned(),
        PointHandle::Center => "center".to_owned(),
        PointHandle::Position => "position".to_owned(),
        PointHandle::CornerA => "cornera".to_owned(),
        PointHandle::CornerB => "cornerb".to_owned(),
        PointHandle::Vertex(index) => format!("vertex{index}"),
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SketchConstraint {
    /// Constraint IDs use their own namespace, independent of drawing IDs.
    pub id: u64,
    pub relation: Relation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintState {
    Satisfied,
    NeedsSolve,
    Conflict,
    NumericalFailure,
}

#[derive(Clone, Debug, Serialize)]
pub struct ConstraintError {
    pub id: u64,
    pub error: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct ConstraintReport {
    pub state: ConstraintState,
    pub constraints: usize,
    pub variables: usize,
    pub equations: usize,
    pub rank: usize,
    /// Only participating entities are counted; this is not the whole drawing.
    pub degrees_of_freedom: usize,
    pub redundant_equations: usize,
    pub tolerance: f64,
    pub max_error: f64,
    /// A proven conflicting equation set, not necessarily a minimal set.
    pub conflicts: Vec<u64>,
    pub errors: Vec<ConstraintError>,
}
impl ConstraintReport {
    pub fn solved(&self) -> bool {
        self.state == ConstraintState::Satisfied
    }
    pub fn summary(&self) -> String {
        format!(
            "{:?}: {} constraints, {} remaining degrees of freedom, maximum error {:.6}; conflicting IDs {:?}",
            self.state, self.constraints, self.degrees_of_freedom, self.max_error, self.conflicts
        )
    }
}

#[derive(Clone, Copy)]
enum Kind {
    Line,
    Rectangle,
    Circle,
    Point,
    Polyline(usize),
}
#[derive(Clone, Copy)]
struct Layout {
    offset: usize,
    kind: Kind,
}
impl Layout {
    fn point(self, handle: PointHandle) -> io::Result<[usize; 2]> {
        let local = match (self.kind, handle) {
            (Kind::Line, PointHandle::Start)
            | (Kind::Rectangle, PointHandle::CornerA)
            | (Kind::Circle, PointHandle::Center)
            | (Kind::Point, PointHandle::Position) => 0,
            (Kind::Line, PointHandle::End) | (Kind::Rectangle, PointHandle::CornerB) => 2,
            (Kind::Polyline(count), PointHandle::Vertex(index)) if index < count => index * 2,
            _ => return Err(invalid("Point handle does not exist on this sketch entity")),
        };
        Ok([self.offset + local, self.offset + local + 1])
    }
}
struct Equation {
    coefficients: Vec<f64>,
    target: f64,
    id: u64,
}
struct System {
    values: Vec<f64>,
    layouts: BTreeMap<u64, Layout>,
    rows: Vec<Equation>,
    tolerance: f64,
}
struct BasisRow {
    coefficients: Vec<f64>,
    rhs: f64,
    provenance: BTreeSet<u64>,
}

impl System {
    fn build(document: &Document) -> io::Result<Self> {
        if document.constraints.len() > MAX_CONSTRAINTS {
            return Err(invalid("Sketch exceeds 256 constraints"));
        }
        let mut ids = BTreeSet::new();
        let mut entities = BTreeSet::new();
        for constraint in &document.constraints {
            if constraint.id == 0 || !ids.insert(constraint.id) {
                return Err(invalid("Sketch constraint IDs must be unique and nonzero"));
            }
            entities.extend(constraint.relation.entity_ids());
        }
        let drawing: BTreeMap<_, _> = document
            .entities
            .iter()
            .map(|e| (e.id, &e.geometry))
            .collect();
        let mut system = Self {
            values: Vec::new(),
            layouts: BTreeMap::new(),
            rows: Vec::new(),
            tolerance: 1e-8,
        };
        for id in entities {
            let geometry = drawing.get(&id).ok_or_else(|| {
                invalid(format!("Sketch constraint refers to missing entity #{id}"))
            })?;
            let (kind, values) = parameters(geometry)?;
            if system.values.len() + values.len() > MAX_VARIABLES {
                return Err(invalid(
                    "Sketch exceeds 128 participating coordinate/radius variables",
                ));
            }
            let layout = Layout {
                offset: system.values.len(),
                kind,
            };
            system.values.extend(values);
            system.layouts.insert(id, layout);
        }
        for constraint in &document.constraints {
            system.add_relation(constraint)?;
        }
        let magnitude = system
            .values
            .iter()
            .chain(system.rows.iter().map(|r| &r.target))
            .fold(1.0f64, |a, b| a.max(b.abs()));
        system.tolerance = 1e-8f64.max(magnitude * f64::EPSILON * 64.0);
        Ok(system)
    }
    fn layout(&self, id: u64) -> io::Result<Layout> {
        self.layouts
            .get(&id)
            .copied()
            .ok_or_else(|| invalid("Sketch entity is unavailable"))
    }
    fn point(&self, reference: PointRef) -> io::Result<[usize; 2]> {
        self.layout(reference.entity)?.point(reference.handle)
    }
    fn line(&self, id: u64) -> io::Result<([usize; 2], [usize; 2])> {
        let layout = self.layout(id)?;
        if !matches!(layout.kind, Kind::Line) {
            return Err(invalid("Horizontal/vertical relations require a LINE"));
        }
        Ok((
            layout.point(PointHandle::Start)?,
            layout.point(PointHandle::End)?,
        ))
    }
    fn radius(&self, id: u64) -> io::Result<usize> {
        let layout = self.layout(id)?;
        if !matches!(layout.kind, Kind::Circle) {
            return Err(invalid("Radius relations require a CIRCLE"));
        }
        Ok(layout.offset + 2)
    }
    fn row(&mut self, id: u64, terms: &[(usize, f64)], target: f64) -> io::Result<()> {
        bounded(target)?;
        let mut coefficients = vec![0.0; self.values.len()];
        for &(index, coefficient) in terms {
            coefficients[index] += coefficient;
        }
        self.rows.push(Equation {
            coefficients,
            target,
            id,
        });
        Ok(())
    }
    fn add_relation(&mut self, constraint: &SketchConstraint) -> io::Result<()> {
        let id = constraint.id;
        match constraint.relation {
            Relation::Horizontal { entity } | Relation::Vertical { entity } => {
                let (a, b) = self.line(entity)?;
                let axis = usize::from(matches!(constraint.relation, Relation::Horizontal { .. }));
                self.row(id, &[(b[axis], 1.0), (a[axis], -1.0)], 0.0)?;
            }
            Relation::Coincident { a, b }
            | Relation::DistanceX { a, b, .. }
            | Relation::DistanceY { a, b, .. } => {
                let a = self.point(a)?;
                let b = self.point(b)?;
                match constraint.relation {
                    Relation::Coincident { .. } => {
                        for axis in 0..2 {
                            self.row(id, &[(b[axis], 1.0), (a[axis], -1.0)], 0.0)?;
                        }
                    }
                    Relation::DistanceX { value, .. } => {
                        self.row(id, &[(b[0], 1.0), (a[0], -1.0)], value)?
                    }
                    Relation::DistanceY { value, .. } => {
                        self.row(id, &[(b[1], 1.0), (a[1], -1.0)], value)?
                    }
                    _ => return Err(invalid("Unsupported point relation")),
                }
            }
            Relation::FixPoint { point, position } => {
                let p = self.point(point)?;
                self.row(id, &[(p[0], 1.0)], position.x)?;
                self.row(id, &[(p[1], 1.0)], position.y)?;
            }
            Relation::Radius { entity, value } => {
                if value < 1e-6 {
                    return Err(invalid(
                        "Constrained radii must be at least 0.000001 model units",
                    ));
                }
                self.row(id, &[(self.radius(entity)?, 1.0)], value)?;
            }
            Relation::EqualRadius { first, second } => self.row(
                id,
                &[(self.radius(first)?, 1.0), (self.radius(second)?, -1.0)],
                0.0,
            )?,
        }
        Ok(())
    }
    fn decompose(&self) -> (Vec<BasisRow>, BTreeSet<u64>, usize) {
        let mut basis: Vec<BasisRow> = Vec::new();
        let mut conflicts = BTreeSet::new();
        let mut redundant = 0;
        for row in &self.rows {
            let mut a = row.coefficients.clone();
            let mut rhs = row.target - dot(&a, &self.values);
            let mut provenance = BTreeSet::from([row.id]);
            // Reorthogonalization keeps rank/residual checks stable for nearly
            // dependent rows. These are linear equations, with no local minima.
            for _ in 0..2 {
                for previous in &basis {
                    let projection = dot(&a, &previous.coefficients);
                    if projection.abs() > 1e-12 {
                        provenance.extend(&previous.provenance);
                    }
                    for (value, unit) in a.iter_mut().zip(&previous.coefficients) {
                        *value -= projection * unit;
                    }
                    rhs -= projection * previous.rhs;
                }
            }
            let norm = dot(&a, &a).sqrt();
            if norm < 1e-10 {
                redundant += 1;
                if rhs.abs() > self.tolerance {
                    conflicts.extend(provenance);
                }
            } else {
                for value in &mut a {
                    *value /= norm;
                }
                basis.push(BasisRow {
                    coefficients: a,
                    rhs: rhs / norm,
                    provenance,
                });
            }
        }
        (basis, conflicts, redundant)
    }
    fn report(
        &self,
        count: usize,
        basis: &[BasisRow],
        conflicts: &BTreeSet<u64>,
        redundant: usize,
    ) -> ConstraintReport {
        let mut errors: BTreeMap<u64, f64> = BTreeMap::new();
        for row in &self.rows {
            let error = (dot(&row.coefficients, &self.values) - row.target).abs();
            errors
                .entry(row.id)
                .and_modify(|v| *v = v.max(error))
                .or_insert(error);
        }
        let max_error = errors.values().copied().fold(0.0, f64::max);
        ConstraintReport {
            state: if !conflicts.is_empty() {
                ConstraintState::Conflict
            } else if max_error <= self.tolerance {
                ConstraintState::Satisfied
            } else {
                ConstraintState::NeedsSolve
            },
            constraints: count,
            variables: self.values.len(),
            equations: self.rows.len(),
            rank: basis.len(),
            degrees_of_freedom: self.values.len().saturating_sub(basis.len()),
            redundant_equations: redundant,
            tolerance: self.tolerance,
            max_error,
            conflicts: conflicts.iter().copied().collect(),
            errors: errors
                .into_iter()
                .map(|(id, error)| ConstraintError { id, error })
                .collect(),
        }
    }
    fn write(&self, document: &mut Document) -> io::Result<()> {
        for entity in &mut document.entities {
            let Some(layout) = self.layouts.get(&entity.id) else {
                continue;
            };
            let values = &self.values[layout.offset..];
            let point = |index| Point::new(values[index], values[index + 1]);
            match &mut entity.geometry {
                Geometry::Line { start, end } => {
                    *start = point(0);
                    *end = point(2);
                }
                Geometry::Rectangle { a, b } => {
                    *a = point(0);
                    *b = point(2);
                }
                Geometry::Circle { center, radius } => {
                    *center = point(0);
                    *radius = values[2];
                }
                Geometry::Point { position } => *position = point(0),
                Geometry::Polyline { vertices, .. } => {
                    for (i, vertex) in vertices.iter_mut().enumerate() {
                        vertex.point = point(i * 2);
                    }
                }
                _ => return Err(invalid("Unsupported constrained geometry")),
            }
            entity.geometry.validate()?;
        }
        Ok(())
    }
}

impl Document {
    /// Constraint IDs live in their own namespace, independent of drawing IDs.
    pub fn next_constraint_id(&self) -> u64 {
        self.constraints
            .iter()
            .map(|constraint| constraint.id)
            .max()
            .unwrap_or(0)
            + 1
    }
    pub fn constraint_report(&self) -> io::Result<ConstraintReport> {
        let system = System::build(self)?;
        let (basis, conflicts, redundant) = system.decompose();
        Ok(system.report(self.constraints.len(), &basis, &conflicts, redundant))
    }
    /// A conflicting or numerically invalid solve leaves the entire document
    /// unchanged. Underconstrained systems move participating variables least.
    pub fn solve_constraints(&mut self) -> io::Result<ConstraintReport> {
        let mut system = System::build(self)?;
        let (basis, conflicts, redundant) = system.decompose();
        let report = system.report(self.constraints.len(), &basis, &conflicts, redundant);
        if !conflicts.is_empty() {
            return Ok(report);
        }
        for row in &basis {
            for (value, coefficient) in system.values.iter_mut().zip(&row.coefficients) {
                *value += row.rhs * coefficient;
            }
        }
        for &value in &system.values {
            bounded(value)?;
        }
        let mut candidate = self.clone();
        system.write(&mut candidate)?;
        candidate.validate()?;
        let mut final_report = system.report(self.constraints.len(), &basis, &conflicts, redundant);
        if final_report.solved() {
            *self = candidate;
        } else {
            final_report.state = ConstraintState::NumericalFailure;
        }
        Ok(final_report)
    }
    /// Replace the constraint set and solve as one transaction.
    pub fn set_constraints(
        &mut self,
        constraints: Vec<SketchConstraint>,
    ) -> io::Result<ConstraintReport> {
        let mut candidate = self.clone();
        candidate.constraints = constraints;
        let report = candidate.solve_constraints()?;
        if report.solved() {
            *self = candidate;
        }
        Ok(report)
    }
    pub fn set_constraint(&mut self, constraint: SketchConstraint) -> io::Result<ConstraintReport> {
        let mut constraints = self.constraints.clone();
        if let Some(existing) = constraints.iter_mut().find(|c| c.id == constraint.id) {
            *existing = constraint;
        } else {
            constraints.push(constraint);
        }
        self.set_constraints(constraints)
    }
    pub fn entity_has_dependents(&self, id: u64) -> bool {
        self.constraints
            .iter()
            .any(|c| c.relation.entity_ids().contains(&id))
            || self.extrusions.iter().any(|e| e.source_entity == Some(id))
    }
    /// Referenced sources cannot be removed implicitly. Detach features and
    /// remove their constraints first; a multi-entity deletion is atomic.
    pub fn delete_entities(&mut self, ids: &[u64]) -> io::Result<usize> {
        let ids: BTreeSet<_> = ids.iter().copied().collect();
        for &id in &ids {
            if self.entity_has_dependents(id) {
                return Err(invalid(format!(
                    "Entity #{id} is referenced by sketch constraints or linked extrusions; remove constraints or detach features first"
                )));
            }
        }
        let before = self.entities.len();
        self.entities.retain(|e| !ids.contains(&e.id));
        Ok(before - self.entities.len())
    }
}

pub(crate) fn validate(document: &Document) -> io::Result<()> {
    System::build(document).map(|_| ())
}

pub fn handles(geometry: &Geometry) -> Vec<PointHandle> {
    match geometry {
        Geometry::Line { .. } => vec![PointHandle::Start, PointHandle::End],
        Geometry::Rectangle { .. } => vec![PointHandle::CornerA, PointHandle::CornerB],
        Geometry::Circle { .. } => vec![PointHandle::Center],
        Geometry::Point { .. } => vec![PointHandle::Position],
        Geometry::Polyline { vertices, .. }
            if vertices.len() <= 64 && vertices.iter().all(|v| v.bulge == 0.0) =>
        {
            (0..vertices.len()).map(PointHandle::Vertex).collect()
        }
        _ => Vec::new(),
    }
}
pub fn point_position(document: &Document, point: PointRef) -> io::Result<Point> {
    let entity = document
        .entities
        .iter()
        .find(|e| e.id == point.entity)
        .ok_or_else(|| invalid("Sketch point entity does not exist"))?;
    let (kind, values) = parameters(&entity.geometry)?;
    let [x, y] = Layout { offset: 0, kind }.point(point.handle)?;
    Ok(Point::new(values[x], values[y]))
}
fn parameters(geometry: &Geometry) -> io::Result<(Kind, Vec<f64>)> {
    geometry.validate()?;
    let (kind, values) = match geometry {
        Geometry::Line { start, end } => (Kind::Line, vec![start.x, start.y, end.x, end.y]),
        Geometry::Rectangle { a, b } => (Kind::Rectangle, vec![a.x, a.y, b.x, b.y]),
        Geometry::Circle { center, radius } => (Kind::Circle, vec![center.x, center.y, *radius]),
        Geometry::Point { position } => (Kind::Point, vec![position.x, position.y]),
        Geometry::Polyline { vertices, .. }
            if (2..=64).contains(&vertices.len()) && vertices.iter().all(|v| v.bulge == 0.0) =>
        {
            (
                Kind::Polyline(vertices.len()),
                vertices
                    .iter()
                    .flat_map(|v| [v.point.x, v.point.y])
                    .collect(),
            )
        }
        _ => {
            return Err(invalid(
                "Sketch constraints support lines, rectangles, circles, points and straight polylines with at most 64 vertices",
            ));
        }
    };
    for &value in &values {
        bounded(value)?;
    }
    Ok((kind, values))
}
fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}
fn bounded(value: f64) -> io::Result<()> {
    if !value.is_finite() || value.abs() > COORD_LIMIT {
        return Err(invalid(
            "Constrained coordinates, radii and dimensions must be finite and within +/-1e9 model units",
        ));
    }
    Ok(())
}
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}
