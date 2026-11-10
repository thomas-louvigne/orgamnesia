//! The open project, the menu switching to another one, the button opening (or
//! creating) one.

use leptos::prelude::*;
use wasm_bindgen::{closure::Closure, JsCast};
use wasm_bindgen_futures::spawn_local;

use crate::{
    actions::{self, project_name},
    i18n::t,
    invoke,
    preferences::{self, File},
    state::AppCtx,
};

/// Name of the open project, with the menu of the known projects and, always
/// there, the "preference" project.
#[component]
pub fn ProjectSwitcher() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    // The known projects whose folder exists, asked again at each opening (a folder may come back)
    let shown = RwSignal::new(Vec::<String>::new());
    Effect::new(move |_| {
        if !ctx.ui.show_projects.get() { return; }
        let all = ctx.project.projects.get();
        spawn_local(async move {
            if let Ok(list) = invoke::existing_folders(&all).await { shown.set(list); }
        });
    });
    view! {
        <div class="frame project-switcher">
            <button
                class="project-current"
                title=move || if ctx.project.preference.get() {
                    t("pref_project_title", ctx.lang.get()).to_string()
                } else {
                    ctx.project.vault_path.get().unwrap_or_default()
                }
                on:click=move |_| ctx.ui.show_projects.update(|v| *v = !*v)
            >
                <span class="project-name">
                    {move || if ctx.project.preference.get() {
                        preferences::NAME.to_string()
                    } else {
                        ctx.project.vault_path.get()
                            .map(|p| project_name(&p))
                            .unwrap_or_else(|| t("no_project", ctx.lang.get()).to_string())
                    }}
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
                        each=move || shown.get()
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
                                    class=move || if !ctx.project.preference.get() && ctx.project.vault_path.get().as_deref() == Some(p_cmp.as_str()) {
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
                    // The configuration files, always there: never removed
                    <div
                        class=move || if ctx.project.preference.get() {
                            "project-item list-item project-preference active"
                        } else { "project-item list-item project-preference" }
                        title=move || t("pref_project_title", ctx.lang.get())
                        role="button"
                        tabindex="0"
                        on:keydown=crate::a11y::list_item_keys
                        on:click=move |_| {
                            ctx.ui.show_projects.set(false);
                            preferences::open(ctx, File::Settings, None);
                        }
                    >
                        <span class="project-item-name">{preferences::NAME}</span>
                    </div>
                    <button class="project-open-btn" on:click=move |_| {
                        ctx.ui.show_projects.set(false);
                        spawn_local(actions::pick_and_open_project(ctx));
                    }>
                        {move || t("open_project", ctx.lang.get())}
                    </button>
                    {move || ctx.pref(|p| p.git_ext).then(|| view! {
                        <button class="project-open-btn" on:click=move |_| {
                            ctx.ui.show_projects.set(false);
                            ctx.close_panels();
                            ctx.ui.show_clone.set(true);
                        }>
                            {move || t("git_clone_title", ctx.lang.get())}
                        </button>
                    })}
                </div>
            })}
        </div>
    }
}

/// While the menu is open, Up / Down move through it, Enter opens the project
/// chosen, Escape closes it, wherever the focus is (the editor keeps it, and
/// would take the arrows otherwise): hence a capturing listener, for the whole app.
pub fn listen(ctx: AppCtx) {
    let on_key = Closure::<dyn Fn(web_sys::KeyboardEvent)>::new(move |e: web_sys::KeyboardEvent| {
        if !ctx.ui.show_projects.get_untracked() || e.ctrl_key() || e.alt_key() || e.meta_key() || e.shift_key() { return; }
        let key = e.key();
        if !matches!(key.as_str(), "ArrowUp" | "ArrowDown" | "Enter" | "Escape") { return; }
        e.prevent_default();
        e.stop_propagation();
        if key == "Escape" { ctx.ui.show_projects.set(false); return; }
        let Some(items) = document().query_selector_all(".project-menu .project-item").ok() else { return };
        let items: Vec<web_sys::HtmlElement> = (0..items.length())
            .filter_map(|i| items.item(i)?.dyn_into().ok()).collect();
        let has = |el: &web_sys::HtmlElement, class: &str| el.class_list().contains(class);
        let selected = items.iter().position(|el| has(el, "selected"));
        if key == "Enter" {
            // Nothing chosen yet: Enter keeps the open project
            match selected {
                Some(i) => items[i].click(),
                None => ctx.ui.show_projects.set(false),
            }
            return;
        }
        if items.is_empty() { return; }
        // From the open project the first time
        let from = selected.or_else(|| items.iter().position(|el| has(el, "active")));
        let n = items.len();
        let next = match (from, key == "ArrowDown") {
            (Some(i), true) => (i + 1) % n,
            (Some(i), false) => (i + n - 1) % n,
            (None, true) => 0,
            (None, false) => n - 1,
        };
        for (i, el) in items.iter().enumerate() { let _ = el.class_list().toggle_with_force("selected", i == next); }
        items[next].scroll_into_view_with_bool(false);
    });
    let _ = window().add_event_listener_with_callback_and_bool("keydown", on_key.as_ref().unchecked_ref(), true);
    on_key.forget(); // as long as the app
}
