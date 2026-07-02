use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::error::AppError;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings {
    pub vault_path: Option<String>,
    pub logseq_site_builder_path: Option<String>,
    pub language: Option<String>,
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
