pub mod commands;
pub mod error;
pub mod git;
pub mod index;
pub mod keybindings;
pub mod parser;
pub mod project;
pub mod settings;
pub mod tags;
pub mod vault;
pub mod watch;

use std::sync::Mutex;

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
        self.settings.lock().unwrap().clone()
    }

    pub fn prefs(&self) -> Prefs {
        self.settings.lock().unwrap().prefs()
    }

    /// Run `f` on the open project.
    pub fn with_project<T>(&self, f: impl FnOnce(&mut Project) -> Result<T, AppError>) -> Result<T, AppError> {
        let mut project = self.project.lock().unwrap();
        f(project.as_mut().ok_or(AppError::NoProject)?)
    }
}

impl Default for AppState {
    fn default() -> Self { Self::new() }
}
