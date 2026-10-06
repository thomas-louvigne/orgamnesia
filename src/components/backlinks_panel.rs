use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::{
    components::sidebar::create_and_open_page,
    i18n::t,
    invoke,
    state::{AppCtx, BrokenLink, Drag, Goto, Tab, TagHit},
};

#[component]
pub fn BacklinksPanel() -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");


    // Links pointing to pages that don't exist. Refreshed (debounced) when the
    // project, its pages or its content change.
    let broken = RwSignal::new(Vec::<BrokenLink>::new());
    let broken_open = RwSignal::new(true);
    let refresh_seq = StoredValue::new(0u32);
    Effect::new(move |_| {
        ctx.files.track();
        ctx.links_version.track();
        ctx.case_insensitive_links.track();
        if ctx.vault_path.get().is_none() {
            broken.set(vec![]);
            return;
        }
        let seq = refresh_seq.get_value() + 1;
        refresh_seq.set_value(seq);
        if let Some(w) = web_sys::window() {
            let cb = wasm_bindgen::closure::Closure::once_into_js(move || {
                if refresh_seq.get_value() != seq { return; }
                spawn_local(async move {
                    if let Ok(list) = invoke::get_broken_links().await {
                        if broken.get_untracked() != list { broken.set(list); }
                    }
                });
            });
            let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(
                wasm_bindgen::JsCast::unchecked_ref(&cb), 400);
        }
    });

    // Org-mode tags of the project, for the tags view and the completion of `:tags:`.
    // Refreshed (debounced) like the list above.
    let tags_seq = StoredValue::new(0u32);
    Effect::new(move |_| {
        ctx.files.track();
        ctx.links_version.track();
        if ctx.vault_path.get().is_none() {
            ctx.tags.set(vec![]);
            return;
        }
        let seq = tags_seq.get_value() + 1;
        tags_seq.set_value(seq);
        crate::keybindings::after_ms(400, move || {
            if tags_seq.get_value() != seq { return; }
            spawn_local(async move {
                if let Ok(list) = invoke::list_tags().await {
                    if ctx.tags.get_untracked() != list { ctx.tags.set(list); }
                }
            });
        });
    });

    // Pages and headlines matching the tag search
    let hits = RwSignal::new(Vec::<TagHit>::new());
    let hits_seq = StoredValue::new(0u32);
    Effect::new(move |_| {
        let query = ctx.tag_query.get();
        ctx.links_version.track();
        ctx.files.track();
        let seq = hits_seq.get_value() + 1;
        hits_seq.set_value(seq);
        if query.trim().is_empty() || !ctx.show_tags.get() {
            hits.set(vec![]);
            return;
        }
        crate::keybindings::after_ms(150, move || {
            if hits_seq.get_value() != seq { return; }
            spawn_local(async move {
                if let Ok(list) = invoke::search_tags(&query).await {
                    if hits_seq.get_value() == seq { hits.set(list); }
                }
            });
        });
    });

    // Open the page called `name`; with `target`, also move the cursor there.
    let open_page = move |name: String, target: Option<Goto>| {
        let file = ctx.files.get().into_iter().find(|f| ctx.same_page(&f.name, &name));
        let Some(file) = file else { return };
        let goto = move |path: String| {
            if let Some(t) = target.clone() { ctx.goto.set(Some((path, t))); }
        };
        if let Some(idx) = ctx.tabs.get().iter().position(|t| t.path == file.path) {
            ctx.active_tab.set(Some(idx));
            goto(file.path.clone());
            return;
        }
        spawn_local(async move {
            match invoke::read_file(&file.path).await {
                Ok(content) => {
                    ctx.tabs.update(|tabs| {
                        tabs.push(Tab {
                            path: file.path.clone(),
                            name: file.name.clone(),
                            content: RwSignal::new(content),
                            dirty: RwSignal::new(false),
                        });
                    });
                    let idx = ctx.tabs.get().len() - 1;
                    ctx.active_tab.set(Some(idx));
                    goto(file.path.clone());
                    if let Ok(bl) = invoke::get_backlinks(&file.name).await {
                        ctx.backlinks.set(bl);
                    }
                }
                Err(e) => ctx.status.set(Some(format!("Error: {e}"))),
            }
        });
    };
    let open_backlink = move |name: String| open_page(name, None);

    // `+` of the tags frame: a field to add a file tag to the active page
    let adding = RwSignal::new(false);
    let new_tag = RwSignal::new(String::new());
    let add_ref = NodeRef::<leptos::html::Input>::new();
    Effect::new(move |_| {
        if adding.get() {
            crate::keybindings::after_ms(0, move || {
                if let Some(el) = add_ref.get_untracked() { let _ = el.focus(); }
            });
        }
    });
    let add_tag = move || {
        let lang = ctx.lang.get_untracked();
        let tag = new_tag.get_untracked().trim().trim_matches(':').trim_start_matches('#').to_string();
        if tag.is_empty() { adding.set(false); return; }
        if !tag.chars().all(crate::motion::is_org_tag_char) {
            ctx.status.set(Some(t("tag_invalid", lang).to_string()));
            return;
        }
        let Some(tab) = ctx.active_tab_data() else {
            ctx.status.set(Some(t("tag_add_no_page", lang).to_string()));
            return;
        };
        match crate::motion::add_filetag(&tab.content.get_untracked(), &tag) {
            Some(content) => {
                // Like typing in the page: marked unsaved, then auto-saved if enabled
                tab.dirty.set(true);
                tab.content.set(content);
                ctx.status.set(Some(format!("{} :{tag}: → {}", t("tag_added", lang), tab.name)));
            }
            None => ctx.status.set(Some(format!("{} :{tag}:", t("tag_exists", lang)))),
        }
        new_tag.set(String::new());
        adding.set(false);
    };

    // Right-click menu of a missing page, and which source a left click visits next
    let menu = RwSignal::new(None::<(i32, i32, BrokenLink)>);
    let next_source = StoredValue::new(std::collections::HashMap::<String, usize>::new());

    view! {
        <div class="backlinks-panel">
            {move || ctx.show_backlinks.get().then(|| view! {
                <div class="panel-section">
                    <div class="panel-header">{move || t("backlinks", ctx.lang.get())}</div>
                    <div class="panel-body">
                        {move || {
                            let bl = ctx.backlinks.get();
                            if bl.is_empty() {
                                let msg = t("no_backlinks", ctx.lang.get());
                                view! { <p class="no-backlinks">{msg}</p> }.into_any()
                            } else {
                                let items = bl.into_iter().map(|name| {
                                    let name2 = name.clone();
                                    view! {
                                        <div class="backlink-item" on:click=move |_| open_backlink(name2.clone())>
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
            {move || ctx.show_tags.get().then(|| view! {
                <div class="panel-section">
                    <div class="panel-header panel-header-row">
                        <span>{move || t("tags", ctx.lang.get())}</span>
                        <button
                            class="panel-add"
                            disabled=move || ctx.active_tab_data().is_none()
                            title=move || if ctx.active_tab_data().is_some() {
                                t("tag_add_title", ctx.lang.get())
                            } else {
                                t("tag_add_no_page", ctx.lang.get())
                            }
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
                                    ctx.active_tab_data().map(|t| t.name).unwrap_or_default())
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
                                {move || ctx.tags.get().into_iter()
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
                            prop:value=move || ctx.tag_query.get()
                            on:input=move |e| ctx.tag_query.set(event_target_value(&e))
                            on:keydown=move |e: web_sys::KeyboardEvent| {
                                if e.key() == "Escape" { ctx.tag_query.set(String::new()); }
                            }
                        />
                    </div>
                    <div class="panel-body">
                        {move || {
                            let lang = ctx.lang.get();
                            if ctx.tag_query.get().trim().is_empty() {
                                // No search: every tag of the project
                                let tags = ctx.tags.get();
                                if tags.is_empty() {
                                    return view! { <p class="no-backlinks">{t("no_tags", lang)}</p> }.into_any();
                                }
                                let items = tags.into_iter().map(|tag| {
                                    let name = tag.name.clone();
                                    view! {
                                        <div class="backlink-item tag-item" on:click=move |_| ctx.show_tag(name.clone())>
                                            <span class="tags">{format!(":{}:", tag.name)}</span>
                                            <span class="tag-count">{tag.count}</span>
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
                                    <div class="backlink-item tag-hit" title=tip
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
            {move || (ctx.show_broken_links.get() && (ctx.show_backlinks.get() || ctx.show_tags.get()))
                .then(|| view! {
                    <div class="resizer-h" on:mousedown=move |e: web_sys::MouseEvent| {
                        e.prevent_default();
                        ctx.drag.set(Some(Drag::Broken));
                    } />
                })}
            {move || ctx.show_broken_links.get().then(|| view! {
                <div class="broken-links">
                    <button class="broken-links-header" on:click=move |_| broken_open.update(|v| *v = !*v)>
                        <span>{move || format!("{} ({})", t("broken_links", ctx.lang.get()), broken.get().len())}</span>
                        <span class="project-caret">{move || if broken_open.get() { "▾" } else { "▸" }}</span>
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
                                                class="broken-link-item"
                                                title=tip
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
                            create_and_open_page(ctx, create_target.clone());
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
        </div>
    }
}
