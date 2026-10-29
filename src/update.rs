//! New versions of the app: looked for at start (unless turned off) or from the
//! settings, and offered in a window with what changed, to install or not.

use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::{invoke, state::AppCtx};

/// Where the versions are published, for the packages the app can't install itself.
pub const RELEASES_URL: &str = "https://github.com/thomas-louvigne/orgamnesia/releases/latest";

/// How long after the start the new version is looked for: the app is shown first.
const AT_START_AFTER: std::time::Duration = std::time::Duration::from_secs(8);

/// Look for a new version a little after the start, when the settings say so;
/// the one the user skipped is not offered again. Says nothing otherwise (offline…).
pub fn at_start(ctx: AppCtx) {
    set_timeout(move || {
        if !ctx.pref_untracked(|p| p.check_updates) { return; }
        spawn_local(async move {
            let Ok(Some(info)) = invoke::check_update().await else { return };
            if ctx.pref_untracked(|p| p.skipped_update == info.version) { return; }
            ctx.ui.update.set(Some(info));
        });
    }, AT_START_AFTER);
}

/// Look for a new version now (from the settings), saying what was found.
pub fn check_now(ctx: AppCtx) {
    ctx.notify_t("update_checking", "");
    spawn_local(async move {
        match invoke::check_update().await {
            Ok(Some(info)) => {
                ctx.ui.status.set(None);
                ctx.ui.update.set(Some(info));
            }
            Ok(None) => ctx.notify_t("update_none", env!("CARGO_PKG_VERSION")),
            Err(e) => ctx.error("update_error", &e),
        }
    });
}

/// Don't offer `version` again at start.
pub fn skip(ctx: AppCtx, version: String) {
    let mut prefs = ctx.prefs_untracked();
    prefs.skipped_update = version;
    spawn_local(async move {
        let settings = prefs.to_settings(ctx.project.projects.get_untracked());
        match invoke::save_settings(&settings).await {
            Ok(_) => ctx.prefs.update(|p| p.skipped_update = prefs.skipped_update.clone()),
            Err(e) => ctx.error("error", &e),
        }
    });
}
