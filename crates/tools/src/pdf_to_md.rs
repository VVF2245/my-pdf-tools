//! PDF -> Markdown tool.
//!
//! Two modes: slides (every page is a slide, see `outmark_engines::pdf_reader::slides`) and
//! notes (running text, see `outmark_engines::pdf_reader::notes`). The `mode` option picks one;
//! `auto` looks at the page orientation and reports what it chose.
//!
//! Limit: the glyphs of the whole page range are held in memory at once (about 60 bytes
//! per character, so a 500-page book takes a few tens of MB). The slide heuristics need
//! document-wide statistics (body size, repeated footers), so the conversion cannot
//! stream page by page without reading the PDF twice.

use std::io::Write;
use std::path::{Path, PathBuf};

use outmark_core::{
    CancelToken, ConversionError, ConversionOutcome, ConversionReport, ConversionRequest,
    ConversionWarning, OptionChoice, OptionKind, OptionSpec, Progress, Tool, ToolDescriptor,
};
use outmark_engines::markdown_writer::{MarkdownOutput, to_markdown};
use outmark_engines::pdf_reader::images::{
    ExtractedMedia, GRAPHIC_PAGE_SHAPES, RENDER_WIDTH_PX, add_media, extract_media, filter_images, render_pages,
};
use outmark_engines::pdf_reader::notes::{NoteOptions, notes_to_document};
use outmark_engines::pdf_reader::slides::{SlideOptions, slides_to_document};
use outmark_engines::pdf_reader::{PageLines, PdfSource, PdfiumLocation, extract_pages_with, group_lines};

use crate::config::ToolsConfig;
use crate::paths::{check_extension, claim_dir, claim_path, output_path};

pub struct PdfToMd {
    pdfium: PdfiumLocation,
}

impl PdfToMd {
    pub const ID: &'static str = "pdf_to_md";
    const EXTENSIONS: [&'static str; 1] = ["pdf"];

    pub fn new(config: &ToolsConfig) -> Self {
        Self { pdfium: config.pdfium.clone() }
    }
}

/// A page number option: 0 (the default) means "no limit".
fn page_option(request: &ConversionRequest, key: &str) -> u32 {
    request.number_option(key).unwrap_or(0.0).max(0.0).round() as u32
}

/// The folder that goes with `<stem>.md`: `<stem>_images`.
fn images_dir_name(markdown: &Path) -> String {
    format!("{}_images", markdown.file_stem().unwrap_or_default().to_string_lossy())
}

/// Writes the images and then the Markdown text into the already claimed `file`.
fn write_markdown(file: &mut std::fs::File, folder: &Path, markdown: &MarkdownOutput) -> std::io::Result<()> {
    for image in &markdown.images {
        let path = folder.join(&image.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, &image.data)?;
    }
    file.write_all(markdown.text.as_bytes())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Auto,
    Notes,
    Slides,
}

impl Mode {
    fn from_option(request: &ConversionRequest) -> Self {
        match request.text_option("mode") {
            Some("notes") => Self::Notes,
            Some("slides") => Self::Slides,
            _ => Self::Auto,
        }
    }
}

/// What to do with the images of the PDF.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImagesMode {
    /// Embedded images, and a picture of each slide that holds a diagram (slides mode).
    Auto,
    Embedded,
    None,
}

impl ImagesMode {
    fn from_option(request: &ConversionRequest) -> Self {
        match request.text_option("images") {
            Some("embedded") => Self::Embedded,
            Some("none") => Self::None,
            _ => Self::Auto,
        }
    }
}

fn warning(code: &str, detail: String, page: Option<u32>) -> ConversionWarning {
    ConversionWarning { code: code.to_string(), detail, page }
}

/// Most pages portrait: running text. Otherwise (landscape, or an even split): slides.
fn looks_like_notes(pages: &[PageLines]) -> bool {
    let portrait = pages.iter().filter(|p| p.height > p.width).count();
    portrait * 2 > pages.len()
}

/// Name and description are translation keys, like every label of a descriptor.
impl Tool for PdfToMd {
    fn descriptor(&self) -> ToolDescriptor {
        let page = |key: &str, label: &str| OptionSpec {
            key: key.to_string(),
            label: label.to_string(),
            help: Some(format!("{label}.help")),
            kind: OptionKind::Number { min: 0.0, max: 100_000.0, default: 0.0, step: 1.0 },
        };
        ToolDescriptor {
            id: Self::ID.to_string(),
            name: "tool.pdfToMd.name".to_string(),
            description: "tool.pdfToMd.description".to_string(),
            inputs: Self::EXTENSIONS.iter().map(|e| e.to_string()).collect(),
            outputs: vec!["md".to_string()],
            options: vec![
                OptionSpec {
                    key: "mode".to_string(),
                    label: "option.mode".to_string(),
                    help: Some("option.mode.help".to_string()),
                    kind: OptionKind::Choice {
                        choices: ["auto", "notes", "slides"]
                            .iter()
                            .map(|m| OptionChoice { value: m.to_string(), label: format!("mode.{m}") })
                            .collect(),
                        default: "auto".to_string(),
                    },
                },
                OptionSpec {
                    key: "images".to_string(),
                    label: "option.images".to_string(),
                    help: Some("option.images.help".to_string()),
                    kind: OptionKind::Choice {
                        choices: ["auto", "embedded", "none"]
                            .iter()
                            .map(|m| OptionChoice { value: m.to_string(), label: format!("images.{m}") })
                            .collect(),
                        default: "auto".to_string(),
                    },
                },
                page("firstPage", "option.firstPage"),
                page("lastPage", "option.lastPage"),
                OptionSpec {
                    key: "removeHeadersFooters".to_string(),
                    label: "option.removeHeadersFooters".to_string(),
                    help: Some("option.removeHeadersFooters.help".to_string()),
                    kind: OptionKind::Bool { default: true },
                },
            ],
        }
    }

    fn run(
        &self,
        request: &ConversionRequest,
        progress: &dyn Progress,
        cancel: &CancelToken,
    ) -> Result<ConversionOutcome, ConversionError> {
        let first = page_option(request, "firstPage");
        let last = page_option(request, "lastPage");
        let remove_furniture = request.bool_option("removeHeadersFooters").unwrap_or(true);
        let mode = Mode::from_option(request);
        let images_mode = ImagesMode::from_option(request);
        let total = request.inputs.len().max(1) as f32;
        let mut outcome = ConversionOutcome::default();
        let mut report = ConversionReport::default();

        for (i, input) in request.inputs.iter().enumerate() {
            let done = |fraction: f32| progress.report((i as f32 + fraction) / total);
            cancel.check()?;
            let input = Path::new(input);
            check_extension(input, &Self::EXTENSIONS)?;

            // Reading is half of the work; it reports and can be cancelled between pages.
            let pages = extract_pages_with(&self.pdfium, PdfSource::Path(input), first, last, &|read, of| {
                done(0.5 * read as f32 / of.max(1) as f32);
                !cancel.is_cancelled()
            })?;
            done(0.5);
            cancel.check()?;

            let pages: Vec<PageLines> = pages.into_iter().map(group_lines).collect();

            let mut media = ExtractedMedia::default();
            if images_mode != ImagesMode::None {
                media = extract_media(&self.pdfium, PdfSource::Path(input), first, last, &|_, _| {
                    !cancel.is_cancelled()
                })?;
                let dropped = filter_images(&mut media.pages);
                if dropped > 0 {
                    report.warnings.push(warning("imagesSkipped", dropped.to_string(), None));
                }
                if media.failed > 0 {
                    report.warnings.push(warning("imagesFailed", media.failed.to_string(), None));
                }
            }
            let has_images = media.pages.iter().any(|p| !p.images.is_empty());
            if !has_images && pages.iter().all(|page| page.lines.is_empty()) {
                return Err(ConversionError::NoExtractableText);
            }
            let notes = match mode {
                Mode::Notes => true,
                Mode::Slides => false,
                Mode::Auto => {
                    let notes = looks_like_notes(&pages);
                    report.warnings.push(ConversionWarning {
                        code: if notes { "modeChosenNotes" } else { "modeChosenSlides" }.to_string(),
                        detail: String::new(),
                        page: None,
                    });
                    notes
                }
            };
            let (mut document, mut warnings) = if notes {
                notes_to_document(&pages, NoteOptions { remove_furniture })
            } else {
                slides_to_document(&pages, SlideOptions { remove_furniture })
            };

            // A slide with a diagram is drawn with shapes, not images: it gets a picture.
            let mut renders = Vec::new();
            if !notes && images_mode == ImagesMode::Auto {
                let graphic: Vec<u32> =
                    media.pages.iter().filter(|p| p.graphics >= GRAPHIC_PAGE_SHAPES).map(|p| p.number).collect();
                if !graphic.is_empty() {
                    renders = render_pages(&self.pdfium, PdfSource::Path(input), &graphic, RENDER_WIDTH_PX)?;
                    for (number, _) in &renders {
                        report.warnings.push(warning("pageRendered", String::new(), Some(*number)));
                    }
                }
            }
            // a page with pictures is not empty
            warnings.retain(|w| {
                w.code != "emptySlide"
                    || !w.page.is_some_and(|n| {
                        renders.iter().any(|(r, _)| *r == n)
                            || media.pages.iter().any(|p| p.number == n && !p.images.is_empty())
                    })
            });
            add_media(&mut document, &media.pages, &renders);
            report.warnings.extend(warnings);
            done(0.8);
            cancel.check()?;

            // Claim `<name>.md` only if `<name>_images` is free too (a leftover folder from
            // an earlier run must not have its images overwritten).
            let mut wanted = output_path(input, request.output_dir.as_deref(), "md");
            // A conversion with images gets a folder of its own (the `.md` and its images
            // folder inside), so Downloads is not filled with loose pictures.
            let mut own_folder = None;
            if !document.assets.is_empty() {
                let parent = wanted.parent().map(Path::to_path_buf).unwrap_or_default();
                let stem = input.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                let folder = claim_dir(&parent, &stem)?;
                wanted = folder.join(wanted.file_name().unwrap_or_default());
                own_folder = Some(folder);
            }
            let folder_free = |md: &Path| {
                let folder = md.parent().map(Path::to_path_buf).unwrap_or_else(PathBuf::new);
                !folder.join(images_dir_name(md)).exists()
            };
            let (target, mut file) = match claim_path(&wanted, &folder_free) {
                Ok(claimed) => claimed,
                Err(e) => {
                    if let Some(folder) = &own_folder {
                        let _ = std::fs::remove_dir(folder);
                    }
                    return Err(e.into());
                }
            };
            let images_dir = images_dir_name(&target);
            let markdown = to_markdown(&document, &images_dir);

            let folder = target.parent().map(Path::to_path_buf).unwrap_or_default();
            if let Err(e) = write_markdown(&mut file, &folder, &markdown) {
                // do not leave half of the output behind
                let _ = std::fs::remove_file(&target);
                if !markdown.images.is_empty() {
                    let _ = std::fs::remove_dir_all(folder.join(&images_dir));
                }
                if let Some(own) = &own_folder {
                    let _ = std::fs::remove_dir(own); // only if empty
                }
                return Err(e.into());
            }
            outcome.outputs.push(target.to_string_lossy().into_owned());
            done(1.0);
        }

        outcome.report = report;
        Ok(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{TempDir, pdfium};
    use outmark_core::{NoProgress, OptionValue};
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/pdf").join(name)
    }

    fn tool() -> Option<PdfToMd> {
        Some(PdfToMd { pdfium: pdfium()? })
    }

    fn request(input: &Path, out_dir: &Path, options: &[(&str, OptionValue)]) -> ConversionRequest {
        ConversionRequest {
            tool_id: PdfToMd::ID.into(),
            inputs: vec![input.to_string_lossy().into_owned()],
            output_dir: Some(out_dir.to_string_lossy().into_owned()),
            options: options.iter().map(|(k, v)| (k.to_string(), v.clone())).collect::<BTreeMap<_, _>>(),
        }
    }

    fn run(tool: &PdfToMd, req: &ConversionRequest) -> Result<ConversionOutcome, ConversionError> {
        tool.run(req, &NoProgress, &CancelToken::new())
    }

    #[test]
    fn the_descriptor_offers_pdf_in_markdown_out_and_page_options() {
        let descriptor = PdfToMd { pdfium: PdfiumLocation::new(vec![]) }.descriptor();
        assert_eq!(descriptor.id, "pdf_to_md");
        assert_eq!(descriptor.inputs, ["pdf"]);
        assert_eq!(descriptor.outputs, ["md"]);
        let keys: Vec<&str> = descriptor.options.iter().map(|o| o.key.as_str()).collect();
        assert_eq!(keys, ["mode", "images", "firstPage", "lastPage", "removeHeadersFooters"]);
        assert!(matches!(descriptor.options[0].kind, OptionKind::Choice { .. }));
        assert!(matches!(descriptor.options[2].kind, OptionKind::Number { step: 1.0, .. }));
    }

    #[test]
    fn the_slides_fixture_converts_to_the_golden_markdown() {
        let Some(tool) = tool() else { return };
        let out = TempDir::new("pdf-md-golden");

        let outcome = run(&tool, &request(&fixture("slides.pdf"), out.path(), &[])).unwrap();

        assert_eq!(outcome.outputs.len(), 1);
        assert!(outcome.outputs[0].ends_with("slides.md"));
        let written = std::fs::read_to_string(&outcome.outputs[0]).unwrap();
        let golden = std::fs::read_to_string(fixture("slides.expected.md")).unwrap();
        assert_eq!(written, golden.replace("\r\n", "\n"));
        // auto mode says what it chose
        assert_eq!(outcome.report.warnings.len(), 1);
        assert_eq!(outcome.report.warnings[0].code, "modeChosenSlides");
    }

    #[test]
    fn a_page_range_converts_only_those_pages() {
        let Some(tool) = tool() else { return };
        let out = TempDir::new("pdf-md-range");
        let req = request(
            &fixture("slides.pdf"),
            out.path(),
            &[("firstPage", OptionValue::Number(2.0)), ("lastPage", OptionValue::Number(2.0))],
        );
        let outcome = run(&tool, &req).unwrap();
        let text = std::fs::read_to_string(&outcome.outputs[0]).unwrap();
        assert!(text.starts_with("## ¿Qué es una base de datos?"), "{text}");
        assert_eq!(text.matches("## ").count(), 1);
    }

    #[test]
    fn an_invalid_page_range_is_an_error() {
        let Some(tool) = tool() else { return };
        let out = TempDir::new("pdf-md-badrange");
        let req = request(&fixture("slides.pdf"), out.path(), &[("firstPage", OptionValue::Number(50.0))]);
        assert!(matches!(run(&tool, &req), Err(ConversionError::InvalidPageRange { first: 50, .. })));
    }

    #[test]
    fn headers_and_footers_stay_when_asked() {
        let Some(tool) = tool() else { return };
        let out = TempDir::new("pdf-md-furniture");
        let req = request(&fixture("slides.pdf"), out.path(), &[("removeHeadersFooters", OptionValue::Bool(false))]);
        let outcome = run(&tool, &req).unwrap();
        let text = std::fs::read_to_string(&outcome.outputs[0]).unwrap();
        assert!(text.contains("Bases de datos · Curso 2026"), "{text}");
    }

    #[test]
    fn it_never_overwrites_an_existing_markdown() {
        let Some(tool) = tool() else { return };
        let out = TempDir::new("pdf-md-unique");
        let req = request(&fixture("table.pdf"), out.path(), &[]);
        let first = run(&tool, &req).unwrap();
        let second = run(&tool, &req).unwrap();
        assert!(first.outputs[0].ends_with("table.md"));
        assert!(second.outputs[0].ends_with("table (1).md"));
    }

    #[test]
    fn a_pdf_without_text_has_no_extractable_text() {
        let Some(tool) = tool() else { return };
        let out = TempDir::new("pdf-md-empty");
        // a valid PDF with one blank page and no text at all (like a scan without OCR)
        let pdf = outmark_engines::pdf::compile_typst("#set page(paper: \"a4\")\n#v(1cm)\n", Default::default()).unwrap();
        let input = out.path().join("blank.pdf");
        std::fs::write(&input, pdf).unwrap();
        let req = request(&input, out.path(), &[]);
        assert_eq!(run(&tool, &req), Err(ConversionError::NoExtractableText));
    }

    #[test]
    fn bad_inputs_have_typed_errors() {
        let Some(tool) = tool() else { return };
        let out = TempDir::new("pdf-md-errors");

        let wrong = out.path().join("a.docx");
        std::fs::write(&wrong, "x").unwrap();
        assert_eq!(
            run(&tool, &request(&wrong, out.path(), &[])),
            Err(ConversionError::UnsupportedInput { extension: "docx".into() })
        );

        let garbage = out.path().join("garbage.pdf");
        std::fs::write(&garbage, "this is not a pdf").unwrap();
        assert_eq!(run(&tool, &request(&garbage, out.path(), &[])), Err(ConversionError::Corrupt));

        let missing = out.path().join("missing.pdf");
        assert!(matches!(run(&tool, &request(&missing, out.path(), &[])), Err(ConversionError::Io { .. })));
    }

    #[test]
    fn cancelling_before_the_start_does_nothing() {
        // needs no PDFium: the check comes first
        let tool = PdfToMd { pdfium: PdfiumLocation::new(vec![]) };
        let out = TempDir::new("pdf-md-cancel");
        let cancel = CancelToken::new();
        cancel.cancel();
        let req = request(&fixture("slides.pdf"), out.path(), &[]);
        assert_eq!(tool.run(&req, &NoProgress, &cancel), Err(ConversionError::Cancelled));
        assert!(!out.path().join("slides.md").exists());
    }

    /// Cancels the job as soon as the tool reports its first progress.
    struct CancelOnProgress(CancelToken);

    impl Progress for CancelOnProgress {
        fn report(&self, _fraction: f32) {
            self.0.cancel();
        }
    }

    #[test]
    fn cancelling_while_pages_are_being_read_stops_before_the_end_and_leaves_no_file() {
        let Some(tool) = tool() else { return };
        let out = TempDir::new("pdf-md-cancel-mid");
        let cancel = CancelToken::new();
        let req = request(&fixture("notes.pdf"), out.path(), &[]);

        let result = tool.run(&req, &CancelOnProgress(cancel.clone()), &cancel);

        assert_eq!(result, Err(ConversionError::Cancelled));
        assert!(!out.path().join("notes.md").exists());
    }

    fn mode_chosen(outcome: &ConversionOutcome) -> Vec<&str> {
        let codes = outcome.report.warnings.iter().map(|w| w.code.as_str());
        codes.filter(|c| c.starts_with("modeChosen")).collect()
    }

    #[test]
    fn auto_picks_notes_for_portrait_pages_and_slides_for_landscape_ones() {
        let Some(tool) = tool() else { return };
        let out = TempDir::new("pdf-md-auto");

        let notes = run(&tool, &request(&fixture("notes.pdf"), out.path(), &[])).unwrap();
        assert_eq!(mode_chosen(&notes), ["modeChosenNotes"]);

        let slides = run(&tool, &request(&fixture("slides.pdf"), out.path(), &[])).unwrap();
        assert_eq!(mode_chosen(&slides), ["modeChosenSlides"]);
    }

    #[test]
    fn a_forced_mode_is_used_and_not_reported() {
        let Some(tool) = tool() else { return };
        let out = TempDir::new("pdf-md-forced");
        let forced = |mode: &str| [("mode", OptionValue::Text(mode.to_string()))];

        // portrait pages forced to slides: one section per page, no heading levels from sizes
        let as_slides = run(&tool, &request(&fixture("notes.pdf"), out.path(), &forced("slides"))).unwrap();
        assert!(mode_chosen(&as_slides).is_empty());
        let text = std::fs::read_to_string(&as_slides.outputs[0]).unwrap();
        assert!(!text.contains("
# "), "{text}");

        let as_notes = run(&tool, &request(&fixture("notes.pdf"), out.path(), &forced("notes"))).unwrap();
        assert!(mode_chosen(&as_notes).is_empty());
        let text = std::fs::read_to_string(&as_notes.outputs[0]).unwrap();
        assert!(text.contains("# "), "{text}");
    }

    fn codes(outcome: &ConversionOutcome) -> Vec<&str> {
        outcome.report.warnings.iter().map(|w| w.code.as_str()).collect()
    }

    #[test]
    fn a_pdf_with_a_photo_gets_its_own_folder_with_the_image_inside() {
        let Some(tool) = tool() else { return };
        let out = TempDir::new("pdf-md-photo");
        let outcome = run(&tool, &request(&fixture("images.pdf"), out.path(), &[])).unwrap();

        let md = PathBuf::from(&outcome.outputs[0]);
        assert_eq!(md.parent().unwrap(), out.path().join("images"));
        let text = std::fs::read_to_string(&md).unwrap();
        assert!(text.contains("![p1-1](images_images/img-1.png)"), "{text}");
        assert!(out.path().join("images/images_images/img-1.png").is_file());
        // the repeated logo was dropped, and the report says so
        assert!(codes(&outcome).contains(&"imagesSkipped"), "{:?}", codes(&outcome));
        // a second conversion does not mix with the first
        let again = run(&tool, &request(&fixture("images.pdf"), out.path(), &[])).unwrap();
        assert_eq!(PathBuf::from(&again.outputs[0]).parent().unwrap(), out.path().join("images (1)"));
    }

    #[test]
    fn without_images_the_result_stays_loose() {
        let Some(tool) = tool() else { return };
        let out = TempDir::new("pdf-md-loose");
        let outcome = run(&tool, &request(&fixture("images.pdf"), out.path(), &[("images", OptionValue::Text("none".into()))])).unwrap();
        assert_eq!(PathBuf::from(&outcome.outputs[0]), out.path().join("images.md"));
        assert!(!out.path().join("images_images").exists());
    }

    #[test]
    fn a_slide_with_a_diagram_gets_a_picture_only_in_auto_mode() {
        let Some(tool) = tool() else { return };
        let out = TempDir::new("pdf-md-diagram");

        let auto = run(&tool, &request(&fixture("diagram.pdf"), out.path(), &[])).unwrap();
        let warning = auto.report.warnings.iter().find(|w| w.code == "pageRendered").expect("pageRendered");
        assert_eq!(warning.page, Some(1));
        let md = PathBuf::from(&auto.outputs[0]);
        assert!(std::fs::read_to_string(&md).unwrap().contains("![p1](diagram_images/img-1.png)"));
        assert!(md.parent().unwrap().join("diagram_images/img-1.png").is_file());

        let embedded =
            run(&tool, &request(&fixture("diagram.pdf"), out.path(), &[("images", OptionValue::Text("embedded".into()))])).unwrap();
        assert!(!codes(&embedded).contains(&"pageRendered"));
        assert!(PathBuf::from(&embedded.outputs[0]).ends_with("diagram.md"));
    }

    #[test]
    fn a_leftover_images_folder_makes_the_tool_pick_another_name() {
        let Some(tool) = tool() else { return };
        let out = TempDir::new("pdf-md-leftover");
        // `table.md` does not exist, but a folder from an earlier run does
        std::fs::create_dir_all(out.path().join("table_images")).unwrap();
        std::fs::write(out.path().join("table_images").join("img-1.png"), "old").unwrap();

        let outcome = run(&tool, &request(&fixture("table.pdf"), out.path(), &[])).unwrap();

        assert!(outcome.outputs[0].ends_with("table (1).md"), "{:?}", outcome.outputs);
        assert_eq!(std::fs::read(out.path().join("table_images").join("img-1.png")).unwrap(), b"old");
    }

    #[test]
    fn a_failed_write_leaves_no_empty_file_behind() {
        let Some(tool) = tool() else { return };
        let out = TempDir::new("pdf-md-nowrite");
        // an output folder that does not exist: nothing can be claimed there
        let missing = out.path().join("no-such-folder");
        let req = request(&fixture("table.pdf"), &missing, &[]);
        assert!(matches!(run(&tool, &req), Err(ConversionError::Io { .. })));
        assert!(!missing.exists());
    }

    #[test]
    fn page_options_are_rounded_and_never_negative() {
        let req = request(Path::new("a.pdf"), Path::new("."), &[("firstPage", OptionValue::Number(-3.0)), ("lastPage", OptionValue::Number(2.6))]);
        assert_eq!(page_option(&req, "firstPage"), 0);
        assert_eq!(page_option(&req, "lastPage"), 3);
        assert_eq!(page_option(&req, "missing"), 0);
    }
}
