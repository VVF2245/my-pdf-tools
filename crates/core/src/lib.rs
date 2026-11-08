//! Domain model and contracts of my-pdf-tools.
//!
//! This crate is the center of the hexagonal architecture: it depends on no
//! other crate of the workspace and knows nothing about PDFium, Typst or Tauri.
//!
//! - [`document`]: the intermediate representation (IR) every format converts to or from.
//! - [`tool`]: the [`Tool`] contract, its descriptor and the request/outcome types.
//! - [`error`]: typed errors with stable codes the UI translates.
//! - [`job`]: job state exposed to the UI.

pub mod document;
pub mod error;
pub mod job;
pub mod tool;

pub use document::{
    Asset, AssetId, Block, BlockKind, Document, Inline, ListItem, Metadata, TableAlign,
};
pub use error::ConversionError;
pub use job::{Job, JobStatus};
pub use tool::{
    CancelToken, ConversionOutcome, ConversionReport, ConversionRequest, ConversionWarning,
    NoProgress, OptionChoice, OptionKind, OptionSpec, OptionValue, Progress, Tool, ToolDescriptor,
};
