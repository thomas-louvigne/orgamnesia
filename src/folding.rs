//! Folding headlines as Emacs' org-mode does with `TAB`: a headline cycles between
//! FOLDED (all below it hidden), CHILDREN (only its sub-headlines shown, folded)
//! and SUBTREE (all shown).
//!
//! A fold is a page range: from the `\n` ending a line to the end of the last
//! hidden line (see `tables::View`, which hides them).

/// Level of the headline starting at `ls` (its stars), if the line is one.
fn level(chars: &[char], ls: usize) -> Option<usize> {
    let stars = chars[ls..].iter().take_while(|&&c| c == '*').count();
    (stars > 0 && matches!(chars.get(ls + stars), Some(' ' | '\n') | None)).then_some(stars)
}

/// Whether the line starting at `ls` is a headline (`* Title`).
pub fn is_headline(chars: &[char], ls: usize) -> bool {
    level(chars, ls).is_some()
}

fn line_end(chars: &[char], ls: usize) -> usize {
    chars[ls..].iter().position(|&c| c == '\n').map_or(chars.len(), |p| ls + p)
}

/// Start of the line holding `pos`.
pub fn line_start(chars: &[char], pos: usize) -> usize {
    chars[..pos.min(chars.len())].iter().rposition(|&c| c == '\n').map_or(0, |i| i + 1)
}

/// The headline at `ls` (level `level`): the end of its line, the end of its subtree
/// (the `\n` before the next headline of the same level or above, or the page end),
/// and the starts of its direct sub-headlines.
fn subtree(chars: &[char], ls: usize, level_of: usize) -> (usize, usize, Vec<usize>) {
    let le = line_end(chars, ls);
    let mut children = Vec::new();
    let mut min_child = usize::MAX;
    let mut p = le;
    while p < chars.len() {
        let next = p + 1;
        if let Some(l) = level(chars, next) {
            if l <= level_of { return (le, p, children); }
            // A sub-headline with no headline above it in the subtree is a child
            if l <= min_child {
                children.push(next);
                min_child = l;
            }
        }
        p = line_end(chars, next);
    }
    // The empty line ending the page stays shown
    let end = if chars.last() == Some(&'\n') { chars.len() - 1 } else { chars.len() };
    (le, end.max(le), children)
}

/// The folds after `TAB` on the headline starting at `ls`, given the current `folds`.
/// `None` when the line is not a headline, or has nothing under it.
pub fn cycle(chars: &[char], ls: usize, folds: &[(usize, usize)]) -> Option<Vec<(usize, usize)>> {
    let level_of = level(chars, ls)?;
    let (le, end, children) = subtree(chars, ls, level_of);
    if le >= end { return None; }
    let inside = |&(a, b): &(usize, usize)| a >= le && b <= end;
    let mut out: Vec<(usize, usize)> = folds.iter().copied().filter(|f| !inside(f)).collect();
    if folds.contains(&(le, end)) {
        // FOLDED → CHILDREN: the text before the first child hidden, each child folded.
        // Without children, all is shown.
        if let Some(&first) = children.first() && first - 1 > le {
            out.push((le, first - 1));
        }
        for &c in &children {
            let (cle, cend, _) = subtree(chars, c, level(chars, c).unwrap_or(1));
            if cle < cend { out.push((cle, cend)); }
        }
    } else if !folds.iter().any(inside) {
        // SUBTREE → FOLDED
        out.push((le, end));
    }
    // CHILDREN → SUBTREE: the folds inside are dropped
    out.sort();
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(s: &str) -> Vec<char> { s.chars().collect() }

    #[test]
    fn tab_cycles_folded_children_subtree() {
        let t = c("* A\ntexte\n** B\nb\n** C\n*** D\n* E\n");
        let b = 10;
        let cc = 17;
        // SUBTREE → FOLDED: everything from the end of `* A` to the end of the line before `* E`
        let folded = cycle(&t, 0, &[]).unwrap();
        assert_eq!(folded, vec![(3, 27)]);
        // FOLDED → CHILDREN: `texte` hidden, `** B` and `** C` folded
        let children = cycle(&t, 0, &folded).unwrap();
        assert_eq!(children, vec![(3, b - 1), (b + 4, cc - 1), (cc + 4, 27)]);
        // CHILDREN → SUBTREE
        assert_eq!(cycle(&t, 0, &children).unwrap(), Vec::<(usize, usize)>::new());
        // Another headline's fold is kept
        assert_eq!(cycle(&t, 0, &[(32, 33)]).unwrap(), vec![(3, 27), (32, 33)]);
    }

    #[test]
    fn last_line_of_the_page_stays_shown() {
        let t = c("* A\ntexte\n");
        assert_eq!(cycle(&t, 0, &[]), Some(vec![(3, 9)]));
    }

    #[test]
    fn nothing_to_fold() {
        let t = c("* A\n* B\ntexte");
        assert_eq!(cycle(&t, 0, &[]), None);        // empty headline
        assert_eq!(cycle(&t, 8, &[]), None);        // not a headline
        assert!(is_headline(&t, 0) && !is_headline(&t, 8));
        assert_eq!(cycle(&t, 4, &[]), Some(vec![(7, 13)]));
        // Without children, FOLDED shows everything again
        assert_eq!(cycle(&t, 4, &[(7, 13)]), Some(vec![]));
    }
}
