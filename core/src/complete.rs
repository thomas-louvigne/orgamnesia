//! Completion while a name is typed: what is being typed at the caret (a
//! `[[link`, a `#tag`, an org-mode `:tag`, the file of a Markdown link, a place
//! `[[Page::*Titre`), the names completing it, and the text accepting one puts in.
//! The window's editor and the terminal's suggest the same names.

/// The `[[target` being typed when the caret is at `caret` (inside `[[…`, before
/// any `]` or `[`, on the same line): returns the index where the target starts
/// and the part already typed.
pub fn link_prefix(chars: &[char], caret: usize) -> Option<(usize, String)> {
    let mut start = caret;
    while start > 0 && !matches!(chars[start - 1], '[' | ']' | '\n') { start -= 1; }
    if start < 2 || chars[start - 1] != '[' || chars[start - 2] != '[' { return None; }
    Some((start, chars[start..caret].iter().collect()))
}

/// The place being typed in a link: `Page::*Ti` → (`Page`, `*Ti`, where `*Ti`
/// starts); `*Ti` → (``, `*Ti`, 0), a headline of this page.
pub fn place_prefix(prefix: &str) -> Option<(String, String, usize)> {
    match prefix.split_once(crate::links::SEPARATOR) {
        Some((page, place)) => Some((page.trim().to_string(), place.to_string(), page.chars().count() + 2)),
        None => prefix.starts_with('*').then(|| (String::new(), prefix.to_string(), 0)),
    }
}

/// Page names completing `prefix` (ignoring case and accents): names starting with it first
/// or whose last part (a page in a folder: `Jeux/Dragon`) does (shortest first),
/// then names containing it; the exact name is left out. With
/// `tag: Some(_)`, only names usable as such a `#tag` are kept.
pub fn complete_page(names: &[String], prefix: &str, tag: Option<crate::Hashtags>, limit: usize) -> Vec<String> {
    let p = crate::names::fold(prefix.trim());
    let mut starts = Vec::new();
    let mut contains = Vec::new();
    for name in names {
        let valid = tag.is_none_or(|t| name.chars().next().is_some_and(char::is_alphanumeric)
            && name.chars().all(|c| t.is_tag_char(c)));
        let n = crate::names::fold(name);
        if !valid || n == p { continue; }
        // `dra` finds `Jeux/Dragon` as `Dragon`: by the page's own name too
        if n.starts_with(&p) || n.rsplit('/').next().is_some_and(|leaf| leaf.starts_with(&p)) { starts.push(name.clone()); }
        else if n.contains(&p) { contains.push(name.clone()); }
    }
    starts.sort_by_key(|n| (n.chars().count(), n.to_lowercase()));
    contains.sort_by_key(|n| n.to_lowercase());
    starts.into_iter().chain(contains).take(limit).collect()
}

/// Characters to type after `[[` before pages are suggested.
pub const LINK_MIN_CHARS: usize = 2;

/// What is being completed.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    /// A page name in `[[link`: accepting also closes the link with `]]`.
    Link,
    /// A page name after `#`.
    Hashtag,
    /// An org-mode tag (`* Title :tag`, `#+FILETAGS:`): accepting adds the closing `:`.
    OrgTag,
    /// A headline of a page in `[[Page::*Titre` or `[[*Titre`: accepting also
    /// closes the link with `]]`.
    Place,
    /// A headline of a page in `[[Page#Titre` (when that syntax is read): the
    /// same, written without the `*`.
    HashPlace,
    /// The file of a page in a Markdown link `[texte](Pa`, written `%`-encoded:
    /// accepting also closes the link with `)`.
    MdLink,
}

impl Kind {
    /// What closes the name once accepted.
    pub fn close(self) -> &'static str {
        match self {
            Kind::Link | Kind::Place | Kind::HashPlace => "]]",
            Kind::MdLink => ")",
            Kind::OrgTag => ":",
            Kind::Hashtag => "",
        }
    }

    /// What the popup shows before each name.
    pub fn sigil(self) -> &'static str {
        match self {
            Kind::Link => "[[",
            Kind::Hashtag => "#",
            Kind::OrgTag => ":",
            Kind::Place => "::",
            Kind::HashPlace => "#",
            Kind::MdLink => "](",
        }
    }
}

/// What is being typed at `caret` that names can complete: where the typed part
/// starts, that part, and what it is.
pub fn being_typed(chars: &[char], caret: usize, tags: crate::TagSyntax, format: crate::Format) -> Option<(usize, String, Kind)> {
    link_prefix(chars, caret)
        .filter(|(_, p)| p.trim().chars().count() >= LINK_MIN_CHARS)
        .map(|(start, p)| (start, p, Kind::Link))
        .or_else(|| tags.links(format).markdown.then(|| md_link_prefix(chars, caret)).flatten()
            .filter(|(_, p)| p.trim().chars().count() >= LINK_MIN_CHARS)
            .map(|(start, p)| (start, p, Kind::MdLink)))
        .or_else(|| crate::tags::org_tag_prefix(chars, caret, tags.org).map(|(start, p)| (start, p, Kind::OrgTag)))
        .or_else(|| crate::tags::hashtag_prefix(chars, caret, tags.hashtags).map(|(hash, p)| (hash + 1, p, Kind::Hashtag)))
}

/// The target of a Markdown link being typed at `caret` (`[texte](Pa`): where it
/// starts, and what is typed of it; not an address, nor a place (`#titre`).
pub fn md_link_prefix(chars: &[char], caret: usize) -> Option<(usize, String)> {
    let start = caret - chars[..caret].iter().rev().take_while(|&&c| !matches!(c, '(' | ')' | '[' | ']' | '\n')).count();
    if start < 2 || chars[start - 1] != '(' || chars[start - 2] != ']' { return None; }
    let typed: String = chars[start..caret].iter().collect();
    (!typed.contains("://") && !typed.contains('#') && !typed.starts_with('<')).then_some((start, typed))
}

/// The change accepting a name makes: `insert` replaces the characters `from..to`,
/// then the caret goes to `cursor`.
#[derive(Debug, PartialEq)]
pub struct Accepted { pub from: usize, pub to: usize, pub insert: String, pub cursor: usize }

/// Accepting `name` for what was typed between `start` and `caret` in `chars`.
/// In a link already closed (`[[jeu de r|ôle][texte]]`) the name replaces the
/// whole target and the caret goes past `]]` (or before `][`, to the text shown);
/// otherwise the closing characters of `kind` are added, or stepped over when
/// already there.
pub fn accepted(chars: &[char], start: usize, caret: usize, kind: Kind, name: &str) -> Accepted {
    if kind == Kind::MdLink {
        // The file name, `Ma%20page.md`; a target already closed is replaced whole
        let name = crate::links::percent_encode(name);
        let target_end = caret + chars[caret..].iter().take_while(|&&ch| !matches!(ch, ')' | '\n')).count();
        let after_name = start + name.chars().count();
        return if chars.get(target_end) == Some(&')') {
            Accepted { from: start, to: target_end, insert: name, cursor: after_name + 1 }
        } else {
            Accepted { from: start, to: caret, insert: format!("{name})"), cursor: after_name + 1 }
        };
    }
    let target_end = caret + chars[caret..].iter().take_while(|&&ch| !matches!(ch, ']' | '[' | '\n')).count();
    let closed = matches!(kind, Kind::Link | Kind::Place | Kind::HashPlace)
        && matches!((chars.get(target_end), chars.get(target_end + 1)), (Some(']'), Some(']' | '[')));
    let after_name = start + name.chars().count();
    if closed {
        let past = if chars.get(target_end + 1) == Some(&']') { 2 } else { 0 };
        return Accepted { from: start, to: target_end, insert: name.to_string(), cursor: after_name + past };
    }
    let close = kind.close();
    let n = close.chars().count();
    let there = chars.iter().skip(caret).take(n).collect::<String>() == close;
    let insert = if there { name.to_string() } else { format!("{name}{close}") };
    Accepted { from: start, to: caret, insert, cursor: after_name + n }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Hashtags;

    fn c(s: &str) -> Vec<char> { s.chars().collect() }

    #[test]
    fn place_being_typed() {
        assert_eq!(place_prefix("Page::*Ti"), Some(("Page".into(), "*Ti".into(), 6)));
        assert_eq!(place_prefix(" Idée ::cib"), Some(("Idée".into(), "cib".into(), 8)));
        assert_eq!(place_prefix("*Ti"), Some((String::new(), "*Ti".into(), 0)));
        assert_eq!(place_prefix("Page"), None);
    }
    #[test]
    fn pages_in_folders_complete_by_their_own_name() {
        let names: Vec<String> = ["Jeux/Quêtes/Dragon", "Notes/Dragonnier", "Hydra", "Dragée"].iter().map(|s| s.to_string()).collect();
        assert_eq!(complete_page(&names, "dra", None, 10), vec!["Dragée", "Notes/Dragonnier", "Jeux/Quêtes/Dragon", "Hydra"]);
        assert_eq!(complete_page(&names, "jeux/", None, 10), vec!["Jeux/Quêtes/Dragon"]);
    }

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

}
