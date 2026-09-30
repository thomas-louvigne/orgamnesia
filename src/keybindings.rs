use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// An action maps to a list of bindings (0, 1 or 2 accelerators).
pub type Bindings = BTreeMap<String, Vec<String>>;

fn default_preset() -> String { "classic".to_string() }

/// A user-made shortcut profile. The built-in ones ("classic", "emacs") are not stored.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Profile {
    pub id: String,
    pub name: String,
    /// Built-in preset this profile derives from (decides which editor actions exist).
    pub editor_preset: String,
    pub app: Bindings,
    pub editor: Bindings,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Keybindings {
    /// Base preset of the active profile ("classic" | "emacs").
    #[serde(default = "default_preset")]
    pub editor_preset: String,
    /// Application-level shortcuts of the active profile (action id -> bindings).
    #[serde(default)]
    pub app: Bindings,
    /// Editor shortcuts of the active profile (action id -> bindings).
    #[serde(default)]
    pub editor: Bindings,
    /// Id of the active profile: a built-in preset id or a custom `Profile::id`.
    #[serde(default)]
    pub active_profile: String,
    /// Custom profiles.
    #[serde(default)]
    pub profiles: Vec<Profile>,
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
            active_profile: default_preset(),
            profiles: vec![],
        }
    }

    /// Id of the active profile, falling back to the base preset for files
    /// written before profiles existed.
    pub fn resolved_active(&self) -> String {
        if is_builtin(&self.active_profile)
            || self.profiles.iter().any(|p| p.id == self.active_profile)
        {
            self.active_profile.clone()
        } else {
            self.editor_preset.clone()
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
    ActionDef { id: "split_vertical",   label_key: "shortcut_split_vertical" },
    ActionDef { id: "split_horizontal", label_key: "shortcut_split_horizontal" },
    ActionDef { id: "close_split",      label_key: "shortcut_close_split" },
    ActionDef { id: "single_window",    label_key: "shortcut_single_window" },
    ActionDef { id: "other_window",     label_key: "shortcut_other_window" },
];

/// Every editor action, whatever the profile: a profile simply leaves the ones
/// it doesn't use unbound.
pub const EDITOR_ACTIONS: &[ActionDef] = &[
    ActionDef { id: "copy",                label_key: "shortcut_copy" },
    ActionDef { id: "cut",                 label_key: "shortcut_cut" },
    ActionDef { id: "paste",               label_key: "shortcut_paste" },
    ActionDef { id: "select_all",          label_key: "shortcut_select_all" },
    ActionDef { id: "undo",                label_key: "shortcut_undo_edit" },
    ActionDef { id: "redo",                label_key: "shortcut_redo" },
    ActionDef { id: "new_heading",         label_key: "shortcut_new_heading" },
    ActionDef { id: "beginning_of_line",   label_key: "shortcut_bol" },
    ActionDef { id: "end_of_line",         label_key: "shortcut_eol" },
    ActionDef { id: "kill_line",           label_key: "shortcut_kill_line" },
    ActionDef { id: "kill_word_forward",   label_key: "shortcut_kill_word_fwd" },
    ActionDef { id: "kill_word_backward",  label_key: "shortcut_kill_word_bwd" },
    ActionDef { id: "yank",                label_key: "shortcut_yank" },
    ActionDef { id: "kill_region",         label_key: "shortcut_kill_region" },
    ActionDef { id: "copy_region",         label_key: "shortcut_copy_region" },
    ActionDef { id: "delete_char_forward", label_key: "shortcut_del_char_fwd" },
    ActionDef { id: "forward_char",        label_key: "shortcut_forward_char" },
    ActionDef { id: "backward_char",       label_key: "shortcut_backward_char" },
    ActionDef { id: "next_line",           label_key: "shortcut_next_line" },
    ActionDef { id: "previous_line",       label_key: "shortcut_previous_line" },
    ActionDef { id: "forward_word",        label_key: "shortcut_forward_word" },
    ActionDef { id: "backward_word",       label_key: "shortcut_backward_word" },
    ActionDef { id: "beginning_of_buffer", label_key: "shortcut_bob" },
    ActionDef { id: "end_of_buffer",       label_key: "shortcut_eob" },
    ActionDef { id: "scroll_down",         label_key: "shortcut_scroll_down" },
    ActionDef { id: "scroll_up",           label_key: "shortcut_scroll_up" },
    ActionDef { id: "backward_sentence",   label_key: "shortcut_backward_sentence" },
    ActionDef { id: "forward_sentence",    label_key: "shortcut_forward_sentence" },
    ActionDef { id: "backward_paragraph",  label_key: "shortcut_backward_paragraph" },
    ActionDef { id: "forward_paragraph",   label_key: "shortcut_forward_paragraph" },
    ActionDef { id: "back_to_indentation", label_key: "shortcut_back_to_indentation" },
    ActionDef { id: "previous_heading",    label_key: "shortcut_previous_heading" },
    ActionDef { id: "next_heading",        label_key: "shortcut_next_heading" },
    ActionDef { id: "open_link",           label_key: "shortcut_open_link" },
    ActionDef { id: "set_mark",            label_key: "shortcut_set_mark" },
    ActionDef { id: "keyboard_quit",       label_key: "shortcut_keyboard_quit" },
];

/// Preset ids available in the UI (VI intentionally left as a future addition).
pub const EDITOR_PRESETS: &[&str] = &["classic", "emacs"];

pub fn is_builtin(id: &str) -> bool {
    EDITOR_PRESETS.contains(&id)
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
        // Editor panes. `ctrl+k` is taken by quick open, so no `ctrl+k …` chords here.
        ("split_vertical",   &["ctrl+alt+v"]),
        ("split_horizontal", &["ctrl+alt+h"]),
        ("close_split",      &["ctrl+alt+w"]),
        ("single_window",    &["ctrl+alt+o"]),
        ("other_window",     &["ctrl+o", "F6"]),
    ])
}

/// Add the shortcuts of `base` for every action a stored profile doesn't mention
/// yet (actions introduced after the profile was saved). Actions the user cleared
/// are still present as empty entries, so they are left alone.
pub fn fill_missing(app: &mut Bindings, editor: &mut Bindings, base: &str) {
    for (k, v) in preset_app(base) { app.entry(k).or_insert(v); }
    for (k, v) in preset_editor(base) { editor.entry(k).or_insert(v); }
}

/// Default application bindings for a preset. Emacs uses its own `C-x` chords
/// (find-file, save-buffer, switch-to-buffer, kill-buffer, next/previous-buffer).
pub fn preset_app(preset: &str) -> Bindings {
    match preset {
        "emacs" => m(&[
            ("new_page",      &["ctrl+x ctrl+f"]),
            ("save",          &["ctrl+x ctrl+s"]),
            ("open_settings", &["ctrl+,"]),
            ("quick_open",    &["ctrl+x b", "alt+x"]),
            ("close_tab",     &["ctrl+x k"]),
            ("next_tab",      &["ctrl+x ArrowRight", "ctrl+Tab"]),
            ("prev_tab",      &["ctrl+x ArrowLeft", "ctrl+shift+Tab"]),
            // Digits need Shift on an AZERTY keyboard, hence the second variant
            ("split_vertical",   &["ctrl+x 3", "ctrl+x shift+3"]),
            ("split_horizontal", &["ctrl+x 2", "ctrl+x shift+2"]),
            ("close_split",      &["ctrl+x 0", "ctrl+x shift+0"]),
            ("single_window",    &["ctrl+x 1", "ctrl+x shift+1"]),
            ("other_window",     &["ctrl+x o", "ctrl+o"]),
        ]),
        _ => default_app(),
    }
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
            ("open_link",   &["ctrl+Enter"]),
        ]),
    }
}

// ─── Matching / capture / display ─────────────────────────────────────────────

// A binding is one key combination ("ctrl+s") or a chord of several, separated
// by spaces ("ctrl+x ctrl+s"): the keys are pressed one after the other.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Scope { App, Editor }

#[derive(Debug, Clone, PartialEq)]
pub enum Resolution {
    /// The key (or completed chord) triggers this action.
    Action(Scope, String),
    /// First key(s) of a chord: waiting for the next one. Carries the keys so far.
    Pending(String),
    /// A chord was started and the key that followed matches nothing, or it was cancelled.
    Aborted,
    /// Not a shortcut.
    None,
}

/// A chord is dropped when its next key takes longer than this (ms).
const CHORD_TIMEOUT_MS: f64 = 4000.0;

#[derive(Default)]
struct ChordState {
    pending: Vec<String>,
    at: f64,
    /// Last event resolved, so the editor and the global handler, which both see
    /// the same event, don't advance a chord twice.
    last_ts: f64,
    last_key: String,
    last: Option<Resolution>,
}

thread_local! {
    static CHORD: std::cell::RefCell<ChordState> = Default::default();
}

/// Resolve a key press against the application and editor bindings, tracking
/// multi-key chords. Safe to call several times for the same event.
pub fn resolve(kb: &Keybindings, event: &web_sys::KeyboardEvent) -> Resolution {
    let ts = event.time_stamp();
    let key = event.key();
    CHORD.with(|c| {
        let mut c = c.borrow_mut();
        if c.last_ts == ts && c.last_key == key {
            if let Some(r) = &c.last { return r.clone(); }
        }
        let res = resolve_step(&mut c, kb, event, ts);
        c.last_ts = ts;
        c.last_key = key;
        c.last = Some(res.clone());
        res
    })
}

fn resolve_step(c: &mut ChordState, kb: &Keybindings, event: &web_sys::KeyboardEvent, ts: f64) -> Resolution {
    // A bare modifier is just the start of a combination
    let Some(token) = capture_from_event(event) else { return Resolution::None };
    if !c.pending.is_empty() && ts - c.at > CHORD_TIMEOUT_MS {
        c.pending.clear();
    }
    if !c.pending.is_empty() && token == "Escape" {
        c.pending.clear();
        return Resolution::Aborted;
    }
    let mut seq = c.pending.clone();
    seq.push(token);
    let seq_str = seq.join(" ");

    let tables = [(Scope::Editor, &kb.editor), (Scope::App, &kb.app)];
    let find = |seq_str: &str| -> Option<(Scope, String)> {
        for (scope, map) in tables {
            for (id, binds) in map {
                if binds.iter().any(|b| b == seq_str) {
                    return Some((scope, id.clone()));
                }
            }
        }
        None
    };
    if let Some((scope, id)) = find(&seq_str) {
        c.pending.clear();
        return Resolution::Action(scope, id);
    }
    // Emacs users often keep Ctrl held through a chord (`C-x C-o` for `C-x o`):
    // when the key after the first one matches nothing with Ctrl, try it without.
    if !c.pending.is_empty() {
        if let Some(bare) = seq.last().and_then(|t| t.strip_prefix("ctrl+")) {
            let relaxed = format!("{} {}", c.pending.join(" "), bare);
            if let Some((scope, id)) = find(&relaxed) {
                c.pending.clear();
                return Resolution::Action(scope, id);
            }
        }
    }
    let prefix = format!("{seq_str} ");
    let is_prefix = tables.iter().any(|(_, map)| {
        map.values().flatten().any(|b| b.starts_with(&prefix))
    });
    if is_prefix {
        c.pending = seq;
        c.at = ts;
        return Resolution::Pending(display(&seq_str));
    }
    let was_chord = !c.pending.is_empty();
    c.pending.clear();
    if was_chord { Resolution::Aborted } else { Resolution::None }
}

/// Run `f` after `ms` milliseconds.
pub fn after_ms(ms: i32, f: impl FnOnce() + 'static) {
    if let Some(w) = web_sys::window() {
        let cb = wasm_bindgen::closure::Closure::once_into_js(f);
        let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(
            wasm_bindgen::JsCast::unchecked_ref(&cb), ms);
    }
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
    let ev_key_norm = match ev_key.as_str() {
        " " => "space".to_string(),
        k if k.len() == 1 => k.to_lowercase(),
        _ => ev_key,
    };
    event.ctrl_key()  == need_ctrl
        && event.shift_key() == need_shift
        && event.alt_key()   == need_alt
        && ev_key_norm == *key
}

/// Best-effort key name from a physical `KeyboardEvent.code` ("KeyA" -> "a", "Minus" -> "-").
fn key_from_code(code: &str) -> Option<String> {
    if let Some(l) = code.strip_prefix("Key") { return Some(l.to_lowercase()); }
    if let Some(d) = code.strip_prefix("Digit") { return Some(d.to_string()); }
    Some(match code {
        "Space" => "space", "Minus" => "-", "Equal" => "=", "Comma" => ",", "Period" => ".", "Slash" => "/",
        "Semicolon" => ";", "Quote" => "'", "Backquote" => "`", "Backslash" => "\\",
        "BracketLeft" => "[", "BracketRight" => "]",
        _ => return None,
    }.to_string())
}

/// Builds a binding string from a KeyboardEvent (returns None for bare modifiers).
pub fn capture_from_event(event: &web_sys::KeyboardEvent) -> Option<String> {
    let mut key = event.key();
    if matches!(key.as_str(), "Control" | "Shift" | "Alt" | "Meta") {
        return None;
    }
    // Some layouts/input methods report no usable `key` while a modifier is held:
    // fall back to the physical key.
    if matches!(key.as_str(), "Unidentified" | "Dead") {
        key = key_from_code(&event.code())?;
    }
    let mut parts: Vec<String> = Vec::new();
    if event.ctrl_key()  { parts.push("ctrl".into()); }
    if event.shift_key() { parts.push("shift".into()); }
    if event.alt_key()   { parts.push("alt".into()); }
    let key_part = match key.as_str() {
        " " => "space".to_string(),
        k if k.len() == 1 => k.to_lowercase(),
        _ => key,
    };
    parts.push(key_part);
    Some(parts.join("+"))
}

/// Returns a human-readable label ("ctrl+k" → "Ctrl+K", "ctrl+x ctrl+s" → "Ctrl+X Ctrl+S").
pub fn display(binding: &str) -> String {
    if binding.is_empty() { return "—".into(); }
    binding.split_whitespace().map(display_combo).collect::<Vec<_>>().join(" ")
}

fn display_combo(binding: &str) -> String {
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
