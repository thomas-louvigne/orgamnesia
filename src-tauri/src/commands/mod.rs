//! The commands the interface calls. Each one only hands the request to the
//! project (`project.rs`), the settings or a tool, and returns its result.

pub mod app;
pub mod clipboard;
pub mod config;
pub mod git;
pub mod links;
pub mod org_bloom;
pub mod pages;
pub mod projects;
pub mod search;
pub mod spell;

use tauri::{AppHandle, Manager};

use crate::{
    error::AppError,
    settings::{self, Settings},
    AppState,
};

type Cmd<T> = Result<T, AppError>;

fn config_dir(app: &AppHandle) -> Cmd<std::path::PathBuf> {
    app.path().app_config_dir().map_err(|e| AppError::Settings(e.to_string()))
}

/// Write what the app keeps for itself of `settings` (the open project, the
/// projects) to `state.json`, and keep `settings` as the ones in effect.
fn store_state(state: &AppState, app: &AppHandle, settings: Settings) -> Cmd<()> {
    settings::save_state(&config_dir(app)?, &settings)?;
    *crate::lock(&state.settings) = settings;
    Ok(())
}
