//! Writing the tabs to disk, and taking the changes made on disk by another program.

use super::*;

/// Write the tab's page to disk; a configuration file is applied (when valid).
pub fn save_tab(ctx: AppCtx, tab: Tab) {
    if preferences::config_file(&tab.path).is_some() {
        preferences::apply(ctx, tab);
        return;
    }
    spawn_local(async move {
        if write_tab(ctx, &tab).await { ctx.notify_t("saved", &tab.name); }
    });
}

/// Write the tab's page to disk, again as long as its text changed meanwhile, so
/// it is only marked saved once the disk holds what it shows. False (and the
/// error shown) when it could not be written.
pub async fn write_tab(ctx: AppCtx, tab: &Tab) -> bool {
    loop {
        // The tab may be closed meanwhile: its signals are then gone
        let Some(content) = tab.content.try_get_untracked() else { return true };
        if let Err(e) = invoke::write_file(&tab.path, &content).await {
            ctx.error("save_error", &e);
            return false;
        }
        if tab.content.try_get_untracked().is_none_or(|c| c == content) {
            tab.dirty.try_set(false);
            ctx.bump_links();
            return true;
        }
    }
}

/// Write every page with unsaved changes. False (and the error shown) when one
/// could not be written.
pub async fn save_dirty_tabs(ctx: AppCtx) -> bool {
    for tab in ctx.work.tabs.get_untracked() {
        // A configuration file is only written when applied
        if !tab.dirty.get_untracked() || preferences::config_file(&tab.path).is_some() { continue; }
        if !write_tab(ctx, &tab).await { return false; }
    }
    true
}

/// The first page of `Ui::disk_conflicts` keeps the text being edited (`take_disk`
/// false: written to disk now when saving is automatic) or takes the one on disk.
pub async fn settle_disk_conflict(ctx: AppCtx, take_disk: bool) {
    let Some(path) = ctx.ui.disk_conflicts.with_untracked(|q| q.first().cloned()) else { return };
    ctx.ui.disk_conflicts.update(|q| { q.remove(0); });
    let Some(tab) = ctx.work.tabs.with_untracked(|tabs| tabs.iter().find(|t| t.path == path).cloned()) else { return };
    if take_disk {
        match read_page(&path).await {
            Ok(content) => {
                tab.content.try_set(content);
                tab.dirty.try_set(false);
            }
            Err(e) => ctx.error("error", &e),
        }
    } else if ctx.pref_untracked(|p| p.autosave) && preferences::config_file(&path).is_none() {
        save_tab(ctx, tab);
    }
}

/// Reload the content of open tabs without unsaved changes (after links were
/// rewritten on disk by a rename: every tab, as the rename saves them first).
pub async fn reload_clean_tabs(ctx: AppCtx) {
    for tab in ctx.work.tabs.get_untracked() {
        if tab.dirty.get_untracked() { continue; }
        if let Ok(content) = invoke::read_file(&tab.path).await && content != tab.content.get_untracked() {
            tab.content.set(content);
        }
    }
}

/// Apply changes made to the project's pages by another program: refresh the
/// page list, reload the open tabs without unsaved changes; for the others, ask
/// which version to keep (unless the disk holds the same text).
pub async fn apply_disk_changes(ctx: AppCtx, changes: VaultChanges) {
    let Some(changes) = preferences::defer_disk_changes(ctx, changes) else { return };
    if ctx.project.files.get_untracked() != changes.files {
        ctx.project.files.set(changes.files);
    }
    let lang = ctx.lang.get_untracked();
    for tab in ctx.work.tabs.get_untracked() {
        // The tab may be closed while a file is read: its signals are then gone
        let dirty = || tab.dirty.try_get_untracked().unwrap_or(true);
        if changes.removed.contains(&tab.path) {
            ctx.ui.status.set(Some(format!("{} : {}", tab.name, t("deleted_on_disk", lang))));
        } else if changes.changed.contains(&tab.path) {
            if dirty() {
                if let Ok(content) = invoke::read_file(&tab.path).await {
                    if tab.content.try_get_untracked().is_some_and(|c| c == content) {
                        tab.dirty.try_set(false);
                    } else {
                        ctx.ui.disk_conflicts.update(|q| if !q.contains(&tab.path) { q.push(tab.path.clone()) });
                    }
                }
                continue;
            }
            if let Ok(content) = invoke::read_file(&tab.path).await
                && !dirty() && tab.content.try_get_untracked().is_some_and(|c| c != content)
            {
                tab.content.try_set(content);
            }
        }
    }
    ctx.bump_links();
}
