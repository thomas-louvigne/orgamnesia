//! The spelling extension.

use std::path::Path;

use tauri::{AppHandle, State};

use super::{config_dir, Cmd};
use crate::{settings::Prefs, spell::{self, Speller}, AppState};

/// Run `f` on the dictionary, with the settings and the config folder.
fn with_speller<T>(
    state: &AppState, app: &AppHandle,
    f: impl FnOnce(&mut Speller, &Prefs, &Path) -> Cmd<T>,
) -> Cmd<T> {
    let prefs = state.prefs();
    f(&mut crate::lock(&state.spell), &prefs, &config_dir(app)?)
}

/// Names of the dictionaries found (`fr_FR`, `en_US`…), with `dict_dir` looked in first.
#[tauri::command]
pub async fn spell_languages(dict_dir: String) -> Cmd<Vec<String>> {
    Ok(spell::languages(&dict_dir))
}

/// Of `words`, those the dictionary of the settings doesn't know.
#[tauri::command]
pub async fn spell_check(state: State<'_, AppState>, app: AppHandle, words: Vec<String>) -> Cmd<Vec<String>> {
    with_speller(&state, &app, |s, prefs, dir| s.misspelled(prefs, dir, words))
}

/// Words `word` may have been meant as.
#[tauri::command]
pub async fn spell_suggest(state: State<'_, AppState>, app: AppHandle, word: String) -> Cmd<Vec<String>> {
    with_speller(&state, &app, |s, prefs, dir| s.suggest(prefs, dir, &word))
}

/// Add `word` to the user's dictionary.
#[tauri::command]
pub async fn spell_add(state: State<'_, AppState>, app: AppHandle, word: String) -> Cmd<()> {
    with_speller(&state, &app, |s, prefs, dir| s.add(prefs, dir, &word))
}
