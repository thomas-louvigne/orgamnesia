//! The window of the first run: the format of the pages and the shortcuts,
//! both changed later in the settings.

use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::{i18n::t, invoke, keybindings::Keybindings, state::AppCtx};
use orgamnesia_core::Format;

#[component]
pub fn WelcomeModal() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();
    let format = RwSignal::new(ctx.pref_untracked(|p| p.default_format));
    let profile = RwSignal::new(ctx.keybindings.with_untracked(|kb| kb.editor_preset.clone()));
    let busy = RwSignal::new(false);

    let start = move |_| {
        if busy.get_untracked() { return; }
        busy.set(true);
        let mut prefs = ctx.prefs.get_untracked();
        prefs.default_format = format.get_untracked();
        let kb = Keybindings::with_profiles(Some(&profile.get_untracked()), ctx.keybindings.with_untracked(|k| k.profiles.clone()));
        spawn_local(async move {
            let settings = prefs.to_settings(ctx.project.projects.get_untracked());
            match invoke::save_settings(&settings).await {
                Ok(()) => ctx.prefs.set(prefs),
                Err(e) => ctx.error("error", &e),
            }
            match invoke::set_keybindings(&kb).await {
                Ok(mut kb) => {
                    kb.upgrade();
                    ctx.keybindings.set(kb);
                }
                Err(e) => ctx.error("keybindings_error", &e),
            }
            ctx.ui.show_welcome.set(false);
        });
    };

    // One choice among two, each with what it means
    let choice = move |name: &'static str, value: &'static str, label: &'static str, hint: &'static str, checked: Signal<bool>, pick: Callback<()>| view! {
        <label class=move || if checked.get() { "welcome-choice selected" } else { "welcome-choice" }>
            <input type="radio" name=name value=value prop:checked=move || checked.get() on:change=move |_| pick.run(()) />
            <span class="welcome-choice-text">
                <strong>{move || t(label, lang())}</strong>
                <span class="setting-hint">{move || t(hint, lang())}</span>
            </span>
        </label>
    };

    view! {
        <div class="modal-overlay">
            <div class="modal welcome" role="dialog" aria-modal="true" aria-labelledby="welcome-title">
                <div class="modal-header">
                    <h2 id="welcome-title">{move || t("welcome_title", lang())}</h2>
                </div>
                <div class="modal-body">
                    <p>{move || t("welcome_intro", lang())}</p>
                    <fieldset class="welcome-group">
                        <legend>{move || t("default_format", lang())}</legend>
                        {choice("welcome-format", "org", "format_org", "welcome_org_hint",
                            Signal::derive(move || format.get() == Format::Org), Callback::new(move |_| format.set(Format::Org)))}
                        {choice("welcome-format", "markdown", "format_markdown", "welcome_markdown_hint",
                            Signal::derive(move || format.get() == Format::Markdown), Callback::new(move |_| format.set(Format::Markdown)))}
                    </fieldset>
                    <fieldset class="welcome-group">
                        <legend>{move || t("welcome_shortcuts", lang())}</legend>
                        {choice("welcome-keys", "classic", "preset_classic", "welcome_classic_hint",
                            Signal::derive(move || profile.get() == "classic"), Callback::new(move |_| profile.set("classic".into())))}
                        {choice("welcome-keys", "emacs", "preset_emacs", "welcome_emacs_hint",
                            Signal::derive(move || profile.get() == "emacs"), Callback::new(move |_| profile.set("emacs".into())))}
                    </fieldset>
                    <p class="setting-hint">{move || t("welcome_later", lang())}</p>
                </div>
                <div class="modal-footer">
                    <button class="btn-primary" prop:disabled=move || busy.get() on:click=start>
                        {move || t("welcome_start", lang())}
                    </button>
                </div>
            </div>
        </div>
    }
}
