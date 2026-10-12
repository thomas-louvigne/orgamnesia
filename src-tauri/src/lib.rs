pub mod commands;
pub mod error;
pub mod git;
pub mod index;
pub mod keybindings;
pub mod org_bloom;
pub mod parser;
pub mod project;
pub mod settings;
pub mod tags;
pub mod vault;
pub mod watch;

use std::sync::{Mutex, MutexGuard};

/// Lock `m`, even when a command panicked while holding it: the state it guards
/// is still usable, and every later command would panic too otherwise.
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

use error::AppError;
use project::Project;
use settings::{Prefs, Settings};

pub struct AppState {
    /// The open project, if any.
    pub project: Mutex<Option<Project>>,
    /// The settings, as last read from or written to `settings.json`.
    pub settings: Mutex<Settings>,
    /// Watches the open project's folder.
    pub watcher: Mutex<Option<watch::Watcher>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            project: Mutex::new(None),
            settings: Mutex::new(Settings::default()),
            watcher: Mutex::new(None),
        }
    }

    pub fn settings(&self) -> Settings {
        lock(&self.settings).clone()
    }

    pub fn prefs(&self) -> Prefs {
        lock(&self.settings).prefs()
    }

    /// Run `f` on the open project.
    pub fn with_project<T>(&self, f: impl FnOnce(&mut Project) -> Result<T, AppError>) -> Result<T, AppError> {
        let mut project = lock(&self.project);
        f(project.as_mut().ok_or(AppError::NoProject)?)
    }
}

impl Default for AppState {
    fn default() -> Self { Self::new() }
}
