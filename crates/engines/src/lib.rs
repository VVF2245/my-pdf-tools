//! Format adapters. They depend on `outmark-core` and implement the IR conversions.
//!
//! - [`markdown`]: Markdown text -> [`outmark_core::Document`] (reader).
//! - [`markdown_writer`]: [`outmark_core::Document`] -> Markdown text (writer).
//! - [`pdf`]: [`outmark_core::Document`] -> PDF bytes through an embedded Typst (writer).
//! - [`pdf_reader`]: PDF bytes -> positioned glyphs through PDFium (reader, work in progress).

pub mod markdown;
pub mod markdown_writer;
pub mod pdf;
pub mod pdf_reader;
