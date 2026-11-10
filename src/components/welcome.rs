//! What the editor shows on the first run, in place of the empty state: the
//! format of the pages, the shortcuts and the modules (git, the to-do list), all
//! changed later in the settings.

use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::{i18n::t, invoke, keybindings::Keybindings, state::AppCtx};
use orgamnesia_core::Format;

#[component]
pub fn Welcome() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();
    let format = RwSignal::new(ctx.pref_untracked(|p| p.default_format));
    let profile = RwSignal::new(ctx.keybindings.with_untracked(|kb| kb.editor_preset.clone()));
    let git = RwSignal::new(ctx.pref_untracked(|p| p.git_ext));
    let todos = RwSignal::new(ctx.pref_untracked(|p| p.show_todos));
    let graph = RwSignal::new(ctx.pref_untracked(|p| p.graph));
    let busy = RwSignal::new(false);

    let start = move |_| {
        if busy.get_untracked() { return; }
        busy.set(true);
        let mut prefs = ctx.prefs.get_untracked();
        prefs.default_format = format.get_untracked();
        prefs.git_ext = git.get_untracked();
        prefs.show_todos = todos.get_untracked();
        prefs.graph = graph.get_untracked();
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

    // One choice among two (a radio), or a module to take or not (a checkbox), each with what it means
    let choice = move |kind: &'static str, name: &'static str, value: &'static str, label: &'static str, hint: &'static str, checked: Signal<bool>, pick: Callback<()>| view! {
        <label class=move || if checked.get() { "welcome-choice selected" } else { "welcome-choice" }>
            <input type=kind name=name value=value prop:checked=move || checked.get() on:change=move |_| pick.run(()) />
            <span class="welcome-choice-text">
                <strong>{move || t(label, lang())}</strong>
                <span class="setting-hint">{move || t(hint, lang())}</span>
            </span>
        </label>
    };

    view! {
        <div class="editor-empty welcome-page">
            <section class="welcome" aria-labelledby="welcome-title">
                <div class="welcome-head">
                    <img class="empty-logo" src="app-icon.svg" alt="" />
                    <h2 id="welcome-title" class="empty-title">{move || t("welcome_title", lang())}</h2>
                    <p class="empty-text">{move || t("welcome_intro", lang())}</p>
                </div>
                <div class="welcome-body">
                    <fieldset class="welcome-group">
                        <legend>{move || t("default_format", lang())}</legend>
                        {choice("radio", "welcome-format", "org", "format_org", "welcome_org_hint",
                            Signal::derive(move || format.get() == Format::Org), Callback::new(move |_| format.set(Format::Org)))}
                        {choice("radio", "welcome-format", "markdown", "format_markdown", "welcome_markdown_hint",
                            Signal::derive(move || format.get() == Format::Markdown), Callback::new(move |_| format.set(Format::Markdown)))}
                    </fieldset>
                    <fieldset class="welcome-group">
                        <legend>{move || t("welcome_shortcuts", lang())}</legend>
                        {choice("radio", "welcome-keys", "classic", "preset_classic", "welcome_classic_hint",
                            Signal::derive(move || profile.get() == "classic"), Callback::new(move |_| profile.set("classic".into())))}
                        {choice("radio", "welcome-keys", "emacs", "preset_emacs", "welcome_emacs_hint",
                            Signal::derive(move || profile.get() == "emacs"), Callback::new(move |_| profile.set("emacs".into())))}
                    </fieldset>
                    <fieldset class="welcome-group">
                        <legend>{move || t("welcome_modules", lang())}</legend>
                        {choice("checkbox", "welcome-git", "git", "welcome_git", "welcome_git_hint",
                            Signal::derive(move || git.get()), Callback::new(move |_| git.update(|v| *v = !*v)))}
                        {choice("checkbox", "welcome-todos", "todos", "welcome_todos", "welcome_todos_hint",
                            Signal::derive(move || todos.get()), Callback::new(move |_| todos.update(|v| *v = !*v)))}
                        {choice("checkbox", "welcome-graph", "graph", "welcome_graph", "welcome_graph_hint",
                            Signal::derive(move || graph.get()), Callback::new(move |_| graph.update(|v| *v = !*v)))}
                    </fieldset>
                    <p class="setting-hint">{move || t("welcome_later", lang())}</p>
                </div>
                <div class="empty-actions">
                    <button class="btn-primary" prop:disabled=move || busy.get() on:click=start>
                        {move || t("welcome_start", lang())}
                    </button>
                </div>
            </section>
        </div>
    }
}
