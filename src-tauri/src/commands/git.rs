//! Git extension: state, commit, pull and push of the project, the history of a
//! page, and setting up a repository.

use orgamnesia_core::{GitChange, GitCommit, GitStatus};
use tauri::State;

use super::Cmd;
use crate::{error::AppError, AppState};

/// Git state of a project folder (git extension); `None` when git can't be run.
#[tauri::command]
pub async fn git_status(path: String) -> Cmd<Option<GitStatus>> {
    Ok(crate::git::status(&path))
}

/// Run a git command on the open project, away from the async runtime (it may
/// wait for the network). With `writes`, one at a time (see `git::WRITING`).
async fn with_git<T: Send + 'static>(
    state: &State<'_, AppState>,
    writes: bool,
    f: impl FnOnce(&str, &str) -> Result<T, AppError> + Send + 'static,
) -> Cmd<T> {
    let path = state.with_project(|p| Ok(p.path.clone()))?;
    let key = state.prefs().git_ssh_key;
    tauri::async_runtime::spawn_blocking(move || {
        let _one = writes.then(|| crate::lock(&crate::git::WRITING));
        f(&path, &key)
    })
    .await
    .map_err(|e| AppError::Git(e.to_string()))?
}

/// Files of the open project changed since the last commit.
#[tauri::command]
pub async fn git_changes(state: State<'_, AppState>) -> Cmd<Vec<GitChange>> {
    with_git(&state, false, |path, _| crate::git::changes(path)).await
}

/// What changed in `file` (relative to the root of the repository) since the last commit.
#[tauri::command]
pub async fn git_diff(state: State<'_, AppState>, file: String, old_path: Option<String>) -> Cmd<String> {
    with_git(&state, false, move |path, _| crate::git::diff(path, &file, old_path.as_deref())).await
}

/// Commit the changes of the open project with `message`; with `files`, only those.
#[tauri::command]
pub async fn git_commit(state: State<'_, AppState>, message: String, files: Option<Vec<String>>) -> Cmd<String> {
    with_git(&state, true, move |path, _| crate::git::commit(path, &message, files.as_deref())).await
}

#[tauri::command]
pub async fn git_fetch(state: State<'_, AppState>) -> Cmd<String> {
    with_git(&state, true, crate::git::fetch).await
}

/// Pull; a diverged history is rebased when the settings say so.
#[tauri::command]
pub async fn git_pull(state: State<'_, AppState>) -> Cmd<String> {
    let rebase = state.prefs().git_pull_rebase;
    with_git(&state, true, move |path, key| crate::git::pull(path, key, rebase)).await
}

#[tauri::command]
pub async fn git_push(state: State<'_, AppState>) -> Cmd<String> {
    with_git(&state, true, crate::git::push).await
}

/// The commits that changed the page `file` (full path), newest first.
#[tauri::command]
pub async fn git_log(state: State<'_, AppState>, file: String) -> Cmd<Vec<GitCommit>> {
    with_git(&state, false, move |path, _| crate::git::log(path, &file)).await
}

/// What `hash` changed in `file` (relative to the root of the repository).
#[tauri::command]
pub async fn git_commit_diff(state: State<'_, AppState>, hash: String, file: String) -> Cmd<String> {
    with_git(&state, false, move |path, _| crate::git::commit_diff(path, &hash, &file)).await
}

/// The text of `file` (relative to the root of the repository) in `hash`.
#[tauri::command]
pub async fn git_show(state: State<'_, AppState>, hash: String, file: String) -> Cmd<String> {
    with_git(&state, false, move |path, _| crate::git::show(path, &hash, &file)).await
}

/// The name and email git signs the commits of the open project with (empty when not set).
#[tauri::command]
pub async fn git_identity(state: State<'_, AppState>) -> Cmd<(String, String)> {
    with_git(&state, false, |path, _| Ok(crate::git::identity(path))).await
}

/// Set the name and email git signs the commits with, for all the user's repositories.
#[tauri::command]
pub async fn git_set_identity(state: State<'_, AppState>, name: String, email: String) -> Cmd<()> {
    with_git(&state, true, move |path, _| crate::git::set_identity(path, &name, &email)).await
}

/// Make the open project a repository.
#[tauri::command]
pub async fn git_init(state: State<'_, AppState>) -> Cmd<String> {
    with_git(&state, true, |path, _| crate::git::init(path)).await
}

/// Give the open project's repository the remote `origin` at `url`.
#[tauri::command]
pub async fn git_add_remote(state: State<'_, AppState>, url: String) -> Cmd<String> {
    with_git(&state, true, move |path, _| crate::git::add_remote(path, &url)).await
}

/// Clone `url` into a new folder of `parent`; returns that folder.
#[tauri::command]
pub async fn git_clone(state: State<'_, AppState>, parent: String, url: String) -> Cmd<String> {
    let key = state.prefs().git_ssh_key;
    tauri::async_runtime::spawn_blocking(move || crate::git::clone(&parent, &url, &key))
        .await
        .map_err(|e| AppError::Git(e.to_string()))?
}
