use crate::Format;
mod elements;
pub use elements::*;
use elements::is_table_line;

/// What the editor's view indents a line under a headline with (`tables::INDENT`
/// of the interface): no space of the text, counted as indentation.
pub const INDENT: char = '\u{a0}';

thread_local! {
    /// The format of the page being edited or drawn. Set by `FormatGuard`.
    static FORMAT: std::cell::Cell<Format> = const { std::cell::Cell::new(Format::Org) };
}

/// The format of the page being edited or drawn (see `FormatGuard`).
pub fn format() -> Format {
    FORMAT.with(|f| f.get())
}

/// The character headlines start with in that page: `*` (org-mode) or `#` (Markdown).
pub fn heading_mark() -> char {
    format().heading_mark()
}

/// While it lives, the page is in `format` (`# Titre` in Markdown); the format
/// before it comes back when it goes.
pub struct FormatGuard(Format);

impl FormatGuard {
    pub fn set(format: Format) -> Self {
        Self(FORMAT.with(|f| f.replace(format)))
    }
}

impl Drop for FormatGuard {
    fn drop(&mut self) {
        FORMAT.with(|f| f.set(self.0));
    }
}

use crate::org::{line_end, line_start};

/// Level of the headline `line` in the page's format; `line` may go on past its `\n`.
pub fn headline_level(line: &[char]) -> Option<usize> {
    format().heading_level(line)
}

/// Line with no text: only whitespace, or a heading's bare `*`s. Separates paragraphs.
fn is_empty_line(chars: &[char], start: usize, end: usize) -> bool {
    chars[start..end].iter().all(|&c| c.is_whitespace() || c == heading_mark())
}

fn is_terminator(c: char) -> bool {
    matches!(c, '.' | '!' | '?')
}

/// First non-blank char of the line (Emacs `back-to-indentation`, `M-m`).
pub fn back_to_indentation(chars: &[char], pos: usize) -> usize {
    let mut i = line_start(chars, pos);
    while i < chars.len() && (chars[i] == ' ' || chars[i] == '\t') { i += 1; }
    i
}

/// Where the text of the headline starting at `ls` begins (after its stars and
/// spaces), if the line is one.
fn heading_text(chars: &[char], ls: usize) -> Option<usize> {
    let mut i = ls + headline_level(&chars[ls..])?;
    while i < chars.len() && chars[i] == ' ' { i += 1; }
    Some(i)
}

/// A line that ends a paragraph of sentences: empty, or a headline (a paragraph
/// of its own, as in org-mode).
fn is_paragraph_break(chars: &[char], ls: usize) -> bool {
    is_empty_line(chars, ls, line_end(chars, ls)) || heading_text(chars, ls).is_some()
}

/// Where the text of the paragraph holding `pos` starts.
fn sentence_paragraph_start(chars: &[char], pos: usize) -> usize {
    let mut ls = line_start(chars, pos);
    if let Some(t) = heading_text(chars, ls) { return t; }
    while ls > 0 {
        let prev = line_start(chars, ls - 1);
        if is_paragraph_break(chars, prev) { break; }
        ls = prev;
    }
    ls
}

/// Where the text of the paragraph holding `pos` ends.
fn sentence_paragraph_end(chars: &[char], pos: usize) -> usize {
    let mut le = line_end(chars, pos);
    if heading_text(chars, line_start(chars, pos)).is_some() { return le; }
    while le < chars.len() && !is_paragraph_break(chars, le + 1) {
        le = line_end(chars, le + 1);
    }
    le
}

/// End of the current sentence (just after its `.`, `!` or `?`), or of the
/// paragraph text when it ends first (Emacs `forward-sentence`, `M-e`). A
/// headline is a paragraph of its own.
pub fn forward_sentence(chars: &[char], pos: usize) -> usize {
    let n = chars.len();
    let mut i = pos;
    // Past the blanks and the stars of headlines, to the next text
    loop {
        while i < n && chars[i].is_whitespace() { i += 1; }
        let ls = line_start(chars, i);
        match heading_text(chars, ls) {
            Some(t) if i < t => i = t,
            _ => break,
        }
    }
    let end = sentence_paragraph_end(chars, i);
    while i < end {
        if is_terminator(chars[i]) && (i + 1 == end || chars[i + 1].is_whitespace()) {
            return i + 1;
        }
        i += 1;
    }
    end.max(pos)
}

/// Start of the current sentence, or of the previous one when already at a start
/// (Emacs `backward-sentence`, `M-a`). A headline is a paragraph of its own.
pub fn backward_sentence(chars: &[char], pos: usize) -> usize {
    let mut j = pos;
    // Back past the blanks and the stars of headlines, to the previous text
    while j > 0 {
        if chars[j - 1].is_whitespace() { j -= 1; continue; }
        let ls = line_start(chars, j);
        match heading_text(chars, ls) {
            Some(t) if j <= t && ls < j => j = ls,
            _ => break,
        }
    }
    if j > 0 && is_terminator(chars[j - 1]) { j -= 1; }
    let para = sentence_paragraph_start(chars, j);
    let mut start = para;
    let mut i = j;
    while i > para {
        if i < chars.len() && is_terminator(chars[i - 1]) && chars[i].is_whitespace() { start = i; break; }
        i -= 1;
    }
    while start < pos && start < chars.len() && chars[start].is_whitespace() { start += 1; }
    start.min(pos)
}

/// A field of a table row: where its text begins and ends.
type Field = (usize, usize);

/// The table holding the line of `pos`, if it is one: where it starts and ends,
/// and the fields of its rows (`|---+---|` has none). The text of an empty field
/// is one space after its `|`.
fn table_around(chars: &[char], pos: usize) -> Option<(usize, usize, Vec<Field>)> {
    let mut start = line_start(chars, pos);
    if !is_table_line(chars, start) { return None; }
    while start > 0 && is_table_line(chars, line_start(chars, start - 1)) { start = line_start(chars, start - 1); }
    let mut fields = Vec::new();
    let mut ls = start;
    loop {
        let le = line_end(chars, ls);
        let first = ls + indent_of(chars, ls);
        if chars.get(first + 1) != Some(&'-') {
            let mut bars: Vec<usize> = (first..le).filter(|&i| chars[i] == '|').collect();
            // A last field with no `|` after it
            if bars.last().is_some_and(|&b| chars[b + 1..le].iter().any(|c| !c.is_whitespace())) { bars.push(le); }
            for w in bars.windows(2) {
                let mut a = w[0] + 1;
                if a < w[1] && chars[a] == ' ' { a += 1; }
                let mut b = w[1];
                while b > a && chars[b - 1] == ' ' { b -= 1; }
                fields.push((a, b));
            }
        }
        if le >= chars.len() || !is_table_line(chars, le + 1) { return Some((start, le, fields)); }
        ls = le + 1;
    }
}

/// In a table, `M-e` goes to the end of the field, or of the next one when
/// already there; past the last one, out of the table (org-mode's
/// `org-forward-sentence`). `None` out of a table.
pub fn forward_sentence_in_table(chars: &[char], pos: usize) -> Option<usize> {
    let (_, end, fields) = table_around(chars, pos)?;
    Some(fields.iter().map(|&(_, b)| b).find(|&b| b > pos)
        .unwrap_or_else(|| forward_sentence(chars, (end + 1).min(chars.len()))))
}

/// In a table, `M-a` goes to the start of the field, or of the previous one when
/// already there; before the first one, out of the table (org-mode's
/// `org-backward-sentence`). `None` out of a table.
pub fn backward_sentence_in_table(chars: &[char], pos: usize) -> Option<usize> {
    let (start, _, fields) = table_around(chars, pos)?;
    Some(fields.iter().rev().map(|&(a, _)| a).find(|&a| a < pos)
        .unwrap_or_else(|| backward_sentence(chars, start)))
}

/// Empty line that ends the current paragraph (Emacs `forward-paragraph`); lands
/// at its end, so after the `*`s of a bare heading.
pub fn forward_paragraph(chars: &[char], pos: usize) -> usize {
    let n = chars.len();
    let mut ls = line_start(chars, pos);
    // skip blank lines, land on the first line of text
    while ls < n {
        let e = line_end(chars, ls);
        if !is_empty_line(chars, ls, e) { break; }
        if e >= n { return n; }
        ls = e + 1;
    }
    // walk down to the next blank line
    loop {
        let e = line_end(chars, ls);
        if e >= n { return n; }
        ls = e + 1;
        if ls >= n { return n; }
        if is_empty_line(chars, ls, line_end(chars, ls)) { return line_end(chars, ls); }
    }
}

/// Empty line that starts the current paragraph (Emacs `backward-paragraph`).
pub fn backward_paragraph(chars: &[char], pos: usize) -> usize {
    let mut ls = line_start(chars, pos);
    // on a blank line: first hop up to the text of the previous paragraph
    if is_empty_line(chars, ls, line_end(chars, ls)) {
        loop {
            if ls == 0 { return 0; }
            ls = line_start(chars, ls - 1);
            if !is_empty_line(chars, ls, line_end(chars, ls)) { break; }
        }
    }
    // walk up to the blank line above this paragraph
    loop {
        if ls == 0 { return 0; }
        let prev = line_start(chars, ls - 1);
        if is_empty_line(chars, prev, line_end(chars, prev)) { return line_end(chars, prev); }
        ls = prev;
    }
}

/// Start of the next heading line after the current line (org `C-c C-n`).
pub fn next_heading(chars: &[char], pos: usize) -> usize {
    let n = chars.len();
    let mut e = line_end(chars, pos);
    while e < n {
        let s = e + 1;
        let le = line_end(chars, s);
        if headline_level(&chars[s..le]).is_some() { return s; }
        e = le;
    }
    pos
}

/// Start of the closest heading line before the current line (org `C-c C-p`).
pub fn previous_heading(chars: &[char], pos: usize) -> usize {
    let mut s = line_start(chars, pos);
    while s > 0 {
        s = line_start(chars, s - 1);
        if headline_level(&chars[s..]).is_some() { return s; }
    }
    pos
}

/// Start of the headline holding the caret (its line or the text under it).
fn enclosing_heading(chars: &[char], pos: usize) -> Option<usize> {
    let mut s = line_start(chars, pos);
    loop {
        if headline_level(&chars[s..]).is_some() { return Some(s); }
        if s == 0 { return None; }
        s = line_start(chars, s - 1);
    }
}

/// The next (or previous) headline of the same level as the one holding the
/// caret, without leaving the parent's subtree; else the start of that headline
/// (org `C-c C-f` / `C-c C-b`). Before the first headline: the next headline, or
/// the start of the page.
fn heading_same_level(chars: &[char], pos: usize, forward: bool) -> usize {
    let Some(h) = enclosing_heading(chars, pos) else {
        return if forward { next_heading(chars, pos) } else { 0 };
    };
    let level = headline_level(&chars[h..]).unwrap_or(0);
    let n = chars.len();
    let mut s = h;
    loop {
        if forward {
            let e = line_end(chars, s);
            if e >= n { return h; }
            s = e + 1;
        } else {
            if s == 0 { return h; }
            s = line_start(chars, s - 1);
        }
        match headline_level(&chars[s..]) {
            Some(l) if l < level => return h,
            Some(l) if l == level => return s,
            _ => {}
        }
    }
}

pub fn backward_heading(chars: &[char], pos: usize) -> usize { heading_same_level(chars, pos, false) }

pub fn forward_heading(chars: &[char], pos: usize) -> usize { heading_same_level(chars, pos, true) }

/// The parent of the headline holding the caret (org `C-c C-u`); stays put at
/// the top level or before the first headline.
pub fn up_heading(chars: &[char], pos: usize) -> usize {
    let Some(h) = enclosing_heading(chars, pos) else { return pos };
    let level = headline_level(&chars[h..]).unwrap_or(0);
    let mut s = h;
    while s > 0 {
        s = line_start(chars, s - 1);
        if headline_level(&chars[s..]).is_some_and(|l| l < level) { return s; }
    }
    pos
}

#[cfg(test)]
mod tests;
