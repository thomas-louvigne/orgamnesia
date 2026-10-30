//! Links to a place in a page, written as org-mode's search options
//! (`file:page.org::*Titre`): `[[Page::*Titre]]` leads to the headline `Titre` of
//! `Page`, `[[Page::cible]]` to its dedicated target `<<cible>>`. Without a page
//! (`[[*Titre]]`), the place is in the page the link is in.

use crate::{names::fold, TodoKeywords};

/// Between the page and the place in it.
pub const SEPARATOR: &str = "::";

/// The page a link target names and the place in it: `Page::*Titre` →
/// (`Page`, `Some("*Titre")`), `*Titre` → (``, `Some("*Titre")`), `Page` →
/// (`Page`, `None`). Only for the targets that are not files or addresses.
pub fn split(target: &str) -> (&str, Option<&str>) {
    let t = target.trim();
    if let Some((page, place)) = t.split_once(SEPARATOR) {
        return (page.trim(), Some(place.trim()).filter(|p| !p.is_empty()));
    }
    if t.starts_with('*') || t.starts_with('#') { ("", Some(t)) } else { (t, None) }
}

/// A link `[[target]]` or `[[target][description]]` of a text, by char
/// positions: `start..end` is the whole link, `target` and `description` the
/// ranges of their text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Link {
    pub start: usize,
    pub end: usize,
    pub target: (usize, usize),
    pub description: Option<(usize, usize)>,
}

impl Link {
    /// The target as written (not trimmed).
    pub fn target(&self, chars: &[char]) -> String {
        chars[self.target.0..self.target.1].iter().collect()
    }

    pub fn description(&self, chars: &[char]) -> Option<String> {
        self.description.map(|(a, b)| chars[a..b].iter().collect())
    }
}

/// The link whose `[[` is at `start`, if a whole one is written there: a target
/// holding no bracket nor line break, then `]]`, or `][`, a description and `]]`.
/// This is how every part of the app reads links.
pub fn link_at(chars: &[char], start: usize) -> Option<Link> {
    if chars.get(start) != Some(&'[') || chars.get(start + 1) != Some(&'[') { return None; }
    let t0 = start + 2;
    let t1 = (t0..chars.len()).find(|&i| matches!(chars[i], '[' | ']' | '\n'))?;
    match (chars[t1], chars.get(t1 + 1)) {
        (']', Some(']')) => Some(Link { start, end: t1 + 2, target: (t0, t1), description: None }),
        (']', Some('[')) => {
            let d0 = t1 + 2;
            let d1 = (d0..chars.len().saturating_sub(1)).find(|&j| chars[j] == ']' && chars[j + 1] == ']')?;
            Some(Link { start, end: d1 + 2, target: (t0, t1), description: Some((d0, d1)) })
        }
        _ => None,
    }
}

/// The links of `chars`, in order.
pub fn links(chars: &[char]) -> impl Iterator<Item = Link> + '_ {
    let mut i = 0;
    std::iter::from_fn(move || {
        while i + 1 < chars.len() {
            if let Some(link) = link_at(chars, i) {
                i = link.end;
                return Some(link);
            }
            i += 1;
        }
        None
    })
}

/// The targets of the links of `text`, as written.
pub fn targets(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    links(&chars).map(|l| l.target(&chars)).collect()
}

/// The path a link target points to when it names a file: `file:x`, `./x`, `../x`,
/// `/x` or `~/x`.
pub fn file_target(target: &str) -> Option<&str> {
    let t = target.trim();
    let t = t.strip_prefix("file:").unwrap_or(t);
    let file = t.starts_with("./") || t.starts_with("../") || t.starts_with('/') || t.starts_with("~/");
    (file || target.trim().starts_with("file:")).then_some(t).filter(|t| !t.is_empty())
}

/// Whether a link target names a page: not a file, an address (`https://`,
/// `mailto:`) nor a place in the page (`#id`, `*Titre`). `Page::*Titre` names `Page`.
pub fn is_page_target(target: &str) -> bool {
    let t = split(target).0;
    !(t.is_empty() || t.contains("://") || t.starts_with("mailto:") || t.starts_with("file:")
        || t.starts_with('#') || t.starts_with('*') || file_target(t).is_some())
}

/// The title of the headline `line` as links name it, without its stars, TODO
/// keyword, priority (`[#A]`), `COMMENT`, statistics cookies (`[1/3]`, `[50%]`)
/// and tags; spaces collapsed. `None` when the line is not a headline.
pub fn headline_title(line: &str, keywords: Option<&TodoKeywords>) -> Option<String> {
    let chars: Vec<char> = line.chars().collect();
    let stars = chars.iter().take_while(|&&c| c == '*').count();
    if stars == 0 || !matches!(chars.get(stars), Some(' ') | None) { return None; }
    let start = keywords.and_then(|k| k.keyword(&chars)).map_or(stars, |k| k.end);
    let mut words: Vec<String> = chars[start..].iter().collect::<String>()
        .split_whitespace().map(String::from).collect();
    if words.first().is_some_and(|w| is_priority(w)) { words.remove(0); }
    if words.first().is_some_and(|w| w == "COMMENT") { words.remove(0); }
    if words.last().is_some_and(|w| is_tags(w)) { words.pop(); }
    words.retain(|w| !is_cookie(w));
    Some(words.join(" "))
}

/// `[#A]`
fn is_priority(w: &str) -> bool {
    w.len() == 4 && w.starts_with("[#") && w.ends_with(']')
}

/// `:a:b:`
fn is_tags(w: &str) -> bool {
    w.len() > 2 && w.starts_with(':') && w.ends_with(':') && !w[1..w.len() - 1].contains("::")
}

/// `[1/3]`, `[/]`, `[50%]`, `[%]`
fn is_cookie(w: &str) -> bool {
    let Some(inner) = w.strip_prefix('[').and_then(|w| w.strip_suffix(']')) else { return false };
    let digits = |s: &str| s.chars().all(|c| c.is_ascii_digit());
    match inner.split_once('/') {
        Some((a, b)) => digits(a) && digits(b),
        None => inner.strip_suffix('%').is_some_and(digits),
    }
}

/// The titles of the headlines of the page `text`, in order, each once.
pub fn headlines(text: &str, keywords: Option<&TodoKeywords>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for title in text.lines().filter_map(|l| headline_title(l, keywords)) {
        if !title.is_empty() && !out.contains(&title) { out.push(title); }
    }
    out
}

/// Where the place `place` of a link is in the page `text`, as a char range: the
/// line of the headline for `*Titre`; the `<<cible>>` for `cible`, failing that
/// a headline titled `cible`, as org-mode does. Case and accents are ignored.
/// `None` when it isn't there, and for a `#custom-id` (not supported).
pub fn find(text: &str, place: &str, keywords: Option<&TodoKeywords>) -> Option<(usize, usize)> {
    if place.starts_with('#') { return None; }
    let headline = |title: &str| {
        let wanted = fold(&title.split_whitespace().collect::<Vec<_>>().join(" "));
        lines(text).find(|(_, line)| headline_title(line, keywords).is_some_and(|t| fold(&t) == wanted))
            .map(|(at, line)| (at, at + line.chars().count()))
    };
    match place.strip_prefix('*') {
        Some(title) => headline(title),
        None => target(text, place).or_else(|| headline(place)),
    }
}

/// The lines of `text` with the char position each starts at.
fn lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut at = 0;
    text.split('\n').map(move |line| {
        let start = at;
        at += line.chars().count() + 1;
        (start, line)
    })
}

/// Char range of the first `<<name>>` (or radio target `<<<name>>>`) of `text`.
fn target(text: &str, name: &str) -> Option<(usize, usize)> {
    let wanted = fold(name.trim());
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i + 1 < chars.len() {
        if chars[i] == '<' && chars[i + 1] == '<' {
            let start = i;
            while i < chars.len() && chars[i] == '<' { i += 1; }
            let inner = i;
            while i < chars.len() && chars[i] != '>' && chars[i] != '\n' { i += 1; }
            if chars.get(i) == Some(&'>') && chars.get(i + 1) == Some(&'>') {
                let name: String = chars[inner..i].iter().collect();
                while i < chars.len() && chars[i] == '>' { i += 1; }
                if fold(name.trim()) == wanted { return Some((start, i)); }
            }
        } else {
            i += 1;
        }
    }
    None
}

/// Char position where the part of `text` shown for the place at `at` ends: the
/// end of the subtree when `at` is on a headline, else the next headline.
pub fn section_end(text: &str, at: usize) -> usize {
    let level = crate::org::headline_level_str;
    let mut lines = lines(text).skip_while(|(start, line)| start + line.chars().count() < at);
    let Some((_, first)) = lines.next() else { return text.chars().count() };
    let max = level(first).unwrap_or(usize::MAX);
    lines.find(|(_, line)| level(line).is_some_and(|l| l <= max))
        .map_or(text.chars().count(), |(start, _)| start.saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kw() -> TodoKeywords { TodoKeywords::parse("TODO | DONE") }

    #[test]
    fn links_as_written() {
        let c: Vec<char> = "[[a]] [[b][c d]]".chars().collect();
        assert_eq!(link_at(&c, 0), Some(Link { start: 0, end: 5, target: (2, 3), description: None }));
        assert_eq!(link_at(&c, 6), Some(Link { start: 6, end: 16, target: (8, 9), description: Some((11, 14)) }));
        assert_eq!(link_at(&c, 1), None);
        let text = "voir [[A]] et [[../b.png][image]], pas [[c\nd]] ni [[e] ni [[f ni [[g[h]]";
        assert_eq!(targets(text), vec!["A", "../b.png"]);
        assert_eq!(targets("[[x]]{:width 3} [[[y]]] [[z][a [b] c]]"), vec!["x", "y", "z"]);
        assert_eq!(targets("[[ Page ::*Titre]]"), vec![" Page ::*Titre"]);
    }

    #[test]
    fn targets_are_pages_or_files() {
        assert_eq!(file_target("../assets/a.png"), Some("../assets/a.png"));
        assert_eq!(file_target("file:notes.pdf"), Some("notes.pdf"));
        assert_eq!(file_target("~/doc.pdf"), Some("~/doc.pdf"));
        assert_eq!(file_target("Page"), None);
        assert!(is_page_target("Ma page"));
        assert!(is_page_target("Recettes::*Desserts"));
        for other in ["../assets/a.png", "file:../assets/a.svg", "file:a.org::*Titre", "/home/a.pdf",
            "https://orgmode.org", "mailto:a@b.c", "*Titre", "#id", "::cible"] {
            assert!(!is_page_target(other), "{other}");
        }
    }

    #[test]
    fn splits_page_and_place() {
        assert_eq!(split("Recettes::*Desserts"), ("Recettes", Some("*Desserts")));
        assert_eq!(split(" Recettes :: cible "), ("Recettes", Some("cible")));
        assert_eq!(split("Recettes::"), ("Recettes", None));
        assert_eq!(split("Recettes"), ("Recettes", None));
        assert_eq!(split("*Desserts"), ("", Some("*Desserts")));
        assert_eq!(split("#id"), ("", Some("#id")));
    }

    #[test]
    fn titles_as_links_name_them() {
        let k = kw();
        assert_eq!(headline_title("** TODO [#A] Faire [1/3] le pain :cuisine:", Some(&k)).as_deref(), Some("Faire le pain"));
        assert_eq!(headline_title("* COMMENT Brouillon  [50%]", None).as_deref(), Some("Brouillon"));
        assert_eq!(headline_title("* TODO Titre", None).as_deref(), Some("TODO Titre"));
        assert_eq!(headline_title("*gras*", None), None);
        assert_eq!(headline_title("texte", None), None);
        assert_eq!(headline_title("*", None).as_deref(), Some(""));
    }

    #[test]
    fn finds_headlines_and_targets() {
        let text = "#+TITLE: R\n* Entrées\n** DONE Desserts :sucre:\nla <<pâte>> ici\n";
        let k = kw();
        assert_eq!(find(text, "*desserts", Some(&k)), Some((21, 45)));
        assert_eq!(find(text, "*Entrees", Some(&k)), Some((11, 20)));
        assert_eq!(find(text, "pate", Some(&k)), Some((49, 57)));
        // No target of that name: a headline
        assert_eq!(find(text, "Desserts", Some(&k)), Some((21, 45)));
        assert_eq!(find(text, "*Plats", Some(&k)), None);
        assert_eq!(find(text, "#pate", Some(&k)), None);
        assert_eq!(find("<<<radio>>> x", "radio", None), Some((0, 11)));
    }

    #[test]
    fn lists_headlines_once() {
        assert_eq!(headlines("* A\ntexte\n** TODO B :t:\n* A\n*\n", Some(&kw())), vec!["A", "B"]);
    }

    #[test]
    fn sections_end_at_the_next_headline() {
        let text = "* A\na\n** B\nb\n* C\nc";
        assert_eq!(section_end(text, 0), 12);
        assert_eq!(section_end(text, 6), 12);
        assert_eq!(section_end(text, 4), 5);
        assert_eq!(section_end(text, 13), text.chars().count());
    }
}
