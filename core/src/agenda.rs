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

/// A change of a page: its chars `a..b` become `text`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub a: usize,
    pub b: usize,
    pub text: String,
}

/// The text once `edits` (in the order of the text, not overlapping) are made.
pub fn apply(text: &str, edits: &[Edit]) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut at = 0;
    for e in edits {
        out.extend(&chars[at..e.a]);
        out.push_str(&e.text);
        at = e.b;
    }
    out.extend(&chars[at..]);
    out
}

/// Where the position `p` goes once `edits` are made: after an edit, moved by
/// it; inside one, kept; at an insertion, before it (a fold ending a headline
/// keeps hiding the planning line put under it).
pub fn moved(p: usize, edits: &[Edit]) -> usize {
    let mut delta = 0i64;
    for e in edits {
        if p >= e.b && !(e.a == e.b && p == e.a) {
            delta += e.text.chars().count() as i64 - (e.b - e.a) as i64;
        }
    }
    (p as i64 + delta).max(0) as usize
}

/// Char ranges of the lines of `text` (without their line break).
fn line_ranges(text: &str) -> Vec<(usize, usize)> {
    let mut out = vec![];
    let mut start = 0;
    for l in text.split('\n') {
        let n = l.chars().count();
        out.push((start, start + n));
        start += n + 1;
    }
    out
}

/// The headline the line `line` is under (or is), if any.
pub fn headline_of(text: &str, line: usize, format: Format) -> Option<usize> {
    text.split('\n').take(line + 1).enumerate()
        .filter(|(_, l)| format.heading_level_of(l).is_some())
        .map(|(n, _)| n).last()
}

/// The planning line `line` with `key` set to `stamp`, or taken away.
fn set_key(line: &str, key: &'static str, stamp: Option<&Stamp>) -> String {
    let chars: Vec<char> = line.chars().collect();
    let found = planning(&chars).unwrap_or_default().into_iter().find(|p| p.0 == key);
    let s = |a: usize, b: usize| chars[a..b].iter().collect::<String>();
    match (found, stamp) {
        (Some((_, a, b, _)), Some(st)) => format!("{}{}{}", s(0, a), st.text(), s(b, chars.len())),
        (Some((_, a, b, _)), None) => {
            let before = s(0, a);
            let k = before.trim_end().chars().count() - key.chars().count();
            let (left, right) = (s(0, k), s(b, chars.len()));
            let (left_t, right_t) = (left.trim_end(), right.trim_start());
            if left_t.is_empty() { format!("{left}{right_t}") }
            else if right_t.is_empty() { left_t.to_string() }
            else { format!("{left_t} {right_t}") }
        }
        (None, Some(st)) if line.trim().is_empty() => format!("{key} {}", st.text()),
        // org-mode puts CLOSED first
        (None, Some(st)) if key == CLOSED => {
            let indent: String = chars.iter().take_while(|c| c.is_whitespace()).collect();
            format!("{indent}{key} {} {}", st.text(), line.trim_start())
        }
        (None, Some(st)) => format!("{} {key} {}", line.trim_end(), st.text()),
        (None, None) => line.to_string(),
    }
}

/// The edit that sets the dates `changes` (`None`: taken away) of the headline at
/// line `line` of `text`: on its planning line, made under it when needed,
/// gone when it holds nothing more. `None` when nothing changes.
pub fn plan(text: &str, line: usize, changes: &[(&'static str, Option<Stamp>)]) -> Option<Edit> {
    let ranges = line_ranges(text);
    let lines: Vec<&str> = text.split('\n').collect();
    let (_, he) = *ranges.get(line)?;
    let next = lines.get(line + 1).filter(|l| planning(&l.chars().collect::<Vec<_>>()).is_some());
    let old = next.map_or("", |l| *l);
    let new = changes.iter().fold(old.to_string(), |l, (k, s)| set_key(&l, k, s.as_ref()));
    if new == old { return None; }
    Some(match next {
        Some(_) if new.trim().is_empty() => Edit { a: he, b: ranges[line + 1].1, text: String::new() },
        Some(_) => Edit { a: ranges[line + 1].0, b: ranges[line + 1].1, text: new },
        None => Edit { a: he, b: he, text: format!("\n{new}") },
    })
}

/// The edits that move the headline at line `line` of `text` to its next (or
/// previous) TODO state, as org-mode does: once done, a headline with a repeated
/// `SCHEDULED:` or `DEADLINE:` gets its next dates and its TODO state back;
/// another gets `CLOSED: [now]`, which goes when it is no longer done. In
/// Markdown, a checkbox is ticked or unticked instead. `None` when the line is
/// neither (or there are no keywords).
pub fn cycle_todo(text: &str, line: usize, keywords: &TodoKeywords, format: Format, forward: bool, now: &Stamp) -> Option<Vec<Edit>> {
    let keywords = TodoKeywords::for_page(text, keywords);
    let (ls, _) = *line_ranges(text).get(line)?;
    let chars: Vec<char> = text.split('\n').nth(line)?.chars().collect();
    let (a, b, with) = keywords.cycle(&chars, forward, format)?;
    let mut edits = vec![Edit { a: ls + a, b: ls + b, text: with.clone() }];
    if format.heading_level(&chars).is_none() { return Some(edits); }
    let was = keywords.keyword(&chars, format);
    let mut after = chars[..a].to_vec();
    after.extend(with.chars());
    after.extend(&chars[b..]);
    let done = keywords.keyword(&after, format).is_some_and(|k| k.done);
    let was_done = was.is_some_and(|k| k.done);
    let today = now.day_number();
    let entry = entries(text, Some(&keywords), format).into_iter().find(|e| e.line == line);
    let repeated: Vec<(&'static str, Stamp)> = entry.iter().flat_map(|e| [(SCHEDULED, &e.scheduled), (DEADLINE, &e.deadline)])
        .filter_map(|(k, s)| {
            let s = s.as_ref()?;
            Some((k, next_date(s, repeater(&s.rest)?, today)?))
        }).collect();
    let changes: Vec<(&'static str, Option<Stamp>)> = if done && !was_done && !repeated.is_empty() {
        // Back to the state it was in (the first one, from none)
        let back = match was {
            Some(k) => chars[k.start..k.end].iter().collect(),
            None => keywords.step(None, true)?,
        };
        let start = format.heading_level(&chars).map(|l| l + usize::from(chars.get(l) == Some(&' ')))?;
        edits[0] = match was {
            Some(k) => Edit { a: ls + k.start, b: ls + k.end, text: back },
            None => Edit { a: ls + start, b: ls + start, text: format!("{back} ") },
        };
        repeated.into_iter().map(|(k, s)| (k, Some(s))).collect()
    } else if done && !was_done {
        vec![(CLOSED, Some(Stamp { active: false, rest: String::new(), ..now.clone() }))]
    } else if was_done && !done {
        vec![(CLOSED, None)]
    } else {
        vec![]
    };
    if let Some(p) = plan(text, line, &changes) { edits.push(p); }
    Some(edits)
}

/// The date typed for a headline: `2026-12-01`, `+3d`, `-1w`, `+2` (days),
/// `+1m`, `today` or `.`, `tomorrow`, `yesterday`, a day of the week (`fri`,
/// `vendredi`: that day or the next), or their French names; a time `14:30` may
/// follow (or come alone, today). From `today`: the day and the time.
pub fn read_date(input: &str, today: Day) -> Option<(Day, Option<(u32, u32)>)> {
    const EN: [&str; 7] = ["monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday"];
    const FR: [&str; 7] = ["lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche"];
    let (mut day, mut time) = (None, None);
    for w in input.split_whitespace() {
        let w = w.to_lowercase();
        if time.is_none() && let Some(t) = parse_time(&w) { time = Some(t); continue; }
        if day.is_some() { return None; }
        day = Some(match w.as_str() {
            "." | "today" | "auj" | "aujourd'hui" | "aujourdhui" => today,
            "tomorrow" | "demain" => today + 1,
            "yesterday" | "hier" => today - 1,
            _ if w.starts_with(['+', '-']) => {
                let sign = if w.starts_with('-') { -1 } else { 1 };
                let spec = &w[1..];
                let (n, unit) = match spec.chars().last()? {
                    u @ ('d' | 'w' | 'm' | 'y' | 'j' | 's' | 'a') => (&spec[..spec.len() - 1], u),
                    _ => (spec, 'd'),
                };
                // French units: jours, semaines, ans
                let unit = match unit { 'j' => 'd', 's' => 'w', 'a' => 'y', u => u };
                let n: i64 = n.parse().ok()?;
                Stamp::from_fields("1970-01-01", "", true, "")?.on(today).shifted(sign * n, unit)?.day_number()
            }
            _ if w.len() == 10 && w.as_bytes()[4] == b'-' => Stamp::from_fields(&w, "", true, "")?.day_number(),
            _ => {
                let found: Vec<usize> = (0..7).filter(|&i| w.chars().count() >= 2 && (EN[i].starts_with(&w) || FR[i].starts_with(&w))).collect();
                let &[i] = found.as_slice() else { return None };
                today + (i as i64 - weekday(today) as i64).rem_euclid(7)
            }
        });
    }
    if day.is_none() && time.is_none() { return None; }
    Some((day.unwrap_or(today), time))
}

/// What was typed for a date is none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotADate;

/// The edit that sets (`None`: takes away) the `key` date (`SCHEDULED:`,
/// `DEADLINE:`) of the headline holding the line `line`, from what was typed
/// (`read_date`; `-` takes it away). Its repeater and warning are kept, and its
/// time unless another is typed.
pub fn plan_typed(text: &str, line: usize, format: Format, key: &'static str, typed: &str, today: Day) -> Result<Option<Edit>, NotADate> {
    let Some(head) = headline_of(text, line, format) else { return Ok(None) };
    if typed.trim() == "-" { return Ok(plan(text, head, &[(key, None)])); }
    let (day, time) = read_date(typed, today).ok_or(NotADate)?;
    let old = text.split('\n').nth(head + 1)
        .and_then(|l| planning(&l.chars().collect::<Vec<_>>()))
        .and_then(|p| p.into_iter().find(|p| p.0 == key)).map(|p| p.3);
    let base = old.unwrap_or(Stamp { year: 1970, month: 1, day: 1, time: None, active: true, rest: String::new() });
    let stamp = Stamp { time: time.or(base.time), active: true, ..base.on(day) };
    Ok(plan(text, head, &[(key, Some(stamp))]))
}

/// The edit that moves by `days` the date that put the entry `e` of the page
/// `text` in the agenda (`when`, and for a timestamp which one).
pub fn shift(text: &str, e: &Entry, when: When, stamp: usize, days: i64) -> Option<Edit> {
    match when {
        When::Scheduled | When::Deadline => {
            let (key, s) = if when == When::Scheduled { (SCHEDULED, e.scheduled.as_ref()?) } else { (DEADLINE, e.deadline.as_ref()?) };
            plan(text, e.line, &[(key, Some(s.on(s.day_number() + days)))])
        }
        When::Page => None,
        When::Timestamp => {
            let (line, a, b, s) = e.stamps.get(stamp)?;
            let (ls, _) = *line_ranges(text).get(*line)?;
            Some(Edit { a: ls + a, b: ls + b, text: s.on(s.day_number() + days).text() })
        }
    }
}

/// The date typed for a timestamp of the agenda, as `plan_typed` reads it.
pub fn typed_date(e: &Entry, when: When) -> String {
    let s = match when {
        When::Scheduled => e.scheduled.as_ref(),
        When::Deadline => e.deadline.as_ref(),
        When::Timestamp | When::Page => None,
    };
    s.map(|s| if s.time.is_some() { format!("{} {}", s.date_field(), s.time_field()) } else { s.date_field() }).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kw() -> TodoKeywords { TodoKeywords::parse("TODO | DONE") }
    fn st(date: &str, rest: &str) -> Stamp { Stamp::from_fields(date, "", true, rest).unwrap() }
    fn d(date: &str) -> Day { st(date, "").day_number() }
    fn now() -> Stamp { Stamp::from_fields("2026-11-24", "10:05", false, "").unwrap() }

    #[test]
    fn days_and_dates() {
        assert_eq!(day_of(1970, 1, 1), 0);
        assert_eq!(date_of(day_of(2024, 2, 29)), (2024, 2, 29));
        assert_eq!(date_of(day_of(2000, 3, 1) - 1), (2000, 2, 29));
        assert_eq!(weekday(d("2026-11-24")), 1); // a Tuesday
        assert_eq!(week_start(d("2026-11-29")), d("2026-11-23"));
        assert_eq!(st("2026-01-31", "").shifted(1, 'm').unwrap().date_field(), "2026-02-28");
        assert_eq!(st("2024-02-29", "").shifted(1, 'y').unwrap().date_field(), "2025-02-28");
        assert_eq!(st("2026-12-30", "").shifted(1, 'w').unwrap().date_field(), "2027-01-06");
    }

    #[test]
    fn repeaters() {
        let today = d("2026-11-24");
        let r = |rest: &str| repeater(rest).unwrap();
        assert_eq!(r("+1w -2d"), Repeater { kind: Repeat::Plus, n: 1, unit: 'w' });
        assert_eq!(r(".+2d/4d").kind, Repeat::Restart);
        assert_eq!(repeater("-2d"), None);
        assert_eq!(repeater("+2h"), None);
        assert_eq!(next_date(&st("2026-11-01", "+1w"), r("+1w"), today).unwrap().date_field(), "2026-11-08");
        assert_eq!(next_date(&st("2026-11-01", "++1w"), r("++1w"), today).unwrap().date_field(), "2026-11-29");
        assert_eq!(next_date(&st("2026-11-01", ".+1m"), r(".+1m"), today).unwrap().date_field(), "2026-12-24");
        assert_eq!((warning_days("-3d"), warning_days("+1w --1w"), warning_days("")), (3, 7, 14));
    }

    #[test]
    fn reads_the_entries() {
        let page = "#+TODO: TODO WAIT | DONE\nintro <2026-11-24 Tue>\n* TODO Écrire :x:\nSCHEDULED: <2026-11-20 Fri> DEADLINE: <2026-11-30 Mon -3d>\ntexte <2026-11-25 Wed 10:00> [2026-11-26 Thu]\n** WAIT Attendre\n* Rien\n* Réunion <2026-11-26 Thu 14:00>";
        let e = entries(page, Some(&kw()), Format::Org);
        // The line before the headlines: an entry of its own
        assert_eq!((e.len(), e[0].line, e[0].title.as_str()), (4, 1, "intro"));
        let e = &e[1..];
        assert_eq!((e[0].line, e[0].keyword.as_deref(), e[0].title.as_str()), (2, Some("TODO"), "Écrire"));
        assert_eq!(e[0].scheduled.as_ref().unwrap().date_field(), "2026-11-20");
        assert_eq!(e[0].deadline.as_ref().unwrap().rest, "-3d");
        assert_eq!(e[0].stamps.len(), 1);
        assert_eq!((e[0].stamps[0].0, e[0].stamps[0].3.time), (4, Some((10, 0))));
        assert_eq!(e[1].keyword.as_deref(), Some("WAIT"));
        assert_eq!((e[2].keyword.as_deref(), e[2].stamps[0].0), (None, 7));
        // Markdown: the same lines under a `#` headline
        let md = "## TODO Écrire\nDEADLINE: <2026-11-30 Mon>\n- [ ] acheter <2026-11-24 Tue> du pain";
        let e = entries(md, Some(&kw()), Format::Markdown);
        assert_eq!((e.len(), e[0].deadline.is_some(), e[0].stamps.len()), (2, true, 0));
        // A list item with a date: its own line in the agenda
        assert_eq!((e[1].line, e[1].title.as_str(), e[1].stamps.len()), (2, "acheter du pain", 1));
        assert_eq!(day_of_name("journals/2026-11-24"), Some(d("2026-11-24")));
        assert_eq!((day_of_name("2026-13-01"), day_of_name("Notes")), (None, None));
    }

    #[test]
    fn the_agenda_of_a_week() {
        let page = "* TODO Tard\nSCHEDULED: <2026-11-20 Fri>\n* TODO Bientôt\nDEADLINE: <2026-11-30 Mon -7d>\n* DONE Fait\nSCHEDULED: <2026-11-18 Wed>\n* Sport <2026-11-02 Mon 18:00 +1w>\n* Loin\nDEADLINE: <2027-01-30 Sat>";
        let e = entries(page, Some(&kw()), Format::Org);
        let today = d("2026-11-24");
        let got: Vec<(String, When, i64, usize)> = agenda(&e, week_start(today), 7, today).into_iter()
            .map(|i| (Stamp { ..st("1970-01-01", "") }.on(i.day).date_field(), i.when, i.late, i.entry)).collect();
        assert_eq!(got, vec![
            ("2026-11-23".into(), When::Timestamp, 0, 3),
            ("2026-11-24".into(), When::Deadline, -6, 1),
            ("2026-11-24".into(), When::Scheduled, 4, 0),
        ]);
        // The next week: the deadline on its day, the repeat
        let next: Vec<(When, usize)> = agenda(&e, week_start(today) + 7, 7, today).into_iter().map(|i| (i.when, i.entry)).collect();
        assert_eq!(next, vec![(When::Timestamp, 3), (When::Deadline, 1)]);
    }

    #[test]
    fn plans_a_headline() {
        let page = "* TODO A\ntexte\n* B";
        let s = st("2026-11-25", "");
        let e = plan(page, 0, &[(SCHEDULED, Some(s.clone()))]).unwrap();
        let page = apply(page, &[e]);
        assert_eq!(page, "* TODO A\nSCHEDULED: <2026-11-25 Wed>\ntexte\n* B");
        let page = apply(&page, &[plan(&page, 0, &[(DEADLINE, Some(s))]).unwrap()]);
        assert_eq!(page, "* TODO A\nSCHEDULED: <2026-11-25 Wed> DEADLINE: <2026-11-25 Wed>\ntexte\n* B");
        let page = apply(&page, &[plan(&page, 0, &[(SCHEDULED, None)]).unwrap()]);
        assert_eq!(page, "* TODO A\nDEADLINE: <2026-11-25 Wed>\ntexte\n* B");
        let page = apply(&page, &[plan(&page, 0, &[(DEADLINE, None)]).unwrap()]);
        assert_eq!(page, "* TODO A\ntexte\n* B");
        assert_eq!(plan(&page, 0, &[(DEADLINE, None)]), None);
        // Typed: kept repeater, the headline found from its text
        let page = "* TODO A\nSCHEDULED: <2026-11-20 Fri 09:00 +1w>\ntexte";
        let today = d("2026-11-24");
        let e = plan_typed(page, 2, Format::Org, SCHEDULED, "+2d", today).unwrap().unwrap();
        assert_eq!(apply(page, &[e]), "* TODO A\nSCHEDULED: <2026-11-26 Thu 09:00 +1w>\ntexte");
        assert!(plan_typed(page, 2, Format::Org, SCHEDULED, "n'importe", today).is_err());
        let e = plan_typed(page, 0, Format::Org, SCHEDULED, "-", today).unwrap().unwrap();
        assert_eq!(apply(page, &[e]), "* TODO A\ntexte");
    }

    #[test]
    fn reads_what_is_typed() {
        let today = d("2026-11-24"); // Tuesday
        let r = |s: &str| read_date(s, today).map(|(d, t)| (st("1970-01-01", "").on(d).date_field(), t));
        assert_eq!(r("2026-12-01"), Some(("2026-12-01".into(), None)));
        assert_eq!(r("+3d 14:30"), Some(("2026-11-27".into(), Some((14, 30)))));
        assert_eq!(r("+2").unwrap().0, "2026-11-26");
        assert_eq!(r("-1w").unwrap().0, "2026-11-17");
        assert_eq!(r("+1s").unwrap().0, "2026-12-01");
        assert_eq!(r("demain").unwrap().0, "2026-11-25");
        assert_eq!(r("ven").unwrap().0, "2026-11-27");
        assert_eq!(r("tue").unwrap().0, "2026-11-24");
        assert_eq!(r("lundi").unwrap().0, "2026-11-30");
        assert_eq!(r("10:00"), Some(("2026-11-24".into(), Some((10, 0)))));
        assert_eq!(r("m"), None);
        assert_eq!(r(""), None);
        assert_eq!(r("demain hier"), None);
    }

    #[test]
    fn done_closes_or_repeats() {
        let k = TodoKeywords::parse("TODO | DONE");
        let cycle = |page: &str, forward| apply(page, &cycle_todo(page, 0, &k, Format::Org, forward, &now()).unwrap());
        let page = cycle("* TODO A\ntexte", true);
        assert_eq!(page, "* DONE A\nCLOSED: [2026-11-24 Tue 10:05]\ntexte");
        assert_eq!(cycle(&page, false), "* TODO A\ntexte");
        assert_eq!(cycle("* TODO A\nDEADLINE: <2026-11-30 Mon>", true), "* DONE A\nCLOSED: [2026-11-24 Tue 10:05] DEADLINE: <2026-11-30 Mon>");
        // Repeated: its next dates, its state back
        assert_eq!(cycle("* TODO A\nSCHEDULED: <2026-11-20 Fri +1w> DEADLINE: <2026-11-22 Sun .+1d>", true),
                   "* TODO A\nSCHEDULED: <2026-11-27 Fri +1w> DEADLINE: <2026-11-25 Wed .+1d>");
        assert_eq!(cycle("* A\nSCHEDULED: <2026-11-20 Fri ++1w>", false), "* TODO A\nSCHEDULED: <2026-11-27 Fri ++1w>");
        // Not done to not done: nothing more
        let k2 = TodoKeywords::parse("TODO WAIT | DONE");
        assert_eq!(apply("* TODO A", &cycle_todo("* TODO A", 0, &k2, Format::Org, true, &now()).unwrap()), "* WAIT A");
        // A Markdown checkbox
        let md = "- [ ] pain";
        assert_eq!(apply(md, &cycle_todo(md, 0, &k, Format::Markdown, true, &now()).unwrap()), "- [x] pain");
    }

    #[test]
    fn positions_move_with_the_edits() {
        let k = TodoKeywords::parse("TODO DOING | DONE");
        let page = "* TODO A\ntexte";
        let edits = cycle_todo(page, 0, &k, Format::Org, true, &now()).unwrap();
        // The end of the headline (a fold's start) moves with the keyword
        assert_eq!(moved(8, &edits), 9);
        assert_eq!(moved(0, &edits), 0);
        let edits = cycle_todo("* DOING A\ntexte", 0, &k, Format::Org, true, &now()).unwrap();
        // CLOSED put under the headline: the fold starts before it, ends after it
        assert_eq!(moved(9, &edits), 8);
        assert_eq!(moved(15, &edits), 15 - 1 + "\nCLOSED: [2026-11-24 Tue 10:05]".len());
    }

    #[test]
    fn shifts_a_date() {
        let page = "* TODO A\nSCHEDULED: <2026-11-20 Fri>\nvoir <2026-11-21 Sat>";
        let e = &entries(page, Some(&kw()), Format::Org)[0];
        assert_eq!(apply(page, &[shift(page, e, When::Scheduled, 0, 1).unwrap()]), "* TODO A\nSCHEDULED: <2026-11-21 Sat>\nvoir <2026-11-21 Sat>");
        assert_eq!(apply(page, &[shift(page, e, When::Timestamp, 0, -1).unwrap()]), "* TODO A\nSCHEDULED: <2026-11-20 Fri>\nvoir <2026-11-20 Fri>");
        assert_eq!(typed_date(e, When::Scheduled), "2026-11-20");
    }
}
