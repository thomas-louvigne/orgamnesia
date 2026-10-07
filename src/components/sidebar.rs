use leptos::{ev, prelude::*};
use wasm_bindgen_futures::spawn_local;

use crate::{
    i18n::t,
    invoke,
    state::{AppCtx, FileEntry, Tab, VaultChanges},
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

/// Apply changes made to the project's pages by another program: refresh the
/// page list, reload the open tabs without unsaved changes, warn about the others.
pub async fn apply_disk_changes(ctx: AppCtx, changes: VaultChanges) {
    if ctx.files.get_untracked() != changes.files {
        ctx.files.set(changes.files);
    }
    let lang = ctx.lang.get_untracked();
    for tab in ctx.tabs.get_untracked() {
        // The tab may be closed while a file is read: its signals are then gone
        let dirty = || tab.dirty.try_get_untracked().unwrap_or(true);
        if changes.removed.contains(&tab.path) {
            ctx.status.set(Some(format!("{} : {}", tab.name, t("deleted_on_disk", lang))));
        } else if changes.changed.contains(&tab.path) {
            if dirty() {
                ctx.status.set(Some(format!("{} : {}", tab.name, t("changed_on_disk", lang))));
                continue;
            }
            if let Ok(content) = invoke::read_file(&tab.path).await {
                if !dirty() && tab.content.try_get_untracked().is_some_and(|c| c != content) {
                    tab.content.try_set(content);
                }
            }
        }
    }
    ctx.links_version.update(|v| *v += 1);
}

/// Delete a page after confirmation, and close its tab if open.
pub fn delete_page(ctx: AppCtx, file: FileEntry) {
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
            {move || ctx.show_brand.get().then(|| view! {
                <div class="app-brand">
                    <img class="app-brand-logo" src="app-icon.svg" alt="" />
                    <span class="app-brand-name">"Orgamnesia"</span>
                </div>
            })}
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
            {move || ctx.git_ext.get().then(|| view! { <GitLine /> })}
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
                                    delete_page(ctx, f_del.clone());
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

/// Git logo (simple-icons, CC0), drawn in the current text color.
#[component]
fn GitIcon() -> impl IntoView {
    view! {
        <svg class="git-logo" viewBox="0 0 24 24" aria-hidden="true">
            <path fill="currentColor" d="M23.546 10.93L13.067.452c-.604-.603-1.582-.603-2.188 0L8.708 2.627l2.76 2.76c.645-.215 1.379-.07 1.889.441.516.515.658 1.258.438 1.9l2.658 2.66c.645-.223 1.387-.078 1.9.435.721.72.721 1.884 0 2.604-.719.719-1.881.719-2.6 0-.539-.541-.674-1.337-.404-1.996L12.86 8.955v6.525c.176.086.342.203.488.348.713.721.713 1.883 0 2.6-.719.721-1.889.721-2.609 0-.719-.719-.719-1.879 0-2.598.182-.18.387-.316.605-.406V8.835c-.217-.091-.424-.222-.6-.401-.545-.545-.676-1.342-.396-2.009L7.636 3.7.45 10.881c-.6.605-.6 1.584 0 2.189l10.48 10.477c.604.604 1.582.604 2.186 0l10.43-10.43c.605-.603.605-1.582 0-2.187" />
        </svg>
    }
}

/// Git state of the open project, under its name (git extension).
#[component]
fn GitLine() -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");
    let lang = move || ctx.lang.get();
    move || {
        let st = ctx.git.get()?;
        ctx.vault_path.get()?;
        if !st.repo {
            return Some(view! {
                <div class="git-status git-off">
                    <GitIcon />
                    <span class="git-branch">{move || t("git_no_repo", lang())}</span>
                </div>
            }.into_any());
        }
        let branch = st.branch.clone().unwrap_or_else(|| t("git_detached", lang()).to_string());
        let mut items: Vec<(&'static str, String)> = vec![];
        if st.changes > 0 { items.push(("git-dirty", format!("● {} {}", st.changes, t("git_to_commit", lang())))); }
        if st.ahead > 0 { items.push(("git-ahead", format!("↑ {} {}", st.ahead, t("git_to_push", lang())))); }
        if st.behind > 0 { items.push(("git-behind", format!("↓ {} {}", st.behind, t("git_to_pull", lang())))); }
        if !st.upstream { items.push(("git-local", t("git_no_upstream", lang()).to_string())); }
        if items.is_empty() { items.push(("git-clean", format!("✓ {}", t("git_up_to_date", lang())))); }
        Some(view! {
            <div class="git-status">
                <div class="git-head">
                    <GitIcon />
                    <span class="git-branch" title=branch.clone()>{branch.clone()}</span>
                </div>
                <div class="git-items">
                    {items.into_iter().map(|(cls, text)| view! {
                        <span class=format!("git-item {cls}")>{text}</span>
                    }).collect_view()}
                </div>
            </div>
        }.into_any())
    }
}
