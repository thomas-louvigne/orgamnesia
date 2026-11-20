//! The editor of the terminal: the text of a page, its cursor and mark, and
//! the editing actions of the keybinding profiles, done by the same functions
//! as the window's editor (`orgamnesia_core::{edit, motion, folding, table}`).

use orgamnesia_core::edit::{self, Stamp};
use orgamnesia_core::keybindings::EditorAction;
use orgamnesia_core::motion::{self, FormatGuard};
use orgamnesia_core::org::{line_end, line_start};
use orgamnesia_core::{folding, search::find_all, table, Format, Prefs, SearchOptions, TodoKeywords};
use unicode_width::UnicodeWidthChar;

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

    fn run(&mut self, action: EditorAction, page: usize, todo: Option<&TodoKeywords>) -> bool {
        use EditorAction::*;
        let c = self.cursor;
        let len = self.text.len();
        // Moving: the region (if any) follows
        let target = match action {
            NextLine | PreviousLine | ScrollDown | ScrollUp => {
                let n = match action { NextLine => 1, PreviousLine => -1, ScrollDown => page.max(1) as isize, _ => -(page.max(1) as isize) };
                Some(self.vertical(n))
            }
            _ => edit::motion_target(action, &self.text, c),
        };
        if let Some(t) = target {
            self.cursor = t;
            self.out_of_folds(t >= c);
            // A kill after a move starts a new entry of the ring
            self.last = None;
            return true;
        }
        match action {
            Undo => self.restore(true),
            Redo => self.restore(false),
            SetMark if !self.prefs.emacs_mark => return false,
            SetMark => self.mark = if self.mark == Some(c) { None } else { Some(c) },
            KeyboardQuit => { self.mark = None; self.zap = false; }
            SelectAll => { self.mark = Some(0); self.cursor = len; }
            KillLine => { let (a, b) = edit::kill_line_range(&self.text, c); self.kill(a, b); }
            KillWordForward => { let b = edit::word_end_forward(&self.text, c); self.kill(c, b); }
            KillWordBackward => { let a = edit::word_start_backward(&self.text, c); self.kill(a, c); }
            KillRegion | Cut => match self.region() {
                Some((a, b)) => { self.cursor = b; self.kill(a, b); }
                // Cut without a region: the key does what it usually does
                None if action == Cut => return false,
                None => {}
            },
            CopyRegion | Copy => {
                if let Some((a, b)) = self.region() {
                    let text: String = self.text[a..b].iter().collect();
                    self.push_ring(text.clone());
                    self.clipboard = Some(text);
                }
                self.mark = None;
                if action == Copy && self.clipboard.is_none() { return false; }
            }
            Yank | Paste => match self.ring.last().cloned() {
                Some(text) => {
                    self.paste(&text);
                    self.yanked = Some((c, c + text.chars().count(), self.ring.len() - 1));
                }
                None => self.message = Some("nothing to yank".into()),
            },
            YankPop => match self.yanked {
                Some((a, b, i)) if self.last == Some(Change::Yank) => {
                    let i = (i + self.ring.len() - 1) % self.ring.len();
                    let text = self.ring[i].clone();
                    let end = a + text.chars().count();
                    self.replace(a, b, &text, end, Change::Yank);
                    self.yanked = Some((a, end, i));
                }
                _ => self.message = Some("the last command was not a yank".into()),
            },
            DeleteCharForward => self.delete(),
            TransposeChars | TransposeWords | UpcaseWord | DowncaseWord | CapitalizeWord
            | DeleteHorizontalSpace | JustOneSpace | DeleteIndentation => {
                if let Some((a, b, with)) = edit::text_edit(action, &self.text, c) {
                    let cursor = edit::caret_after_edit(action, a, &with);
                    self.replace(a, b, &with, cursor, Change::Other);
                }
            }
            UpcaseRegion | DowncaseRegion => if let Some((a, b)) = self.region() {
                let case = if action == UpcaseRegion { edit::Case::Upper } else { edit::Case::Lower };
                let with = edit::change_case(&self.text[a..b], case);
                self.replace(a, b, &with, c, Change::Other);
            },
            FillParagraph => if let Some((a, b, with)) = motion::fill_paragraph(&self.text, c, edit::FILL_COLUMN) {
                // The cursor stays on the same word: as many non-blank characters before it
                let kept = self.text[a..c].iter().filter(|c| !c.is_whitespace()).count();
                let at = with.char_indices().filter(|(_, c)| !c.is_whitespace()).nth(kept)
                    .map_or(with.chars().count(), |(i, _)| with[..i].chars().count());
                self.replace(a, b, &with, a + at, Change::Other);
            },
            ZapToChar => { self.zap = true; self.message = Some("Zap to char: ".into()); }
            Recenter => {
                let row = self.row();
                self.top = row.saturating_sub(page / 2);
            }
            NewHeading => {
                let (a, b, with) = edit::new_heading(&self.text, c);
                let end = a + with.chars().count();
                self.replace(a, b, &with, end, Change::Other);
            }
            Bold | Italic | Underline | Strikethrough | Quote => {
                let (s, e) = self.region().unwrap_or((c, c));
                let (a, b, with, sel_start, sel_end) = match edit::emphasis_marks(action, self.format()) {
                    Some((open, close)) => edit::toggle_emphasis(&self.text, s, e, open, close),
                    None => edit::toggle_quote(&self.text, s, e, self.format()),
                };
                let had_region = s < e;
                self.replace(a, b, &with, sel_end, Change::Other);
                if had_region { self.mark = Some(sel_start); }
            }
            InsertDate | InsertInactiveDate => {
                let active = action == InsertDate;
                let (a, b, stamp) = match edit::stamp_around(&self.text, c) {
                    Some((a, b, s)) => (a, b, Stamp { active, ..s }),
                    None => {
                        use chrono::Datelike;
                        let now = chrono::Local::now();
                        (c, c, Stamp { year: now.year(), month: now.month(), day: now.day(), time: None, active, rest: String::new() })
                    }
                };
                let with = stamp.text();
                let end = a + with.chars().count();
                self.replace(a, b, &with, end, Change::Other);
            }
            TableNextCell | TablePrevCell => match table::move_cell(&self.text, c, action == TableNextCell) {
                Some(table::CellMove::Caret(p)) => self.cursor = p,
                Some(table::CellMove::Edit { from, to, text, caret }) => self.replace(from, to, &text, caret, Change::Other),
                // Out of a table: a headline folds as org-mode's TAB does (when the settings say so)
                None if self.prefs.tab_folds => return self.cycle(action == TableNextCell),
                None => return false,
            },
            GlobalCycle => return self.cycle(false),
            TodoNext | TodoPrev => {
                let Some(keywords) = todo else { return false };
                let (ls, le) = (line_start(&self.text, c), line_end(&self.text, c));
                let Some((a, b, with)) = keywords.cycle(&self.text[ls..le], action == TodoNext, self.format()) else { return false };
                let (a, b) = (ls + a, ls + b);
                let new_b = a + with.chars().count();
                // The cursor keeps its place in the title
                let cursor = if c >= b { c + new_b - b } else { c.min(a) };
                self.replace(a, b, &with, cursor, Change::Other);
            }
            PromoteSubtree | DemoteSubtree => {
                let demote = action == DemoteSubtree;
                match folding::shift_subtree(&self.text, self.format(), line_start(&self.text, c), demote) {
                    Ok(lines) => {
                        let mark = self.format().heading_mark();
                        self.change(Change::Other);
                        for &p in lines.iter().rev() {
                            if demote { self.text.insert(p, mark); } else { self.text.remove(p); }
                        }
                        self.folds.clear();
                        self.cursor = folding::caret_after_shift(c, &lines, demote);
                    }
                    Err(folding::ShiftError::NotHeadline) => return false,
                    Err(folding::ShiftError::TopLevel) => self.message = Some("already at the top level".into()),
                }
            }
            MoveLineUp | MoveLineDown => {
                let up = action == MoveLineUp;
                let line = line_start(&self.text, c);
                if folding::is_headline(&self.text, self.format(), line) {
                    if let Some(s) = folding::move_subtree(&self.text, self.format(), line, up) {
                        let with = s.text(&self.text);
                        let cursor = s.pos(c);
                        self.replace(s.a, s.b, &with, cursor, Change::Other);
                    }
                } else if let Some(m) = edit::move_line(&self.text, c, c, up) {
                    self.replace(m.a, m.b, &m.text, m.sel.1, Change::Other);
                }
            }
            _ => return false,
        }
        if !matches!(action, Yank | Paste | YankPop) && self.last == Some(Change::Yank) { self.last = None; }
        true
    }

    /// TAB (`!global`: Shift+TAB) folding: the headline of the cursor cycles
    /// FOLDED, CHILDREN, SUBTREE; the whole page OVERVIEW, CONTENTS, SHOW ALL.
    fn cycle(&mut self, here: bool) -> bool {
        let f = self.format();
        if here {
            let line = line_start(&self.text, self.cursor);
            let next = if folding::is_headline(&self.text, f, line) {
                match folding::cycle(&self.text, f, line, &self.folds) {
                    Some(next) => next,
                    None => { self.message = Some("empty entry".into()); return true; }
                }
            } else {
                match folding::cycle_element(&self.text, f, line, &self.folds) { Some(next) => next, None => return false }
            };
            self.folds = next;
        } else {
            let Some((shown, folds)) = folding::global_cycle(&self.text, f, &self.folds) else { return false };
            self.folds = folds;
            self.message = Some(match shown {
                folding::Global::Overview => "overview",
                folding::Global::Contents => "contents",
                folding::Global::ShowAll => "show all",
            }.into());
        }
        self.out_of_folds(false);
        true
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
        self.cursor = edit::nth_line_start(&self.text, n).min(self.text.len());
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

    /// The whole text replaced (a tag added to the page…), the cursor kept.
    pub fn set_text(&mut self, text: &str) {
        let (len, c) = (self.text.len(), self.cursor);
        self.replace(0, len, text, c, Change::Other);
    }

    /// The matches of `query` (case and accents ignored).
    pub fn find_all(&self, query: &str) -> Vec<(usize, usize)> {
        if query.is_empty() { return vec![]; }
        find_all(&self.text, query, SearchOptions { match_case: self.prefs.match_case, ..SearchOptions::default() })
    }

    /// The matches of the search going on.
    pub fn matches(&self) -> Vec<(usize, usize)> {
        self.search.as_ref().map_or(vec![], |s| self.find_all(&s.query))
    }

    /// `find` (`find_previous` with `!forward`): start a search, else go to its
    /// next match (the text last searched when nothing is typed yet).
    pub fn find(&mut self, forward: bool) {
        let Some(s) = &mut self.search else {
            self.search = Some(Search { query: String::new(), origin: self.cursor, forward, current: None, wrapped: false });
            return;
        };
        if s.query.is_empty() { s.query = self.last_query.clone(); }
        s.forward = forward;
        // Past the match the cursor is on
        let from = match s.current { Some((a, _)) if forward => a + 1, Some((a, _)) => a, None => self.cursor };
        self.locate(from, true);
    }

    /// A search of `query` started and on its first match (a project search opening a page).
    pub fn search_for(&mut self, query: &str) {
        self.search = Some(Search { query: query.to_string(), origin: self.cursor, forward: true, current: None, wrapped: false });
        self.locate(0, true);
    }

    /// A replacement of `from` starting (Emacs' `query-replace`): on its first
    /// match from the cursor; false when there is none.
    pub fn start_replace(&mut self, from: &str) -> bool {
        self.search = Some(Search { query: from.to_string(), origin: self.cursor, forward: true, current: None, wrapped: false });
        let c = self.cursor;
        self.locate(c, false);
        let found = self.search.as_ref().is_some_and(|s| s.current.is_some());
        if !found { self.search = None; }
        found
    }

    /// The match of the replacement replaced with `with` (`None`: left as it
    /// is), then on to the next one; false when there is none.
    pub fn replace_match(&mut self, with: Option<&str>) -> bool {
        let Some((a, b)) = self.search.as_ref().and_then(|s| s.current) else { return false };
        let next = match with {
            Some(with) => {
                let end = a + with.chars().count();
                self.replace(a, b, with, end, Change::Other);
                end
            }
            None => b,
        };
        self.locate(next, false);
        self.search.as_ref().is_some_and(|s| s.current.is_some())
    }

    /// Put the cursor on the first match from `from` in the way of the search,
    /// going round the page when `wrap` and there is none.
    fn locate(&mut self, from: usize, wrap: bool) {
        let all = self.matches();
        let Some(s) = &mut self.search else { return };
        let pick = |from: usize| if s.forward { all.iter().find(|m| m.0 >= from).copied() } else { all.iter().rev().find(|m| m.0 < from).copied() };
        let mut hit = pick(from);
        if hit.is_none() && wrap && !all.is_empty() {
            hit = if s.forward { all.first().copied() } else { all.last().copied() };
            s.wrapped = true;
        }
        s.current = hit;
        if let Some((a, b)) = hit {
            self.cursor = if s.forward { b } else { a };
            // A match in a fold shows it
            self.folds.retain(|&(x, y)| !(x < b && a <= y));
        }
    }

    /// The text searched typed (`Some`) or shortened (`None`): the first match
    /// from where the search started.
    pub fn search_edit(&mut self, c: Option<char>) {
        let Some(s) = &mut self.search else { return };
        match c { Some(c) => s.query.push(c), None => { s.query.pop(); } }
        let origin = s.origin;
        s.wrapped = false;
        // Backwards, the match may start at the origin
        let from = if s.forward { origin } else { origin + 1 };
        self.locate(from, false);
        if self.search.as_ref().is_some_and(|s| s.current.is_none()) { self.cursor = origin; }
    }

    /// End the search: the cursor left where it is, else (`!keep`) put back where it started.
    pub fn end_search(&mut self, keep: bool) {
        if let Some(s) = self.search.take() {
            if !s.query.is_empty() { self.last_query = s.query; }
            if !keep { self.cursor = s.origin; }
        }
        self.goal = None;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn ed(text: &str, at: usize) -> Editor {
        let mut e = Editor::new("Note.org".into(), text, true);
        e.cursor = at;
        e
    }

    #[test]
    fn types_moves_and_undoes() {
        let mut e = ed("ab\ncdef\ng", 1);
        e.insert("X");
        e.insert("Y");
        assert_eq!(e.text(), "aXYb\ncdef\ng");
        e.action("next_line", 10, None);
        assert_eq!(e.line_col(), (1, 3));
        e.action("next_line", 10, None);
        assert_eq!(e.line_col(), (2, 1));
        e.action("undo", 10, None);
        assert_eq!(e.text(), "ab\ncdef\ng");
        e.action("redo", 10, None);
        assert_eq!(e.text(), "aXYb\ncdef\ng");
        assert!(e.dirty);
    }

    #[test]
    fn kills_yanks_and_pops() {
        let mut e = ed("un\ndeux\ntrois", 0);
        e.action("kill_line", 10, None);
        e.action("kill_line", 10, None);
        assert_eq!(e.text(), "deux\ntrois");
        // A move in between: the next kill is a new entry
        e.action("beginning_of_line", 10, None);
        e.action("kill_word_forward", 10, None);
        assert_eq!(e.text(), "\ntrois");
        e.action("end_of_buffer", 10, None);
        e.action("yank", 10, None);
        assert_eq!(e.text(), "\ntroisdeux");
        e.action("yank_pop", 10, None);
        assert_eq!(e.text(), "\ntroisun\n");
        assert_eq!(e.clipboard.as_deref(), Some("deux"));
    }

    #[test]
    fn the_region_is_killed_copied_and_cased() {
        let mut e = ed("un deux trois", 3);
        e.action("set_mark", 10, None);
        e.action("forward_word", 10, None);
        assert_eq!(e.region(), Some((3, 7)));
        e.action("upcase_region", 10, None);
        assert_eq!(e.text(), "un DEUX trois");
        e.action("set_mark", 10, None);
        e.action("backward_word", 10, None);
        e.action("kill_region", 10, None);
        assert_eq!(e.text(), "un  trois");
        e.action("bold", 10, None);
        assert_eq!(e.text(), "un ** trois");
    }

    #[test]
    fn edits_as_the_window_does() {
        let mut e = ed("ab cd", 1);
        e.action("transpose_chars", 10, None);
        assert_eq!(e.text(), "ba cd");
        e.action("capitalize_word", 10, None);
        e.action("zap_to_char", 10, None);
        assert!(e.zap);
        let mut e = ed("ligne un\nligne deux", 0);
        e.action("move_line_down", 10, None);
        assert_eq!(e.text(), "ligne deux\nligne un");
        let mut e = ed("x", 1);
        e.action("insert_date", 10, None);
        assert!(e.text().starts_with("x<20"), "{}", e.text());
    }

    #[test]
    fn headlines_fold_and_shift() {
        let todo = TodoKeywords::parse("TODO | DONE");
        let mut e = ed("* Titre\ntexte\n** Sous\nplus\n* Autre", 0);
        e.action("todo_next", 10, Some(&todo));
        assert_eq!(e.text(), "* TODO Titre\ntexte\n** Sous\nplus\n* Autre");
        e.action("table_next_cell", 10, None);
        assert_eq!(e.lines_shown().iter().filter(|s| s.0).count(), 3);
        e.action("next_line", 10, None);
        assert_eq!(e.line_col().0, 4);
        e.action("global_cycle", 10, None);
        e.action("global_cycle", 10, None);
        e.action("global_cycle", 10, None);
        e.action("next_heading", 10, None);
        e.cursor = 0;
        e.action("demote_subtree", 10, None);
        assert!(e.text().starts_with("** TODO Titre\ntexte\n*** Sous"));
        e.action("next_heading", 10, None);
        e.action("next_heading", 10, None);
        e.action("new_heading", 10, None);
        assert!(e.text().ends_with("* Autre\n* "), "{}", e.text());
    }

    #[test]
    fn searches_as_one_types() {
        let mut e = ed("Média, media et MEDIA", 0);
        e.find(true);
        for c in "media".chars() { e.search_edit(Some(c)); }
        assert_eq!(e.matches().len(), 3);
        assert_eq!(e.cursor, 5);
        e.find(true);
        assert_eq!(e.cursor, 12);
        e.find(true);
        e.find(true);
        assert!(e.search.as_ref().unwrap().wrapped);
        assert_eq!(e.cursor, 5);
        e.find(false);
        assert_eq!(e.cursor, 16);
        e.end_search(false);
        assert_eq!(e.cursor, 0);
        // Searching again with nothing typed: the text last searched
        e.find(true);
        e.find(true);
        assert_eq!(e.cursor, 5);
        e.end_search(true);
        assert_eq!(e.cursor, 5);
    }

    #[test]
    fn a_click_puts_the_cursor() {
        let mut e = ed("ab
漢字x
fin", 0);
        e.click(1, 2);
        assert_eq!(e.line_col(), (1, 1));
        e.click(1, 9);
        assert_eq!(e.line_col(), (1, 3));
        e.click(7, 0);
        assert_eq!(e.line_col(), (2, 0));
    }

    #[test]
    fn follows_the_settings_of_the_editor() {
        let mut e = ed("ab", 2);
        e.type_char('(');
        assert_eq!((e.text().as_str(), e.cursor), ("ab()", 3));
        e.prefs.electric = true;
        e.mark = Some(0);
        e.cursor = 2;
        e.type_char('[');
        assert_eq!(e.text(), "[ab]()");
        e.prefs.emacs_mark = false;
        assert!(!e.action("set_mark", 10, None));
        e.prefs.match_case = true;
        assert_eq!(e.find_all("AB").len(), 0);
        e.prefs.match_case = false;
        assert_eq!(e.find_all("AB").len(), 1);
        e.prefs.numbers = true;
        assert_eq!(e.gutter(), 2);
        let mut e = ed("* Titre\ntexte", 0);
        e.prefs.tab_folds = false;
        assert!(!e.action("table_next_cell", 10, None));
        e.prefs.tab_folds = true;
        assert!(e.action("table_next_cell", 10, None));
        assert!(!e.folds.is_empty());
    }

    #[test]
    fn the_wheel_moves_the_view() {
        let text: String = (0..50).map(|n| format!("ligne {n}\n")).collect();
        let mut e = ed(&text, 0);
        e.scroll(3, 10);
        assert_eq!((e.top, e.line_col().0), (3, 3));
        e.cursor = edit::nth_line_start(&e.text, 8);
        e.scroll(3, 10);
        // Still in view: the cursor stays
        assert_eq!((e.top, e.line_col().0), (6, 8));
        e.scroll(-6, 10);
        assert_eq!((e.top, e.line_col().0), (0, 8));
        e.scroll(-3, 10);
        assert_eq!(e.top, 0);
    }
}
