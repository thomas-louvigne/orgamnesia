//! Keys pressed in the editor: the completion popup and the find bar first,
//! then the chords, the keys the drawn view handles itself (folds, indentation),
//! auto-pairing, and at last the editor actions of the keybindings.

use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use orgamnesia_core::agenda;
use orgamnesia_core::org::line_start;

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
    i18n::t,
    keybindings::{EditorAction, Resolution, Scope},
    tables,
    text_field,
};
mod commands;
use commands::*;

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
    // Headlines are those of the page's format while the key is handled
    let _format = crate::motion::FormatGuard::set(pane.fmt());
    let Some(el) = e.target().and_then(|t| t.dyn_into::<Textarea>().ok()) else { return };
    {
        let el = el.clone();
        after_tick(move || pane.track_caret(&el));
    }

    // The key after `M-z` is the character to kill up to
    if let Some(zap) = text_field::zap_key(ctx, &el, &e) {
        swallow(&e);
        if let Some(c) = zap { zap_to(pane, &el, c); }
        return;
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
        EditorAction::Schedule | EditorAction::Deadline => {
            swallow(&e);
            plan(pane, &el, action);
        }
        EditorAction::PromoteSubtree | EditorAction::DemoteSubtree => shift_subtree(pane, &e, &el, action),
        _ => {
            swallow(&e);
            run_action(pane, find, insert, &el, action);
        }
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
        EditorAction::SelectAll => {
            pane.mark.set(None);
            el.select();
        }
        EditorAction::Recenter => recenter(el, &chars, end),
        EditorAction::Undo => { pane.undo(false); }
        EditorAction::Redo => { pane.undo(true); }

        // ── Emphasis (*bold* in org-mode, **bold** in Markdown…) and quotes ───
        EditorAction::Bold | EditorAction::Italic | EditorAction::Underline | EditorAction::Strikethrough
        | EditorAction::Quote | EditorAction::Comment => {
            let (a, b, with, sel_start, sel_end) = match edit::emphasis_marks(action, pane.fmt()) {
                Some((open, close)) => edit::toggle_emphasis(&chars, start, end, open, close),
                None if action == EditorAction::Comment => edit::toggle_comment(&chars, start, end, pane.fmt()),
                None => edit::toggle_quote(&chars, start, end, pane.fmt()),
            };
            splice(el, a, b, &with);
            set_selection(el, sel_start, sel_end);
            pane.commit(el);
        }

        // ── Org: new heading at the same level as the current one ─────────────
        EditorAction::NewHeading => {
            let (from, to, heading) = edit::new_heading(&chars, end);
            replace(from, to, &heading);
        }
        EditorAction::GlobalCycle => { global_cycle(pane, el); }
        EditorAction::InsertLink => insert.link(el),
        EditorAction::InsertDate | EditorAction::InsertInactiveDate => insert.date(el, action == EditorAction::InsertDate),
        EditorAction::InsertFootnote => {
            let (a, b, with, cursor) = orgamnesia_core::footnotes::insert(&chars, end, pane.fmt());
            splice(el, a, b, &with);
            set_cursor(el, cursor);
            pane.commit(el);
        }
        EditorAction::OpenLink => {
            // On a footnote: to its definition, or back to its reference
            if let Some(to) = orgamnesia_core::footnotes::other_end(&chars, start, pane.fmt()) {
                set_cursor(el, to);
                recenter(el, &chars, to);
            } else if let Some(name) = edit::linked_page_at(&chars, start, pane.ctx.tags_untracked(), pane.fmt()) {
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
                let at = pane.page_pos(&el, start);
                let cursor = splice(&el, start, end, &text);
                set_cursor(&el, cursor);
                pane.commit(&el);
                crate::kill_ring::yanked(&el, at, text);
            });
        }
        EditorAction::YankPop => {
            let page: Vec<char> = pane.content.with_untracked(|c| c.chars().collect());
            match crate::kill_ring::yank_pop(el, pane.page_pos(el, caret(el)), &page) {
                Some((a, b, with)) => replace_page(pane, el, &page, a, b, &with, a + with.chars().count()),
                None => ctx.notify_t("yank_pop_none", ""),
            }
        }
        EditorAction::MoveLineUp | EditorAction::MoveLineDown => {
            move_line(pane, el, action == EditorAction::MoveLineUp);
        }
        EditorAction::DeleteCharForward => {
            let (a, b) = edit::delete_forward_range(&chars, start, end);
            replace(a, b, "");
        }
        // Worked out on the page: what the view draws in its place (fold, indentation) is not text
        EditorAction::TransposeChars | EditorAction::TransposeWords
        | EditorAction::UpcaseWord | EditorAction::DowncaseWord | EditorAction::CapitalizeWord
        | EditorAction::DeleteHorizontalSpace | EditorAction::JustOneSpace | EditorAction::DeleteIndentation => {
            let page: Vec<char> = pane.content.with_untracked(|c| c.chars().collect());
            if let Some((a, b, with)) = edit::text_edit(action, &page, pane.page_pos(el, caret(el))) {
                replace_page(pane, el, &page, a, b, &with, edit::caret_after_edit(action, a, &with));
            }
        }
        EditorAction::FillParagraph => {
            let page: Vec<char> = pane.content.with_untracked(|c| c.chars().collect());
            let pos = pane.page_pos(el, caret(el));
            if let Some((a, b, with)) = crate::motion::fill_paragraph(&page, pos, edit::FILL_COLUMN) {
                // The caret stays on the same word: as many non-blank characters before it
                let kept = page[a..pos].iter().filter(|c| !c.is_whitespace()).count();
                let in_new = with.char_indices().filter(|(_, c)| !c.is_whitespace()).nth(kept)
                    .map_or(with.chars().count(), |(i, _)| with[..i].chars().count());
                replace_page(pane, el, &page, a, b, &with, a + in_new);
            }
        }
        // The region's case: the view's own characters (a fold, indentation) have none
        EditorAction::UpcaseRegion | EditorAction::DowncaseRegion => {
            let (a, b) = pane.region_of(el);
            if a < b {
                let case = if action == EditorAction::UpcaseRegion { edit::Case::Upper } else { edit::Case::Lower };
                replace(a, b, &edit::change_case(&chars[a..b], case));
            }
            pane.mark.set(None);
        }
        EditorAction::ZapToChar => text_field::arm_zap(ctx, el),
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
            // A selection made with Shift (or the mouse) goes too, as the region does
            set_cursor(el, caret(el));
            if find.is_open() { find.close(); }
        }
        EditorAction::Find | EditorAction::FindPrevious => find.open(false, el),
        EditorAction::Replace => find.open(true, el),
        EditorAction::SpellNext => super::spell::next(pane, el),
        // Movements are handled above; copy / cut without a selection do nothing
        _ => {}
    }
}
