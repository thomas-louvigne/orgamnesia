use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use crate::error::AppError;

pub use orgamnesia_core::FileEntry;

/// Directory holding the pages: `<vault>/pages/` when it exists, else the folder itself.
pub fn pages_dir(vault_path: &str) -> PathBuf {
    let pages = Path::new(vault_path).join("pages");
    if pages.is_dir() { pages } else { PathBuf::from(vault_path) }
}

fn has_org_files(dir: &Path) -> bool {
    std::fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok())
            .any(|e| e.path().extension().and_then(|s| s.to_str()) == Some("org")))
        .unwrap_or(false)
}

/// List all `.org` files of the vault (see `pages_dir`), sorted by name.
pub fn list_org_files(vault_path: &str) -> Result<Vec<FileEntry>, AppError> {
    let pages = pages_dir(vault_path);
    if !pages.exists() {
        return Err(AppError::VaultNotFound(format!(
            "{vault_path} not found"
        )));
    }
    let mut files: Vec<FileEntry> = std::fs::read_dir(&pages)?
        .filter_map(|entry| entry.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("org"))
        .map(|e| {
            let path = e.path();
            let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
            FileEntry { name, path: path.to_string_lossy().to_string() }
        })
        .collect();
    files.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(files)
}

/// Modification time and size of a file, to notice when it changes on disk.
pub type Stamp = (Option<SystemTime>, u64);

/// Stamp of every page, by path.
pub type Snapshot = HashMap<String, Stamp>;

pub fn stamp(path: &str) -> Option<Stamp> {
    let m = std::fs::metadata(path).ok()?;
    Some((m.modified().ok(), m.len()))
}

pub fn snapshot(files: &[FileEntry]) -> Snapshot {
    files.iter()
        .filter_map(|f| Some((f.path.clone(), stamp(&f.path)?)))
        .collect()
}

/// Paths whose stamp differs between two snapshots (new or modified), and paths gone.
pub fn diff(old: &Snapshot, new: &Snapshot) -> (Vec<String>, Vec<String>) {
    let changed = new.iter().filter(|(p, s)| old.get(*p) != Some(*s)).map(|(p, _)| p.clone()).collect();
    let removed = old.keys().filter(|p| !new.contains_key(*p)).cloned().collect();
    (changed, removed)
}

/// Ensure `<vault>/pages/` and `<vault>/assets/` exist, unless the folder is a
/// plain directory of `.org` files (then it is used as is).
pub fn ensure_structure(vault_path: &str) -> Result<(), AppError> {
    let base = Path::new(vault_path);
    if !base.is_dir() {
        return Err(AppError::VaultNotFound(format!("{vault_path} is not a folder")));
    }
    if !base.join("pages").is_dir() && has_org_files(base) {
        return Ok(());
    }
    for dir in ["pages", "assets"] {
        std::fs::create_dir_all(base.join(dir))?;
    }
    Ok(())
}

/// File stem of a `.org` path (the page name).
pub fn page_name(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string()
}

/// Full path for a new page in the vault.
pub fn page_path(vault_path: &str, name: &str) -> PathBuf {
    pages_dir(vault_path).join(format!("{name}.org"))
}

/// True when a page holds no text: only spaces, tabs, line breaks or `*`.
pub fn is_blank(content: &str) -> bool {
    content.chars().all(|c| c.is_whitespace() || c == '*')
}

/// True when a page holds nothing but its title heading (`* Name`, as a new page starts).
pub fn is_title_only(content: &str) -> bool {
    let mut lines = content.lines().filter(|l| !l.trim().is_empty());
    let Some(first) = lines.next() else { return false };
    let first = first.trim_start();
    let stars = first.len() - first.trim_start_matches('*').len();
    stars > 0
        && (first.len() == stars || first[stars..].starts_with([' ', '\t']))
        && lines.next().is_none()
}

/// Delete the pages of the vault that are blank (see `is_blank`) and/or hold only
/// their title heading, depending on the flags. Returns the deleted paths.
pub fn delete_blank_pages(vault_path: &str, empty: bool, title_only: bool) -> Vec<String> {
    if !empty && !title_only {
        return vec![];
    }
    let Ok(files) = list_org_files(vault_path) else { return vec![] };
    files.into_iter()
        .filter(|f| {
            let Ok(content) = std::fs::read_to_string(&f.path) else { return false };
            (empty && is_blank(&content)) || (title_only && is_title_only(&content))
        })
        .filter(|f| std::fs::remove_file(&f.path).is_ok())
        .map(|f| f.path)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_pages() {
        assert!(is_blank(""));
        assert!(is_blank("  \t\n\n"));
        assert!(is_blank("*\n** \n\t*"));
        assert!(!is_blank("* a"));
        assert!(!is_blank("  x  "));
    }

    #[test]
    fn title_only_pages() {
        assert!(is_title_only("* Ma page\n"));
        assert!(is_title_only("\n* Ma page\n\n  \n"));
        assert!(is_title_only("*"));
        assert!(!is_title_only(""));
        assert!(!is_title_only("   \n"));
        assert!(!is_title_only("* Ma page\ndu texte\n"));
        assert!(!is_title_only("* Ma page\n** Sous-titre\n"));
        assert!(!is_title_only("du texte\n"));
        assert!(!is_title_only("*gras* du texte\n"));
    }

    #[test]
    fn snapshot_diff() {
        let t = Some(SystemTime::UNIX_EPOCH);
        let old: Snapshot = [("a".to_string(), (t, 1)), ("b".to_string(), (t, 2)), ("c".to_string(), (t, 3))].into();
        let new: Snapshot = [("a".to_string(), (t, 1)), ("b".to_string(), (t, 5)), ("d".to_string(), (t, 4))].into();
        let (mut changed, removed) = diff(&old, &new);
        changed.sort();
        assert_eq!(changed, vec!["b", "d"]);
        assert_eq!(removed, vec!["c"]);
        assert_eq!(diff(&new, &new), (vec![], vec![]));
    }

    #[test]
    fn deletes_only_matching_pages() {
        let dir = std::env::temp_dir().join(format!("orgamnesia-blank-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (name, content) in [("vide", ""), ("espaces", " \t\n*\n"), ("titre", "* titre\n"), ("texte", "* texte\nbla\n")] {
            std::fs::write(dir.join(format!("{name}.org")), content).unwrap();
        }
        let vp = dir.to_str().unwrap();
        assert_eq!(delete_blank_pages(vp, false, false).len(), 0);
        assert_eq!(delete_blank_pages(vp, true, false).len(), 2);
        assert!(!dir.join("vide.org").exists() && !dir.join("espaces.org").exists() && dir.join("titre.org").exists());
        assert_eq!(delete_blank_pages(vp, false, true).len(), 1);
        assert!(!dir.join("titre.org").exists() && dir.join("texte.org").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
