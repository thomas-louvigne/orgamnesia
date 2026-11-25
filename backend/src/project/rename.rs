//! Renaming a page (its links rewritten in every page naming it), and the
//! folders of the pages: made, renamed (moved with their pages), deleted.

use std::path::Path;

use orgamnesia_core::{pages::Resolver, FileEntry, Format, Prefs};

use super::{files, links, valid_path, Project};
use crate::{error::AppError, tags};

/// Whether renaming the page `old` to `new` would land on another page at
/// `new_path`. Only the case changing (`note` → `Note`), the page itself is found
/// there on a disk ignoring case (macOS, Windows): not another one, unless a
/// file of exactly that name is there too (a disk minding case).
fn taken(new_path: &Path, old: &str, new: &str) -> bool {
    if !new_path.exists() { return false; }
    if old.to_lowercase() != new.to_lowercase() { return true; }
    let exact = new_path.file_name();
    new_path.parent().and_then(|d| std::fs::read_dir(d).ok())
        .is_some_and(|mut entries| entries.any(|e| e.is_ok_and(|e| Some(e.file_name().as_os_str()) == exact)))
}

/// What is left in the folder `from` (no page: they went first) put at the same
/// place in `to`, then `from` removed.
fn move_rest(from: &Path, to: &Path) -> std::io::Result<()> {
    if !to.exists() {
        if let Some(parent) = to.parent() { std::fs::create_dir_all(parent)?; }
        return std::fs::rename(from, to);
    }
    for e in std::fs::read_dir(from)?.filter_map(|e| e.ok()) {
        let target = to.join(e.file_name());
        if e.file_type()?.is_dir() { move_rest(&e.path(), &target)?; } else if !target.exists() { std::fs::rename(e.path(), target)?; }
    }
    // Not empty: a file was in the way, it stays here
    let _ = std::fs::remove_dir(from);
    Ok(())
}

/// The folder `dir` removed with its folders, those holding nothing (but empty folders).
fn remove_empty(dir: &Path) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.filter_map(|e| e.ok()).filter(|e| e.file_type().is_ok_and(|t| t.is_dir())) { remove_empty(&e.path()); }
    }
    let _ = std::fs::remove_dir(dir);
}

impl Project {
    /// Rename a page; `Jeux/Dragon` moves it to the folder `Jeux` (made if needed).
    /// When the settings say so, the links to it in every page follow.
    pub fn rename(&mut self, old_path: &str, new_name: &str, prefs: &Prefs) -> Result<FileEntry, AppError> {
        self.check_page(old_path)?;
        let new_name = valid_path(new_name)?;
        let ext = Format::of_page(old_path).ext();
        let old_name = self.name_of(old_path);
        // A day of the journal keeps its folder, unless sent to one of the pages
        let in_journal = Path::new(old_path).parent() == Some(files::journal_dir(&self.path, &self.journal).as_path())
            && !new_name.contains('/');
        let new_path = if in_journal {
            Path::new(old_path).with_file_name(format!("{new_name}.{ext}"))
        } else {
            files::pages_dir(&self.path).join(format!("{new_name}.{ext}"))
        };
        if new_name != old_name && taken(&new_path, &old_name, &new_name) {
            return Err(AppError::PageExists(new_name));
        }
        // The links leading to the page, as they did before
        let files = self.files();
        let resolver = Resolver::new(&files, prefs.case_insensitive_links, self.formats.default);
        // A link naming one of its aliases still leads to it: left as it is
        let leads_here = |target: &str| resolver.resolve_name(target).is_some_and(|f| f.path == old_path);
        let by_name = leads_here(&old_name);
        // In the page's folder, as the interface knows it (not the canonical one)
        if let Some(dir) = new_path.parent() { std::fs::create_dir_all(dir)?; }
        std::fs::rename(old_path, &new_path)?;
        let new_path = crate::slash(&new_path);
        let tags = prefs.tags();

        self.forget(old_path);
        self.store(&new_path, std::fs::read_to_string(&new_path).unwrap_or_default(), tags);
        self.remember(&new_path);

        if prefs.update_links && old_name != new_name {
            // Only the pages linking to it (the index says which) or naming it in a
            // tag, not all of them: a folder renamed does this once per page
            let norm = |s: &str| orgamnesia_core::names::page_key(s, prefs.case_insensitive_links);
            let old_key = norm(&old_name);
            let mut paths = self.links.sources_where(leads_here);
            if by_name {
                paths.extend(self.pages.iter()
                    .filter(|(p, text)| tags::written_tags(text, tags.org, Format::of_page(p)).iter().any(|t| norm(t) == old_key))
                    .map(|(p, _)| p.clone()));
            }
            for path in paths {
                let Ok(text) = std::fs::read_to_string(&path) else { continue };
                let rename = links::Rename { leads_here: &leads_here, old: &old_name, by_name, new: &new_name };
                let updated = links::rewrite_links(&text, &rename, prefs.case_insensitive_links, tags, Format::of_page(&path));
                if let Some(updated) = updated {
                    // The page is renamed already: a page that can't be written keeps
                    // its old link (shown among the broken ones), the others follow
                    if let Err(e) = crate::write_atomic(&path, &updated) {
                        eprintln!("rename: links of {path} not updated: {e}");
                        continue;
                    }
                    self.store(&path, updated, tags);
                    self.remember(&path);
                }
            }
        }
        Ok(FileEntry { name: new_name, path: new_path, ..Default::default() })
    }

    /// Make the folder `name` of the pages (`Jeux/Quêtes`: a folder in a folder).
    pub fn create_folder(&self, name: &str) -> Result<String, AppError> {
        let name = valid_path(name)?;
        let dir = files::pages_dir(&self.path).join(&name);
        if dir.exists() { return Err(AppError::PageExists(name)); }
        std::fs::create_dir_all(dir)?;
        Ok(name)
    }

    /// The pages in the folder `folder` of the pages, and in its folders.
    fn pages_in(&self, folder: &str) -> Vec<FileEntry> {
        let prefix = format!("{folder}/");
        self.files().into_iter().filter(|f| f.name.starts_with(&prefix)).collect()
    }

    /// Rename (or move) the folder `old` of the pages `new`: each of its pages
    /// renamed, the links to them following as for one page (see `rename`), then
    /// what else it holds (images…). Refused when `new` is there already.
    pub fn rename_folder(&mut self, old: &str, new: &str, prefs: &Prefs) -> Result<String, AppError> {
        let (old, new) = (valid_path(old)?, valid_path(new)?);
        let base = files::pages_dir(&self.path);
        let (from, to) = (base.join(&old), base.join(&new));
        if !from.is_dir() { return Err(AppError::NotAPage(old)); }
        if to.exists() || to.starts_with(&from) { return Err(AppError::PageExists(new)); }
        for page in self.pages_in(&old) {
            self.rename(&page.path, &format!("{new}{}", &page.name[old.len()..]), prefs)?;
        }
        move_rest(&from, &to)?;
        Ok(new)
    }

    /// Delete the folder `name` of the pages: its pages go to the trash (back in
    /// their folder when restored), then the folders left empty. What else it
    /// holds stays (an image…), with its folder. Returns the pages' paths in the trash.
    pub fn delete_folder(&mut self, name: &str) -> Result<Vec<String>, AppError> {
        let name = valid_path(name)?;
        let dir = files::pages_dir(&self.path).join(&name);
        if !dir.is_dir() { return Err(AppError::NotAPage(name)); }
        let mut trashed = vec![];
        for page in self.pages_in(&name) {
            trashed.push(self.delete(&page.path)?);
        }
        remove_empty(&dir);
        Ok(trashed)
    }
}
