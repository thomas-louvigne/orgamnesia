//! What the panels shown in the main area, over the pages, have in common (the
//! settings, a commit, the history of a page…): the pages below stay mounted.

use leptos::{html, prelude::*};
use wasm_bindgen::JsCast;

use crate::state::AppCtx;

/// To call when the panel `root` is built. The keyboard comes to it, not to the
/// page hidden below, and goes back to the editor on closing. Another page or
/// project shown (the columns stay usable) would be hidden below: `close` then.
pub fn opened(ctx: AppCtx, root: NodeRef<html::Div>, close: impl Fn() + 'static) {
    Effect::new(move |_| {
        if let Some(root) = root.get() { let _ = root.focus(); }
    });
    on_cleanup(|| {
        let editor = document().query_selector(".editor-area.focused textarea").ok().flatten();
        if let Some(editor) = editor.and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok()) {
            let _ = editor.focus();
        }
    });
    Effect::new(move |shown: Option<_>| {
        let now = (ctx.work.active_tab.get(), ctx.project.vault_path.get(), ctx.project.preference.get());
        if shown.is_some_and(|shown| shown != now) { close(); }
        now
    });
}
