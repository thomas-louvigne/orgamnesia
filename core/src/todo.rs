//! Org-mode TODO keywords on headlines (`* TODO Titre`): the keywords in effect
//! and stepping a headline from one state to the next, as Emacs' `S-<right>` /
//! `S-<left>` do.
//!
//! Keywords are written as in Emacs: `TODO DOING | DONE`, the ones after the `|`
//! being the done states. Without `|`, the last keyword is the only done one.

/// One sequence of keywords: `TODO DOING | DONE`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sequence {
    pub todo: Vec<String>,
    pub done: Vec<String>,
}

impl Sequence {
    /// `TODO DOING | DONE`. Emacs' fast-access keys and logging marks (`TODO(t)`,
    /// `DONE(d!)`) are dropped. `None` when there is no keyword.
    pub fn parse(s: &str) -> Option<Self> {
        let words = |part: &str| -> Vec<String> {
            part.split_whitespace()
                .map(|w| w.split('(').next().unwrap_or(w).to_string())
                .filter(|w| !w.is_empty())
                .collect()
        };
        let (mut todo, mut done) = match s.split_once('|') {
            Some((a, b)) => (words(a), words(b)),
            None => (words(s), vec![]),
        };
        if done.is_empty() && todo.len() > 1 {
            done.push(todo.pop()?);
        }
        (!todo.is_empty() || !done.is_empty()).then_some(Self { todo, done })
    }

    fn states(&self) -> impl Iterator<Item = &String> {
        self.todo.iter().chain(&self.done)
    }
}

/// The keywords in effect: those of the settings, or those of the page's
/// `#+TODO:` lines (one sequence each).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TodoKeywords {
    pub sequences: Vec<Sequence>,
}

/// The keyword of a headline: its char range in the line, and whether it is a done state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Keyword {
    pub start: usize,
    pub end: usize,
    pub done: bool,
}

impl TodoKeywords {
    /// The keywords as written in the settings: one sequence.
    pub fn parse(s: &str) -> Self {
        Self { sequences: Sequence::parse(s).into_iter().collect() }
    }

    /// The keywords of the page `text`: its `#+TODO:` lines (also `#+SEQ_TODO:` and
    /// `#+TYP_TODO:`, as in Emacs), else `default`.
    pub fn for_page(text: &str, default: &TodoKeywords) -> Self {
        let sequences: Vec<Sequence> = text.lines()
            .filter_map(|l| {
                let l = l.trim_start();
                ["#+todo:", "#+seq_todo:", "#+typ_todo:"].iter().find_map(|k| {
                    l.get(..k.len()).filter(|p| p.eq_ignore_ascii_case(k)).map(|_| &l[k.len()..])
                })
            })
            .filter_map(Sequence::parse)
            .collect();
        if sequences.is_empty() { default.clone() } else { Self { sequences } }
    }

    /// Whether `word` is a keyword, and a done one.
    fn kind(&self, word: &str) -> Option<bool> {
        self.sequences.iter().find_map(|s| {
            if s.todo.iter().any(|k| k == word) { Some(false) }
            else if s.done.iter().any(|k| k == word) { Some(true) }
            else { None }
        })
    }

    /// The keyword of the headline `line` (`** TODO Titre`), if it has one.
    pub fn keyword(&self, line: &[char]) -> Option<Keyword> {
        let start = title_start(line)?;
        let end = line[start..].iter().position(|&c| c == ' ').map_or(line.len(), |p| start + p);
        let word: String = line[start..end].iter().collect();
        self.kind(&word).map(|done| Keyword { start, end, done })
    }

    /// The state after `current` (`None`: no keyword). The cycle goes through
    /// no keyword between the last state and the first, as in Emacs, and stays
    /// in the sequence of `current`.
    pub fn step(&self, current: Option<&str>, forward: bool) -> Option<String> {
        let seq = current
            .and_then(|c| self.sequences.iter().find(|s| s.states().any(|k| k == c)))
            .or_else(|| self.sequences.first())?;
        let states: Vec<&String> = seq.states().collect();
        let at = current.and_then(|c| states.iter().position(|k| *k == c));
        let next = match (at, forward) {
            (None, true) => Some(0),
            (None, false) => Some(states.len() - 1),
            (Some(i), true) => (i + 1 < states.len()).then_some(i + 1),
            (Some(i), false) => i.checked_sub(1),
        };
        next.map(|i| states[i].clone())
    }

    /// The edit that moves the headline `line` to its next (or previous) state:
    /// the range of the line to replace and its new text. `None` when the line is
    /// not a headline, or there is no keyword at all.
    pub fn cycle(&self, line: &[char], forward: bool) -> Option<(usize, usize, String)> {
        let start = title_start(line)?;
        let kw = self.keyword(line);
        let current: Option<String> = kw.map(|k| line[k.start..k.end].iter().collect());
        let next = self.step(current.as_deref(), forward);
        Some(match (kw, next) {
            (Some(k), Some(n)) => (k.start, k.end, n),
            // The keyword goes, with the space after it
            (Some(k), None) => (k.start, (k.end + 1).min(line.len()), String::new()),
            (None, Some(n)) if start >= line.len() => {
                // `*` alone gets its space; `* ` keeps it
                let space = if line.get(start - 1) == Some(&' ') { "" } else { " " };
                (start, start, format!("{space}{n}"))
            }
            (None, Some(n)) => (start, start, format!("{n} ")),
            (None, None) => return None,
        })
    }
}

/// A headline carrying a TODO keyword, found by `headlines`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodoHeadline {
    /// Line of the headline in the page (0-based).
    pub line: usize,
    pub level: usize,
    pub keyword: String,
    pub done: bool,
    /// The title after the keyword, without its `:tags:`.
    pub title: String,
}

/// The headlines of the page `text` that carry one of its keywords (its `#+TODO:`
/// lines, else `default`).
pub fn headlines(text: &str, default: &TodoKeywords) -> Vec<TodoHeadline> {
    let keywords = TodoKeywords::for_page(text, default);
    text.lines().enumerate().filter_map(|(line, l)| {
        let chars: Vec<char> = l.chars().collect();
        let k = keywords.keyword(&chars)?;
        let mut title: String = chars[k.end..].iter().collect::<String>().trim().to_string();
        // `Titre :a:b:`: the tags go
        if let Some((before, last)) = title.rsplit_once(' ').or(Some(("", title.as_str())))
            && last.len() > 1 && last.starts_with(':') && last.ends_with(':') && !last.contains(char::is_whitespace)
        {
            title = before.trim_end().to_string();
        }
        Some(TodoHeadline {
            line,
            level: k.start.saturating_sub(1),
            keyword: chars[k.start..k.end].iter().collect(),
            done: k.done,
            title,
        })
    }).collect()
}

/// Where the title of the headline `line` starts (after its stars and the space),
/// or the end of the line for a headline with no title.
fn title_start(line: &[char]) -> Option<usize> {
    let stars = line.iter().take_while(|&&c| c == '*').count();
    match line.get(stars) {
        _ if stars == 0 => None,
        Some(' ') => Some(stars + 1),
        None => Some(stars),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(s: &str) -> Vec<char> { s.chars().collect() }

    fn kw() -> TodoKeywords { TodoKeywords::parse("TODO DOING HANGUP | DONE") }

    /// The line after one step.
    fn step(line: &str, forward: bool) -> Option<String> {
        let chars = c(line);
        let (a, b, with) = kw().cycle(&chars, forward)?;
        Some(chars[..a].iter().collect::<String>() + &with + &chars[b..].iter().collect::<String>())
    }

    #[test]
    fn parses_like_emacs() {
        let s = Sequence::parse("TODO(t) DOING | DONE(d!) CANCELED").unwrap();
        assert_eq!(s.todo, vec!["TODO", "DOING"]);
        assert_eq!(s.done, vec!["DONE", "CANCELED"]);
        // Without `|`, the last keyword is the done one
        let s = Sequence::parse("A B C").unwrap();
        assert_eq!((s.todo.len(), s.done), (2, vec!["C".to_string()]));
        assert_eq!(Sequence::parse("  | "), None);
        assert!(TodoKeywords::parse("").sequences.is_empty());
    }

    #[test]
    fn cycles_through_no_keyword() {
        assert_eq!(step("** Titre", true).as_deref(), Some("** TODO Titre"));
        assert_eq!(step("** TODO Titre", true).as_deref(), Some("** DOING Titre"));
        assert_eq!(step("** HANGUP Titre", true).as_deref(), Some("** DONE Titre"));
        assert_eq!(step("** DONE Titre", true).as_deref(), Some("** Titre"));
        assert_eq!(step("** Titre", false).as_deref(), Some("** DONE Titre"));
        assert_eq!(step("** TODO Titre", false).as_deref(), Some("** Titre"));
        assert_eq!(step("* DONE", true).as_deref(), Some("* "));
        assert_eq!(step("* ", true).as_deref(), Some("* TODO"));
        assert_eq!(step("*", true).as_deref(), Some("* TODO"));
        // Not a headline; a word that is not a keyword is the title
        assert_eq!(step("texte", true), None);
        assert_eq!(step("*gras*", true), None);
        assert_eq!(step("* TODOS", true).as_deref(), Some("* TODO TODOS"));
        assert_eq!(TodoKeywords::default().cycle(&c("* A"), true), None);
    }

    #[test]
    fn finds_the_keyword() {
        assert_eq!(kw().keyword(&c("** DONE fini")), Some(Keyword { start: 3, end: 7, done: true }));
        assert_eq!(kw().keyword(&c("* TODO")), Some(Keyword { start: 2, end: 6, done: false }));
        assert_eq!(kw().keyword(&c("* todo")), None);
        assert_eq!(kw().keyword(&c("TODO")), None);
    }

    #[test]
    fn lists_the_headlines_with_a_keyword() {
        let page = "* Projet\n** TODO Écrire :urgent:\ntexte TODO\n*** DONE Relire\n* HANGUP\n";
        let found = headlines(page, &kw());
        assert_eq!(found, vec![
            TodoHeadline { line: 1, level: 2, keyword: "TODO".into(), done: false, title: "Écrire".into() },
            TodoHeadline { line: 3, level: 3, keyword: "DONE".into(), done: true, title: "Relire".into() },
            TodoHeadline { line: 4, level: 1, keyword: "HANGUP".into(), done: false, title: String::new() },
        ]);
    }

    #[test]
    fn page_keywords_win() {
        let page = "#+title: x\n#+TODO: A B | C\n#+seq_todo: X | Y\n* A t\n";
        let k = TodoKeywords::for_page(page, &kw());
        assert_eq!(k.sequences.len(), 2);
        // Each keyword stays in its sequence; no keyword starts the first one
        assert_eq!(k.step(Some("C"), true), None);
        assert_eq!(k.step(Some("X"), true).as_deref(), Some("Y"));
        assert_eq!(k.step(None, true).as_deref(), Some("A"));
        assert_eq!(TodoKeywords::for_page("* TODO a", &kw()), kw());
    }
}
