//! Footnotes: a reference in the text (`[fn:1]` in org-mode, `[^1]` in
//! Markdown), its definition on a line of its own (`[fn:1] Texte`, `[^1]: Texte`).
//! Org-mode's inline ones (`[fn:: Texte]`, `[fn:nom: Texte]`) are read too.

use crate::Format;

/// A footnote marker: chars `start..end`, its label (empty for an inline note
/// without one), and whether it starts the definition of its note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub start: usize,
    pub end: usize,
    pub label: String,
    pub definition: bool,
}

fn is_label(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '-' | '_')
}

/// The footnote marker starting at `i`, if any: `[fn:label]`, `[fn:label: …]`,
/// `[fn:: …]` (org-mode, the inline ones whole), `[^label]` (Markdown, with the
/// `:` after it when it starts a definition).
pub fn at(chars: &[char], i: usize, format: Format) -> Option<Note> {
    let open: &[char] = if format == Format::Org { &['[', 'f', 'n', ':'] } else { &['[', '^'] };
    if !chars[i..].starts_with(open) { return None; }
    let a = i + open.len();
    let n = chars[a..].iter().take_while(|&&c| is_label(c)).count();
    let label: String = chars[a..a + n].iter().collect();
    // A definition starts its line (spaces before it allowed: the view may indent it)
    let line_start = chars[..i].iter().rev().take_while(|&&c| c != '\n').all(|c| *c == ' ' || *c == '\t');
    match (format, chars.get(a + n)) {
        (_, Some(']')) if n > 0 => {
            let mut end = a + n + 1;
            let definition = line_start && match format {
                Format::Org => true,
                Format::Markdown => chars.get(end) == Some(&':') && { end += 1; true },
            };
            Some(Note { start: i, end, label, definition })
        }
        // Inline (org-mode): to its closing bracket, those of links inside skipped
        (Format::Org, Some(':')) => {
            let mut depth = 1;
            let mut j = a + n + 1;
            while j < chars.len() && chars[j] != '\n' {
                match chars[j] {
                    '[' => depth += 1,
                    ']' => { depth -= 1; if depth == 0 { return Some(Note { start: i, end: j + 1, label, definition: false }); } }
                    _ => {}
                }
                j += 1;
            }
            None
        }
        _ => None,
    }
}

/// The footnote marker around the char `pos` (or just before it), if any.
pub fn under(chars: &[char], pos: usize, format: Format) -> Option<Note> {
    let line = chars[..pos.min(chars.len())].iter().rposition(|&c| c == '\n').map_or(0, |i| i + 1);
    (line..=pos.min(chars.len().saturating_sub(1))).rev()
        .filter(|&i| chars.get(i) == Some(&'['))
        .find_map(|i| at(chars, i, format).filter(|n| pos <= n.end))
}

/// Every footnote marker of `chars`.
pub fn all(chars: &[char], format: Format) -> Vec<Note> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        match (chars[i] == '[').then(|| at(chars, i, format)).flatten() {
            Some(n) => { i = n.end; out.push(n); }
            None => i += 1,
        }
    }
    out
}

/// Where following the footnote at `pos` leads: from a reference, the text of
/// its definition; from a definition (anywhere on its first line), its first
/// reference. `None` when the cursor is on no footnote, or the other end is missing.
pub fn other_end(chars: &[char], pos: usize, format: Format) -> Option<usize> {
    let pos = pos.min(chars.len());
    let line = chars[..pos].iter().rposition(|&c| c == '\n').map_or(0, |i| i + 1);
    let indent = chars[line..].iter().take_while(|c| **c == ' ' || **c == '\t').count();
    let here = under(chars, pos, format)
        .or_else(|| (chars.get(line + indent) == Some(&'[')).then(|| at(chars, line + indent, format)).flatten().filter(|n| n.definition))
        .filter(|n| !n.label.is_empty())?;
    let notes = all(chars, format);
    let there = notes.iter().find(|n| n.label == here.label && n.definition != here.definition)?;
    Some(if there.definition {
        // After the marker and the space following it
        there.end + usize::from(chars.get(there.end) == Some(&' '))
    } else {
        there.start
    })
}

/// A new footnote at `pos`: its reference there, numbered after the highest
/// number used, its definition at the end of the page (in org-mode, under a
/// `Footnotes` headline, added when there is none). The chars `a..b` become
/// `with`; the cursor goes to `cursor`, on the definition's text.
pub fn insert(chars: &[char], pos: usize, format: Format) -> (usize, usize, String, usize) {
    let next = all(chars, format).iter().filter_map(|n| n.label.parse::<u32>().ok()).max().unwrap_or(0) + 1;
    let (reference, definition) = match format {
        Format::Org => (format!("[fn:{next}]"), format!("[fn:{next}] ")),
        Format::Markdown => (format!("[^{next}]"), format!("[^{next}]: ")),
    };
    let rest: String = chars[pos..].iter().collect();
    let text: String = chars.iter().collect();
    let mut with = format!("{reference}{rest}");
    if !with.ends_with('\n') { with.push('\n'); }
    let has_section = format == Format::Org && text.lines().any(|l| {
        l.starts_with('*') && l.trim_start_matches('*').starts_with(' ') && l.trim_start_matches('*').trim() == "Footnotes"
    });
    if format == Format::Org && !has_section {
        with.push_str("\n* Footnotes\n");
    } else if format == Format::Markdown && !text.trim_end().lines().last().is_some_and(|l| at(&l.chars().collect::<Vec<_>>(), 0, format).is_some_and(|n| n.definition)) {
        // A blank line before the first definition, none between the others
        with.push('\n');
    }
    with.push_str(&definition);
    let cursor = pos + with.chars().count();
    (pos, chars.len(), with, cursor)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(s: &str) -> Vec<char> { s.chars().collect() }

    #[test]
    fn reads_references_and_definitions() {
        let t = c("Voir[fn:1] et [fn:: en ligne [[A]]] ou [fn:x: idem].\n[fn:1] La note.");
        let notes = all(&t, Format::Org);
        assert_eq!(notes.iter().map(|n| (n.label.as_str(), n.definition)).collect::<Vec<_>>(), [("1", false), ("", false), ("x", false), ("1", true)]);
        assert_eq!(notes[1].end - notes[1].start, "[fn:: en ligne [[A]]]".len());
        let md = c("Voir[^a].\n\n[^a]: La note.");
        assert_eq!(all(&md, Format::Markdown).iter().map(|n| n.definition).collect::<Vec<_>>(), [false, true]);
        assert!(all(&c("[fn:1]"), Format::Markdown).is_empty());
    }

    #[test]
    fn goes_from_one_end_to_the_other() {
        let t = c("Voir[fn:1].\n[fn:1] La note.");
        assert_eq!(other_end(&t, 6, Format::Org), Some(19));
        assert_eq!(other_end(&t, 13, Format::Org), Some(4));
        assert_eq!(other_end(&t, 22, Format::Org), Some(4));
        assert_eq!(other_end(&t, 1, Format::Org), None);
        let md = c("Voir[^1].\n\n[^1]: La note.");
        assert_eq!(other_end(&md, 5, Format::Markdown), Some(17));
    }

    #[test]
    fn inserts_a_new_note() {
        let apply = |t: &str, pos: usize, f: Format| {
            let chars = c(t);
            let (a, b, with, cursor) = insert(&chars, pos, f);
            let out: String = chars[..a].iter().collect::<String>() + &with + &chars[b..].iter().collect::<String>();
            (out, cursor)
        };
        let (out, cursor) = apply("* Titre\nTexte.", 13, Format::Org);
        assert_eq!(out, "* Titre\nTexte[fn:1].\n\n* Footnotes\n[fn:1] ");
        assert_eq!(cursor, out.chars().count());
        let (out, _) = apply("A[fn:1].\n* Footnotes\n[fn:1] x\n", 0, Format::Org);
        assert_eq!(out, "[fn:2]A[fn:1].\n* Footnotes\n[fn:1] x\n[fn:2] ");
        let (out, _) = apply("Texte.", 5, Format::Markdown);
        assert_eq!(out, "Texte[^1].\n\n[^1]: ");
        let (out, _) = apply(&out, 0, Format::Markdown);
        assert_eq!(out, "[^2]Texte[^1].\n\n[^1]: \n[^2]: ");
    }
}
