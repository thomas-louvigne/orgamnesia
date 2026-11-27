//! The keys and the mouse of the terminal interface: what has them (a popup,
//! a line typed at the bottom, the quick open, a frame, the suggestions, the
//! editor, the list of the pages) and what they do there.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::widgets::ListState;

use super::*;

impl App {
    /// Handle `key`: what has the keys first (see `Mode`), then the shortcuts of
    /// the window, then the keys of the editor or of the list. Returns whether to quit.
    pub(super) fn key(&mut self, key: KeyEvent) -> bool {
        // Each mode its keys; a frame or the suggestions let the others go on
        let taken = match self.mode {
            Mode::Normal => false,
            Mode::Popup(_) => { self.popup_key(key); true }
            Mode::Prompt(_) => { self.prompt_key(key); true }
            Mode::Quick(_) => return self.quick_key(key),
            Mode::Replace(..) => { self.replace_key(key); true }
            Mode::Frame(_) => self.frame_key(key),
            Mode::Suggest(_) => self.suggest_key(key),
        };
        if taken { return false; }
        if let Some(e) = &mut self.editor && e.zap {
            let plain = !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
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
                    if let Some(quit) = self.run_action(&id) { return quit; }
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

    /// Do the shortcut `id` (a key or the quick open's `>`): `None` when nothing
    /// does it here, else whether to quit.
    pub(super) fn run_action(&mut self, id: &str) -> Option<bool> {
        if id == "find" || id == "find_previous" {
            // Among the pages, a search in the page chosen
            if self.editor.is_none() && let Some(p) = self.selected() { self.edit(&p.path); }
            if let Some(e) = &mut self.editor { e.find(id == "find"); }
            return Some(false);
        }
        // Any other shortcut ends the search (`keyboard_quit` cancels it), then does its work
        if let Some(e) = &mut self.editor && e.search.is_some() {
            e.end_search(id != "keyboard_quit");
            if id == "keyboard_quit" { return Some(false); }
        }
        if let Some(quit) = self.action(id) { return Some(quit); }
        if self.command(id).is_some() { return Some(false); }
        let page = self.height as usize;
        let todo = self.prefs.todo();
        if let Some(e) = &mut self.editor && e.action(id, page, todo.as_ref()) { return Some(false); }
        None
    }

    /// A key on text or a list over the page: moving, opening the line chosen;
    /// another key closes it.
    fn popup_key(&mut self, key: KeyEvent) {
        let next = keys::bound(&self.kb, &key, "next_line");
        let previous = keys::bound(&self.kb, &key, "previous_line");
        let half = self.height / 2;
        // The agenda: its keys (S-→/S-← a day later/earlier)
        if matches!(&self.mode, Mode::Popup(p) if p.agenda.is_some()) {
            let shift = key.modifiers.contains(KeyModifiers::SHIFT);
            let done = match key.code {
                KeyCode::Right if shift => self.agenda_key('+'),
                KeyCode::Left if shift => self.agenda_key('-'),
                KeyCode::Char(c) => self.agenda_key(c),
                _ => false,
            };
            if done { return; }
        }
        let Mode::Popup(p) = &mut self.mode else { return };
        let list = !p.targets.is_empty();
        match key.code {
            // A list: the line chosen, opened with Enter
            KeyCode::Enter if list => self.open_target(),
            KeyCode::Down if list => p.step(true),
            KeyCode::Up if list => p.step(false),
            _ if list && next => p.step(true),
            _ if list && previous => p.step(false),
            KeyCode::PageDown if list => for _ in 0..10 { p.step(true) },
            KeyCode::PageUp if list => for _ in 0..10 { p.step(false) },
            KeyCode::Down => p.scroll = p.scroll.saturating_add(1),
            KeyCode::Up => p.scroll = p.scroll.saturating_sub(1),
            KeyCode::PageDown => p.scroll = p.scroll.saturating_add(half),
            KeyCode::PageUp => p.scroll = p.scroll.saturating_sub(half),
            // The commit's files: ticked, committed; a commit of the history: put back
            KeyCode::Char(' ') if p.targets.iter().any(|t| matches!(t, frames::Target::Change { .. })) => self.tick_change(),
            KeyCode::Char('c') if p.targets.iter().any(|t| matches!(t, frames::Target::Change { .. })) => self.commit_ticked(),
            KeyCode::Char('r') if matches!(p.targets.get(p.selected), Some(frames::Target::Commit { .. })) => self.restore_version(),
            KeyCode::Char('l') if matches!(p.targets.get(p.selected), Some(frames::Target::Unlinked(_))) => {
                if let Some(frames::Target::Unlinked(m)) = p.targets.get(p.selected).cloned() { self.link_mention(&m); }
            }
            // Closed: back to the popup it came from, if any
            _ => {
                let back = p.parent.take();
                self.mode = back.map_or(Mode::Normal, |b| Mode::Popup(*b));
            }
        }
    }

    /// A key on the value typed at the bottom: typing, Tab completing, Enter giving it.
    fn prompt_key(&mut self, key: KeyEvent) {
        let plain = !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        let Mode::Prompt(p) = &mut self.mode else { return };
        match key.code {
            _ if cancel(&self.kb, &key) => { self.mode = Mode::Normal; self.status.clear(); }
            KeyCode::Enter => self.submit(),
            KeyCode::Tab => if let Some(names) = p.completes() { self.complete_prompt(names) },
            KeyCode::Backspace => { p.input.pop(); p.choices.clear(); }
            KeyCode::Char(c) if plain => { p.input.push(c); p.choices.clear(); }
            _ => {}
        }
    }

    /// A key during the page's replacement: `y`/Space replace, `n` skip, `!` all; another stops.
    fn replace_key(&mut self, key: KeyEvent) {
        let Mode::Replace(_, with) = &self.mode else { return };
        let with = with.clone();
        let Some(e) = &mut self.editor else { self.mode = Mode::Normal; return };
        let more = match key.code {
            KeyCode::Char('y' | ' ') => e.replace_match(Some(&with)),
            KeyCode::Char('n') | KeyCode::Delete | KeyCode::Backspace => e.replace_match(None),
            KeyCode::Char('!') => { while e.replace_match(Some(&with)) {} false }
            _ => false,
        };
        if !more { self.mode = Mode::Normal; e.end_search(true); }
    }

    /// A key while names are suggested: moving among them, accepting one, closing
    /// them. False when the key goes on (typing, which refreshes them).
    fn suggest_key(&mut self, key: KeyEvent) -> bool {
        let (next, previous) = (keys::bound(&self.kb, &key, "next_line"), keys::bound(&self.kb, &key, "previous_line"));
        let Mode::Suggest(s) = &mut self.mode else { return false };
        let n = s.items.len();
        match key.code {
            KeyCode::Down => s.selected = (s.selected + 1) % n,
            KeyCode::Up => s.selected = (s.selected + n - 1) % n,
            _ if next => s.selected = (s.selected + 1) % n,
            _ if previous => s.selected = (s.selected + n - 1) % n,
            KeyCode::Tab | KeyCode::Enter if key.modifiers.is_empty() => self.accept_suggest(),
            _ if cancel(&self.kb, &key) => self.mode = Mode::Normal,
            // Typing goes on (and refreshes them); anything else closes them, then does its work
            KeyCode::Char(_) | KeyCode::Backspace if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => return false,
            _ => { self.mode = Mode::Normal; return false; }
        }
        true
    }

    /// A key among the pages.
    pub(super) fn list_key(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => return self.cancel(),
            KeyCode::Down => { self.list.select_next(); self.moved(); }
            KeyCode::Up => { self.list.select_previous(); self.moved(); }
            KeyCode::PageDown => self.scroll = self.scroll.saturating_add(self.height / 2),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(self.height / 2),
            KeyCode::Tab => self.next_link(true),
            KeyCode::BackTab => self.next_link(false),
            KeyCode::Enter if self.link.is_some() => self.follow_chosen(),
            KeyCode::Enter => if let Some(page) = self.selected() { self.edit(&page.path); } else { self.fold_folder(None); },
            KeyCode::Right => { self.fold_folder(Some(true)); }
            KeyCode::Left => { self.fold_folder(Some(false)); }
            KeyCode::Backspace => { self.filter.pop(); self.list.select_first(); self.moved(); }
            KeyCode::Char(c) if !ctrl => { self.filter.push(c); self.list.select_first(); self.moved(); }
            _ => {}
        }
        false
    }

    /// A key in the editor: typing, and the keys every editor has.
    pub(super) fn edit_key(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let page = self.height as usize;
        let Some(e) = &mut self.editor else { return false };
        let act = |e: &mut Editor, id: &str| { e.action(id, page, None); };
        match key.code {
            KeyCode::Esc => { self.back_to_list(); }
            KeyCode::Char(c) if !ctrl && !key.modifiers.contains(KeyModifiers::ALT) => e.type_char(c),
            KeyCode::Enter => e.enter(),
            KeyCode::Tab => e.insert("\t"),
            KeyCode::Backspace => e.backspace(),
            KeyCode::Delete => e.delete(),
            KeyCode::Left if ctrl => act(e, "backward_word"),
            KeyCode::Right if ctrl => act(e, "forward_word"),
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
        if typing { self.refresh_suggest(); } else if matches!(self.mode, Mode::Suggest(_)) { self.mode = Mode::Normal; }
        false
    }

    /// Do the action `id` of the window's shortcuts: `None` when the window
    /// does not do it here (an editing action, or none), else whether to quit.
    pub(super) fn action(&mut self, id: &str) -> Option<bool> {
        let editing = self.editor.is_some();
        match id {
            "quit" => return Some(true),
            "save" if editing => { self.save(); }
            // In the editor, it lets go of the region (and of a zap waiting for its character)
            "keyboard_quit" => {
                match &mut self.editor { Some(e) => { e.action("keyboard_quit", 0, None); } None => { self.cancel(); } }
                self.status.clear();
            }
            "quick_open" => self.mode = Mode::Quick(quick::Quick::default()),
            "new_page" => {
                // The filter of the list names it; otherwise asked, as Emacs' find-file
                let name = if editing { String::new() } else { self.filter.trim().to_string() };
                if name.is_empty() { self.ask("Find or create page: ", "", commands::Then::FindPage); }
                else { self.open_or_create(name); }
            }
            "journal" => self.journal(),
            "journal_previous" => self.journal_step(false),
            "journal_next" => self.journal_step(true),
            "capture" => self.capture(),
            "new_folder" | "move_page" | "rename_folder" | "delete_folder" => { self.folder_action(id); }
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

    /// A key on the frame with the focus: moving, opening; another key leaves it.
    /// True when the key was the frame's.
    pub(super) fn frame_key(&mut self, key: KeyEvent) -> bool {
        let Some(frame) = self.frame_focus() else { return false };
        let (next, previous) = (keys::bound(&self.kb, &key, "next_line"), keys::bound(&self.kb, &key, "previous_line"));
        let (_, items) = self.frame_items(frame);
        let state = self.frame_state.entry(frame).or_default();
        let step = |state: &mut ListState, forward: bool| {
            let n = items.len();
            let mut i = state.selected().unwrap_or(0);
            for _ in 0..n {
                i = if forward { (i + 1).min(n.saturating_sub(1)) } else { i.saturating_sub(1) };
                if items[i].1 != frames::Target::None { state.select(Some(i)); return; }
            }
        };
        match key.code {
            KeyCode::Down => step(state, true),
            KeyCode::Up => step(state, false),
            _ if next => step(state, true),
            _ if previous => step(state, false),
            KeyCode::Enter => if let Some((_, target)) = state.selected().and_then(|i| items.get(i)).cloned() { self.open(target); },
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Char('l') if matches!(state.selected().and_then(|i| items.get(i)), Some((_, frames::Target::Unlinked(_)))) => {
                if let Some((_, frames::Target::Unlinked(m))) = state.selected().and_then(|i| items.get(i)).cloned() { self.link_mention(&m); }
            }
            _ => { self.mode = Mode::Normal; return false; }
        }
        true
    }

    /// A key in the quick open: typing, moving among the pages found, Enter
    /// opening the one chosen (made when it is to create).
    pub(super) fn quick_key(&mut self, key: KeyEvent) -> bool {
        let (next, previous) = (keys::bound(&self.kb, &key, "next_line"), keys::bound(&self.kb, &key, "previous_line"));
        let plain = !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        let quit = cancel(&self.kb, &key);
        let Mode::Quick(q) = &mut self.mode else { return false };
        match key.code {
            _ if quit => self.mode = Mode::Normal,
            KeyCode::Down | KeyCode::Tab => q.selected += 1,
            _ if next => q.selected += 1,
            KeyCode::Up | KeyCode::BackTab => q.selected = q.selected.saturating_sub(1),
            _ if previous => q.selected = q.selected.saturating_sub(1),
            KeyCode::Backspace => { q.query.pop(); q.selected = 0; }
            KeyCode::Char(c) if plain || key.modifiers == KeyModifiers::SHIFT => { q.query.push(c); q.selected = 0; }
            KeyCode::Enter => {
                let (query, selected) = (q.query.clone(), q.selected);
                let chosen = self.quick_list(&query).into_iter().nth(selected);
                self.mode = Mode::Normal;
                match chosen {
                    Some(quick::QuickItem::Page { path, .. }) => { self.hits = None; self.edit(&path); }
                    Some(quick::QuickItem::Create(name)) => self.open_or_create(name),
                    Some(quick::QuickItem::Command { id, .. }) => return self.run_action(&id).unwrap_or(false),
                    Some(item) => self.template_chosen(item),
                    None => {}
                }
            }
            _ => {}
        }
        false
    }

    /// The border between the pages and the page, dragged with the left button.
    pub(super) fn mouse(&mut self, m: MouseEvent) {
        self.set_pointer(self.dragging.is_some() || self.border_at(m.column).is_some());
        match m.kind {
            MouseEventKind::Down(MouseButton::Left) if self.border_at(m.column).is_some() => self.dragging = self.border_at(m.column),
            MouseEventKind::Down(MouseButton::Left) if self.popup().is_some_and(|p| !p.targets.is_empty()) => self.click_popup(m.row),
            MouseEventKind::Down(MouseButton::Left) if self.tab_bar.contains(ratatui::layout::Position::new(m.column, m.row)) => self.click_tab(m.column),
            MouseEventKind::Down(MouseButton::Left) if self.click_frame(m.column, m.row, None) => {}
            MouseEventKind::Down(MouseButton::Left) => {
                if matches!(self.mode, Mode::Frame(_)) { self.mode = Mode::Normal; }
                self.click_list(m.column, m.row);
                // Ctrl+click on a link follows it, as in the window; a plain click too when the settings say so
                if !self.click_page(m.column, m.row) {
                } else if m.modifiers.contains(KeyModifiers::CONTROL) {
                    self.follow_at_cursor();
                } else if m.modifiers.is_empty() && self.prefs.click_follows_links
                    && let Some(target) = self.link_at_cursor() {
                    self.follow(&target);
                }
            }
            MouseEventKind::Down(MouseButton::Right) => self.format_menu(m.column, m.row),
            MouseEventKind::Drag(MouseButton::Left) if self.dragging == Some(false) => self.split = Some(m.column + 1),
            MouseEventKind::Drag(MouseButton::Left) if self.dragging == Some(true) => self.right_split = Some(self.total.saturating_sub(m.column)),
            MouseEventKind::ScrollDown => self.wheel(m.column, m.row, 3),
            MouseEventKind::ScrollUp => self.wheel(m.column, m.row, -3),
            MouseEventKind::Up(_) => self.dragging = None,
            _ => {}
        }
    }

    /// Width of the column of the pages, for a screen `total` wide: the page keeps 20 columns.
    pub(super) fn split_width(&self, total: u16) -> u16 {
        self.split.unwrap_or((total / 5).clamp(24, 40)).min(total.saturating_sub(20)).max(8)
    }

    /// A click on a frame of a column (`wheel`: the wheel turned so many lines over it):
    /// the focus on it, the line clicked opened. False when (`column`, `row`) is in none.
    pub(super) fn click_frame(&mut self, column: u16, row: u16, wheel: Option<isize>) -> bool {
        use orgamnesia_core::Frame;
        let inside = |a: &Rect| column >= a.x && column < a.x + a.width && row >= a.y && row < a.y + a.height;
        let Some((frame, a)) = self.frame_areas.iter().find(|(_, a)| inside(a)).copied() else { return false };
        // The pages have their own clicks; the lines of the project and git nothing
        if frame == Frame::Pages { return false; }
        if frame == Frame::Tips { self.tip += 1; return true; }
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
        if let Some((_, target)) = items.get(i).cloned() && target != frames::Target::None {
            state.select(Some(i));
            self.open(target);
        }
        true
    }

    /// A right click in the page: the region kept (else the cursor there), and the
    /// menu of what to make of it: bold, italic, underline, strikethrough, quote.
    pub(super) fn format_menu(&mut self, column: u16, row: u16) {
        if !matches!(self.mode, Mode::Normal) { return; }
        let region = self.editor.as_ref().is_some_and(|e| e.region().is_some_and(|(a, b)| a < b));
        if !region && !self.click_page(column, row) { return; }
        let item = |label: &str, id| (format!(" {label}"), frames::Target::Action(id));
        let mut items = vec![];
        if region {
            items.extend([item("Bold", "bold"), item("Italic", "italic"), item("Underline", "underline"), item("Strikethrough", "strikethrough")]);
        }
        items.extend([item("Quote", "quote"), item("Comment", "comment")]);
        self.mode = Mode::Popup(frames::Popup::pick("Format".into(), items));
    }

    /// A click on a line of a popup list: that line opened.
    pub(super) fn click_popup(&mut self, row: u16) {
        let a = self.page_area_popup();
        let Mode::Popup(p) = &mut self.mode else { return };
        if row <= a.y || row + 1 >= a.y + a.height { self.mode = Mode::Normal; return; }
        let i = p.scroll as usize + (row - a.y - 1) as usize;
        if p.targets.get(i).is_some_and(|t| *t != frames::Target::None) {
            p.selected = i;
            self.open_target();
        }
    }

    /// A click in the list of the pages: back to it (the page edited saved),
    /// on the page clicked.
    pub(super) fn click_list(&mut self, column: u16, row: u16) {
        let a = self.list_area;
        // Inside its borders
        if column <= a.x || column + 1 >= a.x + a.width || row <= a.y || row + 1 >= a.y + a.height { return; }
        if !self.back_to_list() { return; }
        let i = self.list.offset() + (row - a.y - 1) as usize;
        if i < self.rows().len() && self.list.selected() != Some(i) {
            self.list.select(Some(i));
            self.moved();
        // A folder clicked again: folded or unfolded
        } else if self.list.selected() == Some(i) {
            self.fold_folder(None);
        }
    }

    /// The mouse wheel turned `n` lines over (`column`, `row`): the window
    /// there scrolls, or the list of the pages moves.
    pub(super) fn wheel(&mut self, column: u16, row: u16, n: isize) {
        let inside = |a: Rect| column >= a.x && column < a.x + a.width && row >= a.y && row < a.y + a.height;
        let height = self.height as usize;
        if let Mode::Popup(p) = &mut self.mode {
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

    /// A click in the page: the cursor there (the page edited first when it is
    /// not); the other window clicked becomes the active one. True when the click put the cursor.
    pub(super) fn click_page(&mut self, column: u16, row: u16) -> bool {
        let inside = |a: Rect| column > a.x && column + 1 < a.x + a.width && row > a.y && row + 1 < a.y + a.height;
        if self.other.is_some() && inside(self.other_area) {
            self.edit_previewed();
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

    /// Text pasted in the terminal: in the value typed at the bottom, else in the page.
    pub(super) fn paste(&mut self, text: &str) {
        match &mut self.mode {
            Mode::Quick(q) => q.query.push_str(text.lines().next().unwrap_or_default()),
            Mode::Prompt(p) => p.input.push_str(text.lines().next().unwrap_or_default()),
            _ => if let Some(e) = &mut self.editor { e.paste(text); },
        }
    }
}

/// Esc or the user's `keyboard_quit`: what closes a popup, a line typed, the suggestions.
fn cancel(kb: &Keybindings, key: &KeyEvent) -> bool {
    key.code == KeyCode::Esc || keys::bound(kb, key, "keyboard_quit")
}
