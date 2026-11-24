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
    #[serde(default)]
    assets_declined: Vec<String>,
    #[serde(default)]
    keep_json: bool,
    #[serde(default)]
    tui_left_width: Option<u16>,
    #[serde(default)]
    tui_right_width: Option<u16>,
}

/// The settings (`settings.yaml`) and what the app keeps for itself
/// (`state.json`), as the app starts; `config` learns what the configuration
/// files hold (the keybinding profiles too). Files of older versions are
/// brought up to date, kept as they were as versions: settings holding
/// what `state.json` now does is split; the keybinding profile chosen, once in
/// `keybindings.json`, is now a setting.
pub fn load(config_dir: &Path, config: &mut ConfigState) -> Settings {
    // The JSON files of older versions as YAML first: the steps below read YAML
    let converted = !config_files::path(config_dir, &File::Settings).exists() && config_dir.join(File::Settings.legacy_name()).exists();
    config_files::convert_legacy(config_dir);
    let text = std::fs::read_to_string(config_files::path(config_dir, &File::Settings)).ok();
    let mut settings: Settings = text.as_deref().and_then(|s| serde_yaml_ng::from_str::<Option<Settings>>(s).ok()).flatten().unwrap_or_default();
    // Nothing set yet: the interface asks what matters first
    settings.first_run = text.is_none();
    // The text read as the one in effect, and whether to write it again up to date
    // (a file converted gets the comments the app writes)
    let (mut in_effect, mut rewrite) = (text.clone(), converted);
    match std::fs::read_to_string(config_dir.join(STATE_FILE)).ok().and_then(|s| serde_json::from_str::<State>(&s).ok()) {
        Some(state) => {
            settings.vault_path = state.vault_path;
            settings.projects = state.projects;
            settings.session = state.session;
            settings.assets_declined = state.assets_declined;
            settings.keep_json = state.keep_json;
            (settings.tui_left_width, settings.tui_right_width) = (state.tui_left_width, state.tui_right_width);
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

/// `text`, settings written before `state.json` existed, without what
/// `state.json` now holds; `None` when it holds none of it.
fn without_state(text: &str) -> Option<String> {
    let mut map = serde_yaml_ng::from_str::<serde_yaml_ng::Mapping>(text).ok()?;
    let before = map.len();
    for k in STATE_KEYS { map.remove(k); }
    (map.len() < before).then(|| serde_yaml_ng::to_string(&map).unwrap_or_default())
}

/// The text of `settings.yaml` for `settings`.
pub fn config_text(settings: &Settings) -> String {
    to_text(&Content::Settings(settings.config.clone()))
}

/// Write what the app keeps for itself to `state.json`.
pub fn save_state(config_dir: &Path, settings: &Settings) -> Result<(), AppError> {
    let state = State {
        vault_path: settings.vault_path.clone(),
        projects: settings.projects.clone(),
        session: settings.session.clone(),
        assets_declined: settings.assets_declined.clone(),
        keep_json: settings.keep_json,
        tui_left_width: settings.tui_left_width,
        tui_right_width: settings.tui_right_width,
    };
    std::fs::create_dir_all(config_dir)?;
    crate::write_atomic(config_dir.join(STATE_FILE), serde_json::to_string_pretty(&state)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontend_payload_is_accepted() {
        // What the settings window sends on Apply/Save
        let json = r#"{"vault_path":"/x","org_bloom_path":null,"language":"fr",
            "update_links_on_rename":true,"case_insensitive_links":true,"emacs_mark":true,"webkit_emoji_shortcuts":false,
            "electric_mode":false,"autosave":true,"projects":["/x"]}"#;
        let s: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(s.config.electric_mode, Some(false));
        assert_eq!(s.config.webkit_emoji_shortcuts, Some(false));
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
    fn nothing_saved_is_the_first_run() {
        let dir = std::env::temp_dir().join(format!("orgamnesia-settings-first-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(load(&dir, &mut ConfigState::default()).first_run);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn old_settings_are_split_in_two() {
        let dir = std::env::temp_dir().join(format!("orgamnesia-settings-split-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("settings.json"), r#"{"vault_path":"/x","autosave":false,"projects":["/x"]}"#).unwrap();
        let mut config = ConfigState::default();
        let s = load(&dir, &mut config);
        assert!(!s.first_run);
        assert_eq!((s.vault_path.as_deref(), s.config.autosave), (Some("/x"), Some(false)));
        assert!(config.problems().is_empty());
        let text = std::fs::read_to_string(dir.join("settings.yaml")).unwrap();
        assert!(!text.contains("\nprojects:") && text.contains("autosave: false"), "{text}");
        assert!(text.starts_with("# Orgamnesia"));
        // The JSON file stays, for an older version of the app
        assert!(dir.join("settings.json").exists());
        assert_eq!(config_files::versions(&dir, &File::Settings).len(), 1);
        // Read again: the same, from both files
        let again = load(&dir, &mut ConfigState::default());
        assert_eq!(again, Settings { session: None, ..s });
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_update_from_1_7_converts_the_json_files() {
        // The configuration folder as version 1.7.0 left it
        let dir = std::env::temp_dir().join(format!("orgamnesia-settings-update-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("keybindings")).unwrap();
        std::fs::create_dir_all(dir.join("history")).unwrap();
        std::fs::write(dir.join("settings.json"), r#"{
  "language": "fr",
  "autosave": true,
  "org_links": {"pipe": true, "hash": false, "markdown": true},
  "frames_left": ["pages", "tags"],
  "trash_keep_days": 30,
  "git_commit_template": "Notes du {date}",
  "keybinding_profile": "Personnalisé"
}"#).unwrap();
        std::fs::write(dir.join("state.json"), r#"{"vault_path": "/notes", "projects": ["/notes"]}"#).unwrap();
        std::fs::write(dir.join("keybindings/Personnalisé.json"), r#"{"editor_preset": "emacs", "app": {"quick_open": ["ctrl+x b", "alt+k"]}, "editor": {"find": ["ctrl+s"]}}"#).unwrap();
        std::fs::write(dir.join("history/settings-1.json"), r#"{"autosave": false}"#).unwrap();
        let json = std::fs::read_to_string(dir.join("settings.json")).unwrap();

        let mut config = ConfigState::default();
        let s = load(&dir, &mut config);
        assert!(config.problems().is_empty(), "{:?}", config.problems());
        assert!(!s.first_run);
        // The same settings, read from the YAML written
        let c = &s.config;
        assert_eq!((c.language.as_deref(), c.autosave, c.trash_keep_days), (Some("fr"), Some(true), Some(30)));
        assert_eq!(c.org_links.map(|l| (l.pipe, l.hash, l.markdown)), Some((true, false, true)));
        assert_eq!(c.frames_left.as_deref(), Some(&["pages".to_string(), "tags".to_string()][..]));
        assert_eq!(c.git_commit_template.as_deref(), Some("Notes du {date}"));
        assert_eq!((s.vault_path.as_deref(), s.projects.len()), (Some("/notes"), 1));
        let yaml = std::fs::read_to_string(dir.join("settings.yaml")).unwrap();
        assert!(yaml.starts_with("# Orgamnesia") && yaml.contains("# window only: no images"), "{yaml}");
        // The profile, YAML too, in effect
        assert!(dir.join("keybindings/Personnalisé.yaml").exists());
        let kb = config.keybindings(c.keybinding_profile.as_deref());
        assert_eq!(kb.active_profile, "Personnalisé");
        assert_eq!(kb.app_binds("quick_open"), &["ctrl+x b".to_string(), "alt+k".to_string()]);
        // The versions kept by 1.7.0 still listed and read
        assert_eq!(config_files::version(&dir, &File::Settings, 1).unwrap(), r#"{"autosave": false}"#);
        // The JSON files left as they were (for 1.7.0), and the conversion done once
        assert_eq!(std::fs::read_to_string(dir.join("settings.json")).unwrap(), json);
        let again = load(&dir, &mut ConfigState::default());
        assert_eq!(again.config, s.config);
        assert_eq!(std::fs::read_to_string(dir.join("settings.yaml")).unwrap(), yaml);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
