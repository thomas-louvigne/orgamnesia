//! The application: file dialogs, images, quitting.

use tauri::{AppHandle, Manager, State};

use super::{config_dir, Cmd};
use crate::{error::AppError, settings, AppState};

#[tauri::command]
pub async fn pick_folder(app: AppHandle) -> Cmd<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel::<Option<String>>();
    app.dialog().file().pick_folder(move |folder| {
        let _ = tx.send(folder.map(|f| f.to_string()));
    });
    rx.await.map_err(|_| AppError::Dialog)
}

/// Save dialog naming the folder of a new project, titled `title`: the folder is
/// created (kept when it already is one) and its path returned.
#[tauri::command]
pub async fn pick_new_folder(app: AppHandle, title: String) -> Cmd<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel::<Option<String>>();
    app.dialog().file().set_title(title).save_file(move |folder| {
        let _ = tx.send(folder.map(|f| f.to_string()));
    });
    let Some(path) = rx.await.map_err(|_| AppError::Dialog)? else { return Ok(None) };
    if std::path::Path::new(&path).exists() && !std::path::Path::new(&path).is_dir() {
        return Err(AppError::NotAFolder(path));
    }
    std::fs::create_dir_all(&path)?;
    Ok(Some(path))
}

/// File picker (for the SSH key), opening in `start` when it exists.
#[tauri::command]
pub async fn pick_file(app: AppHandle, start: Option<String>) -> Cmd<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel::<Option<String>>();
    let mut dialog = app.dialog().file();
    let home = std::env::var("HOME").unwrap_or_default();
    let start = start.map(|d| match d.strip_prefix("~/") { Some(rest) => format!("{home}/{rest}"), None => d });
    if let Some(dir) = start.filter(|d| std::path::Path::new(d).is_dir()) {
        dialog = dialog.set_directory(dir);
    }
    dialog.pick_file(move |file| {
        let _ = tx.send(file.map(|f| f.to_string()));
    });
    rx.await.map_err(|_| AppError::Dialog)
}

/// File picker for an image to add to a page.
#[tauri::command]
pub async fn pick_image(app: AppHandle) -> Cmd<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel::<Option<String>>();
    app.dialog().file()
        .add_filter("Images", orgamnesia_core::assets::EXTENSIONS)
        .pick_file(move |file| { let _ = tx.send(file.map(|f| f.to_string())); });
    rx.await.map_err(|_| AppError::Dialog)
}

/// Copy the image `source` into the project's assets; returns the link to it from `page`.
#[tauri::command]
pub async fn import_image(state: State<'_, AppState>, page: String, source: String) -> Cmd<String> {
    state.with_project(|p| p.import_image(&page, &source))
}

/// Of `paths`, those that are neither files nor folders on the disk (`~/`: the home
/// folder), to mark the links to them.
#[tauri::command]
pub async fn missing_files(paths: Vec<String>) -> Cmd<Vec<String>> {
    let home = std::env::var("HOME").unwrap_or_default();
    Ok(paths.into_iter().filter(|p| {
        let path = match p.strip_prefix("~/") { Some(rest) => format!("{home}/{rest}"), None => p.clone() };
        !std::path::Path::new(&path).exists()
    }).collect())
}

/// The bytes of the image at `path` (`~/`: the home folder), for the page to show it:
/// the webview can't load it from the asset protocol when the page comes from another
/// origin (`http://localhost` while developing, WebKitGTK).
#[tauri::command]
pub async fn read_image(path: String) -> Cmd<tauri::ipc::Response> {
    if !orgamnesia_core::assets::EXTENSIONS.iter().any(|e| path.to_ascii_lowercase().ends_with(&format!(".{e}"))) {
        return Err(AppError::Io(std::io::Error::new(std::io::ErrorKind::InvalidInput, "not an image")));
    }
    let home = std::env::var("HOME").unwrap_or_default();
    let path = match path.strip_prefix("~/") { Some(rest) => format!("{home}/{rest}"), None => path };
    Ok(tauri::ipc::Response::new(std::fs::read(path).map_err(AppError::Io)?))
}

/// Open the web page `url` (`https://` only) in the browser.
#[tauri::command]
pub fn open_url(url: String) -> Cmd<()> {
    if !url.starts_with("https://") {
        return Err(AppError::Io(std::io::Error::new(std::io::ErrorKind::InvalidInput, "not a web page")));
    }
    std::process::Command::new("xdg-open").arg(&url).spawn()
        .map(|_| ())
        .map_err(|e| AppError::CannotRun { program: "xdg-open".into(), error: e.to_string() })
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
    let path = crate::lock(&state.project).as_ref().map(|p| p.path.clone());
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
