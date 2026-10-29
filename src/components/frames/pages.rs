//! The pages of the project.

use leptos::prelude::*;

use crate::{
    actions,
    i18n::t,
    state::{AppCtx, FileEntry},
};

/// The pages of the project, with their filter.
#[component]
pub fn PagesFrame() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let filter = RwSignal::new(String::new());
    // Right-click menu: position, and the file clicked (None = click on empty area)
    let ctx_menu = RwSignal::new(None::<(i32, i32, Option<FileEntry>)>);
    // Path of the file being renamed inline, and the text in the input
    let renaming     = RwSignal::new(None::<String>);
    let rename_value = RwSignal::new(String::new());
    let rename_ref   = NodeRef::<leptos::html::Input>::new();

    Effect::new(move |_| {
        if renaming.get().is_some() && let Some(el) = rename_ref.get() {
            let _ = el.focus();
            el.select();
        }
    });

    let do_rename = move || {
        let Some(old_path) = renaming.get_untracked() else { return };
        renaming.set(None);
        actions::rename_page(ctx, old_path, rename_value.get_untracked());
    };

    let filtered = move || {
        let q = orgamnesia_core::names::fold(&filter.get());
        ctx.project.files.get().into_iter().filter(|f| {
            q.is_empty() || orgamnesia_core::names::fold(&f.name).contains(&q)
        }).collect::<Vec<_>>()
    };

    let active_path = Memo::new(move |_| ctx.work.active_tab_data().map(|t| t.path));
    // Pages differing from the remote (git extension): marked, to push
    let unpushed = Memo::new(move |_| {
        if !ctx.pref(|p| p.git_ext && p.git_unpushed_marker) || ctx.project.preference.get() { return std::collections::HashSet::new(); }
        ctx.project.git.with(|g| g.iter().flat_map(|g| g.unpushed.iter().cloned()).collect::<std::collections::HashSet<String>>())
    });

    let trigger_new_page = move || {
        if ctx.project.vault_path.get_untracked().is_none() {
            ctx.ui.status.set(Some(t("no_vault", ctx.lang.get_untracked()).to_string()));
            return;
        }
        ctx.ui.show_new_page.set(true);
    };

    view! {
        <nav class="frame frame-grow pages-frame" aria-label=move || t("pages", ctx.lang.get())>
            <div class="sidebar-header">
                <span class="sidebar-title">{move || t("pages", ctx.lang.get())}</span>
                // The configuration files can't be created, renamed nor deleted
                {move || (!ctx.project.preference.get()).then(|| view! {
                    <button
                        class="btn-icon"
                        title={move || ctx.with_shortcut("new_page_title", "new_page")}
                        aria-label={move || t("new_page_title", ctx.lang.get())}
                        on:click=move |_| trigger_new_page()
                    >"+"</button>
                })}
            </div>
            <div class="sidebar-search">
                <input
                    type="text"
                    class="search-input"
                    aria-label={move || t("filter_hint", ctx.lang.get())}
                    placeholder={move || t("filter_hint", ctx.lang.get())}
                    prop:value=move || filter.get()
                    on:input=move |e| filter.set(event_target_value(&e))
                />
            </div>
            <div
                class="sidebar-list"
                on:contextmenu=move |e: web_sys::MouseEvent| {
                    e.prevent_default();
                    if !ctx.project.preference.get_untracked() { ctx_menu.set(Some((e.client_x(), e.client_y(), None))); }
                }
            >
                <For
                    each=filtered
                    key={|f| f.path.clone()}
                    let:file
                >
                    {
                        let path = file.path.clone();
                        let path_r = file.path.clone();
                        let path_c = file.path.clone();
                        let path_u = file.path.clone();
                        let name = file.name.clone();
                        let file_open = file.clone();
                        let file_menu = file.clone();
                        view! {
                            <div
                                class=move || if active_path.get().as_deref() == Some(path.as_str()) {
                                    "file-item list-item active"
                                } else { "file-item list-item" }
                                role="button"
                                tabindex="0"
                                aria-current=move || (active_path.get().as_deref() == Some(path_c.as_str())).then_some("page")
                                on:keydown=crate::a11y::list_item_keys
                                on:click=move |_| {
                                    if renaming.get_untracked().is_none() {
                                        actions::open_file(ctx, file_open.clone(), None);
                                    }
                                }
                                on:contextmenu=move |e: web_sys::MouseEvent| {
                                    e.prevent_default();
                                    e.stop_propagation();
                                    if !ctx.project.preference.get_untracked() {
                                        ctx_menu.set(Some((e.client_x(), e.client_y(), Some(file_menu.clone()))));
                                    }
                                }
                            >
                                {move || if renaming.get().as_deref() == Some(path_r.as_str()) {
                                    view! {
                                        <input
                                            node_ref=rename_ref
                                            class="search-input"
                                            type="text"
                                            prop:value=move || rename_value.get()
                                            on:input=move |e| rename_value.set(event_target_value(&e))
                                            on:click=|e: web_sys::MouseEvent| e.stop_propagation()
                                            on:keydown=move |e: web_sys::KeyboardEvent| {
                                                e.stop_propagation();
                                                match e.key().as_str() {
                                                    "Enter" => do_rename(),
                                                    "Escape" => renaming.set(None),
                                                    _ => {}
                                                }
                                            }
                                            on:blur=move |_| do_rename()
                                        />
                                    }.into_any()
                                } else {
                                    name.clone().into_any()
                                }}
                                {move || unpushed.with(|u| u.contains(&path_u)).then(|| view! {
                                    <span class="file-unpushed"
                                        title=move || t("git_unpushed_file", ctx.lang.get())
                                        aria-label=move || t("git_unpushed_file", ctx.lang.get())>"↑"</span>
                                })}
                            </div>
                        }
                    }
                </For>
            </div>

            {move || ctx_menu.get().map(|(x, y, target)| {
                let close = move || ctx_menu.set(None);
                view! {
                    <div
                        class="ctx-overlay"
                        on:click=move |_| close()
                        on:contextmenu=move |e: web_sys::MouseEvent| {
                            e.prevent_default();
                            close();
                        }
                    />
                    <div class="ctx-menu" style=format!("left:{x}px;top:{y}px")>
                        {target.clone().map(|f| view! {
                            <button class="ctx-menu-item" on:click=move |_| {
                                close();
                                rename_value.set(f.name.clone());
                                renaming.set(Some(f.path.clone()));
                            }>
                                {move || t("rename", ctx.lang.get())}
                            </button>
                        })}
                        {target.clone().map(|f| view! {
                            <button
                                class="ctx-menu-item"
                                title=move || t("no_export_title", ctx.lang.get())
                                on:click=move |_| {
                                    close();
                                    actions::tag_page(ctx, f.clone(), "noexport");
                                }
                            >
                                {move || t("no_export", ctx.lang.get())}
                            </button>
                        })}
                        {target.clone().filter(|_| crate::git::history_available(ctx)).map(|f| view! {
                            <button class="ctx-menu-item" on:click=move |_| {
                                close();
                                crate::git::open_history(ctx, f.clone());
                            }>
                                {move || t("git_history_menu", ctx.lang.get())}
                            </button>
                        })}
                        <button class="ctx-menu-item" on:click=move |_| {
                            close();
                            trigger_new_page();
                        }>
                            {move || t("new_page_title", ctx.lang.get())}
                        </button>
                        // Deleting comes last, set apart
                        {target.map(|f| view! {
                            <div class="ctx-menu-sep" role="separator" />
                            <button class="ctx-menu-item ctx-menu-danger" on:click=move |_| {
                                close();
                                actions::delete_page(ctx, f.clone());
                            }>
                                {move || t("delete_page", ctx.lang.get())}
                            </button>
                        })}
                    </div>
                }
            })}
        </nav>
    }
}
