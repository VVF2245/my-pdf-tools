//! Runs in its own process on purpose: PDFium binds once per process, so "library not
//! found" can only be tested before any other test has loaded it.

use std::path::PathBuf;

use mpt_core::{CancelToken, ConversionError, ConversionRequest, NoProgress};
use mpt_engines::pdf_reader::PdfiumLocation;
use mpt_tools::{Registry, ToolsConfig};

#[test]
fn without_the_pdfium_library_the_tool_reports_engine_unavailable() {
    // only the folders listed count: the test must not depend on a PDFium installed here
    let pdfium = PdfiumLocation::new(vec![PathBuf::from("definitely/not/here")]).without_system_library();
    let registry = Registry::new(&ToolsConfig { pdfium });
    let tool = registry.get("pdf_to_md").expect("registered");

    let slides = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/pdf/slides.pdf");
    let request = ConversionRequest {
        tool_id: "pdf_to_md".into(),
        inputs: vec![slides.to_string_lossy().into_owned()],
        output_dir: None,
        options: Default::default(),
    };

    match tool.run(&request, &NoProgress, &CancelToken::new()) {
        Err(ConversionError::EngineUnavailable { detail }) => assert!(detail.contains("definitely/not/here"), "{detail}"),
        other => panic!("expected EngineUnavailable, got {other:?}"),
    }
}
