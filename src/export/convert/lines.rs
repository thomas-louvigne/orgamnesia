//! Lines of a page as the export reads them: paragraph ends, blocks, drawers,
//! list items, images, emphasis.

use super::*;

/// A line that ends a paragraph in a page of `format`: a headline, block,
/// keyword, table, list item, rule…
pub(super) fn is_structure(line: &str, format: Format) -> bool {
    if format == Format::Markdown { return markdown::is_structure(line); }
    let t = line.trim();
    format.heading_level_of(line).is_some() || t.starts_with("#+") || t.starts_with('|') || t.starts_with(": ")
        || t == ":" || list_item(line, format).is_some() || image_line(t).is_some() || is_drawer(t)
        || (t.len() >= 5 && t.chars().all(|c| c == '-'))
}

/// `#+BEGIN_SRC python` → `src`.
pub(super) fn block_kind(t: &str) -> Option<String> {
    let rest = t.get(..8).filter(|k| k.eq_ignore_ascii_case("#+begin_"))
        .map(|_| &t[8..])?;
    let kind = rest.split_whitespace().next()?.to_ascii_lowercase();
    (!kind.is_empty()).then_some(kind)
}

/// `:PROPERTIES:`, `:LOGBOOK:`…
pub(super) fn is_drawer(t: &str) -> bool {
    t.len() > 2 && t.starts_with(':') && t.ends_with(':') && !t.eq_ignore_ascii_case(":end:")
        && t[1..t.len() - 1].chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

/// The `:width` of an `#+ATTR_ORG:` line, in pixels when it has no unit.
pub(super) fn attr_width(t: &str) -> Option<String> {
    let mut words = t.split_whitespace();
    words.find(|w| *w == ":width")?;
    let w = words.next()?;
    Some(if w.chars().all(|c| c.is_ascii_digit()) { format!("{w}px") } else { w.to_string() })
}

/// A list item (`- a`, `+ a`, ` * a`, `1. a`, `1) a`): its indent, whether it is
/// numbered, and its text.
pub(super) fn list_item(line: &str, format: Format) -> Option<(usize, bool, &str)> {
    let md = format == Format::Markdown;
    let indent = line.len() - line.trim_start().len();
    let t = line.trim_start();
    for bullet in ["- ", "+ ", "* "] {
        // `* ` at the start of a line is an org-mode headline, a bullet in Markdown
        if (t.starts_with(bullet) || t == bullet.trim()) && (bullet != "* " || indent > 0 || md) {
            return Some((indent, false, t.get(2..).unwrap_or("")));
        }
    }
    let digits = t.chars().take_while(char::is_ascii_digit).count();
    let after = t.get(digits..)?;
    (digits > 0 && (after.starts_with(". ") || after.starts_with(") "))).then(|| (indent, true, &after[2..]))
}

/// How many of `lines` the list starting at the first takes: its items, the
/// lines indented under them, and the blank lines between.
pub(super) fn list_lines(lines: &[&str], format: Format) -> usize {
    let mut n = 0;
    for (i, line) in lines.iter().enumerate() {
        if line.trim().is_empty() {
            let next = lines[i + 1..].iter().find(|l| !l.trim().is_empty());
            if !next.is_some_and(|l| list_item(l, format).is_some() || l.starts_with([' ', '\t'])) { break; }
            continue;
        }
        let indented = line.starts_with([' ', '\t']) && format.heading_level_of(line).is_none();
        if list_item(line, format).is_none() && !indented { break; }
        n = i + 1;
    }
    n
}

/// A line holding only a link to an image, maybe with Logseq's size after it:
/// its target and width.
pub(super) fn image_line(t: &str) -> Option<(String, Option<String>)> {
    let chars: Vec<char> = t.chars().collect();
    let link = place::link_at(&chars, 0).filter(|l| l.description.is_none())?;
    let target = link.target(&chars);
    let rest = chars[link.end..].iter().collect::<String>();
    let rest = rest.trim();
    if !images::is_image(&target) { return None; }
    if rest.is_empty() { return Some((target, None)); }
    // `{:height 200, :width 300}`
    let size = rest.strip_prefix('{')?.strip_suffix('}')?;
    let width = size.split(',').find_map(|p| p.trim().strip_prefix(":width").map(|w| format!("{}px", w.trim())));
    Some((target, width))
}

/// Where the emphasis opened by the marker at `start` closes, as org-mode reads it:
/// the marker after a space or punctuation, the text not starting nor ending with
/// a space, the closing marker followed by a space or punctuation.
pub(super) fn emphasis_end(chars: &[char], start: usize) -> Option<usize> {
    let m = chars[start];
    let before_ok = start == 0 || chars[start - 1].is_whitespace() || "-({'\"".contains(chars[start - 1]);
    if !before_ok || chars.get(start + 1).is_none_or(|c| c.is_whitespace() || *c == m) { return None; }
    (start + 2..chars.len()).find(|&j| {
        chars[j] == m && !chars[j - 1].is_whitespace()
            && chars.get(j + 1).is_none_or(|c| c.is_whitespace() || "-.,:!?;'\")}[".contains(*c))
    })
}

pub(super) fn starts_with(chars: &[char], s: &str) -> bool {
    s.chars().enumerate().all(|(i, c)| chars.get(i) == Some(&c))
}
