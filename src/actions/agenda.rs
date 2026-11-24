//! The agenda's changes to a page (its TODO state, its dates): in its tab when it
//! is open, else on disk; written either way, so the agenda reads them back.

use orgamnesia_core::agenda::{self, Day, Edit};
use orgamnesia_core::edit::Stamp;

use super::*;

/// Now, as the inactive timestamp org-mode writes when a headline is done.
pub fn now() -> Stamp {
    let d = js_sys::Date::new_0();
    Stamp {
        year: d.get_full_year() as i32, month: d.get_month() + 1, day: d.get_date(),
        time: Some((d.get_hours(), d.get_minutes())), active: false, rest: String::new(),
    }
}

/// Today, from 1970-01-01.
pub fn today() -> Day {
    now().day_number()
}

/// The page `path` changed by the edits `f` makes of its text, then written;
/// `then` runs once it is (or when `f` changes nothing).
pub fn change_page(ctx: AppCtx, path: String, f: impl FnOnce(&str) -> Option<Vec<Edit>> + 'static, then: impl FnOnce() + 'static) {
    spawn_local(async move {
        let tab = ctx.work.tabs.with_untracked(|tabs| tabs.iter().find(|t| t.path == path).cloned());
        let text = match &tab {
            Some(t) => t.content.get_untracked(),
            None => match invoke::read_file(&path).await {
                Ok(c) => c,
                Err(e) => return ctx.error("error", &e),
            },
        };
        if let Some(edits) = f(&text) {
            let new = agenda::apply(&text, &edits);
            match &tab {
                Some(t) => {
                    t.content.set(new);
                    t.dirty.set(true);
                    if !write_tab(ctx, t).await { return; }
                }
                None => {
                    if let Err(e) = invoke::write_file(&path, &new).await { return ctx.error("save_error", &e); }
                    ctx.bump_links();
                }
            }
        }
        then();
    });
}
