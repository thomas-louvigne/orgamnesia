use leptos::{ev, prelude::*};
use wasm_bindgen_futures::spawn_local;

use crate::{
    i18n::t,
    invoke,
    state::{AppCtx, FileEntry, Tab},
};


// ─── New-page modal (mounted at app root to avoid fragment issues) ────────────

#[component]
pub fn NewPageModal() -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");
    let lang = move || ctx.lang.get();
    let name = RwSignal::new(String::new());
    let input_ref = NodeRef::<leptos::html::Input>::new();

    // Autofocus input on mount
    Effect::new(move |_| {
        if let Some(el) = input_ref.get() {
            let _ = el.focus();
        }
    });

    // Escape closes
    let esc = window_event_listener(ev::keydown, move |e| {
        if e.key() == "Escape" {
            ctx.show_new_page.set(false);
        }
    });
    on_cleanup(move || drop(esc));

    let do_create = move || {
        let n = name.get();
        let n = n.trim().to_string();
        if n.is_empty() {
            return;
        }
        ctx.show_new_page.set(false);
        name.set(String::new());

        // Allocate signals here (reactive owner scope) — not inside the async block
        let content_sig = RwSignal::new(String::new());
        let dirty_sig = RwSignal::new(true);

        spawn_local(async move {
            match invoke::create_page(&n).await {
                Ok(file) => {
                    content_sig.set(format!("* {}\n", file.name));
                    ctx.files.update(|files| {
                        files.push(file.clone());
                        files.sort_by(|a, b| a.name.cmp(&b.name));
                    });
                    ctx.tabs.update(|tabs| {
                        tabs.push(Tab {
                            path: file.path.clone(),
                            name: file.name.clone(),
                            content: content_sig,
                            dirty: dirty_sig,
                        });
                    });
                    let idx = ctx.tabs.get().len() - 1;
                    ctx.active_tab.set(Some(idx));
                }
                Err(e) => ctx.status.set(Some(format!("Error: {e}"))),
            }
        });
    };

    view! {
        <div class="modal-overlay" on:click=move |_| ctx.show_new_page.set(false)>
            <div class="modal" on:click=|e| e.stop_propagation()>
                <div class="modal-header">
                    <h2>{move || t("new_page_title", lang())}</h2>
                    <button class="btn-close" on:click=move |_| ctx.show_new_page.set(false)>"×"</button>
                </div>
                <div class="modal-body">
                    <input
                        node_ref=input_ref
                        type="text"
                        class="setting-input"
                        placeholder={move || t("new_page_ph", lang())}
                        prop:value=move || name.get()
                        on:input=move |e| name.set(event_target_value(&e))
                        on:keydown=move |e| { if e.key() == "Enter" { do_create(); } }
                    />
                </div>
                <div class="modal-footer">
                    <button class="btn-secondary" on:click=move |_| ctx.show_new_page.set(false)>
                        {move || t("cancel", lang())}
                    </button>
                    <button class="btn-primary" on:click=move |_| do_create()>
                        {move || t("create", lang())}
                    </button>
                </div>
            </div>
        </div>
    }
}

// ─── Sidebar ─────────────────────────────────────────────────────────────────

#[component]
pub fn Sidebar() -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");
    let filter = RwSignal::new(String::new());
    let show_ctx_menu = RwSignal::new(false);
    let ctx_menu_x   = RwSignal::new(0i32);
    let ctx_menu_y   = RwSignal::new(0i32);

    let filtered = move || {
        let q = filter.get().to_lowercase();
        ctx.files.get().into_iter().filter(|f| {
            q.is_empty() || f.name.to_lowercase().contains(&q)
        }).collect::<Vec<_>>()
    };

    let open_file = move |file: FileEntry| {
        spawn_local(async move {
            let idx = ctx.tabs.get().iter().position(|t| t.path == file.path);
            if let Some(i) = idx {
                ctx.active_tab.set(Some(i));
                return;
            }
            // Pre-allocate signals before the await
            let content_sig = RwSignal::new(String::new());
            let dirty_sig = RwSignal::new(false);
            match invoke::read_file(&file.path).await {
                Ok(content) => {
                    content_sig.set(content);
                    ctx.tabs.update(|tabs| {
                        tabs.push(Tab {
                            path: file.path.clone(),
                            name: file.name.clone(),
                            content: content_sig,
                            dirty: dirty_sig,
                        });
                    });
                    let new_idx = ctx.tabs.get().len() - 1;
                    ctx.active_tab.set(Some(new_idx));
                    if let Ok(bl) = invoke::get_backlinks(&file.name).await {
                        ctx.backlinks.set(bl);
                    }
                }
                Err(e) => ctx.status.set(Some(format!("Error: {e}"))),
            }
        });
    };

    let open_ctx_menu = move |e: web_sys::MouseEvent| {
        e.prevent_default();
        ctx_menu_x.set(e.client_x());
        ctx_menu_y.set(e.client_y());
        show_ctx_menu.set(true);
    };

    let trigger_new_page = move || {
        if ctx.vault_path.get().is_none() {
            ctx.status.set(Some(t("no_vault", ctx.lang.get()).to_string()));
            return;
        }
        ctx.show_new_page.set(true);
    };
    let new_page = move |_: web_sys::MouseEvent| trigger_new_page();

    view! {
        <div class="sidebar">
            <div class="sidebar-header">
                <span class="sidebar-title">{move || t("pages", ctx.lang.get())}</span>
                <button
                    class="btn-icon"
                    title={move || t("new_page_title", ctx.lang.get())}
                    on:click=new_page
                >"+"</button>
            </div>
            <div class="sidebar-search">
                <input
                    type="text"
                    class="search-input"
                    placeholder={move || t("filter_hint", ctx.lang.get())}
                    prop:value=move || filter.get()
                    on:input=move |e| filter.set(event_target_value(&e))
                />
            </div>
            <div class="sidebar-list" on:contextmenu=open_ctx_menu>
                <For
                    each=filtered
                    key={|f| f.path.clone()}
                    let:file
                >
                    {
                        let path = file.path.clone();
                        let name = file.name.clone();
                        let file2 = file.clone();
                        let is_active = move || {
                            ctx.active_tab.get()
                                .and_then(|i| ctx.tabs.get().into_iter().nth(i))
                                .map(|t| t.path == path)
                                .unwrap_or(false)
                        };
                        view! {
                            <div
                                class=move || if is_active() { "file-item active" } else { "file-item" }
                                on:click=move |_| open_file(file2.clone())
                            >
                                {name}
                            </div>
                        }
                    }
                </For>
            </div>
            <div class="sidebar-footer">
                <button class="btn-settings" on:click=move |_| ctx.show_settings.set(true)>
                    {move || t("settings_btn", ctx.lang.get())}
                </button>
            </div>

            {move || show_ctx_menu.get().then(|| {
                let x = ctx_menu_x.get();
                let y = ctx_menu_y.get();
                view! {
                    <div
                        class="ctx-overlay"
                        on:click=move |_| show_ctx_menu.set(false)
                        on:contextmenu=move |e: web_sys::MouseEvent| {
                            e.prevent_default();
                            show_ctx_menu.set(false);
                        }
                    />
                    <div class="ctx-menu" style=format!("left:{x}px;top:{y}px")>
                        <button class="ctx-menu-item" on:click=move |_| {
                            show_ctx_menu.set(false);
                            trigger_new_page();
                        }>
                            {move || t("new_page_title", ctx.lang.get())}
                        </button>
                    </div>
                }
            })}
        </div>
    }
}
