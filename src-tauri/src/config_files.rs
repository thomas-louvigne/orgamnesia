//! Watching the configuration files for the changes made outside the app, and
//! putting them in effect in the app's state; the files themselves are
//! `orgamnesia_backend::config_files`.

use std::path::PathBuf;

use orgamnesia_core::keybindings::Keybindings;
use tauri::{AppHandle, Emitter, Manager};

pub use orgamnesia_backend::config_files::*;

use crate::{error::AppError, watch::Watcher, AppState};

/// Event carrying the `Changed` file, valid, changed outside the app and now in effect.
pub const CONFIG_CHANGED: &str = "config-changed";
/// Event carrying the `Invalid` file, changed outside the app: the one before stays in effect.
pub const CONFIG_INVALID: &str = "config-invalid";

/// The configuration in effect: the settings, and the keybindings of the profile they name.
pub fn in_effect(state: &AppState) -> InEffect {
    let settings = state.settings().config;
    let keybindings = keybindings(state);
    InEffect { settings, keybindings }
}

/// The keybindings in effect: those of the profile the settings name.
pub fn keybindings(state: &AppState) -> Keybindings {
    let active = state.settings().config.keybinding_profile;
    crate::lock(&state.config).keybindings(active.as_deref())
}

/// Put the settings `content` in effect in the backend (the keybindings are
/// read from `ConfigState` when asked for).
pub fn put_in_effect(state: &AppState, content: &Content) {
    let Content::Settings(config) = content else { return };
    let (tags_changed, formats, journal) = {
        let mut settings = crate::lock(&state.settings);
        let before = settings.prefs().tags();
        settings.config = config.clone();
        let prefs = settings.prefs();
        (prefs.tags() != before, prefs.formats(), prefs.journal_folder)
    };
    if let Some(p) = crate::lock(&state.project).as_mut() {
        // The pages of a format no longer read go at the next look at the disk
        p.set_formats(formats);
        p.set_journal(&journal);
        // `#tags`, `:tags:` or links are read differently: read the links again
        if tags_changed { p.reindex(state.prefs().tags()); }
    }
}

/// Watch the configuration folder: a file changed by another program is put in
/// effect when valid (`CONFIG_CHANGED`), else the interface is told what is wrong
/// with it (`CONFIG_INVALID`).
pub fn watch(app: AppHandle, dir: PathBuf) -> Result<Watcher, AppError> {
    std::fs::create_dir_all(&dir)?;
    let folder = dir.clone();
    crate::watch::start(&dir, move |events| {
        let state = app.state::<AppState>();
        let mut files: Vec<File> = events.iter()
            .filter(|e| e.path.parent().is_some_and(|p| p == folder || p == folder.join(PROFILES_DIR)))
            .filter_map(|e| File::from_path(&e.path.to_string_lossy()))
            .collect();
        files.sort_by_key(File::name);
        files.dedup();
        // The profiles first: the settings may name one just added
        files.sort_by_key(|f| *f == File::Settings);
        for file in files {
            let reread = crate::lock(&state.config).reread(&folder, &file);
            match reread {
                Some(Ok(())) => {
                    if file == File::Settings {
                        let text = crate::lock(&state.config).applied(&file).map(str::to_string);
                        if let Some(Ok(checked)) = text.map(|t| check(&file, &t, &[])) { put_in_effect(&state, &checked.content); }
                    }
                    let effect = in_effect(&state);
                    let _ = app.emit(CONFIG_CHANGED, Changed { file, effect });
                }
                Some(Err(problem)) => { let _ = app.emit(CONFIG_INVALID, Invalid { file, problem }); }
                None => {}
            }
        }
    })
}
