//! Data exchanged between the backend commands and the interface.

use serde::{Deserialize, Serialize};

/// A page of the project.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
}

/// A `[[link]]` whose page does not exist, with the pages using it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BrokenLink {
    pub target: String,
    pub sources: Vec<String>,
    /// Total number of times the link appears in the project.
    #[serde(default)]
    pub count: usize,
}

/// Git state of a project folder, as shown under the project name.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct GitStatus {
    /// The folder is inside a git work tree.
    pub repo: bool,
    /// Current branch, `None` when detached.
    pub branch: Option<String>,
    /// The branch tracks a remote branch.
    pub upstream: bool,
    /// Files changed, staged or untracked (to commit).
    pub changes: u32,
    /// Commits not pushed yet.
    pub ahead: u32,
    /// Commits of the remote not pulled yet (as of the last fetch).
    pub behind: u32,
}

/// An org-mode tag of the project and how many pages and headlines carry it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TagCount {
    pub name: String,
    pub count: usize,
}

/// A page, or a headline of it, matching a tag search.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TagHit {
    pub page: String,
    pub path: String,
    /// `None` when the whole page matches (`#+FILETAGS:`).
    pub heading: Option<String>,
    pub level: usize,
    pub line: usize,
    /// All its tags, inherited ones included.
    pub tags: Vec<String>,
}

/// A headline with a TODO keyword, somewhere in the project.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TodoHit {
    pub page: String,
    pub path: String,
    pub keyword: String,
    pub done: bool,
    /// The title after the keyword, without its tags.
    pub title: String,
    pub level: usize,
    /// Line of the headline in the page (0-based).
    pub line: usize,
}

/// Pages changed on disk by another program since the last look.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VaultChanges {
    /// All the pages of the project, as now on disk.
    pub files: Vec<FileEntry>,
    /// Paths of the pages created or modified.
    pub changed: Vec<String>,
    /// Paths of the pages deleted.
    pub removed: Vec<String>,
}
