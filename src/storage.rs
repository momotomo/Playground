use std::error::Error;
use std::fmt::{self, Display, Formatter};
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};

use crate::model::{PaintDocument, PaintElement, Stroke};
use crate::render::render_document_png;
use serde::{Deserialize, Serialize};

const EDITABLE_FORMAT_ID: &str = "rust-paint-foundation/document";
const EDITABLE_FORMAT_VERSION: u32 = 4;
const PREVIOUS_EDITABLE_FORMAT_VERSION: u32 = 3;
const EARLIER_EDITABLE_FORMAT_VERSION: u32 = 2;
const LEGACY_EDITABLE_FORMAT_VERSION: u32 = 1;
const DEFAULT_FILE_NAME: &str = "untitled.paint.json";
const DEFAULT_PNG_FILE_NAME: &str = "untitled.png";
const JSON_EXTENSION: &str = "json";
const PNG_EXTENSION: &str = "png";

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
    Render(String),
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
            Self::Render(error) => write!(f, "Failed to render export: {error}"),
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
pub struct ExportedImage {
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

        let header: EditablePaintFileHeader = serde_json::from_slice(bytes)
            .map_err(|error| StorageError::Deserialize(error.to_string()))?;

        if header.format.id != EDITABLE_FORMAT_ID {
            return Err(StorageError::UnsupportedFormat(header.format.id));
        }

        match header.format.version {
            EDITABLE_FORMAT_VERSION => {
                let payload: EditablePaintFile = serde_json::from_slice(bytes)
                    .map_err(|error| StorageError::Deserialize(error.to_string()))?;
                Ok(payload.document.sanitized())
            }
            PREVIOUS_EDITABLE_FORMAT_VERSION | EARLIER_EDITABLE_FORMAT_VERSION => {
                let payload: FlatEditablePaintFile = serde_json::from_slice(bytes)
                    .map_err(|error| StorageError::Deserialize(error.to_string()))?;
                Ok(payload.document.into_current())
            }
            LEGACY_EDITABLE_FORMAT_VERSION => {
                let payload: LegacyEditablePaintFile = serde_json::from_slice(bytes)
                    .map_err(|error| StorageError::Deserialize(error.to_string()))?;
                Ok(payload.document.into_current())
            }
            version => Err(StorageError::UnsupportedVersion(version)),
        }
    }

    pub fn export_png_bytes(&self, document: &PaintDocument) -> Result<Vec<u8>, StorageError> {
        render_document_png(document).map_err(|error| StorageError::Render(error.to_string()))
    }

    pub const fn suggested_file_name(&self) -> &'static str {
        DEFAULT_FILE_NAME
    }

    pub fn suggested_png_file_name(&self, document_name: &str) -> String {
        to_png_file_name(document_name)
    }

    pub const fn editable_format_label(&self) -> &'static str {
        "Editable JSON envelope v4 (.paint.json)"
    }

    pub const fn planned_export_format(&self) -> &'static str {
        "PNG raster export (.png)"
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
    pub fn export_png_to_path<P: AsRef<Path>>(
        &self,
        document: &PaintDocument,
        path: P,
    ) -> Result<ExportedImage, StorageError> {
        let bytes = self.export_png_bytes(document)?;
        let path = ensure_png_file_name(path.as_ref().to_path_buf());
        std::fs::write(&path, bytes).map_err(|error| StorageError::Io(error.to_string()))?;

        Ok(ExportedImage {
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
    pub fn export_png_via_dialog(
        &self,
        document: &PaintDocument,
        suggested_name: &str,
    ) -> Result<ExportedImage, StorageError> {
        let path = rfd::FileDialog::new()
            .set_title("Export PNG")
            .set_file_name(suggested_name)
            .add_filter("PNG image", &[PNG_EXTENSION])
            .save_file()
            .ok_or(StorageError::Cancelled)?;

        self.export_png_to_path(document, path)
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
            file_name: normalize_file_name(file.file_name(), DEFAULT_FILE_NAME),
        })
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn export_png_via_dialog(
        &self,
        document: &PaintDocument,
        suggested_name: &str,
    ) -> Result<ExportedImage, StorageError> {
        let bytes = self.export_png_bytes(document)?;
        let file = rfd::AsyncFileDialog::new()
            .set_title("Export PNG")
            .set_file_name(suggested_name)
            .add_filter("PNG image", &[PNG_EXTENSION])
            .save_file()
            .await
            .ok_or(StorageError::Cancelled)?;

        file.write(&bytes)
            .await
            .map_err(|error| StorageError::Io(error.to_string()))?;

        Ok(ExportedImage {
            file_name: normalize_file_name(file.file_name(), DEFAULT_PNG_FILE_NAME),
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
            file_name: normalize_file_name(file.file_name(), DEFAULT_FILE_NAME),
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct EditablePaintFileHeader {
    format: FileFormatDescriptor,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct FlatEditablePaintFile {
    format: FileFormatDescriptor,
    #[serde(default)]
    metadata: FileMetadata,
    document: PaintDocumentV3,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct PaintDocumentV3 {
    canvas_size: crate::model::CanvasSize,
    background: crate::model::RgbaColor,
    #[serde(default)]
    elements: Vec<PaintElement>,
}

impl PaintDocumentV3 {
    fn into_current(self) -> PaintDocument {
        PaintDocument::from_flat_elements(self.canvas_size, self.background, self.elements)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct LegacyEditablePaintFile {
    format: FileFormatDescriptor,
    #[serde(default)]
    metadata: FileMetadata,
    document: PaintDocumentV1,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct PaintDocumentV1 {
    canvas_size: crate::model::CanvasSize,
    background: crate::model::RgbaColor,
    strokes: Vec<Stroke>,
}

impl PaintDocumentV1 {
    fn into_current(self) -> PaintDocument {
        PaintDocument::from_flat_elements(
            self.canvas_size,
            self.background,
            self.strokes.into_iter().map(PaintElement::Stroke).collect(),
        )
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn ensure_json_file_name(path: PathBuf) -> PathBuf {
    if file_name_from_path(&path).ends_with(".json") {
        path
    } else {
        path.with_extension("paint.json")
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn ensure_png_file_name(path: PathBuf) -> PathBuf {
    if file_name_from_path(&path).ends_with(".png") {
        path
    } else {
        path.with_extension(PNG_EXTENSION)
    }
}

fn to_png_file_name(document_name: &str) -> String {
    if let Some(stripped) = document_name.strip_suffix(".paint.json") {
        format!("{stripped}.png")
    } else if let Some(stripped) = document_name.strip_suffix(".json") {
        format!("{stripped}.png")
    } else if let Some(stripped) = document_name.strip_suffix(".png") {
        format!("{stripped}.png")
    } else if document_name.is_empty() {
        DEFAULT_PNG_FILE_NAME.to_owned()
    } else {
        format!("{document_name}.png")
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn file_name_from_path(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .map_or_else(|| DEFAULT_FILE_NAME.to_owned(), ToOwned::to_owned)
}

#[cfg(target_arch = "wasm32")]
fn normalize_file_name(file_name: String, fallback: &str) -> String {
    if file_name.is_empty() {
        fallback.to_owned()
    } else {
        file_name
    }
}

#[cfg(test)]
mod tests {
    use super::{StorageError, StorageFacade};
    use crate::model::{
        CanvasSize, GroupElement, PaintDocument, PaintElement, PaintPoint, RgbaColor, ShapeElement,
        ShapeKind, Stroke, ToolKind,
    };
    #[cfg(not(target_arch = "wasm32"))]
    use std::time::{SystemTime, UNIX_EPOCH};
    use tiny_skia::Pixmap;

    fn sample_document() -> PaintDocument {
        let mut document = PaintDocument {
            canvas_size: CanvasSize::new(64.0, 32.0),
            background: RgbaColor::white(),
            ..PaintDocument::default()
        };

        let mut stroke = Stroke::new(ToolKind::Brush, RgbaColor::default(), 6.0);
        stroke.push_point(PaintPoint::new(4.0, 4.0));
        stroke.push_point(PaintPoint::new(28.0, 12.0));
        document.push_stroke(stroke);
        document.push_shape(ShapeElement::with_rotation(
            ShapeKind::Rectangle,
            RgbaColor::new(220, 64, 64, 255),
            3.0,
            PaintPoint::new(36.0, 6.0),
            PaintPoint::new(58.0, 26.0),
            0.45,
        ));
        document
    }

    fn grouped_document() -> PaintDocument {
        PaintDocument::from_flat_elements(
            CanvasSize::new(96.0, 64.0),
            RgbaColor::white(),
            vec![PaintElement::Group(GroupElement {
                elements: vec![
                    PaintElement::Stroke({
                        let mut stroke = Stroke::new(ToolKind::Brush, RgbaColor::charcoal(), 6.0);
                        stroke.push_point(PaintPoint::new(10.0, 10.0));
                        stroke.push_point(PaintPoint::new(30.0, 24.0));
                        stroke
                    }),
                    PaintElement::Shape(ShapeElement::with_rotation(
                        ShapeKind::Ellipse,
                        RgbaColor::new(220, 64, 64, 255),
                        4.0,
                        PaintPoint::new(38.0, 12.0),
                        PaintPoint::new(68.0, 40.0),
                        0.35,
                    )),
                ],
            })],
        )
    }

    fn layered_document() -> PaintDocument {
        let document = sample_document();
        let (mut next, top_layer_id) = document.add_layer_document();
        next.push_shape(ShapeElement::new(
            ShapeKind::Ellipse,
            RgbaColor::new(64, 96, 220, 255),
            3.0,
            PaintPoint::new(20.0, 6.0),
            PaintPoint::new(30.0, 18.0),
        ));
        next = next
            .toggled_layer_locked_document(top_layer_id)
            .expect("lock top layer");
        next
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
    fn shape_rotation_survives_round_trip() {
        let storage = StorageFacade::new();
        let encoded = storage
            .encode_document(&sample_document())
            .expect("must encode");
        let decoded = storage.decode_document(&encoded).expect("must decode");

        let Some(crate::model::PaintElement::Shape(shape)) = decoded.element(1) else {
            panic!("second element should be a shape");
        };

        assert!((shape.rotation_radians - 0.45).abs() < 0.0001);
    }

    #[test]
    fn group_round_trip_preserves_nested_elements() {
        let storage = StorageFacade::new();
        let encoded = storage
            .encode_document(&grouped_document())
            .expect("must encode");
        let decoded = storage.decode_document(&encoded).expect("must decode");

        assert_eq!(decoded, grouped_document());
    }

    #[test]
    fn layer_round_trip_preserves_visibility_and_lock_state() {
        let storage = StorageFacade::new();
        let encoded = storage
            .encode_document(&layered_document())
            .expect("must encode");
        let decoded = storage.decode_document(&encoded).expect("must decode");

        assert_eq!(decoded, layered_document());
        assert_eq!(decoded.layer_count(), 2);
    }

    #[test]
    fn layer_transfer_round_trip_preserves_destination_elements() {
        let storage = StorageFacade::new();
        let document = sample_document();
        let source_layer_id = document.active_layer_id();
        let (mut document, destination_layer_id) = document.add_layer_document();
        assert!(document.set_active_layer(source_layer_id));

        let (duplicated, _) = document
            .duplicated_selection_to_layer_document(&[0, 1], destination_layer_id)
            .expect("duplicate should succeed");
        let encoded = storage.encode_document(&duplicated).expect("must encode");
        let decoded = storage.decode_document(&encoded).expect("must decode");

        assert_eq!(decoded, duplicated);
        assert_eq!(
            decoded.layer(destination_layer_id).unwrap().elements.len(),
            2
        );
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
                br#"{"format":{"id":"another-app","version":2},"metadata":{},"document":{"canvas_size":{"width":1600.0,"height":900.0},"background":{"r":255,"g":255,"b":255,"a":255},"elements":[]}}"#,
            )
            .expect_err("wrong format id should fail");

        assert_eq!(
            error,
            StorageError::UnsupportedFormat(String::from("another-app"))
        );
    }

    #[test]
    fn decode_legacy_v1_document() {
        let storage = StorageFacade::new();
        let legacy = br#"{
          "format":{"id":"rust-paint-foundation/document","version":1},
          "metadata":{},
          "document":{
            "canvas_size":{"width":64.0,"height":32.0},
            "background":{"r":255,"g":255,"b":255,"a":255},
            "strokes":[
              {
                "tool":"brush",
                "color":{"r":37,"g":37,"b":41,"a":255},
                "width":6.0,
                "points":[{"x":4.0,"y":4.0},{"x":28.0,"y":12.0}]
              }
            ]
          }
        }"#;

        let decoded = storage
            .decode_document(legacy)
            .expect("legacy should decode");
        assert_eq!(decoded.element_count(), 1);
    }

    #[test]
    fn decode_previous_v2_document_without_groups() {
        let storage = StorageFacade::new();
        let previous = br#"{
          "format":{"id":"rust-paint-foundation/document","version":2},
          "metadata":{},
          "document":{
            "canvas_size":{"width":64.0,"height":32.0},
            "background":{"r":255,"g":255,"b":255,"a":255},
            "elements":[
              {
                "element_type":"shape",
                "kind":"rectangle",
                "color":{"r":220,"g":64,"b":64,"a":255},
                "width":3.0,
                "start":{"x":12.0,"y":8.0},
                "end":{"x":40.0,"y":24.0},
                "rotation_radians":0.2
              }
            ]
          }
        }"#;

        let decoded = storage
            .decode_document(previous)
            .expect("previous version should decode");

        assert_eq!(decoded.element_count(), 1);
    }

    #[test]
    fn decode_previous_v3_flat_document() {
        let storage = StorageFacade::new();
        let previous = br#"{
          "format":{"id":"rust-paint-foundation/document","version":3},
          "metadata":{},
          "document":{
            "canvas_size":{"width":64.0,"height":32.0},
            "background":{"r":255,"g":255,"b":255,"a":255},
            "elements":[
              {
                "element_type":"group",
                "elements":[
                  {
                    "element_type":"shape",
                    "kind":"rectangle",
                    "color":{"r":220,"g":64,"b":64,"a":255},
                    "width":3.0,
                    "start":{"x":12.0,"y":8.0},
                    "end":{"x":40.0,"y":24.0},
                    "rotation_radians":0.2
                  }
                ]
              }
            ]
          }
        }"#;

        let decoded = storage
            .decode_document(previous)
            .expect("previous version should decode");

        assert_eq!(decoded.layer_count(), 1);
        assert_eq!(decoded.element_count(), 1);
    }

    #[test]
    fn export_png_bytes_can_be_decoded() {
        let storage = StorageFacade::new();
        let bytes = storage
            .export_png_bytes(&sample_document())
            .expect("png export should succeed");
        let pixmap = Pixmap::decode_png(&bytes).expect("png bytes should decode");

        assert_eq!(pixmap.width(), 64);
        assert_eq!(pixmap.height(), 32);
    }

    #[test]
    fn serialized_document_can_render_to_png() {
        let storage = StorageFacade::new();
        let encoded = storage
            .encode_document(&sample_document())
            .expect("must encode");
        let decoded = storage.decode_document(&encoded).expect("must decode");
        let png = storage.export_png_bytes(&decoded).expect("must render");
        let pixmap = Pixmap::decode_png(&png).expect("png should decode");
        let found_shape_pixel = (30..60).any(|x| {
            (4..28).any(|y| {
                let pixel = pixmap
                    .pixel(x, y)
                    .expect("shape pixel should exist")
                    .demultiply();
                (pixel.red(), pixel.green(), pixel.blue()) != (255, 255, 255)
            })
        });

        assert!(found_shape_pixel, "expected a non-background shape pixel");
    }

    #[test]
    fn grouped_document_can_render_to_png() {
        let storage = StorageFacade::new();
        let png = storage
            .export_png_bytes(&grouped_document())
            .expect("group png export should succeed");
        let pixmap = Pixmap::decode_png(&png).expect("png should decode");
        let found_non_background = (0..pixmap.width()).any(|x| {
            (0..pixmap.height()).any(|y| {
                let pixel = pixmap.pixel(x, y).expect("pixel should exist").demultiply();
                (pixel.red(), pixel.green(), pixel.blue()) != (255, 255, 255)
            })
        });

        assert!(found_non_background, "group should render into png export");
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
