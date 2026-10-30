//! Org-mode tags and `#tags` as the editor handles them: the tag under the caret
//! or being typed, and a file tag added to a page.

use orgamnesia_core::hashtags::{filetags_value_start, tag_ranges};
use orgamnesia_core::org::{line_end, line_start};
use orgamnesia_core::{Hashtags, OrgTags};

/// The `#tag` being typed when the caret is at `caret`: returns the index of the
/// `#` and the part of the tag already typed (possibly empty, right after `#`).
pub fn hashtag_prefix(chars: &[char], caret: usize, tags: Hashtags) -> Option<(usize, String)> {
    if tags == Hashtags::Off || chars.get(caret).is_some_and(|&c| tags.is_tag_char(c)) { return None; }
    let mut start = caret;
    while start > 0 && tags.is_tag_char(chars[start - 1]) { start -= 1; }
    let hash = start.checked_sub(1)?;
    if chars[hash] != '#' { return None; }
    if hash > 0 && !(chars[hash - 1].is_whitespace() || "([{\"'".contains(chars[hash - 1])) { return None; }
    if start < caret && !chars[start].is_alphanumeric() { return None; }
    Some((hash, chars[start..caret].iter().collect()))
}

/// The org-mode tag under `pos`: in the `:a:b:` of a headline, a `#+FILETAGS:` line
/// or the text (`voir :exemple:`).
pub fn org_tag_at(chars: &[char], pos: usize, org: OrgTags) -> Option<String> {
    tag_ranges(chars, org, true).into_iter()
        .find(|&(a, b)| a <= pos && pos <= b)
        .map(|(a, b)| chars[a..b].iter().collect())
}

/// The org-mode tag being typed when the caret is at `caret`: after `:` in the tags
/// of a headline (`* Title :pro`), in a `#+FILETAGS:` line, or as `:pro` in the
/// text (a character typed at least). Returns the index
/// where the tag starts and the part already typed (possibly empty).
pub fn org_tag_prefix(chars: &[char], caret: usize, org: OrgTags) -> Option<(usize, String)> {
    let is_org_tag_char = |c: char| org.is_tag_char(c);
    if chars.get(caret).is_some_and(|&c| is_org_tag_char(c)) { return None; }
    let (ls, le) = (line_start(chars, caret), line_end(chars, caret));
    let mut start = caret;
    while start > ls && is_org_tag_char(chars[start - 1]) { start -= 1; }
    let prefix: String = chars[start..caret].iter().collect();
    let line = &chars[ls..le];
    if let Some(v) = filetags_value_start(line) {
        let ok = start - ls >= v && (start - ls == v || chars[start - 1] == ':' || chars[start - 1].is_whitespace());
        return ok.then_some((start, prefix));
    }
    let stars = line.iter().take_while(|&&c| c == '*').count();
    if stars == 0 || line.get(stars) != Some(&' ') {
        // In the text: a word starting with `:`, at the start of the line or after a
        // space, once a character of the tag is typed (`10:30`, `http://` are not tags)
        let ok = !prefix.is_empty() && start > ls && chars[start - 1] == ':'
            && (start - 1 == ls || chars[start - 2].is_whitespace());
        return ok.then_some((start, prefix));
    }
    // Headline: the caret is in its last word, which starts with `:` after a space
    let after: String = chars[caret..le].iter().collect();
    if after.trim_end().chars().any(|c| c != ':' && !is_org_tag_char(c)) { return None; }
    if start == 0 || chars[start - 1] != ':' { return None; }
    let mut b = start - 1;
    while b > ls + stars && (chars[b - 1] == ':' || is_org_tag_char(chars[b - 1])) { b -= 1; }
    (chars[b] == ':' && chars[b - 1].is_whitespace()).then_some((start, prefix))
}

/// `content` with the file tag `tag` added to its `#+FILETAGS:` line, which is
/// created after the leading `#+` keywords (the top of the page) when missing.
/// `None` when the page already has that file tag.
pub fn add_filetag(content: &str, tag: &str) -> Option<String> {
    let mut lines: Vec<String> = content.split('\n').map(String::from).collect();
    let found = lines.iter().position(|l| filetags_value_start(&l.chars().collect::<Vec<_>>()).is_some());
    match found {
        Some(i) => {
            let chars: Vec<char> = lines[i].chars().collect();
            let v = filetags_value_start(&chars)?;
            let head: String = chars[..v].iter().collect();
            let value: String = chars[v..].iter().collect();
            if value.split(':').any(|t| t.trim().eq_ignore_ascii_case(tag)) {
                return None;
            }
            let value = value.trim_end();
            lines[i] = if value.trim().is_empty() {
                format!("{head} :{tag}:")
            } else if value.ends_with(':') {
                format!("{head}{value}{tag}:")
            } else {
                format!("{head}{value} :{tag}:")
            };
        }
        None => {
            let at = lines.iter().take_while(|l| l.trim_start().starts_with("#+")).count();
            lines.insert(at, format!("#+FILETAGS: :{tag}:"));
        }
    }
    Some(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(s: &str) -> Vec<char> { s.chars().collect() }

    const ORG: OrgTags = OrgTags { links: true, dashes: false };

    #[test]
    fn filetag_added() {
        assert_eq!(add_filetag("* Page\n", "pro").as_deref(), Some("#+FILETAGS: :pro:\n* Page\n"));
        assert_eq!(add_filetag("", "pro").as_deref(), Some("#+FILETAGS: :pro:\n"));
        assert_eq!(add_filetag("#+title: P\n* P", "pro").as_deref(), Some("#+title: P\n#+FILETAGS: :pro:\n* P"));
        assert_eq!(add_filetag("#+FILETAGS: :a:\nx", "b").as_deref(), Some("#+FILETAGS: :a:b:\nx"));
        assert_eq!(add_filetag("#+filetags:\nx", "b").as_deref(), Some("#+filetags: :b:\nx"));
        assert_eq!(add_filetag("#+filetags: :a:  \nx", "b").as_deref(), Some("#+filetags: :a:b:\nx"));
        assert_eq!(add_filetag("x\n#+filetags: :A:\n", "a"), None);
    }

    #[test]
    fn hashtag_prefix_while_typing() {
        let d = Hashtags::Dashes;
        assert_eq!(hashtag_prefix(&c("see #"), 5, d), Some((4, String::new())));
        assert_eq!(hashtag_prefix(&c("see #ru"), 7, d), Some((4, "ru".into())));
        assert_eq!(hashtag_prefix(&c("#ru"), 3, d), Some((0, "ru".into())));
        assert_eq!(hashtag_prefix(&c("(#ru)"), 4, d), Some((1, "ru".into())));
        // Caret in the middle of a tag, `#` glued to a word, heading-like `#_`
        assert_eq!(hashtag_prefix(&c("#rust"), 3, d), None);
        assert_eq!(hashtag_prefix(&c("a#ru"), 4, d), None);
        assert_eq!(hashtag_prefix(&c("#_x"), 3, d), None);
        assert_eq!(hashtag_prefix(&c("no tag"), 6, d), None);
        // A dash ends an org-mode style tag
        assert_eq!(hashtag_prefix(&c("#mon-ta"), 7, d), Some((0, "mon-ta".into())));
        assert_eq!(hashtag_prefix(&c("#mon-ta"), 7, Hashtags::Org), None);
        assert_eq!(hashtag_prefix(&c("#ru"), 3, Hashtags::Off), None);
    }

    #[test]
    fn org_tags_under_the_caret() {
        let t = c("* Titre :projet:urgent:\n#+FILETAGS: :a:b:\ntexte :x:");
        assert_eq!(org_tag_at(&t, 10, ORG), Some("projet".into()));
        assert_eq!(org_tag_at(&t, 20, ORG), Some("urgent".into()));
        assert_eq!(org_tag_at(&t, 3, ORG), None);            // in the title
        assert_eq!(org_tag_at(&t, 38, ORG), Some("a".into()));
        assert_eq!(org_tag_at(&t, 49, ORG), Some("x".into())); // in the text
        assert_eq!(org_tag_at(&t, 44, ORG), None);
    }

    #[test]
    fn org_tag_prefix_while_typing() {
        // In the text
        assert_eq!(org_tag_prefix(&c("voir :pro"), 9, ORG), Some((6, "pro".into())));
        assert_eq!(org_tag_prefix(&c(":pro"), 4, ORG), Some((1, "pro".into())));
        assert_eq!(org_tag_prefix(&c("voir :"), 6, ORG), None);
        assert_eq!(org_tag_prefix(&c("à 10:30"), 7, ORG), None);
        assert_eq!(org_tag_prefix(&c("http://x"), 8, ORG), None);
        assert_eq!(org_tag_prefix(&c("* T :pro"), 8, ORG), Some((5, "pro".into())));
        assert_eq!(org_tag_prefix(&c("* T :"), 5, ORG), Some((5, String::new())));
        assert_eq!(org_tag_prefix(&c("* T :a:b"), 8, ORG), Some((7, "b".into())));
        assert_eq!(org_tag_prefix(&c("* T :pr:"), 7, ORG), Some((5, "pr".into())));
        assert_eq!(org_tag_prefix(&c("#+FILETAGS: :a"), 14, ORG), Some((13, "a".into())));
        assert_eq!(org_tag_prefix(&c("#+filetags: "), 12, ORG), Some((12, String::new())));
        // `Note:` glued to a word, a caret before more words
        assert_eq!(org_tag_prefix(&c("* Note:"), 7, ORG), None);
        assert_eq!(org_tag_prefix(&c("texte :a"), 8, ORG), Some((7, "a".into())));
        assert_eq!(org_tag_prefix(&c("* T :a suite"), 6, ORG), None);
        assert_eq!(org_tag_prefix(&c("#+TITLE: x"), 10, ORG), None);
        // A dash ends the tag, unless allowed
        assert_eq!(org_tag_prefix(&c("voir :mon-t"), 11, ORG), None);
        let dashes = OrgTags { dashes: true, ..ORG };
        assert_eq!(org_tag_prefix(&c("voir :mon-t"), 11, dashes), Some((6, "mon-t".into())));
    }
}
