use leptos::{ev, prelude::*};
use wasm_bindgen_futures::spawn_local;

use crate::{
    components::{
        backlinks_panel::BacklinksPanel,
        editor::EditorArea,
        quick_open::QuickOpenModal,
        settings_modal::SettingsModal,
        sidebar::{NewPageModal, Sidebar},
        tabs::TabBar,
    },
    i18n::{t, Lang},
    invoke,
    state::AppCtx,
};

#[component]
pub fn App() -> impl IntoView {
    let ctx = AppCtx::new();
    provide_context(ctx);

    // Load saved vault, settings, and keybindings on startup
    spawn_local(async move {
        if let Ok(settings) = invoke::get_settings().await {
            if let Some(lang_str) = settings.language {
                ctx.lang.set(Lang::from_str(&lang_str));
            }
            if let Some(path) = settings.vault_path {
                match invoke::open_vault(&path).await {
                    Ok(files) => {
                        ctx.vault_path.set(Some(path));
                        ctx.files.set(files);
                    }
                    Err(e) => ctx.status.set(Some(format!("Could not open vault: {e}"))),
                }
            }
        }
        if let Ok(kb) = invoke::get_keybindings().await {
            ctx.keybindings.set(kb);
        }
    });

    // Global keyboard shortcuts (app-level; editor actions are handled in the editor)
    let _kb_handle = window_event_listener(ev::keydown, move |e: web_sys::KeyboardEvent| {
        let kb = ctx.keybindings.get();
        let Some(action) = crate::keybindings::match_action(&kb.app, &e) else { return };
        e.prevent_default();

        let close_active_tab = || {
            let mut tabs = ctx.tabs.get();
            if let Some(idx) = ctx.active_tab.get() {
                tabs.remove(idx);
                let new_active = if tabs.is_empty() { None } else { Some(idx.saturating_sub(1)) };
                ctx.tabs.set(tabs);
                ctx.active_tab.set(new_active);
            }
        };

        match action {
            "new_page"      => ctx.show_new_page.set(true),
            "open_settings" => ctx.show_settings.set(true),
            "quick_open"    => ctx.show_quick_open.set(true),
            "close_tab"     => close_active_tab(),
            "save" => {
                if let Some(tab) = ctx.active_tab_data() {
                    spawn_local(async move {
                        let content = tab.content.get();
                        match invoke::write_file(&tab.path, &content).await {
                            Ok(_) => {
                                tab.dirty.set(false);
                                ctx.status.set(Some(format!("Saved {}", tab.name)));
                            }
                            Err(err) => ctx.status.set(Some(format!("Save error: {err}"))),
                        }
                    });
                }
            }
            "next_tab" => {
                let tabs = ctx.tabs.get();
                if !tabs.is_empty() {
                    let idx = ctx.active_tab.get().unwrap_or(0);
                    ctx.active_tab.set(Some((idx + 1) % tabs.len()));
                }
            }
            "prev_tab" => {
                let tabs = ctx.tabs.get();
                if !tabs.is_empty() {
                    let idx = ctx.active_tab.get().unwrap_or(0);
                    ctx.active_tab.set(Some(if idx == 0 { tabs.len() - 1 } else { idx - 1 }));
                }
            }
            _ => {}
        }
    });
    on_cleanup(move || drop(_kb_handle));

    let export = move |_| {
        spawn_local(async move {
            ctx.status.set(Some("Exporting…".to_string()));
            match invoke::export_vault().await {
                Ok(_) => ctx.status.set(Some("Export successful!".to_string())),
                Err(e) => ctx.status.set(Some(format!("Export failed: {e}"))),
            }
        });
    };

    view! {
        <div class="app">
            <div class="app-main">
                <Sidebar />
                <div class="workspace">
                    <div class="workspace-topbar">
                        <TabBar />
                        <button
                            class="btn-export"
                            title={move || t("export_title", ctx.lang.get())}
                            on:click=export
                        >
                            {move || t("export", ctx.lang.get())}
                        </button>
                    </div>
                    <EditorArea />
                </div>
                <BacklinksPanel />
            </div>
            {move || ctx.show_quick_open.get().then(|| view! { <QuickOpenModal /> })}
            {move || ctx.show_settings.get().then(|| view! { <SettingsModal /> })}
            {move || ctx.show_new_page.get().then(|| view! { <NewPageModal /> })}
            {move || ctx.status.get().map(|msg| view! {
                <div class="status-bar">
                    <span>{msg}</span>
                    <button class="status-close" on:click=move |_| ctx.status.set(None)>"×"</button>
                </div>
            })}
        </div>
    }
}
