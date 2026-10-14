//! The undo history of a page: the states it went through, kept by the editor
//! itself. The textarea's own history can't be used: it is lost each time the
//! editor gives the textarea a new view of the page (a table drawn, the
//! indentation under a headline, a fold…).

/// A state of the page, and where the caret goes when it comes back.
#[derive(Debug, Clone, PartialEq)]
struct Step {
    page: String,
    caret: usize,
}

/// Steps kept at most; the oldest go first.
const MAX_STEPS: usize = 300;
/// A pause longer than this (ms) ends a run of typing: the next key starts a new step.
const PAUSE_MS: f64 = 1000.0;
/// Edits closer than this (ms) are parts of a single command (a headline and its
/// sub-headlines demoted line by line…): one step.
const BURST_MS: f64 = 30.0;

#[derive(Debug, Default)]
pub struct History {
    undo: Vec<Step>,
    redo: Vec<Step>,
    /// When and where (char position after it) the last edit was made.
    last: Option<(f64, usize)>,
}

/// Where `a` and `b` differ: the length of their common start, and the chars
/// that differ in each (once their common end is left out).
fn diff(a: &[char], b: &[char]) -> (usize, usize, usize) {
    let prefix = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let suffix = a[prefix..].iter().rev().zip(b[prefix..].iter().rev()).take_while(|(x, y)| x == y).count();
    (prefix, a.len() - prefix - suffix, b.len() - prefix - suffix)
}

impl History {
    /// The page went from `old` to `new` at `now` (ms). A key typed or erased
    /// right after the last one, at the same place, joins its step, as does the
    /// rest of a command made of several edits; any other edit (a paste, a moved
    /// line, a new line…) starts a new one.
    pub fn record(&mut self, old: &str, new: &str, now: f64) {
        if old == new { return; }
        let (o, n): (Vec<char>, Vec<char>) = (old.chars().collect(), new.chars().collect());
        let (at, removed, added) = diff(&o, &n);
        let one_key = removed + added == 1 && !(added == 1 && n[at] == '\n');
        let follows = self.last.is_some_and(|(t, p)| now - t < PAUSE_MS && (p == at || p == at + 1));
        let burst = self.last.is_some_and(|(t, _)| now - t < BURST_MS);
        if !(burst || one_key && follows) {
            self.undo.push(Step { page: old.to_string(), caret: at + removed });
            if self.undo.len() > MAX_STEPS { self.undo.remove(0); }
        }
        self.redo.clear();
        self.last = Some((now, at + added));
    }

    /// The state before the last step, given the `current` one: the page and
    /// where its caret goes (the end of what comes back).
    pub fn undo(&mut self, current: &str) -> Option<(String, usize)> {
        let step = self.undo.pop()?;
        self.redo.push(Self::step_to(current, &step.page));
        self.last = None;
        Some((step.page, step.caret))
    }

    /// The state an undo left, given the `current` one.
    pub fn redo(&mut self, current: &str) -> Option<(String, usize)> {
        let step = self.redo.pop()?;
        self.undo.push(Self::step_to(current, &step.page));
        self.last = None;
        Some((step.page, step.caret))
    }

    /// `page` as a step to come back to from `other`: its caret at the end of
    /// what differs.
    fn step_to(page: &str, other: &str) -> Step {
        let (p, o): (Vec<char>, Vec<char>) = (page.chars().collect(), other.chars().collect());
        let (at, len, _) = diff(&p, &o);
        Step { page: page.to_string(), caret: at + len }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Type `text` at the end of `page`, one key every 100 ms from `t`.
    fn type_text(h: &mut History, page: &mut String, text: &str, t: f64) {
        for (i, c) in text.chars().enumerate() {
            let old = page.clone();
            page.push(c);
            h.record(&old, page, t + i as f64 * 100.0);
        }
    }

    #[test]
    fn typing_is_one_step_and_new_lines_split_it() {
        let mut h = History::default();
        let mut page = String::from("* Titre\n");
        type_text(&mut h, &mut page, "un mot", 0.0);
        type_text(&mut h, &mut page, "\ndeux", 700.0);
        assert_eq!(page, "* Titre\nun mot\ndeux");
        assert_eq!(h.undo(&page), Some(("* Titre\nun mot".to_string(), 14)));
        assert_eq!(h.undo("* Titre\nun mot"), Some(("* Titre\n".to_string(), 8)));
        assert_eq!(h.undo("* Titre\n"), None);
        // And back
        assert_eq!(h.redo("* Titre\n"), Some(("* Titre\nun mot".to_string(), 14)));
        assert_eq!(h.redo("* Titre\nun mot"), Some(("* Titre\nun mot\ndeux".to_string(), 19)));
        assert_eq!(h.redo("* Titre\nun mot\ndeux"), None);
    }

    #[test]
    fn a_pause_or_another_edit_starts_a_step() {
        let mut h = History::default();
        let mut page = String::new();
        type_text(&mut h, &mut page, "ab", 0.0);
        type_text(&mut h, &mut page, "cd", 5000.0);
        // A paste is a step of its own, even right after typing
        h.record("abcd", "abcdXYZ", 5200.0);
        assert_eq!(h.undo("abcdXYZ").map(|s| s.0).as_deref(), Some("abcd"));
        assert_eq!(h.undo("abcd").map(|s| s.0).as_deref(), Some("ab"));
        assert_eq!(h.undo("ab").map(|s| s.0).as_deref(), Some(""));
    }

    #[test]
    fn erasing_joins_the_typing_and_a_new_edit_drops_the_redo() {
        let mut h = History::default();
        let mut page = String::new();
        type_text(&mut h, &mut page, "abc", 0.0);
        h.record("abc", "ab", 400.0); // Backspace
        assert_eq!(h.undo("ab"), Some((String::new(), 0)));
        assert!(h.redo("").is_some());
        h.record("ab", "abX", 9000.0);
        assert_eq!(h.redo("abX"), None);
    }

    #[test]
    fn a_command_made_of_several_edits_is_one_step() {
        let mut h = History::default();
        h.record("* a\n* b", "** a\n* b", 0.0);
        h.record("** a\n* b", "** a\n** b", 1.0);
        assert_eq!(h.undo("** a\n** b").map(|s| s.0).as_deref(), Some("* a\n* b"));
    }

    #[test]
    fn caret_goes_to_the_change() {
        let mut h = History::default();
        // A line moved up, in the middle of the page
        h.record("a\nb\nc", "b\na\nc", 0.0);
        assert_eq!(h.undo("b\na\nc"), Some(("a\nb\nc".to_string(), 3)));
    }
}
