//! The shortcuts of the window in the terminal: a key pressed written as the
//! keybinding profiles write it (`ctrl+shift+k`, see `capture_from_event` in
//! `src/keybindings.rs`), and the chords (`ctrl+x ctrl+f`) followed as there.

use orgamnesia_core::keybindings::Keybindings;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// What a key does with the shortcuts.
#[derive(Debug, PartialEq)]
pub enum Resolution {
    /// The key (or the chord it ends) is the shortcut of this action.
    Action(String),
    /// The first keys of a chord, as shown: waiting for the next one.
    Pending(String),
    /// A chord begun, the key after it matching nothing (or Escape).
    Aborted,
    /// Not a shortcut.
    None,
}

/// The key as a binding writes it; `None` for a key no binding names.
pub fn token(key: &KeyEvent) -> Option<String> {
    let name = match key.code {
        KeyCode::Char(' ') => "space".into(),
        KeyCode::Char(c) => c.to_lowercase().to_string(),
        KeyCode::Enter => "Enter".into(),
        KeyCode::Tab | KeyCode::BackTab => "Tab".into(),
        KeyCode::Esc => "Escape".into(),
        KeyCode::Backspace => "Backspace".into(),
        KeyCode::Delete => "Delete".into(),
        KeyCode::Left => "ArrowLeft".into(),
        KeyCode::Right => "ArrowRight".into(),
        KeyCode::Up => "ArrowUp".into(),
        KeyCode::Down => "ArrowDown".into(),
        KeyCode::Home => "Home".into(),
        KeyCode::End => "End".into(),
        KeyCode::PageUp => "PageUp".into(),
        KeyCode::PageDown => "PageDown".into(),
        KeyCode::F(n) => format!("F{n}"),
        _ => return None,
    };
    let m = key.modifiers;
    let shift = m.contains(KeyModifiers::SHIFT) || key.code == KeyCode::BackTab
        || matches!(key.code, KeyCode::Char(c) if c.is_uppercase());
    let mut parts = vec![];
    if m.contains(KeyModifiers::CONTROL) { parts.push("ctrl"); }
    if shift { parts.push("shift"); }
    if m.contains(KeyModifiers::ALT) { parts.push("alt"); }
    parts.push(&name);
    Some(parts.join("+"))
}

/// Whether `key` is one of the user's shortcuts of the action `id` (a key, not a chord).
pub fn bound(kb: &Keybindings, key: &KeyEvent, id: &str) -> bool {
    token(key).is_some_and(|t| kb.app.get(id).into_iter().chain(kb.editor.get(id)).flatten().any(|b| *b == t))
}

/// `ctrl+x ctrl+s` → `Ctrl+X Ctrl+S`.
pub fn display(binding: &str) -> String {
    binding.split(' ').map(|key| key.split('+').map(|p| match p {
        "ctrl" => "Ctrl".to_string(),
        "shift" => "Shift".to_string(),
        "alt" => "Alt".to_string(),
        "ArrowLeft" => "←".to_string(),
        "ArrowRight" => "→".to_string(),
        p if p.chars().count() == 1 => p.to_uppercase(),
        p => p.to_string(),
    }).collect::<Vec<_>>().join("+")).collect::<Vec<_>>().join(" ")
}

/// The keys of a chord pressed so far.
#[derive(Default)]
pub struct Chord(Vec<String>);

impl Chord {
    /// `token` pressed, against the shortcuts of `kb` (the window's and the editor's).
    pub fn step(&mut self, kb: &Keybindings, token: String) -> Resolution {
        if !self.0.is_empty() && token == "Escape" {
            self.0.clear();
            return Resolution::Aborted;
        }
        // No editor here: the window's shortcuts first
        let tables = [&kb.app, &kb.editor];
        let find = |seq: &str| tables.iter().find_map(|map| map.iter().find(|(_, b)| b.iter().any(|b| b == seq)).map(|(id, _)| id.clone()));
        let mut seq = self.0.clone();
        seq.push(token);
        let seq_str = seq.join(" ");
        // Emacs users keep Ctrl held through a chord (`C-x C-o` for `C-x o`)
        let relaxed = (!self.0.is_empty()).then(|| seq.last().and_then(|t| t.strip_prefix("ctrl+")).map(|bare| format!("{} {bare}", self.0.join(" ")))).flatten();
        if let Some(id) = find(&seq_str).or_else(|| relaxed.and_then(|r| find(&r))) {
            self.0.clear();
            return Resolution::Action(id);
        }
        let prefix = format!("{seq_str} ");
        if tables.iter().any(|map| map.values().flatten().any(|b| b.starts_with(&prefix))) {
            self.0 = seq;
            return Resolution::Pending(display(&seq_str));
        }
        let was_chord = !self.0.is_empty();
        self.0.clear();
        if was_chord { return Resolution::Aborted; }
        // Most terminals send Ctrl+Shift+J as Ctrl+J: the user's Ctrl+Shift binding then
        if let Some(id) = seq_str.strip_prefix("ctrl+").filter(|r| !r.starts_with("shift+")).and_then(|r| find(&format!("ctrl+shift+{r}"))) {
            return Resolution::Action(id);
        }
        Resolution::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode, m: KeyModifiers) -> String { token(&KeyEvent::new(code, m)).unwrap() }

    #[test]
    fn keys_as_bindings_write_them() {
        assert_eq!(key(KeyCode::Char('k'), KeyModifiers::CONTROL), "ctrl+k");
        assert_eq!(key(KeyCode::Char('J'), KeyModifiers::CONTROL | KeyModifiers::SHIFT), "ctrl+shift+j");
        assert_eq!(key(KeyCode::Left, KeyModifiers::ALT), "alt+ArrowLeft");
        assert_eq!(key(KeyCode::Enter, KeyModifiers::CONTROL), "ctrl+Enter");
        assert_eq!(display("ctrl+x ctrl+f"), "Ctrl+X Ctrl+F");
    }

    #[test]
    fn follows_chords_of_the_emacs_profile() {
        let kb = Keybindings::with_profiles(Some("emacs"), vec![]);
        let mut c = Chord::default();
        assert_eq!(c.step(&kb, "ctrl+x".into()), Resolution::Pending("Ctrl+X".into()));
        assert_eq!(c.step(&kb, "ctrl+f".into()), Resolution::Action("new_page".into()));
        assert_eq!(c.step(&kb, "ctrl+c".into()), Resolution::Pending("Ctrl+C".into()));
        assert_eq!(c.step(&kb, "ctrl+o".into()), Resolution::Action("open_link".into()));
        assert_eq!(c.step(&kb, "ctrl+x".into()), Resolution::Pending("Ctrl+X".into()));
        assert_eq!(c.step(&kb, "q".into()), Resolution::Aborted);
        assert_eq!(c.step(&kb, "a".into()), Resolution::None);
        let classic = Keybindings::with_profiles(None, vec![]);
        assert_eq!(c.step(&classic, "ctrl+shift+j".into()), Resolution::Action("journal".into()));
        // The terminal's Ctrl+J for it; nothing when the journal is bound elsewhere
        assert_eq!(c.step(&classic, "ctrl+j".into()), Resolution::Action("journal".into()));
        assert_eq!(c.step(&kb, "ctrl+j".into()), Resolution::None);
    }
}
