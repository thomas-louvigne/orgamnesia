//! What the editor's commands do to the text, as pure functions over the
//! characters (`&[char]`, positions in chars): the editor component only
//! applies the result to its textarea. See also `motion.rs`.

use crate::keybindings::EditorAction;
use crate::motion::{self, Hashtags};

/// Lines moved by the page-up / page-down actions.
pub const PAGE_LINES: i32 = 20;

/// Start and end (before the `\n`) of the line holding `pos`.
pub fn line_bounds(chars: &[char], pos: usize) -> (usize, usize) {
    let start = chars[..pos].iter().rposition(|&c| c == '\n').map(|i| i + 1).unwrap_or(0);
    let end   = chars[pos..].iter().position(|&c| c == '\n').map(|i| pos + i).unwrap_or(chars.len());
    (start, end)
}

/// Position `delta` lines down (negative: up) keeping the column when possible.
pub fn move_lines(chars: &[char], pos: usize, delta: i32) -> usize {
    let (mut start, _) = line_bounds(chars, pos);
    let col = pos - start;
    for _ in 0..delta.unsigned_abs() {
        if delta > 0 {
            let (_, end) = line_bounds(chars, start);
            if end >= chars.len() { return chars.len(); }
            start = end + 1;
        } else {
            if start == 0 { return pos; }
            start = line_bounds(chars, start - 1).0;
        }
    }
    let (s, e) = line_bounds(chars, start);
    (s + col).min(e)
}

pub fn word_end_forward(chars: &[char], pos: usize) -> usize {
    let mut i = pos;
    while i < chars.len() && !chars[i].is_alphanumeric() { i += 1; }
    while i < chars.len() &&  chars[i].is_alphanumeric() { i += 1; }
    i
}

pub fn word_start_backward(chars: &[char], pos: usize) -> usize {
    let mut i = pos;
    while i > 0 && !chars[i - 1].is_alphanumeric() { i -= 1; }
    while i > 0 &&  chars[i - 1].is_alphanumeric() { i -= 1; }
    i
}

/// If the line `[start..end)` is an org heading (`*`… followed by a space or EOL),
/// its level (number of leading `*`).
fn heading_level(chars: &[char], start: usize, end: usize) -> Option<usize> {
    let mut i = start;
    while i < end && chars[i] == '*' { i += 1; }
    let level = i - start;
    if level > 0 && (i >= end || chars[i] == ' ') { Some(level) } else { None }
}

/// Heading level in effect at `caret`: the current line if it is a heading,
/// otherwise the nearest heading above it. Defaults to 1 when none is found.
pub fn nearest_heading_level(chars: &[char], caret: usize) -> usize {
    let (mut line_start, _) = line_bounds(chars, caret);
    loop {
        let (_, line_end) = line_bounds(chars, line_start);
        if let Some(l) = heading_level(chars, line_start, line_end) { return l; }
        if line_start == 0 { return 1; }
        line_start = line_bounds(chars, line_start - 1).0;
    }
}

/// Where to insert a new heading of the current level, and what: after the
/// current line, `\n` and the stars.
pub fn new_heading(chars: &[char], caret: usize) -> (usize, String) {
    let level = nearest_heading_level(chars, caret);
    let (_, line_end) = line_bounds(chars, caret);
    (line_end, format!("\n{} ", "*".repeat(level)))
}

/// What Emacs `C-k` removes at `pos`: up to the end of the line, or the line
/// break itself when already there.
pub fn kill_line_range(chars: &[char], pos: usize) -> (usize, usize) {
    let (_, line_end) = line_bounds(chars, pos);
    if pos == line_end && pos < chars.len() { (pos, pos + 1) } else { (pos, line_end) }
}

/// What "delete forward" removes: the selection, else the next character.
pub fn delete_forward_range(chars: &[char], start: usize, end: usize) -> (usize, usize) {
    if start != end { (start, end) } else { (start, (start + 1).min(chars.len())) }
}

/// The pair a typed character belongs to: `(` and `)` → `("(", ")")`…
pub fn pair_of(key: &str) -> Option<(&'static str, &'static str)> {
    match key {
        "(" | ")" => Some(("(", ")")),
        "[" | "]" => Some(("[", "]")),
        "{" | "}" => Some(("{", "}")),
        "\"" => Some(("\"", "\"")),
        "'" => Some(("'", "'")),
        _ => None,
    }
}

/// Typing this character with no selection also inserts its closing one.
pub fn auto_closes(key: &str) -> bool {
    matches!(key, "(" | "[" | "{" | "\"")
}

/// Where a cursor movement action takes the caret from `pos`; `None` when the
/// action is not a movement.
pub fn motion_target(action: EditorAction, chars: &[char], pos: usize) -> Option<usize> {
    use EditorAction::*;
    Some(match action {
        ForwardChar => (pos + 1).min(chars.len()),
        BackwardChar => pos.saturating_sub(1),
        NextLine => move_lines(chars, pos, 1),
        PreviousLine => move_lines(chars, pos, -1),
        ForwardWord => word_end_forward(chars, pos),
        BackwardWord => word_start_backward(chars, pos),
        BeginningOfBuffer => 0,
        EndOfBuffer => chars.len(),
        ScrollDown => move_lines(chars, pos, PAGE_LINES),
        ScrollUp => move_lines(chars, pos, -PAGE_LINES),
        ForwardSentence => motion::forward_sentence(chars, pos),
        BackwardSentence => motion::backward_sentence(chars, pos),
        ForwardParagraph => motion::forward_paragraph(chars, pos),
        BackwardParagraph => motion::backward_paragraph(chars, pos),
        BackToIndentation => motion::back_to_indentation(chars, pos),
        NextHeading => motion::next_heading(chars, pos),
        PreviousHeading => motion::previous_heading(chars, pos),
        BeginningOfLine => line_bounds(chars, pos).0,
        EndOfLine => line_bounds(chars, pos).1,
        _ => return None,
    })
}

/// Position of the start of line `n` (0-based); the end of the text past the last line.
pub fn line_start(chars: &[char], n: usize) -> usize {
    if n == 0 { return 0; }
    chars.iter().enumerate().filter(|&(_, &c)| c == '\n').nth(n - 1)
        .map_or(chars.len(), |(i, _)| i + 1)
}

/// The page a link at `pos` points to: `[[target]]`, `[[target][text]]`, a
/// `#tag` (per `hashtags`) or an org-mode `:tag:`.
pub fn link_at(chars: &[char], pos: usize, hashtags: Hashtags) -> Option<String> {
    wiki_link_at(chars, pos, hashtags).or_else(|| motion::org_tag_at(chars, pos))
}

fn wiki_link_at(chars: &[char], pos: usize, hashtags: Hashtags) -> Option<String> {
    let n = chars.len();
    let mut i = 0;
    while i < n {
        if let Some((tag, end)) = motion::hashtag_at(chars, i, hashtags) {
            if pos >= i && pos < end { return Some(tag); }
            i = end;
            continue;
        }
        if i + 1 >= n { break; }
        if chars[i] != '[' || chars[i + 1] != '[' {
            i += 1;
            continue;
        }
        let link_start = i;
        let content_start = i + 2;
        // End of the target: `]]` or `][`
        let mut j = content_start;
        while j < n && !(chars[j] == ']' && j + 1 < n && (chars[j + 1] == ']' || chars[j + 1] == '[')) {
            j += 1;
        }
        if j + 1 >= n { break; }
        let target: String = chars[content_start..j].iter().collect();
        let link_end = if chars[j + 1] == ']' {
            j + 2
        } else {
            // [[target][text]]
            let mut k = j + 2;
            while k + 1 < n && !(chars[k] == ']' && chars[k + 1] == ']') { k += 1; }
            if k + 1 >= n { i += 1; continue; }
            k + 2
        };
        if pos >= link_start && pos < link_end {
            return Some(target.trim().to_string());
        }
        i = link_end;
    }
    None
}

/// Char ranges of the occurrences of `query` in `chars`, without overlap.
pub fn find_all(chars: &[char], query: &str, match_case: bool) -> Vec<(usize, usize)> {
    let norm = |c: char| if match_case { c } else { c.to_lowercase().next().unwrap_or(c) };
    let q: Vec<char> = query.chars().map(norm).collect();
    let mut found = Vec::new();
    if q.is_empty() { return found; }
    let mut i = 0;
    while i + q.len() <= chars.len() {
        if chars[i..i + q.len()].iter().zip(&q).all(|(&c, &k)| norm(c) == k) {
            found.push((i, i + q.len()));
            i += q.len();
        } else {
            i += 1;
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(s: &str) -> Vec<char> { s.chars().collect() }

    #[test]
    fn lines() {
        let t = c("ab\ncde\n\nf");
        assert_eq!(line_bounds(&t, 4), (3, 6));
        assert_eq!(move_lines(&t, 1, 1), 4);
        assert_eq!(move_lines(&t, 5, 1), 7);
        assert_eq!(move_lines(&t, 5, -1), 2);
        assert_eq!(move_lines(&t, 1, -1), 1);
        assert_eq!(move_lines(&t, 0, 50), t.len());
        assert_eq!(line_start(&t, 2), 7);
        assert_eq!(line_start(&t, 9), t.len());
    }

    #[test]
    fn words() {
        let t = c("un, deux trois");
        assert_eq!(word_end_forward(&t, 2), 8);
        assert_eq!(word_start_backward(&t, 8), 4);
        assert_eq!(word_start_backward(&t, 0), 0);
    }

    #[test]
    fn kill_line_takes_the_line_break_at_end_of_line() {
        let t = c("abc\ndef");
        assert_eq!(kill_line_range(&t, 1), (1, 3));
        assert_eq!(kill_line_range(&t, 3), (3, 4));
        assert_eq!(kill_line_range(&t, 7), (7, 7));
    }

    #[test]
    fn delete_forward() {
        let t = c("abc");
        assert_eq!(delete_forward_range(&t, 0, 2), (0, 2));
        assert_eq!(delete_forward_range(&t, 1, 1), (1, 2));
        assert_eq!(delete_forward_range(&t, 3, 3), (3, 3));
    }

    #[test]
    fn new_heading_uses_the_level_in_effect() {
        let t = c("* a\n** b\ntexte");
        assert_eq!(new_heading(&t, 11), (14, "\n** ".to_string()));
        assert_eq!(new_heading(&c("rien"), 2), (4, "\n* ".to_string()));
    }

    #[test]
    fn pairs() {
        assert_eq!(pair_of(")"), Some(("(", ")")));
        assert_eq!(pair_of("a"), None);
        assert!(auto_closes("[") && !auto_closes("'") && !auto_closes(")"));
    }

    #[test]
    fn motions() {
        let t = c("ab\ncd");
        assert_eq!(motion_target(EditorAction::EndOfLine, &t, 3), Some(5));
        assert_eq!(motion_target(EditorAction::BeginningOfLine, &t, 4), Some(3));
        assert_eq!(motion_target(EditorAction::BackwardChar, &t, 0), Some(0));
        assert_eq!(motion_target(EditorAction::ForwardChar, &t, 5), Some(5));
        assert_eq!(motion_target(EditorAction::Copy, &t, 0), None);
    }

    #[test]
    fn links_under_the_cursor() {
        let t = c("voir [[Ma page]] et [[cible][texte]] #tag fin");
        assert_eq!(link_at(&t, 8, Hashtags::Dashes).as_deref(), Some("Ma page"));
        assert_eq!(link_at(&t, 30, Hashtags::Dashes).as_deref(), Some("cible"));
        assert_eq!(link_at(&t, 39, Hashtags::Dashes).as_deref(), Some("tag"));
        assert_eq!(link_at(&t, 39, Hashtags::Off), None);
        assert_eq!(link_at(&t, 1, Hashtags::Dashes), None);
        assert_eq!(link_at(&c("* Titre :projet:"), 11, Hashtags::Off).as_deref(), Some("projet"));
    }

    #[test]
    fn finds_every_occurrence() {
        let c: Vec<char> = "Élan, élan ÉLAN aaa".chars().collect();
        assert_eq!(find_all(&c, "élan", false), vec![(0, 4), (6, 10), (11, 15)]);
        assert_eq!(find_all(&c, "élan", true), vec![(6, 10)]);
        assert_eq!(find_all(&c, "aa", false), vec![(16, 18)]);
        assert!(find_all(&c, "", false).is_empty());
    }
}
