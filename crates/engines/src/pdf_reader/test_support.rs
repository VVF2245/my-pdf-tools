//! Helpers shared by the reader tests.

use super::pdfium::PdfiumLocation;
use crate::markdown::parse_markdown;
use crate::pdf::{PdfOptions, render_pdf};

/// PDFium for tests (see [`super::testing::pdfium`]).
pub fn pdfium() -> Option<PdfiumLocation> {
    super::testing::pdfium()
}

/// A PDF made by our own Markdown -> PDF writer: public, reproducible test input.
pub fn pdf_from_markdown(markdown: &str) -> Vec<u8> {
    let document = parse_markdown(markdown, None).document;
    render_pdf(&document, &PdfOptions::default()).expect("render sample PDF")
}
