//! The git extension's commands: state, changes, commit, pull and push, remote,
//! history, branches.

use super::*;

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

/// The passphrase of the SSH key, kept in memory until the app closes; `None` forgets it.
pub async fn git_set_passphrase(passphrase: Option<&str>) -> Result<(), String> {
    call("git_set_passphrase", serde_json::json!({ "passphrase": passphrase })).await
}

pub async fn git_branches() -> Result<Vec<String>, String> {
    call("git_branches", serde_json::json!({})).await
}

/// Switch to the branch `name`; with `create`, a new one.
pub async fn git_switch(name: &str, create: bool) -> Result<String, String> {
    call("git_switch", serde_json::json!({ "name": name, "create": create })).await
}
