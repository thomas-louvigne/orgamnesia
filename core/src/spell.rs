//! The words of a page the spell checker looks at: the text, without what isn't
//! prose (code and example blocks, `#+KEYWORD:` lines, drawers, link targets, URLs,
//! `~code~`, `=verbatim=`, `:tags:`, `#tags`) nor the words a dictionary can't know
//! (with digits, in capitals, camelCase, paths and identifiers).

/// What joins letters in a path, an identifier or org markup: a word holding one
/// is not checked.
const JOINERS: &str = "_./@\\:#+=~*";

/// Blocks whose lines are not prose.
const CODE_BLOCKS: &[&str] = &["src", "example", "export", "comment", "latex", "html"];

/// Char ranges, in `content`, of the words to check, in order.
pub fn words(content: &str) -> Vec<(usize, usize)> {
    let chars: Vec<char> = content.chars().collect();
    let mut out = Vec::new();
    // The block (`#+begin_src`…) or drawer (`:PROPERTIES:`…) the line is in
    let mut block: Option<String> = None;
    let mut drawer = false;
    let mut start = 0;
    while start <= chars.len() {
        let end = chars[start..].iter().position(|&c| c == '\n').map_or(chars.len(), |i| start + i);
        let line = &chars[start..end];
        let text: String = line.iter().collect::<String>();
        let trimmed = text.trim();
        let lower = trimmed.to_lowercase();
        if let Some(name) = &block {
            if lower.starts_with(&format!("#+end_{name}")) { block = None; }
        } else if drawer {
            if lower == ":end:" { drawer = false; }
        } else if let Some(rest) = lower.strip_prefix("#+begin_") {
            let name = rest.split_whitespace().next().unwrap_or("");
            if CODE_BLOCKS.contains(&name) { block = Some(name.to_string()); }
        } else if lower.starts_with("#+") || trimmed == ":" || trimmed.starts_with(": ") {
            // Keyword line, fixed-width line
        } else if is_drawer_start(trimmed) {
            drawer = true;
        } else {
            line_words(line, start, &mut out);
        }
        start = end + 1;
    }
    out
}

/// `:PROPERTIES:`, `:LOGBOOK:`… alone on their line.
fn is_drawer_start(line: &str) -> bool {
    line.len() > 2 && line.starts_with(':') && line.ends_with(':')
        && line[1..line.len() - 1].chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

/// The words of a line starting at `offset` in the page.
fn line_words(line: &[char], offset: usize, out: &mut Vec<(usize, usize)>) {
    let skip = masked(line);
    let mut i = 0;
    while i < line.len() {
        if !is_token_char(line[i]) { i += 1; continue; }
        let a = i;
        while i < line.len() && is_token_char(line[i]) { i += 1; }
        if let Some((s, e)) = word_of(&line[a..i]) && !skip[a + s..a + e].iter().any(|&m| m) {
            out.push((offset + a + s, offset + a + e));
        }
    }
}

/// Chars a token is made of: letters, digits, and what joins them in words,
/// paths, identifiers or org markup.
fn is_token_char(c: char) -> bool {
    c.is_alphanumeric() || c == '\'' || c == '’' || c == '-' || JOINERS.contains(c)
}

/// The word of a token (without the punctuation around it), as a range in it;
/// `None` when it is not one to check.
fn word_of(token: &[char]) -> Option<(usize, usize)> {
    let s = token.iter().position(|c| c.is_alphanumeric())?;
    let e = token.iter().rposition(|c| c.is_alphanumeric())? + 1;
    // `#tag`, `:tag:`, `\command`, `@name`
    if token[..s].iter().any(|c| "#:\\@".contains(*c)) { return None; }
    let word = &token[s..e];
    if word.len() < 2 { return None; }
    let mut prev = None;
    for &c in word {
        if c.is_numeric() || JOINERS.contains(c) { return None; }
        // A capital inside the word (but after an apostrophe or a hyphen): ABC, camelCase
        if c.is_uppercase() && prev.is_some_and(|p: char| p.is_alphanumeric()) { return None; }
        prev = Some(c);
    }
    Some((s, e))
}

/// Chars of the line that are not prose: link targets, URLs, inline code and
/// verbatim, the tags ending a headline.
fn masked(line: &[char]) -> Vec<bool> {
    let mut skip = vec![false; line.len()];
    let at = |i: usize, s: &str| s.chars().enumerate().all(|(k, c)| line.get(i + k) == Some(&c));
    let mut i = 0;
    while i < line.len() {
        if at(i, "[[") {
            let close = (i + 2..line.len()).find(|&k| at(k, "]]"));
            let Some(close) = close else { break };
            // `[[target][description]]`: only the description is prose
            let desc = (i + 2..close).find(|&k| at(k, "]["));
            let to = desc.map_or(close + 2, |d| d + 2);
            skip[i..to].fill(true);
            skip[close..close + 2].fill(true);
            i = close + 2;
        } else if ["http://", "https://", "file:", "mailto:", "ftp://"].iter().any(|p| at(i, p))
            && (i == 0 || !line[i - 1].is_alphanumeric())
        {
            let e = (i..line.len()).find(|&k| line[k].is_whitespace() || line[k] == ']').unwrap_or(line.len());
            skip[i..e].fill(true);
            i = e;
        } else if (line[i] == '~' || line[i] == '=')
            && (i == 0 || !line[i - 1].is_alphanumeric())
            && line.get(i + 1).is_some_and(|c| !c.is_whitespace())
            && let Some(close) = (i + 2..line.len()).find(|&k| {
                line[k] == line[i] && !line[k - 1].is_whitespace()
                    && line.get(k + 1).is_none_or(|c| !c.is_alphanumeric())
            })
        {
            skip[i..=close].fill(true);
            i = close + 1;
        } else {
            i += 1;
        }
    }
    // Tags at the end of a headline: `* Titre  :tag1:tag2:`
    if line.first() == Some(&'*') {
        let end = line.iter().rposition(|c| !c.is_whitespace()).map_or(0, |e| e + 1);
        let start = line[..end].iter().rposition(|c| c.is_whitespace()).map_or(0, |s| s + 1);
        let tags = &line[start..end];
        if tags.len() > 2 && tags[0] == ':' && tags[tags.len() - 1] == ':' {
            skip[start..end].fill(true);
        }
    }
    skip
}

/// The range of `words` holding the char `pos` (or ending at it).
pub fn word_at(words: &[(usize, usize)], pos: usize) -> Option<(usize, usize)> {
    words.iter().copied().find(|&(a, b)| a <= pos && pos <= b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checked(content: &str) -> Vec<String> {
        let chars: Vec<char> = content.chars().collect();
        words(content).into_iter().map(|(a, b)| chars[a..b].iter().collect()).collect()
    }

    #[test]
    fn prose_words_with_apostrophes_and_hyphens() {
        assert_eq!(checked("L'homme, peut-être *gras* /italique/ (vrai)."),
            ["L'homme", "peut-être", "gras", "italique", "vrai"]);
        assert_eq!(checked("aujourd’hui"), ["aujourd’hui"]);
    }

    #[test]
    fn words_a_dictionary_cant_know_are_left_out() {
        assert_eq!(checked("a TODO v0.16 camelCase ABC x86 foo_bar a/b moi@ici.fr fichier.org Paris"),
            ["Paris"]);
    }

    #[test]
    fn org_markup_is_left_out() {
        assert_eq!(checked("* TODO Titre ici   :tag:autre:"), ["Titre", "ici"]);
        assert_eq!(checked("voir [[Page cible][la description]] et [[Autre page]]"),
            ["voir", "la", "description", "et"]);
        assert_eq!(checked("lien https://exemple.fr/chemin et ~code ici~ ou =verbatim= fin"),
            ["lien", "et", "ou", "fin"]);
        assert_eq!(checked("un #hashtag et :tag: là \\alpha"), ["un", "et", "là"]);
        assert!(checked("a=b ~ c").is_empty());
    }

    #[test]
    fn blocks_keywords_and_drawers_are_left_out() {
        let page = "#+TITLE: Mon titre\n\
                    avant\n\
                    #+begin_src rust\n\
                    fn mauvais() {}\n\
                    #+END_SRC\n\
                    :PROPERTIES:\n\
                    :ID: abc\n\
                    :END:\n\
                    : fixe\n\
                    #+begin_quote\n\
                    citation\n\
                    #+end_quote\n\
                    après";
        assert_eq!(checked(page), ["avant", "citation", "après"]);
    }

    #[test]
    fn ranges_are_in_chars_of_the_page() {
        let page = "été\nça va";
        assert_eq!(words(page), [(0, 3), (4, 6), (7, 9)]);
        assert_eq!(word_at(&words(page), 5), Some((4, 6)));
        assert_eq!(word_at(&words(page), 3), Some((0, 3)));
    }
}
