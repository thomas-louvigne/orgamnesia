//! New versions of the app: looking for one, and installing it.

use orgamnesia_core::UpdateInfo;
use tauri::{AppHandle, State};
use tauri_plugin_updater::UpdaterExt;

use super::Cmd;
use crate::{error::AppError, AppState};

fn failed(e: impl std::fmt::Display) -> AppError {
    AppError::Update(e.to_string())
}

/// A newer version than the one running, with what changed since; `None` when
/// this one is the latest. The update found is kept for `install_update`.
#[tauri::command]
pub async fn check_update(app: AppHandle, state: State<'_, AppState>) -> Cmd<Option<UpdateInfo>> {
    let found = app.updater().map_err(failed)?.check().await.map_err(failed)?;
    let info = found.as_ref().map(|u| UpdateInfo {
        version: u.version.clone(),
        current: u.current_version.clone(),
        changes: crate::update::notes_between(&u.raw_json["changelog"], &u.current_version, &u.version),
        can_install: crate::update::can_install(),
    });
    *crate::lock(&state.update) = found;
    Ok(info)
}

/// Download and install the version `check_update` found, then start the app
/// again on it (what is kept on quitting is saved first).
#[tauri::command]
pub async fn install_update(app: AppHandle, state: State<'_, AppState>) -> Cmd<()> {
    let update = crate::lock(&state.update).take().ok_or_else(|| failed("no update to install"))?;
    update.download_and_install(|_, _| {}, || {}).await.map_err(failed)?;
    super::app::on_exit(&app);
    app.restart()
}
