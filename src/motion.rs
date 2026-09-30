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

/// A `#hashtag` starting at `i` (same rules as the backend indexer): returns the tag
/// (without `#`) and the index just after it.
pub fn hashtag_at(chars: &[char], i: usize) -> Option<(String, usize)> {
    if chars.get(i) != Some(&'#') { return None; }
    if i > 0 && !(chars[i - 1].is_whitespace() || "([{\"'".contains(chars[i - 1])) { return None; }
    let is_tag_char = |c: char| c.is_alphanumeric() || c == '_' || c == '-';
    let mut end = i + 1;
    while end < chars.len() && is_tag_char(chars[end]) { end += 1; }
    while end > i + 1 && chars[end - 1] == '-' { end -= 1; }
    if end == i + 1 || !chars[i + 1].is_alphanumeric() { return None; }
    Some((chars[i + 1..end].iter().collect(), end))
}

/// Char range of the target text of the first `[[target]]` / `[[target][label]]`
/// link naming `target`.
pub fn find_link(chars: &[char], target: &str, ignore_case: bool, hashtags: bool) -> Option<(usize, usize)> {
    let norm = |s: &str| if ignore_case { s.trim().to_lowercase() } else { s.trim().to_string() };
    let wanted = norm(target);
    let mut i = 0;
    while i < chars.len() {
        if let (true, Some((tag, end))) = (hashtags, hashtag_at(chars, i)) {
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
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(s: &str) -> Vec<char> { s.chars().collect() }

    #[test]
    fn hashtag_link_found() {
        let t = c("Voir #Cible ici [[Autre]]");
        assert_eq!(find_link(&t, "cible", true, true), Some((6, 11)));
        assert_eq!(find_link(&t, "cible", true, false), None);
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
        assert_eq!(find_link(&t, "Cible", false, false), Some((23, 28)));
        assert_eq!(find_link(&t, "cible", false, false), None);
        assert_eq!(find_link(&t, "cible", true, false), Some((23, 28)));
        assert_eq!(find_link(&t, "Absent", true, false), None);
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
