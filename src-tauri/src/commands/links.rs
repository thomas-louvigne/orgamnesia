//! Backlinks, broken links, tags and TODO headlines of the project.

use orgamnesia_core::{BrokenLink, TagCount, TagHit, TodoHit};
use tauri::State;

use super::Cmd;
use crate::AppState;

#[tauri::command]
pub async fn get_backlinks(state: State<'_, AppState>, page_name: String) -> Cmd<Vec<String>> {
    let ignore_case = state.prefs().case_insensitive_links;
    state.with_project(|p| Ok(p.backlinks(&page_name, ignore_case)))
}

#[tauri::command]
pub async fn get_broken_links(state: State<'_, AppState>) -> Cmd<Vec<BrokenLink>> {
    let ignore_case = state.prefs().case_insensitive_links;
    state.with_project(|p| Ok(p.broken_links(ignore_case)))
}

#[tauri::command]
pub async fn list_tags(state: State<'_, AppState>) -> Cmd<Vec<TagCount>> {
    let org = state.prefs().tags().org;
    state.with_project(|p| Ok(p.tags(org)))
}

#[tauri::command]
pub async fn search_tags(state: State<'_, AppState>, query: String) -> Cmd<Vec<TagHit>> {
    let org = state.prefs().tags().org;
    state.with_project(|p| Ok(p.search_tags(&query, org)))
}

/// Headlines of the project with a TODO keyword; none when TODO keywords are off.
#[tauri::command]
pub async fn list_todos(state: State<'_, AppState>) -> Cmd<Vec<TodoHit>> {
    let Some(keywords) = state.prefs().todo() else { return Ok(vec![]) };
    state.with_project(|p| Ok(p.todos(&keywords)))
}
