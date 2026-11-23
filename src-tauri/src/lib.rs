pub mod commands;
pub mod config_files;
pub mod tui;
pub mod update;
pub mod watch;

use std::sync::Mutex;

pub use orgamnesia_backend::{
    command, error, expand_home, git, git_program, home_dir, keybindings, lock, open_with_system,
    org_bloom, project, resource_dir, set_resource_dir, settings, slash, spell, tags, write_atomic,
};

use error::AppError;
use project::Project;
use settings::{Prefs, Settings};

pub struct AppState {
    /// The open project, if any.
    pub project: Mutex<Option<Project>>,
    /// The settings, as last read from or written to `settings.yaml` and `state.json`.
    pub settings: Mutex<Settings>,
    /// The configuration files: the text in effect of each, what is wrong in them.
    pub config: Mutex<config_files::ConfigState>,
    /// Watches the configuration folder.
    pub config_watcher: Mutex<Option<watch::Watcher>>,
    /// Watches the open project's folder.
    pub watcher: Mutex<Option<watch::Watcher>>,
    /// The dictionary of the spelling extension.
    pub spell: Mutex<spell::Speller>,
    /// The new version found, until it is installed.
    pub update: Mutex<Option<tauri_plugin_updater::Update>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            project: Mutex::new(None),
            settings: Mutex::new(Settings::default()),
            config: Mutex::new(config_files::ConfigState::default()),
            config_watcher: Mutex::new(None),
            watcher: Mutex::new(None),
            spell: Mutex::new(spell::Speller::default()),
            update: Mutex::new(None),
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
