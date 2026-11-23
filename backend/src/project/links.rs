//! Reading org pages: the targets of their links (backlinks), and renaming a
//! page in the links to it.

pub use orgamnesia_core::hashtags::{hashtag_at, Hashtags, OrgTags, TagSyntax};
use orgamnesia_core::{format::split_ext, links};

/// Extract all `[[target]]` link targets — used for backlink indexing: the page
/// they name (`Page` for `[[Page::*Titre]]`), else the target as it is.
/// Also those of `tags.links(format)` among `[[Page|texte]]`, `[[Page#Titre]]` and
/// `[texte](Page.md)` (Markdown links to other things than pages are left out).
/// Unless `tags.hashtags` is `Off`, every `#tag` counts as a link to the page `tag`.
/// With `tags.org.links`, org-mode tags (`#+FILETAGS:`, `* Title :tag:`, `:tag:` in
/// the text) are links to their page too.
pub fn extract_links(content: &str, tags: TagSyntax, format: orgamnesia_core::Format) -> Vec<String> {
    let chars: Vec<char> = content.chars().collect();
    let found = links::scan(&chars, tags.links(format));
    let mut out = Vec::new();
    let mut next = found.iter().peekable();
    let mut i = 0;
    while i < chars.len() {
        if let Some(f) = next.next_if(|f| f.start == i) {
            if f.kind == links::Kind::Org || f.page.is_some() {
                let t = match links::split(&f.target) {
                    ("", _) => f.target.trim(),
                    (page, _) => page,
                };
                if !t.is_empty() { out.push(t.to_string()); }
            }
            i = f.end;
        } else if let Some((tag, end)) = hashtag_at(&chars, i, tags.hashtags) {
            out.push(tag);
            i = end;
        } else {
            i += 1;
        }
    }
    if tags.org.links {
        out.extend(crate::tags::written_tags(content, tags.org, format));
        out.extend(orgamnesia_core::hashtags::text_tag_ranges(&chars, tags.org).into_iter()
            .map(|(a, b)| chars[a..b].iter().collect::<String>()));
    }
    out
}

/// A page renamed, as `rewrite_links` follows it.
pub struct Rename<'a> {
    /// Whether a link naming this page (`Old`, `Old.md`…) leads to it.
    pub leads_here: &'a dyn Fn(&str) -> bool,
    /// Its name before.
    pub old: &'a str,
    /// Its name alone leads to it (not to a page of the same name in the other
    /// format): the tags naming it are renamed too.
    pub by_name: bool,
    pub new: &'a str,
}

/// Rewrite every link leading to the renamed page so it targets its new name:
/// `[[old]]`, `[[old][label]]`, `[[old::*Titre]]`, and with the extended syntax
/// `[[old|label]]`, `[[old#Titre]]`, `[label](old.md)`. A name written with its
/// extension keeps it. With `ignore_case`, `[[pageMagique]]` also matches
/// `PageMagique`, and `[[elody]]` `Élody` (as `leads_here` says).
/// Hashtags and org-mode tags naming `old` are renamed too (in the text only when
/// they are links), when `new` can be a tag.
/// Returns `None` when nothing changed. The display label is left untouched.
pub fn rewrite_links(content: &str, rename: &Rename, ignore_case: bool, tags: TagSyntax, format: orgamnesia_core::Format) -> Option<String> {
    let hashtags = tags.hashtags;
    let chars: Vec<char> = content.chars().collect();
    let norm = |s: &str| orgamnesia_core::names::page_key(s, ignore_case);
    let old = norm(rename.old);
    let new = rename.new;
    let found = links::scan(&chars, tags.links(format));
    let mut next = found.iter().peekable();
    let mut out = String::with_capacity(content.len());
    let mut changed = false;
    let mut i = 0;
    while i < chars.len() {
        if let Some(f) = next.next_if(|f| f.start == i) {
            let written = f.page.map(|(a, b)| (a, b, chars[a..b].iter().collect::<String>()));
            let name = written.as_ref().map(|(_, _, w)| match f.kind {
                links::Kind::Markdown => links::percent_decode(w),
                links::Kind::Org => w.clone(),
            });
            match (written, name) {
                (Some((a, b, _)), Some(name)) if (rename.leads_here)(&name) => {
                    // `Old.md` stays `New.md`
                    let mut to = match split_ext(&name).filter(|(stem, _)| norm(stem.trim()) == old) {
                        Some((stem, _)) => format!("{new}{}", &name[stem.len()..]),
                        None => new.to_string(),
                    };
                    let angle = a > 0 && chars[a - 1] == '<';
                    if f.kind == links::Kind::Markdown && !angle { to = links::percent_encode(&to); }
                    out.extend(&chars[i..a]);
                    out.push_str(&to);
                    out.extend(&chars[b..f.end]);
                    changed = true;
                }
                _ => out.extend(&chars[i..f.end]),
            }
            i = f.end;
        } else if let Some((tag, end)) = hashtag_at(&chars, i, hashtags) {
            if rename.by_name && norm(&tag) == old {
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
    let tags_renamed = rename.by_name.then(|| crate::tags::rename(&text, rename.old, new, ignore_case, tags.org, tags.org.links)).flatten();
    tags_renamed.or(changed.then_some(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `rewrite_links` for a page `old` alone of its name.
    fn rw(content: &str, old: &str, new: &str, ignore_case: bool, tags: TagSyntax) -> Option<String> {
        let norm = |s: &str| orgamnesia_core::names::page_key(s.trim(), ignore_case);
        let leads_here = |t: &str| norm(split_ext(t).map_or(t, |(s, _)| s)) == norm(old);
        rewrite_links(content, &Rename { leads_here: &leads_here, old, by_name: true, new }, ignore_case, tags, orgamnesia_core::Format::Org)
    }

    fn extended() -> TagSyntax { TagSyntax { org_links: links::LinkSyntax::ALL, ..Hashtags::Off.into() } }

    #[test]
    fn extended_links_are_read_and_renamed() {
        let t = "[[Old|x]] [[Old#Titre]] [y](Old.md) [z](https://a.b) [w](./assets/a.png) [[Old.md]]";
        assert_eq!(extract_links(t, extended(), orgamnesia_core::Format::Org), vec!["Old", "Old", "Old.md", "Old.md"]);
        assert_eq!(extract_links("[[Old|x]] [y](Old.md)", Hashtags::Off.into(), orgamnesia_core::Format::Org), vec!["Old|x"]);
        let out = rw(t, "Old", "Deux mots", false, extended()).unwrap();
        assert_eq!(out, "[[Deux mots|x]] [[Deux mots#Titre]] [y](Deux%20mots.md) [z](https://a.b) [w](./assets/a.png) [[Deux mots.md]]");
        assert_eq!(rw("[y](<Old.md>)", "Old", "A b", false, extended()).unwrap(), "[y](<A b.md>)");
    }

    // ── extract_links ──
    #[test]
    fn extract_links_finds_simple() {
        let links = extract_links("See [[Page One]] done", Hashtags::Off.into(), orgamnesia_core::Format::Org);
        assert_eq!(links, vec!["Page One"]);
    }
    #[test]
    fn extract_links_with_display() {
        let links = extract_links("[[Target][label]]", Hashtags::Off.into(), orgamnesia_core::Format::Org);
        assert_eq!(links, vec!["Target"]);
    }
    #[test]
    fn extract_links_multiple() {
        let links = extract_links("[[A]] and [[B]] and [[C][display]]", Hashtags::Off.into(), orgamnesia_core::Format::Org);
        assert_eq!(links, vec!["A", "B", "C"]);
    }
    #[test]
    fn rewrite_links_simple_and_labeled() {
        let out = rw("a [[Old]] b [[Old][lbl]] c [[Other]]", "Old", "New", false, Hashtags::Off.into()).unwrap();
        assert_eq!(out, "a [[New]] b [[New][lbl]] c [[Other]]");
    }
    #[test]
    fn links_to_a_place_in_a_page() {
        let links = extract_links("[[Recettes::*Desserts]] [[ A :: cible ][x]] [[*Titre]]", Hashtags::Off.into(), orgamnesia_core::Format::Org);
        assert_eq!(links, vec!["Recettes", "A", "*Titre"]);
        let out = rw("[[Old::*Titre]] [[Old ::cible][l]]", "Old", "New", false, Hashtags::Off.into()).unwrap();
        assert_eq!(out, "[[New::*Titre]] [[New ::cible][l]]");
    }
    #[test]
    fn only_whole_links_count() {
        let links = extract_links("[[A]] [[B sans fin [[C][voir #x]] [[D]", Hashtags::Dashes.into(), orgamnesia_core::Format::Org);
        assert_eq!(links, vec!["A", "C"]);
        let out = rw("[[Old][voir #Old]] [[Old", "Old", "New", false, Hashtags::Dashes.into()).unwrap();
        assert_eq!(out, "[[New][voir #Old]] [[Old");
    }
    #[test]
    fn rewrite_links_ignores_case() {
        let out = rw("[[pageMagique]] [[PAGEMAGIQUE][x]]", "PageMagique", "PageMagique3", true, Hashtags::Off.into()).unwrap();
        assert_eq!(out, "[[PageMagique3]] [[PageMagique3][x]]");
    }
    #[test]
    fn rewrite_links_case_sensitive_when_asked() {
        assert!(rw("[[pageMagique]]", "PageMagique", "X", false, Hashtags::Off.into()).is_none());
    }
    #[test]
    fn rewrite_links_no_match() {
        assert!(rw("[[Other]] and Old", "Old", "New", false, Hashtags::Off.into()).is_none());
    }
    #[test]
    fn hashtags_are_links_only_when_enabled() {
        let t = "Voir #idée et (#Rust) #+TITLE: x # note a#b [[P]] #fin-";
        assert_eq!(extract_links(t, Hashtags::Dashes.into(), orgamnesia_core::Format::Org), vec!["idée", "Rust", "P", "fin"]);
        assert_eq!(extract_links(t, Hashtags::Off.into(), orgamnesia_core::Format::Org), vec!["P"]);
    }
    #[test]
    fn hashtag_dashes_only_when_allowed() {
        let t = "#mon-tag et #mon_tag";
        assert_eq!(extract_links(t, Hashtags::Dashes.into(), orgamnesia_core::Format::Org), vec!["mon-tag", "mon_tag"]);
        assert_eq!(extract_links(t, Hashtags::Org.into(), orgamnesia_core::Format::Org), vec!["mon", "mon_tag"]);
    }
    #[test]
    fn rewrite_hashtags() {
        let out = rw("a #Old b #Older", "old", "New", true, Hashtags::Dashes.into()).unwrap();
        assert_eq!(out, "a #New b #Older");
        let out = rw("a #Old", "Old", "Deux mots", false, Hashtags::Dashes.into()).unwrap();
        assert_eq!(out, "a [[Deux mots]]");
        // A dash is not allowed in an org-mode style tag
        let out = rw("a #Old", "Old", "New-name", false, Hashtags::Org.into()).unwrap();
        assert_eq!(out, "a [[New-name]]");
        assert!(rw("a #Old", "Old", "New", false, Hashtags::Off.into()).is_none());
    }
    #[test]
    fn org_tags_are_links() {
        let t = "#+FILETAGS: :projet:\n* T :urgent:projet:\n[[P]]";
        assert_eq!(extract_links(t, Hashtags::Off.into(), orgamnesia_core::Format::Org), vec!["P", "projet", "urgent", "projet"]);
        let out = rw(t, "projet", "Projet2", false, Hashtags::Off.into()).unwrap();
        assert_eq!(out, "#+FILETAGS: :Projet2:\n* T :urgent:Projet2:\n[[P]]");
    }
    #[test]
    fn org_tags_in_text_are_links_when_enabled() {
        let t = "* T :a:\nvoir :ex: et :mon-tag:";
        // Dashes allowed by default
        assert_eq!(extract_links(t, Hashtags::Off.into(), orgamnesia_core::Format::Org), vec!["a", "ex", "mon-tag"]);
        let no_dashes = TagSyntax { hashtags: Hashtags::Off, org: OrgTags { links: true, dashes: false }, ..Hashtags::Off.into() };
        assert_eq!(extract_links(t, no_dashes, orgamnesia_core::Format::Org), vec!["a", "ex"]);
        let off = TagSyntax { hashtags: Hashtags::Off, org: OrgTags { links: false, dashes: false }, ..Hashtags::Off.into() };
        assert!(extract_links(t, off, orgamnesia_core::Format::Org).is_empty());
        let out = rw(t, "ex", "Exemple", false, Hashtags::Off.into()).unwrap();
        assert_eq!(out, "* T :a:\nvoir :Exemple: et :mon-tag:");
    }
    #[test]
    fn extract_links_empty() {
        assert!(extract_links("no links here", Hashtags::Off.into(), orgamnesia_core::Format::Org).is_empty());
    }
    // ── split_title_tags ──
    // ── parse_table_row ──
    // ── parse_document ──
}
