//! Setting up git (git extension). In the git frame: adding a remote to the
//! project's repository, making a branch. In the main area: who signs the commits,
//! and cloning a repository as a new project.

use leptos::{ev, prelude::*};
use wasm_bindgen_futures::spawn_local;

use crate::{
    actions, git,
    i18n::t,
    invoke,
    state::AppCtx,
};

use super::{panel, settings::fields::PathField};

/// One text to give, in the git frame: its title, what it is, the button sending it.
#[component]
fn FrameField(
    title: &'static str,
    hint: &'static str,
    button: &'static str,
    #[prop(optional)] placeholder: &'static str,
    close: Callback<()>,
    send: Callback<String>,
) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();
    let value = RwSignal::new(String::new());
    let input = NodeRef::<leptos::html::Input>::new();
    Effect::new(move |_| if let Some(el) = input.get() { let _ = el.focus(); });
    view! {
        <form class="git-form"
            on:submit=move |e: web_sys::SubmitEvent| {
                e.prevent_default();
                let text = value.get_untracked().trim().to_string();
                if !text.is_empty() { close.run(()); send.run(text); }
            }
            // App shortcuts don't run while it is typed
            on:keydown=move |e: web_sys::KeyboardEvent| {
                e.stop_propagation();
                if e.key() == "Escape" { close.run(()); }
            }
        >
            <label>
                {move || t(title, lang())}
                <input node_ref=input type="text" class="setting-input" placeholder=placeholder
                    prop:value=move || value.get()
                    on:input=move |e| value.set(event_target_value(&e)) />
            </label>
            <span class="setting-hint">{move || t(hint, lang())}</span>
            <div class="git-form-actions">
                <button type="button" class="btn-secondary" on:click=move |_| close.run(())>
                    {move || t("cancel", lang())}
                </button>
                <button type="submit" class="btn-primary" prop:disabled=move || value.with(|v| v.trim().is_empty())>
                    {move || t(button, lang())}
                </button>
            </div>
        </form>
    }
}

/// The address of the remote to add as `origin`.
#[component]
pub fn RemoteForm() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    view! {
        <FrameField title="git_remote_title" hint="git_remote_hint" button="git_remote_add"
            placeholder="git@github.com:me/notes.git"
            close=Callback::new(move |_| ctx.ui.show_git_remote.set(false))
            send=Callback::new(move |url| git::add_remote(ctx, url)) />
    }
}

/// The name of a new branch, made from here and switched to.
#[component]
pub fn BranchForm() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    view! {
        <FrameField title="git_new_branch" hint="git_new_branch_hint" button="git_branch_create"
            close=Callback::new(move |_| ctx.ui.show_git_branch.set(false))
            send=Callback::new(move |name| git::switch_branch(ctx, name, true)) />
    }
}

/// The name and email git signs the commits with, asked before the first
/// commit; saved for all the user's repositories, then the commit opens.
#[component]
pub fn IdentityPage() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();
    let close = move || ctx.ui.show_git_identity.set(false);
    let root = NodeRef::<leptos::html::Div>::new();
    panel::opened(ctx, root, close);
    let name = RwSignal::new(String::new());
    let email = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let input_ref = NodeRef::<leptos::html::Input>::new();
    Effect::new(move |_| if let Some(el) = input_ref.get() { let _ = el.focus(); });
    let esc = window_event_listener(ev::keydown, move |e| if e.key() == "Escape" { close(); });
    on_cleanup(move || esc.remove());
    // What git knew already, if anything
    spawn_local(async move {
        if let Ok((n, e)) = invoke::git_identity().await {
            let _ = name.try_set(n);
            let _ = email.try_set(e);
        }
    });

    let valid = move || name.with(|n| !n.trim().is_empty()) && email.with(|e| e.contains('@') && !e.trim().contains(' '));
    let save = move || {
        if !valid() || busy.get_untracked() { return; }
        busy.set(true);
        spawn_local(async move {
            match invoke::git_set_identity(&name.get_untracked(), &email.get_untracked()).await {
                Ok(()) => {
                    ctx.ui.show_git_identity.set(false);
                    git::open_commit(ctx);
                }
                Err(e) => {
                    let _ = busy.try_set(false);
                    ctx.error("git_identity_error", &e);
                }
            }
        });
    };
    let keys = move |e: web_sys::KeyboardEvent| {
        e.stop_propagation();
        if e.key() == "Enter" { save(); } else if e.key() == "Escape" { close(); }
    };

    view! {
        <div node_ref=root class="main-panel" tabindex="-1" role="region" aria-label=move || t("git_identity_title", lang())>
            <div class="modal-header">
                <h2>{move || t("git_identity_title", lang())}</h2>
                <button class="btn-close" aria-label=move || t("close", lang()) on:click=move |_| close()>"×"</button>
            </div>
            <div class="modal-body">
                <p class="setting-hint">{move || t("git_identity_hint", lang())}</p>
                <label class="commit-label" for="git-name">{move || t("git_identity_name", lang())}</label>
                <input id="git-name" node_ref=input_ref type="text" class="setting-input" autocomplete="name"
                    prop:value=move || name.get() on:input=move |e| name.set(event_target_value(&e)) on:keydown=keys />
                <label class="commit-label" for="git-email">{move || t("git_identity_email", lang())}</label>
                <input id="git-email" type="email" class="setting-input" autocomplete="email"
                    prop:value=move || email.get() on:input=move |e| email.set(event_target_value(&e)) on:keydown=keys />
            </div>
            <div class="modal-footer">
                <button class="btn-secondary" on:click=move |_| close()>{move || t("cancel", lang())}</button>
                <button class="btn-primary" prop:disabled=move || !valid() || busy.get() on:click=move |_| save()>
                    {move || t("git_identity_save", lang())}
                </button>
            </div>
        </div>
    }
}

/// The address of a repository and the folder to clone it in; the clone is
/// then opened as the project.
#[component]
pub fn ClonePage() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();
    let busy = RwSignal::new(false);
    let close = move || if !busy.get_untracked() { ctx.ui.show_clone.set(false); };
    let root = NodeRef::<leptos::html::Div>::new();
    panel::opened(ctx, root, close);
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
        <div node_ref=root class="main-panel" tabindex="-1" role="region" aria-label=move || t("git_clone_title", lang())>
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
    }
}
