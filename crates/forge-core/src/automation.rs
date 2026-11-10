//! Typed, atomic native-project edits for reproducible workbench authoring.
use crate::{
    Document,
    building::Building,
    cam::Profile,
    civil::Alignment,
    plot::PrintLayout,
    scene::{Keyframe, SceneObject},
};
use serde::{Deserialize, Serialize};
use std::io;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectEdit {
    SceneObject {
        object: SceneObject,
    },
    SceneParent {
        object_id: u64,
        parent_id: Option<u64>,
    },
    SceneKeyframe {
        object_id: u64,
        keyframe: Keyframe,
    },
    SceneRemove {
        object_id: u64,
    },
    Alignment {
        alignment: Alignment,
    },
    AlignmentRemove {
        id: u64,
    },
    CamOperation {
        profile: Profile,
    },
    CamRemove {
        id: u64,
    },
    CamSetup {
        settings: CamSettings,
    },
    Layout {
        layout: PrintLayout,
    },
    Building {
        building: Building,
    },
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CamSettings {
    pub tool_diameter: Option<f64>,
    pub depth: Option<f64>,
    pub step_down: Option<f64>,
    pub step_over: Option<f64>,
    pub safe_z: Option<f64>,
    pub feed: Option<f64>,
    pub plunge_feed: Option<f64>,
    pub spindle_rpm: Option<u32>,
    pub stock_min: Option<[f64; 3]>,
    pub stock_max: Option<[f64; 3]>,
}

/// Apply in order, validating after each edit. Forward references must be
/// authored after their targets. A failure retains the original entire project.
pub fn apply_edits(document: &mut Document, edits: &[ProjectEdit]) -> io::Result<()> {
    document.validate()?;
    if edits.is_empty() || edits.len() > 1000 {
        return Err(invalid("Project scripts require 1..1000 edits"));
    }
    let mut candidate = document.clone();
    for (index, edit) in edits.iter().enumerate() {
        apply_one(&mut candidate, edit)
            .and_then(|_| candidate.validate())
            .map_err(|error| {
                io::Error::new(error.kind(), format!("Project edit {}: {error}", index + 1))
            })?;
    }
    *document = candidate;
    Ok(())
}

fn apply_one(doc: &mut Document, edit: &ProjectEdit) -> io::Result<()> {
    match edit {
        ProjectEdit::SceneObject { object } => {
            if let Some(saved) = doc.scene.objects.iter_mut().find(|o| o.id == object.id) {
                *saved = object.clone();
            } else {
                doc.scene.objects.push(object.clone());
            }
        }
        ProjectEdit::SceneParent {
            object_id,
            parent_id,
        } => doc.scene.set_parent(*object_id, *parent_id)?,
        ProjectEdit::SceneKeyframe {
            object_id,
            keyframe,
        } => doc
            .scene
            .objects
            .iter_mut()
            .find(|o| o.id == *object_id)
            .ok_or_else(|| invalid("Scene object does not exist"))?
            .set_keyframe(keyframe.clone())?,
        ProjectEdit::SceneRemove { object_id } => {
            doc.scene.remove_subtree(*object_id)?;
        }
        ProjectEdit::Alignment { alignment } => {
            if let Some(saved) = doc
                .terrain
                .alignments
                .iter_mut()
                .find(|a| a.id == alignment.id)
            {
                *saved = alignment.clone();
            } else {
                doc.terrain.alignments.push(alignment.clone());
            }
        }
        ProjectEdit::AlignmentRemove { id } => {
            if !doc.terrain.alignments.iter().any(|a| a.id == *id) {
                return Err(invalid("Alignment does not exist"));
            }
            doc.terrain.alignments.retain(|a| a.id != *id);
        }
        ProjectEdit::CamOperation { profile } => {
            if let Some(saved) = doc
                .manufacturing
                .profiles
                .iter_mut()
                .find(|p| p.id == profile.id)
            {
                *saved = profile.clone();
            } else {
                doc.manufacturing.profiles.push(profile.clone());
            }
        }
        ProjectEdit::CamRemove { id } => {
            if !doc.manufacturing.profiles.iter().any(|p| p.id == *id) {
                return Err(invalid("CAM operation does not exist"));
            }
            doc.manufacturing.profiles.retain(|p| p.id != *id);
        }
        ProjectEdit::CamSetup { settings: s } => {
            let job = &mut doc.manufacturing;
            if let Some(v) = s.tool_diameter {
                job.tool_diameter = v;
            }
            if let Some(v) = s.depth {
                job.depth = v;
            }
            if let Some(v) = s.step_down {
                job.step_down = v;
            }
            if let Some(v) = s.step_over {
                job.step_over = v;
            }
            if let Some(v) = s.safe_z {
                job.safe_z = v;
            }
            if let Some(v) = s.feed {
                job.feed = v;
            }
            if let Some(v) = s.plunge_feed {
                job.plunge_feed = v;
            }
            if let Some(v) = s.spindle_rpm {
                job.spindle_rpm = v;
            }
            if let Some(v) = s.stock_min {
                job.stock_min = v;
            }
            if let Some(v) = s.stock_max {
                job.stock_max = v;
            }
        }
        ProjectEdit::Layout { layout } => {
            if let Some(saved) = doc
                .layouts
                .iter_mut()
                .find(|l| l.name.eq_ignore_ascii_case(&layout.name))
            {
                *saved = layout.clone();
            } else {
                doc.layouts.push(layout.clone());
            }
        }
        ProjectEdit::Building { building } => doc.building = building.clone(),
    }
    Ok(())
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}
