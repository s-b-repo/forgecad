//! Shared semantic workbench canvas, manufacturing forms and derived exports.
use super::*;
use forge_core::cam::{CamJob, Compensation, Operation, Profile};
use std::sync::Arc;

type WorkbenchMeshes = Vec<(u64, [u8; 3], TriangleMesh)>;
type CachedGeometry = (
    Arc<WorkbenchMeshes>,
    Arc<Vec<forge_core::cam::Motion>>,
    Arc<Vec<forge_core::terrain::ContourSegment>>,
    Arc<Vec<[[f64; 3]; 2]>>,
);
struct CanvasCache {
    document: Document,
    workspace: Workspace,
    time: f64,
    interval: f64,
    meshes: Arc<WorkbenchMeshes>,
    motions: Arc<Vec<forge_core::cam::Motion>>,
    contours: Arc<Vec<forge_core::terrain::ContourSegment>>,
    alignments: Arc<Vec<[[f64; 3]; 2]>>,
}

fn cam_unit_scale(units: u16) -> Option<f64> {
    match units {
        0 | 4 => Some(1.0),
        1 => Some(25.4),
        2 => Some(304.8),
        5 => Some(10.0),
        6 => Some(1000.0),
        _ => None,
    }
}

pub(super) struct WorkbenchUiState {
    job: CamJob,
    source_job: Option<CamJob>,
    selected_profile: Option<u64>,
    compensation: Compensation,
    operation: Operation,
    pub(super) export_format: String,
    canvas_cache: Option<CanvasCache>,
    plan_cache: Option<(CamJob, Result<forge_core::cam::Toolpath, String>)>,
}
impl Default for WorkbenchUiState {
    fn default() -> Self {
        Self {
            job: CamJob::default(),
            source_job: None,
            selected_profile: None,
            compensation: Compensation::Outside,
            operation: Operation::Profile,
            export_format: "obj".into(),
            canvas_cache: None,
            plan_cache: None,
        }
    }
}

impl ForgeApp {
    pub(super) fn cam_tools(&mut self, ui: &mut egui::Ui) {
        section_header(ui, "MANUFACTURING", "Profiles, pockets and drilling · mm");
        if ui.button("Create machining example").clicked() {
            let mut doc = self.document.clone();
            doc.manufacturing = CamJob::example();
            self.commit_workbench_document(doc, "Machining example created");
        }
        ui.label("Copy Draft contours for profiles, closed shapes for pockets, or points for drilling. Selected closed shapes can also become uncut pocket islands. Drawing units convert to mm.");
        egui::ComboBox::from_id_salt("cam_operation")
            .selected_text(format!("{:?}", self.workbench_ui.operation))
            .show_ui(ui, |ui| {
                for operation in [Operation::Profile, Operation::Pocket, Operation::Drill] {
                    ui.selectable_value(
                        &mut self.workbench_ui.operation,
                        operation,
                        format!("{operation:?}"),
                    );
                }
            });
        egui::ComboBox::from_id_salt("cam_compensation")
            .selected_text(format!("{:?}", self.workbench_ui.compensation))
            .show_ui(ui, |ui| {
                for value in [
                    Compensation::Center,
                    Compensation::Inside,
                    Compensation::Outside,
                ] {
                    ui.selectable_value(
                        &mut self.workbench_ui.compensation,
                        value,
                        format!("{value:?}"),
                    );
                }
            });
        if ui.button("Add selected Draft operations").clicked() {
            self.copy_draft_profiles_to_cam();
        }
        ui.separator();
        for profile in &self.document.manufacturing.profiles {
            if ui
                .selectable_label(
                    self.workbench_ui.selected_profile == Some(profile.id),
                    format!("#{} {} · {:?}", profile.id, profile.name, profile.operation),
                )
                .clicked()
            {
                self.workbench_ui.selected_profile = Some(profile.id);
            }
        }
        if let Some(id) = self.workbench_ui.selected_profile {
            if ui.button("Remove profile").clicked() {
                let mut doc = self.document.clone();
                doc.manufacturing.profiles.retain(|p| p.id != id);
                if self.commit_workbench_document(doc, "Machining profile removed") {
                    self.workbench_ui.selected_profile = None;
                }
            }
        }
        ui.separator();
        ui.label("Preview: blue cutting moves, amber positioning moves. Stock top is Z=0.");
    }

    fn copy_draft_profiles_to_cam(&mut self) {
        let Some(factor) = cam_unit_scale(self.document.units) else {
            self.status="CAM import supports unitless/mm, inches, feet, centimetres and metres. Convert drawing units explicitly before importing other unit codes.".into();
            self.status_is_error = true;
            return;
        };
        let mut doc = self.document.clone();
        let mut id = doc
            .manufacturing
            .profiles
            .iter()
            .map(|p| p.id)
            .max()
            .unwrap_or(0);
        let mut added = 0;
        for entity in self
            .document
            .entities
            .iter()
            .filter(|e| self.selected_entities.contains(&e.id))
        {
            let Some(next) = id.checked_add(1) else {
                self.status = "CAM profile ID space exhausted".into();
                self.status_is_error = true;
                return;
            };
            id = next;
            let geometry = match entity
                .geometry
                .transformed(Transform2d::uniform_scale(factor, Point::default()))
            {
                Ok(g) => g,
                Err(error) => {
                    self.status = error.to_string();
                    self.status_is_error = true;
                    return;
                }
            };
            doc.manufacturing.profiles.push(Profile {
                id,
                name: format!("Draft object {}", entity.id),
                geometry,
                compensation: match self.workbench_ui.operation {
                    Operation::Profile => self.workbench_ui.compensation,
                    Operation::Pocket => Compensation::Inside,
                    Operation::Drill => Compensation::Center,
                },
                operation: self.workbench_ui.operation,
                islands: Vec::new(),
            });
            added += 1;
        }
        if added == 0 {
            self.status = "Select closed shapes in Draft first".into();
            self.status_is_error = true;
        } else {
            self.commit_workbench_document(
                doc,
                "Draft profiles copied into machining job in millimetres",
            );
        }
    }

    pub(super) fn cam_inspector(&mut self, ui: &mut egui::Ui) {
        section_header(ui, "MACHINING SETUP", "Tool, stock and profile parameters");
        if self.workbench_ui.source_job.as_ref() != Some(&self.document.manufacturing) {
            self.workbench_ui.job = self.document.manufacturing.clone();
            self.workbench_ui.source_job = Some(self.document.manufacturing.clone());
        }
        let job = &mut self.workbench_ui.job;
        for (label, value) in [
            ("Tool diameter", &mut job.tool_diameter),
            ("Depth", &mut job.depth),
            ("Step down", &mut job.step_down),
            ("Pocket step over", &mut job.step_over),
            ("Clearance Z", &mut job.safe_z),
            ("Cut feed mm/min", &mut job.feed),
            ("Plunge mm/min", &mut job.plunge_feed),
        ] {
            ui.add(
                egui::DragValue::new(value)
                    .speed(0.1)
                    .prefix(format!("{label}: ")),
            );
        }
        ui.add(
            egui::DragValue::new(&mut job.spindle_rpm)
                .speed(100.0)
                .prefix("Spindle RPM: "),
        );
        ui.label("Stock bounds (top Z must be 0)");
        for axis in 0..3 {
            ui.horizontal(|ui| {
                ui.label(["X", "Y", "Z"][axis]);
                ui.add(egui::DragValue::new(&mut job.stock_min[axis]).prefix("Min "));
                ui.add(egui::DragValue::new(&mut job.stock_max[axis]).prefix("Max "));
            });
        }
        if let Some(id) = self.workbench_ui.selected_profile {
            if let Some(profile) = job.profiles.iter_mut().find(|p| p.id == id) {
                ui.text_edit_singleline(&mut profile.name);
                egui::ComboBox::from_id_salt("cam_profile_operation")
                    .selected_text(format!("{:?}", profile.operation))
                    .show_ui(ui, |ui| {
                        for operation in [Operation::Profile, Operation::Pocket, Operation::Drill] {
                            if ui
                                .selectable_value(
                                    &mut profile.operation,
                                    operation,
                                    format!("{operation:?}"),
                                )
                                .changed()
                            {
                                profile.compensation = match operation {
                                    Operation::Pocket => Compensation::Inside,
                                    Operation::Drill => Compensation::Center,
                                    Operation::Profile => profile.compensation,
                                };
                                if operation != Operation::Pocket {
                                    profile.islands.clear();
                                }
                            }
                        }
                    });
                egui::ComboBox::from_id_salt("cam_profile_side")
                    .selected_text(format!("{:?}", profile.compensation))
                    .show_ui(ui, |ui| {
                        for value in [
                            Compensation::Center,
                            Compensation::Inside,
                            Compensation::Outside,
                        ] {
                            ui.selectable_value(
                                &mut profile.compensation,
                                value,
                                format!("{value:?}"),
                            );
                        }
                    });
                if profile.operation == Operation::Pocket {
                    ui.label(format!(
                        "{} island(s) · cut around, never through",
                        profile.islands.len()
                    ));
                    if ui.button("Set islands from Draft selection").clicked() {
                        let Some(factor) = cam_unit_scale(self.document.units) else {
                            self.status = "CAM islands require unitless/mm, inches, feet, centimetres or metres".into();
                            self.status_is_error = true;
                            return;
                        };
                        let mut islands = Vec::new();
                        for entity in self
                            .document
                            .entities
                            .iter()
                            .filter(|entity| self.selected_entities.contains(&entity.id))
                        {
                            match entity
                                .geometry
                                .transformed(Transform2d::uniform_scale(factor, Point::default()))
                            {
                                Ok(geometry) => islands.push(geometry),
                                Err(error) => {
                                    self.status = error.to_string();
                                    self.status_is_error = true;
                                    return;
                                }
                            }
                        }
                        if islands.is_empty() {
                            self.status = "Select closed Draft shapes to use as islands".into();
                            self.status_is_error = true;
                        } else {
                            profile.islands = islands;
                        }
                    }
                    if ui.button("Clear islands").clicked() {
                        profile.islands.clear();
                    }
                } else if !profile.islands.is_empty() {
                    profile.islands.clear();
                }
            }
        }
        if ui.button("Apply machining setup").clicked() {
            let mut doc = self.document.clone();
            doc.manufacturing = self.workbench_ui.job.clone();
            self.commit_workbench_document(doc, "Machining setup updated");
        }
        ui.separator();
        if self
            .workbench_ui
            .plan_cache
            .as_ref()
            .is_none_or(|(job, _)| job != &self.document.manufacturing)
        {
            self.workbench_ui.plan_cache = Some((
                self.document.manufacturing.clone(),
                self.document
                    .manufacturing
                    .plan()
                    .map_err(|e| e.to_string()),
            ));
        }
        match &self
            .workbench_ui
            .plan_cache
            .as_ref()
            .expect("plan cached")
            .1
        {
            Ok(path) => {
                ui.label(format!(
                    "{} passes · {} motions",
                    path.passes,
                    path.motions.len()
                ));
                ui.label(format!("Cut distance {:.2} mm", path.cutting_distance));
                ui.label(format!("Estimated {:.2} min", path.estimated_minutes));
            }
            Err(error) => {
                ui.colored_label(Color32::LIGHT_RED, error.to_string());
            }
        }
        ui.label(RichText::new("Generic profile program. No stock-removal, fixture collision or machine-specific postprocessing. Preview and controller setup remain separate.").size(10.0).color(MUTED));
    }

    pub(super) fn workbench_export_controls(&mut self, ui: &mut egui::Ui) {
        let formats: &[&str] = match self.active_workspace {
            Workspace::Build => &["obj", "csv"],
            Workspace::Terrain => &["obj", "csv", "dxf"],
            Workspace::Motion => &["obj"],
            Workspace::Make => &["nc"],
            Workspace::Connect => &["csv"],
            _ => &[],
        };
        if !formats.contains(&self.workbench_ui.export_format.as_str()) {
            self.workbench_ui.export_format = formats.first().copied().unwrap_or("obj").into();
        }
        ui.separator();
        ui.label("Derived workbench export");
        egui::ComboBox::from_id_salt("workbench_export_format")
            .selected_text(self.workbench_ui.export_format.to_uppercase())
            .show_ui(ui, |ui| {
                for value in formats {
                    ui.selectable_value(
                        &mut self.workbench_ui.export_format,
                        (*value).into(),
                        value.to_uppercase(),
                    );
                }
            });
        ui.label(RichText::new("OBJ: surfaces only. CSV: quantities/survey. DXF: terrain contours. Native Save retains editable models.").size(10.0).color(MUTED));
        if ui.button("Export this workbench").clicked() {
            self.export_current();
        }
    }

    pub(super) fn prepare_workbench_export(&self) -> std::io::Result<PreparedExport> {
        self.document.validate()?;
        let default = match self.active_workspace {
            Workspace::Make => "nc",
            Workspace::Connect => "csv",
            _ => "obj",
        };
        let requested = self.workbench_ui.export_format.as_str();
        let extension = match (self.active_workspace, requested) {
            (Workspace::Build, "obj" | "csv")
            | (Workspace::Terrain, "obj" | "csv" | "dxf")
            | (Workspace::Motion, "obj")
            | (Workspace::Make, "nc")
            | (Workspace::Connect, "csv") => requested,
            _ => default,
        };
        let bytes = match (self.active_workspace, extension) {
            (Workspace::Connect, _) => self.document.review.csv()?.into_bytes(),
            (Workspace::Build, "csv") => self.document.building.schedule_csv()?.into_bytes(),
            (Workspace::Terrain, "csv") => format!(
                "# units: {}\n{}",
                self.document.units,
                self.document.terrain.points_csv()?
            )
            .into_bytes(),
            (Workspace::Make, _) => self.document.manufacturing.gcode()?.into_bytes(),
            (Workspace::Terrain, "dxf") => {
                let doc = self
                    .document
                    .terrain
                    .contour_drawing(self.terrain_ui.contour_interval, self.document.units)?;
                forge_core::dxf::write(&doc)?.into_bytes()
            }
            _ => {
                let mut mesh = TriangleMesh::default();
                for (_, _, part) in self.workbench_meshes()? {
                    mesh.append(&part)?;
                }
                if mesh.triangles.is_empty() {
                    return Err(std::io::Error::other(
                        "This workbench has no surface geometry to export",
                    ));
                }
                mesh.to_obj(&self.document.name)?.into_bytes()
            }
        };
        // Include the area in the filename so workbenches don't collide.
        let base = export_path(&self.file_path, extension);
        let stem = base
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("design");
        let path = base.with_file_name(format!(
            "{stem}-{}.{}",
            workbench(self.active_workspace).name.to_ascii_lowercase(),
            extension
        ));
        let replace_existing = std::fs::symlink_metadata(&path).is_ok();
        Ok(PreparedExport {
            path,
            bytes,
            losses: Vec::new(),
            replace_existing,
        })
    }

    fn workbench_meshes(&self) -> std::io::Result<Vec<(u64, [u8; 3], TriangleMesh)>> {
        match self.active_workspace {
            Workspace::Build => Ok(self
                .document
                .building
                .meshes()?
                .into_iter()
                .map(|(id, _, mesh)| (id, [160, 180, 195], mesh))
                .collect()),
            Workspace::Terrain => Ok(vec![(0, [100, 150, 105], self.document.terrain.mesh()?)]),
            Workspace::Motion => Ok(self
                .document
                .scene
                .meshes_at(self.scene_ui.time)?
                .into_iter()
                .map(|(id, _, color, mesh)| (id, color, mesh))
                .collect()),
            Workspace::Make => {
                let job = &self.document.manufacturing;
                let solid = Solid {
                    id: 1,
                    name: "Stock".into(),
                    kind: SolidKind::Box,
                    center: std::array::from_fn(|i| {
                        job.stock_min[i] / 2.0 + job.stock_max[i] / 2.0
                    }),
                    size: std::array::from_fn(|i| job.stock_max[i] - job.stock_min[i]),
                };
                Ok(vec![(
                    0,
                    [100, 110, 125],
                    tessellate_solids(&[solid], TessellationOptions::default())?,
                )])
            }
            _ => Ok(Vec::new()),
        }
    }

    fn cached_workbench_geometry(&mut self) -> std::io::Result<CachedGeometry> {
        let time = if self.active_workspace == Workspace::Motion {
            self.scene_ui.time
        } else {
            0.0
        };
        let interval = if self.active_workspace == Workspace::Terrain {
            self.terrain_ui.contour_interval
        } else {
            1.0
        };
        let valid = self.workbench_ui.canvas_cache.as_ref().is_some_and(|c| {
            c.document == self.document
                && c.workspace == self.active_workspace
                && c.time == time
                && c.interval == interval
        });
        if !valid {
            let meshes = self.workbench_meshes()?;
            let motions = if self.active_workspace == Workspace::Make {
                if self
                    .workbench_ui
                    .plan_cache
                    .as_ref()
                    .is_none_or(|(job, _)| job != &self.document.manufacturing)
                {
                    self.workbench_ui.plan_cache = Some((
                        self.document.manufacturing.clone(),
                        self.document
                            .manufacturing
                            .plan()
                            .map_err(|e| e.to_string()),
                    ));
                }
                self.workbench_ui
                    .plan_cache
                    .as_ref()
                    .expect("plan cached")
                    .1
                    .as_ref()
                    .map(|p| p.motions.clone())
                    .map_err(|e| std::io::Error::other(e.clone()))?
            } else {
                Vec::new()
            };
            let contours = if self.active_workspace == Workspace::Terrain {
                self.document.terrain.contours(interval)?
            } else {
                Vec::new()
            };
            // Alignments describe XY plan geometry, shown on a reference plane
            // above the surface. Profiles separately evaluate actual elevations.
            let mut alignments = Vec::new();
            if self.active_workspace == Workspace::Terrain {
                let z = self
                    .document
                    .terrain
                    .points
                    .iter()
                    .map(|p| p[2])
                    .reduce(f64::max)
                    .unwrap_or(0.0);
                for alignment in &self.document.terrain.alignments {
                    for pair in alignment.vertices.windows(2) {
                        alignments.push([[pair[0].x, pair[0].y, z], [pair[1].x, pair[1].y, z]]);
                    }
                }
            }
            self.workbench_ui.canvas_cache = Some(CanvasCache {
                document: self.document.clone(),
                workspace: self.active_workspace,
                time,
                interval,
                meshes: Arc::new(meshes),
                motions: Arc::new(motions),
                contours: Arc::new(contours),
                alignments: Arc::new(alignments),
            });
        }
        let c = self
            .workbench_ui
            .canvas_cache
            .as_ref()
            .expect("geometry cached");
        Ok((
            c.meshes.clone(),
            c.motions.clone(),
            c.contours.clone(),
            c.alignments.clone(),
        ))
    }

    pub(super) fn workbench_canvas(&mut self, ui: &mut egui::Ui) {
        if self.active_workspace == Workspace::Motion
            && self.scene_ui.playing
            && self.pending_action.is_none()
            && self.pending_export.is_none()
        {
            let dt = ui.input(|i| i.stable_dt).min(0.1) as f64;
            self.scene_ui.time = (self.scene_ui.time + dt).rem_euclid(self.document.scene.duration);
            ui.ctx().request_repaint();
        }
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, BG);
        let (meshes, motions, contours, alignments) = match self.cached_workbench_geometry() {
            Ok(m) => m,
            Err(error) => {
                painter.text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    error.to_string(),
                    FontId::proportional(14.0),
                    Color32::LIGHT_RED,
                );
                return;
            }
        };
        if self.model_fit_requested {
            let camera = Camera3d {
                zoom: 1.0,
                pan: Vec2::ZERO,
                ..self.camera_3d
            };
            let mut bounds = Rect::NOTHING;
            for (_, _, mesh) in meshes.iter() {
                for point in &mesh.vertices {
                    bounds.extend_with(camera.project(*point, rect).0);
                }
            }
            for motion in motions.iter() {
                bounds.extend_with(camera.project(motion.position, rect).0);
            }
            for segment in alignments.iter() {
                for point in segment {
                    bounds.extend_with(camera.project(*point, rect).0);
                }
            }
            if bounds.is_finite() && bounds.is_positive() {
                self.camera_3d.zoom = ((rect.width() - 70.0).max(20.0) / bounds.width().max(1.0))
                    .min((rect.height() - 70.0).max(20.0) / bounds.height().max(1.0))
                    .clamp(0.00001, 1000.0);
                self.camera_3d.pan = (rect.center() - bounds.center()) * self.camera_3d.zoom;
            }
            self.model_fit_requested = false;
        }
        self.interact_model(ui, rect, &response);
        paint_model_grid(&painter, rect, self.camera_3d);
        if response.clicked()
            && let Some(pointer) = response.interact_pointer_pos()
        {
            let nearest = meshes
                .iter()
                .filter_map(|(id, _, mesh)| {
                    let mut bounds = Rect::NOTHING;
                    for point in &mesh.vertices {
                        bounds.extend_with(self.camera_3d.project(*point, rect).0);
                    }
                    bounds
                        .expand(5.0)
                        .contains(pointer)
                        .then_some((*id, bounds.center().distance(pointer)))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|p| p.0);
            match self.active_workspace {
                Workspace::Build => self.building_ui.selected = nearest,
                Workspace::Motion => self.scene_ui.selected = nearest,
                _ => {}
            }
        }
        let selected = match self.active_workspace {
            Workspace::Build => self.building_ui.selected.and_then(|id| {
                self.document
                    .building
                    .openings
                    .iter()
                    .find(|o| o.id == id)
                    .map(|o| o.wall_id)
                    .or(Some(id))
            }),
            Workspace::Motion => self.scene_ui.selected,
            _ => None,
        };
        paint_workbench_meshes(&painter, rect, self.camera_3d, &meshes, selected);
        if self.active_workspace == Workspace::Terrain {
            for segment in alignments.iter() {
                painter.line_segment(
                    segment.map(|p| self.camera_3d.project(p, rect).0),
                    Stroke::new(2.5, ACCENT),
                );
            }
            for point in &self.document.terrain.points {
                painter.circle_filled(self.camera_3d.project(*point, rect).0, 2.5, SELECTED);
            }
            if self.terrain_ui.show_contours {
                for s in contours.iter() {
                    painter.line_segment(
                        [
                            self.camera_3d
                                .project([s.start.x, s.start.y, s.level], rect)
                                .0,
                            self.camera_3d.project([s.end.x, s.end.y, s.level], rect).0,
                        ],
                        Stroke::new(1.5, SELECTED),
                    );
                }
            }
        }
        for pair in motions.windows(2) {
            let color = if pair[1].rapid { SELECTED } else { ACCENT };
            painter.line_segment(
                [
                    self.camera_3d.project(pair[0].position, rect).0,
                    self.camera_3d.project(pair[1].position, rect).0,
                ],
                Stroke::new(1.5, color),
            );
        }
        paint_axis_badge(&painter, rect);
        painter.text(
            rect.left_top() + Vec2::new(15.0, 15.0),
            Align2::LEFT_TOP,
            format!(
                "{} · LOCAL MODEL",
                workbench(self.active_workspace).subtitle
            ),
            FontId::proportional(11.0),
            TEXT,
        );
        if meshes.iter().all(|(_, _, m)| m.vertices.is_empty())
            && self.active_workspace != Workspace::Make
        {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "Add elements using the tools at left, or create an example.",
                FontId::proportional(15.0),
                MUTED,
            );
        }
    }
}

fn paint_workbench_meshes(
    painter: &Painter,
    rect: Rect,
    camera: Camera3d,
    meshes: &[(u64, [u8; 3], TriangleMesh)],
    selected: Option<u64>,
) {
    let mut faces = Vec::new();
    for (id, color, mesh) in meshes {
        let vertices: Vec<_> = mesh
            .vertices
            .iter()
            .map(|p| camera.project(*p, rect))
            .collect();
        for triangle in &mesh.triangles {
            let [a, b, c] = triangle.map(|index| vertices[index as usize]);
            let points = [a.0, b.0, c.0];
            if !rect.intersects(Rect::from_points(&points)) {
                continue;
            }
            let [p, q, r] = triangle.map(|index| mesh.vertices[index as usize]);
            let u = std::array::from_fn::<_, 3, _>(|i| q[i] - p[i]);
            let v = std::array::from_fn::<_, 3, _>(|i| r[i] - p[i]);
            let n = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ];
            let length = n[0].hypot(n[1]).hypot(n[2]);
            let light =
                0.45 + 0.55 * (n[0] * 0.3 + n[1] * 0.4 + n[2] * 0.866).abs() / length.max(1e-20);
            let rgb = if selected == Some(*id) {
                [220, 170, 75]
            } else {
                *color
            };
            let fill = Color32::from_rgb(
                (rgb[0] as f64 * light) as u8,
                (rgb[1] as f64 * light) as u8,
                (rgb[2] as f64 * light) as u8,
            );
            faces.push(((a.1 + b.1 + c.1) / 3.0, points, fill));
        }
    }
    faces.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (_, points, fill) in faces {
        painter.add(Shape::convex_polygon(
            points.to_vec(),
            fill,
            Stroke::new(0.4, fill.gamma_multiply(0.75)),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn semantic_workbench_changes_share_undo_and_native_persistence() {
        let mut app = ForgeApp::default();
        let mut doc = app.document.clone();
        doc.manufacturing = CamJob::example();
        assert!(app.commit_workbench_document(doc.clone(), "Job"));
        assert_eq!(app.undo_stack.len(), 1);
        let native = Document::from_json(&app.document.to_json().unwrap()).unwrap();
        assert_eq!(native, doc);
        app.undo();
        assert!(app.document.manufacturing.is_empty());
        app.redo();
        assert_eq!(app.document, doc);
        let mut bad = doc.clone();
        bad.manufacturing.step_down = 0.0;
        assert!(!app.commit_workbench_document(bad, "Invalid"));
        assert_eq!(app.document, doc);
        assert_eq!(app.undo_stack.len(), 1);
    }
    #[test]
    fn exporting_one_workbench_keeps_other_domains_intact() {
        let mut app = ForgeApp::starting_at(Workspace::Make);
        app.document.manufacturing = CamJob::example();
        app.document.building = forge_core::building::Building::example();
        let before = app.document.clone();
        let output = app.prepare_workbench_export().unwrap();
        assert!(output.path.to_string_lossy().ends_with("-make.nc"));
        assert!(
            String::from_utf8(output.bytes)
                .unwrap()
                .contains("G17 G21 G90 G94")
        );
        assert_eq!(app.document, before);
    }

    #[test]
    fn drafting_profiles_convert_to_mm_without_relabeling_source_units() {
        let mut app = ForgeApp::default();
        app.document.units = 1;
        let id = app.document.add_geometry(Geometry::Rectangle {
            a: Point::new(0.0, 0.0),
            b: Point::new(2.0, 1.0),
        });
        app.selected_entities.insert(id);
        let source = app.document.entities.clone();
        app.copy_draft_profiles_to_cam();
        assert!(!app.status_is_error, "{}", app.status);
        assert_eq!(app.document.units, 1);
        assert_eq!(app.document.entities, source);
        let bounds = app.document.manufacturing.profiles[0]
            .geometry
            .bounds()
            .unwrap();
        assert_eq!(bounds.max, Point::new(50.8, 25.4));
        let before = app.document.clone();
        app.document.units = 7;
        let invalid = app.document.clone();
        app.copy_draft_profiles_to_cam();
        assert!(app.status_is_error);
        assert_eq!(app.document, invalid);
        assert_eq!(app.document.manufacturing, before.manufacturing);
    }

    #[test]
    fn open_routes_between_drawing_model_and_semantic_workbenches() {
        let folder = PathBuf::from(format!(
            "target/workbench-open-tests-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("drawing.forge.json");
        let mut drawing = Document::default();
        drawing.add_geometry(Geometry::Point {
            position: Point::default(),
        });
        drawing.save_json(&path).unwrap();
        let mut app = ForgeApp::starting_at(Workspace::Build);
        app.perform_open_document(path.clone());
        assert_eq!(app.active_workspace, Workspace::Draft);
        assert_eq!(app.selected_product_id, "autocad");
        let mut model = Document::default();
        model.add_solid("Part", SolidKind::Box);
        model.save_json(&path).unwrap();
        app.perform_open_document(path.clone());
        assert_eq!(app.active_workspace, Workspace::Model);
        assert_eq!(app.selected_product_id, "inventor");
        let mut building = Document::default();
        building.building = forge_core::building::Building::example();
        building.save_json(&path).unwrap();
        app.perform_open_document(path);
        assert_eq!(app.active_workspace, Workspace::Build);
        assert_eq!(app.selected_product_id, "revit");
    }

    #[test]
    fn view_cache_reuses_static_geometry_and_evaluates_changed_scene_times() {
        let mut app = ForgeApp::starting_at(Workspace::Motion);
        app.document.scene = forge_core::scene::Scene::example();
        let first = app.cached_workbench_geometry().unwrap().0;
        let repeated = app.cached_workbench_geometry().unwrap().0;
        assert!(Arc::ptr_eq(&first, &repeated));
        app.scene_ui.time = 5.0;
        let last = app.cached_workbench_geometry().unwrap().0;
        assert!(!Arc::ptr_eq(&first, &last));
        assert_ne!(first[0].2.bounds(), last[0].2.bounds());
        app.document.scene.objects[0].color = [255, 0, 0];
        let recolored = app.cached_workbench_geometry().unwrap().0;
        assert_eq!(recolored[0].1, [255, 0, 0]);
    }
}
