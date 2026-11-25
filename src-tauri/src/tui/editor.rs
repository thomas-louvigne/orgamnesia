//! The editor of the terminal: the text of a page, its cursor and mark, and
//! the editing actions of the keybinding profiles, done by the same functions
//! as the window's editor (`orgamnesia_core::{edit, motion, folding, table}`).

use orgamnesia_core::edit::{self, Stamp};
use orgamnesia_core::keybindings::EditorAction;
use orgamnesia_core::motion::{self, FormatGuard};
use orgamnesia_core::org::{line_end, line_start};
use orgamnesia_core::{agenda, folding, search::find_all, table, Format, Prefs, SearchOptions, TodoKeywords};
use unicode_width::UnicodeWidthChar;

mod actions;
mod search;

/// What the last change was: a run of typing is undone at once, kills one after
/// the other go to the same entry of the kill ring, a yank can be replaced.
#[derive(PartialEq, Clone, Copy)]
enum Change { Typing, Kill, Yank, Other }

/// The settings the editor follows, as the window's editor does.
#[derive(Debug, Clone, Copy)]
pub struct EditPrefs {
    /// The search matches case (and accents).
    pub match_case: bool,
    /// Tab out of a table folds the headline (org-mode's TAB).
    pub tab_folds: bool,
    /// `set_mark` starts a region.
    pub emacs_mark: bool,
    /// A bracket or quote typed on a region wraps it; with `electric_emphasis`, an emphasis marker too.
    pub electric: bool,
    pub electric_emphasis: bool,
    /// The number of each line in the margin.
    pub numbers: bool,
}

impl From<&Prefs> for EditPrefs {
    fn from(p: &Prefs) -> Self {
        Self {
            match_case: p.find_match_case, tab_folds: p.tab_folds, emacs_mark: p.emacs_mark,
            electric: p.electric_mode, electric_emphasis: p.electric_emphasis, numbers: p.show_line_numbers,
        }
    }
}

/// Entries kept in the kill ring.
const RING: usize = 60;

pub struct Editor {
    pub path: String,
    /// A page of the project (else a file of the settings).
    pub page: bool,
    text: Vec<char>,
    /// Position of the cursor, in chars.
    pub cursor: usize,
    /// The other end of the region, when it is active (Emacs' mark).
    pub mark: Option<usize>,
    /// The column kept going up and down through shorter lines.
    goal: Option<usize>,
    /// Changed since read or saved.
    pub dirty: bool,
    undo: Vec<(Vec<char>, usize)>,
    redo: Vec<(Vec<char>, usize)>,
    last: Option<Change>,
    /// What the kills took, the last one last; `yank` gives it back.
    ring: Vec<String>,
    /// The text the last yank inserted (its range) and its entry of the ring.
    yanked: Option<(usize, usize, usize)>,
    /// What was copied or killed, to put on the system's clipboard.
    pub clipboard: Option<String>,
    /// The hidden parts of the page (see `folding`): from the `\n` ending a
    /// headline to the end of the last line hidden.
    pub folds: Vec<(usize, usize)>,
    /// First line shown (of the lines not hidden) and first column.
    pub top: usize,
    pub left: usize,
    /// The search going on (Emacs' isearch), and the text last searched.
    pub search: Option<Search>,
    last_query: String,
    /// Waiting for the character to kill up to (`zap_to_char`).
    pub zap: bool,
    /// What the last action says (a fold cycled, nothing to yank…).
    pub message: Option<String>,
    pub prefs: EditPrefs,
}

/// A search as one types: the text searched, where it started, the match the
/// cursor is on.
pub struct Search {
    pub query: String,
    origin: usize,
    forward: bool,
    pub current: Option<(usize, usize)>,
    /// Gone round past the end (the start, backwards) of the page.
    pub wrapped: bool,
}

impl Editor {
    pub fn new(path: String, text: &str, page: bool) -> Self {
        Self {
            path, page, text: text.chars().collect(), cursor: 0, mark: None, goal: None, dirty: false,
            undo: vec![], redo: vec![], last: None, ring: vec![], yanked: None, clipboard: None, folds: vec![],
            top: 0, left: 0, search: None, last_query: String::new(), zap: false, message: None,
            prefs: EditPrefs::from(&Prefs::default()),
        }
    }

    pub fn text(&self) -> String { self.text.iter().collect() }

    /// The same page in another window: on its cursor.
    pub fn twin(&self) -> Self {
        let mut e = Self::new(self.path.clone(), &self.text(), self.page);
        (e.cursor, e.top, e.left, e.dirty, e.prefs) = (self.cursor, self.top, self.left, self.dirty, self.prefs);
        e
    }

    /// The text of `other`, the same page edited in the other window: one text
    /// for both, so neither saves over what the other changed. Its undo is
    /// left behind (it would bring back the text before).
    pub fn sync_from(&mut self, other: &Editor) {
        if self.text == other.text { self.dirty = other.dirty; return; }
        self.text = other.text.clone();
        self.dirty = other.dirty;
        self.cursor = self.cursor.min(self.text.len());
        self.mark = self.mark.filter(|&m| m <= self.text.len());
        (self.folds, self.undo, self.redo, self.search, self.last) = (vec![], vec![], vec![], None, None);
    }

    pub fn format(&self) -> Format { Format::of_page(&self.path) }

    /// The region (mark to cursor), when the mark is set.
    pub fn region(&self) -> Option<(usize, usize)> {
        self.mark.map(|m| (m.min(self.cursor), m.max(self.cursor)))
    }

    /// Line and column (in chars) of the cursor.
    pub fn line_col(&self) -> (usize, usize) {
        let start = line_start(&self.text, self.cursor);
        (self.text[..start].iter().filter(|&&c| c == '\n').count(), self.cursor - start)
    }

    /// Columns the cursor is from the start of its line on screen (wide chars count two).
    pub fn screen_col(&self) -> usize {
        let start = line_start(&self.text, self.cursor);
        self.text[start..self.cursor].iter().map(|c| c.width().unwrap_or(0)).sum()
    }

    /// For each line: whether a fold hides it, and whether a fold starts at its end.
    pub fn lines_shown(&self) -> Vec<(bool, bool)> {
        let mut out = vec![];
        let mut start = 0;
        loop {
            let end = line_end(&self.text, start);
            let hidden = self.folds.iter().any(|&(a, b)| a < start && start <= b);
            out.push((hidden, self.folds.iter().any(|&(a, _)| a == end)));
            if end >= self.text.len() { return out; }
            start = end + 1;
        }
    }

    /// The fold hiding `pos`, if one does.
    fn fold_at(&self, pos: usize) -> Option<(usize, usize)> {
        self.folds.iter().copied().find(|&(a, b)| a < pos && pos <= b)
    }

    /// The cursor out of the hidden text: to the end of the headline coming
    /// back (`!forward`), else past the fold.
    fn out_of_folds(&mut self, forward: bool) {
        while let Some((a, b)) = self.fold_at(self.cursor) {
            self.cursor = if forward && b < self.text.len() { b + 1 } else { a };
        }
    }

    /// Before a change of kind `kind`: what to go back to with `undo`.
    fn change(&mut self, kind: Change) {
        if kind == Change::Other || kind == Change::Yank || self.last != Some(kind) {
            self.undo.push((self.text.clone(), self.cursor));
        }
        self.redo.clear();
        self.last = Some(kind);
        self.dirty = true;
    }

    /// The text `a..b` replaced with `with`, the cursor at `cursor`. The folds
    /// after it follow; one it reaches into opens (nothing changes out of sight).
    fn replace(&mut self, a: usize, b: usize, with: &str, cursor: usize, kind: Change) {
        self.change(kind);
        let new: Vec<char> = with.chars().collect();
        let grown = new.len() as isize - (b - a) as isize;
        self.text.splice(a..b, new);
        // A fold hides its chars x..=y: one the change removes or inserts into opens
        self.folds.retain(|&(x, y)| if a < b { !(a <= y && b > x) } else { !(x < a && a <= y) });
        for f in &mut self.folds {
            if f.0 >= b { *f = (f.0.saturating_add_signed(grown), f.1.saturating_add_signed(grown)); }
        }
        self.cursor = cursor.min(self.text.len());
        self.mark = None;
        self.goal = None;
    }

    /// Enter: in a cell of a table, to the cell below (org-mode's `org-table-next-row`);
    /// elsewhere, a new line.
    pub fn enter(&mut self) {
        let _format = FormatGuard::set(self.format());
        match table::next_row(&self.text, self.cursor) {
            Some(table::CellMove::Caret(p)) => self.cursor = p,
            Some(table::CellMove::Edit { from, to, text, caret }) => self.replace(from, to, &text, caret, Change::Other),
            None => self.insert("\n"),
        }
    }

    pub fn insert(&mut self, s: &str) {
        let kind = if s.chars().all(|c| !c.is_whitespace()) { Change::Typing } else { Change::Other };
        // A new line at the end of a folded headline comes after its hidden text
        let a = match self.folds.iter().find(|f| f.0 == self.cursor) {
            Some(f) if s == "\n" => f.1,
            _ => self.cursor,
        };
        self.replace(a, a, s, a + s.chars().count(), kind);
    }

    /// A character typed, as the window's editor takes it: on a region, a
    /// bracket or quote wraps it (electric mode), an emphasis marker too
    /// (`electric_emphasis`); else `( [ { "` come with their closing one.
    pub fn type_char(&mut self, ch: char) {
        let key = ch.to_string();
        // `[` on the region: a link to it, the region done with
        if let Some((a, b)) = self.region() {
            let selected: String = self.text[a..b].iter().collect();
            if let Some(link) = edit::link_of(&key, &selected) {
                let end = a + link.chars().count();
                self.replace(a, b, &link, end, Change::Other);
                self.mark = None;
                return;
            }
        }
        if let Some((a, b)) = self.region().filter(|_| self.prefs.electric) {
            let emphasis = || self.prefs.electric_emphasis.then(|| edit::emphasis_pair(&key, self.format())).flatten();
            if let Some((open, close)) = edit::pair_of(&key).or_else(emphasis) {
                let inner: String = self.text[a..b].iter().collect();
                let end = b + open.chars().count();
                self.replace(a, b, &format!("{open}{inner}{close}"), end, Change::Other);
                self.mark = Some(a + open.chars().count());
                return;
            }
        }
        if self.mark.is_none() && edit::auto_closes(&key) && let Some((open, close)) = edit::pair_of(&key) {
            let c = self.cursor;
            self.replace(c, c, &format!("{open}{close}"), c + open.chars().count(), Change::Typing);
            return;
        }
        self.insert(&key);
    }

    /// Columns of the line numbers in the margin (and the space after them); none without them.
    pub fn gutter(&self) -> usize {
        if !self.prefs.numbers { return 0; }
        (self.text.iter().filter(|&&c| c == '\n').count() + 1).to_string().len() + 1
    }

    pub fn backspace(&mut self) {
        if self.cursor == 0 { return; }
        let c = self.cursor;
        self.replace(c - 1, c, "", c - 1, Change::Typing);
    }

    pub fn delete(&mut self) {
        let (a, b) = edit::delete_forward_range(&self.text, self.cursor, self.cursor);
        if a < b { self.replace(a, b, "", a, Change::Typing); }
    }

    /// Kill `a..b` into the kill ring: with the kills just before, in one entry.
    fn kill(&mut self, a: usize, b: usize) {
        if a >= b { return; }
        let killed: String = self.text[a..b].iter().collect();
        if self.last == Some(Change::Kill) && let Some(top) = self.ring.last_mut() {
            if a < self.cursor { top.insert_str(0, &killed); } else { top.push_str(&killed); }
        } else {
            self.push_ring(killed);
        }
        self.clipboard = self.ring.last().cloned();
        self.replace(a, b, "", a, Change::Kill);
    }

    fn push_ring(&mut self, text: String) {
        self.ring.push(text);
        if self.ring.len() > RING { self.ring.remove(0); }
    }

    /// Text pasted from the terminal (or given back by `yank`) at the cursor.
    pub fn paste(&mut self, text: &str) {
        let at = self.cursor;
        self.replace(at, at, text, at + text.chars().count(), Change::Yank);
    }

    /// Do the editing action `id` (`page` lines a screen, `todo` the keywords
    /// when they are on); false when the editor has none such, or it has
    /// nothing to do here (the key then does what it usually does).
    pub fn action(&mut self, id: &str, page: usize, todo: Option<&TodoKeywords>) -> bool {
        let Some(action) = EditorAction::from_id(id) else { return false };
        let _format = FormatGuard::set(self.format());
        self.message = None;
        let was_yank = matches!(action, EditorAction::YankPop);
        let done = self.run(action, page, todo);
        if !was_yank && !matches!(action, EditorAction::Yank) { self.yanked = None; }
        if done && !matches!(action, EditorAction::NextLine | EditorAction::PreviousLine | EditorAction::ScrollDown | EditorAction::ScrollUp) {
            self.goal = None;
        }
        done
    }

    /// The position `n` lines below (above when negative) on screen, the
    /// hidden lines skipped, keeping the column.
    fn vertical(&mut self, n: isize) -> usize {
        let (line, col) = self.line_col();
        let goal = *self.goal.get_or_insert(col);
        let shown = self.lines_shown();
        let visible: Vec<usize> = (0..shown.len()).filter(|&l| !shown[l].0).collect();
        let row = visible.iter().position(|&l| l >= line).unwrap_or(visible.len().saturating_sub(1));
        let row = (row as isize + n).clamp(0, visible.len() as isize - 1) as usize;
        let start = edit::nth_line_start(&self.text, visible[row]);
        (start + goal).min(line_end(&self.text, start))
    }

    /// The cursor put where the screen shows row `row` (of the lines shown,
    /// from the top of the page) and column `col` (past its end: its end).
    pub fn click(&mut self, row: usize, col: usize) {
        let shown = self.lines_shown();
        let Some(line) = (0..shown.len()).filter(|&l| !shown[l].0).nth(row).or_else(|| (0..shown.len()).rfind(|&l| !shown[l].0)) else { return };
        let start = edit::nth_line_start(&self.text, line);
        let end = line_end(&self.text, start);
        let mut x = 0;
        self.cursor = (start..end).find(|&i| {
            let w = self.text[i].width().unwrap_or(0);
            // On the left half of a wide char: before it
            let hit = col < x + w.max(1);
            x += w;
            hit
        }).unwrap_or(end);
        self.goal = None;
        self.last = None;
        self.yanked = None;
    }

    /// The view moved `n` lines down (up when negative), as the mouse wheel does;
    /// the cursor follows only when it would leave the `height` lines shown.
    pub fn scroll(&mut self, n: isize, height: usize) {
        let rows = self.lines_shown().iter().filter(|(hidden, _)| !hidden).count();
        self.top = (self.top as isize + n).clamp(0, rows.saturating_sub(1) as isize) as usize;
        let row = self.row();
        if row < self.top {
            self.cursor = self.vertical((self.top - row) as isize);
        } else if height > 0 && row >= self.top + height {
            self.cursor = self.vertical(-((row + 1 - self.top - height) as isize));
        }
    }

    /// The cursor at the start of the line `n` (from 0), shown (out of any fold).
    pub fn goto_line(&mut self, n: usize) {
        self.goto(edit::nth_line_start(&self.text, n));
    }

    /// On a footnote, the cursor to its definition, or back to its reference.
    /// False when it is on none (or the other end is missing).
    pub fn footnote_jump(&mut self) -> bool {
        match orgamnesia_core::footnotes::other_end(&self.text, self.cursor, self.format()) {
            Some(to) => { self.goto(to); true }
            None => false,
        }
    }

    /// The cursor at the char `at`, shown (out of any fold).
    pub fn goto(&mut self, at: usize) {
        self.cursor = at.min(self.text.len());
        let c = self.cursor;
        self.folds.retain(|&(a, b)| !(a < c && c <= b));
        self.goal = None;
    }

    /// Row of the cursor among the lines shown.
    pub fn row(&self) -> usize {
        let (line, _) = self.line_col();
        self.lines_shown()[..line].iter().filter(|(hidden, _)| !hidden).count()
    }

    /// Put back the text before the last change (`undo`), or after it.
    fn restore(&mut self, undo: bool) {
        let (from, to) = if undo { (&mut self.undo, &mut self.redo) } else { (&mut self.redo, &mut self.undo) };
        let Some((text, cursor)) = from.pop() else { self.message = Some("no further undo information".into()); return };
        to.push((std::mem::replace(&mut self.text, text), self.cursor));
        self.cursor = cursor.min(self.text.len());
        self.folds.clear();
        self.mark = None;
        self.last = None;
        self.dirty = true;
    }

    /// `zap_to_char` once its character is typed: kill up to and including it.
    pub fn zap_to(&mut self, ch: char) {
        self.zap = false;
        match edit::zap_end(&self.text, self.cursor, ch) {
            Some(b) => { let c = self.cursor; self.last = None; self.kill(c, b); }
            None => self.message = Some(format!("“{ch}” not found")),
        }
    }

    /// The range of the link around the cursor, its target and text; else the
    /// region (or the cursor) and its text.
    pub fn link_here(&self) -> (usize, usize, String, String) {
        match edit::link_around(&self.text, self.cursor) {
            Some(l) => (l.start, l.end, l.target, l.description),
            None => {
                let (a, b) = self.region().unwrap_or((self.cursor, self.cursor));
                (a, b, String::new(), self.text[a..b].iter().collect())
            }
        }
    }

    /// The date around the cursor (else today's), `active` or not, its range.
    pub fn date_here(&self, active: bool) -> (usize, usize, Stamp) {
        match edit::stamp_around(&self.text, self.cursor) {
            Some((a, b, s)) => (a, b, Stamp { active, ..s }),
            None => {
                use chrono::Datelike;
                let now = chrono::Local::now();
                (self.cursor, self.cursor, Stamp { year: now.year(), month: now.month(), day: now.day(), time: None, active, rest: String::new() })
            }
        }
    }

    /// `a..b` replaced with `with`, the cursor after it (a link, a date written).
    pub fn put(&mut self, a: usize, b: usize, with: &str) {
        let end = a + with.chars().count();
        self.replace(a, b, with, end, Change::Other);
    }

    /// The line of the cursor (from 0).
    pub fn line(&self) -> usize {
        self.text[..self.cursor].iter().filter(|&&c| c == '\n').count()
    }

    /// The edits of the agenda (in the order of the text) made as one change,
    /// the cursor moved with them.
    pub fn edits(&mut self, edits: &[agenda::Edit]) {
        let (Some(first), Some(last)) = (edits.first(), edits.last()) else { return };
        let (a, b) = (first.a, last.b.min(self.text.len()));
        let span: String = self.text[a..b].iter().collect();
        let local: Vec<agenda::Edit> = edits.iter().map(|e| agenda::Edit { a: e.a - a, b: e.b - a, text: e.text.clone() }).collect();
        let cursor = agenda::moved(self.cursor, edits);
        self.replace(a, b, &agenda::apply(&span, &local), cursor, Change::Other);
    }

    /// The whole text replaced (a tag added to the page…), the cursor kept.
    pub fn set_text(&mut self, text: &str) {
        let (len, c) = (self.text.len(), self.cursor);
        self.replace(0, len, text, c, Change::Other);
    }

    /// Scroll so the cursor is in the `height` × `width` view.
    pub fn follow_cursor(&mut self, height: usize, width: usize) {
        let row = self.row();
        let col = self.screen_col();
        if row < self.top { self.top = row; }
        if height > 0 && row >= self.top + height { self.top = row + 1 - height; }
        if col < self.left { self.left = col; }
        if width > 0 && col >= self.left + width { self.left = col + 1 - width; }
    }
}

/// Now, as the inactive timestamp org-mode writes when a headline is done.
pub fn now() -> Stamp {
    use chrono::{Datelike, Timelike};
    let n = chrono::Local::now();
    Stamp { year: n.year(), month: n.month(), day: n.day(), time: Some((n.hour(), n.minute())), active: false, rest: String::new() }
}

#[cfg(test)]
mod tests;
