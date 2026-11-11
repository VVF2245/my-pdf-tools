//! Format adapters. They depend on `mpt-core` and implement the IR conversions.
//!
//! - [`markdown`]: Markdown text -> [`mpt_core::Document`] (reader).
//! - [`markdown_writer`]: [`mpt_core::Document`] -> Markdown text (writer).
//! - [`pdf`]: [`mpt_core::Document`] -> PDF bytes through an embedded Typst (writer).
//! - [`pdf_reader`]: PDF bytes -> positioned glyphs through PDFium (reader, work in progress).

pub mod markdown;
pub mod markdown_writer;
pub mod pdf;
pub mod pdf_reader;
