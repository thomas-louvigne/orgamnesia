//! `keybindings.json`. The shortcuts themselves (actions, presets) are defined
//! in `orgamnesia_core::keybindings`; the backend only stores them.

use std::path::Path;

use crate::error::AppError;

pub use orgamnesia_core::keybindings::{fill_missing, Keybindings};

/// The saved shortcuts; the presets' shortcuts for actions the file doesn't
/// mention (new actions, or no file yet).
pub fn load(config_dir: &Path) -> Keybindings {
    let mut kb = std::fs::read_to_string(config_dir.join("keybindings.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<Keybindings>(&s).ok())
        .unwrap_or_default();
    let base = kb.editor_preset.clone();
    fill_missing(&mut kb.app, &mut kb.editor, &base);
    kb
}

pub fn save(config_dir: &Path, kb: &Keybindings) -> Result<(), AppError> {
    crate::settings::write_json(config_dir, "keybindings.json", kb)
}
