//! The panes and the tabs: a page opened, shown, saved, closed, split, swapped;
//! the session kept between runs; another project or the settings opened.

use super::*;

impl App {
    /// Another page chosen: no link of it chosen yet, seen from its top.
    pub(super) fn moved(&mut self) {
        self.link = None;
        self.scroll = 0;
    }

    /// Choose the page `path`, the filter cleared.
    pub(super) fn select_path(&mut self, path: &str) {
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
    pub(super) fn save(&mut self) -> bool {
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
    pub(super) fn close(&mut self) -> bool {
        if !self.save() || !self.save_other() { return false; }
        self.editor = None;
        self.single();
        true
    }

    /// Back to the list of the pages, the pages edited saved; the screen stays
    /// split, the page chosen then opened in the active window. False when not saved.
    pub(super) fn back_to_list(&mut self) -> bool {
        if !self.save() || !self.save_other() { return false; }
        self.editor = None;
        true
    }

    /// The previewed page edited first, so that the window left keeps it.
    pub(super) fn edit_previewed(&mut self) {
        if self.editor.is_none() && let Some(p) = self.selected() { self.edit(&p.path); }
    }

    /// Write the page of the other window, if it is another page and changed.
    pub(super) fn save_other(&mut self) -> bool {
        let same = matches!((&self.editor, &self.other), (Some(a), Some(b)) if a.path == b.path);
        if same || !self.other.as_ref().is_some_and(|o| o.dirty) { return true; }
        self.swap();
        let saved = self.save();
        self.swap();
        saved
    }

    /// The other window becomes the active one; on the same page, with its text.
    pub(super) fn swap(&mut self) {
        std::mem::swap(&mut self.editor, &mut self.other);
        self.active_slot ^= 1;
        if let (Some(a), Some(b)) = (&mut self.editor, &self.other) && a.path == b.path { a.sync_from(b); }
    }

    /// The screen split `dir` (the page in both halves), else turned the other way.
    pub(super) fn split(&mut self, dir: Direction) {
        if self.editor.is_none() && let Some(p) = self.selected() { self.edit(&p.path); }
        let Some(e) = &self.editor else { return };
        if self.other.is_none() { self.other = Some(e.twin()); }
        self.split_dir = Some(dir);
    }

    /// One window only: the other one closed (its page saved).
    pub(super) fn single(&mut self) {
        self.other = None;
        self.split_dir = None;
        self.active_slot = 0;
    }

    /// Edit the page `path` (the one edited saved first).
    pub(super) fn edit(&mut self, path: &str) {
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
    pub(super) fn show(&mut self, path: &str) {
        if self.editor.is_some() { self.edit(path); } else { self.select_path(path); }
    }

    /// Open again the pages and windows of `session`, when it is this project's
    /// (the pages deleted since left out), as the window does.
    pub(super) fn restore_session(&mut self, session: orgamnesia_core::Session) {
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
    pub(super) fn session(&self) -> Option<orgamnesia_core::Session> {
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

    pub(super) fn switch_project(&mut self, path: &str) {
        if !self.close() { return; }
        match Project::open(path, self.prefs.tags(), self.prefs.formats(), &self.prefs.journal_folder, &self.prefs.templates_folder) {
            Ok(project) => {
                super::prune_trash(&project, &self.prefs);
                self.project = project;
                self.folders_changed();
                self.watch_disk();
                self.disk_conflicts.clear();
                self.tabs.clear();
                self.back.clear();
                self.forward.clear();
                self.hits = None;
                self.filter.clear();
                self.list.select_first();
                self.moved();
                self.status = format!("project {}", crate::project::files::page_name(path));
            }
            Err(e) => self.status = e.to_string(),
        }
    }

    /// `settings.yaml` in the editor; what is saved counts at the next start.
    pub(super) fn open_settings(&mut self) {
        let Some(path) = self.config_dir.as_ref().map(|d| crate::config_files::path(d, &crate::config_files::File::Settings)) else { return };
        if !self.close() { return; }
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let mut e = Editor::new(crate::slash(&path), &text, false);
        e.prefs = super::editor::EditPrefs::from(&self.prefs);
        self.editor = Some(e);
        self.status = "settings.yaml: saved, it counts at the next start".into();
    }

    /// The next (previous) page opened in the editor.
    pub(super) fn next_tab(&mut self, forward: bool) {
        if self.tabs.is_empty() { return; }
        let here = self.editor.as_ref().and_then(|e| self.tabs.iter().position(|t| *t == e.path));
        let n = self.tabs.len();
        let i = match here { Some(i) if forward => (i + 1) % n, Some(i) => (i + n - 1) % n, None => n - 1 };
        let path = self.tabs[i].clone();
        self.edit(&path);
    }

    /// The page edited closed (saved), the one opened before it in its place.
    pub(super) fn close_tab(&mut self) {
        let Some(path) = self.editor.as_ref().map(|e| e.path.clone()) else { return };
        if !self.close() { return; }
        self.tabs.retain(|t| *t != path);
        if let Some(last) = self.tabs.last().cloned() { self.edit(&last); }
    }

    /// From the editor to the pages, and back.
    pub(super) fn other_window(&mut self) {
        if self.editor.is_some() {
            self.close();
        } else if let Some(p) = self.selected() {
            self.edit(&p.path);
        }
    }
}
