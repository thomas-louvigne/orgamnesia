//! The configuration files (`settings.json`, and the user's keybinding profiles
//! in `keybindings/`): the text in effect of each,
//! the versions kept to go back to, and watching them for the changes made
//! outside the app.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use orgamnesia_core::keybindings::{Keybindings, ProfileFile};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

pub use orgamnesia_core::config::{check, to_text, Content, File, InEffect, Problem, PROFILES_DIR};

use crate::{error::AppError, watch::Watcher, AppState};

/// Versions kept of each file.
const HISTORY_LEN: usize = 10;

/// Event carrying the `Changed` file, valid, changed outside the app and now in effect.
pub const CONFIG_CHANGED: &str = "config-changed";
/// Event carrying the `Invalid` file, changed outside the app: the one before stays in effect.
pub const CONFIG_INVALID: &str = "config-invalid";

#[derive(Debug, Clone, Serialize)]
pub struct Changed {
    pub file: File,
    pub effect: InEffect,
}

#[derive(Debug, Clone, Serialize)]
pub struct Invalid {
    pub file: File,
    pub problem: Problem,
}

/// What the app knows of its configuration files.
#[derive(Default)]
pub struct ConfigState {
    /// The text of each file last read as valid or written: the one in effect.
    applied: HashMap<File, String>,
    /// The files found wrong (on start, or changed outside the app), and why.
    problems: HashMap<File, Problem>,
}

impl ConfigState {
    /// `text` (`None`: no such file) was read as the app starts: in effect when
    /// valid, else what is wrong with it is noted.
    pub fn loaded(&mut self, file: File, text: Option<&str>) {
        let Some(text) = text else { return };
        match check(&file, text, &self.profile_names()) {
            Ok(_) => { self.applied.insert(file, text.to_string()); }
            Err(p) => { self.problems.insert(file, p); }
        }
    }

    /// The files found wrong, and why.
    pub fn problems(&self) -> Vec<Invalid> {
        let mut list: Vec<Invalid> = self.problems.iter()
            .map(|(file, p)| Invalid { file: file.clone(), problem: p.clone() })
            .collect();
        list.sort_by_key(|i| i.file.name());
        list
    }

    /// The names of the user's keybinding profiles in effect.
    pub fn profile_names(&self) -> Vec<String> {
        self.applied.keys().filter_map(|f| match f { File::Profile(id) => Some(id.clone()), _ => None }).collect()
    }

    /// The text in effect of `file`.
    pub fn applied(&self, file: &File) -> Option<&str> {
        self.applied.get(file).map(String::as_str)
    }

    /// The keybindings in effect: those of the profile files in effect, `active`
    /// the profile the settings name.
    pub fn keybindings(&self, active: Option<&str>) -> Keybindings {
        let mut profiles: Vec<(String, ProfileFile)> = self.applied.iter().filter_map(|(f, text)| match f {
            File::Profile(name) => serde_json::from_str(text).ok().map(|p| (name.clone(), p)),
            _ => None,
        }).collect();
        profiles.sort_by(|a, b| a.0.cmp(&b.0));
        Keybindings::from_files(active, profiles)
    }

    /// Write `text`, valid, as `file` (unless it is that already); the text in
    /// effect until now is kept as a version.
    pub fn write(&mut self, dir: &Path, file: &File, text: &str) -> Result<(), AppError> {
        let target = path(dir, file);
        if self.applied(file) == Some(text) && std::fs::read_to_string(&target).ok().as_deref() == Some(text) {
            return Ok(());
        }
        if let Some(parent) = target.parent() { std::fs::create_dir_all(parent)?; }
        if let Some(old) = self.applied.get(file).filter(|old| *old != text) {
            archive(dir, file, old)?;
        }
        std::fs::write(target, text)?;
        self.applied.insert(file.clone(), text.to_string());
        self.problems.remove(file);
        Ok(())
    }

    /// Delete `file` (a profile no longer there); its text is kept as a version.
    pub fn remove(&mut self, dir: &Path, file: &File) -> Result<(), AppError> {
        if let Some(old) = self.applied.remove(file) { archive(dir, file, &old)?; }
        self.problems.remove(file);
        match std::fs::remove_file(path(dir, file)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        }
    }

    /// `file` may have changed on disk: `Ok` when what is in effect changed
    /// (its new text is valid, or it is gone), else what is wrong with it.
    /// `None` when there is nothing new.
    pub fn reread(&mut self, dir: &Path, file: &File) -> Option<Result<(), Problem>> {
        let Ok(text) = std::fs::read_to_string(path(dir, file)) else {
            // Gone: a profile deleted
            self.problems.remove(file);
            return self.applied.remove(file).map(|_| Ok(()));
        };
        if self.applied.get(file) == Some(&text) {
            self.problems.remove(file);
            return None;
        }
        match check(file, &text, &self.profile_names()) {
            Ok(_) => {
                if let Some(old) = self.applied.get(file) { let _ = archive(dir, file, old); }
                self.applied.insert(file.clone(), text);
                self.problems.remove(file);
                Some(Ok(()))
            }
            Err(p) if self.problems.get(file) == Some(&p) => None,
            Err(p) => {
                self.problems.insert(file.clone(), p.clone());
                Some(Err(p))
            }
        }
    }

    /// The text to go back to: the one in effect, else the newest version kept.
    pub fn last_valid(&self, dir: &Path, file: &File) -> Option<String> {
        self.applied.get(file).cloned()
            .or_else(|| versions(dir, file).first().and_then(|&v| version(dir, file, v).ok()))
    }
}

/// Where `file` is.
pub fn path(dir: &Path, file: &File) -> PathBuf {
    dir.join(file.name())
}

/// The user's keybinding profiles on disk (valid or not), by id.
pub fn profiles_on_disk(dir: &Path) -> Vec<File> {
    let mut files: Vec<File> = std::fs::read_dir(dir.join(PROFILES_DIR)).into_iter().flatten()
        .filter_map(|e| e.ok())
        .filter_map(|e| File::from_path(&e.path().to_string_lossy()))
        .collect();
    files.sort_by_key(File::name);
    files
}

fn history_dir(dir: &Path) -> PathBuf {
    dir.join("history")
}

/// What the versions of `file` are named after (`settings-<stamp>.json`): `settings`, `profile-<name>`.
fn version_stem(file: &File) -> String {
    match file {
        File::Profile(id) => format!("profile-{id}"),
        _ => file.name().trim_end_matches(".json").to_string(),
    }
}

/// The versions kept of `file`, newest first: when each was replaced (ms since 1970).
pub fn versions(dir: &Path, file: &File) -> Vec<u64> {
    stamps(dir, &version_stem(file))
}

/// When the versions named after `stem` were replaced, newest first.
fn stamps(dir: &Path, stem: &str) -> Vec<u64> {
    let prefix = format!("{stem}-");
    let mut stamps: Vec<u64> = std::fs::read_dir(history_dir(dir)).into_iter().flatten()
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name.strip_prefix(&prefix)?.strip_suffix(".json")?.parse().ok()
        })
        .collect();
    stamps.sort_unstable_by(|a, b| b.cmp(a));
    stamps
}

/// The text of the version of `file` replaced at `stamp`.
pub fn version(dir: &Path, file: &File, stamp: u64) -> Result<String, AppError> {
    Ok(std::fs::read_to_string(history_dir(dir).join(format!("{}-{stamp}.json", version_stem(file))))?)
}

/// Keep `text` as a version of `file` (unless it is the newest one already); the
/// oldest ones beyond `HISTORY_LEN` go.
pub fn archive(dir: &Path, file: &File, text: &str) -> Result<(), AppError> {
    archive_as(dir, &version_stem(file), text)
}

/// Keep `text` as a version named after `stem` (also a file of an older version, gone).
pub fn archive_as(dir: &Path, stem: &str, text: &str) -> Result<(), AppError> {
    let read = |stamp: u64| std::fs::read_to_string(history_dir(dir).join(format!("{stem}-{stamp}.json"))).ok();
    let kept = stamps(dir, stem);
    if kept.first().and_then(|&v| read(v)).as_deref() == Some(text) {
        return Ok(());
    }
    let history = history_dir(dir);
    std::fs::create_dir_all(&history)?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
    // Two in the same millisecond: the later one a millisecond after
    let stamp = kept.first().map_or(now, |&last| now.max(last + 1));
    std::fs::write(history.join(format!("{stem}-{stamp}.json")), text)?;
    for old in stamps(dir, stem).into_iter().skip(HISTORY_LEN) {
        let _ = std::fs::remove_file(history.join(format!("{stem}-{old}.json")));
    }
    Ok(())
}

/// The configuration in effect: the settings, and the keybindings of the profile they name.
pub fn in_effect(state: &AppState) -> InEffect {
    let settings = state.settings().config;
    let keybindings = keybindings(state);
    InEffect { settings, keybindings }
}

/// The keybindings in effect: those of the profile the settings name.
pub fn keybindings(state: &AppState) -> Keybindings {
    let active = state.settings().config.keybinding_profile;
    crate::lock(&state.config).keybindings(active.as_deref())
}

/// Put the settings `content` in effect in the backend (the keybindings are
/// read from `ConfigState` when asked for).
pub fn put_in_effect(state: &AppState, content: &Content) {
    let Content::Settings(config) = content else { return };
    let tags_changed = {
        let mut settings = crate::lock(&state.settings);
        let before = settings.prefs().tags();
        settings.config = config.clone();
        settings.prefs().tags() != before
    };
    // `#tags` or `:tags:` are read differently: read the links again
    if tags_changed && let Some(p) = crate::lock(&state.project).as_mut() {
        p.reindex(state.prefs().tags());
    }
}

/// Watch the configuration folder: a file changed by another program is put in
/// effect when valid (`CONFIG_CHANGED`), else the interface is told what is wrong
/// with it (`CONFIG_INVALID`).
pub fn watch(app: AppHandle, dir: PathBuf) -> Result<Watcher, AppError> {
    std::fs::create_dir_all(&dir)?;
    let folder = dir.clone();
    crate::watch::start(&dir, move |events| {
        let state = app.state::<AppState>();
        let mut files: Vec<File> = events.iter()
            .filter(|e| e.path.parent().is_some_and(|p| p == folder || p == folder.join(PROFILES_DIR)))
            .filter_map(|e| File::from_path(&e.path.to_string_lossy()))
            .collect();
        files.sort_by_key(File::name);
        files.dedup();
        // The profiles first: the settings may name one just added
        files.sort_by_key(|f| *f == File::Settings);
        for file in files {
            let reread = crate::lock(&state.config).reread(&folder, &file);
            match reread {
                Some(Ok(())) => {
                    if file == File::Settings {
                        let text = crate::lock(&state.config).applied(&file).map(str::to_string);
                        if let Some(Ok(checked)) = text.map(|t| check(&file, &t, &[])) { put_in_effect(&state, &checked.content); }
                    }
                    let effect = in_effect(&state);
                    let _ = app.emit(CONFIG_CHANGED, Changed { file, effect });
                }
                Some(Err(problem)) => { let _ = app.emit(CONFIG_INVALID, Invalid { file, problem }); }
                None => {}
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(test: &str) -> PathBuf {
        crate::commands::testing::folder(&format!("config-{test}"))
    }

    #[test]
    fn writes_keep_the_versions_replaced() {
        let dir = folder("history");
        let mut state = ConfigState::default();
        state.loaded(File::Settings, Some("{}"));
        state.write(&dir, &File::Settings, r#"{"autosave": false}"#).unwrap();
        state.write(&dir, &File::Settings, r#"{"autosave": true}"#).unwrap();
        let kept = versions(&dir, &File::Settings);
        assert_eq!(kept.len(), 2);
        assert_eq!(version(&dir, &File::Settings, kept[0]).unwrap(), r#"{"autosave": false}"#);
        assert_eq!(version(&dir, &File::Settings, kept[1]).unwrap(), "{}");
        assert!(versions(&dir, &File::Profile("x".into())).is_empty());
        for n in 0..HISTORY_LEN + 3 {
            state.write(&dir, &File::Settings, &format!(r#"{{"trash_keep_days": {n}}}"#)).unwrap();
        }
        assert_eq!(versions(&dir, &File::Settings).len(), HISTORY_LEN);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn outside_changes_are_checked() {
        let dir = folder("reread");
        let mut state = ConfigState::default();
        state.write(&dir, &File::Settings, "{}").unwrap();
        // The app's own write: nothing new
        assert_eq!(state.reread(&dir, &File::Settings), None);
        std::fs::write(path(&dir, &File::Settings), "{ oops").unwrap();
        assert!(matches!(state.reread(&dir, &File::Settings), Some(Err(_))));
        // Told once
        assert_eq!(state.reread(&dir, &File::Settings), None);
        assert_eq!(state.problems().len(), 1);
        assert_eq!(state.last_valid(&dir, &File::Settings).as_deref(), Some("{}"));
        std::fs::write(path(&dir, &File::Settings), r#"{"autosave": false}"#).unwrap();
        assert_eq!(state.reread(&dir, &File::Settings), Some(Ok(())));
        assert!(state.problems().is_empty());
        assert_eq!(versions(&dir, &File::Settings).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn keybindings_come_from_the_files_in_effect() {
        let dir = folder("profiles");
        let mut state = ConfigState::default();
        let mine = File::Profile("Mine".into());
        state.write(&dir, &mine, r#"{"editor_preset": "emacs", "app": {"save": ["ctrl+j"]}}"#).unwrap();
        assert_eq!(profiles_on_disk(&dir), vec![mine.clone()]);
        assert_eq!(state.keybindings(Some("Mine")).app_binds("save"), &["ctrl+j".to_string()]);
        // A profile spoilt outside the app: the one before stays in effect
        std::fs::write(path(&dir, &mine), "{").unwrap();
        assert!(matches!(state.reread(&dir, &mine), Some(Err(_))));
        assert_eq!(state.keybindings(Some("Mine")).app_binds("save"), &["ctrl+j".to_string()]);
        // Deleted: the default profile
        state.remove(&dir, &mine).unwrap();
        assert_eq!(state.keybindings(Some("Mine")).active_profile, "classic");
        assert_eq!(versions(&dir, &mine).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
