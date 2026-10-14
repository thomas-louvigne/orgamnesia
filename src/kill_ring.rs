//! The Emacs kill ring, shared by the editor and the other text fields, and
//! kept in step with the system clipboard: a kill is copied there too, and a
//! yank takes what is there (text copied in another app as well).
//!
//! As in Emacs, kills made one after the other make a single entry: `C-k C-k C-k`
//! kills three lines (and their line breaks) that `C-y` puts back together.

use std::cell::RefCell;

use leptos::prelude::*;
use wasm_bindgen_futures::{spawn_local, JsFuture};

use crate::state::AppCtx;

/// Where the last kill left the text: the field, the caret (where the text was
/// taken out) and the length of the text after it, in chars.
struct LastKill {
    field: web_sys::Element,
    at: usize,
    len: usize,
}

thread_local! {
    static LAST: RefCell<Option<LastKill>> = const { RefCell::new(None) };
    /// The last kill as it was put in the system clipboard; `None` when it
    /// couldn't be: the clipboard then still holds something older.
    static IN_CLIPBOARD: RefCell<Option<String>> = const { RefCell::new(None) };
}

fn web_clipboard() -> Option<web_sys::Clipboard> {
    web_sys::window().map(|w| w.navigator().clipboard())
}

/// The text of the system clipboard (copied here or in another application);
/// `None` when it can't be read. The backend reads it; the webview's own clipboard
/// is only a fallback: it can't read what another application copied.
pub async fn read_clipboard() -> Option<String> {
    if let Ok(text) = crate::invoke::read_clipboard().await { return text; }
    let promise = web_clipboard()?.read_text();
    JsFuture::from(promise).await.ok().and_then(|v| v.as_string())
}

/// Put `text` in the system clipboard; whether it went there.
pub async fn write_clipboard(text: &str) -> bool {
    if crate::invoke::write_clipboard(text).await == Ok(true) { return true; }
    match web_clipboard().map(|cb| cb.write_text(text)) {
        Some(promise) => JsFuture::from(promise).await.is_ok(),
        None => false,
    }
}

/// Kill `text`, the chars `a..b` of `field`, a text of `len` chars. Right after
/// another kill there (same place, nothing typed since), it joins it: after it
/// when killing forward (`C-k`, `M-d`), before it when killing backward (`M-DEL`).
pub fn kill_range(ctx: AppCtx, field: &web_sys::Element, text: String, a: usize, b: usize, len: usize) {
    if text.is_empty() { return; }
    let follows = LAST.with(|l| l.borrow().as_ref()
        .filter(|k| k.field == *field && k.len == len)
        .map(|k| (k.at == a, k.at == b)));
    let text = match follows {
        Some((true, _)) => ctx.work.kill_ring.get_untracked() + &text,
        Some((false, true)) => text + &ctx.work.kill_ring.get_untracked(),
        _ => text,
    };
    keep(ctx, text);
    LAST.with(|l| *l.borrow_mut() = Some(LastKill { field: field.clone(), at: a, len: len - (b - a) }));
}

/// Keep `text` as the last kill (a copy: it doesn't join the kill before it).
pub fn kill(ctx: AppCtx, text: String) {
    if text.is_empty() { return; }
    LAST.with(|l| *l.borrow_mut() = None);
    keep(ctx, text);
}

/// The kill ring holds `text`, and the system clipboard too.
fn keep(ctx: AppCtx, text: String) {
    IN_CLIPBOARD.with(|c| *c.borrow_mut() = None);
    let written = text.clone();
    spawn_local(async move {
        if write_clipboard(&written).await {
            IN_CLIPBOARD.with(|c| *c.borrow_mut() = Some(written));
        }
    });
    ctx.work.kill_ring.set(text);
}

/// What to yank, given what the system clipboard holds (`None`: unreadable), the
/// last kill, and the last kill as it went in the clipboard (`None`: it didn't).
/// The clipboard wins only when something else was copied there since that kill.
fn to_yank(clipboard: Option<String>, killed: String, in_clipboard: Option<String>) -> String {
    match (clipboard.filter(|c| !c.is_empty()), in_clipboard) {
        // Copied elsewhere after the kill
        (Some(c), Some(k)) if c != k => c,
        // The clipboard is the kill, or older than it (the kill couldn't go there)
        (Some(c), _) if killed.is_empty() => c,
        _ => killed,
    }
}

/// Hand the text to yank to `insert`: the last kill, or what was copied in the
/// system clipboard since (in another app too). Nothing when both are empty.
pub fn yank(ctx: AppCtx, insert: impl FnOnce(String) + 'static) {
    spawn_local(async move {
        let pasted = read_clipboard().await;
        let killed = ctx.work.kill_ring.try_get_untracked().unwrap_or_default();
        let text = to_yank(pasted, killed, IN_CLIPBOARD.with(|c| c.borrow().clone()));
        if !text.is_empty() { insert(text); }
    });
}

#[cfg(test)]
mod tests {
    use super::to_yank;

    fn s(t: &str) -> String { t.to_string() }

    #[test]
    fn yanks_the_kill_unless_something_was_copied_since() {
        // The kill went in the clipboard: same text
        assert_eq!(to_yank(Some(s("a\nb\n")), s("a\nb\n"), Some(s("a\nb\n"))), "a\nb\n");
        // Copied in another app after the kill
        assert_eq!(to_yank(Some(s("ailleurs")), s("a\nb\n"), Some(s("a\nb\n"))), "ailleurs");
        // The kill couldn't go in the clipboard: what it holds is older
        assert_eq!(to_yank(Some(s("vieux")), s("a\nb\n"), None), "a\nb\n");
        // Unreadable clipboard, or nothing killed yet
        assert_eq!(to_yank(None, s("a\nb\n"), Some(s("a\nb\n"))), "a\nb\n");
        assert_eq!(to_yank(Some(s("copié")), s(""), None), "copié");
    }
}
