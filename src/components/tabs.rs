use leptos::{html, prelude::*};
use wasm_bindgen_futures::spawn_local;

use crate::{invoke, state::AppCtx};

/// The open pages. With `pane` (false: left / top, true: right / bottom), the bar
/// belongs to one pane of a split: it shows that pane's page and clicking a tab
/// opens the page there. Without, it follows the focused pane.
#[component]
pub fn TabBar(#[prop(optional)] pane: Option<bool>) -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");

    view! {
        <div class="tab-bar">
            <For
                each={move || ctx.tabs.get().into_iter().enumerate().collect::<Vec<_>>()}
                key={|(_, t)| t.path.clone()}
                let:item
            >
                {
                    let (idx, tab) = item;
                    let is_active = move || match pane {
                        Some(second) => {
                            let path = ctx.tabs.with(|tabs| tabs.get(idx).map(|t| t.path.clone()));
                            path.is_some() && ctx.pane_path(second) == path
                        }
                        None => ctx.active_tab.get() == Some(idx),
                    };
                    let tab_name = tab.name.clone();
                    let dirty = tab.dirty;

                    let editing   = RwSignal::new(false);
                    let edit_value = RwSignal::new(String::new());
                    let input_ref = NodeRef::<html::Input>::new();

                    let ctx_open = RwSignal::new(false);
                    let ctx_x    = RwSignal::new(0i32);
                    let ctx_y    = RwSignal::new(0i32);

                    // Auto-focus + select-all when edit mode starts
                    Effect::new(move |_| {
                        if editing.get() {
                            if let Some(el) = input_ref.get() {
                                let _ = el.focus();
                                let _ = el.select();
                            }
                        }
                    });

                    let close = move |e: web_sys::MouseEvent| {
                        e.stop_propagation();
                        ctx.tabs.update(|tabs| { tabs.remove(idx); });
                        ctx.active_tab.update(|active| {
                            *active = match *active {
                                None => None,
                                Some(i) if i == idx => {
                                    let len = ctx.tabs.get().len();
                                    if len == 0 { None }
                                    else if idx > 0 { Some(idx - 1) }
                                    else { Some(0) }
                                }
                                Some(i) if i > idx => Some(i - 1),
                                Some(i) => Some(i),
                            };
                        });
                        if ctx.active_tab.get().is_none() {
                            ctx.backlinks.set(vec![]);
                        }
                    };

                    // Rename helpers — inlined in two separate handlers to avoid clone()
                    // Both closures capture only Copy values so this is fine.

                    let do_rename = move || {
                        if !editing.get() { return; }
                        let new_name = edit_value.get().trim().to_string();
                        let current_path = ctx.tabs.get().get(idx)
                            .map(|t| t.path.clone()).unwrap_or_default();
                        let current_name = ctx.tabs.get().get(idx)
                            .map(|t| t.name.clone()).unwrap_or_default();
                        if new_name.is_empty() || new_name == current_name {
                            editing.set(false);
                            return;
                        }
                        editing.set(false);
                        spawn_local(async move {
                            match invoke::rename_page(&current_path, &new_name).await {
                                Ok(nf) => {
                                    ctx.tabs.update(|tabs| {
                                        if let Some(t) = tabs.get_mut(idx) {
                                            t.name = nf.name.clone();
                                            t.path = nf.path.clone();
                                        }
                                    });
                                    ctx.files.update(|files| {
                                        if let Some(f) = files.iter_mut()
                                            .find(|f| f.path == current_path)
                                        {
                                            f.name = nf.name.clone();
                                            f.path = nf.path.clone();
                                        }
                                    });
                                    crate::components::sidebar::reload_clean_tabs(ctx).await;
                                    ctx.status.set(Some(format!("Renamed to {}", nf.name)));
                                }
                                Err(e) => ctx.status.set(Some(format!("Rename error: {e}"))),
                            }
                        });
                    };

                    // Enter/Escape in the rename input
                    let on_rename_keydown = move |e: web_sys::KeyboardEvent| {
                        e.stop_propagation();
                        match e.key().as_str() {
                            "Escape" => editing.set(false),
                            "Enter"  => do_rename(),
                            _        => {}
                        }
                    };

                    // Blur confirms rename (same as Enter)
                    let on_rename_blur = move |_| do_rename();

                    // Double-click on tab name → enter edit mode
                    let on_dblclick = move |e: web_sys::MouseEvent| {
                        e.prevent_default();
                        e.stop_propagation();
                        let name = ctx.tabs.get().get(idx)
                            .map(|t| t.name.clone()).unwrap_or_default();
                        edit_value.set(name);
                        editing.set(true);
                    };

                    // Right-click → context menu
                    let on_ctx_menu = move |e: web_sys::MouseEvent| {
                        e.prevent_default();
                        e.stop_propagation();
                        ctx_x.set(e.client_x());
                        ctx_y.set(e.client_y());
                        ctx_open.set(true);
                    };

                    view! {
                        <div
                            class=move || match (is_active(), editing.get()) {
                                (_, true)      => "tab active editing",
                                (true, false)  => "tab active",
                                (false, false) => "tab",
                            }
                            on:click=move |_| {
                                if editing.get() { return; }
                                match pane {
                                    Some(second) => ctx.show_in_pane(second, idx),
                                    None => ctx.active_tab.set(Some(idx)),
                                }
                            }
                            on:dblclick=on_dblclick
                            on:contextmenu=on_ctx_menu
                        >
                            <input
                                node_ref=input_ref
                                class="tab-rename-input"
                                type="text"
                                style=move || if editing.get() { "" } else { "display:none;" }
                                prop:value=move || edit_value.get()
                                on:input=move |e| edit_value.set(event_target_value(&e))
                                on:keydown=on_rename_keydown
                                on:blur=on_rename_blur
                                on:click=|e: web_sys::MouseEvent| e.stop_propagation()
                                on:dblclick=|e: web_sys::MouseEvent| e.stop_propagation()
                            />
                            <span
                                class="tab-name"
                                style=move || if editing.get() { "display:none;" } else { "" }
                            >{tab_name}</span>
                            {move || dirty.get().then(|| view! {
                                <span
                                    class="tab-dot"
                                    style=move || if editing.get() { "display:none;" } else { "" }
                                    title={move || crate::i18n::t("unsaved", ctx.lang.get())}
                                >"•"</span>
                            })}
                            <button
                                class="tab-close"
                                style=move || if editing.get() { "display:none;" } else { "" }
                                on:click=close
                                on:dblclick=|e: web_sys::MouseEvent| e.stop_propagation()
                            >"×"</button>

                            {move || ctx_open.get().then(|| view! {
                                <div>
                                    <div
                                        class="ctx-overlay"
                                        on:click=move |_| ctx_open.set(false)
                                    />
                                    <div
                                        class="ctx-menu"
                                        style=move || format!(
                                            "left:{}px;top:{}px;",
                                            ctx_x.get(), ctx_y.get()
                                        )
                                    >
                                        <button
                                            class="ctx-menu-item"
                                            on:click=move |_| {
                                                ctx_open.set(false);
                                                let name = ctx.tabs.get().get(idx)
                                                    .map(|t| t.name.clone())
                                                    .unwrap_or_default();
                                                edit_value.set(name);
                                                editing.set(true);
                                            }
                                        >
                                            {move || crate::i18n::t("rename", ctx.lang.get())}
                                        </button>
                                    </div>
                                </div>
                            })}
                        </div>
                    }
                }
            </For>
        </div>
    }
}
