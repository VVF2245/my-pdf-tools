//! The tool registry: the single place that knows which tools exist.

use std::sync::Arc;

use mpt_core::{Tool, ToolDescriptor};

use crate::MdToPdf;

#[derive(Clone)]
pub struct Registry {
    tools: Vec<Arc<dyn Tool>>,
}

impl Registry {
    /// All tools of the application. Adding a tool means adding a line here.
    pub fn new() -> Self {
        Self { tools: vec![Arc::new(MdToPdf)] }
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
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_and_finds_md_to_pdf() {
        let registry = Registry::new();
        let ids: Vec<String> = registry.descriptors().into_iter().map(|d| d.id).collect();
        assert_eq!(ids, vec!["md_to_pdf".to_string()]);
        assert!(registry.get("md_to_pdf").is_some());
        assert!(registry.get("nope").is_none());
    }

    #[test]
    fn descriptor_ids_are_unique() {
        let ids: Vec<String> = Registry::new().descriptors().into_iter().map(|d| d.id).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(ids.len(), sorted.len());
    }
}
