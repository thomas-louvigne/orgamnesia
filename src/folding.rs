//! Folding headlines as Emacs' org-mode does with `TAB`: a headline cycles between
//! FOLDED (all below it hidden), CHILDREN (only its sub-headlines shown, folded)
//! and SUBTREE (all shown). A list item, a block or a drawer is folded or shown;
//! `S-TAB` cycles the whole page between OVERVIEW, CONTENTS and SHOW ALL.
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

/// The lines after the line ending at `le`: where each one starts and ends.
fn lines_after(chars: &[char], le: usize) -> impl Iterator<Item = (usize, usize)> + '_ {
    let first = (le < chars.len()).then(|| (le + 1, line_end(chars, le + 1)));
    std::iter::successors(first, move |&(_, b)| (b < chars.len()).then(|| (b + 1, line_end(chars, b + 1))))
}

/// Where the list item, block (`#+begin_…` to `#+end_…`) or drawer (`:NAME:` to
/// `:END:`) opened by the line `ls..le` ends: the end of its last line.
fn element_end(chars: &[char], ls: usize, le: usize) -> Option<usize> {
    let text = |a: usize, b: usize| -> String { chars[a..b].iter().collect::<String>().trim().to_lowercase() };
    let line = text(ls, le);
    if let Some(kind) = line.strip_prefix("#+begin_") {
        let close = format!("#+end_{}", kind.split_whitespace().next().unwrap_or(""));
        return lines_after(chars, le).find(|&(a, b)| text(a, b) == close).map(|(_, b)| b);
    }
    if crate::motion::is_drawer_start(chars, ls) {
        return lines_after(chars, le).find(|&(a, b)| text(a, b) == ":end:").map(|(_, b)| b);
    }
    crate::motion::item_contents(chars, ls)?;
    // The item goes on with the lines indented more than its bullet; the blank
    // lines after it stay out, and two of them end the list
    let indent = crate::motion::indent_of(chars, ls);
    let mut end = le;
    let mut blanks = 0;
    for (a, b) in lines_after(chars, le) {
        if chars[a..b].iter().all(|c| c.is_whitespace()) {
            blanks += 1;
            if blanks == 2 { break; }
            continue;
        }
        if crate::motion::indent_of(chars, a) <= indent || is_headline(chars, a) { break; }
        blanks = 0;
        end = b;
    }
    Some(end)
}

/// The folds after `TAB` on the line starting at `ls` when it opens a list item
/// with lines under it, a block or a drawer (org-mode folds them too): what is
/// under the line is hidden, or shown again. `None` when the line opens none of
/// them, or has nothing under it.
pub fn cycle_element(chars: &[char], ls: usize, folds: &[(usize, usize)]) -> Option<Vec<(usize, usize)>> {
    let le = line_end(chars, ls);
    let end = element_end(chars, ls, le)?;
    if end <= le { return None; }
    let folded = folds.contains(&(le, end));
    let mut out: Vec<(usize, usize)> = folds.iter().copied().filter(|&(a, b)| !(a >= le && b <= end)).collect();
    if !folded { out.push((le, end)); }
    out.sort();
    Some(out)
}

/// What `S-TAB` shows of the page (org-mode's global cycling).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Global {
    /// The top headlines only.
    Overview,
    /// All the headlines, their text hidden.
    Contents,
    /// Everything.
    ShowAll,
}

/// The folds after `S-TAB`, given the current `folds`: OVERVIEW, then CONTENTS,
/// then SHOW ALL, and OVERVIEW again (from any other folds too). `None` when the
/// page has no headline.
pub fn global_cycle(chars: &[char], folds: &[(usize, usize)]) -> Option<(Global, Vec<(usize, usize)>)> {
    let mut heads = Vec::new();
    let mut ls = 0;
    loop {
        if let Some(l) = level(chars, ls) { heads.push((ls, l)); }
        let le = line_end(chars, ls);
        if le >= chars.len() { break; }
        ls = le + 1;
    }
    let top = heads.iter().map(|&(_, l)| l).min()?;
    let overview: Vec<(usize, usize)> = heads.iter().filter(|&&(_, l)| l == top)
        .map(|&(ls, l)| subtree(chars, ls, l))
        .filter(|&(le, end, _)| le < end)
        .map(|(le, end, _)| (le, end))
        .collect();
    // The empty line ending the page stays shown
    let page_end = if chars.last() == Some(&'\n') { chars.len() - 1 } else { chars.len() };
    let contents: Vec<(usize, usize)> = heads.iter().enumerate()
        .map(|(i, &(ls, _))| (line_end(chars, ls), heads.get(i + 1).map_or(page_end, |&(next, _)| next - 1)))
        .filter(|&(le, end)| le < end)
        .collect();
    let mut current = folds.to_vec();
    current.sort();
    Some(if current == overview && contents != overview {
        (Global::Contents, contents)
    } else if current == overview || (current == contents && !current.is_empty()) {
        (Global::ShowAll, Vec::new())
    } else {
        (Global::Overview, overview)
    })
}

/// Why a headline's subtree can't be promoted or demoted.
#[derive(Debug, PartialEq)]
pub enum ShiftError {
    /// The line is not a headline.
    NotHeadline,
    /// A level-1 headline can't be promoted (Emacs: "Cannot promote to level 0").
    TopLevel,
}

/// The starts of the headlines of the subtree of the headline starting at `ls`
/// (itself first): where org-mode's `M-S-<right>` / `M-S-<left>` add or remove a
/// star, to demote or promote the whole subtree.
pub fn shift_subtree(chars: &[char], ls: usize, demote: bool) -> Result<Vec<usize>, ShiftError> {
    let level_of = level(chars, ls).ok_or(ShiftError::NotHeadline)?;
    if !demote && level_of == 1 { return Err(ShiftError::TopLevel); }
    let (le, end, _) = subtree(chars, ls, level_of);
    let mut lines = vec![ls];
    let mut p = le;
    while p < end {
        let next = p + 1;
        if is_headline(chars, next) { lines.push(next); }
        p = line_end(chars, next);
    }
    Ok(lines)
}

/// Where the caret at `pos` goes when the headlines starting at `lines` are
/// shifted: only the star of the first one (the caret's) moves it.
pub fn caret_after_shift(pos: usize, lines: &[usize], demote: bool) -> usize {
    if demote { pos + 1 } else if pos > lines[0] { pos - 1 } else { pos }
}

/// The page with a star added to (`demote`) or removed from the start of each
/// line of `lines` (see `shift_subtree`), and the `folds` moved with the text.
pub fn shift_page(page: &[char], lines: &[usize], demote: bool, folds: &[(usize, usize)]) -> (String, Vec<(usize, usize)>) {
    let shift = |p: usize| -> usize {
        let n = lines.iter().take_while(|&&l| l < p).count();
        if demote { p + n } else { p - n }
    };
    let mut out = String::with_capacity(page.len() + lines.len());
    let mut next = lines.iter().peekable();
    for (i, &c) in page.iter().enumerate() {
        if next.next_if(|&&l| l == i).is_some() {
            if demote { out.push('*'); } else { continue; }
        }
        out.push(c);
    }
    (out, folds.iter().map(|&(a, b)| (shift(a), shift(b))).collect())
}

/// Two neighbour ranges to swap: `a..mid` and `mid + 1..b`, the `\n` at `mid`
/// between them.
#[derive(Debug, PartialEq, Clone, Copy)]
pub struct Swap {
    pub a: usize,
    pub mid: usize,
    pub b: usize,
}

impl Swap {
    /// The text of `a..b` once swapped.
    pub fn text(self, chars: &[char]) -> String {
        let first: String = chars[self.a..self.mid].iter().collect();
        let second: String = chars[self.mid + 1..self.b].iter().collect();
        format!("{second}\n{first}")
    }

    /// Where the position `p` goes once swapped (a fold's `\n` ends follow their text).
    pub fn pos(self, p: usize) -> usize {
        let Swap { a, mid, b } = self;
        if (a..=mid).contains(&p) { p + (b - mid) } else if p > mid && p <= b { p - (mid + 1 - a) } else { p }
    }
}

/// The subtree of the headline starting at `ls` and the one of its sibling above
/// (`up`) or below, to swap them as org-mode's `M-<up>` / `M-<down>`. `None` when the
/// line is not a headline, or has no sibling there (a headline above it in between).
pub fn move_subtree(chars: &[char], ls: usize, up: bool) -> Option<Swap> {
    let level_of = level(chars, ls)?;
    let (_, end, _) = subtree(chars, ls, level_of);
    if up {
        let mut p = ls;
        while p > 0 {
            p = line_start(chars, p - 1);
            match level(chars, p) {
                Some(l) if l == level_of => return Some(Swap { a: p, mid: ls - 1, b: end }),
                Some(l) if l < level_of => return None,
                _ => {}
            }
        }
        None
    } else {
        let next = end + 1;
        if next >= chars.len() || level(chars, next) != Some(level_of) { return None; }
        let (_, next_end, _) = subtree(chars, next, level_of);
        Some(Swap { a: ls, mid: end, b: next_end })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(s: &str) -> Vec<char> { s.chars().collect() }

    #[test]
    fn shift_subtree_takes_the_sub_headlines() {
        let t = c("* A\n** B\ntexte\n*** C\n** D\n* E\n** F\n");
        // `** B` takes `*** C`, not its sibling `** D`
        assert_eq!(shift_subtree(&t, 4, true), Ok(vec![4, 15]));
        assert_eq!(shift_subtree(&t, 4, false), Ok(vec![4, 15]));
        assert_eq!(shift_subtree(&t, 0, true), Ok(vec![0, 4, 15, 21]));
        assert_eq!(shift_subtree(&t, 0, false), Err(ShiftError::TopLevel));
        assert_eq!(shift_subtree(&t, 9, true), Err(ShiftError::NotHeadline));
        assert_eq!(shift_subtree(&t, 26, true), Ok(vec![26, 30]));
    }

    #[test]
    fn shifted_page_keeps_its_folds_in_place() {
        let t = c("* A\n** B\ntexte\n*** C\n** D\n");
        let lines = shift_subtree(&t, 4, true).unwrap();
        // `** B` folded: from the end of its line to the end of `*** C`
        let (page, folds) = shift_page(&t, &lines, true, &[(8, 20)]);
        assert_eq!(page, "* A\n*** B\ntexte\n**** C\n** D\n");
        assert_eq!(folds, vec![(9, 22)]);
        let page = c(&page);
        let (back, folds) = shift_page(&page, &shift_subtree(&page, 4, false).unwrap(), false, &folds);
        assert_eq!(back, t.iter().collect::<String>());
        assert_eq!(folds, vec![(8, 20)]);
        // The caret on the headline moves with its own star only
        assert_eq!(caret_after_shift(6, &lines, true), 7);
        assert_eq!(caret_after_shift(6, &lines, false), 5);
        assert_eq!(caret_after_shift(4, &lines, false), 4);
    }

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
    #[test]
    fn tab_folds_items_blocks_and_drawers() {
        let t = c("- a\n  suite\n  - b\n\n- c\n#+BEGIN_SRC rust\nx\n#+end_src\n:PROPERTIES:\n:ID: 1\n:END:\nfin");
        // The item `- a`: its lines and its sub-item, not the blank line after them
        let folded = cycle_element(&t, 0, &[]).unwrap();
        assert_eq!(folded, vec![(3, 17)]);
        assert_eq!(cycle_element(&t, 0, &folded).unwrap(), Vec::<(usize, usize)>::new());
        // Nothing under `- b` nor `- c`
        assert_eq!(cycle_element(&t, 12, &[]), None);
        assert_eq!(cycle_element(&t, 19, &[]), None);
        // The block up to its end line, case aside; the drawer up to `:END:`
        assert_eq!(cycle_element(&t, 23, &[]), Some(vec![(39, 51)]));
        assert_eq!(cycle_element(&t, 52, &[]), Some(vec![(64, 77)]));
        // Not one of them
        assert_eq!(cycle_element(&t, 78, &[]), None);
        // An item ends at a line no more indented than its bullet, or two blank lines
        let t = c("  - a\n    b\n  c\n- d\n  e\n\n\n  f");
        assert_eq!(cycle_element(&t, 0, &[]), Some(vec![(5, 11)]));
        assert_eq!(cycle_element(&t, 16, &[]), Some(vec![(19, 23)]));
    }

    #[test]
    fn shift_tab_cycles_overview_contents_show_all() {
        let t = c("intro\n* A\ntexte\n** B\nb\n* C\n** D\n");
        let overview = vec![(9, 22), (26, 31)];
        let contents = vec![(9, 15), (20, 22)];
        assert_eq!(global_cycle(&t, &[]), Some((Global::Overview, overview.clone())));
        assert_eq!(global_cycle(&t, &overview), Some((Global::Contents, contents.clone())));
        assert_eq!(global_cycle(&t, &contents), Some((Global::ShowAll, vec![])));
        // From folds of its own, the page goes back to OVERVIEW
        assert_eq!(global_cycle(&t, &[(9, 15)]).unwrap().0, Global::Overview);
        // Headlines with nothing but sub-headlines: CONTENTS shows them all, then SHOW ALL
        let t = c("* A\n** B\n");
        assert_eq!(global_cycle(&t, &[]), Some((Global::Overview, vec![(3, 8)])));
        assert_eq!(global_cycle(&t, &[(3, 8)]), Some((Global::Contents, vec![])));
        assert_eq!(global_cycle(&c("texte"), &[]), None);
    }

    #[test]
    fn subtree_moves_past_its_sibling() {
        let t = c("* A\ntexte\n** A1\n* B\n** B1\n* C\n");
        let up = move_subtree(&t, 16, true).unwrap();
        assert_eq!(up.text(&t), "* B\n** B1\n* A\ntexte\n** A1");
        assert_eq!(up.pos(16), 0);
        let down = move_subtree(&t, 0, false).unwrap();
        assert_eq!(down, up);
        assert_eq!(down.pos(0), 10);
        // The last one, no line break after it
        let t = c("* A\n** A1\n* B");
        assert_eq!(move_subtree(&t, 0, false).unwrap().text(&t), "* B\n* A\n** A1");
        // No sibling: not past a headline above, nor at the ends
        let t = c("* A\ntexte\n** A1\n** A2\n* B\n");
        assert_eq!(move_subtree(&t, 10, true), None);
        assert_eq!(move_subtree(&t, 16, false), None);
        assert_eq!(move_subtree(&t, 0, true), None);
        assert_eq!(move_subtree(&t, 22, false), None);
        assert_eq!(move_subtree(&t, 4, true), None);
    }
}
