//! Saved paper sheets, viewport editing and derived SVG export.
use super::*;
use forge_core::plot::{PaperText, PrintLayout, SvgOptions, export_layout_svg};

#[derive(Default)]
pub(super) struct LayoutUiState {
    selected: Option<String>,
    source: Option<PrintLayout>,
    edit: Option<PrintLayout>,
    new_name: String,
    viewport: usize,
}

impl ForgeApp {
    pub(super) fn layout_controls(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Saved print layouts", |ui| {
            ui.label("Named sheets, clipped viewports and paper notes");
            ui.add(egui::TextEdit::singleline(&mut self.drafting.layouts.new_name).hint_text("New sheet name"));
            if ui.button("Create fitted A4 layout").clicked() {
                self.create_print_layout();
            }
            let names: Vec<_> = self.document.layouts.iter().map(|l| l.name.clone()).collect();
            for name in names {
                if ui.selectable_label(self.drafting.layouts.selected.as_ref() == Some(&name), &name).clicked() {
                    self.drafting.layouts.selected = Some(name);
                    self.drafting.layouts.viewport = 0;
                }
            }
            let source = self.drafting.layouts.selected.as_ref().and_then(|name| self.document.layouts.iter().find(|l| &l.name == name)).cloned();
            if source != self.drafting.layouts.source {
                self.drafting.layouts.edit = source.clone();
                self.drafting.layouts.source = source;
            }
            let state = &mut self.drafting.layouts;
            let Some(layout) = &mut state.edit else { return; };
            ui.separator();
            ui.text_edit_singleline(&mut layout.name);
            ui.add(egui::DragValue::new(&mut layout.options.page_width_mm).prefix("Page width mm "));
            ui.add(egui::DragValue::new(&mut layout.options.page_height_mm).prefix("Page height mm "));
            ui.add(egui::DragValue::new(&mut layout.options.margin_mm).prefix("Margin mm "));
            ui.checkbox(&mut layout.options.monochrome, "Monochrome sheet");
            ui.horizontal(|ui| {
                if ui.button("Add viewport").clicked() {
                    match PrintLayout::fitted("Template", &self.document, layout.options) {
                        Ok(mut fitted) => {
                        let mut viewport = fitted.viewports.remove(0);
                        let mut number = layout.viewports.len()+1;
                        while layout.viewports.iter().any(|v| v.name == format!("Detail {number}")) { number += 1; }
                        viewport.name = format!("Detail {number}");
                        layout.viewports.push(viewport);
                        state.viewport = layout.viewports.len()-1;
                        }
                        Err(error) => { self.status = error.to_string(); self.status_is_error = true; }
                    }
                }
                if ui.button("Remove viewport").clicked() && state.viewport < layout.viewports.len() {
                    layout.viewports.remove(state.viewport);
                    state.viewport = 0;
                }
            });
            egui::ComboBox::from_id_salt("sheet_viewport")
                .selected_text(layout.viewports.get(state.viewport).map(|v| v.name.as_str()).unwrap_or("No viewport"))
                .show_ui(ui, |ui| {
                    for (index, viewport) in layout.viewports.iter().enumerate() {
                        ui.selectable_value(&mut state.viewport, index, &viewport.name);
                    }
                });
            if let Some(viewport) = layout.viewports.get_mut(state.viewport) {
                ui.text_edit_singleline(&mut viewport.name);
                ui.label("Frame: mm from sheet top left");
                layout_point(ui, "Paper", &mut viewport.paper_position);
                ui.add(egui::DragValue::new(&mut viewport.paper_size[0]).prefix("Width mm "));
                ui.add(egui::DragValue::new(&mut viewport.paper_size[1]).prefix("Height mm "));
                layout_point(ui, "Model center", &mut viewport.model_center);
                ui.add(egui::DragValue::new(&mut viewport.scale).speed(0.01).prefix("mm/model unit "));
                ui.add(egui::DragValue::new(&mut viewport.rotation_deg).speed(0.5).prefix("View rotation ° CW "));
                ui.checkbox(&mut viewport.border, "Viewport border");
                ui.collapsing("Hide layers in this viewport", |ui| {
                    for layer in &self.document.layers {
                        let mut hidden = viewport.hidden_layer_ids.contains(&layer.id);
                        if ui.checkbox(&mut hidden, &layer.name).changed() {
                            viewport.hidden_layer_ids.retain(|id| *id != layer.id);
                            if hidden { viewport.hidden_layer_ids.push(layer.id); }
                        }
                    }
                });
            }
            ui.separator();
            ui.label("Paper notes");
            let mut remove = None;
            for (index,note) in layout.notes.iter_mut().enumerate() {
                ui.push_id(index, |ui| {
                    ui.text_edit_singleline(&mut note.text);
                    layout_point(ui, "Position mm", &mut note.position);
                    ui.add(egui::DragValue::new(&mut note.height_mm).speed(0.1).prefix("Text height mm "));
                    if ui.small_button("Remove note").clicked() { remove = Some(index); }
                });
            }
            if let Some(index) = remove { layout.notes.remove(index); }
            if ui.button("Add paper note").clicked() {
                layout.notes.push(PaperText { position: Point::new(10.0,10.0),text: "Sheet note".into(),height_mm: 3.5 });
            }
            ui.label(RichText::new("Apply stores these settings in the project and adds one undo step. Viewport frames intentionally clip the drawing.").size(9.0).color(MUTED));
            if ui.button("Apply layout changes").clicked() { self.apply_print_layout(); }
            if ui.button("Export saved layout to SVG").clicked() { self.export_print_layout(); }
            if ui.button("Delete saved layout").clicked() {
                let selected = self.drafting.layouts.selected.clone();
                let mut document = self.document.clone();
                document.layouts.retain(|l| Some(&l.name) != selected.as_ref());
                if self.commit_workbench_document(document, "Print layout deleted") { self.drafting.layouts.selected = None; }
            }
        });
    }

    fn create_print_layout(&mut self) {
        let name = self.drafting.layouts.new_name.trim();
        match PrintLayout::fitted(name, &self.document, SvgOptions::default()) {
            Ok(layout) => {
                let mut document = self.document.clone();
                let selected = layout.name.clone();
                document.layouts.push(layout);
                if self.commit_workbench_document(document, "Print layout created") {
                    self.drafting.layouts.selected = Some(selected);
                    self.drafting.layouts.new_name.clear();
                }
            }
            Err(error) => {
                self.status = error.to_string();
                self.status_is_error = true;
            }
        }
    }

    fn apply_print_layout(&mut self) {
        let Some(layout) = self.drafting.layouts.edit.clone() else {
            return;
        };
        let mut document = self.document.clone();
        let Some(saved) = document
            .layouts
            .iter_mut()
            .find(|l| Some(&l.name) == self.drafting.layouts.selected.as_ref())
        else {
            return;
        };
        let name = layout.name.clone();
        *saved = layout;
        if self.commit_workbench_document(document, "Print layout updated") {
            self.drafting.layouts.selected = Some(name);
        }
    }

    fn export_print_layout(&mut self) {
        let Some(name) = self.drafting.layouts.selected.as_deref() else {
            return;
        };
        match export_layout_svg(&self.document, name) {
            Ok(svg) => {
                let base = export_path(&self.file_path, "svg");
                let stem = base
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("design");
                let path = base.with_file_name(format!("{stem}-sheet.svg"));
                let replace_existing = std::fs::symlink_metadata(&path).is_ok();
                let job = PreparedExport {
                    path,
                    bytes: svg.into_bytes(),
                    replace_existing,
                    losses: Vec::new(),
                };
                if replace_existing {
                    self.pending_export = Some(job);
                } else {
                    self.publish_export(job);
                }
            }
            Err(error) => {
                self.status = error.to_string();
                self.status_is_error = true;
            }
        }
    }
}

fn layout_point(ui: &mut egui::Ui, label: &str, point: &mut Point) {
    ui.label(label);
    ui.horizontal(|ui| {
        ui.add(egui::DragValue::new(&mut point.x).prefix("X "));
        ui.add(egui::DragValue::new(&mut point.y).prefix("Y "));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn layout_creation_and_edits_are_undoable_and_invalid_frames_preserve_history() {
        let mut app = ForgeApp::default();
        app.document
            .add_geometry(Geometry::Rectangle {
                a: Point::default(),
                b: Point::new(100.0, 100.0),
            })
            .unwrap();
        app.drafting.layouts.new_name = "Plan".into();
        app.create_print_layout();
        assert_eq!(app.document.layouts.len(), 1);
        assert_eq!(app.undo_stack.len(), 1);
        let before = app.document.clone();
        let mut layout = before.layouts[0].clone();
        layout.viewports[0].scale = 0.0;
        app.drafting.layouts.edit = Some(layout);
        app.apply_print_layout();
        assert!(app.status_is_error);
        assert_eq!(app.document, before);
        assert_eq!(app.undo_stack.len(), 1);
        let mut layout = before.layouts[0].clone();
        layout.name = "Revised plan".into();
        layout.viewports[0].scale = 2.0;
        app.drafting.layouts.edit = Some(layout);
        app.apply_print_layout();
        assert!(!app.status_is_error);
        assert_eq!(app.undo_stack.len(), 2);
        let saved = Document::from_json(&app.document.to_json().unwrap()).unwrap();
        assert_eq!(saved, app.document);
        app.undo();
        assert_eq!(app.document, before);
        app.redo();
        assert_eq!(app.document, saved);
    }
}
