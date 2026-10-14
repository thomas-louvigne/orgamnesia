//! The tags of the project, and the search of the pages and headlines by tag.

use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::{
    i18n::t,
    invoke,
    state::{AppCtx, Goto, TagHit},
};
use super::open_page;

/// Add the file tag `tag` (`#+FILETAGS:`) to the page being edited.
fn tag_active_page(ctx: AppCtx, tag: &str) {
    let lang = ctx.lang.get_untracked();
    let Some(tab) = ctx.work.active_tab_data() else {
        ctx.ui.status.set(Some(t("tag_add_no_page", lang).to_string()));
        return;
    };
    match crate::motion::add_filetag(&tab.content.get_untracked(), tag) {
        Some(content) => {
            // Like typing in the page: marked unsaved, then auto-saved if enabled
            tab.dirty.set(true);
            tab.content.set(content);
            ctx.ui.status.set(Some(format!("{} :{tag}: → {}", t("tag_added", lang), tab.name)));
        }
        None => ctx.ui.status.set(Some(format!("{} :{tag}:", t("tag_exists", lang)))),
    }
}

/// A field to add a file tag to the page being edited, proposing the tags of the
/// project. `on_close` runs once a tag was added, or on Escape or leaving the field.
#[component]
pub fn TagAddField(list_id: &'static str, on_close: Callback<()>) -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");
    let new_tag = RwSignal::new(String::new());
    let add_ref = NodeRef::<leptos::html::Input>::new();
    Effect::new(move |_| {
        if add_ref.get().is_some() {
            crate::keybindings::after_ms(0, move || {
                if let Some(el) = add_ref.try_get_untracked().flatten() { let _ = el.focus(); }
            });
        }
    });
    let add_tag = move || {
        let tag = new_tag.get_untracked().trim().trim_matches(':').trim_start_matches('#').to_string();
        if tag.is_empty() { on_close.run(()); return; }
        let org = ctx.tags_untracked().org;
        if !tag.chars().all(|c| org.is_tag_char(c)) {
            ctx.ui.status.set(Some(t("tag_invalid", ctx.lang.get_untracked()).to_string()));
            return;
        }
        tag_active_page(ctx, &tag);
        new_tag.set(String::new());
        on_close.run(());
    };
    view! {
        <input
            node_ref=add_ref
            class="search-input"
            type="text"
            list=list_id
            placeholder=move || format!("{} « {} »",
                t("tag_add_ph", ctx.lang.get()),
                ctx.work.active_tab_data().map(|t| t.name).unwrap_or_default())
            prop:value=move || new_tag.get()
            on:input=move |e| new_tag.set(event_target_value(&e))
            on:keydown=move |e: web_sys::KeyboardEvent| match e.key().as_str() {
                "Enter" => { e.stop_propagation(); add_tag(); }
                "Escape" => { e.stop_propagation(); new_tag.set(String::new()); on_close.run(()); }
                _ => {}
            }
            on:blur=move |_| on_close.run(())
        />
        <datalist id=list_id>
            {move || ctx.project.tags.get().into_iter()
                .map(|tag| view! { <option value=tag.name /> })
                .collect_view()}
        </datalist>
    }
}

/// The field to add a tag, over the focused pane at the height of the page
/// title: for the shortcut, whether the tags frame is shown or not.
#[component]
pub fn TagAddPopin() -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");
    let close = Callback::new(move |_| {
        ctx.ui.tag_add.set(false);
        crate::components::project_search::focus_editor();
    });
    view! {
        <div class="title-popin tag-popin" role="dialog" aria-label=move || t("shortcut_add_tag", ctx.lang.get())>
            <TagAddField list_id="tag-popin-names" on_close=close />
        </div>
    }
}

/// The tags of the project, and the search of the pages and headlines by tag.
#[component]
pub fn TagsFrame() -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");

    // Pages and headlines matching the tag search
    let hits = RwSignal::new(Vec::<TagHit>::new());
    let hits_seq = StoredValue::new(0u32);
    Effect::new(move |_| {
        let query = ctx.project.tag_query.get();
        ctx.project.links_version.track();
        ctx.project.files.track();
        let seq = hits_seq.get_value() + 1;
        hits_seq.set_value(seq);
        if query.trim().is_empty() {
            hits.set(vec![]);
            return;
        }
        crate::keybindings::after_ms(150, move || {
            if hits_seq.try_get_value() != Some(seq) { return; }
            spawn_local(async move {
                if let Ok(list) = invoke::search_tags(&query).await
                    && hits_seq.try_get_value() == Some(seq) { hits.set(list); }
            });
        });
    });

    // `+` of the tags frame: a field to add a file tag to the active page
    let adding = RwSignal::new(false);

    // Completion of the tag search: the tags starting with (or holding) the word
    // being typed, the last one of the query (after `+`, `-`, `|` or a space; when tags
    // may hold dashes, a `-` only counts at the start of a word)
    let suggest_open = RwSignal::new(false);
    let suggest_sel = RwSignal::new(0usize);
    let last_word = move |q: &str| -> usize {
        if !ctx.tags_untracked().org.dashes { return q.rfind(['+', '-', '|', ' ']).map_or(0, |i| i + 1); }
        let at = q.rfind(['+', '|', ' ']).map_or(0, |i| i + 1);
        if q[at..].starts_with('-') { at + 1 } else { at }
    };
    let suggestions = move || -> Vec<String> {
        if !suggest_open.get() { return vec![]; }
        let q = ctx.project.tag_query.get();
        let word = q[last_word(&q)..].trim_matches(':').to_string();
        if word.is_empty() { return vec![]; }
        let names: Vec<String> = ctx.project.tags.with(|tags| tags.iter().map(|t| t.name.clone()).collect());
        crate::motion::complete_page(&names, &word, None, 8)
    };
    let accept_suggestion = move |tag: String| {
        ctx.project.tag_query.update(|q| {
            let at = last_word(q);
            q.truncate(at);
            q.push_str(&tag);
        });
        suggest_open.set(false);
    };

    let open_page = move |name: String, target: Option<Goto>| open_page(ctx, name, target);

    view! {
        <div class="frame frame-grow panel-section">
            <div class="panel-header panel-header-row">
                <span>{move || t("tags", ctx.lang.get())}</span>
                <button
                    class="panel-add"
                    disabled=move || ctx.work.active_tab_data().is_none()
                    title=move || if ctx.work.active_tab_data().is_some() {
                        t("tag_add_title", ctx.lang.get())
                    } else {
                        t("tag_add_no_page", ctx.lang.get())
                    }
                    aria-label=move || t("tag_add_title", ctx.lang.get())
                    on:click=move |_| adding.update(|a| *a = !*a)
                >"+"</button>
            </div>
            {move || adding.get().then(|| view! {
                <div class="tag-search tag-add">
                    <TagAddField list_id="tag-add-names" on_close=Callback::new(move |_| adding.set(false)) />
                </div>
            })}
            <div class="tag-search">
                <input
                    class="search-input"
                    type="text"
                    placeholder=move || t("tag_search_ph", ctx.lang.get())
                    title=move || t("tag_search_help", ctx.lang.get())
                    prop:value=move || ctx.project.tag_query.get()
                    on:input=move |e| {
                        ctx.project.tag_query.set(event_target_value(&e));
                        suggest_open.set(true);
                        suggest_sel.set(0);
                    }
                    on:keydown=move |e: web_sys::KeyboardEvent| {
                        let list = suggestions();
                        let n = list.len();
                        let open = n > 0;
                        match e.key().as_str() {
                            "ArrowDown" if open => suggest_sel.update(|s| *s = (*s + 1) % n),
                            "ArrowUp" if open => suggest_sel.update(|s| *s = (*s + n - 1) % n),
                            "Enter" | "Tab" if open => {
                                accept_suggestion(list[suggest_sel.get_untracked().min(n - 1)].clone());
                            }
                            "Escape" if open => suggest_open.set(false),
                            "Escape" => ctx.project.tag_query.set(String::new()),
                            _ => return,
                        }
                        e.prevent_default();
                        e.stop_propagation();
                    }
                    on:blur=move |_| suggest_open.set(false)
                />
                {move || {
                    let list = suggestions();
                    (!list.is_empty()).then(|| view! {
                        <ul class="tag-suggest">
                            {list.into_iter().enumerate().map(|(i, tag)| {
                                let pick = tag.clone();
                                view! {
                                    <li
                                        class=move || if suggest_sel.get() == i { "selected" } else { "" }
                                        // mousedown + preventDefault keeps the focus in the field
                                        on:mousedown=move |e: web_sys::MouseEvent| {
                                            e.prevent_default();
                                            accept_suggestion(pick.clone());
                                        }
                                    >{format!(":{tag}:")}</li>
                                }
                            }).collect_view()}
                        </ul>
                    })
                }}
            </div>
            <div class="panel-body">
                {move || {
                    let lang = ctx.lang.get();
                    if ctx.project.tag_query.get().trim().is_empty() {
                        // No search: every tag of the project
                        let tags = ctx.project.tags.get();
                        if tags.is_empty() {
                            return view! { <p class="no-backlinks">{t("no_tags", lang)}</p> }.into_any();
                        }
                        let items = tags.into_iter().map(|tag| {
                            let name = tag.name.clone();
                            let to_add = tag.name.clone();
                            view! {
                                <div class="backlink-item tag-item list-item" role="button" tabindex="0"
                                    on:keydown=crate::a11y::list_item_keys
                                    on:click=move |_| ctx.show_tag(name.clone())>
                                    <span class="tags">{format!(":{}:", tag.name)}</span>
                                    <span class="tag-item-end">
                                        <span class="tag-count">{tag.count}</span>
                                        <button
                                            class="btn-icon tag-add-btn"
                                            title=t("tag_add_to_page", lang)
                                            aria-label=format!("{} :{}:", t("tag_add_to_page", lang), tag.name)
                                            prop:disabled=move || ctx.work.active_tab.get().is_none()
                                            on:click=move |e: web_sys::MouseEvent| {
                                                e.stop_propagation();
                                                tag_active_page(ctx, &to_add);
                                            }
                                        >"+"</button>
                                    </span>
                                </div>
                            }
                        }).collect_view();
                        return view! { <div>{items}</div> }.into_any();
                    }
                    let list = hits.get();
                    if list.is_empty() {
                        return view! { <p class="no-backlinks">{t("no_tag_hits", lang)}</p> }.into_any();
                    }
                    let items = list.into_iter().map(|h| {
                        let (page, line) = (h.page.clone(), h.line);
                        let tip = format!(":{}:", h.tags.join(":"));
                        view! {
                            <div class="backlink-item tag-hit list-item" title=tip role="button" tabindex="0"
                                on:keydown=crate::a11y::list_item_keys
                                on:click=move |_| open_page(page.clone(), Some(Goto::Line(line)))
                            >
                                <div class="tag-hit-page">{h.page.clone()}</div>
                                {h.heading.clone().map(|heading| view! {
                                    <div class="tag-hit-heading">{format!("{} {}", "*".repeat(h.level), heading)}</div>
                                })}
                            </div>
                        }
                    }).collect_view();
                    view! { <div>{items}</div> }.into_any()
                }}
            </div>
        </div>
    }
}
