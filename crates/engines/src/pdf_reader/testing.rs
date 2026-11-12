//! Finding PDFium in tests, for this crate and the ones above it.
//!
//! Not part of the supported API (hidden from the docs): it exists so every crate's tests
//! decide in the same way whether PDFium is available.

use std::path::PathBuf;

use super::pdfium::PdfiumLocation;

/// PDFium for tests: the standard lookup (`$PDFIUM_DIR`, next to the test executable)
/// plus the folders `scripts/get-pdfium` downloads into.
///
/// Returns `None` (the test is skipped) when it is not available, except on CI,
/// where a missing library is a failure.
pub fn pdfium() -> Option<PdfiumLocation> {
    let downloads = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../pdfium");
    let location = PdfiumLocation::standard(vec![downloads.join("win-x64/bin"), downloads.join("mac-univ/lib")]);

    let library = pdfium_render::prelude::Pdfium::pdfium_platform_library_name();
    if location.dirs().iter().any(|dir| dir.join(&library).exists()) {
        return Some(location);
    }
    if std::env::var_os("CI").is_some() {
        panic!("PDFium not found on CI: run scripts/get-pdfium.ps1 and set PDFIUM_DIR");
    }
    eprintln!("skipped: PDFium not found (run scripts/get-pdfium.ps1)");
    None
}
