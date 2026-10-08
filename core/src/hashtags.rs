//! How `#tags` and org-mode `:tags:` are read, for the link index (backend) and
//! the editor (interface).

/// How `#tags` in the text are read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hashtags {
    /// `#tags` are plain text.
    Off,
    /// `#tags` are links to pages; letters, digits and `_` (like org-mode tags).
    Org,
    /// Same, also allowing `-` (`#mon-tag`), which org-mode tags don't.
    Dashes,
}

impl Hashtags {
    pub fn new(enabled: bool, dashes: bool) -> Self {
        match (enabled, dashes) {
            (false, _) => Self::Off,
            (true, false) => Self::Org,
            (true, true) => Self::Dashes,
        }
    }

    pub fn is_tag_char(self, c: char) -> bool {
        c.is_alphanumeric() || c == '_' || (c == '-' && self == Self::Dashes)
    }
}

/// A `#hashtag` starting at `i` (preceded by a space or an opening bracket, followed by a
/// letter or digit — so `#+TITLE`, `# comment` and `a#b` are not tags), unless `tags` is `Off`.
/// Returns the tag name (without `#`) and the index just after it.
pub fn hashtag_at(chars: &[char], i: usize, tags: Hashtags) -> Option<(String, usize)> {
    if tags == Hashtags::Off || chars.get(i) != Some(&'#') { return None; }
    if i > 0 && !(chars[i - 1].is_whitespace() || "([{\"'".contains(chars[i - 1])) { return None; }
    let mut end = i + 1;
    while end < chars.len() && tags.is_tag_char(chars[end]) { end += 1; }
    while end > i + 1 && chars[end - 1] == '-' { end -= 1; }
    if end == i + 1 || !chars[i + 1].is_alphanumeric() { return None; }
    Some((chars[i + 1..end].iter().collect(), end))
}

/// How org-mode `:tags:` are read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrgTags {
    /// `:tags:` are links to pages: in `#+FILETAGS:`, at the end of a headline and
    /// in the text (`voir :exemple:`).
    pub links: bool,
    /// Allow `-` in tags (`:mon-tag:`), which org-mode doesn't.
    pub dashes: bool,
}

impl Default for OrgTags {
    fn default() -> Self {
        Self { links: true, dashes: false }
    }
}

impl OrgTags {
    /// Letters, digits, `_ @ # %` (as in org-mode), and `-` when allowed.
    pub fn is_tag_char(self, c: char) -> bool {
        c.is_alphanumeric() || matches!(c, '_' | '@' | '#' | '%') || (c == '-' && self.dashes)
    }
}

/// How the tags in the text are read: `#tags` and org-mode `:tags:`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TagSyntax {
    pub hashtags: Hashtags,
    pub org: OrgTags,
}

impl From<Hashtags> for TagSyntax {
    fn from(hashtags: Hashtags) -> Self {
        Self { hashtags, org: OrgTags::default() }
    }
}

/// In a `#+FILETAGS:` line, the index where its value starts (after the keyword).
pub fn filetags_value_start(line: &[char]) -> Option<usize> {
    const KW: &str = "#+filetags:";
    let indent = line.iter().take_while(|c| c.is_whitespace()).count();
    let n = KW.chars().count();
    let head: String = line.get(indent..indent + n)?.iter().collect();
    head.eq_ignore_ascii_case(KW).then_some(indent + n)
}

/// In a headline, the range of its `:a:b:` tags (the last word, between colons).
pub fn headline_tags_range(line: &[char], org: OrgTags) -> Option<(usize, usize)> {
    let stars = line.iter().take_while(|&&c| c == '*').count();
    if stars == 0 || line.get(stars) != Some(&' ') { return None; }
    let mut end = line.len();
    while end > stars && line[end - 1].is_whitespace() { end -= 1; }
    let mut start = end;
    while start > stars && !line[start - 1].is_whitespace() { start -= 1; }
    let block = &line[start..end];
    let valid = block.len() >= 3 && block[0] == ':' && block[block.len() - 1] == ':'
        && block.iter().all(|&c| c == ':' || org.is_tag_char(c));
    valid.then_some((start, end))
}

/// Tags written in the text as `:a:` or `:a:b:` starting at `i` (preceded by a space or an
/// opening bracket, followed by a space or a punctuation mark — so `10:30:` and `http://`
/// are not tags). Returns the char range of each tag and the index just after the last `:`.
pub fn text_tags_at(chars: &[char], i: usize, org: OrgTags) -> Option<(Vec<(usize, usize)>, usize)> {
    if chars.get(i) != Some(&':') { return None; }
    if i > 0 && !(chars[i - 1].is_whitespace() || "([{\"'".contains(chars[i - 1])) { return None; }
    let mut tags = Vec::new();
    let mut j = i + 1;
    loop {
        let a = j;
        while j < chars.len() && org.is_tag_char(chars[j]) { j += 1; }
        if j == a || chars.get(j) != Some(&':') { return None; }
        tags.push((a, j));
        j += 1;
        if !chars.get(j).is_some_and(|&c| org.is_tag_char(c)) { break; }
    }
    let ends = chars.get(j).is_none_or(|&c| c.is_whitespace() || ".,;!?)]}\"'".contains(c));
    ends.then_some((tags, j))
}

/// Char ranges of the org-mode tags in `chars`: in `#+FILETAGS:` lines, at the end of
/// headlines and, with `in_text`, those written in the text (`voir :exemple:`).
pub fn tag_ranges(chars: &[char], org: OrgTags, in_text: bool) -> Vec<(usize, usize)> {
    ranges(chars, org, true, in_text)
}

/// Char ranges of the org-mode tags written in the text only (`voir :exemple:`).
pub fn text_tag_ranges(chars: &[char], org: OrgTags) -> Vec<(usize, usize)> {
    ranges(chars, org, false, true)
}

fn ranges(chars: &[char], org: OrgTags, zones: bool, in_text: bool) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut ls = 0;
    while ls <= chars.len() {
        let le = chars[ls..].iter().position(|&c| c == '\n').map_or(chars.len(), |p| ls + p);
        let line = &chars[ls..le];
        let filetags = filetags_value_start(line).map(|v| (v, line.len()));
        let zone = filetags.or_else(|| headline_tags_range(line, org));
        if let Some((zs, ze)) = zone.filter(|_| zones) {
            let mut a = zs;
            while a < ze {
                let mut b = a;
                while b < ze && org.is_tag_char(line[b]) { b += 1; }
                if b > a { out.push((ls + a, ls + b)); }
                a = b + 1;
            }
        }
        if in_text && filetags.is_none() {
            // The text of the line, before the tags of a headline
            let text = &line[..zone.map_or(line.len(), |(zs, _)| zs)];
            let mut i = 0;
            while i < text.len() {
                match text_tags_at(text, i, org) {
                    Some((tags, end)) => {
                        out.extend(tags.into_iter().map(|(a, b)| (ls + a, ls + b)));
                        i = end;
                    }
                    None => i += 1,
                }
            }
        }
        ls = le + 1;
    }
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(s: &str, tags: Hashtags) -> Option<String> {
        let chars: Vec<char> = s.chars().collect();
        hashtag_at(&chars, s.find('#')?, tags).map(|(t, _)| t)
    }

    #[test]
    fn reads_tags_per_setting() {
        assert_eq!(tag("a #mon-tag", Hashtags::Dashes).as_deref(), Some("mon-tag"));
        assert_eq!(tag("a #mon-tag", Hashtags::Org).as_deref(), Some("mon"));
        assert_eq!(tag("a #mon-tag", Hashtags::Off), None);
        assert_eq!(tag("#fin- x", Hashtags::Dashes).as_deref(), Some("fin"));
    }

    fn ranges(s: &str, org: OrgTags, in_text: bool) -> Vec<String> {
        let chars: Vec<char> = s.chars().collect();
        tag_ranges(&chars, org, in_text).into_iter().map(|(a, b)| chars[a..b].iter().collect()).collect()
    }

    #[test]
    fn org_tags_in_text() {
        let org = OrgTags::default();
        let t = "#+FILETAGS: :a:b:\n* T :x: titre :h:\nvoir :ex: et (:y:z:), 10:30: http://u :mon-tag: fin:no:";
        assert_eq!(ranges(t, org, true), vec!["a", "b", "x", "h", "ex", "y", "z"]);
        assert_eq!(ranges(t, org, false), vec!["a", "b", "h"]);
        let dashes = OrgTags { dashes: true, ..org };
        assert_eq!(ranges(t, dashes, true), vec!["a", "b", "x", "h", "ex", "y", "z", "mon-tag"]);
        assert_eq!(ranges("* T :mon-tag:", org, false), Vec::<String>::new());
        assert_eq!(ranges("* T :mon-tag:", dashes, false), vec!["mon-tag"]);
    }

    #[test]
    fn ignores_org_keywords_and_words() {
        assert_eq!(tag("#+TITLE: x", Hashtags::Dashes), None);
        assert_eq!(tag("# comment", Hashtags::Dashes), None);
        assert_eq!(tag("a#b", Hashtags::Dashes), None);
    }
}
