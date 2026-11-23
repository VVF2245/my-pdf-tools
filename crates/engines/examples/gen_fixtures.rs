//! Developer helper: compiles every `fixtures/pdf/*.typ` into a PDF next to it.
//!
//! `cargo run -p outmark-engines --example gen_fixtures`

use std::path::PathBuf;

use outmark_engines::pdf::compile_typst;
use outmark_engines::pdf_reader::images::fixture_assets;

fn main() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/pdf");
    let mut sources: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("fixtures/pdf exists")
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "typ"))
        .collect();
    sources.sort();

    for source in sources {
        let typst = std::fs::read_to_string(&source).expect("readable source");
        let pdf = compile_typst(&typst, fixture_assets())
            .unwrap_or_else(|e| panic!("{}: {e}", source.display()));
        let target = source.with_extension("pdf");
        std::fs::write(&target, &pdf).expect("writable target");
        println!("{} -> {} bytes", target.file_name().unwrap().to_string_lossy(), pdf.len());
    }
}
