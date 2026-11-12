//! Typed errors with stable codes. The UI translates each `code` into a message.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize, TS)]
#[serde(tag = "code", rename_all = "camelCase")]
#[ts(export)]
pub enum ConversionError {
    #[error("the document is encrypted")]
    Encrypted,
    #[error("the document is corrupt")]
    Corrupt,
    #[error("the document has no extractable text")]
    NoExtractableText,
    #[error("I/O error: {message}")]
    Io { message: String },
    #[error("the operation was cancelled")]
    Cancelled,
    #[error("unsupported input: {extension}")]
    UnsupportedInput { extension: String },
    /// The requested page range does not fit the document (`last` 0 means "to the end").
    #[error("invalid page range {first}-{last} (the document has {total} pages)")]
    InvalidPageRange { first: u32, last: u32, total: u32 },
    /// A native engine the tool needs (e.g. the PDFium library) could not be loaded.
    #[error("engine unavailable: {detail}")]
    EngineUnavailable { detail: String },
    #[error("internal error: {message}")]
    Internal { message: String },
}

impl From<std::io::Error> for ConversionError {
    fn from(e: std::io::Error) -> Self {
        Self::Io { message: e.to_string() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_with_stable_code() {
        let json = serde_json::to_string(&ConversionError::NoExtractableText).unwrap();
        assert_eq!(json, r#"{"code":"noExtractableText"}"#);
        let json = serde_json::to_string(&ConversionError::UnsupportedInput {
            extension: "xyz".into(),
        })
        .unwrap();
        assert_eq!(json, r#"{"code":"unsupportedInput","extension":"xyz"}"#);
        let json = serde_json::to_string(&ConversionError::EngineUnavailable {
            detail: "pdfium".into(),
        })
        .unwrap();
        assert_eq!(json, r#"{"code":"engineUnavailable","detail":"pdfium"}"#);
        let json = serde_json::to_string(&ConversionError::InvalidPageRange {
            first: 10,
            last: 3,
            total: 20,
        })
        .unwrap();
        assert_eq!(json, r#"{"code":"invalidPageRange","first":10,"last":3,"total":20}"#);
    }

    #[test]
    fn io_error_converts() {
        let e: ConversionError = std::io::Error::other("boom").into();
        assert_eq!(e, ConversionError::Io { message: "boom".into() });
    }
}
