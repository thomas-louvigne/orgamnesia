use tauri::{AppHandle, Manager, State};

use crate::{
    AppState,
    index::backlinks::BacklinkIndex,
    keybindings,
    parser,
    settings,
    vault::{self, FileEntry},
};

type Cmd<T> = Result<T, String>;

// ─── Settings ────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_settings(app: AppHandle) -> Cmd<settings::Settings> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    Ok(settings::load(&dir))
}

#[tauri::command]
pub async fn set_settings(app: AppHandle, settings: settings::Settings) -> Cmd<()> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    settings::save(&dir, &settings).map_err(|e| e.to_string())
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
    settings::save(&dir, &s).map_err(|e| e.to_string())?;

    let files = vault::list_org_files(&path).map_err(|e| e.to_string())?;

    // Rebuild backlink index
    let mut idx = BacklinkIndex::new();
    for f in &files {
        let content = std::fs::read_to_string(&f.path).unwrap_or_default();
        idx.index_file(&f.name, &parser::extract_links(&content));
    }

    *state.vault_path.lock().unwrap() = Some(path);
    *state.backlinks.lock().unwrap() = idx;

    Ok(files)
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
    path: String,
    content: String,
) -> Cmd<()> {
    std::fs::write(&path, &content).map_err(|e| e.to_string())?;
    // Re-index this file's links
    let name = vault::page_name(&path);
    state.backlinks.lock().unwrap()
        .index_file(&name, &parser::extract_links(&content));
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
    Ok(FileEntry {
        name: page_name,
        path: file_path.to_string_lossy().to_string(),
    })
}

// ─── Backlinks ───────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_backlinks(
    state: State<'_, AppState>,
    page_name: String,
) -> Cmd<Vec<String>> {
    Ok(state.backlinks.lock().unwrap().get_backlinks(&page_name))
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

    // Update backlink index: remove old source, index new name
    let old_name = vault::page_name(&old_path);
    let content = std::fs::read_to_string(&new_path).unwrap_or_default();
    let links = parser::extract_links(&content);
    {
        let mut idx = state.backlinks.lock().unwrap();
        idx.remove_source(&old_name);
        idx.index_file(&new_name, &links);
    }

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
