use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

use crate::error::AppError;

/// An action maps to a list of bindings (0, 1 or 2 accelerators).
pub type Bindings = BTreeMap<String, Vec<String>>;

fn default_preset() -> String { "classic".to_string() }

/// A user-made shortcut profile. Built-in ones ("classic", "emacs") are not stored.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub editor_preset: String,
    pub app: Bindings,
    pub editor: Bindings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Keybindings {
    #[serde(default = "default_preset")]
    pub editor_preset: String,
    #[serde(default)]
    pub app: Bindings,
    #[serde(default)]
    pub editor: Bindings,
    /// Active profile id: a built-in preset id or a custom `Profile::id`.
    #[serde(default)]
    pub active_profile: String,
    #[serde(default)]
    pub profiles: Vec<Profile>,
}

impl Default for Keybindings {
    fn default() -> Self {
        Self {
            editor_preset: default_preset(),
            app: default_app(),
            editor: classic_editor(),
            active_profile: default_preset(),
            profiles: vec![],
        }
    }
}

fn m(pairs: &[(&str, &[&str])]) -> Bindings {
    pairs
        .iter()
        .map(|(k, binds)| (k.to_string(), binds.iter().map(|b| b.to_string()).collect()))
        .collect()
}

fn default_app() -> Bindings {
    m(&[
        ("new_page",      &["ctrl+n"]),
        ("save",          &["ctrl+s"]),
        ("open_settings", &["ctrl+,"]),
        ("quick_open",    &["ctrl+k", "ctrl+shift+k"]),
        ("close_tab",     &["ctrl+w", "ctrl+shift+w"]),
        ("next_tab",      &["ctrl+Tab"]),
        ("prev_tab",      &["ctrl+shift+Tab"]),
    ])
}

fn classic_editor() -> Bindings {
    m(&[
        ("copy",        &["ctrl+c"]),
        ("cut",         &["ctrl+x"]),
        ("paste",       &["ctrl+v"]),
        ("select_all",  &["ctrl+a"]),
        ("undo",        &["ctrl+z"]),
        ("redo",        &["ctrl+shift+z", "ctrl+y"]),
        ("new_heading", &["shift+Enter"]),
        ("open_link",   &["ctrl+Enter"]),
    ])
}

fn emacs_editor() -> Bindings {
    m(&[
        ("beginning_of_line",   &["ctrl+a"]),
        ("end_of_line",         &["ctrl+e"]),
        ("kill_line",           &["ctrl+k"]),
        ("kill_word_forward",   &["alt+d"]),
        ("kill_word_backward",  &["alt+Backspace"]),
        ("yank",                &["ctrl+y"]),
        ("kill_region",         &["ctrl+w"]),
        ("copy_region",         &["alt+w"]),
        ("delete_char_forward", &["ctrl+d"]),
        ("undo",                &["ctrl+/", "ctrl+x u"]),
        ("new_heading",         &["alt+Enter", "shift+Enter"]),
        ("open_link",           &["ctrl+c ctrl+o"]),
        ("forward_char",        &["ctrl+f"]),
        ("backward_char",       &["ctrl+b"]),
        ("next_line",           &["ctrl+n"]),
        ("previous_line",       &["ctrl+p"]),
        ("forward_word",        &["alt+f"]),
        ("backward_word",       &["alt+b"]),
        ("beginning_of_buffer", &["alt+<", "alt+shift+<"]),
        ("end_of_buffer",       &["alt+>", "alt+shift+>"]),
        ("scroll_down",         &["ctrl+v"]),
        ("scroll_up",           &["alt+v"]),
        ("backward_sentence",   &["alt+a"]),
        ("forward_sentence",    &["alt+e"]),
        ("backward_paragraph",  &["alt+shift+{", "ctrl+ArrowUp"]),
        ("forward_paragraph",   &["alt+shift+}", "ctrl+ArrowDown"]),
        ("back_to_indentation", &["alt+m"]),
        ("previous_heading",    &["ctrl+c ctrl+p"]),
        ("next_heading",        &["ctrl+c ctrl+n"]),
        ("set_mark",            &["ctrl+space"]),
        ("keyboard_quit",       &["ctrl+g"]),
    ])
}

fn preset_editor(preset: &str) -> Bindings {
    match preset {
        "emacs" => emacs_editor(),
        _       => classic_editor(),
    }
}

/// Fill in any action missing from the persisted file with its default binding.
/// Existing actions are left untouched (a slot the user removed is not restored).
fn merge_defaults(kb: &mut Keybindings) {
    for (k, v) in default_app() {
        kb.app.entry(k).or_insert(v);
    }
    for (k, v) in preset_editor(&kb.editor_preset) {
        kb.editor.entry(k).or_insert(v);
    }
}

pub fn load(config_dir: &Path) -> Keybindings {
    let path = config_dir.join("keybindings.json");
    let mut kb = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<Keybindings>(&s).ok())
        .unwrap_or_default();
    merge_defaults(&mut kb);
    kb
}

pub fn save(config_dir: &Path, kb: &Keybindings) -> Result<(), AppError> {
    std::fs::create_dir_all(config_dir)
        .map_err(|e| AppError::SettingsError(e.to_string()))?;
    let json = serde_json::to_string_pretty(kb)
        .map_err(|e| AppError::SettingsError(e.to_string()))?;
    std::fs::write(config_dir.join("keybindings.json"), json)
        .map_err(|e| AppError::SettingsError(e.to_string()))
}
