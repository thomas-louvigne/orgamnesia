pub use orgamnesia_core::keybindings::*;

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
