//! The TODO headlines of the project.

use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::{
    i18n::t,
    invoke,
    keybindings::after_ms,
    state::{AppCtx, Goto, TodoHit},
};
use super::open_page;

/// Headlines of the project with a TODO keyword.
#[component]
pub fn TodosFrame() -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");

    // Headlines of the project with a TODO keyword. Refreshed (debounced) like the tags.
    let todos = RwSignal::new(Vec::<TodoHit>::new());
    let todos_seq = StoredValue::new(0u32);
    Effect::new(move |_| {
        ctx.project.files.track();
        ctx.project.links_version.track();
        ctx.pref(|p| p.todo_keywords.clone());
        if ctx.project.vault_path.get().is_none() {
            todos.set(vec![]);
            return;
        }
        let seq = todos_seq.get_value() + 1;
        todos_seq.set_value(seq);
        after_ms(400, move || {
            if todos_seq.try_get_value() != Some(seq) { return; }
            spawn_local(async move {
                if let Ok(list) = invoke::list_todos().await
                    && todos.try_get_untracked().is_some_and(|t| t != list) { todos.set(list); }
            });
        });
    });
    // Which ones are shown: "open" (not done), "done", "all", or one keyword
    let todo_filter = RwSignal::new("open".to_string());
    let todo_keywords = move || -> Vec<String> {
        let mut seen: Vec<String> = Vec::new();
        todos.with(|list| for h in list {
            if !seen.contains(&h.keyword) { seen.push(h.keyword.clone()); }
        });
        seen
    };

    let open_page = move |name: String, target: Option<Goto>| open_page(ctx, name, target);

    view! {
        <div class="frame frame-grow panel-section">
            <div class="panel-header panel-header-row">
                <span>{move || t("todo_panel", ctx.lang.get())}</span>
                <select
                    class="todo-filter"
                    aria-label=move || t("todo_filter", ctx.lang.get())
                    prop:value=move || todo_filter.get()
                    on:change=move |e| todo_filter.set(event_target_value(&e))
                >
                    <option value="open">{move || t("todo_filter_open", ctx.lang.get())}</option>
                    <option value="done">{move || t("todo_filter_done", ctx.lang.get())}</option>
                    <option value="all">{move || t("todo_filter_all", ctx.lang.get())}</option>
                    {move || todo_keywords().into_iter().map(|k| view! {
                        <option value=k.clone() selected=move || todo_filter.get() == k>{k.clone()}</option>
                    }).collect_view()}
                </select>
            </div>
            <div class="panel-body">
                {move || {
                    let filter = todo_filter.get();
                    let list: Vec<TodoHit> = todos.get().into_iter().filter(|h| match filter.as_str() {
                        "open" => !h.done,
                        "done" => h.done,
                        "all" => true,
                        k => h.keyword == k,
                    }).collect();
                    if list.is_empty() {
                        return view! { <p class="no-backlinks">{t("no_todos", ctx.lang.get())}</p> }.into_any();
                    }
                    // By page, in the order of the pages
                    let mut last_page = String::new();
                    list.into_iter().map(|h| {
                        let page_title = (h.page != last_page).then(|| {
                            last_page = h.page.clone();
                            view! { <div class="todo-page">{h.page.clone()}</div> }
                        });
                        let (page, line) = (h.page.clone(), h.line);
                        let class = if h.done { "backlink-item todo-item list-item is-done" } else { "backlink-item todo-item list-item" };
                        let tip = format!("{} — {} {}", h.page, "*".repeat(h.level), h.title);
                        view! {
                            {page_title}
                            <div class=class title=tip role="button" tabindex="0"
                                on:keydown=crate::a11y::list_item_keys
                                on:click=move |_| open_page(page.clone(), Some(Goto::Line(line)))
                            >
                                <span class=if h.done { "done" } else { "todo" }>{h.keyword.clone()}</span>
                                <span class="todo-item-title">{h.title.clone()}</span>
                            </div>
                        }
                    }).collect_view().into_any()
                }}
            </div>
        </div>
    }
}
