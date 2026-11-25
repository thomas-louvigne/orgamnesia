//! The built-in profiles' bindings (classic, emacs), the conflicts between
//! shortcuts, and the new actions' shortcuts added to an older profile.

use super::{Bindings, APP_ACTIONS, EDITOR_ACTIONS};

pub(super) fn m(pairs: &[(&str, &[&str])]) -> Bindings {
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
        ("git_history",      &["ctrl+shift+l"]),
        ("export_pdf",       &["ctrl+p"]),
        ("export_html",      &["ctrl+shift+e"]),
        ("graph",            &["ctrl+shift+m"]),
        ("switch_project",   &["ctrl+alt+p"]),
        ("journal",          &["ctrl+shift+j"]),
        ("random_page",      &["ctrl+alt+r"]),
        ("capture",          &["ctrl+alt+c"]),
        ("agenda",           &["ctrl+alt+a"]),
        ("journal_previous", &["ctrl+alt+PageUp"]),
        ("journal_next",     &["ctrl+alt+PageDown"]),
        ("zen_mode",         &["F11", "ctrl+alt+z"]),
        // Function keys free in both profiles (and in terminals)
        ("rename_page",      &["F2"]),
        ("panel_todos",      &["F4"]),
        ("panel_tags",       &["F5"]),
        ("panel_backlinks",  &["F8"]),
        ("panel_broken",     &["F9"]),
        ("panel_pages",      &["F10"]),
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
pub(super) fn clash(a: &str, b: &str) -> bool {
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
            // Magit: `C-x g` (magit-status), then `c c` (commit), `F p` (pull), `P p` (push), `l l` (log)
            ("git_commit",       &["ctrl+x g c c"]),
            ("git_pull",         &["ctrl+x g shift+f p"]),
            ("git_push",         &["ctrl+x g shift+p p"]),
            ("git_history",      &["ctrl+x g l l"]),
            // org-export-dispatch: `C-c C-e l p` (PDF), `C-c C-e h h` (HTML file)
            ("export_pdf",       &["ctrl+c ctrl+e l p"]),
            ("export_html",      &["ctrl+c ctrl+e h h"]),
            ("graph",            &["ctrl+c g"]),
            // project.el: `C-x p p` (project-switch-project)
            ("switch_project",   &["ctrl+x p p", "ctrl+alt+p"]),
            // As org-journal's `C-c C-j`, without the second Ctrl
            ("journal",          &["ctrl+c j"]),
            ("random_page",      &["ctrl+c r"]),
            // org-capture
            ("capture",          &["ctrl+c c"]),
            // org-agenda
            ("agenda",           &["ctrl+c a"]),
            // org-journal's `C-c C-b` / `C-c C-f` are org-mode's headline moves here
            ("journal_previous", &["ctrl+c ArrowLeft"]),
            ("journal_next",     &["ctrl+c ArrowRight"]),
            ("zen_mode",         &["F11", "ctrl+c z"]),
            ("rename_page",      &["F2"]),
            ("panel_todos",      &["F4"]),
            ("panel_tags",       &["F5"]),
            ("panel_backlinks",  &["F8"]),
            ("panel_broken",     &["F9"]),
            ("panel_pages",      &["F3"]),
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
            ("transpose_chars",     &["ctrl+t"]),
            ("transpose_words",     &["alt+t"]),
            ("upcase_word",         &["alt+u"]),
            ("downcase_word",       &["alt+l"]),
            ("capitalize_word",     &["alt+c"]),
            ("upcase_region",       &["ctrl+x ctrl+u"]),
            ("downcase_region",     &["ctrl+x ctrl+l"]),
            ("delete_horizontal_space", &["alt+\\"]),
            ("just_one_space",      &["alt+space"]),
            // `^` needs Shift on QWERTY; on AZERTY it is a dead key, known by its place (`[`)
            ("delete_indentation",  &["alt+^", "shift+alt+^", "alt+["]),
            ("zap_to_char",         &["alt+z"]),
            ("fill_paragraph",      &["alt+q"]),
            ("yank_pop",            &["alt+y"]),
            ("undo",                &["ctrl+/", "ctrl+x u"]),
            // undo-redo: `C-?`, `C-M-_` (`_` needs Shift on QWERTY, not on AZERTY)
            ("redo",                &["ctrl+shift+?", "ctrl+shift+alt+_", "ctrl+alt+_"]),
            // mark-whole-buffer
            ("select_all",          &["ctrl+x h"]),
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
            // org-footnote-action
            ("insert_footnote",     &["ctrl+c ctrl+x f"]),
            // org-emphasize (`C-c C-x C-f`) then the marker. Some markers need Shift,
            // depending on the keyboard (QWERTY / AZERTY), hence the second variant.
            ("bold",                &["ctrl+c ctrl+x ctrl+f *", "ctrl+c ctrl+x ctrl+f shift+*"]),
            ("italic",              &["ctrl+c ctrl+x ctrl+f /", "ctrl+c ctrl+x ctrl+f shift+/"]),
            ("underline",           &["ctrl+c ctrl+x ctrl+f _", "ctrl+c ctrl+x ctrl+f shift+_"]),
            ("strikethrough",       &["ctrl+c ctrl+x ctrl+f +", "ctrl+c ctrl+x ctrl+f shift++"]),
            // org-shiftright / org-shiftleft on a headline (elsewhere, Shift+arrows select)
            ("todo_next",           &["shift+ArrowRight", "ctrl+c ctrl+t"]),
            ("todo_prev",           &["shift+ArrowLeft"]),
            // org-schedule / org-deadline
            ("schedule",            &["ctrl+c ctrl+s"]),
            ("deadline",            &["ctrl+c ctrl+d"]),
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
            ("insert_footnote", &["ctrl+alt+f"]),
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
