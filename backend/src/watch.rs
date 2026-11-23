//! Watch a folder for the changes made on disk (another editor, a sync tool, a
//! commit…), and tell what they touch.

use std::path::Path;
use std::time::Duration;

use notify_debouncer_mini::{new_debouncer, notify::RecommendedWatcher, notify::RecursiveMode, DebounceEventResult, DebouncedEvent, Debouncer};

use crate::error::AppError;

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
pub fn touched<'a>(paths: impl Iterator<Item = &'a Path>) -> (bool, bool) {
    let is_page = |p: &Path| p.extension().and_then(|e| e.to_str()).and_then(orgamnesia_core::Format::from_ext).is_some();
    let in_git = |p: &Path| p.components().any(|c| c.as_os_str() == ".git");
    paths.fold((false, false), |(pages, git), p| (pages || (is_page(p) && !in_git(p)), git || in_git(p)))
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
        // The folder's own change may come in a batch of its own first
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let page_seen = std::iter::from_fn(|| rx.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now())).ok())
            .any(|(pages, _)| pages);
        assert!(page_seen);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
