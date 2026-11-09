use std::{
    io,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub(crate) enum PendingAction {
    New,
    Open(PathBuf),
    Close,
}

pub(crate) fn native_save_path(path: &str) -> io::Result<PathBuf> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Choose a project filename before saving.",
        ));
    }
    if trimmed.to_ascii_lowercase().ends_with(".forge.json") {
        return Ok(PathBuf::from(trimmed));
    }
    let path = Path::new(trimmed);
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match extension.as_str() {
        "" => Ok(PathBuf::from(format!("{trimmed}.forge.json"))),
        "json" | "dxf" | "stl" | "obj" => Ok(path.with_extension("forge.json")),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Native project saves require .forge.json. Exchange files use Export.",
        )),
    }
}

pub(crate) fn export_path(path: &str, extension: &str) -> PathBuf {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".forge.json") {
        PathBuf::from(format!(
            "{}.{}",
            &path[..path.len() - ".forge.json".len()],
            extension
        ))
    } else {
        Path::new(path).with_extension(extension)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imported_exchange_file_is_never_native_save_destination() {
        for extension in ["dxf", "DXF", "stl", "obj"] {
            let source = format!("/projects/component.{extension}");
            assert_eq!(
                native_save_path(&source).unwrap(),
                PathBuf::from("/projects/component.forge.json")
            );
            assert_ne!(native_save_path(&source).unwrap(), PathBuf::from(source));
        }
    }

    #[test]
    fn native_paths_and_export_stems_are_preserved() {
        assert_eq!(
            native_save_path("design.forge.json").unwrap(),
            PathBuf::from("design.forge.json")
        );
        assert_eq!(
            native_save_path("design").unwrap(),
            PathBuf::from("design.forge.json")
        );
        assert_eq!(
            export_path("/projects/design.forge.json", "dxf"),
            PathBuf::from("/projects/design.dxf")
        );
        assert_eq!(
            export_path("/projects/design.dxf", "stl"),
            PathBuf::from("/projects/design.stl")
        );
    }

    #[test]
    fn unsupported_and_empty_native_destinations_are_rejected() {
        for path in ["", "  ", "model.rvt", "drawing.dwg", "project.max"] {
            assert_eq!(
                native_save_path(path).unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            );
        }
    }
}
