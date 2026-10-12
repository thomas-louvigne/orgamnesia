//! Tab / Shift+Tab in a table: to the next / previous cell, aligning the table
//! as org-mode does.

use super::{is_number, is_table_line};

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
}
