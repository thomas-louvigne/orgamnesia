//! Settings and keybindings; their files edited as text (the "preference" project).
//! Each command hands its work to a function of the configuration folder `dir`.

use std::path::Path;

use orgamnesia_core::FileEntry;
use serde::Serialize;
use tauri::{AppHandle, State};

use super::{config_dir, store_state, Cmd};
use crate::{
    config_files::{self, check, Content, File, InEffect, Invalid},
    error::AppError,
    keybindings::{self, Keybindings},
    settings::{self, Session, Settings},
    AppState,
};

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Cmd<Settings> {
    Ok(state.settings())
}

#[tauri::command]
pub async fn set_settings(state: State<'_, AppState>, app: AppHandle, settings: Settings) -> Cmd<()> {
    save_settings(&state, &config_dir(&app)?, settings)
}

/// Write `settings`, from the settings window, and put them in effect.
fn save_settings(state: &AppState, dir: &Path, mut settings: Settings) -> Cmd<()> {
    // The settings window doesn't know the session, nor the keybinding profile
    // (saved with the keybindings): keep them
    let current = state.settings();
    settings.session = current.session;
    settings.config.keybinding_profile = current.config.keybinding_profile;
    crate::lock(&state.config).write(dir, &File::Settings, &settings::config_text(&settings))?;
    config_files::put_in_effect(state, &Content::Settings(settings.config.clone()));
    store_state(state, dir, settings)
}

/// The pages and panes open now, written to `state.json` on quit.
/// Not async: calls run in order, so the last state sent is the one kept.
#[tauri::command]
pub fn set_session(state: State<'_, AppState>, session: Option<Session>) {
    crate::lock(&state.settings).session = session;
}

#[tauri::command]
pub async fn get_keybindings(state: State<'_, AppState>) -> Cmd<Keybindings> {
    Ok(config_files::keybindings(&state))
}

/// Write the profiles of `keybindings`, and the one in effect to the settings;
/// returns the keybindings as they now are (the profiles named after their files).
#[tauri::command]
pub async fn set_keybindings(state: State<'_, AppState>, app: AppHandle, keybindings: Keybindings) -> Cmd<Keybindings> {
    save_keybindings(&state, &config_dir(&app)?, &keybindings)
}

fn save_keybindings(state: &AppState, dir: &Path, kb: &Keybindings) -> Cmd<Keybindings> {
    let active = keybindings::save(dir, &mut crate::lock(&state.config), kb)?;
    let mut settings = state.settings();
    if settings.config.keybinding_profile.as_deref() != Some(active.as_str()) {
        settings.config.keybinding_profile = Some(active);
        crate::lock(&state.config).write(dir, &File::Settings, &settings::config_text(&settings))?;
        *crate::lock(&state.settings) = settings;
    }
    Ok(config_files::keybindings(state))
}

/// The configuration files, as the pages of the "preference" project: the
/// settings, then the user's own keybinding profiles.
#[tauri::command]
pub async fn config_files(app: AppHandle) -> Cmd<Vec<FileEntry>> {
    Ok(list_files(&config_dir(&app)?))
}

fn list_files(dir: &Path) -> Vec<FileEntry> {
    File::FIXED.into_iter().chain(config_files::profiles_on_disk(dir)).map(|f| FileEntry {
        name: f.name(),
        path: crate::slash(config_files::path(dir, &f)),
    }).collect()
}

/// The text of `file` on disk; when there is none yet, the one of the configuration in effect.
#[tauri::command]
pub async fn read_config(state: State<'_, AppState>, app: AppHandle, file: File) -> Cmd<String> {
    read(&state, &config_dir(&app)?, &file)
}

fn read(state: &AppState, dir: &Path, file: &File) -> Cmd<String> {
    match std::fs::read_to_string(config_files::path(dir, file)) {
        Ok(text) => Ok(text),
        Err(_) => read_default(state, file),
    }
}

/// Write `content` as `file` and put it in effect, when it is valid; returns the
/// configuration now in effect.
#[tauri::command]
pub async fn apply_config(state: State<'_, AppState>, app: AppHandle, file: File, content: String) -> Cmd<InEffect> {
    apply(&state, &config_dir(&app)?, &file, &content)
}

fn apply(state: &AppState, dir: &Path, file: &File, content: &str) -> Cmd<InEffect> {
    let mut config = crate::lock(&state.config);
    let checked = check(file, content, &config.profile_names())
        .map_err(|p| AppError::ConfigInvalid(format!("{}:{} {:?}", p.line, p.column, p.kind)))?;
    config.write(dir, file, content)?;
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
#[derive(Debug, Serialize)]
pub struct Restored {
    content: String,
    effect: InEffect,
}

/// Put back in effect the version of `file` replaced at `version`, or with
/// `None` the last valid one (when the file was spoilt outside the app).
#[tauri::command]
pub async fn restore_config(state: State<'_, AppState>, app: AppHandle, file: File, version: Option<u64>) -> Cmd<Restored> {
    restore(&state, &config_dir(&app)?, &file, version)
}

fn restore(state: &AppState, dir: &Path, file: &File, version: Option<u64>) -> Cmd<Restored> {
    let content = match version {
        Some(v) => config_files::version(dir, file, v)?,
        None => {
            let last = crate::lock(&state.config).last_valid(dir, file);
            match last {
                Some(text) => text,
                None => read_default(state, file)?,
            }
        }
    };
    let effect = apply(state, dir, file, &content)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::testing::{block_on, folder, mock_app};
    use orgamnesia_core::keybindings::Profile;
    use tauri::Manager;

    fn profile(id: &str, name: &str) -> Profile {
        Profile { id: id.into(), name: name.into(), editor_preset: "emacs".into(), app: Default::default(), editor: Default::default() }
    }

    #[test]
    fn settings_are_saved_keeping_what_the_window_doesnt_know() {
        let dir = folder("cmd-settings");
        let state = AppState::new();
        crate::lock(&state.settings).config.keybinding_profile = Some("emacs".into());
        let mut s = Settings::default();
        s.config.autosave = Some(false);
        s.projects = vec!["/x".into()];
        save_settings(&state, &dir, s).unwrap();
        let now = state.settings();
        assert_eq!((now.config.autosave, now.config.keybinding_profile.as_deref()), (Some(false), Some("emacs")));
        assert!(std::fs::read_to_string(dir.join("settings.json")).unwrap().contains(r#""autosave": false"#));
        assert!(std::fs::read_to_string(dir.join("state.json")).unwrap().contains("/x"));
    }

    #[test]
    fn keybindings_are_saved_as_profile_files() {
        let dir = folder("cmd-keybindings");
        let state = AppState::new();
        let kb = Keybindings::with_profiles(Some("custom-1"), vec![profile("custom-1", "Mine")]);
        let saved = save_keybindings(&state, &dir, &kb).unwrap();
        assert_eq!(saved.active_profile, "Mine");
        assert_eq!(state.settings().config.keybinding_profile.as_deref(), Some("Mine"));
        let names: Vec<String> = list_files(&dir).into_iter().map(|f| f.name).collect();
        assert_eq!(names, vec!["settings.json", "keybindings/Mine.json"]);
        // The same again: the settings are not written again
        save_keybindings(&state, &dir, &saved).unwrap();
        assert_eq!(config_files::versions(&dir, &File::Settings).len(), 0);
    }

    #[test]
    fn applying_checks_writes_and_puts_in_effect() {
        let dir = folder("cmd-apply");
        let state = AppState::new();
        // Nothing on disk yet: the settings in effect
        assert!(read(&state, &dir, &File::Settings).unwrap().contains("\"autosave\": null"));
        assert!(read(&state, &dir, &File::Profile("Mine".into())).is_err());
        // Wrong: refused, nothing written
        assert!(matches!(apply(&state, &dir, &File::Settings, "{"), Err(AppError::ConfigInvalid(_))));
        assert!(!dir.join("settings.json").exists());
        let effect = apply(&state, &dir, &File::Settings, r#"{"autosave": false}"#).unwrap();
        assert_eq!(effect.settings.autosave, Some(false));
        assert_eq!(state.settings().config.autosave, Some(false));
        assert_eq!(read(&state, &dir, &File::Settings).unwrap(), r#"{"autosave": false}"#);
        // A profile, then the setting naming it
        apply(&state, &dir, &File::Profile("Mine".into()), r#"{"editor_preset": "emacs", "app": {"save": ["ctrl+j"]}}"#).unwrap();
        let effect = apply(&state, &dir, &File::Settings, r#"{"keybinding_profile": "Mine"}"#).unwrap();
        assert_eq!(effect.keybindings.app_binds("save"), &["ctrl+j".to_string()]);
    }

    #[test]
    fn versions_are_put_back() {
        let dir = folder("cmd-restore");
        let state = AppState::new();
        apply(&state, &dir, &File::Settings, r#"{"autosave": false}"#).unwrap();
        apply(&state, &dir, &File::Settings, r#"{"autosave": true}"#).unwrap();
        let versions = config_files::versions(&dir, &File::Settings);
        assert_eq!(versions.len(), 1);
        let back = restore(&state, &dir, &File::Settings, Some(versions[0])).unwrap();
        assert_eq!((back.content.as_str(), back.effect.settings.autosave), (r#"{"autosave": false}"#, Some(false)));
        // Spoilt outside the app: the last valid one comes back
        std::fs::write(dir.join("settings.json"), "{").unwrap();
        let back = restore(&state, &dir, &File::Settings, None).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("settings.json")).unwrap(), back.content);
        // None known: the one in effect
        let fresh = AppState::new();
        let back = restore(&fresh, &folder("cmd-restore-none"), &File::Settings, None).unwrap();
        assert!(back.content.contains("\"autosave\": null"));
    }

    #[test]
    fn commands_of_the_state() {
        let app = mock_app();
        set_session(app.state::<AppState>(), None);
        assert_eq!(block_on(get_settings(app.state::<AppState>())).unwrap(), Settings::default());
        assert_eq!(block_on(get_keybindings(app.state::<AppState>())).unwrap().active_profile, "classic");
        assert!(block_on(config_problems(app.state::<AppState>())).unwrap().is_empty());
    }
}
