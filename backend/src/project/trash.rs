//! The trash of the project (`.trash/` in its folder): pages deleted go there,
//! dated, and come back from it; it is emptied by hand or pruned by age and size.

use std::path::{Path, PathBuf};

use orgamnesia_core::{FileEntry, Format, TagSyntax};

use super::{files, Project};
use crate::error::AppError;

/// A page in the trash.
struct Trashed {
    path: PathBuf,
    /// When it was deleted (ms since 1970), from its name.
    deleted_at: Option<u128>,
    size: u64,
}

impl Project {
    /// Delete a page: it goes to the project's trash (`.trash/`, named
    /// `<time>-<page>.org` or `.md`), from where `restore` brings it back. Returns its path there.
    pub fn delete(&mut self, path: &str) -> Result<String, AppError> {
        self.check_page(path)?;
        let to = files::to_trash(&self.path, path)?;
        self.forget(path);
        Ok(crate::slash(&to))
    }

    /// Bring back a page `delete` put in the trash, under its name.
    pub fn restore(&mut self, trashed: &str, tags: TagSyntax) -> Result<FileEntry, AppError> {
        let p = Path::new(trashed);
        let in_trash = p.parent().and_then(|d| std::fs::canonicalize(d).ok())
            == std::fs::canonicalize(self.trash_dir()?).ok();
        let name = p.file_stem().and_then(|s| s.to_str()).and_then(files::from_trash_name);
        let name = name.as_deref();
        let format = Format::of_path(trashed).filter(|f| self.formats.accepts(*f));
        let (Some(name), true, Some(format)) = (name, in_trash, format) else {
            return Err(AppError::NotAPage(trashed.to_string()));
        };
        let path = files::page_path(&self.path, name, format, &self.journal)?;
        if path.exists() {
            return Err(AppError::PageExists(name.to_string()));
        }
        std::fs::rename(p, &path)?;
        let path = crate::slash(&path);
        self.store(&path, std::fs::read_to_string(&path).unwrap_or_default(), tags);
        self.remember(&path);
        Ok(FileEntry { name: name.to_string(), path, ..Default::default() })
    }

    /// The pages in the trash, newest first: the name each had (`Jeux/Dragon`),
    /// and its path in the trash (for `restore`).
    pub fn trash_list(&self) -> Result<Vec<FileEntry>, AppError> {
        let mut pages = self.trashed()?;
        pages.sort_by_key(|p| std::cmp::Reverse(p.deleted_at));
        Ok(pages.into_iter().filter_map(|p| {
            let name = p.path.file_stem().and_then(|s| s.to_str()).and_then(files::from_trash_name)?;
            Some(FileEntry { name, path: crate::slash(&p.path), ..Default::default() })
        }).collect())
    }

    /// Number of pages in the trash.
    pub fn trash_count(&self) -> Result<usize, AppError> {
        Ok(self.trashed()?.len())
    }

    /// Delete for good every page in the trash. Returns how many went.
    pub fn empty_trash(&self) -> Result<usize, AppError> {
        self.prune_trash(Some(0), 0)
    }

    /// Delete for good the pages of the trash deleted more than `keep_days` days
    /// ago (`Some(0)`: all of them; `None`: no limit), then the oldest ones while
    /// the trash holds more than `max_bytes` bytes (0: no limit). Returns how many went.
    pub fn prune_trash(&self, keep_days: Option<u64>, max_bytes: u64) -> Result<usize, AppError> {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
        let limit = keep_days.map(|d| now.saturating_sub(u128::from(d) * 86_400_000));
        let mut pages = self.trashed()?;
        // Oldest first; a page with no time in its name counts as just deleted
        pages.sort_by_key(|p| p.deleted_at.unwrap_or(now));
        let mut size: u64 = pages.iter().map(|p| p.size).sum();
        let mut gone = 0;
        for p in pages {
            let too_old = limit.is_some_and(|l| p.deleted_at.unwrap_or(now) <= l);
            let too_big = max_bytes > 0 && size > max_bytes;
            if !too_old && !too_big { continue; }
            std::fs::remove_file(&p.path)?;
            size -= p.size;
            gone += 1;
        }
        Ok(gone)
    }

    /// The pages in the trash.
    fn trashed(&self) -> Result<Vec<Trashed>, AppError> {
        Ok(std::fs::read_dir(self.trash_dir()?)?
            .filter_map(|e| e.ok())
            .filter(|e| Format::of_path(&e.path().to_string_lossy()).is_some())
            .map(|e| {
                let path = e.path();
                let deleted_at = path.file_stem().and_then(|s| s.to_str()).and_then(|s| s.split_once('-'))
                    .and_then(|(t, _)| t.parse().ok());
                Trashed { size: e.metadata().map_or(0, |m| m.len()), path, deleted_at }
            })
            .collect())
    }

    /// The trash folder of the project (see `files::trash_dir`).
    fn trash_dir(&self) -> Result<PathBuf, AppError> {
        Ok(files::trash_dir(&self.path)?)
    }
}
