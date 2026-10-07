//! Small values the webview remembers between runs (sizes of the panels…).

fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

pub fn load(key: &str) -> Option<String> {
    local_storage()?.get_item(key).ok()?
}

pub fn store(key: &str, value: &str) {
    if let Some(st) = local_storage() {
        let _ = st.set_item(key, value);
    }
}
