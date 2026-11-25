//! Following links and moving between pages: the link under the cursor or
//! chosen, a place in a page, the history, the journal, quick capture, a random page.

use super::*;

impl App {
    /// After a key or a click: what the editor copied to the clipboard, what
    /// it says, and the page saved when the settings say so (`autosave`).
    pub(super) fn after_event(&mut self) {
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
    pub(super) fn cancel(&mut self) -> bool {
        if self.link.is_some() { self.link = None; return false; }
        if !self.filter.is_empty() || self.hits.is_some() { self.filter.clear(); self.hits = None; self.list.select_first(); self.moved(); return false; }
        true
    }

    /// Choose the next link of the page (the previous one with `!forward`), in view.
    pub(super) fn next_link(&mut self, forward: bool) {
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

    pub(super) fn follow_chosen(&mut self) {
        if let Some(l) = self.link.and_then(|i| self.page_links().into_iter().nth(i)) { self.follow(&l.target); }
    }

    /// Follow the link under the cursor of the editor; on a footnote, go to its
    /// other end.
    pub(super) fn follow_at_cursor(&mut self) {
        if self.editor.as_mut().is_some_and(|e| e.footnote_jump()) { return; }
        match self.link_at_cursor() {
            Some(target) => self.follow(&target),
            None => self.status = "no link under the cursor".into(),
        }
    }

    /// The target of the link under the cursor of the page, if any.
    pub(super) fn link_at_cursor(&self) -> Option<String> {
        let (line, col) = self.editor.as_ref()?.line_col();
        self.page_links().into_iter().find(|l| l.line == line && (l.start..=l.end).contains(&col)).map(|l| l.target)
    }

    /// Follow the link `target` as the window does: an address or a file with the
    /// system's program, a page shown (made when there is none).
    pub(super) fn follow(&mut self, target: &str) {
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
    pub(super) fn goto_place(&mut self, place: &str) {
        let todo = self.prefs.todo();
        let Some(e) = &mut self.editor else { return };
        match links::find(&e.text(), place, todo.as_ref(), e.format()) {
            Some((at, _)) => e.goto(at),
            None => self.status = format!("{place}: not found in this page"),
        }
    }

    /// Back to the page left by following a link (`forward`: the other way).
    pub(super) fn go(&mut self, forward: bool) {
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
    pub(super) fn capture(&mut self) {
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

    pub(super) fn journal(&mut self) {
        if !self.prefs.journal { self.status = "the journal extension is off (settings)".into(); return; }
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        self.open_or_create(today);
    }

    /// Edit the page of the journal before the day edited (today, on another
    /// page), or after it with `forward`.
    pub(super) fn journal_step(&mut self, forward: bool) {
        use orgamnesia_core::agenda;
        if !self.prefs.journal { self.status = "the journal extension is off (settings)".into(); return; }
        let files = self.project.files();
        let shown = self.editor.as_ref().and_then(|e| agenda::day_of_name(&self.project.name_of(&e.path)))
            .unwrap_or_else(|| editor::now().day_number());
        match agenda::journal_step(files.iter().filter_map(|f| agenda::day_of_name(&f.name)), shown, forward) {
            Some(day) => {
                let (y, m, d) = agenda::date_of(day);
                self.open_or_create(format!("{y:04}-{m:02}-{d:02}"));
            }
            None => self.status = format!("no {} day in the journal", if forward { "later" } else { "earlier" }),
        }
    }

    /// The first shortcut of the action `id`, as the terminal shows it.
    pub(super) fn shortcut(&self, id: &str) -> Option<String> {
        self.kb.app.get(id).and_then(|b| b.iter().find(|b| !b.is_empty())).map(|b| keys::display(b))
    }

    /// Edit a page picked at random, other than the one edited.
    pub(super) fn random_page(&mut self) {
        let r = clock_nanos() as f64 / 1e9;
        let files = self.project.files();
        let shown = self.editor.as_ref().map(|e| e.path.as_str());
        match orgamnesia_core::pages::random_page(&files, shown, r).map(|f| f.path.clone()) {
            Some(path) => self.edit(&path),
            None => self.status = "no page in this project yet".into(),
        }
    }

    /// Edit the page `name`, made first when there is none.
    pub(super) fn open_or_create(&mut self, name: String) {
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
