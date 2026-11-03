//! The files of a project on disk: its pages (in `pages/` when there is one),
//! their state as last seen (to notice outside changes), and its folders.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use crate::error::AppError;

pub use orgamnesia_core::FileEntry;
use orgamnesia_core::{Format, Formats};

/// Directory holding the pages: `<vault>/pages/` when it exists, else the folder itself.
pub fn pages_dir(vault_path: &str) -> PathBuf {
    let pages = Path::new(vault_path).join("pages");
    if pages.is_dir() { pages } else { PathBuf::from(vault_path) }
}

/// Whether the file `path` is a page of either format (`.org`, `.md`).
fn is_page_file(path: &Path) -> bool {
    page_format(path).is_some()
}

/// The format of the page `path`; `None` for any other file, and for hidden
/// ones (Emacs' locks `.#Note.org`, macOS' `._Note.md`, the app's temporary files).
fn page_format(path: &Path) -> Option<Format> {
    let hidden = path.file_name().is_none_or(|n| n.to_string_lossy().starts_with('.'));
    if hidden { return None; }
    path.extension().and_then(|s| s.to_str()).and_then(Format::from_ext)
}

fn has_org_files(dir: &Path) -> bool {
    std::fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok()).any(|e| is_page_file(&e.path())))
        .unwrap_or(false)
}

/// List the pages of the vault (see `pages_dir`): its `.org` and `.md` files,
/// those `formats` accepts, sorted by name.
pub fn list_pages(vault_path: &str, formats: Formats) -> Result<Vec<FileEntry>, AppError> {
    let pages = pages_dir(vault_path);
    if !pages.exists() {
        return Err(AppError::VaultNotFound(vault_path.to_string()));
    }
    let mut files: Vec<FileEntry> = std::fs::read_dir(&pages)?
        .filter_map(|entry| entry.ok())
        .filter(|e| page_format(&e.path()).is_some_and(|f| formats.accepts(f)))
        .map(|e| {
            let path = e.path();
            let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
            FileEntry { name, path: crate::slash(&path) }
        })
        .collect();
    files.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.path.cmp(&b.path)));
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
    if !base.exists() {
        return Err(AppError::VaultNotFound(vault_path.to_string()));
    }
    if !base.is_dir() {
        return Err(AppError::NotAFolder(vault_path.to_string()));
    }
    if !base.join("pages").is_dir() && has_org_files(base) {
        return Ok(());
    }
    for dir in ["pages", "assets"] {
        std::fs::create_dir_all(base.join(dir))?;
    }
    Ok(())
}

/// Whether the project `vault_path` is a plain folder of `.org` files (used as
/// is) without an `assets/` folder for the images of its pages.
pub fn lacks_assets(vault_path: &str) -> bool {
    let base = Path::new(vault_path);
    !base.join("pages").is_dir() && has_org_files(base) && !base.join("assets").exists()
}

/// The project `vault_path` lacks its `assets/` folder and is called `pages`,
/// next to an `assets/` folder: the `pages/` of a project (Logseq's layout)
/// opened instead of the project itself. That project's folder.
pub fn parent_with_assets(vault_path: &str) -> Option<PathBuf> {
    let base = Path::new(vault_path);
    let parent = base.parent()?;
    let named_pages = base.file_name().is_some_and(|n| n == "pages");
    (named_pages && lacks_assets(vault_path) && parent.join("assets").is_dir()).then(|| parent.to_path_buf())
}

/// File stem of a page path (the page name).
pub fn page_name(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string()
}

/// Full path for a new page of `format` in the vault.
pub fn page_path(vault_path: &str, name: &str, format: Format) -> PathBuf {
    pages_dir(vault_path).join(format!("{name}.{}", format.ext()))
}

/// True when a page holds no text: only spaces, tabs, line breaks, `*` or `#`.
pub fn is_blank(content: &str) -> bool {
    content.chars().all(|c| c.is_whitespace() || c == '*' || c == '#')
}

/// True when a page holds nothing but its title heading (`* Name` or `# Name`,
/// as a new page starts).
pub fn is_title_only(content: &str) -> bool {
    let mut lines = content.lines().filter(|l| !l.trim().is_empty());
    let Some(first) = lines.next() else { return false };
    let first = first.trim_start();
    let mark = if first.starts_with('#') { '#' } else { '*' };
    let stars = first.len() - first.trim_start_matches(mark).len();
    stars > 0
        && (first.len() == stars || first[stars..].starts_with([' ', '\t']))
        && lines.next().is_none()
}

/// The trash folder of the project `vault_path`, created if needed, with a
/// `.gitignore` keeping it out of git.
pub fn trash_dir(vault_path: &str) -> std::io::Result<PathBuf> {
    let dir = Path::new(vault_path).join(".trash");
    std::fs::create_dir_all(&dir)?;
    let ignore = dir.join(".gitignore");
    if !ignore.exists() {
        std::fs::write(ignore, "*\n")?;
    }
    Ok(dir)
}

/// Move the page `path` to the trash of `vault_path`, named `<time>-<page>.org`
/// (or `.md`); its path there.
pub fn to_trash(vault_path: &str, path: &str) -> std::io::Result<PathBuf> {
    let stamp = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_millis());
    let to = trash_dir(vault_path)?.join(format!("{stamp}-{}", orgamnesia_core::pages::file_name(path)));
    std::fs::rename(path, &to)?;
    Ok(to)
}

/// Move to the trash the pages of the vault that are blank (see `is_blank`)
/// and/or hold only their title heading, depending on the flags. Returns their
/// paths before.
pub fn delete_blank_pages(vault_path: &str, formats: Formats, empty: bool, title_only: bool) -> Vec<String> {
    if !empty && !title_only {
        return vec![];
    }
    let Ok(files) = list_pages(vault_path, formats) else { return vec![] };
    files.into_iter()
        .filter(|f| {
            let Ok(content) = std::fs::read_to_string(&f.path) else { return false };
            (empty && is_blank(&content)) || (title_only && is_title_only(&content))
        })
        .filter(|f| to_trash(vault_path, &f.path).is_ok())
        .map(|f| f.path)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_of_pages_may_lack_its_assets() {
        let dir = std::env::temp_dir().join(format!("orgamnesia-vault-assets-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.to_string_lossy().to_string();
        // Empty: it gets both folders
        assert!(!lacks_assets(&path));
        std::fs::write(dir.join("note.org"), "* Note\n").unwrap();
        assert!(lacks_assets(&path));
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        assert!(!lacks_assets(&path));
        assert_eq!(parent_with_assets(&path), None);
        // The `pages/` of a project with its `assets/` beside
        let pages = dir.join("projet/pages");
        std::fs::create_dir_all(&pages).unwrap();
        std::fs::write(pages.join("note.org"), "* Note\n").unwrap();
        let pages = pages.to_string_lossy().to_string();
        assert_eq!(parent_with_assets(&pages), None);
        std::fs::create_dir_all(dir.join("projet/assets")).unwrap();
        assert_eq!(parent_with_assets(&pages), Some(dir.join("projet")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hidden_files_are_no_pages() {
        let dir = std::env::temp_dir().join(format!("orgamnesia-hidden-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["Note.org", ".#Note.org", "._Note.md", ".Note.org.123.tmp"] {
            std::fs::write(dir.join(name), "").unwrap();
        }
        let names: Vec<String> = list_pages(dir.to_str().unwrap(), Formats::default()).unwrap().into_iter().map(|f| f.name).collect();
        assert_eq!(names, ["Note"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

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
        assert!(is_title_only("# Ma page\n"));
        assert!(!is_title_only("#tag du texte\n"));
        assert!(!is_title_only("# Ma page\n## Sous-titre\n"));
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
        assert_eq!(delete_blank_pages(vp, Formats::default(), false, false).len(), 0);
        assert_eq!(delete_blank_pages(vp, Formats::default(), true, false).len(), 2);
        assert!(!dir.join("vide.org").exists() && !dir.join("espaces.org").exists() && dir.join("titre.org").exists());
        assert_eq!(delete_blank_pages(vp, Formats::default(), false, true).len(), 1);
        assert!(!dir.join("titre.org").exists() && dir.join("texte.org").exists());
        // In the trash, not gone for good
        assert_eq!(std::fs::read_dir(dir.join(".trash")).unwrap().flatten().filter(|e| e.file_name() != ".gitignore").count(), 3);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
