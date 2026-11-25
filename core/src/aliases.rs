//! The other names of a page: a link naming one of them leads to the page. Read
//! from `#+ALIAS: a, b` (org-mode, one or more lines), Logseq's `alias:: a, b`
//! (both formats), and the `aliases:` of the front matter (Markdown, as Obsidian
//! writes them).

use crate::Format;

/// The aliases written in the page `text`, in the order found, without doubles.
pub fn aliases(text: &str, format: Format) -> Vec<String> {
    let mut out = match format {
        Format::Markdown => crate::frontmatter::aliases(text),
        Format::Org => Vec::new(),
    };
    for line in text.lines() {
        let Some(v) = value(line, format) else { continue };
        for a in v.split(',') {
            let a = a.trim().trim_matches(|c| c == '"' || c == '[' || c == ']').trim();
            if !a.is_empty() && !out.iter().any(|o| o == a) { out.push(a.to_string()); }
        }
    }
    out
}

/// The text after `#+ALIAS:` (org-mode, any case) or `alias::` / `aliases::`
/// (any format) when `line` is one of them.
fn value(line: &str, format: Format) -> Option<&str> {
    let line = line.trim_start();
    let after = |key: &str| line.get(..key.len()).filter(|k| k.eq_ignore_ascii_case(key)).map(|_| &line[key.len()..]);
    (format == Format::Org).then(|| after("#+alias:")).flatten()
        .or_else(|| after("aliases::"))
        .or_else(|| after("alias::"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_every_form() {
        let org = "#+TITLE: Dragon\n#+ALIAS: Le dragon, Drake\n#+alias: Wyrm\nalias:: Drake, Ver\n* Texte #+ALIAS: non";
        assert_eq!(aliases(org, Format::Org), vec!["Le dragon", "Drake", "Wyrm", "Ver"]);
        let md = "---\naliases: [Le dragon]\n---\nalias:: [[Drake]]\n#+ALIAS: pas en Markdown";
        assert_eq!(aliases(md, Format::Markdown), vec!["Le dragon", "Drake"]);
        assert!(aliases("* Rien", Format::Org).is_empty());
    }
}
