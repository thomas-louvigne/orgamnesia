use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Block {
    Headline(Headline),
    Paragraph { inlines: Vec<Inline> },
    Table(Table),
    Empty,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Headline {
    pub level: u8,
    pub title: Vec<Inline>,
    pub tags: Vec<String>,
    pub children: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Inline {
    Text { content: String },
    Bold { children: Vec<Inline> },
    Italic { children: Vec<Inline> },
    Underline { children: Vec<Inline> },
    WikiLink { target: String, display: Option<String> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Table {
    pub rows: Vec<TableRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableRow {
    pub cells: Vec<String>,
    pub is_separator: bool,
}
