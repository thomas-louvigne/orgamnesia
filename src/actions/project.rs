//! Opening a project: picking its folder, reading its pages, offering its `assets/` folder.

use super::*;

/// Last path component, used as the project's display name.
pub fn project_name(path: &str) -> String {
    path.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(path)
        .to_string()
}

/// Switch to the project at `path`: loads its files and closes the tabs.
/// Asks for confirmation first when open tabs have unsaved changes. From the
/// "preference" project, comes back to the project put aside first.
pub async fn open_project(ctx: AppCtx, path: String) {
    if ctx.project.preference.get_untracked() {
        preferences::leave(ctx, move || spawn_local(open_project(ctx, path.clone())));
        return;
    }
    if ctx.project.vault_path.get_untracked().as_deref() == Some(path.as_str()) {
        return;
    }
    if ctx.work.has_unsaved_tabs() {
        let msg = t("discard_confirm", ctx.lang.get_untracked()).to_string();
        ctx.ask_confirm(msg, move || spawn_local(load_project(ctx, path.clone())));
        return;
    }
    load_project(ctx, path).await;
}

async fn load_project(ctx: AppCtx, path: String) {
    match invoke::open_vault(&path).await {
        Ok(files) => {
            ctx.project.vault_path.set(Some(path.clone()));
            ctx.project.files.set(files);
            ctx.work.tabs.set(vec![]);
            ctx.work.active_tab.set(None);
            ctx.project.projects.update(|p| {
                p.retain(|x| x != &path);
                p.insert(0, path.clone());
            });
            offer_assets(ctx, path).await;
        }
        Err(e) => ctx.error("open_project_error", &e),
    }
}

/// A folder of `.org` files opened as it is, without an `assets/` folder: offer to
/// make one, saying what it is for. Declined, it is not offered again there;
/// closed, it is offered on the next opening. When the folder is the `pages/` of
/// a project with its `assets/`, first offer to open that project instead.
async fn offer_assets(ctx: AppCtx, path: String) {
    let Ok(Some(offer)) = invoke::assets_offer(&path).await else { return };
    match offer.parent {
        Some(parent) => {
            let lang = ctx.lang.get_untracked();
            let message = format!("{}\n\n{parent}", t("assets_parent_offer", lang));
            let keep = path.clone();
            ctx.ask_choice(
                message,
                "assets_keep_folder", move || offer_to_make_assets(ctx, keep.clone()),
                "assets_use_parent", move || {
                    let (pages, parent) = (path.clone(), parent.clone());
                    spawn_local(async move {
                        // The project is the folder around: `pages/` leaves the list
                        if let Ok(list) = invoke::remove_project(&pages).await { ctx.project.projects.set(list); }
                        load_project(ctx, parent).await;
                    });
                },
            );
        }
        None => offer_to_make_assets(ctx, path),
    }
}

/// Native folder picker, where a new folder may be created, then open the chosen
/// folder as a project (a new or empty one gets `pages/` and `assets/`).
pub async fn pick_and_open_project(ctx: AppCtx) {
    let title = t("open_project_title", ctx.lang.get_untracked());
    match invoke::pick_folder(Some(title)).await {
        Ok(Some(path)) => open_project(ctx, path).await,
        Ok(None) => {}
        Err(e) => ctx.error("open_project_error", &e),
    }
}

fn offer_to_make_assets(ctx: AppCtx, path: String) {
    let lang = ctx.lang.get_untracked();
    let answer = move |create: bool| {
        let path = path.clone();
        move || {
            let path = path.clone();
            spawn_local(async move {
                if let Err(e) = invoke::answer_assets(&path, create).await { ctx.error("error", &e); }
            });
        }
    };
    ctx.ask_choice(t("assets_offer", lang).to_string(), "assets_decline", answer(false), "assets_create", answer(true));
}
