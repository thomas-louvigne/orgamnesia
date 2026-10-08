use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::{
    actions,
    i18n::t,
    invoke,
    keybindings::after_ms,
    state::{AppCtx, BrokenLink, Drag, Goto, TagHit},
};

#[component]
pub fn BacklinksPanel() -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");

    // Pages linking to the active page. Refreshed (debounced) when another
    // page becomes active or the links of the project change.
    let backlinks = RwSignal::new(Vec::<String>::new());
    let backlinks_seq = StoredValue::new(0u32);
    Effect::new(move |_| {
        ctx.project.links_version.track();
        ctx.pref(|p| p.case_insensitive_links);
        let page = ctx.work.active_tab_data().map(|t| t.name).filter(|_| ctx.pref(|p| p.show_backlinks));
        let seq = backlinks_seq.get_value() + 1;
        backlinks_seq.set_value(seq);
        let Some(page) = page else { backlinks.set(vec![]); return };
        after_ms(150, move || {
            if backlinks_seq.try_get_value() != Some(seq) { return; }
            spawn_local(async move {
                if let Ok(list) = invoke::get_backlinks(&page).await
                    && backlinks_seq.try_get_value() == Some(seq) && backlinks.try_get_untracked().is_some_and(|b| b != list)
                {
                    backlinks.set(list);
                }
            });
        });
    });

    // Links pointing to pages that don't exist. Refreshed (debounced) when the
    // project, its pages or its content change.
    let broken = RwSignal::new(Vec::<BrokenLink>::new());
    let broken_open = RwSignal::new(true);
    let refresh_seq = StoredValue::new(0u32);
    Effect::new(move |_| {
        ctx.project.files.track();
        ctx.project.links_version.track();
        ctx.pref(|p| p.case_insensitive_links);
        if ctx.project.vault_path.get().is_none() {
            broken.set(vec![]);
            return;
        }
        let seq = refresh_seq.get_value() + 1;
        refresh_seq.set_value(seq);
        after_ms(400, move || {
            if refresh_seq.try_get_value() != Some(seq) { return; }
            spawn_local(async move {
                if let Ok(list) = invoke::get_broken_links().await
                    && broken.try_get_untracked().is_some_and(|b| b != list) { broken.set(list); }
            });
        });
    });

    // Org-mode tags of the project, for the tags view and the completion of `:tags:`.
    // Refreshed (debounced) like the list above.
    let tags_seq = StoredValue::new(0u32);
    Effect::new(move |_| {
        ctx.project.files.track();
        ctx.project.links_version.track();
        if ctx.project.vault_path.get().is_none() {
            ctx.project.tags.set(vec![]);
            return;
        }
        let seq = tags_seq.get_value() + 1;
        tags_seq.set_value(seq);
        crate::keybindings::after_ms(400, move || {
            if tags_seq.try_get_value() != Some(seq) { return; }
            spawn_local(async move {
                if let Ok(list) = invoke::list_tags().await
                    && ctx.project.tags.get_untracked() != list { ctx.project.tags.set(list); }
            });
        });
    });

    // Pages and headlines matching the tag search
    let hits = RwSignal::new(Vec::<TagHit>::new());
    let hits_seq = StoredValue::new(0u32);
    Effect::new(move |_| {
        let query = ctx.project.tag_query.get();
        ctx.project.links_version.track();
        ctx.project.files.track();
        let seq = hits_seq.get_value() + 1;
        hits_seq.set_value(seq);
        if query.trim().is_empty() || !ctx.pref(|p| p.show_tags) {
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

    // Open the page called `name`; with `target`, also move the cursor there.
    let open_page = move |name: String, target: Option<Goto>| actions::open_page_named(ctx, &name, target);
    let open_backlink = move |name: String| open_page(name, None);

    // `+` of the tags frame: a field to add a file tag to the active page
    let adding = RwSignal::new(false);
    let new_tag = RwSignal::new(String::new());
    let add_ref = NodeRef::<leptos::html::Input>::new();
    Effect::new(move |_| {
        if adding.get() {
            crate::keybindings::after_ms(0, move || {
                if let Some(el) = add_ref.try_get_untracked().flatten() { let _ = el.focus(); }
            });
        }
    });
    // Add the file tag `tag` to the page being edited
    let tag_active_page = move |tag: &str| {
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
    };
    let add_tag = move || {
        let tag = new_tag.get_untracked().trim().trim_matches(':').trim_start_matches('#').to_string();
        if tag.is_empty() { adding.set(false); return; }
        let org = ctx.tags_untracked().org;
        if !tag.chars().all(|c| org.is_tag_char(c)) {
            ctx.ui.status.set(Some(t("tag_invalid", ctx.lang.get_untracked()).to_string()));
            return;
        }
        tag_active_page(&tag);
        new_tag.set(String::new());
        adding.set(false);
    };

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

    // Right-click menu of a missing page, and which source a left click visits next
    let menu = RwSignal::new(None::<(i32, i32, BrokenLink)>);
    let next_source = StoredValue::new(std::collections::HashMap::<String, usize>::new());

    view! {
        <aside class="backlinks-panel" aria-label=move || t("side_panel", ctx.lang.get())>
            {move || ctx.pref(|p| p.show_backlinks).then(|| view! {
                <div class="panel-section">
                    <div class="panel-header">{move || t("backlinks", ctx.lang.get())}</div>
                    <div class="panel-body">
                        {move || {
                            let bl = backlinks.get();
                            if bl.is_empty() {
                                let msg = t("no_backlinks", ctx.lang.get());
                                view! { <p class="no-backlinks">{msg}</p> }.into_any()
                            } else {
                                let items = bl.into_iter().map(|name| {
                                    let name2 = name.clone();
                                    view! {
                                        <div class="backlink-item list-item" role="button" tabindex="0"
                                            on:keydown=crate::a11y::list_item_keys
                                            on:click=move |_| open_backlink(name2.clone())>
                                            {name.clone()}
                                        </div>
                                    }
                                }).collect_view();
                                view! { <div>{items}</div> }.into_any()
                            }
                        }}
                    </div>
                </div>
            })}
            {move || ctx.pref(|p| p.show_tags).then(|| view! {
                <div class="panel-section">
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
                            <input
                                node_ref=add_ref
                                class="search-input"
                                type="text"
                                list="tag-add-names"
                                placeholder=move || format!("{} « {} »",
                                    t("tag_add_ph", ctx.lang.get()),
                                    ctx.work.active_tab_data().map(|t| t.name).unwrap_or_default())
                                prop:value=move || new_tag.get()
                                on:input=move |e| new_tag.set(event_target_value(&e))
                                on:keydown=move |e: web_sys::KeyboardEvent| match e.key().as_str() {
                                    "Enter" => { e.stop_propagation(); add_tag(); }
                                    "Escape" => { e.stop_propagation(); new_tag.set(String::new()); adding.set(false); }
                                    _ => {}
                                }
                                on:blur=move |_| adding.set(false)
                            />
                            <datalist id="tag-add-names">
                                {move || ctx.project.tags.get().into_iter()
                                    .map(|tag| view! { <option value=tag.name /> })
                                    .collect_view()}
                            </datalist>
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
                                                        tag_active_page(&to_add);
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
            })}
            // The bar resizes the block below; alone in the panel, the block fills it
            {move || (ctx.pref(|p| p.show_broken_links) && (ctx.pref(|p| p.show_backlinks) || ctx.pref(|p| p.show_tags)))
                .then(|| view! {
                    <div class="resizer-h" on:mousedown=move |e: web_sys::MouseEvent| {
                        e.prevent_default();
                        ctx.ui.drag.set(Some(Drag::Broken));
                    } />
                })}
            {move || ctx.pref(|p| p.show_broken_links).then(|| view! {
                <div class="broken-links">
                    <button class="broken-links-header" aria-expanded=move || broken_open.get().to_string()
                        on:click=move |_| broken_open.update(|v| *v = !*v)>
                        <span>{move || format!("{} ({})", t("broken_links", ctx.lang.get()), broken.get().len())}</span>
                        <span class="project-caret" aria-hidden="true">{move || if broken_open.get() { "▾" } else { "▸" }}</span>
                    </button>
                    {move || broken_open.get().then(|| view! {
                        <div class="broken-links-list">
                            {move || {
                                let list = broken.get();
                                if list.is_empty() {
                                    view! { <div class="broken-links-empty">{t("no_broken_links", ctx.lang.get())}</div> }.into_any()
                                } else {
                                    list.into_iter().map(|b| {
                                        let tip = format!("{} {}", t("used_in", ctx.lang.get()), b.sources.join(", "));
                                        let b_click = b.clone();
                                        let b_menu = b.clone();
                                        view! {
                                            <div
                                                class="broken-link-item list-item"
                                                title=tip
                                                role="button"
                                                tabindex="0"
                                                on:keydown=crate::a11y::list_item_keys
                                                on:click=move |_| {
                                                    // Visit the pages using this link, one after the other
                                                    let n = b_click.sources.len();
                                                    if n == 0 { return; }
                                                    let i = next_source.with_value(|m| m.get(&b_click.target).copied().unwrap_or(0)) % n;
                                                    next_source.update_value(|m| { m.insert(b_click.target.clone(), i + 1); });
                                                    open_page(b_click.sources[i].clone(), Some(Goto::Link(b_click.target.clone())));
                                                }
                                                on:contextmenu=move |e: web_sys::MouseEvent| {
                                                    e.prevent_default();
                                                    menu.set(Some((e.client_x(), e.client_y(), b_menu.clone())));
                                                }
                                            >{format!("{} ({})", b.target, b.count)}</div>
                                        }
                                    }).collect_view().into_any()
                                }
                            }}
                        </div>
                    })}
                </div>
            })}
            {move || menu.get().map(|(x, y, b)| {
                let create_target = b.target.clone();
                view! {
                    <div class="ctx-overlay"
                        on:click=move |_| menu.set(None)
                        on:contextmenu=move |e: web_sys::MouseEvent| { e.prevent_default(); menu.set(None); }
                    />
                    <div class="ctx-menu" style=format!("left:{x}px;top:{y}px")>
                        <button class="ctx-menu-item" on:click=move |_| {
                            menu.set(None);
                            actions::create_and_open_page(ctx, create_target.clone());
                        }>{move || t("create_page_action", ctx.lang.get())}</button>
                        {b.sources.iter().map(|src| {
                            let src_name = src.clone();
                            let target = b.target.clone();
                            let label = format!("{} « {} »", t("goto_link", ctx.lang.get()), src);
                            view! {
                                <button class="ctx-menu-item" on:click=move |_| {
                                    menu.set(None);
                                    open_page(src_name.clone(), Some(Goto::Link(target.clone())));
                                }>{label}</button>
                            }
                        }).collect_view()}
                    </div>
                }
            })}
        </aside>
    }
}
