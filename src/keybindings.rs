use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// An action maps to a list of bindings (0, 1 or 2 accelerators).
pub type Bindings = BTreeMap<String, Vec<String>>;

fn default_preset() -> String { "classic".to_string() }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Keybindings {
    /// Which editor preset is currently applied ("classic" | "emacs").
    #[serde(default = "default_preset")]
    pub editor_preset: String,
    /// Application-level shortcuts (action id -> bindings).
    #[serde(default)]
    pub app: Bindings,
    /// Editor shortcuts (action id -> bindings), driven by `editor_preset`.
    #[serde(default)]
    pub editor: Bindings,
}

impl Default for Keybindings {
    fn default() -> Self { Self::defaults() }
}

impl Keybindings {
    pub fn defaults() -> Self {
        Self {
            editor_preset: default_preset(),
            app: default_app(),
            editor: preset_editor("classic"),
        }
    }

    /// Bindings for an app action (empty slice if unbound).
    pub fn app_binds(&self, id: &str) -> &[String] {
        self.app.get(id).map(|v| v.as_slice()).unwrap_or(&[])
    }
}

// ─── Action registry (single source of truth for UI + presets) ────────────────

pub struct ActionDef {
    pub id: &'static str,
    pub label_key: &'static str,
}

pub const APP_ACTIONS: &[ActionDef] = &[
    ActionDef { id: "new_page",      label_key: "shortcut_new_page" },
    ActionDef { id: "save",          label_key: "shortcut_save" },
    ActionDef { id: "open_settings", label_key: "shortcut_settings" },
    ActionDef { id: "quick_open",    label_key: "shortcut_quick_open" },
    ActionDef { id: "close_tab",     label_key: "shortcut_close_tab" },
    ActionDef { id: "next_tab",      label_key: "shortcut_next_tab" },
    ActionDef { id: "prev_tab",      label_key: "shortcut_prev_tab" },
];

const CLASSIC_ACTIONS: &[ActionDef] = &[
    ActionDef { id: "copy",        label_key: "shortcut_copy" },
    ActionDef { id: "cut",         label_key: "shortcut_cut" },
    ActionDef { id: "paste",       label_key: "shortcut_paste" },
    ActionDef { id: "select_all",  label_key: "shortcut_select_all" },
    ActionDef { id: "undo",        label_key: "shortcut_undo_edit" },
    ActionDef { id: "redo",        label_key: "shortcut_redo" },
    ActionDef { id: "new_heading", label_key: "shortcut_new_heading" },
];

const EMACS_ACTIONS: &[ActionDef] = &[
    ActionDef { id: "beginning_of_line",   label_key: "shortcut_bol" },
    ActionDef { id: "end_of_line",         label_key: "shortcut_eol" },
    ActionDef { id: "kill_line",           label_key: "shortcut_kill_line" },
    ActionDef { id: "kill_word_forward",   label_key: "shortcut_kill_word_fwd" },
    ActionDef { id: "kill_word_backward",  label_key: "shortcut_kill_word_bwd" },
    ActionDef { id: "yank",                label_key: "shortcut_yank" },
    ActionDef { id: "kill_region",         label_key: "shortcut_kill_region" },
    ActionDef { id: "copy_region",         label_key: "shortcut_copy_region" },
    ActionDef { id: "delete_char_forward", label_key: "shortcut_del_char_fwd" },
    ActionDef { id: "undo",                label_key: "shortcut_undo_edit" },
    ActionDef { id: "new_heading",         label_key: "shortcut_new_heading" },
];

/// Preset ids available in the UI (VI intentionally left as a future addition).
pub const EDITOR_PRESETS: &[&str] = &["classic", "emacs"];

/// The editor actions shown/handled for a given preset.
pub fn editor_actions(preset: &str) -> &'static [ActionDef] {
    match preset {
        "emacs" => EMACS_ACTIONS,
        _       => CLASSIC_ACTIONS,
    }
}

// ─── Default / preset binding tables ──────────────────────────────────────────

fn m(pairs: &[(&str, &[&str])]) -> Bindings {
    pairs
        .iter()
        .map(|(k, binds)| (k.to_string(), binds.iter().map(|b| b.to_string()).collect()))
        .collect()
}

pub fn default_app() -> Bindings {
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

/// Default editor bindings for a preset (used on first run and when applying a preset).
pub fn preset_editor(preset: &str) -> Bindings {
    match preset {
        "emacs" => m(&[
            ("beginning_of_line",   &["ctrl+a"]),
            ("end_of_line",         &["ctrl+e"]),
            ("kill_line",           &["ctrl+k"]),
            ("kill_word_forward",   &["alt+d"]),
            ("kill_word_backward",  &["alt+Backspace"]),
            ("yank",                &["ctrl+y"]),
            ("kill_region",         &["ctrl+w"]),
            ("copy_region",         &["alt+w"]),
            ("delete_char_forward", &["ctrl+d"]),
            ("undo",                &["ctrl+/", "ctrl+z"]),
            ("new_heading",         &["shift+Enter"]),
        ]),
        // "classic" and any unknown preset fall back to the classic table.
        _ => m(&[
            ("copy",        &["ctrl+c"]),
            ("cut",         &["ctrl+x"]),
            ("paste",       &["ctrl+v"]),
            ("select_all",  &["ctrl+a"]),
            ("undo",        &["ctrl+z"]),
            ("redo",        &["ctrl+shift+z", "ctrl+y"]),
            ("new_heading", &["shift+Enter"]),
        ]),
    }
}

// ─── Matching / capture / display ─────────────────────────────────────────────

/// Returns the id of the first action in `map` whose any binding matches the event.
pub fn match_action<'a>(map: &'a Bindings, event: &web_sys::KeyboardEvent) -> Option<&'a str> {
    map.iter()
        .find(|(_, binds)| binds.iter().any(|b| key_matches(event, b)))
        .map(|(id, _)| id.as_str())
}

/// Returns true if the keyboard event matches a binding string like "ctrl+shift+n".
pub fn key_matches(event: &web_sys::KeyboardEvent, binding: &str) -> bool {
    if binding.is_empty() { return false; }
    let parts: Vec<&str> = binding.split('+').collect();
    let Some(key) = parts.last() else { return false };
    let need_ctrl  = parts.contains(&"ctrl");
    let need_shift = parts.contains(&"shift");
    let need_alt   = parts.contains(&"alt");
    let ev_key = event.key();
    let ev_key_norm = if ev_key.len() == 1 { ev_key.to_lowercase() } else { ev_key };
    event.ctrl_key()  == need_ctrl
        && event.shift_key() == need_shift
        && event.alt_key()   == need_alt
        && ev_key_norm == *key
}

/// Builds a binding string from a KeyboardEvent (returns None for bare modifiers).
pub fn capture_from_event(event: &web_sys::KeyboardEvent) -> Option<String> {
    let key = event.key();
    if matches!(key.as_str(), "Control" | "Shift" | "Alt" | "Meta") {
        return None;
    }
    let mut parts: Vec<String> = Vec::new();
    if event.ctrl_key()  { parts.push("ctrl".into()); }
    if event.shift_key() { parts.push("shift".into()); }
    if event.alt_key()   { parts.push("alt".into()); }
    let key_part = if key.len() == 1 { key.to_lowercase() } else { key };
    parts.push(key_part);
    Some(parts.join("+"))
}

/// Returns a human-readable label ("ctrl+k" → "Ctrl+K").
pub fn display(binding: &str) -> String {
    if binding.is_empty() { return "—".into(); }
    binding.split('+')
        .map(|p| match p {
            "ctrl"  => "Ctrl".to_string(),
            "shift" => "Shift".to_string(),
            "alt"   => "Alt".to_string(),
            k       => {
                let mut c = k.chars();
                match c.next() {
                    None    => String::new(),
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                }
            }
        })
        .collect::<Vec<_>>()
        .join("+")
}
