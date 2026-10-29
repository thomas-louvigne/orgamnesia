//! Pure cursor-motion helpers (Emacs-style sentence, paragraph and heading moves).
//! Positions are char indexes into `chars`.

fn line_start(chars: &[char], pos: usize) -> usize {
    chars[..pos].iter().rposition(|&c| c == '\n').map(|i| i + 1).unwrap_or(0)
}

fn line_end(chars: &[char], pos: usize) -> usize {
    chars[pos..].iter().position(|&c| c == '\n').map(|i| pos + i).unwrap_or(chars.len())
}

/// Line with no text: only whitespace, or a heading's bare `*`s. Separates paragraphs.
fn is_empty_line(chars: &[char], start: usize, end: usize) -> bool {
    chars[start..end].iter().all(|&c| c.is_whitespace() || c == '*')
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
    let stars = chars[ls..].iter().take_while(|&&c| c == '*').count();
    if stars == 0 || !matches!(chars.get(ls + stars), Some(' ' | '\n') | None) { return None; }
    let mut i = ls + stars;
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

/// `*`… followed by a space (or alone on the line): an org heading line.
fn is_heading(chars: &[char], start: usize, end: usize) -> bool {
    let stars = chars[start..end].iter().take_while(|&&c| c == '*').count();
    stars > 0 && (start + stars == end || chars[start + stars] == ' ')
}

/// Start of the next heading line after the current line (org `C-c C-n`).
pub fn next_heading(chars: &[char], pos: usize) -> usize {
    let n = chars.len();
    let mut e = line_end(chars, pos);
    while e < n {
        let s = e + 1;
        let le = line_end(chars, s);
        if is_heading(chars, s, le) { return s; }
        e = le;
    }
    pos
}

/// Start of the closest heading line before the current line (org `C-c C-p`).
pub fn previous_heading(chars: &[char], pos: usize) -> usize {
    let mut s = line_start(chars, pos);
    while s > 0 {
        s = line_start(chars, s - 1);
        if is_heading(chars, s, line_end(chars, s)) { return s; }
    }
    pos
}

fn heading_level(chars: &[char], ls: usize) -> Option<usize> {
    heading_text(chars, ls).map(|_| chars[ls..].iter().take_while(|&&c| c == '*').count())
}

/// Start of the headline holding the caret (its line or the text under it).
fn enclosing_heading(chars: &[char], pos: usize) -> Option<usize> {
    let mut s = line_start(chars, pos);
    loop {
        if heading_level(chars, s).is_some() { return Some(s); }
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
    let level = heading_level(chars, h).unwrap_or(0);
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
        match heading_level(chars, s) {
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
    let level = heading_level(chars, h).unwrap_or(0);
    let mut s = h;
    while s > 0 {
        s = line_start(chars, s - 1);
        if heading_level(chars, s).is_some_and(|l| l < level) { return s; }
    }
    pos
}

// ─── Org elements (M-{ / M-}) ─────────────────────────────────────────────────

/// An org element of a section: paragraph, plain list, item, table, block,
/// drawer, keyword. `end` takes in the blank lines that follow it; `contents_end`
/// is where its last child ends (greater elements: lists, items).
struct Elem { begin: usize, end: usize, contents_end: usize, parent: Option<usize> }

/// Indentation of the line at `ls` (the view's indent counts too).
pub(crate) fn indent_of(chars: &[char], ls: usize) -> usize {
    chars[ls..].iter().take_while(|&&c| matches!(c, ' ' | '\t' | crate::tables::INDENT)).count()
}

fn is_blank(chars: &[char], ls: usize) -> bool {
    chars[ls..line_end(chars, ls)].iter().all(|c| c.is_whitespace())
}

/// Where the contents of the list item at `ls` begin (after its bullet and
/// checkbox), if the line is one: `-`, `+`, `*` (indented), `1.`, `1)`.
pub(crate) fn item_contents(chars: &[char], ls: usize) -> Option<usize> {
    let ind = indent_of(chars, ls);
    let mut i = ls + ind;
    let le = line_end(chars, ls);
    match chars.get(i) {
        Some('-' | '+') => i += 1,
        Some('*') if ind > 0 => i += 1,
        Some(c) if c.is_ascii_digit() => {
            while i < le && chars[i].is_ascii_digit() { i += 1; }
            if !matches!(chars.get(i), Some('.' | ')')) { return None; }
            i += 1;
        }
        _ => return None,
    }
    if i < le && chars[i] != ' ' { return None; }
    while i < le && chars[i] == ' ' { i += 1; }
    if le - i >= 3 && chars[i] == '[' && matches!(chars[i + 1], ' ' | 'X' | 'x' | '-') && chars[i + 2] == ']' {
        i += 3;
        while i < le && chars[i] == ' ' { i += 1; }
    }
    Some(i)
}

fn starts_with_ci(chars: &[char], at: usize, s: &str) -> bool {
    s.chars().enumerate().all(|(k, c)| chars.get(at + k).is_some_and(|d| d.to_ascii_lowercase() == c))
}

fn is_table_line(chars: &[char], ls: usize) -> bool {
    chars.get(ls + indent_of(chars, ls)) == Some(&'|')
}

/// `:NAME:` alone on its line: a drawer opens.
pub(crate) fn is_drawer_start(chars: &[char], ls: usize) -> bool {
    let t: String = chars[ls + indent_of(chars, ls)..line_end(chars, ls)].iter().collect();
    let t = t.trim_end();
    t.len() > 2 && t.starts_with(':') && t.ends_with(':') && !t.eq_ignore_ascii_case(":end:")
        && t[1..t.len() - 1].chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

/// A line that starts an element of its own, ending the paragraph above.
fn starts_element(chars: &[char], ls: usize) -> bool {
    let t = ls + indent_of(chars, ls);
    heading_level(chars, ls).is_some() || item_contents(chars, ls).is_some()
        || is_table_line(chars, ls) || starts_with_ci(chars, t, "#+") || is_drawer_start(chars, ls)
}

struct Elements<'a> { chars: &'a [char], elems: Vec<Elem> }

impl Elements<'_> {
    fn next_line(&self, ls: usize) -> usize {
        let e = line_end(self.chars, ls);
        if e >= self.chars.len() { self.chars.len() } else { e + 1 }
    }

    fn push(&mut self, begin: usize, parent: Option<usize>) -> usize {
        self.elems.push(Elem { begin, end: begin, contents_end: begin, parent });
        self.elems.len() - 1
    }

    /// The elements from the line `ls` up to `limit`, a headline, or (inside an
    /// item indented at `item`) a line not indented more than its bullet. Blank
    /// lines go to the element above (`last` at first); returns the last one.
    fn section(&mut self, mut ls: usize, limit: usize, item: Option<usize>, parent: Option<usize>,
               mut last: Option<usize>) -> Option<usize> {
        while ls < limit {
            if is_blank(self.chars, ls) {
                ls = self.next_line(ls).min(limit);
                if let Some(i) = last { self.elems[i].end = ls; }
                continue;
            }
            if heading_level(self.chars, ls).is_some() { break; }
            if item.is_some_and(|k| indent_of(self.chars, ls) <= k) { break; }
            let i = self.element(ls, ls, limit, item, parent);
            ls = self.elems[i].end;
            last = Some(i);
        }
        last
    }

    /// The element whose first line starts at `ls`, its text at `at`.
    fn element(&mut self, ls: usize, at: usize, limit: usize, item: Option<usize>, parent: Option<usize>) -> usize {
        let chars = self.chars;
        let t = ls + indent_of(chars, ls);
        if at == ls && item_contents(chars, ls).is_some() { return self.list(ls, limit, parent); }
        let i = self.push(at, parent);
        let mut e = self.next_line(ls).min(limit);
        if at != ls {
            // The text after a bullet: a paragraph
        } else if is_table_line(chars, ls) {
            while e < limit && is_table_line(chars, e) { e = self.next_line(e).min(limit); }
        } else if starts_with_ci(chars, t, "#+begin_") || is_drawer_start(chars, ls) {
            let end = if is_drawer_start(chars, ls) { ":end:" } else { "#+end_" };
            let mut f = e;
            while f < limit && heading_level(chars, f).is_none() {
                let g = self.next_line(f).min(limit);
                if starts_with_ci(chars, f + indent_of(chars, f), end) { e = g; break; }
                f = g;
            }
        }
        if at != ls || !starts_element(chars, ls) {
            while e < limit && !is_blank(chars, e) && !starts_element(chars, e)
                && item.is_none_or(|k| indent_of(chars, e) > k)
            {
                e = self.next_line(e).min(limit);
            }
        }
        self.elems[i].end = e;
        i
    }

    /// The plain list whose first item starts at `ls`, and its items.
    fn list(&mut self, ls: usize, limit: usize, parent: Option<usize>) -> usize {
        let chars = self.chars;
        let k = indent_of(chars, ls);
        let list = self.push(ls, parent);
        let mut cur = ls;
        loop {
            let item = self.push(cur, Some(list));
            // The item runs over the lines indented more than its bullet; two
            // blank lines in a row end the list.
            let mut e = self.next_line(cur).min(limit);
            let mut blanks = 0;
            while e < limit && heading_level(chars, e).is_none() {
                if is_blank(chars, e) {
                    blanks += 1;
                    if blanks == 2 { break; }
                } else if indent_of(chars, e) <= k {
                    break;
                } else {
                    blanks = 0;
                }
                e = self.next_line(e).min(limit);
            }
            let two_blanks = blanks == 2;
            while e < limit && is_blank(chars, e) { e = self.next_line(e).min(limit); }
            self.elems[item].end = e;
            let text = item_contents(chars, cur).unwrap_or(cur);
            let first_line_end = line_end(chars, cur);
            let (rest, first) = if text < first_line_end {
                let p = self.element(cur, text, e, Some(k), Some(item));
                (self.elems[p].end, Some(p))
            } else {
                (self.next_line(cur).min(e), None)
            };
            let last = self.section(rest, e, Some(k), Some(item), first);
            self.elems[item].contents_end = last.map_or(e, |p| self.elems[p].end);
            let next_item = !two_blanks && e < limit && indent_of(chars, e) == k
                && item_contents(chars, e).is_some() && heading_level(chars, e).is_none();
            if !next_item {
                self.elems[list].end = e;
                self.elems[list].contents_end = e;
                return list;
            }
            cur = e;
        }
    }
}

fn elements(chars: &[char]) -> Vec<Elem> {
    let mut els = Elements { chars, elems: vec![] };
    let n = chars.len();
    let mut ls = 0;
    while ls < n {
        if heading_level(chars, ls).is_some() { ls = els.next_line(ls); continue; }
        let mut limit = ls;
        while limit < n && heading_level(chars, limit).is_none() { limit = els.next_line(limit); }
        els.section(ls, limit, None, None, None);
        ls = limit;
    }
    els.elems
}

/// The bullet that starts a new item after the list item holding `pos` (org
/// `M-RET`): its indentation, the same bullet or the next number, and an empty
/// checkbox when it has one.
pub fn new_item_bullet(chars: &[char], pos: usize) -> Option<String> {
    let mut ls = line_start(chars, pos);
    let mut below = usize::MAX;
    loop {
        if is_blank(chars, ls) || heading_level(chars, ls).is_some() { return None; }
        let ind = indent_of(chars, ls);
        if ind < below {
            if let Some(text) = item_contents(chars, ls) {
                let b = ls + ind;
                let mut e = b;
                while chars[e].is_ascii_digit() { e += 1; }
                let bullet = if e > b {
                    let n: u64 = chars[b..e].iter().collect::<String>().parse().ok()?;
                    format!("{}{}", n + 1, chars[e])
                } else {
                    chars[b].to_string()
                };
                let checkbox = if chars[b..text].contains(&'[') { "[ ] " } else { "" };
                return Some(format!("{}{bullet} {checkbox}", chars[ls..b].iter().collect::<String>()));
            }
            if ind == 0 { return None; }
            below = ind;
        }
        if ls == 0 { return None; }
        ls = line_start(chars, ls - 1);
    }
}

/// The innermost element holding `pos`.
fn element_at(els: &[Elem], pos: usize, n: usize) -> Option<usize> {
    els.iter().enumerate()
        .filter(|(_, e)| e.begin <= pos && (pos < e.end || (pos == n && e.end == n)))
        .max_by_key(|(i, e)| (e.begin, *i))
        .map(|(i, _)| i)
}

/// The next element (org `M-}`, `org-forward-element`): on a headline, the next
/// one past its subtree; else the end of the element holding the caret (of its
/// parent too when it is the last child: from an item, the next item).
pub fn forward_element(chars: &[char], pos: usize) -> usize {
    let n = chars.len();
    if pos >= n { return pos; }
    let ls = line_start(chars, pos);
    if let Some(level) = heading_level(chars, ls) {
        let mut e = line_end(chars, ls);
        while e < n {
            let s = e + 1;
            if heading_level(chars, s).is_some_and(|l| l <= level) { return s; }
            e = line_end(chars, s);
        }
        return pos;
    }
    let els = elements(chars);
    match element_at(&els, pos, n) {
        Some(i) => match els[i].parent {
            Some(p) if els[p].contents_end == els[i].end => els[p].end,
            _ => els[i].end,
        },
        // Blank lines under a headline: the first element below, or the next headline
        None => {
            let h = next_heading(chars, pos);
            let h = if h == pos { n } else { h };
            els.iter().map(|e| e.begin).filter(|&b| b > pos).fold(h, usize::min)
        }
    }
}

/// The previous element (org `M-{`, `org-backward-element`): on a headline, the
/// previous one of the same level, else its parent; else the start of the
/// element holding the caret, or of the one before when already there.
pub fn backward_element(chars: &[char], pos: usize) -> usize {
    if pos == 0 { return 0; }
    let n = chars.len();
    let ls = line_start(chars, pos);
    if heading_level(chars, ls).is_some() {
        let b = heading_same_level(chars, pos, false);
        return if b != pos { b } else { up_heading(chars, pos) };
    }
    let els = elements(chars);
    let beg = match element_at(&els, pos, n) {
        Some(i) if els[i].begin != pos => return els[i].begin,
        Some(i) => els[i].begin,
        None => pos,
    };
    let mut j = beg;
    while j > 0 && chars[j - 1].is_whitespace() { j -= 1; }
    if j == 0 { return 0; }
    if heading_level(chars, line_start(chars, j)).is_some() { return line_start(chars, j); }
    let Some(mut cur) = element_at(&els, j, n) else { return line_start(chars, j) };
    let mut dest = els[cur].begin;
    while let Some(p) = els[cur].parent {
        if els[p].end > beg { break; }
        dest = els[p].begin;
        cur = p;
    }
    dest
}

pub use orgamnesia_core::hashtags::{hashtag_at, Hashtags};

/// The `#tag` being typed when the caret is at `caret`: returns the index of the
/// `#` and the part of the tag already typed (possibly empty, right after `#`).
pub fn hashtag_prefix(chars: &[char], caret: usize, tags: Hashtags) -> Option<(usize, String)> {
    if tags == Hashtags::Off || chars.get(caret).is_some_and(|&c| tags.is_tag_char(c)) { return None; }
    let mut start = caret;
    while start > 0 && tags.is_tag_char(chars[start - 1]) { start -= 1; }
    let hash = start.checked_sub(1)?;
    if chars[hash] != '#' { return None; }
    if hash > 0 && !(chars[hash - 1].is_whitespace() || "([{\"'".contains(chars[hash - 1])) { return None; }
    if start < caret && !chars[start].is_alphanumeric() { return None; }
    Some((hash, chars[start..caret].iter().collect()))
}

pub use orgamnesia_core::hashtags::{filetags_value_start, headline_tags_range, tag_ranges, OrgTags, TagSyntax};

/// The org-mode tag under `pos`: in the `:a:b:` of a headline, a `#+FILETAGS:` line
/// or the text (`voir :exemple:`).
pub fn org_tag_at(chars: &[char], pos: usize, org: OrgTags) -> Option<String> {
    tag_ranges(chars, org, true).into_iter()
        .find(|&(a, b)| a <= pos && pos <= b)
        .map(|(a, b)| chars[a..b].iter().collect())
}

/// The org-mode tag being typed when the caret is at `caret`: after `:` in the tags
/// of a headline (`* Title :pro`), in a `#+FILETAGS:` line, or as `:pro` in the
/// text (a character typed at least). Returns the index
/// where the tag starts and the part already typed (possibly empty).
pub fn org_tag_prefix(chars: &[char], caret: usize, org: OrgTags) -> Option<(usize, String)> {
    let is_org_tag_char = |c: char| org.is_tag_char(c);
    if chars.get(caret).is_some_and(|&c| is_org_tag_char(c)) { return None; }
    let (ls, le) = (line_start(chars, caret), line_end(chars, caret));
    let mut start = caret;
    while start > ls && is_org_tag_char(chars[start - 1]) { start -= 1; }
    let prefix: String = chars[start..caret].iter().collect();
    let line = &chars[ls..le];
    if let Some(v) = filetags_value_start(line) {
        let ok = start - ls >= v && (start - ls == v || chars[start - 1] == ':' || chars[start - 1].is_whitespace());
        return ok.then_some((start, prefix));
    }
    let stars = line.iter().take_while(|&&c| c == '*').count();
    if stars == 0 || line.get(stars) != Some(&' ') {
        // In the text: a word starting with `:`, at the start of the line or after a
        // space, once a character of the tag is typed (`10:30`, `http://` are not tags)
        let ok = !prefix.is_empty() && start > ls && chars[start - 1] == ':'
            && (start - 1 == ls || chars[start - 2].is_whitespace());
        return ok.then_some((start, prefix));
    }
    // Headline: the caret is in its last word, which starts with `:` after a space
    let after: String = chars[caret..le].iter().collect();
    if after.trim_end().chars().any(|c| c != ':' && !is_org_tag_char(c)) { return None; }
    if start == 0 || chars[start - 1] != ':' { return None; }
    let mut b = start - 1;
    while b > ls + stars && (chars[b - 1] == ':' || is_org_tag_char(chars[b - 1])) { b -= 1; }
    (chars[b] == ':' && chars[b - 1].is_whitespace()).then_some((start, prefix))
}

/// The `[[target` being typed when the caret is at `caret` (inside `[[…`, before
/// any `]` or `[`, on the same line): returns the index where the target starts
/// and the part already typed.
pub fn link_prefix(chars: &[char], caret: usize) -> Option<(usize, String)> {
    let mut start = caret;
    while start > 0 && !matches!(chars[start - 1], '[' | ']' | '\n') { start -= 1; }
    if start < 2 || chars[start - 1] != '[' || chars[start - 2] != '[' { return None; }
    Some((start, chars[start..caret].iter().collect()))
}

/// Page names completing `prefix` (ignoring case and accents): names starting with it first
/// (shortest first), then names containing it; the exact name is left out. With
/// `tag: Some(_)`, only names usable as such a `#tag` are kept.
pub fn complete_page(names: &[String], prefix: &str, tag: Option<Hashtags>, limit: usize) -> Vec<String> {
    let p = orgamnesia_core::names::fold(prefix.trim());
    let mut starts = Vec::new();
    let mut contains = Vec::new();
    for name in names {
        let valid = tag.is_none_or(|t| name.chars().next().is_some_and(char::is_alphanumeric)
            && name.chars().all(|c| t.is_tag_char(c)));
        let n = orgamnesia_core::names::fold(name);
        if !valid || n == p { continue; }
        if n.starts_with(&p) { starts.push(name.clone()); }
        else if n.contains(&p) { contains.push(name.clone()); }
    }
    starts.sort_by_key(|n| (n.chars().count(), n.to_lowercase()));
    contains.sort_by_key(|n| n.to_lowercase());
    starts.into_iter().chain(contains).take(limit).collect()
}

/// Char range of the target text of the first `[[target]]` / `[[target][label]]`
/// (or `[[target::place]]`) link naming `target`, else of the first `#tag` or `:tag:` naming it (per `tags`).
pub fn find_link(chars: &[char], target: &str, ignore_case: bool, tags: TagSyntax) -> Option<(usize, usize)> {
    let norm = |s: &str| orgamnesia_core::names::page_key(s.trim(), ignore_case);
    let wanted = norm(target);
    let mut i = 0;
    while i < chars.len() {
        if let Some((tag, end)) = hashtag_at(chars, i, tags.hashtags) {
            if norm(&tag) == wanted { return Some((i + 1, end)); }
            i = end;
        } else if i + 1 < chars.len() && chars[i] == '[' && chars[i + 1] == '[' {
            let start = i + 2;
            let mut end = start;
            while end < chars.len() && chars[end] != ']' && chars[end] != '[' { end += 1; }
            let raw: String = chars[start..end].iter().collect();
            // `[[Page::*Titre]]` is a link to `Page`
            if norm(&raw) == wanted || norm(orgamnesia_core::links::split(&raw).0) == wanted {
                let lead = raw.chars().take_while(|c| c.is_whitespace()).count();
                let trail = raw.chars().rev().take_while(|c| c.is_whitespace()).count();
                return Some((start + lead, end - trail));
            }
            i = end;
        } else {
            i += 1;
        }
    }
    // Then the org-mode tags (`#+FILETAGS:`, `* Title :tag:`, `:tag:` in the text)
    if !tags.org.links { return None; }
    tag_ranges(chars, tags.org, true).into_iter()
        .find(|&(a, b)| norm(&chars[a..b].iter().collect::<String>()) == wanted)
}

/// `content` with the file tag `tag` added to its `#+FILETAGS:` line, which is
/// created after the leading `#+` keywords (the top of the page) when missing.
/// `None` when the page already has that file tag.
pub fn add_filetag(content: &str, tag: &str) -> Option<String> {
    let mut lines: Vec<String> = content.split('\n').map(String::from).collect();
    let found = lines.iter().position(|l| filetags_value_start(&l.chars().collect::<Vec<_>>()).is_some());
    match found {
        Some(i) => {
            let chars: Vec<char> = lines[i].chars().collect();
            let v = filetags_value_start(&chars)?;
            let head: String = chars[..v].iter().collect();
            let value: String = chars[v..].iter().collect();
            if value.split(':').any(|t| t.trim().eq_ignore_ascii_case(tag)) {
                return None;
            }
            let value = value.trim_end();
            lines[i] = if value.trim().is_empty() {
                format!("{head} :{tag}:")
            } else if value.ends_with(':') {
                format!("{head}{value}{tag}:")
            } else {
                format!("{head}{value} :{tag}:")
            };
        }
        None => {
            let at = lines.iter().take_while(|l| l.trim_start().starts_with("#+")).count();
            lines.insert(at, format!("#+FILETAGS: :{tag}:"));
        }
    }
    Some(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filetag_added() {
        assert_eq!(add_filetag("* Page\n", "pro").as_deref(), Some("#+FILETAGS: :pro:\n* Page\n"));
        assert_eq!(add_filetag("", "pro").as_deref(), Some("#+FILETAGS: :pro:\n"));
        assert_eq!(add_filetag("#+title: P\n* P", "pro").as_deref(), Some("#+title: P\n#+FILETAGS: :pro:\n* P"));
        assert_eq!(add_filetag("#+FILETAGS: :a:\nx", "b").as_deref(), Some("#+FILETAGS: :a:b:\nx"));
        assert_eq!(add_filetag("#+filetags:\nx", "b").as_deref(), Some("#+filetags: :b:\nx"));
        assert_eq!(add_filetag("#+filetags: :a:  \nx", "b").as_deref(), Some("#+filetags: :a:b:\nx"));
        assert_eq!(add_filetag("x\n#+filetags: :A:\n", "a"), None);
    }

    fn c(s: &str) -> Vec<char> { s.chars().collect() }

    #[test]
    fn hashtag_prefix_while_typing() {
        let d = Hashtags::Dashes;
        assert_eq!(hashtag_prefix(&c("see #"), 5, d), Some((4, String::new())));
        assert_eq!(hashtag_prefix(&c("see #ru"), 7, d), Some((4, "ru".into())));
        assert_eq!(hashtag_prefix(&c("#ru"), 3, d), Some((0, "ru".into())));
        assert_eq!(hashtag_prefix(&c("(#ru)"), 4, d), Some((1, "ru".into())));
        // Caret in the middle of a tag, `#` glued to a word, heading-like `#_`
        assert_eq!(hashtag_prefix(&c("#rust"), 3, d), None);
        assert_eq!(hashtag_prefix(&c("a#ru"), 4, d), None);
        assert_eq!(hashtag_prefix(&c("#_x"), 3, d), None);
        assert_eq!(hashtag_prefix(&c("no tag"), 6, d), None);
        // A dash ends an org-mode style tag
        assert_eq!(hashtag_prefix(&c("#mon-ta"), 7, d), Some((0, "mon-ta".into())));
        assert_eq!(hashtag_prefix(&c("#mon-ta"), 7, Hashtags::Org), None);
        assert_eq!(hashtag_prefix(&c("#ru"), 3, Hashtags::Off), None);
    }

    #[test]
    fn complete_page_ranks_prefix_matches_first() {
        let names: Vec<String> = ["Rust", "rust-async", "Trust", "Notes", "my page", "Rustacean"]
            .iter().map(|s| s.to_string()).collect();
        let d = Some(Hashtags::Dashes);
        assert_eq!(complete_page(&names, "ru", d, 10), vec!["Rust", "Rustacean", "rust-async", "Trust"]);
        // The exact name is not proposed; names with spaces can't be tags
        assert_eq!(complete_page(&names, "rust", d, 10), vec!["Rustacean", "rust-async", "Trust"]);
        assert_eq!(complete_page(&names, "", d, 2), vec!["Rust", "Notes"]);
        assert!(complete_page(&names, "page", d, 10).is_empty());
        // Names with a dash can't be org-mode style tags
        assert_eq!(complete_page(&names, "ru", Some(Hashtags::Org), 10), vec!["Rust", "Rustacean", "Trust"]);
        // Links accept any page name
        assert_eq!(complete_page(&names, "page", None, 10), vec!["my page"]);
        // Accents ignored
        let names: Vec<String> = ["Règles", "Élody"].iter().map(|s| s.to_string()).collect();
        assert_eq!(complete_page(&names, "regle", None, 10), vec!["Règles"]);
        assert_eq!(complete_page(&names, "elo", None, 10), vec!["Élody"]);
    }

    #[test]
    fn link_prefix_while_typing() {
        assert_eq!(link_prefix(&c("see [[my pa"), 11), Some((6, "my pa".into())));
        assert_eq!(link_prefix(&c("[[]]"), 2), Some((2, String::new())));
        assert_eq!(link_prefix(&c("[[a][lab"), 8), None);   // in the label
        assert_eq!(link_prefix(&c("[[a]] b"), 7), None);    // after the link
        assert_eq!(link_prefix(&c("[x"), 2), None);
        assert_eq!(link_prefix(&c("[[a\nb"), 5), None);     // other line
    }

    #[test]
    fn org_tags_under_the_caret() {
        let t = c("* Titre :projet:urgent:\n#+FILETAGS: :a:b:\ntexte :x:");
        assert_eq!(org_tag_at(&t, 10, ORG), Some("projet".into()));
        assert_eq!(org_tag_at(&t, 20, ORG), Some("urgent".into()));
        assert_eq!(org_tag_at(&t, 3, ORG), None);            // in the title
        assert_eq!(org_tag_at(&t, 38, ORG), Some("a".into()));
        assert_eq!(org_tag_at(&t, 49, ORG), Some("x".into())); // in the text
        assert_eq!(org_tag_at(&t, 44, ORG), None);
    }

    const ORG: OrgTags = OrgTags { links: true, dashes: false };

    #[test]
    fn org_tag_prefix_while_typing() {
        // In the text
        assert_eq!(org_tag_prefix(&c("voir :pro"), 9, ORG), Some((6, "pro".into())));
        assert_eq!(org_tag_prefix(&c(":pro"), 4, ORG), Some((1, "pro".into())));
        assert_eq!(org_tag_prefix(&c("voir :"), 6, ORG), None);
        assert_eq!(org_tag_prefix(&c("à 10:30"), 7, ORG), None);
        assert_eq!(org_tag_prefix(&c("http://x"), 8, ORG), None);
        assert_eq!(org_tag_prefix(&c("* T :pro"), 8, ORG), Some((5, "pro".into())));
        assert_eq!(org_tag_prefix(&c("* T :"), 5, ORG), Some((5, String::new())));
        assert_eq!(org_tag_prefix(&c("* T :a:b"), 8, ORG), Some((7, "b".into())));
        assert_eq!(org_tag_prefix(&c("* T :pr:"), 7, ORG), Some((5, "pr".into())));
        assert_eq!(org_tag_prefix(&c("#+FILETAGS: :a"), 14, ORG), Some((13, "a".into())));
        assert_eq!(org_tag_prefix(&c("#+filetags: "), 12, ORG), Some((12, String::new())));
        // `Note:` glued to a word, a caret before more words
        assert_eq!(org_tag_prefix(&c("* Note:"), 7, ORG), None);
        assert_eq!(org_tag_prefix(&c("texte :a"), 8, ORG), Some((7, "a".into())));
        assert_eq!(org_tag_prefix(&c("* T :a suite"), 6, ORG), None);
        assert_eq!(org_tag_prefix(&c("#+TITLE: x"), 10, ORG), None);
        // A dash ends the tag, unless allowed
        assert_eq!(org_tag_prefix(&c("voir :mon-t"), 11, ORG), None);
        let dashes = OrgTags { dashes: true, ..ORG };
        assert_eq!(org_tag_prefix(&c("voir :mon-t"), 11, dashes), Some((6, "mon-t".into())));
    }

    #[test]
    fn org_tag_link_found() {
        let t = c("texte :x:\n* T :a:Cible:\n");
        assert_eq!(find_link(&t, "cible", true, Hashtags::Off.into()), Some((17, 22)));
        assert_eq!(find_link(&t, "x", true, Hashtags::Off.into()), Some((7, 8)));
        let off = TagSyntax { hashtags: Hashtags::Off, org: OrgTags { links: false, dashes: false } };
        assert_eq!(find_link(&t, "cible", true, off), None);
    }

    #[test]
    fn hashtag_link_found() {
        let t = c("Voir #Cible ici [[Autre]]");
        assert_eq!(find_link(&t, "cible", true, Hashtags::Org.into()), Some((6, 11)));
        assert_eq!(find_link(&t, "cible", true, Hashtags::Off.into()), None);
    }

    #[test]
    fn sentences_forward_and_back() {
        let t = c("Un deux. Trois quatre! Cinq");
        assert_eq!(forward_sentence(&t, 0), 8);
        assert_eq!(forward_sentence(&t, 8), 22);
        assert_eq!(backward_sentence(&t, 15), 9);   // inside sentence 2 -> its start
        assert_eq!(backward_sentence(&t, 9), 0);    // at start of 2 -> start of 1
        assert_eq!(backward_sentence(&t, 3), 0);
    }

    #[test]
    fn sentences_stop_at_headlines() {
        let t = c("* Titre\nUn. Deux\ntrois\n** Sous\nQuatre");
        // From the text under `** Sous`: up to it, not to the top
        assert_eq!(backward_sentence(&t, t.len()), 31);
        assert_eq!(backward_sentence(&t, 31), 26);      // the headline's text
        assert_eq!(backward_sentence(&t, 26), 12);      // "Deux\ntrois"
        assert_eq!(backward_sentence(&t, 12), 8);
        assert_eq!(backward_sentence(&t, 8), 2);
        assert_eq!(forward_sentence(&t, 0), 7);
        assert_eq!(forward_sentence(&t, 7), 11);
        assert_eq!(forward_sentence(&t, 11), 22);       // stops before `** Sous`
        assert_eq!(forward_sentence(&t, 22), 30);
    }

    #[test]
    fn sentences_in_a_table_go_field_by_field() {
        let t = c("Avant. Fin\n| ab | c |\n|----+---|\n|    | d\napres.");
        // Fields: ab (13..15), c (18..19), empty (35..35), d (40..41)
        assert_eq!(forward_sentence_in_table(&t, 13), Some(15));
        assert_eq!(forward_sentence_in_table(&t, 15), Some(19));
        assert_eq!(forward_sentence_in_table(&t, 19), Some(35));
        assert_eq!(forward_sentence_in_table(&t, 35), Some(41));
        // Past the last field: the sentence after the table
        assert_eq!(forward_sentence_in_table(&t, 41), Some(48));
        assert_eq!(backward_sentence_in_table(&t, 41), Some(40));
        assert_eq!(backward_sentence_in_table(&t, 40), Some(35));
        assert_eq!(backward_sentence_in_table(&t, 35), Some(18));
        assert_eq!(backward_sentence_in_table(&t, 14), Some(13));
        // Before the first field: the sentence before the table
        assert_eq!(backward_sentence_in_table(&t, 13), Some(7));
        assert_eq!(forward_sentence_in_table(&t, 3), None);
    }

    #[test]
    fn org_elements() {
        let t = c("* A\ntexte un\nsuite\n\n- un\n  - sous\n- deux\n\n| t |\nfin\n** B\n* C\n");
        let at = |s: &str| t.iter().collect::<String>().find(s).unwrap();
        let (texte, un, sous, deux, table, fin, b, cc) =
            (at("texte"), at("- un"), at("- sous") - 2, at("- deux"), at("| t"), at("fin"), at("** B"), at("* C"));
        assert_eq!(forward_element(&t, texte), un);         // paragraph -> the list
        assert_eq!(forward_element(&t, un), deux);          // item -> next item
        assert_eq!(forward_element(&t, un + 2), sous);      // text of an item -> its sub-list
        assert_eq!(forward_element(&t, sous), deux);        // last sub-item -> past its list
        assert_eq!(forward_element(&t, deux), table);       // last item -> past the list
        assert_eq!(forward_element(&t, table), fin);
        assert_eq!(forward_element(&t, fin), b);
        assert_eq!(forward_element(&t, 0), cc);             // headline -> past its subtree
        assert_eq!(forward_element(&t, b), cc);
        assert_eq!(forward_element(&t, cc), cc);            // nothing after
        assert_eq!(backward_element(&t, b), 0);             // no sibling: the parent
        assert_eq!(backward_element(&t, cc), 0);            // the sibling
        assert_eq!(backward_element(&t, cc + 2), 0);
        assert_eq!(backward_element(&t, fin), table);
        assert_eq!(backward_element(&t, table), un);        // over the whole list
        assert_eq!(backward_element(&t, deux + 2), deux);   // text of an item -> its bullet
        assert_eq!(backward_element(&t, deux), un);         // item -> previous item
        assert_eq!(backward_element(&t, texte + 7), texte);
        assert_eq!(backward_element(&t, texte), 0);         // first element -> its headline
    }

    #[test]
    fn org_elements_blocks_and_drawers() {
        let t = c("* A\n\n:PROPERTIES:\n:ID: 1\n:END:\n#+begin_src\na\n\nb\n#+end_src\nx\n");
        let at = |s: &str| t.iter().collect::<String>().find(s).unwrap();
        assert_eq!(forward_element(&t, 4), at(":PROP"));    // blank under the headline
        assert_eq!(forward_element(&t, at(":PROP")), at("#+begin"));
        assert_eq!(forward_element(&t, at("#+begin")), at("x\n"));
        assert_eq!(backward_element(&t, at("x\n")), at("#+begin"));
        assert_eq!(backward_element(&t, at("b\n")), at("#+begin"));
    }

    #[test]
    fn heading_siblings_and_parent() {
        let t = c("intro\n* A\ntexte\n** A1\n** A2\n*** A21\n* B\n** B1\n");
        // intro 0, * A 6, texte 10, ** A1 16, ** A2 22, *** A21 28, * B 36, ** B1 40
        assert_eq!(backward_heading(&t, 12), 6);        // from the text: its headline
        assert_eq!(backward_heading(&t, 24), 16);       // inside the line: the sibling above
        assert_eq!(backward_heading(&t, 22), 16);
        assert_eq!(backward_heading(&t, 16), 16);       // first of its parent: stays
        assert_eq!(backward_heading(&t, 36), 6);        // over the sub-headlines of A
        assert_eq!(backward_heading(&t, 6), 6);
        assert_eq!(backward_heading(&t, 3), 0);         // before the first headline
        assert_eq!(forward_heading(&t, 6), 36);         // over the sub-headlines of A
        assert_eq!(forward_heading(&t, 16), 22);
        assert_eq!(forward_heading(&t, 24), 22);        // last of its parent: its start
        assert_eq!(forward_heading(&t, 12), 36);        // from the text: its headline's sibling
        assert_eq!(forward_heading(&t, 0), 6);          // before the first headline
        assert_eq!(forward_heading(&t, 36), 36);
        assert_eq!(up_heading(&t, 30), 22);
        assert_eq!(up_heading(&t, 22), 6);
        assert_eq!(up_heading(&t, 12), 12);             // top level: stays
        assert_eq!(up_heading(&t, 41), 36);
        assert_eq!(up_heading(&t, 3), 3);
    }

    #[test]
    fn sentence_stops_at_blank_line() {
        let t = c("Fin sans point\n\nSuite.");
        assert_eq!(forward_sentence(&t, 0), 14);
    }

    #[test]
    fn paragraphs() {
        let t = c("a\nb\n\nc\nd\n\ne");
        assert_eq!(forward_paragraph(&t, 0), 4);
        assert_eq!(forward_paragraph(&t, 4), 9);
        assert_eq!(forward_paragraph(&t, 10), t.len());
        assert_eq!(backward_paragraph(&t, 6), 4);
        assert_eq!(backward_paragraph(&t, 5), 4);
        assert_eq!(backward_paragraph(&t, 10), 9);
        assert_eq!(backward_paragraph(&t, 2), 0);
    }

    #[test]
    fn bare_stars_line_is_empty() {
        let t = c("a\n** \nb\n*\nc");
        assert_eq!(forward_paragraph(&t, 0), 5);
        assert_eq!(forward_paragraph(&t, 5), 9);
        assert_eq!(backward_paragraph(&t, 10), 9);
        assert_eq!(backward_paragraph(&t, 6), 5);
        let t = c("* Titre\ntexte\n\nsuite");
        assert_eq!(forward_paragraph(&t, 0), 14);
    }

    #[test]
    fn finds_links() {
        let t = c("voir [[Autre]] puis [[ Cible ][label]] fin");
        assert_eq!(find_link(&t, "Cible", false, Hashtags::Off.into()), Some((23, 28)));
        assert_eq!(find_link(&t, "cible", false, Hashtags::Off.into()), None);
        assert_eq!(find_link(&t, "cible", true, Hashtags::Off.into()), Some((23, 28)));
        assert_eq!(find_link(&t, "Absent", true, Hashtags::Off.into()), None);
    }

    #[test]
    fn indentation() {
        let t = c("x\n   texte");
        assert_eq!(back_to_indentation(&t, 8), 5);
    }

    #[test]
    fn headings() {
        let t = c("* A\ntexte\n** B\nplus\n* C");
        assert_eq!(next_heading(&t, 0), 10);
        assert_eq!(next_heading(&t, 5), 10);
        assert_eq!(next_heading(&t, 16), 20);
        assert_eq!(previous_heading(&t, 16), 10);
        assert_eq!(previous_heading(&t, 5), 0);   // body text -> its own heading
        assert_eq!(previous_heading(&t, 12), 0);  // on "** B" -> the heading before it
        assert_eq!(previous_heading(&t, 2), 2);   // first heading: nowhere to go
    }
}
