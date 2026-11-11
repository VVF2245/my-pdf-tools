//! The reader stages run over the synthetic PDFs in `fixtures/pdf/`
//! (written in Typst by us; see the README there).
//!
//! If a test here fails after upgrading Typst, regenerate the fixtures first:
//! `cargo run -p mpt-engines --example gen_fixtures`.

use std::collections::HashMap;
use std::path::PathBuf;

use super::clean::{MarginBands, repeated_margin_lines};
use super::lines::{PageLines, group_lines};
use super::pdfium::{PdfSource, PdfiumLocation};
use super::{extract_pages, test_support};
use crate::pdf::compile_typst;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/pdf")
}

fn lines_of(location: &PdfiumLocation, source: PdfSource<'_>) -> Vec<PageLines> {
    extract_pages(location, source, 1, 0)
        .expect("fixture readable")
        .into_iter()
        .map(group_lines)
        .collect()
}

/// Reads a committed fixture into lines; `None` when PDFium is not available (skipped).
fn read(name: &str) -> Option<Vec<PageLines>> {
    let location = test_support::pdfium()?;
    let path = fixtures_dir().join(name);
    Some(lines_of(&location, PdfSource::Path(&path)))
}

fn texts(page: &PageLines) -> Vec<&str> {
    page.lines.iter().map(|l| l.text.as_str()).collect()
}

/// Sizes are rounded to 0.1 pt by the reader, so compare with a small tolerance.
#[track_caller]
fn assert_size(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.06, "size {actual}, expected {expected}");
}

#[test]
fn committed_pdfs_match_their_typst_sources() {
    let Some(location) = test_support::pdfium() else { return };
    for name in ["slides", "notes", "twocol", "table"] {
        let source = std::fs::read_to_string(fixtures_dir().join(format!("{name}.typ"))).unwrap();
        let fresh = compile_typst(&source, HashMap::new()).expect("fixture source compiles");
        let committed = lines_of(&location, PdfSource::Path(&fixtures_dir().join(format!("{name}.pdf"))));
        let regenerated = lines_of(&location, PdfSource::Bytes(&fresh));
        // Compare what the reader sees (not the bytes, which may differ across Typst versions).
        let as_text = |pages: &[PageLines]| -> Vec<Vec<String>> {
            pages.iter().map(|p| p.lines.iter().map(|l| l.text.clone()).collect()).collect()
        };
        assert_eq!(
            as_text(&committed),
            as_text(&regenerated),
            "{name}.pdf is stale: run `cargo run -p mpt-engines --example gen_fixtures`"
        );
    }
}

#[test]
fn slides_are_landscape_with_the_title_on_top_and_a_repeated_footer() {
    let Some(pages) = read("slides.pdf") else { return };
    assert_eq!(pages.len(), 8);
    assert!(pages.iter().all(|p| p.width > p.height), "16:9 slides are landscape");

    assert_eq!(pages[0].lines[0].text, "Introducción a las bases de datos");
    assert_size(pages[0].lines[0].size, 28.0);

    let second = &pages[1];
    assert_eq!(second.lines[0].text, "¿Qué es una base de datos?");
    // title and bullets are close in size (20 vs 18 pt): position, not size, tells them apart
    assert_size(second.lines[0].size, 20.0);
    assert!(second.lines[1..5].iter().all(|l| (l.size - 18.0).abs() < 0.06));
    assert!(second.lines.iter().all(|l| l.baseline <= second.lines[0].baseline));
    // bullets have a marker glyph (whichever the Typst version draws) and nested ones are indented
    assert!(second.lines[1].text.ends_with(" Colección organizada de datos"));
    assert!(!second.lines[1].text.starts_with(char::is_alphanumeric), "{}", second.lines[1].text);
    assert!(second.lines[3].left > second.lines[1].left, "nested bullet is indented");

    // slide mode: footers only. The footer goes on every slide; nothing else does.
    let flags = repeated_margin_lines(&pages, MarginBands::BottomOnly);
    for (page, page_flags) in pages.iter().zip(&flags) {
        let footer = page.lines.len() - 1;
        assert!(page.lines[footer].text.starts_with("Bases de datos · Curso 2026"));
        assert!(page_flags[footer], "footer on page {} must be flagged", page.number);
        assert_eq!(page_flags.iter().filter(|f| **f).count(), 1, "only the footer is flagged");
    }
}

#[test]
fn a_title_repeated_on_several_slides_is_kept() {
    let Some(pages) = read("slides.pdf") else { return };
    let repeated: Vec<&PageLines> =
        pages.iter().filter(|p| p.lines[0].text == "Ejemplo").collect();
    assert_eq!(repeated.len(), 3);

    // with either band setting: the title is bigger than the body, so it is never furniture
    for bands in [MarginBands::BottomOnly, MarginBands::TopAndBottom] {
        let flags = repeated_margin_lines(&pages, bands);
        for page in &repeated {
            assert!(!flags[page.number as usize - 1][0], "{bands:?}: the title was removed");
        }
    }
}

#[test]
fn notes_have_three_heading_levels_code_and_page_furniture() {
    let Some(pages) = read("notes.pdf") else { return };
    assert!(pages.len() >= 3);

    let first = &pages[0];
    let size_of = |text: &str| first.lines.iter().find(|l| l.text == text).map(|l| l.size).unwrap();
    assert_size(size_of("Fundamentos"), 25.0);
    assert_size(size_of("Variables y tipos"), 17.0);
    assert_size(size_of("Tipos básicos"), 14.5);

    // body text is the most frequent size by number of glyphs
    let glyphs_at = |is_body: bool| -> usize {
        pages
            .iter()
            .flat_map(|p| &p.lines)
            .filter(|l| ((l.size - 12.0).abs() < 0.06) == is_body)
            .map(|l| l.glyphs.len())
            .sum()
    };
    assert!(glyphs_at(true) > glyphs_at(false));

    let code: Vec<&str> = first.lines.iter().filter(|l| l.mono).map(|l| l.text.as_str()).collect();
    assert_eq!(code, ["edad = 20", "nombre = \"Ana\"", "print(nombre, edad)"]);

    // the running header and the page numbers are found; the body is not touched
    let flags = repeated_margin_lines(&pages, MarginBands::TopAndBottom);
    for (page, page_flags) in pages.iter().zip(&flags) {
        for (line, flagged) in page.lines.iter().zip(page_flags) {
            let furniture = line.text.starts_with("Apuntes de programación")
                || line.text.chars().all(|c| c.is_ascii_digit());
            assert_eq!(*flagged, furniture, "page {} line {:?}", page.number, line.text);
        }
    }
}

#[test]
fn two_columns_come_out_left_column_first() {
    let Some(pages) = read("twocol.pdf") else { return };
    assert!(pages.len() >= 2, "the text fills more than one page");
    assert_eq!(pages[0].lines[0].text, "Un artículo de ejemplo en dos columnas");

    for (index, page) in pages.iter().enumerate() {
        let body: Vec<_> = page.lines.iter().filter(|l| (l.size - 10.0).abs() < 0.06).collect();
        let middle = page.width / 2.0;
        let first_right = body.iter().position(|l| l.left > middle);
        let last_left = body.iter().rposition(|l| l.left < middle);
        if index + 1 < pages.len() {
            // a page that is not the last one is full: both columns must be there
            assert!(first_right.is_some() && last_left.is_some(), "page {} lacks a column", page.number);
        }
        if let (Some(right), Some(left)) = (first_right, last_left) {
            assert!(left < right, "page {}: a left-column line comes after a right-column one", page.number);
        }
    }
}

#[test]
fn table_cells_on_one_baseline_become_one_spaced_line() {
    let Some(pages) = read("table.pdf") else { return };
    let page = &pages[0];
    let texts = texts(page);
    assert!(texts.contains(&"Muestra Ensayo 1 Ensayo 2 Ensayo 3"), "{texts:?}");
    assert!(texts.contains(&"Alfa 0,69 0,51 0,53"), "{texts:?}");
    assert!(texts.contains(&"Delta 7,01 6,98 7,03"), "{texts:?}");
    // the table is indented relative to the paragraphs: it is centered
    let row = page.lines.iter().find(|l| l.text.starts_with("Alfa")).unwrap();
    let paragraph = page.lines.iter().find(|l| l.text.starts_with("La siguiente")).unwrap();
    assert!(row.left > paragraph.left);
}
