//! Spike S0-D: generate TypeScript types from Rust with `ts-rs`.
//!
//! Run `cargo test` to (re)generate the files under `bindings/`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Stable error codes the UI translates into messages (see design doc, 3.5).
#[derive(Serialize, Deserialize, TS)]
#[serde(tag = "code", rename_all = "camelCase")]
#[ts(export)]
pub enum ConversionError {
    Encrypted,
    Corrupt,
    NoExtractableText,
    Io { message: String },
    Cancelled,
    UnsupportedInput { extension: String },
}

#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ConversionMode {
    Notes,
    Slides,
}

#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ToolDescriptor {
    pub id: String,
    pub name: String,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}

#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PdfToMdOptions {
    pub mode: ConversionMode,
    pub page_range: Option<(u32, u32)>,
    pub extract_images: bool,
}
