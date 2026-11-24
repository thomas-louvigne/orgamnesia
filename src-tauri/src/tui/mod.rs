//! The terminal interface (beta), started by `orgamnesia -nw [folder]` (or `--tui`, `-t`): the
//! pages of the project, filtered as one types, drawn in colour beside the
//! list and edited there; their links followed; the shortcuts those of the
//! window (the keybinding profile in effect).

mod commands;
mod complete;
mod editor;
mod input;
mod view;
#[cfg(test)]
mod tests;
mod highlight;
mod keys;

use std::path::{Path, PathBuf};

use orgamnesia_core::{keybindings::Keybindings, links, pages::Resolver, FileEntry, Prefs};
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui::crossterm::event::{DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture};
use ratatui::crossterm::event::{KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags};
use ratatui::layout::{Direction, Rect};
use ratatui::widgets::ListState;
use ratatui::DefaultTerminal;

use crate::{config_files::ConfigState, project::Project, settings};
use editor::Editor;
use highlight::Draw;
use keys::Resolution;

/// The folder Tauri gives the app's configuration (`app_config_dir`), found
/// without Tauri: the GUI's settings are those of the terminal.
fn config_dir() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        crate::home_dir().map(|h| h.join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME").filter(|d| !d.is_empty()).map(PathBuf::from)
            .or_else(|| crate::home_dir().map(|h| h.join(".config")))
    };
    base.map(|b| b.join("com.orgamnesia.app"))
}

/// Run the terminal interface on `folder`, else on the project last opened.
pub fn run(folder: Option<String>) -> Result<(), String> {
    let mut config = ConfigState::default();
    let settings = config_dir().map(|d| settings::load(&d, &mut config)).unwrap_or_default();
    let kb = config.keybindings(settings.config.keybinding_profile.as_deref());
    let prefs = settings.prefs();
    let path = folder.map(|f| crate::expand_home(&f)).or(settings.vault_path.clone())
        .ok_or("no project: give its folder, `orgamnesia -nw ~/notes`")?;
    let project = Project::open(&path, prefs.tags(), prefs.formats(), &prefs.journal_folder, &prefs.templates_folder).map_err(|e| e.to_string())?;
    prune_trash(&project, &prefs);
    let mut app = App::new(project, prefs, kb);
    app.watch_disk();
    app.projects = settings.projects.clone();
    // The columns as wide as they were dragged
    (app.split, app.right_split) = (settings.tui_left_width, settings.tui_right_width);
    app.config_dir = config_dir();
    // The pages and windows open when the app was last quit (restore_session)
    if let Some(session) = settings.session.clone().filter(|_| app.prefs.restore_session) { app.restore_session(session); }
    if let Some(dir) = app.config_dir.clone() && !settings.keep_json {
        let files = crate::config_files::json_leftovers(&dir, &config);
        if !files.is_empty() { app.offer_json_removal(files, settings.clone()); }
    }
    let mut terminal = init();
    let result = app.run(&mut terminal);
    // On quit, as the window does: the sync (git_sync_on_quit), the blank pages deleted
    if result.is_ok() && app.prefs.git_sync_on_quit { app.sync(); }
    let p = &app.prefs;
    crate::project::files::delete_blank_pages(&app.project.path, p.formats(), &p.journal_folder, &p.templates_folder, p.delete_empty, p.delete_title_only);
    // The state read again first: the window may have changed it meanwhile
    if let Some(dir) = &app.config_dir {
        let mut state = settings::load(dir, &mut ConfigState::default());
        // The session only for the window's project: another one's would take the place of its own
        let same = state.vault_path.as_deref().is_some_and(|v| crate::slash(v).trim_end_matches('/') == app.project.path.trim_end_matches('/'));
        if same { state.session = app.last_session.clone().filter(|_| app.prefs.restore_session); }
        (state.tui_left_width, state.tui_right_width) = (app.split, app.right_split);
        let _ = settings::save_state(dir, &state);
    }
    restore();
    result.map_err(|e| e.to_string())
}

/// The trash of `project` emptied of the pages deleted for too long, or beyond
/// its size, as the window does on opening a project (`trash_*` settings).
fn prune_trash(project: &Project, prefs: &Prefs) {
    if prefs.trash_auto_empty {
        let keep = (prefs.trash_keep_days > 0).then_some(prefs.trash_keep_days);
        let _ = project.prune_trash(keep, prefs.trash_max_bytes);
    }
}

/// Whether the keys are told apart as below, to undo it.
static ENHANCED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The terminal taken over; keys told apart as the window tells them
/// (Ctrl+Shift+J, Ctrl+Enter) where the terminal can (kitty's protocol).
fn init() -> DefaultTerminal {
    let terminal = ratatui::init();
    let enhanced = ratatui::crossterm::terminal::supports_keyboard_enhancement().unwrap_or(false)
        && ratatui::crossterm::execute!(std::io::stdout(), PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)).is_ok();
    ENHANCED.store(enhanced, std::sync::atomic::Ordering::Relaxed);
    // The mouse to click and drag the border of the pages; a paste as one text
    let _ = ratatui::crossterm::execute!(std::io::stdout(), EnableMouseCapture, EnableBracketedPaste);
    terminal
}

fn restore() {
    // Sent to a terminal without the protocol, it would be printed
    if ENHANCED.swap(false, std::sync::atomic::Ordering::Relaxed) {
        let _ = ratatui::crossterm::execute!(std::io::stdout(), PopKeyboardEnhancementFlags);
    }
    let _ = ratatui::crossterm::execute!(std::io::stdout(), DisableMouseCapture, DisableBracketedPaste);
    ratatui::restore();
}

/// The line of help, the window's shortcuts for what the terminal does: among
/// the pages, or (`editing`) in the editor.
fn help(kb: &Keybindings, editing: bool) -> String {
    let first = |id: &str| kb.app.get(id).or_else(|| kb.editor.get(id))
        .and_then(|b| b.iter().find(|b| !b.is_empty())).map(|b| keys::display(b));
    let (mut parts, ids): (Vec<String>, &[(&str, &str)]) = if editing {
        (vec!["Esc pages".into()], &[("show_shortcuts", "shortcuts"), ("save", "save"), ("find", "search"), ("open_link", "follow"), ("undo", "undo"), ("page_back", "back"), ("quit", "quit")])
    } else {
        (vec!["↑↓ pages".into(), "Enter edit".into(), "Tab links".into()],
         &[("show_shortcuts", "shortcuts"), ("quick_open", "find"), ("find", "search"), ("new_page", "new"), ("journal", "today"), ("open_link", "follow"), ("page_back", "back"), ("quit", "quit")])
    };
    for (id, what) in ids {
        if let Some(keys) = first(id) { parts.push(format!("{keys} {what}")); }
    }
    parts.join("  ")
}

/// What has the keys besides the list of the pages and the page edited: one
/// thing at a time, each getting the keys before the shortcuts.
pub(super) enum Mode {
    /// The list of the pages, or the page edited.
    Normal,
    /// A frame of a column (TODO, tags, backlinks, broken links).
    Frame(orgamnesia_core::Frame),
    /// Text or a list over the page (the shortcuts, a panel, the history…).
    Popup(commands::Popup),
    /// A value typed at the bottom (a link, a date, a tag, a search…).
    Prompt(commands::Prompt),
    /// The quick open.
    Quick(commands::Quick),
    /// The names completing what is typed at the cursor (a link, a tag).
    Suggest(complete::Suggest),
    /// The page's replacement going on: what, with what.
    Replace(String, String),
}

struct App {
    project: Project,
    prefs: Prefs,
    kb: Keybindings,
    chord: keys::Chord,
    /// The lines of help among the pages and in the editor.
    help: [String; 2],
    filter: String,
    list: ListState,
    /// The page being edited, in the place of the view of the page chosen.
    editor: Option<Editor>,
    /// The link of the page chosen (with Tab), by its index in the page.
    link: Option<usize>,
    /// Lines of the page above the view, and the size of the view.
    scroll: u16,
    height: u16,
    width: u16,
    /// Width of the column of the pages (`None`: 30 % of the screen), of the
    /// screen, and whether its border is being dragged.
    split: Option<u16>,
    total: u16,
    /// The border being dragged: of the left column (`false`), of the right one (`true`).
    dragging: Option<bool>,
    /// Width of the right column of frames (`None`: a fifth of the screen), and where it starts.
    right_split: Option<u16>,
    right_x: u16,
    /// Where the list of the pages is drawn, to find the page clicked.
    list_area: Rect,
    /// The pages left by following links, and those left by going back.
    back: Vec<String>,
    forward: Vec<String>,
    status: String,
    /// Zen mode: no column, only the page and the way out.
    zen: bool,
    /// The tip of the "Did you know?" frame (one more at each click on it).
    tip: usize,
    /// Where the page is drawn, to put the cursor where it is clicked; where
    /// the other window is.
    page_area: Rect,
    other_area: Rect,
    /// The screen split (Emacs' `C-x 2`, `C-x 3`): the other window, its page
    /// edited too, which way the screen is split, which half the active window is.
    other: Option<Editor>,
    split_dir: Option<Direction>,
    active_slot: usize,
    /// The quick capture is open in the other window; `true` when it split the screen itself.
    capture: Option<bool>,
    /// The folders of the list folded (see `orgamnesia_core::tree`); the journal's at first.
    closed: std::collections::HashSet<String>,
    /// The pages opened in the editor, in the order they were.
    tabs: Vec<String>,
    /// A search of the project: what, and the pages holding it (with how many times).
    hits: Option<(String, Vec<(String, usize)>)>,
    /// What has the keys (see `Mode`).
    mode: Mode,
    /// The line chosen in each frame of a column, and where each is drawn (for the mouse).
    frame_state: std::collections::HashMap<orgamnesia_core::Frame, ListState>,
    frame_areas: Vec<(orgamnesia_core::Frame, Rect)>,
    /// The right column is drawn (the screen is wide enough).
    right_column: bool,
    /// The pages and windows open when quitting, for the next start.
    last_session: Option<orgamnesia_core::Session>,
    /// The git state of the project shown at the bottom (`git_status_enabled`).
    git: Option<orgamnesia_core::GitStatus>,
    /// The projects of the settings, and the folder of the settings.
    projects: Vec<String>,
    config_dir: Option<PathBuf>,
    /// The folders of the pages, read from the disk again only when they may have
    /// changed (see `folders_changed`), not at each drawing.
    folders: Vec<String>,
    /// The pages open with unsaved changes that another program changed on disk:
    /// which text to keep is asked, one after the other (see `disk_changes`).
    disk_conflicts: Vec<String>,
    /// Watches the project's folder (see `watch_disk`): set when a page changed
    /// there. `None` (not watched): the folder is read again at each pause.
    disk_watch: Option<(crate::watch::Watcher, std::sync::Arc<std::sync::atomic::AtomicBool>)>,
}

impl App {
    fn new(project: Project, prefs: Prefs, kb: Keybindings) -> Self {
        let folders = project.folders();
        let mut app = Self {
            project, prefs, help: [help(&kb, false), help(&kb, true)], kb, chord: keys::Chord::default(),
            filter: String::new(), list: ListState::default(), editor: None, link: None,
            scroll: 0, height: 0, width: 0, split: None, total: 0, dragging: None, right_split: None, right_x: 0, list_area: Rect::default(), back: vec![], forward: vec![], status: String::new(), zen: false, tip: clock_nanos() as usize,
            page_area: Rect::default(), other_area: Rect::default(), other: None, split_dir: None, active_slot: 0, capture: None, closed: [orgamnesia_core::tree::JOURNAL.to_string()].into(), tabs: vec![], hits: None,
            projects: vec![], config_dir: None, git: None, last_session: None, folders, disk_conflicts: vec![], disk_watch: None,
            mode: Mode::Normal, frame_state: Default::default(), frame_areas: vec![], right_column: true,
        };
        app.list.select_first();
        app
    }

    /// The pages whose name holds the filter, ignoring case.
    fn shown(&self) -> Vec<FileEntry> {
        let filter = self.filter.to_lowercase();
        let in_hits = |f: &FileEntry| self.hits.as_ref().is_none_or(|(_, h)| h.iter().any(|(p, _)| *p == f.path));
        let mut pages: Vec<FileEntry> = self.project.files().into_iter()
            .filter(|f| f.name.to_lowercase().contains(&filter) && in_hits(f)).collect();
        pages.sort_by_cached_key(|f| f.name.to_lowercase());
        pages
    }

    /// The lines of the list: the tree of the folders and pages (see
    /// `orgamnesia_core::tree`), filtered; the pages found alone after a search.
    fn rows(&self) -> Vec<orgamnesia_core::tree::Row> {
        use orgamnesia_core::tree::{rows, Row};
        if self.hits.is_some() {
            return self.shown().into_iter().map(|f| Row::Page { name: f.name.clone(), file: f, depth: 0 }).collect();
        }
        rows(&self.project.files(), &self.folders, Some("Journal"), &self.closed, &self.filter)
    }

    /// Watch the open project's folder, so that `disk_changes` only reads it
    /// again when a page changed there.
    fn watch_disk(&mut self) {
        use std::sync::{atomic::{AtomicBool, Ordering}, Arc};
        let changed = Arc::new(AtomicBool::new(false));
        let flag = changed.clone();
        self.disk_watch = crate::watch::start(Path::new(&self.project.path), move |events| {
            if crate::watch::touched(events.iter().map(|e| e.path.as_path())).0 { flag.store(true, Ordering::Relaxed); }
        }).ok().map(|w| (w, changed));
    }

    /// The folders of the pages read again, after what may have made, moved or
    /// deleted one (a page created in a folder, a rename, a pull…).
    fn folders_changed(&mut self) {
        self.folders = self.project.folders();
    }

    /// The page chosen in the list (none on a folder).
    fn selected(&self) -> Option<FileEntry> {
        match self.list.selected().and_then(|i| self.rows().into_iter().nth(i)) {
            Some(orgamnesia_core::tree::Row::Page { file, .. }) => Some(file),
            _ => None,
        }
    }

    /// The folder chosen in the list, or the one of the page chosen (`None`: at the top).
    fn selected_folder(&self) -> Option<String> {
        match self.list.selected().and_then(|i| self.rows().into_iter().nth(i)) {
            Some(orgamnesia_core::tree::Row::Folder { key, .. }) if key != orgamnesia_core::tree::JOURNAL => Some(key),
            Some(orgamnesia_core::tree::Row::Page { file, .. }) => file.name.rsplit_once('/').map(|(d, _)| d.to_string()),
            _ => None,
        }
    }

    /// The folder chosen in the list folded or unfolded (`open`: `None` turns it over).
    fn fold_folder(&mut self, open: Option<bool>) -> bool {
        let Some(orgamnesia_core::tree::Row::Folder { key, open: is_open, .. }) = self.list.selected().and_then(|i| self.rows().into_iter().nth(i)) else { return false };
        if open.unwrap_or(!is_open) { self.closed.remove(&key); } else { self.closed.insert(key); }
        true
    }

    /// The page in view: the one edited, else the one chosen.
    fn current_path(&self) -> Option<String> {
        self.editor.as_ref().map(|e| e.path.clone()).or_else(|| self.selected().map(|p| p.path))
    }

    fn resolver(&self) -> Resolver<'static> {
        Resolver::new(self.project.files(), self.prefs.case_insensitive_links, self.prefs.formats().default)
    }

    /// How the page `path` is read and drawn, `broken` telling its dead links.
    fn draw_of<'a>(&self, path: &str, todo: Option<&'a orgamnesia_core::TodoKeywords>, broken: &'a dyn Fn(&str) -> bool) -> Draw<'a> {
        Draw { format: orgamnesia_core::Format::of_page(path), tags: self.prefs.tags(), todo, broken, hide_stars: self.prefs.hide_leading_stars,
            live: None, text_first: self.prefs.link_text_first }
    }

    /// The links of the page in view (as edited).
    fn page_links(&self) -> Vec<highlight::Link> {
        let Some(path) = self.current_path() else { return vec![] };
        let text = match &self.editor {
            Some(e) => e.text(),
            None => self.project.read(&path).unwrap_or_default(),
        };
        highlight::links(&text, &self.draw_of(&path, None, &|_| false))
    }

    /// Another page chosen: no link of it chosen yet, seen from its top.
    fn moved(&mut self) {
        self.link = None;
        self.scroll = 0;
    }

    /// Choose the page `path`, the filter cleared.
    fn select_path(&mut self, path: &str) {
        self.filter.clear();
        // Its folders unfolded
        let name = self.project.name_of(path);
        let mut at = 0;
        while let Some(i) = name[at..].find('/') { self.closed.remove(&name[..at + i]); at += i + 1; }
        if orgamnesia_core::tree::is_day(&name) { self.closed.remove(orgamnesia_core::tree::JOURNAL); }
        let i = self.rows().iter().position(|r| matches!(r, orgamnesia_core::tree::Row::Page { file, .. } if file.path == path));
        self.list.select(i);
        self.moved();
    }

    /// Write the page edited, if changed; false when it could not be (the
    /// editor then stays, its text kept).
    fn save(&mut self) -> bool {
        let Some(e) = self.editor.as_mut().filter(|e| e.dirty) else { return true };
        let written = if e.page { self.project.write(&e.path, e.text(), self.prefs.tags()) } else { crate::write_atomic(&e.path, e.text()).map_err(Into::into) };
        match written {
            Ok(()) => {
                e.dirty = false;
                self.status = "saved".into();
                if let Some(o) = &mut self.other && o.path == e.path { o.sync_from(e); }
                true
            }
            Err(err) => {
                self.status = format!("not saved: {err}");
                // Changed by another program: which text to keep is asked
                if matches!(err, crate::error::AppError::ChangedOnDisk(_)) { self.disk_changes(true); }
                false
            }
        }
    }

    /// Leave the editor, the page saved; false when it could not be.
    fn close(&mut self) -> bool {
        if !self.save() || !self.save_other() { return false; }
        self.editor = None;
        self.single();
        true
    }

    /// Back to the list of the pages, the pages edited saved; the screen stays
    /// split, the page chosen then opened in the active window. False when not saved.
    fn back_to_list(&mut self) -> bool {
        if !self.save() || !self.save_other() { return false; }
        self.editor = None;
        true
    }

    /// The previewed page edited first, so that the window left keeps it.
    fn edit_previewed(&mut self) {
        if self.editor.is_none() && let Some(p) = self.selected() { self.edit(&p.path); }
    }

    /// Write the page of the other window, if it is another page and changed.
    fn save_other(&mut self) -> bool {
        let same = matches!((&self.editor, &self.other), (Some(a), Some(b)) if a.path == b.path);
        if same || !self.other.as_ref().is_some_and(|o| o.dirty) { return true; }
        self.swap();
        let saved = self.save();
        self.swap();
        saved
    }

    /// The other window becomes the active one; on the same page, with its text.
    fn swap(&mut self) {
        std::mem::swap(&mut self.editor, &mut self.other);
        self.active_slot ^= 1;
        if let (Some(a), Some(b)) = (&mut self.editor, &self.other) && a.path == b.path { a.sync_from(b); }
    }

    /// The screen split `dir` (the page in both halves), else turned the other way.
    fn split(&mut self, dir: Direction) {
        if self.editor.is_none() && let Some(p) = self.selected() { self.edit(&p.path); }
        let Some(e) = &self.editor else { return };
        if self.other.is_none() { self.other = Some(e.twin()); }
        self.split_dir = Some(dir);
    }

    /// One window only: the other one closed (its page saved).
    fn single(&mut self) {
        self.other = None;
        self.split_dir = None;
        self.active_slot = 0;
    }

    /// Edit the page `path` (the one edited saved first).
    fn edit(&mut self, path: &str) {
        if !self.save() { return; }
        match self.project.read(path) {
            Ok(text) => {
                let query = self.hits.as_ref().map(|(q, _)| q.clone());
                self.select_path(path);
                let mut e = Editor::new(path.to_string(), &text, true);
                e.prefs = editor::EditPrefs::from(&self.prefs);
                // Opened from a search of the project: on its first match
                if let Some(q) = query { e.search_for(&q); }
                self.editor = Some(e);
                if !self.tabs.iter().any(|t| t == path) { self.tabs.push(path.to_string()); }
            }
            Err(e) => self.status = e.to_string(),
        }
    }

    /// Go to the page `path`: edited when the editor is open, else chosen.
    fn show(&mut self, path: &str) {
        if self.editor.is_some() { self.edit(path); } else { self.select_path(path); }
    }

    fn run(&mut self, terminal: &mut DefaultTerminal) -> std::io::Result<()> {
        let minutes = |n: u64| std::time::Duration::from_secs(n * 60);
        let (mut synced, mut fetched, mut looked) = (std::time::Instant::now(), None::<std::time::Instant>, None::<std::time::Instant>);
        loop {
            terminal.draw(|f| self.draw(f))?;
            if event::poll(std::time::Duration::from_secs(1))? {
                // Between keys only: what runs git waits for a pause
            } else {
                // The git sync every so many minutes (git_auto_sync_minutes)
                if self.prefs.git_auto_sync > 0 && synced.elapsed() >= minutes(self.prefs.git_auto_sync) {
                    synced = std::time::Instant::now();
                    self.sync();
                    looked = None;
                }
                // The remote fetched when the project opens, then every so many minutes (git_fetch_minutes)
                if self.prefs.git_ext && self.prefs.git_fetch > 0 && fetched.is_none_or(|t| t.elapsed() >= minutes(self.prefs.git_fetch)) {
                    fetched = Some(std::time::Instant::now());
                    if self.git.as_ref().is_some_and(|g| g.remote) { let _ = crate::git::fetch(&self.project.path, &self.prefs.git_ssh_key); }
                    looked = None;
                }
                // The pages changed on disk by another program (a sync tool, the window)
                self.disk_changes(false);
                // The state at the bottom looked at again every few seconds
                if looked.is_none_or(|t| t.elapsed().as_secs() >= 5) {
                    looked = Some(std::time::Instant::now());
                    self.look_at_git();
                }
                continue;
            }
            let quit = match event::read()? {
                Event::Key(key) if key.kind != KeyEventKind::Release => self.key(key),
                Event::Mouse(m) => { self.mouse(m); false }
                Event::Paste(text) => { self.paste(&text.replace("\r\n", "\n").replace('\r', "\n")); false }
                _ => false,
            };
            self.after_event();
            // Quitting saves the page edited first, and waits while it can't be; the
            // pages and windows open kept for the next start first
            if quit {
                let session = self.session();
                if self.close() { self.last_session = session; return Ok(()); }
            }
        }
    }

    /// The git state of the project, when the settings show it.
    pub(super) fn look_at_git(&mut self) {
        self.git = self.prefs.git_ext.then(|| crate::git::status(&self.project.path)).flatten().filter(|g| g.repo);
    }

    /// Open again the pages and windows of `session`, when it is this project's
    /// (the pages deleted since left out), as the window does.
    fn restore_session(&mut self, session: orgamnesia_core::Session) {
        let same = |a: &str, b: &str| a.trim_end_matches('/') == b.trim_end_matches('/');
        if !same(&crate::slash(&session.project), &self.project.path) { return; }
        let files = self.project.files();
        let exists = |p: &Option<String>| p.clone().filter(|p| files.iter().any(|f| f.path == *p));
        self.tabs = session.tabs.iter().filter(|t| files.iter().any(|f| f.path == **t)).cloned().collect();
        let (Some(first), second) = (exists(&session.active), exists(&session.other)) else { return };
        // The window with the focus is the one edited, the other one beside it
        let (focused, beside) = match (&second, session.focus_second) {
            (Some(second), true) => (second.clone(), Some(first)),
            _ => (first, second),
        };
        self.edit(&focused);
        let (Some(split), Some(beside)) = (session.split, beside) else { return };
        if let Ok(text) = self.project.read(&beside) {
            let mut e = Editor::new(beside, &text, true);
            e.prefs = editor::EditPrefs::from(&self.prefs);
            self.other = Some(e);
            self.split_dir = Some(match split {
                orgamnesia_core::SplitKind::Vertical => Direction::Horizontal,
                orgamnesia_core::SplitKind::Horizontal => Direction::Vertical,
            });
            self.active_slot = session.focus_second as usize;
        }
    }

    /// The pages and windows open, as the window saves them on quit.
    fn session(&self) -> Option<orgamnesia_core::Session> {
        let (first, second) = if self.active_slot == 0 { (&self.editor, &self.other) } else { (&self.other, &self.editor) };
        let page = |e: &Option<Editor>| e.as_ref().filter(|e| e.page).map(|e| e.path.clone());
        Some(orgamnesia_core::Session {
            project: self.project.path.clone(),
            tabs: self.tabs.clone(),
            active: page(first),
            split: self.split_dir.filter(|_| self.other.is_some()).map(|d| match d {
                Direction::Horizontal => orgamnesia_core::SplitKind::Vertical,
                Direction::Vertical => orgamnesia_core::SplitKind::Horizontal,
            }),
            other: page(second),
            focus_second: self.active_slot == 1 && self.other.is_some(),
        })
    }

    pub(super) fn popup(&self) -> Option<&commands::Popup> {
        if let Mode::Popup(p) = &self.mode { Some(p) } else { None }
    }

    #[cfg(test)]
    pub(super) fn prompt(&self) -> Option<&commands::Prompt> {
        if let Mode::Prompt(p) = &self.mode { Some(p) } else { None }
    }

    #[cfg(test)]
    pub(super) fn prompt_mut(&mut self) -> Option<&mut commands::Prompt> {
        if let Mode::Prompt(p) = &mut self.mode { Some(p) } else { None }
    }

    pub(super) fn suggest(&self) -> Option<&complete::Suggest> {
        if let Mode::Suggest(s) = &self.mode { Some(s) } else { None }
    }

    pub(super) fn frame_focus(&self) -> Option<orgamnesia_core::Frame> {
        if let Mode::Frame(f) = self.mode { Some(f) } else { None }
    }

    /// Back to the list or the page: what had the keys, given back.
    pub(super) fn normal(&mut self) -> Mode {
        std::mem::replace(&mut self.mode, Mode::Normal)
    }

    /// Whether a column of the screen shows the frame `frame` (as the window's layout puts it).
    pub(super) fn frame_shown_here(&self, frame: orgamnesia_core::Frame) -> bool {
        self.prefs.column(false).contains(&frame) || (self.right_column && self.prefs.column(true).contains(&frame))
    }

    /// The focus on the frame `frame`, on its first line leading somewhere.
    pub(super) fn focus_frame(&mut self, frame: orgamnesia_core::Frame) {
        let (_, items) = self.frame_items(frame);
        let first = items.iter().position(|(_, t)| *t != commands::Target::None);
        let state = self.frame_state.entry(frame).or_default();
        if state.selected().is_none_or(|i| i >= items.len()) { state.select(first); }
        self.mode = Mode::Frame(frame);
    }

    /// After a key or a click: what the editor copied to the clipboard, what
    /// it says, and the page saved when the settings say so (`autosave`).
    fn after_event(&mut self) {
        if let Some(e) = &mut self.editor {
            if let Some(text) = e.clipboard.take() { commands::to_clipboard(&text); }
            if let Some(m) = e.message.take() { self.status = m; }
        }
        // Not while a page changed on disk waits for its answer: saving would choose for it
        let waiting = !self.disk_conflicts.is_empty() || matches!(&self.mode, Mode::Prompt(p) if p.asks_disk_conflict());
        if self.prefs.autosave && !waiting && self.editor.as_ref().is_some_and(|e| e.dirty) {
            let status = std::mem::take(&mut self.status);
            if self.save() { self.status = status; }
        }
    }

    /// Escape: leave the link chosen, else the filter; with neither, quit.
    fn cancel(&mut self) -> bool {
        if self.link.is_some() { self.link = None; return false; }
        if !self.filter.is_empty() || self.hits.is_some() { self.filter.clear(); self.hits = None; self.list.select_first(); self.moved(); return false; }
        true
    }

    /// Choose the next link of the page (the previous one with `!forward`), in view.
    fn next_link(&mut self, forward: bool) {
        let links = self.page_links();
        if links.is_empty() { self.status = "no link in this page".into(); return; }
        let n = links.len();
        let i = match self.link {
            Some(i) if forward => (i + 1) % n,
            Some(i) => (i + n - 1) % n,
            None if forward => 0,
            None => n - 1,
        };
        self.link = Some(i);
        self.scroll = (links[i].line as u16).saturating_sub(self.height / 3);
        self.status = links[i].target.clone();
    }

    fn follow_chosen(&mut self) {
        if let Some(l) = self.link.and_then(|i| self.page_links().into_iter().nth(i)) { self.follow(&l.target); }
    }

    /// Follow the link under the cursor of the editor.
    fn follow_at_cursor(&mut self) {
        match self.link_at_cursor() {
            Some(target) => self.follow(&target),
            None => self.status = "no link under the cursor".into(),
        }
    }

    /// The target of the link under the cursor of the page, if any.
    fn link_at_cursor(&self) -> Option<String> {
        let (line, col) = self.editor.as_ref()?.line_col();
        self.page_links().into_iter().find(|l| l.line == line && (l.start..=l.end).contains(&col)).map(|l| l.target)
    }

    /// Follow the link `target` as the window does: an address or a file with the
    /// system's program, a page shown (made when there is none).
    fn follow(&mut self, target: &str) {
        let current = self.current_path();
        let outside = if target.contains("://") || target.starts_with("mailto:") {
            Some(target.to_string())
        } else {
            links::file_target(target).map(|f| {
                let f = crate::expand_home(f);
                let base = current.as_deref().and_then(|c| Path::new(c).parent().map(Path::to_path_buf)).unwrap_or_default();
                crate::slash(base.join(f))
            })
        };
        if let Some(outside) = outside {
            self.status = match crate::open_with_system(&outside) { Ok(()) => format!("opened {outside}"), Err(e) => e.to_string() };
            return;
        }
        // `[[Page::*Titre]]`: the page, at the headline; `[[*Titre]]`: this page's
        let (name, place) = links::split(target);
        if name.is_empty() {
            if let Some(place) = place {
                if self.editor.is_none() && let Some(path) = current.clone() { self.edit(&path); }
                self.goto_place(place);
            }
            return;
        }
        let file = match self.resolver().resolve(name).cloned() {
            Some(f) => f,
            None => match self.project.create(name, None, self.prefs.tags()) {
                Ok(f) => { self.folders_changed(); self.status = format!("{name}: page created"); f }
                Err(e) => { self.status = e.to_string(); return; }
            },
        };
        if !self.save() { return; }
        self.back.extend(current);
        self.forward.clear();
        match place {
            // A place is in the editor, where the cursor can go
            Some(place) => { self.edit(&file.path); self.goto_place(place); }
            None => self.show(&file.path),
        }
    }

    /// The cursor on the place `place` of the page edited: the headline of `*Titre`,
    /// the `<<cible>>` of `cible` (else its headline), as the window finds them.
    fn goto_place(&mut self, place: &str) {
        let todo = self.prefs.todo();
        let Some(e) = &mut self.editor else { return };
        match links::find(&e.text(), place, todo.as_ref(), e.format()) {
            Some((at, _)) => e.goto(at),
            None => self.status = format!("{place}: not found in this page"),
        }
    }

    /// Back to the page left by following a link (`forward`: the other way).
    fn go(&mut self, forward: bool) {
        if !self.save() { return; }
        let current = self.current_path();
        let (from, to) = if forward { (&mut self.forward, &mut self.back) } else { (&mut self.back, &mut self.forward) };
        let Some(path) = from.pop() else { return };
        to.extend(current);
        self.show(&path);
    }

    /// Quick capture, as the window does it (see `orgamnesia_core::capture`): the
    /// page notes go to (the day's, else `Inbox`, made if needed) in the other
    /// window, the screen split when it isn't, a new note at its end. Again, it
    /// closes (saved), back to the page edited.
    fn capture(&mut self) {
        if let Some(split_here) = self.capture.take() && self.other.is_some() {
            if !self.save() { self.capture = Some(split_here); return; }
            self.swap();
            if split_here { self.single(); }
            return;
        }
        use chrono::{Datelike, Timelike};
        let t = chrono::Local::now();
        let now = orgamnesia_core::templates::Now { year: t.year(), month: t.month(), day: t.day(), hour: t.hour(), minute: t.minute() };
        let name = orgamnesia_core::capture::page(self.prefs.journal, now);
        let existing = self.project.files().into_iter().find(|f| f.name.eq_ignore_ascii_case(&name));
        let page = match existing {
            Some(page) => page,
            None => match self.project.create(&name, None, self.prefs.tags()) {
                Ok(page) => { self.folders_changed(); page }
                Err(e) => { self.status = e.to_string(); return; }
            },
        };
        if self.editor.is_some() {
            let split_here = self.other.is_none();
            if split_here { self.split(Direction::Horizontal); }
            self.swap();
            self.capture = Some(split_here);
        }
        self.edit(&page.path);
        if let Some(e) = self.editor.as_mut().filter(|e| e.path == page.path) {
            let text = e.text();
            let end = text.chars().count();
            e.put(end, end, &orgamnesia_core::capture::entry(&text, now));
        }
        if let Some(keys) = self.shortcut("capture") { self.status = format!("capture: {keys} again to close"); }
    }

    fn journal(&mut self) {
        if !self.prefs.journal { self.status = "the journal extension is off (settings)".into(); return; }
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        self.open_or_create(today);
    }

    /// The first shortcut of the action `id`, as the terminal shows it.
    fn shortcut(&self, id: &str) -> Option<String> {
        self.kb.app.get(id).and_then(|b| b.iter().find(|b| !b.is_empty())).map(|b| keys::display(b))
    }

    /// Edit a page picked at random, other than the one edited.
    fn random_page(&mut self) {
        let r = clock_nanos() as f64 / 1e9;
        let files = self.project.files();
        let shown = self.editor.as_ref().map(|e| e.path.as_str());
        match orgamnesia_core::pages::random_page(&files, shown, r).map(|f| f.path.clone()) {
            Some(path) => self.edit(&path),
            None => self.status = "no page in this project yet".into(),
        }
    }

    /// Edit the page `name`, made first when there is none.
    fn open_or_create(&mut self, name: String) {
        if name.is_empty() { self.status = "type the name of the new page first".into(); return; }
        let existing = self.project.files().into_iter().find(|f| f.name.eq_ignore_ascii_case(&name));
        // The format of each new page asked, when the settings say so
        if existing.is_none() && self.prefs.ask_format && !self.prefs.single_format {
            self.ask_format(name);
            return;
        }
        let page = match existing {
            Some(page) => page,
            None => match self.project.create(&name, None, self.prefs.tags()) {
                Ok(page) => { self.folders_changed(); page }
                Err(e) => { self.status = e.to_string(); return; }
            },
        };
        self.edit(&page.path);
    }

}

/// shortcut: the clock's nanoseconds as the dice, enough to pick a page or a tip.
fn clock_nanos() -> u32 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos())
}
