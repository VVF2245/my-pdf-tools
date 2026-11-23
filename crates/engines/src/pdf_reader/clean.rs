//! Stage 3/5: text clean-up that does not depend on the document mode.
//!
//! Everything here is a pure function over text or lines, so it is easy to test.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use unicode_normalization::UnicodeNormalization;

use super::lines::{Line, PageLines};

/// Spacing accents that LaTeX fonts emit as separate glyphs before the letter,
/// with the combining mark they stand for.
const LOOSE_ACCENTS: [(char, char); 5] = [
    ('\u{00B4}', '\u{0301}'), // acute accent
    ('`', '\u{0300}'),        // grave accent
    ('\u{00A8}', '\u{0308}'), // diaeresis
    ('\u{02C6}', '\u{0302}'), // circumflex (modifier letter)
    ('\u{02DC}', '\u{0303}'), // tilde (modifier letter)
];

/// Text of `line` with loose accents recomposed (`introducci´on` -> `introducción`).
///
/// Monospaced lines are returned as they are: a backtick before a letter is legitimate code.
pub fn fix_loose_accents(line: &Line) -> String {
    if line.mono {
        line.text.clone()
    } else {
        recompose_accents(&line.text)
    }
}

/// The letter with the accent applied, only when that exists as one precomposed character.
fn compose(letter: char, mark: char) -> Option<char> {
    // The dotless i of LaTeX fonts would not compose with the accent.
    let letter = if letter == '\u{0131}' { 'i' } else { letter };
    let mut composed = [letter, mark].into_iter().nfc();
    match (composed.next(), composed.next()) {
        (Some(single), None) => Some(single),
        _ => None,
    }
}

fn recompose_accents(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        let mark = LOOSE_ACCENTS.iter().find(|(accent, _)| *accent == c).map(|(_, m)| *m);
        let composed = mark.zip(chars.peek().copied()).and_then(|(m, next)| compose(next, m));
        match composed {
            Some(letter) => {
                chars.next();
                out.push(letter);
            }
            // Not followed by a letter that can carry it (an apostrophe typed as an accent,
            // for instance): keep the character as it is.
            None => out.push(c),
        }
    }
    out.nfc().collect()
}

/// Hyphens that can end a line inside a word.
const HYPHEN: char = '-';
const HYPHEN_UNICODE: char = '\u{2010}';
/// Invisible: only ever marks a place where a word may be broken.
const SOFT_HYPHEN: char = '\u{00AD}';

/// Joins the lines of one paragraph into a single string.
///
/// A line that ends in a hyphen after a letter and is followed by a lowercase letter is
/// joined without the hyphen (`pre-` / `training` -> `pretraining`). A soft hyphen at the end
/// of a line always joins the word, and stray soft hyphens are removed. This is a heuristic:
/// a real compound split at the line end (`state-` / `of-the-art`) loses its hyphen.
pub fn join_lines<S: AsRef<str>>(lines: &[S]) -> String {
    let mut out = String::new();
    for line in lines {
        let line = line.as_ref().trim();
        if line.is_empty() {
            continue;
        }
        if out.is_empty() {
            out.push_str(line);
            continue;
        }
        let last = out.chars().last();
        let after_letter = out.chars().rev().nth(1).is_some_and(char::is_alphabetic);
        let next_is_lowercase = line.chars().next().is_some_and(char::is_lowercase);
        if last == Some(SOFT_HYPHEN) {
            out.pop();
        } else if matches!(last, Some(HYPHEN | HYPHEN_UNICODE)) && after_letter && next_is_lowercase {
            out.pop();
        } else {
            out.push(' ');
        }
        out.push_str(line);
    }
    out.retain(|c| c != SOFT_HYPHEN);
    out
}

/// Characters used as leaders, with the number of dots they count as.
fn leader_weight(c: char) -> Option<usize> {
    match c {
        '.' | '\u{00B7}' => Some(1), // full stop, middle dot
        '\u{2026}' => Some(3),       // ellipsis
        _ => None,
    }
}

/// Replaces the dot leaders of an index line (`Chapter 1 . . . . . 63`) by one space.
/// Returns the cleaned text and whether the line had leaders.
///
/// A line made only of leaders gives an empty string: callers must skip empty lines.
pub fn strip_dot_leaders(text: &str) -> (String, bool) {
    const MIN_DOTS: usize = 4;
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut found = false;
    let mut i = 0;
    while i < chars.len() {
        if leader_weight(chars[i]).is_some() {
            // A run of leader characters, optionally separated by spaces.
            let (mut j, mut dots, mut end) = (i, 0, i);
            while j < chars.len() && (chars[j] == ' ' || leader_weight(chars[j]).is_some()) {
                if let Some(w) = leader_weight(chars[j]) {
                    dots += w;
                    end = j + 1;
                }
                j += 1;
            }
            if dots >= MIN_DOTS {
                found = true;
                if !out.ends_with(' ') && !out.is_empty() {
                    out.push(' ');
                }
                i = end;
                while i < chars.len() && chars[i] == ' ' {
                    i += 1;
                }
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    (out.trim_end().to_string(), found)
}

/// A page's header or footer must sit this close to the top or bottom edge
/// (fraction of the page height).
const MARGIN_BAND: f32 = 0.12;
/// Lines at the same place in at least this many pages (and fraction of pages) are repeated.
const MIN_REPEATS: usize = 3;
const MIN_REPEAT_FRACTION: f32 = 0.3;
/// Vertical distance, in points, below which two positions are "the same".
const POSITION_TOLERANCE: f32 = 4.0;

/// Whether a margin line may be a header or footer. Lines with letters qualify. Without
/// letters only a page number does (a few digits, optionally between dashes, bars or a
/// slash): rows of a numeric table that sit in the band must not be taken for a footer.
fn eligible_for_repeat(text: &str) -> bool {
    if text.chars().any(char::is_alphabetic) {
        return true;
    }
    let digits = text.chars().filter(char::is_ascii_digit).count();
    let only_page_number_chars = text
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, '-' | '\u{2013}' | '\u{2014}' | '|' | '/' | ' '));
    digits > 0 && digits <= 6 && only_page_number_chars
}

/// A run of digits (with the spaces a kerned page number may show, `1 2`) becomes one `#`, so
/// `Page 3 | 13` and `Page 4 | 13` compare equal, and so do `9 Title` and `10 Title`.
fn normalize_for_repeat(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_run = false;
    let mut spaces = 0;
    for c in text.chars() {
        if c.is_ascii_digit() {
            if !in_run {
                out.push('#');
                in_run = true;
            }
            spaces = 0;
        } else if c == ' ' && in_run {
            spaces += 1;
        } else {
            if spaces > 0 {
                out.push(' ');
            }
            spaces = 0;
            in_run = false;
            out.push(c);
        }
    }
    out
}

/// Which page margins may hold repeated furniture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarginBands {
    /// Running headers (top) and footers (bottom): articles and notes.
    TopAndBottom,
    /// Footers only: on slides the top of the page holds the slide title, and titles such
    /// as "Example" or "Summary" legitimately repeat.
    BottomOnly,
}

/// The font size most glyphs are set in (size rounded to 0.1 pt), or 0 without text.
pub(crate) fn body_size(pages: &[PageLines]) -> f32 {
    let mut glyphs_by_size: BTreeMap<i32, usize> = BTreeMap::new();
    for line in pages.iter().flat_map(|p| &p.lines) {
        *glyphs_by_size.entry((line.size * 10.0).round() as i32).or_default() += line.glyphs.len();
    }
    let mut best = (0, 0);
    for (size, count) in glyphs_by_size {
        if count > best.1 {
            best = (size, count);
        }
    }
    best.0 as f32 / 10.0
}

/// Lines set bigger than the body text by more than this factor are titles, never furniture.
const TITLE_SIZE_FACTOR: f32 = 1.05;

/// Finds headers, footers and page numbers: text that repeats at the same height, in the
/// allowed margin bands, on many pages. Returns, for each page, one flag per line
/// (`true` = remove it).
///
/// Text set bigger than the body is never flagged: it is a heading or a slide title.
pub fn repeated_margin_lines(pages: &[PageLines], bands: MarginBands) -> Vec<Vec<bool>> {
    let body = body_size(pages);
    // (normalized text, top band?) -> every occurrence as (distance from page bottom, page, line)
    let mut occurrences: HashMap<(String, bool), Vec<(f32, usize, usize)>> = HashMap::new();
    for (page_index, page) in pages.iter().enumerate() {
        for (line_index, line) in page.lines.iter().enumerate() {
            if line.size > body * TITLE_SIZE_FACTOR {
                continue;
            }
            let from_bottom = line.baseline - page.bottom;
            let fraction = from_bottom / page.height;
            let top_band = bands == MarginBands::TopAndBottom && fraction > 1.0 - MARGIN_BAND;
            if (top_band || fraction < MARGIN_BAND) && eligible_for_repeat(&line.text) {
                occurrences
                    .entry((normalize_for_repeat(&line.text), top_band))
                    .or_default()
                    .push((from_bottom, page_index, line_index));
            }
        }
    }

    let needed = MIN_REPEATS.max((MIN_REPEAT_FRACTION * pages.len() as f32).ceil() as usize);
    let mut flags: Vec<Vec<bool>> = pages.iter().map(|p| vec![false; p.lines.len()]).collect();
    for mut found in occurrences.into_values() {
        // Cluster by height: neighbours closer than the tolerance are the same position,
        // so a header that drifts a fraction of a point is still one group.
        found.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut start = 0;
        for end in 1..=found.len() {
            if end == found.len() || found[end].0 - found[end - 1].0 > POSITION_TOLERANCE {
                let cluster = &found[start..end];
                let on_pages: BTreeSet<usize> = cluster.iter().map(|&(_, page, _)| page).collect();
                if on_pages.len() >= needed {
                    for &(_, page, line) in cluster {
                        flags[page][line] = true;
                    }
                }
                start = end;
            }
        }
    }
    flags
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_numbers_of_any_width_normalize_alike() {
        assert_eq!(normalize_for_repeat("9 Title"), normalize_for_repeat("10 Title"));
        assert_eq!(normalize_for_repeat("1 2 Title"), normalize_for_repeat("9 Title"));
        assert_eq!(normalize_for_repeat("Page 3 | 13"), normalize_for_repeat("Page 4 | 130"));
        // digits inside text still tell different lines apart
        assert_ne!(normalize_for_repeat("Chapter 3 Title"), normalize_for_repeat("Chapter Title"));
    }
    use crate::pdf_reader::glyphs::Glyph;
    use std::sync::Arc;

    fn make_line(page: u32, text: &str, baseline: f32, mono: bool) -> Line {
        let glyphs = text
            .chars()
            .enumerate()
            .map(|(i, ch)| Glyph {
                ch,
                left: i as f32 * 5.0,
                right: i as f32 * 5.0 + 5.0,
                bottom: baseline,
                top: baseline + 10.0,
                size: 10.0,
                font: Arc::from("F"),
                mono,
            })
            .collect();
        Line::from_glyphs(page, glyphs).unwrap()
    }

    fn line(page: u32, text: &str, baseline: f32) -> Line {
        make_line(page, text, baseline, false)
    }

    fn page_lines(number: u32, bottom: f32, lines: Vec<Line>) -> PageLines {
        PageLines { number, width: 595.0, height: 842.0, bottom, lines }
    }

    #[test]
    fn recomposes_latex_accents() {
        assert_eq!(recompose_accents("introducci´on"), "introducción");
        assert_eq!(recompose_accents("m´aximo"), "máximo");
        assert_eq!(recompose_accents("pi˜na ping¨uino"), "piña pingüino");
        assert_eq!(recompose_accents("caf`e"), "cafè");
        assert_eq!(recompose_accents("fl\u{02C6}ute"), "flûte");
        assert_eq!(recompose_accents("cr´ıtico"), "crítico");
    }

    #[test]
    fn leaves_plain_text_alone() {
        assert_eq!(recompose_accents("plain text, 3 items"), "plain text, 3 items");
        // an accent not followed by a letter is kept as it is
        assert_eq!(recompose_accents("end´"), "end´");
        assert_eq!(recompose_accents("a´ 1"), "a´ 1");
        assert_eq!(recompose_accents(""), "");
    }

    #[test]
    fn an_accent_used_as_an_apostrophe_is_not_glued_to_a_consonant() {
        // no precomposed "t with acute" exists in Latin-1/NFC: keep the text readable
        assert_eq!(recompose_accents("don´t"), "don´t");
        assert_eq!(recompose_accents("l´homme"), "l´homme");
    }

    #[test]
    fn already_composed_text_is_stable() {
        assert_eq!(recompose_accents("introducción"), "introducción");
        // decomposed input is normalized to the composed form
        assert_eq!(recompose_accents("o\u{0301}"), "ó");
    }

    #[test]
    fn monospaced_lines_are_not_touched() {
        let code = make_line(1, "`foo` and ´x", 100.0, true);
        assert_eq!(fix_loose_accents(&code), "`foo` and ´x");
        let prose = make_line(1, "m´aximo", 100.0, false);
        assert_eq!(fix_loose_accents(&prose), "máximo");
    }

    #[test]
    fn dehyphenates_a_word_split_across_lines() {
        assert_eq!(join_lines(&["a pre-", "training step"]), "a pretraining step");
        assert_eq!(join_lines(&["a pre\u{2010}", "training"]), "a pretraining");
    }

    #[test]
    fn soft_hyphens_join_the_word_and_never_show() {
        assert_eq!(join_lines(&["trans\u{00AD}", "formación"]), "transformación");
        // even before an uppercase letter: a soft hyphen is only a break opportunity
        assert_eq!(join_lines(&["Wolfgang\u{00AD}", "Amadeus"]), "WolfgangAmadeus");
        assert_eq!(join_lines(&["mid\u{00AD}word text"]), "midword text");
    }

    #[test]
    fn keeps_hyphens_that_do_not_break_a_word() {
        // next line starts with a capital or a digit: real hyphen
        assert_eq!(join_lines(&["Smith-", "Jones paper"]), "Smith- Jones paper");
        assert_eq!(join_lines(&["pages 1-", "3 of the text"]), "pages 1- 3 of the text");
        // a dash standing alone is not a word break
        assert_eq!(join_lines(&["wait -", "then go"]), "wait - then go");
    }

    #[test]
    fn joins_lines_with_spaces() {
        assert_eq!(join_lines(&["one", "two", "three"]), "one two three");
        assert_eq!(join_lines::<&str>(&[]), "");
        assert_eq!(join_lines(&["  ", "x"]), "x");
    }

    #[test]
    fn strips_dot_leaders() {
        assert_eq!(strip_dot_leaders("Introducción . . . . . . 63"), ("Introducción 63".to_string(), true));
        assert_eq!(strip_dot_leaders("Chapter 2 ............ 10"), ("Chapter 2 10".to_string(), true));
    }

    #[test]
    fn strips_middle_dot_and_ellipsis_leaders() {
        assert_eq!(strip_dot_leaders("Index \u{00B7}\u{00B7}\u{00B7}\u{00B7}\u{00B7} 7"), ("Index 7".to_string(), true));
        assert_eq!(strip_dot_leaders("Index \u{2026}\u{2026} 7"), ("Index 7".to_string(), true));
    }

    #[test]
    fn a_line_of_only_leaders_becomes_empty() {
        assert_eq!(strip_dot_leaders("........"), (String::new(), true));
    }

    #[test]
    fn keeps_ellipses_and_decimals() {
        assert_eq!(strip_dot_leaders("and then... nothing"), ("and then... nothing".to_string(), false));
        assert_eq!(strip_dot_leaders("and then\u{2026} nothing"), ("and then\u{2026} nothing".to_string(), false));
        assert_eq!(strip_dot_leaders("version 1.2.3"), ("version 1.2.3".to_string(), false));
        assert_eq!(strip_dot_leaders("wait . . . what"), ("wait . . . what".to_string(), false));
    }

    #[test]
    fn finds_repeated_headers_and_page_numbers() {
        let pages: Vec<PageLines> = (1..=5u32)
            .map(|n| {
                page_lines(
                    n,
                    0.0,
                    vec![
                        line(n, "Book Title", 800.0),              // header, same on all pages
                        line(n, &format!("Body text {n}"), 400.0), // content, differs
                        line(n, &format!("Page {n} | 13"), 30.0),  // footer, digits change
                    ],
                )
            })
            .collect();

        for flags in repeated_margin_lines(&pages, MarginBands::TopAndBottom) {
            assert_eq!(flags, vec![true, false, true]);
        }
    }

    #[test]
    fn bare_page_numbers_are_footers() {
        let pages: Vec<PageLines> =
            (1..=4u32).map(|n| page_lines(n, 0.0, vec![line(n, &n.to_string(), 30.0)])).collect();
        assert!(repeated_margin_lines(&pages, MarginBands::TopAndBottom).iter().all(|f| f == &vec![true]));
    }

    #[test]
    fn numeric_table_rows_in_the_band_are_not_footers() {
        // same shape and height on every page, but data, not a footer
        let rows = ["0.12 0.55", "3.40 1.20", "7.01 9.99", "2.20 4.48"];
        let pages: Vec<PageLines> =
            (1..=4u32).map(|n| page_lines(n, 0.0, vec![line(n, rows[n as usize - 1], 30.0)])).collect();
        assert!(repeated_margin_lines(&pages, MarginBands::TopAndBottom).iter().all(|f| f == &vec![false]));
    }

    #[test]
    fn content_in_the_middle_is_never_removed_even_if_it_repeats() {
        let pages: Vec<PageLines> =
            (1..=5u32).map(|n| page_lines(n, 0.0, vec![line(n, "same text", 400.0)])).collect();
        assert!(repeated_margin_lines(&pages, MarginBands::TopAndBottom).iter().all(|f| f == &vec![false]));
    }

    #[test]
    fn needs_several_pages_to_call_something_repeated() {
        let pages: Vec<PageLines> =
            (1..=2u32).map(|n| page_lines(n, 0.0, vec![line(n, "Header", 800.0)])).collect();
        assert!(repeated_margin_lines(&pages, MarginBands::TopAndBottom).iter().all(|f| f == &vec![false]));
    }

    #[test]
    fn same_text_at_a_different_height_is_not_the_same_header() {
        let heights = [800.0, 760.0, 800.0, 760.0, 800.0];
        let pages: Vec<PageLines> = heights
            .iter()
            .enumerate()
            .map(|(i, y)| page_lines(i as u32 + 1, 0.0, vec![line(i as u32 + 1, "Header", *y)]))
            .collect();
        // 800 appears on 3 of 5 pages: removed; 760 only on 2: kept
        let flags = repeated_margin_lines(&pages, MarginBands::TopAndBottom);
        assert_eq!(flags.iter().map(|f| f[0]).collect::<Vec<_>>(), [true, false, true, false, true]);
    }

    #[test]
    fn a_header_that_drifts_a_fraction_of_a_point_is_still_one_header() {
        // 801.9 / 802.1 / ... straddle any fixed bucket boundary
        let ys = [801.9, 802.1, 801.95, 802.05];
        let pages: Vec<PageLines> = ys
            .iter()
            .enumerate()
            .map(|(i, y)| page_lines(i as u32 + 1, 0.0, vec![line(i as u32 + 1, "Header", *y)]))
            .collect();
        assert!(repeated_margin_lines(&pages, MarginBands::TopAndBottom).iter().all(|f| f == &vec![true]));
    }

    fn sized_line(page: u32, text: &str, baseline: f32, size: f32) -> Line {
        let glyphs = text
            .chars()
            .enumerate()
            .map(|(i, ch)| Glyph {
                ch,
                left: i as f32 * size * 0.5,
                right: (i as f32 + 1.0) * size * 0.5,
                bottom: baseline,
                top: baseline + size,
                size,
                font: Arc::from("F"),
                mono: false,
            })
            .collect();
        Line::from_glyphs(page, glyphs).unwrap()
    }

    /// Slides: a 20 pt title at the top over 18 pt bullets, with the same title on 4 slides.
    fn slides_with_repeated_title() -> Vec<PageLines> {
        (1..=4u32)
            .map(|n| {
                PageLines {
                    number: n,
                    width: 720.0,
                    height: 405.0,
                    bottom: 0.0,
                    lines: vec![
                        sized_line(n, "Ejemplo", 380.0, 20.0),
                        sized_line(n, &format!("Punto numero {n} de la lista de ejemplo"), 300.0, 18.0),
                        sized_line(n, &format!("Otro punto del ejemplo {n} con mas texto"), 270.0, 18.0),
                        sized_line(n, &format!("Curso {n}"), 25.0, 9.0),
                    ],
                }
            })
            .collect()
    }

    #[test]
    fn a_title_bigger_than_the_body_is_never_a_header() {
        let flags = repeated_margin_lines(&slides_with_repeated_title(), MarginBands::TopAndBottom);
        for page_flags in flags {
            assert_eq!(page_flags, vec![false, false, false, true], "only the footer goes");
        }
    }

    #[test]
    fn bottom_only_keeps_top_text_even_at_body_size() {
        // same-size top text: with both bands it counts as a running header...
        let pages: Vec<PageLines> = (1..=4u32)
            .map(|n| {
                PageLines {
                    number: n,
                    width: 720.0,
                    height: 405.0,
                    bottom: 0.0,
                    lines: vec![
                        sized_line(n, "Resumen", 380.0, 18.0),
                        sized_line(n, &format!("Texto del cuerpo de la pagina {n}"), 300.0, 18.0),
                    ],
                }
            })
            .collect();
        let both = repeated_margin_lines(&pages, MarginBands::TopAndBottom);
        assert!(both.iter().all(|f| f == &vec![true, false]));
        // ...but the slide mode asks for footers only, so titles survive
        let bottom = repeated_margin_lines(&pages, MarginBands::BottomOnly);
        assert!(bottom.iter().all(|f| f == &vec![false, false]));
    }

    #[test]
    fn bottom_only_still_finds_the_footer() {
        let flags = repeated_margin_lines(&slides_with_repeated_title(), MarginBands::BottomOnly);
        assert!(flags.iter().all(|f| f == &vec![false, false, false, true]));
    }

    #[test]
    fn body_size_is_the_most_common_size_by_glyphs() {
        let pages = slides_with_repeated_title();
        assert_eq!(body_size(&pages), 18.0);
        assert_eq!(body_size(&[]), 0.0);
    }

    #[test]
    fn bands_follow_the_page_box_not_the_origin() {
        // Page box starts at y=100: header at 900 is 800pt above the bottom edge (top band),
        // footer at 130 is 30pt above it (bottom band). Body at 500 is in the middle.
        let pages: Vec<PageLines> = (1..=4u32)
            .map(|n| {
                page_lines(
                    n,
                    100.0,
                    vec![line(n, "Header", 900.0), line(n, "Body", 500.0), line(n, "Footer", 130.0)],
                )
            })
            .collect();
        for flags in repeated_margin_lines(&pages, MarginBands::TopAndBottom) {
            assert_eq!(flags, vec![true, false, true]);
        }
    }
}
