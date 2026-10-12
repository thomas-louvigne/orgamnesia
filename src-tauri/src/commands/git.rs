//! Git extension: state, commit, pull and push of the project.

use orgamnesia_core::{GitChange, GitStatus};
use tauri::State;

use super::Cmd;
use crate::{error::AppError, AppState};

/// Git state of a project folder (git extension); `None` when git can't be run.
#[tauri::command]
pub async fn git_status(path: String) -> Cmd<Option<GitStatus>> {
    Ok(crate::git::status(&path))
}

/// Run a git command on the open project, away from the async runtime (it may
/// wait for the network).
async fn with_git<T: Send + 'static>(
    state: &State<'_, AppState>,
    f: impl FnOnce(&str, &str) -> Result<T, AppError> + Send + 'static,
) -> Cmd<T> {
    let path = state.with_project(|p| Ok(p.path.clone()))?;
    let key = state.prefs().git_ssh_key;
    tauri::async_runtime::spawn_blocking(move || f(&path, &key))
        .await
        .map_err(|e| AppError::Git(e.to_string()))?
}

/// Files of the open project changed since the last commit.
#[tauri::command]
pub async fn git_changes(state: State<'_, AppState>) -> Cmd<Vec<GitChange>> {
    with_git(&state, |path, _| crate::git::changes(path)).await
}

/// Commit every change of the open project with `message`.
#[tauri::command]
pub async fn git_commit(state: State<'_, AppState>, message: String) -> Cmd<String> {
    with_git(&state, move |path, _| crate::git::commit(path, &message)).await
}

#[tauri::command]
pub async fn git_pull(state: State<'_, AppState>) -> Cmd<String> {
    with_git(&state, crate::git::pull).await
}

#[tauri::command]
pub async fn git_push(state: State<'_, AppState>) -> Cmd<String> {
    with_git(&state, crate::git::push).await
}
