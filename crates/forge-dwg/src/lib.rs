//! Native DWG database I/O using the unmodified acadrust engine.
//!
//! This retains its native document model instead of routing a rewrite through
//! Forge's smaller display model. Supported tests do not establish full AutoCAD
//! parity. Engine diagnostics remain visible and severe diagnostics block saves.
use acadrust::io::dwg::DwgReadOptions;
use acadrust::notification::NotificationType;
use acadrust::{CadDocument, DwgReader, DwgWriter, DxfReader, DxfVersion, DxfWriter};
use acadrust::{
    entities::EntityType,
    types::{Handle, Vector3},
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, Cursor},
    panic::{AssertUnwindSafe, catch_unwind},
};

const MAX_FILE_BYTES: usize = 128 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeVersion {
    R2000,
    R2018,
}
impl NativeVersion {
    fn engine(self) -> DxfVersion {
        match self {
            Self::R2000 => DxfVersion::AC1015,
            Self::R2018 => DxfVersion::AC1032,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct EntitySummary {
    pub handle: u64,
    pub kind: String,
    pub layer: String,
}
#[derive(Debug, Serialize)]
pub struct DrawingSummary {
    pub engine: String,
    pub version: String,
    pub entity_count: usize,
    pub entity_types: BTreeMap<String, usize>,
    pub layers: Vec<String>,
    pub blocks: Vec<String>,
    pub entities: Vec<EntitySummary>,
    pub diagnostics: Vec<String>,
}

pub struct NativeDrawing {
    document: CadDocument,
}
impl NativeDrawing {
    pub fn from_dwg(bytes: &[u8]) -> io::Result<Self> {
        check_input(bytes)?;
        guarded(|| {
            let options = DwgReadOptions::default()
                .with_mesh_limits(2_000_000, 8_000_000)
                .with_max_decompressed_bytes(256 * 1024 * 1024);
            let mut reader = DwgReader::from_stream_with_options(Cursor::new(bytes), options);
            reader
                .read()
                .map(|document| Self { document })
                .map_err(engine_error)
        })
    }
    pub fn from_dxf(bytes: &[u8]) -> io::Result<Self> {
        check_input(bytes)?;
        guarded(|| {
            DxfReader::from_reader(Cursor::new(bytes.to_vec()))
                .and_then(|reader| reader.read())
                .map(|document| Self { document })
                .map_err(engine_error)
        })
    }
    pub fn summary(&self) -> DrawingSummary {
        let mut types = BTreeMap::new();
        let entities = self
            .document
            .entities()
            .map(|entity| {
                let kind = entity.as_entity().entity_type().to_owned();
                *types.entry(kind.clone()).or_default() += 1;
                EntitySummary {
                    handle: entity.common().handle.value(),
                    kind,
                    layer: entity.common().layer.clone(),
                }
            })
            .collect();
        DrawingSummary {
            engine: "acadrust 0.6.8 (MPL-2.0)".into(),
            version: format!("{:?}", self.document.version),
            entity_count: self.document.entity_count(),
            entity_types: types,
            layers: self
                .document
                .layers
                .iter()
                .map(|layer| layer.name.clone())
                .collect(),
            blocks: self
                .document
                .block_records
                .iter()
                .map(|block| block.name.clone())
                .collect(),
            entities,
            diagnostics: self
                .document
                .notifications
                .iter()
                .map(ToString::to_string)
                .chain((self.document.notifications.omitted_count() > 0).then(|| {
                    format!(
                        "{} additional diagnostics omitted by engine",
                        self.document.notifications.omitted_count()
                    )
                }))
                .collect(),
        }
    }
    pub fn write_dwg(&self, target: Option<NativeVersion>) -> io::Result<Vec<u8>> {
        self.require_saveable()?;
        guarded(|| {
            let mut document = self.document.clone();
            if let Some(version) = target {
                if document.has_version_locked_data(version.engine()) {
                    return Err(unsupported(
                        "Document has version-locked native data; version conversion refused",
                    ));
                }
                document.version = version.engine();
            }
            let bytes = DwgWriter::write_to_vec(&document).map_err(engine_error)?;
            let reread = Self::from_dwg(&bytes)?;
            reread.require_saveable()?;
            let before = Self { document }.summary();
            let after = reread.summary();
            if before.entity_types != after.entity_types
                || before.layers != after.layers
                || before.blocks != after.blocks
            {
                return Err(unsupported(
                    "Native DWG read-back inventory changed; output was not accepted",
                ));
            }
            Ok(bytes)
        })
    }
    pub fn write_dxf(&self) -> io::Result<Vec<u8>> {
        self.require_saveable()?;
        guarded(|| {
            DxfWriter::new(&self.document)
                .write_to_vec()
                .map_err(engine_error)
        })
    }
    /// Translate only explicitly supported native primitives, preserving the
    /// remaining database. The transaction is validated before mutation.
    pub fn translate_primitives(&mut self, handles: &[u64], delta: [f64; 3]) -> io::Result<usize> {
        if delta.iter().any(|v| !v.is_finite()) {
            return Err(invalid("Translation must be finite"));
        }
        let requested: BTreeSet<u64> = handles.iter().copied().collect();
        for handle in &requested {
            let entity = self
                .document
                .get_entity(Handle::new(*handle))
                .ok_or_else(|| invalid(format!("Native entity handle {handle:X} not found")))?;
            if !matches!(
                entity,
                EntityType::Line(_)
                    | EntityType::Circle(_)
                    | EntityType::Arc(_)
                    | EntityType::Point(_)
            ) {
                return Err(unsupported(format!(
                    "Native translation is not implemented for {}",
                    entity.as_entity().entity_type()
                )));
            }
        }
        let mut candidate = self.document.clone();
        for handle in &requested {
            candidate
                .get_entity_mut(Handle::new(*handle))
                .expect("validated handle")
                .translate(Vector3::new(delta[0], delta[1], delta[2]));
        }
        self.document = candidate;
        Ok(requested.len())
    }
    fn require_saveable(&self) -> io::Result<()> {
        if self.document.notifications.omitted_count() > 0
            || self.document.notifications.iter().any(|notice| {
                matches!(
                    notice.notification_type,
                    NotificationType::Error
                        | NotificationType::NotImplemented
                        | NotificationType::NotSupported
                )
            })
        {
            return Err(unsupported(format!(
                "Native engine reported unsupported/recovered content; save refused: {}",
                self.summary().diagnostics.join("; ")
            )));
        }
        Ok(())
    }
}

fn check_input(bytes: &[u8]) -> io::Result<()> {
    if bytes.is_empty() {
        return Err(invalid("Input file is empty"));
    }
    if bytes.len() > MAX_FILE_BYTES {
        return Err(invalid("Native input exceeds the 128 MiB supported limit"));
    }
    Ok(())
}
fn guarded<T>(operation: impl FnOnce() -> io::Result<T>) -> io::Result<T> {
    catch_unwind(AssertUnwindSafe(operation))
        .map_err(|_| invalid("Native CAD engine rejected input with an internal failure"))?
}
fn engine_error(error: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}
fn unsupported(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    const SOURCE: &str = "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1015\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nLINE\n8\n0\n10\n1.25\n20\n2.5\n30\n3.75\n11\n10.5\n21\n20.25\n31\n30.75\n0\nCIRCLE\n8\n0\n10\n20\n20\n30\n30\n0\n40\n5\n0\nENDSEC\n0\nEOF\n";
    #[test]
    fn native_dwg_2000_and_2018_roundtrip_preserves_primitives() {
        let source = NativeDrawing::from_dxf(SOURCE.as_bytes()).unwrap();
        for version in [NativeVersion::R2000, NativeVersion::R2018] {
            let bytes = source.write_dwg(Some(version)).unwrap();
            assert_eq!(
                &bytes[..6],
                if version == NativeVersion::R2000 {
                    b"AC1015"
                } else {
                    b"AC1032"
                }
            );
            let read = NativeDrawing::from_dwg(&bytes).unwrap();
            assert_eq!(read.summary().entity_types, source.summary().entity_types);
            let line = read
                .document
                .entities()
                .find_map(|e| {
                    if let EntityType::Line(line) = e {
                        Some(line)
                    } else {
                        None
                    }
                })
                .unwrap();
            assert_eq!(line.start, Vector3::new(1.25, 2.5, 3.75));
            assert_eq!(line.end, Vector3::new(10.5, 20.25, 30.75));
            let circle = read
                .document
                .entities()
                .find_map(|e| {
                    if let EntityType::Circle(circle) = e {
                        Some(circle)
                    } else {
                        None
                    }
                })
                .unwrap();
            assert_eq!(circle.radius, 5.0);
        }
    }
    #[test]
    fn native_edit_preserves_unselected_geometry_and_rejects_partial_transactions() {
        let mut drawing = NativeDrawing::from_dxf(SOURCE.as_bytes()).unwrap();
        let summary = drawing.summary();
        let handle = summary
            .entities
            .iter()
            .find(|e| e.kind == "LINE")
            .unwrap()
            .handle;
        assert!(
            drawing
                .translate_primitives(&[handle, u64::MAX], [10.0, 20.0, 30.0])
                .is_err()
        );
        assert_eq!(
            drawing
                .translate_primitives(&[handle], [10.0, 20.0, 30.0])
                .unwrap(),
            1
        );
        let reread = NativeDrawing::from_dwg(&drawing.write_dwg(None).unwrap()).unwrap();
        let line = reread
            .document
            .entities()
            .find_map(|e| {
                if let EntityType::Line(line) = e {
                    Some(line)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(line.start, Vector3::new(11.25, 22.5, 33.75));
        let circle = reread
            .document
            .entities()
            .find_map(|e| {
                if let EntityType::Circle(circle) = e {
                    Some(circle)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(circle.center, Vector3::new(20.0, 30.0, 0.0));
    }
    #[test]
    fn malformed_native_files_return_errors() {
        for source in [
            b"".as_slice(),
            b"AC1032",
            b"AC1015 broken",
            b"not a drawing",
        ] {
            assert!(NativeDrawing::from_dwg(source).is_err());
        }
    }
}
