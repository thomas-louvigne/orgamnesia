//! The commands of the configuration: themes, keybindings, the configuration
//! files edited as text and their versions.

use super::*;

/// File names of the user's themes (`themes/*.css` of the configuration).
pub async fn list_themes() -> Result<Vec<String>, String> {
    call("list_themes", serde_json::json!({})).await
}

/// The CSS of the user's theme `name`.
pub async fn read_theme(name: &str) -> Result<String, String> {
    call("read_theme", serde_json::json!({ "name": name })).await
}

/// A new theme of the user holding `text`, named after `name`; its file name.
pub async fn create_theme(name: &str, text: &str) -> Result<String, String> {
    call("create_theme", serde_json::json!({ "name": name, "text": text })).await
}

/// Show the themes folder in the file manager.
pub async fn open_themes_folder() -> Result<(), String> {
    call("open_themes_folder", serde_json::json!({})).await
}

/// Show a folder in the file manager.
pub async fn open_folder(path: &str) -> Result<(), String> {
    call("open_folder", serde_json::json!({ "path": path })).await
}

/// Folder picker (titled `title`), where a folder may also be created.
pub async fn pick_folder(title: Option<&str>) -> Result<Option<String>, String> {
    call("pick_folder", serde_json::json!({ "title": title })).await
}

pub async fn get_keybindings() -> Result<Keybindings, String> {
    call("get_keybindings", serde_json::json!({})).await
}

/// Write the keybinding profiles; the keybindings as they now are (the profiles
/// named after their files).
pub async fn set_keybindings(kb: &Keybindings) -> Result<Keybindings, String> {
    call("set_keybindings", serde_json::json!({ "keybindings": kb })).await
}

/// The configuration files, as the pages of the "preference" project.
pub async fn config_files() -> Result<Vec<FileEntry>, String> {
    call("config_files", serde_json::json!({})).await
}

pub async fn read_config(file: &ConfigFile) -> Result<String, String> {
    call("read_config", serde_json::json!({ "file": file })).await
}

/// Write `content` as `file` and put it in effect; what it holds.
pub async fn apply_config(file: &ConfigFile, content: &str) -> Result<InEffect, String> {
    call("apply_config", serde_json::json!({ "file": file, "content": content })).await
}

/// The versions kept of `file`, newest first: when each was replaced (ms since 1970).
pub async fn config_versions(file: &ConfigFile) -> Result<Vec<u64>, String> {
    call("config_versions", serde_json::json!({ "file": file })).await
}

/// Put back in effect the version of `file` replaced at `version`, or the last valid one.
pub async fn restore_config(file: &ConfigFile, version: Option<u64>) -> Result<Restored, String> {
    call("restore_config", serde_json::json!({ "file": file, "version": version })).await
}

/// The configuration files found wrong as the app started.
pub async fn config_problems() -> Result<Vec<ConfigInvalid>, String> {
    call("config_problems", serde_json::json!({})).await
}
