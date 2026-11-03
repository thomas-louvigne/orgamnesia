//! The Markdown of a page made into HTML: its fenced code blocks, quotes,
//! headlines, rules and images, and its inline markup.

use super::{list_item, place, Doc};
use crate::{html::escape, images};
use orgamnesia_core::Format;

impl Doc<'_> {
    /// The Markdown block starting at the line `i`, as HTML, and the lines it takes.
    pub(super) fn markdown_block(&self, lines: &[&str], i: usize) -> Option<(String, usize)> {
        let line = lines[i];
        let t = line.trim();
        // A fenced code block
        if t.starts_with("```") || t.starts_with("~~~") {
            let fence = &t[..3];
            let end = lines[i + 1..].iter().position(|l| l.trim().starts_with(fence)).unwrap_or(lines.len() - i - 1);
            let html = format!("<pre>{}</pre>", escape(&lines[i + 1..i + 1 + end].join("\n")));
            return Some((html, end + 2));
        }
        // A quote: its lines without their `>`
        if t.starts_with('>') {
            let n = lines[i..].iter().take_while(|l| l.trim_start().starts_with('>')).count();
            let inner: Vec<&str> = lines[i..i + n].iter()
                .map(|l| { let l = l.trim_start().trim_start_matches('>'); l.strip_prefix(' ').unwrap_or(l) })
                .collect();
            return Some((format!("<blockquote>{}</blockquote>", self.body(&inner)), n));
        }
        if let Some(level) = Format::Markdown.heading_level_of(line) {
            return Some((self.headline(line, level), 1));
        }
        if is_rule_line(t) {
            return Some(("<hr>".to_string(), 1));
        }
        if let Some(target) = image_line(t) {
            return Some((self.image(&target, None), 1));
        }
        None
    }

    /// The inline Markdown markup opening at `i` (`code`, emphasis), as HTML,
    /// and where it ends.
    pub(super) fn markdown_inline(&self, chars: &[char], i: usize) -> Option<(String, usize)> {
        if chars[i] == '`' && let Some(end) = (i + 1..chars.len()).find(|&j| chars[j] == '`') {
            return Some((format!("<code>{}</code>", escape(&chars[i + 1..end].iter().collect::<String>())), end + 1));
        }
        if let Some((tag, n, end)) = emphasis(chars, i) {
            let inner: String = chars[i + n..end].iter().collect();
            return Some((format!("<{tag}>{}</{tag}>", self.inline(&inner)), end + n));
        }
        None
    }
}

/// A line that ends a paragraph of a Markdown page: a headline, table, quote,
/// code block, list item, image or rule.
pub(super) fn is_structure(line: &str) -> bool {
    let t = line.trim();
    Format::Markdown.heading_level_of(line).is_some() || t.starts_with('|') || t.starts_with('>') || t.starts_with("```")
        || t.starts_with("~~~") || list_item(line, Format::Markdown).is_some() || image_line(t).is_some() || is_rule_line(t)
}

/// A Markdown rule: `---`, `***`, `___` (spaces between allowed).
pub(crate) fn is_rule_line(t: &str) -> bool {
    let mut marks = t.chars().filter(|c| !c.is_whitespace());
    let Some(first @ ('-' | '*' | '_')) = marks.next() else { return false };
    let mut n = 1;
    for c in marks {
        if c != first { return false; }
        n += 1;
    }
    n >= 3
}

/// A line holding only a Markdown image: its target.
fn image_line(t: &str) -> Option<String> {
    let chars: Vec<char> = t.chars().collect();
    if !t.starts_with("![") { return None; }
    let (target, end) = place::markdown_target(&chars, 1)?;
    (end == chars.len() && (images::is_image(&target) || target.contains("://"))).then_some(target)
}

/// Markdown emphasis opening at `start`: the HTML tag, the length of its marks
/// and where its text ends (`**gras**`, `__gras__`, `*it*`, `_it_`, `~~barré~~`).
fn emphasis(chars: &[char], start: usize) -> Option<(&'static str, usize, usize)> {
    for (marks, tag) in [("**", "b"), ("__", "b"), ("~~", "s"), ("*", "i"), ("_", "i")] {
        let m: Vec<char> = marks.chars().collect();
        let n = m.len();
        if chars.get(start..start + n) != Some(&m[..]) { continue; }
        if chars.get(start + n).is_none_or(|c| c.is_whitespace()) { continue; }
        if m[0] == '_' && start > 0 && chars[start - 1].is_alphanumeric() { continue; }
        let end = (start + n + 1..=chars.len().saturating_sub(n)).find(|&j| {
            chars[j..j + n] == m[..] && !chars[j - 1].is_whitespace() && chars.get(j + n) != Some(&m[0])
                && !(m[0] == '_' && chars.get(j + n).is_some_and(|c| c.is_alphanumeric()))
        });
        if let Some(end) = end { return Some((tag, n, end)); }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules() {
        for t in ["---", "***", "_ _ _", "- - - -"] { assert!(is_rule_line(t), "{t}"); }
        for t in ["--", "-*-", "- -", "--- x", ""] { assert!(!is_rule_line(t), "{t}"); }
    }
}
