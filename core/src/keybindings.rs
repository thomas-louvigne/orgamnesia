use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// An action maps to a list of bindings (0, 1 or 2 accelerators).
pub type Bindings = BTreeMap<String, Vec<String>>;

pub fn default_preset() -> String { "classic".to_string() }

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

/// Name of a profile of the user's own when it has none (the French of the app's
/// default language, as the settings window names it).
pub const DEFAULT_PROFILE_NAME: &str = "Personnalisé";

/// `keybindings/<name>.json`: a profile of the user's own, named after its file.
/// Which profile is active is a setting (`keybinding_profile`); the built-in
/// profiles are in the code.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProfileFile {
    /// Built-in preset it derives from (decides which editor actions exist).
    pub editor_preset: String,
    #[serde(default)]
    pub app: Bindings,
    #[serde(default)]
    pub editor: Bindings,
}

/// `name` as a file name: without `/` nor `\`, not hidden, not empty.
fn file_name(name: &str) -> String {
    let name: String = name.trim().chars().map(|c| if matches!(c, '/' | '\\') { '-' } else { c }).collect();
    let name = name.trim_start_matches('.').trim().to_string();
    if name.is_empty() { DEFAULT_PROFILE_NAME.to_string() } else { name }
}

impl Keybindings {
    /// The keybindings with the user's `profiles`, `active` the active one
    /// (built-in or one of them): its shortcuts in effect. A profile that is not
    /// there gives way to the default one.
    pub fn with_profiles(active: Option<&str>, profiles: Vec<Profile>) -> Self {
        let profiles: Vec<Profile> = profiles.into_iter().filter(|p| !is_builtin(&p.id)).collect();
        let wanted = active.unwrap_or_default();
        let (active, preset, app, editor) = match profiles.iter().find(|p| p.id == wanted) {
            Some(p) => (p.id.clone(), p.editor_preset.clone(), p.app.clone(), p.editor.clone()),
            None => {
                let id = if is_builtin(wanted) { wanted.to_string() } else { default_preset() };
                (id.clone(), id.clone(), preset_app(&id), preset_editor(&id))
            }
        };
        let mut kb = Self { editor_preset: preset, app, editor, active_profile: active, profiles };
        kb.upgrade();
        kb
    }

    /// The keybindings from their files: the user's profiles by name, `active`
    /// the name of the active one (the setting).
    pub fn from_files(active: Option<&str>, profiles: Vec<(String, ProfileFile)>) -> Self {
        let profiles = profiles.into_iter()
            .map(|(name, p)| Profile { id: name.clone(), name, editor_preset: p.editor_preset, app: p.app, editor: p.editor })
            .collect();
        Self::with_profiles(active, profiles)
    }

    /// The files of these keybindings: the user's own profiles, by the names of
    /// their files (their own names, made unique and fit for a file), the
    /// shortcuts of the active one as they are now; and the name of the active
    /// profile (the setting).
    pub fn to_files(&self) -> (String, Vec<(String, ProfileFile)>) {
        let active = self.resolved_active();
        let mut active_name = active.clone();
        let mut taken: Vec<String> = EDITOR_PRESETS.iter().map(|s| s.to_string()).collect();
        let mut files = vec![];
        for p in self.profiles.iter().filter(|p| !is_builtin(&p.id)) {
            let base = file_name(&p.name);
            let mut name = base.clone();
            let mut n = 2;
            while taken.iter().any(|t| t.to_lowercase() == name.to_lowercase()) {
                name = format!("{base} {n}");
                n += 1;
            }
            taken.push(name.clone());
            if p.id == active { active_name = name.clone(); }
            let (app, editor) = if p.id == active { (self.app.clone(), self.editor.clone()) } else { (p.app.clone(), p.editor.clone()) };
            files.push((name, ProfileFile { editor_preset: p.editor_preset.clone(), app, editor }));
        }
        (active_name, files)
    }

    /// `text`, a `keybindings.json` of an older version (the profiles in it, the
    /// active one, its shortcuts at the top): the keybindings it held. Shortcuts
    /// of a file older than the profiles that are not those of its preset become
    /// a profile of their own. `None` when it holds no shortcuts.
    pub fn from_old(text: &str) -> Option<Self> {
        let map = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(text).ok()?;
        if !["app", "editor", "profiles", "editor_preset"].iter().any(|k| map.contains_key(*k)) { return None; }
        let mut kb = serde_json::from_str::<Keybindings>(text).ok()?;
        let base = kb.editor_preset.clone();
        if kb.active_profile.is_empty() {
            let same = kb.app == preset_app(&base) && kb.editor == preset_editor(&base);
            kb.active_profile = if same { base.clone() } else { "custom".to_string() };
            if !same && !kb.profiles.iter().any(|p| p.id == "custom") {
                kb.profiles.push(Profile {
                    id: "custom".into(), name: DEFAULT_PROFILE_NAME.into(), editor_preset: base,
                    app: kb.app.clone(), editor: kb.editor.clone(),
                });
            }
        }
        // The active profile's shortcuts are those at the top
        let active = kb.resolved_active();
        let (app, editor) = (kb.app.clone(), kb.editor.clone());
        if let Some(p) = kb.profiles.iter_mut().find(|p| p.id == active) {
            p.app = app;
            p.editor = editor;
        }
        Some(Self::with_profiles(Some(&active), kb.profiles))
    }
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
    NewPage, Save, OpenSettings, ShowShortcuts, QuickOpen, ProjectSearch, ProjectReplace, AddTag, CloseTab, NextTab, PrevTab, PageBack, PageForward,
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
    BeginningOfBuffer, EndOfBuffer, ScrollDown, ScrollUp, Recenter,
    BackwardSentence, ForwardSentence, BackwardParagraph, ForwardParagraph, BackwardElement, ForwardElement,
    BackToIndentation, PreviousHeading, NextHeading, BackwardHeading, ForwardHeading, UpHeading, OpenLink, SetMark, KeyboardQuit,
    Find, FindPrevious, Replace, TableNextCell, TablePrevCell, GlobalCycle,
    InsertLink, InsertDate, InsertInactiveDate,
    Bold, Italic, Underline, Strikethrough, TodoNext, TodoPrev,
    PromoteSubtree, DemoteSubtree, MoveLineUp, MoveLineDown, SpellNext,
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
    ActionDef { action: AppAction::ShowShortcuts, id: "show_shortcuts", label_key: "shortcut_show_shortcuts" },
    ActionDef { action: AppAction::QuickOpen, id: "quick_open",    label_key: "shortcut_quick_open" },
    ActionDef { action: AppAction::ProjectSearch, id: "project_search",   label_key: "shortcut_project_search" },
    ActionDef { action: AppAction::ProjectReplace, id: "project_replace", label_key: "shortcut_project_replace" },
    ActionDef { action: AppAction::AddTag, id: "add_tag",                 label_key: "shortcut_add_tag" },
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
    ActionDef { action: EditorAction::Recenter, id: "recenter",            label_key: "shortcut_recenter" },
    ActionDef { action: EditorAction::BackwardSentence, id: "backward_sentence",   label_key: "shortcut_backward_sentence" },
    ActionDef { action: EditorAction::ForwardSentence, id: "forward_sentence",    label_key: "shortcut_forward_sentence" },
    ActionDef { action: EditorAction::BackwardParagraph, id: "backward_paragraph",  label_key: "shortcut_backward_paragraph" },
    ActionDef { action: EditorAction::ForwardParagraph, id: "forward_paragraph",   label_key: "shortcut_forward_paragraph" },
    ActionDef { action: EditorAction::BackToIndentation, id: "back_to_indentation", label_key: "shortcut_back_to_indentation" },
    ActionDef { action: EditorAction::PreviousHeading, id: "previous_heading",    label_key: "shortcut_previous_heading" },
    ActionDef { action: EditorAction::NextHeading, id: "next_heading",        label_key: "shortcut_next_heading" },
    ActionDef { action: EditorAction::BackwardHeading, id: "backward_heading", label_key: "shortcut_backward_heading" },
    ActionDef { action: EditorAction::ForwardHeading, id: "forward_heading",   label_key: "shortcut_forward_heading" },
    ActionDef { action: EditorAction::BackwardElement, id: "backward_element", label_key: "shortcut_backward_element" },
    ActionDef { action: EditorAction::ForwardElement, id: "forward_element",   label_key: "shortcut_forward_element" },
    ActionDef { action: EditorAction::UpHeading, id: "up_heading",             label_key: "shortcut_up_heading" },
    ActionDef { action: EditorAction::OpenLink, id: "open_link",           label_key: "shortcut_open_link" },
    ActionDef { action: EditorAction::SetMark, id: "set_mark",            label_key: "shortcut_set_mark" },
    ActionDef { action: EditorAction::KeyboardQuit, id: "keyboard_quit",       label_key: "shortcut_keyboard_quit" },
    ActionDef { action: EditorAction::Find, id: "find",                label_key: "shortcut_find" },
    ActionDef { action: EditorAction::FindPrevious, id: "find_previous",       label_key: "shortcut_find_previous" },
    ActionDef { action: EditorAction::Replace, id: "replace",             label_key: "shortcut_replace" },
    ActionDef { action: EditorAction::TableNextCell, id: "table_next_cell",     label_key: "shortcut_table_next_cell" },
    ActionDef { action: EditorAction::TablePrevCell, id: "table_prev_cell",     label_key: "shortcut_table_prev_cell" },
    ActionDef { action: EditorAction::GlobalCycle, id: "global_cycle",         label_key: "shortcut_global_cycle" },
    ActionDef { action: EditorAction::InsertLink, id: "insert_link",           label_key: "shortcut_insert_link" },
    ActionDef { action: EditorAction::InsertDate, id: "insert_date",           label_key: "shortcut_insert_date" },
    ActionDef { action: EditorAction::InsertInactiveDate, id: "insert_inactive_date", label_key: "shortcut_insert_inactive_date" },
    ActionDef { action: EditorAction::Bold, id: "bold",                label_key: "shortcut_bold" },
    ActionDef { action: EditorAction::Italic, id: "italic",              label_key: "shortcut_italic" },
    ActionDef { action: EditorAction::Underline, id: "underline",           label_key: "shortcut_underline" },
    ActionDef { action: EditorAction::Strikethrough, id: "strikethrough",       label_key: "shortcut_strikethrough" },
    ActionDef { action: EditorAction::TodoNext, id: "todo_next",           label_key: "shortcut_todo_next" },
    ActionDef { action: EditorAction::TodoPrev, id: "todo_prev",           label_key: "shortcut_todo_prev" },
    ActionDef { action: EditorAction::PromoteSubtree, id: "promote_subtree", label_key: "shortcut_promote_subtree" },
    ActionDef { action: EditorAction::DemoteSubtree, id: "demote_subtree",   label_key: "shortcut_demote_subtree" },
    ActionDef { action: EditorAction::MoveLineUp, id: "move_line_up",       label_key: "shortcut_move_line_up" },
    ActionDef { action: EditorAction::MoveLineDown, id: "move_line_down",   label_key: "shortcut_move_line_down" },
    ActionDef { action: EditorAction::SpellNext, id: "spell_next",       label_key: "shortcut_spell_next" },
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
        ("show_shortcuts", &["F1"]),
        ("quick_open",    &["ctrl+k", "ctrl+shift+k"]),
        ("project_search",  &["ctrl+shift+f"]),
        ("project_replace", &["ctrl+shift+h"]),
        ("add_tag",         &["ctrl+shift+t"]),
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

/// Combinations bound to more than one action, with the label keys of those actions.
pub type Conflicts = Vec<(String, Vec<&'static str>)>;

/// Application and editor shortcuts share one keyboard, so a combination may
/// only be given to a single action across both lists. A shortcut that is the
/// first key(s) of another action's chord ("ctrl+x" vs "ctrl+x ctrl+s") is a
/// conflict too: the chord could never be completed. Giving the same
/// combination to both slots of one action is fine.
pub fn conflicts(app: &Bindings, editor: &Bindings) -> Conflicts {
    // (binding, action label), one entry per distinct binding of each action
    let mut entries: Vec<(String, &'static str)> = Vec::new();
    let app_ids = APP_ACTIONS.iter().map(|a| (app, a.id, a.label_key));
    let editor_ids = EDITOR_ACTIONS.iter().map(|a| (editor, a.id, a.label_key));
    for (map, id, label_key) in app_ids.chain(editor_ids) {
        let mut seen = std::collections::HashSet::new();
        for b in map.get(id).into_iter().flatten().filter(|b| !b.is_empty()) {
            if seen.insert(b.clone()) {
                entries.push((b.clone(), label_key));
            }
        }
    }
    let mut by_combo: std::collections::BTreeMap<String, Vec<&'static str>> = Default::default();
    for (bind, label) in &entries {
        let chord_prefix = format!("{bind} ");
        for (other, other_label) in &entries {
            if other_label != label && (other == bind || other.starts_with(&chord_prefix)) {
                let labels = by_combo.entry(bind.clone()).or_default();
                for l in [label, other_label] {
                    if !labels.contains(l) { labels.push(l); }
                }
            }
        }
    }
    by_combo.into_iter().collect()
}

/// Default editor shortcuts a preset handed over to a new action:
/// (preset, shortcut, old action, new action).
const MOVED: &[(&str, &str, &str, &str)] = &[
    ("emacs", "shift+alt+{", "backward_paragraph", "backward_element"),
    ("emacs", "shift+alt+}", "forward_paragraph", "forward_element"),
];

/// Add the shortcuts of `base` for every action a stored profile doesn't mention
/// yet (actions introduced after the profile was saved). Actions the user cleared
/// are still present as empty entries, so they are left alone. A new shortcut the
/// profile already gives to another action (or that starts a chord of it, or the
/// reverse) is left out: a conflict would block saving the settings, unless the
/// preset itself moved that shortcut from the old action to the new one (`MOVED`).
pub fn fill_missing(app: &mut Bindings, editor: &mut Bindings, base: &str) {
    for &(_, key, from, to) in MOVED.iter().filter(|m| m.0 == base) {
        if editor.contains_key(to) { continue; }
        if let Some(v) = editor.get_mut(from) { v.retain(|b| b != key); }
    }
    let mut taken: Vec<String> = app.values().chain(editor.values()).flatten()
        .filter(|b| !b.is_empty()).cloned().collect();
    let mut fill = |map: &mut Bindings, preset: Bindings| {
        for (k, v) in preset {
            if map.contains_key(&k) { continue; }
            let free: Vec<String> = v.into_iter().filter(|b| !taken.iter().any(|t| clash(t, b))).collect();
            taken.extend(free.iter().cloned());
            map.insert(k, free);
        }
    };
    fill(app, preset_app(base));
    fill(editor, preset_editor(base));
}

/// Whether two shortcuts can't both be bound: the same, or one the first key(s) of the other.
fn clash(a: &str, b: &str) -> bool {
    a == b || a.starts_with(&format!("{b} ")) || b.starts_with(&format!("{a} "))
}

/// Default application bindings for a preset. Emacs uses its own `C-x` chords
/// (find-file, save-buffer, switch-to-buffer, kill-buffer, next/previous-buffer).
pub fn preset_app(preset: &str) -> Bindings {
    match preset {
        "emacs" => m(&[
            ("new_page",      &["ctrl+x ctrl+f"]),
            ("save",          &["ctrl+x ctrl+s"]),
            ("open_settings", &["ctrl+c ,"]),
            // describe-bindings
            ("show_shortcuts", &["F1", "ctrl+h b"]),
            ("quick_open",    &["ctrl+x b", "alt+x"]),
            // project.el: `C-x p g` (find regexp), `C-x p r` (query replace)
            ("project_search",  &["ctrl+x p g", "ctrl+shift+f"]),
            ("project_replace", &["ctrl+x p r", "ctrl+shift+h"]),
            // Org: `C-c C-q` (org-set-tags-command)
            ("add_tag",         &["ctrl+c ctrl+q"]),
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
            // org-meta-return: a new headline, or a new item on a list item
            ("new_heading",         &["alt+Enter"]),
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
            // recenter-top-bottom
            ("recenter",            &["ctrl+l"]),
            ("backward_sentence",   &["alt+a"]),
            ("forward_sentence",    &["alt+e"]),
            // org-backward/forward-paragraph; org-backward/forward-element
            ("backward_paragraph",  &["ctrl+ArrowUp"]),
            ("forward_paragraph",   &["ctrl+ArrowDown"]),
            ("backward_element",    &["shift+alt+{"]),
            ("forward_element",     &["shift+alt+}"]),
            ("back_to_indentation", &["alt+m"]),
            ("previous_heading",    &["ctrl+c ctrl+p"]),
            ("next_heading",        &["ctrl+c ctrl+n"]),
            // org-backward/forward-heading-same-level, outline-up-heading
            ("backward_heading",    &["ctrl+c ctrl+b"]),
            ("forward_heading",     &["ctrl+c ctrl+f"]),
            ("up_heading",          &["ctrl+c ctrl+u"]),
            ("set_mark",            &["ctrl+space"]),
            ("keyboard_quit",       &["ctrl+g"]),
            // isearch-forward / isearch-backward / query-replace
            ("find",                &["ctrl+s"]),
            ("find_previous",       &["ctrl+r"]),
            ("replace",             &["alt+%", "shift+alt+%"]),
            // org-cycle / org-shifttab: a table's cells, else folding
            ("table_next_cell",     &["Tab"]),
            ("table_prev_cell",     &["shift+Tab"]),
            // org-global-cycle
            ("global_cycle",        &["ctrl+u Tab"]),
            // org-insert-link, org-time-stamp, org-time-stamp-inactive. `.` and `!`
            // need Shift on some keyboards (AZERTY, QWERTY), hence the second variant.
            ("insert_link",         &["ctrl+c ctrl+l"]),
            ("insert_date",         &["ctrl+c .", "ctrl+c shift+."]),
            ("insert_inactive_date", &["ctrl+c !", "ctrl+c shift+!"]),
            // org-emphasize (`C-c C-x C-f`) then the marker. Some markers need Shift,
            // depending on the keyboard (QWERTY / AZERTY), hence the second variant.
            ("bold",                &["ctrl+c ctrl+x ctrl+f *", "ctrl+c ctrl+x ctrl+f shift+*"]),
            ("italic",              &["ctrl+c ctrl+x ctrl+f /", "ctrl+c ctrl+x ctrl+f shift+/"]),
            ("underline",           &["ctrl+c ctrl+x ctrl+f _", "ctrl+c ctrl+x ctrl+f shift+_"]),
            ("strikethrough",       &["ctrl+c ctrl+x ctrl+f +", "ctrl+c ctrl+x ctrl+f shift++"]),
            // org-shiftright / org-shiftleft on a headline (elsewhere, Shift+arrows select)
            ("todo_next",           &["shift+ArrowRight", "ctrl+c ctrl+t"]),
            ("todo_prev",           &["shift+ArrowLeft"]),
            // org-promote-subtree / org-demote-subtree (M-S-<left> / M-S-<right>)
            ("promote_subtree",     &["shift+alt+ArrowLeft"]),
            ("demote_subtree",      &["shift+alt+ArrowRight"]),
            ("move_line_up",        &["alt+ArrowUp"]),
            ("move_line_down",      &["alt+ArrowDown"]),
            // flyspell-goto-next-error
            ("spell_next",          &["ctrl+,"]),
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
            // `ctrl+k`, the usual one, opens a page; `ctrl+;` inserts the date in spreadsheets
            ("insert_link",     &["ctrl+l"]),
            ("insert_date",     &["ctrl+;"]),
            ("bold",          &["ctrl+b"]),
            ("italic",        &["ctrl+i"]),
            ("underline",     &["ctrl+u"]),
            ("strikethrough", &["ctrl+shift+x"]),
            ("promote_subtree", &["shift+alt+ArrowLeft"]),
            ("demote_subtree",  &["shift+alt+ArrowRight"]),
            ("move_line_up",    &["alt+ArrowUp"]),
            ("move_line_down",  &["alt+ArrowDown"]),
            // Spelling, as in Word and LibreOffice
            ("spell_next",      &["F7"]),
        ]),
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_files_hold_profiles() {
        // A custom profile active: its shortcuts as they are in effect (at the top)
        let old = r#"{"editor_preset": "emacs", "active_profile": "c1",
            "app": {"save": ["ctrl+j"]}, "editor": {},
            "profiles": [{"id": "c1", "name": "Mine", "editor_preset": "emacs", "app": {}, "editor": {}}]}"#;
        let (active, profiles) = Keybindings::from_old(old).unwrap().to_files();
        assert_eq!((active.as_str(), profiles.len(), profiles[0].0.as_str()), ("Mine", 1, "Mine"));
        assert_eq!(profiles[0].1.app.get("save"), Some(&vec!["ctrl+j".to_string()]));
        let kb = Keybindings::from_files(Some(&active), profiles);
        assert_eq!(kb.app_binds("save"), &["ctrl+j".to_string()]);
        // Older than the profiles: shortcuts of its own become a profile
        let (active, profiles) = Keybindings::from_old(r#"{"editor_preset": "classic", "app": {"save": ["ctrl+j"]}, "editor": {}}"#).unwrap().to_files();
        assert_eq!((active.as_str(), profiles.len()), (DEFAULT_PROFILE_NAME, 1));
        assert_eq!(Keybindings::from_old(r#"{"active_profile": "emacs"}"#), None);
    }

    #[test]
    fn profile_files_are_named_after_the_profiles() {
        let profile = |id: &str, name: &str| Profile { id: id.into(), name: name.into(), editor_preset: "emacs".into(), app: Bindings::new(), editor: Bindings::new() };
        let kb = Keybindings::with_profiles(Some("b"), vec![profile("a", "Perso"), profile("b", "perso"), profile("c", "a/b"), profile("d", " "), profile("e", "emacs")]);
        let (active, files) = kb.to_files();
        let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["Perso", "perso 2", "a-b", DEFAULT_PROFILE_NAME, "emacs 2"]);
        assert_eq!(active, "perso 2");
    }

    #[test]
    fn a_built_in_profile_is_its_preset() {
        let kb = Keybindings::from_files(Some("emacs"), vec![]);
        assert_eq!((kb.app.clone(), kb.editor_preset.as_str()), (preset_app("emacs"), "emacs"));
        // A profile that is not there: the default one
        let kb = Keybindings::from_files(Some("gone"), vec![]);
        assert_eq!(kb.active_profile, "classic");
        assert_eq!(kb.to_files().0, "classic");
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

    #[test]
    fn fill_missing_leaves_out_shortcuts_already_taken() {
        // A profile saved when the settings were on `ctrl+,` in Emacs, before `spell_next` took it
        let mut app = m(&[("open_settings", &["ctrl+,"]), ("quit", &["ctrl+x ctrl+c"])]);
        let mut editor = Bindings::new();
        fill_missing(&mut app, &mut editor, "emacs");
        assert_eq!(app["open_settings"], vec!["ctrl+,"]);
        assert!(editor["spell_next"].is_empty());
        // Still filled when nothing is in the way
        assert_eq!(editor["find"], vec!["ctrl+s"]);
        assert!(!clash("ctrl+x", "ctrl+xb") && clash("ctrl+x", "ctrl+x ctrl+s"));
    }

    #[test]
    fn fill_missing_moves_shortcuts_the_preset_handed_over() {
        // An Emacs profile saved when Alt+{ / Alt+} were the paragraph motions
        let mut app = Bindings::new();
        let mut editor = m(&[("backward_paragraph", &["shift+alt+{", "ctrl+ArrowUp"])]);
        fill_missing(&mut app, &mut editor, "emacs");
        assert_eq!(editor["backward_element"], vec!["shift+alt+{"]);
        assert_eq!(editor["backward_paragraph"], vec!["ctrl+ArrowUp"]);
        // Left alone once the profile knows the new action
        let mut editor = m(&[("backward_paragraph", &["shift+alt+{"]), ("backward_element", &[])]);
        fill_missing(&mut app, &mut editor, "emacs");
        assert_eq!(editor["backward_paragraph"], vec!["shift+alt+{"]);
    }
}
