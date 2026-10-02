use js_sys::Promise;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use crate::{keybindings::Keybindings, state::{BrokenLink, FileEntry, Settings}};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], js_name = "invoke")]
    fn tauri_invoke_raw(cmd: &str, args: JsValue) -> Promise;
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
        .map_err(|e| format!("invoke '{cmd}': {:?}", e.as_string()))?;
    serde_wasm_bindgen::from_value(value)
        .map_err(|e| format!("deserialize: {e}"))
}

pub async fn get_settings() -> Result<Settings, String> {
    call("get_settings", serde_json::json!({})).await
}

pub async fn save_settings(s: &Settings) -> Result<(), String> {
    call("set_settings", serde_json::json!({ "settings": s })).await
}

pub async fn open_vault(path: &str) -> Result<Vec<FileEntry>, String> {
    call("open_vault", serde_json::json!({ "path": path })).await
}

pub async fn remove_project(path: &str) -> Result<Vec<String>, String> {
    call("remove_project", serde_json::json!({ "path": path })).await
}

pub async fn list_files() -> Result<Vec<FileEntry>, String> {
    call("list_files", serde_json::json!({})).await
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

pub async fn delete_page(path: &str) -> Result<(), String> {
    call("delete_page", serde_json::json!({ "path": path })).await
}

pub async fn get_backlinks(page: &str) -> Result<Vec<String>, String> {
    call("get_backlinks", serde_json::json!({ "pageName": page })).await
}

pub async fn get_broken_links() -> Result<Vec<BrokenLink>, String> {
    call("get_broken_links", serde_json::json!({})).await
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
