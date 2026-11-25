//! A line of a page cut into the pieces its highlighting shows: headlines (their
//! stars, TODO keyword, tags), links (their brackets, target and text), emphasis,
//! tags, tables, Markdown code, quotes and rules. Each piece has the class the
//! window's stylesheet draws (`h1`, `link-t`, `em-b`, `mk`…); the window writes
//! them as HTML, the terminal as colours: the same reading of the text in both.

use crate::links::{Found, Kind, LinkSyntax};
use crate::{Format, TagSyntax, TodoKeywords};

/// Whether a link target leads nowhere.
pub type Broken<'a> = Box<dyn Fn(&str) -> bool + 'a>;

/// What drawing a page depends on.
pub struct Draw<'a> {
    pub format: Format,
    /// How `#tags`, `:tags:` and links are read, for that format.
    pub tags: TagSyntax,
    /// The TODO keywords drawn on headlines (`None` when they are off).
    pub todo: Option<TodoKeywords>,
    /// Whether a link target leads nowhere, when such links are marked.
    pub broken: Option<Broken<'a>>,
}

impl Draw<'_> {
    pub fn new(format: Format, tags: TagSyntax) -> Self {
        Self { format, tags, todo: None, broken: None }
    }

    /// The links not of org-mode read in the page.
    fn links(&self) -> LinkSyntax {
        self.tags.links(self.format)
    }

    /// Class of a link to `target`.
    fn link_class(&self, target: &str) -> &'static str {
        if self.broken.as_ref().is_some_and(|f| f(target)) { "link broken" } else { "link" }
    }
}

/// A piece of a line: text, or a span of a class holding pieces (`data`: the
/// target a link leads to when it is not the one written).
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Text(String),
    Span { class: String, data: Option<String>, children: Vec<Node> },
}

fn text(s: impl Into<String>) -> Node {
    Node::Text(s.into())
}

fn span(class: impl Into<String>, children: Vec<Node>) -> Node {
    Node::Span { class: class.into(), data: None, children }
}

fn span_text(class: &str, s: impl Into<String>) -> Node {
    span(class, vec![text(s)])
}

/// The line `line` of a page (no table drawn whole, no code block: see
/// `code_lines`, `table_heads`) cut into its pieces.
pub fn line(line: &str, d: &Draw) -> Vec<Node> {
    let chars: Vec<char> = line.chars().collect();
    let part = |a: usize, b: usize| chars[a..b].iter().collect::<String>();

    // Headline: its stars (`#` in Markdown)
    let format = d.format;
    let mark = format.heading_mark();
    if let Some(stars) = format.heading_level(&chars) {
        let level = stars.min(6);
        // `TODO`, `DONE`…: drawn on their own, the title after them
        let keyword = d.todo.as_ref().and_then(|k| k.keyword(&chars, format));
        let (keyword_node, title_start) = match keyword {
            Some(k) => (Some(span_text(if k.done { "done" } else { "todo" }, part(k.start, k.end))), k.end),
            None => (None, (stars + 1).min(chars.len())),
        };
        let (title, tag_nodes) = match crate::hashtags::headline_tags_range(&chars, d.tags.org) {
            Some((a, b)) if a >= title_start => (part(title_start, a), vec![span("tags", tag_links(&part(a, b), d)), text(part(b, chars.len()))]),
            _ => (part(title_start, chars.len()), vec![]),
        };
        // The stars before the last, hidden when the setting asks it
        let mut star_nodes = vec![];
        if stars > 1 { star_nodes.push(span_text("h-lead", mark.to_string().repeat(stars - 1))); }
        star_nodes.push(text(mark.to_string()));
        let mut children = vec![span("h-stars", star_nodes)];
        if stars < chars.len() { children.push(text(" ")); }
        children.extend(keyword_node);
        children.extend(inline(&title, d));
        children.extend(tag_nodes);
        let done = if keyword.is_some_and(|k| k.done) { " h-done" } else { "" };
        return vec![span(format!("h{level}{done}"), children)];
    }

    if format == Format::Markdown { return markdown_line(line, d); }

    // `#+FILETAGS: :a:b:`
    if let Some(v) = crate::hashtags::filetags_value_start(&chars) {
        return vec![span_text("kw", part(0, v)), span("tags", tag_links(&part(v, chars.len()), d))];
    }

    // `#+TITLE: …`, `#+DATE: …` (any keyword): the keyword dimmed, the value as text
    let indent = chars.iter().take_while(|c| c.is_whitespace()).count();
    let name = chars[indent..].strip_prefix(&['#', '+'])
        .map(|rest| rest.iter().take_while(|c| c.is_alphanumeric() || matches!(c, '_' | '-')).count());
    if let Some(n) = name.filter(|&n| n > 0 && chars.get(indent + 2 + n) == Some(&':')) {
        let end = indent + 3 + n;
        let mut out = vec![span_text("kw", part(0, end))];
        out.extend(inline(&part(end, chars.len()), d));
        return out;
    }

    // Table line
    if line.trim_start().starts_with('|') {
        return vec![span_text("tbl", line)];
    }

    inline(line, d)
}

/// Org-mode tags (`:a:b:`), each a link to its page (when they are links); the
/// separators as they are.
fn tag_links(s: &str, d: &Draw) -> Vec<Node> {
    let org = d.tags.org;
    let mut out = vec![];
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut Vec<Node>| {
        if word.is_empty() { return; }
        out.push(if org.links { span_text("tag-t", word.as_str()) } else { text(word.as_str()) });
        word.clear();
    };
    for c in s.chars() {
        if org.is_tag_char(c) { word.push(c); } else { flush(&mut word, &mut out); out.push(text(c.to_string())); }
    }
    flush(&mut word, &mut out);
    merge(out)
}

/// The inline markup of a piece of text (links, emphasis, tags…), as in a paragraph.
pub fn inline(s: &str, d: &Draw) -> Vec<Node> {
    let chars: Vec<char> = s.chars().collect();
    let mut out = vec![];
    let mut i = 0;
    let md = d.format == Format::Markdown;
    while i < chars.len() {
        // A timestamp `<2026-11-24 Tue 14:05 +1w>` (`[…]` inactive), in both formats: the agenda reads both
        if matches!(chars[i], '<' | '[') && let Some((a, b, _)) = crate::edit::stamp_around(&chars, i) && a == i {
            out.push(stamp(&chars[a..b]));
            i = b;
            continue;
        }
        // `SCHEDULED:`, `DEADLINE:`, `CLOSED:` dimmed as the `#+` keywords
        if (i == 0 || chars[i - 1].is_whitespace())
            && let Some(k) = crate::agenda::PLANNING.iter().find(|k| chars[i..].starts_with(&k.chars().collect::<Vec<_>>())) {
            out.push(span_text("kw", *k));
            i += k.chars().count();
            continue;
        }
        // A footnote `[fn:1]`, `[^1]` (`fn-def` starting its definition)
        if chars[i] == '[' && let Some(n) = crate::footnotes::at(&chars, i, d.format) {
            out.push(span_text(if n.definition { "fn fn-def" } else { "fn" }, chars[i..n.end].iter().collect::<String>()));
            i = n.end;
            continue;
        }
        // A link `[[…]]` (and the extended ones: `[[a|b]]`, `[texte](Page.md)`)
        if chars[i] == '[' && let Some(f) = crate::links::link_starting(&chars, i, d.links()) {
            out.push(link(&chars, &f, d));
            i = f.end;
            continue;
        }
        // Markdown: `code`, **bold**, __bold__, *italic*, _italic_, ~~struck~~
        if md {
            if let Some((node, len)) = inline_markup(&chars, i, d) {
                out.push(node);
                i += len;
                continue;
            }
            out.push(text(chars[i].to_string()));
            i += 1;
            continue;
        }
        // #hashtag
        if chars[i] == '#' && let Some((tag, end)) = crate::hashtags::hashtag_at(&chars, i, d.tags.hashtags) {
            out.push(span("link", vec![text("#"), span_text("link-t", tag)]));
            i = end;
            continue;
        }
        // :tag: in the text
        if chars[i] == ':' && d.tags.org.links && let Some((_, end)) = crate::hashtags::text_tags_at(&chars, i, d.tags.org) {
            out.push(span("tags", tag_links(&chars[i..end].iter().collect::<String>(), d)));
            i = end;
            continue;
        }
        let emphasis = match chars[i] {
            '*' => Some("em-b"),
            '/' => Some("em-i"),
            // Not inside a word: `1+2+3`
            '+' if i == 0 || !chars[i - 1].is_alphanumeric() => Some("em-s"),
            '_' => Some("em-u"),
            _ => None,
        };
        if let Some(class) = emphasis && let Some((node, len)) = em(&chars, i, chars[i], class, d) {
            out.push(node);
            i += len;
            continue;
        }
        out.push(text(chars[i].to_string()));
        i += 1;
    }
    merge(out)
}

/// A timestamp in pieces: its date, day name and time each in their class, the
/// brackets as markup, the rest (repeater, warning) dimmed.
fn stamp(chars: &[char]) -> Node {
    let inner: String = chars[1..chars.len() - 1].iter().collect();
    let mut children = vec![span_text("mk", chars[0].to_string())];
    let mut word = String::new();
    let mut first = true;
    let mut push = |w: &mut String, children: &mut Vec<Node>| {
        if w.is_empty() { return; }
        let class = if std::mem::take(&mut first) { "ts-date" }
            else if w.starts_with(|c: char| c.is_ascii_digit()) && w.contains(':') { "ts-time" }
            else if w.chars().all(|c| c.is_alphabetic() || c == '.') { "ts-day" }
            else { "ts-rest" };
        children.push(span_text(class, std::mem::take(w)));
    };
    for c in inner.chars() {
        if c.is_whitespace() { push(&mut word, &mut children); children.push(text(c.to_string())); } else { word.push(c); }
    }
    push(&mut word, &mut children);
    children.push(span_text("mk", chars[chars.len() - 1].to_string()));
    span(if chars[0] == '<' { "ts" } else { "ts ts-off" }, merge(children))
}

/// The text pieces next to each other made one.
fn merge(nodes: Vec<Node>) -> Vec<Node> {
    let mut out: Vec<Node> = Vec::with_capacity(nodes.len());
    for n in nodes {
        match (out.last_mut(), n) {
            (Some(Node::Text(a)), Node::Text(b)) => a.push_str(&b),
            (_, n) => out.push(n),
        }
    }
    out
}

/// The length of the link starting at `start`, if one does (to skip it).
fn link_len(chars: &[char], start: usize, d: &Draw) -> Option<usize> {
    crate::links::link_starting(chars, start, d.links()).map(|f| f.end - start)
}

/// The link found: its target (`link-t`) and text (`link-d`), the markup around
/// them (`mk`); the target it leads to in `data` when it is not the one written
/// (`[[Page|texte]]`, `[texte](Ma%20page.md)`).
fn link(chars: &[char], f: &Found, d: &Draw) -> Node {
    let part = |a: usize, b: usize| chars[a..b].iter().collect::<String>();
    let (written, children) = match f.kind {
        Kind::Org => {
            let inner_end = f.end - 2;
            let split = (f.start + 2..inner_end).find(|&j| chars[j] == ']' && chars[j + 1] == '[')
                .map(|j| (j, 2))
                .or_else(|| d.links().pipe.then(|| (f.start + 2..inner_end).find(|&j| chars[j] == '|').map(|j| (j, 1))).flatten());
            match split {
                Some((j, n)) => (part(f.start + 2, j), vec![
                    span("mk", vec![text("[["), span_text("link-t", part(f.start + 2, j)), text(part(j, j + n))]),
                    span_text("link-d", part(j + n, inner_end)),
                    span_text("mk", "]]"),
                ]),
                None => (part(f.start + 2, inner_end), vec![
                    span_text("mk", "[["), span_text("link-t", part(f.start + 2, inner_end)), span_text("mk", "]]"),
                ]),
            }
        }
        Kind::Markdown => {
            let close = (f.start + 1..f.end).find(|&j| chars[j] == ']' && chars.get(j + 1) == Some(&'(')).unwrap_or(f.start);
            (String::new(), vec![
                span_text("mk", "["),
                span_text("link-d", part(f.start + 1, close)),
                span("mk", vec![text("]("), span_text("link-t", part(close + 2, f.end - 1)), text(")")]),
            ])
        }
    };
    let data = (written.trim() != f.target).then(|| f.target.clone());
    Node::Span { class: d.link_class(&f.target).to_string(), data, children }
}

/// Org-mode emphasis opening with `delim` at `start` (`*gras*`): its node and
/// the chars it takes.
fn em(chars: &[char], start: usize, delim: char, class: &str, d: &Draw) -> Option<(Node, usize)> {
    if start + 2 >= chars.len() { return None; }
    let next = chars[start + 1];
    if next.is_whitespace() || next == delim { return None; }
    let mut i = start + 1;
    while i < chars.len() {
        // A link inside is drawn as a link: its target may hold the delimiter (`/[[a/b]]/`)
        if chars[i] == '[' && let Some(len) = link_len(chars, i, d) {
            i += len;
            continue;
        }
        if chars[i] == delim && !chars[i - 1].is_whitespace() && i > start + 1 {
            let inner: String = chars[start + 1..i].iter().collect();
            let mut children = vec![span_text("mk", delim.to_string())];
            children.extend(inline(&inner, d));
            children.push(span_text("mk", delim.to_string()));
            return Some((span(class, children), i + 1 - start));
        }
        i += 1;
    }
    None
}

/// Whether a Markdown line is a rule: three or more `-`, `*` or `_` (spaces between allowed).
pub fn is_rule_line(t: &str) -> bool {
    let mut marks = t.chars().filter(|c| !c.is_whitespace());
    let Some(first @ ('-' | '*' | '_')) = marks.next() else { return false };
    let mut n = 1;
    for c in marks {
        if c != first { return false; }
        n += 1;
    }
    n >= 3
}

/// The lines of a Markdown page drawn as they are, with their class: those of
/// its fenced code blocks (```` ``` ```` or `~~~` to the same, `md-code`), and
/// of its front matter (`---` on the first line, to the next `---`, `kw`).
pub fn code_lines(lines: &[&str], text: &str) -> std::collections::HashMap<usize, &'static str> {
    let mut out = std::collections::HashMap::new();
    let mut n = crate::frontmatter::lines(text).map_or(0, |(_, end)| end);
    out.extend((0..n).map(|i| (i, "kw")));
    let mut fence: Option<&str> = None;
    while n < lines.len() {
        let t = lines[n].trim_start();
        match fence {
            None if t.starts_with("```") || t.starts_with("~~~") => {
                fence = Some(&t[..3]);
                out.insert(n, "md-code");
            }
            Some(f) => {
                out.insert(n, "md-code");
                if t.trim_end() == f || (t.starts_with(f) && t[3..].trim_start_matches(f.chars().next().unwrap_or('`')).trim().is_empty()) {
                    fence = None;
                }
            }
            None => {}
        }
        n += 1;
    }
    out
}

/// Lines that are table headers: in a table, the rows above its first rule
/// (`|---+---|`), as org-mode draws them (class `tbl tbl-head`).
pub fn table_heads(lines: &[&str]) -> std::collections::HashSet<usize> {
    let is_table = |l: &str| l.trim_start().starts_with('|');
    let mut heads = std::collections::HashSet::new();
    let mut n = 0;
    while n < lines.len() {
        if !is_table(lines[n]) { n += 1; continue; }
        let start = n;
        while n < lines.len() && is_table(lines[n]) { n += 1; }
        if let Some(rule) = (start..n).find(|&i| crate::table::is_rule(lines[i])) {
            heads.extend(start..rule);
        }
    }
    heads
}

/// A line of a Markdown page that is no headline: a rule (`---`), a quote
/// (`> `), a table line, a checkbox, or a paragraph line.
fn markdown_line(line: &str, d: &Draw) -> Vec<Node> {
    let t = line.trim();
    if is_rule_line(t) {
        return vec![span_text("kw", line)];
    }
    if line.trim_start().starts_with('>') {
        return vec![span("md-quote", inline(line, d))];
    }
    if line.trim_start().starts_with('|') {
        return vec![span_text("tbl", line)];
    }
    // A checkbox, drawn as a TODO keyword: `[ ]` to do, `[x]` done
    let chars: Vec<char> = line.chars().collect();
    if let Some((at, done)) = crate::todo::checkbox(&chars) {
        let part = |a: usize, b: usize| chars[a..b].iter().collect::<String>();
        let mut out = vec![text(part(0, at - 1)), span_text(if done { "done" } else { "todo" }, part(at - 1, at + 2))];
        out.extend(inline(&part(at + 2, chars.len()), d));
        return merge(out);
    }
    inline(line, d)
}

/// The inline Markdown markup opening at `i`, and the chars it takes.
fn inline_markup(chars: &[char], i: usize, d: &Draw) -> Option<(Node, usize)> {
    if chars[i] == '`' && let Some(len) = closing(chars, i, &['`'], true, d) {
        return Some((span_text("md-code", chars[i..i + len].iter().collect::<String>()), len));
    }
    const DELIMS: [(&[char], &str); 5] =
        [(&['*', '*'], "em-b"), (&['_', '_'], "em-b"), (&['~', '~'], "em-s"), (&['*'], "em-i"), (&['_'], "em-i")];
    let found = matches!(chars[i], '*' | '_' | '~').then(|| DELIMS.into_iter()
        .filter(|(m, _)| chars[i..].starts_with(m))
        .find_map(|(m, class)| closing(chars, i, m, false, d).map(|len| (m, class, len)))).flatten();
    let (m, class, len) = found?;
    let n = m.len();
    let inner: String = chars[i + n..i + len - n].iter().collect();
    let m: String = m.iter().collect();
    let mut children = vec![span_text("mk", m.as_str())];
    children.extend(inline(&inner, d));
    children.push(span_text("mk", m));
    Some((span(class, children), len))
}

/// The Markdown markup `delim` opening at `start` (not followed by a space, not
/// inside a word for `_`): the length up to its closing `delim` (not after a
/// space) included. `verbatim`: nothing inside is markup.
fn closing(chars: &[char], start: usize, delim: &[char], verbatim: bool, d: &Draw) -> Option<usize> {
    let n = delim.len();
    let open = start + n;
    if open >= chars.len() || chars[open].is_whitespace() { return None; }
    if delim[0] == '_' && start > 0 && chars[start - 1].is_alphanumeric() { return None; }
    let mut i = open;
    while i + n <= chars.len() {
        if !verbatim && chars[i] == '[' && let Some(len) = link_len(chars, i, d) {
            i += len;
            continue;
        }
        if chars[i..i + n] == delim[..] && i > open && (verbatim || !chars[i - 1].is_whitespace())
            // `**` does not close `*`
            && (verbatim || chars.get(i + n) != Some(&delim[0]))
        {
            let after = chars.get(i + n);
            if delim[0] == '_' && after.is_some_and(|c| c.is_alphanumeric()) { i += 1; continue; }
            return Some(i + n - start);
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests;
