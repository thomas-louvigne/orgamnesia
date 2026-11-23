//! "Did you know?": one tip at a time on a feature, with its shortcut in the
//! active profile, and a button for the next one.

use leptos::prelude::*;

use crate::{i18n::{t, Lang}, state::AppCtx};

/// The tips that make sense now: the action has a shortcut, the feature is on.
fn available(ctx: AppCtx) -> Vec<String> {
    let french = ctx.lang.get() == Lang::Fr;
    ctx.prefs.with(|prefs| orgamnesia_core::tips::available(prefs, false, french, |id| ctx.shortcut(id)))
}

#[component]
pub fn TipsFrame() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    // A different one at each start
    let index = RwSignal::new((js_sys::Math::random() * orgamnesia_core::tips::TIPS.len() as f64) as usize);
    let tip = move || {
        let tips = available(ctx);
        (!tips.is_empty()).then(|| tips[index.get() % tips.len()].clone())
    };
    view! {
        <div class="frame panel-section tips-frame">
            <div class="panel-header panel-header-row">
                <span>{move || t("tips_title", ctx.lang.get())}</span>
                <button
                    class="panel-add"
                    title=move || t("tips_next", ctx.lang.get())
                    aria-label=move || t("tips_next", ctx.lang.get())
                    on:click=move |_| index.update(|i| *i += 1)
                >"›"</button>
            </div>
            <p class="tips-text" aria-live="polite">{tip}</p>
        </div>
    }
}
