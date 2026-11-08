//! IR -> Typst markup.
//!
//! Text is emitted as Typst string literals (`#"..."`), so no markup character
//! (`#`, `$`, `*`, `_`...) ever needs escaping.

use mpt_core::{Asset, Block, BlockKind, Document, Inline, TableAlign};

use super::PdfOptions;

/// Virtual path (without leading slash) under which an asset is exposed to Typst.
pub fn asset_path(asset: &Asset) -> String {
    let ext = match asset.mime.as_str() {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/svg+xml" => "svg",
        "image/webp" => "webp",
        _ => "bin",
    };
    format!("assets/{}.{ext}", asset.id.0)
}

/// Quotes arbitrary text as a Typst string literal.
fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Converts the document into a complete Typst source (preamble included).
pub fn to_typst(doc: &Document, options: &PdfOptions) -> String {
    let lang = doc.metadata.language.as_deref().unwrap_or("es");
    let mut out = String::new();
    out.push_str(&format!(
        "#set page(paper: {}, margin: {}cm, numbering: \"1\")\n",
        quote(options.paper.typst_name()),
        options.margin_cm
    ));
    out.push_str(&format!(
        "#set text(font: \"Libertinus Serif\", size: {}pt, lang: {})\n",
        options.font_size_pt,
        quote(lang)
    ));
    out.push_str("#set par(justify: true, leading: 0.65em)\n");
    out.push_str("#show raw: set text(font: \"DejaVu Sans Mono\", size: 0.82em)\n");
    out.push_str("#show raw.where(block: true): it => block(fill: luma(245), inset: 8pt, radius: 3pt, width: 100%, it)\n");
    out.push_str("#show heading: set block(above: 1.4em, below: 0.8em)\n");
    out.push_str("#show table: set text(size: 0.92em)\n");
    out.push_str("#show link: set text(fill: rgb(\"#1a56db\"))\n");
    if let Some(title) = &doc.metadata.title {
        out.push_str(&format!("#set document(title: {})\n", quote(title)));
    }
    out.push('\n');
    write_blocks(&doc.blocks, doc, &mut out);
    out
}

fn write_blocks(blocks: &[Block], doc: &Document, out: &mut String) {
    for block in blocks {
        write_block(&block.kind, doc, out);
    }
}

fn write_block(kind: &BlockKind, doc: &Document, out: &mut String) {
    match kind {
        BlockKind::Heading { level, content } => {
            out.push_str(&"=".repeat((*level).clamp(1, 6) as usize));
            out.push(' ');
            write_inlines(content, out);
            out.push_str("\n\n");
        }
        BlockKind::Paragraph(content) => {
            write_inlines(content, out);
            out.push_str("\n\n");
        }
        BlockKind::List { ordered, start, items } => {
            if *ordered {
                match start {
                    Some(n) => out.push_str(&format!("#enum(start: {n}, ")),
                    None => out.push_str("#enum("),
                }
            } else {
                out.push_str("#list(");
            }
            for item in items {
                out.push('[');
                match item.checked {
                    Some(true) => out.push_str("#\"\u{2611} \""),
                    Some(false) => out.push_str("#\"\u{2610} \""),
                    None => {}
                }
                write_blocks(&item.blocks, doc, out);
                out.push_str("], ");
            }
            out.push_str(")\n\n");
        }
        BlockKind::CodeBlock { lang, code } => {
            let lang_arg = match lang {
                Some(l) => format!("lang: {}, ", quote(l)),
                None => String::new(),
            };
            out.push_str(&format!("#raw(block: true, {lang_arg}{})\n\n", quote(code)));
        }
        BlockKind::Table { aligns, header, rows } => {
            let cols = aligns.len().max(header.len()).max(1);
            let align_list = (0..cols)
                .map(|i| match aligns.get(i).copied().unwrap_or(TableAlign::None) {
                    TableAlign::Right => "right",
                    TableAlign::Center => "center",
                    _ => "left",
                })
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str(&format!(
                "#align(center, table(columns: {cols}, inset: 6pt, align: ({align_list},), stroke: 0.5pt + luma(150),\n"
            ));
            out.push_str("table.header(");
            for cell in header {
                write_cell(cell, out);
            }
            out.push_str("),\n");
            for row in rows {
                for cell in row {
                    write_cell(cell, out);
                }
                out.push('\n');
            }
            out.push_str("))\n\n");
        }
        BlockKind::Image { asset, alt } => match doc.asset(asset) {
            Some(a) => out.push_str(&format!(
                "#align(center, image({}, width: 70%, alt: {}))\n\n",
                quote(&format!("/{}", asset_path(a))),
                quote(alt)
            )),
            None => out.push_str(&format!(
                "#emph[{}]\n\n",
                quote(&format!("[missing image: {}]", asset.0))
            )),
        },
        BlockKind::Quote(blocks) => {
            out.push_str("#quote(block: true)[\n");
            write_blocks(blocks, doc, out);
            out.push_str("]\n\n");
        }
        BlockKind::ThematicBreak => {
            out.push_str("#line(length: 100%, stroke: 0.5pt + luma(160))\n\n");
        }
        BlockKind::PageBreak => out.push_str("#pagebreak()\n\n"),
    }
}

fn write_cell(content: &[Inline], out: &mut String) {
    out.push('[');
    write_inlines(content, out);
    out.push_str("], ");
}

fn write_inlines(inlines: &[Inline], out: &mut String) {
    for inline in inlines {
        match inline {
            Inline::Text(t) => out.push_str(&format!("#{}", quote(t))),
            Inline::Code(c) => out.push_str(&format!("#raw({})", quote(c))),
            Inline::Emphasis(inner) => wrap("#emph[", inner, out),
            Inline::Strong(inner) => wrap("#strong[", inner, out),
            Inline::Strikethrough(inner) => wrap("#strike[", inner, out),
            Inline::Link { url, content } => wrap(&format!("#link({})[", quote(url)), content, out),
            Inline::LineBreak => out.push_str("#linebreak()"),
        }
    }
}

fn wrap(open: &str, inner: &[Inline], out: &mut String) {
    out.push_str(open);
    write_inlines(inner, out);
    out.push(']');
}

#[cfg(test)]
mod tests {
    use super::*;
    use mpt_core::{AssetId, Metadata};

    fn doc(blocks: Vec<BlockKind>) -> Document {
        Document { blocks: blocks.into_iter().map(Block::new).collect(), ..Default::default() }
    }

    #[test]
    fn quote_escapes_backslash_quote_and_newline() {
        assert_eq!(quote("a\\b\"c\nd"), "\"a\\\\b\\\"c\\nd\"");
    }

    #[test]
    fn special_characters_stay_inside_string_literals() {
        let d = doc(vec![BlockKind::Paragraph(vec![Inline::Text("# $ * _ <x>".into())])]);
        let typ = to_typst(&d, &PdfOptions::default());
        assert!(typ.contains("#\"# $ * _ <x>\""));
    }

    #[test]
    fn heading_uses_equal_signs() {
        let d = doc(vec![BlockKind::Heading { level: 3, content: vec![Inline::Text("h".into())] }]);
        assert!(to_typst(&d, &PdfOptions::default()).contains("=== #\"h\""));
    }

    #[test]
    fn asset_path_uses_mime_extension() {
        let a = Asset { id: AssetId("img-1".into()), mime: "image/svg+xml".into(), data: vec![] };
        assert_eq!(asset_path(&a), "assets/img-1.svg");
    }

    #[test]
    fn title_and_language_go_into_preamble() {
        let d = Document {
            metadata: Metadata {
                title: Some("Mi doc".into()),
                language: Some("en".into()),
                ..Default::default()
            },
            ..Default::default()
        };
        let typ = to_typst(&d, &PdfOptions::default());
        assert!(typ.contains("lang: \"en\""));
        assert!(typ.contains("#set document(title: \"Mi doc\")"));
    }

    #[test]
    fn missing_asset_becomes_placeholder() {
        let d = doc(vec![BlockKind::Image { asset: AssetId("zz".into()), alt: String::new() }]);
        assert!(to_typst(&d, &PdfOptions::default()).contains("missing image: zz"));
    }
}
