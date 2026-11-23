//! Page templates, as Obsidian has them: the files of a folder of the project
//! (`templates/` by default, see `Prefs::templates_folder`), applied from the
//! quick open to a page of the same format. `{{title}}`, `{{date}}`, `{{time}}`
//! and `{{date:YYYY-MM-DD}}` in them are replaced when applied.

use crate::Format;

/// The templates offered to a project that has none, for pages of `format`,
/// in the language `lang` (`fr`, `en`): their name and text.
pub fn defaults(lang: &str, format: Format) -> Vec<(&'static str, &'static str)> {
    let list: &[(&str, Format, &str)] = if lang == "en" { EN } else { FR };
    list.iter().filter(|(_, f, _)| *f == format).map(|(name, _, text)| (*name, *text)).collect()
}

/// Whether a template takes the place of the page `text` (blank, or holding only
/// its title); else it goes in at the cursor, after the user said yes.
pub fn replaces(text: &str) -> bool {
    crate::pages::is_blank(text) || crate::pages::is_title_only(text)
}

/// The template among `templates` (name, text) the page `page` holds already:
/// every headline of its text (2 at least, those without a variable) is a line
/// of the page. Then the new one is offered to replace it.
pub fn applied<'a>(page: &str, templates: &'a [(String, String)]) -> Option<&'a str> {
    let lines: std::collections::HashSet<&str> = page.lines().map(str::trim_end).collect();
    templates.iter().find(|(_, text)| {
        let heads: Vec<&str> = text.lines().map(str::trim_end)
            .filter(|l| is_heading(l) && !l.contains("{{"))
            .collect();
        heads.len() >= 2 && heads.iter().all(|h| lines.contains(h))
    }).map(|(name, _)| name.as_str())
}

/// `* Titre` (org-mode) or `# Titre` (Markdown).
fn is_heading(line: &str) -> bool {
    let mark = if line.starts_with('#') { '#' } else { '*' };
    let rest = line.trim_start_matches(mark);
    rest.len() < line.len() && rest.starts_with(' ') && !rest.trim().is_empty()
}

/// The moment a template is applied.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Now { pub year: i32, pub month: u32, pub day: u32, pub hour: u32, pub minute: u32 }

/// `text` with Obsidian's variables replaced: `{{title}}`; `{{date}}` (`2026-11-23`)
/// and `{{time}}` (`14:05`), or written as `{{date:DD/MM/YYYY}}`, `{{time:HH[h]mm}}`
/// (see `format_now`). A variable it does not know is left as it is.
pub fn fill(text: &str, title: &str, now: Now) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else { out.push_str(&rest[start..]); return out };
        let inner = &after[..end];
        let (name, format) = inner.split_once(':').map_or((inner.trim(), None), |(n, f)| (n.trim(), Some(f)));
        match (name, format) {
            ("title", None) => out.push_str(title),
            ("date", f) => out.push_str(&format_now(f.unwrap_or("YYYY-MM-DD"), now)),
            ("time", f) => out.push_str(&format_now(f.unwrap_or("HH:mm"), now)),
            _ => out.push_str(&rest[start..start + 2 + end + 2]),
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

/// `now` written as `format` says, with the tokens of Obsidian (moment.js) most
/// used: `YYYY`, `YY`, `MM`, `M`, `DD`, `D`, `HH`, `H`, `mm`; `[text]` as it is.
// shortcut: no week days nor month names (`ddd`, `MMMM`), add them when asked
pub fn format_now(format: &str, now: Now) -> String {
    let tokens: [(&str, String); 9] = [
        ("YYYY", format!("{:04}", now.year)), ("YY", format!("{:02}", now.year.rem_euclid(100))),
        ("MM", format!("{:02}", now.month)), ("M", now.month.to_string()),
        ("DD", format!("{:02}", now.day)), ("D", now.day.to_string()),
        ("HH", format!("{:02}", now.hour)), ("H", now.hour.to_string()),
        ("mm", format!("{:02}", now.minute)),
    ];
    let mut out = String::new();
    let mut rest = format;
    'next: while let Some(c) = rest.chars().next() {
        if c == '[' && let Some(end) = rest.find(']') {
            out.push_str(&rest[1..end]);
            rest = &rest[end + 1..];
            continue;
        }
        for (token, value) in &tokens {
            if let Some(r) = rest.strip_prefix(token) { out.push_str(value); rest = r; continue 'next; }
        }
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

const FR: &[(&str, Format, &str)] = &[
    ("Réunion", Format::Org, "#+TITLE: {{title}}\n#+DATE: [{{date}}]\n#+FILETAGS: :reunion:\n\n* Participants\n- \n* Ordre du jour\n- \n* Notes\n\n* Décisions\n- \n* Actions\n** TODO \n"),
    ("Personne", Format::Org, "#+TITLE: {{title}}\n#+FILETAGS: :personne:\n\n* Contact\n- Courriel : \n- Téléphone : \n* Contexte\nRencontré le [{{date}}], \n* Notes\n\n* À suivre\n** TODO \n"),
    ("Lecture", Format::Org, "#+TITLE: {{title}}\n#+FILETAGS: :lecture:\n\n* Fiche\n- Auteur : \n- Source : \n- Commencé le : [{{date}}]\n* Idées clés\n- \n* Citations\n#+BEGIN_QUOTE\n\n#+END_QUOTE\n* Ce que j'en retiens\n\n* Liens\n- \n"),
    ("Projet", Format::Org, "#+TITLE: {{title}}\n#+FILETAGS: :projet:\n\n* Objectif\n\n* Contexte\n\n* Tâches\n** TODO \n* Ressources\n- \n* Journal\n** [{{date}}]\n"),
    ("Réunion", Format::Markdown, "---\ntitle: {{title}}\ndate: {{date}}\ntags: [reunion]\n---\n\n# Participants\n- \n\n# Ordre du jour\n- \n\n# Notes\n\n# Décisions\n- \n\n# Actions\n- [ ] \n"),
    ("Personne", Format::Markdown, "---\ntitle: {{title}}\ntags: [personne]\n---\n\n# Contact\n- Courriel : \n- Téléphone : \n\n# Contexte\nRencontré le {{date}}, \n\n# Notes\n\n# À suivre\n- [ ] \n"),
    ("Lecture", Format::Markdown, "---\ntitle: {{title}}\ntags: [lecture]\n---\n\n# Fiche\n- Auteur : \n- Source : \n- Commencé le : {{date}}\n\n# Idées clés\n- \n\n# Citations\n> \n\n# Ce que j'en retiens\n\n# Liens\n- \n"),
    ("Projet", Format::Markdown, "---\ntitle: {{title}}\ntags: [projet]\n---\n\n# Objectif\n\n# Contexte\n\n# Tâches\n- [ ] \n\n# Ressources\n- \n\n# Journal\n## {{date}}\n"),
];

const EN: &[(&str, Format, &str)] = &[
    ("Meeting", Format::Org, "#+TITLE: {{title}}\n#+DATE: [{{date}}]\n#+FILETAGS: :meeting:\n\n* Attendees\n- \n* Agenda\n- \n* Notes\n\n* Decisions\n- \n* Actions\n** TODO \n"),
    ("Person", Format::Org, "#+TITLE: {{title}}\n#+FILETAGS: :person:\n\n* Contact\n- Email: \n- Phone: \n* Context\nMet on [{{date}}], \n* Notes\n\n* Follow-up\n** TODO \n"),
    ("Reading", Format::Org, "#+TITLE: {{title}}\n#+FILETAGS: :reading:\n\n* Details\n- Author: \n- Source: \n- Started: [{{date}}]\n* Key ideas\n- \n* Quotes\n#+BEGIN_QUOTE\n\n#+END_QUOTE\n* Takeaways\n\n* Links\n- \n"),
    ("Project", Format::Org, "#+TITLE: {{title}}\n#+FILETAGS: :project:\n\n* Goal\n\n* Context\n\n* Tasks\n** TODO \n* Resources\n- \n* Log\n** [{{date}}]\n"),
    ("Meeting", Format::Markdown, "---\ntitle: {{title}}\ndate: {{date}}\ntags: [meeting]\n---\n\n# Attendees\n- \n\n# Agenda\n- \n\n# Notes\n\n# Decisions\n- \n\n# Actions\n- [ ] \n"),
    ("Person", Format::Markdown, "---\ntitle: {{title}}\ntags: [person]\n---\n\n# Contact\n- Email: \n- Phone: \n\n# Context\nMet on {{date}}, \n\n# Notes\n\n# Follow-up\n- [ ] \n"),
    ("Reading", Format::Markdown, "---\ntitle: {{title}}\ntags: [reading]\n---\n\n# Details\n- Author: \n- Source: \n- Started: {{date}}\n\n# Key ideas\n- \n\n# Quotes\n> \n\n# Takeaways\n\n# Links\n- \n"),
    ("Project", Format::Markdown, "---\ntitle: {{title}}\ntags: [project]\n---\n\n# Goal\n\n# Context\n\n# Tasks\n- [ ] \n\n# Resources\n- \n\n# Log\n## {{date}}\n"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_replaces_obsidian_variables() {
        let now = Now { year: 2026, month: 3, day: 7, hour: 9, minute: 5 };
        assert_eq!(fill("{{title}} {{date}} {{time}} {{ title }}", "Note", now), "Note 2026-03-07 09:05 Note");
        assert_eq!(fill("{{date:DD/MM/YY}} {{time:H[h]mm}} {{date:D M YYYY}}", "", now), "07/03/26 9h05 7 3 2026");
        // Unknown or unclosed: left as written
        assert_eq!(fill("{{autre}} {x} {{title", "Note", now), "{{autre}} {x} {{title");
    }

    #[test]
    fn a_template_already_applied_is_found() {
        let now = Now { year: 2026, month: 3, day: 7, hour: 9, minute: 5 };
        let named = |lang, format| defaults(lang, format).into_iter().map(|(n, t)| (n.to_string(), t.to_string())).collect::<Vec<_>>();
        let org = named("fr", Format::Org);
        let page = fill(&org.iter().find(|(n, _)| n == "Projet").unwrap().1, "X", now) + "du texte écrit\n";
        assert_eq!(applied(&page, &org), Some("Projet"));
        assert_eq!(applied("* X\nrien\n", &org), None);
        // One headline in common is no template applied
        assert_eq!(applied("* Notes\n", &org), None);
        let md = named("en", Format::Markdown);
        assert_eq!(applied(&fill(&md[0].1, "X", now), &md), Some(md[0].0.as_str()));
    }

    #[test]
    fn defaults_for_each_format_and_language() {
        for lang in ["fr", "en"] {
            for format in Format::ALL {
                assert_eq!(defaults(lang, format).len(), 4);
            }
        }
        assert!(defaults("fr", Format::Org)[0].1.starts_with("#+TITLE: {{title}}"));
        assert!(defaults("en", Format::Markdown)[0].1.starts_with("---\ntitle: {{title}}"));
        assert_ne!(defaults("fr", Format::Org)[0].0, defaults("en", Format::Org)[0].0);
    }
}
