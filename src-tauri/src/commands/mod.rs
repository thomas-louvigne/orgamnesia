//! The commands the interface calls. Each one only hands the request to the
//! project (`project.rs`), the settings or a tool, and returns its result.

use orgamnesia_core::{BrokenLink, FileEntry, GitStatus, TagCount, TagHit};
use tauri::{AppHandle, Manager, State};

use crate::{
    error::AppError,
    keybindings,
    project::Project,
    settings::{self, Session, Settings},
    watch, AppState,
};

type Cmd<T> = Result<T, AppError>;

fn config_dir(app: &AppHandle) -> Cmd<std::path::PathBuf> {
    app.path().app_config_dir().map_err(|e| AppError::Settings(e.to_string()))
}

/// Write `settings` to `settings.json` and keep them as the ones in effect.
fn store_settings(state: &AppState, app: &AppHandle, settings: Settings) -> Cmd<()> {
    settings::save(&config_dir(app)?, &settings)?;
    *state.settings.lock().unwrap() = settings;
    Ok(())
}

// ─── Settings ────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Cmd<Settings> {
    Ok(state.settings())
}

#[tauri::command]
pub async fn set_settings(state: State<'_, AppState>, app: AppHandle, mut settings: Settings) -> Cmd<()> {
    // The settings window doesn't know the session: keep it
    settings.session = state.settings().session;
    let hashtags = settings.prefs().hashtags();
    let hashtags_changed = state.prefs().hashtags() != hashtags;
    store_settings(&state, &app, settings)?;
    // `#tags` are read differently (as links or not, with or without `-`): read the links again
    if hashtags_changed {
        if let Some(p) = state.project.lock().unwrap().as_mut() {
            p.reindex(hashtags);
        }
    }
    Ok(())
}

/// The pages and panes open now, written to `settings.json` on quit.
/// Not async: calls run in order, so the last state sent is the one kept.
#[tauri::command]
pub fn set_session(state: State<'_, AppState>, session: Option<Session>) {
    state.settings.lock().unwrap().session = session;
}

#[tauri::command]
pub async fn get_keybindings(app: AppHandle) -> Cmd<keybindings::Keybindings> {
    Ok(keybindings::load(&config_dir(&app)?))
}

#[tauri::command]
pub async fn set_keybindings(app: AppHandle, keybindings: keybindings::Keybindings) -> Cmd<()> {
    keybindings::save(&config_dir(&app)?, &keybindings)
}

// ─── Projects ────────────────────────────────────────────────────────────────

/// Open the folder `path` as the project; returns its pages.
#[tauri::command]
pub async fn open_vault(state: State<'_, AppState>, app: AppHandle, path: String) -> Cmd<Vec<FileEntry>> {
    let project = Project::open(&path, state.prefs().hashtags())?;
    let files = project.files();

    // Remember it as the open project, first of the list
    let mut s = state.settings();
    s.vault_path = Some(path.clone());
    s.projects.retain(|p| p != &path);
    s.projects.insert(0, path.clone());
    store_settings(&state, &app, s)?;

    *state.project.lock().unwrap() = Some(project);
    // Without watching, outside changes are only seen when the project is reopened
    *state.watcher.lock().unwrap() = watch::watch(app.clone(), &path).ok();
    Ok(files)
}

/// Forget a project (the folder itself is left untouched); returns the remaining list.
#[tauri::command]
pub async fn remove_project(state: State<'_, AppState>, app: AppHandle, path: String) -> Cmd<Vec<String>> {
    let mut s = state.settings();
    s.projects.retain(|p| p != &path);
    let projects = s.projects.clone();
    store_settings(&state, &app, s)?;
    Ok(projects)
}

// ─── Pages ───────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn read_file(state: State<'_, AppState>, path: String) -> Cmd<String> {
    state.with_project(|p| p.read(&path))
}

#[tauri::command]
pub async fn write_file(state: State<'_, AppState>, path: String, content: String) -> Cmd<()> {
    let hashtags = state.prefs().hashtags();
    state.with_project(|p| p.write(&path, content, hashtags))
}

#[tauri::command]
pub async fn create_page(state: State<'_, AppState>, page_name: String) -> Cmd<FileEntry> {
    let hashtags = state.prefs().hashtags();
    state.with_project(|p| p.create(&page_name, hashtags))
}

/// Delete a page of the open project (irreversible).
#[tauri::command]
pub async fn delete_page(state: State<'_, AppState>, path: String) -> Cmd<()> {
    state.with_project(|p| p.delete(&path))
}

#[tauri::command]
pub async fn rename_page(state: State<'_, AppState>, old_path: String, new_name: String) -> Cmd<FileEntry> {
    let prefs = state.prefs();
    state.with_project(|p| p.rename(&old_path, &new_name, &prefs))
}

// ─── Links and tags ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_backlinks(state: State<'_, AppState>, page_name: String) -> Cmd<Vec<String>> {
    let ignore_case = state.prefs().case_insensitive_links;
    state.with_project(|p| Ok(p.backlinks(&page_name, ignore_case)))
}

#[tauri::command]
pub async fn get_broken_links(state: State<'_, AppState>) -> Cmd<Vec<BrokenLink>> {
    let ignore_case = state.prefs().case_insensitive_links;
    state.with_project(|p| Ok(p.broken_links(ignore_case)))
}

#[tauri::command]
pub async fn list_tags(state: State<'_, AppState>) -> Cmd<Vec<TagCount>> {
    state.with_project(|p| Ok(p.tags()))
}

#[tauri::command]
pub async fn search_tags(state: State<'_, AppState>, query: String) -> Cmd<Vec<TagHit>> {
    state.with_project(|p| Ok(p.search_tags(&query)))
}

// ─── Extensions ──────────────────────────────────────────────────────────────

/// Build the site with logseq-site-builder; returns its output.
#[tauri::command]
pub async fn export_vault(state: State<'_, AppState>) -> Cmd<String> {
    let vault_path = state.with_project(|p| Ok(p.path.clone()))?;
    let bin = state.settings().logseq_site_builder_path
        .unwrap_or_else(|| "logseq-site-builder".to_string());
    let out = std::process::Command::new(&bin)
        .arg("build")
        .arg(&vault_path)
        .output()
        .map_err(|e| AppError::Export(format!("cannot run '{bin}': {e}")))?;
    if !out.status.success() {
        return Err(AppError::Export(String::from_utf8_lossy(&out.stderr).to_string()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// Git state of a project folder (git extension); `None` when git can't be run.
#[tauri::command]
pub async fn git_status(path: String) -> Cmd<Option<GitStatus>> {
    Ok(crate::git::status(&path))
}

// ─── Application ─────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn pick_folder(app: AppHandle) -> Cmd<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel::<Option<String>>();
    app.dialog().file().pick_folder(move |folder| {
        let _ = tx.send(folder.map(|f| f.to_string()));
    });
    rx.await.map_err(|_| AppError::Dialog)
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}

/// When the application exits: delete the blank pages of the open project and
/// save the open pages and panes, as chosen in the settings.
pub fn on_exit(app: &AppHandle) {
    let state = app.state::<AppState>();
    let prefs = state.prefs();
    let path = state.project.lock().unwrap().as_ref().map(|p| p.path.clone());
    if let Some(path) = path {
        crate::vault::delete_blank_pages(&path, prefs.delete_empty, prefs.delete_title_only);
    }
    let mut s = state.settings();
    if !prefs.restore_session {
        s.session = None;
    }
    if let Ok(dir) = config_dir(app) {
        let _ = settings::save(&dir, &s);
    }
}
