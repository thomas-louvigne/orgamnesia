use leptos::{ev, prelude::*};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

use crate::{
    components::{
        backlinks_panel::BacklinksPanel,
        editor::EditorSplit,
        quick_open::QuickOpenModal,
        settings_modal::SettingsModal,
        sidebar::{NewPageModal, Sidebar},
        tabs::TabBar,
    },
    i18n::{t, Lang},
    invoke,
    keybindings::{Resolution, Scope},
    state::{AppCtx, Drag, SplitKind},
};

const SIDEBAR_KEY: &str = "sidebar_width";
const PANEL_KEY: &str = "panel_width";
const BROKEN_KEY: &str = "broken_height";
const MIN_SIDE: i32 = 140;
const MIN_BROKEN: i32 = 40;

fn load_size(key: &str) -> Option<i32> {
    web_sys::window()?.local_storage().ok()??.get_item(key).ok()??.parse().ok()
}

fn store_size(key: &str, v: i32) {
    if let Some(st) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = st.set_item(key, &v.to_string());
    }
}

/// Draggable bar between the pages menu / backlinks panel and the workspace.
#[component]
fn Resizer(which: Drag) -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");
    view! {
        <div
            class="resizer"
            on:mousedown=move |e: web_sys::MouseEvent| {
                e.prevent_default();
                ctx.drag.set(Some(which));
            }
        />
    }
}

/// In-app yes/no dialog driven by `ctx.confirm`.
#[component]
fn ConfirmDialog() -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");
    let lang = move || ctx.lang.get();
    let close = move || ctx.confirm.set(None);
    let accept = move || {
        if let Some(req) = ctx.confirm.get_untracked() {
            ctx.confirm.set(None);
            req.on_yes.run(());
        }
    };
    let keys = window_event_listener(ev::keydown, move |e: web_sys::KeyboardEvent| {
        match e.key().as_str() {
            "Escape" => { e.stop_propagation(); close(); }
            "Enter"  => { e.stop_propagation(); accept(); }
            _ => {}
        }
    });
    on_cleanup(move || drop(keys));

    view! {
        <div class="alert-overlay" on:click=move |_| close()>
            <div class="alert-box confirm" role="alertdialog" on:click=|e| e.stop_propagation()>
                <p>{move || ctx.confirm.get().map(|r| r.message).unwrap_or_default()}</p>
                <div class="alert-buttons">
                    <button class="btn-secondary" on:click=move |_| close()>{move || t("cancel", lang())}</button>
                    <button class="btn-primary" on:click=move |_| accept()>{move || t("confirm", lang())}</button>
                </div>
            </div>
        </div>
    }
}

#[component]
pub fn App() -> impl IntoView {
    let ctx = AppCtx::new();
    provide_context(ctx);

    // Sizes of the side menus and of the "pages not created" block, remembered between runs
    let sidebar_w = RwSignal::new(load_size(SIDEBAR_KEY).unwrap_or(220));
    let panel_w = RwSignal::new(load_size(PANEL_KEY).unwrap_or(200));
    let broken_h = RwSignal::new(load_size(BROKEN_KEY).unwrap_or(160));
    let drag_move = window_event_listener(ev::mousemove, move |e: web_sys::MouseEvent| {
        let Some(which) = ctx.drag.get_untracked() else { return };
        let win = web_sys::window();
        let dim = |f: fn(&web_sys::Window) -> Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue>| {
            win.as_ref().and_then(|w| f(w).ok()).and_then(|v| v.as_f64()).unwrap_or(1000.0) as i32
        };
        let (win_w, win_h) = (dim(|w| w.inner_width()), dim(|w| w.inner_height()));
        match which {
            Drag::Sidebar => sidebar_w.set(e.client_x().clamp(MIN_SIDE, (win_w / 2).max(MIN_SIDE))),
            Drag::Panel => panel_w.set((win_w - e.client_x()).clamp(MIN_SIDE, (win_w / 2).max(MIN_SIDE))),
            Drag::Broken => {
                // Height of the list = what is left under the bar, minus the block's title
                let header = web_sys::window().and_then(|w| w.document())
                    .and_then(|d| d.query_selector(".broken-links-header").ok().flatten())
                    .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok())
                    .map(|el| el.offset_height()).unwrap_or(30);
                broken_h.set((win_h - e.client_y() - header).clamp(MIN_BROKEN, (win_h * 7 / 10).max(MIN_BROKEN)));
            }
        }
    });
    let drag_up = window_event_listener(ev::mouseup, move |_| {
        if let Some(which) = ctx.drag.get_untracked() {
            match which {
                Drag::Sidebar => store_size(SIDEBAR_KEY, sidebar_w.get_untracked()),
                Drag::Panel => store_size(PANEL_KEY, panel_w.get_untracked()),
                Drag::Broken => store_size(BROKEN_KEY, broken_h.get_untracked()),
            }
            ctx.drag.set(None);
        }
    });
    on_cleanup(move || { drop(drag_move); drop(drag_up); });

    // Load saved vault, settings, and keybindings on startup
    spawn_local(async move {
        if let Ok(settings) = invoke::get_settings().await {
            if let Some(lang_str) = settings.language {
                ctx.lang.set(Lang::from_str(&lang_str));
            }
            ctx.projects.set(settings.projects);
            ctx.case_insensitive_links.set(settings.case_insensitive_links.unwrap_or(true));
            ctx.hashtag_links.set(settings.hashtag_links.unwrap_or(true));
            ctx.hashtag_dashes.set(settings.hashtag_dashes.unwrap_or(true));
            ctx.electric_mode.set(settings.electric_mode.unwrap_or(true));
            ctx.emacs_mark.set(settings.emacs_mark.unwrap_or(true));
            ctx.autosave.set(settings.autosave.unwrap_or(true));
            ctx.show_pages.set(settings.show_pages.unwrap_or(true));
            ctx.show_backlinks.set(settings.show_backlinks.unwrap_or(true));
            ctx.show_tags.set(settings.show_tags.unwrap_or(true));
            ctx.show_broken_links.set(settings.show_broken_links.unwrap_or(true));
            ctx.site_builder.set(settings.site_builder_enabled.unwrap_or(false));
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
        if let Ok(mut kb) = invoke::get_keybindings().await {
            // A built-in profile is always its current definition, so improved
            // defaults reach existing users. (Files written before profiles
            // existed have no active profile and keep their bindings as they are.)
            // Custom profiles get the shortcuts of their base preset for actions
            // added since they were saved.
            for p in kb.profiles.iter_mut() {
                let base = p.editor_preset.clone();
                crate::keybindings::fill_missing(&mut p.app, &mut p.editor, &base);
            }
            if !kb.active_profile.is_empty() {
                let active = kb.resolved_active();
                if !crate::keybindings::is_builtin(&active) {
                    let base = kb.editor_preset.clone();
                    crate::keybindings::fill_missing(&mut kb.app, &mut kb.editor, &base);
                }
                if crate::keybindings::is_builtin(&active) {
                    kb.app = crate::keybindings::preset_app(&active);
                    kb.editor = crate::keybindings::preset_editor(&active);
                    kb.editor_preset = active.clone();
                    kb.active_profile = active;
                }
            }
            ctx.keybindings.set(kb);
        }
    });

    // Every second, pick up the pages changed on disk by another program
    // (another editor, a sync tool…). One check at a time.
    let polling = StoredValue::new(false);
    let poll = set_interval_with_handle(move || {
        if ctx.vault_path.get_untracked().is_none() || polling.get_value() { return; }
        polling.set_value(true);
        spawn_local(async move {
            if let Ok(Some(changes)) = invoke::poll_vault().await {
                crate::components::sidebar::apply_disk_changes(ctx, changes).await;
            }
            polling.set_value(false);
        });
    }, std::time::Duration::from_secs(1));
    on_cleanup(move || if let Ok(h) = poll { h.clear() });

    // Drop the split when the page shown in the other pane is closed
    Effect::new(move |_| {
        let tabs = ctx.tabs.get();
        if ctx.split.get_untracked().is_none() { return; }
        let gone = match ctx.other_path.get_untracked() {
            Some(p) => !tabs.iter().any(|t| t.path == p),
            None => true,
        };
        if gone || tabs.is_empty() { ctx.single_window(); }
    });

    // Global keyboard shortcuts (app-level; editor actions are handled in the editor)
    let _kb_handle = window_event_listener(ev::keydown, move |e: web_sys::KeyboardEvent| {
        let kb = ctx.keybindings.get();
        let action = match crate::keybindings::resolve(&kb, &e) {
            Resolution::Action(Scope::App, id) => { ctx.clear_chord_status(); id }
            Resolution::Pending(keys) => {
                e.prevent_default();
                ctx.set_chord_status(&keys);
                return;
            }
            Resolution::Aborted => {
                e.prevent_default();
                ctx.clear_chord_status();
                return;
            }
            _ => { ctx.clear_chord_status(); return }
        };
        e.prevent_default();
        let action = action.as_str();

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
                                ctx.links_version.update(|v| *v += 1);
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
            "split_vertical"   => ctx.split_window(SplitKind::Vertical),
            "split_horizontal" => ctx.split_window(SplitKind::Horizontal),
            "close_split"      => ctx.close_window(),
            "single_window"    => ctx.single_window(),
            "other_window"     => ctx.other_window(),
            "quit" => {
                let quit = move || spawn_local(async move {
                    if let Err(err) = invoke::quit_app().await {
                        ctx.status.set(Some(format!("Quit error: {err}")));
                    }
                });
                if ctx.tabs.get_untracked().iter().any(|t| t.dirty.get_untracked()) {
                    ctx.ask_confirm(t("quit_unsaved_confirm", ctx.lang.get_untracked()).to_string(), quit);
                } else {
                    quit();
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

    // No webview context menu (Back / Forward / Stop / Reload) outside text
    // fields; the editor and inputs keep theirs for cut / copy / paste.
    let _ctx_menu_handle = window_event_listener(ev::contextmenu, move |e: web_sys::MouseEvent| {
        let editable = e.target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
            .is_some_and(|el| el.closest("textarea, input").ok().flatten().is_some());
        if !editable { e.prevent_default(); }
    });
    on_cleanup(move || drop(_ctx_menu_handle));

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
        <div class=move || if ctx.drag.get().is_some() { "app resizing" } else { "app" }>
            <div
                class="app-main"
                style=move || format!(
                    "--sidebar-w: {}px; --panel-w: {}px; --broken-h: {}px;",
                    sidebar_w.get(), panel_w.get(), broken_h.get()
                )
            >
                {move || ctx.show_pages.get().then(|| view! {
                    <Sidebar />
                    <Resizer which=Drag::Sidebar />
                })}
                <div class="workspace">
                    // Side by side, each pane has its own tabs under its title (see EditorArea)
                    {move || {
                        let split = ctx.split.get();
                        (split != Some(SplitKind::Vertical) || ctx.site_builder.get()).then(|| view! {
                            <div class="workspace-topbar">
                                {match split {
                                    None => view! { <TabBar /> }.into_any(),
                                    Some(SplitKind::Horizontal) => view! { <TabBar pane=false /> }.into_any(),
                                    Some(SplitKind::Vertical) => ().into_any(),
                                }}
                                {move || ctx.site_builder.get().then(|| view! {
                                    <button
                                        class="btn-export"
                                        title={move || t("export_title", ctx.lang.get())}
                                        on:click=export
                                    >
                                        {move || t("export", ctx.lang.get())}
                                    </button>
                                })}
                            </div>
                        })
                    }}
                    <EditorSplit />
                </div>
                {move || ctx.has_right_panel().then(|| view! {
                    <Resizer which=Drag::Panel />
                    <BacklinksPanel />
                })}
            </div>
            {move || ctx.show_quick_open.get().then(|| view! { <QuickOpenModal /> })}
            {move || ctx.show_settings.get().then(|| view! { <SettingsModal /> })}
            {move || ctx.confirm.get().is_some().then(|| view! { <ConfirmDialog /> })}
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
