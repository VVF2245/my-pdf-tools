//! Markdown -> PDF tool.

use std::io::Write;
use std::path::Path;

use mpt_core::{
    CancelToken, ConversionError, ConversionOutcome, ConversionReport, ConversionRequest,
    OptionChoice, OptionKind, OptionSpec, Progress, Tool, ToolDescriptor,
};
use mpt_engines::markdown::parse_markdown;
use mpt_engines::pdf::{Paper, PdfOptions, render_pdf};

use crate::paths::{check_extension, claim_path, output_path};

pub struct MdToPdf;

impl MdToPdf {
    pub const ID: &'static str = "md_to_pdf";
    const EXTENSIONS: [&'static str; 2] = ["md", "markdown"];
}

/// Name and description are translation keys, like every label of a descriptor.
impl Tool for MdToPdf {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            id: Self::ID.to_string(),
            name: "tool.mdToPdf.name".to_string(),
            description: "tool.mdToPdf.description".to_string(),
            inputs: Self::EXTENSIONS.iter().map(|e| e.to_string()).collect(),
            outputs: vec!["pdf".to_string()],
            options: vec![
                OptionSpec {
                    key: "paper".to_string(),
                    label: "option.paper".to_string(),
                    help: Some("option.paper.help".to_string()),
                    kind: OptionKind::Choice {
                        choices: vec![
                            OptionChoice { value: "a4".into(), label: "paper.a4".into() },
                            OptionChoice { value: "letter".into(), label: "paper.letter".into() },
                        ],
                        default: "a4".to_string(),
                    },
                },
                OptionSpec {
                    key: "marginCm".to_string(),
                    label: "option.margin".to_string(),
                    help: Some("option.margin.help".to_string()),
                    kind: OptionKind::Number { min: 1.0, max: 5.0, default: 2.2, step: 0.1 },
                },
                OptionSpec {
                    key: "fontSizePt".to_string(),
                    label: "option.fontSize".to_string(),
                    help: Some("option.fontSize.help".to_string()),
                    kind: OptionKind::Number { min: 8.0, max: 16.0, default: 11.0, step: 0.5 },
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
        let options = pdf_options(request);
        let total = request.inputs.len().max(1) as f32;
        let mut outcome = ConversionOutcome::default();
        let mut report = ConversionReport::default();

        for (i, input) in request.inputs.iter().enumerate() {
            cancel.check()?;
            let input = Path::new(input);
            check_extension(input, &Self::EXTENSIONS)?;

            let bytes = std::fs::read(input)?;
            let md = String::from_utf8(bytes).map_err(|_| ConversionError::Corrupt)?;
            cancel.check()?;

            let parsed = parse_markdown(&md, input.parent());
            report.warnings.extend(parsed.warnings);
            progress.report((i as f32 + 0.4) / total);
            cancel.check()?;

            let pdf = render_pdf(&parsed.document, &options)?;
            cancel.check()?;

            let (target, mut file) = claim_path(&output_path(input, request.output_dir.as_deref(), "pdf"), &|_| true)?;
            if let Err(e) = file.write_all(&pdf) {
                let _ = std::fs::remove_file(&target);
                return Err(e.into());
            }
            outcome.outputs.push(target.to_string_lossy().into_owned());
            progress.report((i as f32 + 1.0) / total);
        }

        outcome.report = report;
        Ok(outcome)
    }
}

fn pdf_options(request: &ConversionRequest) -> PdfOptions {
    let defaults = PdfOptions::default();
    PdfOptions {
        paper: match request.text_option("paper") {
            Some("letter") => Paper::Letter,
            _ => Paper::A4,
        },
        margin_cm: request.number_option("marginCm").unwrap_or(defaults.margin_cm).clamp(1.0, 5.0),
        font_size_pt: request
            .number_option("fontSizePt")
            .unwrap_or(defaults.font_size_pt)
            .clamp(8.0, 16.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::TempDir;
    use mpt_core::{NoProgress, OptionValue};
    use std::collections::BTreeMap;

    fn request(inputs: Vec<String>, output_dir: Option<String>) -> ConversionRequest {
        ConversionRequest {
            tool_id: MdToPdf::ID.into(),
            inputs,
            output_dir,
            options: BTreeMap::new(),
        }
    }

    fn run(req: &ConversionRequest) -> Result<ConversionOutcome, ConversionError> {
        MdToPdf.run(req, &NoProgress, &CancelToken::new())
    }

    #[test]
    fn converts_markdown_with_accents_in_the_file_name() {
        let dir = TempDir::new("accents");
        let input = dir.path().join("apuntes ñandú.md");
        std::fs::write(&input, "# Título\n\nTexto.\n").unwrap();

        let outcome = run(&request(vec![input.to_string_lossy().into_owned()], None)).unwrap();

        assert_eq!(outcome.outputs.len(), 1);
        let out = Path::new(&outcome.outputs[0]);
        assert_eq!(out.file_name().unwrap().to_string_lossy(), "apuntes ñandú.pdf");
        assert!(std::fs::read(out).unwrap().starts_with(b"%PDF-"));
    }

    #[test]
    fn writes_into_output_dir_and_never_overwrites() {
        let dir = TempDir::new("outdir");
        let input = dir.path().join("a.md");
        std::fs::write(&input, "hola\n").unwrap();
        let out_dir = dir.path().join("out");
        std::fs::create_dir_all(&out_dir).unwrap();
        let req = request(
            vec![input.to_string_lossy().into_owned()],
            Some(out_dir.to_string_lossy().into_owned()),
        );

        let first = run(&req).unwrap();
        let second = run(&req).unwrap();

        assert!(first.outputs[0].ends_with("a.pdf"));
        assert!(second.outputs[0].ends_with("a (1).pdf"));
    }

    #[test]
    fn rejects_unsupported_extension() {
        let dir = TempDir::new("ext");
        let input = dir.path().join("a.docx");
        std::fs::write(&input, "x").unwrap();
        let err = run(&request(vec![input.to_string_lossy().into_owned()], None)).unwrap_err();
        assert_eq!(err, ConversionError::UnsupportedInput { extension: "docx".into() });
    }

    #[test]
    fn missing_file_is_an_io_error() {
        let err = run(&request(vec!["no-such-file.md".into()], None)).unwrap_err();
        assert!(matches!(err, ConversionError::Io { .. }));
    }

    #[test]
    fn invalid_utf8_is_corrupt() {
        let dir = TempDir::new("utf8");
        let input = dir.path().join("a.md");
        std::fs::write(&input, [0xff, 0xfe, 0xfd]).unwrap();
        let err = run(&request(vec![input.to_string_lossy().into_owned()], None)).unwrap_err();
        assert_eq!(err, ConversionError::Corrupt);
    }

    #[test]
    fn cancelled_before_start_does_nothing() {
        let dir = TempDir::new("cancel");
        let input = dir.path().join("a.md");
        std::fs::write(&input, "hola\n").unwrap();
        let cancel = CancelToken::new();
        cancel.cancel();
        let req = request(vec![input.to_string_lossy().into_owned()], None);
        assert_eq!(MdToPdf.run(&req, &NoProgress, &cancel), Err(ConversionError::Cancelled));
        assert!(!dir.path().join("a.pdf").exists());
    }

    #[test]
    fn warnings_reach_the_report() {
        let dir = TempDir::new("warn");
        let input = dir.path().join("a.md");
        std::fs::write(&input, "![x](missing.png)\n").unwrap();
        let outcome = run(&request(vec![input.to_string_lossy().into_owned()], None)).unwrap();
        assert_eq!(outcome.report.warnings.len(), 1);
        assert_eq!(outcome.report.warnings[0].code, "imageMissing");
    }

    #[test]
    fn options_are_read_and_clamped() {
        let mut req = request(vec![], None);
        req.options.insert("paper".into(), OptionValue::Text("letter".into()));
        req.options.insert("marginCm".into(), OptionValue::Number(99.0));
        let options = pdf_options(&req);
        assert_eq!(options.paper, Paper::Letter);
        assert_eq!(options.margin_cm, 5.0);
        assert_eq!(options.font_size_pt, 11.0);
    }

    #[test]
    fn descriptor_defaults_match_engine_defaults() {
        let d = MdToPdf.descriptor();
        let defaults = PdfOptions::default();
        for spec in &d.options {
            match (spec.key.as_str(), &spec.kind) {
                ("marginCm", OptionKind::Number { default, .. }) => {
                    assert_eq!(*default, defaults.margin_cm)
                }
                ("fontSizePt", OptionKind::Number { default, .. }) => {
                    assert_eq!(*default, defaults.font_size_pt)
                }
                _ => {}
            }
        }
    }
}
