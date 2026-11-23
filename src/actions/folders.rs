//! The folders of the pages (`pages/Jeux/Quêtes/`): made, renamed (moved),
//! deleted; a page moved to one. A page in a folder is named by its path from the
//! pages folder (`Jeux/Quêtes/Dragon`), links too.

use super::*;

/// Read the folders of the pages again (after one was made, renamed, deleted).
pub fn refresh_folders(ctx: AppCtx) {
    if ctx.project.vault_path.get_untracked().is_none() || ctx.project.preference.get_untracked() { return; }
    spawn_local(async move {
        if let Ok(folders) = invoke::list_folders().await { ctx.project.folders.set(folders); }
    });
}

/// The folders, with `""` (the pages folder itself) first: the places a page may go.
fn places(ctx: AppCtx) -> Vec<String> {
    std::iter::once(String::new()).chain(ctx.project.folders.get_untracked()).collect()
}

/// The folder of the page named `name` (`Jeux` for `Jeux/Dragon`), `""` at the top.
pub fn folder_of(name: &str) -> &str {
    name.rsplit_once('/').map_or("", |(dir, _)| dir)
}

/// Ask the name of a new folder, in `parent` (`None`: at the top), and make it.
pub fn new_folder(ctx: AppCtx, parent: Option<String>) {
    let lang = ctx.lang.get_untracked();
    let start = parent.map(|p| format!("{p}/")).unwrap_or_default();
    ctx.ask_text(t("folder_new_ask", lang).to_string(), start, ctx.project.folders.get_untracked(), move |name| {
        spawn_local(async move {
            match invoke::create_folder(&name).await {
                Ok(name) => { refresh_folders(ctx); ctx.notify_t("folder_created", &name); }
                Err(e) => ctx.error("error", &e),
            }
        });
    });
}

/// A new page in the folder `folder`: the new page window, its name started.
pub fn new_page_in(ctx: AppCtx, folder: String) {
    ctx.ui.new_page_name.set(format!("{folder}/"));
    ctx.ui.show_new_page.set(true);
}

/// Ask the folder to move the page `file` to (empty: the top), and move it; its
/// links follow as for a rename.
pub fn move_page(ctx: AppCtx, file: FileEntry) {
    let lang = ctx.lang.get_untracked();
    let leaf = file.name.rsplit('/').next().unwrap_or(&file.name).to_string();
    let label = t("move_page_ask", lang).replace("{}", &leaf);
    ctx.ask_text(label, folder_of(&file.name).to_string(), places(ctx), move |folder| {
        let folder = folder.trim_matches('/');
        let name = if folder.is_empty() { leaf.clone() } else { format!("{folder}/{leaf}") };
        rename_page(ctx, file.path.clone(), name);
    });
}

/// Move the page open (the command list's action).
pub fn move_current_page(ctx: AppCtx) {
    match ctx.work.active_tab_data().and_then(|tab| ctx.project.files.with_untracked(|fs| fs.iter().find(|f| f.path == tab.path).cloned())) {
        Some(file) => move_page(ctx, file),
        None => ctx.notify_t("no_page_open", ""),
    }
}

/// The folder asked first (when `folder` is `None`, from the command list), then `then` done with it.
fn with_folder(ctx: AppCtx, folder: Option<String>, then: impl Fn(AppCtx, String) + Send + Sync + 'static) {
    if let Some(folder) = folder { return then(ctx, folder); }
    let folders = ctx.project.folders.get_untracked();
    if folders.is_empty() { return ctx.notify_t("folder_none", ""); }
    ctx.ask_text(t("folder_which", ctx.lang.get_untracked()).to_string(), String::new(), folders, move |f| then(ctx, f));
}

/// Ask the new name (or place: `Autre/Jeux`) of the folder `folder`, and rename
/// it: its pages and the links to them follow; the tabs open on them too.
pub fn rename_folder(ctx: AppCtx, folder: Option<String>) {
    with_folder(ctx, folder, |ctx, old| {
        let label = t("folder_rename_ask", ctx.lang.get_untracked()).replace("{}", &old);
        ctx.ask_text(label, old.clone(), places(ctx), move |new| {
            let old = old.clone();
            spawn_local(async move {
                if !save_dirty_tabs(ctx).await { return; }
                match invoke::rename_folder(&old, &new).await {
                    Ok(new) => {
                        let files = invoke::list_pages().await.unwrap_or_default();
                        // The tabs of the pages moved follow them
                        let (from, to) = (format!("{old}/"), format!("{new}/"));
                        ctx.work.tabs.update(|tabs| for tab in tabs.iter_mut().filter(|t| t.name.starts_with(&from)) {
                            let name = format!("{to}{}", &tab.name[from.len()..]);
                            if let Some(f) = files.iter().find(|f| f.name == name) { tab.name = name; tab.path = f.path.clone(); }
                        });
                        ctx.project.files.set(files);
                        refresh_folders(ctx);
                        reload_clean_tabs(ctx).await;
                        ctx.bump_links();
                        ctx.notify_t("renamed", &new);
                    }
                    Err(e) => ctx.error("rename_error", &e),
                }
            });
        });
    });
}

/// Delete the folder `folder` after confirmation: its pages go to the trash (the
/// status bar offers to bring them back, in their folder), their tabs close.
pub fn delete_folder(ctx: AppCtx, folder: Option<String>) {
    with_folder(ctx, folder, |ctx, name| {
        let lang = ctx.lang.get_untracked();
        let count = ctx.project.files.with_untracked(|fs| fs.iter().filter(|f| f.name.starts_with(&format!("{name}/"))).count());
        let msg = t("folder_delete_confirm", lang).replacen("{}", &name, 1).replacen("{}", &count.to_string(), 1);
        ctx.ask_confirm_danger(msg, "folder_delete_button", move || {
            let name = name.clone();
            spawn_local(async move {
                match invoke::delete_folder(&name).await {
                    Ok(trashed) => {
                        let prefix = format!("{name}/");
                        let gone: Vec<String> = ctx.project.files.with_untracked(|fs| fs.iter().filter(|f| f.name.starts_with(&prefix)).map(|f| f.path.clone()).collect());
                        ctx.project.files.update(|fs| fs.retain(|f| !f.name.starts_with(&prefix)));
                        for path in &gone { close_tab_of(ctx, path); }
                        refresh_folders(ctx);
                        ctx.bump_links();
                        ctx.notify_undo(format!("{} {}", t("deleted", lang), name), move || {
                            for trashed in trashed.clone() { restore_page_quietly(ctx, trashed); }
                            refresh_folders(ctx);
                        });
                    }
                    Err(e) => ctx.error("delete_error", &e),
                }
            });
        });
    });
}
