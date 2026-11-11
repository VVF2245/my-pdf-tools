//! Stage 1: every character of a page with its position, font and size.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use mpt_core::ConversionError;
use pdfium_render::prelude::PdfPageIndex;

use super::pdfium::{PdfSource, PdfiumLocation, with_document};

/// A character with the data the later stages need. Coordinates are PDF points,
/// origin at the bottom-left of the page.
#[derive(Debug, Clone, PartialEq)]
pub struct Glyph {
    pub ch: char,
    pub left: f32,
    pub right: f32,
    pub bottom: f32,
    pub top: f32,
    /// Effective size in points (font size scaled by the text matrix).
    pub size: f32,
    /// Shared between all glyphs of the same font (a page can have thousands).
    pub font: Arc<str>,
    pub mono: bool,
}

/// All usable glyphs of one page, in content-stream order.
#[derive(Debug, Clone, PartialEq)]
pub struct PageGlyphs {
    /// 1-based page number.
    pub number: u32,
    pub width: f32,
    pub height: f32,
    pub glyphs: Vec<Glyph>,
}

/// Text turned more than this many degrees away from the page's main direction
/// (axis labels, watermarks) is dropped.
const MAX_ANGLE_DEGREES: f32 = 5.0;

/// Angular distance in degrees between two angles, in `0..=180`.
fn angular_distance(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

/// Keeps the glyphs that run in the page's dominant direction.
///
/// The direction is measured, not assumed to be 0 degrees: PDFium reports angles in the
/// page's own space, so a page stored with `/Rotate 90` has all its text at 90 degrees.
/// Limitation: later stages assume horizontal lines, so such pages still read poorly.
fn keep_dominant_direction(raw: Vec<(Glyph, f32)>) -> Vec<Glyph> {
    // Histogram in 5-degree buckets; ties go to the smallest angle (deterministic).
    let mut buckets: BTreeMap<i32, usize> = BTreeMap::new();
    for (_, angle) in &raw {
        let bucket = (angle.rem_euclid(360.0) / MAX_ANGLE_DEGREES).round() as i32;
        *buckets.entry(bucket).or_default() += 1;
    }
    let mut dominant = 0.0;
    let mut best = 0;
    for (bucket, count) in buckets {
        if count > best {
            best = count;
            dominant = bucket as f32 * MAX_ANGLE_DEGREES;
        }
    }
    raw.into_iter()
        .filter(|(_, angle)| angular_distance(*angle, dominant) <= MAX_ANGLE_DEGREES)
        .map(|(glyph, _)| glyph)
        .collect()
}

/// Extracts the glyphs of pages `first..=last` (1-based). `last == 0` means "to the end".
///
/// A range that does not fit the document is an error, not an empty result.
pub fn extract_pages(
    location: &PdfiumLocation,
    source: PdfSource<'_>,
    first: u32,
    last: u32,
) -> Result<Vec<PageGlyphs>, ConversionError> {
    with_document(location, source, |document| {
        let total = document.pages().len() as u32;
        if total == 0 {
            return Err(ConversionError::Corrupt);
        }
        let first = first.max(1);
        let end = if last == 0 { total } else { last };
        if first > end || first > total {
            return Err(ConversionError::InvalidPageRange { first, last, total });
        }
        let end = end.min(total);

        let mut fonts: HashMap<String, Arc<str>> = HashMap::new();
        let mut pages = Vec::new();

        // Only the requested pages are loaded.
        for number in first..=end {
            let page = document
                .pages()
                .get((number - 1) as PdfPageIndex)
                .map_err(|e| ConversionError::Internal { message: format!("page {number}: {e:?}") })?;
            let text = page
                .text()
                .map_err(|e| ConversionError::Internal { message: format!("page {number}: {e:?}") })?;

            let mut raw: Vec<(Glyph, f32)> = Vec::new();
            for c in text.chars().iter() {
                // Synthetic characters (line ends PDFium adds) are not part of the document.
                if c.is_generated().unwrap_or(false) {
                    continue;
                }
                let Some(ch) = c.unicode_char() else { continue };
                if ch.is_control() {
                    continue;
                }
                let size = c.scaled_font_size().value;
                // Size 0 is how some rotated text shows up with the scaled size.
                if size <= 0.0 {
                    continue;
                }
                let Ok(bounds) = c.loose_bounds() else { continue };
                let name = c.font_name();
                let font = match fonts.get(&name) {
                    Some(shared) => shared.clone(),
                    None => {
                        let shared: Arc<str> = Arc::from(name.as_str());
                        fonts.insert(name, shared.clone());
                        shared
                    }
                };
                let angle = c.angle_degrees().unwrap_or(0.0);
                raw.push((
                    Glyph {
                        ch,
                        left: bounds.left().value,
                        right: bounds.right().value,
                        bottom: bounds.bottom().value,
                        top: bounds.top().value,
                        size,
                        font,
                        mono: c.font_is_fixed_pitch(),
                    },
                    angle,
                ));
            }
            pages.push(PageGlyphs {
                number,
                width: page.width().value,
                height: page.height().value,
                glyphs: keep_dominant_direction(raw),
            });
        }
        Ok(pages)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf_reader::test_support;

    fn text_of(page: &PageGlyphs) -> String {
        page.glyphs.iter().map(|g| g.ch).collect()
    }

    fn glyph(ch: char) -> Glyph {
        Glyph {
            ch,
            left: 0.0,
            right: 1.0,
            bottom: 0.0,
            top: 1.0,
            size: 10.0,
            font: Arc::from("F"),
            mono: false,
        }
    }

    #[test]
    fn extracts_text_sizes_and_page_dimensions() {
        let Some(location) = test_support::pdfium() else { return };
        let pdf = test_support::pdf_from_markdown("# Título\n\nHola mundo con texto de cuerpo.\n");

        let pages = extract_pages(&location, PdfSource::Bytes(&pdf), 1, 0).unwrap();

        assert_eq!(pages.len(), 1);
        let page = &pages[0];
        assert_eq!(page.number, 1);
        assert!((page.width - 595.0).abs() < 2.0 && (page.height - 842.0).abs() < 2.0, "A4 expected");
        let text = text_of(page);
        assert!(text.contains("Título"), "{text}");
        assert!(text.contains("cuerpo"), "{text}");

        let heading = page.glyphs.iter().find(|g| g.ch == 'T').unwrap();
        let body = page.glyphs.iter().find(|g| g.ch == 'H').unwrap();
        assert!(heading.size > body.size, "heading {} vs body {}", heading.size, body.size);
        assert!(!heading.font.is_empty());
    }

    #[test]
    fn glyphs_of_the_same_font_share_one_allocation() {
        let Some(location) = test_support::pdfium() else { return };
        let pdf = test_support::pdf_from_markdown("abc def ghi\n");
        let pages = extract_pages(&location, PdfSource::Bytes(&pdf), 1, 0).unwrap();
        let glyphs = &pages[0].glyphs;
        assert!(Arc::ptr_eq(&glyphs[0].font, &glyphs[1].font));
    }

    #[test]
    fn monospaced_fonts_are_flagged() {
        let Some(location) = test_support::pdfium() else { return };
        let pdf = test_support::pdf_from_markdown("```\ncodigo\n```\n\nprosa\n");
        let pages = extract_pages(&location, PdfSource::Bytes(&pdf), 1, 0).unwrap();
        let glyphs = &pages[0].glyphs;
        assert!(glyphs.iter().find(|g| g.ch == 'c').unwrap().mono);
        assert!(!glyphs.iter().find(|g| g.ch == 'p').unwrap().mono);
    }

    fn long_pdf() -> Vec<u8> {
        let body = (1..=150).map(|i| format!("linea {i}\n\n")).collect::<String>();
        test_support::pdf_from_markdown(&body)
    }

    #[test]
    fn page_range_is_respected() {
        let Some(location) = test_support::pdfium() else { return };
        let pdf = long_pdf();
        let all = extract_pages(&location, PdfSource::Bytes(&pdf), 1, 0).unwrap();
        assert!(all.len() >= 3, "expected several pages, got {}", all.len());
        let second = extract_pages(&location, PdfSource::Bytes(&pdf), 2, 2).unwrap();
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].number, 2);
        // A last page beyond the end is clamped, not an error.
        let tail = extract_pages(&location, PdfSource::Bytes(&pdf), 2, 999).unwrap();
        assert_eq!(tail.len(), all.len() - 1);
    }

    #[test]
    fn invalid_page_ranges_are_errors() {
        let Some(location) = test_support::pdfium() else { return };
        let pdf = test_support::pdf_from_markdown("hola\n");
        assert_eq!(
            extract_pages(&location, PdfSource::Bytes(&pdf), 10, 3),
            Err(ConversionError::InvalidPageRange { first: 10, last: 3, total: 1 })
        );
        assert_eq!(
            extract_pages(&location, PdfSource::Bytes(&pdf), 5, 0),
            Err(ConversionError::InvalidPageRange { first: 5, last: 0, total: 1 })
        );
    }

    #[test]
    fn off_axis_text_is_dropped_on_an_upright_page() {
        let raw = vec![
            (glyph('a'), 0.0),
            (glyph('b'), 1.0),
            (glyph('c'), 359.0),
            (glyph('y'), 90.0), // rotated axis label
        ];
        let kept: String = keep_dominant_direction(raw).iter().map(|g| g.ch).collect();
        assert_eq!(kept, "abc");
    }

    #[test]
    fn a_rotated_page_keeps_its_own_text() {
        // Page stored with /Rotate 90: all text reads at 90 degrees. It must not vanish.
        let raw = vec![(glyph('a'), 90.0), (glyph('b'), 90.0), (glyph('x'), 0.0)];
        let kept: String = keep_dominant_direction(raw).iter().map(|g| g.ch).collect();
        assert_eq!(kept, "ab");
    }

    #[test]
    fn no_glyphs_is_fine() {
        assert!(keep_dominant_direction(Vec::new()).is_empty());
    }

    #[test]
    fn angular_distance_wraps_around() {
        assert_eq!(angular_distance(359.0, 1.0), 2.0);
        assert_eq!(angular_distance(0.0, 180.0), 180.0);
    }
}
