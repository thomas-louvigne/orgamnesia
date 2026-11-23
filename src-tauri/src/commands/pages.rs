//! Reading, writing, creating, renaming and deleting pages; the trash.

use orgamnesia_core::{FileEntry, Format};
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
pub async fn create_page(state: State<'_, AppState>, page_name: String, format: Option<Format>) -> Cmd<FileEntry> {
    let tags = state.prefs().tags();
    state.with_project(|p| p.create(&page_name, format, tags))
}

/// The pages of the open project, as the backend knows them.
#[tauri::command]
pub async fn list_pages(state: State<'_, AppState>) -> Cmd<Vec<FileEntry>> {
    state.with_project(|p| Ok(p.files()))
}

/// The folders of the pages, by their path from the pages folder (`Jeux/Quêtes`).
#[tauri::command]
pub async fn list_folders(state: State<'_, AppState>) -> Cmd<Vec<String>> {
    state.with_project(|p| Ok(p.folders()))
}

#[tauri::command]
pub async fn create_folder(state: State<'_, AppState>, name: String) -> Cmd<String> {
    state.with_project(|p| p.create_folder(&name))
}

/// Rename or move a folder of the pages; the links to its pages follow (settings).
#[tauri::command]
pub async fn rename_folder(state: State<'_, AppState>, old: String, new: String) -> Cmd<String> {
    let prefs = state.prefs();
    state.with_project(|p| p.rename_folder(&old, &new, &prefs))
}

/// Delete a folder of the pages: its pages go to the trash. Their paths there.
#[tauri::command]
pub async fn delete_folder(state: State<'_, AppState>, name: String) -> Cmd<Vec<String>> {
    state.with_project(|p| p.delete_folder(&name))
}

/// The page templates of the open project (`templates/`).
#[tauri::command]
pub async fn list_templates(state: State<'_, AppState>) -> Cmd<Vec<FileEntry>> {
    state.with_project(|p| Ok(p.templates()))
}

/// Create the template `name` in `format`, holding `text`.
#[tauri::command]
pub async fn create_template(state: State<'_, AppState>, name: String, format: Format, text: String) -> Cmd<FileEntry> {
    state.with_project(|p| p.create_template(&name, format, &text))
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
