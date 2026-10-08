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
//! The view also indents the text under a headline to the column of its title, as
//! Emacs' `org-indent-mode` does: each line gets `INDENT` characters in front of it,
//! which are not in the page either.

use crate::highlight;

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

/// A table of the page drawn in place of its text.
#[derive(Debug, Clone, PartialEq)]
pub struct Collapsed {
    /// Its text in the page.
    pub raw: String,
    /// Where it starts in the page.
    pub start: usize,
    /// Its length in the page (chars).
    pub len: usize,
    /// Lines it takes in the view.
    pub lines: usize,
}

/// Lines of the page hidden under a folded headline.
#[derive(Debug, Clone, PartialEq)]
pub struct Fold {
    /// Its text in the page: from the `\n` ending the headline to the end of the
    /// last hidden line.
    pub raw: String,
    /// Where it starts in the page.
    pub start: usize,
    /// Its length in the page (chars).
    pub len: usize,
}

/// A piece of the view, as `View::walk` reads it.
enum Piece {
    /// A character of the page.
    Char(char),
    /// A character of a collapsed table's block: the table, the line within the block.
    Table(usize, usize),
    /// The character standing for a fold.
    Fold(usize),
}

/// The page as the textarea shows it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct View {
    pub text: String,
    /// The collapsed tables, in order.
    pub tables: Vec<Collapsed>,
    /// Width of the text, in characters: how wide a drawn table can be.
    pub width: usize,
    /// The indented lines: where they start in the page, and by how many columns.
    pub indents: Vec<(usize, usize)>,
    /// The folded parts, in order.
    pub folds: Vec<Fold>,
}

impl View {
    /// The view of `content` where, with `collapse`, every table is collapsed but the one
    /// holding the page position `keep` (if any), the text being `width` characters
    /// wide. `lines_of` gives the lines a table takes once drawn. With `indent`, the
    /// text under a headline is indented to the column of its title. `folds` are the
    /// page ranges to hide (see `Fold`); those that don't fit the page are ignored.
    pub fn build(
        content: &str, keep: Option<usize>, width: usize, mut lines_of: impl FnMut(&str) -> usize,
        collapse: bool, indent: bool, folds: &[(usize, usize)],
    ) -> Self {
        let chars: Vec<char> = content.chars().collect();
        let mut all_indents = if indent { heading_indents(&chars) } else { vec![] };
        let mut indents = Vec::new();
        // The folds that fit the page (a `\n` at both ends), not inside another one
        let mut hidden: Vec<(usize, usize)> = Vec::new();
        let mut sorted = folds.to_vec();
        sorted.sort();
        for (a, b) in sorted {
            let fits = a < b && b <= chars.len() && chars[a] == '\n' && (b == chars.len() || chars[b] == '\n');
            if !fits || hidden.last().is_some_and(|&(_, end)| a < end) { continue; }
            if hidden.len() as u32 >= MAX_FOLDS { break; }
            hidden.push((a, b));
        }
        // The hidden lines get no indentation
        all_indents.retain(|&(at, _)| !hidden.iter().any(|&(a, b)| a < at && at <= b));
        let mut text = String::with_capacity(content.len() + all_indents.len() * 3);
        let mut next = 0;
        // Copy the page from `a` to `b`, each line with its indentation
        let mut copy = |text: &mut String, a: usize, b: usize| {
            for (p, c) in chars[a..b].iter().map(Some).chain([None]).enumerate() {
                let p = a + p;
                while next < all_indents.len() && all_indents[next].0 < p { next += 1; }
                if let Some(&(at, n)) = all_indents.get(next).filter(|&&(at, _)| at == p) {
                    text.extend(std::iter::repeat_n(INDENT, n));
                    indents.push((at, n));
                    next += 1;
                }
                if let Some(&c) = c { text.push(c); }
            }
        };
        // What the view replaces, in order: folds, and tables (but those folded)
        let mut blocks: Vec<(usize, usize, bool)> = hidden.iter().map(|&(a, b)| (a, b, true)).collect();
        if collapse {
            for (start, end) in find_tables(&chars) {
                let folded = hidden.iter().any(|&(a, b)| start > a && start < b);
                if !folded && !keep.is_some_and(|k| (start..=end).contains(&k)) {
                    blocks.push((start, end, false));
                }
            }
        }
        blocks.sort();
        let mut tables = Vec::new();
        let mut folded = Vec::new();
        let mut at = 0;
        for (start, end, fold) in blocks {
            if !fold && tables.len() as u32 >= MAX_TABLES { continue; }
            copy(&mut text, at, start);
            let raw: String = chars[start..end].iter().collect();
            if fold {
                text.push(fold_tag(folded.len()));
                folded.push(Fold { raw, start, len: end - start });
            } else {
                let lines = lines_of(&raw).max(1);
                text.push(tag(tables.len()));
                for _ in 1..lines {
                    text.push('\n');
                    text.push(FILL);
                }
                tables.push(Collapsed { raw, start, len: end - start, lines });
            }
            at = end;
        }
        copy(&mut text, at, chars.len());
        View { text, tables, width, indents, folds: folded }
    }

    /// The page ranges folded in `text` (this view, possibly edited since): a fold
    /// whose character is no longer at the end of a line is opened.
    pub fn fold_ranges(&self, text: &str) -> Vec<(usize, usize)> {
        let chars: Vec<char> = text.chars().collect();
        let mut out = Vec::new();
        let mut content = 0;
        let mut last = None;
        self.walk(text, |i, piece| match piece {
            Piece::Char(_) => { content += 1; last = None; }
            Piece::Table(t, _) => {
                if last != Some(t) { content += self.tables[t].len; last = Some(t); }
            }
            Piece::Fold(f) => {
                let len = self.folds[f].len;
                if chars.get(i + 1).is_none_or(|&c| c == '\n') { out.push((content, content + len)); }
                content += len;
                last = None;
            }
        });
        out
    }

    /// The run of `INDENT`s starting the line of the view position `pos` in `text`,
    /// when `pos` is in it or right after it.
    pub fn indent_run(&self, text: &str, pos: usize) -> Option<(usize, usize)> {
        if self.indents.is_empty() { return None; }
        let chars: Vec<char> = text.chars().collect();
        let pos = pos.min(chars.len());
        let ls = chars[..pos].iter().rposition(|&c| c == '\n').map_or(0, |i| i + 1);
        let n = chars[ls..].iter().take_while(|&&c| c == INDENT).count();
        (n > 0 && pos <= ls + n).then_some((ls, ls + n))
    }

    /// Walk `text` (this view, possibly edited since) calling `on` for each piece:
    /// a page character, each char of a collapsed table's block (line within the
    /// block), or a fold. What is left of a block whose tag was deleted is skipped.
    fn walk(&self, text: &str, mut on: impl FnMut(usize, Piece)) {
        let chars: Vec<char> = text.chars().collect();
        let indented = !self.indents.is_empty();
        let mut line_start = true;
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if indented && line_start && c == INDENT {
                // Indentation of the view, not in the page
                i += 1;
                continue;
            }
            line_start = c == '\n';
            if let Some(f) = fold_index(c).filter(|&f| f < self.folds.len()) {
                on(i, Piece::Fold(f));
                i += 1;
            } else if let Some(t) = tag_index(c).filter(|&t| t < self.tables.len()) {
                on(i, Piece::Table(t, 0));
                i += 1;
                let mut line = 0;
                while i + 1 < chars.len() && chars[i] == '\n' && chars[i + 1] == FILL {
                    line += 1;
                    on(i, Piece::Table(t, line));
                    on(i + 1, Piece::Table(t, line));
                    i += 2;
                }
            } else if c == '\n' && chars.get(i + 1) == Some(&FILL) {
                // A line of a block whose first line was deleted
                line_start = false;
                i += 2;
            } else {
                if c != FILL && tag_index(c).is_none() && fold_index(c).is_none() { on(i, Piece::Char(c)); }
                i += 1;
            }
        }
    }

    /// The page text for `text`, this view after edits in the textarea.
    pub fn to_content(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut last = None;
        self.walk(text, |_, piece| match piece {
            Piece::Char(c) => { out.push(c); last = None; }
            Piece::Table(t, _) if last != Some(t) => { out.push_str(&self.tables[t].raw); last = Some(t); }
            Piece::Table(..) => {}
            Piece::Fold(f) => { out.push_str(&self.folds[f].raw); last = None; }
        });
        out
    }

    /// Page position of the view position `pos` in `text` (this view, possibly
    /// edited). Inside a collapsed table: the start of the row at about the same height.
    pub fn content_pos(&self, text: &str, pos: usize) -> usize {
        let mut content = 0;
        let mut found = None;
        let mut last = None;
        // Page position of the table being walked through
        let mut table_at = 0;
        self.walk(text, |i, piece| {
            if found.is_some() { return; }
            match piece {
                Piece::Char(_) => {
                    if i >= pos { found = Some(content); }
                    content += 1;
                    last = None;
                }
                // Before the fold: the end of the headline
                Piece::Fold(f) => {
                    if i >= pos { found = Some(content); }
                    content += self.folds[f].len;
                    last = None;
                }
                Piece::Table(t, line) => {
                    let table = &self.tables[t];
                    if last != Some(t) {
                        table_at = content;
                        content += table.len;
                        last = Some(t);
                    }
                    if i >= pos {
                        found = Some(table_at + row_start(&table.raw, line, table.lines));
                    }
                }
            }
        });
        found.unwrap_or(content)
    }

    /// View position of the page position `pos`; a position inside a collapsed
    /// table goes to the start of its block, one inside a fold before the fold.
    pub fn view_pos(&self, pos: usize) -> usize {
        // Indentation of the lines starting at or before `p` (a line start goes after it)
        let indent = |p: usize| -> isize {
            self.indents.iter().take_while(|&&(at, _)| at <= p).map(|&(_, n)| n as isize).sum()
        };
        // Collapsed tables and folds: where they start, their length, the chars they take
        let mut blocks: Vec<(usize, usize, usize)> = self.tables.iter()
            .map(|t| (t.start, t.len, 2 * t.lines - 1))
            .chain(self.folds.iter().map(|f| (f.start, f.len, 1)))
            .collect();
        blocks.sort();
        let mut shift: isize = 0;
        for (start, len, chars) in blocks {
            if pos <= start { break; }
            if pos < start + len {
                return (start as isize + shift + indent(start)) as usize;
            }
            shift += chars as isize - len as isize;
        }
        (pos as isize + shift + indent(pos)).max(0) as usize
    }
}

/// The lines under a headline: where they start and their indentation,
/// the width of the stars of the headline and the space after them.
fn heading_indents(chars: &[char]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut level = 0;
    let mut start = 0;
    loop {
        let end = chars[start..].iter().position(|&c| c == '\n').map_or(chars.len(), |p| start + p);
        let line = &chars[start..end];
        let stars = line.iter().take_while(|&&c| c == '*').count();
        if stars > 0 && matches!(line.get(stars), Some(' ') | None) {
            level = stars;
        } else if level > 0 {
            out.push((start, level + 1));
        }
        if end == chars.len() { break; }
        start = end + 1;
    }
    out
}

/// Position in `raw` of the start of the row shown at line `line` of `lines`.
fn row_start(raw: &str, line: usize, lines: usize) -> usize {
    let rows: Vec<&str> = raw.split('\n').collect();
    let row = (line * rows.len() / lines.max(1)).min(rows.len() - 1);
    rows[..row].iter().map(|r| r.chars().count() + 1).sum()
}

/// Shares of the width (in %) the columns get when the table is too wide for
/// `width` characters, so that its cells wrap: shares follow the length of the
/// columns' text, but none gets less than 80 % of an equal share (40 % of two
/// columns), even if that takes more lines. `None` when the table fits.
fn column_shares(lengths: &[usize], width: usize) -> Option<Vec<f64>> {
    let n = lengths.len();
    // Each cell also takes about 3 characters of padding and border
    let needed: usize = lengths.iter().map(|l| l + 3).sum();
    if n < 2 || width == 0 || needed <= width { return None; }
    let floor = 0.8 / n as f64;
    let mut fixed = vec![false; n];
    loop {
        let free = 1.0 - floor * fixed.iter().filter(|&&f| f).count() as f64;
        let total: usize = (0..n).filter(|&i| !fixed[i]).map(|i| lengths[i].max(1)).sum();
        let share = |i: usize| if fixed[i] { floor } else { free * lengths[i].max(1) as f64 / total as f64 };
        let below: Vec<usize> = (0..n).filter(|&i| !fixed[i] && share(i) < floor).collect();
        if below.is_empty() {
            return Some((0..n).map(|i| (share(i) * 1000.0).round() / 10.0).collect());
        }
        for i in below { fixed[i] = true; }
    }
}

/// A table's text as an HTML table, for text `width` characters wide.
/// Rows above the first `|---|` line are headers.
pub fn to_html(raw: &str, width: usize) -> String {
    let lines: Vec<&str> = raw.split('\n').collect();
    let is_rule = |l: &str| l.trim_start().starts_with("|-");
    let header_rows = lines.iter().position(|l| is_rule(l)).unwrap_or(0);
    let cells_of = |l: &str| -> Vec<String> {
        let t = l.trim();
        let t = t.strip_prefix('|').unwrap_or(t);
        let t = t.strip_suffix('|').unwrap_or(t);
        t.split('|').map(|c| c.trim().to_string()).collect()
    };
    let columns = lines.iter().filter(|l| !is_rule(l)).map(|l| cells_of(l).len()).max().unwrap_or(1);
    let lengths: Vec<usize> = (0..columns).map(|i| {
        lines.iter().filter(|l| !is_rule(l))
            .map(|l| cells_of(l).get(i).map_or(0, |c| c.chars().count()))
            .max().unwrap_or(0)
    }).collect();
    // Columns of numbers to the right, as in the text (see `align`)
    let numeric: Vec<bool> = (0..columns).map(|i| {
        let filled: Vec<String> = lines.iter().enumerate()
            .filter(|(n, l)| !is_rule(l) && *n >= header_rows)
            .filter_map(|(_, l)| cells_of(l).get(i).cloned())
            .filter(|c| !c.is_empty())
            .collect();
        !filled.is_empty() && filled.iter().filter(|c| is_number(c)).count() * 2 > filled.len()
    }).collect();
    let mut out = match column_shares(&lengths, width) {
        Some(shares) => {
            let cols: String = shares.iter().map(|s| format!("<col style='width: {s}%'>")).collect();
            format!("<table class='org-table wide'><colgroup>{cols}</colgroup>")
        }
        None => String::from("<table class='org-table'>"),
    };
    let mut rule_before = false;
    for (n, line) in lines.iter().enumerate() {
        if is_rule(line) {
            rule_before = true;
            continue;
        }
        let cells = cells_of(line);
        let empty = cells.iter().all(|c| c.is_empty());
        let mut class = Vec::new();
        if rule_before && n > 0 { class.push("rule"); }
        if empty { class.push("empty"); }
        rule_before = false;
        out.push_str(&format!("<tr class='{}'>", class.join(" ")));
        let cell_tag = if n < header_rows { "th" } else { "td" };
        for (i, &num) in numeric.iter().enumerate() {
            let cell = cells.get(i).map(String::as_str).unwrap_or("");
            let class = if num { " class='num'" } else { "" };
            out.push_str(&format!("<{cell_tag}{class}>{}</{cell_tag}>", highlight::inline(cell)));
        }
        out.push_str("</tr>");
    }
    out.push_str("</table>");
    out
}

/// Where Tab / Shift+Tab takes the caret in a table.
#[derive(Debug, PartialEq)]
pub enum CellMove {
    /// To this position.
    Caret(usize),
    /// Replace `from..to` with `text`, then put the caret at `caret`
    /// (a new row past the last cell, a completed `|-` rule…).
    Edit { from: usize, to: usize, text: String, caret: usize },
}

/// A line of a table.
struct Row {
    /// A rule (`|---+---|`, or just `|-` being typed).
    rule: bool,
    /// Position of the `|` opening each cell.
    opens: Vec<usize>,
    /// Text of each cell, trimmed.
    cells: Vec<String>,
}

impl Row {
    fn read(chars: &[char], start: usize, end: usize) -> Self {
        let line = &chars[start..end];
        let rule = line.iter().skip_while(|c| c.is_whitespace()).take(2).eq(['|', '-'].iter());
        let pipes: Vec<usize> = (start..end).filter(|&i| chars[i] == '|').collect();
        let closed = line.iter().rev().find(|c| !c.is_whitespace()) == Some(&'|');
        let opens = pipes[..if closed { pipes.len() - 1 } else { pipes.len() }].to_vec();
        let cells = if rule { vec![] } else {
            opens.iter().enumerate()
                .map(|(i, &p)| chars[p + 1..pipes.get(i + 1).copied().unwrap_or(end)].iter().collect::<String>().trim().to_string())
                .collect()
        };
        Row { rule, opens, cells }
    }
}

/// Whether a cell holds a number (org-mode right-aligns the columns of numbers).
fn is_number(cell: &str) -> bool {
    cell.chars().any(|c| c.is_ascii_digit())
        && cell.chars().all(|c| c.is_ascii_digit() || ".,%+-eE ".contains(c))
}

/// The table's rows aligned as org-mode does: every column as wide as its
/// widest cell, a space on each side of the text, rules drawn across, columns
/// of numbers to the right. Returns the lines and, for each data row, where
/// each cell's text starts in its line.
fn align(rows: &[Row], indent: &str) -> (Vec<String>, Vec<Vec<usize>>) {
    let columns = rows.iter().map(|r| r.cells.len()).max().unwrap_or(0).max(1);
    fn cell(r: &Row, c: usize) -> &str { r.cells.get(c).map_or("", String::as_str) }
    let width = |s: &str| s.chars().count();
    let widths: Vec<usize> = (0..columns)
        .map(|c| rows.iter().filter(|r| !r.rule).map(|r| width(cell(r, c))).max().unwrap_or(0).max(1))
        .collect();
    let numeric: Vec<bool> = (0..columns).map(|c| {
        let filled: Vec<&str> = rows.iter().filter(|r| !r.rule).map(|r| cell(r, c)).filter(|t| !t.is_empty()).collect();
        !filled.is_empty() && filled.iter().filter(|t| is_number(t)).count() * 2 > filled.len()
    }).collect();
    let mut lines = Vec::new();
    let mut starts = Vec::new();
    for r in rows {
        if r.rule {
            let dashes: Vec<String> = widths.iter().map(|&w| "-".repeat(w + 2)).collect();
            lines.push(format!("{indent}|{}|", dashes.join("+")));
            starts.push(vec![]);
            continue;
        }
        let mut line = format!("{indent}|");
        let mut row_starts = Vec::new();
        for (c, &w) in widths.iter().enumerate() {
            let text = cell(r, c);
            let pad = " ".repeat(w - width(text));
            line.push(' ');
            row_starts.push(line.chars().count() + if numeric[c] { pad.len() } else { 0 });
            if numeric[c] { line.push_str(&pad); line.push_str(text); } else { line.push_str(text); line.push_str(&pad); }
            line.push_str(" |");
        }
        lines.push(line);
        starts.push(row_starts);
    }
    (lines, starts)
}

/// Tab (`forward`) or Shift+Tab in an org table, as org-mode does: the table is
/// aligned again (see `align`), then the caret goes to the start of the next or
/// previous cell; Tab past the last cell adds a row. `None` when `pos` is not in
/// a table.
pub fn move_cell(chars: &[char], pos: usize, forward: bool) -> Option<CellMove> {
    let line_start = |p: usize| chars[..p].iter().rposition(|&c| c == '\n').map_or(0, |n| n + 1);
    let line_end = |p: usize| chars[p..].iter().position(|&c| c == '\n').map_or(chars.len(), |n| p + n);
    let here = line_start(pos.min(chars.len()));
    if !is_table_line(&chars[here..line_end(here)]) { return None; }
    // The table's lines
    let mut first = here;
    while first > 0 {
        let prev = line_start(first - 1);
        if !is_table_line(&chars[prev..first - 1]) { break; }
        first = prev;
    }
    let mut rows = Vec::new();
    let mut here_row = 0;
    let mut start = first;
    let mut table_end = first;
    loop {
        let end = line_end(start);
        if !is_table_line(&chars[start..end]) { break; }
        if start == here { here_row = rows.len(); }
        rows.push(Row::read(chars, start, end));
        table_end = end;
        if end >= chars.len() { break; }
        start = end + 1;
    }
    let columns = rows.iter().map(|r| r.cells.len()).max().unwrap_or(0).max(1);

    // Every cell, as (row, column), the short rows counted with the columns they get
    let cells: Vec<(usize, usize)> = rows.iter().enumerate()
        .filter(|(_, r)| !r.rule)
        .flat_map(|(n, _)| (0..columns).map(move |c| (n, c)))
        .collect();
    let index = |row: usize, col: usize| cells.iter().position(|&x| x == (row, col));
    let row = &rows[here_row];
    let target = if row.rule {
        if forward { cells.iter().position(|&(r, _)| r > here_row) }
        else { cells.iter().rposition(|&(r, _)| r < here_row).or(Some(0)) }
    } else {
        // The cell the caret is in; before the first `|`, just before the first cell
        match row.opens.iter().filter(|&&p| p < pos).count() {
            0 => {
                let i = index(here_row, 0).unwrap_or(0);
                Some(if forward { i } else { i.saturating_sub(1) })
            }
            n => {
                let i = index(here_row, n - 1).unwrap_or(0);
                Some(if forward { i + 1 } else { i.saturating_sub(1) })
            }
        }
    };
    // Past the last cell: a new row
    let target = match target.and_then(|t| cells.get(t).copied()) {
        Some(cell) => cell,
        None => {
            rows.push(Row { rule: false, opens: vec![], cells: vec![] });
            (rows.len() - 1, 0)
        }
    };

    let indent: String = chars[first..].iter().take_while(|c| *c == &' ' || *c == &'\t').collect();
    let (lines, starts) = align(&rows, &indent);
    let text = lines.join("\n");
    let line_at: usize = lines[..target.0].iter().map(|l| l.chars().count() + 1).sum();
    let caret = first + line_at + starts[target.0][target.1];
    let unchanged = chars[first..table_end].iter().copied().eq(text.chars());
    Some(if unchanged {
        CellMove::Caret(caret)
    } else {
        CellMove::Edit { from: first, to: table_end, text, caret }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "* Titre\n| a | b |\n|---+---|\n| c | d |\nfin\n| x |";

    fn view(keep: Option<usize>) -> View {
        View::build(PAGE, keep, 80, |raw| raw.split('\n').count() + 1, true, false, &[])
    }

    #[test]
    fn finds_tables() {
        let c: Vec<char> = PAGE.chars().collect();
        assert_eq!(find_tables(&c), vec![(8, 37), (42, 47)]);
        assert!(find_tables(&"a\nb".chars().collect::<Vec<_>>()).is_empty());
    }

    #[test]
    fn collapsed_tables_map_back_to_the_page() {
        let v = view(None);
        assert_eq!(v.tables.len(), 2);
        assert_eq!(v.text.lines().count(), 1 + 4 + 1 + 2);
        assert_eq!(v.to_content(&v.text), PAGE);
        // An edit outside the tables
        let edited = v.text.replacen("fin", "FIN!", 1);
        assert_eq!(v.to_content(&edited), PAGE.replacen("fin", "FIN!", 1));
        // The table holding the kept position stays as text
        let v = view(Some(20));
        assert_eq!(v.tables.len(), 1);
        assert!(v.text.contains("| c | d |"));
        assert_eq!(v.to_content(&v.text), PAGE);
    }

    #[test]
    fn text_under_headlines_is_indented() {
        let page = "intro\n* A\ntexte\n- item\n** B\n\n| t |\nfin";
        let v = View::build(page, None, 80, |raw| raw.split('\n').count(), true, true, &[]);
        let i = |n: usize| INDENT.to_string().repeat(n);
        assert_eq!(v.text, format!("intro\n* A\n{}texte\n{}- item\n** B\n{}\n{}\u{e000}\n{}fin", i(2), i(2), i(3), i(3), i(3)));
        assert_eq!(v.to_content(&v.text), page);
        // A line start goes after its indentation, and back
        let texte = page.find("texte").unwrap();
        assert_eq!(v.view_pos(texte), texte + 2);
        assert_eq!(v.content_pos(&v.text, texte + 2), texte);
        assert_eq!(v.content_pos(&v.text, texte + 1), texte);
        let fin = page.find("fin").unwrap();
        assert_eq!(v.content_pos(&v.text, v.view_pos(fin)), fin);
        // Typed in the indentation, a character goes to the start of the line
        let edited = v.text.replacen(&format!("{}texte", i(2)), &format!("{}X{}texte", i(1), i(1)), 1);
        assert_eq!(v.to_content(&edited), page.replacen("texte", "X\u{a0}texte", 1));
        // The indentation around a position
        assert_eq!(v.indent_run(&v.text, texte + 2), Some((texte, texte + 2)));
        assert_eq!(v.indent_run(&v.text, 2), None);
        // Not indented: the page as it is
        let plain = View::build(page, None, 80, |_| 1, false, false, &[]);
        assert_eq!(plain.text, page);
    }

    #[test]
    fn folded_lines_are_hidden_and_kept() {
        let page = "* A\ntexte\n| t |\n* B\nfin";
        let fold = (3, 15); // from the end of `* A` to the end of `| t |`
        let v = View::build(page, None, 80, |_| 1, true, false, &[fold]);
        assert_eq!(v.text, "* A\u{f800}\n* B\nfin");
        assert_eq!(v.to_content(&v.text), page);
        // Before the fold: the end of the headline; inside it: there too; after it: the next line
        assert_eq!(v.content_pos(&v.text, 3), 3);
        assert_eq!(v.view_pos(8), 3);
        let b = page.find("* B").unwrap();
        assert_eq!(v.view_pos(b), 5);
        assert_eq!(v.content_pos(&v.text, 5), b);
        // Typing above moves the fold along
        let edited = format!("x{}", v.text);
        assert_eq!(v.fold_ranges(&edited), vec![(4, 16)]);
        assert_eq!(v.to_content(&edited), format!("x{page}"));
        // A fold no longer at the end of a line is opened
        let joined = v.text.replacen("\u{f800}\n", "\u{f800}", 1);
        assert_eq!(v.fold_ranges(&joined), vec![]);
        // A range that doesn't fit the page is ignored
        assert_eq!(View::build(page, None, 80, |_| 1, true, false, &[(2, 15)]).text, View::build(page, None, 80, |_| 1, true, false, &[]).text);
    }

    #[test]
    fn positions_map_both_ways() {
        let v = view(None);
        let fin_page = PAGE.find("fin").unwrap();
        let fin_view = v.text.chars().collect::<String>().find("fin").map(|b| v.text[..b].chars().count()).unwrap();
        assert_eq!(v.view_pos(fin_page), fin_view);
        assert_eq!(v.content_pos(&v.text, fin_view), fin_page);
        assert_eq!(v.view_pos(3), 3);
        assert_eq!(v.content_pos(&v.text, 3), 3);
        // In a table's block: the start of a row of the table
        assert_eq!(v.content_pos(&v.text, 8), 8);
        // Third line of the block (4 lines for 3 rows): the start of the third row
        assert_eq!(v.content_pos(&v.text, 8 + 5), 28);
        assert_eq!(v.view_pos(20), 8);
    }

    #[test]
    fn deleting_across_a_table_keeps_or_drops_it_whole() {
        let v = view(None);
        let block_start = v.text.find(tag(0)).unwrap();
        let cut = format!("{}{}", &v.text[..block_start + tag(0).len_utf8()], &v.text[block_start + tag(0).len_utf8() + 4..]);
        assert!(v.to_content(&cut).contains("| a | b |\n|---+---|\n| c | d |"));
        assert_eq!(v.to_content(&v.text.replace(tag(0), "")), "* Titre\n\nfin\n| x |");
    }

    #[test]
    fn html_has_headers_and_empty_rows() {
        let html = to_html("| a | b |\n|---+---|\n| c |  |\n|  |  |", 80);
        assert!(html.contains("<th>a</th><th>b</th>"));
        assert!(html.contains("<tr class='rule'><td>c</td><td></td>"));
        assert!(html.contains("<tr class='empty'>"));
        // Columns of numbers to the right, their header too
        let html = to_html("| Livre | Pages |\n|---+---|\n| A | 560 |\n| B | 92 |", 80);
        assert!(html.contains("<th>Livre</th><th class='num'>Pages</th>") && html.contains("<td class='num'>560</td>"));
    }

    #[test]
    fn tab_moves_between_cells() {
        let t: Vec<char> = "x\n| a | b |\n|---+---|\n| c | d |\ny".chars().collect();
        // Already aligned: the caret only moves. From "a" to "b", over the rule to "c"
        assert_eq!(move_cell(&t, 4, true), Some(CellMove::Caret(8)));
        assert_eq!(move_cell(&t, 8, true), Some(CellMove::Caret(24)));
        assert_eq!(move_cell(&t, 26, false), Some(CellMove::Caret(8)));
        assert_eq!(move_cell(&t, 4, false), Some(CellMove::Caret(4)));
        // Past the last cell: a new row
        assert_eq!(move_cell(&t, 30, true), Some(CellMove::Edit {
            from: 2, to: 31, text: "| a | b |\n|---+---|\n| c | d |\n|   |   |".into(), caret: 34,
        }));
        assert_eq!(move_cell(&t, 0, true), None);
        assert_eq!(move_cell(&t, 32, true), None);
    }

    #[test]
    fn tab_aligns_the_table() {
        // A space on each side of the text, the row closed, the columns as wide as their widest cell
        let t: Vec<char> = "|a|b\n| ccc | d |".chars().collect();
        assert_eq!(move_cell(&t, 1, true), Some(CellMove::Edit {
            from: 0, to: 16, text: "| a   | b |\n| ccc | d |".into(), caret: 8,
        }));
        // Numbers to the right
        let t: Vec<char> = "| x | 1 |\n| yy | 100 |".chars().collect();
        let Some(CellMove::Edit { text, .. }) = move_cell(&t, 1, true) else { panic!() };
        assert_eq!(text, "| x  |   1 |\n| yy | 100 |");
        // Short rows get the missing cells
        let t: Vec<char> = "| a | b |\n| c |".chars().collect();
        let Some(CellMove::Edit { text, caret, .. }) = move_cell(&t, 12, true) else { panic!() };
        assert_eq!((text.as_str(), caret), ("| a | b |\n| c |   |", 16));
    }

    #[test]
    fn tab_completes_a_rule() {
        // Under the header, nothing below: the rule, then a new row
        let t: Vec<char> = "| Nom | Âge |\n|-".chars().collect();
        assert_eq!(move_cell(&t, 16, true), Some(CellMove::Edit {
            from: 0, to: 16, text: "| Nom | Âge |\n|-----+-----|\n|     |     |".into(), caret: 30,
        }));
        // A row below: the caret goes to its first cell
        let t: Vec<char> = "| a | bb |\n|-\n| c | d |".chars().collect();
        assert_eq!(move_cell(&t, 13, true), Some(CellMove::Edit {
            from: 0, to: 23, text: "| a | bb |\n|---+----|\n| c | d  |".into(), caret: 24,
        }));
        // Shift+Tab on a rule: the last cell of the row above
        let Some(CellMove::Edit { caret, .. }) = move_cell(&t, 13, false) else { panic!() };
        assert_eq!(caret, 6);
    }

    #[test]
    fn header_rows_are_the_ones_above_the_first_rule() {
        let html = to_html("| a | b |\n|---+---|", 80);
        assert!(html.contains("<th>a</th>"));
        assert!(!to_html("|---|\n| a |", 80).contains("<th>"));
    }

    #[test]
    fn wide_tables_keep_balanced_columns() {
        // Fits: no forced widths
        assert_eq!(column_shares(&[10, 20], 80), None);
        // Too wide: the short column still gets 40 %
        assert_eq!(column_shares(&[30, 600], 100), Some(vec![40.0, 60.0]));
        // Shares follow the text when both columns are long
        assert_eq!(column_shares(&[300, 600], 100), Some(vec![40.0, 60.0]));
        assert_eq!(column_shares(&[500, 500], 100), Some(vec![50.0, 50.0]));
        // Three columns: at least 80 % of a third each
        let s = column_shares(&[3, 10, 900], 100).unwrap();
        assert!(s[0] >= 26.6 && s[1] >= 26.6 && (s.iter().sum::<f64>() - 100.0).abs() < 0.2, "{s:?}");
        let html = to_html("| a | b |", 3);
        assert!(html.contains("<col style='width: 50%'>"));
    }
}
