#[test]
fn the_calendar_of_a_month_and_the_journal_days() {
    // November 2026 starts on a Sunday, ends on a Monday: 6 weeks
    let grid = super::month_grid(super::day_of(2026, 11, 18));
    assert_eq!(grid.len(), 42);
    assert_eq!(super::date_of(grid[0]), (2026, 10, 26));
    assert_eq!(super::date_of(*grid.last().unwrap()), (2026, 12, 6));
    assert_eq!(super::month_grid(super::day_of(2027, 2, 1)).len(), 28);
    let days = [10, 3, 20];
    assert_eq!(super::journal_step(days, 10, false), Some(3));
    assert_eq!(super::journal_step(days, 11, true), Some(20));
    assert_eq!(super::journal_step(days, 20, true), None);
}

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
