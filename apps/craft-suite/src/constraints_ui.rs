//! Sketch constraints: linear relations, the solver report and a transactional
//! authoring form. Every commit clones the document, applies the relation and
//! solves it; a rejected or conflicting sketch leaves the document and the undo
//! history exactly as they were.
use super::*;
use forge_core::constraints::{ConstraintState, PointHandle, PointRef, Relation, SketchConstraint};

/// The eight relations the desktop form can author; the drafting command
/// language and the solver accept exactly this set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum ConstraintKind {
    #[default]
    Horizontal,
    Vertical,
    Coincident,
    FixPoint,
    DistanceX,
    DistanceY,
    Radius,
    EqualRadius,
}

impl ConstraintKind {
    const ALL: [Self; 8] = [
        Self::Horizontal,
        Self::Vertical,
        Self::Coincident,
        Self::FixPoint,
        Self::DistanceX,
        Self::DistanceY,
        Self::Radius,
        Self::EqualRadius,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Horizontal => "Horizontal",
            Self::Vertical => "Vertical",
            Self::Coincident => "Coincident",
            Self::FixPoint => "Fix point",
            Self::DistanceX => "Distance X",
            Self::DistanceY => "Distance Y",
            Self::Radius => "Radius",
            Self::EqualRadius => "Equal radius",
        }
    }

    fn needs_handles(self) -> bool {
        matches!(
            self,
            Self::Coincident | Self::DistanceX | Self::DistanceY | Self::FixPoint
        )
    }

    fn needs_second(self) -> bool {
        matches!(
            self,
            Self::Coincident | Self::DistanceX | Self::DistanceY | Self::EqualRadius
        )
    }

    fn needs_value(self) -> bool {
        matches!(self, Self::DistanceX | Self::DistanceY | Self::Radius)
    }

    fn needs_fixed(self) -> bool {
        matches!(self, Self::FixPoint)
    }
}

#[derive(Default)]
pub(super) struct ConstraintUiState {
    pub(super) kind: ConstraintKind,
    pub(super) first_entity: u64,
    pub(super) first_handle: usize,
    pub(super) second_entity: u64,
    pub(super) second_handle: usize,
    pub(super) value: f64,
    pub(super) fixed: [f64; 2],
}

impl ForgeApp {
    pub(super) fn constraints_controls(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Sketch constraints", |ui| {
            self.constraints_report(ui);
            self.constraints_actions(ui);
            self.constraints_authoring(ui);
            self.constraints_list(ui);
        });
    }

    /// The solver state of the whole sketch, recomputed every frame.
    fn constraints_report(&mut self, ui: &mut egui::Ui) {
        ui.label(
            RichText::new(
                "Linear relations on lines, rectangles, circles, points and straight polylines. Coordinates and radii are model units.",
            )
            .size(9.0)
            .color(MUTED),
        );
        match self.document.constraint_report() {
            Ok(report) => {
                let solved = report.solved();
                ui.label(
                    RichText::new(format!("{:?}", report.state))
                        .size(11.0)
                        .strong()
                        .color(if solved { ACCENT } else { Color32::LIGHT_RED }),
                );
                ui.label(format!(
                    "{} relation(s) · {} variable(s) · {} degree(s) of freedom · {} redundant equation(s)",
                    report.constraints,
                    report.variables,
                    report.degrees_of_freedom,
                    report.redundant_equations
                ));
                ui.label(format!(
                    "Maximum error {:.6} within tolerance {:.9}",
                    report.max_error, report.tolerance
                ));
                if !report.conflicts.is_empty() {
                    ui.label(
                        RichText::new(format!("Conflicting relation IDs {:?}", report.conflicts))
                            .size(10.0)
                            .color(Color32::LIGHT_RED),
                    );
                }
            }
            Err(error) => {
                ui.label(
                    RichText::new(error.to_string())
                        .size(10.0)
                        .color(Color32::LIGHT_RED),
                );
            }
        }
    }

    fn constraints_actions(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.button("Report").clicked() {
                self.constraints_show_report();
            }
            if ui.button("Solve").clicked() {
                self.constraints_solve();
            }
            if ui.button("Clear all").clicked() {
                self.constraints_clear();
            }
        });
    }

    fn constraints_authoring(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);
        ui.separator();
        ui.label(
            RichText::new("ADD RELATION")
                .size(9.0)
                .strong()
                .color(MUTED),
        );
        egui::ComboBox::from_id_salt("constraint_relation_kind")
            .selected_text(self.constraints_ui.kind.label())
            .show_ui(ui, |ui| {
                for kind in ConstraintKind::ALL {
                    ui.selectable_value(&mut self.constraints_ui.kind, kind, kind.label());
                }
            });
        let kind = self.constraints_ui.kind;
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut self.constraints_ui.first_entity)
                    .prefix("First object #"),
            );
            if ui.small_button("Use selected").clicked() {
                self.constraints_fill_from_selection();
            }
        });
        if kind.needs_handles() {
            let handles = self.entity_handles(self.constraints_ui.first_entity);
            constraint_handle_combo(
                ui,
                "constraint_first_handle",
                "First sketch point",
                &handles,
                &mut self.constraints_ui.first_handle,
            );
        }
        if kind.needs_second() {
            ui.add(
                egui::DragValue::new(&mut self.constraints_ui.second_entity)
                    .prefix("Second object #"),
            );
            if kind.needs_handles() {
                let handles = self.entity_handles(self.constraints_ui.second_entity);
                constraint_handle_combo(
                    ui,
                    "constraint_second_handle",
                    "Second sketch point",
                    &handles,
                    &mut self.constraints_ui.second_handle,
                );
            }
        }
        if kind.needs_value() {
            ui.add(
                egui::DragValue::new(&mut self.constraints_ui.value)
                    .speed(0.5)
                    .prefix("Value "),
            );
        }
        if kind.needs_fixed() {
            ui.label(RichText::new("Fixed position").size(10.0).color(MUTED));
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut self.constraints_ui.fixed[0])
                        .speed(1.0)
                        .prefix("X "),
                );
                ui.add(
                    egui::DragValue::new(&mut self.constraints_ui.fixed[1])
                        .speed(1.0)
                        .prefix("Y "),
                );
            });
        }
        if ui.button("Add relation").clicked() {
            self.constraints_add();
        }
    }

    fn constraints_list(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);
        ui.separator();
        ui.label(
            RichText::new("STORED RELATIONS")
                .size(9.0)
                .strong()
                .color(MUTED),
        );
        if self.document.constraints.is_empty() {
            ui.label(
                RichText::new(
                    "No relations yet. Add one above, or enter RELATION in the command bar.",
                )
                .size(10.0)
                .color(MUTED),
            );
        }
        let mut remove = None;
        for constraint in &self.document.constraints {
            ui.push_id(constraint.id, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "#{}  {}",
                            constraint.id,
                            constraint.relation.describe()
                        ))
                        .size(10.0),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.small_button("Remove").clicked() {
                            remove = Some(constraint.id);
                        }
                    });
                });
            });
        }
        if let Some(id) = remove {
            self.constraints_remove(id);
        }
    }

    fn constraints_fill_from_selection(&mut self) {
        let ids: Vec<u64> = self.selected_entities.iter().copied().collect();
        if ids.is_empty() {
            self.status = "Select objects in the canvas before using the selection".into();
            self.status_is_error = true;
            return;
        }
        if let Some(first) = ids.first() {
            self.constraints_ui.first_entity = *first;
            self.constraints_ui.second_entity = 0;
            self.constraints_ui.first_handle = 0;
            self.constraints_ui.second_handle = 0;
        }
        if let Some(second) = ids.get(1) {
            self.constraints_ui.second_entity = *second;
        }
    }

    fn constraints_show_report(&mut self) {
        match self.document.constraint_report() {
            Ok(report) => {
                self.status_is_error = matches!(
                    report.state,
                    ConstraintState::Conflict | ConstraintState::NumericalFailure
                );
                self.status = report.summary();
            }
            Err(error) => {
                self.status = error.to_string();
                self.status_is_error = true;
            }
        }
    }

    fn constraints_solve(&mut self) {
        let mut candidate = self.document.clone();
        let applied = candidate.solve_constraints();
        self.commit_constraint_transaction(candidate, "Sketch relations solved", applied);
    }

    fn constraints_add(&mut self) {
        let Some(relation) = self.authored_relation() else {
            return;
        };
        let mut candidate = self.document.clone();
        let id = candidate.next_constraint_id();
        let applied = candidate.set_constraint(SketchConstraint { id, relation });
        self.commit_constraint_transaction(candidate, "Sketch relation added", applied);
    }

    fn constraints_remove(&mut self, id: u64) {
        let kept: Vec<SketchConstraint> = self
            .document
            .constraints
            .iter()
            .filter(|constraint| constraint.id != id)
            .cloned()
            .collect();
        self.constraints_replace(kept, format!("Relation #{id} removed"));
    }

    fn constraints_clear(&mut self) {
        self.constraints_replace(Vec::new(), "All sketch relations cleared");
    }

    fn constraints_replace(
        &mut self,
        constraints: Vec<SketchConstraint>,
        label: impl Into<String>,
    ) {
        let mut candidate = self.document.clone();
        let applied = candidate.set_constraints(constraints);
        self.commit_constraint_transaction(candidate, label, applied);
    }

    /// One transaction per commit: an unsolved or rejected sketch reports why
    /// and leaves the document, the history and the selection untouched.
    fn commit_constraint_transaction(
        &mut self,
        candidate: Document,
        label: impl Into<String>,
        applied: std::io::Result<forge_core::constraints::ConstraintReport>,
    ) -> bool {
        match applied {
            Ok(report) if report.solved() => self.commit_workbench_document(candidate, label),
            Ok(report) => {
                self.status = report.summary();
                self.status_is_error = true;
                false
            }
            Err(error) => {
                self.status = error.to_string();
                self.status_is_error = true;
                false
            }
        }
    }

    /// The relation described by the authoring form, or `None` after the form
    /// reported why it cannot be built.
    fn authored_relation(&mut self) -> Option<Relation> {
        let (kind, first, first_handle, second, second_handle, value, fixed) = {
            let form = &self.constraints_ui;
            (
                form.kind,
                form.first_entity,
                form.first_handle,
                form.second_entity,
                form.second_handle,
                form.value,
                form.fixed,
            )
        };
        if first == 0 || (kind.needs_second() && second == 0) {
            return self
                .constraint_form_error("Every referenced object needs a positive object ID");
        }
        if !self.form_entity(first) || (kind.needs_second() && !self.form_entity(second)) {
            return None;
        }
        match kind {
            ConstraintKind::Horizontal => Some(Relation::Horizontal { entity: first }),
            ConstraintKind::Vertical => Some(Relation::Vertical { entity: first }),
            ConstraintKind::Radius => Some(Relation::Radius {
                entity: first,
                value,
            }),
            ConstraintKind::EqualRadius => Some(Relation::EqualRadius { first, second }),
            ConstraintKind::Coincident => Some(Relation::Coincident {
                a: self.form_point(first, first_handle)?,
                b: self.form_point(second, second_handle)?,
            }),
            ConstraintKind::DistanceX => Some(Relation::DistanceX {
                a: self.form_point(first, first_handle)?,
                b: self.form_point(second, second_handle)?,
                value,
            }),
            ConstraintKind::DistanceY => Some(Relation::DistanceY {
                a: self.form_point(first, first_handle)?,
                b: self.form_point(second, second_handle)?,
                value,
            }),
            ConstraintKind::FixPoint => Some(Relation::FixPoint {
                point: self.form_point(first, first_handle)?,
                position: Point::new(fixed[0], fixed[1]),
            }),
        }
    }

    fn form_entity(&mut self, entity: u64) -> bool {
        if self.document.entities.iter().any(|e| e.id == entity) {
            return true;
        }
        self.status = format!("Object #{entity} does not exist in this drawing");
        self.status_is_error = true;
        false
    }

    /// The stored handle index is resolved against the handles the entity
    /// currently has, so a document reload or a changed geometry cannot pick a
    /// point that no longer exists.
    fn form_point(&mut self, entity: u64, handle: usize) -> Option<PointRef> {
        let handles = self.entity_handles(entity);
        let Some(handle) = handles.get(handle).copied() else {
            return self
                .constraint_form_error(format!("Pick one of object #{entity}'s sketch points"));
        };
        Some(PointRef { entity, handle })
    }

    fn constraint_form_error<T>(&mut self, message: impl Into<String>) -> Option<T> {
        self.status = message.into();
        self.status_is_error = true;
        None
    }

    fn entity_handles(&self, entity: u64) -> Vec<PointHandle> {
        self.document
            .entities
            .iter()
            .find(|e| e.id == entity)
            .map(|e| forge_core::constraints::handles(&e.geometry))
            .unwrap_or_default()
    }
}

fn constraint_handle_combo(
    ui: &mut egui::Ui,
    salt: &str,
    label: &str,
    handles: &[PointHandle],
    current: &mut usize,
) {
    let text = match handles.get(*current) {
        Some(handle) => handle_label(*handle),
        None => "No sketch point".to_owned(),
    };
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(10.0).color(MUTED));
        egui::ComboBox::from_id_salt(salt)
            .selected_text(text)
            .show_ui(ui, |ui| {
                if handles.is_empty() {
                    ui.label(
                        RichText::new("This object has no sketch points")
                            .size(9.0)
                            .color(MUTED),
                    );
                }
                for (index, handle) in handles.iter().enumerate() {
                    ui.selectable_value(current, index, handle_label(*handle));
                }
            });
    });
}

/// The same names the relation listing and the command language use.
fn handle_label(handle: PointHandle) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn line(app: &mut ForgeApp, end_y: f64) -> u64 {
        app.document
            .add_geometry(Geometry::Line {
                start: Point::new(0.0, 0.0),
                end: Point::new(10.0, end_y),
            })
            .unwrap()
    }

    fn endpoints(app: &ForgeApp, id: u64) -> (Point, Point) {
        match &app
            .document
            .entities
            .iter()
            .find(|entity| entity.id == id)
            .expect("the entity still exists")
            .geometry
        {
            Geometry::Line { start, end } => (*start, *end),
            other => panic!("expected a line, found {other:?}"),
        }
    }

    /// Solved coordinates settle within solver tolerance, not bit-exactly.
    fn near(left: f64, right: f64) {
        assert!((left - right).abs() < 1e-9, "{left} vs {right}");
    }

    /// The panel renders headlessly while holding an empty, a populated, a
    /// conflicting and a stale-form sketch state, and the Draft inspector
    /// exposes it with and without a selection.
    #[test]
    fn the_panel_renders_every_form_state() {
        let mut app = ForgeApp::default();
        let render = |app: &mut ForgeApp| {
            let context = egui::Context::default();
            context
                .run_ui(Default::default(), |ui| {
                    app.draft_inspector(ui);
                })
                .drop_without_applying_deltas();
        };
        render(&mut app);
        let sloped = line(&mut app, 7.0);
        let circle = app
            .document
            .add_geometry(Geometry::Circle {
                center: Point::new(20.0, 0.0),
                radius: 5.0,
            })
            .unwrap();
        app.constraints_ui.kind = ConstraintKind::Horizontal;
        app.constraints_ui.first_entity = sloped;
        app.constraints_add();
        render(&mut app);
        app.constraints_ui.kind = ConstraintKind::Radius;
        app.constraints_ui.first_entity = circle;
        app.constraints_ui.value = 5.0;
        app.constraints_add();
        assert!(!app.status_is_error, "{}", app.status);
        app.constraints_ui.value = 9.0;
        app.constraints_add();
        assert!(app.status_is_error, "{}", app.status);
        render(&mut app);
        // A stale handle index and a missing entity must not break rendering.
        app.constraints_ui.kind = ConstraintKind::Coincident;
        app.constraints_ui.first_handle = 7;
        app.constraints_ui.second_entity = 999;
        render(&mut app);
        app.constraints_ui.kind = ConstraintKind::FixPoint;
        app.constraints_ui.first_entity = sloped;
        render(&mut app);
        app.constraints_ui.kind = ConstraintKind::EqualRadius;
        render(&mut app);
        assert_eq!(app.document.constraints.len(), 2);
        // The panel also renders beside a live selection in the Draft inspector.
        app.selected_entities.insert(sloped);
        render(&mut app);
        assert_eq!(app.document.constraints.len(), 2);
    }

    #[test]
    fn horizontal_relation_from_the_panel_moves_the_line_with_one_undo_entry() {
        let mut app = ForgeApp::default();
        let sloped = line(&mut app, 7.0);
        app.constraints_ui.kind = ConstraintKind::Horizontal;
        app.constraints_ui.first_entity = sloped;
        app.constraints_add();
        assert!(!app.status_is_error, "{}", app.status);
        assert_eq!(app.document.constraints.len(), 1);
        assert_eq!(
            app.document.constraints[0].relation.describe(),
            format!("Horizontal #{sloped}")
        );
        assert_eq!(app.undo_stack.len(), 1);
        assert!(app.dirty);
        // Minimum displacement: both endpoints settle on the shared midpoint.
        let (start, end) = endpoints(&app, sloped);
        near(start.y, end.y);
        near(start.y, 3.5);
        assert_eq!(start.x, 0.0);
        assert_eq!(end.x, 10.0);
        app.undo();
        let (start, end) = endpoints(&app, sloped);
        assert_eq!(start, Point::new(0.0, 0.0));
        assert_eq!(end, Point::new(10.0, 7.0));
        assert!(app.document.constraints.is_empty());
    }

    #[test]
    fn conflicting_relations_are_refused_without_touching_history() {
        let mut app = ForgeApp::default();
        let circle = app
            .document
            .add_geometry(Geometry::Circle {
                center: Point::new(0.0, 0.0),
                radius: 5.0,
            })
            .unwrap();
        app.constraints_ui.kind = ConstraintKind::Radius;
        app.constraints_ui.first_entity = circle;
        app.constraints_ui.value = 5.0;
        app.constraints_add();
        assert!(!app.status_is_error, "{}", app.status);
        assert_eq!(app.undo_stack.len(), 1);
        app.constraints_ui.value = 7.0;
        let before = app.document.to_json().unwrap();
        let history = app.undo_stack.len();
        app.constraints_add();
        assert!(app.status_is_error);
        assert!(app.status.contains("Conflict"), "{}", app.status);
        assert_eq!(app.document.to_json().unwrap(), before);
        assert_eq!(app.document.constraints.len(), 1);
        assert_eq!(app.undo_stack.len(), history);
    }

    #[test]
    fn remove_and_clear_relations_are_transactional() {
        let mut app = ForgeApp::default();
        let first = line(&mut app, 7.0);
        let second = line(&mut app, 5.0);
        app.constraints_ui.kind = ConstraintKind::Horizontal;
        app.constraints_ui.first_entity = first;
        app.constraints_add();
        app.constraints_ui.first_entity = second;
        app.constraints_add();
        assert_eq!(app.document.constraints.len(), 2);
        assert_eq!(app.undo_stack.len(), 2);
        let ids: Vec<u64> = app.document.constraints.iter().map(|c| c.id).collect();
        app.constraints_remove(ids[0]);
        assert!(!app.status_is_error, "{}", app.status);
        assert_eq!(app.document.constraints.len(), 1);
        assert_eq!(
            app.document.constraints[0].relation.entity_ids(),
            vec![second]
        );
        assert_eq!(app.undo_stack.len(), 3);
        app.undo();
        assert_eq!(app.document.constraints.len(), 2);
        app.constraints_clear();
        assert!(!app.status_is_error, "{}", app.status);
        assert!(app.document.constraints.is_empty());
        // The geometry the relations held survives their removal.
        let (start, end) = endpoints(&app, second);
        near(start.y, end.y);
        assert_eq!(app.undo_stack.len(), 3);
    }

    #[test]
    fn deleting_a_constrained_entity_is_refused_until_relations_are_cleared() {
        let mut app = ForgeApp::default();
        let constrained = line(&mut app, 7.0);
        app.constraints_ui.kind = ConstraintKind::Horizontal;
        app.constraints_ui.first_entity = constrained;
        app.constraints_add();
        assert_eq!(app.document.constraints.len(), 1);
        let before = app.document.clone();
        app.selected_entities.insert(constrained);
        app.delete_selected_entities();
        assert_eq!(app.document.entities.len(), 1);
        assert_eq!(app.document.constraints.len(), 1);
        assert_eq!(app.document, before);
        app.constraints_clear();
        assert!(app.document.constraints.is_empty());
        app.selected_entities.insert(constrained);
        app.delete_selected_entities();
        assert!(app.document.entities.is_empty());
        assert!(!app.status_is_error, "{}", app.status);
    }

    #[test]
    fn the_shared_command_bar_solves_desktop_relations() {
        let mut app = ForgeApp::default();
        let sloped = line(&mut app, 7.0);
        app.run_draft_command(&format!("RELATION HORIZONTAL {sloped}"));
        assert!(!app.status_is_error, "{}", app.status);
        assert_eq!(app.document.constraints.len(), 1);
        app.run_draft_command("CONSTRAINT SOLVE");
        assert!(!app.status_is_error, "{}", app.status);
        assert!(app.status.contains("Satisfied"), "{}", app.status);
        let (start, end) = endpoints(&app, sloped);
        assert_eq!(start.y, end.y);
    }

    #[test]
    fn an_invalid_object_id_leaves_history_untouched() {
        let mut app = ForgeApp::default();
        line(&mut app, 7.0);
        app.constraints_ui.kind = ConstraintKind::Horizontal;
        app.constraints_ui.first_entity = 999;
        app.constraints_add();
        assert!(app.status_is_error);
        assert_eq!(app.document.entities.len(), 1);
        assert!(app.document.constraints.is_empty());
        assert!(app.undo_stack.is_empty());
        assert!(!app.dirty);
    }
}
