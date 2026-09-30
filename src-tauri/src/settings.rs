use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::error::AppError;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings {
    pub vault_path: Option<String>,
    pub logseq_site_builder_path: Option<String>,
    pub language: Option<String>,
    /// Every project (folder) the user has opened, most recent first.
    /// Rewrite `[[links]]` in other pages when a page is renamed (default: true).
    pub update_links_on_rename: Option<bool>,
    /// Match `[[links]]` to pages ignoring case (default: true).
    pub case_insensitive_links: Option<bool>,
    /// Treat `#tags` as links to pages (default: true).
    pub hashtag_links: Option<bool>,
    /// Emacs mark: Ctrl+Space starts a region that follows the cursor (default: true).
    pub emacs_mark: Option<bool>,
    /// Electric mode: typing a bracket or quote around a selection wraps it (default: true).
    pub electric_mode: Option<bool>,
    /// Save the active page on every change (default: true).
    pub autosave: Option<bool>,
    #[serde(default)]
    pub projects: Vec<String>,
}

impl Settings {
    pub fn case_insensitive_links(&self) -> bool {
        self.case_insensitive_links.unwrap_or(true)
    }

    pub fn hashtag_links(&self) -> bool {
        self.hashtag_links.unwrap_or(true)
    }

    pub fn update_links_on_rename(&self) -> bool {
        self.update_links_on_rename.unwrap_or(true)
    }
}

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

pub fn load(config_dir: &Path) -> Settings {
    let path = config_dir.join("settings.json");
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(config_dir: &Path, settings: &Settings) -> Result<(), AppError> {
    std::fs::create_dir_all(config_dir)
        .map_err(|e| AppError::SettingsError(e.to_string()))?;
    let json = serde_json::to_string_pretty(settings)
        .map_err(|e| AppError::SettingsError(e.to_string()))?;
    std::fs::write(config_dir.join("settings.json"), json)
        .map_err(|e| AppError::SettingsError(e.to_string()))
}
