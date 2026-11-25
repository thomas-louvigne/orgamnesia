//! The quick open (the window's `quick_open`, `Ctrl+P`): the pages found by
//! typing part of their name, the shortcuts after `>`, the page templates.

use orgamnesia_core::keybindings::{APP_ACTIONS, EDITOR_ACTIONS, PALETTE_EDITOR_ACTIONS};
use orgamnesia_core::Format;

use super::{commands::{Then, NOT_HERE}, keys, App};

/// Commands of the terminal's command list (`>`) with no shortcut of the window.
const TERMINAL_COMMANDS: &[&str] = &["delete_page", "trash", "empty_trash", "undo_replace", "todo_filter"];

/// The quick open (the window's `quick_open`): a page found by typing part of its name.
#[derive(Default)]
pub struct Quick {
    pub query: String,
    pub selected: usize,
}

/// A line of the quick open: a page (and whether it is open), or the page to create.
#[derive(Debug, PartialEq)]
pub enum QuickItem {
    Page { name: String, path: String, open: bool },
    Create(String),
    /// A shortcut of the window (`>` typed first), with its keys.
    Command { id: String, keys: String },
    /// A template of the project (`templates/`): applied to the page edited, or edited itself.
    Template { name: String, path: String, edit: bool },
    /// A new template, its name asked.
    NewTemplate,
    /// The suggested templates added to the project (see `templates::defaults`).
    SuggestedTemplates,
}

/// What the quick open lists for `query`: the pages open first, then the
/// others, as the window lists them (case and accents ignored, those starting
/// with it first, then the shortest), and the page to create.
pub fn quick_items(query: &str, tabs: &[String], files: &[orgamnesia_core::FileEntry]) -> Vec<QuickItem> {
    use orgamnesia_core::names::{fold, search_order};
    let q = fold(query.trim());
    let matches = |name: &str| q.is_empty() || fold(name).contains(&q);
    let name_of = |path: &str| files.iter().find(|f| f.path == path).map(|f| f.name.clone());
    let mut list: Vec<QuickItem> = tabs.iter().rev()
        .filter_map(|path| name_of(path).map(|name| (name, path)))
        .filter(|(name, _)| matches(name))
        .map(|(name, path)| QuickItem::Page { name, path: path.clone(), open: true })
        .collect();
    let mut others: Vec<&orgamnesia_core::FileEntry> = files.iter().filter(|f| matches(&f.name) && !tabs.contains(&f.path)).collect();
    others.sort_by_cached_key(|f| if q.is_empty() { (false, 0, fold(&f.name)) } else { search_order(&f.name, &q) });
    if !q.is_empty() { list.sort_by_cached_key(|i| match i { QuickItem::Page { name, .. } => search_order(name, &q), _ => Default::default() }); }
    list.extend(others.into_iter().map(|f| QuickItem::Page { name: f.name.clone(), path: f.path.clone(), open: false }));
    if !q.is_empty() && !files.iter().any(|f| fold(&f.name) == q) { list.push(QuickItem::Create(query.trim().to_string())); }
    list
}

impl App {
    /// What the quick open lists for `query`: after `>`, the shortcuts of the
    /// window the terminal does (as the window lists them); else the pages, then
    /// the shortcuts whose name holds it, the page to create last.
    pub(super) fn quick_list(&self, query: &str) -> Vec<QuickItem> {
        if let Some(rest) = query.strip_prefix('>') { return self.quick_commands(rest); }
        let mut list = quick_items(query, &self.tabs, &self.project.files());
        if query.trim().is_empty() { return list; }
        let create = matches!(list.last(), Some(QuickItem::Create(_))).then(|| list.pop()).flatten();
        list.extend(self.quick_commands(query));
        list.extend(create);
        list
    }

    pub(super) fn quick_commands(&self, typed: &str) -> Vec<QuickItem> {
        use orgamnesia_core::names::{fold, search_order};
        let q = fold(typed.trim());
        // The editing ones only on a page edited
        let editing = EDITOR_ACTIONS.iter().filter(|a| self.editor.is_some() && PALETTE_EDITOR_ACTIONS.contains(&a.action)).map(|a| a.id);
        let mut list: Vec<(String, QuickItem)> = APP_ACTIONS.iter()
            // Opening this very list is no command
            .filter(|a| a.id != "quick_open" && !NOT_HERE.contains(&a.id))
            .map(|a| a.id)
            .chain(TERMINAL_COMMANDS.iter().copied())
            .chain(editing)
            .map(|id| (id.replace('_', " "), id))
            .filter(|(name, _)| q.is_empty() || fold(name).contains(&q))
            .map(|(name, id)| {
                let bound = self.kb.app.get(id).or_else(|| self.kb.editor.get(id));
                let keys = bound.map(|b| b.iter().filter(|b| !b.is_empty()).map(|b| keys::display(b)).collect::<Vec<_>>().join(", ")).unwrap_or_default();
                (name, QuickItem::Command { id: id.to_string(), keys })
            })
            .collect();
        list.extend(self.template_items().into_iter().filter(|(name, _)| q.is_empty() || fold(name).contains(&q)));
        if !q.is_empty() { list.sort_by_cached_key(|(name, _)| search_order(name, &q)); }
        list.into_iter().map(|(_, i)| i).collect()
    }

    /// The templates of the project, by the name they are found by: those of the
    /// page edited's format to apply, all to edit, and the two to make some.
    pub(super) fn template_items(&self) -> Vec<(String, QuickItem)> {
        let format = self.editor.as_ref().filter(|e| e.page).map(|e| e.format());
        let templates = self.project.templates();
        let apply = templates.iter().filter(|t| Some(Format::of_page(&t.path)) == format)
            .map(|t| (format!("template {}", t.name), QuickItem::Template { name: t.name.clone(), path: t.path.clone(), edit: false }));
        let edit = templates.iter()
            .map(|t| (format!("edit template {}", t.name), QuickItem::Template { name: t.name.clone(), path: t.path.clone(), edit: true }));
        apply.chain(edit).chain([
            ("new template".to_string(), QuickItem::NewTemplate),
            ("add suggested templates".to_string(), QuickItem::SuggestedTemplates),
        ]).collect()
    }

    /// The quick open's template line chosen.
    pub(super) fn template_chosen(&mut self, item: QuickItem) {
        match item {
            QuickItem::Template { path, edit: true, .. } => self.edit(&path),
            QuickItem::Template { path, name, .. } => match self.project.read(&path) {
                Ok(text) => self.apply_template(&text, &name),
                Err(e) => self.status = e.to_string(),
            },
            QuickItem::NewTemplate => self.ask("New template name: ", "", Then::NewTemplate),
            QuickItem::SuggestedTemplates => {
                let format = self.prefs.default_format;
                let made = orgamnesia_core::templates::defaults(&self.prefs.lang, format).into_iter()
                    .filter(|(name, text)| self.project.create_template(name, format, text).is_ok())
                    .count();
                self.status = format!("{made} template(s) added to {}/", self.prefs.templates_folder);
            }
            _ => {}
        }
    }

    /// The template `text` put in the page edited, as the window does: over the
    /// whole page when it is blank or holds only its title, else at the cursor
    /// (over the region).
    pub(super) fn apply_template(&mut self, text: &str, name: &str) {
        let Some(e) = self.editor.as_ref() else { return };
        use chrono::{Datelike, Timelike};
        let t = chrono::Local::now();
        let now = orgamnesia_core::templates::Now { year: t.year(), month: t.month(), day: t.day(), hour: t.hour(), minute: t.minute() };
        let text = orgamnesia_core::templates::fill(text, &crate::project::files::page_name(&e.path), now);
        let page = e.text();
        if orgamnesia_core::templates::replaces(&page) { return self.put_template(&text, true); }
        // On a page already written: a template applied there is offered to be
        // replaced; else the new one adds to the page, asked first
        let format = e.format();
        let templates: Vec<(String, String)> = self.project.templates().into_iter()
            .filter(|t| Format::of_page(&t.path) == format)
            .filter_map(|t| Some((t.name, self.project.read(&t.path).ok()?)))
            .collect();
        match orgamnesia_core::templates::applied(&page, &templates) {
            Some(old) => {
                let label = format!("The page holds the template “{old}”: (r)eplace the page with “{name}”, or (a)dd it at the cursor? ");
                self.ask(&label, "", Then::ApplyTemplate { text, replace: true });
            }
            None => self.ask(&format!("The page holds text: add the template “{name}” at the cursor? (y/n) "), "", Then::ApplyTemplate { text, replace: false }),
        }
    }

    /// The filled template `text` put in the page edited: over the whole page
    /// (`whole`, or when it is blank or holds only its title), else at the cursor
    /// (over the region).
    pub(super) fn put_template(&mut self, text: &str, whole: bool) {
        let Some(e) = self.editor.as_mut() else { return };
        let page = e.text();
        let (a, b) = if whole || orgamnesia_core::templates::replaces(&page) {
            (0, page.chars().count())
        } else {
            e.region().unwrap_or((e.cursor, e.cursor))
        };
        e.put(a, b, text);
        e.mark = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quick_open_lists_as_the_window() {
        let file = |n: &str| orgamnesia_core::FileEntry { name: n.into(), path: format!("/p/{n}.org"), ..Default::default() };
        let files = [file("Ma quête"), file("Quête"), file("Règle"), file("Autre")];
        let names = |items: Vec<QuickItem>| items.into_iter().map(|i| match i { QuickItem::Page { name, open, .. } => format!("{name}{}", if open { "*" } else { "" }), QuickItem::Create(n) => format!("+{n}"), QuickItem::Command { id, .. } => format!(">{id}"), _ => String::new() }).collect::<Vec<_>>();
        assert_eq!(names(quick_items("quete", &[], &files)), ["Quête", "Ma quête"]);
        assert_eq!(names(quick_items("regle", &[], &files)), ["Règle"]);
        // A name no page has: the page to create
        assert_eq!(names(quick_items("quet", &[], &files)), ["Quête", "Ma quête", "+quet"]);
        assert_eq!(names(quick_items("", &["/p/Règle.org".into()], &files)), ["Règle*", "Autre", "Ma quête", "Quête"]);
    }
}
