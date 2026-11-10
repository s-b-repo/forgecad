//! Local review board with staged issue edits and undoable lifecycle changes.
use super::*;
use forge_core::review::{ProjectReview, ReviewItem, ReviewPriority, ReviewStatus};

#[derive(Default)]
pub(super) struct ReviewUiState {
    pub(super) selected: Option<u64>,
    search: String,
    status_filter: Option<ReviewStatus>,
    new_title: String,
    source: Option<ReviewItem>,
    draft: Option<ReviewItem>,
}

impl ForgeApp {
    pub(super) fn review_tools(&mut self, ui: &mut egui::Ui) {
        section_header(ui, "PROJECT REVIEW", "Local issues & decisions");
        ui.add_space(8.0);
        ui.label("Issues are stored in this native project. Save the project to share a copy.");
        ui.add_space(8.0);
        ui.add(
            egui::TextEdit::singleline(&mut self.review_ui.new_title).hint_text("New issue title"),
        );
        if ui.button("Create issue").clicked() {
            self.review_create_item();
        }
        ui.separator();
        ui.add(egui::TextEdit::singleline(&mut self.review_ui.search).hint_text("Search issues"));
        egui::ComboBox::from_id_salt("review_status_filter")
            .selected_text(
                self.review_ui
                    .status_filter
                    .map(ReviewStatus::label)
                    .unwrap_or("All statuses"),
            )
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.review_ui.status_filter, None, "All statuses");
                for status in [
                    ReviewStatus::Open,
                    ReviewStatus::InProgress,
                    ReviewStatus::Resolved,
                ] {
                    ui.selectable_value(
                        &mut self.review_ui.status_filter,
                        Some(status),
                        status.label(),
                    );
                }
            });
        ui.add_space(8.0);
        for status in [
            ReviewStatus::Open,
            ReviewStatus::InProgress,
            ReviewStatus::Resolved,
        ] {
            let count = self
                .document
                .review
                .items
                .iter()
                .filter(|item| item.status == status)
                .count();
            ui.label(format!("{}: {count}", status.label()));
        }
        ui.add_space(12.0);
        if ui
            .button(if self.document.review.is_empty() {
                "Load review example"
            } else {
                "Replace review with example"
            })
            .clicked()
        {
            let mut candidate = self.document.clone();
            candidate.review = ProjectReview::example();
            if self.commit_workbench_document(candidate, "Review example loaded") {
                self.review_ui.selected = Some(1);
            }
        }
        ui.label(
            RichText::new(
                "Local review only. No account, notifications, cloud sync or external messaging.",
            )
            .size(10.0)
            .color(MUTED),
        );
    }

    pub(super) fn review_inspector(&mut self, ui: &mut egui::Ui) {
        section_header(ui, "ISSUE", "Edit then Apply");
        self.review_sync_editor();
        let Some(id) = self.review_ui.selected else {
            ui.label("Choose an issue on the board to edit its decision, priority and status.");
            return;
        };
        ui.label(RichText::new(format!("Issue #{id}")).strong());
        if let Some(item) = &mut self.review_ui.draft {
            ui.label("Title");
            ui.add(egui::TextEdit::singleline(&mut item.title).desired_width(f32::INFINITY));
            ui.label("Description / decision");
            ui.add(
                egui::TextEdit::multiline(&mut item.description)
                    .desired_rows(8)
                    .desired_width(f32::INFINITY),
            );
            ui.label("Element reference");
            ui.add(
                egui::TextEdit::singleline(&mut item.element_reference)
                    .hint_text("e.g. Building · Wall #2"),
            );
            ui.label(RichText::new("References are descriptive text; they do not follow element renumbering automatically.").size(9.0).color(MUTED));
            egui::ComboBox::from_id_salt("review_status")
                .selected_text(item.status.label())
                .show_ui(ui, |ui| {
                    for status in [
                        ReviewStatus::Open,
                        ReviewStatus::InProgress,
                        ReviewStatus::Resolved,
                    ] {
                        ui.selectable_value(&mut item.status, status, status.label());
                    }
                });
            egui::ComboBox::from_id_salt("review_priority")
                .selected_text(format!("{} priority", item.priority.label()))
                .show_ui(ui, |ui| {
                    for priority in [
                        ReviewPriority::Low,
                        ReviewPriority::Normal,
                        ReviewPriority::High,
                    ] {
                        ui.selectable_value(&mut item.priority, priority, priority.label());
                    }
                });
        }
        ui.horizontal(|ui| {
            if ui.button("Apply issue").clicked() {
                self.review_apply_item();
            }
            if ui.button("Reset").clicked() {
                self.review_ui.draft = self.review_ui.source.clone();
            }
        });
        if ui.button("Delete issue").clicked() {
            self.review_delete_item();
        }
    }

    pub(super) fn review_canvas(&mut self, ui: &mut egui::Ui) {
        ui.add_space(12.0);
        ui.heading("Project review");
        ui.label("Track local design issues, decisions and resolution status.");
        ui.separator();
        let items: Vec<ReviewItem> = self
            .document
            .review
            .items
            .iter()
            .filter(|item| {
                review_matches(item, &self.review_ui.search, self.review_ui.status_filter)
            })
            .cloned()
            .collect();
        ui.label(format!(
            "{} of {} issues",
            items.len(),
            self.document.review.items.len()
        ));
        if items.is_empty() {
            ui.add_space(24.0);
            ui.label(if self.document.review.is_empty() {
                "Create your first issue using the tools at left."
            } else {
                "No issues match these filters."
            });
        }
        egui::ScrollArea::vertical()
            .id_salt("project_review_board")
            .show(ui, |ui| {
                for item in items {
                    egui::Frame::new()
                        .fill(PANEL)
                        .stroke(Stroke::new(
                            1.0,
                            if self.review_ui.selected == Some(item.id) {
                                ACCENT
                            } else {
                                BORDER
                            },
                        ))
                        .inner_margin(12.0)
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                if ui
                                    .selectable_label(
                                        self.review_ui.selected == Some(item.id),
                                        RichText::new(format!("#{}  {}", item.id, item.title))
                                            .strong(),
                                    )
                                    .clicked()
                                {
                                    self.review_ui.selected = Some(item.id);
                                }
                                ui.label(
                                    RichText::new(item.status.label())
                                        .color(review_status_color(item.status)),
                                );
                                ui.label(format!("{} priority", item.priority.label()));
                            });
                            if !item.description.is_empty() {
                                let mut description: String =
                                    item.description.chars().take(240).collect();
                                if item.description.chars().count() > 240 {
                                    description.push('…');
                                }
                                ui.label(description);
                            }
                            if !item.element_reference.is_empty() {
                                ui.label(
                                    RichText::new(&item.element_reference)
                                        .size(10.0)
                                        .color(MUTED),
                                );
                            }
                            if ui.small_button("Edit issue").clicked() {
                                self.review_ui.selected = Some(item.id);
                            }
                        });
                    ui.add_space(8.0);
                }
            });
    }

    fn review_sync_editor(&mut self) {
        let item = self
            .review_ui
            .selected
            .and_then(|id| self.document.review.items.iter().find(|item| item.id == id))
            .cloned();
        if item != self.review_ui.source {
            self.review_ui.source = item.clone();
            self.review_ui.draft = item;
        }
        if self.review_ui.source.is_none() {
            self.review_ui.selected = None;
        }
    }

    fn review_create_item(&mut self) {
        let Some(id) = self
            .document
            .review
            .items
            .iter()
            .map(|item| item.id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
        else {
            self.status = "Review issue ID space is exhausted".into();
            self.status_is_error = true;
            return;
        };
        let mut candidate = self.document.clone();
        candidate
            .review
            .items
            .push(ReviewItem::new(id, self.review_ui.new_title.clone()));
        if self.commit_workbench_document(candidate, "Review issue created") {
            self.review_ui.selected = Some(id);
            self.review_ui.new_title.clear();
        }
    }

    fn review_apply_item(&mut self) {
        let Some(draft) = self.review_ui.draft.clone() else {
            return;
        };
        let mut candidate = self.document.clone();
        let Some(item) = candidate
            .review
            .items
            .iter_mut()
            .find(|item| item.id == draft.id)
        else {
            return;
        };
        *item = draft;
        self.commit_workbench_document(candidate, "Review issue updated");
    }

    fn review_delete_item(&mut self) {
        let Some(id) = self.review_ui.selected else {
            return;
        };
        let mut candidate = self.document.clone();
        candidate.review.items.retain(|item| item.id != id);
        if self.commit_workbench_document(candidate, "Review issue deleted") {
            self.review_ui.selected = None;
        }
    }
}

fn review_matches(item: &ReviewItem, query: &str, status: Option<ReviewStatus>) -> bool {
    if status.is_some_and(|status| status != item.status) {
        return false;
    }
    let query = query.to_lowercase();
    [
        item.title.as_str(),
        item.description.as_str(),
        item.element_reference.as_str(),
    ]
    .iter()
    .any(|text| text.to_lowercase().contains(&query))
}
fn review_status_color(status: ReviewStatus) -> Color32 {
    match status {
        ReviewStatus::Open => SELECTED,
        ReviewStatus::InProgress => ACCENT,
        ReviewStatus::Resolved => Color32::from_rgb(120, 205, 145),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_lifecycle_uses_undo_and_preserves_other_workbenches() {
        let mut app = ForgeApp::starting_at(Workspace::Connect);
        app.document.building = forge_core::building::Building::example();
        let building = app.document.building.clone();
        app.review_ui.new_title = "Check the door".into();
        app.review_create_item();
        app.review_sync_editor();
        assert_eq!(app.document.review.items.len(), 1);
        assert_eq!(app.undo_stack.len(), 1);
        app.review_ui.draft.as_mut().unwrap().status = ReviewStatus::InProgress;
        app.review_apply_item();
        assert_eq!(
            app.document.review.items[0].status,
            ReviewStatus::InProgress
        );
        assert_eq!(app.undo_stack.len(), 2);
        app.undo();
        assert_eq!(app.document.review.items[0].status, ReviewStatus::Open);
        app.redo();
        assert_eq!(
            app.document.review.items[0].status,
            ReviewStatus::InProgress
        );
        assert_eq!(app.document.building, building);
        assert_eq!(
            Document::from_json(&app.document.to_json().unwrap()).unwrap(),
            app.document
        );
    }

    #[test]
    fn invalid_issue_edits_and_exhausted_ids_preserve_history() {
        let mut app = ForgeApp::starting_at(Workspace::Connect);
        app.document.review = ProjectReview::example();
        app.review_ui.selected = Some(1);
        app.review_sync_editor();
        let before = app.document.clone();
        app.review_ui.draft.as_mut().unwrap().title.clear();
        app.review_apply_item();
        assert!(app.status_is_error);
        assert_eq!(app.document, before);
        assert!(app.undo_stack.is_empty());
        app.document.review.items[0].id = u64::MAX;
        let exhausted = app.document.clone();
        app.review_ui.new_title = "More".into();
        app.review_create_item();
        assert_eq!(app.document, exhausted);
        assert!(app.undo_stack.is_empty());
    }

    #[test]
    fn board_filters_match_status_and_all_issue_text() {
        let board = ProjectReview::example();
        assert!(review_matches(
            &board.items[0],
            "DOOR",
            Some(ReviewStatus::Open)
        ));
        assert!(!review_matches(
            &board.items[0],
            "DOOR",
            Some(ReviewStatus::Resolved)
        ));
        assert!(review_matches(&board.items[1], "field measurements", None));
        assert!(review_matches(&board.items[2], "", None));
    }
}
