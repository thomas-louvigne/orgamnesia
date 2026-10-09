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

    /// Bring shortcuts read from the file up to date. A built-in profile is
    /// always its current definition, so improved defaults reach existing users.
    /// (Files written before profiles existed have no active profile and keep
    /// their bindings as they are.) Custom profiles get the shortcuts of their
    /// base preset for actions added since they were saved.
    pub fn upgrade(&mut self) {
        for p in self.profiles.iter_mut() {
            let base = p.editor_preset.clone();
            fill_missing(&mut p.app, &mut p.editor, &base);
            canonicalize(&mut p.app);
            canonicalize(&mut p.editor);
        }
        canonicalize(&mut self.app);
        canonicalize(&mut self.editor);
        if self.active_profile.is_empty() { return; }
        let active = self.resolved_active();
        if is_builtin(&active) {
            self.app = preset_app(&active);
            self.editor = preset_editor(&active);
            self.editor_preset = active.clone();
            self.active_profile = active;
        } else {
            let base = self.editor_preset.clone();
            fill_missing(&mut self.app, &mut self.editor, &base);
        }
    }

    /// Bindings for an app action (empty slice if unbound).
    pub fn app_binds(&self, id: &str) -> &[String] {
        self.app.get(id).map(|v| v.as_slice()).unwrap_or(&[])
    }
}

/// A binding with the modifiers of each key in the order key presses are
/// written in (`ctrl+shift+alt+key`), the only one they are matched with.
/// Bindings written otherwise ("alt+shift+<", once in the Emacs preset) never matched.
pub fn canonical(binding: &str) -> String {
    binding.split(' ').map(|combo| {
        let parts: Vec<&str> = combo.split('+').collect();
        // The key itself may be "+" ("ctrl+shift++"): the last part, or an empty one
        let (mods, key) = match parts.as_slice() {
            [mods @ .., "", ""] => (mods, "+"),
            [mods @ .., key] => (mods, *key),
            [] => return combo.to_string(),
        };
        let mut out: Vec<&str> = ["ctrl", "shift", "alt"].into_iter().filter(|m| mods.contains(m)).collect();
        out.push(key);
        out.join("+")
    }).collect::<Vec<_>>().join(" ")
}

fn canonicalize(map: &mut Bindings) {
    for binds in map.values_mut() {
        for b in binds.iter_mut() { *b = canonical(b); }
    }
}

// ─── Action registry (single source of truth for UI + presets) ────────────────

/// An action that can be given shortcuts: its typed value, the id stored in
/// `keybindings.json` and the i18n key of its label.
pub struct ActionDef<A: 'static> {
    pub action: A,
    pub id: &'static str,
    pub label_key: &'static str,
}

/// Application actions (handled outside the editor).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppAction {
    NewPage, Save, OpenSettings, QuickOpen, CloseTab, NextTab, PrevTab, PageBack, PageForward,
    SplitVertical, SplitHorizontal, CloseSplit, SingleWindow, OtherWindow, NextRegion, PrevRegion, Quit,
    GitCommit, GitPull, GitPush,
}

/// Editor actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorAction {
    Copy, Cut, Paste, SelectAll, Undo, Redo, NewHeading,
    BeginningOfLine, EndOfLine, KillLine, KillWordForward, KillWordBackward, Yank,
    KillRegion, CopyRegion, DeleteCharForward,
    ForwardChar, BackwardChar, NextLine, PreviousLine, ForwardWord, BackwardWord,
    BeginningOfBuffer, EndOfBuffer, ScrollDown, ScrollUp,
    BackwardSentence, ForwardSentence, BackwardParagraph, ForwardParagraph,
    BackToIndentation, PreviousHeading, NextHeading, OpenLink, SetMark, KeyboardQuit,
    Find, FindPrevious, Replace, TableNextCell, TablePrevCell,
    Bold, Italic, Underline, Strikethrough, TodoNext, TodoPrev,
    PromoteSubtree, DemoteSubtree,
}

impl AppAction {
    pub fn from_id(id: &str) -> Option<Self> {
        APP_ACTIONS.iter().find(|d| d.id == id).map(|d| d.action)
    }
}

impl EditorAction {
    pub fn from_id(id: &str) -> Option<Self> {
        EDITOR_ACTIONS.iter().find(|d| d.id == id).map(|d| d.action)
    }
}

pub const APP_ACTIONS: &[ActionDef<AppAction>] = &[
    ActionDef { action: AppAction::NewPage, id: "new_page",      label_key: "shortcut_new_page" },
    ActionDef { action: AppAction::Save, id: "save",          label_key: "shortcut_save" },
    ActionDef { action: AppAction::OpenSettings, id: "open_settings", label_key: "shortcut_settings" },
    ActionDef { action: AppAction::QuickOpen, id: "quick_open",    label_key: "shortcut_quick_open" },
    ActionDef { action: AppAction::CloseTab, id: "close_tab",     label_key: "shortcut_close_tab" },
    ActionDef { action: AppAction::NextTab, id: "next_tab",      label_key: "shortcut_next_tab" },
    ActionDef { action: AppAction::PrevTab, id: "prev_tab",      label_key: "shortcut_prev_tab" },
    ActionDef { action: AppAction::PageBack, id: "page_back",     label_key: "shortcut_page_back" },
    ActionDef { action: AppAction::PageForward, id: "page_forward", label_key: "shortcut_page_forward" },
    ActionDef { action: AppAction::SplitVertical, id: "split_vertical",   label_key: "shortcut_split_vertical" },
    ActionDef { action: AppAction::SplitHorizontal, id: "split_horizontal", label_key: "shortcut_split_horizontal" },
    ActionDef { action: AppAction::CloseSplit, id: "close_split",      label_key: "shortcut_close_split" },
    ActionDef { action: AppAction::SingleWindow, id: "single_window",    label_key: "shortcut_single_window" },
    ActionDef { action: AppAction::OtherWindow, id: "other_window",     label_key: "shortcut_other_window" },
    ActionDef { action: AppAction::NextRegion, id: "next_region",      label_key: "shortcut_next_region" },
    ActionDef { action: AppAction::PrevRegion, id: "prev_region",      label_key: "shortcut_prev_region" },
    ActionDef { action: AppAction::Quit, id: "quit",             label_key: "shortcut_quit" },
    ActionDef { action: AppAction::GitCommit, id: "git_commit",  label_key: "shortcut_git_commit" },
    ActionDef { action: AppAction::GitPull, id: "git_pull",      label_key: "shortcut_git_pull" },
    ActionDef { action: AppAction::GitPush, id: "git_push",      label_key: "shortcut_git_push" },
];

/// Every editor action, whatever the profile: a profile simply leaves the ones
/// it doesn't use unbound.
pub const EDITOR_ACTIONS: &[ActionDef<EditorAction>] = &[
    ActionDef { action: EditorAction::Copy, id: "copy",                label_key: "shortcut_copy" },
    ActionDef { action: EditorAction::Cut, id: "cut",                 label_key: "shortcut_cut" },
    ActionDef { action: EditorAction::Paste, id: "paste",               label_key: "shortcut_paste" },
    ActionDef { action: EditorAction::SelectAll, id: "select_all",          label_key: "shortcut_select_all" },
    ActionDef { action: EditorAction::Undo, id: "undo",                label_key: "shortcut_undo_edit" },
    ActionDef { action: EditorAction::Redo, id: "redo",                label_key: "shortcut_redo" },
    ActionDef { action: EditorAction::NewHeading, id: "new_heading",         label_key: "shortcut_new_heading" },
    ActionDef { action: EditorAction::BeginningOfLine, id: "beginning_of_line",   label_key: "shortcut_bol" },
    ActionDef { action: EditorAction::EndOfLine, id: "end_of_line",         label_key: "shortcut_eol" },
    ActionDef { action: EditorAction::KillLine, id: "kill_line",           label_key: "shortcut_kill_line" },
    ActionDef { action: EditorAction::KillWordForward, id: "kill_word_forward",   label_key: "shortcut_kill_word_fwd" },
    ActionDef { action: EditorAction::KillWordBackward, id: "kill_word_backward",  label_key: "shortcut_kill_word_bwd" },
    ActionDef { action: EditorAction::Yank, id: "yank",                label_key: "shortcut_yank" },
    ActionDef { action: EditorAction::KillRegion, id: "kill_region",         label_key: "shortcut_kill_region" },
    ActionDef { action: EditorAction::CopyRegion, id: "copy_region",         label_key: "shortcut_copy_region" },
    ActionDef { action: EditorAction::DeleteCharForward, id: "delete_char_forward", label_key: "shortcut_del_char_fwd" },
    ActionDef { action: EditorAction::ForwardChar, id: "forward_char",        label_key: "shortcut_forward_char" },
    ActionDef { action: EditorAction::BackwardChar, id: "backward_char",       label_key: "shortcut_backward_char" },
    ActionDef { action: EditorAction::NextLine, id: "next_line",           label_key: "shortcut_next_line" },
    ActionDef { action: EditorAction::PreviousLine, id: "previous_line",       label_key: "shortcut_previous_line" },
    ActionDef { action: EditorAction::ForwardWord, id: "forward_word",        label_key: "shortcut_forward_word" },
    ActionDef { action: EditorAction::BackwardWord, id: "backward_word",       label_key: "shortcut_backward_word" },
    ActionDef { action: EditorAction::BeginningOfBuffer, id: "beginning_of_buffer", label_key: "shortcut_bob" },
    ActionDef { action: EditorAction::EndOfBuffer, id: "end_of_buffer",       label_key: "shortcut_eob" },
    ActionDef { action: EditorAction::ScrollDown, id: "scroll_down",         label_key: "shortcut_scroll_down" },
    ActionDef { action: EditorAction::ScrollUp, id: "scroll_up",           label_key: "shortcut_scroll_up" },
    ActionDef { action: EditorAction::BackwardSentence, id: "backward_sentence",   label_key: "shortcut_backward_sentence" },
    ActionDef { action: EditorAction::ForwardSentence, id: "forward_sentence",    label_key: "shortcut_forward_sentence" },
    ActionDef { action: EditorAction::BackwardParagraph, id: "backward_paragraph",  label_key: "shortcut_backward_paragraph" },
    ActionDef { action: EditorAction::ForwardParagraph, id: "forward_paragraph",   label_key: "shortcut_forward_paragraph" },
    ActionDef { action: EditorAction::BackToIndentation, id: "back_to_indentation", label_key: "shortcut_back_to_indentation" },
    ActionDef { action: EditorAction::PreviousHeading, id: "previous_heading",    label_key: "shortcut_previous_heading" },
    ActionDef { action: EditorAction::NextHeading, id: "next_heading",        label_key: "shortcut_next_heading" },
    ActionDef { action: EditorAction::OpenLink, id: "open_link",           label_key: "shortcut_open_link" },
    ActionDef { action: EditorAction::SetMark, id: "set_mark",            label_key: "shortcut_set_mark" },
    ActionDef { action: EditorAction::KeyboardQuit, id: "keyboard_quit",       label_key: "shortcut_keyboard_quit" },
    ActionDef { action: EditorAction::Find, id: "find",                label_key: "shortcut_find" },
    ActionDef { action: EditorAction::FindPrevious, id: "find_previous",       label_key: "shortcut_find_previous" },
    ActionDef { action: EditorAction::Replace, id: "replace",             label_key: "shortcut_replace" },
    ActionDef { action: EditorAction::TableNextCell, id: "table_next_cell",     label_key: "shortcut_table_next_cell" },
    ActionDef { action: EditorAction::TablePrevCell, id: "table_prev_cell",     label_key: "shortcut_table_prev_cell" },
    ActionDef { action: EditorAction::Bold, id: "bold",                label_key: "shortcut_bold" },
    ActionDef { action: EditorAction::Italic, id: "italic",              label_key: "shortcut_italic" },
    ActionDef { action: EditorAction::Underline, id: "underline",           label_key: "shortcut_underline" },
    ActionDef { action: EditorAction::Strikethrough, id: "strikethrough",       label_key: "shortcut_strikethrough" },
    ActionDef { action: EditorAction::TodoNext, id: "todo_next",           label_key: "shortcut_todo_next" },
    ActionDef { action: EditorAction::TodoPrev, id: "todo_prev",           label_key: "shortcut_todo_prev" },
    ActionDef { action: EditorAction::PromoteSubtree, id: "promote_subtree", label_key: "shortcut_promote_subtree" },
    ActionDef { action: EditorAction::DemoteSubtree, id: "demote_subtree",   label_key: "shortcut_demote_subtree" },
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
        ("page_back",     &["alt+ArrowLeft"]),
        ("page_forward",  &["alt+ArrowRight"]),
        // Editor panes. `ctrl+k` is taken by quick open, so no `ctrl+k …` chords here.
        ("split_vertical",   &["ctrl+alt+v"]),
        ("split_horizontal", &["ctrl+alt+h"]),
        ("close_split",      &["ctrl+alt+w"]),
        ("single_window",    &["ctrl+alt+o"]),
        ("other_window",     &["ctrl+o"]),
        // Pages, editor panes, side panel
        ("next_region",      &["F6"]),
        ("prev_region",      &["shift+F6"]),
        ("quit",             &["ctrl+q"]),
        ("git_commit",       &["ctrl+shift+g"]),
        ("git_pull",         &["ctrl+shift+u"]),
        ("git_push",         &["ctrl+shift+p"]),
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
            ("page_back",     &["alt+ArrowLeft"]),
            ("page_forward",  &["alt+ArrowRight"]),
            // Digits need Shift on an AZERTY keyboard, hence the second variant
            ("split_vertical",   &["ctrl+x 3", "ctrl+x shift+3"]),
            ("split_horizontal", &["ctrl+x 2", "ctrl+x shift+2"]),
            ("close_split",      &["ctrl+x 0", "ctrl+x shift+0"]),
            ("single_window",    &["ctrl+x 1", "ctrl+x shift+1"]),
            ("other_window",     &["ctrl+x o", "ctrl+o"]),
            ("next_region",      &["F6"]),
            ("prev_region",      &["shift+F6"]),
            ("quit",             &["ctrl+x ctrl+c"]),
            // Magit: `C-x g` (magit-status), then `c c` (commit), `F p` (pull), `P p` (push)
            ("git_commit",       &["ctrl+x g c c"]),
            ("git_pull",         &["ctrl+x g shift+f p"]),
            ("git_push",         &["ctrl+x g shift+p p"]),
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
            ("beginning_of_buffer", &["alt+<", "shift+alt+<"]),
            ("end_of_buffer",       &["alt+>", "shift+alt+>"]),
            ("scroll_down",         &["ctrl+v"]),
            ("scroll_up",           &["alt+v"]),
            ("backward_sentence",   &["alt+a"]),
            ("forward_sentence",    &["alt+e"]),
            ("backward_paragraph",  &["shift+alt+{", "ctrl+ArrowUp"]),
            ("forward_paragraph",   &["shift+alt+}", "ctrl+ArrowDown"]),
            ("back_to_indentation", &["alt+m"]),
            ("previous_heading",    &["ctrl+c ctrl+p"]),
            ("next_heading",        &["ctrl+c ctrl+n"]),
            ("set_mark",            &["ctrl+space"]),
            ("keyboard_quit",       &["ctrl+g"]),
            // isearch-forward / isearch-backward / query-replace
            ("find",                &["ctrl+s"]),
            ("find_previous",       &["ctrl+r"]),
            ("replace",             &["alt+%", "shift+alt+%"]),
            ("table_next_cell",     &["Tab"]),
            ("table_prev_cell",     &["shift+Tab"]),
            // org-emphasize (`C-c C-x C-f`) then the marker. Some markers need Shift,
            // depending on the keyboard (QWERTY / AZERTY), hence the second variant.
            ("bold",                &["ctrl+c ctrl+x ctrl+f *", "ctrl+c ctrl+x ctrl+f shift+*"]),
            ("italic",              &["ctrl+c ctrl+x ctrl+f /", "ctrl+c ctrl+x ctrl+f shift+/"]),
            ("underline",           &["ctrl+c ctrl+x ctrl+f _", "ctrl+c ctrl+x ctrl+f shift+_"]),
            ("strikethrough",       &["ctrl+c ctrl+x ctrl+f +", "ctrl+c ctrl+x ctrl+f shift++"]),
            // org-shiftright / org-shiftleft on a headline (elsewhere, Shift+arrows select)
            ("todo_next",           &["shift+ArrowRight"]),
            ("todo_prev",           &["shift+ArrowLeft"]),
            // org-promote-subtree / org-demote-subtree (M-S-<left> / M-S-<right>)
            ("promote_subtree",     &["shift+alt+ArrowLeft"]),
            ("demote_subtree",      &["shift+alt+ArrowRight"]),
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
            ("find",          &["ctrl+f", "F3"]),
            ("find_previous", &["shift+F3"]),
            ("replace",       &["ctrl+h"]),
            ("table_next_cell", &["Tab"]),
            ("table_prev_cell", &["shift+Tab"]),
            ("bold",          &["ctrl+b"]),
            ("italic",        &["ctrl+i"]),
            ("underline",     &["ctrl+u"]),
            ("strikethrough", &["ctrl+shift+x"]),
            ("promote_subtree", &["shift+alt+ArrowLeft"]),
            ("demote_subtree",  &["shift+alt+ArrowRight"]),
        ]),
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_ids_are_unique_and_round_trip() {
        for (i, d) in APP_ACTIONS.iter().enumerate() {
            assert!(APP_ACTIONS[i + 1..].iter().all(|o| o.id != d.id), "{}", d.id);
            assert_eq!(AppAction::from_id(d.id), Some(d.action));
        }
        for (i, d) in EDITOR_ACTIONS.iter().enumerate() {
            assert!(EDITOR_ACTIONS[i + 1..].iter().all(|o| o.id != d.id), "{}", d.id);
            assert_eq!(EditorAction::from_id(d.id), Some(d.action));
        }
        assert_eq!(AppAction::from_id("nope"), None);
    }

    #[test]
    fn presets_only_bind_known_actions() {
        for preset in EDITOR_PRESETS {
            for id in preset_app(preset).keys() {
                assert!(AppAction::from_id(id).is_some(), "{preset}: {id}");
            }
            for id in preset_editor(preset).keys() {
                assert!(EditorAction::from_id(id).is_some(), "{preset}: {id}");
            }
        }
    }

    #[test]
    fn preset_bindings_are_canonical() {
        for preset in EDITOR_PRESETS {
            for b in preset_app(preset).values().chain(preset_editor(preset).values()).flatten() {
                assert_eq!(&canonical(b), b, "{preset}");
            }
        }
        assert_eq!(canonical("alt+shift+<"), "shift+alt+<");
        assert_eq!(canonical("ctrl+c ctrl+x ctrl+f shift++"), "ctrl+c ctrl+x ctrl+f shift++");
        assert_eq!(canonical("alt+ctrl+x"), "ctrl+alt+x");

        let mut kb = Keybindings::defaults();
        kb.profiles.push(Profile {
            id: "c1".into(), name: "Mine".into(), editor_preset: "emacs".into(),
            app: Bindings::new(), editor: m(&[("replace", &["alt+shift+%"])]),
        });
        kb.upgrade();
        assert_eq!(kb.profiles[0].editor["replace"], vec!["shift+alt+%"]);
    }

    #[test]
    fn upgrade_resets_builtin_profile_and_fills_custom_ones() {
        let mut kb = Keybindings { active_profile: "emacs".into(), ..Keybindings::defaults() };
        kb.app.clear();
        kb.upgrade();
        assert_eq!(kb.app, preset_app("emacs"));
        assert_eq!(kb.editor_preset, "emacs");

        let mut custom = Keybindings::defaults();
        custom.profiles.push(Profile {
            id: "c1".into(), name: "Mine".into(), editor_preset: "classic".into(),
            app: Bindings::new(), editor: Bindings::new(),
        });
        custom.active_profile = "c1".into();
        custom.app = Bindings::new();
        custom.upgrade();
        assert_eq!(custom.profiles[0].app, preset_app("classic"));
        assert_eq!(custom.app, preset_app("classic"));
    }
}
