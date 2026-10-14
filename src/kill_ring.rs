//! The Emacs kill ring, shared by the editor and the other text fields, and
//! kept in step with the system clipboard: a kill is copied there too, and a
//! yank takes what is there (text copied in another app as well).

use leptos::prelude::*;
use wasm_bindgen_futures::{spawn_local, JsFuture};

use crate::state::AppCtx;

fn clipboard() -> Option<web_sys::Clipboard> {
    web_sys::window().map(|w| w.navigator().clipboard())
}

/// Keep `text` as the last kill, and put it in the system clipboard.
pub fn kill(ctx: AppCtx, text: String) {
    if text.is_empty() { return; }
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
