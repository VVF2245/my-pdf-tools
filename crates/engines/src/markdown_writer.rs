//! Markdown writer: [`Document`] -> Markdown text (CommonMark + GFM tables, task lists,
//! strikethrough). Images are returned separately so the caller can save them to disk.

use mpt_core::{Asset, BlockKind, Document, Inline, TableAlign};

/// An image the Markdown refers to, to be saved at `path` (relative to the `.md` file).
#[derive(Debug, Clone, PartialEq)]
pub struct MarkdownImage {
    pub path: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MarkdownOutput {
    pub text: String,
    /// Only the assets some `Image` block refers to.
    pub images: Vec<MarkdownImage>,
}

/// Writes `doc` as Markdown. Image links point into `images_dir` (e.g. `deck_images`).
pub fn to_markdown(doc: &Document, images_dir: &str) -> MarkdownOutput {
    let mut writer = Writer { doc, images_dir, images: Vec::new() };
    let blocks: Vec<String> = doc.blocks.iter().map(|b| writer.block(&b.kind)).filter(|s| !s.is_empty()).collect();
    let mut text = blocks.join("\n\n");
    if !text.is_empty() {
        text.push('\n');
    }
    MarkdownOutput { text, images: writer.images }
}

struct Writer<'a> {
    doc: &'a Document,
    images_dir: &'a str,
    images: Vec<MarkdownImage>,
}

fn extension(asset: &Asset) -> &'static str {
    match asset.mime.as_str() {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/svg+xml" => "svg",
        "image/webp" => "webp",
        _ => "bin",
    }
}

/// Whether `after_ampersand` (the text following a `&`) reads as an entity: `name;` or `#digits;`.
fn starts_entity(after_ampersand: &str) -> bool {
    let name_len = after_ampersand.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '#').count();
    name_len > 0 && after_ampersand[name_len..].starts_with(';')
}

/// Backslash-escapes the characters that would be read as Markdown inline syntax.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for (index, c) in text.char_indices() {
        match c {
            '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '|' | '~' => {
                out.push('\\');
                out.push(c);
            }
            // `&copy;` or `&#65;` in the text must not be read back as an entity
            // (`R&D` and `AT&T` are left alone: no `;` follows)
            '&' if starts_entity(&text[index + 1..]) => out.push_str("\\&"),
            '\n' | '\r' => out.push(' '),
            _ => out.push(c),
        }
    }
    out
}

/// Escapes what would turn a paragraph into another block: `# `, `- `, `+ `, `1. `...
fn escape_block_start(text: String) -> String {
    let first = text.chars().next();
    if matches!(first, Some('#' | '-' | '+')) {
        return format!("\\{text}");
    }
    let digits = text.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 && matches!(text[digits..].chars().next(), Some('.' | ')')) {
        let (number, rest) = text.split_at(digits);
        let mut rest = rest.chars();
        let mark = rest.next().unwrap();
        return format!("{number}\\{mark}{}", rest.as_str());
    }
    text
}

/// Percent-encodes whitespace, angle brackets, parentheses and the characters in `also`.
/// Everything else, accents included, is kept as it is.
fn encode_target(target: &str, also: &[char]) -> String {
    let mut out = String::with_capacity(target.len());
    for c in target.chars() {
        match c {
            ' ' | '\n' | '\r' | '\t' => out.push_str("%20"),
            '<' | '>' | '(' | ')' => out.push_str(&format!("%{:02X}", c as u32)),
            c if also.contains(&c) => out.push_str(&format!("%{:02X}", c as u32)),
            _ => out.push(c),
        }
    }
    out
}

/// A URL that cannot end early. `%`, `#` and `?` already mean something in a URL, so they
/// are left alone.
fn url_destination(url: &str) -> String {
    encode_target(url, &[])
}

/// The link to a file on disk (`my notes_images/a.png`). Unlike a URL, every `%`, `#` and
/// `?` in a file name is literal text, so those are encoded too.
fn path_destination(path: &str) -> String {
    encode_target(path, &['%', '#', '?'])
}

fn longest_run(text: &str, c: char) -> usize {
    let (mut longest, mut current) = (0, 0);
    for ch in text.chars() {
        if ch == c {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    longest
}

fn indent(text: &str, prefix: &str) -> String {
    text.lines()
        .map(|line| if line.is_empty() { String::new() } else { format!("{prefix}{line}") })
        .collect::<Vec<_>>()
        .join("\n")
}

impl Writer<'_> {
    /// `flat`: the text must stay on one line (headings, table cells), so a hard line break
    /// is written as a space.
    fn inlines(&self, inlines: &[Inline], flat: bool) -> String {
        inlines.iter().map(|i| self.inline(i, flat)).collect()
    }

    fn inline(&self, inline: &Inline, flat: bool) -> String {
        match inline {
            Inline::Text(t) => escape(t),
            Inline::Emphasis(inner) => wrap("*", &self.inlines(inner, flat)),
            Inline::Strong(inner) => wrap("**", &self.inlines(inner, flat)),
            Inline::Strikethrough(inner) => wrap("~~", &self.inlines(inner, flat)),
            Inline::Code(code) => {
                let fence = "`".repeat(longest_run(code, '`') + 1);
                let edge_space = code.starts_with(' ') || code.ends_with(' ');
                let pad = if (code.starts_with('`') || code.ends_with('`') || edge_space) && !code.chars().all(|c| c == ' ') { " " } else { "" };
                format!("{fence}{pad}{code}{pad}{fence}")
            }
            Inline::Link { url, content } => format!("[{}]({})", self.inlines(content, flat), url_destination(url)),
            Inline::LineBreak if flat => " ".to_string(),
            Inline::LineBreak => "  \n".to_string(),
        }
    }

    fn block(&mut self, kind: &BlockKind) -> String {
        match kind {
            BlockKind::Heading { level, content } => {
                format!("{} {}", "#".repeat((*level).clamp(1, 6) as usize), self.inlines(content, true))
            }
            BlockKind::Paragraph(content) => escape_block_start(self.inlines(content, false)),
            BlockKind::List { ordered, start, items } => self.list(*ordered, *start, items),
            BlockKind::CodeBlock { lang, code } => {
                let fence = "`".repeat((longest_run(code, '`') + 1).max(3));
                format!("{fence}{}\n{code}\n{fence}", lang.as_deref().unwrap_or(""))
            }
            BlockKind::Table { aligns, header, rows } => self.table(aligns, header, rows),
            BlockKind::Image { asset, alt } => match self.doc.asset(asset) {
                Some(a) => {
                    let path = format!("{}/{}.{}", self.images_dir, a.id.0, extension(a));
                    if !self.images.iter().any(|i| i.path == path) {
                        self.images.push(MarkdownImage { path: path.clone(), data: a.data.clone() });
                    }
                    format!("![{}]({})", escape(alt), path_destination(&path))
                }
                None => String::new(),
            },
            BlockKind::Quote(blocks) => {
                let inner: Vec<String> = blocks.iter().map(|b| self.block(&b.kind)).collect();
                inner
                    .join("\n\n")
                    .lines()
                    .map(|l| if l.is_empty() { ">".to_string() } else { format!("> {l}") })
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            BlockKind::ThematicBreak | BlockKind::PageBreak => "---".to_string(),
        }
    }

    fn list(&mut self, ordered: bool, start: Option<u64>, items: &[mpt_core::ListItem]) -> String {
        let first_number = start.unwrap_or(1);
        let mut out: Vec<String> = Vec::new();
        for (index, item) in items.iter().enumerate() {
            let marker = if ordered { format!("{}. ", first_number + index as u64) } else { "- ".to_string() };
            let task = match item.checked {
                Some(true) => "[x] ",
                Some(false) => "[ ] ",
                None => "",
            };
            let pad = " ".repeat(marker.len());

            let mut parts = item.blocks.iter().map(|b| (matches!(b.kind, BlockKind::List { .. }), self.block(&b.kind)));
            let mut text = String::new();
            if let Some((_, first)) = parts.next() {
                text = indent_after_first(&first, &pad);
            }
            let mut rendered = format!("{marker}{task}{text}");
            for (is_list, part) in parts {
                // a nested list follows directly; any other block is separated by a blank line
                rendered.push_str(if is_list { "\n" } else { "\n\n" });
                rendered.push_str(&indent(&part, &pad));
            }
            out.push(rendered);
        }
        out.join("\n")
    }

    fn table(&self, aligns: &[TableAlign], header: &[Vec<Inline>], rows: &[Vec<Vec<Inline>>]) -> String {
        let columns = aligns.len().max(header.len());
        if columns == 0 {
            return String::new();
        }
        let cell = |cells: &[Vec<Inline>], i: usize| cells.get(i).map(|c| self.inlines(c, true)).unwrap_or_default();
        let row = |cells: &[Vec<Inline>]| {
            format!("| {} |", (0..columns).map(|i| cell(cells, i)).collect::<Vec<_>>().join(" | "))
        };
        let separator = (0..columns)
            .map(|i| match aligns.get(i).copied().unwrap_or(TableAlign::None) {
                TableAlign::Left => ":---",
                TableAlign::Center => ":---:",
                TableAlign::Right => "---:",
                TableAlign::None => "---",
            })
            .collect::<Vec<_>>()
            .join(" | ");
        let mut lines = vec![row(header), format!("| {separator} |")];
        lines.extend(rows.iter().map(|r| row(r)));
        lines.join("\n")
    }
}

/// Wraps already-escaped content in emphasis markers, keeping the surrounding spaces outside.
fn wrap(marker: &str, content: &str) -> String {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return content.to_string();
    }
    let lead = &content[..content.len() - content.trim_start().len()];
    let trail = &content[content.trim_end().len()..];
    format!("{lead}{marker}{trimmed}{marker}{trail}")
}

/// Indents every line after the first (the first sits next to the list marker).
fn indent_after_first(text: &str, pad: &str) -> String {
    let mut lines = text.lines();
    let mut out = lines.next().unwrap_or("").to_string();
    for line in lines {
        out.push('\n');
        if !line.is_empty() {
            out.push_str(pad);
        }
        out.push_str(line);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown::parse_markdown;
    use mpt_core::{AssetId, Block, ListItem};

    fn doc(blocks: Vec<BlockKind>) -> Document {
        Document { blocks: blocks.into_iter().map(Block::new).collect(), ..Default::default() }
    }

    fn md(blocks: Vec<BlockKind>) -> String {
        to_markdown(&doc(blocks), "img").text
    }

    fn text(s: &str) -> Vec<Inline> {
        vec![Inline::Text(s.to_string())]
    }

    fn item(s: &str) -> ListItem {
        ListItem { checked: None, blocks: vec![Block::new(BlockKind::Paragraph(text(s)))] }
    }

    #[test]
    fn headings_and_paragraphs_are_separated_by_blank_lines() {
        let out = md(vec![
            BlockKind::Heading { level: 2, content: text("Title") },
            BlockKind::Paragraph(text("Body.")),
        ]);
        assert_eq!(out, "## Title\n\nBody.\n");
    }

    #[test]
    fn inline_formatting() {
        let out = md(vec![BlockKind::Paragraph(vec![
            Inline::Text("a ".into()),
            Inline::Strong(text("bold")),
            Inline::Text(" ".into()),
            Inline::Emphasis(text("it")),
            Inline::Text(" ".into()),
            Inline::Strikethrough(text("old")),
            Inline::Text(" ".into()),
            Inline::Code("x".into()),
            Inline::Text(" ".into()),
            Inline::Link { url: "https://e.com".into(), content: text("link") },
        ])]);
        assert_eq!(out, "a **bold** *it* ~~old~~ `x` [link](https://e.com)\n");
    }

    #[test]
    fn spaces_stay_outside_emphasis_markers() {
        let out = md(vec![BlockKind::Paragraph(vec![Inline::Strong(text(" spaced "))])]);
        assert_eq!(out, " **spaced** \n");
    }

    #[test]
    fn markdown_characters_in_text_are_escaped() {
        let out = md(vec![BlockKind::Paragraph(text("a*b_c [d] <e> `f` g|h"))]);
        assert_eq!(out, "a\\*b\\_c \\[d\\] \\<e\\> \\`f\\` g\\|h\n");
    }

    #[test]
    fn text_that_looks_like_a_block_marker_is_escaped() {
        assert_eq!(md(vec![BlockKind::Paragraph(text("# not a heading"))]), "\\# not a heading\n");
        assert_eq!(md(vec![BlockKind::Paragraph(text("- not a list"))]), "\\- not a list\n");
        assert_eq!(md(vec![BlockKind::Paragraph(text("2026. a year"))]), "2026\\. a year\n");
        assert_eq!(md(vec![BlockKind::Paragraph(text("2026 is fine"))]), "2026 is fine\n");
    }

    #[test]
    fn nested_and_ordered_lists() {
        let nested = ListItem {
            checked: None,
            blocks: vec![
                Block::new(BlockKind::Paragraph(text("two"))),
                Block::new(BlockKind::List { ordered: false, start: None, items: vec![item("inner")] }),
            ],
        };
        let out = md(vec![
            BlockKind::List { ordered: false, start: None, items: vec![item("one"), nested] },
            BlockKind::List { ordered: true, start: Some(3), items: vec![item("a"), item("b")] },
        ]);
        assert_eq!(out, "- one\n- two\n  - inner\n\n3. a\n4. b\n");
    }

    #[test]
    fn task_list_markers() {
        let items = vec![
            ListItem { checked: Some(true), blocks: vec![Block::new(BlockKind::Paragraph(text("done")))] },
            ListItem { checked: Some(false), blocks: vec![Block::new(BlockKind::Paragraph(text("todo")))] },
        ];
        assert_eq!(md(vec![BlockKind::List { ordered: false, start: None, items }]), "- [x] done\n- [ ] todo\n");
    }

    #[test]
    fn code_blocks_use_a_fence_longer_than_any_backtick_run_inside() {
        let out = md(vec![BlockKind::CodeBlock { lang: Some("md".into()), code: "```\ninner\n```".into() }]);
        assert_eq!(out, "````md\n```\ninner\n```\n````\n");
        let plain = md(vec![BlockKind::CodeBlock { lang: None, code: "x = 1".into() }]);
        assert_eq!(plain, "```\nx = 1\n```\n");
    }

    #[test]
    fn inline_code_with_backticks() {
        let out = md(vec![BlockKind::Paragraph(vec![Inline::Code("a`b".into())])]);
        assert_eq!(out, "``a`b``\n");
    }

    #[test]
    fn tables_have_a_header_separator_with_alignment() {
        let out = md(vec![BlockKind::Table {
            aligns: vec![TableAlign::Left, TableAlign::Right, TableAlign::Center, TableAlign::None],
            header: vec![text("a"), text("b"), text("c"), text("d")],
            rows: vec![vec![text("1"), text("2"), text("3"), text("x|y")]],
        }]);
        assert_eq!(out, "| a | b | c | d |\n| :--- | ---: | :---: | --- |\n| 1 | 2 | 3 | x\\|y |\n");
    }

    #[test]
    fn quotes_rules_and_breaks() {
        let out = md(vec![
            BlockKind::Quote(vec![Block::new(BlockKind::Paragraph(text("one"))), Block::new(BlockKind::Paragraph(text("two")))]),
            BlockKind::ThematicBreak,
        ]);
        assert_eq!(out, "> one\n>\n> two\n\n---\n");
    }

    #[test]
    fn images_are_returned_for_the_caller_to_save() {
        let document = Document {
            blocks: vec![
                Block::new(BlockKind::Image { asset: AssetId("img-1".into()), alt: "a [pic]".into() }),
                Block::new(BlockKind::Image { asset: AssetId("img-1".into()), alt: "again".into() }),
            ],
            assets: vec![
                Asset { id: AssetId("img-1".into()), mime: "image/png".into(), data: vec![1, 2] },
                Asset { id: AssetId("unused".into()), mime: "image/png".into(), data: vec![9] },
            ],
            ..Default::default()
        };
        let out = to_markdown(&document, "deck_images");
        assert_eq!(out.text, "![a \\[pic\\]](deck_images/img-1.png)\n\n![again](deck_images/img-1.png)\n");
        assert_eq!(out.images, vec![MarkdownImage { path: "deck_images/img-1.png".into(), data: vec![1, 2] }]);
    }

    #[test]
    fn image_links_survive_spaces_and_accents_in_the_folder_name() {
        let document = Document {
            blocks: vec![Block::new(BlockKind::Image { asset: AssetId("img-1".into()), alt: String::new() })],
            assets: vec![Asset { id: AssetId("img-1".into()), mime: "image/png".into(), data: vec![1] }],
            ..Default::default()
        };
        let out = to_markdown(&document, "apuntes \u{00F1}and\u{00FA} (1)_images");
        // the link is encoded; the file is saved under the real (unencoded) name
        assert_eq!(out.text, "![](apuntes%20\u{00F1}and\u{00FA}%20%281%29_images/img-1.png)\n");
        assert_eq!(out.images[0].path, "apuntes \u{00F1}and\u{00FA} (1)_images/img-1.png");
    }

    #[test]
    fn image_paths_encode_percent_hash_and_question_mark() {
        let document = Document {
            blocks: vec![Block::new(BlockKind::Image { asset: AssetId("img-1".into()), alt: String::new() })],
            assets: vec![Asset { id: AssetId("img-1".into()), mime: "image/png".into(), data: vec![1] }],
            ..Default::default()
        };
        let out = to_markdown(&document, "Tema #2 100% ok?_images");
        assert_eq!(out.text, "![](Tema%20%232%20100%25%20ok%3F_images/img-1.png)\n");
        assert_eq!(out.images[0].path, "Tema #2 100% ok?_images/img-1.png");
    }

    #[test]
    fn link_urls_keep_their_fragment_and_query() {
        let out = md(vec![BlockKind::Paragraph(vec![Inline::Link {
            url: "https://e.com/p?a=1&b=2#top".into(),
            content: text("t"),
        }])]);
        assert_eq!(out, "[t](https://e.com/p?a=1&b=2#top)\n");
    }

    #[test]
    fn an_ampersand_that_could_start_an_entity_is_escaped() {
        assert_eq!(md(vec![BlockKind::Paragraph(text("&copy; &#65; AT&T R&D & more &x"))]), "\\&copy; \\&#65; AT&T R&D & more &x\n");
    }

    #[test]
    fn inline_code_keeps_its_edge_spaces() {
        let one = |code: &str| md(vec![BlockKind::Paragraph(vec![Inline::Code(code.into())])]);
        // one space is stripped from each side when both are present, so pad to compensate
        assert_eq!(one("  x "), "`   x  `\n");
        assert_eq!(one(" x"), "`  x `\n");
        assert_eq!(one("x"), "`x`\n");
        assert_eq!(one("   "), "`   `\n", "all spaces: nothing is stripped");
    }

    #[test]
    fn link_urls_with_spaces_or_brackets_do_not_end_early() {
        let out = md(vec![BlockKind::Paragraph(vec![Inline::Link {
            url: "https://e.com/a b(1)<x>".into(),
            content: text("t"),
        }])]);
        assert_eq!(out, "[t](https://e.com/a%20b%281%29%3Cx%3E)\n");
    }

    #[test]
    fn a_hard_line_break_stays_a_break_in_a_paragraph_but_not_in_a_cell_or_heading() {
        let with_break = || vec![Inline::Text("a".into()), Inline::LineBreak, Inline::Text("b".into())];
        assert_eq!(md(vec![BlockKind::Paragraph(with_break())]), "a  \nb\n");
        assert_eq!(md(vec![BlockKind::Heading { level: 2, content: with_break() }]), "## a b\n");
        let table = md(vec![BlockKind::Table {
            aligns: vec![TableAlign::None],
            header: vec![with_break()],
            rows: vec![vec![with_break()]],
        }]);
        assert_eq!(table, "| a b |\n| --- |\n| a b |\n");
    }

    #[test]
    fn an_empty_document_is_an_empty_string() {
        assert_eq!(to_markdown(&Document::default(), "img").text, "");
    }

    #[test]
    fn writing_and_reading_back_gives_the_same_document() {
        let source = "# Title\n\nPara with **bold**, *em*, ~~old~~, `code` and a [link](https://e.com). \
            Specials: \\* \\_ \\# 1\\. \\[x\\].\n\n\
            - one\n- two\n  - nested\n    - deeper\n\n3. a\n4. b\n\n- [x] done\n- [ ] todo\n\n\
            ```rust\nfn main() {}\n```\n\n\
            | a | b |\n|:--|--:|\n| 1 | 2 |\n\n> quoted\n\n---\n\nEnd.\n";
        let first = parse_markdown(source, None).document;
        let written = to_markdown(&first, "img").text;
        let second = parse_markdown(&written, None).document;
        assert_eq!(first, second, "written Markdown:\n{written}");
    }
}
