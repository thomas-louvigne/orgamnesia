//! Finding text in pages and replacing it: in one page (the find bar) or in
//! the whole project (the project search), with the line shown around each match.

use serde::{Deserialize, Serialize};

use crate::names::fold_char;

/// How a search compares the text.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct SearchOptions {
    /// Case and accents count; else "regle" finds "Règle".
    pub match_case: bool,
    /// Only whole words: "chat" does not find "chaton".
    pub whole_word: bool,
}

/// A match in a page.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchHit {
    /// Char range of the match in the page.
    pub start: usize,
    pub end: usize,
    /// Its line (0-based).
    pub line: usize,
    /// The line around it: the text before the match, the match, the text after,
    /// cut with "…" when the line is long.
    pub before: String,
    pub matched: String,
    pub after: String,
}

/// The matches of a page.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PageHits {
    pub page: String,
    pub path: String,
    pub hits: Vec<SearchHit>,
    /// Number of matches in the page; more than `hits` when the results were cut.
    pub count: usize,
}

/// What a project search found.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SearchResults {
    pub pages: Vec<PageHits>,
    /// Number of matches in the project.
    pub total: usize,
    /// Not every match is in `pages` (too many to show).
    pub truncated: bool,
}

/// The matches a project replacement leaves alone.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ReplaceSkip {
    /// Pages left out entirely.
    pub pages: Vec<String>,
    /// Single matches left out: (page path, start of the match).
    pub hits: Vec<(String, usize)>,
}

/// What a project replacement (or its undo) did.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct ReplaceOutcome {
    /// Pages written.
    pub pages: usize,
    /// Matches replaced (0 for an undo).
    pub replacements: usize,
    /// Pages left as they were: an undo doesn't restore a page changed since.
    pub skipped: usize,
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Char ranges of the occurrences of `query` in `chars`, without overlap.
pub fn find_all(chars: &[char], query: &str, opts: SearchOptions) -> Vec<(usize, usize)> {
    let norm = |c: char| if opts.match_case { c } else { fold_char(c) };
    let q: Vec<char> = query.chars().map(norm).collect();
    let mut found = Vec::new();
    if q.is_empty() { return found; }
    let mut i = 0;
    while i + q.len() <= chars.len() {
        let end = i + q.len();
        let whole = !opts.whole_word || (
            (i == 0 || !is_word_char(chars[i - 1]) || !is_word_char(q[0]))
            && (end == chars.len() || !is_word_char(chars[end]) || !is_word_char(q[q.len() - 1]))
        );
        if whole && chars[i..end].iter().zip(&q).all(|(&c, &k)| norm(c) == k) {
            found.push((i, end));
            i = end;
        } else {
            i += 1;
        }
    }
    found
}

/// `chars` with each of the `ranges` (sorted, not overlapping) replaced by `with`.
pub fn replace_ranges(chars: &[char], ranges: &[(usize, usize)], with: &str) -> String {
    let mut out = String::new();
    let mut at = 0;
    for &(a, b) in ranges {
        out.extend(&chars[at..a]);
        out.push_str(with);
        at = b;
    }
    out.extend(&chars[at..]);
    out
}

/// Chars of the line kept before and after a match.
const CONTEXT_BEFORE: usize = 40;
const CONTEXT_AFTER: usize = 80;

/// The hits of the `ranges` found in `chars`: their line and the text around them.
pub fn hits(chars: &[char], ranges: &[(usize, usize)]) -> Vec<SearchHit> {
    let mut line = 0;
    let mut line_start = 0;
    let mut scanned = 0;
    ranges.iter().map(|&(start, end)| {
        for (i, &c) in chars[scanned..start].iter().enumerate() {
            if c == '\n' {
                line += 1;
                line_start = scanned + i + 1;
            }
        }
        scanned = start;
        let line_end = chars[end..].iter().position(|&c| c == '\n').map_or(chars.len(), |p| end + p);
        // The match itself may span lines: shown on one
        let flat = |s: &[char]| s.iter().map(|&c| if c == '\n' { ' ' } else { c }).collect::<String>();
        let from = start.saturating_sub(CONTEXT_BEFORE).max(line_start);
        let to = (end + CONTEXT_AFTER).min(line_end);
        let mut before = flat(&chars[from..start]);
        if from == line_start {
            before = before.trim_start().to_string();
        } else {
            before.insert(0, '…');
        }
        let mut after = flat(&chars[end..to]);
        if to < line_end { after.push('…'); }
        SearchHit { start, end, line, before, matched: flat(&chars[start..end]), after }
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(s: &str) -> Vec<char> { s.chars().collect() }
    const ANY: SearchOptions = SearchOptions { match_case: false, whole_word: false };
    const CASE: SearchOptions = SearchOptions { match_case: true, whole_word: false };
    const WORD: SearchOptions = SearchOptions { match_case: false, whole_word: true };

    #[test]
    fn finds_every_occurrence() {
        let t = c("Élan, élan ÉLAN aaa");
        assert_eq!(find_all(&t, "élan", ANY), vec![(0, 4), (6, 10), (11, 15)]);
        assert_eq!(find_all(&t, "élan", CASE), vec![(6, 10)]);
        assert_eq!(find_all(&t, "aa", ANY), vec![(16, 18)]);
        assert!(find_all(&t, "", ANY).is_empty());
        // Accents ignored too, unless matching case
        let t = c("La règle, les Règles, regle");
        assert_eq!(find_all(&t, "regle", ANY), vec![(3, 8), (14, 19), (22, 27)]);
        assert_eq!(find_all(&t, "regle", CASE), vec![(22, 27)]);
    }

    #[test]
    fn whole_words_only() {
        let t = c("chat chaton, le chat_noir et Chat.");
        assert_eq!(find_all(&t, "chat", WORD), vec![(0, 4), (29, 33)]);
        assert_eq!(find_all(&t, "chat", ANY).len(), 4);
        // A query that doesn't start or end with a letter needs no boundary there
        assert_eq!(find_all(&c("a.b a.bc"), ".b", WORD), vec![(1, 3)]);
    }

    #[test]
    fn replace_ranges_replaces_every_match() {
        let t = c("un chat, deux chats");
        assert_eq!(replace_ranges(&t, &[(3, 7), (14, 18)], "chien"), "un chien, deux chiens");
        assert_eq!(replace_ranges(&t, &[], "x"), "un chat, deux chats");
        assert_eq!(replace_ranges(&t, &[(0, 3)], ""), "chat, deux chats");
    }

    #[test]
    fn hits_show_their_line() {
        let t = c("* Titre\n   un chat noir\nle chat");
        let found = find_all(&t, "chat", ANY);
        let h = hits(&t, &found);
        assert_eq!(h.len(), 2);
        assert_eq!((h[0].line, h[0].before.as_str(), h[0].matched.as_str(), h[0].after.as_str()), (1, "un ", "chat", " noir"));
        assert_eq!((h[1].line, h[1].before.as_str(), h[1].after.as_str()), (2, "le ", ""));
        assert_eq!((h[1].start, h[1].end), (27, 31));
    }

    #[test]
    fn long_lines_are_cut() {
        let line = format!("{}chat{}", "a".repeat(100), "b".repeat(100));
        let t = c(&line);
        let h = hits(&t, &find_all(&t, "chat", ANY));
        assert_eq!(h[0].before, format!("…{}", "a".repeat(CONTEXT_BEFORE)));
        assert_eq!(h[0].after, format!("{}…", "b".repeat(CONTEXT_AFTER)));
    }
}
