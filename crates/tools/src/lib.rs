//! Use cases: the tools users run, plus the registry that lists them.
//!
//! The registry is filled at compile time (no dynamic plugin loader).

mod md_to_pdf;
mod registry;

pub use md_to_pdf::MdToPdf;
pub use registry::Registry;
