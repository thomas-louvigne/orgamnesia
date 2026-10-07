use tauri::{AppHandle, Manager, State};

use crate::{
    AppState,
    index::backlinks::BacklinkIndex,
    keybindings,
    parser,
    settings,
    tags,
    vault::{self, FileEntry},
};

type Cmd<T> = Result<T, String>;

fn hashtags(app: &AppHandle) -> parser::Hashtags {
    app.path().app_config_dir().ok()
        .map(|d| settings::load(&d).hashtags())
        .unwrap_or(parser::Hashtags::Dashes)
}

// ─── Settings ────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_settings(app: AppHandle) -> Cmd<settings::Settings> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    Ok(settings::load(&dir))
}

#[tauri::command]
pub async fn set_settings(
    state: State<'_, AppState>,
    app: AppHandle,
    settings: settings::Settings,
) -> Cmd<()> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    let hashtags_changed = settings::load(&dir).hashtags() != settings.hashtags();
    settings::save(&dir, &settings).map_err(|e| e.to_string())?;

    // `#tags` are read differently (as links or not, with or without `-`): rebuild the index
    let vault_path = state.vault_path.lock().unwrap().clone();
    if let (true, Some(vp)) = (hashtags_changed, vault_path) {
        let hashtags = settings.hashtags();
        let mut idx = BacklinkIndex::new();
        for f in vault::list_org_files(&vp).map_err(|e| e.to_string())? {
            let content = std::fs::read_to_string(&f.path).unwrap_or_default();
            idx.index_file(&f.name, &parser::extract_links(&content, hashtags));
        }
        *state.backlinks.lock().unwrap() = idx;
    }
    Ok(())
}

// ─── Vault ───────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn open_vault(
    state: State<'_, AppState>,
    app: AppHandle,
    path: String,
) -> Cmd<Vec<FileEntry>> {
    vault::ensure_structure(&path).map_err(|e| e.to_string())?;

    // Persist chosen path
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    let mut s = settings::load(&dir);
    s.vault_path = Some(path.clone());
    s.projects.retain(|p| p != &path);
    s.projects.insert(0, path.clone());
    settings::save(&dir, &s).map_err(|e| e.to_string())?;

    let files = vault::list_org_files(&path).map_err(|e| e.to_string())?;

    // Rebuild backlink index
    let hashtags = s.hashtags();
    let mut idx = BacklinkIndex::new();
    for f in &files {
        let content = std::fs::read_to_string(&f.path).unwrap_or_default();
        idx.index_file(&f.name, &parser::extract_links(&content, hashtags));
    }

    *state.vault_path.lock().unwrap() = Some(path);
    *state.backlinks.lock().unwrap() = idx;
    *state.snapshot.lock().unwrap() = vault::snapshot(&files);

    Ok(files)
}

/// Pages changed on disk since the last look (by another program).
#[derive(serde::Serialize)]
pub struct VaultChanges {
    /// All the pages of the project, as now on disk.
    pub files: Vec<FileEntry>,
    /// Paths of the pages created or modified.
    pub changed: Vec<String>,
    /// Paths of the pages deleted.
    pub removed: Vec<String>,
}

/// Look for pages created, modified or deleted outside the app since the last
/// call, and update the link index. `None` when nothing changed.
#[tauri::command]
pub async fn poll_vault(state: State<'_, AppState>, app: AppHandle) -> Cmd<Option<VaultChanges>> {
    let Some(vp) = state.vault_path.lock().unwrap().clone() else { return Ok(None) };
    let files = vault::list_org_files(&vp).map_err(|e| e.to_string())?;
    let now = vault::snapshot(&files);
    let (changed, removed) = {
        let mut snap = state.snapshot.lock().unwrap();
        // Another project was opened meanwhile: its snapshot is not ours to replace
        if state.vault_path.lock().unwrap().as_deref() != Some(vp.as_str()) {
            return Ok(None);
        }
        let (changed, removed) = vault::diff(&snap, &now);
        if changed.is_empty() && removed.is_empty() {
            return Ok(None);
        }
        *snap = now;
        (changed, removed)
    };

    let hashtags = hashtags(&app);
    let mut idx = state.backlinks.lock().unwrap();
    for path in &removed {
        idx.remove_source(&vault::page_name(path));
    }
    for path in &changed {
        let content = std::fs::read_to_string(path).unwrap_or_default();
        idx.index_file(&vault::page_name(path), &parser::extract_links(&content, hashtags));
    }
    Ok(Some(VaultChanges { files, changed, removed }))
}

/// Record the current stamp of a page written by the app, so `poll_vault`
/// does not report it as changed outside.
fn remember(state: &AppState, path: &str) {
    if let Some(s) = vault::stamp(path) {
        state.snapshot.lock().unwrap().insert(path.to_string(), s);
    }
}

/// Git state of a project folder (git extension); `None` when git can't be run.
#[tauri::command]
pub async fn git_status(path: String) -> Cmd<Option<crate::git::GitStatus>> {
    Ok(crate::git::status(&path))
}

/// Forget a project (the folder itself is left untouched); returns the remaining list.
#[tauri::command]
pub async fn remove_project(app: AppHandle, path: String) -> Cmd<Vec<String>> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    let mut s = settings::load(&dir);
    s.projects.retain(|p| p != &path);
    settings::save(&dir, &s).map_err(|e| e.to_string())?;
    Ok(s.projects)
}

#[tauri::command]
pub async fn list_files(state: State<'_, AppState>) -> Cmd<Vec<FileEntry>> {
    let path = state.vault_path.lock().unwrap().clone().ok_or("No vault open")?;
    vault::list_org_files(&path).map_err(|e| e.to_string())
}

// ─── File I/O ────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn read_file(path: String) -> Cmd<String> {
    std::fs::read_to_string(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn write_file(
    state: State<'_, AppState>,
    app: AppHandle,
    path: String,
    content: String,
) -> Cmd<()> {
    std::fs::write(&path, &content).map_err(|e| e.to_string())?;
    if state.snapshot.lock().unwrap().contains_key(&path) {
        remember(&state, &path);
    }
    // Re-index this file's links
    let name = vault::page_name(&path);
    state.backlinks.lock().unwrap()
        .index_file(&name, &parser::extract_links(&content, hashtags(&app)));
    Ok(())
}

#[tauri::command]
pub async fn create_page(
    state: State<'_, AppState>,
    page_name: String,
) -> Cmd<FileEntry> {
    let vault_path = state.vault_path.lock().unwrap().clone().ok_or("No vault open")?;
    let file_path = vault::page_path(&vault_path, &page_name);
    if file_path.exists() {
        return Err(format!("Page '{page_name}' already exists"));
    }
    std::fs::write(&file_path, format!("* {page_name}\n"))
        .map_err(|e| e.to_string())?;
    let path = file_path.to_string_lossy().to_string();
    remember(&state, &path);
    Ok(FileEntry { name: page_name, path })
}

/// Delete a page file of the open vault (irreversible).
#[tauri::command]
pub async fn delete_page(state: State<'_, AppState>, path: String) -> Cmd<()> {
    let vault_path = state.vault_path.lock().unwrap().clone().ok_or("No vault open")?;
    let file = std::path::Path::new(&path);
    let pages = std::fs::canonicalize(vault::pages_dir(&vault_path)).map_err(|e| e.to_string())?;
    let inside = std::fs::canonicalize(file).ok()
        .and_then(|f| f.parent().map(|p| p == pages))
        .unwrap_or(false);
    if !inside || file.extension().and_then(|s| s.to_str()) != Some("org") {
        return Err("Not a page of the open project".to_string());
    }
    std::fs::remove_file(file).map_err(|e| e.to_string())?;
    state.snapshot.lock().unwrap().remove(&path);
    state.backlinks.lock().unwrap().remove_source(&vault::page_name(&path));
    Ok(())
}

// ─── Backlinks ───────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_backlinks(
    state: State<'_, AppState>,
    app: AppHandle,
    page_name: String,
) -> Cmd<Vec<String>> {
    let ignore_case = app.path().app_config_dir().ok()
        .map(|d| settings::load(&d).case_insensitive_links())
        .unwrap_or(true);
    let idx = state.backlinks.lock().unwrap();
    Ok(if ignore_case {
        idx.get_backlinks_ignore_case(&page_name)
    } else {
        idx.get_backlinks(&page_name)
    })
}

/// A `[[link]]` whose page does not exist, with the pages using it.
#[derive(serde::Serialize)]
pub struct BrokenLink {
    pub target: String,
    pub sources: Vec<String>,
    /// Total number of times the link appears in the project.
    pub count: usize,
}

#[tauri::command]
pub async fn get_broken_links(
    state: State<'_, AppState>,
    app: AppHandle,
) -> Cmd<Vec<BrokenLink>> {
    let vault_path = state.vault_path.lock().unwrap().clone().ok_or("No vault open")?;
    let ignore_case = app.path().app_config_dir().ok()
        .map(|d| settings::load(&d).case_insensitive_links())
        .unwrap_or(true);
    let norm = |s: &str| if ignore_case { s.trim().to_lowercase() } else { s.trim().to_string() };

    let pages: std::collections::HashSet<String> = vault::list_org_files(&vault_path)
        .map_err(|e| e.to_string())?
        .iter()
        .map(|f| norm(&f.name))
        .collect();

    // Group targets that only differ by case when case is ignored
    let mut grouped: Vec<(String, BrokenLink)> = Vec::new();
    for (target, sources, count) in state.backlinks.lock().unwrap().all_links() {
        if !parser::is_page_link(&target) || pages.contains(&norm(&target)) {
            continue;
        }
        let key = norm(&target);
        match grouped.iter_mut().find(|(k, _)| *k == key) {
            Some((_, b)) => {
                b.count += count;
                for s in sources {
                    if !b.sources.contains(&s) { b.sources.push(s); }
                }
                b.sources.sort();
            }
            None => grouped.push((key, BrokenLink { target: target.trim().to_string(), sources, count })),
        }
    }
    // Most used first; alphabetical among equals
    let mut links: Vec<BrokenLink> = grouped.into_iter().map(|(_, b)| b).collect();
    links.sort_by(|a, b| {
        b.count.cmp(&a.count).then_with(|| a.target.to_lowercase().cmp(&b.target.to_lowercase()))
    });
    Ok(links)
}

// ─── Tags ────────────────────────────────────────────────────────────────────

/// An org-mode tag of the project and how many pages and headlines carry it.
#[derive(serde::Serialize)]
pub struct TagCount {
    pub name: String,
    pub count: usize,
}

/// A page, or a headline of it, matching a tag search.
#[derive(serde::Serialize)]
pub struct TagHit {
    pub page: String,
    pub path: String,
    /// `None` when the whole page matches (`#+FILETAGS:`).
    pub heading: Option<String>,
    pub level: usize,
    pub line: usize,
    /// All its tags, inherited ones included.
    pub tags: Vec<String>,
}

/// Every `:tag:` and `#+FILETAGS:` tag of the project, sorted by name.
#[tauri::command]
pub async fn list_tags(state: State<'_, AppState>) -> Cmd<Vec<TagCount>> {
    let vault_path = state.vault_path.lock().unwrap().clone().ok_or("No vault open")?;
    let mut counts = std::collections::HashMap::<String, usize>::new();
    for f in vault::list_org_files(&vault_path).map_err(|e| e.to_string())? {
        let content = std::fs::read_to_string(&f.path).unwrap_or_default();
        for t in tags::written_tags(&content) { *counts.entry(t).or_default() += 1; }
    }
    let mut list: Vec<TagCount> = counts.into_iter().map(|(name, count)| TagCount { name, count }).collect();
    list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then_with(|| a.name.cmp(&b.name)));
    Ok(list)
}

/// Pages and headlines matching an org-mode tag search (`projet+urgent-perso|idée`).
#[tauri::command]
pub async fn search_tags(state: State<'_, AppState>, query: String) -> Cmd<Vec<TagHit>> {
    let vault_path = state.vault_path.lock().unwrap().clone().ok_or("No vault open")?;
    let Some(query) = tags::Query::parse(&query) else { return Ok(vec![]) };
    let mut hits = Vec::new();
    for f in vault::list_org_files(&vault_path).map_err(|e| e.to_string())? {
        let content = std::fs::read_to_string(&f.path).unwrap_or_default();
        hits.extend(tags::search(&content, &query).into_iter().map(|h| TagHit {
            page: f.name.clone(),
            path: f.path.clone(),
            heading: h.heading,
            level: h.level,
            line: h.line,
            tags: h.tags,
        }));
    }
    Ok(hits)
}

// ─── Export ──────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn export_vault(
    state: State<'_, AppState>,
    app: AppHandle,
) -> Cmd<String> {
    let vault_path = state.vault_path.lock().unwrap().clone().ok_or("No vault open")?;
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    let s = settings::load(&dir);
    let bin = s.logseq_site_builder_path.unwrap_or_else(|| "logseq-site-builder".to_string());

    let out = std::process::Command::new(&bin)
        .arg("build")
        .arg(&vault_path)
        .output()
        .map_err(|e| format!("Cannot run '{bin}': {e}"))?;

    if !out.status.success() {
        return Err(format!(
            "Export failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

// ─── Rename ──────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn rename_page(
    state: State<'_, AppState>,
    app: AppHandle,
    old_path: String,
    new_name: String,
) -> Cmd<FileEntry> {
    let new_name = new_name.trim().to_string();
    if new_name.is_empty() {
        return Err("Name cannot be empty".to_string());
    }
    let old = std::path::Path::new(&old_path);
    let parent = old.parent().ok_or_else(|| "Invalid path".to_string())?;
    let new_path = parent.join(format!("{new_name}.org"));
    if new_path.exists() {
        return Err(format!("Page '{new_name}' already exists"));
    }
    std::fs::rename(&old_path, &new_path).map_err(|e| e.to_string())?;

    // Point every `[[old]]` link of the vault at the new name (if enabled in settings)
    let old_name = vault::page_name(&old_path);
    let cfg = app.path().app_config_dir().ok().map(|d| settings::load(&d));
    let update_links = cfg.as_ref().map(|s| s.update_links_on_rename()).unwrap_or(true);
    let ignore_case = cfg.as_ref().map(|s| s.case_insensitive_links()).unwrap_or(true);
    let hashtags = cfg.as_ref().map(|s| s.hashtags()).unwrap_or(parser::Hashtags::Dashes);
    let vault_path = state.vault_path.lock().unwrap().clone();
    if let (true, Some(vp), true) = (update_links, vault_path, old_name != new_name) {
        for f in vault::list_org_files(&vp).map_err(|e| e.to_string())? {
            let Ok(text) = std::fs::read_to_string(&f.path) else { continue };
            if let Some(updated) = parser::rewrite_links(&text, &old_name, &new_name, ignore_case, hashtags) {
                std::fs::write(&f.path, &updated).map_err(|e| e.to_string())?;
                remember(&state, &f.path);
                state.backlinks.lock().unwrap()
                    .index_file(&f.name, &parser::extract_links(&updated, hashtags));
            }
        }
    }

    // Update backlink index: remove old source, index new name
    let content = std::fs::read_to_string(&new_path).unwrap_or_default();
    let links = parser::extract_links(&content, hashtags);
    {
        let mut idx = state.backlinks.lock().unwrap();
        idx.remove_source(&old_name);
        idx.index_file(&new_name, &links);
    }
    state.snapshot.lock().unwrap().remove(&old_path);
    remember(&state, &new_path.to_string_lossy());

    Ok(FileEntry {
        name: new_name,
        path: new_path.to_string_lossy().to_string(),
    })
}

// ─── Keybindings ─────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_keybindings(app: AppHandle) -> Cmd<keybindings::Keybindings> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    Ok(keybindings::load(&dir))
}

#[tauri::command]
pub async fn set_keybindings(app: AppHandle, keybindings: keybindings::Keybindings) -> Cmd<()> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    keybindings::save(&dir, &keybindings).map_err(|e| e.to_string())
}

// ─── Dialog ──────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn pick_folder(app: AppHandle) -> Cmd<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel::<Option<String>>();
    app.dialog().file().pick_folder(move |folder| {
        let _ = tx.send(folder.map(|f| f.to_string()));
    });
    rx.await.map_err(|_| "dialog cancelled".to_string())
}

// ─── Application ─────────────────────────────────────────────────────────────

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}

/// When the application exits: delete the blank pages of the open project,
/// as chosen in the settings.
pub fn on_exit(app: &AppHandle) {
    let Ok(dir) = app.path().app_config_dir() else { return };
    let s = settings::load(&dir);
    let vault_path = app.state::<AppState>().vault_path.lock().unwrap().clone();
    if let Some(vp) = vault_path {
        vault::delete_blank_pages(
            &vp,
            s.delete_empty_pages.unwrap_or(false),
            s.delete_title_only_pages.unwrap_or(false),
        );
    }
}
