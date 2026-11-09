//! The user's themes: CSS files in the `themes/` folder of the configuration.

use std::path::{Path, PathBuf};

use tauri::AppHandle;

use super::{config_dir, Cmd};
use crate::error::AppError;

const THEMES_DIR: &str = "themes";

/// The file `name` of the themes folder of `dir`; a name holding a path is refused.
fn theme_path(dir: &Path, name: &str) -> Cmd<PathBuf> {
    if name.contains(['/', '\\']) || name.starts_with('.') || !name.ends_with(".css") {
        return Err(AppError::InvalidName(name.to_string()));
    }
    Ok(dir.join(THEMES_DIR).join(name))
}

fn list(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir.join(THEMES_DIR)).into_iter().flatten().flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".css") && !n.starts_with('.'))
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names
}

/// Write `text` as a new theme named after `name` (`name-2.css`… when taken); its file name.
fn create(dir: &Path, name: &str, text: &str) -> Cmd<String> {
    std::fs::create_dir_all(dir.join(THEMES_DIR))?;
    let taken = list(dir);
    let file = (1..).map(|i| if i == 1 { format!("{name}.css") } else { format!("{name}-{i}.css") })
        .find(|f| !taken.contains(f))
        .expect("an endless range always has a free name");
    std::fs::write(theme_path(dir, &file)?, text)?;
    Ok(file)
}

/// File names of the user's themes, sorted.
#[tauri::command]
pub async fn list_themes(app: AppHandle) -> Cmd<Vec<String>> {
    Ok(list(&config_dir(&app)?))
}

/// The CSS of the user's theme `name`.
#[tauri::command]
pub async fn read_theme(app: AppHandle, name: String) -> Cmd<String> {
    Ok(std::fs::read_to_string(theme_path(&config_dir(&app)?, &name)?)?)
}

/// A new theme of the user holding `text`; its file name.
#[tauri::command]
pub async fn create_theme(app: AppHandle, name: String, text: String) -> Cmd<String> {
    create(&config_dir(&app)?, &name, &text)
}

/// Show the themes folder (made if missing) in the file manager.
#[tauri::command]
pub async fn open_themes_folder(app: AppHandle) -> Cmd<()> {
    let dir = config_dir(&app)?.join(THEMES_DIR);
    std::fs::create_dir_all(&dir)?;
    crate::open_with_system(&dir.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::testing::folder;

    #[test]
    fn themes_are_css_files_of_their_folder() {
        let dir = folder("themes");
        assert!(list(&dir).is_empty());
        assert_eq!(create(&dir, "mine", ":root{}").unwrap(), "mine.css");
        assert_eq!(create(&dir, "mine", ":root{}").unwrap(), "mine-2.css");
        std::fs::write(dir.join(THEMES_DIR).join("notes.txt"), "").unwrap();
        assert_eq!(list(&dir), vec!["mine-2.css", "mine.css"]);
        assert!(theme_path(&dir, "../settings.json").is_err());
        assert!(theme_path(&dir, "mine.txt").is_err());
        assert!(theme_path(&dir, ".hidden.css").is_err());
    }
}
