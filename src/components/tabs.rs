use leptos::{html, prelude::*};

use crate::{actions, i18n::t, state::{AppCtx, FileEntry}};

/// The open pages. With `pane` (false: left / top, true: right / bottom), the bar
/// belongs to one pane of a split: it shows that pane's page and clicking a tab
/// opens the page there. Without, it follows the focused pane.
#[component]
pub fn TabBar(#[prop(optional)] pane: Option<bool>) -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");

    view! {
        <div class="tab-bar">
            <For
                each=move || ctx.work.tabs.get()
                key=|t| t.path.clone()
                let:tab
            >
                <TabItem tab_path=tab.path name=tab.name dirty=tab.dirty pane=pane />
            </For>
        </div>
    }
}

/// One tab. Rows are keyed by path, so the tab's position is looked up when
/// needed: tabs closed before it shift it.
#[component]
fn TabItem(
    tab_path: String,
    name: String,
    dirty: RwSignal<bool>,
    pane: Option<bool>,
) -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");
    let path = StoredValue::new(tab_path);
    let index = move || path.with_value(|p| ctx.work.tab_index(p));

    let is_active = move || {
        let shown = match pane {
            Some(second) => ctx.work.pane_path(second),
            None => ctx.work.active_tab_data().map(|t| t.path),
        };
        // Also re-run when the pane is closed (C-x 0), after this tab is disposed
        path.try_with_value(|p| shown.as_ref() == Some(p)).unwrap_or(false)
    };

    let editing    = RwSignal::new(false);
    let edit_value = RwSignal::new(String::new());
    let input_ref  = NodeRef::<html::Input>::new();
    let menu       = RwSignal::new(None::<(i32, i32)>);

    // Auto-focus + select-all when edit mode starts
    Effect::new(move |_| {
        if editing.get() && let Some(el) = input_ref.get() {
            let _ = el.focus();
            el.select();
        }
    });

    let name_for_edit = name.clone();
    let start_rename = move || {
        edit_value.set(name_for_edit.clone());
        editing.set(true);
    };
    let do_rename = move || {
        if !editing.get_untracked() { return; }
        editing.set(false);
        actions::rename_page(ctx, path.get_value(), edit_value.get_untracked());
    };

    let close = move |e: web_sys::MouseEvent| {
        e.stop_propagation();
        if let Some(idx) = index() { actions::close_tab(ctx, idx); }
    };

    let select = move |_| {
        if editing.get_untracked() { return; }
        let Some(idx) = index() else { return };
        match pane {
            Some(second) => ctx.work.show_in_pane(second, idx),
            None => ctx.work.active_tab.set(Some(idx)),
        }
    };

    let hidden_while_editing = move || if editing.get() { "display:none;" } else { "" };
    let start_rename_dbl = start_rename.clone();
    let name_for_delete = name.clone();

    view! {
        <div
            class=move || match (is_active(), editing.try_get().unwrap_or(false)) {
                (_, true)      => "tab active editing",
                (true, false)  => "tab active",
                (false, false) => "tab",
            }
            on:click=select
            on:dblclick=move |e: web_sys::MouseEvent| {
                e.prevent_default();
                e.stop_propagation();
                start_rename_dbl();
            }
            on:contextmenu=move |e: web_sys::MouseEvent| {
                e.prevent_default();
                e.stop_propagation();
                menu.set(Some((e.client_x(), e.client_y())));
            }
        >
            <input
                node_ref=input_ref
                class="tab-rename-input"
                type="text"
                style=move || if editing.get() { "" } else { "display:none;" }
                prop:value=move || edit_value.get()
                on:input=move |e| edit_value.set(event_target_value(&e))
                on:keydown=move |e: web_sys::KeyboardEvent| {
                    e.stop_propagation();
                    match e.key().as_str() {
                        "Escape" => editing.set(false),
                        "Enter"  => do_rename(),
                        _        => {}
                    }
                }
                on:blur=move |_| do_rename()
                on:click=|e: web_sys::MouseEvent| e.stop_propagation()
                on:dblclick=|e: web_sys::MouseEvent| e.stop_propagation()
            />
            <span class="tab-name" style=hidden_while_editing>{name}</span>
            {move || dirty.get().then(|| view! {
                <span
                    class="tab-dot"
                    style=hidden_while_editing
                    title={move || t("unsaved", ctx.lang.get())}
                >"•"</span>
            })}
            <button
                class="tab-close"
                aria-label=move || t("shortcut_close_tab", ctx.lang.get())
                style=hidden_while_editing
                on:click=close
                on:dblclick=|e: web_sys::MouseEvent| e.stop_propagation()
            >"×"</button>

            {move || menu.get().map(|(x, y)| {
                let start_rename = start_rename.clone();
                let name = name_for_delete.clone();
                view! {
                    <div>
                        <div class="ctx-overlay" on:click=move |_| menu.set(None) />
                        <div class="ctx-menu" style=format!("left:{x}px;top:{y}px;")>
                            <button
                                class="ctx-menu-item"
                                on:click=move |_| {
                                    menu.set(None);
                                    start_rename();
                                }
                            >
                                {move || t("rename", ctx.lang.get())}
                            </button>
                            <div class="ctx-menu-sep" role="separator" />
                            <button
                                class="ctx-menu-item ctx-menu-danger"
                                on:click=move |_| {
                                    menu.set(None);
                                    actions::delete_page(ctx, FileEntry { name: name.clone(), path: path.get_value() });
                                }
                            >
                                {move || t("delete_page", ctx.lang.get())}
                            </button>
                        </div>
                    </div>
                }
            })}
        </div>
    }
}
