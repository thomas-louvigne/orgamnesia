use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::{i18n::{t, Lang}, state::AppCtx};

/// A site built longer ago than this (in seconds: a week) is marked as old.
const STALE_AFTER: u64 = 7 * 24 * 3600;

/// State of the site shown by the marker of the button.
#[derive(Clone, Copy, PartialEq)]
enum Site {
    Building,
    /// Built this many seconds ago.
    Built(u64),
    Stale(u64),
    Never,
}

impl Site {
    fn class(self) -> &'static str {
        match self {
            Site::Building => "bloom-mark running",
            Site::Built(_) => "bloom-mark built",
            Site::Stale(_) | Site::Never => "bloom-mark stale",
        }
    }

    /// What the marker says, for the tooltip.
    fn label(self, lang: Lang) -> String {
        match self {
            Site::Building => t("bloom_running", lang).to_string(),
            Site::Built(age) => format!("{} ({})", t("bloom_mark_built", lang), age_text(age, lang)),
            Site::Stale(age) => format!("{} ({})", t("bloom_mark_stale", lang), age_text(age, lang)),
            Site::Never => t("bloom_mark_never", lang).to_string(),
        }
    }
}

/// `3 days ago`, `2 h ago`…
fn age_text(secs: u64, lang: Lang) -> String {
    let (n, unit) = match secs {
        s if s < 60 => return t("bloom_age_now", lang).to_string(),
        s if s < 3600 => (s / 60, "min"),
        s if s < 24 * 3600 => (s / 3600, "h"),
        s => (s / (24 * 3600), t("bloom_age_days", lang)),
    };
    t("bloom_age_ago", lang).replace("{}", &format!("{n} {unit}"))
}

fn now_secs() -> u64 {
    (js_sys::Date::now() / 1000.0) as u64
}

/// Button building the site of the project with org-bloom (org-bloom extension),
/// with a marker telling whether the site is built, being built, or old.
#[component]
pub fn OrgBloomFrame() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let busy = move || ctx.project.bloom_busy.get();
    // When the site was built, read again for every project opened
    Effect::new(move |_| {
        ctx.project.vault_path.track();
        ctx.project.bloom_built.set(None);
        spawn_local(crate::org_bloom::read_built(ctx));
    });
    // The age of the site grows while the app stays open
    let now = RwSignal::new(now_secs());
    if let Ok(tick) = set_interval_with_handle(move || now.set(now_secs()), std::time::Duration::from_secs(600)) {
        on_cleanup(move || tick.clear());
    }
    let site = move || {
        if busy() { return Site::Building; }
        match ctx.project.bloom_built.get() {
            None => Site::Never,
            Some(at) => {
                let age = now.get().saturating_sub(at);
                if age > STALE_AFTER { Site::Stale(age) } else { Site::Built(age) }
            }
        }
    };
    let tip = move || {
        let lang = ctx.lang.get();
        let state = site().label(lang);
        if busy() { state } else { format!("{}\n{state}", t("bloom_title", lang)) }
    };
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
                {move || ctx.project.vault_path.get().is_some().then(|| view! {
                    <span class=move || site().class() aria-hidden="true"></span>
                })}
            </button>
        </div>
    }
}
