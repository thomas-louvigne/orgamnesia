//! Small frames: the name of the app, the settings and quit buttons.

use leptos::prelude::*;

use crate::{
    i18n::t,
    keybindings::AppAction,
    state::AppCtx,
};

/// Name and logo of the app.
#[component]
pub fn Brand() -> impl IntoView {
    view! {
        <div class="frame app-brand">
            <img class="app-brand-logo" src="app-icon.svg" alt="" />
            <span class="app-brand-name">"Orgamnesia"</span>
        </div>
    }
}

/// Button opening the settings.
#[component]
pub fn SettingsButton() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    view! {
        <div class="frame sidebar-footer">
            <button
                class="btn-settings"
                title=move || ctx.with_shortcut("settings_btn", "open_settings")
                on:click=move |_| ctx.ui.show_settings.set(true)
            >
                {move || t("settings_btn", ctx.lang.get())}
            </button>
        </div>
    }
}

/// Button quitting the app, with its shortcut in its tooltip.
#[component]
pub fn QuitButton() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    view! {
        <div class="frame quit-frame">
            <button
                class="btn-quit"
                aria-label=move || t("quit_button_title", ctx.lang.get())
                title=move || ctx.with_shortcut("quit_button_title", "quit")
                on:click=move |_| crate::app::run_app_action(ctx, AppAction::Quit)
            >
                <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor"
                    stroke-width="2.2" stroke-linecap="round" aria-hidden="true">
                    <path d="M12 3v8" />
                    <path d="M6.3 6.8a8 8 0 1 0 11.4 0" />
                </svg>
            </button>
        </div>
    }
}
