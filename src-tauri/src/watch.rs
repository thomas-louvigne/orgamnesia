//! Watch the open project's folder, and tell the interface when its pages or
//! its git repository change on disk (another editor, a sync tool, a commit…).

use std::path::Path;
use std::time::Duration;

use notify_debouncer_mini::{new_debouncer, notify::RecommendedWatcher, notify::RecursiveMode, DebounceEventResult, Debouncer};
use tauri::{AppHandle, Emitter, Manager};

use crate::{error::AppError, AppState};

/// Event carrying the `VaultChanges` of pages modified outside the app.
pub const VAULT_CHANGED: &str = "vault-changed";
/// Event telling that the git state of the project may have changed.
pub const GIT_CHANGED: &str = "git-changed";

/// Keeps watching while it is alive.
pub type Watcher = Debouncer<RecommendedWatcher>;

pub fn watch(app: AppHandle, path: &str) -> Result<Watcher, AppError> {
    let project_path = path.to_string();
    let mut debouncer = new_debouncer(Duration::from_millis(300), move |res: DebounceEventResult| {
        let Ok(events) = res else { return };
        let is_page = |p: &Path| p.extension().and_then(|e| e.to_str()) == Some("org");
        let in_git = |p: &Path| p.components().any(|c| c.as_os_str() == ".git");
        let pages = events.iter().any(|e| is_page(&e.path) && !in_git(&e.path));
        let git = events.iter().any(|e| in_git(&e.path));

        if pages {
            let state = app.state::<AppState>();
            let tags = state.prefs().tags();
            let changes = state.project.lock().unwrap().as_mut()
                // Another project may have been opened meanwhile
                .filter(|p| p.path == project_path)
                .and_then(|p| p.refresh(tags).ok().flatten());
            if let Some(changes) = changes {
                let _ = app.emit(VAULT_CHANGED, changes);
            }
        }
        // Editing a page changes what there is to commit
        if pages || git {
            let _ = app.emit(GIT_CHANGED, ());
        }
    }).map_err(|e| AppError::Io(std::io::Error::other(e)))?;
    debouncer.watcher()
        .watch(Path::new(path), RecursiveMode::Recursive)
        .map_err(|e| AppError::Io(std::io::Error::other(e)))?;
    Ok(debouncer)
}
