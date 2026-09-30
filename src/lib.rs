mod app;
mod components;
mod highlight;
pub mod i18n;
mod invoke;
pub mod keybindings;
pub mod motion;
mod state;

use app::App;
use leptos::prelude::*;

#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn run() {
    console_error_panic_hook::set_once();
    mount_to_body(App);
}
