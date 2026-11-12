//! Desktop controls for transactional numeric editing and vector plotting.
use super::*;
use forge_core::plot::SvgOptions;

pub(super) struct DraftingState {
    pub(super) layouts: super::layout_ui::LayoutUiState,
    input: String,
    history: Vec<String>,
    history_cursor: Option<usize>,
    show_help: bool,
    offset: f64,
    columns: usize,
    rows: usize,
    spacing: [f64; 2],
    polar_count: usize,
    polar_sweep: f64,
    polar_rotate: bool,
    mirror_end: [f64; 2],
    target: u64,
    boundary: u64,
    pick: [f64; 2],
    plot: SvgOptions,
    plot_fit: bool,
    plot_scale: f64,
}

impl Default for DraftingState {
    fn default() -> Self {
        Self {
            layouts: super::layout_ui::LayoutUiState::default(),
            input: String::new(),
            history: Vec::new(),
            history_cursor: None,
            show_help: false,
            offset: 10.0,
            columns: 3,
            rows: 2,
            spacing: [100.0, 100.0],
            polar_count: 6,
            polar_sweep: 360.0,
            polar_rotate: true,
            mirror_end: [0.0, 100.0],
            target: 0,
            boundary: 0,
            pick: [0.0; 2],
            plot: SvgOptions::default(),
            plot_fit: true,
            plot_scale: 1.0,
        }
    }
}

impl ForgeApp {
    pub(super) fn command_bar(&mut self, root: &mut egui::Ui) {
        egui::Panel::bottom("draft_command_bar")
            .exact_size(64.0)
            .frame(egui::Frame::new().fill(PANEL).inner_margin(8.0))
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Command").color(ACCENT));
                    let input = ui.add(
                        egui::TextEdit::singleline(&mut self.drafting.input)
                            .id_salt("draft_command_input")
                            .desired_width((ui.available_width() - 160.0).max(150.0))
                            .hint_text("LINE 0 0 100 50   ·   SELECT ALL   ·   OFFSET 10"),
                    );
                    if input.has_focus() {
                        let up = ui.input(|i| i.key_pressed(Key::ArrowUp));
                        let down = ui.input(|i| i.key_pressed(Key::ArrowDown));
                        let len = self.drafting.history.len();
                        if up && len > 0 {
                            let index = self.drafting.history_cursor.unwrap_or(len).saturating_sub(1);
                            self.drafting.history_cursor = Some(index);
                            self.drafting.input = self.drafting.history[index].clone();
                        }
                        if down && let Some(index) = self.drafting.history_cursor {
                            if index + 1 < len {
                                self.drafting.history_cursor = Some(index + 1);
                                self.drafting.input = self.drafting.history[index + 1].clone();
                            } else {
                                self.drafting.history_cursor = None;
                                self.drafting.input.clear();
                            }
                        }
                    }
                    let enter = input.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                    if ui.button("Run").clicked() || enter {
                        let command = std::mem::take(&mut self.drafting.input);
                        self.run_draft_command(&command);
                        input.request_focus();
                    }
                    if ui.button("Help").clicked() { self.drafting.show_help = !self.drafting.show_help; }
                });
                ui.label(RichText::new("Exact model coordinates · ↑ / ↓ command history · edits use the current selection · HELP for syntax").size(10.0).color(MUTED));
            });
        if self.drafting.show_help {
            egui::Window::new("Drafting command reference")
                .open(&mut self.drafting.show_help)
                .resizable(true)
                .default_width(660.0)
                .show(root.ctx(), |ui| {
                    ui.label(egui::RichText::new(forge_core::command::HELP).monospace());
                    ui.separator();
                    ui.label("Desktop: UNDO · REDO · FIT · SAVE · NEW");
                    ui.label("Enter all arguments on one line. Angles are degrees. Rotations/scales default to the origin. Use SELECT ALL or space-separated object IDs before modifying objects.");
                    ui.label("Examples: RECTANGLE 0 0 120 80 → OFFSET -5 → ARRAYRECT 3 2 150 100");
                });
        }
    }

    pub(super) fn run_draft_command(&mut self, input: &str) {
        let input = input.trim();
        if input.is_empty() {
            return;
        }
        self.drafting.history.push(input.to_owned());
        if self.drafting.history.len() > 100 {
            self.drafting.history.remove(0);
        }
        self.drafting.history_cursor = None;
        match input.to_ascii_uppercase().as_str() {
            "UNDO" => {
                self.undo();
                return;
            }
            "REDO" => {
                self.redo();
                return;
            }
            "FIT" | "ZOOM EXTENTS" => {
                self.fit_requested = true;
                return;
            }
            "SAVE" => {
                self.save_document();
                return;
            }
            "NEW" => {
                self.request_action(PendingAction::New);
                return;
            }
            "HELP" | "?" => {
                self.drafting.show_help = true;
                self.status_is_error = false;
                return;
            }
            _ => {}
        }
        let before = self.document.clone();
        let ids: Vec<u64> = self.selected_entities.iter().copied().collect();
        match forge_core::command::execute(&mut self.document, &ids, input) {
            Ok(result) => {
                self.selected_entities = result.selection.into_iter().collect();
                if result.changed {
                    self.record_snapshot(before);
                    self.set_draft_tool(DraftTool::Select);
                    self.mark_dirty(result.message);
                } else {
                    self.status = result.message;
                    self.status_is_error = false;
                }
            }
            Err(error) => {
                self.status = error.to_string();
                self.status_is_error = true;
            }
        }
    }

    pub(super) fn advanced_selection_controls(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Offset, mirror & arrays", |ui| {
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut self.drafting.offset)
                        .speed(1.0)
                        .prefix("Distance "),
                );
                if ui.button("Offset").clicked() {
                    self.run_draft_command(&format!("OFFSET {}", self.drafting.offset));
                }
            });
            ui.label(
                RichText::new("Positive: left of lines, outside closed shapes.")
                    .size(10.0)
                    .color(MUTED),
            );
            ui.separator();
            ui.label("Mirror axis: pivot above → endpoint below");
            xy_controls(ui, "End", &mut self.drafting.mirror_end);
            if ui.button("Mirror selection").clicked() {
                self.run_draft_command(&format!(
                    "MIRROR {} {} {} {} {}",
                    self.selection_pivot[0],
                    self.selection_pivot[1],
                    self.drafting.mirror_end[0],
                    self.drafting.mirror_end[1],
                    if self.selection_copy { "COPY" } else { "" }
                ));
            }
            ui.separator();
            ui.label("Rectangular array (counts include originals)");
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut self.drafting.columns)
                        .range(1..=1000)
                        .prefix("Cols "),
                );
                ui.add(
                    egui::DragValue::new(&mut self.drafting.rows)
                        .range(1..=1000)
                        .prefix("Rows "),
                );
            });
            xy_controls(ui, "Step", &mut self.drafting.spacing);
            if ui.button("Create rectangular array").clicked() {
                self.run_draft_command(&format!(
                    "ARRAYRECT {} {} {} {}",
                    self.drafting.columns,
                    self.drafting.rows,
                    self.drafting.spacing[0],
                    self.drafting.spacing[1]
                ));
            }
            ui.separator();
            ui.label("Polar array around pivot above");
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut self.drafting.polar_count)
                        .range(1..=10000)
                        .prefix("Count "),
                );
                ui.add(
                    egui::DragValue::new(&mut self.drafting.polar_sweep)
                        .speed(5.0)
                        .suffix("°"),
                );
            });
            ui.checkbox(&mut self.drafting.polar_rotate, "Rotate objects with array");
            if ui.button("Create polar array").clicked() {
                self.run_draft_command(&format!(
                    "ARRAYPOLAR {} {} {} {} {}",
                    self.drafting.polar_count,
                    self.selection_pivot[0],
                    self.selection_pivot[1],
                    self.drafting.polar_sweep,
                    if self.drafting.polar_rotate {
                        ""
                    } else {
                        "NOROTATE"
                    }
                ));
            }
        });
        ui.collapsing("Trim / extend lines", |ui| {
            ui.label("Target and boundary are object IDs. Pick the side to remove (trim) or endpoint to move (extend).");
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut self.drafting.target).prefix("Line #"));
                if ui.small_button("Selected").clicked() && let Some(id) = self.selected_entities.first() { self.drafting.target = *id; }
            });
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut self.drafting.boundary).prefix("Boundary #"));
                if ui.small_button("Selected").clicked() && let Some(id) = self.selected_entities.first() { self.drafting.boundary = *id; }
            });
            xy_controls(ui, "Pick", &mut self.drafting.pick);
            ui.horizontal(|ui| {
                for name in ["TRIM", "EXTEND"] {
                    if ui.button(name).clicked() {
                        self.run_draft_command(&format!("{name} {} {} {} {}", self.drafting.target, self.drafting.boundary, self.drafting.pick[0], self.drafting.pick[1]));
                    }
                }
            });
        });
    }

    pub(super) fn plot_controls(&mut self, ui: &mut egui::Ui) {
        self.layout_controls(ui);
        ui.collapsing("Plot to SVG", |ui| {
            ui.label("Vector sheet · white paper · visible layers");
            ui.horizontal(|ui| {
                if ui.small_button("A4 landscape").clicked() {
                    self.drafting.plot.page_width_mm = 297.0;
                    self.drafting.plot.page_height_mm = 210.0;
                }
                if ui.small_button("A3").clicked() {
                    self.drafting.plot.page_width_mm = 420.0;
                    self.drafting.plot.page_height_mm = 297.0;
                }
            });
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut self.drafting.plot.page_width_mm)
                        .range(1.0..=10000.0)
                        .prefix("W "),
                );
                ui.add(
                    egui::DragValue::new(&mut self.drafting.plot.page_height_mm)
                        .range(1.0..=10000.0)
                        .prefix("H "),
                );
            });
            ui.add(
                egui::DragValue::new(&mut self.drafting.plot.margin_mm)
                    .range(0.0..=1000.0)
                    .suffix(" mm margin"),
            );
            ui.checkbox(&mut self.drafting.plot_fit, "Fit drawing to sheet");
            if !self.drafting.plot_fit {
                ui.add(
                    egui::DragValue::new(&mut self.drafting.plot_scale)
                        .speed(0.01)
                        .prefix("mm per model unit "),
                );
            }
            ui.checkbox(&mut self.drafting.plot.monochrome, "Monochrome");
            ui.label(
                RichText::new(
                    "SVG is a sheet for printing; save a native project for editable CAD data.",
                )
                .size(10.0)
                .color(MUTED),
            );
            if ui.button("Export vector sheet").clicked() {
                self.export_plot();
            }
        });
    }

    fn export_plot(&mut self) {
        let mut options = self.drafting.plot;
        options.scale = if self.drafting.plot_fit {
            None
        } else {
            Some(self.drafting.plot_scale)
        };
        match forge_core::plot::export_svg(&self.document, options) {
            Ok(svg) => {
                let path = export_path(&self.file_path, "svg");
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
                self.status = format!("Plot failed: {error}");
                self.status_is_error = true;
            }
        }
    }
}

fn xy_controls(ui: &mut egui::Ui, label: &str, point: &mut [f64; 2]) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::DragValue::new(&mut point[0]).speed(1.0).prefix("X "));
        ui.add(egui::DragValue::new(&mut point[1]).speed(1.0).prefix("Y "));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn numeric_edits_and_arrays_each_have_one_reversible_history_entry() {
        let mut app = ForgeApp::default();
        app.run_draft_command("RECTANGLE 0 0 120 80");
        app.run_draft_command("OFFSET -5");
        let before = app.document.clone();
        app.run_draft_command("ARRAYRECT 3 2 150 100");
        assert_eq!(app.document.entities.len(), 7);
        assert_eq!(app.selected_entities.len(), 5);
        assert_eq!(app.undo_stack.len(), 3);
        let after = app.document.clone();
        app.run_draft_command("UNDO");
        assert_eq!(app.document, before);
        app.run_draft_command("REDO");
        assert_eq!(app.document, after);
    }
    #[test]
    fn commands_respect_selection_and_errors_leave_history_untouched() {
        let mut app = ForgeApp::default();
        app.run_draft_command("CIRCLE 10 20 5");
        app.dirty = false;
        app.run_draft_command("SELECT ALL");
        assert!(!app.dirty);
        let doc = app.document.clone();
        let selection = app.selected_entities.clone();
        app.run_draft_command("OFFSET -10");
        assert!(app.status_is_error);
        assert!(!app.dirty);
        assert_eq!(app.document, doc);
        assert_eq!(app.selected_entities, selection);
        assert_eq!(app.undo_stack.len(), 1);
    }
    #[test]
    fn plot_exports_are_nonmutating_and_report_3d_content() {
        let mut app = ForgeApp::default();
        app.document.add_solid("Box", SolidKind::Box).unwrap();
        app.export_plot();
        assert!(app.status_is_error);
        assert!(app.pending_export.is_none());
        assert!(!app.dirty);
        assert!(app.undo_stack.is_empty());
    }

    #[test]
    fn selection_pivots_use_block_contents_instead_of_insertion_points() {
        let mut app = ForgeApp::default();
        app.run_draft_command("RECTANGLE 100 200 140 260");
        app.run_draft_command("BLOCK Offset detail");
        let bounds = app.selection_bounds().unwrap();
        assert_eq!(bounds.min, Point::new(100.0, 200.0));
        assert_eq!(bounds.max, Point::new(140.0, 260.0));
    }
}
