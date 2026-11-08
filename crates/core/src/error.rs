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
    }

    #[test]
    fn io_error_converts() {
        let e: ConversionError = std::io::Error::other("boom").into();
        assert_eq!(e, ConversionError::Io { message: "boom".into() });
    }
}
