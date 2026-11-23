//! Images: the bitmaps embedded in the pages, and whole pages rendered to PNG.
//!
//! This stage is separate from the text stages: it opens the document on its own, so the
//! glyph and line types do not carry any image data. The converted document gets the images
//! by page ([`add_media`]), after the text stages have built it.
//!
//! Limits: only images set directly on the page are found (not those nested in a form
//! XObject, which is how some LaTeX figures are stored), and an image is placed after the
//! text of its page, not at its exact position.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::io::Cursor;
use std::sync::Arc;

use image::{DynamicImage, ImageFormat};
use outmark_core::{Asset, AssetId, Block, BlockKind, ConversionError, Document};
use pdfium_render::prelude::*;

use super::pdfium::{PdfSource, PdfiumLocation, with_document};

/// The PNG files the image fixtures refer to (`assets/photo.png`, `assets/logo.png`), made on
/// the fly so no binary besides the PDFs is committed.
#[doc(hidden)]
pub fn fixture_assets() -> HashMap<String, Vec<u8>> {
    fn png(width: u32, height: u32, pixel: impl Fn(u32, u32) -> [u8; 3]) -> Vec<u8> {
        let image = image::RgbImage::from_fn(width, height, |x, y| image::Rgb(pixel(x, y)));
        let mut out = Vec::new();
        DynamicImage::ImageRgb8(image).write_to(&mut Cursor::new(&mut out), ImageFormat::Png).expect("png encodes");
        out
    }
    let mut files = HashMap::new();
    files.insert("assets/photo.png".to_string(), png(400, 300, |x, y| [(x % 256) as u8, (y % 256) as u8, 128]));
    files.insert("assets/logo.png".to_string(), png(160, 80, |x, _| if x < 80 { [200, 30, 30] } else { [30, 30, 200] }));
    files
}

/// An embedded image of a page. Coordinates are PDF points, origin at the bottom-left.
#[derive(Debug, Clone)]
pub struct PageImage {
    pub left: f32,
    pub right: f32,
    pub bottom: f32,
    pub top: f32,
    pub width_px: u32,
    pub height_px: u32,
    pub png: Arc<Vec<u8>>,
}

/// What a page holds besides text.
#[derive(Debug, Clone, Default)]
pub struct PageMedia {
    /// 1-based page number.
    pub number: u32,
    pub width: f32,
    pub height: f32,
    pub images: Vec<PageImage>,
    /// Drawn shapes (lines, boxes, curves) that are not a page background: a diagram has many.
    pub graphics: usize,
}

/// The pages' media and how many images could not be decoded.
#[derive(Debug, Clone, Default)]
pub struct ExtractedMedia {
    pub pages: Vec<PageMedia>,
    pub failed: usize,
}

/// An image smaller than this (pixels, on its shorter side) is an icon or a bullet.
const MIN_SIDE_PX: u32 = 32;
/// An image covering less than this fraction of the page is decoration.
const MIN_PAGE_FRACTION: f32 = 0.01;
/// An image that appears on at least this many pages, and on this fraction of them, is a
/// logo or a watermark.
const REPEAT_MIN_PAGES: usize = 3;
const REPEAT_MIN_FRACTION: f32 = 0.3;
/// A shape this close to the size of the page is its background, not a graphic.
const BACKGROUND_FRACTION: f32 = 0.9;
/// A page with at least this many drawn shapes holds a diagram (slides mode renders it).
pub const GRAPHIC_PAGE_SHAPES: usize = 6;
/// Width of a rendered page, in pixels.
pub const RENDER_WIDTH_PX: u32 = 1200;

fn to_png(image: &DynamicImage) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let flat = if image.color().has_alpha() {
        DynamicImage::ImageRgba8(image.to_rgba8())
    } else {
        DynamicImage::ImageRgb8(image.to_rgb8())
    };
    flat.write_to(&mut Cursor::new(&mut out), ImageFormat::Png).ok()?;
    Some(out)
}

/// The page range, as `extract_pages_with` reads it (`last == 0`: to the end).
fn page_range(total: u32, first: u32, last: u32) -> Result<(u32, u32), ConversionError> {
    let first = first.max(1);
    let end = if last == 0 { total } else { last };
    if total == 0 {
        return Err(ConversionError::Corrupt);
    }
    if first > end || first > total {
        return Err(ConversionError::InvalidPageRange { first, last, total });
    }
    Ok((first, end.min(total)))
}

/// Extracts the embedded images and counts the shapes of pages `first..=last`.
pub fn extract_media(
    location: &PdfiumLocation,
    source: PdfSource<'_>,
    first: u32,
    last: u32,
    keep_going: &dyn Fn(u32, u32) -> bool,
) -> Result<ExtractedMedia, ConversionError> {
    with_document(location, source, |document| {
        let (first, end) = page_range(document.pages().len() as u32, first, last)?;
        let mut media = ExtractedMedia::default();
        for number in first..=end {
            if !keep_going(number - first, end - first + 1) {
                return Err(ConversionError::Cancelled);
            }
            let page = document
                .pages()
                .get((number - 1) as PdfPageIndex)
                .map_err(|e| ConversionError::Internal { message: format!("page {number}: {e:?}") })?;
            let (width, height) = (page.width().value, page.height().value);
            let mut entry = PageMedia { number, width, height, ..Default::default() };
            for object in page.objects().iter() {
                match object.object_type() {
                    PdfPageObjectType::Path => {
                        let background = object.bounds().is_ok_and(|b| {
                            b.width().value >= BACKGROUND_FRACTION * width
                                && b.height().value >= BACKGROUND_FRACTION * height
                        });
                        if !background {
                            entry.graphics += 1;
                        }
                    }
                    PdfPageObjectType::Image => {
                        let Some(image_object) = object.as_image_object() else { continue };
                        let decoded = image_object
                            .get_processed_image(document)
                            .ok()
                            .and_then(|img| to_png(&img).map(|png| (img.width(), img.height(), png)));
                        let (Some((width_px, height_px, png)), Ok(bounds)) = (decoded, object.bounds()) else {
                            media.failed += 1;
                            continue;
                        };
                        entry.images.push(PageImage {
                            left: bounds.left().value,
                            right: bounds.right().value,
                            bottom: bounds.bottom().value,
                            top: bounds.top().value,
                            width_px,
                            height_px,
                            png: Arc::new(png),
                        });
                    }
                    _ => {}
                }
            }
            media.pages.push(entry);
        }
        Ok(media)
    })
}

/// Renders the given pages (1-based numbers) to PNG, `width_px` wide.
pub fn render_pages(
    location: &PdfiumLocation,
    source: PdfSource<'_>,
    numbers: &[u32],
    width_px: u32,
) -> Result<Vec<(u32, Arc<Vec<u8>>)>, ConversionError> {
    with_document(location, source, |document| {
        let mut out = Vec::new();
        for &number in numbers {
            let page = document
                .pages()
                .get((number - 1) as PdfPageIndex)
                .map_err(|e| ConversionError::Internal { message: format!("page {number}: {e:?}") })?;
            let config = PdfRenderConfig::new().set_target_width(width_px as i32);
            let bitmap = page
                .render_with_config(&config)
                .map_err(|e| ConversionError::Internal { message: format!("render page {number}: {e:?}") })?;
            if let Some(png) = bitmap.as_image().ok().and_then(|img| to_png(&img)) {
                out.push((number, Arc::new(png)));
            }
        }
        Ok(out)
    })
}

fn fingerprint(png: &[u8]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    png.hash(&mut hasher);
    hasher.finish()
}

/// Drops the images that are not content: icons, decoration and logos repeated on many
/// pages. Returns how many it dropped.
pub fn filter_images(pages: &mut [PageMedia]) -> usize {
    let mut dropped = 0;
    for page in pages.iter_mut() {
        let area = page.width * page.height;
        let before = page.images.len();
        page.images.retain(|image| {
            let covered = (image.right - image.left) * (image.top - image.bottom);
            image.width_px.min(image.height_px) >= MIN_SIDE_PX && covered >= MIN_PAGE_FRACTION * area
        });
        dropped += before - page.images.len();
    }

    let mut pages_of: HashMap<u64, HashSet<u32>> = HashMap::new();
    for page in pages.iter() {
        for image in &page.images {
            pages_of.entry(fingerprint(&image.png)).or_default().insert(page.number);
        }
    }
    let total = pages.len();
    let repeated: HashSet<u64> = pages_of
        .into_iter()
        .filter(|(_, on)| on.len() >= REPEAT_MIN_PAGES && on.len() as f32 >= REPEAT_MIN_FRACTION * total as f32)
        .map(|(hash, _)| hash)
        .collect();
    for page in pages.iter_mut() {
        let before = page.images.len();
        page.images.retain(|image| !repeated.contains(&fingerprint(&image.png)));
        dropped += before - page.images.len();
    }
    dropped
}

/// Adds the images to the converted document: the embedded ones after the last block of their
/// page, a rendered page right after the first block of its page (the slide's title).
pub fn add_media(document: &mut Document, pages: &[PageMedia], renders: &[(u32, Arc<Vec<u8>>)]) {
    let mut next_asset = document.assets.len() + 1;
    let mut new_asset = |document: &mut Document, png: &Arc<Vec<u8>>| {
        let id = AssetId(format!("img-{next_asset}"));
        next_asset += 1;
        document.assets.push(Asset { id: id.clone(), mime: "image/png".to_string(), data: png.to_vec() });
        id
    };

    for (number, png) in renders {
        let at = document
            .blocks
            .iter()
            .position(|b| b.page == Some(*number))
            .map_or_else(|| end_of_page(document, *number), |i| i + 1);
        let asset = new_asset(document, png);
        let block = image_block(asset, *number, 0);
        document.blocks.insert(at, block);
    }
    for page in pages {
        for (k, image) in page.images.iter().enumerate() {
            let at = end_of_page(document, page.number);
            let asset = new_asset(document, &image.png);
            document.blocks.insert(at, image_block(asset, page.number, k + 1));
        }
    }
}

/// Index right after the last block of pages up to `number` (so a page without text keeps
/// its place between its neighbours).
fn end_of_page(document: &Document, number: u32) -> usize {
    document.blocks.iter().rposition(|b| b.page.is_some_and(|p| p <= number)).map_or(0, |i| i + 1)
}

fn image_block(asset: AssetId, page: u32, k: usize) -> Block {
    let alt = if k == 0 { format!("p{page}") } else { format!("p{page}-{k}") };
    let mut block = Block::new(BlockKind::Image { asset, alt });
    block.page = Some(page);
    block
}

#[cfg(test)]
mod tests {
    use super::*;
    use outmark_core::Inline;

    fn image(png: &[u8], side_px: u32, side_pt: f32) -> PageImage {
        PageImage {
            left: 100.0,
            right: 100.0 + side_pt,
            bottom: 100.0,
            top: 100.0 + side_pt,
            width_px: side_px,
            height_px: side_px,
            png: Arc::new(png.to_vec()),
        }
    }

    fn page(number: u32, images: Vec<PageImage>) -> PageMedia {
        PageMedia { number, width: 600.0, height: 800.0, images, graphics: 0 }
    }

    #[test]
    fn icons_and_decoration_are_dropped() {
        let mut pages = vec![page(
            1,
            vec![image(b"big", 400, 300.0), image(b"icon", 16, 300.0), image(b"speck", 400, 20.0)],
        )];
        assert_eq!(filter_images(&mut pages), 2);
        assert_eq!(pages[0].images.len(), 1);
    }

    #[test]
    fn a_logo_on_many_pages_is_dropped_but_a_figure_stays() {
        let mut pages: Vec<PageMedia> = (1..=6)
            .map(|n| page(n, vec![image(b"logo", 200, 100.0)]))
            .collect();
        pages[2].images.push(image(b"figure", 400, 300.0));
        assert_eq!(filter_images(&mut pages), 6);
        assert_eq!(pages.iter().map(|p| p.images.len()).sum::<usize>(), 1);
        assert_eq!(&*pages[2].images[0].png, b"figure");
    }

    #[test]
    fn two_uses_of_an_image_are_not_a_logo() {
        let mut pages: Vec<PageMedia> = (1..=6).map(|n| page(n, vec![])).collect();
        pages[0].images.push(image(b"chart", 400, 300.0));
        pages[1].images.push(image(b"chart", 400, 300.0));
        assert_eq!(filter_images(&mut pages), 0);
    }

    fn text_block(page: u32) -> Block {
        let mut block = Block::new(BlockKind::Paragraph(vec![Inline::Text("t".into())]));
        block.page = Some(page);
        block
    }

    #[test]
    fn images_go_after_the_text_of_their_page_and_renders_after_the_title() {
        let mut document = Document { blocks: vec![text_block(1), text_block(1), text_block(3)], ..Default::default() };
        let pages = vec![page(1, vec![image(b"a", 100, 100.0)]), page(2, vec![image(b"b", 100, 100.0)])];
        add_media(&mut document, &pages, &[(3, Arc::new(b"r".to_vec()))]);

        let order: Vec<(Option<u32>, bool)> =
            document.blocks.iter().map(|b| (b.page, matches!(b.kind, BlockKind::Image { .. }))).collect();
        assert_eq!(
            order,
            [
                (Some(1), false),
                (Some(1), false),
                (Some(1), true),
                // page 2 has no text: its image sits between its neighbours
                (Some(2), true),
                (Some(3), false),
                (Some(3), true)
            ]
        );
        assert_eq!(document.assets.len(), 3);
        // the render comes right after the first block of page 3
        let render_ids: Vec<&str> = document
            .blocks
            .iter()
            .filter_map(|b| match &b.kind {
                BlockKind::Image { asset, alt } if alt == "p3" => Some(asset.0.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(render_ids.len(), 1);
    }
}
