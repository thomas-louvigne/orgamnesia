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
}

fn clipboard() -> Option<web_sys::Clipboard> {
    web_sys::window().map(|w| w.navigator().clipboard())
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
    if let Some(cb) = clipboard() {
        let _ = cb.write_text(&text);
    }
    ctx.work.kill_ring.set(text);
}

/// Hand the text to yank to `insert`: the system clipboard's, else the last
/// kill when the clipboard is empty or can't be read. Nothing when both are empty.
pub fn yank(ctx: AppCtx, insert: impl FnOnce(String) + 'static) {
    let promise = clipboard().map(|cb| cb.read_text());
    spawn_local(async move {
        let pasted = match promise {
            Some(p) => JsFuture::from(p).await.ok().and_then(|v| v.as_string()),
            None => None,
        };
        let text = pasted.filter(|p| !p.is_empty())
            .or_else(|| ctx.work.kill_ring.try_get_untracked())
            .unwrap_or_default();
        if !text.is_empty() { insert(text); }
    });
}
