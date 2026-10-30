//! `[[links]]` as the editor handles them: what a link points to (to tell the
//! ones that lead nowhere), the link being typed and the pages completing it,
//! and where a page links to another.

pub use orgamnesia_core::links::{file_target, is_page_target, targets};
use orgamnesia_core::hashtags::{hashtag_at, tag_ranges};
use orgamnesia_core::links::{link_at, split};
use orgamnesia_core::{Hashtags, TagSyntax};

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
        } else if let Some(link) = link_at(chars, i) {
            let (start, end) = link.target;
            let raw = link.target(chars);
            // `[[Page::*Titre]]` is a link to `Page`
            if norm(&raw) == wanted || norm(split(&raw).0) == wanted {
                let lead = raw.chars().take_while(|c| c.is_whitespace()).count();
                let trail = raw.chars().rev().take_while(|c| c.is_whitespace()).count();
                return Some((start + lead, end - trail));
            }
            i = link.end;
        } else {
            i += 1;
        }
    }
    // Then the org-mode tags (`#+FILETAGS:`, `* Title :tag:`, `:tag:` in the text)
    if !tags.org.links { return None; }
    tag_ranges(chars, tags.org, true).into_iter()
        .find(|&(a, b)| norm(&chars[a..b].iter().collect::<String>()) == wanted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use orgamnesia_core::OrgTags;

    fn c(s: &str) -> Vec<char> { s.chars().collect() }

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
    fn finds_links() {
        let t = c("voir [[Autre]] puis [[ Cible ][label]] fin");
        assert_eq!(find_link(&t, "Cible", false, Hashtags::Off.into()), Some((23, 28)));
        assert_eq!(find_link(&t, "cible", false, Hashtags::Off.into()), None);
        assert_eq!(find_link(&t, "cible", true, Hashtags::Off.into()), Some((23, 28)));
        assert_eq!(find_link(&t, "Absent", true, Hashtags::Off.into()), None);
    }
}
