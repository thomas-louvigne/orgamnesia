//! The Markdown of a page drawn: its code blocks and front matter, its quotes,
//! rules and checkboxes, its inline markup (`code`, **bold**, *italic*, ~~struck~~).

use super::{inline_html, link_len, Draw};
use crate::html::escape;

/// The lines of a Markdown page drawn as they are, with their class: those of
/// its fenced code blocks (```` ``` ```` or `~~~` to the same), and of its front
/// matter (`---` on the first line, to the next `---`).
pub(super) fn markdown_code_lines(lines: &[&str], text: &str) -> std::collections::HashMap<usize, &'static str> {
    let mut out = std::collections::HashMap::new();
    let mut n = orgamnesia_core::frontmatter::lines(text).map_or(0, |(_, end)| end);
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

/// A line of a Markdown page that is no headline: a quote (`> `), a rule
/// (`---`), a table line, or a paragraph line.
pub(super) fn markdown_line(line: &str, d: &Draw) -> String {
    let t = line.trim();
    if crate::export::is_rule_line(t) {
        return format!("<span class='kw'>{}</span>", escape(line));
    }
    if line.trim_start().starts_with('>') {
        return format!("<span class='md-quote'>{}</span>", inline_html(line, d));
    }
    if line.trim_start().starts_with('|') {
        return format!("<span class='tbl'>{}</span>", escape(line));
    }
    // A checkbox, drawn as a TODO keyword: `[ ]` to do, `[x]` done
    let chars: Vec<char> = line.chars().collect();
    if let Some((at, done)) = orgamnesia_core::todo::checkbox(&chars) {
        let part = |a: usize, b: usize| chars[a..b].iter().collect::<String>();
        return format!("{}<span class='{}'>{}</span>{}", escape(&part(0, at - 1)), if done { "done" } else { "todo" },
            escape(&part(at - 1, at + 2)), inline_html(&part(at + 2, chars.len()), d));
    }
    inline_html(line, d)
}

/// The inline Markdown markup opening at `i`, drawn, and the chars it takes.
pub(super) fn inline_markup(chars: &[char], i: usize, d: &Draw) -> Option<(String, usize)> {
    if chars[i] == '`' && let Some(len) = closing(chars, i, &['`'], true, d) {
        let inner: String = chars[i..i + len].iter().collect();
        let html = format!("<span class='md-code'>{}</span>", escape(&inner));
        return Some((html, len));
    }
    const DELIMS: [(&[char], &str); 5] =
        [(&['*', '*'], "em-b"), (&['_', '_'], "em-b"), (&['~', '~'], "em-s"), (&['*'], "em-i"), (&['_'], "em-i")];
    let found = matches!(chars[i], '*' | '_' | '~').then(|| DELIMS.into_iter()
        .filter(|(m, _)| chars[i..].starts_with(m))
        .find_map(|(m, class)| closing(chars, i, m, false, d).map(|len| (m, class, len)))).flatten();
    if let Some((m, class, len)) = found {
        let n = m.len();
        let inner: String = chars[i + n..i + len - n].iter().collect();
        let m: String = m.iter().collect();
        let html = format!("<span class='{class}'>{m}{}{m}</span>", inline_html(&inner, d));
        return Some((html, len));
    }
    None
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
