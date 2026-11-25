//! The agenda, as org-mode's: the dates of the headlines (`SCHEDULED:` and
//! `DEADLINE:` on the line under them, active timestamps `<…>` in their text),
//! the agenda of some days made from them, and the edits that plan a headline or
//! change its TODO state (done, a repeated one goes to its next date, another
//! gets its `CLOSED:` date).
//!
//! Markdown pages are read the same way for now (`## TODO Titre`, then
//! `SCHEDULED: <2026-11-24 Tue>`): `planning` and `stamps` are the only places
//! that read the dates of a page.

use serde::{Deserialize, Serialize};

use crate::edit::{parse_time, stamp_around, Stamp};
use crate::{Format, TodoKeywords};
mod edits;
pub use edits::*;

/// A day, counted from 1970-01-01.
pub type Day = i64;

/// The day of a date (Howard Hinnant's `days_from_civil`).
pub fn day_of(year: i32, month: u32, day: u32) -> Day {
    let y = if month <= 2 { year as i64 - 1 } else { year as i64 };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = month as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + day as i64 - 1;
    era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468
}

/// The date of a day: year, month, day.
pub fn date_of(d: Day) -> (i32, u32, u32) {
    let z = d + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    ((yoe + era * 400 + if month <= 2 { 1 } else { 0 }) as i32, month, day)
}

/// Day of the week: 0 Monday … 6 Sunday (1970-01-01 was a Thursday).
pub fn weekday(d: Day) -> u32 {
    (d + 3).rem_euclid(7) as u32
}

/// The Monday of the week of `d`.
pub fn week_start(d: Day) -> Day {
    d - weekday(d) as Day
}

impl Stamp {
    /// Its day.
    pub fn day_number(&self) -> Day {
        day_of(self.year, self.month, self.day)
    }

    /// The same timestamp on the day `d`.
    pub fn on(&self, d: Day) -> Stamp {
        let (year, month, day) = date_of(d);
        Stamp { year, month, day, ..self.clone() }
    }

    /// `n` days (`d`), weeks (`w`), months (`m`) or years (`y`) later; the day of
    /// the month kept when it can be (the 31st becomes the last day of a shorter
    /// month). `None` for another unit (hours).
    pub fn shifted(&self, n: i64, unit: char) -> Option<Stamp> {
        let months = match unit {
            'd' => return Some(self.on(self.day_number() + n)),
            'w' => return Some(self.on(self.day_number() + 7 * n)),
            'm' => n,
            'y' => 12 * n,
            _ => return None,
        };
        let total = self.year as i64 * 12 + self.month as i64 - 1 + months;
        let (year, month) = (total.div_euclid(12) as i32, total.rem_euclid(12) as u32 + 1);
        let next = if month == 12 { day_of(year + 1, 1, 1) } else { day_of(year, month + 1, 1) };
        let last = (next - day_of(year, month, 1)) as u32;
        Some(Stamp { year, month, day: self.day.min(last), ..self.clone() })
    }
}

/// How a repeated timestamp goes on once done: `+1w` a week after its date,
/// `++1w` to its first date in the future a week apart, `.+1w` a week after the
/// day it is done.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Repeat { Plus, CatchUp, Restart }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Repeater {
    pub kind: Repeat,
    pub n: i64,
    /// `d`, `w`, `m` or `y`.
    pub unit: char,
}

/// The repeater of a timestamp, from what follows its date (`+1w -2d`).
pub fn repeater(rest: &str) -> Option<Repeater> {
    rest.split_whitespace().find_map(|w| {
        let (kind, spec) = if let Some(s) = w.strip_prefix(".+") { (Repeat::Restart, s) }
            else if let Some(s) = w.strip_prefix("++") { (Repeat::CatchUp, s) }
            else { (Repeat::Plus, w.strip_prefix('+')?) };
        // A habit's `.+2d/4d`: the first interval
        let spec = spec.split('/').next()?;
        let unit = spec.chars().last().filter(|u| "dwmy".contains(*u))?;
        let n: i64 = spec[..spec.len() - 1].parse().ok()?;
        (n > 0).then_some(Repeater { kind, n, unit })
    })
}

/// The days before a deadline it shows in the agenda: its `-3d`, else 14 (org-mode's
/// `org-deadline-warning-days`).
pub fn warning_days(rest: &str) -> i64 {
    rest.split_whitespace().find_map(|w| {
        let spec = w.strip_prefix("--").or_else(|| w.strip_prefix('-'))?;
        let unit = spec.chars().last()?;
        let n: i64 = spec[..spec.len() - unit.len_utf8()].parse().ok()?;
        Some(n * match unit { 'd' => 1, 'w' => 7, 'm' => 30, 'y' => 365, _ => return None })
    }).unwrap_or(14)
}

/// The date a repeated timestamp goes to when its headline is done on `today`.
pub fn next_date(s: &Stamp, r: Repeater, today: Day) -> Option<Stamp> {
    match r.kind {
        Repeat::Plus => s.shifted(r.n, r.unit),
        Repeat::CatchUp => {
            let mut next = s.shifted(r.n, r.unit)?;
            while next.day_number() <= today { next = next.shifted(r.n, r.unit)?; }
            Some(next)
        }
        Repeat::Restart => s.on(today).shifted(r.n, r.unit),
    }
}

/// The days of `s` in `from..to`: its own, and those it repeats on after it.
fn occurrences(s: &Stamp, from: Day, to: Day) -> Vec<Day> {
    let base = s.day_number();
    let Some(r) = repeater(&s.rest) else {
        return if (from..to).contains(&base) { vec![base] } else { vec![] };
    };
    let mut out = vec![];
    // Days and weeks jump to the range; months and years step to it
    let (mut k, step) = match r.unit {
        'd' | 'w' => {
            let days = r.n * if r.unit == 'w' { 7 } else { 1 };
            (((from - base) / days).max(0), Some(days))
        }
        _ => (0, None),
    };
    loop {
        let d = match step {
            Some(days) => base + k * days,
            None => match s.shifted(k * r.n, r.unit) { Some(t) => t.day_number(), None => break },
        };
        if d >= to || k > 100_000 { break; }
        if d >= from { out.push(d); }
        k += 1;
    }
    out
}

/// The keys of a planning line, as org-mode writes them.
pub const SCHEDULED: &str = "SCHEDULED:";
pub const DEADLINE: &str = "DEADLINE:";
pub const CLOSED: &str = "CLOSED:";
pub const PLANNING: [&str; 3] = [SCHEDULED, DEADLINE, CLOSED];

/// The timestamps of a line: their char ranges.
pub fn stamps(line: &[char]) -> Vec<(usize, usize, Stamp)> {
    let mut out = vec![];
    let mut i = 0;
    while i < line.len() {
        if matches!(line[i], '<' | '[') && let Some((a, b, s)) = stamp_around(line, i) && a == i {
            out.push((a, b, s));
            i = b;
        } else {
            i += 1;
        }
    }
    out
}

/// The dates of a planning line (`SCHEDULED: <…> DEADLINE: <…>`), each with its
/// key and char range; `None` when the line is not one.
pub fn planning(line: &[char]) -> Option<Vec<(&'static str, usize, usize, Stamp)>> {
    let text: String = line.iter().collect();
    if !PLANNING.iter().any(|k| text.trim_start().starts_with(k)) { return None; }
    Some(stamps(line).into_iter().filter_map(|(a, b, s)| {
        let before: String = line[..a].iter().collect();
        let key = PLANNING.iter().find(|k| before.trim_end().ends_with(**k))?;
        Some((*key, a, b, s))
    }).collect())
}

/// A headline with what the agenda needs of it.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// Its line in the page (from 0).
    pub line: usize,
    pub level: usize,
    pub keyword: Option<String>,
    pub done: bool,
    /// Its title, without its keyword and its `:tags:`.
    pub title: String,
    pub scheduled: Option<Stamp>,
    pub deadline: Option<Stamp>,
    /// The active timestamps of the headline and its text (the planning line
    /// aside): their line, char range in it, and value.
    pub stamps: Vec<(usize, usize, usize, Stamp)>,
}

/// The text of a list item (`- [ ] texte`, `1. texte`) without its mark, its
/// checkbox and its timestamps; `None` when the line is no list item.
fn item_text(line: &[char]) -> Option<String> {
    let text: String = line.iter().collect();
    let t = text.trim_start();
    let rest = ["- ", "+ "].iter().find_map(|m| t.strip_prefix(m)).or_else(|| {
        let digits = t.chars().take_while(char::is_ascii_digit).count();
        (digits > 0).then(|| t[digits..].strip_prefix(". ").or_else(|| t[digits..].strip_prefix(") "))).flatten()
    })?;
    let rest = ["[ ] ", "[x] ", "[X] "].iter().find_map(|c| rest.strip_prefix(c)).unwrap_or(rest);
    let chars: Vec<char> = rest.chars().collect();
    let mut out = String::new();
    let mut at = 0;
    for (a, b, _) in stamps(&chars) {
        out.extend(&chars[at..a]);
        at = b;
    }
    out.extend(&chars[at..]);
    Some(out.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// The headlines of the page `text` of `format` (its TODO keywords: its
/// `#+TODO:` lines, else `keywords`; none when `None`). A list item with an
/// active timestamp (a note of the quick capture), and a line with one before
/// the first headline, are entries of their own: their text is their title.
pub fn entries(text: &str, keywords: Option<&TodoKeywords>, format: Format) -> Vec<Entry> {
    let keywords = keywords.map(|k| TodoKeywords::for_page(text, k));
    let lines: Vec<Vec<char>> = text.split('\n').map(|l| l.chars().collect()).collect();
    let mut out: Vec<Entry> = vec![];
    let mut own: Vec<Entry> = vec![];
    for (n, line) in lines.iter().enumerate() {
        if let Some(level) = format.heading_level(line) {
            let mut start = level + usize::from(line.get(level) == Some(&' '));
            let kw = keywords.as_ref().and_then(|k| k.keyword(line, format));
            if let Some(k) = kw { start = (k.end + 1).min(line.len()); }
            let mut title: String = line[start.min(line.len())..].iter().collect::<String>().trim().to_string();
            if let Some((before, last)) = title.rsplit_once(' ').or(Some(("", title.as_str())))
                && last.len() > 1 && last.starts_with(':') && last.ends_with(':')
            {
                title = before.trim_end().to_string();
            }
            let plan = lines.get(n + 1).and_then(|l| planning(l)).unwrap_or_default();
            let date = |key: &str| plan.iter().find(|p| p.0 == key).map(|p| p.3.clone());
            out.push(Entry {
                line: n,
                level,
                keyword: kw.map(|k| line[k.start..k.end].iter().collect()),
                done: kw.is_some_and(|k| k.done),
                title,
                scheduled: date(SCHEDULED),
                deadline: date(DEADLINE),
                stamps: vec![],
            });
        }
        if out.last().is_some_and(|e| n == e.line + 1 && planning(line).is_some()) { continue; }
        let found: Vec<(usize, usize, usize, Stamp)> = stamps(line).into_iter().filter(|s| s.2.active).map(|(a, b, s)| (n, a, b, s)).collect();
        if found.is_empty() { continue; }
        let item = item_text(line);
        match out.last_mut() {
            Some(e) if item.is_none() && e.line != n || item.is_none() && format.heading_level(line).is_some() => e.stamps.extend(found),
            _ => {
                let title = item.unwrap_or_else(|| {
                    let mut t = String::new();
                    let mut at = 0;
                    for (_, a, b, _) in &found { t.extend(&line[at..*a]); at = *b; }
                    t.extend(&line[at..]);
                    t.split_whitespace().collect::<Vec<_>>().join(" ")
                });
                own.push(Entry { line: n, level: 0, keyword: None, done: false, title, scheduled: None, deadline: None, stamps: found });
            }
        }
    }
    out.retain(|e| e.keyword.is_some() || e.scheduled.is_some() || e.deadline.is_some() || !e.stamps.is_empty());
    out.extend(own);
    out.sort_by_key(|e| e.line);
    out
}

/// The day a page is named after (`2026-11-24`, `journals/2026-11-24`), if any.
pub fn day_of_name(name: &str) -> Option<Day> {
    let last = name.rsplit('/').next()?;
    (last.len() == 10 && last.as_bytes()[4] == b'-' && last.as_bytes()[7] == b'-')
        .then(|| Stamp::from_fields(last, "", true, "")).flatten().map(|s| s.day_number())
}

/// The days of the calendar of the month of `d`: from the Monday of its first
/// week to the Sunday of its last one.
pub fn month_grid(d: Day) -> Vec<Day> {
    let (y, m, _) = date_of(d);
    let first = day_of(y, m, 1);
    let next = if m == 12 { day_of(y + 1, 1, 1) } else { day_of(y, m + 1, 1) };
    (week_start(first)..week_start(next - 1) + 7).collect()
}

/// The day of `days` (the pages of the journal) closest before `from`, or after
/// it with `forward`; `None` when there is none.
pub fn journal_step(days: impl IntoIterator<Item = Day>, from: Day, forward: bool) -> Option<Day> {
    let other = days.into_iter().filter(|&d| if forward { d > from } else { d < from });
    if forward { other.min() } else { other.max() }
}

/// Why a headline is in the agenda on a day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum When {
    /// The page named after the day (a page of the journal).
    Page,
    Deadline,
    Scheduled,
    Timestamp,
}

/// A line of the agenda.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub day: Day,
    pub when: When,
    pub time: Option<(u32, u32)>,
    /// Days late (a deadline passed, a scheduled day gone); below 0, days left
    /// before a deadline shown early. 0 on its own day.
    pub late: i64,
    /// Its entry among those given.
    pub entry: usize,
    /// For a timestamp: which of its entry's.
    pub stamp: usize,
}

/// The agenda of the `days` days from `from`, `today` being today: each entry on
/// the days of its dates (and their repeats). Not done, a scheduled day gone and
/// a deadline passed or close show today too, as org-mode shows them.
pub fn agenda(entries: &[Entry], from: Day, days: i64, today: Day) -> Vec<Item> {
    let to = from + days;
    let has_today = (from..to).contains(&today);
    let mut out = vec![];
    for (i, e) in entries.iter().enumerate() {
        let item = |day, when, time, late, stamp| Item { day, when, time, late, entry: i, stamp };
        for (k, (_, _, _, s)) in e.stamps.iter().enumerate() {
            out.extend(occurrences(s, from, to).into_iter().map(|d| item(d, When::Timestamp, s.time, 0, k)));
        }
        for (when, s) in [(When::Scheduled, &e.scheduled), (When::Deadline, &e.deadline)] {
            let Some(s) = s else { continue };
            let base = s.day_number();
            // A repeat before today is no date of its own
            out.extend(occurrences(s, from, to).into_iter().filter(|&d| d == base || d >= today).map(|d| item(d, when, s.time, 0, 0)));
            if !has_today || e.done || base == today { continue; }
            if base < today || (when == When::Deadline && base - today <= warning_days(&s.rest)) {
                out.push(item(today, when, s.time, today - base, 0));
            }
        }
    }
    out.sort_by_key(|it| (it.day, it.time.is_none(), it.time, it.when, it.late.signum() != 1, it.entry));
    out
}

#[cfg(test)]
mod tests;
