//! Loading PDFium and opening documents.
//!
//! The library is not bundled in the crate: it is a shared library (`pdfium.dll`,
//! `libpdfium.dylib`) that must sit next to the executable or in a configured folder.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use mpt_core::ConversionError;
use pdfium_render::prelude::*;

/// Folders where the PDFium shared library is looked for, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfiumLocation {
    dirs: Vec<PathBuf>,
    system_fallback: bool,
}

impl Default for PdfiumLocation {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl PdfiumLocation {
    /// Searches exactly these folders, then the system library path.
    pub fn new(dirs: Vec<PathBuf>) -> Self {
        Self { dirs, system_fallback: true }
    }

    /// `extra` folders first, then `$PDFIUM_DIR`, the executable's folder and the usual
    /// packaging layouts relative to it (`pdfium/`, `../Frameworks` inside a macOS app).
    pub fn standard(extra: Vec<PathBuf>) -> Self {
        let mut dirs = extra;
        if let Some(dir) = std::env::var_os("PDFIUM_DIR") {
            dirs.push(PathBuf::from(dir));
        }
        if let Some(exe_dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)) {
            dirs.push(exe_dir.clone());
            dirs.push(exe_dir.join("pdfium"));
            dirs.push(exe_dir.join("../Frameworks"));
            dirs.push(exe_dir.join("../Resources"));
        }
        Self::new(dirs)
    }

    /// Do not fall back to a PDFium installed system-wide: only the listed folders count.
    pub fn without_system_library(mut self) -> Self {
        self.system_fallback = false;
        self
    }

    pub fn dirs(&self) -> &[PathBuf] {
        &self.dirs
    }
}

/// PDFium can only be bound once per process, so the loaded engine is shared.
/// (`pdfium-render` serializes calls into the library itself.)
static ENGINE: Mutex<Option<Arc<Pdfium>>> = Mutex::new(None);

fn engine(location: &PdfiumLocation) -> Result<Arc<Pdfium>, ConversionError> {
    let mut slot = ENGINE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(pdfium) = slot.as_ref() {
        return Ok(pdfium.clone());
    }

    // One entry per folder, with the reason it failed (not found vs wrong architecture...).
    let mut failures = Vec::new();
    let mut bindings = None;
    for dir in location.dirs() {
        let path = Pdfium::pdfium_platform_library_name_at_path(dir);
        match Pdfium::bind_to_library(&path) {
            Ok(b) => {
                bindings = Some(b);
                break;
            }
            Err(e) => failures.push(format!("{}: {e}", dir.display())),
        }
    }
    if bindings.is_none() && location.system_fallback {
        match Pdfium::bind_to_system_library() {
            Ok(b) => bindings = Some(b),
            Err(e) => failures.push(format!("system library: {e}")),
        }
    }
    let Some(bindings) = bindings else {
        return Err(ConversionError::EngineUnavailable {
            detail: format!("PDFium library could not be loaded ({})", failures.join("; ")),
        });
    };

    let pdfium = Arc::new(Pdfium::new(bindings));
    *slot = Some(pdfium.clone());
    Ok(pdfium)
}

/// Where to read the PDF from.
#[derive(Debug, Clone, Copy)]
pub enum PdfSource<'a> {
    Path(&'a Path),
    Bytes(&'a [u8]),
}

impl PdfSource<'_> {
    /// Short description for error messages.
    fn describe(&self) -> String {
        match self {
            PdfSource::Path(path) => path.display().to_string(),
            PdfSource::Bytes(_) => "in-memory document".to_string(),
        }
    }
}

fn map_error(error: PdfiumError, source: &PdfSource<'_>) -> ConversionError {
    match error {
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => {
            ConversionError::Encrypted
        }
        // Not a password problem: a password would not help, so do not call it "encrypted".
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::SecurityError) => {
            ConversionError::Internal {
                message: format!(
                    "{}: the document uses a security handler PDFium cannot open",
                    source.describe()
                ),
            }
        }
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::FileError) => {
            ConversionError::Io { message: format!("{}: the file could not be opened", source.describe()) }
        }
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::FormatError) => {
            ConversionError::Corrupt
        }
        PdfiumError::IoError(e) => ConversionError::Io {
            message: format!("{}: {e}", source.describe()),
        },
        other => ConversionError::Internal { message: format!("{other:?}") },
    }
}

/// Opens `source` and runs `f` on the document. The document is closed afterwards.
pub fn with_document<T>(
    location: &PdfiumLocation,
    source: PdfSource<'_>,
    f: impl FnOnce(&PdfDocument<'_>) -> Result<T, ConversionError>,
) -> Result<T, ConversionError> {
    let pdfium = engine(location)?;
    let document = match source {
        PdfSource::Path(path) => pdfium.load_pdf_from_file(path, None),
        PdfSource::Bytes(bytes) => pdfium.load_pdf_from_byte_slice(bytes, None),
    }
    .map_err(|e| map_error(e, &source))?;
    f(&document)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf_reader::test_support;

    #[test]
    fn garbage_bytes_are_corrupt() {
        let Some(location) = test_support::pdfium() else { return };
        let result = with_document(&location, PdfSource::Bytes(b"this is not a pdf"), |_| Ok(()));
        assert_eq!(result, Err(ConversionError::Corrupt));
    }

    #[test]
    fn missing_file_is_an_io_error_that_names_the_file() {
        let Some(location) = test_support::pdfium() else { return };
        let result = with_document(&location, PdfSource::Path(Path::new("no-such-file.pdf")), |_| Ok(()));
        match result {
            Err(ConversionError::Io { message }) => assert!(message.contains("no-such-file.pdf"), "{message}"),
            other => panic!("expected Io, got {other:?}"),
        }
    }

    #[test]
    fn standard_location_includes_extra_dirs_first() {
        let location = PdfiumLocation::standard(vec![PathBuf::from("custom")]);
        assert_eq!(location.dirs()[0], PathBuf::from("custom"));
        assert!(location.dirs().len() > 1);
    }

    #[test]
    fn security_error_is_not_reported_as_encrypted() {
        let error = map_error(
            PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::SecurityError),
            &PdfSource::Bytes(b""),
        );
        assert!(matches!(error, ConversionError::Internal { .. }), "{error:?}");
        let error = map_error(
            PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError),
            &PdfSource::Bytes(b""),
        );
        assert_eq!(error, ConversionError::Encrypted);
    }
}
