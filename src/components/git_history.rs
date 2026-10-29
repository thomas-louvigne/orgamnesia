//! The history window of a page (git extension): the commits that changed it,
//! what each one changed or the page as it was then, and putting a version back.

use leptos::{ev, prelude::*};
use wasm_bindgen_futures::spawn_local;

use crate::{
    git,
    i18n::{t, Lang},
    invoke,
    state::{AppCtx, FileEntry, GitCommit},
};

/// A unified diff, its lines colored by what they are; git's headers left out.
pub fn diff_view(text: &str) -> impl IntoView + use<> {
    let lines: Vec<(&'static str, String)> = text.lines()
        .filter(|l| !["diff --git", "index ", "--- ", "+++ ", "new file mode", "deleted file mode", "similarity index", "rename from", "rename to"]
            .iter().any(|h| l.starts_with(h)))
        .map(|l| {
            let class = match l.chars().next() {
                Some('+') => "diff-line diff-add",
                Some('-') => "diff-line diff-del",
                Some('@') => "diff-line diff-hunk",
                _ => "diff-line",
            };
            (class, l.to_string())
        })
        .collect();
    view! {
        <pre class="diff-view">
            {lines.into_iter().map(|(class, l)| view! { <div class=class>{l}</div> }).collect_view()}
        </pre>
    }
}

/// When a commit was made, in the words of `lang`.
fn date(secs: i64, lang: Lang) -> String {
    let d = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(secs as f64 * 1000.0));
    let locale = if matches!(lang, Lang::Fr) { "fr-FR" } else { "en-GB" };
    String::from(d.to_locale_string(locale, &wasm_bindgen::JsValue::UNDEFINED))
}

#[component]
pub fn GitHistoryModal(file: FileEntry) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();
    let close = move || ctx.ui.git_history.set(None);
    // None while git is read
    let commits = RwSignal::new(None::<Vec<GitCommit>>);
    let selected = RwSignal::new(None::<usize>);
    // Show the page as it was (true), or what the commit changed in it
    let whole = RwSignal::new(false);
    // What is shown of the commit selected: None while read
    let shown = RwSignal::new(None::<Result<String, String>>);

    let path = file.path.clone();
    spawn_local(async move {
        match invoke::git_log(&path).await {
            Ok(list) => {
                let any = !list.is_empty();
                let _ = commits.try_set(Some(list));
                if any { let _ = selected.try_set(Some(0)); }
            }
            Err(e) => {
                ctx.error("git_history_error", &e);
                let _ = ctx.ui.git_history.try_set(None);
            }
        }
    });
    let commit = move || selected.get().and_then(|i| commits.with(|c| c.as_ref().and_then(|c| c.get(i).cloned())));
    Effect::new(move |_| {
        let (Some(c), as_whole) = (commit(), whole.get()) else { return };
        shown.set(None);
        spawn_local(async move {
            let got = if as_whole { invoke::git_show(&c.hash, &c.path).await } else { invoke::git_commit_diff(&c.hash, &c.path).await };
            // Still the one asked for
            if commit().is_some_and(|now| now.hash == c.hash) && whole.get_untracked() == as_whole {
                let _ = shown.try_set(Some(got));
            }
        });
    });

    let file_restore = StoredValue::new(file.clone());
    let restore = move || {
        let Some(c) = commit() else { return };
        let file = file_restore.get_value();
        let open = ctx.work.tab_index(&file.path).is_some();
        let go = move || {
            let (c, file) = (c.clone(), file.clone());
            spawn_local(async move {
                match invoke::git_show(&c.hash, &c.path).await {
                    Ok(text) => {
                        let _ = ctx.ui.git_history.try_set(None);
                        git::restore(ctx, file, text);
                    }
                    Err(e) => ctx.error("git_history_error", &e),
                }
            });
        };
        // In its tab, the editor's undo takes it back; on disk, it replaces the page
        if open {
            go();
        } else {
            ctx.ask_confirm_danger(t("git_restore_confirm", ctx.lang.get_untracked()).to_string(), "git_restore", go);
        }
    };

    let esc = window_event_listener(ev::keydown, move |e| {
        if e.key() == "Escape" && ctx.ui.confirm.with_untracked(Option::is_none) { close(); }
    });
    on_cleanup(move || esc.remove());

    let name = file.name.clone();
    view! {
        <div class="modal-overlay" on:click=move |_| close()>
            <div class="modal history-modal" role="dialog" aria-label=move || t("git_history_title", lang())
                on:click=|e| e.stop_propagation()>
                <div class="modal-header">
                    <h2>{move || format!("{} — {name}", t("git_history_title", lang()))}</h2>
                    <button class="btn-close" aria-label=move || t("close", lang()) on:click=move |_| close()>"×"</button>
                </div>
                <div class="modal-body history-body">
                    {move || match commits.get() {
                        None => view! { <p class="setting-hint">{t("git_reading", lang())}</p> }.into_any(),
                        Some(list) if list.is_empty() => view! {
                            <p class="setting-hint">{t("git_history_empty", lang())}</p>
                        }.into_any(),
                        Some(list) => view! {
                            <ul class="history-list" role="listbox">
                                {list.into_iter().enumerate().map(|(i, c)| view! {
                                    <li
                                        class=move || if selected.get() == Some(i) { "history-item list-item active" } else { "history-item list-item" }
                                        role="option"
                                        tabindex="0"
                                        aria-selected=move || (selected.get() == Some(i)).to_string()
                                        on:keydown=crate::a11y::list_item_keys
                                        on:click=move |_| selected.set(Some(i))
                                    >
                                        <span class="history-subject">{c.subject.clone()}</span>
                                        <span class="history-meta">{move || format!("{} · {}", date(c.date, lang()), c.author)}</span>
                                    </li>
                                }).collect_view()}
                            </ul>
                            <div class="history-view">
                                <div class="history-switch" role="group">
                                    <button class=move || if whole.get() { "btn-secondary" } else { "btn-secondary active" }
                                        aria-pressed=move || (!whole.get()).to_string()
                                        on:click=move |_| whole.set(false)>{move || t("git_history_changes", lang())}</button>
                                    <button class=move || if whole.get() { "btn-secondary active" } else { "btn-secondary" }
                                        aria-pressed=move || whole.get().to_string()
                                        on:click=move |_| whole.set(true)>{move || t("git_history_whole", lang())}</button>
                                </div>
                                {move || match shown.get() {
                                    None => view! { <p class="setting-hint">{t("git_reading", lang())}</p> }.into_any(),
                                    Some(Err(e)) => view! { <p class="setting-hint">{e}</p> }.into_any(),
                                    Some(Ok(text)) if whole.get_untracked() => view! { <pre class="diff-view">{text}</pre> }.into_any(),
                                    Some(Ok(text)) => diff_view(&text).into_any(),
                                }}
                            </div>
                        }.into_any(),
                    }}
                </div>
                <div class="modal-footer">
                    <button class="btn-secondary" on:click=move |_| close()>{move || t("close", lang())}</button>
                    <button class="btn-primary" prop:disabled=move || commit().is_none() on:click=move |_| restore()>
                        {move || t("git_restore", lang())}
                    </button>
                </div>
            </div>
        </div>
    }
}
