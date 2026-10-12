use leptos::prelude::*;

use crate::{i18n::t, state::AppCtx};

/// Button building the site of the project with org-bloom (org-bloom extension).
#[component]
pub fn OrgBloomFrame() -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");
    let busy = move || ctx.project.bloom_busy.get();
    let tip = move || t(if busy() { "bloom_running" } else { "bloom_title" }, ctx.lang.get());
    view! {
        <div class="frame bloom-frame">
            <button
                class=move || if busy() { "git-btn bloom-btn running" } else { "git-btn bloom-btn" }
                title=tip
                aria-label=tip
                aria-busy=move || busy().then_some("true")
                prop:disabled=move || busy() || ctx.project.vault_path.get().is_none()
                on:click=move |_| crate::org_bloom::build(ctx)
            >
                {move || if busy() {
                    view! { <span class="git-spinner" aria-hidden="true"></span> }.into_any()
                } else {
                    view! { <span aria-hidden="true">"✿"</span> }.into_any()
                }}
                "org-bloom"
            </button>
        </div>
    }
}
