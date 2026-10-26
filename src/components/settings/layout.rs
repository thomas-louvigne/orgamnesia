//! Where the frames go around the editor (Display tab).

use leptos::prelude::*;

use crate::{i18n::t, state::{AppCtx, Prefs}};
use orgamnesia_core::{Frame, Layout, Move};

/// Name of the setting showing `frame`, also its label.
fn frame_label(frame: Frame) -> &'static str {
    match frame {
        Frame::Brand => "show_brand",
        Frame::Project => "show_project",
        Frame::Git => "show_git",
        Frame::Pages => "show_pages",
        Frame::Settings => "show_settings_button",
        Frame::Backlinks => "show_backlinks",
        Frame::Tags => "show_tags",
        Frame::Todos => "show_todos",
        Frame::Broken => "show_broken_links",
        Frame::Quit => "show_quit_button",
        Frame::OrgBloom => "show_org_bloom",
    }
}

/// Whether the layout editor leaves `frame` out: the frame of an extension that
/// is off (org-bloom).
fn unlisted(p: &Prefs, frame: Frame) -> bool {
    frame == Frame::OrgBloom && !p.org_bloom
}

/// Where the frames go around the editor: the two columns, each with its frames
/// top to bottom, a box to show each one and arrows to move it.
#[component]
pub fn LayoutEditor(draft: RwSignal<Prefs>) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();
    let listed = move |right: bool| draft.with(|p| {
        p.layout.column(right).iter().copied().filter(|f| !unlisted(p, *f)).collect::<Vec<_>>()
    });
    let column = move |right: bool| view! {
        <div class="layout-col" role="group"
            aria-label=move || t(if right { "layout_right" } else { "layout_left" }, lang())>
            <div class="layout-col-title">
                {move || t(if right { "layout_right" } else { "layout_left" }, lang())}
            </div>
            {move || listed(right).is_empty().then(|| view! {
                <div class="layout-empty">{move || t("layout_empty", lang())}</div>
            })}
            <For each=move || listed(right) key=|f| *f let:frame>
                <LayoutFrame draft frame />
            </For>
        </div>
    };
    view! {
        <div class="setting-row">
            <span class="setting-hint">{move || t("layout_hint", lang())}</span>
        </div>
        <div class="layout-editor">
            {column(false)}
            <div class="layout-editor-center" aria-hidden="true">{move || t("layout_editor", lang())}</div>
            {column(true)}
        </div>
        <div class="setting-row">
            <button
                class="btn-pick layout-reset"
                prop:disabled=move || draft.with(|p| p.layout == Layout::default())
                on:click=move |_| draft.update(|p| p.layout = Layout::default())
            >{move || t("layout_reset", lang())}</button>
        </div>
    }
}

/// One frame in the layout editor: its box and its arrows.
#[component]
fn LayoutFrame(draft: RwSignal<Prefs>, frame: Frame) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();
    let label = frame_label(frame);
    // Shown only with the feature it belongs to
    let needs = move || draft.with(|p| match frame {
        Frame::Git if !p.git_ext => Some("layout_needs_git"),
        Frame::Todos if !p.todo_enabled => Some("layout_needs_todo"),
        _ => None,
    });
    // Its column, and its place among the frames listed there
    let place = move || draft.with(|p| {
        let right = p.layout.find(frame).is_some_and(|(r, _)| r);
        let listed: Vec<Frame> = p.layout.column(right).iter().copied().filter(|f| !unlisted(p, *f)).collect();
        (right, listed.iter().position(|f| *f == frame).unwrap_or(0), listed.len())
    });
    let move_it = move |how: Move| draft.update(|p| {
        let shown = p.clone();
        p.layout.move_frame(frame, how, |f| unlisted(&shown, f));
    });
    let arrow = move |how: Move, icon: &'static str, key: Signal<&'static str>, disabled: Signal<bool>| view! {
        <button
            class="btn-icon layout-arrow"
            title=move || t(key.get(), lang())
            aria-label=move || format!("{} — {}", t(key.get(), lang()), t(label, lang()))
            prop:disabled=move || disabled.get()
            on:click=move |_| move_it(how)
        >{icon}</button>
    };
    view! {
        <div class=move || if draft.with(|p| p.frame_shown(frame)) { "layout-frame" } else { "layout-frame off" }>
            <label class="setting-check" title=move || needs().map(|k| t(k, lang())).unwrap_or_default()>
                <input
                    type="checkbox"
                    prop:disabled=move || needs().is_some()
                    prop:checked=move || draft.with(|p| p.frame_on(frame))
                    on:change=move |e| draft.update(|p| *p.frame_setting(frame) = event_target_checked(&e))
                />
                {move || t(label, lang())}
            </label>
            <div class="layout-arrows">
                {arrow(Move::Up, "↑", Signal::stored("layout_up"), Signal::derive(move || place().1 == 0))}
                {arrow(Move::Down, "↓", Signal::stored("layout_down"),
                    Signal::derive(move || { let (_, i, n) = place(); i + 1 >= n }))}
                {move || {
                    let right = place().0;
                    arrow(Move::OtherColumn, if right { "←" } else { "→" },
                        Signal::stored(if right { "layout_to_left" } else { "layout_to_right" }),
                        Signal::stored(false))
                }}
            </div>
        </div>
    }
}
