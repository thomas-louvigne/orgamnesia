use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{i18n::Lang, keybindings::Keybindings};

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

/// Where to put the cursor in a page being opened.
#[derive(Debug, Clone, PartialEq)]
pub enum Goto {
    /// Select the first link to this page.
    Link(String),
    /// Start of this line (0-based).
    Line(usize),
}

/// What the right-hand panel shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PanelView {
    Backlinks,
    Tags,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings {
    pub vault_path: Option<String>,
    pub logseq_site_builder_path: Option<String>,
    pub language: Option<String>,
    pub update_links_on_rename: Option<bool>,
    pub case_insensitive_links: Option<bool>,
    pub hashtag_links: Option<bool>,
    pub hashtag_dashes: Option<bool>,
    pub emacs_mark: Option<bool>,
    pub electric_mode: Option<bool>,
    pub autosave: Option<bool>,
    #[serde(default)]
    pub projects: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Tab {
    pub path: String,
    pub name: String,
    pub content: RwSignal<String>,
    pub dirty: RwSignal<bool>,
}

/// A question shown in the in-app confirmation dialog.
#[derive(Clone)]
pub struct ConfirmReq {
    pub message: String,
    pub on_yes: Callback<()>,
}

/// How the editor is split in two (Emacs windows).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SplitKind {
    /// Side by side (`C-x 3`).
    Vertical,
    /// One above the other (`C-x 2`).
    Horizontal,
}

/// Which resize bar is being dragged.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Drag {
    /// Width of the pages menu (left).
    Sidebar,
    /// Width of the backlinks panel (right).
    Panel,
    /// Height of the "pages not created" block.
    Broken,
}

const CHORD_MARK: &str = "⌨ ";

/// Global app context — Copy because RwSignal is Copy.
#[derive(Clone, Copy)]
pub struct AppCtx {
    pub vault_path: RwSignal<Option<String>>,
    pub projects: RwSignal<Vec<String>>,
    pub show_projects: RwSignal<bool>,
    pub drag: RwSignal<Option<Drag>>,
    /// Editor split in two panes, if any. `active_tab` is the tab of the focused
    /// pane; `other_path` is the tab shown in the other one.
    pub split: RwSignal<Option<SplitKind>>,
    pub other_path: RwSignal<Option<String>>,
    /// Whether the focused pane is the second one (right / bottom).
    pub focus_second: RwSignal<bool>,
    /// (page path, target): the editor showing that page moves the cursor there.
    pub goto: RwSignal<Option<(String, Goto)>>,
    pub panel: RwSignal<PanelView>,
    /// Org-mode tags of the project (refreshed when pages change).
    pub tags: RwSignal<Vec<TagCount>>,
    /// Tag search shown in the tags panel (`projet+urgent-perso`).
    pub tag_query: RwSignal<String>,
    /// Pending confirmation (the webview's own `window.confirm` shows nothing here).
    pub confirm: RwSignal<Option<ConfirmReq>>,
    /// Bumped after every write to disk, so link-derived views can refresh.
    pub links_version: RwSignal<u32>,
    pub autosave: RwSignal<bool>,
    pub emacs_mark: RwSignal<bool>,
    pub electric_mode: RwSignal<bool>,
    pub case_insensitive_links: RwSignal<bool>,
    pub hashtag_links: RwSignal<bool>,
    /// Allow `-` in `#tags` (not valid in org-mode tags).
    pub hashtag_dashes: RwSignal<bool>,
    pub files: RwSignal<Vec<FileEntry>>,
    pub tabs: RwSignal<Vec<Tab>>,
    pub active_tab: RwSignal<Option<usize>>,
    pub backlinks: RwSignal<Vec<String>>,
    pub show_settings: RwSignal<bool>,
    pub show_new_page: RwSignal<bool>,
    pub show_quick_open: RwSignal<bool>,
    pub status: RwSignal<Option<String>>,
    pub lang: RwSignal<Lang>,
    pub keybindings: RwSignal<Keybindings>,
    pub kill_ring: RwSignal<String>,
}

impl AppCtx {
    pub fn new() -> Self {
        Self {
            vault_path: RwSignal::new(None),
            projects: RwSignal::new(vec![]),
            show_projects: RwSignal::new(false),
            drag: RwSignal::new(None),
            split: RwSignal::new(None),
            other_path: RwSignal::new(None),
            focus_second: RwSignal::new(false),
            goto: RwSignal::new(None),
            panel: RwSignal::new(PanelView::Backlinks),
            tags: RwSignal::new(vec![]),
            tag_query: RwSignal::new(String::new()),
            confirm: RwSignal::new(None),
            links_version: RwSignal::new(0),
            autosave: RwSignal::new(true),
            emacs_mark: RwSignal::new(true),
            electric_mode: RwSignal::new(true),
            case_insensitive_links: RwSignal::new(true),
            hashtag_links: RwSignal::new(true),
            hashtag_dashes: RwSignal::new(true),
            files: RwSignal::new(vec![]),
            tabs: RwSignal::new(vec![]),
            active_tab: RwSignal::new(None),
            backlinks: RwSignal::new(vec![]),
            show_settings: RwSignal::new(false),
            show_new_page: RwSignal::new(false),
            show_quick_open: RwSignal::new(false),
            status: RwSignal::new(None),
            lang: RwSignal::new(Lang::default()),
            keybindings: RwSignal::new(Keybindings::defaults()),
            kill_ring: RwSignal::new(String::new()),
        }
    }

    /// Show the pages and headlines tagged `tag` in the right-hand panel.
    pub fn show_tag(&self, tag: String) {
        self.tag_query.set(tag);
        self.panel.set(PanelView::Tags);
    }

    /// How `#tags` are read, from the settings (tracked: re-runs effects when they change).
    pub fn hashtags(&self) -> crate::motion::Hashtags {
        crate::motion::Hashtags::new(self.hashtag_links.get(), self.hashtag_dashes.get())
    }

    pub fn hashtags_untracked(&self) -> crate::motion::Hashtags {
        crate::motion::Hashtags::new(self.hashtag_links.get_untracked(), self.hashtag_dashes.get_untracked())
    }

    /// Whether two page names designate the same page, per the case-sensitivity setting.
    pub fn same_page(&self, a: &str, b: &str) -> bool {
        if self.case_insensitive_links.get_untracked() {
            a.to_lowercase() == b.to_lowercase()
        } else {
            a == b
        }
    }

    /// Split the editor in two panes showing the current page (Emacs `C-x 2` / `C-x 3`).
    /// Splitting again only changes the direction.
    pub fn split_window(&self, kind: SplitKind) {
        if self.split.get_untracked().is_none() {
            let Some(tab) = self.active_tab_data() else {
                self.status.set(Some("Open a page before splitting".to_string()));
                return;
            };
            self.other_path.set(Some(tab.path));
            self.focus_second.set(false);
        }
        self.split.set(Some(kind));
    }

    /// Give the focus to a pane; the tab bar and page opening then act on it.
    pub fn focus_pane(&self, second: bool) {
        if self.split.get_untracked().is_none() || self.focus_second.get_untracked() == second {
            return;
        }
        let current = self.active_tab_data().map(|t| t.path);
        let other = self.other_path.get_untracked();
        let idx = other.as_ref().and_then(|p| {
            self.tabs.get_untracked().iter().position(|t| &t.path == p)
        });
        if idx.is_some() {
            self.active_tab.set(idx);
            self.other_path.set(current);
        }
        self.focus_second.set(second);
    }

    /// Emacs `C-x o`.
    pub fn other_window(&self) {
        self.focus_pane(!self.focus_second.get_untracked());
    }

    /// Emacs `C-x 1`: keep only the focused pane.
    pub fn single_window(&self) {
        self.split.set(None);
        self.other_path.set(None);
        self.focus_second.set(false);
    }

    /// Emacs `C-x 0`: close the focused pane; the other one stays.
    pub fn close_window(&self) {
        if self.split.get_untracked().is_none() { return; }
        let other = self.other_path.get_untracked();
        if let Some(idx) = other.and_then(|p| self.tabs.get_untracked().iter().position(|t| t.path == p)) {
            self.active_tab.set(Some(idx));
        }
        self.single_window();
    }

    /// Ask the user to confirm; `on_yes` runs only if they accept.
    pub fn ask_confirm(&self, message: String, on_yes: impl Fn() + Send + Sync + 'static) {
        self.confirm.set(Some(ConfirmReq { message, on_yes: Callback::new(move |_| on_yes()) }));
    }

    /// Show the keys of a chord in progress ("⌨ Ctrl+X …") in the status bar.
    pub fn set_chord_status(&self, keys: &str) {
        self.status.set(Some(format!("{CHORD_MARK}{keys} …")));
    }

    /// Remove the chord-in-progress message, if that is what the status bar shows.
    pub fn clear_chord_status(&self) {
        if self.status.with_untracked(|s| s.as_deref().map_or(false, |s| s.starts_with(CHORD_MARK))) {
            self.status.set(None);
        }
    }

    pub fn active_tab_data(&self) -> Option<Tab> {
        let idx = self.active_tab.get()?;
        self.tabs.get().into_iter().nth(idx)
    }
}

impl Default for AppCtx {
    fn default() -> Self { Self::new() }
}
