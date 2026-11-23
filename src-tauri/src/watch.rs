//! Watch the open project's folder, and tell the interface when its pages or
//! its git repository change on disk (another editor, a sync tool, a commit…).

use std::path::Path;

use tauri::{AppHandle, Emitter, Manager};

pub use orgamnesia_backend::watch::*;

use crate::{error::AppError, AppState};

/// Event carrying the `VaultChanges` of pages modified outside the app.
pub const VAULT_CHANGED: &str = "vault-changed";
/// Event telling that the git state of the project may have changed.
pub const GIT_CHANGED: &str = "git-changed";

pub fn watch(app: AppHandle, path: &str) -> Result<Watcher, AppError> {
    let project_path = path.to_string();
    start(Path::new(path), move |events| {
        let (pages, git) = touched(events.iter().map(|e| e.path.as_path()));

        if pages {
            let state = app.state::<AppState>();
            let tags = state.prefs().tags();
            let changes = crate::lock(&state.project).as_mut()
                // Another project may have been opened meanwhile
                .filter(|p| p.path == project_path)
                .and_then(|p| p.refresh(tags).ok().flatten());
            if let Some(changes) = changes {
                let _ = app.emit(VAULT_CHANGED, changes);
            }
        }
        // A page in conflict saved without its markers is resolved; skipped while
        // a git command writes to the repository
        if pages && let Ok(_one) = crate::git::WRITING.try_lock() {
            crate::git::mark_resolved(&project_path);
        }
        // Editing a page changes what there is to commit
        if pages || git {
            let _ = app.emit(GIT_CHANGED, ());
        }
    })
}
