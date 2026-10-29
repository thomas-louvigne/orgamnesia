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
pub mod update;
pub mod vault;
pub mod watch;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, MutexGuard, OnceLock};

/// Lock `m`, even when a command panicked while holding it: the state it guards
/// is still usable, and every later command would panic too otherwise.
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// The home folder: `$HOME`, else `%USERPROFILE%` (Windows).
pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).filter(|h| !h.is_empty()).map(PathBuf::from)
}

/// `path` with a leading `~/` (or `~\`) made the home folder (`~/x` → `/home/me/x`);
/// `~` alone is the home folder.
pub fn expand_home(path: &str) -> String {
    let rest = if path == "~" { Some("") } else { path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) };
    match (rest, home_dir()) {
        (Some(""), Some(home)) => slash(home),
        (Some(rest), Some(home)) => slash(home.join(rest)),
        _ => path.to_string(),
    }
}

/// `path` as the interface knows paths: with `/` between folders, also on
/// Windows (where the system takes them too), without the `\\?\` of the paths
/// made canonical there. The interface splits and compares paths by `/`.
pub fn slash(path: impl AsRef<Path>) -> String {
    let s = path.as_ref().to_string_lossy();
    if cfg!(windows) {
        let s = s.replace('\\', "/");
        s.strip_prefix("//?/").map(String::from).unwrap_or(s)
    } else {
        s.into_owned()
    }
}

/// A command running `program`; on Windows, without the console window a
/// program started from a windowed app would flash.
pub fn command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    #[allow(unused_mut)]
    let mut c = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c
}

/// Open `target` (a folder, a file, a web page) with the system's program for
/// it: `open` on macOS, the shell's file handler on Windows, `xdg-open` elsewhere.
pub fn open_with_system(target: &str) -> Result<(), AppError> {
    let (program, args): (&str, Vec<String>) = if cfg!(target_os = "macos") {
        ("open", vec![target.into()])
    } else if cfg!(windows) {
        // A file is given with Windows' separators; no shell reads the target
        let target = if target.contains("://") { target.to_string() } else { target.replace('/', "\\") };
        ("rundll32", vec!["url.dll,FileProtocolHandler".into(), target])
    } else {
        ("xdg-open", vec![target.into()])
    };
    command(program).args(&args).spawn()
        .map(|_| ())
        .map_err(|e| AppError::CannotRun { program: program.into(), error: e.to_string() })
}

/// The folder of the files installed with the app (Windows: git, the
/// dictionaries); set once, when the app starts.
static RESOURCES: OnceLock<PathBuf> = OnceLock::new();

pub fn set_resource_dir(dir: PathBuf) {
    let _ = RESOURCES.set(dir);
}

pub fn resource_dir() -> Option<&'static Path> {
    RESOURCES.get().map(PathBuf::as_path)
}

/// The `git` to run: the one of the system when there is one (its setup, its
/// identifiers), else on Windows the one installed with the app (MinGit, with
/// Git Credential Manager), else `git`, which then fails to start.
pub fn git_program() -> &'static Path {
    static GIT: OnceLock<PathBuf> = OnceLock::new();
    GIT.get_or_init(|| {
        let system = command("git").arg("--version").stdin(std::process::Stdio::null()).output()
            .is_ok_and(|o| o.status.success());
        let bundled = resource_dir().map(|d| d.join("mingit").join("cmd").join("git.exe")).filter(|g| g.is_file());
        match bundled {
            Some(g) if !system => g,
            _ => PathBuf::from("git"),
        }
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_and_slashes() {
        let home = home_dir().map(slash).unwrap_or_default();
        if !home.is_empty() {
            assert_eq!(expand_home("~/notes"), format!("{home}/notes"));
            assert_eq!(expand_home("~"), home);
        }
        assert_eq!(expand_home("/x/~/y"), "/x/~/y");
        assert_eq!(slash("/home/me/notes"), "/home/me/notes");
        if cfg!(windows) {
            assert_eq!(slash(r"C:\Users\me\notes"), "C:/Users/me/notes");
            assert_eq!(slash(r"\\?\C:\Users\me"), "C:/Users/me");
        }
    }
}
