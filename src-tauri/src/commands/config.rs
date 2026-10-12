//! Settings and keybindings.

use tauri::{AppHandle, State};

use super::{config_dir, store_settings, Cmd};
use crate::{
    keybindings,
    settings::{Session, Settings},
    AppState,
};

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Cmd<Settings> {
    Ok(state.settings())
}

#[tauri::command]
pub async fn set_settings(state: State<'_, AppState>, app: AppHandle, mut settings: Settings) -> Cmd<()> {
    // The settings window doesn't know the session: keep it
    settings.session = state.settings().session;
    let tags = settings.prefs().tags();
    let tags_changed = state.prefs().tags() != tags;
    store_settings(&state, &app, settings)?;
    // `#tags` or `:tags:` are read differently (as links or not, with or without `-`):
    // read the links again
    if tags_changed && let Some(p) = state.project.lock().unwrap().as_mut() {
        p.reindex(tags);
    }
    Ok(())
}

/// The pages and panes open now, written to `settings.json` on quit.
/// Not async: calls run in order, so the last state sent is the one kept.
#[tauri::command]
pub fn set_session(state: State<'_, AppState>, session: Option<Session>) {
    state.settings.lock().unwrap().session = session;
}

#[tauri::command]
pub async fn get_keybindings(app: AppHandle) -> Cmd<keybindings::Keybindings> {
    Ok(keybindings::load(&config_dir(&app)?))
}

#[tauri::command]
pub async fn set_keybindings(app: AppHandle, keybindings: keybindings::Keybindings) -> Cmd<()> {
    keybindings::save(&config_dir(&app)?, &keybindings)
}
