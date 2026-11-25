//! What is under the caret: the page a link leads to, the link around it (to
//! edit it), the date stamp around it (to change it).

use super::*;

/// The page a link at `pos` points to: `[[target]]`, `[[target][text]]`, a
/// `#tag` or an org-mode `:tag:` (per `tags`); with the extended links, also
/// `[[target|text]]`, `[[Page#Titre]]` and `[text](Page.md)`, as read in a page of `format`.
pub fn linked_page_at(chars: &[char], pos: usize, tags: TagSyntax, format: crate::Format) -> Option<String> {
    let syntax = tags.links(format);
    let extended = (syntax != crate::links::LinkSyntax::NONE).then(|| {
        crate::links::scan(chars, syntax).into_iter().find(|f| pos >= f.start && pos < f.end).map(|f| f.target)
    }).flatten();
    extended.or_else(|| wiki_link_at(chars, pos, tags.hashtags))
        .or_else(|| tags.org.links.then(|| tags::org_tag_at(chars, pos, tags.org)).flatten())
}

fn wiki_link_at(chars: &[char], pos: usize, hashtags: Hashtags) -> Option<String> {
    let mut i = 0;
    while i < chars.len() {
        let (found, end) = match (hashtag_at(chars, i, hashtags), link_at(chars, i)) {
            (Some((tag, end)), _) => (tag, end),
            (None, Some(link)) => (link.target(chars).trim().to_string(), link.end),
            (None, None) => { i += 1; continue; }
        };
        if pos >= i && pos < end { return Some(found); }
        i = end;
    }
    None
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

pub(crate) fn parse_time(s: &str) -> Option<(u32, u32)> {
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
