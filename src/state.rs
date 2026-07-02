use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{i18n::Lang, keybindings::Keybindings};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings {
    pub vault_path: Option<String>,
    pub logseq_site_builder_path: Option<String>,
    pub language: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Tab {
    pub path: String,
    pub name: String,
    pub content: RwSignal<String>,
    pub dirty: RwSignal<bool>,
}

/// Global app context — Copy because RwSignal is Copy.
#[derive(Clone, Copy)]
pub struct AppCtx {
    pub vault_path: RwSignal<Option<String>>,
    pub files: RwSignal<Vec<FileEntry>>,
    pub tabs: RwSignal<Vec<Tab>>,
    pub active_tab: RwSignal<Option<usize>>,
    pub backlinks: RwSignal<Vec<String>>,
    pub show_settings: RwSignal<bool>,
    pub show_new_page: RwSignal<bool>,
    pub show_quick_open: RwSignal<bool>,
    pub status: RwSignal<Option<String>>,
    pub lang: RwSignal<Lang>,
    pub keybindings: RwSignal<Keybindings>,
    pub kill_ring: RwSignal<String>,
}

impl AppCtx {
    pub fn new() -> Self {
        Self {
            vault_path: RwSignal::new(None),
            files: RwSignal::new(vec![]),
            tabs: RwSignal::new(vec![]),
            active_tab: RwSignal::new(None),
            backlinks: RwSignal::new(vec![]),
            show_settings: RwSignal::new(false),
            show_new_page: RwSignal::new(false),
            show_quick_open: RwSignal::new(false),
            status: RwSignal::new(None),
            lang: RwSignal::new(Lang::default()),
            keybindings: RwSignal::new(Keybindings::defaults()),
            kill_ring: RwSignal::new(String::new()),
        }
    }

    pub fn active_tab_data(&self) -> Option<Tab> {
        let idx = self.active_tab.get()?;
        self.tabs.get().into_iter().nth(idx)
    }
}

impl Default for AppCtx {
    fn default() -> Self { Self::new() }
}
