//! `[[links]]` as the editor handles them: what a link points to (to tell the
//! ones that lead nowhere), the link being typed and the pages completing it,
//! and where a page links to another.

pub use orgamnesia_core::links::{file_target, is_page_target, targets};
use orgamnesia_core::hashtags::{hashtag_at, tag_ranges};
use orgamnesia_core::links::{link_at, split};
use orgamnesia_core::TagSyntax;

pub use orgamnesia_core::complete::{complete_page, place_prefix};

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

    use orgamnesia_core::{Hashtags, OrgTags};

    fn c(s: &str) -> Vec<char> { s.chars().collect() }

    #[test]
    fn org_tag_link_found() {
        let t = c("texte :x:\n* T :a:Cible:\n");
        assert_eq!(find_link(&t, "cible", true, Hashtags::Off.into()), Some((17, 22)));
        assert_eq!(find_link(&t, "x", true, Hashtags::Off.into()), Some((7, 8)));
        let off = TagSyntax { hashtags: Hashtags::Off, org: OrgTags { links: false, dashes: false }, ..Hashtags::Off.into() };
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
