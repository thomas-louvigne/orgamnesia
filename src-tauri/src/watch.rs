//! Watch the open project's folder, and tell the interface when its pages or
//! its git repository change on disk (another editor, a sync tool, a commit…).

use std::path::Path;
use std::time::Duration;

use notify_debouncer_mini::{new_debouncer, notify::RecommendedWatcher, notify::RecursiveMode, DebounceEventResult, DebouncedEvent, Debouncer};
use tauri::{AppHandle, Emitter, Manager};

use crate::{error::AppError, AppState};

/// Event carrying the `VaultChanges` of pages modified outside the app.
pub const VAULT_CHANGED: &str = "vault-changed";
/// Event telling that the git state of the project may have changed.
pub const GIT_CHANGED: &str = "git-changed";

/// Keeps watching while it is alive.
pub type Watcher = Debouncer<RecommendedWatcher>;

/// Watch the folder `path` and what it holds, `on_change` getting the changes
/// (gathered over 300 ms).
pub fn start(path: &Path, mut on_change: impl FnMut(Vec<DebouncedEvent>) + Send + 'static) -> Result<Watcher, AppError> {
    let mut debouncer = new_debouncer(Duration::from_millis(300), move |res: DebounceEventResult| {
        if let Ok(events) = res { on_change(events); }
    }).map_err(|e| AppError::Io(std::io::Error::other(e)))?;
    debouncer.watcher()
        .watch(path, RecursiveMode::Recursive)
        .map_err(|e| AppError::Io(std::io::Error::other(e)))?;
    Ok(debouncer)
}

/// What the changed `paths` touch: pages (`.org`, `.md` out of `.git/`), the git repository.
fn touched<'a>(paths: impl Iterator<Item = &'a Path>) -> (bool, bool) {
    let is_page = |p: &Path| p.extension().and_then(|e| e.to_str()).and_then(orgamnesia_core::Format::from_ext).is_some();
    let in_git = |p: &Path| p.components().any(|c| c.as_os_str() == ".git");
    paths.fold((false, false), |(pages, git), p| (pages || (is_page(p) && !in_git(p)), git || in_git(p)))
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_and_git_changes() {
        let t = |paths: &[&str]| touched(paths.iter().map(Path::new));
        assert_eq!(t(&["/p/pages/Note.org"]), (true, false));
        assert_eq!(t(&["/p/journals/2026-11-18.md", "/p/.git/index"]), (true, true));
        assert_eq!(t(&["/p/.git/COMMIT_EDITMSG.org"]), (false, true));
        assert_eq!(t(&["/p/assets/a.png"]), (false, false));
    }

    #[test]
    fn a_change_on_disk_is_seen() {
        let dir = std::env::temp_dir().join(format!("orgamnesia-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("journals")).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let _w = start(&dir, move |events| { let _ = tx.send(touched(events.iter().map(|e| e.path.as_path()))); }).unwrap();
        // In a folder of the project too
        std::fs::write(dir.join("journals/2026-11-18.org"), "* x\n").unwrap();
        let seen = rx.recv_timeout(Duration::from_secs(5)).expect("a change event");
        assert!(seen.0);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
