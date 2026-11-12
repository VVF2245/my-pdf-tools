//! Packaging smoke test: converts a PDF the way the app does, finding PDFium only through
//! the default lookup (next to this executable, `$PDFIUM_DIR`, the system).
//!
//! CI copies this binary into a folder laid out like the shipped app (executable and
//! PDFium side by side, or `Contents/MacOS` and `Contents/Frameworks` in the macOS app),
//! with `PDFIUM_DIR` unset, and runs it: if the library cannot be found there, so could
//! the app not find it.
//!
//! `smoke <file.pdf>`: exits with an error unless the Markdown comes out with a heading.

use std::path::PathBuf;

use mpt_core::{CancelToken, ConversionRequest, NoProgress};
use mpt_tools::Registry;

fn main() {
    let pdf = std::env::args().nth(1).expect("usage: smoke <file.pdf>");
    let out = std::env::temp_dir().join(format!("mpt-smoke-{}", std::process::id()));
    std::fs::create_dir_all(&out).expect("temp dir");

    let request = ConversionRequest {
        tool_id: "pdf_to_md".into(),
        inputs: vec![pdf],
        output_dir: Some(out.to_string_lossy().into_owned()),
        options: Default::default(),
    };
    let tool = Registry::default().get("pdf_to_md").expect("pdf_to_md is registered");
    let outcome = tool
        .run(&request, &NoProgress, &CancelToken::new())
        .unwrap_or_else(|e| panic!("conversion failed: {e}"));

    let markdown = std::fs::read_to_string(PathBuf::from(&outcome.outputs[0])).expect("output readable");
    let _ = std::fs::remove_dir_all(&out);
    assert!(markdown.starts_with("## "), "unexpected output:\n{markdown}");
    println!("smoke ok: {} bytes of Markdown", markdown.len());
}
