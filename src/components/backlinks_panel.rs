use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::{
    components::sidebar::create_and_open_page,
    i18n::t,
    invoke,
    state::{AppCtx, BrokenLink, Drag, Tab},
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

    // Open the page called `name`; with `link`, also select that link in it.
    let open_page = move |name: String, link: Option<String>| {
        let file = ctx.files.get().into_iter().find(|f| ctx.same_page(&f.name, &name));
        let Some(file) = file else { return };
        let goto = move |path: String| {
            if let Some(l) = link.clone() { ctx.goto.set(Some((path, l))); }
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

    // Right-click menu of a missing page, and which source a left click visits next
    let menu = RwSignal::new(None::<(i32, i32, BrokenLink)>);
    let next_source = StoredValue::new(std::collections::HashMap::<String, usize>::new());

    view! {
        <div class="backlinks-panel">
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
            <div class="resizer-h" on:mousedown=move |e: web_sys::MouseEvent| {
                e.prevent_default();
                ctx.drag.set(Some(Drag::Broken));
            } />
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
                                                open_page(b_click.sources[i].clone(), Some(b_click.target.clone()));
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
                                    open_page(src_name.clone(), Some(target.clone()));
                                }>{label}</button>
                            }
                        }).collect_view()}
                    </div>
                }
            })}
        </div>
    }
}
