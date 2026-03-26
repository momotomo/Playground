use std::error::Error;
use std::fmt::{self, Display, Formatter};
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};

use crate::model::PaintDocument;
use serde::{Deserialize, Serialize};

const EDITABLE_FORMAT_ID: &str = "rust-paint-foundation/document";
const EDITABLE_FORMAT_VERSION: u32 = 1;
const DEFAULT_FILE_NAME: &str = "untitled.paint.json";
const JSON_EXTENSION: &str = "json";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageFeature {
    WorkingDocument,
    PngExport,
}

impl StorageFeature {
    pub const fn label(self) -> &'static str {
        match self {
            Self::WorkingDocument => "editable local document format",
            Self::PngExport => "PNG export",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageError {
    Cancelled,
    EmptyFile,
    UnsupportedFormat(String),
    UnsupportedVersion(u32),
    Serialize(String),
    Deserialize(String),
    Io(String),
}

impl Display for StorageError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => write!(f, "The file dialog was cancelled"),
            Self::EmptyFile => write!(f, "The selected file was empty"),
            Self::UnsupportedFormat(found) => {
                write!(f, "Unsupported paint document format: {found}")
            }
            Self::UnsupportedVersion(version) => {
                write!(f, "Unsupported paint document version: {version}")
            }
            Self::Serialize(error) => write!(f, "Failed to serialize document: {error}"),
            Self::Deserialize(error) => write!(f, "Failed to read document: {error}"),
            Self::Io(error) => write!(f, "File I/O failed: {error}"),
        }
    }
}

impl Error for StorageError {}

#[derive(Debug, Clone, PartialEq)]
pub struct SavedDocument {
    pub file_name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoadedDocument {
    pub file_name: String,
    pub document: PaintDocument,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct StorageFacade;

impl StorageFacade {
    pub const fn new() -> Self {
        Self
    }

    pub fn encode_document(&self, document: &PaintDocument) -> Result<Vec<u8>, StorageError> {
        let payload = EditablePaintFile::from_document(document.clone());
        serde_json::to_vec_pretty(&payload)
            .map_err(|error| StorageError::Serialize(error.to_string()))
    }

    pub fn decode_document(&self, bytes: &[u8]) -> Result<PaintDocument, StorageError> {
        if bytes.is_empty() {
            return Err(StorageError::EmptyFile);
        }

        let payload: EditablePaintFile = serde_json::from_slice(bytes)
            .map_err(|error| StorageError::Deserialize(error.to_string()))?;

        if payload.format.id != EDITABLE_FORMAT_ID {
            return Err(StorageError::UnsupportedFormat(payload.format.id));
        }

        if payload.format.version != EDITABLE_FORMAT_VERSION {
            return Err(StorageError::UnsupportedVersion(payload.format.version));
        }

        Ok(payload.document)
    }

    pub const fn suggested_file_name(&self) -> &'static str {
        DEFAULT_FILE_NAME
    }

    pub const fn editable_format_label(&self) -> &'static str {
        "Editable JSON envelope (.paint.json)"
    }

    pub const fn planned_export_format(&self) -> &'static str {
        "PNG raster export (planned)"
    }

    pub const fn storage_strategy_summary(&self) -> &'static str {
        "Native uses file dialogs. Web uses browser download/upload."
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn save_document_to_path<P: AsRef<Path>>(
        &self,
        document: &PaintDocument,
        path: P,
    ) -> Result<SavedDocument, StorageError> {
        let bytes = self.encode_document(document)?;
        let path = ensure_json_file_name(path.as_ref().to_path_buf());
        std::fs::write(&path, bytes).map_err(|error| StorageError::Io(error.to_string()))?;

        Ok(SavedDocument {
            file_name: file_name_from_path(&path),
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_document_from_path<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Result<LoadedDocument, StorageError> {
        let path = path.as_ref().to_path_buf();
        let bytes = std::fs::read(&path).map_err(|error| StorageError::Io(error.to_string()))?;
        let document = self.decode_document(&bytes)?;

        Ok(LoadedDocument {
            file_name: file_name_from_path(&path),
            document,
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn save_document_via_dialog(
        &self,
        document: &PaintDocument,
        suggested_name: &str,
    ) -> Result<SavedDocument, StorageError> {
        let path = rfd::FileDialog::new()
            .set_title("Save drawing")
            .set_file_name(suggested_name)
            .add_filter("Rust Paint Document", &[JSON_EXTENSION])
            .save_file()
            .ok_or(StorageError::Cancelled)?;

        self.save_document_to_path(document, path)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_document_via_dialog(&self) -> Result<LoadedDocument, StorageError> {
        let path = rfd::FileDialog::new()
            .set_title("Load drawing")
            .add_filter("Rust Paint Document", &[JSON_EXTENSION])
            .pick_file()
            .ok_or(StorageError::Cancelled)?;

        self.load_document_from_path(path)
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn save_document_via_dialog(
        &self,
        document: &PaintDocument,
        suggested_name: &str,
    ) -> Result<SavedDocument, StorageError> {
        let bytes = self.encode_document(document)?;
        let file = rfd::AsyncFileDialog::new()
            .set_title("Save drawing")
            .set_file_name(suggested_name)
            .add_filter("Rust Paint Document", &[JSON_EXTENSION])
            .save_file()
            .await
            .ok_or(StorageError::Cancelled)?;

        file.write(&bytes)
            .await
            .map_err(|error| StorageError::Io(error.to_string()))?;

        Ok(SavedDocument {
            file_name: normalize_file_name(file.file_name()),
        })
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn load_document_via_dialog(&self) -> Result<LoadedDocument, StorageError> {
        let file = rfd::AsyncFileDialog::new()
            .set_title("Load drawing")
            .add_filter("Rust Paint Document", &[JSON_EXTENSION])
            .pick_file()
            .await
            .ok_or(StorageError::Cancelled)?;

        let bytes = file.read().await;
        let document = self.decode_document(&bytes)?;

        Ok(LoadedDocument {
            file_name: normalize_file_name(file.file_name()),
            document,
        })
    }

    pub const fn planned_edit_format(&self) -> &'static str {
        self.editable_format_label()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct FileFormatDescriptor {
    id: String,
    version: u32,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct FileMetadata {
    #[serde(default)]
    title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct EditablePaintFile {
    format: FileFormatDescriptor,
    #[serde(default)]
    metadata: FileMetadata,
    document: PaintDocument,
}

impl EditablePaintFile {
    fn from_document(document: PaintDocument) -> Self {
        Self {
            format: FileFormatDescriptor {
                id: EDITABLE_FORMAT_ID.to_owned(),
                version: EDITABLE_FORMAT_VERSION,
            },
            metadata: FileMetadata::default(),
            document,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn ensure_json_file_name(path: PathBuf) -> PathBuf {
    let file_name = file_name_from_path(&path);
    if file_name.ends_with(".json") {
        path
    } else {
        path.with_extension("paint.json")
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn file_name_from_path(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .map_or_else(|| DEFAULT_FILE_NAME.to_owned(), ToOwned::to_owned)
}

#[cfg(target_arch = "wasm32")]
fn normalize_file_name(file_name: String) -> String {
    if file_name.is_empty() {
        DEFAULT_FILE_NAME.to_owned()
    } else {
        file_name
    }
}

#[cfg(test)]
mod tests {
    use super::{StorageError, StorageFacade};
    use crate::model::{PaintDocument, PaintPoint, RgbaColor, Stroke, ToolKind};
    #[cfg(not(target_arch = "wasm32"))]
    use std::time::{SystemTime, UNIX_EPOCH};

    fn sample_document() -> PaintDocument {
        let mut document = PaintDocument::default();
        let mut stroke = Stroke::new(ToolKind::Brush, RgbaColor::default(), 6.0);
        stroke.push_point(PaintPoint::new(4.0, 4.0));
        stroke.push_point(PaintPoint::new(12.0, 12.0));
        document.push_stroke(stroke);
        document
    }

    #[test]
    fn serialize_round_trip_preserves_document() {
        let storage = StorageFacade::new();
        let document = sample_document();
        let encoded = storage.encode_document(&document).expect("must encode");
        let decoded = storage.decode_document(&encoded).expect("must decode");

        assert_eq!(decoded, document);
    }

    #[test]
    fn decode_rejects_broken_json() {
        let storage = StorageFacade::new();
        let error = storage
            .decode_document(
                br#"{"format":{"id":"rust-paint-foundation/document","version":"oops"}}"#,
            )
            .expect_err("invalid json should fail");

        assert!(matches!(error, StorageError::Deserialize(_)));
    }

    #[test]
    fn decode_rejects_wrong_format_id() {
        let storage = StorageFacade::new();
        let error = storage
            .decode_document(
                br#"{"format":{"id":"another-app","version":1},"metadata":{},"document":{"canvas_size":{"width":1600.0,"height":900.0},"background":{"r":255,"g":255,"b":255,"a":255},"strokes":[]}}"#,
            )
            .expect_err("wrong format id should fail");

        assert_eq!(
            error,
            StorageError::UnsupportedFormat(String::from("another-app"))
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn path_round_trip_preserves_document() {
        let storage = StorageFacade::new();
        let document = sample_document();

        let mut path = std::env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        path.push(format!("rust_paint_foundation_test_{unique}.paint.json"));

        storage
            .save_document_to_path(&document, &path)
            .expect("must save");
        let loaded = storage
            .load_document_from_path(&path)
            .expect("must load the same file");

        assert_eq!(loaded.document, document);
        let _ = std::fs::remove_file(path);
    }
}
