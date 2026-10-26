pub mod commands;
pub mod config_files;
pub mod error;
pub mod git;
pub mod index;
pub mod keybindings;
pub mod org_bloom;
pub mod parser;
pub mod project;
pub mod settings;
pub mod spell;
pub mod tags;
pub mod vault;
pub mod watch;

use std::sync::{Mutex, MutexGuard};

/// Lock `m`, even when a command panicked while holding it: the state it guards
/// is still usable, and every later command would panic too otherwise.
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// `path` with a leading `~/` made the home folder (`~/x` → `/home/me/x`).
pub fn expand_home(path: &str) -> String {
    match (path.strip_prefix("~/"), std::env::var("HOME")) {
        (Some(rest), Ok(home)) => format!("{home}/{rest}"),
        _ => path.to_string(),
    }
}

/// Open `target` (a folder, a web page) with the system's program for it:
/// `open` on macOS, `xdg-open` elsewhere.
pub fn open_with_system(target: &str) -> Result<(), AppError> {
    let program = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    std::process::Command::new(program).arg(target).spawn()
        .map(|_| ())
        .map_err(|e| AppError::CannotRun { program: program.into(), error: e.to_string() })
}

use error::AppError;
use project::Project;
use settings::{Prefs, Settings};

pub struct AppState {
    /// The open project, if any.
    pub project: Mutex<Option<Project>>,
    /// The settings, as last read from or written to `settings.json` and `state.json`.
    pub settings: Mutex<Settings>,
    /// The configuration files: the text in effect of each, what is wrong in them.
    pub config: Mutex<config_files::ConfigState>,
    /// Watches the configuration folder.
    pub config_watcher: Mutex<Option<watch::Watcher>>,
    /// Watches the open project's folder.
    pub watcher: Mutex<Option<watch::Watcher>>,
    /// The dictionary of the spelling extension.
    pub spell: Mutex<spell::Speller>,
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
