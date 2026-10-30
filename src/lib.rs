mod a11y;
mod actions;
mod app;
mod components;
mod edit;
mod export;
mod folding;
mod git;
mod history;
mod highlight;
mod html;
mod images;
pub mod i18n;
mod invoke;
mod kill_ring;
mod links;
mod org_bloom;
mod preferences;
mod session;
pub mod keybindings;
pub mod motion;
mod state;
mod storage;
mod tags;
mod tables;
mod text_field;
mod update;
mod watchers;

use app::App;
use leptos::prelude::*;

#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn run() {
    // A panic leaves the page frozen (no click or key handled any more):
    // show its message instead of failing silently.
    std::panic::set_hook(Box::new(|info| {
        console_error_panic_hook::hook(info);
        if let Some(w) = web_sys::window() {
            let crash = i18n::t("crash", i18n::current());
            let _ = w.alert_with_message(&format!("{crash}\n\n{info}"));
        }
    }));
    mount_to_body(App);
}
