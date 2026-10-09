use leptos::{ev, prelude::*};
use wasm_bindgen_futures::spawn_local;

use crate::{
    actions::{self, project_name},
    i18n::t,
    invoke,
    state::{AppCtx, FileEntry},
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
            ctx.ui.show_new_page.set(false);
        }
    });
    on_cleanup(move || esc.remove());

    let do_create = move || {
        let n = name.get_untracked().trim().to_string();
        if n.is_empty() {
            return;
        }
        ctx.ui.show_new_page.set(false);
        actions::create_and_open_page(ctx, n);
    };

    view! {
        <div class="modal-overlay" on:click=move |_| ctx.ui.show_new_page.set(false)>
            <div class="modal" on:click=|e| e.stop_propagation()>
                <div class="modal-header">
                    <h2>{move || t("new_page_title", lang())}</h2>
                    <button class="btn-close" aria-label=move || t("close", ctx.lang.get()) on:click=move |_| ctx.ui.show_new_page.set(false)>"×"</button>
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
                    <button class="btn-secondary" on:click=move |_| ctx.ui.show_new_page.set(false)>
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

    let trigger_new_page = move || {
        if ctx.project.vault_path.get_untracked().is_none() {
            ctx.ui.status.set(Some(t("no_vault", ctx.lang.get_untracked()).to_string()));
            return;
        }
        ctx.ui.show_new_page.set(true);
    };

    view! {
        <nav class="sidebar" aria-label=move || t("pages", ctx.lang.get())>
            {move || ctx.pref(|p| p.show_brand).then(|| view! {
                <div class="app-brand">
                    <img class="app-brand-logo" src="app-icon.svg" alt="" />
                    <span class="app-brand-name">"Orgamnesia"</span>
                </div>
            })}
            <ProjectSwitcher />
            {move || ctx.pref(|p| p.git_ext).then(|| view! { <GitLine /> })}
            <div class="sidebar-header">
                <span class="sidebar-title">{move || t("pages", ctx.lang.get())}</span>
                <button
                    class="btn-icon"
                    title={move || t("new_page_title", ctx.lang.get())}
                    aria-label={move || t("new_page_title", ctx.lang.get())}
                    on:click=move |_| trigger_new_page()
                >"+"</button>
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
                    ctx_menu.set(Some((e.client_x(), e.client_y(), None)));
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
                                    ctx_menu.set(Some((e.client_x(), e.client_y(), Some(file_menu.clone()))));
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
                <button class="btn-settings" on:click=move |_| ctx.ui.show_settings.set(true)>
                    {move || t("settings_btn", ctx.lang.get())}
                </button>
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

/// Name of the open project, with the menu of the known projects.
#[component]
fn ProjectSwitcher() -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");
    view! {
        <div class="project-switcher">
            <button
                class="project-current"
                title=move || ctx.project.vault_path.get().unwrap_or_default()
                on:click=move |_| ctx.ui.show_projects.update(|v| *v = !*v)
            >
                <span class="project-name">
                    {move || ctx.project.vault_path.get()
                        .map(|p| project_name(&p))
                        .unwrap_or_else(|| t("no_project", ctx.lang.get()).to_string())}
                </span>
                <span class="project-caret">"▾"</span>
            </button>
            <button
                class="btn-icon btn-open-folder"
                title={move || t("open_project", ctx.lang.get())}
                aria-label={move || t("open_project", ctx.lang.get())}
                on:click=move |_| spawn_local(actions::pick_and_open_project(ctx))
            >
                <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor"
                    stroke-width="2" stroke-linejoin="round" aria-hidden="true">
                    <path d="M3 7.5V18a1.5 1.5 0 0 0 1.5 1.5h15A1.5 1.5 0 0 0 21 18V9a1.5 1.5 0 0 0-1.5-1.5h-7.2L10.4 5H4.5A1.5 1.5 0 0 0 3 6.5z" />
                </svg>
            </button>
            {move || ctx.ui.show_projects.get().then(|| view! {
                <div class="ctx-overlay" on:click=move |_| ctx.ui.show_projects.set(false) />
                <div class="project-menu">
                    <For
                        each=move || ctx.project.projects.get()
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
                                    class=move || if ctx.project.vault_path.get().as_deref() == Some(p_cmp.as_str()) {
                                        "project-item list-item active"
                                    } else { "project-item list-item" }
                                    title=path.clone()
                                    role="button"
                                    tabindex="0"
                                    on:keydown=crate::a11y::list_item_keys
                                    on:click=move |_| {
                                        ctx.ui.show_projects.set(false);
                                        spawn_local(actions::open_project(ctx, p_open.clone()));
                                    }
                                >
                                    <span class="project-item-name">{label}</span>
                                    <button
                                        class="btn-icon"
                                        title={move || t("remove_project", ctx.lang.get())}
                                        aria-label={move || t("remove_project", ctx.lang.get())}
                                        on:click=move |e: web_sys::MouseEvent| {
                                            e.stop_propagation();
                                            let p = p_rm.clone();
                                            spawn_local(async move {
                                                if let Ok(list) = invoke::remove_project(&p).await {
                                                    ctx.project.projects.set(list);
                                                }
                                            });
                                        }
                                    >"×"</button>
                                </div>
                            }
                        }
                    </For>
                    <button class="project-open-btn" on:click=move |_| {
                        ctx.ui.show_projects.set(false);
                        spawn_local(actions::pick_and_open_project(ctx));
                    }>
                        {move || t("open_project", ctx.lang.get())}
                    </button>
                </div>
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

/// Pull, commit and push buttons of the git extension, with their shortcuts in
/// their tooltips.
#[component]
fn GitButtons() -> impl IntoView {
    use crate::state::GitOp;
    let ctx = use_context::<AppCtx>().expect("AppCtx");
    let lang = move || ctx.lang.get();
    let busy = move || ctx.project.git_busy.get();
    let git = move || ctx.project.git.get().unwrap_or_default();
    let tip = move |label: &'static str, id: &'static str| {
        let keys = ctx.keybindings.with(|kb| kb.app_binds(id).first().cloned());
        match keys {
            Some(k) => format!("{} ({})", t(label, lang()), crate::keybindings::display(&k)),
            None => t(label, lang()).to_string(),
        }
    };
    let button = move |op: GitOp, label: &'static str, id: &'static str, icon: &'static str,
                       enabled: fn(&orgamnesia_core::GitStatus) -> bool, run: fn(AppCtx)| view! {
        <button
            class=move || if busy() == Some(op) { "git-btn running" } else { "git-btn" }
            title=move || tip(label, id)
            aria-label=move || tip(label, id)
            prop:disabled=move || busy().is_some() || !enabled(&git())
            on:click=move |_| run(ctx)
        >
            <span aria-hidden="true">{icon}</span>
            {move || t(label, lang())}
        </button>
    };
    view! {
        <div class="git-buttons">
            {button(GitOp::Pull, "git_pull", "git_pull", "↓", |g| g.upstream, crate::git::pull)}
            // Also with no change seen yet: unsaved pages are saved first
            {button(GitOp::Commit, "git_commit", "git_commit", "●", |_| true, crate::git::open_commit)}
            {button(GitOp::Push, "git_push", "git_push", "↑", |g| g.ahead > 0 || !g.upstream, crate::git::push)}
        </div>
    }
}

/// Git state of the open project, under its name (git extension).
#[component]
fn GitLine() -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");
    let lang = move || ctx.lang.get();
    move || {
        let st = ctx.project.git.get()?;
        ctx.project.vault_path.get()?;
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
                <GitButtons />
            </div>
        }.into_any())
    }
}
