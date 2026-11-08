//! Markdown reader: CommonMark plus GFM tables, strikethrough and task lists.

use std::iter::Peekable;
use std::path::Path;
use std::vec::IntoIter;

use mpt_core::{
    Asset, AssetId, Block, BlockKind, ConversionWarning, Document, Inline, ListItem, TableAlign,
};
use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

/// Result of reading a Markdown text.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParseOutput {
    pub document: Document,
    /// Non-fatal notices (missing images, ignored HTML...).
    pub warnings: Vec<ConversionWarning>,
}

/// Parses `md` into a [`Document`].
///
/// Relative image paths are resolved against `base_dir`; with `None`, images are not loaded.
pub fn parse_markdown(md: &str, base_dir: Option<&Path>) -> ParseOutput {
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let events: Vec<Event> = Parser::new_ext(md, options).collect();
    let mut reader = Reader {
        events: events.into_iter().peekable(),
        base_dir,
        assets: Vec::new(),
        warnings: Vec::new(),
    };
    let blocks = reader.blocks();
    ParseOutput {
        document: Document { blocks, assets: reader.assets, ..Default::default() },
        warnings: reader.warnings,
    }
}

struct Reader<'a> {
    events: Peekable<IntoIter<Event<'a>>>,
    base_dir: Option<&'a Path>,
    assets: Vec<Asset>,
    warnings: Vec<ConversionWarning>,
}

fn flush(pending: &mut Vec<Inline>, imgs: &mut Vec<Block>, out: &mut Vec<Block>) {
    if !pending.is_empty() {
        out.push(Block::new(BlockKind::Paragraph(std::mem::take(pending))));
    }
    out.append(imgs);
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn mime_for(path: &str) -> Option<&'static str> {
    let ext = path.rsplit('.').next()?.to_ascii_lowercase();
    match ext.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "svg" => Some("image/svg+xml"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

impl<'a> Reader<'a> {
    fn warn(&mut self, code: &str, detail: impl Into<String>) {
        self.warnings.push(ConversionWarning {
            code: code.to_string(),
            detail: detail.into(),
            page: None,
        });
    }

    /// Reads blocks until the matching `End` event (or the end of the stream).
    fn blocks(&mut self) -> Vec<Block> {
        let mut out = Vec::new();
        // Tight list items carry inline events without a Paragraph wrapper.
        let mut pending: Vec<Inline> = Vec::new();
        let mut imgs: Vec<Block> = Vec::new();

        while let Some(ev) = self.events.next() {
            match ev {
                Event::End(_) => break,
                Event::Start(Tag::Paragraph) => {
                    flush(&mut pending, &mut imgs, &mut out);
                    let inlines = self.inlines(&mut imgs);
                    pending = inlines;
                    flush(&mut pending, &mut imgs, &mut out);
                }
                Event::Start(Tag::Heading { level, .. }) => {
                    flush(&mut pending, &mut imgs, &mut out);
                    let content = self.inlines(&mut imgs);
                    out.push(Block::new(BlockKind::Heading {
                        level: heading_level(level),
                        content,
                    }));
                    out.append(&mut imgs);
                }
                Event::Start(Tag::BlockQuote(_)) => {
                    flush(&mut pending, &mut imgs, &mut out);
                    let inner = self.blocks();
                    out.push(Block::new(BlockKind::Quote(inner)));
                }
                Event::Start(Tag::CodeBlock(kind)) => {
                    flush(&mut pending, &mut imgs, &mut out);
                    let lang = match kind {
                        CodeBlockKind::Fenced(l) => l
                            .split_whitespace()
                            .next()
                            .filter(|s| !s.is_empty())
                            .map(str::to_string),
                        CodeBlockKind::Indented => None,
                    };
                    let mut code = String::new();
                    for ev in self.events.by_ref() {
                        match ev {
                            Event::Text(t) => code.push_str(&t),
                            Event::End(TagEnd::CodeBlock) => break,
                            _ => {}
                        }
                    }
                    if code.ends_with('\n') {
                        code.pop();
                    }
                    out.push(Block::new(BlockKind::CodeBlock { lang, code }));
                }
                Event::Start(Tag::List(start)) => {
                    flush(&mut pending, &mut imgs, &mut out);
                    let mut items = Vec::new();
                    while let Some(ev) = self.events.next() {
                        match ev {
                            Event::Start(Tag::Item) => items.push(self.item()),
                            Event::End(TagEnd::List(_)) => break,
                            _ => {}
                        }
                    }
                    out.push(Block::new(BlockKind::List {
                        ordered: start.is_some(),
                        start,
                        items,
                    }));
                }
                Event::Start(Tag::Table(aligns)) => {
                    flush(&mut pending, &mut imgs, &mut out);
                    out.push(Block::new(self.table(aligns)));
                }
                Event::Start(Tag::HtmlBlock) => {
                    flush(&mut pending, &mut imgs, &mut out);
                    let mut raw = String::new();
                    for ev in self.events.by_ref() {
                        match ev {
                            Event::Html(h) => raw.push_str(&h),
                            Event::End(TagEnd::HtmlBlock) => break,
                            _ => {}
                        }
                    }
                    self.warn("htmlIgnored", raw.trim());
                }
                Event::Rule => {
                    flush(&mut pending, &mut imgs, &mut out);
                    out.push(Block::new(BlockKind::ThematicBreak));
                }
                other => self.inline_event(other, &mut pending, &mut imgs),
            }
        }
        flush(&mut pending, &mut imgs, &mut out);
        out
    }

    fn item(&mut self) -> ListItem {
        let checked = match self.events.peek() {
            Some(Event::TaskListMarker(done)) => {
                let done = *done;
                self.events.next();
                Some(done)
            }
            _ => None,
        };
        ListItem { checked, blocks: self.blocks() }
    }

    fn table(&mut self, aligns: Vec<Alignment>) -> BlockKind {
        let aligns = aligns
            .into_iter()
            .map(|a| match a {
                Alignment::None => TableAlign::None,
                Alignment::Left => TableAlign::Left,
                Alignment::Center => TableAlign::Center,
                Alignment::Right => TableAlign::Right,
            })
            .collect();
        let mut header = Vec::new();
        let mut rows: Vec<Vec<Vec<Inline>>> = Vec::new();
        while let Some(ev) = self.events.next() {
            match ev {
                Event::Start(Tag::TableHead) => header = self.cells(),
                Event::Start(Tag::TableRow) => rows.push(self.cells()),
                Event::End(TagEnd::Table) => break,
                _ => {}
            }
        }
        BlockKind::Table { aligns, header, rows }
    }

    /// Reads table cells until the end of the row or header.
    fn cells(&mut self) -> Vec<Vec<Inline>> {
        let mut cells = Vec::new();
        while let Some(ev) = self.events.next() {
            match ev {
                Event::Start(Tag::TableCell) => {
                    let mut imgs = Vec::new();
                    cells.push(self.inlines(&mut imgs));
                }
                Event::End(_) => break,
                _ => {}
            }
        }
        cells
    }

    /// Reads inline content until the matching `End` event.
    /// Images found inside are emitted as separate blocks through `imgs`.
    fn inlines(&mut self, imgs: &mut Vec<Block>) -> Vec<Inline> {
        let mut out = Vec::new();
        while let Some(ev) = self.events.next() {
            if matches!(ev, Event::End(_)) {
                break;
            }
            self.inline_event(ev, &mut out, imgs);
        }
        out
    }

    fn inline_event(&mut self, ev: Event<'a>, out: &mut Vec<Inline>, imgs: &mut Vec<Block>) {
        match ev {
            Event::Text(t) => out.push(Inline::Text(t.to_string())),
            Event::Code(c) => out.push(Inline::Code(c.to_string())),
            Event::SoftBreak => out.push(Inline::Text(" ".to_string())),
            Event::HardBreak => out.push(Inline::LineBreak),
            Event::Html(h) | Event::InlineHtml(h) => self.warn("htmlIgnored", h.trim()),
            Event::Start(Tag::Emphasis) => {
                let inner = self.inlines(imgs);
                out.push(Inline::Emphasis(inner));
            }
            Event::Start(Tag::Strong) => {
                let inner = self.inlines(imgs);
                out.push(Inline::Strong(inner));
            }
            Event::Start(Tag::Strikethrough) => {
                let inner = self.inlines(imgs);
                out.push(Inline::Strikethrough(inner));
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                let content = self.inlines(imgs);
                out.push(Inline::Link { url: dest_url.to_string(), content });
            }
            Event::Start(Tag::Image { dest_url, .. }) => {
                let mut alt = String::new();
                for ev in self.events.by_ref() {
                    match ev {
                        Event::Text(t) | Event::Code(t) => alt.push_str(&t),
                        Event::End(TagEnd::Image) => break,
                        _ => {}
                    }
                }
                match self.load_image(&dest_url) {
                    Some(id) => imgs.push(Block::new(BlockKind::Image { asset: id, alt })),
                    None => out.push(Inline::Emphasis(vec![Inline::Text(format!(
                        "[missing image: {dest_url}]"
                    ))])),
                }
            }
            // Block-level or unsupported events do not belong in inline content.
            _ => {}
        }
    }

    fn load_image(&mut self, dest: &str) -> Option<AssetId> {
        let Some(mime) = mime_for(dest) else {
            self.warn("imageUnsupported", dest);
            return None;
        };
        let Some(base) = self.base_dir else {
            self.warn("imageMissing", dest);
            return None;
        };
        if dest.contains("://") {
            self.warn("imageRemoteSkipped", dest);
            return None;
        }
        match std::fs::read(base.join(dest)) {
            Ok(data) => {
                let id = AssetId(format!("img-{}", self.assets.len() + 1));
                self.assets.push(Asset { id: id.clone(), mime: mime.to_string(), data });
                Some(id)
            }
            Err(e) => {
                self.warn("imageMissing", format!("{dest}: {e}"));
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(md: &str) -> ParseOutput {
        parse_markdown(md, None)
    }

    fn text(s: &str) -> Inline {
        Inline::Text(s.to_string())
    }

    #[test]
    fn headings_and_paragraphs() {
        let out = parse("# Title\n\nHello **world**.\n");
        assert_eq!(out.document.blocks.len(), 2);
        assert_eq!(
            out.document.blocks[0].kind,
            BlockKind::Heading { level: 1, content: vec![text("Title")] }
        );
        assert_eq!(
            out.document.blocks[1].kind,
            BlockKind::Paragraph(vec![
                text("Hello "),
                Inline::Strong(vec![text("world")]),
                text("."),
            ])
        );
    }

    #[test]
    fn tight_nested_lists_keep_structure() {
        let out = parse("- one\n- two\n  - nested\n");
        let BlockKind::List { ordered, items, .. } = &out.document.blocks[0].kind else {
            panic!("expected list");
        };
        assert!(!ordered);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].blocks[0].kind, BlockKind::Paragraph(vec![text("one")]));
        assert_eq!(items[1].blocks.len(), 2);
        assert!(matches!(items[1].blocks[1].kind, BlockKind::List { .. }));
    }

    #[test]
    fn ordered_list_remembers_start() {
        let out = parse("3. a\n4. b\n");
        let BlockKind::List { ordered, start, .. } = &out.document.blocks[0].kind else {
            panic!("expected list");
        };
        assert!(*ordered);
        assert_eq!(*start, Some(3));
    }

    #[test]
    fn task_list_markers() {
        let out = parse("- [x] done\n- [ ] todo\n");
        let BlockKind::List { items, .. } = &out.document.blocks[0].kind else {
            panic!("expected list");
        };
        assert_eq!(items[0].checked, Some(true));
        assert_eq!(items[1].checked, Some(false));
    }

    #[test]
    fn fenced_code_keeps_language_and_text() {
        let out = parse("```rust\nfn main() {}\n```\n");
        assert_eq!(
            out.document.blocks[0].kind,
            BlockKind::CodeBlock { lang: Some("rust".into()), code: "fn main() {}".into() }
        );
    }

    #[test]
    fn table_with_alignment() {
        let out = parse("| a | b |\n|:--|--:|\n| 1 | 2 |\n");
        let BlockKind::Table { aligns, header, rows } = &out.document.blocks[0].kind else {
            panic!("expected table");
        };
        assert_eq!(aligns, &vec![TableAlign::Left, TableAlign::Right]);
        assert_eq!(header, &vec![vec![text("a")], vec![text("b")]]);
        assert_eq!(rows, &vec![vec![vec![text("1")], vec![text("2")]]]);
    }

    #[test]
    fn quote_and_rule() {
        let out = parse("> quoted\n\n---\n");
        assert!(matches!(out.document.blocks[0].kind, BlockKind::Quote(_)));
        assert_eq!(out.document.blocks[1].kind, BlockKind::ThematicBreak);
    }

    #[test]
    fn missing_image_warns_and_leaves_placeholder() {
        let out = parse_markdown("![alt](nope.png)\n", Some(Path::new(".")));
        assert_eq!(out.warnings.len(), 1);
        assert_eq!(out.warnings[0].code, "imageMissing");
        assert!(out.document.assets.is_empty());
        assert!(matches!(out.document.blocks[0].kind, BlockKind::Paragraph(_)));
    }

    #[test]
    fn unsupported_image_extension_warns() {
        let out = parse_markdown("![x](a.bmp)\n", Some(Path::new(".")));
        assert_eq!(out.warnings[0].code, "imageUnsupported");
    }

    #[test]
    fn html_is_ignored_with_warning() {
        let out = parse("<div>hi</div>\n\ntext <b>x</b>\n");
        assert!(out.warnings.iter().all(|w| w.code == "htmlIgnored"));
        assert!(!out.warnings.is_empty());
    }
}
