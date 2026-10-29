//! Windows setting up git (git extension): adding a remote to the project's
//! repository, and cloning a repository as a new project.

use leptos::{ev, prelude::*};
use wasm_bindgen_futures::spawn_local;

use crate::{
    actions, git,
    i18n::t,
    invoke,
    state::AppCtx,
};

use super::settings::fields::PathField;

/// The address of the remote to add as `origin`.
#[component]
pub fn RemoteModal() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();
    let close = move || ctx.ui.show_git_remote.set(false);
    let url = RwSignal::new(String::new());
    let input_ref = NodeRef::<leptos::html::Input>::new();
    Effect::new(move |_| if let Some(el) = input_ref.get() { let _ = el.focus(); });
    let esc = window_event_listener(ev::keydown, move |e| if e.key() == "Escape" { close(); });
    on_cleanup(move || esc.remove());

    let add = move || {
        let u = url.get_untracked().trim().to_string();
        if u.is_empty() { return; }
        close();
        git::add_remote(ctx, u);
    };

    view! {
        <div class="modal-overlay" on:click=move |_| close()>
            <div class="modal" role="dialog" aria-label=move || t("git_remote_title", lang()) on:click=|e| e.stop_propagation()>
                <div class="modal-header">
                    <h2>{move || t("git_remote_title", lang())}</h2>
                    <button class="btn-close" aria-label=move || t("close", lang()) on:click=move |_| close()>"×"</button>
                </div>
                <div class="modal-body">
                    <input
                        node_ref=input_ref
                        type="text"
                        class="setting-input"
                        aria-label=move || t("git_remote_title", lang())
                        placeholder="git@github.com:me/notes.git"
                        prop:value=move || url.get()
                        on:input=move |e| url.set(event_target_value(&e))
                        on:keydown=move |e| { e.stop_propagation(); if e.key() == "Enter" { add(); } else if e.key() == "Escape" { close(); } }
                    />
                    <span class="setting-hint">{move || t("git_remote_hint", lang())}</span>
                </div>
                <div class="modal-footer">
                    <button class="btn-secondary" on:click=move |_| close()>{move || t("cancel", lang())}</button>
                    <button class="btn-primary" prop:disabled=move || url.with(|u| u.trim().is_empty()) on:click=move |_| add()>
                        {move || t("git_remote_add", lang())}
                    </button>
                </div>
            </div>
        </div>
    }
}

/// The address of a repository and the folder to clone it in; the clone is
/// then opened as the project.
#[component]
pub fn CloneModal() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();
    let busy = RwSignal::new(false);
    let close = move || if !busy.get_untracked() { ctx.ui.show_clone.set(false); };
    let url = RwSignal::new(String::new());
    // Beside the open project, else in the home folder
    let parent = RwSignal::new(ctx.project.vault_path.get_untracked()
        .and_then(|p| p.trim_end_matches('/').rsplit_once('/').map(|(dir, _)| dir.to_string()))
        .filter(|d| !d.is_empty())
        .unwrap_or_else(|| "~".to_string()));
    let input_ref = NodeRef::<leptos::html::Input>::new();
    Effect::new(move |_| if let Some(el) = input_ref.get() { let _ = el.focus(); });
    let esc = window_event_listener(ev::keydown, move |e| if e.key() == "Escape" { close(); });
    on_cleanup(move || esc.remove());

    let can_clone = move || !busy.get() && url.with(|u| !u.trim().is_empty()) && parent.with(|p| !p.trim().is_empty());
    let do_clone = move || {
        if !can_clone() { return; }
        busy.set(true);
        spawn_local(async move {
            ctx.notify_t("git_cloning", "");
            match invoke::git_clone(&parent.get_untracked(), &url.get_untracked()).await {
                Ok(path) => {
                    ctx.notify_t("git_cloned", &path);
                    ctx.ui.show_clone.set(false);
                    actions::open_project(ctx, path).await;
                }
                Err(e) => {
                    let _ = busy.try_set(false);
                    ctx.error("git_clone_error", &e);
                }
            }
        });
    };

    view! {
        <div class="modal-overlay" on:click=move |_| close()>
            <div class="modal" role="dialog" aria-label=move || t("git_clone_title", lang()) on:click=|e| e.stop_propagation()>
                <div class="modal-header">
                    <h2>{move || t("git_clone_title", lang())}</h2>
                    <button class="btn-close" aria-label=move || t("close", lang()) on:click=move |_| close()>"×"</button>
                </div>
                <div class="modal-body">
                    <label class="commit-label" for="clone-url">{move || t("git_clone_url", lang())}</label>
                    <input
                        id="clone-url"
                        node_ref=input_ref
                        type="text"
                        class="setting-input"
                        placeholder="git@github.com:me/notes.git"
                        prop:disabled=move || busy.get()
                        prop:value=move || url.get()
                        on:input=move |e| url.set(event_target_value(&e))
                        on:keydown=move |e| { e.stop_propagation(); if e.key() == "Enter" { do_clone(); } else if e.key() == "Escape" { close(); } }
                    />
                    <label class="commit-label">{move || t("git_clone_parent", lang())}</label>
                    <PathField
                        value=parent.into()
                        on_change=Callback::new(move |v| parent.set(v))
                        disabled=Signal::derive(move || busy.get())
                    />
                    <span class="setting-hint">{move || t("git_clone_hint", lang())}</span>
                </div>
                <div class="modal-footer">
                    <button class="btn-secondary" prop:disabled=move || busy.get() on:click=move |_| close()>{move || t("cancel", lang())}</button>
                    <button class="btn-primary" prop:disabled=move || !can_clone() on:click=move |_| do_clone()>
                        {move || busy.get().then(|| view! { <span class="git-spinner" aria-hidden="true"></span> })}
                        {move || t("git_clone", lang())}
                    </button>
                </div>
            </div>
        </div>
    }
}
