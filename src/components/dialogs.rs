//! The dialogs over the whole window: a confirmation, and a page changed on
//! disk while it was being edited.

use leptos::{ev, prelude::*};
use wasm_bindgen_futures::spawn_local;

use crate::{actions, i18n::t, state::AppCtx};

/// In-app yes/no dialog (or message with OK) driven by `ctx.ui.confirm`.
#[component]
pub fn ConfirmDialog() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();
    let close = move || ctx.ui.confirm.set(None);
    let accept = move || {
        if let Some(req) = ctx.ui.confirm.get_untracked() {
            ctx.ui.confirm.set(None);
            req.on_yes.run(());
        }
    };
    // A destructive action is not done by Enter: Cancel has the focus, Enter presses it
    let danger = ctx.ui.confirm.get_untracked().and_then(|r| r.danger);
    let info = ctx.ui.confirm.get_untracked().is_some_and(|r| r.info);
    // A choice: the first button, in place of Cancel, and the label of the second
    let choice = ctx.ui.confirm.get_untracked().and_then(|r| r.choice);
    let no = move || {
        close();
        if let Some((_, on_no, _)) = choice { on_no.run(()); }
    };
    let keys = window_event_listener(ev::keydown, move |e: web_sys::KeyboardEvent| {
        match e.key().as_str() {
            "Escape" => { e.stop_propagation(); close(); }
            "Enter" if danger.is_none() => { e.stop_propagation(); accept(); }
            _ => {}
        }
    });
    on_cleanup(move || keys.remove());
    let cancel_ref = NodeRef::<leptos::html::Button>::new();
    Effect::new(move |_| {
        if danger.is_some() && let Some(b) = cancel_ref.get() { let _ = b.focus(); }
    });

    view! {
        <div class="alert-overlay" on:click=move |_| close()>
            <div class="alert-box confirm" role="alertdialog" on:click=|e| e.stop_propagation()>
                <p>{move || ctx.ui.confirm.get().map(|r| r.message).unwrap_or_default()}</p>
                <div class="alert-buttons">
                    {(!info).then(|| view! {
                        <button class="btn-secondary" node_ref=cancel_ref on:click=move |_| no()>
                            {move || t(choice.map_or("cancel", |(label, _, _)| label), lang())}
                        </button>
                    })}
                    <button class=if danger.is_some() { "btn-danger" } else { "btn-primary" } on:click=move |_| accept()>
                        {move || t(if info { "ok" } else { choice.map(|(_, _, yes)| yes).or(danger).unwrap_or("confirm") }, lang())}
                    </button>
                </div>
            </div>
        </div>
    }
}

/// A value asked (`ctx.ui.prompt`): a field, the choices holding what is typed
/// under it (a click or Tab takes the first), Enter gives it, Escape gives up.
#[component]
pub fn PromptDialog() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();
    let Some(req) = ctx.ui.prompt.get_untracked() else { return ().into_any() };
    let value = RwSignal::new(req.value.clone());
    let close = move || ctx.ui.prompt.set(None);
    let ok = move || {
        let v = value.get_untracked().trim().to_string();
        let Some(req) = ctx.ui.prompt.get_untracked() else { return };
        close();
        if !v.is_empty() { req.on_ok.run(v); }
    };
    let all = req.choices.clone();
    let found = Memo::new(move |_| {
        let q = orgamnesia_core::names::fold(&value.get());
        let mut list: Vec<String> = all.iter().filter(|c| orgamnesia_core::names::fold(c).contains(&q)).cloned().collect();
        list.sort_by_cached_key(|c| orgamnesia_core::names::search_order(c, &q));
        list.truncate(8);
        list
    });
    let input = NodeRef::<leptos::html::Input>::new();
    Effect::new(move |_| if let Some(el) = input.get() { let _ = el.focus(); });
    view! {
        <div class="alert-overlay" on:click=move |_| close()>
            <div class="alert-box confirm prompt-box" role="dialog" on:click=|e| e.stop_propagation()>
                <label for="prompt-input">{req.label.clone()}</label>
                <input
                    id="prompt-input"
                    node_ref=input
                    class="setting-input"
                    type="text"
                    spellcheck="false"
                    prop:value=move || value.get()
                    on:input=move |e| value.set(event_target_value(&e))
                    on:keydown=move |e: web_sys::KeyboardEvent| {
                        e.stop_propagation();
                        match e.key().as_str() {
                            "Enter" => { e.prevent_default(); ok(); }
                            "Escape" => close(),
                            "Tab" => if let Some(first) = found.get_untracked().into_iter().next() { e.prevent_default(); value.set(first); },
                            _ => {}
                        }
                    }
                />
                <div class="prompt-choices">
                    {move || found.get().into_iter().map(|c| {
                        let shown = if c.is_empty() { t("folder_top", lang()).to_string() } else { c.clone() };
                        view! { <button class="ctx-menu-item" on:click=move |_| { value.set(c.clone()); if let Some(el) = input.get_untracked() { let _ = el.focus(); } }>{shown}</button> }
                    }).collect_view()}
                </div>
                <div class="alert-buttons">
                    <button class="btn-secondary" on:click=move |_| close()>{move || t("cancel", lang())}</button>
                    <button class="btn-primary" on:click=move |_| ok()>{move || t("ok", lang())}</button>
                </div>
            </div>
        </div>
    }.into_any()
}

/// A page changed on disk while it has unsaved changes: keep the text being
/// edited, or take the one on disk. Escape keeps the text being edited.
#[component]
pub fn DiskConflictDialog() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();
    let settle = move |take_disk: bool| spawn_local(actions::settle_disk_conflict(ctx, take_disk));
    let keys = window_event_listener(ev::keydown, move |e: web_sys::KeyboardEvent| {
        if e.key() == "Escape" {
            e.stop_propagation();
            settle(false);
        }
    });
    on_cleanup(move || keys.remove());
    // The keyboard leaves the editor: typing on would change the text being decided on
    let keep_ref = NodeRef::<leptos::html::Button>::new();
    Effect::new(move |_| if let Some(b) = keep_ref.get() { let _ = b.focus(); });
    let name = move || {
        let path = ctx.ui.disk_conflicts.with(|q| q.first().cloned()).unwrap_or_default();
        ctx.work.tabs.with(|tabs| tabs.iter().find(|t| t.path == path).map(|t| t.name.clone())).unwrap_or_default()
    };
    view! {
        <div class="alert-overlay">
            <div class="alert-box confirm" role="alertdialog">
                <h3 class="disk-conflict-title">{move || t("disk_conflict_title", lang())}</h3>
                <p>{move || format!("« {} » {}", name(), t("disk_conflict", lang()))}</p>
                <div class="alert-buttons">
                    <button class="btn-secondary" node_ref=keep_ref on:click=move |_| settle(false)>{move || t("disk_keep_mine", lang())}</button>
                    <button class="btn-primary" on:click=move |_| settle(true)>{move || t("disk_take", lang())}</button>
                </div>
            </div>
        </div>
    }
}
