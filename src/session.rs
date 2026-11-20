//! The start of the app: the settings, project and keybindings saved, the pages
//! and panes open when it was last quit; then the backend kept told of them.

use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::{invoke, state::{AppCtx, Session, Tab}};

/// Load the saved settings, project and keybindings, and the pages and panes
/// open when the app was last quit. `owner` is the app's: the tabs' signals belong to it.
pub async fn load_startup(ctx: AppCtx, owner: Owner) {
    if let Ok(settings) = invoke::get_settings().await {
        let prefs = crate::preferences::prefs_of(&settings);
        ctx.prefs.set(prefs.clone());
        if settings.first_run { ctx.ui.show_welcome.set(true); }
        ctx.project.projects.set(settings.projects);
        if let Some(path) = settings.vault_path {
            match invoke::open_vault(&path).await {
                Ok(files) => {
                    ctx.project.vault_path.set(Some(path.clone()));
                    ctx.project.files.set(files);
                    if let Some(session) = settings.session.filter(|s| prefs.restore_session && s.project == path) {
                        restore_session(ctx, &owner, session).await;
                    }
                }
                Err(e) => ctx.error("open_project_error", &e),
            }
        }
    }
    if let Ok(mut kb) = invoke::get_keybindings().await {
        kb.upgrade();
        ctx.keybindings.set(kb);
    }
    track_session(ctx, &owner);
    owner.with(|| crate::preferences::watch(ctx));
    offer_json_removal(ctx).await;
}

/// The settings are YAML since 1.8.0: once converted, the JSON files of the
/// older versions are of no use; their deletion is offered (once, if kept).
async fn offer_json_removal(ctx: AppCtx) {
    let Ok(files) = invoke::json_leftovers().await else { return };
    if files.is_empty() { return; }
    let lang = ctx.lang.get_untracked();
    let message = format!("{}\n\n{}", crate::i18n::t("json_leftovers", lang), files.join("\n"));
    let answer = |remove: bool| move || {
        spawn_local(async move {
            if let Err(e) = invoke::answer_json(remove).await { ctx.error("error", &e); }
        });
    };
    ctx.ask_choice(message, "json_keep", answer(false), "json_remove", answer(true));
}

/// Open again the tabs and panes of `session`; pages deleted since are left out.
async fn restore_session(ctx: AppCtx, owner: &Owner, session: Session) {
    let files = ctx.project.files.get_untracked();
    let mut tabs = vec![];
    for path in &session.tabs {
        let Some(file) = files.iter().find(|f| &f.path == path) else { continue };
        if let Ok(content) = invoke::read_file(path).await {
            tabs.push(owner.with(|| Tab::new(file, content, false)));
        }
    }
    let index = |page: &Option<String>| page.as_ref().and_then(|p| tabs.iter().position(|t| &t.path == p));
    let active = index(&session.active).or((!tabs.is_empty()).then_some(0));
    let other = index(&session.other).map(|i| tabs[i].path.clone());
    let work = ctx.work;
    work.tabs.set(tabs);
    work.active_tab.set(active);
    if let (Some(kind), Some(other), Some(_)) = (session.split, other, active) {
        work.other_path.set(Some(other));
        work.focus_second.set(session.focus_second);
        work.split.set(Some(kind));
    }
}

/// Keep the backend told of the open pages and panes, which it saves on quit.
/// Started after the restore, so the saved session isn't replaced by an empty one.
fn track_session(ctx: AppCtx, owner: &Owner) {
    owner.with(|| Effect::new(move |_| {
        // The "preference" project's files are no pages of the project: the session
        // stays the one of the project put aside
        if ctx.project.preference.get() { return; }
        let work = ctx.work;
        let session = ctx.project.vault_path.get().map(|project| Session {
            project,
            tabs: work.tabs.with(|tabs| tabs.iter().map(|t| t.path.clone()).collect()),
            active: work.active_tab_data().map(|t| t.path),
            split: work.split.get(),
            other: work.other_path.get(),
            focus_second: work.focus_second.get(),
        });
        spawn_local(async move {
            let _ = invoke::set_session(session.as_ref()).await;
        });
    }));
}
