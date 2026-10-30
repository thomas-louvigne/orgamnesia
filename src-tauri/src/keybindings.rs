//! The keybinding profiles of the user's own, each in its file named after it
//! (`keybindings/<name>.json`); the built-in profiles are in the code, and the
//! profile in effect is a setting (`keybinding_profile`). The shortcuts
//! themselves (actions, presets) are defined in `orgamnesia_core::keybindings`;
//! the backend only stores them.

use std::path::Path;

use orgamnesia_core::keybindings::{Profile, ProfileFile};

use crate::{
    config_files::{self, to_text, ConfigState, Content, File},
    error::AppError,
};

pub use orgamnesia_core::keybindings::Keybindings;

/// `keybindings.json`: what older versions kept the keybindings in.
const OLD_FILE: &str = "keybindings.json";

/// Read the profiles as the app starts. Those of older versions are brought up
/// to date, the files as they were kept as versions: a `keybindings.json` (all
/// the shortcuts, or the profile chosen), profiles named by an id with their
/// name in them. Returns then the name of the profile chosen, now a setting.
pub fn load(dir: &Path, config: &mut ConfigState) -> Option<String> {
    let migrated = migrate(dir);
    for file in config_files::profiles_on_disk(dir) {
        let text = std::fs::read_to_string(config_files::path(dir, &file)).ok();
        config.loaded(file, text.as_deref());
    }
    migrated
}

fn migrate(dir: &Path) -> Option<String> {
    let old = std::fs::read_to_string(dir.join(OLD_FILE)).ok();
    // Profiles named by an id, with their name in them
    let named: Vec<(File, String, Profile)> = config_files::profiles_on_disk(dir).into_iter().filter_map(|file| {
        let File::Profile(id) = &file else { return None };
        let text = std::fs::read_to_string(config_files::path(dir, &file)).ok()?;
        let map = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&text).ok()?;
        let name = map.get("name")?.as_str()?.to_string();
        let p = serde_json::from_str::<ProfileFile>(&text).ok()?;
        let profile = Profile { id: id.clone(), name, editor_preset: p.editor_preset, app: p.app, editor: p.editor };
        Some((file, text, profile))
    }).collect();
    if old.is_none() && named.is_empty() { return None; }

    let (active, mut profiles) = match old.as_deref().and_then(Keybindings::from_old) {
        Some(kb) => (Some(kb.active_profile.clone()), kb.profiles),
        None => {
            let choice = old.as_deref()
                .and_then(|t| serde_json::from_str::<serde_json::Value>(t).ok())
                .and_then(|v| v.get("active_profile")?.as_str().map(str::to_string));
            (choice, vec![])
        }
    };
    profiles.extend(named.iter().map(|(_, _, p)| p.clone()));
    let (active, files) = Keybindings::with_profiles(active.as_deref(), profiles).to_files();

    if let Some(text) = &old {
        let _ = config_files::archive_as(dir, "keybindings", text);
        let _ = std::fs::remove_file(dir.join(OLD_FILE));
    }
    for (file, text, _) in &named {
        let _ = config_files::archive(dir, file, text);
        let _ = std::fs::remove_file(config_files::path(dir, file));
    }
    let _ = std::fs::create_dir_all(dir.join(config_files::PROFILES_DIR));
    for (name, profile) in files {
        let path = config_files::path(dir, &File::Profile(name));
        if !path.exists() { let _ = std::fs::write(path, to_text(&Content::Profile(profile))); }
    }
    Some(active)
}

/// Write the profiles of `kb`, each in its file named after it; the files of the
/// profiles no longer there are deleted (kept as versions). Returns the name of
/// the profile in effect, for the settings.
pub fn save(dir: &Path, config: &mut ConfigState, kb: &Keybindings) -> Result<String, AppError> {
    let (active, profiles) = kb.to_files();
    let names: Vec<String> = profiles.iter().map(|(name, _)| name.clone()).collect();
    for (name, profile) in profiles {
        config.write(dir, &File::Profile(name), &to_text(&Content::Profile(profile)))?;
    }
    for file in config_files::profiles_on_disk(dir) {
        if let File::Profile(name) = &file && !names.contains(name) { config.remove(dir, &file)?; }
    }
    Ok(active)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(test: &str) -> std::path::PathBuf {
        crate::commands::testing::folder(&format!("keybindings-{test}"))
    }

    #[test]
    fn old_files_become_profiles_named_after_them() {
        let dir = folder("old");
        std::fs::write(dir.join(OLD_FILE), r#"{"editor_preset": "emacs", "active_profile": "c1",
            "app": {"save": ["ctrl+j"]}, "editor": {},
            "profiles": [{"id": "c1", "name": "Mine", "editor_preset": "emacs", "app": {}, "editor": {}}]}"#).unwrap();
        let mut config = ConfigState::default();
        assert_eq!(load(&dir, &mut config).as_deref(), Some("Mine"));
        assert!(config.problems().is_empty(), "{:?}", config.problems());
        assert!(!dir.join(OLD_FILE).exists() && dir.join("keybindings/Mine.json").exists());
        assert_eq!(config.keybindings(Some("Mine")).app_binds("save"), &["ctrl+j".to_string()]);
        // Nothing left to bring up to date
        assert_eq!(load(&dir, &mut ConfigState::default()), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn profiles_named_by_an_id_are_renamed() {
        let dir = folder("ids");
        std::fs::write(dir.join(OLD_FILE), r#"{"active_profile": "custom-1"}"#).unwrap();
        std::fs::create_dir_all(dir.join("keybindings")).unwrap();
        std::fs::write(dir.join("keybindings/custom-1.json"), r#"{"name": "Personnalisé", "editor_preset": "emacs", "app": {"save": ["ctrl+j"]}, "editor": {}}"#).unwrap();
        let mut config = ConfigState::default();
        assert_eq!(load(&dir, &mut config).as_deref(), Some("Personnalisé"));
        assert_eq!(config_files::profiles_on_disk(&dir), vec![File::Profile("Personnalisé".into())]);
        assert_eq!(config.keybindings(Some("Personnalisé")).app_binds("save"), &["ctrl+j".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn saving_follows_the_profiles() {
        let dir = folder("save");
        let mut config = ConfigState::default();
        let profile = |name: &str| Profile { id: format!("custom-{name}"), name: name.into(), editor_preset: "emacs".into(),
            app: Default::default(), editor: Default::default() };
        let kb = Keybindings::with_profiles(Some("custom-A"), vec![profile("A"), profile("B")]);
        assert_eq!(save(&dir, &mut config, &kb).unwrap(), "A");
        // B renamed C, A deleted
        let kb = Keybindings::with_profiles(Some("emacs"), vec![profile("C")]);
        assert_eq!(save(&dir, &mut config, &kb).unwrap(), "emacs");
        assert_eq!(config_files::profiles_on_disk(&dir), vec![File::Profile("C".into())]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
