use js_sys::Promise;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use crate::{keybindings::Keybindings, state::{BrokenLink, FileEntry, Session, Settings, GitChange, GitStatus, TagCount, TagHit, TodoHit}};

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

async fn call<A, R>(cmd: &str, args: A) -> Result<R, String>
where
    A: Serialize,
    R: for<'de> Deserialize<'de>,
{
    let js_args = serde_wasm_bindgen::to_value(&args)
        .map_err(|e| format!("serialize: {e}"))?;
    let promise = tauri_invoke_raw(cmd, js_args);
    let value = JsFuture::from(promise).await
        // The backend's errors arrive as their message
        .map_err(|e| e.as_string().unwrap_or_else(|| format!("{cmd}: {e:?}")))?;
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

pub async fn open_vault(path: &str) -> Result<Vec<FileEntry>, String> {
    call("open_vault", serde_json::json!({ "path": path })).await
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

pub async fn export_vault() -> Result<String, String> {
    call("export_vault", serde_json::json!({})).await
}

pub async fn pick_folder() -> Result<Option<String>, String> {
    call("pick_folder", serde_json::json!({})).await
}

pub async fn get_keybindings() -> Result<Keybindings, String> {
    call("get_keybindings", serde_json::json!({})).await
}

pub async fn set_keybindings(kb: &Keybindings) -> Result<(), String> {
    call("set_keybindings", serde_json::json!({ "keybindings": kb })).await
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

/// Each returns what git says.
pub async fn git_commit(message: &str) -> Result<String, String> {
    call("git_commit", serde_json::json!({ "message": message })).await
}

pub async fn git_pull() -> Result<String, String> {
    call("git_pull", serde_json::json!({})).await
}

pub async fn git_push() -> Result<String, String> {
    call("git_push", serde_json::json!({})).await
}

/// A file picker opening in `start`.
pub async fn pick_file(start: Option<&str>) -> Result<Option<String>, String> {
    call("pick_file", serde_json::json!({ "start": start })).await
}
