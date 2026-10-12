//! Reading, writing, creating, renaming and deleting pages; the trash.

use orgamnesia_core::FileEntry;
use tauri::State;

use super::Cmd;
use crate::AppState;

#[tauri::command]
pub async fn read_file(state: State<'_, AppState>, path: String) -> Cmd<String> {
    state.with_project(|p| p.read(&path))
}

#[tauri::command]
pub async fn write_file(state: State<'_, AppState>, path: String, content: String) -> Cmd<()> {
    let tags = state.prefs().tags();
    state.with_project(|p| p.write(&path, content, tags))
}

#[tauri::command]
pub async fn create_page(state: State<'_, AppState>, page_name: String) -> Cmd<FileEntry> {
    let tags = state.prefs().tags();
    state.with_project(|p| p.create(&page_name, tags))
}

/// Delete a page of the open project: it goes to the project's trash. Returns its path there.
#[tauri::command]
pub async fn delete_page(state: State<'_, AppState>, path: String) -> Cmd<String> {
    state.with_project(|p| p.delete(&path))
}

/// Number of pages in the project's trash.
#[tauri::command]
pub async fn trash_count(state: State<'_, AppState>) -> Cmd<usize> {
    state.with_project(|p| p.trash_count())
}

/// Delete for good every page in the project's trash; returns how many.
#[tauri::command]
pub async fn empty_trash(state: State<'_, AppState>) -> Cmd<usize> {
    state.with_project(|p| p.empty_trash())
}

/// Bring back a deleted page from the project's trash.
#[tauri::command]
pub async fn restore_page(state: State<'_, AppState>, trashed: String) -> Cmd<FileEntry> {
    let tags = state.prefs().tags();
    state.with_project(|p| p.restore(&trashed, tags))
}

#[tauri::command]
pub async fn rename_page(state: State<'_, AppState>, old_path: String, new_name: String) -> Cmd<FileEntry> {
    let prefs = state.prefs();
    state.with_project(|p| p.rename(&old_path, &new_name, &prefs))
}
