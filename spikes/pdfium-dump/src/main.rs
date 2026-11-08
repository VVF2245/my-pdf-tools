//! Throwaway spike S0-B: dump characters and lines (with font size) of a PDF using PDFium.
//!
//! Usage: pdfium-dump <file.pdf> [--pages A-B|all] [--chars N]
//!   --pages  page range, 1-based (default 1-3)
//!   --chars  also print the first N raw characters of each selected page

use std::collections::BTreeMap;
use std::error::Error;
use std::path::PathBuf;

use pdfium_render::prelude::*;

/// Gap between glyphs (as a fraction of font size) above which we infer a space.
const SPACE_GAP_RATIO: f32 = 0.2;

/// A character with the data the future PDF->MD pipeline cares about.
struct Glyph {
    ch: char,
    left: f32,
    right: f32,
    bottom: f32,
    top: f32,
    size: f32,
    font: String,
    mono: bool,
}

/// A run of glyphs on the same baseline.
struct Line {
    glyphs: Vec<Glyph>,
}

impl Line {
    fn baseline(&self) -> f32 {
        self.glyphs.last().map_or(0.0, |g| g.bottom)
    }

    /// Line text. Some PDFs (e.g. LaTeX output) contain no space characters at all,
    /// only gaps between glyphs, so we insert a space when the gap is wide enough.
    fn text(&self) -> String {
        let mut out = String::new();
        let mut prev: Option<&Glyph> = None;
        for g in &self.glyphs {
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

    /// Most frequent value among the non-space glyphs (ties: first seen).
    fn dominant<T: Ord + Clone>(&self, f: impl Fn(&Glyph) -> T) -> Option<T> {
        let mut counts: BTreeMap<T, usize> = BTreeMap::new();
        for g in self.glyphs.iter().filter(|g| !g.ch.is_whitespace()) {
            *counts.entry(f(g)).or_default() += 1;
        }
        counts.into_iter().max_by_key(|(_, n)| *n).map(|(v, _)| v)
    }
}

fn load_pdfium() -> Result<Pdfium, Box<dyn Error>> {
    // Candidates, in order: $PDFIUM_DIR, repo-relative download dir, current dir, system.
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(d) = std::env::var("PDFIUM_DIR") {
        dirs.push(d.into());
    }
    dirs.push("pdfium/win-x64/bin".into());
    dirs.push("../../pdfium/win-x64/bin".into());
    dirs.push("./".into());

    for dir in &dirs {
        let path = Pdfium::pdfium_platform_library_name_at_path(dir);
        if let Ok(bindings) = Pdfium::bind_to_library(&path) {
            eprintln!("# PDFium loaded from {}", path.display());
            return Ok(Pdfium::new(bindings));
        }
    }
    let bindings = Pdfium::bind_to_system_library().map_err(|e| {
        format!(
            "could not load PDFium from {:?} nor from the system ({e}). \
             Run scripts/get-pdfium.ps1 or set PDFIUM_DIR",
            dirs
        )
    })?;
    eprintln!("# PDFium loaded from system library");
    Ok(Pdfium::new(bindings))
}

fn parse_pages(spec: &str, total: usize) -> (usize, usize) {
    if spec == "all" {
        return (1, total);
    }
    let (a, b) = spec.split_once('-').unwrap_or((spec, spec));
    let a = a.parse().unwrap_or(1usize).max(1);
    let b = b.parse().unwrap_or(a).min(total);
    (a, b)
}

/// Group characters (in PDF content-stream order) into lines.
/// A new line starts when the baseline moves more than 30% of the font size,
/// or when x jumps backwards by more than one font size (e.g. next column).
fn group_lines(glyphs: Vec<Glyph>) -> Vec<Line> {
    let mut lines: Vec<Line> = Vec::new();
    for g in glyphs {
        let starts_new = match lines.last().and_then(|l| l.glyphs.last().map(|p| (l, p))) {
            None => true,
            Some((l, prev)) => {
                let tol = 0.3 * g.size.max(prev.size);
                (g.bottom - l.baseline()).abs() > tol || g.left < prev.right - g.size
            }
        };
        if starts_new {
            lines.push(Line { glyphs: vec![g] });
        } else if let Some(l) = lines.last_mut() {
            l.glyphs.push(g);
        }
    }
    lines.retain(|l| !l.text().is_empty());
    lines
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(pdf_path) = args.first() else {
        return Err("usage: pdfium-dump <file.pdf> [--pages A-B|all] [--chars N]".into());
    };
    let flag = |name: &str| {
        args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
    };
    let pages_spec = flag("--pages").unwrap_or_else(|| "1-3".into());
    let raw_chars: usize = flag("--chars").and_then(|s| s.parse().ok()).unwrap_or(0);

    let pdfium = load_pdfium()?;
    let doc = pdfium.load_pdf_from_file(pdf_path, None)?;
    let total = doc.pages().len() as usize;
    let (first, last) = parse_pages(&pages_spec, total);
    println!("# {pdf_path}: {total} pages, dumping {first}-{last}");

    // font size (rounded to 0.5 pt, stored x10 to be Ord) -> number of non-space chars
    let mut histogram: BTreeMap<i32, usize> = BTreeMap::new();

    for (idx, page) in doc.pages().iter().enumerate() {
        let page_no = idx + 1;
        if page_no < first || page_no > last {
            continue;
        }
        let text = page.text()?;
        let mut glyphs: Vec<Glyph> = Vec::new();
        for c in text.chars().iter() {
            // Skip generated chars (synthetic \r\n) and chars without unicode mapping.
            if c.is_generated().unwrap_or(false) {
                continue;
            }
            let Some(ch) = c.unicode_char() else { continue };
            if ch.is_control() {
                continue;
            }
            let Ok(b) = c.loose_bounds() else { continue };
            glyphs.push(Glyph {
                ch,
                left: b.left().value,
                right: b.right().value,
                bottom: b.bottom().value,
                top: b.top().value,
                // Effective size: some PDFs set size 1 and scale via the text matrix.
                size: c.scaled_font_size().value,
                font: c.font_name(),
                mono: c.font_is_fixed_pitch(),
            });
        }

        if raw_chars > 0 {
            println!("--- page {page_no}: raw chars (first {raw_chars}) ---");
            for g in glyphs.iter().take(raw_chars) {
                println!(
                    "{:?} box=({:.1},{:.1})-({:.1},{:.1}) size={:.1} font={}{}",
                    g.ch, g.left, g.bottom, g.right, g.top, g.size, g.font,
                    if g.mono { " mono" } else { "" }
                );
            }
        }

        for g in glyphs.iter().filter(|g| !g.ch.is_whitespace()) {
            *histogram.entry((g.size * 2.0).round() as i32 * 5).or_default() += 1;
        }

        println!("--- page {page_no} ---");
        for line in group_lines(glyphs) {
            let size = line.dominant(|g| (g.size * 10.0).round() as i32).unwrap_or(0) as f32 / 10.0;
            let font = line.dominant(|g| g.font.clone()).unwrap_or_default();
            let mono = if line.dominant(|g| g.mono) == Some(true) { " mono" } else { "" };
            let x = line.glyphs.first().map_or(0.0, |g| g.left);
            println!(
                "p{page_no} y={:.1} x={:.1} size={:.1} {font}{mono} | {}",
                line.baseline(), x, size, line.text()
            );
        }
    }

    // Body text = most frequent size; anything clearly bigger is a heading candidate.
    let body = histogram.iter().max_by_key(|(_, n)| **n).map(|(s, _)| *s);
    println!("--- font size histogram (non-space chars) ---");
    for (s, n) in histogram.iter().rev() {
        let tag = match body {
            Some(b) if *s == b => "  <- BODY",
            Some(b) if *s > b => "  (bigger than body)",
            _ => "",
        };
        println!("{:5.1} pt: {n}{tag}", *s as f32 / 10.0);
    }
    Ok(())
}
