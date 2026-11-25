//! The terminal interface driven by its keys and its mouse, on a project made for each test.

use super::*;
use super::view::{frame_title, git_line};
use orgamnesia_core::keybindings::Keybindings;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Direction, Rect};
use ratatui::style::Modifier;

mod editing;
mod frames;
mod pages;

fn press(app: &mut App, code: KeyCode, m: KeyModifiers) -> bool { app.key(KeyEvent::new(code, m)) }

/// A project of the pages `pages` (name, text) for a test, and the app on it.
fn app_on(test: &str, pages: &[(&str, &str)]) -> (std::path::PathBuf, App) {
    let dir = std::env::temp_dir().join(format!("orgamnesia-tui-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    for (name, text) in pages { std::fs::write(dir.join("pages").join(format!("{name}.org")), text).unwrap(); }
    let prefs = settings::Settings::default().prefs();
    let project = Project::open(&crate::slash(&dir), prefs.tags(), prefs.formats(), &prefs.journal_folder, &prefs.templates_folder).unwrap();
    (dir, App::new(project, prefs, Keybindings::with_profiles(None, vec![])))
}

fn answer(app: &mut App, typed: &str) {
    for c in typed.chars() { press(app, KeyCode::Char(c), KeyModifiers::NONE); }
    press(app, KeyCode::Enter, KeyModifiers::NONE);
}
