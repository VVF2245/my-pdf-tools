//! The intermediate representation (IR).
//!
//! With N formats we need N readers and N writers instead of N x N converters:
//! every reader produces a [`Document`] and every writer consumes one.

use serde::{Deserialize, Serialize};

/// A whole document: metadata, ordered blocks and the binary assets (images) they reference.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub metadata: Metadata,
    pub blocks: Vec<Block>,
    pub assets: Vec<Asset>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Metadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub language: Option<String>,
}

/// A block-level element plus the page it came from (when the source has pages).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Block {
    pub kind: BlockKind,
    /// 1-based source page. `None` for sources without pages, such as Markdown.
    pub page: Option<u32>,
}

impl Block {
    pub fn new(kind: BlockKind) -> Self {
        Self { kind, page: None }
    }
}

/// Alignment of a table column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TableAlign {
    None,
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BlockKind {
    /// `level` goes from 1 (top) to 6.
    Heading { level: u8, content: Vec<Inline> },
    Paragraph(Vec<Inline>),
    List {
        ordered: bool,
        /// First number of an ordered list.
        start: Option<u64>,
        items: Vec<ListItem>,
    },
    CodeBlock { lang: Option<String>, code: String },
    Table {
        aligns: Vec<TableAlign>,
        header: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
    },
    Image { asset: AssetId, alt: String },
    Quote(Vec<Block>),
    ThematicBreak,
    /// Marks the end of a source page or slide.
    PageBreak,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListItem {
    /// `Some(done)` for task-list items.
    pub checked: Option<bool>,
    /// Content of the item; nested lists are just `List` blocks.
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Inline {
    Text(String),
    Emphasis(Vec<Inline>),
    Strong(Vec<Inline>),
    Strikethrough(Vec<Inline>),
    Code(String),
    Link { url: String, content: Vec<Inline> },
    LineBreak,
}

/// Identifier of an [`Asset`] inside a [`Document`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AssetId(pub String);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Asset {
    pub id: AssetId,
    pub mime: String,
    pub data: Vec<u8>,
}

impl Document {
    /// Looks up an asset by id.
    pub fn asset(&self, id: &AssetId) -> Option<&Asset> {
        self.assets.iter().find(|a| &a.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_asset_by_id() {
        let doc = Document {
            assets: vec![Asset {
                id: AssetId("a".into()),
                mime: "image/png".into(),
                data: vec![1],
            }],
            ..Default::default()
        };
        assert!(doc.asset(&AssetId("a".into())).is_some());
        assert!(doc.asset(&AssetId("b".into())).is_none());
    }

    #[test]
    fn block_new_has_no_page() {
        assert_eq!(Block::new(BlockKind::ThematicBreak).page, None);
    }
}
