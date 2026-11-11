//! Helpers shared by the reader tests.

use std::path::PathBuf;

use super::pdfium::PdfiumLocation;
use crate::markdown::parse_markdown;
use crate::pdf::{PdfOptions, render_pdf};

/// PDFium for tests: the standard lookup (`$PDFIUM_DIR`, next to the test executable)
/// plus the folders `scripts/get-pdfium` downloads into.
///
/// Returns `None` (the test is skipped) when it is not available, except on CI,
/// where a missing library is a failure.
pub fn pdfium() -> Option<PdfiumLocation> {
    let downloads = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../pdfium");
    let location = PdfiumLocation::standard(vec![
        downloads.join("win-x64/bin"),
        downloads.join("mac-univ/lib"),
    ]);

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

/// A PDF made by our own Markdown -> PDF writer: public, reproducible test input.
pub fn pdf_from_markdown(markdown: &str) -> Vec<u8> {
    let document = parse_markdown(markdown, None).document;
    render_pdf(&document, &PdfOptions::default()).expect("render sample PDF")
}
