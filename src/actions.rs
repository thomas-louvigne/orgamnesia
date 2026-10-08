//! Operations on the projects, pages and tabs, shared by the panels: each one
//! talks to the backend and updates the app state the same way wherever it starts.

use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::{
    i18n::t,
    invoke,
    state::{AppCtx, FileEntry, Goto, Tab, VaultChanges},
};

// ─── Projects ────────────────────────────────────────────────────────────────

/// Last path component, used as the project's display name.
pub fn project_name(path: &str) -> String {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(path)
        .to_string()
}

/// Switch to the project at `path`: loads its files and closes the tabs.
/// Asks for confirmation first when open tabs have unsaved changes.
pub async fn open_project(ctx: AppCtx, path: String) {
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
                p.insert(0, path);
            });
        }
        Err(e) => ctx.error("open_project_error", &e),
    }
}

/// Native folder picker, then open the chosen folder as a project.
pub async fn pick_and_open_project(ctx: AppCtx) {
    match invoke::pick_folder().await {
        Ok(Some(path)) => open_project(ctx, path).await,
        Ok(None) => {}
        Err(e) => ctx.error("error", &e),
    }
}

// ─── Tabs ────────────────────────────────────────────────────────────────────

/// Add `tab` and make it the active one; when its page got opened meanwhile
/// (two quick clicks), only activate the existing tab.
fn push_tab(ctx: AppCtx, tab: Tab) {
    if let Some(idx) = ctx.work.tab_index(&tab.path) {
        ctx.work.active_tab.set(Some(idx));
        return;
    }
    ctx.work.tabs.update(|tabs| tabs.push(tab));
    ctx.work.active_tab.set(Some(ctx.work.tabs.with_untracked(|t| t.len()) - 1));
}

/// Open `file` in a tab (or activate its tab); with `goto`, also move the cursor there.
pub fn open_file(ctx: AppCtx, file: FileEntry, goto: Option<Goto>) {
    let go = move |path: String| {
        if let Some(g) = goto { ctx.work.goto.set(Some((path, g))); }
    };
    if let Some(idx) = ctx.work.tab_index(&file.path) {
        ctx.work.active_tab.set(Some(idx));
        go(file.path);
        return;
    }
    // Signals are created here, in the reactive owner, not after the await
    let tab = Tab::new(&file, String::new(), false);
    spawn_local(async move {
        match invoke::read_file(&file.path).await {
            Ok(content) => {
                tab.content.set(content);
                push_tab(ctx, tab);
                go(file.path);
            }
            Err(e) => ctx.error("error", &e),
        }
    });
}

/// Open the existing page called `name` (per the case-sensitivity setting).
pub fn open_page_named(ctx: AppCtx, name: &str, goto: Option<Goto>) {
    if let Some(file) = ctx.find_page(name) {
        open_file(ctx, file, goto);
    }
}

/// Open the page a link points to, creating it when it doesn't exist yet.
pub fn follow_link(ctx: AppCtx, name: String) {
    let open = ctx.work.tabs.with_untracked(|tabs| tabs.iter().position(|t| ctx.same_page(&t.name, &name)));
    if let Some(idx) = open {
        ctx.work.active_tab.set(Some(idx));
    } else if let Some(file) = ctx.find_page(&name) {
        open_file(ctx, file, None);
    } else {
        create_and_open_page(ctx, name);
    }
}

/// Create the page `name`, add it to the list and open it in a new tab.
pub fn create_and_open_page(ctx: AppCtx, name: String) {
    // Signals are created here, in the reactive owner, not after the await
    let content = RwSignal::new(String::new());
    let dirty = RwSignal::new(true);
    spawn_local(async move {
        match invoke::create_page(&name).await {
            Ok(file) => {
                content.set(format!("* {}\n", file.name));
                ctx.project.files.update(|files| {
                    files.push(file.clone());
                    files.sort_by(|a, b| a.name.cmp(&b.name));
                });
                push_tab(ctx, Tab { path: file.path, name: file.name, content, dirty });
            }
            Err(e) => ctx.error("error", &e),
        }
    });
}

/// Close the tab at `idx`; the one before it becomes active.
pub fn close_tab(ctx: AppCtx, idx: usize) {
    let len = ctx.work.tabs.with_untracked(|t| t.len());
    if idx >= len { return; }
    ctx.work.tabs.update(|tabs| { tabs.remove(idx); });
    ctx.work.active_tab.update(|active| *active = active_after_close(*active, idx, len));
}

/// The active tab once tab `closed` of `len` is closed: the same tab (its index
/// shifts when it was after the closed one), or the one before the closed tab.
fn active_after_close(active: Option<usize>, closed: usize, len: usize) -> Option<usize> {
    match active? {
        i if i == closed => (len > 1).then(|| closed.saturating_sub(1)),
        i if i > closed => Some(i - 1),
        i => Some(i),
    }
}

/// Close the tab showing the page at `path`, if any.
pub fn close_tab_of(ctx: AppCtx, path: &str) {
    if let Some(idx) = ctx.work.tab_index(path) {
        close_tab(ctx, idx);
    }
}

/// Write the tab's page to disk.
pub fn save_tab(ctx: AppCtx, tab: Tab) {
    spawn_local(async move {
        let content = tab.content.get_untracked();
        match invoke::write_file(&tab.path, &content).await {
            Ok(_) => {
                ctx.bump_links();
                tab.dirty.set(false);
                ctx.notify_t("saved", &tab.name);
            }
            Err(err) => ctx.error("save_error", &err),
        }
    });
}

// ─── Pages ───────────────────────────────────────────────────────────────────

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
        for tab in ctx.work.tabs.get_untracked() {
            if !tab.dirty.get_untracked() { continue; }
            let content = tab.content.get_untracked();
            match invoke::write_file(&tab.path, &content).await {
                Ok(_) => if tab.content.get_untracked() == content { tab.dirty.set(false) },
                Err(e) => {
                    ctx.error("save_error", &e);
                    return;
                }
            }
        }
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
                    files.sort_by(|a, b| a.name.cmp(&b.name));
                });
                reload_clean_tabs(ctx).await;
                ctx.bump_links();
                ctx.notify_t("renamed", &nf.name);
            }
            Err(e) => ctx.error("rename_error", &e),
        }
    });
}

/// Delete a page after confirmation, and close its tab if open.
pub fn delete_page(ctx: AppCtx, file: FileEntry) {
    let lang = ctx.lang.get_untracked();
    let msg = format!("{} « {} » ?", t("delete_confirm", lang), file.name);
    ctx.ask_confirm(msg, move || {
        let file = file.clone();
        spawn_local(async move {
            match invoke::delete_page(&file.path).await {
                Ok(()) => {
                    ctx.project.files.update(|fs| fs.retain(|f| f.path != file.path));
                    close_tab_of(ctx, &file.path);
                    ctx.bump_links();
                    ctx.ui.status.set(Some(format!("{} {}", t("deleted", lang), file.name)));
                }
                Err(e) => ctx.error("delete_error", &e),
            }
        });
    });
}

/// Reload the content of open tabs without unsaved changes (after links were
/// rewritten on disk by a rename: every tab, as the rename saves them first).
async fn reload_clean_tabs(ctx: AppCtx) {
    for tab in ctx.work.tabs.get_untracked() {
        if tab.dirty.get_untracked() { continue; }
        if let Ok(content) = invoke::read_file(&tab.path).await && content != tab.content.get_untracked() {
            tab.content.set(content);
        }
    }
}

/// Apply changes made to the project's pages by another program: refresh the
/// page list, reload the open tabs without unsaved changes, warn about the others.
pub async fn apply_disk_changes(ctx: AppCtx, changes: VaultChanges) {
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
                ctx.ui.status.set(Some(format!("{} : {}", tab.name, t("changed_on_disk", lang))));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closing_a_tab_keeps_a_sensible_active_one() {
        // Closing the active tab: the previous one, or the first when it was first
        assert_eq!(active_after_close(Some(2), 2, 4), Some(1));
        assert_eq!(active_after_close(Some(0), 0, 3), Some(0));
        // The last tab: none left
        assert_eq!(active_after_close(Some(0), 0, 1), None);
        // Another tab: the active one stays, shifted when it was after it
        assert_eq!(active_after_close(Some(3), 1, 4), Some(2));
        assert_eq!(active_after_close(Some(1), 2, 4), Some(1));
        assert_eq!(active_after_close(None, 0, 2), None);
    }

    #[test]
    fn project_names() {
        assert_eq!(project_name("/home/me/notes/"), "notes");
        assert_eq!(project_name("/home/me/notes"), "notes");
        assert_eq!(project_name("/"), "/");
    }
}
