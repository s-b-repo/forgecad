//! Author and edit local semantic building elements.
use super::*;
use forge_core::building::{Building, Opening, OpeningKind, Slab, Storey, Wall};

#[derive(Default)]
pub(super) struct BuildingUiState {
    pub(super) selected: Option<u64>,
    source: Option<ElementDraft>,
    draft: Option<ElementDraft>,
}

#[derive(Clone, PartialEq)]
enum ElementDraft {
    Storey(Storey),
    Wall(Wall),
    Slab(Slab),
    Opening(Opening),
}

impl ForgeApp {
    pub(super) fn building_tools(&mut self, ui: &mut egui::Ui) {
        section_header(ui, "BUILDING", "Storeys & hosted elements");
        ui.add_space(8.0);
        if self.document.building.is_empty() {
            ui.label("Create a storey, wall or slab to start a local building model.");
        }
        for storey in self.document.building.storeys.clone() {
            if ui
                .selectable_label(
                    self.building_ui.selected == Some(storey.id),
                    format!("▦ {}", storey.name),
                )
                .clicked()
            {
                self.building_ui.selected = Some(storey.id);
            }
            ui.indent(storey.id, |ui| {
                for wall in self
                    .document
                    .building
                    .walls
                    .iter()
                    .filter(|w| w.storey_id == storey.id)
                {
                    if ui
                        .selectable_label(
                            self.building_ui.selected == Some(wall.id),
                            format!("Wall · {}", wall.name),
                        )
                        .clicked()
                    {
                        self.building_ui.selected = Some(wall.id);
                    }
                    ui.indent(wall.id, |ui| {
                        for opening in self
                            .document
                            .building
                            .openings
                            .iter()
                            .filter(|o| o.wall_id == wall.id)
                        {
                            let kind = match opening.kind {
                                OpeningKind::Door => "Door",
                                OpeningKind::Window => "Window",
                            };
                            if ui
                                .selectable_label(
                                    self.building_ui.selected == Some(opening.id),
                                    format!("{kind} · {}", opening.name),
                                )
                                .clicked()
                            {
                                self.building_ui.selected = Some(opening.id);
                            }
                        }
                    });
                }
                for slab in self
                    .document
                    .building
                    .slabs
                    .iter()
                    .filter(|s| s.storey_id == storey.id)
                {
                    if ui
                        .selectable_label(
                            self.building_ui.selected == Some(slab.id),
                            format!("Slab · {}", slab.name),
                        )
                        .clicked()
                    {
                        self.building_ui.selected = Some(slab.id);
                    }
                }
            });
        }
        ui.add_space(12.0);
        ui.separator();
        section_header(ui, "AUTHOR", "Validated semantic elements");
        if ui.button("＋ Storey").clicked() {
            self.add_building_storey();
        }
        if ui.button("＋ Wall").clicked() {
            self.add_building_wall();
        }
        if ui.button("＋ Slab").clicked() {
            self.add_building_slab();
        }
        if ui.button("＋ Door opening").clicked() {
            self.add_building_opening(OpeningKind::Door);
        }
        if ui.button("＋ Window opening").clicked() {
            self.add_building_opening(OpeningKind::Window);
        }
        ui.label(RichText::new("New walls/slabs use the selected storey. Openings use the selected wall or its opening's host.").size(10.0).color(MUTED));
        ui.add_space(10.0);
        if ui
            .button(if self.document.building.is_empty() {
                "Load example building"
            } else {
                "Replace building with example"
            })
            .clicked()
        {
            let mut candidate = self.document.clone();
            candidate.building = Building::example();
            if self.commit_workbench_document(candidate, "Example building loaded") {
                self.building_ui.selected = Some(2);
            }
        }
        ui.label(
            RichText::new(
                "Default dimensions and example use millimetres. Edit all sizes in the inspector.",
            )
            .size(10.0)
            .color(MUTED),
        );
        ui.add_space(12.0);
        ui.label("Right-drag to orbit\nMiddle-drag to pan\nScroll to zoom");
    }

    pub(super) fn building_inspector(&mut self, ui: &mut egui::Ui) {
        section_header(ui, "ELEMENT", "Edit then Apply");
        let original = self
            .building_ui
            .selected
            .and_then(|id| find_element(&self.document.building, id));
        if original != self.building_ui.source {
            self.building_ui.source = original.clone();
            self.building_ui.draft = original;
        }
        let building = &self.document.building;
        if let Some(draft) = &mut self.building_ui.draft {
            match draft {
                ElementDraft::Storey(value) => {
                    ui.label(format!("Storey #{}", value.id));
                    name_control(ui, &mut value.name);
                    number_control(ui, "Elevation", &mut value.elevation);
                    ui.label("Wall bases and slab tops follow this elevation.");
                }
                ElementDraft::Wall(value) => {
                    ui.label(format!("Wall #{}", value.id));
                    name_control(ui, &mut value.name);
                    storey_control(ui, &building.storeys, &mut value.storey_id);
                    point_control(ui, "Start", &mut value.start);
                    point_control(ui, "End", &mut value.end);
                    number_control(ui, "Thickness", &mut value.thickness);
                    number_control(ui, "Height", &mut value.height);
                    let length = value.start.distance(value.end);
                    ui.label(format!("Length: {length:.3}"));
                    let cut: f64 = building
                        .openings
                        .iter()
                        .filter(|o| o.wall_id == value.id)
                        .map(|o| o.width * o.height)
                        .sum();
                    ui.label(format!("Net face area: {:.3}", length * value.height - cut));
                    ui.label(format!(
                        "Net volume: {:.3}",
                        (length * value.height - cut) * value.thickness
                    ));
                    ui.label(
                        RichText::new("Shortening a wall past an opening is refused.")
                            .size(10.0)
                            .color(MUTED),
                    );
                }
                ElementDraft::Slab(value) => {
                    ui.label(format!("Slab #{}", value.id));
                    name_control(ui, &mut value.name);
                    storey_control(ui, &building.storeys, &mut value.storey_id);
                    point_control(ui, "Min", &mut value.min);
                    point_control(ui, "Max", &mut value.max);
                    number_control(ui, "Thickness", &mut value.thickness);
                    let area = (value.max.x - value.min.x) * (value.max.y - value.min.y);
                    ui.label(format!("Plan area: {area:.3}"));
                    ui.label(format!("Volume: {:.3}", area * value.thickness));
                }
                ElementDraft::Opening(value) => {
                    ui.label(format!("Opening #{}", value.id));
                    name_control(ui, &mut value.name);
                    egui::ComboBox::from_id_salt("opening_host")
                        .selected_text(
                            building
                                .walls
                                .iter()
                                .find(|w| w.id == value.wall_id)
                                .map(|w| w.name.as_str())
                                .unwrap_or("Missing wall"),
                        )
                        .show_ui(ui, |ui| {
                            for wall in &building.walls {
                                ui.selectable_value(
                                    &mut value.wall_id,
                                    wall.id,
                                    format!("{} #{}", wall.name, wall.id),
                                );
                            }
                        });
                    let old = value.kind;
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut value.kind, OpeningKind::Door, "Door");
                        ui.selectable_value(&mut value.kind, OpeningKind::Window, "Window");
                    });
                    if old != value.kind && value.kind == OpeningKind::Door {
                        value.sill = 0.0;
                    }
                    number_control(ui, "Offset along wall", &mut value.offset);
                    number_control(ui, "Width", &mut value.width);
                    number_control(ui, "Sill above floor", &mut value.sill);
                    number_control(ui, "Height", &mut value.height);
                    ui.label(format!("Opening area: {:.3}", value.width * value.height));
                    ui.label(RichText::new("Openings remove the full wall thickness. Bounds and overlapping voids are checked on Apply.").size(10.0).color(MUTED));
                }
            }
            ui.add_space(9.0);
            ui.horizontal(|ui| {
                if ui.button("Apply element").clicked() {
                    self.apply_building_draft();
                }
                if ui.button("Reset").clicked() {
                    self.building_ui.draft = self.building_ui.source.clone();
                }
            });
            if ui.button("Delete element and dependents").clicked() {
                self.delete_building_selection();
            }
        } else {
            ui.label(
                "Select an element in the building tree, or add one using the authoring tools.",
            );
        }
        ui.add_space(12.0);
        ui.separator();
        section_header(ui, "QUANTITIES", "Current applied model");
        let building = &self.document.building;
        ui.label(format!(
            "{} storeys · {} walls · {} slabs",
            building.storeys.len(),
            building.walls.len(),
            building.slabs.len()
        ));
        ui.label(format!("{} door/window openings", building.openings.len()));
        let wall_volume: f64 = building
            .walls
            .iter()
            .map(|w| {
                let cut: f64 = building
                    .openings
                    .iter()
                    .filter(|o| o.wall_id == w.id)
                    .map(|o| o.width * o.height)
                    .sum();
                (w.start.distance(w.end) * w.height - cut) * w.thickness
            })
            .sum();
        let slab_volume: f64 = building
            .slabs
            .iter()
            .map(|s| (s.max.x - s.min.x) * (s.max.y - s.min.y) * s.thickness)
            .sum();
        ui.label(format!("Net wall volume: {wall_volume:.3}"));
        ui.label(format!("Slab volume: {slab_volume:.3}"));
        ui.label(RichText::new("Areas are model units²; volumes are model units³. Intersections between separate walls/slabs are not unioned for takeoff.").size(10.0).color(MUTED));
    }

    fn apply_building_draft(&mut self) {
        let Some(draft) = self.building_ui.draft.clone() else {
            return;
        };
        let mut candidate = self.document.clone();
        match draft {
            ElementDraft::Storey(value) => {
                if let Some(target) = candidate
                    .building
                    .storeys
                    .iter_mut()
                    .find(|s| s.id == value.id)
                {
                    *target = value;
                }
            }
            ElementDraft::Wall(value) => {
                if let Some(target) = candidate
                    .building
                    .walls
                    .iter_mut()
                    .find(|s| s.id == value.id)
                {
                    *target = value;
                }
            }
            ElementDraft::Slab(value) => {
                if let Some(target) = candidate
                    .building
                    .slabs
                    .iter_mut()
                    .find(|s| s.id == value.id)
                {
                    *target = value;
                }
            }
            ElementDraft::Opening(value) => {
                if let Some(target) = candidate
                    .building
                    .openings
                    .iter_mut()
                    .find(|s| s.id == value.id)
                {
                    *target = value;
                }
            }
        }
        self.commit_workbench_document(candidate, "Building element updated");
    }

    fn add_building_storey(&mut self) {
        let mut candidate = self.document.clone();
        let Ok(id) = next_id(&candidate.building) else {
            self.building_error("Building element IDs are exhausted");
            return;
        };
        let elevation = candidate
            .building
            .storeys
            .iter()
            .map(|s| s.elevation)
            .max_by(f64::total_cmp)
            .map_or(0.0, |z| z + 3000.0);
        candidate.building.storeys.push(Storey {
            id,
            name: format!("Storey {}", candidate.building.storeys.len() + 1),
            elevation,
        });
        if self.commit_workbench_document(candidate, "Storey added") {
            self.building_ui.selected = Some(id);
        }
    }

    fn add_building_wall(&mut self) {
        let mut candidate = self.document.clone();
        let storey_id = match ensure_storey(&mut candidate.building, self.building_ui.selected) {
            Ok(id) => id,
            Err(error) => {
                self.building_error(error);
                return;
            }
        };
        let Ok(id) = next_id(&candidate.building) else {
            self.building_error("Building element IDs are exhausted");
            return;
        };
        let offset = candidate.building.walls.len() as f64 * 1000.0;
        candidate.building.walls.push(Wall {
            id,
            name: format!("Wall {}", candidate.building.walls.len() + 1),
            storey_id,
            start: Point::new(0.0, offset),
            end: Point::new(4000.0, offset),
            thickness: 200.0,
            height: 2800.0,
        });
        if self.commit_workbench_document(candidate, "Wall added") {
            self.building_ui.selected = Some(id);
        }
    }

    fn add_building_slab(&mut self) {
        let mut candidate = self.document.clone();
        let storey_id = match ensure_storey(&mut candidate.building, self.building_ui.selected) {
            Ok(id) => id,
            Err(error) => {
                self.building_error(error);
                return;
            }
        };
        let Ok(id) = next_id(&candidate.building) else {
            self.building_error("Building element IDs are exhausted");
            return;
        };
        candidate.building.slabs.push(Slab {
            id,
            name: format!("Slab {}", candidate.building.slabs.len() + 1),
            storey_id,
            min: Point::new(0.0, 0.0),
            max: Point::new(6000.0, 4000.0),
            thickness: 200.0,
        });
        if self.commit_workbench_document(candidate, "Slab added") {
            self.building_ui.selected = Some(id);
        }
    }

    fn add_building_opening(&mut self, kind: OpeningKind) {
        let mut candidate = self.document.clone();
        let host = self
            .building_ui
            .selected
            .and_then(|id| {
                candidate
                    .building
                    .walls
                    .iter()
                    .find(|w| w.id == id)
                    .or_else(|| {
                        candidate
                            .building
                            .openings
                            .iter()
                            .find(|o| o.id == id)
                            .and_then(|o| {
                                candidate.building.walls.iter().find(|w| w.id == o.wall_id)
                            })
                    })
            })
            .or_else(|| candidate.building.walls.first())
            .cloned();
        let Some(wall) = host else {
            self.building_error("Add or select a host wall before adding an opening");
            return;
        };
        let Ok(id) = next_id(&candidate.building) else {
            self.building_error("Building element IDs are exhausted");
            return;
        };
        let length = wall.start.distance(wall.end);
        let width = (if kind == OpeningKind::Door {
            900.0_f64
        } else {
            1200.0_f64
        })
        .min(length * 0.25);
        let margin = (length * 0.05).min(200.0);
        let mut offset = margin;
        let mut existing: Vec<_> = candidate
            .building
            .openings
            .iter()
            .filter(|o| o.wall_id == wall.id)
            .collect();
        existing.sort_by(|a, b| a.offset.total_cmp(&b.offset));
        for opening in existing {
            if offset + width <= opening.offset {
                break;
            }
            offset = offset.max(opening.offset + opening.width + margin);
        }
        if offset + width > length {
            self.building_error("No clear span remains for this opening; edit existing openings or choose another wall");
            return;
        }
        let (sill, height) = if kind == OpeningKind::Door {
            (0.0, 2100.0_f64.min(wall.height * 0.8))
        } else {
            (wall.height * 0.3, 1200.0_f64.min(wall.height * 0.4))
        };
        let label = match kind {
            OpeningKind::Door => "Door",
            OpeningKind::Window => "Window",
        };
        candidate.building.openings.push(Opening {
            id,
            name: format!("{label} {}", candidate.building.openings.len() + 1),
            wall_id: wall.id,
            kind,
            offset,
            width,
            sill,
            height,
        });
        if self.commit_workbench_document(candidate, format!("{label} opening added")) {
            self.building_ui.selected = Some(id);
        }
    }

    fn delete_building_selection(&mut self) {
        let Some(id) = self.building_ui.selected else {
            return;
        };
        let mut candidate = self.document.clone();
        let building = &mut candidate.building;
        let removed_walls: BTreeSet<_> = building
            .walls
            .iter()
            .filter(|w| w.id == id || w.storey_id == id)
            .map(|w| w.id)
            .collect();
        building
            .openings
            .retain(|o| o.id != id && !removed_walls.contains(&o.wall_id));
        building.walls.retain(|w| !removed_walls.contains(&w.id));
        building.slabs.retain(|s| s.id != id && s.storey_id != id);
        building.storeys.retain(|s| s.id != id);
        if self.commit_workbench_document(candidate, "Building element and dependents deleted") {
            self.building_ui.selected = None;
        }
    }

    fn building_error(&mut self, message: impl ToString) {
        self.status = message.to_string();
        self.status_is_error = true;
    }
}

fn next_id(building: &Building) -> std::io::Result<u64> {
    building
        .storeys
        .iter()
        .map(|s| s.id)
        .chain(building.walls.iter().map(|s| s.id))
        .chain(building.slabs.iter().map(|s| s.id))
        .chain(building.openings.iter().map(|s| s.id))
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| std::io::Error::other("Building element IDs are exhausted"))
}

fn ensure_storey(building: &mut Building, selected: Option<u64>) -> std::io::Result<u64> {
    if let Some(id) = selected {
        if building.storeys.iter().any(|s| s.id == id) {
            return Ok(id);
        }
        if let Some(wall) = building.walls.iter().find(|w| w.id == id) {
            return Ok(wall.storey_id);
        }
        if let Some(slab) = building.slabs.iter().find(|s| s.id == id) {
            return Ok(slab.storey_id);
        }
        if let Some(opening) = building.openings.iter().find(|s| s.id == id)
            && let Some(wall) = building.walls.iter().find(|w| w.id == opening.wall_id)
        {
            return Ok(wall.storey_id);
        }
    }
    if let Some(storey) = building.storeys.first() {
        return Ok(storey.id);
    }
    let id = next_id(building)?;
    building.storeys.push(Storey {
        id,
        name: "Ground floor".into(),
        elevation: 0.0,
    });
    Ok(id)
}

fn find_element(building: &Building, id: u64) -> Option<ElementDraft> {
    building
        .storeys
        .iter()
        .find(|s| s.id == id)
        .cloned()
        .map(ElementDraft::Storey)
        .or_else(|| {
            building
                .walls
                .iter()
                .find(|s| s.id == id)
                .cloned()
                .map(ElementDraft::Wall)
        })
        .or_else(|| {
            building
                .slabs
                .iter()
                .find(|s| s.id == id)
                .cloned()
                .map(ElementDraft::Slab)
        })
        .or_else(|| {
            building
                .openings
                .iter()
                .find(|s| s.id == id)
                .cloned()
                .map(ElementDraft::Opening)
        })
}

fn name_control(ui: &mut egui::Ui, name: &mut String) {
    ui.label("Name");
    ui.add(egui::TextEdit::singleline(name).desired_width(f32::INFINITY));
}
fn number_control(ui: &mut egui::Ui, label: &str, value: &mut f64) {
    ui.label(label);
    ui.add(egui::DragValue::new(value).speed(10.0).max_decimals(6));
}
fn point_control(ui: &mut egui::Ui, label: &str, point: &mut Point) {
    ui.label(label);
    ui.horizontal(|ui| {
        ui.add(egui::DragValue::new(&mut point.x).speed(10.0).prefix("X "));
        ui.add(egui::DragValue::new(&mut point.y).speed(10.0).prefix("Y "));
    });
}
fn storey_control(ui: &mut egui::Ui, storeys: &[Storey], id: &mut u64) {
    ui.label("Storey");
    egui::ComboBox::from_id_salt("element_storey")
        .selected_text(
            storeys
                .iter()
                .find(|s| s.id == *id)
                .map(|s| s.name.as_str())
                .unwrap_or("Missing storey"),
        )
        .show_ui(ui, |ui| {
            for storey in storeys {
                ui.selectable_value(id, storey.id, format!("{} #{}", storey.name, storey.id));
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn building_authoring_creates_valid_references_and_undoable_openings() {
        let mut app = ForgeApp::starting_at(Workspace::Build);
        app.add_building_wall();
        assert_eq!(app.document.building.storeys.len(), 1);
        assert_eq!(app.document.building.walls.len(), 1);
        app.add_building_opening(OpeningKind::Door);
        let before = app.document.clone();
        app.add_building_opening(OpeningKind::Window);
        assert_eq!(app.document.building.openings.len(), 2);
        assert_eq!(app.undo_stack.len(), 3);
        app.document.building.meshes().unwrap();
        app.undo();
        assert_eq!(app.document, before);
    }
    #[test]
    fn invalid_wall_edits_leave_applied_building_and_history_unchanged() {
        let mut app = ForgeApp::starting_at(Workspace::Build);
        app.add_building_wall();
        app.add_building_opening(OpeningKind::Door);
        let before = app.document.clone();
        let mut wall = app.document.building.walls[0].clone();
        wall.end.x = 100.0;
        app.building_ui.draft = Some(ElementDraft::Wall(wall));
        app.apply_building_draft();
        assert!(app.status_is_error);
        assert_eq!(app.document, before);
        assert_eq!(app.undo_stack.len(), 2);
    }
    #[test]
    fn deleting_storey_cascades_hosted_elements_in_one_history_entry() {
        let mut app = ForgeApp::starting_at(Workspace::Build);
        app.document.building = Building::example();
        let before = app.document.clone();
        app.building_ui.selected = Some(1);
        app.delete_building_selection();
        assert!(app.document.building.is_empty());
        assert_eq!(app.undo_stack.len(), 1);
        app.undo();
        assert_eq!(app.document, before);
    }
}
