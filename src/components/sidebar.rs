use leptos::{ev, prelude::*};
use wasm_bindgen_futures::spawn_local;

use crate::{
    i18n::t,
    invoke,
    state::{AppCtx, FileEntry, Tab},
};


// ─── Projects ────────────────────────────────────────────────────────────────

/// Last path component, used as the project's display name.
pub fn project_name(path: &str) -> String {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(path)
        .to_string()
}

/// Switch to the project at `path`: loads its files and resets tabs/backlinks.
/// Asks for confirmation first when open tabs have unsaved changes.
pub async fn open_project(ctx: AppCtx, path: String) {
    if ctx.vault_path.get_untracked().as_deref() == Some(path.as_str()) {
        return;
    }
    if ctx.tabs.get_untracked().iter().any(|t| t.dirty.get_untracked()) {
        let msg = t("discard_confirm", ctx.lang.get_untracked()).to_string();
        ctx.ask_confirm(msg, move || spawn_local(load_project(ctx, path.clone())));
        return;
    }
    load_project(ctx, path).await;
}

async fn load_project(ctx: AppCtx, path: String) {
    match invoke::open_vault(&path).await {
        Ok(files) => {
            ctx.vault_path.set(Some(path.clone()));
            ctx.files.set(files);
            ctx.tabs.set(vec![]);
            ctx.active_tab.set(None);
            ctx.backlinks.set(vec![]);
            ctx.projects.update(|p| {
                p.retain(|x| x != &path);
                p.insert(0, path);
            });
        }
        Err(e) => ctx.status.set(Some(format!("Could not open project: {e}"))),
    }
}

/// Reload the content of open tabs without unsaved changes (after links were
/// rewritten on disk by a rename).
pub async fn reload_clean_tabs(ctx: AppCtx) {
    for tab in ctx.tabs.get_untracked() {
        if tab.dirty.get_untracked() { continue; }
        if let Ok(content) = invoke::read_file(&tab.path).await {
            if content != tab.content.get_untracked() {
                tab.content.set(content);
            }
        }
    }
}

/// Create the page `name`, add it to the list and open it in a new tab.
pub fn create_and_open_page(ctx: AppCtx, name: String) {
    // Allocate signals in the reactive owner scope, not inside the async block
    let content_sig = RwSignal::new(String::new());
    let dirty_sig = RwSignal::new(true);
    spawn_local(async move {
        match invoke::create_page(&name).await {
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
}

/// Native folder picker, then open the chosen folder as a project.
pub async fn pick_and_open_project(ctx: AppCtx) {
    match invoke::pick_folder().await {
        Ok(Some(path)) => open_project(ctx, path).await,
        Ok(None) => {}
        Err(e) => ctx.status.set(Some(format!("Error: {e}"))),
    }
}

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
    // File right-clicked in the list (None = click on empty area)
    let ctx_target   = RwSignal::new(None::<FileEntry>);
    // Path of the file being renamed inline, and the text in the input
    let renaming     = RwSignal::new(None::<String>);
    let rename_value = RwSignal::new(String::new());
    let rename_ref   = NodeRef::<leptos::html::Input>::new();

    Effect::new(move |_| {
        if renaming.get().is_some() {
            if let Some(el) = rename_ref.get() {
                let _ = el.focus();
                let _ = el.select();
            }
        }
    });

    // Delete a page after confirmation, and close its tab if open
    let delete_file = move |file: FileEntry| {
        let lang = ctx.lang.get_untracked();
        let msg = format!("{} « {} » ?", t("delete_confirm", lang), file.name);
        ctx.ask_confirm(msg, move || {
            let file = file.clone();
            spawn_local(async move {
            match invoke::delete_page(&file.path).await {
                Ok(()) => {
                    ctx.files.update(|fs| fs.retain(|f| f.path != file.path));
                    if let Some(i) = ctx.tabs.get_untracked().iter().position(|t| t.path == file.path) {
                        ctx.tabs.update(|tabs| { tabs.remove(i); });
                        let len = ctx.tabs.get_untracked().len();
                        ctx.active_tab.update(|a| {
                            *a = match *a {
                                Some(x) if x == i => if len == 0 { None } else { Some(i.min(len - 1)) },
                                Some(x) if x > i => Some(x - 1),
                                other => other,
                            };
                        });
                        if ctx.active_tab.get_untracked().is_none() {
                            ctx.backlinks.set(vec![]);
                        }
                    }
                    ctx.links_version.update(|v| *v += 1);
                    ctx.status.set(Some(format!("{} {}", t("deleted", lang), file.name)));
                }
                Err(e) => ctx.status.set(Some(format!("Delete error: {e}"))),
            }
            });
        });
    };

    let do_rename = move || {
        let Some(old_path) = renaming.get_untracked() else { return };
        renaming.set(None);
        let new_name = rename_value.get_untracked().trim().to_string();
        let old_name = ctx.files.get_untracked().into_iter()
            .find(|f| f.path == old_path).map(|f| f.name).unwrap_or_default();
        if new_name.is_empty() || new_name == old_name {
            return;
        }
        spawn_local(async move {
            match invoke::rename_page(&old_path, &new_name).await {
                Ok(nf) => {
                    ctx.tabs.update(|tabs| {
                        if let Some(t) = tabs.iter_mut().find(|t| t.path == old_path) {
                            t.name = nf.name.clone();
                            t.path = nf.path.clone();
                        }
                    });
                    ctx.files.update(|files| {
                        if let Some(f) = files.iter_mut().find(|f| f.path == old_path) {
                            f.name = nf.name.clone();
                            f.path = nf.path.clone();
                        }
                        files.sort_by(|a, b| a.name.cmp(&b.name));
                    });
                    reload_clean_tabs(ctx).await;
                    ctx.status.set(Some(format!("Renamed to {}", nf.name)));
                }
                Err(e) => ctx.status.set(Some(format!("Rename error: {e}"))),
            }
        });
    };

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
        ctx_target.set(None);
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
            <div class="project-switcher">
                <button
                    class="project-current"
                    title=move || ctx.vault_path.get().unwrap_or_default()
                    on:click=move |_| ctx.show_projects.update(|v| *v = !*v)
                >
                    <span class="project-name">
                        {move || ctx.vault_path.get()
                            .map(|p| project_name(&p))
                            .unwrap_or_else(|| t("no_project", ctx.lang.get()).to_string())}
                    </span>
                    <span class="project-caret">"▾"</span>
                </button>
                <button
                    class="btn-icon"
                    title={move || t("open_project", ctx.lang.get())}
                    on:click=move |_| spawn_local(pick_and_open_project(ctx))
                >"📂"</button>
                {move || ctx.show_projects.get().then(|| view! {
                    <div class="ctx-overlay" on:click=move |_| ctx.show_projects.set(false) />
                    <div class="project-menu">
                        <For
                            each=move || ctx.projects.get()
                            key=|p| p.clone()
                            let:path
                        >
                            {
                                let p_open = path.clone();
                                let p_rm = path.clone();
                                let p_cmp = path.clone();
                                let label = project_name(&path);
                                view! {
                                    <div
                                        class=move || if ctx.vault_path.get().as_deref() == Some(p_cmp.as_str()) {
                                            "project-item active"
                                        } else { "project-item" }
                                        title=path.clone()
                                        on:click=move |_| {
                                            ctx.show_projects.set(false);
                                            spawn_local(open_project(ctx, p_open.clone()));
                                        }
                                    >
                                        <span class="project-item-name">{label}</span>
                                        <button
                                            class="btn-icon"
                                            title={move || t("remove_project", ctx.lang.get())}
                                            on:click=move |e: web_sys::MouseEvent| {
                                                e.stop_propagation();
                                                let p = p_rm.clone();
                                                spawn_local(async move {
                                                    if let Ok(list) = invoke::remove_project(&p).await {
                                                        ctx.projects.set(list);
                                                    }
                                                });
                                            }
                                        >"×"</button>
                                    </div>
                                }
                            }
                        </For>
                        <button class="project-open-btn" on:click=move |_| {
                            ctx.show_projects.set(false);
                            spawn_local(pick_and_open_project(ctx));
                        }>
                            {move || t("open_project", ctx.lang.get())}
                        </button>
                    </div>
                })}
            </div>
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
                        let file3 = file.clone();
                        let path_r = file.path.clone();
                        let is_active = move || {
                            ctx.active_tab.get()
                                .and_then(|i| ctx.tabs.get().into_iter().nth(i))
                                .map(|t| t.path == path)
                                .unwrap_or(false)
                        };
                        view! {
                            <div
                                class=move || if is_active() { "file-item active" } else { "file-item" }
                                on:click=move |_| {
                                    if renaming.get().is_none() { open_file(file2.clone()) }
                                }
                                on:contextmenu=move |e: web_sys::MouseEvent| {
                                    e.prevent_default();
                                    e.stop_propagation();
                                    ctx_menu_x.set(e.client_x());
                                    ctx_menu_y.set(e.client_y());
                                    ctx_target.set(Some(file3.clone()));
                                    show_ctx_menu.set(true);
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
                        {ctx_target.get().map(|f| {
                            let f_del = f.clone();
                            view! {
                                <button class="ctx-menu-item" on:click=move |_| {
                                    show_ctx_menu.set(false);
                                    rename_value.set(f.name.clone());
                                    renaming.set(Some(f.path.clone()));
                                }>
                                    {move || t("rename", ctx.lang.get())}
                                </button>
                                <button class="ctx-menu-item ctx-menu-danger" on:click=move |_| {
                                    show_ctx_menu.set(false);
                                    delete_file(f_del.clone());
                                }>
                                    {move || t("delete_page", ctx.lang.get())}
                                </button>
                            }
                        })}
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
