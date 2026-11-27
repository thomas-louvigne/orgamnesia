//! What the editor's commands do to the text, as pure functions over the
//! characters (`&[char]`, positions in chars): the editor component only
//! applies the result to its textarea. See also `motion.rs`.

use crate::keybindings::EditorAction;
use crate::{motion, tags};
use crate::{hashtags::hashtag_at, links::link_at, Hashtags, TagSyntax};
mod under_caret;
pub use under_caret::*;

/// Lines moved by the page-up / page-down actions.
pub const PAGE_LINES: i32 = 20;

use crate::org::line_bounds;

/// A move of lines one line up or down: the char range `a..b` becomes `text`,
/// and the selection is now `sel`.
#[derive(Debug, PartialEq)]
pub struct LineMove {
    pub a: usize,
    pub b: usize,
    pub text: String,
    pub sel: (usize, usize),
}

/// Move the lines of the selection `start..end` above the line before them (`up`)
/// or below the line after them; `None` at the top or the bottom of the text.
pub fn move_line(chars: &[char], start: usize, end: usize, up: bool) -> Option<LineMove> {
    // A selection ending at the start of a line doesn't take that line
    let last = if end > start && end > 0 && chars[end - 1] == '\n' { end - 1 } else { end };
    let (first, _) = line_bounds(chars, start);
    let (_, block_end) = line_bounds(chars, last.max(start));
    let block: String = chars[first..block_end].iter().collect();
    if up {
        if first == 0 { return None; }
        let (prev, _) = line_bounds(chars, first - 1);
        let above: String = chars[prev..first - 1].iter().collect();
        let shift = first - prev;
        Some(LineMove { a: prev, b: block_end, text: format!("{block}\n{above}"), sel: (start - shift, end - shift) })
    } else {
        if block_end >= chars.len() { return None; }
        let (_, next_end) = line_bounds(chars, block_end + 1);
        let below: String = chars[block_end + 1..next_end].iter().collect();
        let shift = next_end - block_end;
        Some(LineMove { a: first, b: next_end, text: format!("{below}\n{block}"), sel: (start + shift, end + shift) })
    }
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

/// Column `fill_paragraph` wraps at.
// shortcut: Emacs' default `fill-column` for every page, a setting if someone wants another width.
pub const FILL_COLUMN: usize = 70;

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

/// Heading level in effect at `caret`: the current line if it is a heading,
/// otherwise the nearest heading above it. Defaults to 1 when none is found.
pub fn nearest_heading_level(chars: &[char], caret: usize) -> usize {
    let (mut line_start, _) = line_bounds(chars, caret);
    loop {
        let (_, line_end) = line_bounds(chars, line_start);
        if let Some(l) = motion::headline_level(&chars[line_start..line_end]) { return l; }
        if line_start == 0 { return 1; }
        line_start = line_bounds(chars, line_start - 1).0;
    }
}

/// A new heading of the current level, or on a list item a new item (as org's
/// `M-RET`), as the char range `from..to` to replace and its text: the line is cut
/// at the caret, what follows it becoming the new one's text (the blanks around
/// the cut go). At the start of a line of text, that line becomes the heading or
/// item; on the stars or bullet of a line, the new one goes below it.
pub fn new_heading(chars: &[char], caret: usize) -> (usize, usize, String) {
    let (line_start, line_end) = line_bounds(chars, caret);
    let (mark, text_start) = match motion::new_item_bullet(chars, caret) {
        Some(bullet) => (bullet, motion::item_contents(chars, line_start)),
        None => {
            let stars = motion::heading_mark().to_string().repeat(nearest_heading_level(chars, caret));
            let level = motion::headline_level(&chars[line_start..line_end]);
            (format!("{stars} "), level.map(|l| line_start + l + 1))
        }
    };
    if text_start.is_some_and(|text| caret <= text) { return (line_end, line_end, format!("\n{mark}")); }
    let blank = |c: &&char| matches!(**c, ' ' | '\t');
    let from = caret - chars[line_start..caret].iter().rev().take_while(blank).count();
    let to = caret + chars[caret..line_end].iter().take_while(blank).count();
    if from == line_start && to < line_end { return (from, to, mark); }
    (from, to, format!("\n{mark}"))
}

/// What Emacs `C-k` removes at `pos`: up to the end of the line, or the line
/// break itself when already there.
pub fn kill_line_range(chars: &[char], pos: usize) -> (usize, usize) {
    let (_, line_end) = line_bounds(chars, pos);
    if pos == line_end && pos < chars.len() { (pos, pos + 1) } else { (pos, line_end) }
}

/// Indentation step of the configuration files (JSON, as the app writes them).
const JSON_INDENT: usize = 2;

/// Tab (`back`: Shift+Tab) in a JSON file, the selection `start..end`: the
/// char range `from..to` becomes `text`, and the new selection.
/// Without a selection, Tab goes to the next indentation step, Shift+Tab takes
/// one step off the line; a selection has its lines indented, or one step taken off.
pub fn json_indent(chars: &[char], start: usize, end: usize, back: bool) -> (usize, usize, String, (usize, usize)) {
    let (line, _) = line_bounds(chars, start);
    if start == end && !back {
        let n = JSON_INDENT - (start - line) % JSON_INDENT;
        return (start, start, " ".repeat(n), (start + n, start + n));
    }
    // The lines of the selection; one it only reaches the start of is left out
    let last = if end > start && chars[end - 1] == '\n' { end - 1 } else { end };
    let (_, to) = line_bounds(chars, last.max(start));
    let mut text = String::new();
    // Chars taken off (or added) before `start`, and before `end`
    let (mut before_start, mut before_end) = (0isize, 0isize);
    let mut at = line;
    for (i, l) in chars[line..to].split(|&c| c == '\n').enumerate() {
        if i > 0 { text.push('\n'); }
        let change = if back {
            -(l.iter().take(JSON_INDENT).take_while(|&&c| c == ' ').count() as isize)
        } else {
            JSON_INDENT as isize
        };
        let kept = if change < 0 { &l[(-change) as usize..] } else { l };
        if change > 0 { text.push_str(&" ".repeat(change as usize)); }
        text.extend(kept);
        // A change at a line start counts for a position from that line start on
        let shift = |pos: usize| if pos >= at { change.max(-((pos - at) as isize)) } else { 0 };
        before_start += shift(start);
        before_end += shift(end);
        at += l.len() + 1;
    }
    let moved = |pos: usize, by: isize| (pos as isize + by).max(line as isize) as usize;
    (line, to, text, (moved(start, before_start), moved(end, before_end)))
}

/// What "delete forward" removes: the selection, else the next character. At the
/// end of the text, on the empty line after a last newline: that newline (the
/// caret goes up to the end of the line above).
pub fn delete_forward_range(chars: &[char], start: usize, end: usize) -> (usize, usize) {
    if start != end { return (start, end); }
    if start >= chars.len() && chars.last() == Some(&'\n') { return (chars.len() - 1, chars.len()); }
    (start, (start + 1).min(chars.len()))
}

/// What Emacs' `transpose-chars` does at `pos`: the characters on each side of it
/// swap, or at the end of a line the two before it. The range `a..b` and its new
/// text, the caret going after it; `None` when there are not two characters.
pub fn transpose_chars(chars: &[char], pos: usize) -> Option<(usize, usize, String)> {
    let at = if chars.get(pos).is_none_or(|&c| c == '\n') { pos.checked_sub(1)? } else { pos };
    if at == 0 || at >= chars.len() { return None; }
    Some((at - 1, at + 1, [chars[at], chars[at - 1]].iter().collect()))
}

/// What Emacs' `transpose-words` does at `pos`: the word holding it or before it
/// swaps with the next one. `None` when there is no next one.
fn transpose_words(chars: &[char], pos: usize) -> Option<(usize, usize, String)> {
    let s1 = word_start_backward(chars, pos);
    let e1 = word_end_forward(chars, s1);
    let e2 = word_end_forward(chars, e1);
    let s2 = word_start_backward(chars, e2);
    if s1 >= e1 || s2 < e1 || s2 >= e2 { return None; }
    Some((s1, e2, chars[s2..e2].iter().chain(&chars[e1..s2]).chain(&chars[s1..e1]).collect()))
}

#[derive(Clone, Copy)]
pub enum Case { Upper, Lower, Capitalized }

/// `text` in that case. Capitalized: each word's first letter or digit up, the rest down.
pub fn change_case(text: &[char], case: Case) -> String {
    let mut starts_word = true;
    let mut out = String::with_capacity(text.len());
    for &c in text {
        match case {
            Case::Upper => out.extend(c.to_uppercase()),
            Case::Capitalized if starts_word => out.extend(c.to_uppercase()),
            Case::Lower | Case::Capitalized => out.extend(c.to_lowercase()),
        }
        starts_word = !c.is_alphanumeric();
    }
    out
}

/// The run of spaces and tabs around `pos`.
fn blanks_around(chars: &[char], pos: usize) -> (usize, usize) {
    let blank = |c: char| c == ' ' || c == '\t';
    let (mut a, mut b) = (pos, pos);
    while a > 0 && blank(chars[a - 1]) { a -= 1; }
    while b < chars.len() && blank(chars[b]) { b += 1; }
    (a, b)
}

/// What Emacs' `delete-indentation` does: the line of `pos` joins the one above,
/// one space between them unless one is empty or that's inside brackets.
fn join_line(chars: &[char], pos: usize) -> Option<(usize, usize, String)> {
    let ls = line_bounds(chars, pos).0;
    if ls == 0 { return None; }
    let (a, _) = blanks_around(chars, ls - 1);
    let (_, b) = blanks_around(chars, ls);
    let space = a > 0 && !matches!(chars[a - 1], '\n' | '(') && chars.get(b).is_some_and(|c| !matches!(c, '\n' | ')'));
    Some((a, b, if space { " ".to_string() } else { String::new() }))
}

/// The text edit of an action that works at the caret: the range `a..b` and its
/// new text. `None` when the action is another one, or has nothing to do there.
pub fn text_edit(action: EditorAction, chars: &[char], pos: usize) -> Option<(usize, usize, String)> {
    use EditorAction::*;
    let word = |case| {
        let b = word_end_forward(chars, pos);
        (pos < b).then(|| (pos, b, change_case(&chars[pos..b], case)))
    };
    let (a, b) = blanks_around(chars, pos.min(chars.len()));
    match action {
        TransposeChars => transpose_chars(chars, pos),
        TransposeWords => transpose_words(chars, pos),
        UpcaseWord => word(Case::Upper),
        DowncaseWord => word(Case::Lower),
        CapitalizeWord => word(Case::Capitalized),
        DeleteHorizontalSpace => (a < b).then(String::new).map(|s| (a, b, s)),
        JustOneSpace => Some((a, b, " ".to_string())),
        DeleteIndentation => join_line(chars, pos),
        _ => None,
    }
}

/// Where the caret goes after `text_edit` on `a..b` with `text`: after it, but
/// where the lines met after a join, as in Emacs.
pub fn caret_after_edit(action: EditorAction, a: usize, text: &str) -> usize {
    if action == EditorAction::DeleteIndentation { a } else { a + text.chars().count() }
}

/// Where a kill up to and including the next `c` after `pos` ends (Emacs' `zap-to-char`).
pub fn zap_end(chars: &[char], pos: usize, c: char) -> Option<usize> {
    Some(pos + chars.get(pos..)?.iter().position(|&d| d == c)? + 1)
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

/// The emphasis markers electric mode wraps a selection in: `* / _` in org-mode
/// (bold, italic, underline), `* _` in Markdown (typing `*` twice gives bold).
pub fn emphasis_pair(key: &str, format: crate::Format) -> Option<(&'static str, &'static str)> {
    match (key, format) {
        ("*", _) => Some(("*", "*")),
        ("_", _) => Some(("_", "_")),
        ("/", crate::Format::Org) => Some(("/", "/")),
        _ => None,
    }
}

/// `[` typed on a selection of one line: the link to the page it names, `[[it]]`.
pub fn link_of(key: &str, selected: &str) -> Option<String> {
    (key == "[" && !selected.trim().is_empty() && !selected.contains('\n')).then(|| format!("[[{selected}]]"))
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
        ForwardSentence => motion::forward_sentence_in_table(chars, pos).unwrap_or_else(|| motion::forward_sentence(chars, pos)),
        BackwardSentence => motion::backward_sentence_in_table(chars, pos).unwrap_or_else(|| motion::backward_sentence(chars, pos)),
        ForwardParagraph => motion::forward_paragraph(chars, pos),
        BackwardParagraph => motion::backward_paragraph(chars, pos),
        BackwardElement => motion::backward_element(chars, pos),
        ForwardElement => motion::forward_element(chars, pos),
        BackToIndentation => motion::back_to_indentation(chars, pos),
        NextHeading => motion::next_heading(chars, pos),
        PreviousHeading => motion::previous_heading(chars, pos),
        BackwardHeading => motion::backward_heading(chars, pos),
        ForwardHeading => motion::forward_heading(chars, pos),
        UpHeading => motion::up_heading(chars, pos),
        BeginningOfLine => line_bounds(chars, pos).0,
        EndOfLine => line_bounds(chars, pos).1,
        _ => return None,
    })
}

/// Position of the start of line `n` (0-based); the end of the text past the last line.
pub fn nth_line_start(chars: &[char], n: usize) -> usize {
    if n == 0 { return 0; }
    chars.iter().enumerate().filter(|&(_, &c)| c == '\n').nth(n - 1)
        .map_or(chars.len(), |(i, _)| i + 1)
}

/// Bold, italic…: wrap the selection `start..end` in `marker` (`*gras*`), or unwrap it
/// when it already is (markers just around it, or at its ends). Without a selection,
/// the two markers are inserted with the caret between them (or removed if the caret
/// is between two). Returns the range to replace, its new text and the new selection.
pub fn toggle_emphasis(chars: &[char], start: usize, end: usize, open: &str, close: &str) -> (usize, usize, String, usize, usize) {
    let (o, c): (Vec<char>, Vec<char>) = (open.chars().collect(), close.chars().collect());
    let (n, m) = (o.len(), c.len());
    let inner: String = chars[start..end].iter().collect();
    if start >= n && chars[start - n..start] == o[..] && chars.get(end..end + m) == Some(&c[..]) {
        return (start - n, end + m, inner, start - n, end - n);
    }
    if end - start >= n + m && chars[start..start + n] == o[..] && chars[end - m..end] == c[..] {
        let inside: String = chars[start + n..end - m].iter().collect();
        return (start, end, inside, start, end - n - m);
    }
    (start, end, format!("{open}{inner}{close}"), start + n, end + n)
}

/// The markers of the emphasis `action` in a page of `format`: org-mode's `*` `/` `_` `+`,
/// Markdown's `**` `*` `~~` (underline, not in Markdown, as the HTML `<u>` Obsidian reads).
pub fn emphasis_marks(action: EditorAction, format: crate::Format) -> Option<(&'static str, &'static str)> {
    use EditorAction::*;
    let md = format == crate::Format::Markdown;
    Some(match action {
        Bold => if md { ("**", "**") } else { ("*", "*") },
        Italic => if md { ("*", "*") } else { ("/", "/") },
        Underline => if md { ("<u>", "</u>") } else { ("_", "_") },
        Strikethrough => if md { ("~~", "~~") } else { ("+", "+") },
        _ => return None,
    })
}

/// The lines of `start..end` made a quote (org-mode's `#+begin_quote` block, Markdown's `> `),
/// or no longer one when they are. Returns the range to replace, its new text and the new selection.
pub fn toggle_quote(chars: &[char], start: usize, end: usize, format: crate::Format) -> (usize, usize, String, usize, usize) {
    // A selection ending at the start of a line leaves that line out
    let last = if end > start && chars[end - 1] == '\n' { end - 1 } else { end };
    let (a, b) = (line_bounds(chars, start).0, line_bounds(chars, last).1);
    let text: String = chars[a..b].iter().collect();
    let with = if format == crate::Format::Markdown {
        if text.lines().all(|l| l.trim_start().starts_with('>')) {
            text.lines().map(|l| { let l = l.trim_start().trim_start_matches('>'); l.strip_prefix(' ').unwrap_or(l) })
                .collect::<Vec<_>>().join("\n")
        } else {
            text.lines().map(|l| format!("> {l}")).collect::<Vec<_>>().join("\n")
        }
    } else {
        return toggle_block(chars, a, b, "#+begin_quote", "#+end_quote");
    };
    let len = with.chars().count();
    (a, b, with, a, a + len)
}

/// The lines of `start..end` made a comment, hidden from the exports (org-mode's
/// `#+begin_comment` block, Markdown's `<!--` `-->`), or no longer one when they
/// are. Returns as `toggle_quote`.
pub fn toggle_comment(chars: &[char], start: usize, end: usize, format: crate::Format) -> (usize, usize, String, usize, usize) {
    let last = if end > start && chars[end - 1] == '\n' { end - 1 } else { end };
    let (a, b) = (line_bounds(chars, start).0, line_bounds(chars, last).1);
    let (open, close) = if format == crate::Format::Markdown { ("<!--", "-->") } else { ("#+begin_comment", "#+end_comment") };
    toggle_block(chars, a, b, open, close)
}

/// The lines `a..b` put between the lines `open` and `close`, or taken out when
/// they already are; the new selection on the lines.
fn toggle_block(chars: &[char], a: usize, b: usize, open: &str, close: &str) -> (usize, usize, String, usize, usize) {
    let text: String = chars[a..b].iter().collect();
    let line = |s: usize, e: usize| chars[s..e].iter().collect::<String>().trim().to_lowercase();
    // Already in a block: its two lines go away
    if a > 0 && b < chars.len() {
        let before = line_bounds(chars, a - 1);
        let after = line_bounds(chars, b + 1);
        if line(before.0, before.1) == open && line(after.0, after.1) == close {
            return (before.0, after.1, text, before.0, before.0 + (b - a));
        }
    }
    let with = format!("{open}\n{text}\n{close}");
    let (o, c) = (open.chars().count() + 1, close.chars().count() + 1);
    let len = with.chars().count();
    (a, b, with, a + o, a + len - c)
}
#[cfg(test)]
mod tests;
