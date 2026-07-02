pub mod commands;
pub mod error;
pub mod index;
pub mod keybindings;
pub mod parser;
pub mod settings;
pub mod vault;

use std::sync::Mutex;
use index::backlinks::BacklinkIndex;

pub struct AppState {
    pub vault_path: Mutex<Option<String>>,
    pub backlinks: Mutex<BacklinkIndex>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            vault_path: Mutex::new(None),
            backlinks: Mutex::new(BacklinkIndex::new()),
        }
    }
}

impl Default for AppState {
    fn default() -> Self { Self::new() }
}
