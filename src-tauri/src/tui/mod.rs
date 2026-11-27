//! The terminal interface (beta), started by `orgamnesia -nw [folder]` (or `--tui`, `-t`): the
//! pages of the project, filtered as one types, drawn in colour beside the
//! list and edited there; their links followed; the shortcuts those of the
//! window (the keybinding profile in effect).

mod agenda;
mod commands;
mod complete;
mod quick;
mod editor;
mod frames;
mod git;
mod input;
mod view;
#[cfg(test)]
mod tests;
mod highlight;
mod images;
mod keys;
mod navigation;
mod pages;
mod panes;

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
    app.pointer = Some(false);
    // Asked once the screen is taken, before the keys are read; else half blocks
    if app.prefs.show_images { app.images.picker = Some(ratatui_image::picker::Picker::from_query_stdio().unwrap_or_else(|_| ratatui_image::picker::Picker::halfblocks())); }
    let result = app.run(&mut terminal);
    app.set_pointer(false);
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
    Popup(frames::Popup),
    /// A value typed at the bottom (a link, a date, a tag, a search…).
    Prompt(commands::Prompt),
    /// The quick open.
    Quick(quick::Quick),
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
    /// Width of the column of the pages (`None`: a fifth of the screen, 24 to 40 columns), of the
    /// screen, and whether its border is being dragged.
    split: Option<u16>,
    total: u16,
    /// The border being dragged: of the left column (`false`), of the right one (`true`).
    dragging: Option<bool>,
    /// The mouse pointer is the resize arrow; `None`: never changed (tests).
    pointer: Option<bool>,
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
    /// What the TODO frame shows (`open`, `done`, `undated`…, then a tag search).
    todo_filter: String,
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
    /// Where the bar of the pages opened is drawn (empty: none), to find the one clicked.
    tab_bar: Rect,
    images: images::Images,
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
            scroll: 0, height: 0, width: 0, split: None, total: 0, dragging: None, pointer: None, right_split: None, right_x: 0, list_area: Rect::default(), back: vec![], forward: vec![], status: String::new(), zen: false, tip: clock_nanos() as usize, todo_filter: String::new(),
            page_area: Rect::default(), other_area: Rect::default(), other: None, split_dir: None, active_slot: 0, capture: None, closed: [orgamnesia_core::tree::JOURNAL.to_string()].into(), tabs: vec![], tab_bar: Rect::default(), images: Default::default(), hits: None,
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

    pub(super) fn popup(&self) -> Option<&frames::Popup> {
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
        let first = items.iter().position(|(_, t)| *t != frames::Target::None);
        let state = self.frame_state.entry(frame).or_default();
        if state.selected().is_none_or(|i| i >= items.len()) { state.select(first); }
        self.mode = Mode::Frame(frame);
    }
}

/// shortcut: the clock's nanoseconds as the dice, enough to pick a page or a tip.
fn clock_nanos() -> u32 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos())
}
