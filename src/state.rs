use leptos::prelude::*;

use crate::{i18n::{t, Lang}, keybindings::Keybindings};

pub use orgamnesia_core::{BrokenLink, FileEntry, GitChange, GitChangeKind, GitCommit, GitStatus, UpdateInfo, VersionNotes, Prefs, ReplaceOutcome, ReplaceSkip, SearchOptions, SearchResults, Session, Settings, SiteConfig, SplitKind, TagCount, TagHit, TodoHit, VaultChanges};
mod ctx;

/// Where to put the cursor in a page being opened.
#[derive(Debug, Clone, PartialEq)]
pub enum Goto {
    /// Select the first link to this page.
    Link(String),
    /// Start of this line (0-based).
    Line(usize),
    /// Select this char range (a match of the project search).
    Range(usize, usize),
    /// The place a link names in the page: `*Titre` (a headline), `cible` (a `<<cible>>`).
    Place(String),
    /// Where it already is: only the keyboard goes to the editor.
    Focus,
    /// Where it already is, then this editing action (from the command list).
    Action(crate::keybindings::EditorAction),
    /// A new note at the end of the page (quick capture), at this time.
    Capture(orgamnesia_core::templates::Now),
    /// Where it already is, this text put in (a template): over the whole page
    /// (`whole`, or when it is blank or holds only its title), else at the cursor.
    Insert { text: String, whole: bool },
}

/// What the project search pop-in shows; kept while it is closed.
#[derive(Clone, Copy)]
pub struct ProjectSearch {
    /// Open, and with the replace field (`Some(true)`).
    pub open: RwSignal<Option<bool>>,
    pub query: RwSignal<String>,
    pub replacement: RwSignal<String>,
    pub options: RwSignal<SearchOptions>,
    pub results: RwSignal<SearchResults>,
    /// Matches left out of the replacement: the pages, and single matches (path, start).
    pub skip: RwSignal<ReplaceSkip>,
    /// The last project replacement can be undone.
    pub can_undo: RwSignal<bool>,
}

#[derive(Clone, Debug)]
pub struct Tab {
    pub path: String,
    pub name: String,
    pub content: RwSignal<String>,
    pub dirty: RwSignal<bool>,
    /// The page's undo history, kept by the editor (see `history`).
    pub history: StoredValue<crate::history::History>,
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
            history: StoredValue::new(Default::default()),
        }
    }
}

/// A question shown in the in-app confirmation dialog.
#[derive(Clone)]
pub struct ConfirmReq {
    pub message: String,
    pub on_yes: Callback<()>,
    /// A destructive action: the label key of its (red) button, and Cancel has the focus.
    pub danger: Option<&'static str>,
    /// Only a message to read: a single OK button, nothing to confirm.
    pub info: bool,
    /// A choice between two actions: the label keys of the buttons, and what
    /// the first one does (it replaces Cancel; the second one runs `on_yes`).
    pub choice: Option<(&'static str, Callback<()>, &'static str)>,
}

/// A value asked in a small dialog (see `AppCtx::ask_text`).
#[derive(Clone)]
pub struct PromptReq {
    pub label: String,
    pub value: String,
    /// Offered while typing (the folders…).
    pub choices: Vec<String>,
    pub on_ok: Callback<String>,
}

/// A git command of the git extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitOp { Pull, Commit, Push, Sync, Setup }

/// Which resize bar is being dragged.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Drag {
    /// Width of the left column.
    Sidebar,
    /// Width of the right column.
    Panel,
    /// Height of the "pages not created" block; `above`: its bar is above it.
    Broken { above: bool },
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
    /// The folders of the pages (`Jeux/Quêtes`), empty ones too.
    pub folders: RwSignal<Vec<String>>,
    /// Finds the page a link names, among `files`, per the settings (ignoring
    /// case, default format); built again when they change.
    pub resolver: Memo<orgamnesia_core::pages::Resolver<'static>>,
    /// Org-mode tags of the project (refreshed when pages change).
    pub tags: RwSignal<Vec<TagCount>>,
    /// Tag search shown in the tags panel (`projet+urgent-perso`).
    pub tag_query: RwSignal<String>,
    /// Last git state read for the project; `None` until read or when git is missing.
    pub git: RwSignal<Option<GitStatus>>,
    /// The git command running (pull, commit, push): one at a time.
    pub git_busy: RwSignal<Option<GitOp>>,
    /// The automatic sync stopped on a failure (a conflict…): it starts again
    /// after a pull or a push that works, or in another project.
    pub git_sync_paused: RwSignal<bool>,
    /// Org-bloom is building the site.
    pub bloom_busy: RwSignal<bool>,
    /// When the site of the project was last built (seconds since the epoch);
    /// `None` when it never was.
    pub bloom_built: RwSignal<Option<u64>>,
    /// Bumped after every write to disk, so link-derived views can refresh.
    pub links_version: RwSignal<u32>,
    /// The "preference" project is shown in place of the open project (see `preferences`).
    pub preference: RwSignal<bool>,
    /// The open project, put aside meanwhile.
    pub aside: StoredValue<Option<crate::preferences::Aside>>,
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
    /// The quick capture is open in the second pane; `true` when it split the editor itself.
    pub capture: RwSignal<Option<bool>>,
    /// (page path, target): the editor showing that page moves the cursor there.
    pub goto: RwSignal<Option<(String, Goto)>>,
    /// Text of the last Emacs kill, for yank.
    pub kill_ring: RwSignal<String>,
    /// Pages visited before the current one (most recent last), and the ones left
    /// by going back, for Alt+← / Alt+→.
    pub history_back: RwSignal<Vec<String>>,
    pub history_forward: RwSignal<Vec<String>>,
    /// Page being reached by going back or forward: not recorded as a new visit.
    pub history_arriving: RwSignal<Option<String>>,
}

/// Dialogs, menus and messages.
#[derive(Clone, Copy)]
pub struct Ui {
    pub show_projects: RwSignal<bool>,
    pub show_settings: RwSignal<bool>,
    /// The SSH passphrase is asked, to run this again; `true` when the last one was refused.
    pub git_passphrase: RwSignal<Option<(GitOp, bool)>>,
    /// The app holds a passphrase: a refusal now means it is not the right one.
    pub git_passphrase_sent: RwSignal<bool>,
    pub show_new_page: RwSignal<bool>,
    /// The name the new page window opens with (a link followed to a page not
    /// created yet, when the format of new pages is asked).
    pub new_page_name: RwSignal<String>,
    /// The window of the first run (format of the pages, shortcuts).
    pub show_welcome: RwSignal<bool>,
    pub show_quick_open: RwSignal<bool>,
    pub show_commit: RwSignal<bool>,
    /// The graph of the pages (graph extension).
    pub show_graph: RwSignal<bool>,
    /// The agenda of the week (org-mode's `C-c a a`).
    pub show_agenda: RwSignal<bool>,
    /// Zen mode: only the page, full screen.
    pub zen: RwSignal<bool>,
    /// The history window of this page (git extension).
    pub git_history: RwSignal<Option<FileEntry>>,
    /// The window adding a remote to the project's repository.
    pub show_git_remote: RwSignal<bool>,
    /// The window naming a new branch.
    pub show_git_branch: RwSignal<bool>,
    /// The window cloning a repository as a new project.
    pub show_clone: RwSignal<bool>,
    /// The window asking the name and email git signs the commits with.
    pub show_git_identity: RwSignal<bool>,
    /// The new version offered, in its window.
    pub update: RwSignal<Option<UpdateInfo>>,
    /// The new version found, even skipped or closed: told on the empty editor.
    pub new_version: RwSignal<Option<UpdateInfo>>,
    /// The cheat sheet of the shortcuts (F1).
    pub show_shortcuts: RwSignal<bool>,
    /// Asked to rename the active page (its title's field opens): counts the times.
    pub rename_request: RwSignal<u32>,
    /// The video played in the app: the address of the site's player, and the
    /// link to it (to open it in the browser).
    pub video: RwSignal<Option<(String, String)>>,
    pub search: ProjectSearch,
    /// Open pages changed on disk while they have unsaved changes: which version
    /// to keep is asked for each, one after the other.
    pub disk_conflicts: RwSignal<Vec<String>>,
    /// The field to add a tag to the page, over the focused pane.
    pub tag_add: RwSignal<bool>,
    /// Pending confirmation (the webview's own `window.confirm` shows nothing here).
    pub confirm: RwSignal<Option<ConfirmReq>>,
    /// A value asked (a folder's name…).
    pub prompt: RwSignal<Option<PromptReq>>,
    /// Message of the status bar.
    pub status: RwSignal<Option<String>>,
    /// A message of the status bar with an "Undo" button, and what the button does.
    /// The button shows while the status bar shows that message.
    pub status_undo: RwSignal<Option<(String, Callback<()>)>>,
    /// Resize bar being dragged.
    pub drag: RwSignal<Option<Drag>>,
    /// Last files dropped on the window, numbered so that the same drop twice is seen:
    /// the editor they fall on adds the images among them.
    pub dropped: RwSignal<Option<(u32, crate::invoke::Dropped)>>,
}

/// Global app context — Copy because signals are Copy.
#[derive(Clone, Copy)]
pub struct AppCtx {
    /// The settings in effect.
    pub prefs: RwSignal<Prefs>,
    /// Interface language (from `prefs`).
    pub lang: Memo<Lang>,
    pub keybindings: RwSignal<Keybindings>,
    /// Changes when a word is added to the spelling dictionary: pages are checked again.
    pub spell_words: RwSignal<u32>,
    pub project: ProjectState,
    pub work: Workspace,
    pub ui: Ui,
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
