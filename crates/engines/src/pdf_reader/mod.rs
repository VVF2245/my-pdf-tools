//! PDF reader built on PDFium (dynamic library, loaded at run time).
//!
//! Pipeline (see the design notes, section 5.4): glyphs -> lines -> blocks -> classification.
//! This module holds the first stage: opening a document and extracting positioned glyphs.

pub mod clean;
pub mod glyphs;
pub mod lines;
pub mod pdfium;

pub use glyphs::{Glyph, PageGlyphs, extract_pages};
pub use lines::{Line, PageLines, group_lines};
pub use pdfium::{PdfSource, PdfiumLocation, with_document};

#[cfg(test)]
pub(crate) mod test_support;
