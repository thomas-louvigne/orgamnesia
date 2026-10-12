//! The window creating a page (mounted at app root to avoid fragment issues).

use leptos::{ev, prelude::*};

use crate::{
    actions,
    i18n::t,
    state::AppCtx,
};

#[component]
pub fn NewPageModal() -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");
    let lang = move || ctx.lang.get();
    let name = RwSignal::new(String::new());
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
        actions::create_and_open_page(ctx, n);
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
