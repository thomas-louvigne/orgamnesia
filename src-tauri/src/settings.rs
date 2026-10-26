use std::path::Path;

use orgamnesia_core::settings::STATE_KEYS;
use serde::{Deserialize, Serialize};

use crate::{
    config_files::{self, to_text, ConfigState, Content, File},
    error::AppError,
};

pub use orgamnesia_core::{Prefs, Session, Settings};

/// What the app keeps for itself: the open project, the projects, the session.
const STATE_FILE: &str = "state.json";

/// The app used to be called "Org Wiki Flow" (config folder `com.org-wiki-flow.app`).
/// On the first run under the new name, carry the old settings and keybindings over.
pub fn migrate_legacy_config(config_dir: &Path) {
    let Some(parent) = config_dir.parent() else { return };
    let old = parent.join("com.org-wiki-flow.app");
    if config_dir.exists() || !old.is_dir() {
        return;
    }
    if std::fs::create_dir_all(config_dir).is_err() {
        return;
    }
    for file in ["settings.json", "keybindings.json"] {
        let _ = std::fs::copy(old.join(file), config_dir.join(file));
    }
}

/// What the app keeps for itself, apart from the settings (`state.json`).
#[derive(Serialize, Deserialize, Default)]
struct State {
    vault_path: Option<String>,
    #[serde(default)]
    projects: Vec<String>,
    session: Option<Session>,
}

/// The settings (`settings.json`) and what the app keeps for itself
/// (`state.json`), as the app starts; `config` learns what the configuration
/// files hold (the keybinding profiles too). Files of older versions are
/// brought up to date, kept as they were as versions: a `settings.json` holding
/// what `state.json` now does is split; the keybinding profile chosen, once in
/// `keybindings.json`, is now a setting.
pub fn load(config_dir: &Path, config: &mut ConfigState) -> Settings {
    let text = std::fs::read_to_string(config_files::path(config_dir, &File::Settings)).ok();
    let mut settings: Settings = text.as_deref().and_then(|s| serde_json::from_str(s).ok()).unwrap_or_default();
    // The text read as the one in effect, and whether to write it again up to date
    let (mut in_effect, mut rewrite) = (text.clone(), false);
    match std::fs::read_to_string(config_dir.join(STATE_FILE)).ok().and_then(|s| serde_json::from_str::<State>(&s).ok()) {
        Some(state) => {
            settings.vault_path = state.vault_path;
            settings.projects = state.projects;
            settings.session = state.session;
        }
        None => {
            if let Some(rest) = text.as_deref().and_then(without_state) && save_state(config_dir, &settings).is_ok() {
                in_effect = Some(rest);
                rewrite = true;
            }
        }
    }
    // The profiles before the settings: the settings name one of them
    if let Some(active) = crate::keybindings::load(config_dir, config) {
        settings.config.keybinding_profile = Some(active);
        rewrite = true;
    }
    config.loaded(File::Settings, in_effect.as_deref());
    // Written again only when valid; else left for the user to correct
    if rewrite && config.problems().iter().all(|p| p.file != File::Settings) {
        let _ = config.write(config_dir, &File::Settings, &config_text(&settings));
    }
    settings
}

/// `text`, a `settings.json` written before `state.json` existed, without what
/// `state.json` now holds; `None` when it holds none of it.
fn without_state(text: &str) -> Option<String> {
    let mut map = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(text).ok()?;
    let before = map.len();
    for k in STATE_KEYS { map.remove(k); }
    (map.len() < before).then(|| serde_json::to_string_pretty(&map).unwrap_or_default())
}

/// The text of `settings.json` for `settings`.
pub fn config_text(settings: &Settings) -> String {
    to_text(&Content::Settings(settings.config.clone()))
}

/// Write what the app keeps for itself to `state.json`.
pub fn save_state(config_dir: &Path, settings: &Settings) -> Result<(), AppError> {
    let state = State {
        vault_path: settings.vault_path.clone(),
        projects: settings.projects.clone(),
        session: settings.session.clone(),
    };
    std::fs::create_dir_all(config_dir)?;
    std::fs::write(config_dir.join(STATE_FILE), serde_json::to_string_pretty(&state)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontend_payload_is_accepted() {
        // What the settings window sends on Apply/Save
        let json = r#"{"vault_path":"/x","org_bloom_path":null,"language":"fr",
            "update_links_on_rename":true,"case_insensitive_links":true,"emacs_mark":true,
            "electric_mode":false,"autosave":true,"projects":["/x"]}"#;
        let s: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(s.config.electric_mode, Some(false));
        // Older files lack the newer fields
        let old: Settings = serde_json::from_str(r#"{"vault_path":null}"#).unwrap();
        assert_eq!(old.config.electric_mode, None);
    }

    #[test]
    fn session_round_trips_through_json() {
        let session = Session {
            project: "/x".into(),
            tabs: vec!["/x/a.org".into(), "/x/b.org".into()],
            active: Some("/x/b.org".into()),
            split: Some(orgamnesia_core::SplitKind::Vertical),
            other: Some("/x/a.org".into()),
            focus_second: true,
        };
        let s = Settings { session: Some(session), ..Settings::default() };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains(r#""split":"vertical""#));
        assert_eq!(serde_json::from_str::<Settings>(&json).unwrap(), s);
    }

    #[test]
    fn old_settings_are_split_in_two() {
        let dir = std::env::temp_dir().join(format!("orgamnesia-settings-split-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("settings.json"), r#"{"vault_path":"/x","autosave":false,"projects":["/x"]}"#).unwrap();
        let mut config = ConfigState::default();
        let s = load(&dir, &mut config);
        assert_eq!((s.vault_path.as_deref(), s.config.autosave), (Some("/x"), Some(false)));
        assert!(config.problems().is_empty());
        let text = std::fs::read_to_string(dir.join("settings.json")).unwrap();
        assert!(!text.contains("projects") && text.contains(r#""autosave": false"#));
        assert_eq!(config_files::versions(&dir, &File::Settings).len(), 1);
        // Read again: the same, from both files
        let again = load(&dir, &mut ConfigState::default());
        assert_eq!(again, Settings { session: None, ..s });
        let _ = std::fs::remove_dir_all(&dir);
    }
}
