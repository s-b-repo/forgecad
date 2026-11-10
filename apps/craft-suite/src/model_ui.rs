//! Native extrusion authoring and a shared derived-mesh cache.
use super::*;
use forge_core::model::Extrusion;
use std::sync::Arc;
type PartMeshes = Arc<Vec<(u64, TriangleMesh)>>;
pub(super) struct ModelUiState {
    name: String,
    height: f64,
    base_z: f64,
    source: Option<Extrusion>,
    edit: Option<Extrusion>,
    pub(super) cache: Option<(Vec<Extrusion>, PartMeshes)>,
}
impl Default for ModelUiState {
    fn default() -> Self {
        Self {
            name: "Extrusion".into(),
            height: 25.0,
            base_z: 0.0,
            source: None,
            edit: None,
            cache: None,
        }
    }
}
impl ForgeApp {
    pub(super) fn cached_extrusions(&mut self) -> std::io::Result<PartMeshes> {
        if self
            .model_ui
            .cache
            .as_ref()
            .is_none_or(|(parts, _)| parts != &self.document.extrusions)
        {
            let meshes = self
                .document
                .extrusions
                .iter()
                .map(|part| part.mesh().map(|mesh| (part.id, mesh)))
                .collect::<std::io::Result<Vec<_>>>()?;
            self.model_ui.cache = Some((self.document.extrusions.clone(), Arc::new(meshes)));
        }
        self.model_ui
            .cache
            .as_ref()
            .map(|(_, meshes)| meshes.clone())
            .ok_or_else(|| std::io::Error::other("Extrusion mesh cache is unavailable"))
    }
    pub(super) fn extrusion_tools(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Profile extrusion", |ui| {
            ui.label("Copy selected Draft rectangles, circles or closed straight polylines.");
            ui.text_edit_singleline(&mut self.model_ui.name);
            ui.add(egui::DragValue::new(&mut self.model_ui.height).prefix("Height "));
            ui.add(egui::DragValue::new(&mut self.model_ui.base_z).prefix("Base Z "));
            if ui.button("Extrude selected Draft profiles").clicked() {
                self.extrude_draft_profiles();
            }
            if ui.button("Add extrusion example").clicked() {
                let mut candidate = self.document.clone();
                let result = candidate.add_extrusion(
                    "Extruded plate",
                    Geometry::Rectangle {
                        a: Point::new(-60.0, -40.0),
                        b: Point::new(60.0, 40.0),
                    },
                    0.0,
                    20.0,
                );
                match result {
                    Ok(id) => {
                        if self.commit_workbench_document(candidate, "Extrusion example created") {
                            self.selected_solid = Some(id);
                            self.selected_mesh = None;
                        }
                    }
                    Err(error) => {
                        self.status = error.to_string();
                        self.status_is_error = true;
                    }
                }
            }
        });
        for part in &self.document.extrusions {
            if ui
                .selectable_label(
                    self.selected_solid == Some(part.id),
                    format!("▣ {} · extrusion", part.name),
                )
                .clicked()
            {
                self.selected_solid = Some(part.id);
                self.selected_mesh = None;
            }
        }
    }
    fn extrude_draft_profiles(&mut self) {
        let profiles: Vec<_> = self
            .document
            .entities
            .iter()
            .filter(|e| self.selected_entities.contains(&e.id))
            .map(|e| e.geometry.clone())
            .collect();
        if profiles.is_empty() {
            self.status = "Select closed Draft profiles first".into();
            self.status_is_error = true;
            return;
        }
        let mut candidate = self.document.clone();
        let mut last = None;
        for profile in profiles {
            match candidate.add_extrusion(
                &self.model_ui.name,
                profile,
                self.model_ui.base_z,
                self.model_ui.height,
            ) {
                Ok(id) => last = Some(id),
                Err(error) => {
                    self.status = error.to_string();
                    self.status_is_error = true;
                    return;
                }
            }
        }
        if self.commit_workbench_document(
            candidate,
            "Draft profiles copied into editable extrusion features",
        ) {
            self.selected_solid = last;
            self.selected_mesh = None;
        }
    }
    pub(super) fn extrusion_inspector(&mut self, ui: &mut egui::Ui) {
        let source = self
            .selected_solid
            .and_then(|id| self.document.extrusions.iter().find(|p| p.id == id))
            .cloned();
        if source != self.model_ui.source {
            self.model_ui.edit = source.clone();
            self.model_ui.source = source;
        }
        let Some(part) = &mut self.model_ui.edit else {
            return;
        };
        ui.label(format!("Editable extrusion #{}", part.id));
        ui.text_edit_singleline(&mut part.name);
        ui.add(egui::DragValue::new(&mut part.height).prefix("Height "));
        ui.add(egui::DragValue::new(&mut part.base_z).prefix("Base Z "));
        ui.label("Profile XY coordinates · project units");
        match &mut part.profile {
            Geometry::Rectangle { a, b } => {
                point_controls(ui, "Corner A", a);
                point_controls(ui, "Corner B", b);
            }
            Geometry::Circle { center, radius } => {
                point_controls(ui, "Center", center);
                ui.add(egui::DragValue::new(radius).prefix("Radius "));
            }
            Geometry::Polyline { vertices, .. } => {
                ui.collapsing("Profile vertices", |ui| {
                    for (index, v) in vertices.iter_mut().enumerate() {
                        ui.push_id(index, |ui| {
                            point_controls(ui, &format!("Point {}", index + 1), &mut v.point)
                        });
                    }
                });
            }
            _ => {}
        }
        if ui.button("Apply extrusion parameters").clicked() {
            self.apply_extrusion();
        }
        if let Some(source) = &self.model_ui.source {
            match source.mesh().and_then(|mesh| {
                mesh.signed_volume()
                    .map(|volume| (mesh.triangles.len(), volume))
            }) {
                Ok((triangles, volume)) => {
                    ui.label(format!(
                        "{triangles} triangles · volume {volume:.6} model units³"
                    ));
                }
                Err(error) => {
                    ui.colored_label(
                        Color32::LIGHT_RED,
                        format!("Could not evaluate extrusion: {error}"),
                    );
                }
            }
        }
        ui.label(RichText::new("Native Save retains the profile and height. Mesh export produces derived triangles.").size(9.0).color(MUTED));
        if ui.button("Delete extrusion").clicked() {
            let mut candidate = self.document.clone();
            if let Some(id) = self.selected_solid {
                candidate.extrusions.retain(|p| p.id != id);
            }
            if self.commit_workbench_document(candidate, "Extrusion deleted") {
                self.selected_solid = None;
            }
        }
    }
    fn apply_extrusion(&mut self) {
        let Some(part) = self.model_ui.edit.clone() else {
            return;
        };
        let mut candidate = self.document.clone();
        let Some(saved) = candidate.extrusions.iter_mut().find(|p| p.id == part.id) else {
            return;
        };
        *saved = part;
        self.commit_workbench_document(candidate, "Extrusion parameters updated");
    }
}
fn point_controls(ui: &mut egui::Ui, label: &str, p: &mut Point) {
    ui.label(label);
    ui.horizontal(|ui| {
        ui.add(egui::DragValue::new(&mut p.x).prefix("X "));
        ui.add(egui::DragValue::new(&mut p.y).prefix("Y "));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extrusion_edits_are_reversible_and_invalid_profiles_preserve_history() {
        let mut app = ForgeApp::starting_at(Workspace::Model);
        let entity = app.document.add_geometry(Geometry::Rectangle {
            a: Point::default(),
            b: Point::new(8.0, 4.0),
        });
        app.selected_entities.insert(entity);
        app.model_ui.height = 3.0;
        app.extrude_draft_profiles();
        assert_eq!(app.document.extrusions.len(), 1);
        assert_eq!(app.undo_stack.len(), 1);
        let before = app.document.clone();
        let id = before.extrusions[0].id;
        app.model_ui.edit = Some(before.extrusions[0].clone());
        app.model_ui.edit.as_mut().unwrap().height = 0.0;
        app.apply_extrusion();
        assert!(app.status_is_error);
        assert_eq!(app.document, before);
        assert_eq!(app.undo_stack.len(), 1);
        app.model_ui.edit.as_mut().unwrap().height = 5.0;
        app.apply_extrusion();
        assert!(!app.status_is_error);
        assert_eq!(app.undo_stack.len(), 2);
        let cached = app.cached_extrusions().unwrap();
        assert!((cached[0].1.signed_volume().unwrap() - 160.0).abs() < 1e-8);
        app.undo();
        assert_eq!(app.document, before);
        assert_eq!(app.document.extrusions[0].id, id);
        app.selected_solid = Some(id);
        assert_eq!(app.document_workspace(), Workspace::Model);
    }
}
