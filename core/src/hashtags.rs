//! How `#tags` are read, for the link index (backend) and the editor (interface).

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

    #[test]
    fn ignores_org_keywords_and_words() {
        assert_eq!(tag("#+TITLE: x", Hashtags::Dashes), None);
        assert_eq!(tag("# comment", Hashtags::Dashes), None);
        assert_eq!(tag("a#b", Hashtags::Dashes), None);
    }
}
