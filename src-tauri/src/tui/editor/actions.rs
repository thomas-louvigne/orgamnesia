//! The editor actions of the keybindings run on the page: motions, edits,
//! emphasis, org-mode structure, folding cycles.

use super::*;

impl Editor {
    pub(super) fn run(&mut self, action: EditorAction, page: usize, todo: Option<&TodoKeywords>) -> bool {
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
            Bold | Italic | Underline | Strikethrough | Quote | Comment => {
                let (s, e) = self.region().unwrap_or((c, c));
                let (a, b, with, sel_start, sel_end) = match edit::emphasis_marks(action, self.format()) {
                    Some((open, close)) => edit::toggle_emphasis(&self.text, s, e, open, close),
                    None if action == Comment => edit::toggle_comment(&self.text, s, e, self.format()),
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
            InsertFootnote => {
                let (a, b, with, cursor) = orgamnesia_core::footnotes::insert(&self.text, c, self.format());
                self.replace(a, b, &with, cursor, Change::Other);
            }
            TodoNext | TodoPrev => {
                let Some(keywords) = todo else { return false };
                // Done, a repeated headline goes to its next dates, another gets its CLOSED date
                let Some(edits) = agenda::cycle_todo(&self.text(), self.line(), keywords, self.format(), action == TodoNext, &now()) else { return false };
                self.edits(&edits);
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
    pub(super) fn cycle(&mut self, here: bool) -> bool {
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
}
