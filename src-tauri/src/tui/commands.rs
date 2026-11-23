//! What the terminal does beyond the editor: the line at the bottom where a
//! value is typed (Emacs' minibuffer: a link, a date, a tag, a search…), the
//! window over the page (shortcuts, history, links), and the actions of the
//! window's shortcuts done with the project (search, replace, git, projects).

use orgamnesia_core::keybindings::{APP_ACTIONS, EDITOR_ACTIONS, PALETTE_EDITOR_ACTIONS};
use orgamnesia_core::{edit::Stamp, Format, Frame, ReplaceSkip, SearchOptions};

use super::{keys, App, Editor, Mode};
use crate::project::Project;

/// A value asked at the bottom of the screen, and what it is for.
pub struct Prompt {
    pub label: String,
    pub input: String,
    then: Then,
    /// The names Tab went through (completing what was typed), the one shown.
    pub choices: Vec<String>,
    pub choice: usize,
}

impl Prompt {
    /// What Tab completes the value with: the tags, the pages, the folders.
    pub(super) fn completes(&self) -> Option<fn(&App) -> Vec<String>> {
        match self.then {
            Then::AddTag => Some(App::tag_names),
            Then::LinkTarget { .. } => Some(App::link_names),
            Then::NewFolder | Then::MovePage(_) | Then::RenameFolder | Then::RenameFolderTo(_) | Then::DeleteFolder => Some(App::folder_names),
            _ => None,
        }
    }
}

enum Then {
    /// A link written over `a..b` of the page edited: its target, then its text.
    LinkTarget { a: usize, b: usize, text: String },
    LinkText { a: usize, b: usize, target: String },
    /// A date written over `a..b`, as `2026-11-20` or `2026-11-20 14:30`.
    Date { a: usize, b: usize, active: bool, rest: String },
    /// The page's `query-replace`: what to replace, then with what.
    ReplaceFrom,
    ReplaceTo(String),
    AddTag,
    ProjectSearch,
    ProjectReplaceFrom,
    ProjectReplaceTo(String),
    ProjectReplaceSure(String, String),
    Commit,
    /// The format of the new page named so (`ask_format`).
    NewPage(String),
    /// The page at this path renamed.
    Rename(String),
    /// A template of this name made (in the default format), then edited.
    NewTemplate,
    /// A folder of this name made (`Jeux/Quêtes`).
    NewFolder,
    /// The page at this path moved to the folder typed (empty: at the top).
    MovePage(String),
    /// The folder to rename, then its new name.
    RenameFolder,
    RenameFolderTo(String),
    /// The folder to delete, then `y` to be sure.
    DeleteFolder,
    DeleteFolderSure(String),
    /// This template (filled) on a page already written: added at the cursor (`y`,
    /// `a`), or put in the page's place (`r`, when it holds a template already).
    ApplyTemplate { text: String, replace: bool },
    /// The JSON settings files of older versions deleted (`y`), or kept for good.
    RemoveJson(Vec<crate::config_files::File>, Box<crate::settings::Settings>),
    /// One of the projects, by its number.
    Project(Vec<String>),
}

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

    fn quick_commands(&self, typed: &str) -> Vec<QuickItem> {
        use orgamnesia_core::names::{fold, search_order};
        let q = fold(typed.trim());
        // The editing ones only on a page edited
        let editing = EDITOR_ACTIONS.iter().filter(|a| self.editor.is_some() && PALETTE_EDITOR_ACTIONS.contains(&a.action)).map(|a| a.id);
        let mut list: Vec<(String, QuickItem)> = APP_ACTIONS.iter()
            // Opening this very list is no command
            .filter(|a| a.id != "quick_open" && !NOT_HERE.contains(&a.id))
            .map(|a| a.id)
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
    fn template_items(&self) -> Vec<(String, QuickItem)> {
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
    fn apply_template(&mut self, text: &str, name: &str) {
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
    fn put_template(&mut self, text: &str, whole: bool) {
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

/// Text shown over the page (the shortcuts, the history of the page…); with
/// `targets`, a list to pick from (TODO, tags, links), the line `selected`.
#[derive(Default)]
pub struct Popup {
    pub title: String,
    pub lines: Vec<String>,
    pub scroll: u16,
    pub targets: Vec<Target>,
    pub selected: usize,
}

/// What a line of a popup leads to.
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    /// A line (from 0) of a page.
    Page(String, usize),
    /// The line of a page naming this (a link to a page): found when opened.
    Mention(String, String),
    /// The headlines and pages of a tag.
    Tag(String),
    /// An editing action on the page (the right-click menu).
    Action(&'static str),
    /// Nothing (a title, a blank line).
    None,
}

impl Popup {
    /// Text to read, scrolled.
    pub fn text(title: String, lines: Vec<String>) -> Self {
        Self { title, lines, ..Self::default() }
    }

    /// A list to pick from: each line and what it leads to.
    pub fn pick(title: String, items: Vec<(String, Target)>) -> Self {
        let (lines, targets): (Vec<String>, Vec<Target>) = items.into_iter().unzip();
        let selected = targets.iter().position(|t| *t != Target::None).unwrap_or(0);
        Self { title, lines, targets, selected, scroll: 0 }
    }

    /// The next (previous) line leading somewhere.
    pub fn step(&mut self, forward: bool) {
        let n = self.targets.len();
        let mut i = self.selected;
        for _ in 0..n {
            i = if forward { (i + 1).min(n.saturating_sub(1)) } else { i.saturating_sub(1) };
            if self.targets[i] != Target::None { self.selected = i; return; }
        }
    }
}

/// The actions of the shortcuts the terminal does not do.
const NOT_HERE: &[&str] = &["export_pdf", "export_html", "spell_next"];

impl App {
    fn ask(&mut self, label: &str, input: &str, then: Then) {
        self.mode = Mode::Prompt(Prompt { label: label.to_string(), input: input.to_string(), then, choices: vec![], choice: 0 });
    }

    /// The value typed in the prompt given (Enter).
    pub(super) fn submit(&mut self) {
        if !matches!(self.mode, Mode::Prompt(_)) { return; }
        let Mode::Prompt(Prompt { input, then, .. }) = self.normal() else { return };
        let input = input.trim().to_string();
        match then {
            Then::LinkTarget { a, b, text } if !input.is_empty() => self.ask("Description: ", &text, Then::LinkText { a, b, target: input }),
            Then::LinkText { a, b, target } => if let Some(e) = &mut self.editor {
                e.put(a, b, &orgamnesia_core::edit::format_link(&target, &input));
            },
            Then::Date { a, b, active, rest } => {
                let (date, time) = input.split_once(' ').unwrap_or((&input, ""));
                match (Stamp::from_fields(date, time, active, &rest), &mut self.editor) {
                    (Some(stamp), Some(e)) => e.put(a, b, &stamp.text()),
                    _ => self.status = format!("not a date: {input}"),
                }
            }
            Then::ReplaceFrom if !input.is_empty() => self.ask(&format!("Replace “{input}” with: "), "", Then::ReplaceTo(input)),
            Then::ReplaceTo(from) => {
                let found = self.editor.as_mut().is_some_and(|e| e.start_replace(&from));
                if found { self.mode = Mode::Replace(from, input); } else { self.status = format!("“{from}” not found"); }
            }
            Then::AddTag if !input.is_empty() => self.add_tag(&input),
            Then::ProjectSearch if !input.is_empty() => self.project_search(&input),
            Then::ProjectReplaceFrom if !input.is_empty() => self.ask(&format!("Replace “{input}” in the project with: "), "", Then::ProjectReplaceTo(input)),
            Then::ProjectReplaceTo(from) => {
                let n = self.project.search(&from, SearchOptions::default(), &Default::default()).total;
                if n == 0 { self.status = format!("“{from}” not found"); return; }
                self.ask(&format!("Replace {n} × “{from}” with “{input}”? (y/n) "), "", Then::ProjectReplaceSure(from, input));
            }
            Then::ProjectReplaceSure(from, to) if matches!(input.as_str(), "y" | "o" | "yes" | "oui") => self.project_replace(&from, &to),
            Then::RemoveJson(files, mut settings) => {
                let Some(dir) = self.config_dir.clone() else { return };
                let done = if matches!(input.as_str(), "y" | "o" | "yes" | "oui") {
                    crate::config_files::remove_json(&dir, &files)
                } else {
                    settings.keep_json = true;
                    crate::settings::save_state(&dir, &settings)
                };
                self.status = match done { Ok(()) => String::new(), Err(e) => e.to_string() };
            }
            Then::Commit => self.git_commit(if input.is_empty() { "Orgamnesia" } else { &input }),
            Then::Rename(old) if !input.is_empty() => self.rename(&old, &input),
            Then::NewPage(name) => {
                let format = match input.to_lowercase().as_str() { "md" | "markdown" => Format::Markdown, _ => Format::Org };
                match self.project.create(&name, Some(format), self.prefs.tags()) {
                    Ok(page) => self.edit(&page.path),
                    Err(e) => self.status = e.to_string(),
                }
            }
            Then::ApplyTemplate { text, replace } => match input.to_lowercase().as_str() {
                "r" if replace => self.put_template(&text, true),
                "y" | "o" | "yes" | "oui" | "a" => self.put_template(&text, false),
                _ => {}
            },
            Then::NewFolder if !input.is_empty() => match self.project.create_folder(&input) {
                Ok(name) => self.status = format!("folder {name} made"),
                Err(e) => self.status = e.to_string(),
            },
            Then::MovePage(path) => {
                let name = self.project.name_of(&path);
                let leaf = name.rsplit('/').next().unwrap_or(&name).to_string();
                let folder = input.trim_matches('/');
                self.rename(&path, &if folder.is_empty() { leaf } else { format!("{folder}/{leaf}") });
            }
            Then::RenameFolder if !input.is_empty() => {
                let label = format!("Rename the folder “{input}” to: ");
                self.ask(&label, &input.clone(), Then::RenameFolderTo(input));
            }
            Then::RenameFolderTo(old) if !input.is_empty() => self.rename_folder(&old, &input),
            Then::DeleteFolder if !input.is_empty() => {
                let count = self.project.files().iter().filter(|f| f.name.starts_with(&format!("{input}/"))).count();
                self.ask(&format!("Delete the folder “{input}”, its {count} pages to the trash? (y/n) "), "", Then::DeleteFolderSure(input));
            }
            Then::DeleteFolderSure(name) => if matches!(input.to_lowercase().as_str(), "y" | "o" | "yes" | "oui") {
                if !self.save() || !self.save_other() { return; }
                match self.project.delete_folder(&name) {
                    Ok(trashed) => {
                        let prefix = format!("{name}/");
                        let gone = |p: &String| self.project.name_of(p).starts_with(&prefix);
                        let gone: Vec<String> = self.tabs.iter().filter(|p| gone(p)).cloned().collect();
                        self.tabs.retain(|p| !gone.contains(p));
                        if self.other.as_ref().is_some_and(|o| gone.contains(&o.path)) { self.single(); }
                        if self.editor.as_ref().is_some_and(|e| gone.contains(&e.path)) { self.editor = None; }
                        self.status = format!("folder {name} deleted, {} pages in the trash", trashed.len());
                    }
                    Err(e) => self.status = e.to_string(),
                }
            },
            Then::NewTemplate if !input.is_empty() => match self.project.create_template(&input, self.prefs.default_format, "") {
                Ok(t) => self.edit(&t.path),
                Err(e) => self.status = e.to_string(),
            },
            Then::Project(paths) => match input.parse::<usize>().ok().and_then(|n| paths.get(n.wrapping_sub(1))) {
                Some(path) => self.switch_project(&path.clone()),
                None => self.status = "no such project".into(),
            },
            _ => {}
        }
    }

    /// Do an action of the window's shortcuts that is not the editor's nor
    /// moving between pages: `None` when the terminal does not know it.
    pub(super) fn command(&mut self, id: &str) -> Option<()> {
        let editing = self.editor.is_some();
        match id {
            "show_shortcuts" => self.show_shortcuts(),
            "insert_link" if editing => {
                let Some(e) = &self.editor else { return Some(()) };
                let (a, b, target, text) = e.link_here();
                self.ask("Link to: ", &target, Then::LinkTarget { a, b, text });
            }
            "insert_date" | "insert_inactive_date" if editing => {
                let Some(e) = &self.editor else { return Some(()) };
                let (a, b, s) = e.date_here(id == "insert_date");
                let time = s.time_field();
                let value = if time.is_empty() { s.date_field() } else { format!("{} {time}", s.date_field()) };
                self.ask("Date: ", &value, Then::Date { a, b, active: s.active, rest: s.rest });
            }
            "replace" if editing => self.ask("Replace: ", "", Then::ReplaceFrom),
            "add_tag" => self.ask("Tag: ", "", Then::AddTag),
            "project_search" => self.ask("Search the project: ", "", Then::ProjectSearch),
            "project_replace" => self.ask("Replace in the project: ", "", Then::ProjectReplaceFrom),
            "git_commit" => if self.save() { let message = self.commit_message(); self.ask("Commit message: ", &message, Then::Commit) },
            "git_pull" => self.git(|p, prefs| crate::git::pull(p, &prefs.git_ssh_key, prefs.git_pull_rebase)),
            "git_push" => self.git(|p, prefs| crate::git::push(p, &prefs.git_ssh_key)),
            "git_history" => self.history(),
            "graph" => self.links_popup(),
            "random_page" => self.random_page(),
            "zen_mode" => self.zen = !self.zen,
            "panel_todos" => self.panel(Frame::Todos),
            "panel_tags" => self.panel(Frame::Tags),
            "panel_backlinks" => self.panel(Frame::Backlinks),
            "panel_broken" => self.panel(Frame::Broken),
            // The list of the pages: the editor left for it (saved)
            "panel_pages" => if self.back_to_list() { self.mode = Mode::Normal; },
            "rename_page" => self.ask_rename(),
            "switch_project" => {
                let paths = self.projects.clone();
                let list: Vec<String> = paths.iter().enumerate().map(|(i, p)| format!("{} {}", i + 1, crate::project::files::page_name(p))).collect();
                self.ask(&format!("Project ({}): ", list.join(", ")), "", Then::Project(paths));
            }
            "open_settings" => self.open_settings(),
            "next_tab" | "prev_tab" => self.next_tab(id == "next_tab"),
            "close_tab" => self.close_tab(),
            "split_vertical" => self.split(ratatui::layout::Direction::Horizontal),
            "split_horizontal" => self.split(ratatui::layout::Direction::Vertical),
            "close_split" => if self.other.is_some() && self.save() {
                self.swap();
                self.single();
            },
            "single_window" => if self.save_other() { self.single(); },
            "other_window" | "next_region" | "prev_region" => if self.other.is_some() { self.edit_previewed(); self.swap() } else { self.other_window() },
            _ if NOT_HERE.contains(&id) => self.status = "only in the window, not in the terminal yet".into(),
            _ => return None,
        }
        Some(())
    }

    /// The window of the shortcuts (F1): those of the profile in effect, the
    /// ones the terminal does not do marked.
    pub(super) fn show_shortcuts(&mut self) {
        let mut lines = vec!["Window".to_string()];
        let row = |id: &str, binds: Option<&Vec<String>>| -> Option<String> {
            let keys: Vec<String> = binds?.iter().filter(|b| !b.is_empty()).map(|b| keys::display(b)).collect();
            if keys.is_empty() { return None; }
            let mark = if NOT_HERE.contains(&id) { "  (window only)" } else { "" };
            Some(format!("  {:<28} {}{mark}", id.replace('_', " "), keys.join(", ")))
        };
        lines.extend(APP_ACTIONS.iter().filter_map(|a| row(a.id, self.kb.app.get(a.id))));
        lines.push(String::new());
        lines.push("Editor".into());
        lines.extend(EDITOR_ACTIONS.iter().filter_map(|a| row(a.id, self.kb.editor.get(a.id))));
        lines.push(String::new());
        lines.push("Terminal".into());
        for (keys, what) in [("Esc", "the pages / stop"), ("Enter", "edit the page / follow the link"),
                             ("Tab", "next link (pages)"), ("Mouse", "click a page or a place; drag the border")] {
            lines.push(format!("  {what:<28} {keys}"));
        }
        self.mode = Mode::Popup(Popup::text(format!("Shortcuts: {}", self.kb.active_profile), lines));
    }

    /// What a frame of the window lists (the TODO headlines, the tags, the pages
    /// linking to the page in view, the links to pages not created), and its title.
    pub(super) fn frame_items(&self, frame: Frame) -> (String, Vec<(String, Target)>) {
        let names = self.prefs.case_insensitive_links;
        match frame {
            Frame::Todos => {
                let Some(keywords) = self.prefs.todo() else { return ("To do".into(), vec![]) };
                let mut hits = self.project.todos(&keywords);
                // Those to do first, by page
                hits.sort_by_key(|a| (a.done, a.page.to_lowercase(), a.line));
                ("To do".into(), hits.into_iter().map(|h| {
                    (format!("{} {} {}  — {}", if h.done { "✓" } else { "·" }, h.keyword, h.title, h.page), Target::Page(h.path, h.line))
                }).collect())
            }
            Frame::Tags => {
                let mut tags = self.project.tags(self.prefs.tags().org);
                tags.sort_by_key(|t| t.name.to_lowercase());
                ("Tags".into(), tags.into_iter().map(|t| (format!("{} ({})", t.name, t.count), Target::Tag(t.name))).collect())
            }
            Frame::Backlinks => {
                let Some(path) = self.current_path() else { return ("Backlinks".into(), vec![]) };
                let name = crate::project::files::page_name(&path);
                (format!("Linking to {name}"), self.project.backlinks(&path, names).into_iter().map(|p| {
                    (crate::project::files::page_name(&p), Target::Mention(p, name.clone()))
                }).collect())
            }
            Frame::Broken => {
                let mut items = vec![];
                for b in self.project.broken_links(names) {
                    items.push((format!("{} ({})", b.target, b.count.max(1)), Target::None));
                    for source in b.sources {
                        items.push((format!("  ← {}", crate::project::files::page_name(&source)), Target::Mention(source, b.target.clone())));
                    }
                }
                ("Pages not created".into(), items)
            }
            _ => (String::new(), vec![]),
        }
    }

    /// The shortcut of a frame (TODO, tags, backlinks, broken links): the focus
    /// on it when a column shows it, as the window does; else it opens over the page.
    fn panel(&mut self, frame: Frame) {
        if frame == Frame::Todos && self.prefs.todo().is_none() { self.status = "TODO keywords are off (settings)".into(); return; }
        if self.frame_shown_here(frame) {
            self.focus_frame(frame);
            return;
        }
        let (title, items) = self.frame_items(frame);
        self.mode = Mode::Popup(Popup::pick(title, items));
    }

    /// The headlines and pages holding the tag `tag`.
    fn tag_panel(&mut self, tag: &str) {
        let items = self.project.search_tags(tag, self.prefs.tags().org).into_iter().map(|h| {
            let place = h.heading.map(|t| format!(" › {t}")).unwrap_or_default();
            (format!("{}{place}", h.page), Target::Page(h.path, h.line))
        }).collect();
        self.mode = Mode::Popup(Popup::pick(format!("Tag {tag}"), items));
    }

    /// The first line of the page `path` naming `name` (a link to it), else its first line.
    fn line_naming(&self, path: &str, name: &str) -> usize {
        let text = self.project.read(path).unwrap_or_default().to_lowercase();
        let name = name.to_lowercase();
        text.lines().position(|l| l.contains(&name)).unwrap_or(0)
    }

    /// The line chosen in the popup opened: a page at its line, or the headlines of a tag.
    pub(super) fn open_target(&mut self) {
        if let Some(target) = self.popup().and_then(|p| p.targets.get(p.selected).cloned()) { self.open(target); }
    }

    /// Open what a line of a list leads to: a page at its line, the headlines of a tag.
    pub(super) fn open(&mut self, target: Target) {
        let (path, line) = match target {
            Target::Page(path, line) => (path, line),
            Target::Mention(path, name) => { let line = self.line_naming(&path, &name); (path, line) }
            Target::Tag(tag) => return self.tag_panel(&tag),
            Target::Action(id) => {
                self.mode = Mode::Normal;
                self.run_action(id);
                return;
            }
            Target::None => return,
        };
        self.mode = Mode::Normal;
        self.hits = None;
        self.edit(&path);
        if let Some(e) = &mut self.editor && e.path == path { e.goto_line(line); }
    }

    fn add_tag(&mut self, tag: &str) {
        let tag = tag.trim_start_matches(['#', ':']).trim_end_matches(':');
        if let Some(e) = &mut self.editor {
            match e.format().add_page_tag(&e.text(), tag) {
                Some(text) => e.set_text(&text),
                None => self.status = format!("already tagged {tag}"),
            }
            return;
        }
        let Some(page) = self.selected() else { return };
        let text = self.project.read(&page.path).unwrap_or_default();
        match orgamnesia_core::Format::of_page(&page.path).add_page_tag(&text, tag) {
            Some(text) => if let Err(e) = self.project.write(&page.path, text, self.prefs.tags()) { self.status = e.to_string(); },
            None => self.status = format!("already tagged {tag}"),
        }
    }

    /// The pages holding `query`, in place of the list; opening one searches it there.
    fn project_search(&mut self, query: &str) {
        let open = self.editor.as_ref().map(|e| (e.path.clone(), e.text())).into_iter().collect();
        let results = self.project.search(query, SearchOptions::default(), &open);
        self.status = format!("{} matches in {} pages", results.total, results.pages.len());
        if !self.close() { return; }
        self.hits = Some((query.to_string(), results.pages.into_iter().map(|p| (p.path, p.count)).collect()));
        self.filter.clear();
        self.list.select_first();
        self.moved();
    }

    fn project_replace(&mut self, from: &str, to: &str) {
        if !self.save() { return; }
        match self.project.replace_all(from, SearchOptions::default(), to, &ReplaceSkip::default(), self.prefs.tags()) {
            Ok(done) => self.status = format!("{} replaced in {} pages", done.replacements, done.pages),
            Err(e) => self.status = e.to_string(),
        }
        self.reload();
    }

    /// The page edited read again from the disk (changed by a replacement, a pull).
    fn reload(&mut self) {
        let Some(e) = &self.editor else { return };
        if e.dirty || !e.page { return; }
        let (path, cursor) = (e.path.clone(), e.cursor);
        if let Ok(text) = self.project.read(&path) {
            let mut fresh = Editor::new(path, &text, true);
            fresh.prefs = super::editor::EditPrefs::from(&self.prefs);
            fresh.cursor = cursor.min(text.chars().count());
            self.editor = Some(fresh);
        }
    }

    /// Run a git command on the project, the page saved first; the pages read again after.
    fn git(&mut self, run: impl FnOnce(&str, &orgamnesia_core::Prefs) -> Result<String, crate::error::AppError>) {
        if !self.save() { return; }
        self.status = match run(&self.project.path, &self.prefs) {
            Ok(out) => out.lines().last().unwrap_or("done").to_string(),
            Err(e) => e.to_string(),
        };
        let _ = self.project.refresh(self.prefs.tags());
        self.reload();
        self.look_at_git();
    }

    fn git_commit(&mut self, message: &str) {
        let message = message.to_string();
        // And pushed, when the settings say so (git_commit_push)
        self.git(move |p, prefs| {
            let out = crate::git::commit(p, &message, None)?;
            if prefs.git_commit_push { crate::git::push(p, &prefs.git_ssh_key) } else { Ok(out) }
        });
    }

    /// The commit message proposed: the settings' template filled
    /// (`git_commit_template`), else how many files changed.
    fn commit_message(&self) -> String {
        let changes = crate::git::changes(&self.project.path).unwrap_or_default();
        let now = chrono::Local::now();
        let template = self.prefs.git_commit_template.trim();
        if template.is_empty() { return format!("Orgamnesia: {} files", changes.len()); }
        orgamnesia_core::model::fill_template(template, &changes, &now.format("%Y-%m-%d").to_string(), &now.format("%H:%M").to_string())
    }

    /// The settings are YAML since 1.8.0: the JSON files of the older versions,
    /// converted, no longer needed; their deletion offered (once, if kept).
    pub(super) fn offer_json_removal(&mut self, files: Vec<crate::config_files::File>, settings: crate::settings::Settings) {
        let names: Vec<String> = files.iter().map(crate::config_files::File::legacy_name).collect();
        let label = format!("Settings are YAML now; the old {} hold the same: delete them (kept in history/)? (y/n) ", names.join(", "));
        self.ask(&label, "", Then::RemoveJson(files, Box::new(settings)));
    }

    /// The folder actions of the window's command list (`new_folder`…): what they ask.
    pub(super) fn folder_action(&mut self, id: &str) -> bool {
        match id {
            "new_folder" => {
                let start = self.selected_folder().map(|f| format!("{f}/")).unwrap_or_default();
                self.ask("New folder (Jeux/Quêtes: in a folder): ", &start, Then::NewFolder);
            }
            "move_page" => match self.current_path().filter(|_| self.editor.as_ref().is_none_or(|e| e.page)) {
                Some(path) => {
                    let name = self.project.name_of(&path);
                    let folder = name.rsplit_once('/').map_or("", |(d, _)| d).to_string();
                    self.ask(&format!("Move “{name}” to the folder (empty: at the top): "), &folder, Then::MovePage(path));
                }
                None => self.status = "no page to move".into(),
            },
            "rename_folder" | "delete_folder" => {
                if self.project.folders().is_empty() { self.status = "no folder in this project yet".into(); return true; }
                let start = self.selected_folder().unwrap_or_default();
                if id == "rename_folder" { self.ask("Folder to rename: ", &start, Then::RenameFolder); }
                else { self.ask("Folder to delete: ", &start, Then::DeleteFolder); }
            }
            _ => return false,
        }
        true
    }

    /// The folders of the pages, for Tab in the line typed.
    pub(super) fn folder_names(&self) -> Vec<String> {
        self.project.folders()
    }

    /// Rename (move) the folder `old` `new`, as the window does: its pages, the
    /// links to them, and the pages open, the tabs and the history follow.
    fn rename_folder(&mut self, old: &str, new: &str) {
        if !self.save() || !self.save_other() { return; }
        let base = crate::slash(crate::project::files::pages_dir(&self.project.path));
        let (from, to) = (format!("{base}/{old}/"), format!("{base}/{}/", new.trim_matches('/')));
        if let Err(e) = self.project.rename_folder(old, new, &self.prefs) { self.status = e.to_string(); return; }
        self.follow_paths(|p| p.strip_prefix(&from).map(|rest| format!("{to}{rest}")));
        self.status = format!("folder {old} renamed {new}");
    }

    /// The new name of the page in view asked (F2).
    fn ask_rename(&mut self) {
        if self.editor.as_ref().is_some_and(|e| !e.page) { return; }
        let Some(path) = self.current_path() else { return };
        let name = self.project.name_of(&path);
        self.ask(&format!("Rename “{name}” to: "), &name, Then::Rename(path));
    }

    /// Rename the page `old` `new_name`, as the window does: the links to it
    /// follow when the settings say so (`update_links_on_rename`); the pages
    /// open, the tabs and the history follow it.
    fn rename(&mut self, old: &str, new_name: &str) {
        if !self.save() || !self.save_other() { return; }
        let new = match self.project.rename(old, new_name, &self.prefs) {
            Ok(entry) => entry.path,
            Err(e) => { self.status = e.to_string(); return; }
        };
        self.follow_paths(|p| (p == old).then(|| new.clone()));
        self.select_path(&new);
        self.status = format!("renamed {}", self.project.name_of(&new));
    }

    /// Pages moved (`to` gives the new path of an old one): the tabs, the history
    /// and the pages open follow; those open read again (their links rewritten).
    fn follow_paths(&mut self, to: impl Fn(&str) -> Option<String>) {
        let follow = |p: &mut String| if let Some(new) = to(p) { *p = new; };
        self.tabs.iter_mut().for_each(follow);
        self.back.iter_mut().chain(self.forward.iter_mut()).for_each(follow);
        for e in [&mut self.editor, &mut self.other].into_iter().flatten() {
            if let Some(new) = to(&e.path) { e.path = new; }
            if let Ok(text) = self.project.read(&e.path) && text != e.text() {
                let (cursor, prefs) = (e.cursor, e.prefs);
                *e = Editor::new(e.path.clone(), &text, true);
                (e.cursor, e.prefs) = (cursor.min(text.chars().count()), prefs);
            }
        }
    }

    /// The format of the new page `name` asked (`ask_format`).
    pub(super) fn ask_format(&mut self, name: String) {
        let default = if self.prefs.default_format == Format::Markdown { "md" } else { "org" };
        self.ask(&format!("Format of “{name}” (org/md): "), default, Then::NewPage(name));
    }

    /// Commit what changed, pull, push: the sync of `git_auto_sync_minutes` and
    /// `git_sync_on_quit`, on a project under git.
    pub(super) fn sync(&mut self) {
        if !crate::git::status(&self.project.path).is_some_and(|g| g.repo) { return; }
        let message = self.commit_message();
        self.git(move |p, prefs| {
            // Nothing to commit is no failure
            let _ = crate::git::commit(p, &message, None);
            crate::git::pull(p, &prefs.git_ssh_key, prefs.git_pull_rebase)?;
            crate::git::push(p, &prefs.git_ssh_key)
        });
    }

    /// The commits of the page in view.
    fn history(&mut self) {
        let Some(path) = self.current_path() else { return };
        let lines = match crate::git::log(&self.project.path, &path) {
            Ok(commits) if commits.is_empty() => vec!["No commit of this page.".to_string()],
            Ok(commits) => commits.iter().map(|c| {
                let date = chrono::DateTime::from_timestamp(c.date, 0).map(|d| d.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M").to_string()).unwrap_or_default();
                format!("{date}  {}  {}  {}", &c.hash[..c.hash.len().min(7)], c.author, c.subject)
            }).collect(),
            Err(e) => vec![e.to_string()],
        };
        self.mode = Mode::Popup(Popup::text(format!("History: {}", crate::project::files::page_name(&path)), lines));
    }

    /// The links of the page in view and the pages linking to it (the graph, as text).
    fn links_popup(&mut self) {
        let Some(path) = self.current_path() else { return };
        let mut lines = vec!["Links of this page".to_string()];
        lines.extend(self.page_links().into_iter().map(|l| format!("  → {}", l.target)));
        lines.push(String::new());
        lines.push("Pages linking here".into());
        lines.extend(self.project.backlinks(&path, self.prefs.case_insensitive_links).into_iter().map(|p| format!("  ← {}", crate::project::files::page_name(&p))));
        self.mode = Mode::Popup(Popup::text(format!("Links: {}", crate::project::files::page_name(&path)), lines));
    }

    fn switch_project(&mut self, path: &str) {
        if !self.close() { return; }
        match Project::open(path, self.prefs.tags(), self.prefs.formats(), &self.prefs.journal_folder, &self.prefs.templates_folder) {
            Ok(project) => {
                super::prune_trash(&project, &self.prefs);
                self.project = project;
                self.tabs.clear();
                self.back.clear();
                self.forward.clear();
                self.hits = None;
                self.filter.clear();
                self.list.select_first();
                self.moved();
                self.status = format!("project {}", crate::project::files::page_name(path));
            }
            Err(e) => self.status = e.to_string(),
        }
    }

    /// `settings.yaml` in the editor; what is saved counts at the next start.
    fn open_settings(&mut self) {
        let Some(path) = self.config_dir.as_ref().map(|d| crate::config_files::path(d, &crate::config_files::File::Settings)) else { return };
        if !self.close() { return; }
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let mut e = Editor::new(crate::slash(&path), &text, false);
        e.prefs = super::editor::EditPrefs::from(&self.prefs);
        self.editor = Some(e);
        self.status = "settings.yaml: saved, it counts at the next start".into();
    }

    /// The next (previous) page opened in the editor.
    fn next_tab(&mut self, forward: bool) {
        if self.tabs.is_empty() { return; }
        let here = self.editor.as_ref().and_then(|e| self.tabs.iter().position(|t| *t == e.path));
        let n = self.tabs.len();
        let i = match here { Some(i) if forward => (i + 1) % n, Some(i) => (i + n - 1) % n, None => n - 1 };
        let path = self.tabs[i].clone();
        self.edit(&path);
    }

    /// The page edited closed (saved), the one opened before it in its place.
    fn close_tab(&mut self) {
        let Some(path) = self.editor.as_ref().map(|e| e.path.clone()) else { return };
        if !self.close() { return; }
        self.tabs.retain(|t| *t != path);
        if let Some(last) = self.tabs.last().cloned() { self.edit(&last); }
    }

    /// From the editor to the pages, and back.
    fn other_window(&mut self) {
        if self.editor.is_some() {
            self.close();
        } else if let Some(p) = self.selected() {
            self.edit(&p.path);
        }
    }
}

/// `text` on the system's clipboard, by the terminal (OSC 52: kitty, foot, WezTerm…).
pub fn to_clipboard(text: &str) {
    use std::io::Write;
    let _ = write!(std::io::stdout(), "\x1b]52;c;{}\x07", base64(text.as_bytes()));
    let _ = std::io::stdout().flush();
}

fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, &b)| n | (b as u32) << (16 - 8 * i));
        for i in 0..4 {
            out.push(if i <= chunk.len() { ABC[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quick_open_lists_as_the_window() {
        let file = |n: &str| orgamnesia_core::FileEntry { name: n.into(), path: format!("/p/{n}.org") };
        let files = [file("Ma quête"), file("Quête"), file("Règle"), file("Autre")];
        let names = |items: Vec<QuickItem>| items.into_iter().map(|i| match i { QuickItem::Page { name, open, .. } => format!("{name}{}", if open { "*" } else { "" }), QuickItem::Create(n) => format!("+{n}"), QuickItem::Command { id, .. } => format!(">{id}"), _ => String::new() }).collect::<Vec<_>>();
        assert_eq!(names(quick_items("quete", &[], &files)), ["Quête", "Ma quête"]);
        assert_eq!(names(quick_items("regle", &[], &files)), ["Règle"]);
        // A name no page has: the page to create
        assert_eq!(names(quick_items("quet", &[], &files)), ["Quête", "Ma quête", "+quet"]);
        assert_eq!(names(quick_items("", &["/p/Règle.org".into()], &files)), ["Règle*", "Autre", "Ma quête", "Quête"]);
    }

    #[test]
    fn base64_as_the_terminal_reads_it() {
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64("é".as_bytes()), "w6k=");
    }
}
