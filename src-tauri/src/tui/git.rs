//! The git extension in the terminal: commit (all, or the files ticked), pull and
//! push, the history of a page and its versions restored.

use super::{commands::Then, frames::{Popup, Target}, App, Mode};

/// The short form of a commit hash.
pub(super) fn short(hash: &str) -> &str {
    &hash[..hash.len().min(7)]
}

impl App {
    /// Run a git command on the project, the page saved first; the pages read again after.
    pub(super) fn git(&mut self, run: impl FnOnce(&str, &orgamnesia_core::Prefs) -> Result<String, crate::error::AppError>) {
        if !self.save() { return; }
        self.status = match run(&self.project.path, &self.prefs) {
            Ok(out) => out.lines().last().unwrap_or("done").to_string(),
            Err(e) => e.to_string(),
        };
        let _ = self.project.refresh(self.prefs.tags());
        self.folders_changed();
        self.reload();
        self.look_at_git();
    }

    pub(super) fn git_commit(&mut self, message: &str, files: Option<Vec<String>>) {
        let message = message.to_string();
        // And pushed, when the settings say so (git_commit_push)
        self.git(move |p, prefs| {
            let out = crate::git::commit(p, &message, files.as_deref())?;
            if prefs.git_commit_push { crate::git::push(p, &prefs.git_ssh_key) } else { Ok(out) }
        });
    }

    /// The commit message proposed: the settings' template filled
    /// (`git_commit_template`), else how many files changed.
    pub(super) fn commit_message(&self) -> String {
        let changes = crate::git::changes(&self.project.path).unwrap_or_default();
        let now = chrono::Local::now();
        let template = self.prefs.git_commit_template.trim();
        if template.is_empty() { return format!("Orgamnesia: {} files", changes.len()); }
        orgamnesia_core::model::fill_template(template, &changes, &now.format("%Y-%m-%d").to_string(), &now.format("%H:%M").to_string())
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
    /// The commits of the page in view: Enter shows what one changed, `r` puts
    /// that version back in the editor (undo takes it back).
    pub(super) fn history(&mut self) {
        let Some(path) = self.current_path() else { return };
        let title = format!("History: {}", crate::project::files::page_name(&path));
        let commits = match crate::git::log(&self.project.path, &path) {
            Ok(commits) if commits.is_empty() => vec![],
            Ok(commits) => commits,
            Err(e) => { self.mode = Mode::Popup(Popup::text(title, vec![e.to_string()])); return; }
        };
        let items = commits.into_iter().map(|c| {
            let date = chrono::DateTime::from_timestamp(c.date, 0).map(|d| d.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M").to_string()).unwrap_or_default();
            let line = format!("{date}  {}  {}  {}", short(&c.hash), c.author, c.subject);
            (line, Target::Commit { hash: c.hash, file: c.path, page: path.clone() })
        }).collect();
        self.mode = Mode::Popup(Popup { keys: Some("↑↓ · Enter: its changes · r: put this version back · Esc"), ..Popup::pick(title, items) });
    }

    /// The version of the page of `Target::Commit` put back in the editor, as an
    /// edit: undo takes it back.
    pub(super) fn restore_version(&mut self) {
        let Some(Target::Commit { hash, file, page }) = self.popup().and_then(|p| p.targets.get(p.selected).cloned()) else { return };
        match crate::git::show(&self.project.path, &hash, &file) {
            Ok(text) => {
                self.mode = Mode::Normal;
                if self.editor.as_ref().is_none_or(|e| e.path != page) { self.edit(&page); }
                if let Some(e) = &mut self.editor && e.path == page {
                    e.set_text(&text);
                    self.status = format!("version {} put back (undo takes it back)", short(&hash));
                }
            }
            Err(e) => self.status = e.to_string(),
        }
    }

    /// The files changed since the last commit, all ticked: Space ticks one or
    /// not, Enter shows what changed in it, `c` commits the ticked ones.
    pub(super) fn commit_popup(&mut self) {
        if !self.save() || !self.save_other() { return; }
        let changes = match crate::git::changes(&self.project.path) {
            Ok(changes) => changes,
            Err(e) => { self.status = e.to_string(); return; }
        };
        if changes.is_empty() { self.status = "nothing to commit".into(); return; }
        use orgamnesia_core::GitChangeKind as K;
        let items = changes.into_iter().map(|c| {
            let kind = match c.kind { K::Added => "added", K::Modified => "modified", K::Deleted => "deleted", K::Renamed => "renamed" };
            (format!("[x] {:<9} {}", kind, c.path), Target::Change { file: c.path, old: c.old_path })
        }).collect();
        self.mode = Mode::Popup(Popup { keys: Some("↑↓ · Space: tick · Enter: changes · c: commit · Esc"), ..Popup::pick("Commit".into(), items) });
    }

    /// The line of the commit's files chosen ticked, or no longer.
    pub(super) fn tick_change(&mut self) {
        let Mode::Popup(p) = &mut self.mode else { return };
        if !matches!(p.targets.get(p.selected), Some(Target::Change { .. })) { return; }
        let line = &mut p.lines[p.selected];
        let ticked = line.starts_with("[x]");
        line.replace_range(..3, if ticked { "[ ]" } else { "[x]" });
    }

    /// The ticked files of the commit's list committed: their message asked.
    pub(super) fn commit_ticked(&mut self) {
        let Some(p) = self.popup() else { return };
        let (mut files, mut all) = (vec![], true);
        for (line, target) in p.lines.iter().zip(&p.targets) {
            let Target::Change { file, old } = target else { continue };
            if line.starts_with("[x]") { files.push(file.clone()); files.extend(old.clone()); } else { all = false; }
        }
        if files.is_empty() { self.status = "no file ticked".into(); return; }
        let message = self.commit_message();
        self.ask("Commit message: ", &message, Then::Commit((!all).then_some(files)));
    }

    /// A diff over the popup it comes from (back to it when closed).
    pub(super) fn diff_popup(&mut self, title: String, diff: Result<String, crate::error::AppError>) {
        let lines = match diff {
            Ok(text) if text.is_empty() => vec!["(no change)".into()],
            Ok(text) => text.lines().map(String::from).collect(),
            Err(e) => vec![e.to_string()],
        };
        let parent = match std::mem::replace(&mut self.mode, Mode::Normal) { Mode::Popup(p) => Some(Box::new(p)), _ => None };
        self.mode = Mode::Popup(Popup { title, lines, diff: true, parent, ..Popup::default() });
    }
}
