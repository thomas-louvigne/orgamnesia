//! The two columns beside the editor, each showing its frames in the order set
//! in the Display settings.

use leptos::prelude::*;
use orgamnesia_core::Frame;

use crate::{
    components::{
        backlinks_panel::{BacklinksFrame, BrokenFrame, TagsFrame, TodosFrame},
        org_bloom::OrgBloomFrame,
        sidebar::{Brand, GitLine, PagesFrame, ProjectSwitcher, SettingsButton},
    },
    i18n::t,
    keybindings::AppAction,
    state::AppCtx,
};

/// The frames shown in the column left of the editor, or right of it.
#[component]
pub fn Column(right: bool) -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");
    // Only when the frames of this column change: the others keep their state
    let frames = Memo::new(move |_| ctx.pref(|p| p.column(right)));
    view! {
        <aside
            class=if right { "side-column right" } else { "side-column left" }
            aria-label=move || t(if right { "layout_right" } else { "layout_left" }, ctx.lang.get())
        >
            <For each=move || frames.get() key=|f| *f let:frame>
                {frame_view(frame, right)}
            </For>
        </aside>
    }
}

fn frame_view(frame: Frame, right: bool) -> AnyView {
    match frame {
        Frame::Brand => view! { <Brand /> }.into_any(),
        Frame::Project => view! { <ProjectSwitcher /> }.into_any(),
        Frame::Git => view! { <GitLine /> }.into_any(),
        Frame::Pages => view! { <PagesFrame /> }.into_any(),
        Frame::Settings => view! { <SettingsButton /> }.into_any(),
        Frame::Backlinks => view! { <BacklinksFrame /> }.into_any(),
        Frame::Tags => view! { <TagsFrame /> }.into_any(),
        Frame::Todos => view! { <TodosFrame /> }.into_any(),
        Frame::Broken => view! { <BrokenFrame right /> }.into_any(),
        Frame::Quit => view! { <QuitButton /> }.into_any(),
        Frame::OrgBloom => view! { <OrgBloomFrame /> }.into_any(),
    }
}

/// Button quitting the app, with its shortcut in its tooltip.
#[component]
fn QuitButton() -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");
    view! {
        <div class="frame quit-frame">
            <button
                class="btn-quit"
                aria-label=move || t("quit_button_title", ctx.lang.get())
                title=move || {
                    let label = t("quit_button_title", ctx.lang.get());
                    match ctx.keybindings.with(|kb| kb.app_binds("quit").first().cloned()) {
                        Some(b) => format!("{label} ({})", crate::keybindings::display(&b)),
                        None => label.to_string(),
                    }
                }
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
