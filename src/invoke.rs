use js_sys::Promise;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use orgamnesia_core::config::{File as ConfigFile, InEffect, Problem};

use crate::{i18n::{t, Lang}, keybindings::Keybindings, state::{BrokenLink, FileEntry, Session, Settings, SiteConfig, GitChange, GitCommit, GitStatus, UpdateInfo, TagCount, TagHit, TodoHit, VaultChanges, ReplaceOutcome, ReplaceSkip, SearchOptions, SearchResults}};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], js_name = "invoke")]
    fn tauri_invoke_raw(cmd: &str, args: JsValue) -> Promise;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"], js_name = "listen")]
    fn tauri_listen(event: &str, handler: &Closure<dyn FnMut(JsValue)>) -> Promise;
}

/// Event sent by the backend with the pages changed on disk outside the app
/// (`src-tauri/src/watch.rs`).
pub const VAULT_CHANGED: &str = "vault-changed";
/// Event sent by the backend when the git state of the project may have changed.
pub const GIT_CHANGED: &str = "git-changed";

/// Event sent by the backend when a configuration file changed outside the app is
/// valid, and now in effect (`src-tauri/src/config_files.rs`).
pub const CONFIG_CHANGED: &str = "config-changed";
/// Event sent by the backend when a configuration file changed outside the app is
/// not valid: the one before stays in effect.
pub const CONFIG_INVALID: &str = "config-invalid";

/// A configuration file changed outside the app, now in effect.
#[derive(Debug, Clone, Deserialize)]
pub struct ConfigChanged {
    pub file: ConfigFile,
    pub effect: InEffect,
}

/// A configuration file found wrong, and what is wrong with it.
#[derive(Debug, Clone, Deserialize)]
pub struct ConfigInvalid {
    pub file: ConfigFile,
    pub problem: Problem,
}

/// A configuration put back in effect: its text, and what it holds.
#[derive(Debug, Clone, Deserialize)]
pub struct Restored {
    pub content: String,
    pub effect: InEffect,
}

/// Event sent by the window when files are dropped on it.
pub const DRAG_DROP: &str = "tauri://drag-drop";

/// Files dropped on the window, and where (see `client_pos`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Dropped {
    pub paths: Vec<String>,
    pub position: DropPosition,
}

impl Dropped {
    /// Where the files were dropped, in CSS pixels of the page. Tauri says physical
    /// pixels, but only WebView2 (Windows) gives them so: WebKitGTK and WKWebView
    /// give the widget's own, logical, coordinates.
    pub fn client_pos(&self) -> (f64, f64) {
        let window = web_sys::window();
        let windows = window.as_ref()
            .and_then(|w| w.navigator().user_agent().ok())
            .is_some_and(|ua| ua.contains("Windows"));
        let ratio = if windows { window.map_or(1.0, |w| w.device_pixel_ratio()) } else { 1.0 };
        (self.position.x / ratio, self.position.y / ratio)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct DropPosition {
    pub x: f64,
    pub y: f64,
}

/// Run `f` with the payload of every `event` the backend emits, for the whole
/// life of the app.
pub fn listen<T: for<'de> Deserialize<'de> + 'static>(event: &str, mut f: impl FnMut(T) + 'static) {
    let handler = Closure::<dyn FnMut(JsValue)>::new(move |e: JsValue| {
        let payload = js_sys::Reflect::get(&e, &JsValue::from_str("payload")).unwrap_or(JsValue::NULL);
        if let Ok(value) = serde_wasm_bindgen::from_value(payload) {
            f(value);
        }
    });
    let _ = tauri_listen(event, &handler);
    handler.forget();
}

/// An error of the backend (`src-tauri/src/error.rs`): a code, the detail that
/// goes with it, and for `cannot_run` the cause.
#[derive(Debug, Deserialize)]
struct BackendError {
    code: String,
    detail: Option<String>,
    cause: Option<String>,
}

/// What a backend error says, in `lang`.
fn error_text(err: &BackendError, lang: Lang) -> String {
    let key = match err.code.as_str() {
        "no_project" => "err_no_project",
        "vault_not_found" => "err_vault_not_found",
        "not_a_folder" => "err_not_a_folder",
        "not_a_page" => "err_not_a_page",
        "page_exists" => "err_page_exists",
        "invalid_name" => "err_invalid_name",
        "org_bloom_not_found" => "err_org_bloom_not_found",
        "cannot_run" => "err_cannot_run",
        "folder_not_found" => "err_folder_not_found",
        "settings" => "err_settings",
        "config_invalid" => "err_config_invalid",
        "dialog" => "err_dialog",
        "no_remote" => "err_no_remote",
        "not_an_image" => "err_not_an_image",
        "no_dictionary" => "err_no_dictionary",
        "bad_dictionary" => "err_bad_dictionary",
        // What a tool or the system said: shown as it is
        _ => return err.detail.clone().unwrap_or_else(|| err.code.clone()),
    };
    let mut text = t(key, lang).to_string();
    if let Some(detail) = &err.detail { text = format!("{text} {detail}"); }
    if let Some(cause) = &err.cause { text = format!("{text} : {cause}"); }
    text
}

async fn call<A, R>(cmd: &str, args: A) -> Result<R, String>
where
    A: Serialize,
    R: for<'de> Deserialize<'de>,
{
    let js_args = serde_wasm_bindgen::to_value(&args)
        .map_err(|e| format!("serialize: {e}"))?;
    let promise = tauri_invoke_raw(cmd, js_args);
    let value = JsFuture::from(promise).await.map_err(|e| match serde_wasm_bindgen::from_value::<BackendError>(e.clone()) {
        Ok(err) => error_text(&err, crate::i18n::current()),
        Err(_) => e.as_string().unwrap_or_else(|| format!("{cmd}: {e:?}")),
    })?;
    serde_wasm_bindgen::from_value(value)
        .map_err(|e| format!("deserialize: {e}"))
}

pub async fn get_settings() -> Result<Settings, String> {
    call("get_settings", serde_json::json!({})).await
}

pub async fn save_settings(s: &Settings) -> Result<(), String> {
    call("set_settings", serde_json::json!({ "settings": s })).await
}

/// Tell the backend which pages and panes are open, to save them on quit.
pub async fn set_session(session: Option<&Session>) -> Result<(), String> {
    call("set_session", serde_json::json!({ "session": session })).await
}

/// The text of the system clipboard, read by the backend (the webview doesn't let
/// the page read a text copied in another application).
pub async fn read_clipboard() -> Result<Option<String>, String> {
    call("read_clipboard", serde_json::json!({})).await
}

/// Put `text` in the system clipboard; whether it went there.
pub async fn write_clipboard(text: &str) -> Result<bool, String> {
    call("write_clipboard", serde_json::json!({ "text": text })).await
}

/// The pages changed on disk outside the app since the last look (or the watcher's).
pub async fn check_disk() -> Result<Option<VaultChanges>, String> {
    call("check_disk", serde_json::json!({})).await
}

pub async fn open_vault(path: &str) -> Result<Vec<FileEntry>, String> {
    call("open_vault", serde_json::json!({ "path": path })).await
}

/// An `assets/` folder to offer the project (see `src-tauri/src/commands/projects.rs`).
#[derive(Debug, Clone, Deserialize)]
pub struct AssetsOffer {
    /// The folder around the project, whose `assets/` may be used.
    pub parent: Option<String>,
}

/// What `assets/` folder to offer the project `path`, if any.
pub async fn assets_offer(path: &str) -> Result<Option<AssetsOffer>, String> {
    call("assets_offer", serde_json::json!({ "path": path })).await
}

/// Make the `assets/` folder of the project `path`, or remember it isn't wanted.
pub async fn answer_assets(path: &str, create: bool) -> Result<(), String> {
    call("answer_assets", serde_json::json!({ "path": path, "create": create })).await
}

pub async fn remove_project(path: &str) -> Result<Vec<String>, String> {
    call("remove_project", serde_json::json!({ "path": path })).await
}

pub async fn read_file(path: &str) -> Result<String, String> {
    call("read_file", serde_json::json!({ "path": path })).await
}

pub async fn write_file(path: &str, content: &str) -> Result<(), String> {
    call("write_file", serde_json::json!({ "path": path, "content": content })).await
}

pub async fn create_page(name: &str) -> Result<FileEntry, String> {
    call("create_page", serde_json::json!({ "pageName": name })).await
}

pub async fn rename_page(old_path: &str, new_name: &str) -> Result<FileEntry, String> {
    call("rename_page", serde_json::json!({ "oldPath": old_path, "newName": new_name })).await
}

/// Returns the path of the page in the project's trash.
pub async fn delete_page(path: &str) -> Result<String, String> {
    call("delete_page", serde_json::json!({ "path": path })).await
}

pub async fn restore_page(trashed: &str) -> Result<FileEntry, String> {
    call("restore_page", serde_json::json!({ "trashed": trashed })).await
}

pub async fn trash_count() -> Result<usize, String> {
    call("trash_count", serde_json::json!({})).await
}

/// Returns how many pages were deleted.
pub async fn empty_trash() -> Result<usize, String> {
    call("empty_trash", serde_json::json!({})).await
}

pub async fn list_todos() -> Result<Vec<TodoHit>, String> {
    call("list_todos", serde_json::json!({})).await
}

pub async fn get_backlinks(page: &str) -> Result<Vec<String>, String> {
    call("get_backlinks", serde_json::json!({ "pageName": page })).await
}

pub async fn get_broken_links() -> Result<Vec<BrokenLink>, String> {
    call("get_broken_links", serde_json::json!({})).await
}

pub async fn list_tags() -> Result<Vec<TagCount>, String> {
    call("list_tags", serde_json::json!({})).await
}

pub async fn search_tags(query: &str) -> Result<Vec<TagHit>, String> {
    call("search_tags", serde_json::json!({ "query": query })).await
}

/// The matches of `query` in the pages; `open`: the text of the pages being edited (path, text).
pub async fn search_project(query: &str, options: SearchOptions, open: &[(String, String)]) -> Result<SearchResults, String> {
    let open: std::collections::HashMap<&String, &String> = open.iter().map(|(p, t)| (p, t)).collect();
    call("search_project", serde_json::json!({ "query": query, "options": options, "open": open })).await
}

pub async fn replace_in_project(query: &str, options: SearchOptions, replacement: &str, skip: &ReplaceSkip) -> Result<ReplaceOutcome, String> {
    call("replace_in_project", serde_json::json!({
        "query": query, "options": options, "replacement": replacement, "skip": skip,
    })).await
}

pub async fn undo_replace() -> Result<ReplaceOutcome, String> {
    call("undo_replace", serde_json::json!({})).await
}

pub async fn can_undo_replace() -> Result<bool, String> {
    call("can_undo_replace", serde_json::json!({})).await
}

/// Build the site of the project with org-bloom; the folder it was written to.
pub async fn build_site() -> Result<String, String> {
    call("build_site", serde_json::json!({})).await
}

/// Where org-bloom builds the site of the open project; `None` without a project.
pub async fn site_config() -> Result<Option<SiteConfig>, String> {
    call("site_config", serde_json::json!({})).await
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

pub async fn quit_app() -> Result<(), String> {
    call("quit_app", serde_json::json!({})).await
}

pub async fn git_status(path: &str) -> Result<Option<GitStatus>, String> {
    call("git_status", serde_json::json!({ "path": path })).await
}

pub async fn git_changes() -> Result<Vec<GitChange>, String> {
    call("git_changes", serde_json::json!({})).await
}

/// What changed in `file` (relative to the root of the repository) since the last commit.
pub async fn git_diff(file: &str, old_path: Option<&str>) -> Result<String, String> {
    call("git_diff", serde_json::json!({ "file": file, "oldPath": old_path })).await
}

/// Each returns what git says. With `files`, only those are committed.
pub async fn git_commit(message: &str, files: Option<&[String]>) -> Result<String, String> {
    call("git_commit", serde_json::json!({ "message": message, "files": files })).await
}

pub async fn git_fetch() -> Result<String, String> {
    call("git_fetch", serde_json::json!({})).await
}

/// The name and email git signs the commits with (empty when not set).
pub async fn git_identity() -> Result<(String, String), String> {
    call("git_identity", serde_json::json!({})).await
}

pub async fn git_set_identity(name: &str, email: &str) -> Result<(), String> {
    call("git_set_identity", serde_json::json!({ "name": name, "email": email })).await
}

pub async fn git_init() -> Result<String, String> {
    call("git_init", serde_json::json!({})).await
}

pub async fn git_add_remote(url: &str) -> Result<String, String> {
    call("git_add_remote", serde_json::json!({ "url": url })).await
}

/// Clone `url` into a new folder of `parent`; returns that folder.
pub async fn git_clone(parent: &str, url: &str) -> Result<String, String> {
    call("git_clone", serde_json::json!({ "parent": parent, "url": url })).await
}

/// The commits that changed the page `file` (full path), newest first.
pub async fn git_log(file: &str) -> Result<Vec<GitCommit>, String> {
    call("git_log", serde_json::json!({ "file": file })).await
}

/// What `hash` changed in `file` (relative to the root of the repository).
pub async fn git_commit_diff(hash: &str, file: &str) -> Result<String, String> {
    call("git_commit_diff", serde_json::json!({ "hash": hash, "file": file })).await
}

/// The text of `file` (relative to the root of the repository) in `hash`.
pub async fn git_show(hash: &str, file: &str) -> Result<String, String> {
    call("git_show", serde_json::json!({ "hash": hash, "file": file })).await
}

pub async fn git_pull() -> Result<String, String> {
    call("git_pull", serde_json::json!({})).await
}

pub async fn git_push() -> Result<String, String> {
    call("git_push", serde_json::json!({})).await
}

/// A newer version of the app, with what changed since this one; `None` when
/// this one is the latest.
pub async fn check_update() -> Result<Option<UpdateInfo>, String> {
    call("check_update", serde_json::json!({})).await
}

/// Download and install the version `check_update` found, then restart on it.
pub async fn install_update() -> Result<(), String> {
    call("install_update", serde_json::json!({})).await
}

/// A file picker opening in `start`.
pub async fn pick_file(start: Option<&str>) -> Result<Option<String>, String> {
    call("pick_file", serde_json::json!({ "start": start })).await
}

/// A file picker for images.
pub async fn pick_image() -> Result<Option<String>, String> {
    call("pick_image", serde_json::json!({})).await
}

/// Copy the image `source` into the project's assets; returns the link to it from
/// the page `page` (`../assets/photo_1759520598218_0.png`).
pub async fn import_image(page: &str, source: &str) -> Result<String, String> {
    call("import_image", serde_json::json!({ "page": page, "source": source })).await
}

/// Copy the file `source` into the project's assets; returns the link to it from
/// the page `page`.
pub async fn import_file(page: &str, source: &str) -> Result<String, String> {
    call("import_file", serde_json::json!({ "page": page, "source": source })).await
}

/// Open the file at `path` with the system's application for it.
pub async fn open_file_link(path: &str) -> Result<(), String> {
    call("open_file_link", serde_json::json!({ "path": path })).await
}

/// Open the web page `url` in the browser.
pub async fn open_url(url: &str) -> Result<(), String> {
    call("open_url", serde_json::json!({ "url": url })).await
}

/// The bytes of the image file at `path`.
pub async fn read_image(path: &str) -> Result<js_sys::Uint8Array, String> {
    let args = serde_wasm_bindgen::to_value(&serde_json::json!({ "path": path })).map_err(|e| e.to_string())?;
    let value = JsFuture::from(tauri_invoke_raw("read_image", args)).await
        .map_err(|e| e.as_string().unwrap_or_else(|| format!("read_image: {e:?}")))?;
    Ok(js_sys::Uint8Array::new(&value))
}

/// Of `paths`, those missing from the disk.
pub async fn missing_files(paths: &[String]) -> Result<Vec<String>, String> {
    call("missing_files", serde_json::json!({ "paths": paths })).await
}

/// Names of the spelling dictionaries found (`fr_FR`…), `dict_dir` looked in first.
pub async fn spell_languages(dict_dir: &str) -> Result<Vec<String>, String> {
    call("spell_languages", serde_json::json!({ "dictDir": dict_dir })).await
}

/// Of `words`, those the spelling dictionary doesn't know.
pub async fn spell_check(words: &[String]) -> Result<Vec<String>, String> {
    call("spell_check", serde_json::json!({ "words": words })).await
}

/// Words `word` may have been meant as.
pub async fn spell_suggest(word: &str) -> Result<Vec<String>, String> {
    call("spell_suggest", serde_json::json!({ "word": word })).await
}

/// Add `word` to the user's spelling dictionary.
pub async fn spell_add(word: &str) -> Result<(), String> {
    call("spell_add", serde_json::json!({ "word": word })).await
}
