//! The editor actions that make sense in the other text fields of the app (tags,
//! search, page title, commit message…): the Emacs kills and yank, sharing the
//! editor's kill ring (see `kill_ring`), the caret movements, the mark and the text
//! edits at the caret. Copy, cut, paste and undo are left to the browser, which does
//! them in any field.

use std::cell::RefCell;

use leptos::prelude::*;
use wasm_bindgen::JsCast;

use crate::{edit, keybindings::EditorAction, state::AppCtx};

/// A text field other than the editor.
#[derive(Clone)]
enum Field {
    Input(web_sys::HtmlInputElement),
    Area(web_sys::HtmlTextAreaElement),
}

thread_local! {
    /// The mark set in a field (Ctrl+Space): the field and its position, in chars.
    static MARK: RefCell<Option<(web_sys::Element, usize)>> = const { RefCell::new(None) };
    /// The field (or the editor) where `M-z` waits for its character.
    static ZAP: RefCell<Option<web_sys::Element>> = const { RefCell::new(None) };
}

/// `M-z` was pressed in `el`: the next key says up to which character to kill.
pub fn arm_zap(ctx: AppCtx, el: &web_sys::Element) {
    ZAP.with(|z| *z.borrow_mut() = Some(el.clone()));
    let keys = ctx.keybindings.with_untracked(|kb| crate::keybindings::first_shortcut(kb, "zap_to_char"));
    ctx.set_chord_status(&keys.unwrap_or_default());
}

/// The key pressed in `el`, when `M-z` waits for a character there: `Some` with the
/// character, or none when the key cancels it (or is only a modifier, and it waits on).
pub fn zap_key(ctx: AppCtx, el: &web_sys::Element, e: &web_sys::KeyboardEvent) -> Option<Option<char>> {
    let armed = ZAP.with(|z| z.borrow().as_ref() == Some(el));
    if !armed {
        ZAP.with(|z| z.borrow_mut().take());
        return None;
    }
    let key = e.key();
    if matches!(key.as_str(), "Shift" | "Control" | "Alt" | "AltGraph" | "Meta" | "Dead") { return Some(None); }
    ZAP.with(|z| z.borrow_mut().take());
    ctx.clear_chord_status();
    // Ctrl alone is a command (`C-g`); with Alt, on some systems, it is AltGr typing a character
    if e.meta_key() || (e.ctrl_key() && !e.alt_key()) { return Some(None); }
    if key == "Enter" { return Some(Some('\n')); }
    let mut chars = key.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => Some(Some(c)),
        _ => Some(None),
    }
}

/// The key that follows `M-z` in a text field; true when it was that.
pub fn on_zap_key(ctx: AppCtx, e: &web_sys::KeyboardEvent) -> bool {
    let Some(field) = Field::of(e) else { return false };
    let Some(zap) = zap_key(ctx, field.element(), e) else { return false };
    e.prevent_default();
    if let (Some(c), Some((_, end))) = (zap, field.selection()) {
        let chars: Vec<char> = field.value().chars().collect();
        let at = to_chars(&chars, end);
        match edit::zap_end(&chars, at, c) {
            Some(b) => {
                let text = chars[at..b].iter().collect();
                crate::kill_ring::kill_range(ctx, field.element(), text, at, b, chars.len());
                replace(&field, &chars, at, b, "");
            }
            None => ctx.notify_t("zap_not_found", &c.to_string()),
        }
    }
    true
}

impl Field {
    /// The editable text field the event happened in; `None` for the editor
    /// (it has its own handling), a read-only field or one without a caret
    /// (a checkbox, a number…).
    fn of(e: &web_sys::KeyboardEvent) -> Option<Self> {
        let target = e.target()?;
        let field = if let Some(el) = target.dyn_ref::<web_sys::HtmlTextAreaElement>() {
            if el.class_list().contains("edit-layer") || el.read_only() || el.disabled() { return None; }
            Self::Area(el.clone())
        } else {
            let el = target.dyn_into::<web_sys::HtmlInputElement>().ok()?;
            if el.read_only() || el.disabled() { return None; }
            Self::Input(el)
        };
        field.selection().map(|_| field)
    }

    fn element(&self) -> &web_sys::Element {
        match self { Self::Input(el) => el, Self::Area(el) => el }
    }

    fn value(&self) -> String {
        match self { Self::Input(el) => el.value(), Self::Area(el) => el.value() }
    }

    /// The selection, in UTF-16 units as the browser counts.
    fn selection(&self) -> Option<(u32, u32)> {
        let (s, e) = match self {
            Self::Input(el) => (el.selection_start(), el.selection_end()),
            Self::Area(el) => (el.selection_start(), el.selection_end()),
        };
        Some((s.ok()??, e.ok()??))
    }

    fn set_selection(&self, start: u32, end: u32, backward: bool) {
        let dir = if backward { "backward" } else { "forward" };
        let _ = match self {
            Self::Input(el) => el.set_selection_range_with_direction(start, end, dir),
            Self::Area(el) => el.set_selection_range_with_direction(start, end, dir),
        };
    }
}

/// Position in chars of a position in UTF-16 units of `chars`, and back.
pub(crate) fn to_chars(chars: &[char], units: u32) -> usize {
    let mut n = 0;
    chars.iter().take_while(|c| { n += c.len_utf16() as u32; n <= units }).count()
}

pub(crate) fn to_units(chars: &[char], pos: usize) -> u32 {
    chars[..pos.min(chars.len())].iter().map(|c| c.len_utf16() as u32).sum()
}

/// Replace `a..b` (chars) of the field with `with`, caret after it. Done by an
/// editing command: the field's `input` event fires and Ctrl+Z undoes it.
fn replace(field: &Field, chars: &[char], a: usize, b: usize, with: &str) {
    field.set_selection(to_units(chars, a), to_units(chars, b), false);
    if let Some(doc) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.dyn_into::<web_sys::HtmlDocument>().ok())
    {
        let _ = doc.exec_command_with_show_ui_and_value("insertText", false, with);
    }
}

/// Run `action` in the text field the key was pressed in; true when it was done
/// there (the key must then do nothing else).
pub fn run(ctx: AppCtx, e: &web_sys::KeyboardEvent, action: EditorAction) -> bool {
    use EditorAction::*;
    let Some(field) = Field::of(e) else { return false };
    let Some((s, en)) = field.selection() else { return false };
    let chars: Vec<char> = field.value().chars().collect();
    let (start, end) = (to_chars(&chars, s), to_chars(&chars, en));
    let text = |a: usize, b: usize| chars[a..b].iter().collect::<String>();
    let mark = MARK.with(|m| m.borrow().as_ref()
        .filter(|(el, _)| el == field.element())
        .map(|&(_, at)| at.min(chars.len())));
    let clear_mark = || MARK.with(|m| m.borrow_mut().take());
    // The caret is the end of the selection that moves
    let caret = if mark.is_some_and(|m| m == end && start < end) { start } else { end };
    let region = || match mark {
        Some(m) => (m.min(caret), m.max(caret)),
        None => (start, end),
    };

    // An Emacs kill of a..b: joins the kill just before it, if any
    let kill_range = |a: usize, b: usize| crate::kill_ring::kill_range(ctx, field.element(), text(a, b), a, b, chars.len());

    let single_line = matches!(field, Field::Input(_));
    if single_line && matches!(action, NextLine | PreviousLine | ScrollDown | ScrollUp
        | ForwardParagraph | BackwardParagraph | NextHeading | PreviousHeading
        | BackwardHeading | ForwardHeading | UpHeading | BackwardElement | ForwardElement)
    {
        return false;
    }
    if let Some(target) = edit::motion_target(action, &chars, caret) {
        match mark {
            // With a mark, the movement stretches the region
            Some(m) => field.set_selection(
                to_units(&chars, m.min(target)), to_units(&chars, m.max(target)), target < m,
            ),
            None => {
                let at = to_units(&chars, target);
                field.set_selection(at, at, false);
            }
        }
        return true;
    }
    match action {
        KillLine => {
            let (a, b) = edit::kill_line_range(&chars, caret);
            kill_range(a, b);
            replace(&field, &chars, a, b, "");
        }
        KillWordForward => {
            let b = edit::word_end_forward(&chars, caret);
            kill_range(caret, b);
            replace(&field, &chars, caret, b, "");
        }
        KillWordBackward => {
            let a = edit::word_start_backward(&chars, caret);
            kill_range(a, caret);
            replace(&field, &chars, a, caret, "");
        }
        KillRegion | CopyRegion => {
            let (a, b) = region();
            if a < b {
                if action == KillRegion { kill_range(a, b); } else { crate::kill_ring::kill(ctx, text(a, b)); }
                if action == KillRegion {
                    replace(&field, &chars, a, b, "");
                } else {
                    let at = to_units(&chars, caret);
                    field.set_selection(at, at, false);
                }
            }
            clear_mark();
        }
        Yank => {
            clear_mark();
            crate::kill_ring::yank(ctx, move |text| {
                let chars: Vec<char> = field.value().chars().collect();
                let Some((s, e)) = field.selection() else { return };
                let at = to_chars(&chars, s);
                replace(&field, &chars, at, to_chars(&chars, e), &text);
                crate::kill_ring::yanked(field.element(), at, text);
            });
        }
        DeleteCharForward => {
            let (a, b) = edit::delete_forward_range(&chars, start, end);
            replace(&field, &chars, a, b, "");
        }
        TransposeChars | TransposeWords | UpcaseWord | DowncaseWord | CapitalizeWord
        | DeleteHorizontalSpace | JustOneSpace | DeleteIndentation => {
            if let Some((a, b, with)) = edit::text_edit(action, &chars, caret) {
                replace(&field, &chars, a, b, &with);
                if action == DeleteIndentation {
                    let at = to_units(&chars, edit::caret_after_edit(action, a, &with));
                    field.set_selection(at, at, false);
                }
            }
        }
        UpcaseRegion | DowncaseRegion => {
            let (a, b) = region();
            if a < b {
                let case = if action == UpcaseRegion { edit::Case::Upper } else { edit::Case::Lower };
                replace(&field, &chars, a, b, &edit::change_case(&chars[a..b], case));
            }
            clear_mark();
        }
        ZapToChar => arm_zap(ctx, field.element()),
        SelectAll => {
            clear_mark();
            field.set_selection(0, to_units(&chars, chars.len()), false);
        }
        YankPop => match crate::kill_ring::yank_pop(field.element(), caret, &chars) {
            Some((a, b, with)) => {
                replace(&field, &chars, a, b, &with);
            }
            None => ctx.notify_t("yank_pop_none", ""),
        },
        SetMark if ctx.pref_untracked(|p| p.emacs_mark) => {
            if mark == Some(caret) {
                clear_mark(); // twice at the same spot cancels
            } else {
                MARK.with(|m| *m.borrow_mut() = Some((field.element().clone(), caret)));
            }
            let at = to_units(&chars, caret);
            field.set_selection(at, at, false);
        }
        KeyboardQuit if mark.is_some() || start != end => {
            clear_mark();
            let at = to_units(&chars, caret);
            field.set_selection(at, at, false);
        }
        // Clipboard, undo, emphasis… are left to the browser, or mean nothing here
        _ => return false,
    }
    true
}

/// Run the editor shortcut of a key pressed in a text field outside the editor;
/// true when it was done (the key is then stopped).
pub fn on_keydown(ctx: AppCtx, e: &web_sys::KeyboardEvent) -> bool {
    use crate::keybindings::{Resolution, Scope};
    if on_zap_key(ctx, e) { return true; }
    let Resolution::Action(Scope::Editor, id) =
        ctx.keybindings.with_untracked(|kb| crate::keybindings::resolve(kb, e))
    else { return false };
    let done = EditorAction::from_id(&id).is_some_and(|action| run(ctx, e, action));
    if done { e.prevent_default(); }
    done
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_positions_round_trip() {
        let chars: Vec<char> = "a😀é b".chars().collect();
        assert_eq!(to_units(&chars, 2), 3);
        assert_eq!(to_chars(&chars, 3), 2);
        assert_eq!(to_chars(&chars, 0), 0);
        assert_eq!(to_chars(&chars, 99), chars.len());
        for i in 0..=chars.len() {
            assert_eq!(to_chars(&chars, to_units(&chars, i)), i);
        }
    }
}
