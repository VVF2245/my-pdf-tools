//! Use cases: the tools users run, plus the registry that lists them.
//!
//! The registry is filled at compile time (no dynamic plugin loader).

mod config;
mod md_to_pdf;
mod paths;
mod pdf_to_md;
mod registry;
#[cfg(test)]
mod test_util;

pub use config::ToolsConfig;
pub use md_to_pdf::MdToPdf;
pub use pdf_to_md::PdfToMd;
pub use registry::Registry;
