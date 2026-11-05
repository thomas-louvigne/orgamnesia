//! WebKitGTK's emoji picker, for the editor's right-click menu: WebKit has no
//! call that opens it (its `GtkInsertEmoji` command only answers to the key
//! binding, not to `execute_editing_command`), so the webview is sent that key.

/// Opens the picker at the caret of the webview's text field; whether the key was sent.
#[tauri::command]
pub fn show_emoji_picker(window: tauri::WebviewWindow) -> bool {
    #[cfg(target_os = "linux")]
    { window.with_webview(send_key).is_ok() }
    #[cfg(not(target_os = "linux"))]
    { let _ = window; false }
}

#[cfg(target_os = "linux")]
fn send_key(webview: tauri::webview::PlatformWebview) {
    use gtk::{gdk, glib::translate::{IntoGlib, ToGlibPtr}, prelude::*};
    let view = webview.inner();
    let Some(window) = view.toplevel().and_then(|t| t.window()) else { return };
    let display = window.display();
    let keyval = gdk::keys::constants::period;
    let keycode = gdk::Keymap::for_display(&display)
        .and_then(|k| k.entries_for_keyval(keyval.into_glib()).first().map(|e| e.keycode()))
        .unwrap_or(0);
    let device = display.default_seat().and_then(|s| s.keyboard());
    for kind in [gdk::EventType::KeyPress, gdk::EventType::KeyRelease] {
        let mut event = gdk::Event::new(kind);
        event.set_device(device.as_ref());
        // gdk-rs has no setters for the fields of a key event
        unsafe {
            let event: *const gdk::ffi::GdkEvent = event.to_glib_none().0;
            let key = event as *mut gdk::ffi::GdkEventKey;
            (*key).window = window.to_glib_full(); // the event owns it
            (*key).time = gtk::current_event_time();
            (*key).state = gdk::ModifierType::CONTROL_MASK.into_glib();
            (*key).keyval = keyval.into_glib();
            (*key).hardware_keycode = keycode as u16;
        }
        gtk::main_do_event(&mut event);
    }
}
