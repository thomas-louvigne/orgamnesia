use leptos::prelude::*;

use crate::{i18n::{t, Lang}, keybindings::Keybindings};

pub use orgamnesia_core::{BrokenLink, FileEntry, GitStatus, Prefs, Settings, TagCount, TagHit, VaultChanges};

/// Where to put the cursor in a page being opened.
#[derive(Debug, Clone, PartialEq)]
pub enum Goto {
    /// Select the first link to this page.
    Link(String),
    /// Start of this line (0-based).
    Line(usize),
}

#[derive(Clone, Debug)]
pub struct Tab {
    pub path: String,
    pub name: String,
    pub content: RwSignal<String>,
    pub dirty: RwSignal<bool>,
}

impl Tab {
    /// A tab showing `file`. Its signals belong to the current reactive owner:
    /// create it before any `.await`.
    pub fn new(file: &FileEntry, content: String, dirty: bool) -> Self {
        Self {
            path: file.path.clone(),
            name: file.name.clone(),
            content: RwSignal::new(content),
            dirty: RwSignal::new(dirty),
        }
    }
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

/// The open project, as the interface knows it.
#[derive(Clone, Copy)]
pub struct ProjectState {
    /// Folder of the open project.
    pub vault_path: RwSignal<Option<String>>,
    /// Every project opened, most recent first.
    pub projects: RwSignal<Vec<String>>,
    pub files: RwSignal<Vec<FileEntry>>,
    /// Org-mode tags of the project (refreshed when pages change).
    pub tags: RwSignal<Vec<TagCount>>,
    /// Tag search shown in the tags panel (`projet+urgent-perso`).
    pub tag_query: RwSignal<String>,
    /// Last git state read for the project; `None` until read or when git is missing.
    pub git: RwSignal<Option<GitStatus>>,
    /// Bumped after every write to disk, so link-derived views can refresh.
    pub links_version: RwSignal<u32>,
}

/// The open pages and how the editor shows them.
#[derive(Clone, Copy)]
pub struct Workspace {
    pub tabs: RwSignal<Vec<Tab>>,
    /// Tab of the focused pane.
    pub active_tab: RwSignal<Option<usize>>,
    /// Editor split in two panes, if any; `other_path` is the page shown in the other one.
    pub split: RwSignal<Option<SplitKind>>,
    pub other_path: RwSignal<Option<String>>,
    /// Whether the focused pane is the second one (right / bottom).
    pub focus_second: RwSignal<bool>,
    /// (page path, target): the editor showing that page moves the cursor there.
    pub goto: RwSignal<Option<(String, Goto)>>,
    /// Text of the last Emacs kill, for yank.
    pub kill_ring: RwSignal<String>,
}

/// Dialogs, menus and messages.
#[derive(Clone, Copy)]
pub struct Ui {
    pub show_projects: RwSignal<bool>,
    pub show_settings: RwSignal<bool>,
    pub show_new_page: RwSignal<bool>,
    pub show_quick_open: RwSignal<bool>,
    /// Pending confirmation (the webview's own `window.confirm` shows nothing here).
    pub confirm: RwSignal<Option<ConfirmReq>>,
    /// Message of the status bar.
    pub status: RwSignal<Option<String>>,
    /// Resize bar being dragged.
    pub drag: RwSignal<Option<Drag>>,
}

/// Global app context — Copy because signals are Copy.
#[derive(Clone, Copy)]
pub struct AppCtx {
    /// The settings in effect.
    pub prefs: RwSignal<Prefs>,
    /// Interface language (from `prefs`).
    pub lang: Memo<Lang>,
    pub keybindings: RwSignal<Keybindings>,
    pub project: ProjectState,
    pub work: Workspace,
    pub ui: Ui,
}

impl AppCtx {
    pub fn new() -> Self {
        let prefs = RwSignal::new(Prefs::default());
        Self {
            prefs,
            lang: Memo::new(move |_| prefs.with(|p| Lang::from_code(&p.lang))),
            keybindings: RwSignal::new(Keybindings::defaults()),
            project: ProjectState {
                vault_path: RwSignal::new(None),
                projects: RwSignal::new(vec![]),
                files: RwSignal::new(vec![]),
                tags: RwSignal::new(vec![]),
                tag_query: RwSignal::new(String::new()),
                git: RwSignal::new(None),
                links_version: RwSignal::new(0),
            },
            work: Workspace {
                tabs: RwSignal::new(vec![]),
                active_tab: RwSignal::new(None),
                split: RwSignal::new(None),
                other_path: RwSignal::new(None),
                focus_second: RwSignal::new(false),
                goto: RwSignal::new(None),
                kill_ring: RwSignal::new(String::new()),
            },
            ui: Ui {
                show_projects: RwSignal::new(false),
                show_settings: RwSignal::new(false),
                show_new_page: RwSignal::new(false),
                show_quick_open: RwSignal::new(false),
                confirm: RwSignal::new(None),
                status: RwSignal::new(None),
                drag: RwSignal::new(None),
            },
        }
    }

    /// One setting, tracked (re-runs effects when the settings change).
    pub fn pref<T>(&self, f: impl FnOnce(&Prefs) -> T) -> T {
        self.prefs.with(f)
    }

    pub fn pref_untracked<T>(&self, f: impl FnOnce(&Prefs) -> T) -> T {
        self.prefs.with_untracked(f)
    }

    /// The settings in effect, with the open project as the project setting.
    pub fn prefs_untracked(&self) -> Prefs {
        Prefs {
            vault: self.project.vault_path.get_untracked().unwrap_or_default(),
            ..self.prefs.get_untracked()
        }
    }

    /// Tell the views derived from the pages' links (backlinks, tags…) to refresh.
    pub fn bump_links(&self) {
        self.project.links_version.update(|v| *v += 1);
    }

    /// The page called `name`, per the case-sensitivity setting.
    pub fn find_page(&self, name: &str) -> Option<FileEntry> {
        self.project.files.with_untracked(|fs| fs.iter().find(|f| self.same_page(&f.name, name)).cloned())
    }

    /// Whether two page names designate the same page, per the case-sensitivity setting.
    pub fn same_page(&self, a: &str, b: &str) -> bool {
        if self.pref_untracked(|p| p.case_insensitive_links) {
            a.to_lowercase() == b.to_lowercase()
        } else {
            a == b
        }
    }

    /// Show the pages and headlines tagged `tag` in the tags frame.
    pub fn show_tag(&self, tag: String) {
        self.project.tag_query.set(tag);
    }

    /// Whether the right-hand panel has any frame to show.
    pub fn has_right_panel(&self) -> bool {
        self.pref(|p| p.show_backlinks || p.show_tags || p.show_broken_links)
    }

    /// How `#tags` are read, from the settings (tracked: re-runs effects when they change).
    pub fn hashtags(&self) -> crate::motion::Hashtags {
        self.pref(|p| p.hashtags())
    }

    pub fn hashtags_untracked(&self) -> crate::motion::Hashtags {
        self.pref_untracked(|p| p.hashtags())
    }

    /// Split the editor in two panes showing the current page (Emacs `C-x 2` / `C-x 3`).
    /// Splitting again only changes the direction.
    pub fn split_window(&self, kind: SplitKind) {
        let work = self.work;
        if work.split.get_untracked().is_none() {
            let Some(tab) = work.active_tab_data() else {
                self.notify_t("split_needs_page", "");
                return;
            };
            work.other_path.set(Some(tab.path));
            work.focus_second.set(false);
        }
        work.split.set(Some(kind));
    }

    /// Ask the user to confirm; `on_yes` runs only if they accept.
    pub fn ask_confirm(&self, message: String, on_yes: impl Fn() + Send + Sync + 'static) {
        self.ui.confirm.set(Some(ConfirmReq { message, on_yes: Callback::new(move |_| on_yes()) }));
    }

    /// Show a message in the status bar.
    pub fn notify(&self, msg: impl Into<String>) {
        self.ui.status.set(Some(msg.into()));
    }

    /// Show the text of `key`, followed by `detail` (a page name…).
    pub fn notify_t(&self, key: &'static str, detail: &str) {
        let label = t(key, self.lang.get_untracked());
        self.notify(if detail.is_empty() { label.to_string() } else { format!("{label} {detail}") });
    }

    /// Show the failure `key`, with the error message.
    pub fn error(&self, key: &'static str, err: &str) {
        self.notify(format!("{} : {err}", t(key, self.lang.get_untracked())));
    }

    /// Show the keys of a chord in progress ("⌨ Ctrl+X …") in the status bar.
    pub fn set_chord_status(&self, keys: &str) {
        self.notify(format!("{CHORD_MARK}{keys} …"));
    }

    /// Remove the chord-in-progress message, if that is what the status bar shows.
    pub fn clear_chord_status(&self) {
        if self.ui.status.with_untracked(|s| s.as_deref().is_some_and(|s| s.starts_with(CHORD_MARK))) {
            self.ui.status.set(None);
        }
    }
}

impl Default for AppCtx {
    fn default() -> Self { Self::new() }
}

impl Workspace {
    pub fn active_tab_data(&self) -> Option<Tab> {
        let idx = self.active_tab.get()?;
        self.tabs.with(|tabs| tabs.get(idx).cloned())
    }

    /// Index of the tab showing the page at `path`.
    pub fn tab_index(&self, path: &str) -> Option<usize> {
        self.tabs.with_untracked(|tabs| tabs.iter().position(|t| t.path == path))
    }

    pub fn has_unsaved_tabs(&self) -> bool {
        self.tabs.with_untracked(|tabs| tabs.iter().any(|t| t.dirty.get_untracked()))
    }

    /// Give the focus to a pane; the tab bar and page opening then act on it.
    pub fn focus_pane(&self, second: bool) {
        if self.split.get_untracked().is_none() || self.focus_second.get_untracked() == second {
            return;
        }
        let current = self.active_tab_data().map(|t| t.path);
        let idx = self.other_path.get_untracked().and_then(|p| self.tab_index(&p));
        if idx.is_some() {
            self.active_tab.set(idx);
            self.other_path.set(current);
        }
        self.focus_second.set(second);
    }

    /// Path of the page shown in a pane (`second`: right / bottom one).
    pub fn pane_path(&self, second: bool) -> Option<String> {
        if self.focus_second.get() == second {
            self.active_tab_data().map(|t| t.path)
        } else {
            self.other_path.get()
        }
    }

    /// Show the tab `idx` in a pane, giving it the focus.
    pub fn show_in_pane(&self, second: bool, idx: usize) {
        self.focus_pane(second);
        self.active_tab.set(Some(idx));
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
        if let Some(idx) = self.other_path.get_untracked().and_then(|p| self.tab_index(&p)) {
            self.active_tab.set(Some(idx));
        }
        self.single_window();
    }
}
