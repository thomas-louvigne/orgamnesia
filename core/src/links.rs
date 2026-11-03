//! Links to a place in a page, written as org-mode's search options
//! (`file:page.org::*Titre`): `[[Page::*Titre]]` leads to the headline `Titre` of
//! `Page`, `[[Page::cible]]` to its dedicated target `<<cible>>`. Without a page
//! (`[[*Titre]]`), the place is in the page the link is in.

use crate::{names::fold, Format, TodoKeywords};

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

/// Which of the links not of org-mode are read: `[[Page|texte]]` (`pipe`),
/// `[[Page#Titre]]` (`hash`), `[texte](Page.md)` (`markdown`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LinkSyntax {
    pub pipe: bool,
    pub hash: bool,
    pub markdown: bool,
}

impl LinkSyntax {
    pub const NONE: Self = Self { pipe: false, hash: false, markdown: false };
    pub const ALL: Self = Self { pipe: true, hash: true, markdown: true };
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

/// The links of `chars`, and those of `syntax` among `[[Page|texte]]`,
/// `[[Page#Titre]]` and `[texte](Page.md)`.
pub fn scan(chars: &[char], syntax: LinkSyntax) -> Vec<Found> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let found = if chars[i] == '[' && chars.get(i + 1) == Some(&'[') {
            org_link(chars, i, syntax)
        } else if syntax.markdown && chars[i] == '[' && (i == 0 || !matches!(chars[i - 1], '[' | '!' | '\\')) {
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

/// The link starting at `at`, if one does (see `scan`).
pub fn link_starting(chars: &[char], at: usize, syntax: LinkSyntax) -> Option<Found> {
    if chars.get(at) != Some(&'[') { return None; }
    if chars.get(at + 1) == Some(&'[') { return org_link(chars, at, syntax); }
    (syntax.markdown && (at == 0 || !matches!(chars[at - 1], '[' | '!' | '\\'))).then(|| markdown_link(chars, at)).flatten()
}

/// The link starting at `at` (`[[`), if it is closed (read by `link_at`).
fn org_link(chars: &[char], at: usize, syntax: LinkSyntax) -> Option<Found> {
    let link = link_at(chars, at)?;
    let ((ts, te), end) = (link.target, link.end);
    // `[[Page|texte]]`: the text after the bar
    let te = if syntax.pipe { (ts..te).find(|&j| chars[j] == '|').unwrap_or(te) } else { te };
    let raw: String = chars[ts..te].iter().collect();
    let trimmed = |a: usize, b: usize| {
        let a = a + chars[a..b].iter().take_while(|c| c.is_whitespace()).count();
        let b = b - chars[a..b].iter().rev().take_while(|c| c.is_whitespace()).count();
        (a < b).then_some((a, b))
    };
    let sep = raw.find(SEPARATOR).map(|b| ts + raw[..b].chars().count());
    let hash = (syntax.hash && sep.is_none() && !raw.contains("://"))
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
/// and tags (in Markdown, the closing `#`s of `## Titre ##`); spaces collapsed.
/// `None` when the line is not a headline of a page of `format`.
pub fn headline_title(line: &str, keywords: Option<&TodoKeywords>, format: Format) -> Option<String> {
    let chars: Vec<char> = line.chars().collect();
    let stars = format.heading_level(&chars)?;
    let start = keywords.and_then(|k| k.keyword_in(&chars, format)).map_or(stars, |k| k.end);
    let mut words: Vec<String> = chars[start..].iter().collect::<String>()
        .split_whitespace().map(String::from).collect();
    if format == Format::Markdown && words.last().is_some_and(|w| w.chars().all(|c| c == '#')) { words.pop(); }
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
pub fn headlines(text: &str, keywords: Option<&TodoKeywords>, format: Format) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for title in text.lines().filter_map(|l| headline_title(l, keywords, format)) {
        if !title.is_empty() && !out.contains(&title) { out.push(title); }
    }
    out
}

/// Where the place `place` of a link is in the page `text`, as a char range: the
/// line of the headline for `*Titre`; the `<<cible>>` for `cible`, failing that
/// a headline titled `cible`, as org-mode does. Case and accents are ignored.
/// `None` when it isn't there, and for a `#custom-id` (not supported).
pub fn find(text: &str, place: &str, keywords: Option<&TodoKeywords>, format: Format) -> Option<(usize, usize)> {
    if place.starts_with('#') { return None; }
    let headline = |title: &str| {
        let wanted = fold(&title.split_whitespace().collect::<Vec<_>>().join(" "));
        lines(text).find(|(_, line)| headline_title(line, keywords, format).is_some_and(|t| fold(&t) == wanted))
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
pub fn section_end(text: &str, at: usize, format: Format) -> usize {
    let level = |line: &str| format.heading_level_of(line);
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
        scan(&chars, if extended { LinkSyntax::ALL } else { LinkSyntax::NONE }).into_iter()
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
    fn each_syntax_on_its_own() {
        let text = "[[a|b]] [[c#d]] [e](f.md)";
        let chars: Vec<char> = text.chars().collect();
        let targets = |s| scan(&chars, s).into_iter().map(|f| f.target).collect::<Vec<_>>();
        assert_eq!(targets(LinkSyntax { pipe: true, ..LinkSyntax::NONE }), vec!["a", "c#d"]);
        assert_eq!(targets(LinkSyntax { hash: true, ..LinkSyntax::NONE }), vec!["a|b", "c::*d"]);
        assert_eq!(targets(LinkSyntax { markdown: true, ..LinkSyntax::NONE }), vec!["a|b", "c#d", "f.md"]);
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
        assert_eq!(headline_title("** TODO [#A] Faire [1/3] le pain :cuisine:", Some(&k), Format::Org).as_deref(), Some("Faire le pain"));
        assert_eq!(headline_title("* COMMENT Brouillon  [50%]", None, Format::Org).as_deref(), Some("Brouillon"));
        assert_eq!(headline_title("* TODO Titre", None, Format::Org).as_deref(), Some("TODO Titre"));
        assert_eq!(headline_title("*gras*", None, Format::Org), None);
        assert_eq!(headline_title("texte", None, Format::Org), None);
        assert_eq!(headline_title("*", None, Format::Org).as_deref(), Some(""));
    }

    #[test]
    fn finds_headlines_and_targets() {
        let text = "#+TITLE: R\n* Entrées\n** DONE Desserts :sucre:\nla <<pâte>> ici\n";
        let k = kw();
        assert_eq!(find(text, "*desserts", Some(&k), Format::Org), Some((21, 45)));
        assert_eq!(find(text, "*Entrees", Some(&k), Format::Org), Some((11, 20)));
        assert_eq!(find(text, "pate", Some(&k), Format::Org), Some((49, 57)));
        // No target of that name: a headline
        assert_eq!(find(text, "Desserts", Some(&k), Format::Org), Some((21, 45)));
        assert_eq!(find(text, "*Plats", Some(&k), Format::Org), None);
        assert_eq!(find(text, "#pate", Some(&k), Format::Org), None);
        assert_eq!(find("<<<radio>>> x", "radio", None, Format::Org), Some((0, 11)));
    }

    #[test]
    fn lists_headlines_once() {
        assert_eq!(headlines("* A\ntexte\n** TODO B :t:\n* A\n*\n", Some(&kw()), Format::Org), vec!["A", "B"]);
    }

    #[test]
    fn markdown_headlines() {
        let text = "# Recettes\ntexte\n## TODO Pâte à crêpes ##\nla pâte\n# Fin";
        let k = kw();
        assert_eq!(headlines(text, Some(&k), Format::Markdown), vec!["Recettes", "Pâte à crêpes", "Fin"]);
        assert_eq!(find(text, "*pate a crepes", Some(&k), Format::Markdown), Some((17, 41)));
        assert_eq!(section_end(text, 17, Format::Markdown), 49);
        assert!(headlines("* pas un titre\n#tag", None, Format::Markdown).is_empty());
    }

    #[test]
    fn sections_end_at_the_next_headline() {
        let text = "* A\na\n** B\nb\n* C\nc";
        assert_eq!(section_end(text, 0, Format::Org), 12);
        assert_eq!(section_end(text, 6, Format::Org), 12);
        assert_eq!(section_end(text, 4, Format::Org), 5);
        assert_eq!(section_end(text, 13, Format::Org), text.chars().count());
    }
}
