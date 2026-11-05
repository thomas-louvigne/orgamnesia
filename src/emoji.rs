//! WebKitGTK's emoji picker, opened from the editor's right-click menu: the
//! backend sends the webview the key that opens it, and the page lets that one by.

use std::cell::Cell;

use wasm_bindgen::{closure::Closure, JsCast};
use wasm_bindgen_futures::spawn_local;

use crate::invoke;

thread_local! {
    /// The backend was asked for the key, which has yet to come.
    static SENT: Cell<bool> = const { Cell::new(false) };
}

/// Whether the app has the picker: only WebKitGTK does.
pub fn available() -> bool {
    !crate::keybindings::is_mac() && !crate::preferences::on_windows()
}

pub fn open() {
    SENT.set(true);
    spawn_local(async {
        if !invoke::show_emoji_picker().await.unwrap_or(false) { SENT.set(false); }
    });
}

/// The key the backend sends reaches WebKit but no shortcut of the page, and is
/// not blocked by the setting that turns off WebKit's shortcuts: heard before all.
pub fn listen() {
    let Some(window) = web_sys::window() else { return };
    let on_key = Closure::<dyn Fn(web_sys::KeyboardEvent)>::new(|e: web_sys::KeyboardEvent| {
        // ";" where "." takes Shift: the key is then Ctrl+;, which opens it too
        if e.ctrl_key() && matches!(e.key().as_str(), "." | ";") && SENT.replace(false) {
            e.stop_propagation();
        }
    });
    let _ = window.add_event_listener_with_callback_and_bool("keydown", on_key.as_ref().unchecked_ref(), true);
    on_key.forget(); // as long as the app
}
