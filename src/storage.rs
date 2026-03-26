use std::error::Error;
use std::fmt::{self, Display, Formatter};

use crate::model::PaintDocument;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageError {
    NotImplemented(StorageFeature),
}

impl Display for StorageError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotImplemented(feature) => {
                write!(f, "{} is planned but not implemented yet", feature.label())
            }
        }
    }
}

impl Error for StorageError {}

#[derive(Debug, Default, Clone, Copy)]
pub struct StorageFacade;

impl StorageFacade {
    pub const fn new() -> Self {
        Self
    }

    pub fn save_document(&self, _document: &PaintDocument) -> Result<(), StorageError> {
        Err(StorageError::NotImplemented(
            StorageFeature::WorkingDocument,
        ))
    }

    pub fn load_document(&self) -> Result<PaintDocument, StorageError> {
        Err(StorageError::NotImplemented(
            StorageFeature::WorkingDocument,
        ))
    }

    pub const fn planned_edit_format(&self) -> &'static str {
        "Editable JSON or RON-based document file (.paint.json / .paint.ron)"
    }

    pub const fn planned_export_format(&self) -> &'static str {
        "PNG raster export"
    }

    pub const fn roadmap_summary(&self) -> &'static str {
        "Save/load buttons are placeholders for future local-file persistence."
    }
}

#[cfg(test)]
mod tests {
    use super::{StorageError, StorageFacade, StorageFeature};

    #[test]
    fn storage_stub_reports_working_document_todo() {
        let storage = StorageFacade::new();
        let error = storage
            .load_document()
            .expect_err("storage should be a TODO in the MVP");

        assert_eq!(
            error,
            StorageError::NotImplemented(StorageFeature::WorkingDocument)
        );
    }
}
