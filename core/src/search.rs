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
    /// `query` is a regular expression (accents then count, `$1` in the replacement).
    #[serde(default)]
    pub regex: bool,
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

/// The regular expression of `query`: `^` and `$` at each line, case per `opts`.
fn build_regex(query: &str, opts: SearchOptions) -> Result<regex::Regex, regex::Error> {
    let pattern = if opts.whole_word { format!(r"\b(?:{query})\b") } else { query.to_string() };
    regex::RegexBuilder::new(&pattern).case_insensitive(!opts.match_case).multi_line(true).build()
}

/// Why `query` is not a valid regular expression, when `opts` asks for one.
pub fn regex_error(query: &str, opts: SearchOptions) -> Option<String> {
    if !opts.regex || query.is_empty() { return None; }
    build_regex(query, opts).err().map(|e| e.to_string())
}

/// Byte offset in the text of `chars` of each char, and of its end.
fn byte_offsets(chars: &[char]) -> Vec<usize> {
    let mut at = 0;
    let mut out: Vec<usize> = chars.iter().map(|c| { let b = at; at += c.len_utf8(); b }).collect();
    out.push(at);
    out
}

/// Char ranges of the occurrences of `query` in `chars`, without overlap.
/// An invalid regular expression finds nothing; empty matches are left out.
pub fn find_all(chars: &[char], query: &str, opts: SearchOptions) -> Vec<(usize, usize)> {
    if opts.regex {
        let Ok(re) = build_regex(query, opts) else { return vec![] };
        if query.is_empty() { return vec![]; }
        let text: String = chars.iter().collect();
        let bytes = byte_offsets(chars);
        let char_at = |b: usize| bytes.binary_search(&b).unwrap_or_else(|i| i);
        return re.find_iter(&text).filter(|m| !m.is_empty()).map(|m| (char_at(m.start()), char_at(m.end()))).collect();
    }
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

/// The text each of the `ranges` (matches of `query`) is replaced by: `with`,
/// where with a regular expression `$1`, `${name}` stand for the groups of the match.
pub fn replacements(chars: &[char], ranges: &[(usize, usize)], query: &str, opts: SearchOptions, with: &str) -> Vec<String> {
    let re = if opts.regex { build_regex(query, opts).ok() } else { None };
    let Some(re) = re else { return vec![with.to_string(); ranges.len()] };
    let text: String = chars.iter().collect();
    let bytes = byte_offsets(chars);
    ranges.iter().map(|&(a, _)| {
        let mut out = String::new();
        // Searched from the match, so `^` and `\b` see the text before it
        if let Some(caps) = re.captures_at(&text, bytes[a]) { caps.expand(with, &mut out); }
        out
    }).collect()
}

/// `chars` with each of the `ranges` (matches of `query`) replaced as `replacements` says.
pub fn replace_matches(chars: &[char], ranges: &[(usize, usize)], query: &str, opts: SearchOptions, with: &str) -> String {
    let texts = replacements(chars, ranges, query, opts, with);
    let mut out = String::new();
    let mut at = 0;
    for (&(a, b), text) in ranges.iter().zip(&texts) {
        out.extend(&chars[at..a]);
        out.push_str(text);
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
        let line_end = crate::org::line_end(chars, end);
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
    const ANY: SearchOptions = SearchOptions { match_case: false, whole_word: false, regex: false };
    const CASE: SearchOptions = SearchOptions { match_case: true, whole_word: false, regex: false };
    const WORD: SearchOptions = SearchOptions { match_case: false, whole_word: true, regex: false };
    const RE: SearchOptions = SearchOptions { match_case: false, whole_word: false, regex: true };

    #[test]
    fn regular_expressions() {
        let t = c("Été 2024, été 25\nfin");
        // Char ranges even after multi-byte chars
        assert_eq!(find_all(&t, r"\d+", RE), vec![(4, 8), (14, 16)]);
        assert_eq!(find_all(&t, "été", RE), vec![(0, 3), (10, 13)]);
        assert_eq!(find_all(&t, "été", SearchOptions { match_case: true, ..RE }), vec![(10, 13)]);
        assert_eq!(find_all(&t, "^fin$", RE), vec![(17, 20)]);
        assert_eq!(find_all(&t, "2", SearchOptions { whole_word: true, ..RE }), vec![]);
        // Empty matches and invalid patterns find nothing
        assert!(find_all(&t, "x*", RE).is_empty());
        assert!(find_all(&t, "(", RE).is_empty());
        assert!(regex_error("(", RE).is_some());
        assert!(regex_error("(", ANY).is_none());
        let r = find_all(&t, r"(\p{L}+) (\d+)", RE);
        assert_eq!(replace_matches(&t, &r, r"(\p{L}+) (\d+)", RE, "$2-$1"), "2024-Été, 25-été\nfin");
        // Without regex, `$1` stays as written
        assert_eq!(replace_matches(&t, &find_all(&t, "fin", ANY), "fin", ANY, "$1"), "Été 2024, été 25\n$1");
    }

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
    fn replace_matches_replaces_every_match() {
        let t = c("un chat, deux chats");
        assert_eq!(replace_matches(&t, &[(3, 7), (14, 18)], "chat", ANY, "chien"), "un chien, deux chiens");
        assert_eq!(replace_matches(&t, &[], "chat", ANY, "x"), "un chat, deux chats");
        assert_eq!(replace_matches(&t, &[(0, 3)], "un ", ANY, ""), "chat, deux chats");
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
