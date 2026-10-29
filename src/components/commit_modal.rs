//! The commit window of the git extension: the files changed, each with what
//! changed in it and a box to leave it out, and the message, proposed from them,
//! to edit before committing.

use std::collections::HashSet;

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
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();
    let close = move || ctx.ui.show_commit.set(false);
    // None while git is read
    let changes = RwSignal::new(None::<Vec<GitChange>>);
    let message = RwSignal::new(String::new());
    // The message was typed: no longer proposed again when the files chosen change
    let edited = StoredValue::new(false);
    // The files to commit (paths), all at first
    let chosen = RwSignal::new(HashSet::<String>::new());
    // The file whose changes are shown, and them (None while read)
    let open_diff = RwSignal::new(None::<String>);
    let diff_text = RwSignal::new(None::<Result<String, String>>);
    let area_ref = NodeRef::<leptos::html::Textarea>::new();

    spawn_local(async move {
        match invoke::git_changes().await {
            Ok(list) if list.is_empty() => {
                ctx.notify_t("git_nothing_to_commit", "");
                let _ = ctx.ui.show_commit.try_set(false);
            }
            Ok(list) => {
                let _ = message.try_set(git::proposed_message(ctx, &list));
                let _ = chosen.try_set(list.iter().map(|c| c.path.clone()).collect());
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

    let all = move || changes.with(|c| c.as_ref().map_or(0, Vec::len)) == chosen.with(HashSet::len);
    // A box ticked or not: the message proposed follows, unless typed
    let toggle = move |path: Option<String>| {
        chosen.update(|set| match path {
            Some(p) => { if !set.remove(&p) { set.insert(p); } }
            None if all() => set.clear(),
            None => *set = changes.with_untracked(|c| c.iter().flatten().map(|c| c.path.clone()).collect()),
        });
        if !edited.get_value() {
            let picked: Vec<GitChange> = changes.with_untracked(|c| c.iter().flatten()
                .filter(|c| chosen.with_untracked(|s| s.contains(&c.path))).cloned().collect());
            message.set(if picked.is_empty() { String::new() } else { git::proposed_message(ctx, &picked) });
        }
    };
    let show_diff = move |c: GitChange| {
        if open_diff.get_untracked().as_deref() == Some(c.path.as_str()) {
            open_diff.set(None);
            return;
        }
        open_diff.set(Some(c.path.clone()));
        diff_text.set(None);
        spawn_local(async move {
            let got = invoke::git_diff(&c.path, c.old_path.as_deref()).await;
            if open_diff.get_untracked().as_deref() == Some(c.path.as_str()) { let _ = diff_text.try_set(Some(got)); }
        });
    };

    let can_commit = move || chosen.with(|c| !c.is_empty()) && message.with(|m| !m.trim().is_empty());
    // The main button (and its keys) also pushes, when the settings say so
    let main_push = ctx.pref_untracked(|p| p.git_commit_push);
    let do_commit = move |and_push: bool| {
        if !can_commit() { return; }
        // All of them: every change of the project, as `git add -A` sees it
        let files = (!all()).then(|| changes.with_untracked(|c| c.iter().flatten()
            .filter(|c| chosen.with_untracked(|s| s.contains(&c.path)))
            .flat_map(|c| std::iter::once(c.path.clone()).chain(c.old_path.clone()))
            .collect::<Vec<_>>()));
        close();
        git::commit(ctx, message.get_untracked(), files, and_push);
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
                "c" if ctrl_only => { e.prevent_default(); do_commit(main_push); return; }
                "k" if ctrl_only => { e.prevent_default(); close(); return; }
                _ => {}
            }
        }
        match key.as_str() {
            "escape" => close(),
            "enter" if ctrl_only => { e.prevent_default(); do_commit(main_push); }
            "c" if ctrl_only && emacs() => { e.prevent_default(); pending_c.set_value(true); }
            // The Emacs kills and yank, the caret movements… as in the other fields
            _ => { crate::text_field::on_keydown(ctx, &e); }
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
                        Some(list) => { let total = list.len(); view! {
                            <label class="commit-files-title setting-check">
                                <input type="checkbox" prop:checked=all on:change=move |_| toggle(None) />
                                {move || format!("{} ({}/{})", t("git_files_changed", lang()), chosen.with(HashSet::len), total)}
                            </label>
                            <ul class="commit-files">
                                {list.into_iter().map(|c| {
                                    let (p_check, p_box, p_open, p_diff) = (c.path.clone(), c.path.clone(), c.path.clone(), c.path.clone());
                                    let change = c.clone();
                                    view! {
                                    <li>
                                        <input type="checkbox"
                                            aria-label=c.path.clone()
                                            prop:checked=move || chosen.with(|s| s.contains(&p_check))
                                            on:change=move |_| toggle(Some(p_box.clone())) />
                                        <span class=format!("commit-kind commit-{:?}", c.kind).to_lowercase()>
                                            {t(git::kind_key(c.kind), lang()).trim_end_matches([':', ' ']).to_string()}
                                        </span>
                                        <button class="commit-path"
                                            title=move || t("git_show_diff", lang())
                                            aria-expanded=move || (open_diff.get().as_deref() == Some(p_open.as_str())).to_string()
                                            on:click=move |_| show_diff(change.clone())>
                                            {c.path.clone()}
                                        </button>
                                    </li>
                                    {move || (open_diff.get().as_deref() == Some(p_diff.as_str())).then(|| view! {
                                        <li class="commit-diff">
                                            {move || match diff_text.get() {
                                                None => view! { <p class="setting-hint">{t("git_reading", lang())}</p> }.into_any(),
                                                Some(Err(e)) => view! { <p class="setting-hint">{e}</p> }.into_any(),
                                                Some(Ok(text)) => crate::components::git_history::diff_view(&text).into_any(),
                                            }}
                                        </li>
                                    })}
                                }}).collect_view()}
                            </ul>
                        }.into_any() }
                    }}
                    <label class="commit-label" for="commit-message">{move || t("git_message", lang())}</label>
                    <textarea
                        id="commit-message"
                        node_ref=area_ref
                        class="setting-input commit-message"
                        spellcheck="true"
                        prop:value=move || message.get()
                        on:input=move |e| {
                            edited.set_value(true);
                            message.set(event_target_value(&e));
                        }
                    />
                    <span class="setting-hint">{move || format!("{} {}",
                        t(if emacs() { "git_commit_keys_emacs" } else { "git_commit_keys" }, lang()),
                        t("git_commit_all", lang()))}</span>
                </div>
                <div class="modal-footer">
                    <button class="btn-secondary" on:click=move |_| close()>{move || t("cancel", lang())}</button>
                    <button class="btn-secondary" prop:disabled=move || !can_commit() on:click=move |_| do_commit(!main_push)>
                        {move || t(if main_push { "git_commit_only" } else { "git_commit_push" }, lang())}
                    </button>
                    <button class="btn-primary" prop:disabled=move || !can_commit() on:click=move |_| do_commit(main_push)>
                        {move || t(if main_push { "git_commit_push" } else { "git_commit" }, lang())}
                    </button>
                </div>
            </div>
        </div>
    }
}
