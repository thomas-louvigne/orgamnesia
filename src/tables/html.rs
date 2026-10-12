//! A table's text drawn as an HTML table.

use super::is_number;
use crate::highlight;

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

#[cfg(test)]
mod tests {
    use super::*;

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
