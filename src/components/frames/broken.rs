//! Links to pages not created.

use leptos::prelude::*;
use orgamnesia_core::Frame;
use wasm_bindgen_futures::spawn_local;

use crate::{
    actions,
    i18n::t,
    invoke,
    keybindings::after_ms,
    state::{AppCtx, BrokenLink, Drag, Goto},
};
use super::open_page;

/// Pages linked to but not created. `right`: the column it is in.
#[component]
pub fn BrokenFrame(right: bool) -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");

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

    // Right-click menu of a missing page, and which source a left click visits next
    let menu = RwSignal::new(None::<(i32, i32, BrokenLink)>);
    let next_source = StoredValue::new(std::collections::HashMap::<String, usize>::new());

    let open_page = move |name: String, target: Option<Goto>| open_page(ctx, name, target);

    // With another list in its column, it keeps the height set with its bar
    // (on the side of that list); alone, it fills the column
    let column = Memo::new(move |_| ctx.pref(|p| p.column(right)));
    let others = move |before: bool| column.with(|c| {
        let at = c.iter().position(|f| *f == Frame::Broken).unwrap_or(0);
        let side = if before { &c[..at] } else { &c[at + 1..] };
        side.iter().any(|f| f.grows())
    });
    let above = move || others(true);
    let fill = move || !others(true) && !others(false);
    let bar = move |at_top: bool| move || (!fill() && above() == at_top).then(|| view! {
        <div class="resizer-h" on:mousedown=move |e: web_sys::MouseEvent| {
            e.prevent_default();
            ctx.ui.drag.set(Some(Drag::Broken { above: at_top }));
        } />
    });

    view! {
        {bar(true)}
        <div class=move || if fill() { "frame broken-links fill" } else { "frame broken-links" }>
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
        {bar(false)}
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
    }
}
