use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use crate::error::AppError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
}

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
    let mut files: Vec<FileEntry> = std::fs::read_dir(&pages)
        .map_err(|e| AppError::Io(e.to_string()))?
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
        std::fs::create_dir_all(base.join(dir))
            .map_err(|e| AppError::Io(e.to_string()))?;
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
