//! PDF writer: IR -> Typst markup -> embedded Typst compiler -> PDF bytes.

mod markup;
mod world;

use std::collections::HashMap;

use mpt_core::{ConversionError, Document};
use typst::diag::SourceDiagnostic;
use typst::foundations::Bytes;
use typst_layout::PagedDocument;

pub use markup::to_typst;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Paper {
    A4,
    Letter,
}

impl Paper {
    /// Name Typst uses for the paper size.
    fn typst_name(self) -> &'static str {
        match self {
            Paper::A4 => "a4",
            Paper::Letter => "us-letter",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PdfOptions {
    pub paper: Paper,
    pub margin_cm: f64,
    pub font_size_pt: f64,
}

impl Default for PdfOptions {
    fn default() -> Self {
        Self { paper: Paper::A4, margin_cm: 2.2, font_size_pt: 11.0 }
    }
}

/// Renders `doc` to PDF bytes. The output is reproducible: no clock, no system fonts.
pub fn render_pdf(doc: &Document, options: &PdfOptions) -> Result<Vec<u8>, ConversionError> {
    let markup = to_typst(doc, options);
    let files = doc
        .assets
        .iter()
        .map(|a| (markup::asset_path(a), a.data.clone()))
        .collect();
    compile_typst(&markup, files)
}

/// Compiles Typst source to PDF bytes. `files` maps a path without leading slash
/// (e.g. `assets/img-1.png`) to the bytes the source can load.
///
/// **Not part of the supported API.** It exists so the test fixtures can be written
/// directly in Typst (slides, two columns...). Typst is a scripting language: the source
/// must be trusted, never user input (a loop in it would run without any limit).
#[doc(hidden)]
pub fn compile_typst(source: &str, files: HashMap<String, Vec<u8>>) -> Result<Vec<u8>, ConversionError> {
    let files = files.into_iter().map(|(path, data)| (path, Bytes::new(data))).collect();
    let world = world::MemoryWorld::new(source.to_string(), files);

    let warned = typst::compile::<PagedDocument>(&world);
    let paged = warned.output.map_err(|errors| internal_error(&errors))?;
    typst_pdf::pdf(&paged, &typst_pdf::PdfOptions::default()).map_err(|errors| internal_error(&errors))
}

/// One `Internal` error carrying every diagnostic message.
fn internal_error(diagnostics: &[SourceDiagnostic]) -> ConversionError {
    ConversionError::Internal {
        message: diagnostics.iter().map(|d| d.message.to_string()).collect::<Vec<_>>().join("; "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown::parse_markdown;

    const SAMPLE: &str = "# Título\n\nPárrafo con **negrita**, *cursiva*, `código` y [enlace](https://example.com). \
        Especiales: # $ * _ @ < > [ ] \\ \" áéíóú ñ.\n\n\
        - uno\n- dos\n  - anidado\n\n1. primero\n2. segundo\n\n- [x] hecho\n- [ ] pendiente\n\n\
        ```rust\nfn main() {}\n```\n\n\
        | a | b |\n|:--|--:|\n| 1 | 2 |\n\n> cita\n\n---\n\nFin.\n";

    #[test]
    fn renders_a_valid_pdf() {
        let doc = parse_markdown(SAMPLE, None).document;
        let pdf = render_pdf(&doc, &PdfOptions::default()).unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        assert!(pdf.len() > 1000);
    }

    #[test]
    fn output_is_deterministic() {
        let doc = parse_markdown(SAMPLE, None).document;
        let a = render_pdf(&doc, &PdfOptions::default()).unwrap();
        let b = render_pdf(&doc, &PdfOptions::default()).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn empty_document_renders() {
        let pdf = render_pdf(&Document::default(), &PdfOptions::default()).unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
    }

    #[test]
    fn letter_and_a4_differ() {
        let doc = parse_markdown("hola\n", None).document;
        let a4 = render_pdf(&doc, &PdfOptions::default()).unwrap();
        let letter =
            render_pdf(&doc, &PdfOptions { paper: Paper::Letter, ..Default::default() }).unwrap();
        assert_ne!(a4, letter);
    }
}
