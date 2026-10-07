use std::path::Path;

use crate::error::AppError;

pub use orgamnesia_core::{Prefs, Settings};

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

pub fn load(config_dir: &Path) -> Settings {
    let path = config_dir.join("settings.json");
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(config_dir: &Path, settings: &Settings) -> Result<(), AppError> {
    write_json(config_dir, "settings.json", settings)
}

/// Write `value` as `<config_dir>/<file>`, creating the folder if needed.
pub fn write_json(config_dir: &Path, file: &str, value: &impl serde::Serialize) -> Result<(), AppError> {
    std::fs::create_dir_all(config_dir)?;
    std::fs::write(config_dir.join(file), serde_json::to_string_pretty(value)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontend_payload_is_accepted() {
        // What the settings window sends on Apply/Save
        let json = r#"{"vault_path":"/x","logseq_site_builder_path":null,"language":"fr",
            "update_links_on_rename":true,"case_insensitive_links":true,"emacs_mark":true,
            "electric_mode":false,"autosave":true,"projects":["/x"]}"#;
        let s: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(s.electric_mode, Some(false));
        // Older files lack the newer fields
        let old: Settings = serde_json::from_str(r#"{"vault_path":null}"#).unwrap();
        assert_eq!(old.electric_mode, None);
    }
}
