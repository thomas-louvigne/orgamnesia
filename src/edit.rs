//! What the editor's commands do to the text, as pure functions over the
//! characters (`&[char]`, positions in chars): the editor component only
//! applies the result to its textarea. See also `motion.rs`.

use crate::keybindings::EditorAction;
use crate::motion::{self, Hashtags, TagSyntax};

/// Lines moved by the page-up / page-down actions.
pub const PAGE_LINES: i32 = 20;

/// Start and end (before the `\n`) of the line holding `pos`.
pub fn line_bounds(chars: &[char], pos: usize) -> (usize, usize) {
    let start = chars[..pos].iter().rposition(|&c| c == '\n').map(|i| i + 1).unwrap_or(0);
    let end   = chars[pos..].iter().position(|&c| c == '\n').map(|i| pos + i).unwrap_or(chars.len());
    (start, end)
}

/// A move of lines one line up or down: the char range `a..b` becomes `text`,
/// and the selection is now `sel`.
#[derive(Debug, PartialEq)]
pub struct LineMove {
    pub a: usize,
    pub b: usize,
    pub text: String,
    pub sel: (usize, usize),
}

/// Move the lines of the selection `start..end` above the line before them (`up`)
/// or below the line after them; `None` at the top or the bottom of the text.
pub fn move_line(chars: &[char], start: usize, end: usize, up: bool) -> Option<LineMove> {
    // A selection ending at the start of a line doesn't take that line
    let last = if end > start && end > 0 && chars[end - 1] == '\n' { end - 1 } else { end };
    let (first, _) = line_bounds(chars, start);
    let (_, block_end) = line_bounds(chars, last.max(start));
    let block: String = chars[first..block_end].iter().collect();
    if up {
        if first == 0 { return None; }
        let (prev, _) = line_bounds(chars, first - 1);
        let above: String = chars[prev..first - 1].iter().collect();
        let shift = first - prev;
        Some(LineMove { a: prev, b: block_end, text: format!("{block}\n{above}"), sel: (start - shift, end - shift) })
    } else {
        if block_end >= chars.len() { return None; }
        let (_, next_end) = line_bounds(chars, block_end + 1);
        let below: String = chars[block_end + 1..next_end].iter().collect();
        let shift = next_end - block_end;
        Some(LineMove { a: first, b: next_end, text: format!("{below}\n{block}"), sel: (start + shift, end + shift) })
    }
}

/// Position `delta` lines down (negative: up) keeping the column when possible.
pub fn move_lines(chars: &[char], pos: usize, delta: i32) -> usize {
    let (mut start, _) = line_bounds(chars, pos);
    let col = pos - start;
    for _ in 0..delta.unsigned_abs() {
        if delta > 0 {
            let (_, end) = line_bounds(chars, start);
            if end >= chars.len() { return chars.len(); }
            start = end + 1;
        } else {
            if start == 0 { return pos; }
            start = line_bounds(chars, start - 1).0;
        }
    }
    let (s, e) = line_bounds(chars, start);
    (s + col).min(e)
}

pub fn word_end_forward(chars: &[char], pos: usize) -> usize {
    let mut i = pos;
    while i < chars.len() && !chars[i].is_alphanumeric() { i += 1; }
    while i < chars.len() &&  chars[i].is_alphanumeric() { i += 1; }
    i
}

pub fn word_start_backward(chars: &[char], pos: usize) -> usize {
    let mut i = pos;
    while i > 0 && !chars[i - 1].is_alphanumeric() { i -= 1; }
    while i > 0 &&  chars[i - 1].is_alphanumeric() { i -= 1; }
    i
}

/// If the line `[start..end)` is an org heading (`*`… followed by a space or EOL),
/// its level (number of leading `*`).
fn heading_level(chars: &[char], start: usize, end: usize) -> Option<usize> {
    let mut i = start;
    while i < end && chars[i] == '*' { i += 1; }
    let level = i - start;
    if level > 0 && (i >= end || chars[i] == ' ') { Some(level) } else { None }
}

/// Heading level in effect at `caret`: the current line if it is a heading,
/// otherwise the nearest heading above it. Defaults to 1 when none is found.
pub fn nearest_heading_level(chars: &[char], caret: usize) -> usize {
    let (mut line_start, _) = line_bounds(chars, caret);
    loop {
        let (_, line_end) = line_bounds(chars, line_start);
        if let Some(l) = heading_level(chars, line_start, line_end) { return l; }
        if line_start == 0 { return 1; }
        line_start = line_bounds(chars, line_start - 1).0;
    }
}

/// Where to insert a new heading of the current level, and what: after the
/// current line, `\n` and the stars. On a list item, a new item instead, as
/// org's `M-RET`.
pub fn new_heading(chars: &[char], caret: usize) -> (usize, String) {
    let (_, line_end) = line_bounds(chars, caret);
    if let Some(bullet) = motion::new_item_bullet(chars, caret) {
        return (line_end, format!("\n{bullet}"));
    }
    let level = nearest_heading_level(chars, caret);
    (line_end, format!("\n{} ", "*".repeat(level)))
}

/// What Emacs `C-k` removes at `pos`: up to the end of the line, or the line
/// break itself when already there.
pub fn kill_line_range(chars: &[char], pos: usize) -> (usize, usize) {
    let (_, line_end) = line_bounds(chars, pos);
    if pos == line_end && pos < chars.len() { (pos, pos + 1) } else { (pos, line_end) }
}

/// Indentation step of the configuration files (JSON, as the app writes them).
const JSON_INDENT: usize = 2;

/// Tab (`back`: Shift+Tab) in a JSON file, the selection `start..end`: the
/// char range `from..to` becomes `text`, and the new selection.
/// Without a selection, Tab goes to the next indentation step, Shift+Tab takes
/// one step off the line; a selection has its lines indented, or one step taken off.
pub fn json_indent(chars: &[char], start: usize, end: usize, back: bool) -> (usize, usize, String, (usize, usize)) {
    let (line, _) = line_bounds(chars, start);
    if start == end && !back {
        let n = JSON_INDENT - (start - line) % JSON_INDENT;
        return (start, start, " ".repeat(n), (start + n, start + n));
    }
    // The lines of the selection; one it only reaches the start of is left out
    let last = if end > start && chars[end - 1] == '\n' { end - 1 } else { end };
    let (_, to) = line_bounds(chars, last.max(start));
    let mut text = String::new();
    // Chars taken off (or added) before `start`, and before `end`
    let (mut before_start, mut before_end) = (0isize, 0isize);
    let mut at = line;
    for (i, l) in chars[line..to].split(|&c| c == '\n').enumerate() {
        if i > 0 { text.push('\n'); }
        let change = if back {
            -(l.iter().take(JSON_INDENT).take_while(|&&c| c == ' ').count() as isize)
        } else {
            JSON_INDENT as isize
        };
        let kept = if change < 0 { &l[(-change) as usize..] } else { l };
        if change > 0 { text.push_str(&" ".repeat(change as usize)); }
        text.extend(kept);
        // A change at a line start counts for a position from that line start on
        let shift = |pos: usize| if pos >= at { change.max(-((pos - at) as isize)) } else { 0 };
        before_start += shift(start);
        before_end += shift(end);
        at += l.len() + 1;
    }
    let moved = |pos: usize, by: isize| (pos as isize + by).max(line as isize) as usize;
    (line, to, text, (moved(start, before_start), moved(end, before_end)))
}

/// What "delete forward" removes: the selection, else the next character. At the
/// end of the text, on the empty line after a last newline: that newline (the
/// caret goes up to the end of the line above).
pub fn delete_forward_range(chars: &[char], start: usize, end: usize) -> (usize, usize) {
    if start != end { return (start, end); }
    if start >= chars.len() && chars.last() == Some(&'\n') { return (chars.len() - 1, chars.len()); }
    (start, (start + 1).min(chars.len()))
}

/// The pair a typed character belongs to: `(` and `)` → `("(", ")")`…
pub fn pair_of(key: &str) -> Option<(&'static str, &'static str)> {
    match key {
        "(" | ")" => Some(("(", ")")),
        "[" | "]" => Some(("[", "]")),
        "{" | "}" => Some(("{", "}")),
        "\"" => Some(("\"", "\"")),
        "'" => Some(("'", "'")),
        _ => None,
    }
}

/// Typing this character with no selection also inserts its closing one.
pub fn auto_closes(key: &str) -> bool {
    matches!(key, "(" | "[" | "{" | "\"")
}

/// Where a cursor movement action takes the caret from `pos`; `None` when the
/// action is not a movement.
pub fn motion_target(action: EditorAction, chars: &[char], pos: usize) -> Option<usize> {
    use EditorAction::*;
    Some(match action {
        ForwardChar => (pos + 1).min(chars.len()),
        BackwardChar => pos.saturating_sub(1),
        NextLine => move_lines(chars, pos, 1),
        PreviousLine => move_lines(chars, pos, -1),
        ForwardWord => word_end_forward(chars, pos),
        BackwardWord => word_start_backward(chars, pos),
        BeginningOfBuffer => 0,
        EndOfBuffer => chars.len(),
        ScrollDown => move_lines(chars, pos, PAGE_LINES),
        ScrollUp => move_lines(chars, pos, -PAGE_LINES),
        ForwardSentence => motion::forward_sentence_in_table(chars, pos).unwrap_or_else(|| motion::forward_sentence(chars, pos)),
        BackwardSentence => motion::backward_sentence_in_table(chars, pos).unwrap_or_else(|| motion::backward_sentence(chars, pos)),
        ForwardParagraph => motion::forward_paragraph(chars, pos),
        BackwardParagraph => motion::backward_paragraph(chars, pos),
        BackwardElement => motion::backward_element(chars, pos),
        ForwardElement => motion::forward_element(chars, pos),
        BackToIndentation => motion::back_to_indentation(chars, pos),
        NextHeading => motion::next_heading(chars, pos),
        PreviousHeading => motion::previous_heading(chars, pos),
        BackwardHeading => motion::backward_heading(chars, pos),
        ForwardHeading => motion::forward_heading(chars, pos),
        UpHeading => motion::up_heading(chars, pos),
        BeginningOfLine => line_bounds(chars, pos).0,
        EndOfLine => line_bounds(chars, pos).1,
        _ => return None,
    })
}

/// Position of the start of line `n` (0-based); the end of the text past the last line.
pub fn line_start(chars: &[char], n: usize) -> usize {
    if n == 0 { return 0; }
    chars.iter().enumerate().filter(|&(_, &c)| c == '\n').nth(n - 1)
        .map_or(chars.len(), |(i, _)| i + 1)
}

/// Bold, italic…: wrap the selection `start..end` in `marker` (`*gras*`), or unwrap it
/// when it already is (markers just around it, or at its ends). Without a selection,
/// the two markers are inserted with the caret between them (or removed if the caret
/// is between two). Returns the range to replace, its new text and the new selection.
pub fn toggle_emphasis(chars: &[char], start: usize, end: usize, marker: char) -> (usize, usize, String, usize, usize) {
    let inner: String = chars[start..end].iter().collect();
    if start > 0 && chars[start - 1] == marker && chars.get(end) == Some(&marker) {
        return (start - 1, end + 1, inner, start - 1, end - 1);
    }
    if end - start >= 2 && chars[start] == marker && chars[end - 1] == marker {
        let inside: String = chars[start + 1..end - 1].iter().collect();
        return (start, end, inside, start, end - 2);
    }
    (start, end, format!("{marker}{inner}{marker}"), start + 1, end + 1)
}

/// The page a link at `pos` points to: `[[target]]`, `[[target][text]]`, a
/// `#tag` or an org-mode `:tag:` (per `tags`).
pub fn link_at(chars: &[char], pos: usize, tags: TagSyntax) -> Option<String> {
    wiki_link_at(chars, pos, tags.hashtags)
        .or_else(|| tags.org.links.then(|| motion::org_tag_at(chars, pos, tags.org)).flatten())
}

fn wiki_link_at(chars: &[char], pos: usize, hashtags: Hashtags) -> Option<String> {
    let n = chars.len();
    let mut i = 0;
    while i < n {
        if let Some((tag, end)) = motion::hashtag_at(chars, i, hashtags) {
            if pos >= i && pos < end { return Some(tag); }
            i = end;
            continue;
        }
        if i + 1 >= n { break; }
        if chars[i] != '[' || chars[i + 1] != '[' {
            i += 1;
            continue;
        }
        let link_start = i;
        let content_start = i + 2;
        // End of the target: `]]` or `][`
        let mut j = content_start;
        while j < n && !(chars[j] == ']' && j + 1 < n && (chars[j + 1] == ']' || chars[j + 1] == '[')) {
            j += 1;
        }
        if j + 1 >= n { break; }
        let target: String = chars[content_start..j].iter().collect();
        let link_end = if chars[j + 1] == ']' {
            j + 2
        } else {
            // [[target][text]]
            let mut k = j + 2;
            while k + 1 < n && !(chars[k] == ']' && chars[k + 1] == ']') { k += 1; }
            if k + 1 >= n { i += 1; continue; }
            k + 2
        };
        if pos >= link_start && pos < link_end {
            return Some(target.trim().to_string());
        }
        i = link_end;
    }
    None
}

pub use orgamnesia_core::search::replace_ranges;

/// Without `match_case`, case and accents are ignored ("regle" finds "Règle").
pub fn find_all(chars: &[char], query: &str, match_case: bool) -> Vec<(usize, usize)> {
    let opts = orgamnesia_core::SearchOptions { match_case, whole_word: false };
    orgamnesia_core::search::find_all(chars, query, opts)
}

/// A `[[target]]` / `[[target][description]]` link of the text.
#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    /// Its range.
    pub start: usize,
    pub end: usize,
    pub target: String,
    pub description: String,
}

/// The link the caret at `pos` is on (`C-c C-l` there edits it).
pub fn link_around(chars: &[char], pos: usize) -> Option<Link> {
    let (ls, le) = line_bounds(chars, pos);
    let text = |a: usize, b: usize| -> String { chars[a..b].iter().collect() };
    let mut i = ls;
    while i + 1 < le {
        if chars[i] != '[' || chars[i + 1] != '[' { i += 1; continue; }
        let Some(t) = (i + 2..le).find(|&j| matches!(chars[j], ']' | '[')) else { break };
        let (description, end) = match chars.get(t + 1) {
            Some(']') if chars[t] == ']' => (String::new(), t + 2),
            Some('[') if chars[t] == ']' => match (t + 2..le.saturating_sub(1)).find(|&j| chars[j] == ']' && chars[j + 1] == ']') {
                Some(d) => (text(t + 2, d), d + 2),
                None => { i += 2; continue; }
            },
            _ => { i += 2; continue; }
        };
        if (i..end).contains(&pos) {
            return Some(Link { start: i, end, target: text(i + 2, t), description });
        }
        i = end;
    }
    None
}

/// The text of a link to `target`, shown as `description` when there is one.
pub fn format_link(target: &str, description: &str) -> String {
    let (target, description) = (target.trim(), description.trim());
    if description.is_empty() || description == target { format!("[[{target}]]") } else { format!("[[{target}][{description}]]") }
}

/// An org-mode timestamp: `<2026-10-26 Mon>`, `[2026-10-26 Mon 14:30 +1w]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Stamp {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    /// Hours and minutes.
    pub time: Option<(u32, u32)>,
    /// `<…>` (in the agenda) rather than `[…]`.
    pub active: bool,
    /// What follows the date and time, kept as it is (a repeater `+1w`, a warning `-2d`…).
    pub rest: String,
}

impl Stamp {
    /// From the values of a date field (`2026-10-26`) and a time field (`14:30`, or empty).
    pub fn from_fields(date: &str, time: &str, active: bool, rest: &str) -> Option<Self> {
        let mut d = date.trim().splitn(3, '-').map(|p| p.parse::<u32>().ok());
        let (year, month, day) = (d.next()??, d.next()??, d.next()??);
        if !(1..=12).contains(&month) || !(1..=31).contains(&day) { return None; }
        let time = if time.trim().is_empty() { None } else { Some(parse_time(time.trim())?) };
        Some(Self { year: year as i32, month, day, time, active, rest: rest.trim().to_string() })
    }

    /// The date as a date field takes it.
    pub fn date_field(&self) -> String { format!("{:04}-{:02}-{:02}", self.year, self.month, self.day) }

    /// The time as a time field takes it (empty without one).
    pub fn time_field(&self) -> String { self.time.map(|(h, m)| format!("{h:02}:{m:02}")).unwrap_or_default() }

    /// The text of the timestamp, its day named as org-mode does by default.
    pub fn text(&self) -> String {
        let (open, close) = if self.active { ('<', '>') } else { ('[', ']') };
        let mut s = format!("{open}{} {}", self.date_field(), weekday(self.year, self.month, self.day));
        if self.time.is_some() { s = format!("{s} {}", self.time_field()); }
        if !self.rest.is_empty() { s = format!("{s} {}", self.rest); }
        format!("{s}{close}")
    }
}

fn parse_time(s: &str) -> Option<(u32, u32)> {
    let (h, m) = s.split_once(':')?;
    let (h, m) = (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?);
    (h < 24 && m < 60 && s.len() <= 5).then_some((h, m))
}

/// English abbreviation of the day of a date (Sakamoto's method).
fn weekday(year: i32, month: u32, day: u32) -> &'static str {
    const OFFSETS: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let y = if month < 3 { year - 1 } else { year };
    let n = (y + y / 4 - y / 100 + y / 400 + OFFSETS[month as usize - 1] + day as i32).rem_euclid(7);
    ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"][n as usize]
}

/// The timestamp the caret at `pos` is on, and its range (`C-c .` there edits it).
pub fn stamp_around(chars: &[char], pos: usize) -> Option<(usize, usize, Stamp)> {
    let (ls, le) = line_bounds(chars, pos);
    for i in ls..le {
        let close = match chars[i] { '<' => '>', '[' => ']', _ => continue };
        let Some(end) = (i + 1..le).find(|&j| chars[j] == close) else { continue };
        if !(i..=end).contains(&pos) { continue; }
        let inner: String = chars[i + 1..end].iter().collect();
        let mut words = inner.split_whitespace();
        let Some(date) = words.next().filter(|d| d.len() == 10 && d.as_bytes()[4] == b'-' && d.as_bytes()[7] == b'-') else { continue };
        let mut rest: Vec<&str> = words.collect();
        // The day name, then the time
        if rest.first().is_some_and(|w| w.chars().all(|c| c.is_alphabetic() || c == '.')) { rest.remove(0); }
        let time = rest.first().and_then(|w| parse_time(w)).map(|_| rest.remove(0)).unwrap_or("");
        if let Some(stamp) = Stamp::from_fields(date, time, close == '>', &rest.join(" ")) {
            return Some((i, end + 1, stamp));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(s: &str) -> Vec<char> { s.chars().collect() }

    #[test]
    fn links_to_insert_or_edit() {
        let t = c("voir [[Page]] et [[https://x.org][le site]] fin");
        assert_eq!(link_around(&t, 7), Some(Link { start: 5, end: 13, target: "Page".into(), description: String::new() }));
        let l = link_around(&t, 40).unwrap();
        assert_eq!((l.start, l.end, l.target.as_str(), l.description.as_str()), (17, 43, "https://x.org", "le site"));
        assert_eq!(link_around(&t, 13), None);
        assert_eq!(link_around(&c("[[a] b"), 1), None);
        assert_eq!(format_link(" Page ", ""), "[[Page]]");
        assert_eq!(format_link("Page", "Page"), "[[Page]]");
        assert_eq!(format_link("Page", "la page"), "[[Page][la page]]");
    }

    #[test]
    fn timestamps_to_insert_or_edit() {
        let s = Stamp::from_fields("2026-10-26", "", true, "").unwrap();
        assert_eq!(s.text(), "<2026-10-26 Mon>");
        let s = Stamp::from_fields("2024-02-29", "9:05", false, "+1w").unwrap();
        assert_eq!(s.text(), "[2024-02-29 Thu 09:05 +1w]");
        assert_eq!((s.date_field(), s.time_field()), ("2024-02-29".into(), "09:05".into()));
        assert_eq!(Stamp::from_fields("2026-13-01", "", true, ""), None);
        assert_eq!(Stamp::from_fields("2026-10-01", "25:00", true, ""), None);
        let t = c("DEADLINE: <2026-01-05 lun. 10:00 -2d> et [2026-01-06 Tue] [x]");
        let (a, b, s) = stamp_around(&t, 15).unwrap();
        assert_eq!((a, b), (10, 37));
        assert_eq!(s, Stamp { year: 2026, month: 1, day: 5, time: Some((10, 0)), active: true, rest: "-2d".into() });
        assert_eq!(s.text(), "<2026-01-05 Mon 10:00 -2d>");
        assert!(stamp_around(&t, 45).unwrap().2.time.is_none());
        assert_eq!(stamp_around(&t, 60), None);
        assert_eq!(stamp_around(&t, 3), None);
    }

    #[test]
    fn lines() {
        let t = c("ab\ncde\n\nf");
        assert_eq!(line_bounds(&t, 4), (3, 6));
        assert_eq!(move_lines(&t, 1, 1), 4);
        assert_eq!(move_lines(&t, 5, 1), 7);
        assert_eq!(move_lines(&t, 5, -1), 2);
        assert_eq!(move_lines(&t, 1, -1), 1);
        assert_eq!(move_lines(&t, 0, 50), t.len());
        assert_eq!(line_start(&t, 2), 7);
        assert_eq!(line_start(&t, 9), t.len());
    }

    #[test]
    fn moves_lines() {
        let t = c("un\ndeux\ntrois");
        // The caret in "deux", moved up: before "un"
        let m = move_line(&t, 4, 4, true).unwrap();
        assert_eq!((m.a, m.b, m.text.as_str(), m.sel), (0, 7, "deux\nun", (1, 1)));
        // Down: after "trois", the last line (no line break after it)
        let m = move_line(&t, 4, 4, false).unwrap();
        assert_eq!((m.a, m.b, m.text.as_str(), m.sel), (3, 13, "trois\ndeux", (10, 10)));
        assert!(move_line(&t, 1, 1, true).is_none());
        assert!(move_line(&t, 9, 9, false).is_none());
        // Two lines selected (ending at the start of the third): both move, not the third
        let m = move_line(&t, 0, 8, false).unwrap();
        assert_eq!((m.a, m.b, m.text.as_str(), m.sel), (0, 13, "trois\nun\ndeux", (6, 14)));
    }

    #[test]
    fn words() {
        let t = c("un, deux trois");
        assert_eq!(word_end_forward(&t, 2), 8);
        assert_eq!(word_start_backward(&t, 8), 4);
        assert_eq!(word_start_backward(&t, 0), 0);
    }

    #[test]
    fn kill_line_takes_the_line_break_at_end_of_line() {
        let t = c("abc\ndef");
        assert_eq!(kill_line_range(&t, 1), (1, 3));
        assert_eq!(kill_line_range(&t, 3), (3, 4));
        assert_eq!(kill_line_range(&t, 7), (7, 7));
    }

    #[test]
    fn json_tab_indents() {
        // The next step of two spaces
        assert_eq!(json_indent(&c("{\n\"a\""), 2, 2, false), (2, 2, "  ".into(), (4, 4)));
        assert_eq!(json_indent(&c("{\n \"a\""), 3, 3, false), (3, 3, " ".into(), (4, 4)));
        // Shift+Tab: one step off the line, the caret with its text
        assert_eq!(json_indent(&c("{\n    \"a\""), 7, 7, true), (2, 9, "  \"a\"".into(), (5, 5)));
        assert_eq!(json_indent(&c("x\n \"a\""), 3, 3, true), (2, 6, "\"a\"".into(), (2, 2)));
        // A selection: its lines, not the one it only reaches the start of
        let t = c("{\n\"a\",\n\"b\"\n}");
        assert_eq!(json_indent(&t, 2, 11, false), (2, 10, "  \"a\",\n  \"b\"".into(), (4, 15)));
        let t = c("  a\n  b\nc");
        assert_eq!(json_indent(&t, 0, 8, true), (0, 7, "a\nb".into(), (0, 4)));
    }

    #[test]
    fn delete_forward() {
        let t = c("abc");
        assert_eq!(delete_forward_range(&t, 0, 2), (0, 2));
        assert_eq!(delete_forward_range(&t, 1, 1), (1, 2));
        assert_eq!(delete_forward_range(&t, 3, 3), (3, 3));
        // At the end, after a last newline: that newline
        assert_eq!(delete_forward_range(&c("ab\n"), 3, 3), (2, 3));
        assert_eq!(delete_forward_range(&c("ab\n"), 2, 2), (2, 3));
    }

    #[test]
    fn new_heading_uses_the_level_in_effect() {
        let t = c("* a\n** b\ntexte");
        assert_eq!(new_heading(&t, 11), (14, "\n** ".to_string()));
        assert_eq!(new_heading(&c("rien"), 2), (4, "\n* ".to_string()));
        // On a list item: a new item
        assert_eq!(new_heading(&c("- a\n  suite"), 7), (11, "\n- ".to_string()));
        assert_eq!(new_heading(&c("  9) [X] a"), 4), (10, "\n  10) [ ] ".to_string()));
    }

    #[test]
    fn pairs() {
        assert_eq!(pair_of(")"), Some(("(", ")")));
        assert_eq!(pair_of("a"), None);
        assert!(auto_closes("[") && !auto_closes("'") && !auto_closes(")"));
    }

    #[test]
    fn motions() {
        let t = c("ab\ncd");
        assert_eq!(motion_target(EditorAction::EndOfLine, &t, 3), Some(5));
        assert_eq!(motion_target(EditorAction::BeginningOfLine, &t, 4), Some(3));
        assert_eq!(motion_target(EditorAction::BackwardChar, &t, 0), Some(0));
        assert_eq!(motion_target(EditorAction::ForwardChar, &t, 5), Some(5));
        assert_eq!(motion_target(EditorAction::Copy, &t, 0), None);
    }

    #[test]
    fn emphasis_toggles() {
        let t = c("un mot ici");
        assert_eq!(toggle_emphasis(&t, 3, 6, '*'), (3, 6, "*mot*".into(), 4, 7));
        let t = c("un *mot* ici");
        assert_eq!(toggle_emphasis(&t, 4, 7, '*'), (3, 8, "mot".into(), 3, 6));
        assert_eq!(toggle_emphasis(&t, 3, 8, '*'), (3, 8, "mot".into(), 3, 6));
        assert_eq!(toggle_emphasis(&t, 4, 7, '_'), (4, 7, "_mot_".into(), 5, 8));
        // No selection: a pair of markers, the caret inside; again, they go away
        assert_eq!(toggle_emphasis(&c("ab"), 1, 1, '+'), (1, 1, "++".into(), 2, 2));
        assert_eq!(toggle_emphasis(&c("a++b"), 2, 2, '+'), (1, 3, String::new(), 1, 1));
    }

    #[test]
    fn links_under_the_cursor() {
        let t = c("voir [[Ma page]] et [[cible][texte]] #tag fin");
        assert_eq!(link_at(&t, 8, Hashtags::Dashes.into()).as_deref(), Some("Ma page"));
        assert_eq!(link_at(&t, 30, Hashtags::Dashes.into()).as_deref(), Some("cible"));
        assert_eq!(link_at(&t, 39, Hashtags::Dashes.into()).as_deref(), Some("tag"));
        assert_eq!(link_at(&t, 39, Hashtags::Off.into()), None);
        assert_eq!(link_at(&t, 1, Hashtags::Dashes.into()), None);
        assert_eq!(link_at(&c("* Titre :projet:"), 11, Hashtags::Off.into()).as_deref(), Some("projet"));
        assert_eq!(link_at(&c("voir :exemple: ici"), 7, Hashtags::Off.into()).as_deref(), Some("exemple"));
        let off = TagSyntax { hashtags: Hashtags::Off, org: motion::OrgTags { links: false, dashes: false } };
        assert_eq!(link_at(&c("voir :exemple: ici"), 7, off), None);
    }

    #[test]
    fn finds_every_occurrence() {
        let c: Vec<char> = "Élan, élan ÉLAN aaa".chars().collect();
        assert_eq!(find_all(&c, "élan", false), vec![(0, 4), (6, 10), (11, 15)]);
        assert_eq!(find_all(&c, "élan", true), vec![(6, 10)]);
        assert_eq!(find_all(&c, "aa", false), vec![(16, 18)]);
        assert!(find_all(&c, "", false).is_empty());
        // Accents ignored too, unless matching case
        let c: Vec<char> = "La règle, les Règles, regle".chars().collect();
        assert_eq!(find_all(&c, "regle", false), vec![(3, 8), (14, 19), (22, 27)]);
        assert_eq!(find_all(&c, "regle", true), vec![(22, 27)]);
    }
}
