//! Opening a project, and forgetting one.

use orgamnesia_core::{FileEntry, VaultChanges};
use tauri::{AppHandle, Manager, State};

use std::path::Path;

use serde::Serialize;

use super::{config_dir, store_state, Cmd};
use crate::{project::Project, watch, AppState};

/// Open the folder `path` as the project; returns its pages.
#[tauri::command]
pub async fn open_vault(state: State<'_, AppState>, app: AppHandle, path: String) -> Cmd<Vec<FileEntry>> {
    let files = open(&state, &config_dir(&app)?, &path)?;
    // The images of its pages are shown from the disk
    let _ = app.asset_protocol_scope().allow_directory(&path, true);
    // Without watching, outside changes are only seen when the project is reopened
    *crate::lock(&state.watcher) = watch::watch(app.clone(), &path).ok();
    Ok(files)
}

/// Open the folder `path` as the project, remembered in `dir`; returns its pages.
fn open(state: &AppState, dir: &Path, path: &str) -> Cmd<Vec<FileEntry>> {
    let project = Project::open(path, state.prefs().tags())?;
    let files = project.files();
    let prefs = state.prefs();
    if prefs.trash_auto_empty {
        let keep = (prefs.trash_keep_days > 0).then_some(prefs.trash_keep_days);
        let _ = project.prune_trash(keep, prefs.trash_max_bytes);
    }

    // Remember it as the open project, first of the list
    let mut s = state.settings();
    s.vault_path = Some(path.to_string());
    s.projects.retain(|p| p != path);
    s.projects.insert(0, path.to_string());
    store_state(state, dir, s)?;
    *crate::lock(&state.project) = Some(project);
    Ok(files)
}

/// The pages changed on disk by another program since the last look, if any:
/// asked every second, in case the watcher misses a change (some sync tools,
/// network folders). What it reports is not reported again by the watcher.
#[tauri::command]
pub async fn check_disk(state: State<'_, AppState>) -> Cmd<Option<VaultChanges>> {
    let tags = state.prefs().tags();
    state.with_project(|p| p.refresh(tags))
}

/// Forget a project (the folder itself is left untouched); returns the remaining list.
#[tauri::command]
pub async fn remove_project(state: State<'_, AppState>, app: AppHandle, path: String) -> Cmd<Vec<String>> {
    forget(&state, &config_dir(&app)?, &path)
}

fn forget(state: &AppState, dir: &Path, path: &str) -> Cmd<Vec<String>> {
    let mut s = state.settings();
    s.projects.retain(|p| p != path);
    let projects = s.projects.clone();
    store_state(state, dir, s)?;
    Ok(projects)
}

/// An `assets/` folder to offer the project: made in it, or that of the folder
/// around it (see `vault::parent_with_assets`), opened as the project instead.
#[derive(Debug, PartialEq, Serialize)]
pub struct AssetsOffer {
    /// The folder around the project, whose `assets/` may be used.
    parent: Option<String>,
}

/// What `assets/` folder to offer the project `path`: none when it has one, or
/// when it is not a plain folder of `.org` files, or when the user didn't want one there.
#[tauri::command]
pub async fn assets_offer(state: State<'_, AppState>, path: String) -> Cmd<Option<AssetsOffer>> {
    if !crate::vault::lacks_assets(&path) || state.settings().assets_declined.contains(&path) {
        return Ok(None);
    }
    let parent = crate::vault::parent_with_assets(&path).map(|p| p.to_string_lossy().to_string());
    Ok(Some(AssetsOffer { parent }))
}

/// Make the `assets/` folder of the project `path` (`create`), or remember that
/// the user doesn't want one there.
#[tauri::command]
pub async fn answer_assets(state: State<'_, AppState>, app: AppHandle, path: String, create: bool) -> Cmd<()> {
    answer(&state, &config_dir(&app)?, &path, create)
}

fn answer(state: &AppState, dir: &Path, path: &str, create: bool) -> Cmd<()> {
    if create {
        std::fs::create_dir_all(Path::new(path).join("assets"))?;
        return Ok(());
    }
    let mut s = state.settings();
    if !s.assets_declined.iter().any(|p| p == path) { s.assets_declined.push(path.to_string()); }
    store_state(state, dir, s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::testing::{block_on, folder, mock_app, open_project};
    use tauri::Manager;

    #[test]
    fn projects_are_opened_and_forgotten() {
        let config = folder("cmd-projects-config");
        let state = AppState::new();
        let a = open_project(&state, "cmd-projects-a", &[("Une", "* Une\n")]);
        let b = open_project(&state, "cmd-projects-b", &[]);
        // The trash emptied on opening, as set
        crate::lock(&state.settings).config.trash_auto_empty = Some(true);
        let files = open(&state, &config, &a).unwrap();
        assert_eq!(files.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), vec!["Une"]);
        open(&state, &config, &b).unwrap();
        // The last opened first, and the open one
        assert_eq!(state.settings().projects, vec![b.clone(), a.clone()]);
        assert_eq!(state.settings().vault_path.as_deref(), Some(b.as_str()));
        assert!(std::fs::read_to_string(config.join("state.json")).unwrap().contains(&a));
        assert_eq!(forget(&state, &config, &a).unwrap(), vec![b.clone()]);
        assert!(open(&state, &config, "/no/such/folder").is_err());
    }

    #[test]
    fn assets_are_offered_once() {
        let app = mock_app();
        let config = folder("cmd-assets-config");
        let project = folder("cmd-assets");
        std::fs::write(project.join("note.org"), "* Note\n").unwrap();
        let path = project.to_string_lossy().to_string();
        let offer = |path: &str| block_on(assets_offer(app.state::<AppState>(), path.to_string())).unwrap();
        assert_eq!(offer(&path), Some(AssetsOffer { parent: None }));
        // Not wanted: not offered again
        answer(&app.state::<AppState>(), &config, &path, false).unwrap();
        assert_eq!(offer(&path), None);
        assert!(std::fs::read_to_string(config.join("state.json")).unwrap().contains("assets_declined"));
        answer(&app.state::<AppState>(), &config, &path, true).unwrap();
        assert!(project.join("assets").is_dir());
        // The `pages/` of a project with its `assets/`: that one
        let pages = project.join("pages");
        std::fs::create_dir_all(&pages).unwrap();
        std::fs::write(pages.join("note.org"), "* Note\n").unwrap();
        assert_eq!(offer(&pages.to_string_lossy()), Some(AssetsOffer { parent: Some(path.clone()) }));
    }

    #[test]
    fn changes_on_disk_are_seen() {
        let app = mock_app();
        let path = open_project(&app.state::<AppState>(), "cmd-check-disk", &[("Une", "* Une\n")]);
        assert_eq!(block_on(check_disk(app.state::<AppState>())).unwrap(), None);
        std::fs::write(std::path::Path::new(&path).join("pages/Deux.org"), "* Deux\n").unwrap();
        let changes = block_on(check_disk(app.state::<AppState>())).unwrap().unwrap();
        assert_eq!(changes.files.len(), 2);
    }
}
