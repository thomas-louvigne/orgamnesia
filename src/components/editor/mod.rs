//! The editor of the pages: a textarea holding a view of the page, under layers
//! drawing it (highlighting, region, find matches).

mod broken;
mod completion;
mod dom;
mod find;
mod images;
mod keys;
mod pane;

use leptos::prelude::*;
use wasm_bindgen::JsCast;

use crate::{
    actions::follow_link,
    components::{empty_state::EmptyState, page_title::PageTitle, project_search::ProjectSearchPopin, frames::TagAddPopin, tabs::TabBar},
    edit,
    highlight,
    i18n::t,
    state::{AppCtx, Goto, SplitKind, Tab},
};
use broken::BrokenLinks;
use completion::Completion;
use dom::{after_tick, caret, caret_coords, number_digits, set_selection, Textarea, LINE_HEIGHT};
use find::Find;
use images::{ImageSizes, Images};
use pane::Pane;

// ─── Components ──────────────────────────────────────────────────────────────

/// The editor: one pane, or two when split (Emacs `C-x 2` / `C-x 3`).
#[component]
pub fn EditorSplit() -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");
    view! {
        <div class=move || match ctx.work.split.get() {
            None => "editor-split",
            Some(SplitKind::Vertical) => "editor-split vertical",
            Some(SplitKind::Horizontal) => "editor-split horizontal",
        }>
            <EditorArea second=false />
            {move || ctx.work.split.get().is_some().then(|| view! { <EditorArea second=true /> })}
        </div>
    }
}

#[component]
pub fn EditorArea(second: bool) -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");

    // The focused pane shows the active tab, the other one `other_path`
    let path = Memo::new(move |_| ctx.work.pane_path(second));
    let focused = move || ctx.work.split.get().is_none() || ctx.work.focus_second.get() == second;
    // Not `open` itself: showing the replace field must not mount the pop-in again
    let searching = Memo::new(move |_| ctx.ui.search.open.get().is_some());

    // When this pane takes the focus (C-x o, C-o…), the keyboard must follow:
    // put the text cursor in its editor.
    let area_ref = NodeRef::<leptos::html::Div>::new();
    Effect::new(move |_| {
        let has_focus = ctx.work.focus_second.get() == second;
        if ctx.work.split.get().is_none() || !has_focus { return; }
        crate::keybindings::after_ms(0, move || {
            let textarea = area_ref.try_get_untracked().flatten()
                .and_then(|div| div.query_selector("textarea").ok().flatten())
                .and_then(|el| el.dyn_into::<Textarea>().ok());
            if let Some(t) = textarea { let _ = t.focus(); }
        });
    });

    view! {
        <div
            node_ref=area_ref
            class=move || if focused() { "editor-area focused" } else { "editor-area" }
            on:mousedown=move |_| ctx.work.focus_pane(second)
            // Keyboard focus entering the pane (F6, Tab) makes it the one commands act on
            on:focusin=move |_| ctx.work.focus_pane(second)
        >
            {move || (ctx.work.split.get() == Some(SplitKind::Vertical)).then(|| {
                let name = path.try_get().flatten()
                    .and_then(|p| ctx.work.tabs.get().into_iter().find(|t| t.path == p))
                    .map(|t| t.name)
                    .unwrap_or_default();
                view! { <div class="pane-tab"><span class="pane-tab-name">{name}</span></div> }
            })}
            // Side by side: under each pane's title, its own tabs to choose its page.
            // One above the other: the bottom pane has its own tabs (the top bar is the top pane's).
            {move || {
                let split = ctx.work.split.get();
                (split == Some(SplitKind::Vertical) || (second && split == Some(SplitKind::Horizontal)))
                    .then(|| view! { <div class="pane-tabs"><TabBar pane=second /></div> })
            }}
            // The project search, at the height of the page title
            {move || (focused() && searching.get()).then(|| view! { <ProjectSearchPopin /> })}
            {move || (focused() && ctx.ui.tag_add.get()).then(|| view! { <TagAddPopin /> })}
            {move || ctx.pref(|p| p.show_page_title).then(|| {
                let p = path.try_get().flatten()?;
                let tab = ctx.work.tabs.with(|tabs| tabs.iter().find(|t| t.path == p).cloned())?;
                Some(view! { <PageTitle tab=tab /> })
            }).flatten()}
            <div class="editor-pane-body">
            // `try_get`: these also re-run when the pane is closed (C-x 0), after `path` is disposed
            {move || match path.try_get().flatten() {
                None => view! { <EmptyState /> }.into_any(),
                Some(p) => {
                    let tab = ctx.work.tabs.get_untracked().into_iter().find(|t| t.path == p);
                    match tab {
                        Some(tab) => view! { <Editor tab=tab second=second /> }.into_any(),
                        None => ().into_any(),
                    }
                }
            }}
            </div>
        </div>
    }
}

#[component]
fn Editor(tab: Tab, second: bool) -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");
    pane::autosave(ctx, tab.clone());

    let pane = Pane::new(ctx, &tab);
    let content = pane.content;
    let completion = Completion::new(pane);
    let find = Find::new(pane);
    // Natural sizes of the images of the page
    let sizes = RwSignal::new(ImageSizes::new());
    let view = pane::drawn_view(pane, tab.path.clone(), sizes);
    let broken = BrokenLinks::new(ctx, tab.path.clone(), content, sizes);
    let wrap_ref = NodeRef::<leptos::html::Div>::new();
    // The textarea gets the focus from the mouse: no caret ping
    let mouse_focus = StoredValue::new(false);
    let images = Images::new(pane, wrap_ref, tab.path.clone(), mouse_focus);
    goto_target(pane, tab.clone(), second);

    let on_input = move |e: web_sys::Event| {
        let Some(el) = e.target().and_then(|t| t.dyn_into::<Textarea>().ok()) else { return };
        // Typing ends the region
        pane.mark.set(None);
        pane.commit(&el);
        completion.on_input(&el);
    };

    // Ctrl+Click navigates to (or creates) the page of the wiki link, #tag or
    // org-mode :tag: under the cursor
    let on_click = move |e: web_sys::MouseEvent| {
        if !e.ctrl_key() { return; }
        let Some(el) = e.target().and_then(|t| t.dyn_into::<Textarea>().ok()) else { return };
        let pos = pane.page_pos(&el, el.selection_start().ok().flatten().unwrap_or(0) as usize);
        let chars: Vec<char> = content.get_untracked().chars().collect();
        if let Some(name) = edit::link_at(&chars, pos, ctx.tags_untracked()) {
            e.prevent_default();
            follow_link(ctx, name);
        }
    };

    // Mouse moves the caret too
    let on_pointer = move |e: web_sys::MouseEvent| {
        let Some(el) = e.target().and_then(|t| t.dyn_into::<Textarea>().ok()) else { return };
        // A click on a drawn table shows it as text
        if !e.ctrl_key() { pane.track_caret(&el); }
        if pane.mark.get_untracked().is_some() {
            after_tick(move || pane.point.set(caret(&el)));
        }
    };

    // Brief pulse at the caret when the keyboard focus lands in this editor
    // (C-x o, Tab from another panel…), so the eye finds the cursor.
    // Not on a click: the cursor is then where the mouse is.
    let ping = RwSignal::new(None::<(f64, f64, f64)>);
    let ping_seq = StoredValue::new(0u32);
    let on_focus = move |_| {
        if mouse_focus.get_value() { return; }
        after_tick(move || {
            let Some(el) = pane.area.try_get_untracked().flatten() else { return };
            let chars: Vec<char> = el.value().chars().collect();
            let pos = caret(&el).min(chars.len());
            let Some(coords) = caret_coords(&el, &chars, pos) else { return };
            let Some(seq) = ping_seq.try_get_value().map(|n| n + 1) else { return };
            ping_seq.set_value(seq);
            ping.set(Some(coords));
            crate::keybindings::after_ms(800, move || {
                if ping_seq.try_get_value() == Some(seq) { ping.set(None); }
            });
        });
    };

    let highlighted = move || view.with(|v| highlight::render_view(v, ctx.tags(), pane.todo_keywords.get(), ctx.pref(|p| p.show_line_numbers), broken.check()));
    let region_html = move || {
        if !ctx.pref(|p| p.emacs_mark) { return String::new(); }
        pane.mark.get().map(|m| {
            let p = pane.point.get();
            view.with(|v| highlight::render_region(&v.text, m.min(p), m.max(p)))
        }).unwrap_or_default()
    };
    let match_html = move || find.matches.with(|m| view.with(|v| {
        let m: Vec<(usize, usize)> = m.iter().map(|&(a, b)| (v.view_pos(a), v.view_pos(b))).collect();
        highlight::render_matches(&v.text, &m, find.current.get())
    }));

    view! {
        <div
            node_ref=wrap_ref
            class=move || format!("editor-wrap{}{}",
                if ctx.pref(|p| p.show_line_numbers) { " line-numbers" } else { "" },
                if ctx.pref(|p| p.hide_leading_stars) { " hide-stars" } else { "" })
            style=move || content.with(|c| format!("--ln-w: {}ch", number_digits(c)))
        >
            <div class="hl-layer" aria-hidden="true" inner_html=highlighted />
            <div class="hl-layer region-layer" aria-hidden="true" inner_html=region_html />
            <div class="hl-layer match-layer" aria-hidden="true" inner_html=match_html />
            <textarea
                node_ref=pane.area
                class="edit-layer"
                aria-label=format!("{} {}", t("editor_label", ctx.lang.get_untracked()), tab.name)
                spellcheck=false
                on:input=on_input
                on:keydown=move |e| keys::on_keydown(pane, completion, find, e)
                // Undo / redo from the context menu: the page's own history, as the keys
                on:beforeinput=move |e: web_sys::InputEvent| {
                    let redo = match e.input_type().as_str() {
                        "historyUndo" => false,
                        "historyRedo" => true,
                        _ => return,
                    };
                    e.prevent_default();
                    pane.undo(redo);
                }
                on:click=on_click
                on:mouseup=on_pointer
                on:mousedown=move |e: web_sys::MouseEvent| {
                    if images.start_drag(&e) { return; }
                    completion.close();
                    mouse_focus.set_value(true);
                    after_tick(move || mouse_focus.set_value(false));
                }
                on:mousemove=move |e| images.on_hover(&e)
                on:focus=on_focus
                on:blur=move |_| completion.close()
                on:scroll=move |_| completion.close()
            />
            <button
                class="img-add"
                title=move || t("image_add", ctx.lang.get())
                aria-label=move || t("image_add", ctx.lang.get())
                // Keeps the caret where it is in the textarea
                on:mousedown=|e: web_sys::MouseEvent| e.prevent_default()
                on:click=move |_| images.pick()
            >
                <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true" fill="none"
                    stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                    <rect x="3" y="4" width="18" height="16" rx="2" />
                    <circle cx="9" cy="10" r="1.8" />
                    <path d="M21 16l-5-5-9 9" />
                </svg>
            </button>
            {find.bar()}
            {move || ping.get().map(|(x, top, bottom)| view! {
                <div
                    class="caret-ping"
                    aria-hidden="true"
                    style=format!("left: {x}px; top: {top}px; height: {}px;", bottom - top)
                />
            })}
            {completion.popup()}
        </div>
    }
}

/// Jump to a link (from the "pages not created" list) or a line (from a tag search)
/// asked for this page, in the focused pane.
fn goto_target(pane: Pane, tab: Tab, second: bool) {
    let ctx = pane.ctx;
    Effect::new(move |_| {
        let Some((path, goto)) = ctx.work.goto.get() else { return };
        if path != tab.path || ctx.work.focus_second.get_untracked() != second { return; }
        let Some(el) = pane.area.get() else { return };
        ctx.work.goto.set(None);
        let chars: Vec<char> = tab.content.get_untracked().chars().collect();
        let range = match goto {
            Goto::Link(target) => {
                let ci = ctx.pref_untracked(|p| p.case_insensitive_links);
                crate::motion::find_link(&chars, &target, ci, ctx.tags_untracked())
            }
            Goto::Line(n) => {
                let start = edit::line_start(&chars, n);
                Some((start, start))
            }
            Goto::Range(a, b) => Some((a.min(chars.len()), b.min(chars.len()))),
        };
        if let Some((start, end)) = range {
            // Show the target's table as text first, if it is in one, and unfold it
            pane.focus_pos.set(Some(start));
            pane.folds.update(|f| f.retain(|&(a, b)| !(a < start && start <= b)));
            after_tick(move || {
                let Some((a, b)) = pane.shown.try_with_value(|v| (v.view_pos(start), v.view_pos(end))) else { return };
                let line = el.value().chars().take(a).filter(|&c| c == '\n').count();
                let _ = el.focus();
                set_selection(&el, a, b);
                // Bring the line to the middle of the view
                let y = (line as f64 * LINE_HEIGHT - el.client_height() as f64 / 2.0).max(0.0);
                el.set_scroll_top(y as i32);
            });
        }
    });
}
