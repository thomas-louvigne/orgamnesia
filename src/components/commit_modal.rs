//! The commit window of the git extension: the files changed, and the message,
//! proposed from them, to edit before committing.

use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::{
    git,
    i18n::t,
    invoke,
    state::{AppCtx, GitChange},
};

#[component]
pub fn CommitModal() -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");
    let lang = move || ctx.lang.get();
    let close = move || ctx.ui.show_commit.set(false);
    // None while git is read
    let changes = RwSignal::new(None::<Vec<GitChange>>);
    let message = RwSignal::new(String::new());
    let area_ref = NodeRef::<leptos::html::Textarea>::new();

    spawn_local(async move {
        match invoke::git_changes().await {
            Ok(list) if list.is_empty() => {
                ctx.notify_t("git_nothing_to_commit", "");
                let _ = ctx.ui.show_commit.try_set(false);
            }
            Ok(list) => {
                let _ = message.try_set(git::default_message(&list, ctx.lang.get_untracked()));
                let _ = changes.try_set(Some(list));
            }
            Err(e) => {
                ctx.error("git_commit_error", &e);
                let _ = ctx.ui.show_commit.try_set(false);
            }
        }
    });
    // The message gets the focus, its first line selected to be typed over
    Effect::new(move |_| {
        if changes.with(Option::is_some) && let Some(el) = area_ref.get() {
            let _ = el.focus();
            let first = message.with_untracked(|m| m.lines().next().map_or(0, |l| l.chars().count()));
            let _ = el.set_selection_range(0, first as u32);
        }
    });

    let can_commit = move || changes.with(|c| c.as_ref().is_some_and(|c| !c.is_empty()))
        && message.with(|m| !m.trim().is_empty());
    let do_commit = move |and_push: bool| {
        if !can_commit() { return; }
        close();
        git::commit(ctx, message.get_untracked(), and_push);
    };
    // Magit's `C-c C-c` (commit) and `C-c C-k` (cancel) with the Emacs keys
    let emacs = move || ctx.keybindings.with_untracked(|kb| kb.editor_preset == "emacs");
    let pending_c = StoredValue::new(false);
    let on_keydown = move |e: web_sys::KeyboardEvent| {
        // App shortcuts don't run while the message is typed
        e.stop_propagation();
        let ctrl_only = e.ctrl_key() && !e.alt_key() && !e.meta_key() && !e.shift_key();
        let key = e.key().to_lowercase();
        if pending_c.get_value() {
            pending_c.set_value(false);
            match key.as_str() {
                "c" if ctrl_only => { e.prevent_default(); do_commit(false); return; }
                "k" if ctrl_only => { e.prevent_default(); close(); return; }
                _ => {}
            }
        }
        match key.as_str() {
            "escape" => close(),
            "enter" if ctrl_only => { e.prevent_default(); do_commit(false); }
            "c" if ctrl_only && emacs() => { e.prevent_default(); pending_c.set_value(true); }
            _ => {}
        }
    };

    view! {
        <div class="modal-overlay" on:click=move |_| close()>
            <div class="modal commit-modal" role="dialog" aria-label=move || t("git_commit_title", lang())
                on:click=|e| e.stop_propagation() on:keydown=on_keydown>
                <div class="modal-header">
                    <h2>{move || t("git_commit_title", lang())}</h2>
                    <button class="btn-close" aria-label=move || t("close", lang()) on:click=move |_| close()>"×"</button>
                </div>
                <div class="modal-body commit-body">
                    {move || match changes.get() {
                        None => view! { <p class="setting-hint">{t("git_reading", lang())}</p> }.into_any(),
                        Some(list) => view! {
                            <div class="commit-files-title">
                                {format!("{} ({})", t("git_files_changed", lang()), list.len())}
                            </div>
                            <ul class="commit-files">
                                {list.into_iter().map(|c| view! {
                                    <li>
                                        <span class=format!("commit-kind commit-{:?}", c.kind).to_lowercase()>
                                            {t(git::kind_key(c.kind), lang()).trim_end_matches([':', ' ']).to_string()}
                                        </span>
                                        <span class="commit-path" title=c.path.clone()>{c.path.clone()}</span>
                                    </li>
                                }).collect_view()}
                            </ul>
                        }.into_any(),
                    }}
                    <label class="commit-label" for="commit-message">{move || t("git_message", lang())}</label>
                    <textarea
                        id="commit-message"
                        node_ref=area_ref
                        class="setting-input commit-message"
                        spellcheck="true"
                        prop:value=move || message.get()
                        on:input=move |e| message.set(event_target_value(&e))
                    />
                    <span class="setting-hint">{move || format!("{} {}",
                        t(if emacs() { "git_commit_keys_emacs" } else { "git_commit_keys" }, lang()),
                        t("git_commit_all", lang()))}</span>
                </div>
                <div class="modal-footer">
                    <button class="btn-secondary" on:click=move |_| close()>{move || t("cancel", lang())}</button>
                    <button class="btn-secondary" prop:disabled=move || !can_commit() on:click=move |_| do_commit(true)>
                        {move || t("git_commit_push", lang())}
                    </button>
                    <button class="btn-primary" prop:disabled=move || !can_commit() on:click=move |_| do_commit(false)>
                        {move || t("git_commit", lang())}
                    </button>
                </div>
            </div>
        </div>
    }
}
