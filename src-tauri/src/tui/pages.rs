//! Pages and folders: renamed (links followed), deleted, put in the trash and
//! restored; the pages changed on disk read again.

use orgamnesia_core::Format;

use super::*;
use commands::Then;
use frames::{Popup, Target};

impl App {
    /// The page edited read again from the disk (changed by a replacement, a pull).
    pub(super) fn reload(&mut self) {
        if let Some(path) = self.editor.as_ref().map(|e| e.path.clone()) { self.reread(&path, false); }
    }

    /// The windows on the page `path` read again from the disk, their cursor
    /// kept; those with unsaved changes too only with `even_dirty`.
    pub(super) fn reread(&mut self, path: &str, even_dirty: bool) {
        let Ok(text) = self.project.read(path) else { return };
        for e in [&mut self.editor, &mut self.other].into_iter().flatten() {
            if e.path != path || !e.page || (e.dirty && !even_dirty) { continue; }
            let (cursor, prefs) = (e.cursor, e.prefs);
            *e = Editor::new(path.to_string(), &text, true);
            (e.cursor, e.prefs) = (cursor.min(text.chars().count()), prefs);
        }
    }

    /// Take the changes another program (a sync tool, the window) made on disk:
    /// the list follows, the pages open without unsaved changes are read again;
    /// for one with some, which text to keep is asked (as the window does). The
    /// folder is only read when the watcher saw a change, or with `now`.
    pub(super) fn disk_changes(&mut self, now: bool) {
        let seen = self.disk_watch.as_ref().is_none_or(|(_, c)| c.swap(false, std::sync::atomic::Ordering::Relaxed));
        if let Some(Ok(Some(changes))) = (now || seen).then(|| self.project.refresh(self.prefs.tags())) {
            self.folders_changed();
            let open: Vec<(String, bool)> = [&self.editor, &self.other].into_iter().flatten()
                .filter(|e| e.page).map(|e| (e.path.clone(), e.dirty)).collect();
            for (path, dirty) in open {
                if changes.removed.contains(&path) {
                    self.status = format!("{}: deleted on disk, saving writes it again", self.project.name_of(&path));
                } else if !changes.changed.contains(&path) {
                } else if !dirty {
                    self.reread(&path, false);
                } else if !self.disk_conflicts.contains(&path) {
                    self.disk_conflicts.push(path);
                }
            }
        }
        // Asked when nothing else has the keys
        if matches!(self.mode, Mode::Normal) && !self.disk_conflicts.is_empty() {
            let path = self.disk_conflicts.remove(0);
            let name = self.project.name_of(&path);
            self.ask(&format!("“{name}” changed on disk. Keep your text (y) or take the one on disk (n)? "), "", Then::DiskConflict(path));
        }
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
    pub(super) fn rename_folder(&mut self, old: &str, new: &str) {
        if !self.save() || !self.save_other() { return; }
        let base = crate::slash(crate::project::files::pages_dir(&self.project.path));
        let (from, to) = (format!("{base}/{old}/"), format!("{base}/{}/", new.trim_matches('/')));
        let renamed = self.project.rename_folder(old, new, &self.prefs);
        self.folders_changed();
        if let Err(e) = renamed { self.status = e.to_string(); return; }
        self.follow_paths(|p| p.strip_prefix(&from).map(|rest| format!("{to}{rest}")));
        self.status = format!("folder {old} renamed {new}");
    }

    /// The new name of the page in view asked (F2).
    pub(super) fn ask_rename(&mut self) {
        if self.editor.as_ref().is_some_and(|e| !e.page) { return; }
        let Some(path) = self.current_path() else { return };
        let name = self.project.name_of(&path);
        self.ask(&format!("Rename “{name}” to: "), &name, Then::Rename(path));
    }

    /// Rename the page `old` `new_name`, as the window does: the links to it
    /// follow when the settings say so (`update_links_on_rename`); the pages
    /// open, the tabs and the history follow it.
    pub(super) fn rename(&mut self, old: &str, new_name: &str) {
        if !self.save() || !self.save_other() { return; }
        let renamed = self.project.rename(old, new_name, &self.prefs);
        self.folders_changed();
        let new = match renamed {
            Ok(entry) => entry.path,
            Err(e) => { self.status = e.to_string(); return; }
        };
        self.follow_paths(|p| (p == old).then(|| new.clone()));
        self.select_path(&new);
        self.status = format!("renamed {}", self.project.name_of(&new));
    }

    /// Pages moved (`to` gives the new path of an old one): the tabs, the history
    /// and the pages open follow; those open read again (their links rewritten).
    pub(super) fn follow_paths(&mut self, to: impl Fn(&str) -> Option<String>) {
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

    /// The page at `path` to the trash (saved first): its tabs and windows closed.
    pub(super) fn delete_page(&mut self, path: &str) {
        if !self.save() || !self.save_other() { return; }
        let name = self.project.name_of(path);
        match self.project.delete(path) {
            Ok(_) => {
                self.tabs.retain(|p| p != path);
                self.back.retain(|p| p != path);
                self.forward.retain(|p| p != path);
                if self.other.as_ref().is_some_and(|o| o.path == path) { self.single(); }
                if self.editor.as_ref().is_some_and(|e| e.path == path) { self.editor = None; }
                self.folders_changed();
                self.status = format!("{name} is in the trash (“trash” brings it back)");
            }
            Err(e) => self.status = e.to_string(),
        }
    }

    /// The pages of the trash, newest first: Enter brings one back.
    pub(super) fn trash_popup(&mut self) {
        match self.project.trash_list() {
            Ok(pages) => {
                let items = pages.into_iter().map(|f| (format!("  {}", f.name), Target::Trashed(f.path))).collect();
                self.mode = Mode::Popup(Popup { keys: Some("↑↓ · Enter: bring it back · Esc"), ..Popup::pick("Trash".into(), items) });
            }
            Err(e) => self.status = e.to_string(),
        }
    }

    /// The page of the trash at `trashed` brought back, and shown.
    pub(super) fn restore_trashed(&mut self, trashed: &str) {
        self.mode = Mode::Normal;
        match self.project.restore(trashed, self.prefs.tags()) {
            Ok(page) => {
                self.folders_changed();
                self.status = format!("{} brought back", page.name);
                self.show(&page.path);
            }
            Err(e) => self.status = e.to_string(),
        }
    }

    /// The last replacement in the project undone (but the pages changed since).
    pub(super) fn undo_replace(&mut self) {
        if !self.project.can_undo_replace() { self.status = "no replacement to undo".into(); return; }
        if !self.save() || !self.save_other() { return; }
        self.status = match self.project.undo_replace(self.prefs.tags()) {
            Ok(done) if done.skipped > 0 => format!("{} pages put back, {} changed since left as they are", done.pages, done.skipped),
            Ok(done) => format!("{} pages put back", done.pages),
            Err(e) => e.to_string(),
        };
        self.reload();
    }

    /// The links of the page in view and the pages linking to it (the graph, as text).
    pub(super) fn links_popup(&mut self) {
        let Some(path) = self.current_path() else { return };
        let mut lines = vec!["Links of this page".to_string()];
        lines.extend(self.page_links().into_iter().map(|l| format!("  → {}", l.target)));
        lines.push(String::new());
        lines.push("Pages linking here".into());
        lines.extend(self.project.backlinks(&path, self.prefs.case_insensitive_links).into_iter().map(|p| format!("  ← {}", crate::project::files::page_name(&p))));
        self.mode = Mode::Popup(Popup::text(format!("Links: {}", crate::project::files::page_name(&path)), lines));
    }
}
