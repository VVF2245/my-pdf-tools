//! Developer helper: `cargo run -p mpt-engines --example md_to_pdf -- <in.md> <out.pdf>`

use std::path::Path;

use mpt_engines::markdown::parse_markdown;
use mpt_engines::pdf::{PdfOptions, render_pdf};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [input, output] = args.as_slice() else {
        eprintln!("usage: md_to_pdf <in.md> <out.pdf>");
        std::process::exit(2);
    };
    let md = std::fs::read_to_string(input).expect("cannot read input");
    let parsed = parse_markdown(&md, Path::new(input).parent());
    for w in &parsed.warnings {
        eprintln!("warning [{}]: {}", w.code, w.detail);
    }
    let pdf = render_pdf(&parsed.document, &PdfOptions::default()).expect("render failed");
    std::fs::write(output, pdf).expect("cannot write output");
}
