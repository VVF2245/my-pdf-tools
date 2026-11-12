//! Slides mode: every page is a slide. The page gives the structure for free:
//! one `##` section per slide, the title being the top text, bullets as nested lists.
//!
//! Titles are found by position, not by size: in many decks title and bullets are almost
//! the same size (18 pt vs 16-18 pt), so the size histogram would not tell them apart.

use mpt_core::{Block, BlockKind, ConversionWarning, Document, Inline, ListItem};

use super::clean::{MarginBands, body_size, fix_loose_accents, join_lines, repeated_margin_lines};
use super::lines::{Line, PageLines};

/// Options of the slides conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlideOptions {
    /// Drop footers and slide numbers repeated on many slides.
    pub remove_furniture: bool,
}

impl Default for SlideOptions {
    fn default() -> Self {
        Self { remove_furniture: true }
    }
}

/// Bullet glyphs: unambiguous, so the text may follow without a space.
const GLYPH_BULLETS: &[char] = &[
    '\u{2022}', '\u{25E6}', '\u{25AA}', '\u{2023}', '\u{25CF}', '\u{25CB}', '\u{25A0}', '\u{25A1}',
    '\u{27A2}', '\u{27A4}', '\u{25BA}',
];
/// Characters that are a bullet only when a space follows (`-5 °C`, `*note` are not).
const SPACED_BULLETS: &[char] = &['-', '\u{2013}', '\u{2212}', '\u{00B7}', '*'];

/// Text smaller than this fraction of the body is page furniture (section labels, logos),
/// not a title, even when it sits at the very top.
const SMALL_TEXT_FACTOR: f32 = 0.95;
/// A title clearly bigger than the body may wrap onto several lines.
const TITLE_SIZE_FACTOR: f32 = 1.05;
/// A line closer than this (in line heights) below the previous one continues it.
const CONTINUATION_GAP: f32 = 1.7;
/// A title line closer than this (in line heights) below the previous one is the same title.
const TITLE_WRAP_GAP: f32 = 1.4;
/// Bullets whose left edges differ by less than this (points) are at the same level.
const INDENT_TOLERANCE: f32 = 4.0;
/// A line higher than the previous one by more than this starts a new text box.
const NEW_BOX_RISE: f32 = 1.0;
/// Lines whose left edges are further apart than this many line heights are in different
/// text boxes (nested bullets are indented by a fraction of this). Limit: two columns
/// whose left edges are closer than this (108 pt at 18 pt) read as a single box.
const SAME_BOX_REACH: f32 = 6.0;
/// A wrapped line must start within this many line heights of where the bullet text starts.
const HANGING_INDENT_TOLERANCE: f32 = 0.5;

/// Converts the slides of a document into a [`Document`]: one level-2 heading per slide.
pub fn slides_to_document(pages: &[PageLines], options: SlideOptions) -> (Document, Vec<ConversionWarning>) {
    let body = body_size(pages);
    let flags = if options.remove_furniture {
        repeated_margin_lines(pages, MarginBands::BottomOnly)
    } else {
        pages.iter().map(|p| vec![false; p.lines.len()]).collect()
    };

    let mut blocks = Vec::new();
    let mut warnings = Vec::new();
    for (page, page_flags) in pages.iter().zip(&flags) {
        let lines: Vec<&Line> =
            page.lines.iter().zip(page_flags).filter(|(_, removed)| !**removed).map(|(l, _)| l).collect();
        if lines.is_empty() {
            warnings.push(ConversionWarning {
                code: "emptySlide".to_string(),
                detail: String::new(),
                page: Some(page.number),
            });
            continue;
        }
        let first_block = blocks.len();
        blocks.extend(slide_blocks(&lines, body));
        for block in &mut blocks[first_block..] {
            block.page = Some(page.number);
        }
    }
    (Document { blocks, ..Default::default() }, warnings)
}

/// The slide title: the topmost line that is not small furniture text.
fn title_index(lines: &[&Line], body: f32) -> usize {
    // the first line in stream order wins a tie
    let topmost = |candidates: Vec<(usize, &&Line)>| {
        candidates
            .into_iter()
            .max_by(|a, b| a.1.baseline.total_cmp(&b.1.baseline).then(b.0.cmp(&a.0)))
            .map(|(i, _)| i)
    };
    let not_small: Vec<_> =
        lines.iter().enumerate().filter(|(_, l)| l.size >= body * SMALL_TEXT_FACTOR).collect();
    topmost(not_small).or_else(|| topmost(lines.iter().enumerate().collect())).unwrap_or(0)
}

/// Indices of the title lines, top to bottom. A title clearly bigger than the body may wrap:
/// the lines right below it, with its size and font, belong to it. (When the title is the
/// same size as the body this is not attempted: it would swallow the first bullet.)
fn title_lines(lines: &[&Line], body: f32) -> Vec<usize> {
    let first = title_index(lines, body);
    let title = lines[first];
    let mut picked = vec![first];
    if title.size > body * TITLE_SIZE_FACTOR {
        let mut current = title.baseline;
        while let Some((i, next)) = lines
            .iter()
            .enumerate()
            .filter(|(i, l)| !picked.contains(i) && l.baseline < current)
            .max_by(|a, b| a.1.baseline.total_cmp(&b.1.baseline))
        {
            let wraps = (next.size - title.size).abs() < 0.5
                && next.font == title.font
                && current - next.baseline <= TITLE_WRAP_GAP * title.size
                && split_marker(&next.text).0.is_none();
            if !wraps {
                break;
            }
            picked.push(i);
            current = next.baseline;
        }
    }
    picked
}

/// One slide: its title as a heading, then its bullets and paragraphs.
fn slide_blocks(lines: &[&Line], body: f32) -> Vec<Block> {
    let title = title_lines(lines, body);
    let title_texts: Vec<String> = title.iter().map(|&i| fix_loose_accents(lines[i])).collect();
    let mut blocks = vec![Block::new(BlockKind::Heading {
        level: 2,
        content: vec![Inline::Text(join_lines(&title_texts))],
    })];

    let rest: Vec<&Line> = lines.iter().enumerate().filter(|(i, _)| !title.contains(i)).map(|(_, l)| *l).collect();
    blocks.extend(body_blocks(&rest));
    blocks
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Marker {
    pub(super) ordered: bool,
    /// The number written in an ordered marker (`3.` -> 3).
    pub(super) number: Option<u64>,
}

/// Splits a leading bullet marker (`•`, `- `, `1.`) from the text.
pub(super) fn split_marker(text: &str) -> (Option<Marker>, &str) {
    let mut chars = text.chars();
    let Some(first) = chars.next() else { return (None, text) };
    let rest = chars.as_str();
    if GLYPH_BULLETS.contains(&first) || (SPACED_BULLETS.contains(&first) && rest.starts_with(' ')) {
        return (Some(Marker { ordered: false, number: None }), rest.trim_start());
    }
    let digits = text.chars().take_while(char::is_ascii_digit).count();
    if (1..=3).contains(&digits) {
        if let Some(after) = text[digits..].strip_prefix(['.', ')']) {
            if after.starts_with(' ') {
                let number = text[..digits].parse().ok();
                return (Some(Marker { ordered: true, number }), after.trim_start());
            }
        }
    }
    (None, text)
}

/// A line of the slide body, parsed.
struct Entry {
    bullet: Option<Marker>,
    text: String,
    left: f32,
    /// Where the text starts: after the marker for a bullet, else the line's left edge.
    text_left: f32,
    baseline: f32,
    size: f32,
}

/// A bullet being collected: its text lines and where its marker sits.
pub(super) struct Bullet {
    pub(super) marker: Marker,
    pub(super) left: f32,
    /// Where the text of the first line starts: wrapped lines are aligned with it.
    pub(super) text_left: f32,
    pub(super) lines: Vec<String>,
}

/// Turns the body lines of one slide into paragraphs and lists, in reading order.
#[derive(Default)]
struct BodyBuilder {
    blocks: Vec<Block>,
    bullets: Vec<Bullet>,
    paragraph: Vec<String>,
}

impl BodyBuilder {
    fn flush_bullets(&mut self) {
        if !self.bullets.is_empty() {
            self.blocks.extend(list_blocks(std::mem::take(&mut self.bullets)));
        }
    }

    fn flush_paragraph(&mut self) {
        if !self.paragraph.is_empty() {
            let text = join_lines(&self.paragraph);
            self.blocks.push(Block::new(BlockKind::Paragraph(vec![Inline::Text(text)])));
            self.paragraph.clear();
        }
    }

    fn finish(mut self) -> Vec<Block> {
        self.flush_bullets();
        self.flush_paragraph();
        self.blocks
    }
}

/// Puts the lines of a slide in reading order. The content stream usually lists a text box
/// top to bottom, but it may be written bottom-up. Each run of lines that keeps rising is
/// reversed; a change of direction (a jump to the next box) ends the run.
///
/// Note: the bottom-up case is a precaution that only synthetic tests cover; no real file
/// in the test set was written that way. Top-down streams (every real deck tried) pass
/// through unchanged.
fn reading_order<'a>(lines: &[&'a Line]) -> Vec<&'a Line> {
    #[derive(Clone, Copy, PartialEq)]
    enum Direction {
        Down,
        Up,
    }
    fn close<'a>(run: &mut Vec<&'a Line>, direction: Option<Direction>, out: &mut Vec<&'a Line>) {
        if direction == Some(Direction::Up) {
            run.reverse();
        }
        out.append(run);
    }

    let mut out = Vec::with_capacity(lines.len());
    let mut run: Vec<&'a Line> = Vec::new();
    let mut direction: Option<Direction> = None;
    for &line in lines {
        if let Some(last) = run.last() {
            // A jump to another column is the next text box, whatever the direction.
            if (line.left - last.left).abs() > SAME_BOX_REACH * line.size {
                close(&mut run, direction, &mut out);
                direction = None;
                run.push(line);
                continue;
            }
            let step = line.baseline - last.baseline;
            let moving = if step > NEW_BOX_RISE {
                Some(Direction::Up)
            } else if step < -NEW_BOX_RISE {
                Some(Direction::Down)
            } else {
                None // same baseline
            };
            match (direction, moving) {
                (_, None) => {}
                (None, now) => direction = now,
                (Some(before), Some(now)) if before == now => {}
                _ => {
                    close(&mut run, direction, &mut out);
                    direction = None;
                }
            }
        }
        run.push(line);
    }
    close(&mut run, direction, &mut out);
    out
}

/// Where the text of `line` starts once the marker (the first `consumed` characters of
/// its text) is skipped: the left edge of the first visible glyph after the marker.
///
/// `text` is the line's text after accent recomposition, while `line.glyphs` are not
/// recomposed. They agree on the marker because it is at the start of the line; an accent
/// glyph before the marker would shift the count.
pub(super) fn text_start(line: &Line, text: &str, rest: &str) -> f32 {
    let consumed = text.chars().count() - rest.chars().count();
    let marker_glyphs = text.chars().take(consumed).filter(|c| !c.is_whitespace()).count();
    line.glyphs
        .iter()
        .filter(|g| !g.ch.is_whitespace())
        .nth(marker_glyphs)
        .map_or(line.left, |g| g.left)
}

fn body_blocks(lines: &[&Line]) -> Vec<Block> {
    let entries: Vec<Entry> = reading_order(lines)
        .into_iter()
        .map(|line| {
            let text = fix_loose_accents(line);
            let (bullet, rest) = split_marker(&text);
            let text_left = if bullet.is_some() { text_start(line, &text, rest) } else { line.left };
            Entry {
                bullet,
                text: rest.to_string(),
                left: line.left,
                text_left,
                baseline: line.baseline,
                size: line.size,
            }
        })
        .collect();

    let mut builder = BodyBuilder::default();
    let mut previous: Option<&Entry> = None;
    for entry in &entries {
        if let Some(prev) = previous {
            // A line higher up than the previous one is the next text box (another column).
            if entry.baseline - prev.baseline > NEW_BOX_RISE {
                builder.flush_bullets();
                builder.flush_paragraph();
            }
        }
        // Directly below the previous line, close enough to continue it.
        let gap = previous.map(|p| p.baseline - entry.baseline);
        let close = gap.is_some_and(|g| g > 0.0 && g <= CONTINUATION_GAP * entry.size.max(previous.map_or(0.0, |p| p.size)));

        match entry.bullet {
            Some(marker) => {
                builder.flush_paragraph();
                builder.bullets.push(Bullet {
                    marker,
                    left: entry.left,
                    text_left: entry.text_left,
                    lines: vec![entry.text.clone()],
                });
            }
            None => {
                // aligned with the text of the current bullet (hanging indent): its wrapped
                // line. A remark indented some other way is a paragraph of its own.
                let wraps_bullet = builder.bullets.last().is_some_and(|b| {
                    entry.left > b.left + 0.3 * entry.size
                        && (entry.left - b.text_left).abs() <= HANGING_INDENT_TOLERANCE * entry.size
                });
                if close && wraps_bullet {
                    builder.bullets.last_mut().expect("a bullet is open").lines.push(entry.text.clone());
                } else {
                    let continues = close
                        && !builder.paragraph.is_empty()
                        && previous.is_some_and(|p| (entry.left - p.left).abs() < 0.5 * entry.size);
                    if !continues {
                        builder.flush_bullets();
                        builder.flush_paragraph();
                    }
                    builder.paragraph.push(entry.text.clone());
                }
            }
        }
        previous = Some(entry);
    }
    builder.finish()
}

/// The nesting level of each bullet: the distinct left edges of the run, left to right.
fn bullet_levels(bullets: &[Bullet]) -> Vec<usize> {
    let mut edges: Vec<f32> = bullets.iter().map(|b| b.left).collect();
    edges.sort_by(f32::total_cmp);
    let mut columns: Vec<f32> = Vec::new();
    for edge in edges {
        if columns.last().is_none_or(|last| edge - last > INDENT_TOLERANCE) {
            columns.push(edge);
        }
    }
    let raw: Vec<usize> = bullets
        .iter()
        .map(|b| columns.iter().rposition(|c| b.left + INDENT_TOLERANCE >= *c).unwrap_or(0))
        .collect();
    // A bullet cannot be more than one level deeper than the one before it.
    let mut levels: Vec<usize> = Vec::with_capacity(raw.len());
    for (i, level) in raw.iter().enumerate() {
        levels.push(if i == 0 { 0 } else { (*level).min(levels[i - 1] + 1) });
    }
    levels
}

/// Builds the (nested) lists of a run of bullets. More than one comes out when the kind of
/// marker changes at the same level (numbers, then bullets).
pub(super) fn list_blocks(bullets: Vec<Bullet>) -> Vec<Block> {
    let levels = bullet_levels(&bullets);
    let mut next = 0;
    let mut lists = Vec::new();
    while next < bullets.len() {
        lists.push(build_list(&bullets, &levels, &mut next, 0));
    }
    lists
}

fn build_list(bullets: &[Bullet], levels: &[usize], next: &mut usize, level: usize) -> Block {
    let first = &bullets[*next];
    let ordered = first.marker.ordered;
    let start = if ordered { first.marker.number } else { None };
    let mut items = Vec::new();
    while *next < bullets.len() && levels[*next] >= level {
        if levels[*next] > level {
            // deeper than this list and without a parent item: attach to the last item
            let nested = build_list(bullets, levels, next, level + 1);
            if let Some(last) = items.last_mut() {
                let last: &mut ListItem = last;
                last.blocks.push(nested);
            }
            continue;
        }
        if bullets[*next].marker.ordered != ordered {
            break; // numbers after bullets (or the reverse): a different list starts here
        }
        let text = join_lines(&bullets[*next].lines);
        *next += 1;
        let mut blocks = vec![Block::new(BlockKind::Paragraph(vec![Inline::Text(text)]))];
        if *next < bullets.len() && levels[*next] > level {
            blocks.push(build_list(bullets, levels, next, level + 1));
        }
        items.push(ListItem { checked: None, blocks });
    }
    Block::new(BlockKind::List { ordered, start, items })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf_reader::glyphs::Glyph;
    use std::sync::Arc;

    fn line(text: &str, left: f32, baseline: f32, size: f32) -> Line {
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

    fn slide(number: u32, lines: Vec<Line>) -> PageLines {
        PageLines { number, width: 720.0, height: 405.0, bottom: 0.0, lines }
    }

    fn convert(pages: Vec<PageLines>) -> Document {
        slides_to_document(&pages, SlideOptions::default()).0
    }

    fn heading(doc: &Document, i: usize) -> String {
        match &doc.blocks[i].kind {
            BlockKind::Heading { level: 2, content } => match &content[0] {
                Inline::Text(t) => t.clone(),
                other => panic!("{other:?}"),
            },
            other => panic!("not a level-2 heading: {other:?}"),
        }
    }

    fn paragraph(block: &BlockKind) -> String {
        match block {
            BlockKind::Paragraph(p) => match &p[0] {
                Inline::Text(t) => t.clone(),
                other => panic!("{other:?}"),
            },
            other => panic!("not a paragraph: {other:?}"),
        }
    }

    fn items(block: &BlockKind) -> Vec<String> {
        match block {
            BlockKind::List { items, .. } => items.iter().map(|item| paragraph(&item.blocks[0].kind)).collect(),
            other => panic!("not a list: {other:?}"),
        }
    }

    #[test]
    fn the_top_line_is_the_title_even_when_bullets_are_as_big() {
        // 18 pt title and 18 pt bullets (a common deck): only the position tells them apart
        let doc = convert(vec![slide(
            1,
            vec![
                line("\u{2022} first point", 40.0, 300.0, 18.0),
                line("The title", 40.0, 360.0, 18.0),
                line("\u{2022} second point", 40.0, 270.0, 18.0),
            ],
        )]);
        assert_eq!(heading(&doc, 0), "The title");
        assert_eq!(items(&doc.blocks[1].kind), ["first point", "second point"]);
    }

    #[test]
    fn small_text_at_the_very_top_is_not_the_title() {
        // a 9 pt section label above the real 20 pt title
        let doc = convert(vec![slide(
            1,
            vec![
                line("Tema 2 - SQL", 40.0, 395.0, 9.0),
                line("Real title", 40.0, 360.0, 20.0),
                line("\u{2022} a point of the slide body", 40.0, 300.0, 18.0),
            ],
        )]);
        assert_eq!(heading(&doc, 0), "Real title");
        // the label is not lost: it stays in the slide as a paragraph
        assert_eq!(paragraph(&doc.blocks[1].kind), "Tema 2 - SQL");
    }

    #[test]
    fn a_title_that_wraps_is_one_heading() {
        let doc = convert(vec![slide(
            1,
            vec![
                line("A long title that wraps", 40.0, 360.0, 24.0),
                line("onto a second line", 40.0, 332.0, 24.0),
                // enough body text for 18 pt to be the document's body size
                line("\u{2022} a bullet of the body with enough text to dominate", 40.0, 280.0, 18.0),
                line("\u{2022} another bullet of the body with enough text", 40.0, 256.0, 18.0),
            ],
        )]);
        assert_eq!(heading(&doc, 0), "A long title that wraps onto a second line");
        assert_eq!(doc.blocks.len(), 2);
    }

    #[test]
    fn a_title_the_size_of_the_body_does_not_swallow_the_next_line() {
        let doc = convert(vec![slide(
            1,
            vec![
                line("Title at body size", 40.0, 360.0, 18.0),
                line("A paragraph right below", 40.0, 338.0, 18.0),
                line("\u{2022} and a bullet", 40.0, 300.0, 18.0),
            ],
        )]);
        assert_eq!(heading(&doc, 0), "Title at body size");
        assert_eq!(paragraph(&doc.blocks[1].kind), "A paragraph right below");
    }

    #[test]
    fn a_big_line_far_below_is_not_part_of_the_title() {
        let doc = convert(vec![slide(
            1,
            vec![
                line("Title", 40.0, 360.0, 24.0),
                line("Another big line", 40.0, 250.0, 24.0),
                line("\u{2022} body", 40.0, 200.0, 18.0),
            ],
        )]);
        assert_eq!(heading(&doc, 0), "Title");
        assert_eq!(paragraph(&doc.blocks[1].kind), "Another big line");
    }

    #[test]
    fn indented_bullets_become_nested_lists() {
        let doc = convert(vec![slide(
            1,
            vec![
                line("Title", 40.0, 360.0, 20.0),
                line("\u{2022} one", 40.0, 300.0, 18.0),
                line("\u{2022} two", 40.0, 276.0, 18.0),
                line("\u{2023} two-a", 60.0, 252.0, 18.0),
                line("\u{2023} two-b", 60.0, 228.0, 18.0),
                line("\u{2022} three", 40.0, 204.0, 18.0),
            ],
        )]);
        assert_eq!(doc.blocks.len(), 2);
        let BlockKind::List { items: top, .. } = &doc.blocks[1].kind else { panic!() };
        assert_eq!(top.len(), 3);
        assert_eq!(top[1].blocks.len(), 2, "the second bullet carries a nested list");
        assert_eq!(items(&top[1].blocks[1].kind), ["two-a", "two-b"]);
    }

    #[test]
    fn a_skipped_level_is_clamped() {
        let doc = convert(vec![slide(
            1,
            vec![
                line("Title", 40.0, 360.0, 20.0),
                line("\u{2022} top", 40.0, 300.0, 18.0),
                line("\u{2022} very deep", 100.0, 276.0, 18.0),
                line("\u{2022} back", 40.0, 252.0, 18.0),
            ],
        )]);
        let BlockKind::List { items: top, .. } = &doc.blocks[1].kind else { panic!() };
        assert_eq!(top.len(), 2);
        assert_eq!(items(&top[0].blocks[1].kind), ["very deep"]);
    }

    #[test]
    fn a_wrapped_bullet_is_joined_and_dehyphenated() {
        let doc = convert(vec![slide(
            1,
            vec![
                line("Title", 40.0, 360.0, 20.0),
                line("\u{2022} a long bullet that wraps onto a sec-", 40.0, 300.0, 18.0),
                line("ond line", 58.0, 280.0, 18.0),
                line("\u{2022} next", 40.0, 250.0, 18.0),
            ],
        )]);
        assert_eq!(items(&doc.blocks[1].kind), ["a long bullet that wraps onto a second line", "next"]);
    }

    #[test]
    fn a_bullet_wrapped_onto_three_lines_stays_one_bullet() {
        let doc = convert(vec![slide(
            1,
            vec![
                line("Title", 40.0, 360.0, 20.0),
                line("\u{2022} first line of a long bullet", 40.0, 300.0, 18.0),
                line("second line of it", 58.0, 280.0, 18.0),
                line("third line of it", 58.0, 260.0, 18.0),
                line("\u{2022} next bullet", 40.0, 230.0, 18.0),
            ],
        )]);
        assert_eq!(doc.blocks.len(), 2, "one list, no stray paragraph");
        assert_eq!(
            items(&doc.blocks[1].kind),
            ["first line of a long bullet second line of it third line of it", "next bullet"]
        );
    }

    #[test]
    fn two_text_boxes_are_not_glued_together() {
        // stream order: the whole left box, then the right box starting higher up
        let doc = convert(vec![slide(
            1,
            vec![
                line("Title", 40.0, 360.0, 20.0),
                line("\u{2022} left one", 40.0, 300.0, 18.0),
                line("\u{2022} left two", 40.0, 276.0, 18.0),
                line("\u{2022} right one", 400.0, 300.0, 18.0),
                line("\u{2022} right two", 400.0, 276.0, 18.0),
            ],
        )]);
        assert_eq!(doc.blocks.len(), 3, "heading and one list per box");
        assert_eq!(items(&doc.blocks[1].kind), ["left one", "left two"]);
        assert_eq!(items(&doc.blocks[2].kind), ["right one", "right two"]);
    }

    #[test]
    fn a_line_in_another_box_is_not_a_continuation() {
        let doc = convert(vec![slide(
            1,
            vec![
                line("Title", 40.0, 360.0, 20.0),
                line("\u{2022} left bullet", 40.0, 280.0, 18.0),
                line("text of the right box", 400.0, 300.0, 18.0),
            ],
        )]);
        assert_eq!(items(&doc.blocks[1].kind), ["left bullet"]);
        assert_eq!(paragraph(&doc.blocks[2].kind), "text of the right box");
    }

    #[test]
    fn a_text_box_written_bottom_up_is_read_top_down() {
        // some exporters write the lines of a box from the last to the first
        let doc = convert(vec![slide(
            1,
            vec![
                line("Title", 40.0, 360.0, 20.0),
                line("\u{2022} third", 40.0, 252.0, 18.0),
                line("\u{2022} second", 40.0, 276.0, 18.0),
                line("\u{2022} first", 40.0, 300.0, 18.0),
            ],
        )]);
        assert_eq!(doc.blocks.len(), 2, "one list, not three");
        assert_eq!(items(&doc.blocks[1].kind), ["first", "second", "third"]);
    }

    #[test]
    fn a_bottom_up_box_keeps_its_nesting() {
        let doc = convert(vec![slide(
            1,
            vec![
                line("Title", 40.0, 360.0, 20.0),
                line("\u{2023} child b", 60.0, 252.0, 18.0),
                line("\u{2023} child a", 60.0, 276.0, 18.0),
                line("\u{2022} parent", 40.0, 300.0, 18.0),
            ],
        )]);
        let BlockKind::List { items: top, .. } = &doc.blocks[1].kind else { panic!() };
        assert_eq!(top.len(), 1);
        assert_eq!(items(&top[0].blocks[1].kind), ["child a", "child b"]);
    }

    #[test]
    fn top_down_boxes_are_left_in_stream_order() {
        // two boxes, each top-down, the second starting higher: no reversal anywhere
        let doc = convert(vec![slide(
            1,
            vec![
                line("Title", 40.0, 360.0, 20.0),
                line("\u{2022} left one", 40.0, 300.0, 18.0),
                line("\u{2022} left two", 40.0, 276.0, 18.0),
                line("\u{2022} right one", 400.0, 320.0, 18.0),
                line("\u{2022} right two", 400.0, 296.0, 18.0),
            ],
        )]);
        assert_eq!(items(&doc.blocks[1].kind), ["left one", "left two"]);
        assert_eq!(items(&doc.blocks[2].kind), ["right one", "right two"]);
    }

    #[test]
    fn numbers_followed_by_bullets_are_two_lists() {
        let doc = convert(vec![slide(
            1,
            vec![
                line("Title", 40.0, 360.0, 20.0),
                line("1. one", 40.0, 300.0, 18.0),
                line("2. two", 40.0, 276.0, 18.0),
                line("\u{2022} bullet", 40.0, 252.0, 18.0),
            ],
        )]);
        assert_eq!(doc.blocks.len(), 3);
        assert!(matches!(&doc.blocks[1].kind, BlockKind::List { ordered: true, .. }));
        assert!(matches!(&doc.blocks[2].kind, BlockKind::List { ordered: false, .. }));
        assert_eq!(items(&doc.blocks[2].kind), ["bullet"]);
    }

    #[test]
    fn a_remark_indented_below_the_last_bullet_is_its_own_paragraph() {
        // text of the bullet starts at x=58; the remark sits at x=100: not a wrapped line
        let doc = convert(vec![slide(
            1,
            vec![
                line("Title", 40.0, 360.0, 20.0),
                line("\u{2022} the last bullet", 40.0, 300.0, 18.0),
                line("Note: an indented remark", 100.0, 280.0, 18.0),
            ],
        )]);
        assert_eq!(items(&doc.blocks[1].kind), ["the last bullet"]);
        assert_eq!(paragraph(&doc.blocks[2].kind), "Note: an indented remark");
    }

    #[test]
    fn numbered_bullets_make_an_ordered_list_that_keeps_its_first_number() {
        let doc = convert(vec![slide(
            1,
            vec![line("Steps", 40.0, 360.0, 20.0), line("3. third", 40.0, 300.0, 18.0), line("4) fourth", 40.0, 276.0, 18.0)],
        )]);
        assert!(matches!(&doc.blocks[1].kind, BlockKind::List { ordered: true, start: Some(3), .. }));
        assert_eq!(items(&doc.blocks[1].kind), ["third", "fourth"]);
    }

    #[test]
    fn plain_lines_make_paragraphs_and_a_year_is_not_a_bullet() {
        let doc = convert(vec![slide(
            1,
            vec![
                line("Title", 40.0, 360.0, 20.0),
                line("2026 was a good year", 40.0, 300.0, 18.0),
                line("and it kept going", 40.0, 280.0, 18.0),
                line("A separate remark", 40.0, 200.0, 18.0),
            ],
        )]);
        assert_eq!(doc.blocks.len(), 3);
        assert_eq!(paragraph(&doc.blocks[1].kind), "2026 was a good year and it kept going");
        assert_eq!(paragraph(&doc.blocks[2].kind), "A separate remark");
    }

    #[test]
    fn ambiguous_markers_need_a_space() {
        assert!(split_marker("-nothing").0.is_none());
        assert!(split_marker("\u{2212}5 \u{00B0}C").0.is_none(), "a minus sign is not a bullet");
        assert!(split_marker("*note").0.is_none());
        assert!(split_marker("\u{2013}10 a 10").0.is_none());
        assert!(split_marker("- item").0.is_some());
        assert!(split_marker("* item").0.is_some());
        assert!(split_marker("\u{2022}item").0.is_some(), "a true bullet glyph needs no space");
        assert!(split_marker("").0.is_none());
    }

    #[test]
    fn a_negative_temperature_line_keeps_its_minus() {
        let doc = convert(vec![slide(
            1,
            vec![line("Weather", 40.0, 360.0, 20.0), line("\u{2212}5 \u{00B0}C at night", 40.0, 300.0, 18.0)],
        )]);
        assert_eq!(paragraph(&doc.blocks[1].kind), "\u{2212}5 \u{00B0}C at night");
    }

    #[test]
    fn every_block_remembers_its_slide() {
        let doc = convert(vec![
            slide(1, vec![line("One", 40.0, 360.0, 20.0)]),
            slide(2, vec![line("Two", 40.0, 360.0, 20.0), line("\u{2022} x", 40.0, 300.0, 18.0)]),
        ]);
        let pages: Vec<Option<u32>> = doc.blocks.iter().map(|b| b.page).collect();
        assert_eq!(pages, [Some(1), Some(2), Some(2)]);
        assert_eq!(heading(&doc, 0), "One");
        assert_eq!(heading(&doc, 1), "Two");
    }

    #[test]
    fn furniture_is_removed_only_when_asked() {
        let make = || -> Vec<PageLines> {
            (1..=4u32)
                .map(|n| {
                    slide(
                        n,
                        vec![
                            line(&format!("Slide {n} title"), 40.0, 360.0, 20.0),
                            line("\u{2022} some body text here", 40.0, 300.0, 18.0),
                            line(&format!("Course 2026 - {n}"), 40.0, 20.0, 9.0),
                        ],
                    )
                })
                .collect()
        };
        let with = slides_to_document(&make(), SlideOptions { remove_furniture: true }).0;
        let without = slides_to_document(&make(), SlideOptions { remove_furniture: false }).0;
        let has_footer = |doc: &Document| {
            doc.blocks.iter().any(|b| matches!(&b.kind, BlockKind::Paragraph(p) if matches!(&p[0], Inline::Text(t) if t.starts_with("Course 2026"))))
        };
        assert!(!has_footer(&with));
        assert!(has_footer(&without));
    }

    #[test]
    fn a_slide_with_only_furniture_is_reported_empty() {
        let pages: Vec<PageLines> = (1..=4u32).map(|n| slide(n, vec![line(&format!("Footer {n}"), 40.0, 20.0, 9.0)])).collect();
        let (doc, warnings) = slides_to_document(&pages, SlideOptions::default());
        assert!(doc.blocks.is_empty());
        assert_eq!(warnings.len(), 4);
        assert_eq!(warnings[0].code, "emptySlide");
        assert_eq!(warnings[0].page, Some(1));
    }

    #[test]
    fn loose_latex_accents_are_fixed_in_titles_and_bullets() {
        let doc = convert(vec![slide(
            1,
            vec![line("Introducci\u{00B4}on", 40.0, 360.0, 20.0), line("\u{2022} m\u{00B4}aximo", 40.0, 300.0, 18.0)],
        )]);
        assert_eq!(heading(&doc, 0), "Introducción");
        assert_eq!(items(&doc.blocks[1].kind), ["máximo"]);
    }

    #[test]
    fn no_slides_no_document() {
        let (doc, warnings) = slides_to_document(&[], SlideOptions::default());
        assert!(doc.blocks.is_empty() && warnings.is_empty());
    }
}
