//! Scene authoring and timeline controls. View time is transient; scene and
//! keyframe edits use validated document transactions and one undo entry.
use super::*;
use forge_core::scene::{Axis, Interpolation, Keyframe, Modifier, Scene, SceneObject};

pub(super) struct SceneUiState {
    pub(super) time: f64,
    pub(super) playing: bool,
    pub(super) selected: Option<u64>,
    cached_object: Option<SceneObject>,
    cached_time: f64,
    properties: Option<SceneObject>,
    frame: Keyframe,
    duration_input: f64,
    fps_input: u32,
    cached_duration: f64,
    cached_fps: u32,
}

impl Default for SceneUiState {
    fn default() -> Self {
        Self {
            time: 0.0,
            playing: false,
            selected: None,
            cached_object: None,
            cached_time: -1.0,
            properties: None,
            frame: Keyframe::default(),
            duration_input: 5.0,
            fps_input: 24,
            cached_duration: 5.0,
            cached_fps: 24,
        }
    }
}

impl ForgeApp {
    pub(super) fn scene_tools(&mut self, ui: &mut egui::Ui) {
        section_header(ui, "SCENE OBJECTS", "Keyframed primitives");
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            for (label, shape) in [
                ("Box", SolidKind::Box),
                ("Cylinder", SolidKind::Cylinder),
                ("Sphere", SolidKind::Sphere),
            ] {
                if ui.button(format!("+ {label}")).clicked() {
                    self.scene_add_shape(shape);
                }
            }
        });
        if ui.button("Load example · replaces scene").clicked() {
            let mut candidate = self.document.clone();
            candidate.scene = Scene::example();
            if self.commit_workbench_document(candidate, "Animated scene example loaded") {
                self.scene_ui.selected = Some(1);
                self.scene_ui.time = 0.0;
                self.scene_ui.playing = false;
            }
        }
        ui.add_space(10.0);
        let objects: Vec<_> = self
            .document
            .scene
            .objects
            .iter()
            .map(|object| {
                (
                    object.id,
                    object.name.clone(),
                    object.shape,
                    object.keyframes.len(),
                    object.parent_id,
                )
            })
            .collect();
        if objects.is_empty() {
            ui.label(
                RichText::new("Add an object, then set poses at different times in the inspector.")
                    .size(10.0)
                    .color(MUTED),
            );
        }
        egui::ScrollArea::vertical()
            .id_salt("scene_object_tree")
            .max_height(280.0)
            .show(ui, |ui| {
                for (id, name, shape, keys, parent) in objects {
                    if ui
                        .selectable_label(
                            self.scene_ui.selected == Some(id),
                            format!("#{id}  {name}"),
                        )
                        .clicked()
                    {
                        self.scene_ui.selected = Some(id);
                    }
                    ui.label(
                        RichText::new(format!(
                            "{shape:?} · {keys} keyframe(s) · {}",
                            parent
                                .map(|id| format!("parent #{id}"))
                                .unwrap_or("world".into())
                        ))
                        .size(9.0)
                        .color(MUTED),
                    );
                }
            });
        ui.add_space(10.0);
        ui.separator();
        section_header(ui, "TIMELINE", "Seconds");
        self.scene_timeline(ui);
        if self.scene_ui.selected.is_some()
            && ui
                .button("Delete selected object and descendants")
                .clicked()
        {
            self.scene_delete_object();
        }
        ui.add_space(12.0);
        ui.label(RichText::new("Per-object interpolation, shortest Euler rotations. Scene objects and animation remain editable in native projects.").size(10.0).color(MUTED));
        ui.label(
            RichText::new("Middle-drag: pan · right-drag: orbit · scroll: zoom")
                .size(10.0)
                .color(MUTED),
        );
        if ui.button("Fit scene").clicked() {
            self.model_fit_requested = true;
        }
    }

    fn scene_timeline(&mut self, ui: &mut egui::Ui) {
        let duration = self.document.scene.duration;
        self.scene_ui.time = self.scene_ui.time.clamp(0.0, duration);
        ui.add(egui::Slider::new(&mut self.scene_ui.time, 0.0..=duration).suffix(" s"));
        ui.horizontal(|ui| {
            if ui
                .button(if self.scene_ui.playing {
                    "Pause"
                } else {
                    "Play"
                })
                .clicked()
            {
                if !self.scene_ui.playing && self.scene_ui.time >= duration {
                    self.scene_ui.time = 0.0;
                }
                self.scene_ui.playing = !self.scene_ui.playing;
            }
            if ui.button("Start").clicked() {
                self.scene_ui.time = 0.0;
                self.scene_ui.playing = false;
            }
            if ui.button("End").clicked() {
                self.scene_ui.time = duration;
                self.scene_ui.playing = false;
            }
        });
        ui.label(
            RichText::new(format!(
                "{:.3} / {:.3} s · {} fps",
                self.scene_ui.time, duration, self.document.scene.fps
            ))
            .size(10.0)
            .color(MUTED),
        );
    }

    pub(super) fn scene_inspector(&mut self, ui: &mut egui::Ui) {
        section_header(ui, "SCENE", "Animation settings");
        let scene = &self.document.scene;
        if self.scene_ui.cached_duration != scene.duration || self.scene_ui.cached_fps != scene.fps
        {
            self.scene_ui.duration_input = scene.duration;
            self.scene_ui.fps_input = scene.fps;
            self.scene_ui.cached_duration = scene.duration;
            self.scene_ui.cached_fps = scene.fps;
        }
        ui.add(
            egui::DragValue::new(&mut self.scene_ui.duration_input)
                .speed(0.1)
                .prefix("Duration ")
                .suffix(" s"),
        );
        ui.add(
            egui::DragValue::new(&mut self.scene_ui.fps_input)
                .range(1..=240)
                .prefix("Frame rate "),
        );
        if ui.button("Apply timing").clicked() {
            let mut candidate = self.document.clone();
            candidate.scene.duration = self.scene_ui.duration_input;
            candidate.scene.fps = self.scene_ui.fps_input;
            if self.commit_workbench_document(candidate, "Scene timing updated") {
                self.scene_ui.time = self.scene_ui.time.min(self.document.scene.duration);
            }
        }
        ui.label(
            RichText::new("Shortening duration is refused if it would exclude an authored key.")
                .size(9.0)
                .color(MUTED),
        );
        ui.add_space(10.0);
        ui.separator();
        self.scene_synchronize_editor();
        let Some(id) = self.scene_ui.selected else {
            ui.label(
                RichText::new("Select a scene object to edit its shape, color and keyframes.")
                    .size(10.0)
                    .color(MUTED),
            );
            return;
        };
        ui.label(RichText::new(format!("OBJECT #{id}")).strong());
        if let Some(properties) = &mut self.scene_ui.properties {
            ui.add(egui::TextEdit::singleline(&mut properties.name).hint_text("Object name"));
            ui.checkbox(&mut properties.visible, "Visible (including descendants)");
            egui::ComboBox::from_id_salt("scene_interpolation")
                .selected_text(format!("{:?}", properties.interpolation))
                .show_ui(ui, |ui| {
                    for mode in [
                        Interpolation::Linear,
                        Interpolation::Step,
                        Interpolation::Smoothstep,
                    ] {
                        ui.selectable_value(
                            &mut properties.interpolation,
                            mode,
                            format!("{mode:?}"),
                        );
                    }
                });
            let parent_name = properties
                .parent_id
                .and_then(|parent| self.document.scene.objects.iter().find(|o| o.id == parent))
                .map(|o| o.name.as_str())
                .unwrap_or("World (no parent)");
            egui::ComboBox::from_id_salt("scene_parent")
                .selected_text(parent_name)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut properties.parent_id, None, "World (no parent)");
                    for object in &self.document.scene.objects {
                        if object.id != id {
                            ui.selectable_value(
                                &mut properties.parent_id,
                                Some(object.id),
                                format!("#{} {}", object.id, object.name),
                            );
                        }
                    }
                });
            ui.label(RichText::new("Poses use parent coordinates. Changing parent keeps local keys; cycles are refused.").size(9.0).color(MUTED));
            ui.horizontal(|ui| {
                ui.label("Color");
                ui.color_edit_button_srgb(&mut properties.color);
            });
            scene_xyz(ui, "Local dimensions", &mut properties.size, 1.0);
            ui.collapsing("Modifier stack", |ui| {
                ui.label("Modifiers apply in local space before object and parent transforms.");
                let mut remove = None;
                let mut swap = None;
                for (index, modifier) in properties.modifiers.iter_mut().enumerate() {
                    ui.push_id(index, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(format!("{}", index + 1));
                            if ui.small_button("Up").clicked() && index > 0 {
                                swap = Some((index, index - 1));
                            }
                            if ui.small_button("Remove").clicked() {
                                remove = Some(index);
                            }
                        });
                        match modifier {
                            Modifier::Array { count, offset } => {
                                ui.add(
                                    egui::DragValue::new(count).range(1..=128).prefix("Copies "),
                                );
                                scene_xyz(ui, "Array step", offset, 1.0);
                            }
                            Modifier::Mirror {
                                axis,
                                offset,
                                keep_original,
                            } => {
                                egui::ComboBox::from_id_salt("mirror_axis")
                                    .selected_text(format!("{axis:?}"))
                                    .show_ui(ui, |ui| {
                                        for value in [Axis::X, Axis::Y, Axis::Z] {
                                            ui.selectable_value(axis, value, format!("{value:?}"));
                                        }
                                    });
                                ui.add(egui::DragValue::new(offset).prefix("Mirror plane "));
                                ui.checkbox(keep_original, "Keep original");
                            }
                        }
                    });
                }
                if let Some(index) = remove {
                    properties.modifiers.remove(index);
                } else if let Some((a, b)) = swap {
                    properties.modifiers.swap(a, b);
                }
                if ui.button("Add array modifier").clicked() {
                    properties.modifiers.push(Modifier::Array {
                        count: 3,
                        offset: [80.0, 0.0, 0.0],
                    });
                }
                if ui.button("Add mirror modifier").clicked() {
                    properties.modifiers.push(Modifier::Mirror {
                        axis: Axis::X,
                        offset: 0.0,
                        keep_original: true,
                    });
                }
            });
        }
        if ui.button("Apply object properties").clicked() {
            self.scene_apply_properties();
        }
        ui.add_space(12.0);
        ui.separator();
        ui.label(RichText::new(format!("POSE AT {:.3} s", self.scene_ui.time)).strong());
        ui.add_enabled_ui(!self.scene_ui.playing, |ui| {
            scene_xyz(ui, "Translation", &mut self.scene_ui.frame.translation, 1.0);
            scene_xyz(
                ui,
                "Rotation · degrees",
                &mut self.scene_ui.frame.rotation_deg,
                1.0,
            );
            scene_xyz(ui, "Scale", &mut self.scene_ui.frame.scale, 0.05);
            if ui.button("Set keyframe at current time").clicked() {
                self.scene_apply_keyframe();
            }
        });
        if self.scene_ui.playing {
            ui.label(
                RichText::new("Pause playback to author a keyframe.")
                    .size(9.0)
                    .color(MUTED),
            );
        }
        ui.label(RichText::new("Edits take effect when applied. Drag the timeline to evaluate the stored animation.").size(9.0).color(MUTED));
        ui.add_space(8.0);
        let times: Vec<f64> = self
            .document
            .scene
            .objects
            .iter()
            .find(|object| object.id == id)
            .map(|object| object.keyframes.iter().map(|frame| frame.time).collect())
            .unwrap_or_default();
        let allow_remove = times.len() > 1;
        egui::ScrollArea::vertical()
            .id_salt("scene_keyframe_list")
            .max_height(180.0)
            .show(ui, |ui| {
                for time in times {
                    ui.horizontal(|ui| {
                        if ui
                            .selectable_label(
                                (self.scene_ui.time - time).abs() < 1e-9,
                                format!("Key {time:.3} s"),
                            )
                            .clicked()
                        {
                            self.scene_ui.time = time;
                            self.scene_ui.playing = false;
                        }
                        if ui
                            .add_enabled(allow_remove, egui::Button::new("Remove"))
                            .clicked()
                        {
                            self.scene_remove_keyframe(time);
                        }
                    });
                }
            });
    }

    fn scene_synchronize_editor(&mut self) {
        let source = self
            .scene_ui
            .selected
            .and_then(|id| {
                self.document
                    .scene
                    .objects
                    .iter()
                    .find(|object| object.id == id)
            })
            .cloned();
        if source.is_none() {
            self.scene_ui.selected = None;
        }
        let object_changed = self.scene_ui.cached_object != source;
        if object_changed {
            self.scene_ui.properties = source.clone();
        }
        if object_changed || self.scene_ui.cached_time != self.scene_ui.time {
            if let Some(object) = &source {
                self.scene_ui.frame = object.pose_at(self.scene_ui.time).unwrap_or_default();
                self.scene_ui.frame.time = self.scene_ui.time;
            }
            self.scene_ui.cached_object = source;
            self.scene_ui.cached_time = self.scene_ui.time;
        }
    }

    fn scene_add_shape(&mut self, shape: SolidKind) {
        let mut candidate = self.document.clone();
        let Some(id) = candidate
            .scene
            .objects
            .iter()
            .map(|object| object.id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
        else {
            self.status = "Scene object ID space is exhausted".into();
            self.status_is_error = true;
            return;
        };
        candidate
            .scene
            .objects
            .push(SceneObject::new(id, format!("{shape:?} {id}"), shape));
        if self.commit_workbench_document(candidate, "Scene object added") {
            self.scene_ui.selected = Some(id);
        }
    }

    fn scene_delete_object(&mut self) {
        let Some(id) = self.scene_ui.selected else {
            return;
        };
        let mut candidate = self.document.clone();
        if let Err(error) = candidate.scene.remove_subtree(id) {
            self.status = error.to_string();
            self.status_is_error = true;
            return;
        }
        if self.commit_workbench_document(candidate, "Scene subtree deleted") {
            self.scene_ui.selected = None;
        }
    }

    fn scene_apply_properties(&mut self) {
        let Some(properties) = self.scene_ui.properties.clone() else {
            return;
        };
        let mut candidate = self.document.clone();
        let Some(object) = candidate
            .scene
            .objects
            .iter_mut()
            .find(|object| object.id == properties.id)
        else {
            return;
        };
        object.name = properties.name;
        object.color = properties.color;
        object.size = properties.size;
        object.parent_id = properties.parent_id;
        object.interpolation = properties.interpolation;
        object.visible = properties.visible;
        object.modifiers = properties.modifiers;
        self.commit_workbench_document(candidate, "Scene object properties updated");
    }

    fn scene_apply_keyframe(&mut self) {
        let Some(id) = self.scene_ui.selected else {
            return;
        };
        let mut candidate = self.document.clone();
        let Some(object) = candidate
            .scene
            .objects
            .iter_mut()
            .find(|object| object.id == id)
        else {
            return;
        };
        let mut frame = self.scene_ui.frame.clone();
        frame.time = self.scene_ui.time;
        match object.set_keyframe(frame) {
            Ok(()) => {
                self.commit_workbench_document(candidate, "Scene keyframe set");
            }
            Err(error) => {
                self.status = error.to_string();
                self.status_is_error = true;
            }
        }
    }

    fn scene_remove_keyframe(&mut self, time: f64) {
        let Some(id) = self.scene_ui.selected else {
            return;
        };
        let mut candidate = self.document.clone();
        let Some(object) = candidate
            .scene
            .objects
            .iter_mut()
            .find(|object| object.id == id)
        else {
            return;
        };
        if object.keyframes.len() <= 1 {
            self.status = "Retain at least one keyframe per scene object".into();
            self.status_is_error = true;
            return;
        }
        object.keyframes.retain(|frame| frame.time != time);
        self.commit_workbench_document(candidate, "Scene keyframe removed");
    }
}

fn scene_xyz(ui: &mut egui::Ui, label: &str, values: &mut [f64; 3], speed: f64) {
    ui.label(RichText::new(label).size(10.0).color(MUTED));
    ui.horizontal(|ui| {
        for (axis, name) in ["X", "Y", "Z"].into_iter().enumerate() {
            ui.add(
                egui::DragValue::new(&mut values[axis])
                    .speed(speed)
                    .prefix(format!("{name} ")),
            );
        }
    });
}

#[cfg(test)]
mod hierarchy_tests {
    use super::*;
    #[test]
    fn parenting_and_subtree_delete_use_one_undo_step_and_cycles_keep_history_unchanged() {
        let mut app = ForgeApp::default();
        app.document.scene = Scene::example();
        app.scene_ui.selected = Some(2);
        app.scene_synchronize_editor();
        app.scene_ui.properties.as_mut().unwrap().parent_id = Some(1);
        app.scene_apply_properties();
        assert!(!app.status_is_error);
        assert_eq!(app.undo_stack.len(), 1);
        let before = app.document.clone();
        app.scene_ui.selected = Some(1);
        app.scene_synchronize_editor();
        app.scene_ui.properties.as_mut().unwrap().parent_id = Some(2);
        app.scene_apply_properties();
        assert!(app.status_is_error);
        assert_eq!(app.document, before);
        assert_eq!(app.undo_stack.len(), 1);
        app.scene_delete_object();
        assert!(!app.status_is_error);
        assert!(app.document.scene.objects.is_empty());
        assert_eq!(app.undo_stack.len(), 2);
        app.undo();
        assert_eq!(app.document, before);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scene_authoring_uses_one_undo_entry_per_keyframe_and_preserves_other_workbenches() {
        let mut app = ForgeApp::default();
        app.document.add_geometry(Geometry::Point {
            position: Point::new(5.0, 6.0),
        });
        app.scene_add_shape(SolidKind::Box);
        app.scene_ui.time = 2.0;
        app.scene_synchronize_editor();
        app.scene_ui.frame.translation = [30.0, 40.0, 50.0];
        app.scene_apply_keyframe();
        assert_eq!(app.document.entities.len(), 1);
        assert_eq!(app.document.scene.objects[0].keyframes.len(), 2);
        assert_eq!(app.undo_stack.len(), 2);
        let before = app.document.clone();
        app.undo();
        assert_eq!(app.document.scene.objects[0].keyframes.len(), 1);
        app.redo();
        assert_eq!(app.document, before);
    }
    #[test]
    fn invalid_keyframe_and_last_key_removal_preserve_document_and_history() {
        let mut app = ForgeApp::default();
        app.scene_add_shape(SolidKind::Sphere);
        app.scene_synchronize_editor();
        let before = app.document.clone();
        app.scene_ui.frame.scale = [-1.0, 1.0, 1.0];
        app.scene_apply_keyframe();
        assert!(app.status_is_error);
        assert_eq!(app.document, before);
        assert_eq!(app.undo_stack.len(), 1);
        app.scene_remove_keyframe(0.0);
        assert_eq!(app.document, before);
        assert_eq!(app.undo_stack.len(), 1);
    }
}
