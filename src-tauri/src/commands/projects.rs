//! Opening a project, and forgetting one.

use orgamnesia_core::FileEntry;
use tauri::{AppHandle, Manager, State};

use super::{store_settings, Cmd};
use crate::{project::Project, watch, AppState};

/// Open the folder `path` as the project; returns its pages.
#[tauri::command]
pub async fn open_vault(state: State<'_, AppState>, app: AppHandle, path: String) -> Cmd<Vec<FileEntry>> {
    let project = Project::open(&path, state.prefs().tags())?;
    let files = project.files();
    let prefs = state.prefs();
    if prefs.trash_auto_empty {
        let keep = (prefs.trash_keep_days > 0).then_some(prefs.trash_keep_days);
        let _ = project.prune_trash(keep, prefs.trash_max_bytes);
    }

    // Remember it as the open project, first of the list
    let mut s = state.settings();
    s.vault_path = Some(path.clone());
    s.projects.retain(|p| p != &path);
    s.projects.insert(0, path.clone());
    store_settings(&state, &app, s)?;

    *crate::lock(&state.project) = Some(project);
    // The images of its pages are shown from the disk
    let _ = app.asset_protocol_scope().allow_directory(&path, true);
    // Without watching, outside changes are only seen when the project is reopened
    *crate::lock(&state.watcher) = watch::watch(app.clone(), &path).ok();
    Ok(files)
}

/// Forget a project (the folder itself is left untouched); returns the remaining list.
#[tauri::command]
pub async fn remove_project(state: State<'_, AppState>, app: AppHandle, path: String) -> Cmd<Vec<String>> {
    let mut s = state.settings();
    s.projects.retain(|p| p != &path);
    let projects = s.projects.clone();
    store_settings(&state, &app, s)?;
    Ok(projects)
}
