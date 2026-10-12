//! The org-bloom extension, on the interface side: building the site of the
//! project (https://github.com/thomas-louvigne/org-bloom).

use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::{actions, i18n::t, invoke, state::AppCtx};

/// Build the site of the open project, unless a build is running; the pages
/// are saved first, so the site shows them as they are on screen.
pub fn build(ctx: AppCtx) {
    if ctx.project.bloom_busy.get_untracked() || ctx.project.vault_path.get_untracked().is_none() { return; }
    ctx.project.bloom_busy.set(true);
    spawn_local(async move {
        if actions::save_dirty_tabs(ctx).await {
            ctx.notify_t("bloom_running", "");
            match invoke::build_site().await {
                Ok(folder) => ctx.notify_t("bloom_done", &folder),
                Err(e) => ctx.notify(format!("{} : {e}", t("bloom_error", ctx.lang.get_untracked()))),
            }
        }
        ctx.project.bloom_busy.set(false);
    });
}
