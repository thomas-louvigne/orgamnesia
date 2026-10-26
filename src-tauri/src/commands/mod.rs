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
#[cfg(test)]
mod tests;

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
/// projects) to `state.json` in `dir`, and keep `settings` as the ones in effect.
fn store_state(state: &AppState, dir: &std::path::Path, settings: Settings) -> Cmd<()> {
    settings::save_state(dir, &settings)?;
    *crate::lock(&state.settings) = settings;
    Ok(())
}

/// What the tests of the commands share.
#[cfg(test)]
pub(crate) mod testing {
    use std::path::PathBuf;

    use tauri::Manager;

    pub use tauri::async_runtime::block_on;

    use crate::AppState;

    /// A new empty folder, for the test `test`.
    pub fn folder(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("orgamnesia-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// An app of Tauri's mock runtime, with its state: the commands taking only
    /// the state run on it.
    pub fn mock_app() -> tauri::App<tauri::test::MockRuntime> {
        let app = tauri::test::mock_app();
        app.manage(AppState::new());
        app
    }

    /// A project folder holding `pages` (name, text) in `pages/`, opened as the
    /// project of `state`; its path.
    pub fn open_project(state: &AppState, test: &str, pages: &[(&str, &str)]) -> String {
        let dir = folder(test);
        std::fs::create_dir_all(dir.join("pages")).unwrap();
        for (name, text) in pages {
            std::fs::write(dir.join("pages").join(format!("{name}.org")), text).unwrap();
        }
        let path = dir.to_string_lossy().to_string();
        let project = crate::project::Project::open(&path, state.prefs().tags()).unwrap();
        *crate::lock(&state.project) = Some(project);
        path
    }
}

