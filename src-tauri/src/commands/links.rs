//! Backlinks, broken links, tags and TODO headlines of the project.

use orgamnesia_core::{AgendaHit, BrokenLink, Graph, MentionHit, TagCount, TagHit, TodoHit};
use tauri::State;

use super::Cmd;
use crate::AppState;

#[tauri::command]
/// Pages linking to the page `path`, by the names links give them.
pub async fn get_backlinks(state: State<'_, AppState>, path: String) -> Cmd<Vec<String>> {
    let ignore_case = state.prefs().case_insensitive_links;
    state.with_project(|p| Ok(p.backlinks(&path, ignore_case)))
}

/// The places other pages name the page `path` without a link.
#[tauri::command]
pub async fn get_unlinked_mentions(state: State<'_, AppState>, path: String) -> Cmd<Vec<MentionHit>> {
    let prefs = state.prefs();
    let (tags, ignore_case) = (prefs.tags(), prefs.case_insensitive_links);
    state.with_project(|p| Ok(p.unlinked_mentions(&path, tags, ignore_case)))
}

/// The pages and the links between them, for the graph view.
#[tauri::command]
pub async fn get_graph(state: State<'_, AppState>) -> Cmd<Graph> {
    let ignore_case = state.prefs().case_insensitive_links;
    let mut graph = state.with_project(|p| Ok(p.graph(ignore_case)))?;
    // Placed once the project is let go, on a thread of its own
    let (n, links) = (graph.pages.len(), graph.links.clone());
    graph.pos = tauri::async_runtime::spawn_blocking(move || crate::project::graph::layout(n, &links))
        .await.map_err(|e| crate::error::AppError::Io(std::io::Error::other(e.to_string())))?;
    Ok(graph)
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

/// Headlines of the project with a TODO keyword, those of the tag search `query`
/// only when it names a tag; none when TODO keywords are off.
#[tauri::command]
pub async fn list_todos(state: State<'_, AppState>, query: Option<String>) -> Cmd<Vec<TodoHit>> {
    let Some(keywords) = state.prefs().todo() else { return Ok(vec![]) };
    let org = state.prefs().tags().org;
    state.with_project(|p| Ok(p.todos(&keywords, query.as_deref().unwrap_or(""), org)))
}

/// The agenda of the `days` days from the day `from` (from 1970-01-01).
#[tauri::command]
pub async fn get_agenda(state: State<'_, AppState>, from: i64, days: i64) -> Cmd<Vec<AgendaHit>> {
    let keywords = state.prefs().todo();
    let now = chrono::Local::now().date_naive();
    let today = { use chrono::Datelike; orgamnesia_core::agenda::day_of(now.year(), now.month(), now.day()) };
    state.with_project(|p| Ok(p.agenda(keywords.as_ref(), from, days.clamp(1, 366), today)))
}
