//! Reading org pages: the targets of their links (backlinks), and renaming a
//! page in the links to it.

pub use orgamnesia_core::hashtags::{hashtag_at, Hashtags, OrgTags, TagSyntax};

/// Extract all `[[target]]` link targets — used for backlink indexing.
/// Unless `tags.hashtags` is `Off`, every `#tag` counts as a link to the page `tag`.
/// With `tags.org.links`, org-mode tags (`#+FILETAGS:`, `* Title :tag:`, `:tag:` in
/// the text) are links to their page too.
pub fn extract_links(content: &str, tags: TagSyntax) -> Vec<String> {
    let chars: Vec<char> = content.chars().collect();
    let mut links = Vec::new();
    let mut i = 0;
    while i + 1 < chars.len() {
        if chars[i] == '[' && chars[i + 1] == '[' {
            i += 2;
            let mut target = String::new();
            while i < chars.len() && chars[i] != ']' && chars[i] != '[' {
                target.push(chars[i]);
                i += 1;
            }
            // skip optional display text [display]]
            if i < chars.len() && chars[i] == '[' {
                while i < chars.len() && chars[i] != ']' { i += 1; }
                if i < chars.len() { i += 1; }
            }
            // skip closing ]]
            if i + 1 < chars.len() && chars[i] == ']' && chars[i + 1] == ']' {
                i += 2;
            }
            // `[[Page::*Titre]]` links to `Page`
            let t = match orgamnesia_core::links::split(&target) {
                ("", _) => target.trim(),
                (page, _) => page,
            };
            if !t.is_empty() { links.push(t.to_string()); }
        } else if let Some((tag, end)) = hashtag_at(&chars, i, tags.hashtags) {
            links.push(tag);
            i = end;
        } else {
            i += 1;
        }
    }
    if tags.org.links {
        links.extend(crate::tags::written_tags(content, tags.org));
        links.extend(orgamnesia_core::hashtags::text_tag_ranges(&chars, tags.org).into_iter()
            .map(|(a, b)| chars[a..b].iter().collect::<String>()));
    }
    links
}

/// True for `[[targets]]` that name a wiki page (not a URL, a file or an internal anchor).
pub fn is_page_link(target: &str) -> bool {
    let t = orgamnesia_core::links::split(target).0;
    !(t.is_empty()
        || t.contains("://")
        || t.starts_with("file:")
        || t.starts_with("mailto:")
        || t.starts_with('#')
        || t.starts_with('*')
        || t.starts_with("./")
        || t.starts_with("../"))
}

/// Rewrite every `[[old]]` / `[[old][label]]` link so it targets `new`.
/// With `ignore_case`, `[[pageMagique]]` also matches `PageMagique`, and `[[elody]]` `Élody`.
/// Org-mode tags naming `old` are renamed too (in the text only when they are links),
/// when `new` can be a tag.
/// Returns `None` when nothing changed. The display label is left untouched.
pub fn rewrite_links(content: &str, old_name: &str, new: &str, ignore_case: bool, tags: TagSyntax) -> Option<String> {
    let hashtags = tags.hashtags;
    let chars: Vec<char> = content.chars().collect();
    let norm = |s: &str| orgamnesia_core::names::page_key(s, ignore_case);
    let old = norm(old_name);
    let mut out = String::with_capacity(content.len());
    let mut changed = false;
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '[' && i + 1 < chars.len() && chars[i + 1] == '[' {
            out.push_str("[[");
            i += 2;
            let mut target = String::new();
            while i < chars.len() && chars[i] != ']' && chars[i] != '[' {
                target.push(chars[i]);
                i += 1;
            }
            // `[[old::*Titre]]` keeps its place in the page
            let (page, place) = match target.split_once(orgamnesia_core::links::SEPARATOR) {
                Some((page, place)) => (page, Some(place)),
                None => (target.as_str(), None),
            };
            if norm(page.trim()) == old {
                let lead: String = page.chars().take_while(|c| c.is_whitespace()).collect();
                let trail: String = page.chars().rev().take_while(|c| c.is_whitespace()).collect();
                out.push_str(&lead);
                out.push_str(new);
                out.push_str(&trail);
                if let Some(place) = place {
                    out.push_str(orgamnesia_core::links::SEPARATOR);
                    out.push_str(place);
                }
                changed = true;
            } else {
                out.push_str(&target);
            }
        } else if let Some((tag, end)) = hashtag_at(&chars, i, hashtags) {
            if norm(&tag) == old {
                // A name that is not a valid tag (spaces…) becomes a regular link
                if new.chars().all(|c| hashtags.is_tag_char(c)) { out.push('#'); out.push_str(new); }
                else { out.push_str(&format!("[[{new}]]")); }
                changed = true;
            } else {
                out.extend(&chars[i..end]);
            }
            i = end;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    let text = if changed { out } else { content.to_string() };
    crate::tags::rename(&text, old_name, new, ignore_case, tags.org, tags.org.links).or(changed.then_some(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── extract_links ──
    #[test]
    fn extract_links_finds_simple() {
        let links = extract_links("See [[Page One]] done", Hashtags::Off.into());
        assert_eq!(links, vec!["Page One"]);
    }
    #[test]
    fn extract_links_with_display() {
        let links = extract_links("[[Target][label]]", Hashtags::Off.into());
        assert_eq!(links, vec!["Target"]);
    }
    #[test]
    fn extract_links_multiple() {
        let links = extract_links("[[A]] and [[B]] and [[C][display]]", Hashtags::Off.into());
        assert_eq!(links, vec!["A", "B", "C"]);
    }
    #[test]
    fn page_links_vs_others() {
        assert!(is_page_link("My Page"));
        assert!(!is_page_link("https://orgmode.org"));
        assert!(!is_page_link("file:../assets/a.svg"));
        assert!(!is_page_link("../assets/a.svg"));
    }
    #[test]
    fn rewrite_links_simple_and_labeled() {
        let out = rewrite_links("a [[Old]] b [[Old][lbl]] c [[Other]]", "Old", "New", false, Hashtags::Off.into()).unwrap();
        assert_eq!(out, "a [[New]] b [[New][lbl]] c [[Other]]");
    }
    #[test]
    fn links_to_a_place_in_a_page() {
        let links = extract_links("[[Recettes::*Desserts]] [[ A :: cible ][x]] [[*Titre]]", Hashtags::Off.into());
        assert_eq!(links, vec!["Recettes", "A", "*Titre"]);
        assert!(is_page_link("Recettes::*Desserts"));
        assert!(!is_page_link("*Titre"));
        assert!(!is_page_link("::cible"));
        let out = rewrite_links("[[Old::*Titre]] [[Old ::cible][l]]", "Old", "New", false, Hashtags::Off.into()).unwrap();
        assert_eq!(out, "[[New::*Titre]] [[New ::cible][l]]");
    }
    #[test]
    fn rewrite_links_ignores_case() {
        let out = rewrite_links("[[pageMagique]] [[PAGEMAGIQUE][x]]", "PageMagique", "PageMagique3", true, Hashtags::Off.into()).unwrap();
        assert_eq!(out, "[[PageMagique3]] [[PageMagique3][x]]");
    }
    #[test]
    fn rewrite_links_case_sensitive_when_asked() {
        assert!(rewrite_links("[[pageMagique]]", "PageMagique", "X", false, Hashtags::Off.into()).is_none());
    }
    #[test]
    fn rewrite_links_no_match() {
        assert!(rewrite_links("[[Other]] and Old", "Old", "New", false, Hashtags::Off.into()).is_none());
    }
    #[test]
    fn hashtags_are_links_only_when_enabled() {
        let t = "Voir #idée et (#Rust) #+TITLE: x # note a#b [[P]] #fin-";
        assert_eq!(extract_links(t, Hashtags::Dashes.into()), vec!["idée", "Rust", "P", "fin"]);
        assert_eq!(extract_links(t, Hashtags::Off.into()), vec!["P"]);
    }
    #[test]
    fn hashtag_dashes_only_when_allowed() {
        let t = "#mon-tag et #mon_tag";
        assert_eq!(extract_links(t, Hashtags::Dashes.into()), vec!["mon-tag", "mon_tag"]);
        assert_eq!(extract_links(t, Hashtags::Org.into()), vec!["mon", "mon_tag"]);
    }
    #[test]
    fn rewrite_hashtags() {
        let out = rewrite_links("a #Old b #Older", "old", "New", true, Hashtags::Dashes.into()).unwrap();
        assert_eq!(out, "a #New b #Older");
        let out = rewrite_links("a #Old", "Old", "Deux mots", false, Hashtags::Dashes.into()).unwrap();
        assert_eq!(out, "a [[Deux mots]]");
        // A dash is not allowed in an org-mode style tag
        let out = rewrite_links("a #Old", "Old", "New-name", false, Hashtags::Org.into()).unwrap();
        assert_eq!(out, "a [[New-name]]");
        assert!(rewrite_links("a #Old", "Old", "New", false, Hashtags::Off.into()).is_none());
    }
    #[test]
    fn org_tags_are_links() {
        let t = "#+FILETAGS: :projet:\n* T :urgent:projet:\n[[P]]";
        assert_eq!(extract_links(t, Hashtags::Off.into()), vec!["P", "projet", "urgent", "projet"]);
        let out = rewrite_links(t, "projet", "Projet2", false, Hashtags::Off.into()).unwrap();
        assert_eq!(out, "#+FILETAGS: :Projet2:\n* T :urgent:Projet2:\n[[P]]");
    }
    #[test]
    fn org_tags_in_text_are_links_when_enabled() {
        let t = "* T :a:\nvoir :ex: et :mon-tag:";
        // Dashes allowed by default
        assert_eq!(extract_links(t, Hashtags::Off.into()), vec!["a", "ex", "mon-tag"]);
        let no_dashes = TagSyntax { hashtags: Hashtags::Off, org: OrgTags { links: true, dashes: false } };
        assert_eq!(extract_links(t, no_dashes), vec!["a", "ex"]);
        let off = TagSyntax { hashtags: Hashtags::Off, org: OrgTags { links: false, dashes: false } };
        assert!(extract_links(t, off).is_empty());
        let out = rewrite_links(t, "ex", "Exemple", false, Hashtags::Off.into()).unwrap();
        assert_eq!(out, "* T :a:\nvoir :Exemple: et :mon-tag:");
    }
    #[test]
    fn extract_links_empty() {
        assert!(extract_links("no links here", Hashtags::Off.into()).is_empty());
    }
    // ── split_title_tags ──
    // ── parse_table_row ──
    // ── parse_document ──
}
