//! The application: file dialogs, images, quitting.

use tauri::{AppHandle, Manager, State};

use super::{config_dir, Cmd};
use crate::{error::AppError, settings, AppState};

/// The answer of a dialog `show` opens, handing it the sender of its answer.
async fn dialog<T>(show: impl FnOnce(tokio::sync::oneshot::Sender<T>)) -> Cmd<T> {
    let (tx, rx) = tokio::sync::oneshot::channel::<T>();
    show(tx);
    rx.await.map_err(|_| AppError::Dialog)
}

/// Folder picker, titled `title` when there is one. The folder may be created
/// there (or named when it doesn't exist yet: then created); its path.
#[tauri::command]
pub async fn pick_folder(app: AppHandle, title: Option<String>) -> Cmd<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    let mut picker = app.dialog().file().set_can_create_directories(true);
    if let Some(title) = title { picker = picker.set_title(title); }
    let picked = dialog(|tx| picker.pick_folder(move |folder| {
        let _ = tx.send(folder.map(|f| f.to_string()));
    })).await?;
    // The interface splits paths by `/`
    let picked = picked.map(crate::slash);
    picked.map(|path| ensure_folder(&path).map(|()| path)).transpose()
}

/// `path` is a folder: created when there is nothing there.
fn ensure_folder(path: &str) -> Cmd<()> {
    let p = std::path::Path::new(path);
    if p.exists() && !p.is_dir() {
        return Err(AppError::NotAFolder(path.to_string()));
    }
    Ok(std::fs::create_dir_all(p)?)
}

/// File picker (for the SSH key), opening in `start` when it exists.
#[tauri::command]
pub async fn pick_file(app: AppHandle, start: Option<String>) -> Cmd<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    let mut picker = app.dialog().file();
    if let Some(dir) = start.map(|d| crate::expand_home(&d)).filter(|d| std::path::Path::new(d).is_dir()) {
        picker = picker.set_directory(dir);
    }
    dialog(|tx| picker.pick_file(move |file| {
        let _ = tx.send(file.map(|f| crate::slash(f.to_string())));
    })).await
}

/// File picker for an image to add to a page.
#[tauri::command]
pub async fn pick_image(app: AppHandle) -> Cmd<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    dialog(|tx| app.dialog().file()
        .add_filter("Images", orgamnesia_core::assets::EXTENSIONS)
        .pick_file(move |file| { let _ = tx.send(file.map(|f| crate::slash(f.to_string()))); })).await
}

/// Copy the image `source` into the project's assets; returns the link to it from `page`.
#[tauri::command]
pub async fn import_image(state: State<'_, AppState>, page: String, source: String) -> Cmd<String> {
    state.with_project(|p| p.import_image(&page, &source))
}

/// Copy the file `source` into the project's assets; returns the link to it from `page`.
#[tauri::command]
pub async fn import_file(state: State<'_, AppState>, page: String, source: String) -> Cmd<String> {
    state.with_project(|p| p.import_file(&page, &source))
}

/// Open the file a link points to (`~/`: the home folder) with the system's
/// application for it. Programs and launchers are refused: a link in a note
/// (maybe one cloned from elsewhere) must never run anything.
#[tauri::command]
pub fn open_file_link(path: String) -> Cmd<()> {
    let path = crate::expand_home(&path);
    let p = std::path::Path::new(&path);
    if !p.exists() {
        return Err(AppError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, format!("not found: {path}"))));
    }
    if runnable(p) {
        return Err(AppError::Io(std::io::Error::new(std::io::ErrorKind::PermissionDenied, format!("a program, not opened: {path}"))));
    }
    crate::open_with_system(&path)
}

/// A file that would be run rather than shown: executable, or a launcher.
fn runnable(p: &std::path::Path) -> bool {
    const LAUNCHERS: &[&str] = &[
        "desktop", "sh", "bash", "zsh", "fish", "py", "pyw", "pl", "rb", "appimage", "jar", "command", "app", "run", "bin",
        // Windows: programs, scripts, shortcuts, installers, settings
        "exe", "bat", "cmd", "com", "msi", "msp", "scr", "pif", "cpl", "msc", "hta", "lnk", "url", "reg", "inf",
        "ps1", "psm1", "psd1", "vbs", "vbe", "js", "jse", "wsf", "wsh", "appref-ms", "application", "gadget", "diagcab",
    ];
    let ext = p.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
    if ext.is_some_and(|e| LAUNCHERS.contains(&e.as_str())) { return true; }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if p.is_file() && std::fs::metadata(p).is_ok_and(|m| m.permissions().mode() & 0o111 != 0) { return true; }
    }
    false
}

/// Of `paths`, those that are neither files nor folders on the disk (`~/`: the home
/// folder), to mark the links to them.
#[tauri::command]
pub async fn missing_files(paths: Vec<String>) -> Cmd<Vec<String>> {
    Ok(paths.into_iter().filter(|p| !std::path::Path::new(&crate::expand_home(p)).exists()).collect())
}

/// The bytes of the image at `path` (`~/`: the home folder), for the page to show it:
/// the webview can't load it from the asset protocol when the page comes from another
/// origin (`http://localhost` while developing, WebKitGTK).
#[tauri::command]
pub async fn read_image(path: String) -> Cmd<tauri::ipc::Response> {
    if !orgamnesia_core::assets::EXTENSIONS.iter().any(|e| path.to_ascii_lowercase().ends_with(&format!(".{e}"))) {
        return Err(AppError::Io(std::io::Error::new(std::io::ErrorKind::InvalidInput, "not an image")));
    }
    Ok(tauri::ipc::Response::new(std::fs::read(crate::expand_home(&path)).map_err(AppError::Io)?))
}

/// Open the web page `url` (`https://` or `http://` only) in the browser.
#[tauri::command]
pub fn open_url(url: String) -> Cmd<()> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(AppError::Io(std::io::Error::new(std::io::ErrorKind::InvalidInput, "not a web page")));
    }
    crate::open_with_system(&url)
}

/// The save dialog for a page exported as HTML (offered as `<name>.html`), then
/// the file `html` written there: its path, `None` when the dialog was cancelled.
#[tauri::command]
pub async fn save_html(app: AppHandle, name: String, html: String) -> Cmd<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    let picker = app.dialog().file().set_file_name(format!("{name}.html")).add_filter("HTML", &["html", "htm"]);
    let picked = dialog(|tx| picker.save_file(move |file| { let _ = tx.send(file.map(|f| f.to_string())); })).await?;
    let Some(path) = picked else { return Ok(None) };
    std::fs::write(&path, html)?;
    Ok(Some(crate::slash(path)))
}

/// The system's print dialog for the window: the page the interface made ready
/// to print can be saved as a PDF there.
/// Async: run on the main thread, the command would hold Tauri's lock on the
/// webviews while the dialog runs its own event loop, and the next call from the
/// interface, waiting for that lock, would stop the app (seen on Linux).
#[tauri::command]
pub async fn print_window(window: tauri::WebviewWindow) -> Cmd<()> {
    window.print().map_err(|e| AppError::CannotRun { program: "print".into(), error: e.to_string() })
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}

/// When the application exits: delete the blank pages of the open project and
/// save the open pages and panes, as chosen in the settings.
pub fn on_exit(app: &AppHandle) {
    if let Ok(dir) = config_dir(app) {
        save_on_exit(&app.state::<AppState>(), &dir);
    }
}

fn save_on_exit(state: &AppState, dir: &std::path::Path) {
    let prefs = state.prefs();
    let path = crate::lock(&state.project).as_ref().map(|p| p.path.clone());
    if let Some(path) = path {
        crate::project::files::delete_blank_pages(&path, prefs.delete_empty, prefs.delete_title_only);
    }
    let mut s = state.settings();
    if !prefs.restore_session {
        s.session = None;
    }
    let _ = settings::save_state(dir, &s);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::testing::{folder, open_project};

    #[test]
    fn a_folder_picked_is_one() {
        let dir = folder("cmd-ensure-folder");
        let new = dir.join("notes/neuves");
        ensure_folder(&new.to_string_lossy()).unwrap();
        assert!(new.is_dir());
        ensure_folder(&new.to_string_lossy()).unwrap();
        std::fs::write(dir.join("fichier"), "").unwrap();
        assert!(matches!(ensure_folder(&dir.join("fichier").to_string_lossy()), Err(AppError::NotAFolder(_))));
    }

    #[test]
    fn on_exit_blank_pages_go_and_the_session_is_kept_as_set() {
        let state = AppState::new();
        let path = open_project(&state, "cmd-exit", &[("Vide", "  \n"), ("Une", "* Une\ntexte\n")]);
        let config = folder("cmd-exit-config");
        {
            let mut s = crate::lock(&state.settings);
            s.config.delete_empty_pages = Some(true);
            s.session = Some(orgamnesia_core::Session { project: path.clone(), ..Default::default() });
        }
        save_on_exit(&state, &config);
        assert!(!std::path::Path::new(&path).join("pages/Vide.org").exists());
        assert!(std::fs::read_to_string(config.join("state.json")).unwrap().contains("\"session\": {"));
        // Not to be restored: not kept
        crate::lock(&state.settings).config.restore_session = Some(false);
        save_on_exit(&state, &config);
        assert!(std::fs::read_to_string(config.join("state.json")).unwrap().contains("\"session\": null"));
    }
}
