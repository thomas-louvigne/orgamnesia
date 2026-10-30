//! What keeps the interface in step with the project as it changes outside it:
//! pages changed on disk, the git state and its timers, the tags; and the project
//! search, started afresh in another project.

use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::{actions, invoke, state::{AppCtx, VaultChanges}};

/// Start watching, for the whole life of the app.
pub fn start(ctx: AppCtx) {
    watch_disk(ctx);
    watch_git(ctx);
    git_timers(ctx);
    watch_tags(ctx);
    reset_search(ctx);
}

/// Pages changed on disk by another program (another editor, a sync tool…),
/// as the backend's watcher reports them.
fn watch_disk(ctx: AppCtx) {
    invoke::listen(invoke::VAULT_CHANGED, move |changes: VaultChanges| {
        spawn_local(actions::apply_disk_changes(ctx, changes));
    });
    // And a look every second, for the changes the watcher misses
    fn poll(ctx: AppCtx) {
        crate::keybindings::after_ms(1000, move || spawn_local(async move {
            if ctx.project.vault_path.try_get_untracked().flatten().is_some()
                && let Ok(Some(changes)) = invoke::check_disk().await
            {
                actions::apply_disk_changes(ctx, changes).await;
            }
            poll(ctx);
        }));
    }
    poll(ctx);
    // Files dropped on the window: the editor under them adds the images
    invoke::listen(invoke::DRAG_DROP, move |d: invoke::Dropped| {
        let n = ctx.ui.dropped.with_untracked(|d| d.as_ref().map_or(0, |(n, _)| n + 1));
        ctx.ui.dropped.set(Some((n, d)));
    });
}

/// Git extension: read the git state of the project when it is opened or the
/// extension enabled, then whenever the watcher sees the project or its
/// repository change (edits, commits, pushes made from elsewhere).
fn watch_git(ctx: AppCtx) {
    let busy = StoredValue::new(false);
    let again = StoredValue::new(false);
    let refresh = move || {
        let Some(path) = ctx.project.vault_path.get_untracked().filter(|_| ctx.pref_untracked(|p| p.git_ext)) else {
            ctx.project.git.set(None);
            return;
        };
        // One read at a time; a change during a read triggers one more
        if busy.get_value() { again.set_value(true); return; }
        busy.set_value(true);
        spawn_local(async move {
            loop {
                again.set_value(false);
                let st = invoke::git_status(&path).await.ok().flatten();
                // Another project may have been opened meanwhile
                // Unchanged, it isn't set again: the git frame would be updated for nothing
                if ctx.project.vault_path.get_untracked().as_deref() == Some(path.as_str())
                    && ctx.project.git.with_untracked(|g| *g != st)
                {
                    ctx.project.git.set(st);
                }
                if !again.get_value() { break; }
            }
            busy.set_value(false);
        });
    };
    // Only when the extension is switched, not on every change of the settings
    let enabled = Memo::new(move |_| ctx.pref(|p| p.git_ext));
    Effect::new(move |_| {
        ctx.project.vault_path.track();
        enabled.track();
        ctx.project.git.set(None);
        refresh();
    });
    invoke::listen(invoke::GIT_CHANGED, move |()| refresh());
}

/// The remote fetched once the project is known to have one, then every few
/// minutes; the automatic sync every so many minutes, as set.
fn git_timers(ctx: AppCtx) {
    use std::time::Duration;
    // Another project: its sync isn't held back by the last one's failure
    Effect::new(move |_| {
        ctx.project.vault_path.track();
        ctx.project.git_sync_paused.set(false);
    });
    let fetchable = Memo::new(move |_| (
        ctx.project.vault_path.get(),
        ctx.project.git.with(|g| g.as_ref().is_some_and(|g| g.repo && g.remote)),
    ));
    let fetch_minutes = Memo::new(move |_| ctx.pref(|p| p.git_fetch));
    Effect::new(move |_| if fetchable.get().1 && fetch_minutes.get() > 0 { crate::git::fetch(ctx); });
    // Started again whenever the setting changes; 0: never
    let every = move |minutes: u64, job: fn(AppCtx)| (minutes > 0).then(|| {
        set_interval_with_handle(move || job(ctx), Duration::from_secs(minutes * 60)).ok()
    }).flatten();
    Effect::new(move |prev: Option<Option<IntervalHandle>>| {
        if let Some(Some(tick)) = prev { tick.clear(); }
        every(fetch_minutes.get(), crate::git::fetch)
    });
    Effect::new(move |prev: Option<Option<IntervalHandle>>| {
        if let Some(Some(tick)) = prev { tick.clear(); }
        every(ctx.pref(|p| p.git_auto_sync), crate::git::auto_sync)
    });
}

/// Another project: the project search starts afresh (its undo was the other project's).
fn reset_search(ctx: AppCtx) {
    Effect::new(move |_| {
        ctx.project.vault_path.track();
        let s = ctx.ui.search;
        s.results.set(Default::default());
        s.skip.set(Default::default());
        s.can_undo.set(false);
    });
}

/// Org-mode tags of the project, for the tags frame and the completion of
/// `:tags:`. Refreshed (debounced) when the project, its pages or their links change.
fn watch_tags(ctx: AppCtx) {
    let seq = StoredValue::new(0u32);
    Effect::new(move |_| {
        ctx.project.files.track();
        ctx.project.links_version.track();
        if ctx.project.vault_path.get().is_none() {
            ctx.project.tags.set(vec![]);
            return;
        }
        let n = seq.get_value() + 1;
        seq.set_value(n);
        crate::keybindings::after_ms(400, move || {
            if seq.try_get_value() != Some(n) { return; }
            spawn_local(async move {
                if let Ok(list) = invoke::list_tags().await
                    && ctx.project.tags.get_untracked() != list { ctx.project.tags.set(list); }
            });
        });
    });
}
