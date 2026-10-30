//! Keys pressed in the editor: the completion popup and the find bar first,
//! then the chords, the keys the drawn view handles itself (folds, indentation),
//! auto-pairing, and at last the editor actions of the keybindings.

use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use orgamnesia_core::org::{line_bounds, line_start};

use super::{
    completion::Completion,
    dom::{after_tick, caret, clipboard_write, get_pos, recenter, set_cursor, set_selection, splice, Textarea},
    find::Find,
    insert::Insert,
    pane::Pane,
};
use crate::{
    actions::follow_link,
    edit,
    folding,
    keybindings::{EditorAction, Resolution, Scope},
    tables,
};

/// Shortcuts a textarea handles natively; they only work when the active profile binds them.
const NATIVE_EDIT_SHORTCUTS: &[&str] = &[
    "ctrl+c", "ctrl+x", "ctrl+v", "ctrl+a", "ctrl+z", "ctrl+y", "ctrl+shift+z",
    "ctrl+Insert", "shift+Insert", "shift+Delete",
];

/// Whether the key is pressed without Ctrl, Alt or Meta.
fn unmodified(e: &web_sys::KeyboardEvent) -> bool {
    !e.ctrl_key() && !e.alt_key() && !e.meta_key()
}

/// The key is handled here: the browser and the app don't see it.
fn swallow(e: &web_sys::KeyboardEvent) {
    e.prevent_default();
    e.stop_propagation();
}

pub fn on_keydown(pane: Pane, completion: Completion, find: Find, insert: Insert, e: web_sys::KeyboardEvent) {
    let ctx = pane.ctx;
    let Some(el) = e.target().and_then(|t| t.dyn_into::<Textarea>().ok()) else { return };
    {
        let el = el.clone();
        after_tick(move || pane.track_caret(&el));
    }

    if completion.on_keydown(&e, &el) { return; }

    if find.is_open() && e.key() == "Escape" && unmodified(&e) && !e.shift_key() {
        e.prevent_default();
        find.close();
        return;
    }

    // Multi-key chords ("ctrl+x ctrl+s"): swallow the keys of a chord in progress
    let resolution = crate::keybindings::resolve(&ctx.keybindings.get(), &e);
    match &resolution {
        Resolution::Pending(keys) => {
            swallow(&e);
            ctx.set_chord_status(keys);
            return;
        }
        Resolution::Aborted => {
            swallow(&e);
            ctx.clear_chord_status();
            return;
        }
        _ => ctx.clear_chord_status(),
    }

    // In a configuration file (JSON), Tab indents as in any text editor, unless it
    // ends a chord of another action
    let other_action = matches!(&resolution, Resolution::Action(_, id) if id != "table_next_cell" && id != "table_prev_cell");
    if pane.config && e.key() == "Tab" && unmodified(&e) && !other_action {
        swallow(&e);
        let (start, end) = get_pos(&el);
        let chars: Vec<char> = el.value().chars().collect();
        let (from, to, text, (a, b)) = edit::json_indent(&chars, start, end, e.shift_key());
        splice(&el, from, to, &text);
        set_selection(&el, a, b);
        pane.commit(&el);
        return;
    }

    // With a mark set, the region follows the caret after each key press
    if pane.mark.get_untracked().is_some() {
        let el = el.clone();
        after_tick(move || pane.point.set(caret(&el)));
    }

    if view_keys(pane, &e, &el) || auto_pair(pane, &e, &el) { return; }

    // ── Configured editor action (classic or emacs preset) ────────────────────
    let action = match resolution {
        Resolution::Action(Scope::Editor, id) => EditorAction::from_id(&id),
        other => {
            // Not an editor shortcut of this profile: the browser's built-in
            // clipboard/undo shortcuts must not work either (unless the
            // combination belongs to an application shortcut).
            if other == Resolution::None
                && NATIVE_EDIT_SHORTCUTS.iter().any(|b| crate::keybindings::key_matches(&e, b))
            {
                e.prevent_default();
            }
            return;
        }
    };
    let Some(action) = action else { return };
    // These only act in some places (a table, a headline); elsewhere the key does what it usually does
    match action {
        EditorAction::TableNextCell | EditorAction::TablePrevCell => table_cell(pane, &e, &el, action),
        EditorAction::TodoNext | EditorAction::TodoPrev => todo_cycle(pane, &e, &el, action),
        EditorAction::PromoteSubtree | EditorAction::DemoteSubtree => shift_subtree(pane, &e, &el, action),
        _ => {
            swallow(&e);
            run_action(pane, find, insert, &el, action);
        }
    }
}

/// Keys the drawn view handles itself, as the page would have them; true when
/// the key was one of them.
fn view_keys(pane: Pane, e: &web_sys::KeyboardEvent, el: &Textarea) -> bool {
    let key = e.key();
    // The fold character of a folded headline is not deleted on its own (that would
    // delete the hidden text): Backspace or Delete next to it unfolds the headline
    if matches!(key.as_str(), "Backspace" | "Delete") && unmodified(e) {
        let (start, end) = get_pos(el);
        let chars: Vec<char> = el.value().chars().collect();
        let at = if key == "Delete" { start } else { start.wrapping_sub(1) };
        if start == end && chars.get(at).is_some_and(|&c| tables::fold_index(c).is_some()) {
            e.prevent_default();
            let pos = pane.page_pos(el, at);
            pane.folds.update(|f| f.retain(|&(a, _)| a != pos));
            return true;
        }
    }
    // Enter at the end of a folded headline: the new line comes after the hidden text
    if key == "Enter" && unmodified(e) && !e.shift_key() {
        let (start, end) = get_pos(el);
        if start == end && el.value().chars().nth(start).is_some_and(|c| tables::fold_index(c).is_some()) {
            set_cursor(el, start + 1);
        }
    }
    // Backspace at the start of an indented line joins it to the line above
    // (the indentation is not in the page: deleting it would change nothing)
    if key == "Backspace" && unmodified(e) {
        let (start, end) = get_pos(el);
        let text = el.value();
        if start == end && let Some((a, b)) = pane.shown.with_value(|v| v.indent_run(&text, start)) {
            e.prevent_default();
            if a > 0 {
                let cursor = splice(el, a - 1, b, "");
                set_cursor(el, cursor);
                pane.commit(el);
            }
            return true;
        }
    }
    false
}

/// Auto-pairing / electric wrapping: without a selection, ( [ { and " insert
/// their closing character too; with a selection, electric mode wraps it in the
/// typed pair. True when the key did so.
fn auto_pair(pane: Pane, e: &web_sys::KeyboardEvent, el: &Textarea) -> bool {
    let key = e.key();
    let (start, end) = get_pos(el);
    let Some((open, close)) = edit::pair_of(&key) else { return false };
    if start != end {
        if !pane.ctx.pref_untracked(|p| p.electric_mode) { return false; }
        e.prevent_default();
        let selected: String = el.value().chars().skip(start).take(end - start).collect();
        let new_end = splice(el, start, end, &format!("{open}{selected}{close}"));
        set_selection(el, start + 1, new_end - 1);
    } else {
        if !edit::auto_closes(&key) { return false; }
        e.prevent_default();
        let cursor = splice(el, start, start, &format!("{open}{close}"));
        set_cursor(el, cursor - 1);
    }
    pane.commit(el);
    true
}

/// Tab / Shift+Tab move between the cells of a table; elsewhere, when the
/// settings say so, they fold / unfold as org-mode's `org-cycle` / `org-shifttab`.
fn table_cell(pane: Pane, e: &web_sys::KeyboardEvent, el: &Textarea, action: EditorAction) {
    let chars: Vec<char> = el.value().chars().collect();
    let forward = action == EditorAction::TableNextCell;
    let Some(to) = tables::move_cell(&chars, caret(el), forward) else {
        if !pane.ctx.pref_untracked(|p| p.tab_folds) { return; }
        if forward { cycle(pane, e, el); } else if global_cycle(pane, el) { swallow(e); }
        return;
    };
    swallow(e);
    match to {
        tables::CellMove::Caret(p) => set_cursor(el, p),
        tables::CellMove::Edit { from, to, text, caret } => {
            splice(el, from, to, &text);
            set_cursor(el, caret);
            pane.commit(el);
        }
    }
    pane.track_caret(el);
}

/// A headline folds / unfolds (FOLDED, CHILDREN, SUBTREE), a list item with
/// lines under it, a block or a drawer too (org-mode's `org-cycle`).
fn cycle(pane: Pane, e: &web_sys::KeyboardEvent, el: &Textarea) {
    let pos = pane.page_pos(el, caret(el));
    let page: Vec<char> = pane.content.with_untracked(|c| c.chars().collect());
    let line = line_start(&page, pos);
    let next = if folding::is_headline(&page, line) {
        // On a headline, Tab never leaves the editor: with nothing under it, say so (Emacs: EMPTY ENTRY)
        swallow(e);
        let Some(next) = pane.folds.with_untracked(|f| folding::cycle(&page, line, f)) else {
            pane.ctx.notify_t("fold_empty", "");
            return;
        };
        next
    } else {
        let Some(next) = pane.folds.with_untracked(|f| folding::cycle_element(&page, line, f)) else { return };
        swallow(e);
        next
    };
    // The caret stays on the line
    pane.focus_pos.set(Some(pos.min(page[line..].iter().position(|&c| c == '\n').map_or(page.len(), |p| line + p))));
    pane.folds.set(next);
}

/// The whole page folds / unfolds: OVERVIEW, CONTENTS, SHOW ALL (org-mode's
/// `org-global-cycle`). False when the page has no headline.
fn global_cycle(pane: Pane, el: &Textarea) -> bool {
    let page: Vec<char> = pane.content.with_untracked(|c| c.chars().collect());
    let Some((shown, folds)) = pane.folds.with_untracked(|f| folding::global_cycle(&page, f)) else { return false };
    // A caret now hidden goes to the end of its headline
    let pos = pane.page_pos(el, caret(el));
    let pos = folds.iter().find(|&&(a, b)| a < pos && pos <= b).map_or(pos, |&(a, _)| a);
    pane.focus_pos.set(Some(pos));
    pane.folds.set(folds);
    pane.ctx.notify_t(match shown {
        folding::Global::Overview => "fold_overview",
        folding::Global::Contents => "fold_contents",
        folding::Global::ShowAll => "fold_show_all",
    }, "");
    true
}

/// On a headline, the TODO keyword goes to its next / previous state (org-mode
/// Shift+arrows), when TODO keywords are on.
fn todo_cycle(pane: Pane, e: &web_sys::KeyboardEvent, el: &Textarea, action: EditorAction) {
    let Some(keywords) = pane.todo_keywords.get_untracked() else { return };
    let chars: Vec<char> = el.value().chars().collect();
    let (_, end) = get_pos(el);
    let (ls, le) = line_bounds(&chars, end);
    // The view may indent the line, and ends a folded headline with its fold character
    let ls = ls + chars[ls..le].iter().take_while(|&&c| c == tables::INDENT).count();
    let le = if le > ls && tables::fold_index(chars[le - 1]).is_some() { le - 1 } else { le };
    let forward = action == EditorAction::TodoNext;
    let Some((a, b, with)) = keywords.cycle(&chars[ls..le], forward) else { return };
    swallow(e);
    let (a, b) = (ls + a, ls + b);
    let new_b = splice(el, a, b, &with);
    // The caret keeps its place in the title
    set_cursor(el, if end >= b { end + new_b - b } else { end.min(a) });
    pane.commit(el);
    pane.track_caret(el);
}

/// On a headline, the headline and its sub-headlines go one level up / down
/// (org-mode M-S-<left> / M-S-<right>).
fn shift_subtree(pane: Pane, e: &web_sys::KeyboardEvent, el: &Textarea, action: EditorAction) {
    let (_, end) = get_pos(el);
    let pos = pane.page_pos(el, end);
    let page: Vec<char> = pane.content.with_untracked(|c| c.chars().collect());
    let demote = action == EditorAction::DemoteSubtree;
    let lines = match folding::shift_subtree(&page, line_start(&page, pos), demote) {
        Ok(lines) => lines,
        Err(folding::ShiftError::NotHeadline) => return,
        Err(folding::ShiftError::TopLevel) => {
            swallow(e);
            pane.ctx.notify_t("promote_top_level", "");
            return;
        }
    };
    swallow(e);
    let folded = pane.folds.get_untracked();
    if !lines.iter().any(|&l| folded.iter().any(|&(a, b)| a < l && l <= b)) {
        // All the headlines are shown: edit the textarea, so the change can be undone
        let at: Vec<usize> = lines.iter().map(|&l| pane.view_pos(l)).collect();
        for &p in at.iter().rev() {
            if demote { splice(el, p, p, "*"); } else { splice(el, p, p + 1, ""); }
        }
        set_cursor(el, folding::caret_after_shift(end, &at, demote));
        pane.commit(el);
        pane.track_caret(el);
    } else {
        // Some are hidden in a fold: change the page, the folds keep their place
        let (out, moved) = folding::shift_page(&page, &lines, demote, &folded);
        pane.folds.set(moved);
        pane.set_page(out, folding::caret_after_shift(pos, &lines, demote));
    }
}

/// Move the line of the caret, or the lines of the selection, one line up or down.
/// On a headline, its whole subtree goes past its sibling's (org-mode's `M-<up>`).
fn move_line(pane: Pane, el: &Textarea, up: bool) {
    let (start, end) = get_pos(el);
    let (start, end) = (pane.page_pos(el, start), pane.page_pos(el, end));
    let page: Vec<char> = pane.content.with_untracked(|c| c.chars().collect());
    let line = line_start(&page, start);
    if line_start(&page, end) == line && folding::is_headline(&page, line) {
        let Some(s) = folding::move_subtree(&page, line, up) else { return };
        // The folds inside go with their text
        let folds = pane.folds.with_untracked(|f| {
            let mut f: Vec<_> = f.iter().map(|&(a, b)| if a >= s.a && b <= s.b { (s.pos(a), s.pos(b)) } else { (a, b) }).collect();
            f.sort();
            f
        });
        replace_lines(pane, el, &page, s.a, s.b, &s.text(&page), (s.pos(start), s.pos(end)), Some(folds));
        return;
    }
    let Some(m) = edit::move_line(&page, start, end, up) else { return };
    // Not across a fold: the hidden text would be moved with no sight of it
    if pane.folds.with_untracked(|f| f.iter().any(|&(a, b)| a < m.b && m.a <= b)) { return; }
    replace_lines(pane, el, &page, m.a, m.b, &m.text, m.sel, None);
}

/// The page range `a..b` becomes `text`, the selection `sel` (page positions), and
/// the folds `folds` when they change.
#[allow(clippy::too_many_arguments)]
fn replace_lines(pane: Pane, el: &Textarea, page: &[char], a: usize, b: usize, text: &str, sel: (usize, usize), folds: Option<Vec<(usize, usize)>>) {
    let (va, vb) = (pane.view_pos(a), pane.view_pos(b));
    let shown: String = el.value().chars().skip(va).take(vb.saturating_sub(va)).collect();
    let page_span: String = page[a..b].iter().collect();
    if shown == page_span {
        // The lines are shown as they are: edit the textarea, so the move can be undone
        splice(el, va, vb, text);
        set_selection(el, pane.view_pos(sel.0), pane.view_pos(sel.1));
        pane.commit(el);
        pane.track_caret(el);
    } else {
        // A table drawn or a fold among them: change the page
        if let Some(f) = folds { pane.folds.set(f); }
        let (before, after): (String, String) = (page[..a].iter().collect(), page[b..].iter().collect());
        pane.set_page(format!("{before}{text}{after}"), sel.1);
    }
}

/// An editor action of the keybindings (or of the context menu): motions,
/// clipboard, emphasis, Emacs editing…
pub(super) fn run_action(pane: Pane, find: Find, insert: Insert, el: &Textarea, action: EditorAction) {
    let ctx = pane.ctx;
    let chars: Vec<char> = el.value().chars().collect();
    let text = |a: usize, b: usize| -> String { pane.page_text(el, a, b) };
    // Replace a..b with `with`, caret after it; the page has changed
    let replace = |a: usize, b: usize, with: &str| {
        let cursor = splice(el, a, b, with);
        set_cursor(el, cursor);
        pane.commit(el);
    };

    if let Some(target) = edit::motion_target(action, &chars, caret(el)) {
        set_cursor(el, target);
        return;
    }
    let (start, end) = get_pos(el);
    // An Emacs kill of a..b: joins the kill just before it, if any. Compared in
    // page positions: the view's (indentation under headlines…) changes with the kill.
    let kill_range = |a: usize, b: usize| {
        let (pa, pb) = (pane.page_pos(el, a), pane.page_pos(el, b));
        let len = pane.content.with_untracked(|c| c.chars().count());
        crate::kill_ring::kill_range(ctx, el, text(a, b), pa, pb, len);
    };
    match action {
        // ── Classic clipboard / editing ───────────────────────────────────────
        EditorAction::Copy if start < end => {
            clipboard_write(&text(start, end));
            set_selection(el, start, end); // keep the selection
        }
        EditorAction::Cut if start < end => {
            clipboard_write(&text(start, end));
            replace(start, end, "");
        }
        EditorAction::Paste => {
            let el = el.clone();
            spawn_local(async move {
                let pasted = crate::kill_ring::read_clipboard().await;
                if let Some(pasted) = pasted.filter(|p| !p.is_empty()) {
                    let (start, end) = get_pos(&el);
                    let cursor = splice(&el, start, end, &pasted);
                    set_cursor(&el, cursor);
                    pane.commit(&el);
                }
            });
        }
        EditorAction::SelectAll => el.select(),
        EditorAction::Recenter => recenter(el, &chars, end),
        EditorAction::Undo => { pane.undo(false); }
        EditorAction::Redo => { pane.undo(true); }

        // ── Org emphasis: *bold*, /italic/, _underline_, +strikethrough+ ──────
        EditorAction::Bold | EditorAction::Italic | EditorAction::Underline | EditorAction::Strikethrough => {
            let marker = match action {
                EditorAction::Bold => '*',
                EditorAction::Italic => '/',
                EditorAction::Underline => '_',
                _ => '+',
            };
            let (a, b, with, sel_start, sel_end) = edit::toggle_emphasis(&chars, start, end, marker);
            splice(el, a, b, &with);
            set_selection(el, sel_start, sel_end);
            pane.commit(el);
        }

        // ── Org: new heading at the same level as the current one ─────────────
        EditorAction::NewHeading => {
            let (at, heading) = edit::new_heading(&chars, end);
            replace(at, at, &heading);
        }
        EditorAction::GlobalCycle => { global_cycle(pane, el); }
        EditorAction::InsertLink => insert.link(el),
        EditorAction::InsertDate | EditorAction::InsertInactiveDate => insert.date(el, action == EditorAction::InsertDate),
        EditorAction::OpenLink => {
            if let Some(name) = edit::linked_page_at(&chars, start, ctx.tags_untracked()) {
                follow_link(ctx, name);
            }
        }

        // ── Emacs editing ─────────────────────────────────────────────────────
        EditorAction::KillLine => {
            let (a, b) = edit::kill_line_range(&chars, start);
            kill_range(a, b);
            replace(a, b, "");
        }
        EditorAction::KillWordForward => {
            let b = edit::word_end_forward(&chars, start);
            kill_range(start, b);
            replace(start, b, "");
        }
        EditorAction::KillWordBackward => {
            let a = edit::word_start_backward(&chars, start);
            kill_range(a, start);
            replace(a, start, "");
        }
        EditorAction::KillRegion | EditorAction::CopyRegion => {
            let (a, b) = pane.region_of(el);
            if a < b {
                if action == EditorAction::KillRegion { kill_range(a, b); } else { crate::kill_ring::kill(ctx, text(a, b)); }
                if action == EditorAction::KillRegion {
                    replace(a, b, "");
                } else if pane.mark.get_untracked().is_none() {
                    set_selection(el, a, b); // keep the native selection
                }
            }
            pane.mark.set(None);
        }
        EditorAction::Yank => {
            let el = el.clone();
            crate::kill_ring::yank(ctx, move |text| {
                let (start, end) = get_pos(&el);
                let cursor = splice(&el, start, end, &text);
                set_cursor(&el, cursor);
                pane.commit(&el);
            });
        }
        EditorAction::MoveLineUp | EditorAction::MoveLineDown => {
            move_line(pane, el, action == EditorAction::MoveLineUp);
        }
        EditorAction::DeleteCharForward => {
            let (a, b) = edit::delete_forward_range(&chars, start, end);
            replace(a, b, "");
        }
        EditorAction::SetMark => {
            if ctx.pref_untracked(|p| p.emacs_mark) {
                let c = caret(el);
                if pane.mark.get_untracked() == Some(c) {
                    pane.mark.set(None); // Ctrl+Space twice at the same spot cancels
                } else {
                    pane.mark.set(Some(c));
                    pane.point.set(c);
                }
            }
        }
        EditorAction::KeyboardQuit => {
            pane.mark.set(None);
            if find.is_open() { find.close(); }
        }
        EditorAction::Find | EditorAction::FindPrevious => find.open(false, el),
        EditorAction::Replace => find.open(true, el),
        EditorAction::SpellNext => super::spell::next(pane, el),
        // Movements are handled above; copy / cut without a selection do nothing
        _ => {}
    }
}
