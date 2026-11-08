//! Golden-file test: the Markdown reader must keep producing the same IR for the
//! sample document. To accept an intended change, regenerate the expected file with
//! `UPDATE_GOLDEN=1 cargo test -p mpt-engines --test golden` and review the diff.

use std::path::PathBuf;

use mpt_engines::markdown::parse_markdown;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/md-sample")
}

#[test]
fn sample_markdown_matches_golden_ir() {
    let dir = fixtures();
    let md = std::fs::read_to_string(dir.join("sample.md")).unwrap();
    let parsed = parse_markdown(&md, Some(&dir));
    assert!(parsed.warnings.is_empty(), "unexpected warnings: {:?}", parsed.warnings);

    // Image bytes would make the golden file unreadable: keep only their size.
    let mut document = parsed.document;
    let sizes: Vec<usize> = document.assets.iter().map(|a| a.data.len()).collect();
    for asset in &mut document.assets {
        asset.data.clear();
    }
    let mut actual = serde_json::to_value(&document).unwrap();
    actual["assetSizes"] = serde_json::json!(sizes);
    let mut actual = serde_json::to_string_pretty(&actual).unwrap();
    actual.push('\n');

    let golden = dir.join("sample.document.json");
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(&golden, &actual).unwrap();
    }
    let expected = std::fs::read_to_string(&golden)
        .expect("golden file missing: run with UPDATE_GOLDEN=1 to create it");
    // Compare ignoring line endings: git may check the file out with CRLF on Windows.
    assert_eq!(expected.replace("\r\n", "\n"), actual);
}
