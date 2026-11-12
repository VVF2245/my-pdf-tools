//! Notes mode: running text in portrait pages (class notes, articles, books).
//!
//! The structure comes from three signals: the size of a line (headings are bigger than the
//! body, and the distinct bigger sizes give the levels), the font (a monospaced run is code)
//! and the vertical and horizontal gaps (paragraph breaks, wrapped list items).
//!
//! Lines are read in content-stream order: PDFs written by LaTeX list each column in turn.
//! That is not verified for files written by Word or InDesign.
//!
//! Not done yet: bold and italic (the glyphs carry no style flag), tables and images.

use std::sync::Arc;

use mpt_core::{Block, BlockKind, ConversionWarning, Document, Inline};

use super::clean::{MarginBands, body_size, join_lines, repeated_margin_lines, strip_dot_leaders};
use super::lines::{Line, PageLines};
use super::slides::{Bullet, list_blocks, split_marker, text_start};

/// Options of the notes conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoteOptions {
    /// Drop page numbers and running headers and footers repeated on many pages.
    pub remove_furniture: bool,
}

impl Default for NoteOptions {
    fn default() -> Self {
        Self { remove_furniture: true }
    }
}

/// A line this much bigger than the body may be a heading.
const HEADING_SIZE_FACTOR: f32 = 1.05;
/// Sizes closer than this (points) are the same heading level.
const SIZE_CLUSTER: f32 = 0.5;
/// A longer line is a paragraph set in a big size, not a heading.
const MAX_HEADING_CHARS: usize = 160;
/// In a document of at least this many pages, a size seen only on the first
/// [`TITLE_PAGE_SPAN`] pages, and on at most [`MAX_TITLE_PAGE_COUNT`] of them, is a title
/// page's, not a heading level (it would push the chapters down to level 3).
const MIN_PAGES_FOR_TITLE_PAGE: usize = 10;
const TITLE_PAGE_SPAN: u32 = 4;
const MAX_TITLE_PAGE_COUNT: usize = 3;
/// Markdown has six heading levels.
const MAX_HEADING_LEVEL: usize = 6;
/// A line further than this many line pitches below the previous one starts a paragraph.
const PARAGRAPH_GAP: f32 = 1.35;
/// A heading line closer than this (in line heights) below the previous one is the same heading.
const HEADING_WRAP_GAP: f32 = 1.4;
/// A line indented by more than this many line heights starts a new paragraph.
const INDENT_BREAK: f32 = 0.8;
/// A wrapped line must start within this many line heights of where the item text starts.
const HANGING_INDENT_TOLERANCE: f32 = 0.5;
/// A line this close (in body sizes) to the previous one, vertically, is the same text line.
const INLINE_RUN_OFFSET: f32 = 0.7;
/// A script (superscript, subscript) is set smaller than this fraction of the body.
const SCRIPT_SIZE_FACTOR: f32 = 0.9;
/// ...and this close horizontally to where the previous one ends.
const INLINE_RUN_GAP: f32 = 0.4;
/// Advance of a monospaced glyph, as a fraction of its size.
const MONO_ADVANCE: f32 = 0.6;
/// Line pitch as a fraction of the body size when it cannot be measured.
const DEFAULT_PITCH: f32 = 1.2;

/// Converts running text into a [`Document`].
///
/// Notes mode decides nothing it has to warn about, so the warnings are empty; the pair keeps
/// the shape of `slides_to_document`.
pub fn notes_to_document(pages: &[PageLines], options: NoteOptions) -> (Document, Vec<ConversionWarning>) {
    let body = body_size(pages);
    let flags = if options.remove_furniture {
        repeated_margin_lines(pages, MarginBands::TopAndBottom)
    } else {
        pages.iter().map(|p| vec![false; p.lines.len()]).collect()
    };

    let mut items: Vec<Item> = Vec::new();
    for (page, page_flags) in pages.iter().zip(&flags) {
        for (line, removed) in page.lines.iter().zip(page_flags) {
            if *removed {
                continue;
            }
            let text = if line.mono {
                line.text.trim_end().to_string()
            } else {
                let fixed = super::clean::fix_loose_accents(line);
                strip_dot_leaders(&fixed).0.trim().to_string()
            };
            if !text.trim().is_empty() {
                items.push(Item { line, text, right: line.right, small_tail: false });
            }
        }
    }
    if items.is_empty() {
        return (Document::default(), Vec::new());
    }
    let items = merge_inline_runs(items, body);

    let sizes = heading_sizes(&items, body);
    let mut builder = Builder::new(line_pitch(&items, body));
    for item in &items {
        builder.push(item, classify(item, body, &sizes));
    }
    (Document { blocks: builder.finish(), ..Default::default() }, Vec::new())
}

struct Item<'a> {
    line: &'a Line,
    text: String,
    /// Right edge of the text (it grows when a superscript is merged in).
    right: f32,
    /// The last part merged in was set smaller than the body (a superscript or subscript):
    /// the text that follows it continues the same line.
    small_tail: bool,
}

/// A superscript or subscript (`2^n`) comes out of the line grouping as a line of its own,
/// a little higher or lower, and the rest of the sentence as another one. Text that starts
/// right where the previous item ends, only slightly above or below it, belongs to the same
/// line: it is merged (the position is that of the first part; the exponent is not marked).
///
/// One of the two must be small (the script itself, or the text right after it): two
/// body-size lines that happen to touch, like two columns with a narrow gutter, stay apart.
fn merge_inline_runs<'a>(items: Vec<Item<'a>>, body: f32) -> Vec<Item<'a>> {
    let mut out: Vec<Item<'a>> = Vec::with_capacity(items.len());
    for item in items {
        let merges = out.last().is_some_and(|prev| {
            let (a, b) = (prev.line, item.line);
            let offset = (a.baseline - b.baseline).abs();
            let gap = b.left - prev.right;
            let small = b.size < body * SCRIPT_SIZE_FACTOR;
            a.page == b.page
                && (small || prev.small_tail)
                && !a.mono
                && !b.mono
                && a.size <= body * HEADING_SIZE_FACTOR
                && b.size <= body * HEADING_SIZE_FACTOR
                && (offset > 0.0 || prev.small_tail)
                && offset <= INLINE_RUN_OFFSET * body
                && (-INLINE_RUN_GAP * body..=INLINE_RUN_GAP * body).contains(&gap)
        });
        if merges {
            let prev = out.last_mut().expect("checked above");
            prev.text.push(' ');
            prev.text.push_str(item.text.trim_start());
            prev.right = item.right;
            prev.small_tail = item.line.size < body * SCRIPT_SIZE_FACTOR;
        } else {
            out.push(item);
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    Heading(u8),
    Code,
    Body,
}

/// The distinct sizes bigger than the body, biggest first: index + 1 is the heading level.
///
/// The one-off sizes of a title page are left out (see [`MIN_PAGES_FOR_TITLE_PAGE`]): such a
/// line still reads as a heading, of the top level (see [`classify`]).
fn heading_sizes(items: &[Item], body: f32) -> Vec<f32> {
    let mut seen: Vec<(f32, u32)> = items
        .iter()
        .filter(|i| !i.line.mono && i.line.size > body * HEADING_SIZE_FACTOR && i.text.chars().count() <= MAX_HEADING_CHARS)
        .map(|i| (i.line.size, i.line.page))
        .collect();
    seen.sort_by(|a, b| b.0.total_cmp(&a.0));
    // (biggest size of the cluster, pages where it appears)
    let mut clusters: Vec<(f32, Vec<u32>)> = Vec::new();
    for (size, page) in seen {
        match clusters.last_mut() {
            Some((top, pages)) if *top - size <= SIZE_CLUSTER => {
                if !pages.contains(&page) {
                    pages.push(page);
                }
            }
            _ => clusters.push((size, vec![page])),
        }
    }
    let document_pages = {
        let mut p: Vec<u32> = items.iter().map(|i| i.line.page).collect();
        p.dedup(); // consecutive only: enough, items come in page order
        p.len()
    };
    // a size seen only on the first pages of a long document: a title page
    let first_page = items.first().map_or(0, |i| i.line.page);
    let front_only = |pages: &[u32]| {
        document_pages >= MIN_PAGES_FOR_TITLE_PAGE
            && pages.len() <= MAX_TITLE_PAGE_COUNT
            && pages.iter().all(|p| *p < first_page + TITLE_PAGE_SPAN)
    };
    let mut first = 0;
    while first + 1 < clusters.len() && front_only(&clusters[first].1) {
        first += 1;
    }
    clusters.iter().skip(first).take(MAX_HEADING_LEVEL).map(|(s, _)| *s).collect()
}

fn classify(item: &Item, body: f32, sizes: &[f32]) -> Kind {
    let line = item.line;
    if line.mono {
        return Kind::Code;
    }
    if line.size > body * HEADING_SIZE_FACTOR && item.text.chars().count() <= MAX_HEADING_CHARS {
        if let Some(index) = sizes.iter().position(|top| line.size >= top - SIZE_CLUSTER) {
            return Kind::Heading(index as u8 + 1);
        }
    }
    Kind::Body
}

/// The usual distance between the baselines of two consecutive lines of one paragraph.
fn line_pitch(items: &[Item], body: f32) -> f32 {
    let mut gaps: Vec<f32> = items
        .windows(2)
        .filter(|w| {
            let (a, b) = (w[0].line, w[1].line);
            a.page == b.page && !a.mono && !b.mono && (a.size - b.size).abs() < 0.5
        })
        .map(|w| w[0].line.baseline - w[1].line.baseline)
        .filter(|gap| *gap > 0.5 * body && *gap < 2.2 * body)
        .collect();
    if gaps.is_empty() {
        return DEFAULT_PITCH * body;
    }
    // lower quartile, not the median: paragraph gaps are bigger than line gaps, and a text
    // of short paragraphs must not stretch the pitch
    gaps.sort_by(f32::total_cmp);
    gaps[gaps.len() / 4]
}

/// True when the text ends a sentence: the next line may then start a new paragraph.
fn ends_sentence(text: &str) -> bool {
    text.trim_end().chars().last().is_some_and(|c| ".!?:;\u{2026}\u{00BB}\u{201D}".contains(c))
}

#[derive(Clone, Copy)]
struct Previous {
    page: u32,
    baseline: f32,
}

struct Paragraph {
    page: u32,
    lines: Vec<String>,
    /// Leftmost edge so far: a first line is indented, so a later line can lower it.
    min_left: f32,
}

struct Heading {
    page: u32,
    level: u8,
    lines: Vec<String>,
    size: f32,
    font: Arc<str>,
    baseline: f32,
}

struct CodeRow {
    left: f32,
    baseline: f32,
    size: f32,
    text: String,
}

struct Code {
    page: u32,
    rows: Vec<CodeRow>,
}

struct Builder {
    pitch: f32,
    blocks: Vec<Block>,
    previous: Option<Previous>,
    paragraph: Option<Paragraph>,
    bullets: Vec<Bullet>,
    bullets_page: u32,
    heading: Option<Heading>,
    code: Option<Code>,
}

impl Builder {
    fn new(pitch: f32) -> Self {
        Self {
            pitch,
            blocks: Vec::new(),
            previous: None,
            paragraph: None,
            bullets: Vec::new(),
            bullets_page: 0,
            heading: None,
            code: None,
        }
    }

    fn emit(&mut self, kind: BlockKind, page: u32) {
        let mut block = Block::new(kind);
        block.page = Some(page);
        self.blocks.push(block);
    }

    fn flush_paragraph(&mut self) {
        if let Some(p) = self.paragraph.take() {
            let text = join_lines(&p.lines);
            self.emit(BlockKind::Paragraph(vec![Inline::Text(text)]), p.page);
        }
    }

    fn flush_bullets(&mut self) {
        if !self.bullets.is_empty() {
            let page = self.bullets_page;
            for block in list_blocks(std::mem::take(&mut self.bullets)) {
                self.emit(block.kind, page);
            }
        }
    }

    fn flush_heading(&mut self) {
        if let Some(h) = self.heading.take() {
            let content = vec![Inline::Text(join_lines(&h.lines))];
            self.emit(BlockKind::Heading { level: h.level, content }, h.page);
        }
    }

    fn flush_code(&mut self) {
        if let Some(c) = self.code.take() {
            let code = code_text(&c.rows);
            self.emit(BlockKind::CodeBlock { lang: None, code }, c.page);
        }
    }

    fn finish(mut self) -> Vec<Block> {
        self.flush_heading();
        self.flush_code();
        self.flush_bullets();
        self.flush_paragraph();
        self.blocks
    }

    fn push(&mut self, item: &Item, kind: Kind) {
        let line = item.line;
        match kind {
            Kind::Heading(level) => {
                self.flush_paragraph();
                self.flush_bullets();
                self.flush_code();
                let wraps = self.heading.as_ref().is_some_and(|h| {
                    let gap = h.baseline - line.baseline;
                    h.level == level
                        && h.page == line.page
                        && h.font == line.font
                        && (h.size - line.size).abs() < 0.5
                        && gap > 0.0
                        && gap <= HEADING_WRAP_GAP * line.size
                });
                if wraps {
                    let h = self.heading.as_mut().expect("a heading is open");
                    h.lines.push(item.text.clone());
                    h.baseline = line.baseline;
                } else {
                    self.flush_heading();
                    self.heading = Some(Heading {
                        page: line.page,
                        level,
                        lines: vec![item.text.clone()],
                        size: line.size,
                        font: line.font.clone(),
                        baseline: line.baseline,
                    });
                }
            }
            Kind::Code => {
                self.flush_heading();
                self.flush_paragraph();
                self.flush_bullets();
                let code = self.code.get_or_insert(Code { page: line.page, rows: Vec::new() });
                code.rows.push(CodeRow {
                    left: line.left,
                    baseline: line.baseline,
                    size: line.size,
                    text: item.text.clone(),
                });
            }
            Kind::Body => {
                self.flush_heading();
                self.flush_code();
                self.body_line(item);
            }
        }
        self.previous = Some(Previous { page: line.page, baseline: line.baseline });
    }

    fn body_line(&mut self, item: &Item) {
        let line = item.line;
        let (marker, rest) = split_marker(&item.text);
        // below the previous line on the same page: the usual flow; else a jump (next
        // column or page)
        let flowing = self.previous.is_some_and(|p| p.page == line.page && p.baseline - line.baseline > 0.0);
        let close = flowing
            && self.previous.is_some_and(|p| p.baseline - line.baseline <= PARAGRAPH_GAP * self.pitch);
        // a number that starts a line in the middle of a sentence is text, not a list item
        let mid_sentence = self.bullets.is_empty()
            && self.paragraph.as_ref().and_then(|p| p.lines.last()).is_some_and(|last| !ends_sentence(last));
        let marker = marker.filter(|m| !(m.ordered && close && mid_sentence));
        if let Some(marker) = marker {
            self.flush_paragraph();
            if self.bullets.is_empty() {
                self.bullets_page = line.page;
            }
            let text_left = text_start(line, &item.text, rest);
            self.bullets.push(Bullet { marker, left: line.left, text_left, lines: vec![rest.to_string()] });
            return;
        }

        if let Some(last) = self.bullets.last() {
            let wraps = close
                && line.left > last.left + 0.3 * line.size
                && (line.left - last.text_left).abs() <= HANGING_INDENT_TOLERANCE * line.size;
            if wraps {
                self.bullets.last_mut().expect("a bullet is open").lines.push(item.text.clone());
                return;
            }
            self.flush_bullets();
        }

        let mut new_column = false;
        if let Some(par) = &self.paragraph {
            let continues = if flowing {
                close && line.left - par.min_left <= INDENT_BREAK * line.size
            } else {
                // a column or page break inside a paragraph has no sentence end before it
                new_column = par.lines.last().is_some_and(|last| !ends_sentence(last));
                new_column
            };
            if !continues {
                self.flush_paragraph();
            }
        }
        let par = self.paragraph.get_or_insert(Paragraph { page: line.page, lines: Vec::new(), min_left: line.left });
        // the left edge of the paragraph is that of its current column
        par.min_left = if new_column { line.left } else { par.min_left.min(line.left) };
        par.lines.push(item.text.clone());
    }
}

/// The text of a code block: indentation from the horizontal offsets, blank lines from the
/// vertical gaps. (Runs of spaces inside a line are read as one: the reader infers a space
/// per gap.)
fn code_text(rows: &[CodeRow]) -> String {
    let left = rows.iter().map(|r| r.left).fold(f32::INFINITY, f32::min);
    let pitch = rows
        .windows(2)
        .map(|w| w[0].baseline - w[1].baseline)
        .filter(|gap| *gap > 0.0)
        .fold(f32::INFINITY, f32::min);
    let mut out = String::new();
    for (i, row) in rows.iter().enumerate() {
        if i > 0 {
            out.push('\n');
            let gap = rows[i - 1].baseline - row.baseline;
            if pitch.is_finite() && gap > 0.0 {
                let blanks = (gap / pitch).round() as usize;
                for _ in 1..blanks.min(4) {
                    out.push('\n');
                }
            }
        }
        let indent = ((row.left - left) / (MONO_ADVANCE * row.size)).round().max(0.0) as usize;
        out.push_str(&" ".repeat(indent));
        out.push_str(row.text.trim_start());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(left: f32, baseline: f32, text: &str) -> CodeRow {
        CodeRow { left, baseline, size: 10.0, text: text.to_string() }
    }

    #[test]
    fn code_keeps_indentation_and_blank_lines() {
        let rows = [
            row(72.0, 700.0, "def f(x):"),
            row(84.0, 688.0, "return x"),
            row(72.0, 664.0, "f(1)"),
        ];
        assert_eq!(code_text(&rows), "def f(x):\n  return x\n\nf(1)");
    }

    fn text_line(text: &str, left: f32, baseline: f32, size: f32) -> Line {
        use crate::pdf_reader::glyphs::Glyph;
        let glyphs = text
            .chars()
            .enumerate()
            .map(|(i, ch)| Glyph {
                ch,
                left: left + i as f32 * size * 0.5,
                right: left + (i as f32 + 1.0) * size * 0.5,
                bottom: baseline,
                top: baseline + size,
                size,
                font: Arc::from("F"),
                mono: false,
            })
            .collect();
        Line::from_glyphs(1, glyphs).unwrap()
    }

    fn item(line: &Line) -> Item<'_> {
        Item { line, text: line.text.clone(), right: line.right, small_tail: false }
    }

    #[test]
    fn a_superscript_and_the_text_after_it_join_their_line() {
        let main = text_line("Con n bits, 2", 72.0, 700.0, 12.0);
        let script = text_line("n", main.right, 705.0, 8.0);
        let rest = text_line("valores.", script.right, 700.0, 12.0);
        let items = vec![item(&main), item(&script), item(&rest)];
        let merged = merge_inline_runs(items, 12.0);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].text, "Con n bits, 2 n valores.");
    }

    #[test]
    fn body_size_lines_that_touch_stay_apart() {
        // two columns with a narrow gutter, almost at the same height
        let left = text_line("end of the first column", 72.0, 700.0, 12.0);
        let right = text_line("start of the second", left.right + 2.0, 703.0, 12.0);
        let merged = merge_inline_runs(vec![item(&left), item(&right)], 12.0);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn a_number_in_the_middle_of_a_sentence_is_not_a_list_item() {
        let first = text_line("the result is shown in", 72.0, 700.0, 12.0);
        let second = text_line("3. The next step", 72.0, 687.0, 12.0);
        let page = PageLines { number: 1, width: 595.0, height: 842.0, bottom: 0.0, lines: vec![first, second] };
        let (document, _) = notes_to_document(&[page], NoteOptions::default());
        assert_eq!(document.blocks.len(), 1);
        assert!(matches!(document.blocks[0].kind, BlockKind::Paragraph(_)));
    }

    #[test]
    fn a_sentence_end_is_recognised() {
        assert!(ends_sentence("Done."));
        assert!(ends_sentence("Really?  "));
        assert!(!ends_sentence("and then the"));
    }
}
