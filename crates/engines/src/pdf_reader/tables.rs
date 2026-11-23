//! Tables: runs of lines that split into the same columns.
//!
//! A line is cut into cells where the gap between two glyphs is much wider than a word space.
//! Three or more consecutive lines with two or more cells each, whose cells line up in the
//! same columns, are a table. Anything less clear is left as text and reported
//! (`tableCandidate`), never guessed.
//!
//! An index (entries followed by dot leaders and a page number) and a program listing (line
//! numbers 1, 2, 3... beside the code) are not tables: they are left as text, as before.
//!
//! Limits: a table that crosses a page is two tables, a cell with several lines is several
//! rows, and merged cells are not understood. The first row is taken as the header.

use outmark_core::TableAlign;

use super::clean::{fix_loose_accents, strip_dot_leaders};
use super::glyphs::Glyph;
use super::lines::Line;

/// A gap wider than this many font sizes separates two cells (a word space is about 0.15).
const CELL_GAP: f32 = 1.0;
/// A table has a header and at least two more rows.
const MIN_ROWS: usize = 3;
const MIN_COLUMNS: usize = 2;
/// A line further than this many line pitches below the previous one is not the next row.
const ROW_GAP: f32 = 2.2;
/// Most rows must fill every column, or the run is not read as a table.
const MIN_FULL_ROWS: f32 = 0.6;

/// A piece of a line between two wide gaps.
#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    pub left: f32,
    pub right: f32,
    pub text: String,
}

/// A table found in a list of lines.
#[derive(Debug, Clone, PartialEq)]
pub struct TableSpan {
    /// Index of the first line and of the one after the last.
    pub start: usize,
    pub end: usize,
    pub page: u32,
    /// Baseline of the last row.
    pub last_baseline: f32,
    pub header: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub aligns: Vec<TableAlign>,
    /// Some row has fewer cells than the table has columns.
    pub low_confidence: bool,
}

/// What [`detect`] found.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Detection {
    pub tables: Vec<TableSpan>,
    /// Pages with a run of lines that looks like a table but is too short or irregular to
    /// convert.
    pub candidates: Vec<u32>,
}

/// Cuts a line into cells. A line without wide gaps is one cell.
pub fn cells(line: &Line) -> Vec<Cell> {
    let mut groups: Vec<Vec<Glyph>> = Vec::new();
    let mut last_visible: Option<&Glyph> = None;
    for glyph in &line.glyphs {
        let wide = !glyph.ch.is_whitespace()
            && last_visible.is_some_and(|p| glyph.left - p.right > CELL_GAP * glyph.size.max(p.size));
        if wide || groups.is_empty() {
            groups.push(Vec::new());
        }
        groups.last_mut().expect("a group exists").push(glyph.clone());
        if !glyph.ch.is_whitespace() {
            last_visible = Some(glyph);
        }
    }
    groups
        .into_iter()
        .filter_map(|glyphs| Line::from_glyphs(line.page, glyphs))
        .map(|cell| Cell { left: cell.left, right: cell.right, text: fix_loose_accents(&cell) })
        .collect()
}

/// A first column that counts 1, 2, 3... (the line numbers of a listing) beside the text.
///
/// Every row counts, the first one too: a real table with a numbered column has a header
/// ("No.", "#") above it, so it does not match. A headerless numbered table is read as a
/// listing and stays text.
fn is_numbered_listing(rows: &[Vec<Cell>]) -> bool {
    let numbers: Vec<Option<u32>> = rows
        .iter()
        .map(|r| r.first().and_then(|c| c.text.trim().parse::<u32>().ok()))
        .collect();
    numbers.iter().all(Option::is_some)
        && numbers.windows(2).all(|w| w[1].zip(w[0]).is_some_and(|(next, prev)| next == prev + 1))
}

fn is_numeric(text: &str) -> bool {
    let t = text.trim();
    !t.is_empty()
        && t.chars().any(|c| c.is_ascii_digit())
        && t.chars().all(|c| c.is_ascii_digit() || matches!(c, ',' | '.' | '%' | '-' | '\u{2212}' | '+' | ' '))
}

/// Merges the x-extents of all cells into columns (overlapping extents are one column).
///
/// The second value is `true` when a column was chained together by a cell that spans over
/// others (two cells of one column do not overlap each other): a header over two columns
/// merges them, so the table has fewer columns than it looks.
fn columns_of(rows: &[Vec<Cell>]) -> (Vec<(f32, f32)>, bool) {
    let mut spans: Vec<(f32, f32)> = rows.iter().flatten().map(|c| (c.left, c.right)).collect();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut columns: Vec<(f32, f32)> = Vec::new();
    let mut min_right = f32::INFINITY;
    let mut chained = false;
    for (left, right) in spans {
        match columns.last_mut() {
            Some(last) if left <= last.1 => {
                chained |= left > min_right;
                last.1 = last.1.max(right);
                min_right = min_right.min(right);
            }
            _ => {
                columns.push((left, right));
                min_right = right;
            }
        }
    }
    (columns, chained)
}

/// Two or more separate numbers in one cell: the gap between two columns was too narrow to
/// cut them (`82.3 91.2`).
fn has_glued_numbers(text: &str) -> bool {
    text.split_whitespace().filter(|token| is_numeric(token)).count() >= 2
}

fn column_of(columns: &[(f32, f32)], cell: &Cell) -> Option<usize> {
    columns.iter().position(|(left, right)| cell.left <= *right && cell.right >= *left)
}

/// Builds the table of a run of rows, or `None` if the columns are not clear enough.
fn build(start: usize, page: u32, last_baseline: f32, rows: &[Vec<Cell>]) -> Option<TableSpan> {
    let (columns, chained) = columns_of(rows);
    if columns.len() < MIN_COLUMNS {
        return None;
    }
    let mut low_confidence = chained;
    let mut full = 0;
    let mut grid: Vec<Vec<String>> = Vec::with_capacity(rows.len());
    for row in rows {
        let mut texts = vec![String::new(); columns.len()];
        for cell in row {
            let index = column_of(&columns, cell)?;
            if !texts[index].is_empty() {
                // two cells in one column: the columns are not clean
                low_confidence = true;
                texts[index].push(' ');
            }
            texts[index].push_str(&cell.text);
        }
        if texts.iter().all(|t| !t.is_empty()) {
            full += 1;
        } else {
            low_confidence = true;
        }
        grid.push(texts);
    }
    if (full as f32) < MIN_FULL_ROWS * rows.len() as f32 {
        return None;
    }
    if grid[1..].iter().flatten().any(|text| has_glued_numbers(text)) {
        low_confidence = true;
    }

    let aligns = (0..columns.len())
        .map(|c| {
            let body: Vec<&str> = grid[1..].iter().map(|r| r[c].as_str()).filter(|t| !t.is_empty()).collect();
            if !body.is_empty() && body.iter().all(|t| is_numeric(t)) { TableAlign::Right } else { TableAlign::Left }
        })
        .collect();
    let mut grid = grid.into_iter();
    let header = grid.next().unwrap_or_default();
    Some(TableSpan {
        start,
        end: start + rows.len(),
        page,
        last_baseline,
        header,
        rows: grid.collect(),
        aligns,
        low_confidence,
    })
}

/// Finds the tables among `lines` (in reading order). `pitch` is the usual distance between
/// two lines of one paragraph.
pub fn detect(lines: &[&Line], pitch: f32) -> Detection {
    let mut found = Detection::default();
    // the cells of every line, computed once (code lines have none)
    let all: Vec<Vec<Cell>> =
        lines.iter().map(|line| if line.mono { Vec::new() } else { cells(line) }).collect();
    let mut i = 0;
    while i < lines.len() {
        if all[i].len() < MIN_COLUMNS {
            i += 1;
            continue;
        }
        // extend the run while the next line is a row too
        let mut rows = vec![all[i].clone()];
        let mut j = i + 1;
        while j < lines.len() {
            let (prev, next) = (lines[j - 1], lines[j]);
            let below = prev.baseline - next.baseline;
            if next.page != prev.page || below <= 0.0 || below > ROW_GAP * pitch || all[j].len() < MIN_COLUMNS {
                break;
            }
            rows.push(all[j].clone());
            j += 1;
        }
        // an index with dot leaders is text, not a table
        if rows.iter().flatten().any(|c| strip_dot_leaders(&c.text).1) || is_numbered_listing(&rows) {
            i = j;
            continue;
        }
        let page = lines[i].page;
        let table = (rows.len() >= MIN_ROWS)
            .then(|| build(i, page, lines[j - 1].baseline, &rows))
            .flatten();
        match table {
            Some(table) => {
                i = table.end;
                found.tables.push(table);
            }
            None => {
                if rows.len() >= 2 && !found.candidates.contains(&page) {
                    found.candidates.push(page);
                }
                i = j;
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// A line made of `cells` (text, left x), each glyph half the size wide.
    fn row(cells: &[(&str, f32)], baseline: f32) -> Line {
        let size = 10.0;
        let mut glyphs = Vec::new();
        for (text, left) in cells {
            for (i, ch) in text.chars().enumerate() {
                glyphs.push(Glyph {
                    ch,
                    left: left + i as f32 * size * 0.5,
                    right: left + (i as f32 + 1.0) * size * 0.5,
                    bottom: baseline,
                    top: baseline + size,
                    size,
                    font: Arc::from("F"),
                    mono: false,
                });
            }
        }
        Line::from_glyphs(1, glyphs).unwrap()
    }

    fn texts(cells: &[Cell]) -> Vec<&str> {
        cells.iter().map(|c| c.text.as_str()).collect()
    }

    #[test]
    fn a_line_is_cut_at_wide_gaps_only() {
        let line = row(&[("Alfa beta", 50.0), ("0,69", 200.0), ("0,51", 300.0)], 700.0);
        assert_eq!(texts(&cells(&line)), ["Alfa beta", "0,69", "0,51"]);
        let plain = row(&[("just a plain sentence of text", 50.0)], 700.0);
        assert_eq!(cells(&plain).len(), 1);
    }

    fn grid() -> Vec<Line> {
        vec![
            row(&[("Muestra", 50.0), ("Ensayo 1", 150.0), ("Ensayo 2", 250.0)], 700.0),
            row(&[("Alfa", 50.0), ("0,69", 170.0), ("0,51", 270.0)], 686.0),
            row(&[("Beta", 50.0), ("1,20", 170.0), ("1,18", 270.0)], 672.0),
            row(&[("Gamma", 50.0), ("3,40", 170.0), ("3,35", 270.0)], 658.0),
        ]
    }

    #[test]
    fn rows_that_line_up_make_a_table_with_header_and_alignments() {
        let lines = grid();
        let refs: Vec<&Line> = lines.iter().collect();
        let found = detect(&refs, 14.0);
        assert_eq!(found.tables.len(), 1);
        let table = &found.tables[0];
        assert_eq!((table.start, table.end), (0, 4));
        assert_eq!(table.header, ["Muestra", "Ensayo 1", "Ensayo 2"]);
        assert_eq!(table.rows.len(), 3);
        assert_eq!(table.rows[0], ["Alfa", "0,69", "0,51"]);
        assert_eq!(table.aligns, [TableAlign::Left, TableAlign::Right, TableAlign::Right]);
        assert!(!table.low_confidence);
    }

    #[test]
    fn a_missing_cell_is_filled_and_reported() {
        let mut lines = grid();
        lines[2] = row(&[("Beta", 50.0), ("1,18", 270.0)], 672.0);
        let refs: Vec<&Line> = lines.iter().collect();
        let found = detect(&refs, 14.0);
        let table = &found.tables[0];
        assert!(table.low_confidence);
        assert_eq!(table.rows[1], ["Beta", "", "1,18"]);
    }

    #[test]
    fn an_index_with_dot_leaders_is_not_a_table() {
        let index: Vec<Line> = (0..4)
            .map(|i| {
                row(
                    &[("1.1. Introduccion . . . . . . . . . . . . . . .", 50.0), ("12", 400.0)],
                    700.0 - 14.0 * i as f32,
                )
            })
            .collect();
        let refs: Vec<&Line> = index.iter().collect();
        assert_eq!(detect(&refs, 14.0), Detection::default());
    }

    #[test]
    fn a_listing_with_line_numbers_is_not_a_table() {
        let listing: Vec<Line> = ["x := 1;", "y := x;", "END;", "BEGIN"]
            .iter()
            .enumerate()
            .map(|(i, code)| row(&[(&(i + 7).to_string(), 50.0), (code, 120.0)], 700.0 - 14.0 * i as f32))
            .collect();
        let refs: Vec<&Line> = listing.iter().collect();
        assert_eq!(detect(&refs, 14.0), Detection::default());
    }

    #[test]
    fn a_numbered_column_with_a_header_is_still_a_table() {
        let lines = vec![
            row(&[("No.", 50.0), ("Nombre", 150.0)], 700.0),
            row(&[("1", 50.0), ("Alfa", 150.0)], 686.0),
            row(&[("2", 50.0), ("Beta", 150.0)], 672.0),
            row(&[("3", 50.0), ("Gamma", 150.0)], 658.0),
        ];
        let refs: Vec<&Line> = lines.iter().collect();
        assert_eq!(detect(&refs, 14.0).tables.len(), 1);
    }

    #[test]
    fn numbers_cut_too_close_together_are_reported() {
        // "82.3 91.2" is one cell: the gap between the columns is narrower than a cell gap
        let mut lines = grid();
        lines[1] = row(&[("Alfa", 50.0), ("82.3 91.2", 170.0), ("0,51", 270.0)], 686.0);
        let refs: Vec<&Line> = lines.iter().collect();
        assert!(detect(&refs, 14.0).tables[0].low_confidence);
    }

    #[test]
    fn a_header_over_two_columns_lowers_the_confidence() {
        let lines = vec![
            row(&[("Resultados de los dos ensayos", 160.0), ("Notas", 330.0)], 700.0),
            row(&[("Alfa", 50.0), ("0,69", 170.0), ("0,51", 250.0), ("ok", 330.0)], 686.0),
            row(&[("Beta", 50.0), ("1,20", 170.0), ("1,18", 250.0), ("ok", 330.0)], 672.0),
            row(&[("Gamma", 50.0), ("3,40", 170.0), ("3,35", 250.0), ("ok", 330.0)], 658.0),
        ];
        let refs: Vec<&Line> = lines.iter().collect();
        let found = detect(&refs, 14.0);
        assert!(found.tables.first().is_none_or(|t| t.low_confidence), "{found:?}");
    }

    #[test]
    fn two_rows_are_only_a_candidate() {
        let lines = grid();
        let refs: Vec<&Line> = lines[..2].iter().collect();
        let found = detect(&refs, 14.0);
        assert!(found.tables.is_empty());
        assert_eq!(found.candidates, [1]);
    }

    #[test]
    fn plain_text_and_distant_lines_are_not_tables() {
        let text: Vec<Line> = (0..5)
            .map(|i| row(&[("an ordinary line of running text", 50.0)], 700.0 - 14.0 * i as f32))
            .collect();
        let refs: Vec<&Line> = text.iter().collect();
        assert_eq!(detect(&refs, 14.0), Detection::default());

        // rows far apart vertically are separate runs of one row each
        let far: Vec<Line> = (0..3)
            .map(|i| row(&[("Alfa", 50.0), ("0,69", 200.0)], 700.0 - 100.0 * i as f32))
            .collect();
        let refs: Vec<&Line> = far.iter().collect();
        assert!(detect(&refs, 14.0).tables.is_empty());
    }
}
