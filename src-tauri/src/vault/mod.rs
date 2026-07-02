use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use crate::error::AppError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
}

/// List all `.org` files under `<vault>/pages/`, sorted by name.
pub fn list_org_files(vault_path: &str) -> Result<Vec<FileEntry>, AppError> {
    let pages = Path::new(vault_path).join("pages");
    if !pages.exists() {
        return Err(AppError::VaultNotFound(format!(
            "pages/ not found in {vault_path}"
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

/// Ensure `<vault>/pages/` and `<vault>/assets/` exist.
pub fn ensure_structure(vault_path: &str) -> Result<(), AppError> {
    let base = Path::new(vault_path);
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
    Path::new(vault_path).join("pages").join(format!("{name}.org"))
}
