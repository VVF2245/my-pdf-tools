//! Stage 2: glyphs -> lines of text.

use std::collections::BTreeMap;
use std::sync::Arc;

use super::glyphs::{Glyph, PageGlyphs};

/// A gap between two glyphs wider than this fraction of the font size is a space.
/// Some PDFs (LaTeX output) contain no space characters at all, only gaps; kerning and
/// ligatures leave gaps below ~0.15 em (with 0.2 some real spaces were missed).
pub const SPACE_GAP_RATIO: f32 = 0.15;

/// A line starts when the baseline moves more than this fraction of the font size.
const BASELINE_TOLERANCE: f32 = 0.4;

/// A run of glyphs on the same baseline.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    /// 1-based page number.
    pub page: u32,
    pub glyphs: Vec<Glyph>,
    /// Text with spaces inferred from the gaps.
    pub text: String,
    pub left: f32,
    pub right: f32,
    /// Median bottom of the visible glyphs: the vertical position of the line. (Not the
    /// first glyph's: a line may start with a raised footnote mark or a drop cap.)
    pub baseline: f32,
    pub top: f32,
    /// Most frequent size / font / monospace flag among the visible glyphs.
    pub size: f32,
    pub font: Arc<str>,
    pub mono: bool,
}

/// The lines of one page, with the page geometry later stages need.
#[derive(Debug, Clone, PartialEq)]
pub struct PageLines {
    /// 1-based page number.
    pub number: u32,
    pub width: f32,
    pub height: f32,
    /// y of the bottom edge of the page; line positions are measured from it.
    pub bottom: f32,
    pub lines: Vec<Line>,
}

impl Line {
    /// Builds a line; `None` when it holds nothing visible.
    pub fn from_glyphs(page: u32, glyphs: Vec<Glyph>) -> Option<Line> {
        let visible: Vec<&Glyph> = glyphs.iter().filter(|g| !g.ch.is_whitespace()).collect();
        if visible.is_empty() {
            return None;
        }

        let mut sizes: BTreeMap<i32, usize> = BTreeMap::new();
        let mut fonts: BTreeMap<&str, usize> = BTreeMap::new();
        let (mut mono, mut other) = (0usize, 0usize);
        for g in &visible {
            *sizes.entry((g.size * 10.0).round() as i32).or_default() += 1;
            *fonts.entry(&g.font).or_default() += 1;
            if g.mono {
                mono += 1;
            } else {
                other += 1;
            }
        }
        let size = most_frequent(sizes.iter().map(|(k, n)| (*k, *n))).unwrap_or(0) as f32 / 10.0;
        let font_name = most_frequent(fonts.iter().map(|(k, n)| (*k, *n))).unwrap_or("");
        let font = visible.iter().find(|g| &*g.font == font_name).map(|g| g.font.clone())?;

        let mut bottoms: Vec<f32> = visible.iter().map(|g| g.bottom).collect();
        bottoms.sort_by(f32::total_cmp);
        let baseline = bottoms[bottoms.len() / 2];

        let line = Line {
            page,
            text: text_of(&glyphs),
            left: visible.iter().map(|g| g.left).fold(f32::MAX, f32::min),
            right: visible.iter().map(|g| g.right).fold(f32::MIN, f32::max),
            baseline,
            top: visible.iter().map(|g| g.top).fold(f32::MIN, f32::max),
            size,
            font,
            mono: mono > other,
            glyphs,
        };
        (!line.text.is_empty()).then_some(line)
    }
}

/// The key with the highest count; ties go to the first one (inputs are ordered).
fn most_frequent<K>(counts: impl Iterator<Item = (K, usize)>) -> Option<K> {
    let mut best: Option<(K, usize)> = None;
    for (key, n) in counts {
        if best.as_ref().is_none_or(|(_, b)| n > *b) {
            best = Some((key, n));
        }
    }
    best.map(|(k, _)| k)
}

/// Joins glyphs into text, inserting a space where the gap between two glyphs is wide.
fn text_of(glyphs: &[Glyph]) -> String {
    let mut out = String::new();
    let mut prev: Option<&Glyph> = None;
    for g in glyphs {
        if let Some(p) = prev {
            let gap = g.left - p.right;
            if gap > SPACE_GAP_RATIO * g.size && !p.ch.is_whitespace() && !g.ch.is_whitespace() {
                out.push(' ');
            }
        }
        out.push(g.ch);
        prev = Some(g);
    }
    out.trim().to_string()
}

/// Groups the glyphs of a page (in content-stream order) into lines. Takes the page by
/// value: the glyphs are moved into their lines, not copied.
///
/// A new line starts when the baseline moves, or when x jumps backwards by more than one
/// font size (the next column of a two-column page, in LaTeX output).
pub fn group_lines(page: PageGlyphs) -> PageLines {
    let mut groups: Vec<Vec<Glyph>> = Vec::new();
    for g in page.glyphs {
        let starts_new = match groups.last() {
            None => true,
            Some(current) => {
                let base = current
                    .iter()
                    .find(|c| !c.ch.is_whitespace())
                    .map_or(current[0].bottom, |c| c.bottom);
                let prev = &current[current.len() - 1];
                (g.bottom - base).abs() > BASELINE_TOLERANCE * g.size.max(prev.size)
                    || g.left < prev.right - g.size
            }
        };
        match groups.last_mut() {
            Some(current) if !starts_new => current.push(g),
            _ => groups.push(vec![g]),
        }
    }
    PageLines {
        number: page.number,
        width: page.width,
        height: page.height,
        bottom: page.bottom,
        lines: groups
            .into_iter()
            .filter_map(|glyphs| Line::from_glyphs(page.number, glyphs))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf_reader::pdfium::PdfSource;
    use crate::pdf_reader::{extract_pages, test_support};

    /// Glyphs of `text` laid out from `x`, each as wide as half the size.
    fn run(text: &str, mut x: f32, y: f32, size: f32) -> Vec<Glyph> {
        text.chars()
            .map(|ch| {
                let g = Glyph {
                    ch,
                    left: x,
                    right: x + 0.5 * size,
                    bottom: y,
                    top: y + size,
                    size,
                    font: Arc::from("Test"),
                    mono: false,
                };
                x += 0.5 * size;
                g
            })
            .collect()
    }

    fn page(glyphs: Vec<Glyph>) -> PageGlyphs {
        PageGlyphs { number: 1, width: 595.0, height: 842.0, bottom: 0.0, glyphs }
    }

    fn lines_of(glyphs: Vec<Glyph>) -> Vec<Line> {
        group_lines(page(glyphs)).lines
    }

    #[test]
    fn infers_spaces_from_gaps() {
        // "Hello" is 5 glyphs wide (25pt at size 10); "world" starts 3pt later: gap 0.3 em.
        let mut glyphs = run("Hello", 0.0, 100.0, 10.0);
        glyphs.extend(run("world", 28.0, 100.0, 10.0));
        let lines = lines_of(glyphs);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "Hello world");
    }

    #[test]
    fn small_gaps_are_kerning_not_spaces() {
        // 1pt gap at size 10 = 0.1 em, below the threshold.
        let mut glyphs = run("of", 0.0, 100.0, 10.0);
        glyphs.extend(run("the", 11.0, 100.0, 10.0));
        assert_eq!(lines_of(glyphs)[0].text, "ofthe");
    }

    #[test]
    fn gap_just_above_threshold_is_a_space() {
        // 1.6pt = 0.16 em.
        let mut glyphs = run("of", 0.0, 100.0, 10.0);
        glyphs.extend(run("the", 11.6, 100.0, 10.0));
        assert_eq!(lines_of(glyphs)[0].text, "of the");
    }

    #[test]
    fn baseline_change_starts_a_new_line() {
        let mut glyphs = run("first", 0.0, 100.0, 10.0);
        glyphs.extend(run("second", 0.0, 86.0, 10.0));
        let lines = lines_of(glyphs);
        assert_eq!(lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), ["first", "second"]);
        assert_eq!(lines[0].baseline, 100.0);
    }

    #[test]
    fn a_superscript_stays_on_its_line() {
        let mut glyphs = run("x", 0.0, 100.0, 10.0);
        glyphs.extend(run("2", 5.0, 103.0, 7.0)); // raised 3pt, smaller
        glyphs.extend(run("+y", 8.5, 100.0, 10.0));
        assert_eq!(lines_of(glyphs).len(), 1);
    }

    #[test]
    fn baseline_is_the_median_not_the_first_glyph() {
        // The line starts with a raised footnote mark; the text sits on y = 100.
        let mut glyphs = run("1", 0.0, 104.0, 6.0);
        glyphs.extend(run("Footnote text", 3.0, 100.0, 10.0));
        let lines = lines_of(glyphs);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].baseline, 100.0);
    }

    #[test]
    fn x_jumping_back_starts_a_new_line_like_the_next_column() {
        let mut glyphs = run("left column", 72.0, 100.0, 10.0);
        glyphs.extend(run("right column", 307.0, 100.0, 10.0));
        // second run starts far right: same line; a run starting back at x=72 is a new line
        glyphs.extend(run("next", 72.0, 100.0, 10.0));
        let lines = lines_of(glyphs);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[1].text, "next");
    }

    #[test]
    fn line_properties_use_the_dominant_values() {
        let mut glyphs = run("Title", 0.0, 100.0, 20.0);
        glyphs.extend(run("a", 60.0, 100.0, 8.0)); // one small glyph
        let lines = lines_of(glyphs);
        let line = &lines[0];
        assert_eq!(line.size, 20.0);
        assert_eq!(line.left, 0.0);
        assert!(line.right > 60.0);
        assert_eq!(line.top, 120.0);
    }

    #[test]
    fn whitespace_only_runs_make_no_line() {
        assert!(lines_of(run("   ", 0.0, 100.0, 10.0)).is_empty());
    }

    #[test]
    fn page_geometry_is_carried_over() {
        let mut p = page(run("a", 0.0, 100.0, 10.0));
        p.bottom = 36.0;
        let result = group_lines(p);
        assert_eq!((result.number, result.width, result.height, result.bottom), (1, 595.0, 842.0, 36.0));
    }

    #[test]
    fn real_pdf_gives_clean_lines_with_spaces() {
        let Some(location) = test_support::pdfium() else { return };
        let pdf = test_support::pdf_from_markdown("# Título\n\nHola mundo con texto de cuerpo.\n");
        let mut pages = extract_pages(&location, PdfSource::Bytes(&pdf), 1, 0).unwrap();

        let result = group_lines(pages.remove(0));
        let lines = result.lines;

        let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        // the last line is the page number our writer puts in the footer
        assert_eq!(texts, ["Título", "Hola mundo con texto de cuerpo.", "1"]);
        assert!(lines[0].size > lines[1].size);
        assert!(lines[0].baseline > lines[1].baseline, "the heading is above the body");
        assert_eq!(result.bottom, 0.0);
    }
}
