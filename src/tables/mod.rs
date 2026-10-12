//! Org-mode tables drawn as real tables while the caret is elsewhere.
//!
//! The editor is a transparent textarea over a highlight layer, both holding the
//! same characters. To draw a table, the textarea gets a *view* of the page in
//! which every table the caret is not in is replaced by a block of as many lines
//! as the drawn table is high: a tag character (which table) on the first line,
//! filler characters on the others. The highlight layer draws the table over
//! those lines. The page itself is never changed: `View` maps the view back to
//! the page text and positions between the two.
//!
//! The view also hides the folded parts of the page (Emacs `TAB` on a headline): a
//! fold character at the end of the headline stands for the lines it hides.
//!
//! The view also draws the images (see `images`): a line holding only a link to
//! an image is collapsed the same way, from the link to the end of the line, into
//! as many lines as the image is high.
//!
//! The view also indents the text under a headline to the column of its title, as
//! Emacs' `org-indent-mode` does: each line gets `INDENT` characters in front of it,
//! which are not in the page either.

mod cells;
mod html;
mod view;

pub use cells::{move_cell, CellMove};
pub use html::to_html;
pub use view::{Shown, View};

/// First line of a collapsed table: `TAG_BASE + index` of the table in the view.
const TAG_BASE: u32 = 0xE000;
const MAX_TABLES: u32 = 0x1800;
/// The other lines of a collapsed table.
const FILL: char = '\u{2063}';
/// End of a folded headline: `FOLD_BASE + index` of the fold in the view.
const FOLD_BASE: u32 = 0xF800;
const MAX_FOLDS: u32 = 0x100;
/// Indentation of a line under a headline (a no-break space: one column wide
/// in every font, and very unlikely to start a line of the page).
pub const INDENT: char = '\u{a0}';

fn tag(i: usize) -> char {
    char::from_u32(TAG_BASE + i as u32).unwrap_or(FILL)
}

/// Index of the table a tag character stands for.
pub fn tag_index(c: char) -> Option<usize> {
    let n = c as u32;
    (TAG_BASE..TAG_BASE + MAX_TABLES).contains(&n).then(|| (n - TAG_BASE) as usize)
}

fn fold_tag(i: usize) -> char {
    char::from_u32(FOLD_BASE + i as u32).unwrap_or(FILL)
}

/// Index of the fold a fold character stands for.
pub fn fold_index(c: char) -> Option<usize> {
    let n = c as u32;
    (FOLD_BASE..FOLD_BASE + MAX_FOLDS).contains(&n).then(|| (n - FOLD_BASE) as usize)
}

pub fn is_fill(c: char) -> bool {
    c == FILL
}

/// Whether a line is a table line (`| a | b |`, `|---+---|`).
pub fn is_table_line(line: &[char]) -> bool {
    line.iter().find(|c| !c.is_whitespace()) == Some(&'|')
}

/// Char ranges of the tables of the page: from the start of their first line
/// to the end of their last one (newline excluded).
pub fn find_tables(chars: &[char]) -> Vec<(usize, usize)> {
    let mut tables: Vec<(usize, usize)> = Vec::new();
    let mut start = 0;
    let mut previous_was_table = false;
    loop {
        let end = chars[start..].iter().position(|&c| c == '\n').map_or(chars.len(), |n| start + n);
        let table = is_table_line(&chars[start..end]);
        if table {
            match tables.last_mut() {
                Some(t) if previous_was_table => t.1 = end,
                _ => tables.push((start, end)),
            }
        }
        previous_was_table = table;
        if end >= chars.len() { break; }
        start = end + 1;
    }
    tables
}

/// Whether a cell holds a number (org-mode right-aligns the columns of numbers).
fn is_number(cell: &str) -> bool {
    cell.chars().any(|c| c.is_ascii_digit())
        && cell.chars().all(|c| c.is_ascii_digit() || ".,%+-eE ".contains(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "* Titre\n| a | b |\n|---+---|\n| c | d |\nfin\n| x |";

    #[test]
    fn finds_tables() {
        let c: Vec<char> = PAGE.chars().collect();
        assert_eq!(find_tables(&c), vec![(8, 37), (42, 47)]);
        assert!(find_tables(&"a\nb".chars().collect::<Vec<_>>()).is_empty());
    }
}
