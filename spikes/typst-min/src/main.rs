//! Spike S0-C: Markdown -> Typst markup -> PDF, fully embedded (no external processes).
//!
//! Usage: typst-min <input.md> <output.pdf> [--typ <output.typ>]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime};
use typst_layout::PagedDocument;
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};

const MAIN_PATH: &str = "/main.typ";

/// In-memory `World`: one source, a set of virtual files (images) and the bundled fonts.
struct SpikeWorld {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
    main: FileId,
    source: Source,
    files: HashMap<String, Bytes>,
}

impl SpikeWorld {
    fn new(markup: String, files: HashMap<String, Bytes>) -> Self {
        let fonts: Vec<Font> = typst_assets::fonts()
            .flat_map(|data| Font::iter(Bytes::new(data)))
            .collect();
        let mut book = FontBook::new();
        for font in &fonts {
            book.push(font.info().clone());
        }
        let main = RootedPath::new(VirtualRoot::Project, VirtualPath::new(MAIN_PATH).unwrap())
            .intern();
        Self {
            library: LazyHash::new(Library::default()),
            book: LazyHash::new(book),
            fonts,
            main,
            source: Source::new(main, markup),
            files,
        }
    }
}

impl World for SpikeWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }
    fn book(&self) -> &LazyHash<FontBook> {
        &self.book
    }
    fn main(&self) -> FileId {
        self.main
    }
    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main {
            Ok(self.source.clone())
        } else {
            Err(FileError::NotFound(id.vpath().get_without_slash().into()))
        }
    }
    fn file(&self, id: FileId) -> FileResult<Bytes> {
        let key = id.vpath().get_without_slash();
        self.files
            .get(key)
            .cloned()
            .ok_or_else(|| FileError::NotFound(key.into()))
    }
    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.get(index).cloned()
    }
    fn today(&self, _offset: Option<typst::foundations::Duration>) -> Option<Datetime> {
        // Fixed date: reproducible output across machines and runs.
        Datetime::from_ymd(2026, 1, 1)
    }
}

/// Quotes arbitrary text as a Typst string literal, so no markup character needs escaping.
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

const PREAMBLE: &str = r##"#set page(paper: "a4", margin: 2.2cm, numbering: "1")
#set text(font: "Libertinus Serif", size: 11pt, lang: "es")
#set par(justify: true, leading: 0.65em)
#show raw: set text(font: "DejaVu Sans Mono", size: 9pt)
#show raw.where(block: true): it => block(fill: luma(245), inset: 8pt, radius: 3pt, width: 100%, it)
#show heading: set block(above: 1.4em, below: 0.8em)
#show table: set text(size: 10pt)
#show link: set text(fill: rgb("#1a56db"))
"##;

/// Converts Markdown into Typst markup. Images found on disk are registered in `files`.
fn md_to_typst(md: &str, base: &Path, files: &mut HashMap<String, Bytes>) -> String {
    let opts = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES;
    let mut out = String::from(PREAMBLE);
    out.push('\n');

    // Stack of open lists: `true` = ordered.
    let mut lists: Vec<bool> = Vec::new();
    let mut in_code: Option<(String, String)> = None; // (lang, accumulated text)
    let mut in_table_head = false;
    let mut image_alt: Option<String> = None;

    for ev in Parser::new_ext(md, opts) {
        // Code block text is accumulated, not emitted inline.
        if let Some((_, buf)) = in_code.as_mut() {
            match &ev {
                Event::Text(t) => {
                    buf.push_str(t);
                    continue;
                }
                Event::End(TagEnd::CodeBlock) => {}
                _ => continue,
            }
        }
        // Alt text of an image is discarded for now (a figure caption could use it later).
        if let Some(alt) = image_alt.as_mut() {
            match &ev {
                Event::Text(t) => {
                    alt.push_str(t);
                    continue;
                }
                Event::End(TagEnd::Image) => {}
                _ => continue,
            }
        }
        match ev {
            Event::Start(tag) => match tag {
                Tag::Heading { level, .. } => {
                    let n = match level {
                        HeadingLevel::H1 => 1,
                        HeadingLevel::H2 => 2,
                        HeadingLevel::H3 => 3,
                        HeadingLevel::H4 => 4,
                        HeadingLevel::H5 => 5,
                        HeadingLevel::H6 => 6,
                    };
                    out.push_str(&"=".repeat(n));
                    out.push(' ');
                }
                Tag::BlockQuote(_) => out.push_str("#quote(block: true)[\n"),
                Tag::CodeBlock(kind) => {
                    let lang = match kind {
                        CodeBlockKind::Fenced(l) => {
                            l.split_whitespace().next().unwrap_or("").to_string()
                        }
                        CodeBlockKind::Indented => String::new(),
                    };
                    in_code = Some((lang, String::new()));
                }
                Tag::List(start) => {
                    if !lists.is_empty() && !out.ends_with('\n') {
                        out.push('\n');
                    }
                    lists.push(start.is_some());
                }
                Tag::Item => {
                    out.push_str(&"  ".repeat(lists.len().saturating_sub(1)));
                    out.push_str(if *lists.last().unwrap_or(&false) { "+ " } else { "- " });
                }
                Tag::Emphasis => out.push_str("#emph["),
                Tag::Strong => out.push_str("#strong["),
                Tag::Strikethrough => out.push_str("#strike["),
                Tag::Link { dest_url, .. } => {
                    out.push_str(&format!("#link({})[", quote(&dest_url)))
                }
                Tag::Image { dest_url, .. } => {
                    let path = base.join(dest_url.as_ref());
                    match std::fs::read(&path) {
                        Ok(bytes) => {
                            let name = path.file_name().unwrap().to_string_lossy().to_string();
                            let key = format!("assets/{name}");
                            files.insert(key.clone(), Bytes::new(bytes));
                            out.push_str(&format!(
                                "#align(center, image({}, width: 70%))",
                                quote(&format!("/{key}"))
                            ));
                        }
                        Err(e) => {
                            eprintln!("warning: image {} omitted: {e}", path.display());
                            out.push_str(&format!(
                                "#emph[{}]",
                                quote(&format!("[missing image: {dest_url}]"))
                            ));
                        }
                    }
                    image_alt = Some(String::new());
                }
                Tag::Table(aligns) => {
                    let cols = aligns.len();
                    let al = aligns
                        .iter()
                        .map(|a| match a {
                            Alignment::Right => "right",
                            Alignment::Center => "center",
                            _ => "left",
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    out.push_str(&format!(
                        "#align(center, table(columns: {cols}, inset: 6pt, align: ({al},), stroke: 0.5pt + luma(150),\n"
                    ));
                }
                Tag::TableHead => {
                    in_table_head = true;
                    out.push_str("table.header(");
                }
                Tag::TableCell => out.push('['),
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Paragraph => out.push_str(if lists.is_empty() { "\n\n" } else { "\n" }),
                TagEnd::Heading(_) => out.push_str("\n\n"),
                TagEnd::BlockQuote(_) => out.push_str("]\n\n"),
                TagEnd::CodeBlock => {
                    if let Some((lang, text)) = in_code.take() {
                        let text = text.trim_end_matches('\n');
                        let lang_arg = if lang.is_empty() {
                            String::new()
                        } else {
                            format!("lang: {}, ", quote(&lang))
                        };
                        out.push_str(&format!("#raw(block: true, {lang_arg}{})\n\n", quote(text)));
                    }
                }
                TagEnd::List(_) => {
                    lists.pop();
                    if lists.is_empty() {
                        out.push('\n');
                    }
                }
                TagEnd::Item => {
                    if !out.ends_with('\n') {
                        out.push('\n');
                    }
                }
                TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough | TagEnd::Link => {
                    out.push(']')
                }
                TagEnd::Image => {
                    image_alt = None;
                    out.push_str("\n\n");
                }
                TagEnd::Table => out.push_str("))\n\n"),
                TagEnd::TableHead => {
                    in_table_head = false;
                    out.push_str("),\n");
                }
                TagEnd::TableRow => out.push('\n'),
                TagEnd::TableCell => out.push_str(if in_table_head { "]," } else { "], " }),
                _ => {}
            },
            Event::Text(t) => out.push_str(&format!("#{}", quote(&t))),
            Event::Code(c) => out.push_str(&format!("#raw({})", quote(&c))),
            Event::SoftBreak => out.push(' '),
            Event::HardBreak => out.push_str("#linebreak()"),
            Event::Rule => out.push_str("#line(length: 100%, stroke: 0.5pt + luma(160))\n\n"),
            Event::TaskListMarker(done) => {
                out.push_str(if done { "#\"\u{2611} \"" } else { "#\"\u{2610} \"" })
            }
            Event::Html(h) | Event::InlineHtml(h) => {
                eprintln!("warning: raw HTML ignored: {}", h.trim());
            }
            _ => {}
        }
    }
    out
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: typst-min <input.md> <output.pdf> [--typ <output.typ>]");
        std::process::exit(2);
    }
    let input = PathBuf::from(&args[0]);
    let output = PathBuf::from(&args[1]);
    let typ_out = args.iter().position(|a| a == "--typ").and_then(|i| args.get(i + 1));

    let md = std::fs::read_to_string(&input).expect("cannot read input");
    let base = input.parent().unwrap_or(Path::new("."));
    let mut files = HashMap::new();

    let t0 = Instant::now();
    let markup = md_to_typst(&md, base, &mut files);
    if let Some(p) = typ_out {
        std::fs::write(p, &markup).expect("cannot write .typ");
    }
    let world = SpikeWorld::new(markup, files);
    let t1 = Instant::now();

    let warned = typst::compile::<PagedDocument>(&world);
    for w in &warned.warnings {
        eprintln!("typst warning: {}", w.message);
    }
    let doc = match warned.output {
        Ok(d) => d,
        Err(errs) => {
            for e in errs.iter() {
                eprintln!("typst error: {}", e.message);
            }
            std::process::exit(1);
        }
    };
    let t2 = Instant::now();
    let pdf = typst_pdf::pdf(&doc, &typst_pdf::PdfOptions::default()).unwrap_or_else(|e| {
        for x in e.iter() {
            eprintln!("pdf error: {}", x.message);
        }
        std::process::exit(1);
    });
    std::fs::write(&output, &pdf).expect("cannot write pdf");
    let t3 = Instant::now();
    println!(
        "{} pages, {} bytes | setup {:?}, compile {:?}, export {:?}",
        doc.pages().len(),
        pdf.len(),
        t1 - t0,
        t2 - t1,
        t3 - t2
    );
}
