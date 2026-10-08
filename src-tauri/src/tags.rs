//! Org-mode tags: `#+FILETAGS: :a:b:` tags the whole page, `* Title :a:b:` a headline.
//! Like in org-mode, a headline inherits the tags of the page and of its parents.
//! Searches use org's match syntax: `projet+urgent-perso|idée`.

use std::collections::HashSet;

use orgamnesia_core::OrgTags;

/// Tags of a `#+FILETAGS:` line (`:a:b:`, or separated by spaces), `None` for other lines.
pub fn filetags(line: &str, org: OrgTags) -> Option<Vec<String>> {
    const KW: &str = "#+filetags:";
    let s = line.trim_start();
    let head = s.get(..KW.len())?;
    if !head.eq_ignore_ascii_case(KW) { return None; }
    Some(s[KW.len()..]
        .split(|c: char| c == ':' || c.is_whitespace())
        .filter(|t| !t.is_empty() && t.chars().all(|c| org.is_tag_char(c)))
        .map(str::to_string)
        .collect())
}

/// Level, title and tags of a headline line (`** Title :a:b:`).
pub fn headline(line: &str, org: OrgTags) -> Option<(usize, &str, Vec<String>)> {
    let level = line.chars().take_while(|&c| c == '*').count();
    let rest = &line[level..];
    if level == 0 || !(rest.is_empty() || rest.starts_with(' ')) { return None; }
    let (title, tags) = split_headline_tags(rest.trim(), org);
    Some((level, title, tags))
}

/// `"Title :a:b:"` → `("Title", ["a", "b"])`. The tags are the last word of the
/// headline, between colons.
pub fn split_headline_tags(s: &str, org: OrgTags) -> (&str, Vec<String>) {
    let s = s.trim_end();
    let start = s.char_indices().rev()
        .find(|(_, c)| c.is_whitespace())
        .map_or(0, |(i, c)| i + c.len_utf8());
    let block = &s[start..];
    let valid = block.len() >= 3 && block.starts_with(':') && block.ends_with(':')
        && block.chars().all(|c| c == ':' || org.is_tag_char(c));
    if !valid { return (s, vec![]); }
    let tags = block.split(':').filter(|t| !t.is_empty()).map(str::to_string).collect();
    (s[..start].trim_end(), tags)
}

/// Rename the tag `old` to `new` everywhere in `content` (its page was renamed):
/// in `#+FILETAGS:`, at the end of headlines and, with `in_text`, in the text.
/// `None` when nothing changed, or when `new` can't be a tag (spaces…).
pub fn rename(content: &str, old: &str, new: &str, ignore_case: bool, org: OrgTags, in_text: bool) -> Option<String> {
    if new.is_empty() || !new.chars().all(|c| org.is_tag_char(c)) { return None; }
    let norm = |s: &str| orgamnesia_core::names::page_key(s, ignore_case);
    let old = norm(old);
    let chars: Vec<char> = content.chars().collect();
    let mut out = String::with_capacity(content.len());
    let mut changed = false;
    let mut at = 0;
    for (a, b) in orgamnesia_core::hashtags::tag_ranges(&chars, org, in_text) {
        if norm(&chars[a..b].iter().collect::<String>()) != old { continue; }
        out.extend(&chars[at..a]);
        out.push_str(new);
        at = b;
        changed = true;
    }
    out.extend(&chars[at..]);
    changed.then_some(out)
}

/// Every tag written in `content` (once per page or headline carrying it).
pub fn written_tags(content: &str, org: OrgTags) -> Vec<String> {
    let mut page: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for line in content.lines() {
        if let Some(tags) = filetags(line, org) {
            for t in tags { if !page.contains(&t) { page.push(t); } }
        } else if let Some((_, _, tags)) = headline(line, org) {
            let mut seen = HashSet::new();
            out.extend(tags.into_iter().filter(|t| seen.insert(t.clone())));
        }
    }
    page.extend(out);
    page
}

/// A tag search in org-mode match syntax: groups separated by `|` (or); in a group,
/// `+tag` (or a bare `tag`) must be there, `-tag` must not. Tags are case-sensitive,
/// accents are ignored (`idee` finds `:idée:`). When tags may hold dashes, a `-`
/// inside a word belongs to the tag: `a-b` is the tag `a-b`, `a -b` excludes `b`.
#[derive(Debug, PartialEq)]
pub struct Query(Vec<Vec<(bool, String)>>);

impl Query {
    /// `None` when the query names no tag.
    pub fn parse(s: &str, org: OrgTags) -> Option<Self> {
        let mut groups = Vec::new();
        for part in s.split('|') {
            let mut group = Vec::new();
            let mut wanted = true;
            let mut chars = part.chars().peekable();
            while let Some(&c) = chars.peek() {
                if c != '-' && org.is_tag_char(c) {
                    let mut tag = String::new();
                    while let Some(&c) = chars.peek().filter(|c| org.is_tag_char(**c)) {
                        tag.push(c);
                        chars.next();
                    }
                    let tag = tag.trim_end_matches('-');
                    group.push((wanted, unaccent(tag)));
                    wanted = true;
                    continue;
                }
                match c {
                    '-' => wanted = false,
                    '+' | '&' => wanted = true,
                    _ => {}
                }
                chars.next();
            }
            if !group.is_empty() { groups.push(group); }
        }
        (!groups.is_empty()).then_some(Self(groups))
    }

    pub fn matches(&self, tags: &HashSet<&str>) -> bool {
        let tags: HashSet<String> = tags.iter().map(|t| unaccent(t)).collect();
        self.0.iter().any(|g| g.iter().all(|(wanted, t)| tags.contains(t) == *wanted))
    }
}

/// `s` without accents, case kept: "Idée" → "Idee".
fn unaccent(s: &str) -> String {
    s.chars().map(|c| {
        let plain = orgamnesia_core::names::fold_char(c);
        if c.is_uppercase() { plain.to_uppercase().next().unwrap_or(plain) } else { plain }
    }).collect()
}

/// The page itself (`heading: None`) or one of its headlines, matching a search.
#[derive(Debug, PartialEq)]
pub struct Hit {
    pub heading: Option<String>,
    pub level: usize,
    /// Line of the headline (0 for the page).
    pub line: usize,
    /// All its tags, inherited ones included.
    pub tags: Vec<String>,
}

fn as_set(v: &[String]) -> HashSet<&str> {
    v.iter().map(String::as_str).collect()
}

/// What matches `query` in `content`, with tag inheritance. When the page matches,
/// it stands for all its headlines; a matching headline stands for its sub-headlines.
pub fn search(content: &str, query: &Query, org: OrgTags) -> Vec<Hit> {
    let lines: Vec<&str> = content.lines().collect();
    let mut file_tags: Vec<String> = Vec::new();
    for t in lines.iter().filter_map(|l| filetags(l, org)).flatten() {
        if !file_tags.contains(&t) { file_tags.push(t); }
    }
    if query.matches(&as_set(&file_tags)) {
        return vec![Hit { heading: None, level: 0, line: 0, tags: file_tags }];
    }

    let mut hits = Vec::new();
    // Ancestors of the current headline: (level, own tags)
    let mut parents: Vec<(usize, Vec<String>)> = Vec::new();
    // Level of the matching headline whose subtree we are in
    let mut matched: Option<usize> = None;
    for (n, line) in lines.iter().enumerate() {
        let Some((level, title, own)) = headline(line, org) else { continue };
        if matched.is_some_and(|m| level > m) { continue; }
        matched = None;
        while parents.last().is_some_and(|(l, _)| *l >= level) { parents.pop(); }
        let mut all = file_tags.clone();
        for t in parents.iter().flat_map(|(_, t)| t).chain(&own) {
            if !all.contains(t) { all.push(t.clone()); }
        }
        if query.matches(&as_set(&all)) {
            hits.push(Hit { heading: Some(title.to_string()), level, line: n, tags: all });
            matched = Some(level);
        }
        parents.push((level, own));
    }
    hits
}

#[cfg(test)]
mod tests {
    #[test]
    fn search_ignores_accents_not_case() {
        let content = "* Note :idée:\n* Autre :Idee:\n";
        let q = Query::parse("idee", O).unwrap();
        let hits: Vec<usize> = search(content, &q, O).iter().map(|h| h.line).collect();
        assert_eq!(hits, vec![0]);
    }

    use super::*;

    const O: OrgTags = OrgTags { links: true, dashes: false };

    fn q(s: &str) -> Query { Query::parse(s, O).unwrap() }

    #[test]
    fn filetags_line() {
        assert_eq!(filetags("#+FILETAGS: :projet:rust:", O), Some(vec!["projet".into(), "rust".into()]));
        assert_eq!(filetags("#+filetags: a b", O), Some(vec!["a".into(), "b".into()]));
        assert_eq!(filetags("#+TITLE: x", O), None);
    }

    #[test]
    fn headline_tags() {
        assert_eq!(split_headline_tags("Réunion :urgent:@bureau:", O), ("Réunion", vec!["urgent".into(), "@bureau".into()]));
        assert_eq!(split_headline_tags(":seul:", O), ("", vec!["seul".into()]));
        // Not tags: glued to the title, a dash, no closing colon
        assert_eq!(split_headline_tags("Note:a:", O).1, Vec::<String>::new());
        assert_eq!(split_headline_tags("T :mon-tag:", O).1, Vec::<String>::new());
        assert_eq!(split_headline_tags("T :a:b", O).1, Vec::<String>::new());
        assert_eq!(headline("** T :a:", O).map(|h| h.0), Some(2));
        assert_eq!(headline("**gras**", O), None);
    }

    #[test]
    fn query_syntax() {
        let query = q("projet+urgent-perso|idée");
        let set = |v: &[&'static str]| v.iter().copied().collect::<HashSet<&str>>();
        assert!(query.matches(&set(&["projet", "urgent"])));
        assert!(!query.matches(&set(&["projet", "urgent", "perso"])));
        assert!(query.matches(&set(&["idée"])));
        assert!(!query.matches(&set(&["projet"])));
        assert!(!q("Projet").matches(&set(&["projet"])));
        assert_eq!(Query::parse(" + | ", O), None);
    }

    #[test]
    fn search_with_inheritance() {
        let page = "#+FILETAGS: :projet:\n* A :urgent:\n** A1\n* B\n** B1 :urgent:\n*** B11\n";
        let hits = search(page, &q("projet+urgent"), O);
        let found: Vec<_> = hits.iter().map(|h| (h.heading.clone().unwrap(), h.line)).collect();
        assert_eq!(found, vec![("A".to_string(), 1), ("B1".to_string(), 4)]);
        assert_eq!(hits[0].tags, vec!["projet", "urgent"]);
        // The whole page matches: a single hit
        assert_eq!(search(page, &q("projet"), O), vec![Hit { heading: None, level: 0, line: 0, tags: vec!["projet".into()] }]);
        // Siblings don't inherit from each other
        assert!(search("* A :x:\n* B\n", &q("x"), O).iter().all(|h| h.heading.as_deref() == Some("A")));
    }

    #[test]
    fn rename_tags() {
        let t = "#+FILETAGS: :Old:x:\n* Old :old:y:\ntexte :old:\n* T :older:\n";
        assert_eq!(rename(t, "old", "Neuf", true, O, false).unwrap(),
            "#+FILETAGS: :Neuf:x:\n* Old :Neuf:y:\ntexte :old:\n* T :older:\n");
        assert_eq!(rename(t, "old", "Neuf", false, O, false).unwrap(),
            "#+FILETAGS: :Old:x:\n* Old :Neuf:y:\ntexte :old:\n* T :older:\n");
        assert_eq!(rename(t, "old", "deux mots", true, O, false), None);
        assert_eq!(rename(t, "absent", "x", true, O, false), None);
    }

    #[test]
    fn rename_tags_in_text_and_with_dashes() {
        let t = "* Old :old:\ntexte :old: et :x:old:\n";
        assert_eq!(rename(t, "old", "Neuf", false, O, true).unwrap(), "* Old :Neuf:\ntexte :Neuf: et :x:Neuf:\n");
        assert_eq!(rename(t, "old", "neuf-2", false, O, true), None);
        let dashes = OrgTags { dashes: true, ..O };
        assert_eq!(rename(t, "old", "neuf-2", false, dashes, false).unwrap(), "* Old :neuf-2:\ntexte :old: et :x:old:\n");
    }

    #[test]
    fn dashes_in_tags_and_queries() {
        let dashes = OrgTags { dashes: true, ..O };
        assert_eq!(split_headline_tags("T :mon-tag:", dashes).1, vec!["mon-tag".to_string()]);
        let set = |v: &[&'static str]| v.iter().copied().collect::<HashSet<&str>>();
        let query = Query::parse("mon-tag -perso", dashes).unwrap();
        assert!(query.matches(&set(&["mon-tag"])));
        assert!(!query.matches(&set(&["mon-tag", "perso"])));
        assert!(!q("mon-tag").matches(&set(&["mon-tag"])));
    }

    #[test]
    fn written_tags_once_per_carrier() {
        assert_eq!(written_tags("#+FILETAGS: :a:\n* T :a:b:b:\n* U :b:\n", O), vec!["a", "a", "b", "b"]);
    }
}
