//! Org elements as Emacs reads them (M-{ / M-}: paragraphs, lists, tables,
//! drawers, blocks), the filling of a paragraph (M-q) and the bullet of a new item.

use super::*;

/// An org element of a section: paragraph, plain list, item, table, block,
/// drawer, keyword. `end` takes in the blank lines that follow it; `contents_end`
/// is where its last child ends (greater elements: lists, items).
struct Elem { begin: usize, end: usize, contents_end: usize, parent: Option<usize> }

/// Indentation of the line at `ls` (the view's indent counts too).
pub fn indent_of(chars: &[char], ls: usize) -> usize {
    chars[ls..].iter().take_while(|&&c| matches!(c, ' ' | '\t' | INDENT)).count()
}

fn is_blank(chars: &[char], ls: usize) -> bool {
    chars[ls..line_end(chars, ls)].iter().all(|c| c.is_whitespace())
}

/// Where the contents of the list item at `ls` begin (after its bullet and
/// checkbox), if the line is one: `-`, `+`, `*` (indented), `1.`, `1)`.
pub fn item_contents(chars: &[char], ls: usize) -> Option<usize> {
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

pub(super) fn is_table_line(chars: &[char], ls: usize) -> bool {
    chars.get(ls + indent_of(chars, ls)) == Some(&'|')
}

/// `:NAME:` alone on its line: a drawer opens.
pub fn is_drawer_start(chars: &[char], ls: usize) -> bool {
    let t: String = chars[ls + indent_of(chars, ls)..line_end(chars, ls)].iter().collect();
    let t = t.trim_end();
    t.len() > 2 && t.starts_with(':') && t.ends_with(':') && !t.eq_ignore_ascii_case(":end:")
        && t[1..t.len() - 1].chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

/// A line that starts an element of its own, ending the paragraph above.
fn starts_element(chars: &[char], ls: usize) -> bool {
    let t = ls + indent_of(chars, ls);
    headline_level(&chars[ls..]).is_some() || item_contents(chars, ls).is_some()
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
            if headline_level(&self.chars[ls..]).is_some() { break; }
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
            while f < limit && headline_level(&chars[f..]).is_none() {
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
            while e < limit && headline_level(&chars[e..]).is_none() {
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
                && item_contents(chars, e).is_some() && headline_level(&chars[e..]).is_none();
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
        if headline_level(&chars[ls..]).is_some() { ls = els.next_line(ls); continue; }
        let mut limit = ls;
        while limit < n && headline_level(&chars[limit..]).is_none() { limit = els.next_line(limit); }
        els.section(ls, limit, None, None, None);
        ls = limit;
    }
    els.elems
}

/// A line a paragraph can't be rewrapped with: not prose, though it is no element
/// of its own (a Markdown fence or quote, an org comment, a planning line…).
fn unfillable_line(line: &[char]) -> bool {
    let t: String = line.iter().collect();
    let t = t.trim_start();
    match format() {
        Format::Markdown => is_fence(t) || t.starts_with('>') || t.starts_with("---"),
        _ => t == "#" || t == ":" || t.starts_with("# ") || t.starts_with(": ")
            || ["SCHEDULED:", "DEADLINE:", "CLOSED:"].iter().any(|k| t.starts_with(k)),
    }
}

fn is_fence(line: &str) -> bool {
    line.starts_with("```") || line.starts_with("~~~")
}

/// The words of a paragraph. A link stays whole: no line break inside brackets
/// (the app reads a link on one line).
fn fill_words(text: &[char]) -> Vec<String> {
    let mut words = vec![String::new()];
    let mut depth = 0usize;
    for &c in text {
        match c {
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
            _ => {}
        }
        match (c.is_whitespace(), depth) {
            (true, 0) => words.push(String::new()),
            (true, _) => words.last_mut().unwrap().push(' '),
            (false, _) => words.last_mut().unwrap().push(c),
        }
    }
    words.retain(|w| !w.is_empty());
    words
}

/// What Emacs' `fill-paragraph` does: the prose paragraph holding `pos` (or the
/// text of its list item) rewrapped at `width` columns. Its range and new text;
/// `None` elsewhere (a table, a block, a headline…) or when it is already so.
pub fn fill_paragraph(chars: &[char], pos: usize, width: usize) -> Option<(usize, usize, String)> {
    let ls = line_start(chars, pos);
    if is_blank(chars, ls) { return None; }
    let at = item_contents(chars, ls).map_or(pos, |text| pos.max(text));
    let els = elements(chars);
    let e = &els[element_at(&els, at, chars.len())?];
    let first = line_start(chars, e.begin);
    let in_item = e.begin > first && item_contents(chars, first) == Some(e.begin);
    if !in_item && starts_element(chars, first) { return None; }
    // Blank lines after it belong to the element, not to the text
    let end = e.begin + chars[e.begin..e.end].iter().rposition(|c| !c.is_whitespace())? + 1;
    let lines = |a: usize, b: usize| chars[a..b].split(|&c| c == '\n');
    if lines(first, end).any(unfillable_line) { return None; }
    if format() == Format::Markdown {
        let fence = |l: &[char]| is_fence(l.iter().collect::<String>().trim_start());
        if lines(0, first).filter(|l| fence(l)).count() % 2 == 1 { return None; }
    }
    let (start, indent) = if in_item {
        (e.begin, " ".repeat(e.begin - first))
    } else {
        let start = first + indent_of(chars, first);
        (start, chars[first..start].iter().collect())
    };
    let mut out: String = chars[first..start].iter().collect();
    let mut column = start - first;
    for (i, word) in fill_words(&chars[start..end]).iter().enumerate() {
        let n = word.chars().count();
        if i > 0 {
            if column + 1 + n > width {
                out.push('\n');
                out.push_str(&indent);
                column = indent.chars().count();
            } else {
                out.push(' ');
                column += 1;
            }
        }
        out.push_str(word);
        column += n;
    }
    (out != chars[first..end].iter().collect::<String>()).then_some((first, end, out))
}

/// The bullet that starts a new item after the list item holding `pos` (org
/// `M-RET`): its indentation, the same bullet or the next number, and an empty
/// checkbox when it has one.
pub fn new_item_bullet(chars: &[char], pos: usize) -> Option<String> {
    let mut ls = line_start(chars, pos);
    let mut below = usize::MAX;
    loop {
        if is_blank(chars, ls) || headline_level(&chars[ls..]).is_some() { return None; }
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
    if let Some(level) = headline_level(&chars[ls..]) {
        let mut e = line_end(chars, ls);
        while e < n {
            let s = e + 1;
            if headline_level(&chars[s..]).is_some_and(|l| l <= level) { return s; }
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
    if headline_level(&chars[ls..]).is_some() {
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
    if headline_level(&chars[line_start(chars, j)..]).is_some() { return line_start(chars, j); }
    let Some(mut cur) = element_at(&els, j, n) else { return line_start(chars, j) };
    let mut dest = els[cur].begin;
    while let Some(p) = els[cur].parent {
        if els[p].end > beg { break; }
        dest = els[p].begin;
        cur = p;
    }
    dest
}
