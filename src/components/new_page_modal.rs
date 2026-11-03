//! The window creating a page (mounted at app root to avoid fragment issues).

use leptos::{ev, prelude::*};

use orgamnesia_core::Format;

use crate::{
    actions,
    i18n::t,
    state::AppCtx,
};

#[component]
pub fn NewPageModal() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();
    let name = RwSignal::new(ctx.ui.new_page_name.get_untracked());
    ctx.ui.new_page_name.set(String::new());
    // The format, when it is asked for each new page
    let ask = actions::asks_format(ctx);
    let format = RwSignal::new(ctx.pref_untracked(|p| p.default_format));
    let input_ref = NodeRef::<leptos::html::Input>::new();

    // Autofocus input on mount
    Effect::new(move |_| {
        if let Some(el) = input_ref.get() {
            let _ = el.focus();
        }
    });

    // Escape closes
    let esc = window_event_listener(ev::keydown, move |e| {
        if e.key() == "Escape" {
            ctx.ui.show_new_page.set(false);
        }
    });
    on_cleanup(move || esc.remove());

    let do_create = move || {
        let n = name.get_untracked().trim().to_string();
        if n.is_empty() {
            return;
        }
        ctx.ui.show_new_page.set(false);
        actions::create_page_as(ctx, n, ask.then(|| format.get_untracked()));
    };

    view! {
        <div class="modal-overlay" on:click=move |_| ctx.ui.show_new_page.set(false)>
            <div class="modal" on:click=|e| e.stop_propagation()>
                <div class="modal-header">
                    <h2>{move || t("new_page_title", lang())}</h2>
                    <button class="btn-close" aria-label=move || t("close", ctx.lang.get()) on:click=move |_| ctx.ui.show_new_page.set(false)>"×"</button>
                </div>
                <div class="modal-body">
                    <input
                        node_ref=input_ref
                        type="text"
                        class="setting-input"
                        placeholder={move || t("new_page_ph", lang())}
                        prop:value=move || name.get()
                        on:input=move |e| name.set(event_target_value(&e))
                        on:keydown=move |e| { if e.key() == "Enter" { do_create(); } }
                    />
                    {ask.then(|| view! {
                        <div class="new-page-format" role="radiogroup" aria-label=move || t("default_format", lang())>
                            {[Format::Org, Format::Markdown].into_iter().map(|f| view! {
                                <label class="setting-check">
                                    <input type="radio" name="new-page-format"
                                        prop:checked=move || format.get() == f
                                        on:change=move |_| format.set(f) />
                                    {move || t(if f == Format::Org { "format_org" } else { "format_markdown" }, lang())}
                                </label>
                            }).collect_view()}
                        </div>
                    })}
                </div>
                <div class="modal-footer">
                    <button class="btn-secondary" on:click=move |_| ctx.ui.show_new_page.set(false)>
                        {move || t("cancel", lang())}
                    </button>
                    <button class="btn-primary" on:click=move |_| do_create()>
                        {move || t("create", lang())}
                    </button>
                </div>
            </div>
        </div>
    }
}
