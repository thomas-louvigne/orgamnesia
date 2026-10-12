//! Org-bloom extension: building the site of the project.

use orgamnesia_core::SiteConfig;
use tauri::State;

use super::Cmd;
use crate::{error::AppError, AppState};

/// Build the site of the open project with org-bloom, away from the async
/// runtime (it takes a while); returns the folder the site was written to.
#[tauri::command]
pub async fn build_site(state: State<'_, AppState>) -> Cmd<String> {
    let path = state.with_project(|p| Ok(p.path.clone()))?;
    let prefs = state.prefs();
    let options: Vec<&str> = [(prefs.org_bloom_zip, "--zip"), (prefs.org_bloom_no_external_deps, "--no-external-deps")]
        .into_iter()
        .filter_map(|(on, flag)| on.then_some(flag))
        .collect();
    let command = prefs.org_bloom_path;
    tauri::async_runtime::spawn_blocking(move || crate::org_bloom::build(&path, &command, &options))
        .await
        .map_err(|e| AppError::OrgBloom(e.to_string()))?
}

/// Where org-bloom builds the site of the open project, read from its
/// `org-bloom.toml`; `None` when no project is open.
#[tauri::command]
pub fn site_config(state: State<'_, AppState>) -> Cmd<Option<SiteConfig>> {
    let Ok(path) = state.with_project(|p| Ok(p.path.clone())) else { return Ok(None) };
    Ok(Some(crate::org_bloom::site_config(&path)))
}

/// Show a folder in the file manager.
#[tauri::command]
pub fn open_folder(path: String) -> Cmd<()> {
    crate::org_bloom::open_folder(&path)
}
