//! Pages linking to the active page.

use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::{
    i18n::t,
    invoke,
    keybindings::after_ms,
    state::AppCtx,
};
use super::open_page;

/// Pages linking to the active page.
#[component]
pub fn BacklinksFrame() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();

    // Pages linking to the active page. Refreshed (debounced) when another
    // page becomes active or the links of the project change.
    let backlinks = RwSignal::new(Vec::<String>::new());
    let backlinks_seq = StoredValue::new(0u32);
    Effect::new(move |_| {
        ctx.project.links_version.track();
        ctx.pref(|p| p.case_insensitive_links);
        let page = ctx.work.active_tab_data().map(|t| t.name);
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

    let open_backlink = move |name: String| open_page(ctx, name, None);

    view! {
        <div class="frame frame-grow panel-section">
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
    }
}
