//! Persistent polyline alignments, station/offset queries and earthwork reports.
use super::*;
use forge_core::civil::{Alignment, Earthwork, StationOffset};

pub(super) struct CivilUiState {
    selected: Option<u64>,
    source: Option<Alignment>,
    edit: Option<Alignment>,
    new_name: String,
    station: f64,
    offset: f64,
    query: Point,
    projection: Option<StationOffset>,
    pub(super) profile: Vec<forge_core::terrain::ProfileSample>,
    samples: usize,
    datum: f64,
    quantities: Option<Earthwork>,
}
impl Default for CivilUiState {
    fn default() -> Self {
        Self {
            selected: None,
            source: None,
            edit: None,
            new_name: "Road centerline".into(),
            station: 0.0,
            offset: 0.0,
            query: Point::default(),
            projection: None,
            profile: Vec::new(),
            samples: 101,
            datum: 0.0,
            quantities: None,
        }
    }
}
impl CivilUiState {
    pub(super) fn clear_analysis(&mut self) {
        self.profile.clear();
        self.projection = None;
        self.quantities = None;
    }
}

impl ForgeApp {
    pub(super) fn civil_controls(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.collapsing("Horizontal alignments", |ui| {
            ui.label("Line / circular-arc stations · left-positive offsets");
            ui.text_edit_singleline(&mut self.terrain_ui.civil.new_name);
            if ui.button("Create from profile endpoints").clicked() {
                self.create_alignment();
            }
            for alignment in &self.document.terrain.alignments {
                if ui
                    .selectable_label(
                        self.terrain_ui.civil.selected == Some(alignment.id),
                        format!("#{} {}", alignment.id, alignment.name),
                    )
                    .clicked()
                {
                    self.terrain_ui.civil.selected = Some(alignment.id);
                    self.terrain_ui.civil.clear_analysis();
                }
            }
            let source = self
                .terrain_ui
                .civil
                .selected
                .and_then(|id| self.document.terrain.alignments.iter().find(|a| a.id == id))
                .cloned();
            if source != self.terrain_ui.civil.source {
                self.terrain_ui.civil.edit = source.clone();
                self.terrain_ui.civil.source = source;
            }
            if let Some(alignment) = &mut self.terrain_ui.civil.edit {
                ui.text_edit_singleline(&mut alignment.name);
                ui.add(egui::DragValue::new(&mut alignment.start_station).prefix("Start station "));
                if alignment.bulges.is_empty() {
                    alignment.bulges.resize(alignment.vertices.len().saturating_sub(1), 0.0);
                }
                ui.label(RichText::new("Segment bulge: 0 = line; tan(sweep/4). Positive arcs turn counterclockwise.").size(9.0).color(MUTED));
                let mut remove = None;
                for (index, point) in alignment.vertices.iter_mut().enumerate() {
                    ui.push_id(index, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(format!("{}", index + 1));
                            ui.add(egui::DragValue::new(&mut point.x).prefix("X "));
                            ui.add(egui::DragValue::new(&mut point.y).prefix("Y "));
                            if ui.small_button("×").clicked() {
                                remove = Some(index);
                            }
                        });
                        if let Some(bulge) = alignment.bulges.get_mut(index) {
                            ui.add(egui::DragValue::new(bulge).speed(0.01).prefix("Arc bulge "));
                        }
                    });
                }
                if let Some(index) = remove {
                    remove_alignment_vertex(alignment, index);
                }
                if ui.button("Append vertex").clicked() {
                    let last = alignment.vertices.last().copied().unwrap_or_default();
                    alignment.vertices.push(Point::new(last.x + 10.0, last.y));
                    if !alignment.bulges.is_empty() {
                        alignment.bulges.push(0.0);
                    }
                }
                if ui.button("Apply alignment").clicked() {
                    self.apply_alignment();
                }
                if ui.button("Delete alignment").clicked() {
                    let id = self.terrain_ui.civil.selected;
                    let mut document = self.document.clone();
                    document.terrain.alignments.retain(|a| Some(a.id) != id);
                    if self.commit_workbench_document(document, "Alignment removed") {
                        self.terrain_ui.civil.selected = None;
                    }
                }
            }
            let Some(id) = self.terrain_ui.civil.selected else {
                return;
            };
            ui.separator();
            ui.add(egui::DragValue::new(&mut self.terrain_ui.civil.query.x).prefix("Query X "));
            ui.add(egui::DragValue::new(&mut self.terrain_ui.civil.query.y).prefix("Query Y "));
            if ui.button("Find station and offset").clicked()
                && let Some(alignment) =
                    self.document.terrain.alignments.iter().find(|a| a.id == id)
            {
                match alignment.station_offset(self.terrain_ui.civil.query) {
                    Ok(projection) => {
                        self.terrain_ui.civil.projection = Some(projection);
                        self.status_is_error = false;
                    }
                    Err(error) => {
                        self.status = error.to_string();
                        self.status_is_error = true;
                    }
                }
            }
            if let Some(p) = self.terrain_ui.civil.projection {
                ui.label(format!(
                    "Station {:.4} · offset {:.4} · distance {:.4}",
                    p.station, p.offset, p.distance
                ));
            }
            ui.add(egui::DragValue::new(&mut self.terrain_ui.civil.station).prefix("Station "));
            ui.add(egui::DragValue::new(&mut self.terrain_ui.civil.offset).prefix("Left offset "));
            if ui.button("Locate station on surface").clicked() {
                let result = self
                    .document
                    .terrain
                    .alignments
                    .iter()
                    .find(|a| a.id == id)
                    .ok_or_else(|| std::io::Error::other("Select an existing alignment"))
                    .and_then(|a| {
                        a.point_at(self.terrain_ui.civil.station, self.terrain_ui.civil.offset)
                    })
                    .and_then(|point| {
                        self.document
                            .terrain
                            .elevation_at(point)
                            .map(|elevation| (point, elevation))
                    });
                match result {
                    Ok((p, z)) => {
                        self.status = format!(
                            "X {:.6}, Y {:.6}, elevation {}",
                            p.x,
                            p.y,
                            z.map(|z| format!("{z:.6}"))
                                .unwrap_or("outside surface".into())
                        );
                        self.status_is_error = false;
                    }
                    Err(error) => {
                        self.status = error.to_string();
                        self.status_is_error = true;
                    }
                }
            }
            ui.add(
                egui::DragValue::new(&mut self.terrain_ui.civil.samples)
                    .range(2..=10_000)
                    .prefix("Samples "),
            );
            if ui.button("Evaluate alignment profile").clicked() {
                match self.document.terrain.alignment_profile(
                    id,
                    self.terrain_ui.civil.samples,
                    self.terrain_ui.civil.offset,
                ) {
                    Ok(profile) => {
                        self.terrain_ui.civil.profile = profile;
                        self.status_is_error = false;
                    }
                    Err(error) => {
                        self.status = error.to_string();
                        self.status_is_error = true;
                    }
                }
            }
            if !self.terrain_ui.civil.profile.is_empty() {
                super::terrain_ui::draw_profile(ui, &self.terrain_ui.civil.profile);
            }
            if ui.button("Export alignment profile CSV").clicked() {
                match self.document.terrain.alignment_csv(
                    id,
                    self.terrain_ui.civil.samples,
                    self.terrain_ui.civil.offset,
                ) {
                    Ok(csv) => {
                        self.publish_civil_report(&format!("alignment-{id}"), csv.into_bytes())
                    }
                    Err(error) => {
                        self.status = error.to_string();
                        self.status_is_error = true;
                    }
                }
            }
        });
        ui.collapsing("Cut / fill to level datum", |ui| {
            if ui.add(egui::DragValue::new(&mut self.terrain_ui.civil.datum).prefix("Design elevation ")).changed() { self.terrain_ui.civil.quantities = None; }
            if ui.button("Calculate exact TIN quantities").clicked() {
                match self.document.terrain.earthwork(self.terrain_ui.civil.datum) {
                    Ok(q) => { self.terrain_ui.civil.quantities = Some(q); self.status_is_error = false; }
                    Err(error) => { self.status = error.to_string(); self.status_is_error = true; }
                }
            }
            if let Some(q) = self.terrain_ui.civil.quantities {
                ui.label(format!("Plan area {:.6} · cut {:.6} · fill {:.6}\nNet cut {:.6}",q.area,q.cut,q.fill,q.net_cut));
                ui.label(RichText::new("Area and volume use model units² / units³. Exact for this triangulated surface and a level design datum.").size(9.0).color(MUTED));
                if ui.button("Export cut / fill CSV").clicked() {
                    self.publish_civil_report("earthwork",format!("unit_code,datum,area,cut,fill,net_cut\n{},{},{},{},{},{}\n",self.document.units,q.datum,q.area,q.cut,q.fill,q.net_cut).into_bytes());
                }
            }
        });
    }

    fn create_alignment(&mut self) {
        let mut document = self.document.clone();
        let Some(id) = document
            .terrain
            .alignments
            .iter()
            .map(|a| a.id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
        else {
            self.status = "Alignment ID space is exhausted".into();
            self.status_is_error = true;
            return;
        };
        document.terrain.alignments.push(Alignment {
            id,
            name: self.terrain_ui.civil.new_name.clone(),
            start_station: 0.0,
            bulges: Vec::new(),
            vertices: vec![
                Point::new(
                    self.terrain_ui.profile_start[0],
                    self.terrain_ui.profile_start[1],
                ),
                Point::new(
                    self.terrain_ui.profile_end[0],
                    self.terrain_ui.profile_end[1],
                ),
            ],
        });
        if self.commit_workbench_document(document, "Alignment created") {
            self.terrain_ui.civil.selected = Some(id);
        }
    }

    fn apply_alignment(&mut self) {
        let Some(alignment) = self.terrain_ui.civil.edit.clone() else {
            return;
        };
        let mut document = self.document.clone();
        let Some(saved) = document
            .terrain
            .alignments
            .iter_mut()
            .find(|a| a.id == alignment.id)
        else {
            return;
        };
        *saved = alignment;
        self.commit_workbench_document(document, "Alignment updated");
    }

    fn publish_civil_report(&mut self, suffix: &str, bytes: Vec<u8>) {
        let base = export_path(&self.file_path, "csv");
        let stem = base
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("design");
        let path = base.with_file_name(format!("{stem}-{suffix}.csv"));
        let replace_existing = std::fs::symlink_metadata(&path).is_ok();
        let job = PreparedExport {
            path,
            bytes,
            replace_existing,
            losses: Vec::new(),
        };
        if replace_existing {
            self.pending_export = Some(job);
        } else {
            self.publish_export(job);
        }
    }
}

fn remove_alignment_vertex(alignment: &mut Alignment, index: usize) {
    let count = alignment.vertices.len();
    if index >= count {
        return;
    }
    alignment.vertices.remove(index);
    if alignment.bulges.is_empty() {
        return;
    }
    let removed_segment = index.min(count.saturating_sub(2));
    if removed_segment < alignment.bulges.len() {
        alignment.bulges.remove(removed_segment);
    }
    // Only the new segment spanning a removed internal vertex becomes straight.
    // Unaffected segment endpoints and arc parameters keep their identity.
    if index > 0 && index + 1 < count {
        if let Some(join) = alignment.bulges.get_mut(index - 1) {
            *join = 0.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deleting_alignment_vertices_preserves_unaffected_curves() {
        let alignment = Alignment {
            id: 1,
            name: "Curved centerline".into(),
            start_station: 0.0,
            vertices: (0..5).map(|i| Point::new(i as f64 * 10.0, 0.0)).collect(),
            bulges: vec![0.1, 0.2, 0.3, 0.4],
        };
        let mut internal = alignment.clone();
        remove_alignment_vertex(&mut internal, 2);
        assert_eq!(internal.bulges, [0.1, 0.0, 0.4]);
        internal.validate().unwrap();
        assert_eq!(internal.point_at(0.0, 0.0).unwrap(), alignment.vertices[0]);
        let mut first = alignment.clone();
        remove_alignment_vertex(&mut first, 0);
        assert_eq!(first.bulges, [0.2, 0.3, 0.4]);
        first.validate().unwrap();
        let mut last = alignment.clone();
        remove_alignment_vertex(&mut last, 4);
        assert_eq!(last.bulges, [0.1, 0.2, 0.3]);
        last.validate().unwrap();
        while !last.vertices.is_empty() {
            remove_alignment_vertex(&mut last, 0);
        }
        assert!(last.bulges.is_empty());
    }
    #[test]
    fn alignment_edits_preserve_survey_and_reject_invalid_vertices_without_history_changes() {
        let mut app = ForgeApp::default();
        app.document.terrain = forge_core::terrain::Terrain::example();
        let surface = app.document.terrain.clone();
        app.create_alignment();
        assert_eq!(app.document.terrain.alignments.len(), 1);
        assert_eq!(app.document.terrain.points, surface.points);
        assert_eq!(app.document.terrain.triangles, surface.triangles);
        let before = app.document.clone();
        let history = app.undo_stack.len();
        let mut alignment = before.terrain.alignments[0].clone();
        alignment.vertices[1] = alignment.vertices[0];
        app.terrain_ui.civil.edit = Some(alignment);
        app.apply_alignment();
        assert!(app.status_is_error);
        assert_eq!(app.document, before);
        assert_eq!(app.undo_stack.len(), history);
        app.undo();
        assert_eq!(app.document.terrain, surface);
        app.redo();
        assert_eq!(app.document, before);
    }
}
