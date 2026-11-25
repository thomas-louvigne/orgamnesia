//! Unlinked mentions: a page's name (or one of its aliases) written in another
//! page as plain text, not as a link; and the edit that makes one a link.

use crate::{agenda::Edit, links::{self, LinkSyntax}, names::fold_char, Format};

/// A mention found: the chars `start..end` of the text name `names[name]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub start: usize,
    pub end: usize,
    pub name: usize,
}

/// Whether `c` makes a word with its neighbours (a name inside a word is no mention).
fn joins(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '#' | '/' | '-')
}

/// The whole-word mentions of `names` in `text`, ignoring case and accents, left
/// out those inside a link (of `syntax`) and on `#+KEYWORD:` lines. The longest
/// name is tried first at each place.
pub fn find(text: &str, names: &[String], syntax: LinkSyntax) -> Vec<Found> {
    let chars: Vec<char> = text.chars().collect();
    let folded: Vec<char> = chars.iter().map(|&c| fold_char(c)).collect();
    let mut order: Vec<(usize, Vec<char>)> = names.iter().enumerate()
        .map(|(i, n)| (i, n.trim().chars().map(fold_char).collect::<Vec<_>>()))
        .filter(|(_, n)| !n.is_empty())
        .collect();
    order.sort_by_key(|(_, n)| std::cmp::Reverse(n.len()));
    let mut skip: Vec<(usize, usize)> = links::scan(&chars, syntax).iter().map(|f| (f.start, f.end)).collect();
    let mut line = 0;
    for l in text.split('\n') {
        let n = l.chars().count();
        if l.trim_start().starts_with("#+") { skip.push((line, line + n)); }
        line += n + 1;
    }
    skip.sort();
    let mut out = Vec::new();
    let (mut i, mut s) = (0, 0);
    while i < chars.len() {
        while s < skip.len() && skip[s].1 <= i { s += 1; }
        if s < skip.len() && skip[s].0 <= i { i = skip[s].1; continue; }
        let starts_word = i == 0 || !joins(chars[i - 1]);
        let hit = starts_word.then(|| order.iter().find(|(_, n)| {
            let end = i + n.len();
            folded.get(i..end) == Some(n.as_slice())
                && chars.get(end).is_none_or(|&c| !joins(c))
                && skip.get(s).is_none_or(|k| k.0 >= end)
        })).flatten();
        match hit {
            Some((name, n)) => {
                out.push(Found { start: i, end: i + n.len(), name: *name });
                i += n.len();
            }
            None => i += 1,
        }
    }
    out
}

/// The edit making the mention `seen` at chars `start..end` of `text` a link to
/// the page `target`: `[[target]]`, or with `seen` as its text when it differs
/// (`[[target][seen]]`, `[[target|seen]]` in Markdown when that syntax is read).
/// When the text moved meanwhile, its first unlinked mention reading `seen` is
/// taken; `None` when there is none.
pub fn link(text: &str, start: usize, end: usize, seen: &str, target: &str, format: Format, syntax: LinkSyntax) -> Option<Vec<Edit>> {
    let at_place = text.chars().skip(start).take(end - start).eq(seen.chars())
        && find(text, &[seen.to_string()], syntax).iter().any(|f| f.start == start);
    let (a, b) = if at_place { (start, end) } else {
        let chars: Vec<char> = text.chars().collect();
        let f = find(text, &[seen.to_string()], syntax).into_iter()
            .find(|f| chars[f.start..f.end].iter().copied().eq(seen.chars()))?;
        (f.start, f.end)
    };
    let link = if seen == target {
        format!("[[{target}]]")
    } else if format == Format::Markdown && syntax.pipe {
        format!("[[{target}|{seen}]]")
    } else {
        format!("[[{target}][{seen}]]")
    };
    Some(vec![Edit { a, b, text: link }])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seen(text: &str, names: &[&str]) -> Vec<String> {
        let chars: Vec<char> = text.chars().collect();
        let names: Vec<String> = names.iter().map(|n| n.to_string()).collect();
        find(text, &names, LinkSyntax::ALL).iter().map(|f| chars[f.start..f.end].iter().collect()).collect()
    }

    #[test]
    fn finds_whole_words_outside_links() {
        let t = "#+TITLE: Le Dragon\nUn dragon, le Grand Dragon et [[Dragon]] [x](Dragon.md) #dragon dragons Dragon-rouge élodie";
        assert_eq!(seen(t, &["Dragon", "Grand Dragon"]), ["dragon", "Grand Dragon"]);
        assert_eq!(seen("Élody et elody", &["elody"]), ["Élody", "elody"]);
        assert!(seen("rien", &[""]).is_empty());
    }

    #[test]
    fn makes_a_mention_a_link() {
        let apply = |t: &str, e: Option<Vec<Edit>>| crate::agenda::apply(t, &e.unwrap());
        let t = "Le dragon dort.";
        assert_eq!(apply(t, link(t, 3, 9, "dragon", "Dragon", Format::Org, LinkSyntax::NONE)), "Le [[Dragon][dragon]] dort.");
        assert_eq!(apply(t, link(t, 3, 9, "dragon", "dragon", Format::Org, LinkSyntax::NONE)), "Le [[dragon]] dort.");
        assert_eq!(apply(t, link(t, 3, 9, "dragon", "Dragon", Format::Markdown, LinkSyntax::ALL)), "Le [[Dragon|dragon]] dort.");
        // The text moved: the mention is found again
        let moved = "Oui. Le dragon dort.";
        assert_eq!(apply(moved, link(moved, 3, 9, "dragon", "Dragon", Format::Org, LinkSyntax::NONE)), "Oui. Le [[Dragon][dragon]] dort.");
        assert!(link("[[dragon]]", 2, 8, "dragon", "Dragon", Format::Org, LinkSyntax::NONE).is_none());
    }
}
