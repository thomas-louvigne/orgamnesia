//! The terminal interface (beta), started by `orgamnesia --nox [folder]`: the
//! pages of the project, filtered as one types, drawn in colour beside the
//! list and edited there; their links followed; the shortcuts those of the
//! window (the keybinding profile in effect).

mod commands;
mod complete;
mod editor;
mod highlight;
mod keys;

use std::path::{Path, PathBuf};

use orgamnesia_core::{keybindings::Keybindings, links, pages::Resolver, FileEntry, Prefs};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::crossterm::event::{DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture, MouseButton, MouseEvent, MouseEventKind};
use ratatui::crossterm::event::{KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags};
use ratatui::layout::{Constraint, Direction, Layout, Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListState, Paragraph, Wrap};
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
        .ok_or("no project: give its folder, `orgamnesia --nox ~/notes`")?;
    let project = Project::open(&path, prefs.tags(), prefs.formats(), &prefs.journal_folder).map_err(|e| e.to_string())?;
    prune_trash(&project, &prefs);
    let mut app = App::new(project, prefs, kb);
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
    crate::project::files::delete_blank_pages(&app.project.path, p.formats(), &p.journal_folder, p.delete_empty, p.delete_title_only);
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
    /// Where the page is drawn, to put the cursor where it is clicked; where
    /// the other window is.
    page_area: Rect,
    other_area: Rect,
    /// The screen split (Emacs' `C-x 2`, `C-x 3`): the other window, its page
    /// edited too, which way the screen is split, which half the active window is.
    other: Option<Editor>,
    split_dir: Option<Direction>,
    active_slot: usize,
    /// The pages opened in the editor, in the order they were.
    tabs: Vec<String>,
    /// A value being typed at the bottom, the text shown over the page.
    prompt: Option<commands::Prompt>,
    quick: Option<commands::Quick>,
    popup: Option<commands::Popup>,
    /// The page's replacement going on: what, with what.
    replacing: Option<(String, String)>,
    /// A search of the project: what, and the pages holding it (with how many times).
    hits: Option<(String, Vec<(String, usize)>)>,
    /// The frame of a column with the focus (TODO, tags, backlinks, broken links),
    /// the line chosen in each, and where each is drawn (for the mouse).
    frame_focus: Option<orgamnesia_core::Frame>,
    frame_state: std::collections::HashMap<orgamnesia_core::Frame, ListState>,
    frame_areas: Vec<(orgamnesia_core::Frame, Rect)>,
    /// The right column is drawn (the screen is wide enough).
    right_column: bool,
    /// The names completing what is typed at the cursor (a link, a tag).
    suggest: Option<complete::Suggest>,
    /// The pages and windows open when quitting, for the next start.
    last_session: Option<orgamnesia_core::Session>,
    /// The git state of the project shown at the bottom (`git_status_enabled`).
    git: Option<orgamnesia_core::GitStatus>,
    /// The projects of the settings, and the folder of the settings.
    projects: Vec<String>,
    config_dir: Option<PathBuf>,
}

impl App {
    fn new(project: Project, prefs: Prefs, kb: Keybindings) -> Self {
        let mut app = Self {
            project, prefs, help: [help(&kb, false), help(&kb, true)], kb, chord: keys::Chord::default(),
            filter: String::new(), list: ListState::default(), editor: None, link: None,
            scroll: 0, height: 0, width: 0, split: None, total: 0, dragging: None, right_split: None, right_x: 0, list_area: Rect::default(), back: vec![], forward: vec![], status: String::new(),
            page_area: Rect::default(), other_area: Rect::default(), other: None, split_dir: None, active_slot: 0, tabs: vec![], prompt: None, quick: None, popup: None, replacing: None, hits: None,
            projects: vec![], config_dir: None, git: None, last_session: None,
            frame_focus: None, frame_state: Default::default(), frame_areas: vec![], right_column: true, suggest: None,
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

    fn selected(&self) -> Option<FileEntry> {
        self.list.selected().and_then(|i| self.shown().into_iter().nth(i))
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
        let i = self.shown().iter().position(|f| f.path == path);
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
            Err(err) => { self.status = format!("not saved: {err}"); false }
        }
    }

    /// Leave the editor, the page saved; false when it could not be.
    fn close(&mut self) -> bool {
        if !self.save() || !self.save_other() { return false; }
        self.editor = None;
        self.single();
        true
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
        self.frame_focus = Some(frame);
    }

    /// A key on the frame with the focus: moving, opening; another key leaves it.
    /// True when the key was the frame's.
    fn frame_key(&mut self, key: KeyEvent) -> bool {
        let Some(frame) = self.frame_focus else { return false };
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let (_, items) = self.frame_items(frame);
        let state = self.frame_state.entry(frame).or_default();
        let step = |state: &mut ListState, forward: bool| {
            let n = items.len();
            let mut i = state.selected().unwrap_or(0);
            for _ in 0..n {
                i = if forward { (i + 1).min(n.saturating_sub(1)) } else { i.saturating_sub(1) };
                if items[i].1 != commands::Target::None { state.select(Some(i)); return; }
            }
        };
        match key.code {
            KeyCode::Down => step(state, true),
            KeyCode::Up => step(state, false),
            KeyCode::Char('n') if ctrl => step(state, true),
            KeyCode::Char('p') if ctrl => step(state, false),
            KeyCode::Enter => if let Some((_, target)) = state.selected().and_then(|i| items.get(i)).cloned() { self.open(target); },
            KeyCode::Esc => self.frame_focus = None,
            _ => { self.frame_focus = None; return false; }
        }
        true
    }

    /// After a key or a click: what the editor copied to the clipboard, what
    /// it says, and the page saved when the settings say so (`autosave`).
    fn after_event(&mut self) {
        if let Some(e) = &mut self.editor {
            if let Some(text) = e.clipboard.take() { commands::to_clipboard(&text); }
            if let Some(m) = e.message.take() { self.status = m; }
        }
        if self.prefs.autosave && self.editor.as_ref().is_some_and(|e| e.dirty) {
            let status = std::mem::take(&mut self.status);
            if self.save() { self.status = status; }
        }
    }

    /// Width of the column of the pages, for a screen `total` wide: the page keeps 20 columns.
    fn split_width(&self, total: u16) -> u16 {
        self.split.unwrap_or(total * 3 / 10).min(total.saturating_sub(20)).max(8)
    }

    /// The border between the pages and the page, dragged with the left button.
    fn mouse(&mut self, m: MouseEvent) {
        let border = self.split_width(self.total);
        match m.kind {
            // Either of the two lines of the border: the list's right one, the page's left one
            MouseEventKind::Down(MouseButton::Left) if m.column + 1 == border || m.column == border => self.dragging = Some(false),
            // The right column's border: its left line, or the page's right one
            MouseEventKind::Down(MouseButton::Left) if self.right_column && (m.column == self.right_x || m.column + 1 == self.right_x) => self.dragging = Some(true),
            MouseEventKind::Down(MouseButton::Left) if self.popup.as_ref().is_some_and(|p| !p.targets.is_empty()) => self.click_popup(m.row),
            MouseEventKind::Down(MouseButton::Left) if self.click_frame(m.column, m.row, None) => {}
            MouseEventKind::Down(MouseButton::Left) => {
                self.frame_focus = None;
                self.click_list(m.column, m.row);
                // Ctrl+click on a link follows it, as in the window
                if self.click_page(m.column, m.row) && m.modifiers.contains(KeyModifiers::CONTROL) { self.follow_at_cursor(); }
            }
            MouseEventKind::Drag(MouseButton::Left) if self.dragging == Some(false) => self.split = Some(m.column + 1),
            MouseEventKind::Drag(MouseButton::Left) if self.dragging == Some(true) => self.right_split = Some(self.total.saturating_sub(m.column)),
            MouseEventKind::ScrollDown => self.wheel(m.column, m.row, 3),
            MouseEventKind::ScrollUp => self.wheel(m.column, m.row, -3),
            MouseEventKind::Up(_) => self.dragging = None,
            _ => {}
        }
    }

    /// A click on a frame of a column (`wheel`: the wheel turned so many lines over it):
    /// the focus on it, the line clicked opened. False when (`column`, `row`) is in none.
    fn click_frame(&mut self, column: u16, row: u16, wheel: Option<isize>) -> bool {
        use orgamnesia_core::Frame;
        let inside = |a: &Rect| column >= a.x && column < a.x + a.width && row >= a.y && row < a.y + a.height;
        let Some((frame, a)) = self.frame_areas.iter().find(|(_, a)| inside(a)).copied() else { return false };
        // The pages have their own clicks; the lines of the project and git nothing
        if frame == Frame::Pages { return false; }
        if !matches!(frame, Frame::Todos | Frame::Tags | Frame::Backlinks | Frame::Broken) { return true; }
        self.focus_frame(frame);
        let (_, items) = self.frame_items(frame);
        let state = self.frame_state.entry(frame).or_default();
        if let Some(n) = wheel {
            let last = items.len().saturating_sub(1) as isize;
            state.select(Some((state.selected().unwrap_or(0) as isize + n).clamp(0, last) as usize));
            return true;
        }
        if row <= a.y || row + 1 >= a.y + a.height { return true; }
        let i = state.offset() + (row - a.y - 1) as usize;
        if let Some((_, target)) = items.get(i).cloned() && target != commands::Target::None {
            state.select(Some(i));
            self.open(target);
        }
        true
    }

    /// The frame `frame` of a column drawn in `area`.
    fn draw_frame(&mut self, f: &mut ratatui::Frame, frame: orgamnesia_core::Frame, area: Rect) {
        use orgamnesia_core::Frame;
        let focused = self.frame_focus == Some(frame);
        let cyan = Style::new().fg(ratatui::style::Color::Cyan);
        match frame {
            Frame::Pages => {
                let pages = self.shown();
                let count = |p: &FileEntry| self.hits.as_ref().and_then(|(_, h)| h.iter().find(|(path, _)| *path == p.path)).map(|(_, n)| format!(" ({n})"));
                let names: Vec<String> = pages.iter().map(|p| format!("{}{}", p.name, count(p).unwrap_or_default())).collect();
                let title = match (&self.hits, self.filter.is_empty()) {
                    (Some((q, _)), _) => format!(" “{q}” "),
                    (None, true) => " Pages ".to_string(),
                    (None, false) => format!(" /{} ", self.filter),
                };
                // The list has the keys when no page is edited (nor a frame chosen)
                let keys = self.editor.is_none() && self.frame_focus.is_none();
                let list = List::new(names).block(Block::bordered().title(frame_title(frame, title)).border_style(if keys { cyan } else { Style::new() }))
                    .highlight_style(Style::new().add_modifier(Modifier::REVERSED));
                self.list_area = area;
                f.render_stateful_widget(list, area, &mut self.list);
            }
            Frame::Project => {
                let name = crate::project::files::page_name(&self.project.path);
                f.render_widget(Paragraph::new(format!(" {name}")).block(Block::bordered().title(frame_title(frame, " Project ".into()))), area);
            }
            Frame::Git => {
                let line = self.git.as_ref().map(git_line).unwrap_or_else(|| " not under git".into());
                f.render_widget(Paragraph::new(line).block(Block::bordered().title(frame_title(frame, " Git ".into()))), area);
            }
            _ => {
                let (title, items) = self.frame_items(frame);
                let lines: Vec<Line> = items.iter().map(|(l, t)| {
                    let s = if *t == commands::Target::None { Style::new().add_modifier(Modifier::BOLD) } else { Style::new() };
                    Line::from(l.clone()).style(s)
                }).collect();
                let lines = if lines.is_empty() { vec![Line::from("(none)").style(Style::new().fg(ratatui::style::Color::DarkGray))] } else { lines };
                let list = List::new(lines)
                    .block(Block::bordered().title(frame_title(frame, format!(" {title} "))).border_style(if focused { cyan } else { Style::new() }))
                    .highlight_style(if focused { Style::new().add_modifier(Modifier::REVERSED) } else { Style::new() });
                let state = self.frame_state.entry(frame).or_default();
                f.render_stateful_widget(list, area, state);
            }
        }
    }

    /// A click on a line of a popup list: that line opened.
    fn click_popup(&mut self, row: u16) {
        let a = self.page_area_popup();
        let Some(p) = &mut self.popup else { return };
        if row <= a.y || row + 1 >= a.y + a.height { self.popup = None; return; }
        let i = p.scroll as usize + (row - a.y - 1) as usize;
        if p.targets.get(i).is_some_and(|t| *t != commands::Target::None) {
            p.selected = i;
            self.open_target();
        }
    }

    /// Where the popup is drawn: over the page, both halves of a split screen.
    fn page_area_popup(&self) -> Rect {
        if self.other.is_some() { self.page_area.union(self.other_area) } else { self.page_area }
    }

    /// A click in the list of the pages: back to it (the page edited saved),
    /// on the page clicked.
    fn click_list(&mut self, column: u16, row: u16) {
        let a = self.list_area;
        // Inside its borders
        if column <= a.x || column + 1 >= a.x + a.width || row <= a.y || row + 1 >= a.y + a.height { return; }
        if !self.close() { return; }
        let i = self.list.offset() + (row - a.y - 1) as usize;
        if i < self.shown().len() && self.list.selected() != Some(i) {
            self.list.select(Some(i));
            self.moved();
        }
    }

    /// A click in the page: the cursor there (the page edited first when it is not).
    /// The mouse wheel turned `n` lines over (`column`, `row`): the window
    /// there scrolls, or the list of the pages moves.
    fn wheel(&mut self, column: u16, row: u16, n: isize) {
        let inside = |a: Rect| column >= a.x && column < a.x + a.width && row >= a.y && row < a.y + a.height;
        let height = self.height as usize;
        if let Some(p) = &mut self.popup {
            p.scroll = (p.scroll as isize + n).max(0) as u16;
        } else if inside(self.page_area) {
            match &mut self.editor {
                Some(e) => e.scroll(n, height),
                None => self.scroll = (self.scroll as isize + n).max(0) as u16,
            }
        } else if inside(self.other_area) && let Some(o) = &mut self.other {
            let h = self.other_area.height.saturating_sub(2) as usize;
            o.scroll(n, h);
        } else if !inside(self.list_area) && self.click_frame(column, row, Some(n)) {
        } else if inside(self.list_area) {
            for _ in 0..n.abs() { if n > 0 { self.list.select_next(); } else { self.list.select_previous(); } }
            self.moved();
        }
    }

    /// The other window clicked becomes the active one. True when the click put the cursor.
    fn click_page(&mut self, column: u16, row: u16) -> bool {
        let inside = |a: Rect| column > a.x && column + 1 < a.x + a.width && row > a.y && row + 1 < a.y + a.height;
        if self.other.is_some() && inside(self.other_area) {
            self.swap();
            std::mem::swap(&mut self.page_area, &mut self.other_area);
        }
        let a = self.page_area;
        if !inside(a) { return false; }
        if self.editor.is_none() && let Some(p) = self.selected() { self.edit(&p.path); }
        let Some(e) = &mut self.editor else { return false };
        e.end_search(true);
        let (row, col) = (e.top + (row - a.y - 1) as usize, (e.left + (column - a.x - 1) as usize).saturating_sub(e.gutter()));
        e.click(row, col);
        true
    }

    /// A key in the quick open: typing, moving among the pages found, Enter
    /// opening the one chosen (made when it is to create).
    fn quick_key(&mut self, key: KeyEvent, cancel: bool, plain: bool) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let Some(q) = &mut self.quick else { return };
        match key.code {
            _ if cancel => self.quick = None,
            KeyCode::Down | KeyCode::Tab => q.selected += 1,
            KeyCode::Char('n') if ctrl => q.selected += 1,
            KeyCode::Up | KeyCode::BackTab => q.selected = q.selected.saturating_sub(1),
            KeyCode::Char('p') if ctrl => q.selected = q.selected.saturating_sub(1),
            KeyCode::Backspace => { q.query.pop(); q.selected = 0; }
            KeyCode::Char(c) if plain || key.modifiers == KeyModifiers::SHIFT => { q.query.push(c); q.selected = 0; }
            KeyCode::Enter => {
                let items = commands::quick_items(&q.query, &self.tabs, &self.project.files());
                let chosen = items.into_iter().nth(q.selected);
                self.quick = None;
                match chosen {
                    Some(commands::QuickItem::Page { path, .. }) => { self.hits = None; self.edit(&path); }
                    Some(commands::QuickItem::Create(name)) => self.open_or_create(name),
                    None => {}
                }
            }
            _ => {}
        }
    }

    /// Text pasted in the terminal: in the value typed at the bottom, else in the page.
    fn paste(&mut self, text: &str) {
        if let Some(q) = &mut self.quick { q.query.push_str(text.lines().next().unwrap_or_default()); return; }
        if let Some(p) = &mut self.prompt { p.input.push_str(text.lines().next().unwrap_or_default()); return; }
        if let Some(e) = &mut self.editor { e.paste(text); }
    }

    /// Handle `key`: the shortcuts of the window first, then the keys of the
    /// terminal. Returns whether to quit.
    fn key(&mut self, key: KeyEvent) -> bool {
        let plain = !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        let cancel = key.code == KeyCode::Esc || (key.code == KeyCode::Char('g') && key.modifiers.contains(KeyModifiers::CONTROL));
        if let Some(p) = &mut self.popup {
            let list = !p.targets.is_empty();
            let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
            match key.code {
                // A list: the line chosen, opened with Enter
                KeyCode::Down if list => p.step(true),
                KeyCode::Up if list => p.step(false),
                KeyCode::Char('n') if list && ctrl => p.step(true),
                KeyCode::Char('p') if list && ctrl => p.step(false),
                KeyCode::PageDown if list => for _ in 0..10 { p.step(true) },
                KeyCode::PageUp if list => for _ in 0..10 { p.step(false) },
                KeyCode::Enter if list => self.open_target(),
                KeyCode::Down => p.scroll = p.scroll.saturating_add(1),
                KeyCode::Up => p.scroll = p.scroll.saturating_sub(1),
                KeyCode::PageDown => p.scroll = p.scroll.saturating_add(self.height / 2),
                KeyCode::PageUp => p.scroll = p.scroll.saturating_sub(self.height / 2),
                _ => self.popup = None,
            }
            return false;
        }
        if self.quick.is_some() { self.quick_key(key, cancel, plain); return false; }
        if let Some(p) = &mut self.prompt {
            match key.code {
                _ if cancel => { self.prompt = None; self.status.clear(); }
                KeyCode::Enter => self.submit(),
                KeyCode::Tab => match p.completes() {
                    Some(true) => self.complete_prompt(App::tag_names),
                    Some(false) => self.complete_prompt(App::link_names),
                    None => {}
                },
                KeyCode::Backspace => { p.input.pop(); p.choices.clear(); }
                KeyCode::Char(c) if plain => { p.input.push(c); p.choices.clear(); }
                _ => {}
            }
            return false;
        }
        if self.frame_key(key) { return false; }
        if let Some(s) = &mut self.suggest {
            let ctrl_only = key.modifiers == KeyModifiers::CONTROL;
            let n = s.items.len();
            match key.code {
                KeyCode::Down => { s.selected = (s.selected + 1) % n; return false; }
                KeyCode::Up => { s.selected = (s.selected + n - 1) % n; return false; }
                KeyCode::Char('n') if ctrl_only => { s.selected = (s.selected + 1) % n; return false; }
                KeyCode::Char('p') if ctrl_only => { s.selected = (s.selected + n - 1) % n; return false; }
                KeyCode::Tab | KeyCode::Enter if key.modifiers.is_empty() => { self.accept_suggest(); return false; }
                KeyCode::Esc => { self.suggest = None; return false; }
                KeyCode::Char('g') if ctrl_only => { self.suggest = None; return false; }
                // Typing goes on (and refreshes them); anything else closes them
                KeyCode::Char(_) | KeyCode::Backspace if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {}
                _ => self.suggest = None,
            }
        }
        if let Some((_, with)) = self.replacing.clone() && let Some(e) = &mut self.editor {
            let more = match key.code {
                KeyCode::Char('y' | ' ') => e.replace_match(Some(&with)),
                KeyCode::Char('n') | KeyCode::Delete | KeyCode::Backspace => e.replace_match(None),
                KeyCode::Char('!') => { while e.replace_match(Some(&with)) {} false }
                _ => false,
            };
            if !more { self.replacing = None; e.end_search(true); }
            return false;
        }
        if let Some(e) = &mut self.editor && e.zap {
            match key.code {
                KeyCode::Char(c) if plain || key.modifiers == KeyModifiers::SHIFT => e.zap_to(c),
                _ => e.zap = false,
            }
            return false;
        }
        if let Some(token) = keys::token(&key) {
            match self.chord.step(&self.kb, token) {
                Resolution::Pending(keys) => { self.status = format!("{keys} …"); return false; }
                Resolution::Aborted => { self.status = String::new(); return false; }
                Resolution::Action(id) => {
                    // The chord shown while it was typed is done
                    if self.status.ends_with(" …") { self.status.clear(); }
                    if id == "find" || id == "find_previous" {
                        // Among the pages, a search in the page chosen
                        if self.editor.is_none() && let Some(p) = self.selected() { self.edit(&p.path); }
                        if let Some(e) = &mut self.editor { e.find(id == "find"); }
                        return false;
                    }
                    // Any other shortcut ends the search (`keyboard_quit` cancels it), then does its work
                    if let Some(e) = &mut self.editor && e.search.is_some() {
                        e.end_search(id != "keyboard_quit");
                        if id == "keyboard_quit" { return false; }
                    }
                    if let Some(quit) = self.action(&id) { return quit; }
                    if self.command(&id).is_some() { return false; }
                    let page = self.height as usize;
                    let todo = self.prefs.todo();
                    if let Some(e) = &mut self.editor && e.action(&id, page, todo.as_ref()) { return false; }
                }
                Resolution::None => {}
            }
        }
        if let Some(e) = &mut self.editor && e.search.is_some() {
            let plain = !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
            match key.code {
                KeyCode::Char(c) if plain => { e.search_edit(Some(c)); return false; }
                KeyCode::Backspace => { e.search_edit(None); return false; }
                KeyCode::Enter | KeyCode::Esc => { e.end_search(true); return false; }
                // Another key ends the search, then does what it does
                _ => e.end_search(true),
            }
        }
        if self.editor.is_some() { self.edit_key(key) } else { self.list_key(key) }
    }

    /// A key among the pages.
    fn list_key(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => return self.cancel(),
            KeyCode::Char('c') if ctrl => return true,
            // Ctrl+Shift+J is Ctrl+J in most terminals
            KeyCode::Char('j') if ctrl => self.journal(),
            KeyCode::Down => { self.list.select_next(); self.moved(); }
            KeyCode::Up => { self.list.select_previous(); self.moved(); }
            KeyCode::PageDown => self.scroll = self.scroll.saturating_add(self.height / 2),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(self.height / 2),
            KeyCode::Tab => self.next_link(true),
            KeyCode::BackTab => self.next_link(false),
            KeyCode::Enter if self.link.is_some() => self.follow_chosen(),
            KeyCode::Enter => if let Some(page) = self.selected() { self.edit(&page.path); },
            KeyCode::Backspace => { self.filter.pop(); self.list.select_first(); self.moved(); }
            KeyCode::Char(c) if !ctrl => { self.filter.push(c); self.list.select_first(); self.moved(); }
            _ => {}
        }
        false
    }

    /// A key in the editor: typing, and the keys every editor has.
    fn edit_key(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let page = self.height as usize;
        let Some(e) = &mut self.editor else { return false };
        let act = |e: &mut Editor, id: &str| { e.action(id, page, None); };
        match key.code {
            KeyCode::Esc => { self.close(); }
            KeyCode::Char('c') if ctrl => return true,
            KeyCode::Char('j') if ctrl => self.journal(),
            KeyCode::Char(c) if !ctrl && !key.modifiers.contains(KeyModifiers::ALT) => e.type_char(c),
            KeyCode::Enter => e.insert("\n"),
            KeyCode::Tab => e.insert("\t"),
            KeyCode::Backspace => e.backspace(),
            KeyCode::Delete => e.delete(),
            KeyCode::Left => act(e, "backward_char"),
            KeyCode::Right => act(e, "forward_char"),
            KeyCode::Up => act(e, "previous_line"),
            KeyCode::Down => act(e, "next_line"),
            KeyCode::Home if ctrl => act(e, "beginning_of_buffer"),
            KeyCode::End if ctrl => act(e, "end_of_buffer"),
            KeyCode::Home => act(e, "beginning_of_line"),
            KeyCode::End => act(e, "end_of_line"),
            KeyCode::PageDown => act(e, "scroll_down"),
            KeyCode::PageUp => act(e, "scroll_up"),
            _ => {}
        }
        // What is typed may be completed (a link, a tag)
        let typing = matches!(key.code, KeyCode::Char(_) | KeyCode::Backspace) && !ctrl && !key.modifiers.contains(KeyModifiers::ALT);
        if typing { self.refresh_suggest(); } else { self.suggest = None; }
        false
    }

    /// Do the action `id` of the window's shortcuts: `None` when the window
    /// does not do it here (an editing action, or none), else whether to quit.
    fn action(&mut self, id: &str) -> Option<bool> {
        let editing = self.editor.is_some();
        match id {
            "quit" => return Some(true),
            "save" if editing => { self.save(); }
            // In the editor, it lets go of the region (and of a zap waiting for its character)
            "keyboard_quit" => {
                match &mut self.editor { Some(e) => { e.action("keyboard_quit", 0, None); } None => { self.cancel(); } }
                self.status.clear();
            }
            "quick_open" => self.quick = Some(commands::Quick::default()),
            "new_page" => {
                let name = if editing { String::new() } else { self.filter.trim().to_string() };
                if name.is_empty() && editing { self.close(); self.status = "type the name of the new page, then this shortcut again".into(); }
                else { self.open_or_create(name); }
            }
            "journal" => self.journal(),
            "open_link" if editing => self.follow_at_cursor(),
            "open_link" => {
                if self.link.is_none() { self.next_link(true); }
                self.follow_chosen();
            }
            "page_back" => self.go(false),
            "page_forward" => self.go(true),
            "next_line" if !editing => { self.list.select_next(); self.moved(); }
            "previous_line" if !editing => { self.list.select_previous(); self.moved(); }
            _ => return None,
        }
        Some(false)
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
        let Some(e) = &self.editor else { return };
        let (line, col) = e.line_col();
        match self.page_links().into_iter().find(|l| l.line == line && (l.start..=l.end).contains(&col)) {
            Some(l) => self.follow(&l.target),
            None => self.status = "no link under the cursor".into(),
        }
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
        // `[[Page::*Titre]]`: the page; the place in it is not reached yet
        let (name, _place) = links::split(target);
        if name.is_empty() { self.status = "a place in this page: not in the terminal yet".into(); return; }
        let file = match self.resolver().resolve(name).cloned() {
            Some(f) => f,
            None => match self.project.create(name, None, self.prefs.tags()) {
                Ok(f) => { self.status = format!("{name}: page created"); f }
                Err(e) => { self.status = e.to_string(); return; }
            },
        };
        if !self.save() { return; }
        self.back.extend(current);
        self.forward.clear();
        self.show(&file.path);
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

    fn journal(&mut self) {
        if !self.prefs.journal { self.status = "the journal extension is off (settings)".into(); return; }
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        self.open_or_create(today);
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
                Ok(page) => page,
                Err(e) => { self.status = e.to_string(); return; }
            },
        };
        self.edit(&page.path);
    }

    fn draw(&mut self, f: &mut ratatui::Frame) {
        let [main, help] = Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(f.area());
        self.total = main.width;
        // The columns of frames as the window's layout puts them, the page between them
        let (left_frames, right_frames) = (tui_frames(self.prefs.column(false)), tui_frames(self.prefs.column(true)));
        self.right_column = !right_frames.is_empty() && main.width >= 100;
        let right_width = if self.right_column { self.right_split.unwrap_or((main.width / 5).clamp(24, 40)).clamp(16, main.width / 3) } else { 0 };
        self.right_x = main.width - right_width;
        let left_width = if left_frames.is_empty() { 0 } else { self.split_width(main.width) };
        let [left_col, right, right_col] = Layout::horizontal([Constraint::Length(left_width), Constraint::Fill(1), Constraint::Length(right_width)]).areas(main);
        self.frame_areas.clear();
        self.list_area = Rect::default();
        for (frames, col) in [(&left_frames, left_col), (&right_frames, right_col)] {
            if col.width == 0 { continue; }
            let heights: Vec<Constraint> = frames.iter().map(|f| if f.grows() { Constraint::Fill(1) } else { Constraint::Length(3) }).collect();
            for (frame, area) in frames.iter().zip(Layout::vertical(heights).split(col).iter()) {
                self.frame_areas.push((*frame, *area));
                self.draw_frame(f, *frame, *area);
            }
        }

        let resolver = self.resolver();
        let mark_broken = self.prefs.mark_broken_links;
        let broken = |t: &str| mark_broken && links::is_page_target(t) && resolver.resolve(links::split(t).0).is_none();
        let todo = self.prefs.todo();
        // The other window: on the same page, the text of the active one
        if let (Some(a), Some(o)) = (&self.editor, &mut self.other) && a.path == o.path { o.sync_from(a); }
        let (active, other) = match self.split_dir {
            Some(dir) if self.other.is_some() => {
                let halves = Layout::new(dir, [Constraint::Fill(1), Constraint::Fill(1)]).split(right);
                (halves[self.active_slot], halves[1 - self.active_slot])
            }
            _ => (right, Rect::default()),
        };
        self.page_area = active;
        self.other_area = other;
        self.height = active.height.saturating_sub(2);
        self.width = active.width.saturating_sub(2);
        let tags = self.prefs.tags();
        let (hide_stars, text_first, live) = (self.prefs.hide_leading_stars, self.prefs.link_text_first, self.prefs.live_preview);
        // Live preview: the markup shown on the line of the cursor only (`usize::MAX`: on none)
        let draw_of = |path: &str, cursor_line: usize| Draw {
            format: orgamnesia_core::Format::of_page(path), tags, todo: todo.as_ref(), broken: &broken, hide_stars,
            live: live.then_some(cursor_line), text_first,
        };
        if let Some(o) = &mut self.other {
            let d = draw_of(&o.path, o.line_col().0);
            draw_editor(f, o, other, &d, false);
        }
        let path = self.current_path();
        match (&mut self.editor, path) {
            (Some(e), Some(_)) => {
                let d = draw_of(&e.path, e.line_col().0);
                draw_editor(f, e, active, &d, true);
            }
            (None, Some(path)) => {
                let d = draw_of(&path, usize::MAX);
                let text = self.project.read(&path).unwrap_or_else(|e| e.to_string());
                let chosen = self.link.and_then(|i| highlight::links(&text, &d).into_iter().nth(i));
                let name = title_of(&path, &text);
                f.render_widget(Paragraph::new(highlight::render(&text, &d, chosen.as_ref(), &Default::default())).wrap(Wrap { trim: false })
                    .scroll((self.scroll, 0)).block(Block::bordered().title(format!(" {name} "))), right);
            }
            _ => f.render_widget(Block::bordered(), right),
        }
        if let (Some(s), Some(e)) = (&self.suggest, &self.editor) {
            let a = self.page_area;
            let (x, y) = ((e.screen_col() + e.gutter()).saturating_sub(e.left) as u16, e.row().saturating_sub(e.top) as u16);
            let width = s.items.iter().map(|i| i.chars().count()).max().unwrap_or(0) as u16 + 4;
            let height = s.items.len() as u16 + 2;
            // Under the cursor, else above it; within the screen
            let below = a.y + 2 + y;
            let top = if below + height <= main.y + main.height { below } else { (a.y + 1 + y).saturating_sub(height) };
            let left = (a.x + 1 + x).min((main.x + main.width).saturating_sub(width));
            let area = Rect { x: left, y: top, width: width.min(main.width), height: height.min(main.height) };
            f.render_widget(Clear, area);
            let mut state = ListState::default().with_selected(Some(s.selected));
            f.render_stateful_widget(List::new(s.items.clone()).block(Block::bordered().border_style(Style::new().fg(ratatui::style::Color::Cyan)))
                .highlight_style(Style::new().add_modifier(Modifier::REVERSED)), area, &mut state);
        }
        if let Some(p) = &mut self.popup {
            f.render_widget(Clear, right);
            let list = !p.targets.is_empty();
            // The line chosen kept in view
            let rows = right.height.saturating_sub(2) as usize;
            if list && rows > 0 && (p.selected < p.scroll as usize || p.selected >= p.scroll as usize + rows) {
                p.scroll = p.selected.saturating_sub(rows / 2) as u16;
            }
            let text: Vec<Line> = p.lines.iter().enumerate().map(|(i, l)| {
                let s = if list && i == p.selected { Style::new().add_modifier(Modifier::REVERSED) } else { Style::new() };
                Line::from(l.as_str()).style(s)
            }).collect();
            let keys = if list { "↑↓ Enter, Esc closes" } else { "↑↓, any other key closes" };
            let empty = if p.lines.is_empty() { vec![Line::from("(nothing)")] } else { text };
            f.render_widget(Paragraph::new(empty).scroll((p.scroll, 0))
                .block(Block::bordered().title(format!(" {} — {keys} ", p.title))), right);
        }
        if let Some(q) = &mut self.quick {
            let items = commands::quick_items(&q.query, &self.tabs, &self.project.files());
            q.selected = q.selected.min(items.len().saturating_sub(1));
            let [_, area, _] = Layout::vertical([Constraint::Percentage(15), Constraint::Percentage(70), Constraint::Fill(1)]).areas(main);
            let [_, area, _] = Layout::horizontal([Constraint::Percentage(20), Constraint::Percentage(60), Constraint::Fill(1)]).areas(area);
            f.render_widget(Clear, area);
            let [input, rest] = Layout::vertical([Constraint::Length(3), Constraint::Fill(1)]).areas(area);
            f.render_widget(Paragraph::new(q.query.as_str()).block(Block::bordered().title(" Open a page ")), input);
            f.set_cursor_position(Position::new(input.x + 1 + q.query.chars().count() as u16, input.y + 1));
            let lines: Vec<String> = items.iter().map(|i| match i {
                commands::QuickItem::Page { name, open: true, .. } => format!("● {name}"),
                commands::QuickItem::Page { name, .. } => format!("  {name}"),
                commands::QuickItem::Create(name) => format!("+ create “{name}”"),
            }).collect();
            let mut state = ListState::default().with_selected(Some(q.selected));
            f.render_stateful_widget(List::new(lines).block(Block::bordered().title(" ↑↓ Enter · Esc "))
                .highlight_style(Style::new().add_modifier(Modifier::REVERSED)), rest, &mut state);
            return;
        }
        if let Some(p) = &self.prompt {
            let line = format!("{}{}", p.label, p.input);
            let hint = match (p.choices.is_empty(), p.completes()) {
                (false, _) => format!("   [{}]", p.choices.join(" | ")),
                (true, Some(_)) => "   (Tab: complete)".to_string(),
                _ => String::new(),
            };
            f.render_widget(Line::from(hint).style(Style::new().fg(ratatui::style::Color::DarkGray)),
                Rect { x: help.x + line.chars().count() as u16, width: help.width.saturating_sub(line.chars().count() as u16), ..help });
            f.set_cursor_position(Position::new(help.x + line.chars().count() as u16, help.y));
            f.render_widget(Line::from(line).style(Style::new().add_modifier(Modifier::BOLD)), help);
            return;
        }
        if let Some((from, to)) = &self.replacing {
            let line = format!("Replace “{from}” with “{to}”? (y/Space: yes, n: no, !: all, other key: stop)");
            f.render_widget(Line::from(line).style(Style::new().add_modifier(Modifier::BOLD)), help);
            return;
        }

        // A search going on: what is searched, in place of the help
        if let Some(s) = self.editor.as_ref().and_then(|e| e.search.as_ref()) {
            let failing = !s.query.is_empty() && s.current.is_none();
            let line = format!("{}Search: {}{}   (Enter/Esc: stop, Ctrl+G: cancel)",
                if s.wrapped { "Wrapped " } else { "" }, s.query, if failing { "  [not found]" } else { "" });
            f.render_widget(Line::from(line).style(Style::new().add_modifier(Modifier::BOLD)), help);
            return;
        }
        let help_line = &self.help[self.editor.is_some() as usize];
        let line = if self.status.is_empty() { help_line.clone() } else { format!("{}  │  {help_line}", self.status) };
        // The git state at the right end: branch, files to commit, commits to push and to pull
        let git = self.git.as_ref().map(git_line).unwrap_or_default();
        let [help, right] = Layout::horizontal([Constraint::Fill(1), Constraint::Length(git.chars().count() as u16)]).areas(help);
        f.render_widget(Line::from(format!("Orgamnesia TUI (beta)  {line}")).style(Style::new().add_modifier(Modifier::DIM)), help);
        let warn = self.git.as_ref().is_some_and(|g| g.conflicts > 0 || g.merging);
        f.render_widget(Line::from(git).style(if warn { Style::new().fg(ratatui::style::Color::Red) } else { Style::new().fg(ratatui::style::Color::Cyan) }), right);
    }
}

/// The git state as the bottom line shows it: ` ⎇ main ●3 ↑1 ↓2`.
fn git_line(g: &orgamnesia_core::GitStatus) -> String {
    let mut out = format!(" ⎇ {}", g.branch.as_deref().unwrap_or("(detached)"));
    if g.changes > 0 { out.push_str(&format!(" ●{}", g.changes)); }
    if g.ahead > 0 { out.push_str(&format!(" ↑{}", g.ahead)); }
    if g.behind > 0 { out.push_str(&format!(" ↓{}", g.behind)); }
    if g.conflicts > 0 { out.push_str(&format!(" ✗{} conflicts", g.conflicts)); }
    if g.merging { out.push_str(" (merging)"); }
    out.push(' ');
    out
}

/// The title of a frame in its colour, bold: each frame known at a glance.
fn frame_title(frame: orgamnesia_core::Frame, title: String) -> Line<'static> {
    use orgamnesia_core::Frame;
    use ratatui::style::Color;
    let color = match frame {
        Frame::Pages => Color::Cyan,
        Frame::Project => Color::Magenta,
        Frame::Git => Color::Green,
        Frame::Backlinks => Color::Blue,
        Frame::Tags => Color::Indexed(141),
        Frame::Todos => Color::Red,
        Frame::Broken => Color::Yellow,
        _ => Color::Reset,
    };
    Line::from(title).style(Style::new().fg(color).add_modifier(Modifier::BOLD))
}

/// The frames of a column the terminal draws: the lists and the lines of the
/// window, not its buttons (brand, settings, quit, graph, org-bloom, tips).
fn tui_frames(frames: Vec<orgamnesia_core::Frame>) -> Vec<orgamnesia_core::Frame> {
    use orgamnesia_core::Frame;
    frames.into_iter().filter(|f| matches!(f, Frame::Project | Frame::Git | Frame::Pages | Frame::Backlinks | Frame::Tags | Frame::Todos | Frame::Broken)).collect()
}

/// What the title of a window shows of the page `path`: the title written in
/// it (`#+TITLE:`, Markdown's `title:`), as the window's title bar, else its name.
fn title_of(path: &str, text: &str) -> String {
    orgamnesia_core::Format::of_page(path).title(text).unwrap_or_else(|| crate::project::files::page_name(path))
}

/// The editor `e` in `area`, with the cursor when it is the active window. A
/// file of the settings is drawn as YAML.
fn draw_editor(f: &mut ratatui::Frame, e: &mut Editor, area: Rect, d: &Draw, active: bool) {
    let gutter = e.gutter();
    let (height, width) = (area.height.saturating_sub(2) as usize, (area.width.saturating_sub(2) as usize).saturating_sub(gutter));
    e.follow_cursor(height, width);
    let found = highlight::Found { all: e.matches(), current: e.search.as_ref().and_then(|s| s.current), region: e.region() };
    let text = e.text();
    let lines = if e.page { highlight::render(&text, d, None, &found) } else { highlight::render_yaml(&text, &found) };
    // The lines hidden by folds left out, the folded headlines ending with …
    let dim = Style::new().fg(ratatui::style::Color::DarkGray);
    let lines: Vec<Line> = lines.into_iter().zip(e.lines_shown()).enumerate()
        .filter(|(_, (_, (hidden, _)))| !hidden)
        .map(|(n, (mut l, (_, folded)))| {
            if folded { l.spans.push(Span::styled(" …", dim)); }
            if gutter > 0 { l.spans.insert(0, Span::styled(format!("{:>w$} ", n + 1, w = gutter - 1), dim)); }
            l
        })
        .collect();
    if active {
        let (x, y) = (e.screen_col() - e.left + gutter, e.row() - e.top);
        f.set_cursor_position(Position::new(area.x + 1 + x as u16, area.y + 1 + y as u16));
    }
    let name = if e.page { title_of(&e.path, &text) } else { crate::project::files::page_name(&e.path) };
    let title = format!(" {name}{} ", if e.dirty { " ●" } else { "" });
    let border = if active { Style::new() } else { Style::new().fg(ratatui::style::Color::DarkGray) };
    f.render_widget(Paragraph::new(lines).scroll((e.top as u16, e.left as u16)).block(Block::bordered().title(title).border_style(border)), area);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(app: &mut App, code: KeyCode, m: KeyModifiers) -> bool { app.key(KeyEvent::new(code, m)) }

    #[test]
    fn edits_follows_links_and_saves() {
        let dir = std::env::temp_dir().join(format!("orgamnesia-tui-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("pages")).unwrap();
        std::fs::write(dir.join("pages/Alpha.org"), "* Alpha\n[[Beta]] [[Gamma]]").unwrap();
        for name in ["Beta.org", "alphabet.org"] { std::fs::write(dir.join("pages").join(name), "x").unwrap(); }
        let prefs = settings::Settings::default().prefs();
        let project = Project::open(&crate::slash(&dir), prefs.tags(), prefs.formats(), &prefs.journal_folder).unwrap();
        let mut app = App::new(project, prefs, Keybindings::with_profiles(None, vec![]));
        app.height = 10;
        app.filter = "ALPHA".into();
        let names: Vec<String> = app.shown().into_iter().map(|f| f.name).collect();
        assert_eq!(names, ["Alpha", "alphabet"]);
        assert!(app.help[0].contains("Ctrl+K find"), "{}", app.help[0]);

        // Enter edits the page here; typing changes it, Ctrl+S writes it
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        let alpha = app.editor.as_ref().unwrap().path.clone();
        press(&mut app, KeyCode::End, KeyModifiers::NONE);
        press(&mut app, KeyCode::Char('!'), KeyModifiers::NONE);
        press(&mut app, KeyCode::Char('s'), KeyModifiers::CONTROL);
        assert_eq!(std::fs::read_to_string(&alpha).unwrap(), "* Alpha!\n[[Beta]] [[Gamma]]");

        // Ctrl+Enter on a link opens its page in the editor, the first one saved
        press(&mut app, KeyCode::Down, KeyModifiers::NONE);
        press(&mut app, KeyCode::Char('x'), KeyModifiers::NONE);
        press(&mut app, KeyCode::Home, KeyModifiers::NONE);
        press(&mut app, KeyCode::Enter, KeyModifiers::CONTROL);
        assert!(app.editor.as_ref().unwrap().path.ends_with("Beta.org"));
        assert_eq!(std::fs::read_to_string(&alpha).unwrap(), "* Alpha!\n[[Beta]]x [[Gamma]]");
        press(&mut app, KeyCode::Left, KeyModifiers::ALT);
        assert_eq!(app.editor.as_ref().unwrap().path, alpha);

        // The border of the pages dragged with the mouse, kept within the screen
        app.total = 100;
        let at = |column, kind| MouseEvent { kind, column, row: 3, modifiers: KeyModifiers::NONE };
        app.mouse(at(5, MouseEventKind::Down(MouseButton::Left)));
        app.mouse(at(50, MouseEventKind::Drag(MouseButton::Left)));
        assert_eq!(app.split_width(100), 30);
        app.mouse(at(29, MouseEventKind::Down(MouseButton::Left)));
        app.mouse(at(49, MouseEventKind::Drag(MouseButton::Left)));
        app.mouse(at(49, MouseEventKind::Up(MouseButton::Left)));
        assert_eq!(app.split_width(100), 50);
        app.mouse(at(99, MouseEventKind::Drag(MouseButton::Left)));
        assert_eq!(app.split_width(100), 50);
        app.split = Some(99);
        assert_eq!(app.split_width(100), 80);

        // A click on a page of the list leaves the editor (saved) for it
        app.list_area = Rect::new(0, 0, 30, 10);
        press(&mut app, KeyCode::Char('z'), KeyModifiers::NONE);
        app.mouse(at(3, MouseEventKind::Down(MouseButton::Left)));
        assert!(app.editor.is_none());
        assert!(std::fs::read_to_string(&alpha).unwrap().starts_with("z"));
        assert_eq!(app.list.selected(), Some(2));
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        press(&mut app, KeyCode::Esc, KeyModifiers::NONE);

        // Esc leaves the editor; quitting from the pages
        press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert!(app.editor.is_none());
        assert!(press(&mut app, KeyCode::Char('q'), KeyModifiers::CONTROL));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn splits_the_screen_as_emacs() {
        let dir = std::env::temp_dir().join(format!("orgamnesia-tui-split-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("pages")).unwrap();
        std::fs::write(dir.join("pages/Alpha.org"), "[[Beta]] alpha").unwrap();
        std::fs::write(dir.join("pages/Beta.org"), "beta").unwrap();
        let prefs = settings::Settings::default().prefs();
        let project = Project::open(&crate::slash(&dir), prefs.tags(), prefs.formats(), &prefs.journal_folder).unwrap();
        let mut app = App::new(project, prefs, Keybindings::with_profiles(Some("emacs"), vec![]));
        let ctrl = KeyModifiers::CONTROL;
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        let alpha = app.editor.as_ref().unwrap().path.clone();

        // C-x 3: the page in both halves; what is typed in one is in the other
        press(&mut app, KeyCode::Char('x'), ctrl);
        press(&mut app, KeyCode::Char('3'), KeyModifiers::NONE);
        assert_eq!(app.split_dir, Some(Direction::Horizontal));
        press(&mut app, KeyCode::Char('!'), KeyModifiers::NONE);
        press(&mut app, KeyCode::Char('x'), ctrl);
        press(&mut app, KeyCode::Char('o'), KeyModifiers::NONE);
        assert_eq!(app.active_slot, 1);
        assert!(app.editor.as_ref().unwrap().text().starts_with('!'));

        // Ctrl+click on the link: Beta in this half, Alpha (saved) in the other
        app.page_area = Rect::new(30, 0, 40, 10);
        app.mouse(MouseEvent { kind: MouseEventKind::Down(MouseButton::Left), column: 33, row: 1, modifiers: ctrl });
        assert!(app.editor.as_ref().unwrap().path.ends_with("Beta.org"));
        assert_eq!(app.other.as_ref().unwrap().path, alpha);
        assert_eq!(std::fs::read_to_string(&alpha).unwrap(), "![[Beta]] alpha");

        // C-SPC starts a region, C-g lets go of it
        press(&mut app, KeyCode::Char(' '), ctrl);
        press(&mut app, KeyCode::Char('f'), ctrl);
        assert!(app.editor.as_ref().unwrap().region().is_some());
        press(&mut app, KeyCode::Char('g'), ctrl);
        assert_eq!(app.editor.as_ref().unwrap().region(), None);
        press(&mut app, KeyCode::Char('b'), ctrl);

        // C-x 0 closes this half, C-x 1 the other: one window again
        press(&mut app, KeyCode::Char('z'), KeyModifiers::NONE);
        press(&mut app, KeyCode::Char('x'), ctrl);
        press(&mut app, KeyCode::Char('0'), KeyModifiers::NONE);
        assert!(app.other.is_none() && app.split_dir.is_none());
        assert_eq!(app.editor.as_ref().unwrap().path, alpha);
        assert_eq!(std::fs::read_to_string(dir.join("pages/Beta.org")).unwrap(), "zbeta");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn follows_the_settings() {
        let dir = std::env::temp_dir().join(format!("orgamnesia-tui-prefs-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("pages")).unwrap();
        std::fs::write(dir.join("pages/Alpha.org"), "alpha").unwrap();
        let mut prefs = settings::Settings::default().prefs();
        (prefs.autosave, prefs.journal, prefs.ask_format) = (true, false, true);
        let project = Project::open(&crate::slash(&dir), prefs.tags(), prefs.formats(), &prefs.journal_folder).unwrap();
        let mut app = App::new(project, prefs, Keybindings::with_profiles(None, vec![]));
        // Saved on every change
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        press(&mut app, KeyCode::Char('!'), KeyModifiers::NONE);
        app.after_event();
        assert_eq!(std::fs::read_to_string(dir.join("pages/Alpha.org")).unwrap(), "!alpha");
        // No journal when its extension is off
        app.journal();
        assert!(app.status.contains("journal"));
        // The format of a new page asked
        app.open_or_create("Beta".into());
        assert!(app.prompt.as_ref().is_some_and(|p| p.label.contains("Beta")));
        app.prompt.as_mut().unwrap().input = "md".into();
        app.submit();
        assert!(dir.join("pages/Beta.md").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_git_state_reads_at_a_glance() {
        let g = orgamnesia_core::GitStatus { repo: true, branch: Some("main".into()), changes: 3, ahead: 1, behind: 2, ..Default::default() };
        assert_eq!(git_line(&g), " ⎇ main ●3 ↑1 ↓2 ");
        let clean = orgamnesia_core::GitStatus { repo: true, branch: Some("main".into()), ..Default::default() };
        assert_eq!(git_line(&clean), " ⎇ main ");
    }

    #[test]
    fn the_session_comes_back() {
        let dir = std::env::temp_dir().join(format!("orgamnesia-tui-session-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("pages")).unwrap();
        for name in ["A", "B", "C"] { std::fs::write(dir.join(format!("pages/{name}.org")), format!("* {name}")).unwrap(); }
        let prefs = settings::Settings::default().prefs();
        let open = || Project::open(&crate::slash(&dir), prefs.tags(), prefs.formats(), &prefs.journal_folder).unwrap();
        let mut app = App::new(open(), prefs.clone(), Keybindings::with_profiles(Some("emacs"), vec![]));
        let path = |n: &str| format!("{}/pages/{n}.org", app.project.path);
        let (a, b, c) = (path("A"), path("B"), path("C"));
        app.edit(&a);
        app.edit(&b);
        app.split(Direction::Horizontal);
        app.swap();
        app.edit(&c);
        let session = app.session().unwrap();
        assert_eq!((session.active.as_deref(), session.other.as_deref()), (Some(b.as_str()), Some(c.as_str())));
        assert_eq!((session.split, session.focus_second), (Some(orgamnesia_core::SplitKind::Vertical), true));
        // Opened again: the same pages and windows, the focus where it was
        let mut again = App::new(open(), prefs, Keybindings::with_profiles(Some("emacs"), vec![]));
        again.restore_session(session.clone());
        assert_eq!(again.editor.as_ref().unwrap().path, c);
        assert_eq!(again.other.as_ref().unwrap().path, b);
        assert_eq!(again.session(), Some(session));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn panels_and_renaming() {
        let dir = std::env::temp_dir().join(format!("orgamnesia-tui-panels-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("pages")).unwrap();
        std::fs::write(dir.join("pages/A.org"), "* TODO Faire :projet:\nvoir [[B]] et [[Rien]]").unwrap();
        std::fs::write(dir.join("pages/B.org"), "* B").unwrap();
        let mut prefs = settings::Settings::default().prefs();
        prefs.update_links = true;
        let project = Project::open(&crate::slash(&dir), prefs.tags(), prefs.formats(), &prefs.journal_folder).unwrap();
        let mut app = App::new(project, prefs, Keybindings::with_profiles(None, vec![]));
        let a = format!("{}/pages/A.org", app.project.path);
        // F4: the TODO frame of the right column takes the focus; Enter opens the page on its line
        press(&mut app, KeyCode::F(4), KeyModifiers::NONE);
        assert_eq!(app.frame_focus, Some(orgamnesia_core::Frame::Todos));
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(app.editor.as_ref().unwrap().path, a);
        assert_eq!(app.frame_focus, None);
        // F5: the tags, Enter on one its headlines (over the page)
        press(&mut app, KeyCode::F(5), KeyModifiers::NONE);
        assert!(app.frame_items(orgamnesia_core::Frame::Tags).1[0].0.starts_with("projet"));
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert!(app.popup.as_ref().unwrap().title.contains("projet"));
        press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        // The pages not created (the tag :projet: is a link too), the pages linking here
        let broken = app.frame_items(orgamnesia_core::Frame::Broken).1;
        assert!(broken.iter().any(|(l, _)| l.starts_with("Rien")), "{broken:?}");
        app.edit(&format!("{}/pages/B.org", app.project.path));
        let back: Vec<String> = app.frame_items(orgamnesia_core::Frame::Backlinks).1.into_iter().map(|(l, _)| l).collect();
        assert_eq!(back, ["A"]);
        // A narrow screen has no right column: the shortcut opens the frame over the page
        app.right_column = false;
        press(&mut app, KeyCode::F(8), KeyModifiers::NONE);
        assert_eq!(app.popup.as_ref().unwrap().lines, ["A"]);
        press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        // F10: the list of the pages
        press(&mut app, KeyCode::F(10), KeyModifiers::NONE);
        assert!(app.editor.is_none());
        app.edit(&format!("{}/pages/B.org", app.project.path));
        // F2: B renamed C, the link in A follows
        press(&mut app, KeyCode::F(2), KeyModifiers::NONE);
        app.prompt.as_mut().unwrap().input = "C".into();
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert!(app.editor.as_ref().unwrap().path.ends_with("C.org"));
        assert!(std::fs::read_to_string(&a).unwrap().contains("[[C]]"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_custom_profile_gets_the_new_actions() {
        // A profile of an older version, without the actions added since
        let file = orgamnesia_core::keybindings::ProfileFile {
            editor_preset: "emacs".into(),
            app: [("quick_open".to_string(), vec!["alt+k".to_string()])].into(),
            editor: Default::default(),
        };
        let kb = Keybindings::from_files(Some("Mine"), vec![("Mine".into(), file)]);
        for (id, key) in [("rename_page", "F2"), ("panel_todos", "F4"), ("panel_tags", "F5"), ("panel_backlinks", "F8"), ("panel_broken", "F9")] {
            assert_eq!(kb.app_binds(id), &[key.to_string()], "{id}");
        }
        assert_eq!(kb.app_binds("quick_open"), &["alt+k".to_string()]);
        assert!(orgamnesia_core::keybindings::conflicts(&kb.app, &kb.editor).is_empty());
        assert!(help(&kb, false).contains("F1 shortcuts"));
    }

    #[test]
    fn names_complete_as_in_the_window() {
        let dir = std::env::temp_dir().join(format!("orgamnesia-tui-complete-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("pages")).unwrap();
        std::fs::write(dir.join("pages/Alpha.org"), "* Alpha :projet:\n").unwrap();
        std::fs::write(dir.join("pages/Lugny.org"), "* Histoire\n* Rues").unwrap();
        std::fs::write(dir.join("pages/Lunes.org"), "x").unwrap();
        let prefs = settings::Settings::default().prefs();
        let project = Project::open(&crate::slash(&dir), prefs.tags(), prefs.formats(), &prefs.journal_folder).unwrap();
        let mut app = App::new(project, prefs, Keybindings::with_profiles(None, vec![]));
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        press(&mut app, KeyCode::End, KeyModifiers::CONTROL);
        let typed = |app: &mut App, s: &str| for c in s.chars() { press(app, KeyCode::Char(c), KeyModifiers::NONE); };
        // [[lu: the pages, the link closed on accepting
        typed(&mut app, "voir [[lu");
        assert_eq!(app.suggest.as_ref().unwrap().items, ["Lugny", "Lunes"]);
        press(&mut app, KeyCode::Down, KeyModifiers::NONE);
        press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
        assert!(app.editor.as_ref().unwrap().text().ends_with("voir [[Lunes]]"), "{}", app.editor.as_ref().unwrap().text());
        // [[Lugny::*: its headlines
        typed(&mut app, " [[Lugny::*R");
        // Those starting with what is typed first, then those holding it (as in the window)
        assert_eq!(app.suggest.as_ref().unwrap().items, ["*Rues", "*Histoire"]);
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert!(app.editor.as_ref().unwrap().text().ends_with("[[Lugny::*Rues]]"));
        // The tag line: Tab completes with the tags of the project
        app.command("add_tag");
        app.prompt.as_mut().unwrap().input = "pro".into();
        press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(app.prompt.as_ref().unwrap().input, "projet");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn each_frame_has_its_colour() {
        use orgamnesia_core::Frame;
        let colours: Vec<_> = [Frame::Pages, Frame::Project, Frame::Git, Frame::Backlinks, Frame::Tags, Frame::Todos, Frame::Broken]
            .into_iter().map(|f| frame_title(f, String::new()).style.fg).collect();
        let mut distinct = colours.clone();
        distinct.dedup();
        assert_eq!(distinct.len(), colours.len());
        assert!(frame_title(Frame::Pages, " Pages ".into()).style.add_modifier.contains(Modifier::BOLD));
    }
}
