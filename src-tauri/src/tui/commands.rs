//! What the terminal does beyond the editor: the line at the bottom where a
//! value is typed (Emacs' minibuffer: a link, a date, a tag, a search…), the
//! window over the page (shortcuts, history, links), and the actions of the
//! window's shortcuts done with the project (search, replace, git, projects).

use orgamnesia_core::keybindings::{APP_ACTIONS, EDITOR_ACTIONS};
use orgamnesia_core::{agenda::{self, When}, edit::Stamp, Format, Frame, ReplaceSkip, SearchOptions};

use super::{frames::Popup, keys, App, Mode};

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
    /// Whether it asks which text of a page changed on disk to keep.
    pub(super) fn asks_disk_conflict(&self) -> bool {
        matches!(self.then, Then::DiskConflict(_))
    }
}

impl Prompt {
    /// What Tab completes the value with: the tags, the pages, the folders.
    pub(super) fn completes(&self) -> Option<fn(&App) -> Vec<String>> {
        match self.then {
            Then::AddTag | Then::TodoFilter => Some(App::tag_names),
            Then::LinkTarget { .. } | Then::FindPage => Some(App::link_names),
            Then::NewFolder | Then::MovePage(_) | Then::RenameFolder | Then::RenameFolderTo(_) | Then::DeleteFolder => Some(App::folder_names),
            _ => None,
        }
    }
}

pub(super) enum Then {
    /// A link written over `a..b` of the page edited: its target, then its text.
    LinkTarget { a: usize, b: usize, text: String },
    LinkText { a: usize, b: usize, target: String },
    /// A date written over `a..b`, as `2026-11-20` or `2026-11-20 14:30`.
    Date { a: usize, b: usize, active: bool, rest: String },
    /// The `SCHEDULED:` or `DEADLINE:` date of the headline of the cursor.
    Plan(&'static str),
    /// The same, for a headline of the agenda (its page, line), the agenda then
    /// shown again from its day, on the line it was on.
    AgendaPlan { path: String, line: usize, key: &'static str, from: i64, selected: usize },
    /// What the TODO frame shows.
    TodoFilter,
    /// The page's `query-replace`: what to replace, then with what.
    ReplaceFrom,
    ReplaceTo(String),
    AddTag,
    ProjectSearch,
    ProjectReplaceFrom,
    ProjectReplaceTo(String),
    ProjectReplaceSure(String, String),
    /// The commit message: then the commit of these files (relative to the
    /// root of the repository), `None`: of all the changes.
    Commit(Option<Vec<String>>),
    /// The page at this path to the trash (`y`).
    DeletePage(String),
    /// Every page of the trash deleted for good (`y`).
    EmptyTrash,
    /// The page of this name opened, made when there is none (`new_page`).
    FindPage,
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
    /// The page at this path, changed on disk while edited: the text edited kept
    /// (`y`, saved) or the one on disk taken.
    DiskConflict(String),
}

/// The actions of the shortcuts the terminal does not do.
pub(super) const NOT_HERE: &[&str] = &["export_pdf", "export_html", "spell_next"];

impl App {
    pub(super) fn ask(&mut self, label: &str, input: &str, then: Then) {
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
            Then::Plan(key) => {
                let today = super::editor::now().day_number();
                let Some(e) = &mut self.editor else { return };
                match agenda::plan_typed(&e.text(), e.line(), e.format(), key, &input, today) {
                    Ok(Some(edit)) => e.edits(&[edit]),
                    Ok(None) => {}
                    Err(_) => self.status = format!("not a date: {input}"),
                }
            }
            Then::AgendaPlan { path, line, key, from, selected } => {
                let today = super::editor::now().day_number();
                let format = Format::of_page(&path);
                let mut bad = false;
                self.change_page(&path, |text| match agenda::plan_typed(text, line, format, key, &input, today) {
                    Ok(edit) => edit.map(|e| vec![e]),
                    Err(_) => { bad = true; None }
                });
                self.agenda(from, selected);
                if bad { self.status = format!("not a date: {input}"); }
            }
            Then::TodoFilter => {
                self.todo_filter = input;
                self.panel(Frame::Todos);
            }
            Then::ProjectSearch if !input.is_empty() => self.project_search(&input),
            Then::ProjectReplaceFrom if !input.is_empty() => self.ask(&format!("Replace “{input}” in the project with: "), "", Then::ProjectReplaceTo(input)),
            Then::ProjectReplaceTo(from) => {
                let n = self.project.search(&from, SearchOptions::default(), &Default::default()).total;
                if n == 0 { self.status = format!("“{from}” not found"); return; }
                self.ask(&format!("Replace {n} × “{from}” with “{input}”? (y/n) "), "", Then::ProjectReplaceSure(from, input));
            }
            Then::ProjectReplaceSure(from, to) if yes(&input) => self.project_replace(&from, &to),
            Then::RemoveJson(files, mut settings) => {
                let Some(dir) = self.config_dir.clone() else { return };
                let done = if yes(&input) {
                    crate::config_files::remove_json(&dir, &files)
                } else {
                    settings.keep_json = true;
                    crate::settings::save_state(&dir, &settings)
                };
                self.status = match done { Ok(()) => String::new(), Err(e) => e.to_string() };
            }
            Then::Commit(files) => self.git_commit(if input.is_empty() { "Orgamnesia" } else { &input }, files),
            Then::DeletePage(path) if yes(&input) => self.delete_page(&path),
            Then::EmptyTrash if yes(&input) => self.status = match self.project.empty_trash() {
                Ok(n) => format!("{n} pages deleted for good"),
                Err(e) => e.to_string(),
            },
            Then::Rename(old) if !input.is_empty() => self.rename(&old, &input),
            Then::FindPage => self.open_or_create(input.trim().to_string()),
            Then::NewPage(name) => {
                let format = match input.to_lowercase().as_str() { "md" | "markdown" => Format::Markdown, _ => Format::Org };
                match self.project.create(&name, Some(format), self.prefs.tags()) {
                    Ok(page) => { self.folders_changed(); self.edit(&page.path) }
                    Err(e) => self.status = e.to_string(),
                }
            }
            Then::ApplyTemplate { text, replace } => match input.to_lowercase().as_str() {
                "r" if replace => self.put_template(&text, true),
                "y" | "o" | "yes" | "oui" | "a" => self.put_template(&text, false),
                _ => {}
            },
            Then::NewFolder if !input.is_empty() => match self.project.create_folder(&input) {
                Ok(name) => { self.folders_changed(); self.status = format!("folder {name} made") }
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
            Then::DeleteFolderSure(name) => if yes(&input) {
                if !self.save() || !self.save_other() { return; }
                match self.project.delete_folder(&name) {
                    Ok(trashed) => {
                        let prefix = format!("{name}/");
                        let gone = |p: &String| self.project.name_of(p).starts_with(&prefix);
                        let gone: Vec<String> = self.tabs.iter().filter(|p| gone(p)).cloned().collect();
                        self.tabs.retain(|p| !gone.contains(p));
                        if self.other.as_ref().is_some_and(|o| gone.contains(&o.path)) { self.single(); }
                        if self.editor.as_ref().is_some_and(|e| gone.contains(&e.path)) { self.editor = None; }
                        self.folders_changed();
                        self.status = format!("folder {name} deleted, {} pages in the trash", trashed.len());
                    }
                    Err(e) => self.status = e.to_string(),
                }
            },
            Then::NewTemplate if !input.is_empty() => match self.project.create_template(&input, self.prefs.default_format, "") {
                Ok(t) => self.edit(&t.path),
                Err(e) => self.status = e.to_string(),
            },
            Then::DiskConflict(path) => if yes(&input) {
                // `refresh` has seen the change: writing goes through
                if self.save() && self.save_other() { self.status = format!("{}: your text kept", self.project.name_of(&path)); }
            } else {
                self.reread(&path, true);
                self.status = format!("{}: the text on disk taken", self.project.name_of(&path));
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
            "schedule" | "deadline" if editing => {
                let Some(e) = &self.editor else { return Some(()) };
                let (key, when) = if id == "schedule" { (agenda::SCHEDULED, When::Scheduled) } else { (agenda::DEADLINE, When::Deadline) };
                let text = e.text();
                let Some(head) = agenda::headline_of(&text, e.line(), e.format()) else {
                    self.status = "not under a headline".into();
                    return Some(());
                };
                let todo = self.prefs.todo();
                let typed = agenda::entries(&text, todo.as_ref(), e.format()).iter().find(|x| x.line == head).map(|x| agenda::typed_date(x, when)).unwrap_or_default();
                self.ask(&format!("{key} (2026-12-01, +3d, fri, 14:30; - removes): "), &typed, Then::Plan(key));
            }
            "agenda" => self.agenda(agenda::week_start(super::editor::now().day_number()), 0),
            "todo_filter" => self.ask("TODOs shown (open, done, all, dated, undated or a keyword; then tags +a-b): ", &self.todo_filter.clone(), Then::TodoFilter),
            "replace" if editing => self.ask("Replace: ", "", Then::ReplaceFrom),
            "add_tag" => self.ask("Tag: ", "", Then::AddTag),
            "project_search" => self.ask("Search the project: ", "", Then::ProjectSearch),
            "project_replace" => self.ask("Replace in the project: ", "", Then::ProjectReplaceFrom),
            "git_commit" => self.commit_popup(),
            "delete_page" => if let Some(path) = self.current_path() {
                let name = self.project.name_of(&path);
                self.ask(&format!("Delete “{name}” (it goes to the trash)? (y/n) "), "", Then::DeletePage(path));
            },
            "trash" => self.trash_popup(),
            "empty_trash" => match self.project.trash_count() {
                Ok(0) => self.status = "the trash is empty".into(),
                Ok(n) => self.ask(&format!("Delete for good the {n} pages of the trash? (y/n) "), "", Then::EmptyTrash),
                Err(e) => self.status = e.to_string(),
            },
            "undo_replace" => self.undo_replace(),
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

/// A yes typed (y, yes, o, oui).
fn yes(input: &str) -> bool {
    matches!(input.to_lowercase().as_str(), "y" | "o" | "yes" | "oui")
}

impl App {
    /// The text of the page `path`: as edited when it is open, else on disk.
    pub(super) fn page_text(&self, path: &str) -> String {
        match [&self.editor, &self.other].into_iter().flatten().find(|e| e.path == path) {
            Some(e) => e.text(),
            None => self.project.read(path).unwrap_or_default(),
        }
    }

    /// The page `path` changed by the edits `f` makes of its text: in its window
    /// when it is open (saved with it), else on disk.
    pub(super) fn change_page(&mut self, path: &str, f: impl FnOnce(&str) -> Option<Vec<agenda::Edit>>) {
        if let Some(e) = [&mut self.editor, &mut self.other].into_iter().flatten().find(|e| e.path == path) {
            if let Some(edits) = f(&e.text()) { e.edits(&edits); }
            return;
        }
        let text = match self.project.read(path) { Ok(t) => t, Err(e) => { self.status = e.to_string(); return } };
        let Some(edits) = f(&text) else { return };
        if let Err(e) = self.project.write(path, agenda::apply(&text, &edits), self.prefs.tags()) { self.status = e.to_string(); }
    }
}

#[cfg(test)]
mod tests;
