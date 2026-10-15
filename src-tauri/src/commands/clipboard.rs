//! The system clipboard, read and written by the application rather than by the
//! page: the webview only lets the page read a text it copied itself, not one
//! copied in another application.

use tauri::AppHandle;

/// The text of the system clipboard; `None` when it holds none (or can't be read).
#[tauri::command]
pub async fn read_clipboard(app: AppHandle) -> Option<String> {
    let (tx, rx) = tokio::sync::oneshot::channel::<Option<String>>();
    imp::read(&app, tx);
    rx.await.ok().flatten()
}

/// Put `text` in the system clipboard; whether it went there.
#[tauri::command]
pub async fn write_clipboard(app: AppHandle, text: String) -> bool {
    let (tx, rx) = tokio::sync::oneshot::channel::<bool>();
    imp::write(&app, text, tx);
    rx.await.unwrap_or(false)
}

#[cfg(target_os = "linux")]
mod imp {
    use gtk::{gdk, Clipboard};
    use tauri::AppHandle;
    use tokio::sync::oneshot::Sender;

    // GTK is used from its own (main) thread only.
    pub fn read(app: &AppHandle, tx: Sender<Option<String>>) {
        let _ = app.run_on_main_thread(move || {
            Clipboard::get(&gdk::SELECTION_CLIPBOARD).request_text(move |_, text| {
                let _ = tx.send(text.map(str::to_string));
            });
        });
    }

    pub fn write(app: &AppHandle, text: String, tx: Sender<bool>) {
        let _ = app.run_on_main_thread(move || {
            Clipboard::get(&gdk::SELECTION_CLIPBOARD).set_text(&text);
            let _ = tx.send(true);
        });
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use tauri::AppHandle;
    use tokio::sync::oneshot::Sender;

    // The pasteboard (AppKit) is used from the main thread only.
    pub fn read(app: &AppHandle, tx: Sender<Option<String>>) {
        let _ = app.run_on_main_thread(move || {
            let _ = tx.send(arboard::Clipboard::new().and_then(|mut c| c.get_text()).ok());
        });
    }

    pub fn write(app: &AppHandle, text: String, tx: Sender<bool>) {
        let _ = app.run_on_main_thread(move || {
            let _ = tx.send(arboard::Clipboard::new().and_then(|mut c| c.set_text(text)).is_ok());
        });
    }
}

/// Elsewhere the page's own clipboard is used.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
mod imp {
    use tauri::AppHandle;
    use tokio::sync::oneshot::Sender;

    pub fn read(_: &AppHandle, tx: Sender<Option<String>>) { let _ = tx.send(None); }
    pub fn write(_: &AppHandle, _: String, tx: Sender<bool>) { let _ = tx.send(false); }
}
