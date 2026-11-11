//! Runs in its own process on purpose: PDFium binds once per process, so "library not
//! found" can only be tested before any other test has loaded it.

use std::path::PathBuf;

use mpt_core::ConversionError;
use mpt_engines::pdf_reader::{PdfSource, PdfiumLocation, with_document};

#[test]
fn missing_library_is_engine_unavailable_and_names_the_folders_tried() {
    // No system fallback: the test must not depend on a PDFium installed on this machine.
    let location =
        PdfiumLocation::new(vec![PathBuf::from("definitely/not/here")]).without_system_library();

    let result = with_document(&location, PdfSource::Bytes(b"%PDF-"), |_| Ok(()));

    match result {
        Err(ConversionError::EngineUnavailable { detail }) => {
            assert!(detail.contains("definitely/not/here"), "{detail}");
        }
        other => panic!("expected EngineUnavailable, got {other:?}"),
    }
}
