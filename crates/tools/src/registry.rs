//! The tool registry: the single place that knows which tools exist.

use std::sync::Arc;

use mpt_core::{Tool, ToolDescriptor};

use crate::{MdToPdf, PdfToMd, ToolsConfig};

#[derive(Clone)]
pub struct Registry {
    tools: Vec<Arc<dyn Tool>>,
}

impl Registry {
    /// All tools of the application. Adding a tool means adding a line here.
    pub fn new(config: &ToolsConfig) -> Self {
        Self { tools: vec![Arc::new(MdToPdf), Arc::new(PdfToMd::new(config))] }
    }

    /// A registry with the given tools (used by tests of higher layers).
    pub fn from_tools(tools: Vec<Arc<dyn Tool>>) -> Self {
        Self { tools }
    }

    pub fn descriptors(&self) -> Vec<ToolDescriptor> {
        self.tools.iter().map(|t| t.descriptor()).collect()
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn Tool>> {
        self.tools.iter().find(|t| t.descriptor().id == id).cloned()
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new(&ToolsConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_and_finds_both_tools() {
        let registry = Registry::default();
        let ids: Vec<String> = registry.descriptors().into_iter().map(|d| d.id).collect();
        assert_eq!(ids, vec!["md_to_pdf".to_string(), "pdf_to_md".to_string()]);
        assert!(registry.get("pdf_to_md").is_some());
        assert!(registry.get("md_to_pdf").is_some());
        assert!(registry.get("nope").is_none());
    }

    #[test]
    fn every_option_explains_itself() {
        for descriptor in Registry::default().descriptors() {
            for option in &descriptor.options {
                let help = option.help.as_deref().unwrap_or("");
                assert_eq!(help, format!("{}.help", option.label), "{}: {}", descriptor.id, option.key);
            }
        }
    }

    #[test]
    fn descriptor_ids_are_unique() {
        let ids: Vec<String> = Registry::default().descriptors().into_iter().map(|d| d.id).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(ids.len(), sorted.len());
    }
}
