//! Operations on pages: renaming, deleting, tagging, restoring from the trash.

use super::*;

/// Rename the page at `old_path`; the backend also rewrites the links to it.
pub fn rename_page(ctx: AppCtx, old_path: String, new_name: String) {
    let new_name = new_name.trim().to_string();
    let old_name = ctx.project.files.with_untracked(|fs| {
        fs.iter().find(|f| f.path == old_path).map(|f| f.name.clone())
    });
    if new_name.is_empty() || old_name.as_deref() == Some(new_name.as_str()) {
        return;
    }
    spawn_local(async move {
        // The backend rewrites the links on disk: first write the pages with
        // unsaved changes, else their next save would bring the old links back.
        if !save_dirty_tabs(ctx).await { return; }
        match invoke::rename_page(&old_path, &new_name).await {
            Ok(nf) => {
                ctx.work.tabs.update(|tabs| {
                    if let Some(t) = tabs.iter_mut().find(|t| t.path == old_path) {
                        t.name = nf.name.clone();
                        t.path = nf.path.clone();
                    }
                });
                ctx.project.files.update(|files| {
                    if let Some(f) = files.iter_mut().find(|f| f.path == old_path) {
                        f.name = nf.name.clone();
                        f.path = nf.path.clone();
                    }
                    files.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.path.cmp(&b.path)));
                });
                reload_clean_tabs(ctx).await;
                ctx.bump_links();
                ctx.notify_t("renamed", &nf.name);
                ctx.warn_twin(&nf);
            }
            Err(e) => ctx.error("rename_error", &e),
        }
    });
}

/// Delete a page after confirmation, and close its tab if open. The page goes to
/// the project's trash: the status bar offers to undo.
pub fn delete_page(ctx: AppCtx, file: FileEntry) {
    let lang = ctx.lang.get_untracked();
    let msg = format!("{} « {} » ? {}", t("delete_confirm", lang), file.name, t("delete_confirm_trash", lang));
    ctx.ask_confirm_danger(msg, "delete_page_button", move || {
        let file = file.clone();
        spawn_local(async move {
            match invoke::delete_page(&file.path).await {
                Ok(trashed) => {
                    let was_open = ctx.work.tabs.with_untracked(|tabs| tabs.iter().any(|t| t.path == file.path));
                    ctx.project.files.update(|fs| fs.retain(|f| f.path != file.path));
                    close_tab_of(ctx, &file.path);
                    ctx.bump_links();
                    ctx.notify_undo(format!("{} {}", t("deleted", lang), file.name), move || {
                        restore_page(ctx, trashed.clone(), was_open);
                    });
                }
                Err(e) => ctx.error("delete_error", &e),
            }
        });
    });
}

/// Add the file tag `tag` (`#+FILETAGS:`) to the page `file`: in its tab when
/// open, like typing in it; else on disk.
pub fn tag_page(ctx: AppCtx, file: FileEntry, tag: &'static str) {
    let lang = ctx.lang.get_untracked();
    let FileEntry { path, name, .. } = file;
    let done = move |added: bool| if added {
        ctx.ui.status.set(Some(format!("{} :{tag}: → {name}", t("tag_added", lang))));
    } else {
        ctx.ui.status.set(Some(format!("{} :{tag}:", t("tag_exists", lang))));
    };
    if let Some(tab) = ctx.work.tabs.with_untracked(|tabs| tabs.iter().find(|t| t.path == path).cloned()) {
        match orgamnesia_core::Format::of_page(&path).add_page_tag(&tab.content.get_untracked(), tag) {
            Some(content) => {
                // Marked unsaved, then auto-saved if enabled
                tab.dirty.set(true);
                tab.content.set(content);
                done(true);
            }
            None => done(false),
        }
        return;
    }
    spawn_local(async move {
        let content = match invoke::read_file(&path).await {
            Ok(c) => c,
            Err(e) => return ctx.error("error", &e),
        };
        let Some(content) = orgamnesia_core::Format::of_page(&path).add_page_tag(&content, tag) else { return done(false) };
        match invoke::write_file(&path, &content).await {
            Ok(_) => {
                ctx.bump_links();
                done(true);
            }
            Err(e) => ctx.error("save_error", &e),
        }
    });
}

/// Bring back the page deleted to `trashed`; reopen it when it was open.
fn restore_page(ctx: AppCtx, trashed: String, reopen: bool) {
    spawn_local(async move {
        match invoke::restore_page(&trashed).await {
            Ok(file) => {
                let name = file.name.clone();
                ctx.project.files.update(|fs| {
                    fs.push(file);
                    fs.sort_by(|a, b| a.name.cmp(&b.name));
                });
                ctx.bump_links();
                if reopen { open_page_named(ctx, &name, None); }
                ctx.notify_t("restored", &name);
            }
            Err(e) => ctx.error("restore_error", &e),
        }
    });
}
