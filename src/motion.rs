//! Pure cursor-motion helpers (Emacs-style sentence, paragraph and heading moves).
//! Positions are char indexes into `chars`.

fn line_start(chars: &[char], pos: usize) -> usize {
    chars[..pos].iter().rposition(|&c| c == '\n').map(|i| i + 1).unwrap_or(0)
}

fn line_end(chars: &[char], pos: usize) -> usize {
    chars[pos..].iter().position(|&c| c == '\n').map(|i| pos + i).unwrap_or(chars.len())
}

/// `[start..end)` holds only whitespace.
fn is_blank(chars: &[char], start: usize, end: usize) -> bool {
    chars[start..end].iter().all(|c| c.is_whitespace())
}

fn is_terminator(c: char) -> bool {
    matches!(c, '.' | '!' | '?')
}

/// First non-blank char of the line (Emacs `back-to-indentation`, `M-m`).
pub fn back_to_indentation(chars: &[char], pos: usize) -> usize {
    let mut i = line_start(chars, pos);
    while i < chars.len() && (chars[i] == ' ' || chars[i] == '\t') { i += 1; }
    i
}

/// End of the current sentence (just after its `.`, `!` or `?`), or of the
/// paragraph text when it ends first (Emacs `forward-sentence`, `M-e`).
pub fn forward_sentence(chars: &[char], pos: usize) -> usize {
    let n = chars.len();
    let mut i = pos;
    while i < n && chars[i].is_whitespace() { i += 1; }
    while i < n {
        if is_terminator(chars[i]) && (i + 1 == n || chars[i + 1].is_whitespace()) {
            return i + 1;
        }
        if chars[i] == '\n' && i + 1 < n && chars[i + 1] == '\n' {
            return i;
        }
        i += 1;
    }
    n
}

/// Start of the current sentence, or of the previous one when already at a start
/// (Emacs `backward-sentence`, `M-a`).
pub fn backward_sentence(chars: &[char], pos: usize) -> usize {
    let mut j = pos;
    while j > 0 && chars[j - 1].is_whitespace() { j -= 1; }
    if j > 0 && is_terminator(chars[j - 1]) { j -= 1; }
    let mut start = 0;
    let mut i = j;
    while i > 0 {
        let boundary = (is_terminator(chars[i - 1]) && chars[i].is_whitespace())
            || (chars[i - 1] == '\n' && chars[i] == '\n');
        if i < chars.len() && boundary { start = i; break; }
        i -= 1;
    }
    while start < pos && start < chars.len() && chars[start].is_whitespace() { start += 1; }
    start
}

/// Blank line that ends the current paragraph (Emacs `forward-paragraph`).
pub fn forward_paragraph(chars: &[char], pos: usize) -> usize {
    let n = chars.len();
    let mut ls = line_start(chars, pos);
    // skip blank lines, land on the first line of text
    while ls < n {
        let e = line_end(chars, ls);
        if !is_blank(chars, ls, e) { break; }
        if e >= n { return n; }
        ls = e + 1;
    }
    // walk down to the next blank line
    loop {
        let e = line_end(chars, ls);
        if e >= n { return n; }
        ls = e + 1;
        if ls >= n { return n; }
        if is_blank(chars, ls, line_end(chars, ls)) { return ls; }
    }
}

/// Blank line that starts the current paragraph (Emacs `backward-paragraph`).
pub fn backward_paragraph(chars: &[char], pos: usize) -> usize {
    let mut ls = line_start(chars, pos);
    // on a blank line: first hop up to the text of the previous paragraph
    if is_blank(chars, ls, line_end(chars, ls)) {
        loop {
            if ls == 0 { return 0; }
            ls = line_start(chars, ls - 1);
            if !is_blank(chars, ls, line_end(chars, ls)) { break; }
        }
    }
    // walk up to the blank line above this paragraph
    loop {
        if ls == 0 { return 0; }
        let prev = line_start(chars, ls - 1);
        if is_blank(chars, prev, line_end(chars, prev)) { return prev; }
        ls = prev;
    }
}

/// `*`… followed by a space (or alone on the line): an org heading line.
fn is_heading(chars: &[char], start: usize, end: usize) -> bool {
    let stars = chars[start..end].iter().take_while(|&&c| c == '*').count();
    stars > 0 && (start + stars == end || chars[start + stars] == ' ')
}

/// Start of the next heading line after the current line (org `C-c C-n`).
pub fn next_heading(chars: &[char], pos: usize) -> usize {
    let n = chars.len();
    let mut e = line_end(chars, pos);
    while e < n {
        let s = e + 1;
        let le = line_end(chars, s);
        if is_heading(chars, s, le) { return s; }
        e = le;
    }
    pos
}

/// Start of the closest heading line before the current line (org `C-c C-p`).
pub fn previous_heading(chars: &[char], pos: usize) -> usize {
    let mut s = line_start(chars, pos);
    while s > 0 {
        s = line_start(chars, s - 1);
        if is_heading(chars, s, line_end(chars, s)) { return s; }
    }
    pos
}

pub use orgamnesia_core::hashtags::{hashtag_at, Hashtags};

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

/// Characters allowed in an org-mode tag (same as the backend): letters, digits, `_ @ # %`.
pub fn is_org_tag_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '@' | '#' | '%')
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
pub fn headline_tags_range(line: &[char]) -> Option<(usize, usize)> {
    let stars = line.iter().take_while(|&&c| c == '*').count();
    if stars == 0 || line.get(stars) != Some(&' ') { return None; }
    let mut end = line.len();
    while end > stars && line[end - 1].is_whitespace() { end -= 1; }
    let mut start = end;
    while start > stars && !line[start - 1].is_whitespace() { start -= 1; }
    let block = &line[start..end];
    let valid = block.len() >= 3 && block[0] == ':' && block[block.len() - 1] == ':'
        && block.iter().all(|&c| c == ':' || is_org_tag_char(c));
    valid.then_some((start, end))
}

/// The org-mode tag under `pos`: in the `:a:b:` of a headline or a `#+FILETAGS:` line.
pub fn org_tag_at(chars: &[char], pos: usize) -> Option<String> {
    let (ls, le) = (line_start(chars, pos), line_end(chars, pos));
    let line = &chars[ls..le];
    let (zs, ze) = filetags_value_start(line).map(|v| (v, line.len()))
        .or_else(|| headline_tags_range(line))?;
    let p = pos - ls;
    if p < zs || p > ze { return None; }
    let mut a = p;
    while a > zs && is_org_tag_char(line[a - 1]) { a -= 1; }
    let mut b = p;
    while b < ze && is_org_tag_char(line[b]) { b += 1; }
    (a < b).then(|| line[a..b].iter().collect())
}

/// The org-mode tag being typed when the caret is at `caret`: after `:` in the tags
/// of a headline (`* Title :pro`), in a `#+FILETAGS:` line, or as `:pro` in the
/// text (a character typed at least). Returns the index
/// where the tag starts and the part already typed (possibly empty).
pub fn org_tag_prefix(chars: &[char], caret: usize) -> Option<(usize, String)> {
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

/// The `[[target` being typed when the caret is at `caret` (inside `[[…`, before
/// any `]` or `[`, on the same line): returns the index where the target starts
/// and the part already typed.
pub fn link_prefix(chars: &[char], caret: usize) -> Option<(usize, String)> {
    let mut start = caret;
    while start > 0 && !matches!(chars[start - 1], '[' | ']' | '\n') { start -= 1; }
    if start < 2 || chars[start - 1] != '[' || chars[start - 2] != '[' { return None; }
    Some((start, chars[start..caret].iter().collect()))
}

/// Page names completing `prefix` (ignoring case and accents): names starting with it first
/// (shortest first), then names containing it; the exact name is left out. With
/// `tag: Some(_)`, only names usable as such a `#tag` are kept.
pub fn complete_page(names: &[String], prefix: &str, tag: Option<Hashtags>, limit: usize) -> Vec<String> {
    let p = orgamnesia_core::names::fold(prefix.trim());
    let mut starts = Vec::new();
    let mut contains = Vec::new();
    for name in names {
        let valid = tag.is_none_or(|t| name.chars().next().is_some_and(char::is_alphanumeric)
            && name.chars().all(|c| t.is_tag_char(c)));
        let n = orgamnesia_core::names::fold(name);
        if !valid || n == p { continue; }
        if n.starts_with(&p) { starts.push(name.clone()); }
        else if n.contains(&p) { contains.push(name.clone()); }
    }
    starts.sort_by_key(|n| (n.chars().count(), n.to_lowercase()));
    contains.sort_by_key(|n| n.to_lowercase());
    starts.into_iter().chain(contains).take(limit).collect()
}

/// Char range of the target text of the first `[[target]]` / `[[target][label]]`
/// link naming `target`.
pub fn find_link(chars: &[char], target: &str, ignore_case: bool, hashtags: Hashtags) -> Option<(usize, usize)> {
    let norm = |s: &str| orgamnesia_core::names::page_key(s.trim(), ignore_case);
    let wanted = norm(target);
    let mut i = 0;
    while i < chars.len() {
        if let Some((tag, end)) = hashtag_at(chars, i, hashtags) {
            if norm(&tag) == wanted { return Some((i + 1, end)); }
            i = end;
        } else if i + 1 < chars.len() && chars[i] == '[' && chars[i + 1] == '[' {
            let start = i + 2;
            let mut end = start;
            while end < chars.len() && chars[end] != ']' && chars[end] != '[' { end += 1; }
            let raw: String = chars[start..end].iter().collect();
            if norm(&raw) == wanted {
                let lead = raw.chars().take_while(|c| c.is_whitespace()).count();
                let trail = raw.chars().rev().take_while(|c| c.is_whitespace()).count();
                return Some((start + lead, end - trail));
            }
            i = end;
        } else {
            i += 1;
        }
    }
    // Then the org-mode tags (`#+FILETAGS:`, `* Title :tag:`)
    let mut ls = 0;
    while ls <= chars.len() {
        let le = line_end(chars, ls);
        let line = &chars[ls..le];
        if let Some((zs, ze)) = filetags_value_start(line).map(|v| (v, line.len())).or_else(|| headline_tags_range(line)) {
            let mut a = zs;
            while a < ze {
                let mut b = a;
                while b < ze && is_org_tag_char(line[b]) { b += 1; }
                if b > a && norm(&line[a..b].iter().collect::<String>()) == wanted { return Some((ls + a, ls + b)); }
                a = b + 1;
            }
        }
        ls = le + 1;
    }
    None
}

/// `content` with the file tag `tag` added to its `#+filetags:` line, which is
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
            lines.insert(at, format!("#+filetags: :{tag}:"));
        }
    }
    Some(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filetag_added() {
        assert_eq!(add_filetag("* Page\n", "pro").as_deref(), Some("#+filetags: :pro:\n* Page\n"));
        assert_eq!(add_filetag("", "pro").as_deref(), Some("#+filetags: :pro:\n"));
        assert_eq!(add_filetag("#+title: P\n* P", "pro").as_deref(), Some("#+title: P\n#+filetags: :pro:\n* P"));
        assert_eq!(add_filetag("#+FILETAGS: :a:\nx", "b").as_deref(), Some("#+FILETAGS: :a:b:\nx"));
        assert_eq!(add_filetag("#+filetags:\nx", "b").as_deref(), Some("#+filetags: :b:\nx"));
        assert_eq!(add_filetag("#+filetags: :a:  \nx", "b").as_deref(), Some("#+filetags: :a:b:\nx"));
        assert_eq!(add_filetag("x\n#+filetags: :A:\n", "a"), None);
    }

    fn c(s: &str) -> Vec<char> { s.chars().collect() }

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

    #[test]
    fn org_tags_under_the_caret() {
        let t = c("* Titre :projet:urgent:\n#+FILETAGS: :a:b:\ntexte :x:");
        assert_eq!(org_tag_at(&t, 10), Some("projet".into()));
        assert_eq!(org_tag_at(&t, 20), Some("urgent".into()));
        assert_eq!(org_tag_at(&t, 3), None);            // in the title
        assert_eq!(org_tag_at(&t, 38), Some("a".into()));
        assert_eq!(org_tag_at(&t, 48), None);           // not a headline
    }

    #[test]
    fn org_tag_prefix_while_typing() {
        // In the text
        assert_eq!(org_tag_prefix(&c("voir :pro"), 9), Some((6, "pro".into())));
        assert_eq!(org_tag_prefix(&c(":pro"), 4), Some((1, "pro".into())));
        assert_eq!(org_tag_prefix(&c("voir :"), 6), None);
        assert_eq!(org_tag_prefix(&c("à 10:30"), 7), None);
        assert_eq!(org_tag_prefix(&c("http://x"), 8), None);
        assert_eq!(org_tag_prefix(&c("* T :pro"), 8), Some((5, "pro".into())));
        assert_eq!(org_tag_prefix(&c("* T :"), 5), Some((5, String::new())));
        assert_eq!(org_tag_prefix(&c("* T :a:b"), 8), Some((7, "b".into())));
        assert_eq!(org_tag_prefix(&c("* T :pr:"), 7), Some((5, "pr".into())));
        assert_eq!(org_tag_prefix(&c("#+FILETAGS: :a"), 14), Some((13, "a".into())));
        assert_eq!(org_tag_prefix(&c("#+filetags: "), 12), Some((12, String::new())));
        // `Note:` glued to a word, a caret before more words
        assert_eq!(org_tag_prefix(&c("* Note:"), 7), None);
        assert_eq!(org_tag_prefix(&c("texte :a"), 8), Some((7, "a".into())));
        assert_eq!(org_tag_prefix(&c("* T :a suite"), 6), None);
        assert_eq!(org_tag_prefix(&c("#+TITLE: x"), 10), None);
    }

    #[test]
    fn org_tag_link_found() {
        let t = c("texte :x:\n* T :a:Cible:\n");
        assert_eq!(find_link(&t, "cible", true, Hashtags::Off), Some((17, 22)));
        assert_eq!(find_link(&t, "x", true, Hashtags::Off), None);
    }

    #[test]
    fn hashtag_link_found() {
        let t = c("Voir #Cible ici [[Autre]]");
        assert_eq!(find_link(&t, "cible", true, Hashtags::Org), Some((6, 11)));
        assert_eq!(find_link(&t, "cible", true, Hashtags::Off), None);
    }

    #[test]
    fn sentences_forward_and_back() {
        let t = c("Un deux. Trois quatre! Cinq");
        assert_eq!(forward_sentence(&t, 0), 8);
        assert_eq!(forward_sentence(&t, 8), 22);
        assert_eq!(backward_sentence(&t, 15), 9);   // inside sentence 2 -> its start
        assert_eq!(backward_sentence(&t, 9), 0);    // at start of 2 -> start of 1
        assert_eq!(backward_sentence(&t, 3), 0);
    }

    #[test]
    fn sentence_stops_at_blank_line() {
        let t = c("Fin sans point\n\nSuite.");
        assert_eq!(forward_sentence(&t, 0), 14);
    }

    #[test]
    fn paragraphs() {
        let t = c("a\nb\n\nc\nd\n\ne");
        assert_eq!(forward_paragraph(&t, 0), 4);
        assert_eq!(forward_paragraph(&t, 4), 9);
        assert_eq!(forward_paragraph(&t, 10), t.len());
        assert_eq!(backward_paragraph(&t, 6), 4);
        assert_eq!(backward_paragraph(&t, 5), 4);
        assert_eq!(backward_paragraph(&t, 10), 9);
        assert_eq!(backward_paragraph(&t, 2), 0);
    }

    #[test]
    fn finds_links() {
        let t = c("voir [[Autre]] puis [[ Cible ][label]] fin");
        assert_eq!(find_link(&t, "Cible", false, Hashtags::Off), Some((23, 28)));
        assert_eq!(find_link(&t, "cible", false, Hashtags::Off), None);
        assert_eq!(find_link(&t, "cible", true, Hashtags::Off), Some((23, 28)));
        assert_eq!(find_link(&t, "Absent", true, Hashtags::Off), None);
    }

    #[test]
    fn indentation() {
        let t = c("x\n   texte");
        assert_eq!(back_to_indentation(&t, 8), 5);
    }

    #[test]
    fn headings() {
        let t = c("* A\ntexte\n** B\nplus\n* C");
        assert_eq!(next_heading(&t, 0), 10);
        assert_eq!(next_heading(&t, 5), 10);
        assert_eq!(next_heading(&t, 16), 20);
        assert_eq!(previous_heading(&t, 16), 10);
        assert_eq!(previous_heading(&t, 5), 0);   // body text -> its own heading
        assert_eq!(previous_heading(&t, 12), 0);  // on "** B" -> the heading before it
        assert_eq!(previous_heading(&t, 2), 2);   // first heading: nowhere to go
    }
}
