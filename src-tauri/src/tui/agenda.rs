//! The agenda over the page (`agenda`, as org-mode's `C-c a a`): the calendar of
//! the month, the dated headlines of the week, and its keys (as org-mode's).

use orgamnesia_core::{agenda::{self, When}, edit::Stamp, AgendaHit, Format};

use super::{commands::Then, frames::{Popup, Target}, App, Mode};

/// The calendar of the month of the week from `from`, as lines: `[24]` today,
/// `24•` a day having a page in the journal, `▸` the week shown.
fn calendar_lines(from: i64, today: i64, journal: &std::collections::HashSet<i64>) -> Vec<String> {
    const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
    let (y, m, _) = agenda::date_of(from);
    let mut lines = vec![format!("{} {y}   (• a page of the journal)", MONTHS[m as usize - 1]), "   Mo  Tu  We  Th  Fr  Sa  Su".into()];
    for week in agenda::month_grid(from).chunks(7) {
        let mut line = String::from(if week[0] == from { "▸" } else { " " });
        for &day in week {
            let (_, dm, d) = agenda::date_of(day);
            line.push_str(&match (dm == m, day == today, journal.contains(&day)) {
                (false, _, _) => "    ".to_string(),
                (true, true, _) => format!("[{d:>2}]"),
                (true, false, true) => format!(" {d:>2}•"),
                (true, false, false) => format!(" {d:>2} "),
            });
        }
        lines.push(line.trim_end().to_string());
    }
    lines.push(String::new());
    lines
}

/// What puts a headline of the agenda on its day, as org-mode writes it.
fn agenda_label(h: &AgendaHit) -> String {
    match (h.when, h.late) {
        (When::Scheduled, 0) => "Scheduled:".into(),
        (When::Scheduled, n) => format!("Sched.{n}x:"),
        (When::Deadline, 0) => "Deadline:".into(),
        (When::Deadline, n) if n > 0 => format!("{n} d. ago:"),
        (When::Deadline, n) => format!("In {} d.:", -n),
        (When::Timestamp, _) => String::new(),
        (When::Page, _) => "Day page:".into(),
    }
}

impl App {
    /// The agenda of the week from the day `from`, over the page, its line `selected` chosen.
    pub(super) fn agenda(&mut self, from: i64, selected: usize) {
        // Read from the pages as written: those edited are saved first
        if !(self.save() && self.save_other()) { return; }
        self.status.clear();
        let today = super::editor::now().day_number();
        let hits = self.project.agenda(self.prefs.todo().as_ref(), from, 7, today);
        let journal: std::collections::HashSet<i64> = self.project.files().iter().filter_map(|f| agenda::day_of_name(&f.name)).collect();
        let calendar = calendar_lines(from, today, &journal);
        let (mut rows, mut items): (Vec<Option<AgendaHit>>, Vec<(String, Target)>) =
            (vec![None; calendar.len()], calendar.into_iter().map(|l| (l, Target::None)).collect());
        for day in from..from + 7 {
            let s = Stamp { year: 1970, month: 1, day: 1, time: None, active: true, rest: String::new() }.on(day);
            let name = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"][agenda::weekday(day) as usize];
            items.push((format!("{name} {}{}", s.date_field(), if day == today { "  (today)" } else { "" }), Target::None));
            rows.push(None);
            for h in hits.iter().filter(|h| h.day == day) {
                let time = h.time.as_deref().map(|t| format!("{t} ")).unwrap_or_default();
                let label = agenda_label(h);
                let kw = h.keyword.as_deref().map(|k| format!("{k} ")).unwrap_or_default();
                items.push((format!("  {:<14} {time}{label}{}{kw}{}", h.page, if label.is_empty() { "" } else { " " }, h.title), Target::Page(h.path.clone(), h.line)));
                rows.push(Some(h.clone()));
            }
        }
        let mut p = Popup {
            keys: Some("Enter open · t state · s/d plan · S-←/→ ±1 day · f/b week · . today · Esc"),
            ..Popup::pick(format!("Agenda, week of {}", Stamp { year: 1970, month: 1, day: 1, time: None, active: true, rest: String::new() }.on(from).date_field()), items)
        };
        if p.targets.get(selected).is_some_and(|t| *t != Target::None) { p.selected = selected; }
        p.agenda = Some((from, rows));
        self.mode = Mode::Popup(p);
    }

    /// A key on the agenda: `t` the next TODO state, `s`/`d` its dates, `f`/`b`
    /// the next/previous week, `.` this week, `+`/`-` (S-→/S-←) a day later/earlier.
    /// False when the key is none of those.
    pub(super) fn agenda_key(&mut self, key: char) -> bool {
        let Some(Popup { agenda: Some((from, rows)), selected, .. }) = self.popup() else { return false };
        let (from, selected, hit) = (*from, *selected, rows.get(*selected).cloned().flatten());
        let today = super::editor::now().day_number();
        match (key, hit) {
            ('f', _) => self.agenda(from + 7, 0),
            ('b', _) => self.agenda(from - 7, 0),
            ('.', _) => self.agenda(agenda::week_start(today), 0),
            // The page of the day: only opened
            ('t' | 's' | 'd' | '+' | '-', Some(h)) if h.when == When::Page => {}
            ('t', Some(h)) => {
                let Some(keywords) = self.prefs.todo() else { self.status = "TODO keywords are off (settings)".into(); return true };
                let format = Format::of_page(&h.path);
                self.change_page(&h.path, |text| agenda::cycle_todo(text, h.line, &keywords, format, true, &super::editor::now()));
                self.agenda(from, selected);
            }
            ('s' | 'd', Some(h)) => {
                let (key, when) = if key == 's' { (agenda::SCHEDULED, When::Scheduled) } else { (agenda::DEADLINE, When::Deadline) };
                let text = self.page_text(&h.path);
                let typed = agenda::entries(&text, self.prefs.todo().as_ref(), Format::of_page(&h.path)).iter()
                    .find(|e| e.line == h.line).map(|e| agenda::typed_date(e, when)).unwrap_or_default();
                self.ask(&format!("{key} (2026-12-01, +3d, fri, 14:30; - removes): "), &typed, Then::AgendaPlan { path: h.path, line: h.line, key, from, selected });
            }
            ('+' | '-', Some(h)) => {
                let days = if key == '+' { 1 } else { -1 };
                let todo = self.prefs.todo();
                let format = Format::of_page(&h.path);
                self.change_page(&h.path, |text| {
                    let e = agenda::entries(text, todo.as_ref(), format).into_iter().find(|e| e.line == h.line)?;
                    agenda::shift(text, &e, h.when, h.stamp, days).map(|e| vec![e])
                });
                self.agenda(from, selected);
            }
            ('t' | 's' | 'd' | '+' | '-', None) => {}
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_calendar_of_the_agenda() {
        let day = |d| agenda::day_of(2026, 11, d);
        let lines = calendar_lines(day(23), day(25), &[day(24), day(30)].into());
        assert_eq!(lines[..3], ["November 2026   (• a page of the journal)", "   Mo  Tu  We  Th  Fr  Sa  Su", "                           1"]);
        assert_eq!(lines[6], "▸ 23  24•[25] 26  27  28  29");
        assert_eq!(lines[7], "  30•");
    }
}
