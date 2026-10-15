//! The open project, the menu switching to another one, the buttons opening and
//! creating one.

use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::{
    actions::{self, project_name},
    i18n::t,
    invoke,
    state::AppCtx,
};

/// Name of the open project, with the menu of the known projects.
#[component]
pub fn ProjectSwitcher() -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");
    view! {
        <div class="frame project-switcher">
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
            <button
                class="btn-icon btn-new-project"
                title={move || t("new_project", ctx.lang.get())}
                aria-label={move || t("new_project", ctx.lang.get())}
                on:click=move |_| spawn_local(actions::create_project(ctx))
            >
                <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor"
                    stroke-width="2" stroke-linejoin="round" stroke-linecap="round" aria-hidden="true">
                    <path d="M3 7.5V18a1.5 1.5 0 0 0 1.5 1.5h15A1.5 1.5 0 0 0 21 18V9a1.5 1.5 0 0 0-1.5-1.5h-7.2L10.4 5H4.5A1.5 1.5 0 0 0 3 6.5z" />
                    <path d="M12 10.5v6M9 13.5h6" />
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
                    <button class="project-open-btn" on:click=move |_| {
                        ctx.ui.show_projects.set(false);
                        spawn_local(actions::create_project(ctx));
                    }>
                        {move || t("new_project", ctx.lang.get())}
                    </button>
                </div>
            })}
        </div>
    }
}
