//! The agenda's changes to a page: the TODO state of a headline cycled, its
//! dates planned, typed or shifted, as edits of the page text.

use super::*;

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
