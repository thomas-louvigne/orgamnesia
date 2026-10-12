//! The frames of the two columns beside the editor, each column showing its
//! frames in the order set in the Display settings.

mod backlinks;
mod broken;
mod buttons;
mod git;
mod org_bloom;
mod pages;
mod project;
mod tags;
mod todos;

use leptos::prelude::*;
use orgamnesia_core::Frame;

use crate::{actions, i18n::t, state::{AppCtx, Goto}};
use backlinks::BacklinksFrame;
use broken::BrokenFrame;
use buttons::{Brand, QuitButton, SettingsButton};
use git::GitLine;
use org_bloom::OrgBloomFrame;
use pages::PagesFrame;
use project::ProjectSwitcher;
use tags::TagsFrame;
use todos::TodosFrame;

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

/// Open the page called `name`; with `target`, also move the cursor there.
fn open_page(ctx: AppCtx, name: String, target: Option<Goto>) {
    actions::open_page_named(ctx, &name, target)
}
