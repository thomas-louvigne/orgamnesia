//! Pages linking to the active page, and those naming it without a link.

use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use orgamnesia_core::MentionHit;

use crate::{
    actions,
    i18n::t,
    invoke,
    keybindings::after_ms,
    state::{AppCtx, FileEntry, Goto},
};
use super::open_page;

/// Pages linking to the active page.
#[component]
pub fn BacklinksFrame() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();

    // Pages linking to the active page. Refreshed (debounced) when another
    // page becomes active or the links of the project change.
    let backlinks = RwSignal::new(Vec::<String>::new());
    let mentions = RwSignal::new(Vec::<MentionHit>::new());
    let backlinks_seq = StoredValue::new(0u32);
    Effect::new(move |_| {
        ctx.project.links_version.track();
        ctx.pref(|p| p.case_insensitive_links);
        let page = ctx.work.active_tab_data().map(|t| t.path);
        let seq = backlinks_seq.get_value() + 1;
        backlinks_seq.set_value(seq);
        let Some(page) = page else { backlinks.set(vec![]); mentions.set(vec![]); return };
        after_ms(150, move || {
            if backlinks_seq.try_get_value() != Some(seq) { return; }
            spawn_local(async move {
                if let Ok(list) = invoke::get_backlinks(&page).await
                    && backlinks_seq.try_get_value() == Some(seq) && backlinks.try_get_untracked().is_some_and(|b| b != list)
                {
                    backlinks.set(list);
                }
                if let Ok(list) = invoke::get_unlinked_mentions(&page).await
                    && backlinks_seq.try_get_value() == Some(seq) && mentions.try_get_untracked().is_some_and(|m| m != list)
                {
                    mentions.set(list);
                }
            });
        });
    });

    let open_backlink = move |name: String| open_page(ctx, name, None);
    let open_mention = move |m: MentionHit| {
        actions::open_file(ctx, FileEntry { name: m.page, path: m.path, ..Default::default() }, Some(Goto::Range(m.start, m.end)));
    };
    // The page writing it changed (in its tab when open): the list follows the links
    let link_mention = move |m: MentionHit| {
        let syntax = ctx.tags().links(orgamnesia_core::Format::of_page(&m.path));
        let format = orgamnesia_core::Format::of_page(&m.path);
        actions::change_page(ctx, m.path.clone(), move |text| {
            orgamnesia_core::mentions::link(text, m.start, m.end, &m.text, &m.target, format, syntax)
        }, || {});
    };

    view! {
        <div class="frame frame-grow panel-section" data-frame="backlinks">
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
                {move || {
                    let ms = mentions.get();
                    (!ms.is_empty()).then(|| {
                        let lang = ctx.lang.get();
                        let head = format!("{} ({})", t("unlinked_mentions", lang), ms.len());
                        let items = ms.into_iter().map(|m| {
                            let (m1, m2) = (m.clone(), m.clone());
                            view! {
                                <div class="mention-item list-item" role="button" tabindex="0" title=m.context.clone()
                                    on:keydown=crate::a11y::list_item_keys
                                    on:click=move |_| open_mention(m1.clone())>
                                    <span class="mention-page">{m.page.clone()}</span>
                                    <span class="mention-context">{m.context.clone()}</span>
                                    <button class="mention-link" title=t("link_mention_title", lang)
                                        on:click=move |e| { e.stop_propagation(); link_mention(m2.clone()); }>
                                        {t("link_mention", lang)}
                                    </button>
                                </div>
                            }
                        }).collect_view();
                        view! { <div class="mentions"><div class="mentions-header">{head}</div>{items}</div> }
                    })
                }}
            </div>
        </div>
    }
}
