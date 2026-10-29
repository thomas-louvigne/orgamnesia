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
    /// Files in conflict (a merge or a rebase stopped on them).
    #[serde(default)]
    pub conflicts: u32,
    /// A merge or a rebase is in progress, to finish elsewhere.
    #[serde(default)]
    pub merging: bool,
    /// The repository has a remote.
    #[serde(default)]
    pub remote: bool,
    /// Git can't read the folder for another reason than it not being a
    /// repository: what it says.
    #[serde(default)]
    pub error: Option<String>,
    /// Files of the project that differ from the remote branch (changed, or
    /// committed and not pushed): their full paths. Without a remote branch,
    /// those differing from the last commit.
    #[serde(default)]
    pub unpushed: Vec<String>,
}

/// A commit of the history of a page (`git log --follow`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GitCommit {
    pub hash: String,
    /// Seconds since the epoch.
    pub date: i64,
    pub author: String,
    /// First line of the message.
    pub subject: String,
    /// Path of the page in that commit, relative to the root of the repository
    /// (it may have been renamed since).
    pub path: String,
}

/// Where org-bloom builds the site of a project, as its config file says.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SiteConfig {
    /// The config file read: `org-bloom.toml` at the root of the project.
    pub config: String,
    /// The file exists.
    pub exists: bool,
    /// The `output_dir` of its `[site]` table, as an absolute path; `None`
    /// when not set.
    pub output_dir: Option<String>,
    /// When the site was last built (last change of that folder, in seconds
    /// since the epoch); `None` when it has never been built.
    #[serde(default)]
    pub built_at: Option<u64>,
}

/// What happened to a file of the project since the last commit.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum GitChangeKind { Added, Modified, Deleted, Renamed }

/// A file of the project changed since the last commit (`git status`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitChange {
    /// Path relative to the root of the repository.
    pub path: String,
    pub kind: GitChangeKind,
    /// Path before a rename.
    #[serde(default)]
    pub old_path: Option<String>,
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
