//! End-to-end: the registry runs `md_to_pdf` on the sample document.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use mpt_core::{CancelToken, ConversionRequest, NoProgress, OptionValue};
use mpt_tools::Registry;

fn sample_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/md-sample")
}

/// Converts the sample into a fresh output folder and returns the PDF bytes.
fn convert(out_dir: &Path, options: BTreeMap<String, OptionValue>) -> Vec<u8> {
    std::fs::create_dir_all(out_dir).unwrap();
    let request = ConversionRequest {
        tool_id: "md_to_pdf".into(),
        inputs: vec![sample_dir().join("sample.md").to_string_lossy().into_owned()],
        output_dir: Some(out_dir.to_string_lossy().into_owned()),
        options,
    };
    let tool = Registry::new().get("md_to_pdf").expect("tool registered");
    let outcome = tool.run(&request, &NoProgress, &CancelToken::new()).unwrap();
    assert!(outcome.report.warnings.is_empty(), "{:?}", outcome.report.warnings);
    assert_eq!(outcome.outputs.len(), 1);
    std::fs::read(&outcome.outputs[0]).unwrap()
}

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("mpt-e2e-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        Self(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn sample_converts_to_a_valid_pdf_with_its_image() {
    let scratch = Scratch::new("valid");
    let pdf = convert(&scratch.0, BTreeMap::new());
    assert!(pdf.starts_with(b"%PDF-"));
    assert!(pdf.len() > 10_000, "the PDF looks too small: {} bytes", pdf.len());
}

#[test]
fn two_conversions_give_identical_bytes() {
    let a = Scratch::new("det-a");
    let b = Scratch::new("det-b");
    assert_eq!(convert(&a.0, BTreeMap::new()), convert(&b.0, BTreeMap::new()));
}

#[test]
fn paper_option_changes_the_output() {
    let a = Scratch::new("a4");
    let l = Scratch::new("letter");
    let mut letter = BTreeMap::new();
    letter.insert("paper".to_string(), OptionValue::Text("letter".into()));
    assert_ne!(convert(&a.0, BTreeMap::new()), convert(&l.0, letter));
}
