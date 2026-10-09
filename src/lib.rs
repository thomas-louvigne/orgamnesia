mod a11y;
mod actions;
mod app;
mod components;
mod edit;
mod folding;
mod git;
mod highlight;
pub mod i18n;
mod invoke;
pub mod keybindings;
pub mod motion;
mod state;
mod storage;
mod tables;

use app::App;
use leptos::prelude::*;

#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn run() {
    // A panic leaves the page frozen (no click or key handled any more):
    // show its message instead of failing silently.
    std::panic::set_hook(Box::new(|info| {
        console_error_panic_hook::hook(info);
        if let Some(w) = web_sys::window() {
            let _ = w.alert_with_message(&format!("Orgamnesia a planté, merci de copier ce message :\n\n{info}"));
        }
    }));
    mount_to_body(App);
}
