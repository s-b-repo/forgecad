//! Local project review issues. These records have no account, messaging,
//! synchronisation or vendor service integration. Element references are human
//! descriptions, not checked links into another workbench's identifier space.
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fmt::Write, io};

pub const MAX_REVIEW_ITEMS: usize = 10_000;
const MAX_TITLE_BYTES: usize = 512;
const MAX_DESCRIPTION_BYTES: usize = 32_768;
const MAX_REFERENCE_BYTES: usize = 1024;
const MAX_TOTAL_TEXT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    #[default]
    Open,
    InProgress,
    Resolved,
}
impl ReviewStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::InProgress => "In progress",
            Self::Resolved => "Resolved",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewPriority {
    Low,
    #[default]
    Normal,
    High,
}
impl ReviewPriority {
    pub fn label(self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Normal => "Normal",
            Self::High => "High",
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewItem {
    pub id: u64,
    pub title: String,
    pub description: String,
    pub status: ReviewStatus,
    pub priority: ReviewPriority,
    pub element_reference: String,
}
impl ReviewItem {
    pub fn new(id: u64, title: impl Into<String>) -> Self {
        Self {
            id,
            title: title.into(),
            description: String::new(),
            status: ReviewStatus::Open,
            priority: ReviewPriority::Normal,
            element_reference: String::new(),
        }
    }
    pub fn validate(&self) -> io::Result<()> {
        if self.id == 0
            || self.title.trim().is_empty()
            || self.title.len() > MAX_TITLE_BYTES
            || self.title.chars().any(char::is_control)
        {
            return Err(invalid(
                "Review items require a nonzero ID and a nonempty single-line title of at most 512 bytes",
            ));
        }
        if self.description.len() > MAX_DESCRIPTION_BYTES
            || self
                .description
                .chars()
                .any(|c| c.is_control() && !matches!(c, '\r' | '\n' | '\t'))
        {
            return Err(invalid(
                "Review descriptions permit paragraphs and tabs, with at most 32768 bytes and no other controls",
            ));
        }
        if self.element_reference.len() > MAX_REFERENCE_BYTES
            || self.element_reference.chars().any(char::is_control)
        {
            return Err(invalid(
                "Review references must be single-line text of at most 1024 bytes",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectReview {
    #[serde(default)]
    pub items: Vec<ReviewItem>,
}
impl ProjectReview {
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    pub fn validate(&self) -> io::Result<()> {
        if self.items.len() > MAX_REVIEW_ITEMS {
            return Err(invalid("Project review exceeds 10000 items"));
        }
        let mut ids = BTreeSet::new();
        let mut text_bytes = 0usize;
        for item in &self.items {
            item.validate()?;
            if !ids.insert(item.id) {
                return Err(invalid("Review item IDs must be unique"));
            }
            text_bytes = text_bytes
                .checked_add(item.title.len())
                .and_then(|n| n.checked_add(item.description.len()))
                .and_then(|n| n.checked_add(item.element_reference.len()))
                .ok_or_else(|| invalid("Review text size overflow"))?;
            if text_bytes > MAX_TOTAL_TEXT_BYTES {
                return Err(invalid("Project review exceeds 8 MiB of text"));
            }
        }
        Ok(())
    }
    /// A derived issue schedule. Quoted fields preserve commas/newlines/quotes;
    /// potentially executable spreadsheet text receives an apostrophe prefix.
    /// The native project retains the exact text and review lifecycle.
    pub fn csv(&self) -> io::Result<String> {
        self.validate()?;
        let mut text = String::from("id,title,description,status,priority,element_reference\n");
        for item in &self.items {
            writeln!(
                text,
                "{},{},{},{},{},{}",
                item.id,
                csv_text(&item.title),
                csv_text(&item.description),
                csv_text(item.status.label()),
                csv_text(item.priority.label()),
                csv_text(&item.element_reference),
            )
            .unwrap();
        }
        Ok(text)
    }
    pub fn example() -> Self {
        Self {
            items: vec![
                ReviewItem {
                    id: 1,
                    title: "Check doorway clearance".into(),
                    description: "Review the opening width against the project requirements. Record the decision here.".into(),
                    status: ReviewStatus::Open,
                    priority: ReviewPriority::High,
                    element_reference: "Building · ground-floor door".into(),
                },
                ReviewItem {
                    id: 2,
                    title: "Confirm survey elevations".into(),
                    description: "Compare the imported survey with the latest field measurements.".into(),
                    status: ReviewStatus::InProgress,
                    priority: ReviewPriority::Normal,
                    element_reference: "Terrain · survey points".into(),
                },
                ReviewItem {
                    id: 3,
                    title: "Archive initial design review".into(),
                    description: "The initial local review was recorded in this project.".into(),
                    status: ReviewStatus::Resolved,
                    priority: ReviewPriority::Low,
                    element_reference: String::new(),
                },
            ],
        }
    }
}

fn csv_text(value: &str) -> String {
    let first = value.trim_start().chars().next();
    let formula = matches!(first, Some('=' | '+' | '-' | '@'));
    format!(
        "\"{}{}\"",
        if formula { "'" } else { "" },
        value.replace('"', "\"\""),
    )
}
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_and_exact_native_text_round_trip() {
        let mut board = ProjectReview::example();
        board.items[0].description = "First line\nSecond line, with \"quotes\"".into();
        board.items[0].status = ReviewStatus::Resolved;
        board.validate().unwrap();
        let text = serde_json::to_string(&board).unwrap();
        assert_eq!(serde_json::from_str::<ProjectReview>(&text).unwrap(), board);
        assert!(text.contains("\"status\":\"resolved\""));
        assert!(serde_json::from_str::<ProjectReview>("{\"items\":[],\"cloud\":true}").is_err());
    }

    #[test]
    fn csv_preserves_structure_and_neutralizes_spreadsheet_formulas() {
        let mut board = ProjectReview::example();
        board.items.truncate(1);
        board.items[0].title = "  =HYPERLINK(\"file\")".into();
        board.items[0].description = "Paragraph one\nParagraph two, \"quoted\"".into();
        board.items[0].element_reference = "+Wall 2".into();
        let before = board.clone();
        let text = board.csv().unwrap();
        assert!(text.starts_with("id,title,description,status,priority,element_reference\n"));
        assert!(text.contains("\"'  =HYPERLINK(\"\"file\"\")\""));
        assert!(text.contains("\"Paragraph one\nParagraph two, \"\"quoted\"\"\""));
        assert!(text.contains("\"'+Wall 2\""));
        assert_eq!(board, before);
    }

    #[test]
    fn invalid_ids_text_and_resource_limits_are_refused() {
        let mut board = ProjectReview::example();
        board.items[1].id = board.items[0].id;
        assert!(board.validate().is_err());
        for title in [
            String::new(),
            " \t ".into(),
            "Two\nlines".into(),
            "x".repeat(513),
        ] {
            assert!(ReviewItem::new(1, title).validate().is_err());
        }
        let mut item = ReviewItem::new(0, "Issue");
        assert!(item.validate().is_err());
        item.id = 1;
        item.description = "bad\0description".into();
        assert!(item.validate().is_err());
        item.description.clear();
        item.element_reference = "bad\nreference".into();
        assert!(item.validate().is_err());
        let board = ProjectReview {
            items: (1..=MAX_REVIEW_ITEMS + 1)
                .map(|id| ReviewItem::new(id as u64, "Issue"))
                .collect(),
        };
        assert!(board.validate().is_err());
    }
}
