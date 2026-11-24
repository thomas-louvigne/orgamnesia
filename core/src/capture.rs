//! Quick capture: a shortcut opens, beside the page in view (the screen split in
//! two), the page notes are captured to, on a new line at its end; the same
//! shortcut closes it. The window and the terminal do it alike.

use crate::templates::Now;

/// The page captured to when the journal is off.
pub const INBOX: &str = "Inbox";

/// The name of the page captured to: the day's (`2026-11-23`) with the journal on, else `Inbox`.
pub fn page(journal: bool, now: Now) -> String {
    if journal { format!("{:04}-{:02}-{:02}", now.year, now.month, now.day) } else { INBOX.to_string() }
}

/// What goes at the end of the page `text` for a new note: a list item with
/// its day and time as an active timestamp (`- <2026-11-24 Tue 14:05> `), so the
/// agenda shows it that day; on a line of its own.
pub fn entry(text: &str, now: Now) -> String {
    let sep = if text.is_empty() || text.ends_with('\n') { "" } else { "\n" };
    let stamp = crate::edit::Stamp { year: now.year, month: now.month, day: now.day, time: Some((now.hour, now.minute)), active: true, rest: String::new() };
    format!("{sep}- {} ", stamp.text())
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: Now = Now { year: 2026, month: 3, day: 7, hour: 9, minute: 5 };

    #[test]
    fn the_day_or_the_inbox() {
        assert_eq!(page(true, NOW), "2026-03-07");
        assert_eq!(page(false, NOW), "Inbox");
    }

    #[test]
    fn a_line_of_its_own() {
        assert_eq!(entry("* Inbox\n", NOW), "- <2026-03-07 Sat 09:05> ");
        assert_eq!(entry("* Inbox", NOW), "\n- <2026-03-07 Sat 09:05> ");
        assert_eq!(entry("", NOW), "- <2026-03-07 Sat 09:05> ");
        // The agenda finds it on its day, as a line of its own
        let page = format!("* Inbox\n{}idée", entry("* Inbox\n", NOW));
        let e = crate::agenda::entries(&page, None, crate::Format::Org);
        assert_eq!((e.len(), e[0].title.as_str(), e[0].stamps[0].3.day), (1, "idée", 7));
    }
}
