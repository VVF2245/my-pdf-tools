//! The [`Tool`] contract: a descriptor plus a `run` operation.
//!
//! The UI is drawn from [`ToolDescriptor`]s, so adding a tool means adding a
//! module in Rust (and optionally a custom screen in React).

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::ConversionError;

/// Static description of a tool, enough for the UI to render its form.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ToolDescriptor {
    pub id: String,
    /// Translation key of the display name (like every label in a descriptor).
    pub name: String,
    /// Translation key of the description.
    pub description: String,
    /// Accepted input file extensions, lowercase and without the dot.
    pub inputs: Vec<String>,
    /// Extension of the produced files.
    pub outputs: Vec<String>,
    pub options: Vec<OptionSpec>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OptionSpec {
    /// Key used in [`ConversionRequest::options`].
    pub key: String,
    /// Label identifier the UI translates (not display text).
    pub label: String,
    pub kind: OptionKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(export)]
pub enum OptionKind {
    Choice { choices: Vec<OptionChoice>, default: String },
    Number { min: f64, max: f64, default: f64, step: f64 },
    Bool { default: bool },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct OptionChoice {
    pub value: String,
    /// Label identifier the UI translates.
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(untagged)]
#[ts(export)]
pub enum OptionValue {
    Bool(bool),
    Number(f64),
    Text(String),
}

/// What the user asks a tool to do.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ConversionRequest {
    pub tool_id: String,
    /// Input file paths.
    pub inputs: Vec<String>,
    /// Destination folder. `None` means next to each input.
    pub output_dir: Option<String>,
    pub options: BTreeMap<String, OptionValue>,
}

impl ConversionRequest {
    pub fn text_option(&self, key: &str) -> Option<&str> {
        match self.options.get(key) {
            Some(OptionValue::Text(s)) => Some(s),
            _ => None,
        }
    }

    pub fn number_option(&self, key: &str) -> Option<f64> {
        match self.options.get(key) {
            Some(OptionValue::Number(n)) => Some(*n),
            _ => None,
        }
    }

    pub fn bool_option(&self, key: &str) -> Option<bool> {
        match self.options.get(key) {
            Some(OptionValue::Bool(b)) => Some(*b),
            _ => None,
        }
    }
}

/// A non-fatal notice, e.g. "image omitted". Part of the conversion report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ConversionWarning {
    /// Stable code the UI translates.
    pub code: String,
    /// Technical detail (file name, page...), not translated.
    pub detail: String,
    pub page: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ConversionReport {
    pub warnings: Vec<ConversionWarning>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ConversionOutcome {
    /// Paths of the files produced.
    pub outputs: Vec<String>,
    pub report: ConversionReport,
}

/// Receives progress from a running tool.
pub trait Progress: Send + Sync {
    /// `fraction` goes from 0.0 to 1.0.
    fn report(&self, fraction: f32);
}

/// A progress sink that ignores everything (tests, headless use).
pub struct NoProgress;

impl Progress for NoProgress {
    fn report(&self, _fraction: f32) {}
}

/// Cooperative cancellation: the tool polls it at safe points.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    /// Returns `Err(Cancelled)` once cancellation was requested; use with `?`.
    pub fn check(&self) -> Result<(), ConversionError> {
        if self.is_cancelled() {
            Err(ConversionError::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// The unit of extension (similar to a Java interface).
pub trait Tool: Send + Sync {
    fn descriptor(&self) -> ToolDescriptor;

    fn run(
        &self,
        request: &ConversionRequest,
        progress: &dyn Progress,
        cancel: &CancelToken,
    ) -> Result<ConversionOutcome, ConversionError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_token_is_shared_between_clones() {
        let token = CancelToken::new();
        let clone = token.clone();
        assert!(token.check().is_ok());
        clone.cancel();
        assert_eq!(token.check(), Err(ConversionError::Cancelled));
    }

    #[test]
    fn option_values_deserialize_untagged() {
        let v: OptionValue = serde_json::from_str("true").unwrap();
        assert_eq!(v, OptionValue::Bool(true));
        let v: OptionValue = serde_json::from_str("2.5").unwrap();
        assert_eq!(v, OptionValue::Number(2.5));
        let v: OptionValue = serde_json::from_str("\"a4\"").unwrap();
        assert_eq!(v, OptionValue::Text("a4".into()));
    }

    #[test]
    fn typed_option_getters() {
        let mut options = BTreeMap::new();
        options.insert("page".to_string(), OptionValue::Text("a4".into()));
        options.insert("margin".to_string(), OptionValue::Number(2.2));
        let req = ConversionRequest {
            tool_id: "t".into(),
            inputs: vec![],
            output_dir: None,
            options,
        };
        assert_eq!(req.text_option("page"), Some("a4"));
        assert_eq!(req.number_option("margin"), Some(2.2));
        assert_eq!(req.bool_option("page"), None);
    }
}
