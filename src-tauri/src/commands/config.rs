//! Settings and keybindings; their files edited as text (the "preference" project).

use orgamnesia_core::FileEntry;
use serde::Serialize;
use tauri::{AppHandle, State};

use super::{config_dir, store_state, Cmd};
use crate::{
    config_files::{self, check, Content, File, InEffect, Invalid},
    error::AppError,
    keybindings,
    settings::{self, Session, Settings},
    AppState,
};

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Cmd<Settings> {
    Ok(state.settings())
}

#[tauri::command]
pub async fn set_settings(state: State<'_, AppState>, app: AppHandle, mut settings: Settings) -> Cmd<()> {
    // The settings window doesn't know the session, nor the keybinding profile
    // (saved with the keybindings): keep them
    let current = state.settings();
    settings.session = current.session;
    settings.config.keybinding_profile = current.config.keybinding_profile;
    crate::lock(&state.config).write(&config_dir(&app)?, &File::Settings, &settings::config_text(&settings))?;
    config_files::put_in_effect(&state, &Content::Settings(settings.config.clone()));
    store_state(&state, &app, settings)
}

/// The pages and panes open now, written to `state.json` on quit.
/// Not async: calls run in order, so the last state sent is the one kept.
#[tauri::command]
pub fn set_session(state: State<'_, AppState>, session: Option<Session>) {
    crate::lock(&state.settings).session = session;
}

#[tauri::command]
pub async fn get_keybindings(state: State<'_, AppState>) -> Cmd<keybindings::Keybindings> {
    Ok(config_files::keybindings(&state))
}

/// Write the profiles of `keybindings`, and the one in effect to the settings;
/// returns the keybindings as they now are (the profiles named after their files).
#[tauri::command]
pub async fn set_keybindings(state: State<'_, AppState>, app: AppHandle, keybindings: keybindings::Keybindings) -> Cmd<keybindings::Keybindings> {
    let dir = config_dir(&app)?;
    let active = keybindings::save(&dir, &mut crate::lock(&state.config), &keybindings)?;
    let mut settings = state.settings();
    if settings.config.keybinding_profile.as_deref() != Some(active.as_str()) {
        settings.config.keybinding_profile = Some(active);
        crate::lock(&state.config).write(&dir, &File::Settings, &settings::config_text(&settings))?;
        *crate::lock(&state.settings) = settings;
    }
    Ok(config_files::keybindings(&state))
}

/// The configuration files, as the pages of the "preference" project: the
/// settings, then the user's own keybinding profiles.
#[tauri::command]
pub async fn config_files(app: AppHandle) -> Cmd<Vec<FileEntry>> {
    let dir = config_dir(&app)?;
    let files = File::FIXED.into_iter().chain(config_files::profiles_on_disk(&dir));
    Ok(files.map(|f| FileEntry {
        name: f.name(),
        path: config_files::path(&dir, &f).to_string_lossy().to_string(),
    }).collect())
}

/// The text of `file` on disk; when there is none yet, the one of the configuration in effect.
#[tauri::command]
pub async fn read_config(state: State<'_, AppState>, app: AppHandle, file: File) -> Cmd<String> {
    let dir = config_dir(&app)?;
    match std::fs::read_to_string(config_files::path(&dir, &file)) {
        Ok(text) => Ok(text),
        Err(_) => read_default(&state, &file),
    }
}

/// Write `content` as `file` and put it in effect, when it is valid; returns the
/// configuration now in effect.
#[tauri::command]
pub async fn apply_config(state: State<'_, AppState>, app: AppHandle, file: File, content: String) -> Cmd<InEffect> {
    apply(&state, &app, &file, &content)
}

fn apply(state: &AppState, app: &AppHandle, file: &File, content: &str) -> Cmd<InEffect> {
    let mut config = crate::lock(&state.config);
    let checked = check(file, content, &config.profile_names())
        .map_err(|p| AppError::ConfigInvalid(format!("{}:{} {:?}", p.line, p.column, p.kind)))?;
    config.write(&config_dir(app)?, file, content)?;
    drop(config);
    config_files::put_in_effect(state, &checked.content);
    Ok(config_files::in_effect(state))
}

/// The versions kept of `file`, newest first: when each was replaced (ms since 1970).
#[tauri::command]
pub async fn config_versions(app: AppHandle, file: File) -> Cmd<Vec<u64>> {
    Ok(config_files::versions(&config_dir(&app)?, &file))
}

/// A configuration put back in effect: the text of its file, and the configuration now in effect.
#[derive(Serialize)]
pub struct Restored {
    content: String,
    effect: InEffect,
}

/// Put back in effect the version of `file` replaced at `version`, or with
/// `None` the last valid one (when the file was spoilt outside the app).
#[tauri::command]
pub async fn restore_config(state: State<'_, AppState>, app: AppHandle, file: File, version: Option<u64>) -> Cmd<Restored> {
    let dir = config_dir(&app)?;
    let content = match version {
        Some(v) => config_files::version(&dir, &file, v)?,
        None => {
            let last = crate::lock(&state.config).last_valid(&dir, &file);
            match last {
                Some(text) => text,
                None => read_default(&state, &file)?,
            }
        }
    };
    let effect = apply(&state, &app, &file, &content)?;
    Ok(Restored { content, effect })
}

/// The text of the configuration in effect, when no valid text of `file` is known.
fn read_default(state: &AppState, file: &File) -> Cmd<String> {
    match file {
        File::Settings => Ok(settings::config_text(&state.settings())),
        File::Profile(_) => Err(AppError::NotAPage(file.name())),
    }
}

/// The configuration files found wrong as the app started, and why.
#[tauri::command]
pub async fn config_problems(state: State<'_, AppState>) -> Cmd<Vec<Invalid>> {
    Ok(crate::lock(&state.config).problems())
}
