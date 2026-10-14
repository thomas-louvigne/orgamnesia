//! Search and replace across the pages of the project.

use std::collections::HashMap;

use orgamnesia_core::{ReplaceOutcome, ReplaceSkip, SearchOptions, SearchResults};
use tauri::State;

use super::Cmd;
use crate::AppState;

/// `open`: the text of the pages being edited (path → text), searched instead of the saved one.
#[tauri::command]
pub async fn search_project(
    state: State<'_, AppState>, query: String, options: SearchOptions, open: HashMap<String, String>,
) -> Cmd<SearchResults> {
    state.with_project(|p| Ok(p.search(&query, options, &open)))
}

#[tauri::command]
pub async fn replace_in_project(
    state: State<'_, AppState>, query: String, options: SearchOptions, replacement: String, skip: ReplaceSkip,
) -> Cmd<ReplaceOutcome> {
    let tags = state.prefs().tags();
    state.with_project(|p| p.replace_all(&query, options, &replacement, &skip, tags))
}

#[tauri::command]
pub async fn undo_replace(state: State<'_, AppState>) -> Cmd<ReplaceOutcome> {
    let tags = state.prefs().tags();
    state.with_project(|p| p.undo_replace(tags))
}

#[tauri::command]
pub async fn can_undo_replace(state: State<'_, AppState>) -> Cmd<bool> {
    state.with_project(|p| Ok(p.can_undo_replace()))
}
