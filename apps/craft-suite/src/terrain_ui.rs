//! Survey authoring and derived civil surface queries.
use super::*;
use forge_core::terrain::{ProfileSample, Terrain};

pub(super) struct TerrainUiState {
    pub(super) civil: super::civil_ui::CivilUiState,
    pub(super) contour_interval: f64,
    pub(super) show_contours: bool,
    selected: Option<usize>,
    edit_point: [f64; 3],
    new_point: [f64; 3],
    query: [f64; 2],
    query_result: Option<Option<f64>>,
    pub(super) profile_start: [f64; 2],
    pub(super) profile_end: [f64; 2],
    profile_samples: usize,
    profile: Vec<ProfileSample>,
    analysis_source: Option<Terrain>,
}

impl Default for TerrainUiState {
    fn default() -> Self {
        Self {
            civil: super::civil_ui::CivilUiState::default(),
            contour_interval: 5.0,
            show_contours: true,
            selected: None,
            edit_point: [0.0; 3],
            new_point: [0.0; 3],
            query: [50.0, 50.0],
            query_result: None,
            profile_start: [0.0, 100.0],
            profile_end: [200.0, 100.0],
            profile_samples: 101,
            profile: Vec::new(),
            analysis_source: None,
        }
    }
}

impl ForgeApp {
    pub(super) fn terrain_tools(&mut self, ui: &mut egui::Ui) {
        section_header(ui, "SURVEY & SURFACE", "XY triangulation · model units");
        ui.label(format!(
            "{} points · {} faces",
            self.document.terrain.points.len(),
            self.document.terrain.triangles.len()
        ));
        if ui.button("Replace with example survey").clicked() {
            let mut candidate = self.document.clone();
            candidate.terrain = Terrain::example();
            if self.commit_workbench_document(candidate, "Example terrain created") {
                self.terrain_ui.selected = None;
                self.terrain_ui.profile.clear();
                self.terrain_ui.query_result = None;
            }
        }
        if ui.button("Rebuild triangulated surface").clicked() {
            self.rebuild_terrain_surface();
        }
        if self.document.terrain.triangles.is_empty() && !self.document.terrain.points.is_empty() {
            ui.label(RichText::new("Survey points are stored. Rebuild the surface to derive elevations and contours.").size(10.0).color(SELECTED));
        }
        ui.add_space(8.0);
        ui.separator();
        ui.label(RichText::new("ADD SURVEY POINT").strong().size(10.0));
        point3_controls(ui, &mut self.terrain_ui.new_point);
        if ui.button("Add point").clicked() {
            self.add_terrain_point(self.terrain_ui.new_point);
        }
        ui.add_space(8.0);
        ui.label(RichText::new("SURVEY POINTS").strong().size(10.0));
        egui::ScrollArea::vertical()
            .id_salt("survey_point_list")
            .max_height(240.0)
            .show(ui, |ui| {
                for (index, point) in self.document.terrain.points.iter().enumerate() {
                    if ui
                        .selectable_label(
                            self.terrain_ui.selected == Some(index),
                            format!(
                                "{}   {:.2}, {:.2}   Z {:.2}",
                                index + 1,
                                point[0],
                                point[1],
                                point[2]
                            ),
                        )
                        .clicked()
                    {
                        self.terrain_ui.selected = Some(index);
                        self.terrain_ui.edit_point = *point;
                    }
                }
            });
        ui.add_space(8.0);
        ui.separator();
        ui.checkbox(&mut self.terrain_ui.show_contours, "Show contour segments");
        ui.add(
            egui::DragValue::new(&mut self.terrain_ui.contour_interval)
                .speed(0.5)
                .range(0.001..=1e9)
                .prefix("Interval "),
        );
        ui.label(RichText::new("Contours and profiles are derived from the planar TIN. No extrapolation or smoothing is applied.").size(10.0).color(MUTED));
        ui.add_space(8.0);
        if ui.button("Fit terrain").clicked() {
            self.model_fit_requested = true;
        }
    }

    pub(super) fn terrain_inspector(&mut self, ui: &mut egui::Ui) {
        // Undo, Open and changes from another workbench can replace a survey.
        // Never retain derived query/profile values from an earlier surface.
        if self.terrain_ui.analysis_source.as_ref() != Some(&self.document.terrain) {
            self.terrain_ui.profile.clear();
            self.terrain_ui.query_result = None;
            self.terrain_ui.analysis_source = Some(self.document.terrain.clone());
            self.terrain_ui.civil.clear_analysis();
        }
        section_header(ui, "TERRAIN INSPECTOR", "Survey editing & analysis");
        if let Some(index) = self.terrain_ui.selected {
            if let Some(point) = self.document.terrain.points.get(index).copied() {
                ui.label(format!("Point {}", index + 1));
                ui.label(
                    RichText::new(format!(
                        "Current: {:.4}, {:.4}, {:.4}",
                        point[0], point[1], point[2]
                    ))
                    .size(10.0)
                    .color(MUTED),
                );
                point3_controls(ui, &mut self.terrain_ui.edit_point);
                ui.horizontal(|ui| {
                    if ui.button("Apply coordinates").clicked() {
                        self.apply_terrain_point(index, self.terrain_ui.edit_point);
                    }
                    if ui.button("Reload").clicked() {
                        self.terrain_ui.edit_point = point;
                    }
                });
                if ui.button("Delete survey point").clicked() {
                    let mut candidate = self.document.clone();
                    candidate.terrain.points.remove(index);
                    candidate.terrain.triangles.clear();
                    if self.commit_workbench_document(
                        candidate,
                        "Survey point deleted; surface requires rebuild",
                    ) {
                        self.terrain_ui.selected = None;
                        self.terrain_ui.profile.clear();
                        self.terrain_ui.query_result = None;
                    }
                }
            } else {
                self.terrain_ui.selected = None;
            }
        } else {
            ui.label(
                RichText::new("Select a survey point in the list to edit its coordinates.")
                    .size(10.0)
                    .color(MUTED),
            );
        }
        ui.add_space(12.0);
        ui.separator();
        ui.label(RichText::new("ELEVATION QUERY").strong().size(10.0));
        point2_controls(ui, "Query", &mut self.terrain_ui.query);
        if ui.button("Query surface elevation").clicked() {
            let position = Point::new(self.terrain_ui.query[0], self.terrain_ui.query[1]);
            match self.document.terrain.elevation_at(position) {
                Ok(value) => {
                    self.terrain_ui.query_result = Some(value);
                    self.status_is_error = false;
                }
                Err(error) => {
                    self.status = error.to_string();
                    self.status_is_error = true;
                }
            }
        }
        match self.terrain_ui.query_result {
            Some(Some(z)) => {
                ui.label(format!("Elevation: {z:.6}"));
            }
            Some(None) => {
                ui.label("Outside the triangulated surface");
            }
            None => {}
        }
        ui.add_space(12.0);
        ui.separator();
        ui.label(RichText::new("LONGITUDINAL PROFILE").strong().size(10.0));
        point2_controls(ui, "Start", &mut self.terrain_ui.profile_start);
        point2_controls(ui, "End", &mut self.terrain_ui.profile_end);
        ui.add(
            egui::DragValue::new(&mut self.terrain_ui.profile_samples)
                .range(2..=10_000)
                .prefix("Samples "),
        );
        if ui.button("Calculate profile").clicked() {
            let start = Point::new(
                self.terrain_ui.profile_start[0],
                self.terrain_ui.profile_start[1],
            );
            let end = Point::new(
                self.terrain_ui.profile_end[0],
                self.terrain_ui.profile_end[1],
            );
            match self
                .document
                .terrain
                .profile(start, end, self.terrain_ui.profile_samples)
            {
                Ok(profile) => {
                    self.terrain_ui.profile = profile;
                    self.status_is_error = false;
                }
                Err(error) => {
                    self.status = error.to_string();
                    self.status_is_error = true;
                }
            }
        }
        if !self.terrain_ui.profile.is_empty() {
            let valid = self
                .terrain_ui
                .profile
                .iter()
                .filter(|sample| sample.elevation.is_some())
                .count();
            ui.label(format!(
                "{valid}/{} samples on surface",
                self.terrain_ui.profile.len()
            ));
            draw_profile(ui, &self.terrain_ui.profile);
        }
        self.civil_controls(ui);
        self.project_controls(ui);
    }

    fn add_terrain_point(&mut self, point: [f64; 3]) {
        let mut candidate = self.document.clone();
        candidate.terrain.points.push(point);
        candidate.terrain.triangles.clear();
        if self.commit_workbench_document(candidate, "Survey point added; surface requires rebuild")
        {
            self.terrain_ui.selected = Some(self.document.terrain.points.len() - 1);
            self.terrain_ui.edit_point = point;
            self.terrain_ui.profile.clear();
            self.terrain_ui.query_result = None;
        }
    }

    fn apply_terrain_point(&mut self, index: usize, point: [f64; 3]) {
        let mut candidate = self.document.clone();
        let Some(existing) = candidate.terrain.points.get_mut(index) else {
            return;
        };
        *existing = point;
        candidate.terrain.triangles.clear();
        if self.commit_workbench_document(
            candidate,
            "Survey coordinates updated; surface requires rebuild",
        ) {
            self.terrain_ui.profile.clear();
            self.terrain_ui.query_result = None;
        }
    }

    fn rebuild_terrain_surface(&mut self) {
        match Terrain::triangulate(self.document.terrain.points.clone()) {
            Ok(mut terrain) => {
                terrain.alignments = self.document.terrain.alignments.clone();
                let selected = self
                    .terrain_ui
                    .selected
                    .and_then(|index| self.document.terrain.points.get(index))
                    .copied();
                let mut candidate = self.document.clone();
                candidate.terrain = terrain;
                if self.commit_workbench_document(candidate, "Terrain triangulation rebuilt") {
                    self.terrain_ui.selected = selected.and_then(|point| {
                        self.document
                            .terrain
                            .points
                            .iter()
                            .position(|p| *p == point)
                    });
                    self.terrain_ui.profile.clear();
                    self.terrain_ui.query_result = None;
                }
            }
            Err(error) => {
                self.status = error.to_string();
                self.status_is_error = true;
            }
        }
    }
}

fn point3_controls(ui: &mut egui::Ui, point: &mut [f64; 3]) {
    for (index, label) in ["X ", "Y ", "Elevation "].iter().enumerate() {
        ui.add(
            egui::DragValue::new(&mut point[index])
                .speed(1.0)
                .prefix(*label),
        );
    }
}
fn point2_controls(ui: &mut egui::Ui, label: &str, point: &mut [f64; 2]) {
    ui.label(label);
    ui.horizontal(|ui| {
        ui.add(egui::DragValue::new(&mut point[0]).speed(1.0).prefix("X "));
        ui.add(egui::DragValue::new(&mut point[1]).speed(1.0).prefix("Y "));
    });
}
pub(super) fn draw_profile(ui: &mut egui::Ui, samples: &[ProfileSample]) {
    let min = samples
        .iter()
        .filter_map(|sample| sample.elevation)
        .fold(f64::INFINITY, f64::min);
    let max = samples
        .iter()
        .filter_map(|sample| sample.elevation)
        .fold(f64::NEG_INFINITY, f64::max);
    if !min.is_finite() || !max.is_finite() {
        return;
    }
    let first = samples.first().map(|sample| sample.distance).unwrap_or(0.0);
    let last = samples.last().map(|sample| sample.distance).unwrap_or(1.0);
    let distance = (last - first).max(f64::EPSILON);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 130.0), Sense::hover());
    let plot = rect.shrink2(Vec2::new(8.0, 16.0));
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, BG);
    painter.line_segment(
        [plot.left_bottom(), plot.right_bottom()],
        Stroke::new(1.0, MUTED),
    );
    let mut previous = None;
    for sample in samples {
        if let Some(elevation) = sample.elevation {
            let x = ((sample.distance - first) / distance) as f32;
            let y = if max > min {
                ((elevation - min) / (max - min)) as f32
            } else {
                0.5
            };
            let current = Pos2::new(
                plot.left() + plot.width() * x,
                plot.bottom() - plot.height() * y,
            );
            if let Some(previous) = previous {
                painter.line_segment([previous, current], Stroke::new(1.5, ACCENT));
            }
            previous = Some(current);
        } else {
            previous = None;
        }
    }
    painter.text(
        rect.left_top() + Vec2::splat(3.0),
        Align2::LEFT_TOP,
        format!("Z {min:.2}–{max:.2}"),
        FontId::proportional(9.0),
        MUTED,
    );
    painter.text(
        rect.right_bottom() - Vec2::splat(3.0),
        Align2::RIGHT_BOTTOM,
        format!("{distance:.2} units"),
        FontId::proportional(9.0),
        MUTED,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn survey_edits_rebuild_and_undo_preserve_derived_surface() {
        let mut app = ForgeApp::default();
        app.add_terrain_point([0.0, 0.0, 0.0]);
        app.add_terrain_point([10.0, 0.0, 10.0]);
        app.add_terrain_point([0.0, 10.0, 20.0]);
        assert!(app.document.terrain.triangles.is_empty());
        app.rebuild_terrain_surface();
        assert_eq!(app.document.terrain.triangles.len(), 1);
        let before = app.document.clone();
        app.apply_terrain_point(0, [0.0, 0.0, 5.0]);
        assert!(app.document.terrain.triangles.is_empty());
        app.undo();
        assert_eq!(app.document, before);
    }
    #[test]
    fn duplicate_and_degenerate_survey_edits_leave_document_and_history_unchanged() {
        let mut app = ForgeApp::default();
        app.add_terrain_point([1.0, 2.0, 3.0]);
        let before = app.document.clone();
        let history = app.undo_stack.len();
        app.add_terrain_point([1.0, 2.0, 9.0]);
        assert!(app.status_is_error);
        assert_eq!(app.document, before);
        assert_eq!(app.undo_stack.len(), history);
        app.rebuild_terrain_surface();
        assert!(app.status_is_error);
        assert_eq!(app.document, before);
        assert_eq!(app.undo_stack.len(), history);
    }
}
