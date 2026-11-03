//! Operations on the projects, pages and tabs, shared by the panels: each one
//! talks to the backend and updates the app state the same way wherever it starts.

use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::{
    i18n::t,
    invoke,
    preferences,
    state::{AppCtx, FileEntry, Goto, Tab, VaultChanges},
};

mod disk;
mod history;
mod pages;
mod project;
mod tabs;

pub use disk::*;
pub use history::*;
pub use pages::*;
pub use project::*;
pub use tabs::*;

/// The text of the page at `path` on disk; a configuration file of the
/// "preference" project is read as such.
pub async fn read_page(path: &str) -> Result<String, String> {
    match preferences::config_file(path) {
        Some(file) => invoke::read_config(&file).await,
        None => invoke::read_file(path).await,
    }
}
