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

/// How a link is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `[[target]]`, `[[target][text]]`, and when the extended syntax is on
    /// `[[target|text]]` and `[[Page#Titre]]`.
    Org,
    /// `[text](target)`, only with the extended syntax.
    Markdown,
}

/// A link of a text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// Char range of the whole link.
    pub start: usize,
    pub end: usize,
    /// What it leads to, as org-mode writes it: `Page`, `Page::*Titre`, `*Titre`,
    /// `Page.md`, `https://…`, `./assets/a.png`.
    pub target: String,
    /// Char range of the page name in the text, what renaming the page rewrites
    /// (`None` for a link to a place of the same page).
    pub page: Option<(usize, usize)>,
    pub kind: Kind,
}

/// The links of `chars`. With `extended` (always for Markdown pages), also
/// `[[Page|texte]]`, `[[Page#Titre]]` and `[texte](Page.md)`.
pub fn scan(chars: &[char], extended: bool) -> Vec<Found> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let found = if chars[i] == '[' && chars.get(i + 1) == Some(&'[') {
            org_link(chars, i, extended)
        } else if extended && chars[i] == '[' && (i == 0 || !matches!(chars[i - 1], '[' | '!' | '\\')) {
            markdown_link(chars, i)
        } else {
            None
        };
        match found {
            Some(f) => { i = f.end; out.push(f); }
            None => i += 1,
        }
    }
    out
}

/// The link starting at `at` (`[[`), if it is closed.
fn org_link(chars: &[char], at: usize, extended: bool) -> Option<Found> {
    let ts = at + 2;
    let te = ts + chars[ts..].iter().position(|&c| matches!(c, ']' | '['))?;
    let end = match (chars.get(te), chars.get(te + 1)) {
        (Some(']'), Some(']')) => te + 2,
        (Some(']'), Some('[')) => {
            let close = (te + 2..chars.len().saturating_sub(1)).find(|&j| chars[j] == ']' && chars[j + 1] == ']')?;
            if chars[te + 2..close].contains(&'[') { return None; }
            close + 2
        }
        _ => return None,
    };
    // `[[Page|texte]]`: the text after the bar
    let te = if extended { (ts..te).find(|&j| chars[j] == '|').unwrap_or(te) } else { te };
    let raw: String = chars[ts..te].iter().collect();
    let trimmed = |a: usize, b: usize| {
        let a = a + chars[a..b].iter().take_while(|c| c.is_whitespace()).count();
        let b = b - chars[a..b].iter().rev().take_while(|c| c.is_whitespace()).count();
        (a < b).then_some((a, b))
    };
    let sep = raw.find(SEPARATOR).map(|b| ts + raw[..b].chars().count());
    let hash = (extended && sep.is_none() && !raw.contains("://"))
        .then(|| (ts..te).find(|&j| chars[j] == '#'))
        .flatten()
        .filter(|&h| chars[ts..h].iter().any(|c| !c.is_whitespace()));
    let (target, page) = if let Some(h) = hash {
        // `[[Page#Titre]]`: org-mode's `Page::*Titre`
        let page: String = chars[ts..h].iter().collect();
        let place: String = chars[h + 1..te].iter().collect();
        (format!("{}{SEPARATOR}*{}", page.trim(), place.trim()), trimmed(ts, h))
    } else {
        let page_end = sep.unwrap_or(te);
        let address = raw.contains("://") || raw.trim_start().starts_with("file:") || raw.trim_start().starts_with("mailto:");
        (raw.trim().to_string(), trimmed(ts, page_end).filter(|&(a, _)| !address && !matches!(chars[a], '*' | '#')))
    };
    Some(Found { start: at, end, target, page, kind: Kind::Org })
}

/// The Markdown link `[texte](cible)` starting at `at`.
fn markdown_link(chars: &[char], at: usize) -> Option<Found> {
    let close = at + 1 + chars[at + 1..].iter().position(|&c| matches!(c, ']' | '[' | '\n'))?;
    if chars[close] != ']' || chars.get(close + 1) != Some(&'(') { return None; }
    let ds = close + 2;
    let (ps, pe, end) = if chars.get(ds) == Some(&'<') {
        let gt = ds + 1 + chars[ds + 1..].iter().position(|&c| matches!(c, '>' | '\n'))?;
        if chars[gt] != '>' || chars.get(gt + 1) != Some(&')') { return None; }
        (ds + 1, gt, gt + 2)
    } else {
        let paren = ds + chars[ds..].iter().position(|&c| c == ')' || c.is_whitespace())?;
        if chars[paren] != ')' { return None; }
        (ds, paren, paren + 1)
    };
    let dest: String = chars[ps..pe].iter().collect();
    let found = |target: String, page| Some(Found { start: at, end, target, page, kind: Kind::Markdown });
    if dest.is_empty() { return None; }
    if dest.contains("://") || dest.starts_with("mailto:") || dest.starts_with('#') || dest.starts_with('/')
        || dest.starts_with("../") {
        return found(dest, None);
    }
    // A page: `Page.md`, `./Page.md`, `Ma%20page.md#titre`
    let skip = if dest.starts_with("./") { 2 } else { 0 };
    let (file, place) = match dest[skip..].split_once('#') {
        Some((f, p)) => (f, Some(p)),
        None => (&dest[skip..], None),
    };
    let name = percent_decode(file);
    if crate::format::split_ext(&name).is_none() || name.contains('/') {
        let target = if skip == 2 { dest } else { format!("./{dest}") };
        return found(target, None);
    }
    let target = match place.map(percent_decode).filter(|p| !p.is_empty()) {
        Some(p) => format!("{name}{SEPARATOR}*{}", p.replace('-', " ")),
        None => name,
    };
    let page_start = ps + skip;
    found(target, Some((page_start, page_start + file.chars().count())))
}

/// `%20` and the like decoded.
pub fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        if bytes[i] == b'%' && i + 2 < bytes.len()
            && let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            out.push((h * 16 + l) as u8);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `name` as a Markdown link writes it: spaces and parentheses encoded.
pub fn percent_encode(name: &str) -> String {
    name.replace('%', "%25").replace(' ', "%20").replace('(', "%28").replace(')', "%29")
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
    let level = |line: &str| {
        let stars = line.chars().take_while(|&c| c == '*').count();
        (stars > 0 && matches!(line.chars().nth(stars), Some(' ') | None)).then_some(stars)
    };
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

    fn scanned(text: &str, extended: bool) -> Vec<(String, Option<String>)> {
        let chars: Vec<char> = text.chars().collect();
        scan(&chars, extended).into_iter()
            .map(|f| (f.target, f.page.map(|(a, b)| chars[a..b].iter().collect())))
            .collect()
    }

    fn pair(t: &str, p: Option<&str>) -> (String, Option<String>) { (t.to_string(), p.map(String::from)) }

    #[test]
    fn scans_org_links() {
        assert_eq!(scanned("a [[ Page ][x]] [[P::*T]] [[*T]] [[b|c]] [[ouvert", false), vec![
            pair("Page", Some("Page")), pair("P::*T", Some("P")), pair("*T", None), pair("b|c", Some("b|c")),
        ]);
        // Not a Markdown link without the extended syntax
        assert!(scanned("[t](Page.md)", false).is_empty());
    }

    #[test]
    fn scans_extended_links() {
        assert_eq!(scanned("[[Page|texte]] [[Page#Mon titre]] [[#id]] [[https://a.b/#x]]", true), vec![
            pair("Page", Some("Page")), pair("Page::*Mon titre", Some("Page")), pair("#id", None),
            pair("https://a.b/#x", None),
        ]);
        assert_eq!(scanned("[t](Ma%20page.md) [u](./P.org#mon-titre) [v](<Deux mots.md>) ![i](a.png) [w](https://x.y) [z](assets/a.pdf)", true), vec![
            pair("Ma page.md", Some("Ma%20page.md")), pair("P.org::*mon titre", Some("P.org")),
            pair("Deux mots.md", Some("Deux mots.md")), pair("https://x.y", None), pair("./assets/a.pdf", None),
        ]);
        assert_eq!(percent_encode("a b(1)"), "a%20b%281%29");
        assert_eq!(percent_decode("%C3%A9t%C3%A9%2"), "été%2");
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
