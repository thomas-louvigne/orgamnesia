use leptos::{ev, prelude::*};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

use crate::{
    actions,
    components::{
        dialogs,
        frames::Column,
        editor::EditorSplit,
        quick_open::QuickOpenModal,
        settings::SettingsModal,
        new_page_modal::NewPageModal,
        tabs::TabBar,
    },
    i18n::t,
    invoke,
    keybindings::{AppAction, EditorAction, Resolution, Scope},
    state::{AppCtx, Drag, SplitKind},
    session,
    storage,
    text_field,
    watchers,
};

const SIDEBAR_KEY: &str = "sidebar_width";
const PANEL_KEY: &str = "panel_width";
const BROKEN_KEY: &str = "broken_height";
const MIN_SIDE: i32 = 140;
const MIN_BROKEN: i32 = 40;
fn load_size(key: &str, default: i32) -> i32 {
    storage::load(key).and_then(|v| v.parse().ok()).unwrap_or(default)
}
/// Draggable bar between a side column and the workspace.
#[component]
fn Resizer(which: Drag) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    view! {
        <div
            class="resizer"
            on:mousedown=move |e: web_sys::MouseEvent| {
                e.prevent_default();
                ctx.ui.drag.set(Some(which));
            }
        />
    }
}
/// Run an application action (shortcut).
pub fn run_app_action(ctx: AppCtx, action: AppAction) {
    // What acts on the pages of a project has nothing to act on in the "preference" one
    let project_only = matches!(action, AppAction::NewPage | AppAction::ProjectSearch | AppAction::ProjectReplace
        | AppAction::AddTag | AppAction::GitCommit | AppAction::GitPull | AppAction::GitPush | AppAction::ExportPdf | AppAction::ExportHtml);
    if project_only && ctx.project.preference.get_untracked() {
        ctx.notify_t("pref_unavailable", "");
        return;
    }
    let tab_count = ctx.work.tabs.with_untracked(|t| t.len());
    let active = ctx.work.active_tab.get_untracked();
    match action {
        AppAction::NewPage      => ctx.ui.show_new_page.set(true),
        AppAction::OpenSettings => ctx.ui.show_settings.set(true),
        AppAction::ShowShortcuts => ctx.ui.show_shortcuts.update(|s| *s = !*s),
        AppAction::QuickOpen    => ctx.ui.show_quick_open.set(true),
        AppAction::ProjectSearch  => ctx.ui.search.open.set(Some(false)),
        AppAction::ProjectReplace => ctx.ui.search.open.set(Some(true)),
        AppAction::AddTag => if ctx.work.active_tab_data().is_some() {
            ctx.ui.tag_add.set(true);
        } else {
            ctx.notify(t("tag_add_no_page", ctx.lang.get_untracked()));
        },
        AppAction::CloseTab     => if let Some(idx) = active { actions::close_tab(ctx, idx) },
        AppAction::Save         => if let Some(tab) = ctx.work.active_tab_data() { actions::save_tab(ctx, tab) },
        AppAction::NextTab | AppAction::PrevTab => {
            let step = if action == AppAction::NextTab { 1 } else { -1 };
            if let Some(idx) = cycle(active, tab_count, step) { ctx.work.active_tab.set(Some(idx)); }
        }
        AppAction::PageBack        => actions::history_step(ctx, true),
        AppAction::PageForward     => actions::history_step(ctx, false),
        AppAction::SplitVertical   => ctx.split_window(SplitKind::Vertical),
        AppAction::SplitHorizontal => ctx.split_window(SplitKind::Horizontal),
        // With a single pane, closes the tab instead
        AppAction::CloseSplit => if ctx.work.split.get_untracked().is_some() {
            ctx.work.close_window()
        } else if let Some(idx) = active {
            actions::close_tab(ctx, idx)
        },
        AppAction::SingleWindow    => ctx.work.single_window(),
        AppAction::OtherWindow     => ctx.work.other_window(),
        AppAction::NextRegion      => crate::a11y::cycle_region(true),
        AppAction::PrevRegion      => crate::a11y::cycle_region(false),
        AppAction::GitCommit       => crate::git::open_commit(ctx),
        AppAction::GitPull         => crate::git::pull(ctx),
        AppAction::GitPush         => crate::git::push(ctx),
        AppAction::ExportPdf | AppAction::ExportHtml => match ctx.work.active_tab_data() {
            Some(tab) if action == AppAction::ExportPdf => crate::export::export_pdf(ctx, tab),
            Some(tab) => crate::export::export_html(ctx, tab),
            None => ctx.notify_t("export_no_page", ""),
        },
        AppAction::Quit => {
            // With the automatic sync on, the project is synced first
            let quit = move || crate::git::sync_then(ctx, move || spawn_local(async move {
                if let Err(err) = invoke::quit_app().await {
                    ctx.error("quit_error", &err);
                }
            }));
            if ctx.work.has_unsaved_tabs() || crate::preferences::aside_unsaved(ctx) {
                ctx.ask_confirm(t("quit_unsaved_confirm", ctx.lang.get_untracked()).to_string(), quit);
            } else {
                quit();
            }
        }
    }
}
/// The tab `step` places after `active` among `count` tabs, wrapping around.
fn cycle(active: Option<usize>, count: usize, step: isize) -> Option<usize> {
    if count == 0 { return None; }
    let from = active.unwrap_or(0) as isize;
    Some((from + step).rem_euclid(count as isize) as usize)
}
#[component]
pub fn App() -> impl IntoView {
    let ctx = AppCtx::new();
    provide_context(ctx);
    // The backend's errors and the crash message are worded in the chosen language
    Effect::new(move |_| crate::i18n::set_current(ctx.lang.get()));

    // Sizes of the side menus and of the "pages not created" block, remembered between runs
    let sidebar_w = RwSignal::new(load_size(SIDEBAR_KEY, 220));
    let panel_w = RwSignal::new(load_size(PANEL_KEY, 200));
    let broken_h = RwSignal::new(load_size(BROKEN_KEY, 160));
    let drag_move = window_event_listener(ev::mousemove, move |e: web_sys::MouseEvent| {
        let Some(which) = ctx.ui.drag.get_untracked() else { return };
        let win = web_sys::window();
        let dim = |f: fn(&web_sys::Window) -> Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue>| {
            win.as_ref().and_then(|w| f(w).ok()).and_then(|v| v.as_f64()).unwrap_or(1000.0) as i32
        };
        let (win_w, win_h) = (dim(|w| w.inner_width()), dim(|w| w.inner_height()));
        match which {
            Drag::Sidebar => sidebar_w.set(e.client_x().clamp(MIN_SIDE, (win_w / 2).max(MIN_SIDE))),
            Drag::Panel => panel_w.set((win_w - e.client_x()).clamp(MIN_SIDE, (win_w / 2).max(MIN_SIDE))),
            Drag::Broken { above } => {
                // Height of the list = from the bar to the other edge of the block, minus its title
                let doc = win.as_ref().and_then(|w| w.document());
                let find = |sel: &str| doc.as_ref().and_then(|d| d.query_selector(sel).ok().flatten());
                let Some(block) = find(".broken-links") else { return };
                let header = find(".broken-links-header")
                    .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok())
                    .map(|el| el.offset_height()).unwrap_or(30);
                let rect = block.get_bounding_client_rect();
                let h = if above { rect.bottom() as i32 - e.client_y() } else { e.client_y() - rect.top() as i32 };
                broken_h.set((h - header).clamp(MIN_BROKEN, (win_h * 7 / 10).max(MIN_BROKEN)));
            }
        }
    });
    let drag_up = window_event_listener(ev::mouseup, move |_| {
        let Some(which) = ctx.ui.drag.get_untracked() else { return };
        let (key, size) = match which {
            Drag::Sidebar => (SIDEBAR_KEY, sidebar_w),
            Drag::Panel => (PANEL_KEY, panel_w),
            Drag::Broken { .. } => (BROKEN_KEY, broken_h),
        };
        storage::store(key, &size.get_untracked().to_string());
        ctx.ui.drag.set(None);
    });
    on_cleanup(move || { drag_move.remove(); drag_up.remove(); });

    spawn_local(session::load_startup(ctx, Owner::current().expect("App owner")));
    watchers::start(ctx);
    crate::update::at_start(ctx);

    // Page history: each time the focused pane shows another page, the one it
    // left is recorded (not when the focus only moves to the other pane).
    Effect::new(move |prev: Option<(Option<String>, bool)>| {
        let page = ctx.work.active_tab_data().map(|t| t.path);
        let second = ctx.work.focus_second.get();
        if let Some((Some(left), was_second)) = prev
            && was_second == second && page.as_ref() != Some(&left)
        {
            actions::record_visit(ctx, left, page.as_deref());
        }
        (page, second)
    });

    // Drop the split when the page shown in the other pane is closed
    Effect::new(move |_| {
        let tabs = ctx.work.tabs.get();
        if ctx.work.split.get_untracked().is_none() { return; }
        let gone = match ctx.work.other_path.get_untracked() {
            Some(p) => !tabs.iter().any(|t| t.path == p),
            None => true,
        };
        if gone || tabs.is_empty() { ctx.work.single_window(); }
    });

    // Global keyboard shortcuts (app-level; editor actions are handled in the editor)
    let kb_handle = window_event_listener(ev::keydown, move |e: web_sys::KeyboardEvent| {
        let resolution = ctx.keybindings.with_untracked(|kb| crate::keybindings::resolve(kb, &e));
        match resolution {
            Resolution::Action(Scope::App, id) => {
                ctx.clear_chord_status();
                e.prevent_default();
                if let Some(action) = AppAction::from_id(&id) { run_app_action(ctx, action); }
            }
            // The editor runs its own; the other text fields get the ones that make sense there
            Resolution::Action(Scope::Editor, id) => {
                ctx.clear_chord_status();
                if let Some(action) = EditorAction::from_id(&id) && text_field::run(ctx, &e, action) {
                    e.prevent_default();
                }
            }
            Resolution::Pending(keys) => {
                e.prevent_default();
                ctx.set_chord_status(&keys);
            }
            Resolution::Aborted => {
                e.prevent_default();
                ctx.clear_chord_status();
            }
            _ => ctx.clear_chord_status(),
        }
    });
    on_cleanup(move || kb_handle.remove());

    // No webview context menu (Back / Forward / Stop / Reload) outside text
    // fields; the editor and inputs keep theirs for cut / copy / paste.
    let ctx_menu_handle = window_event_listener(ev::contextmenu, move |e: web_sys::MouseEvent| {
        let editable = e.target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
            .is_some_and(|el| el.closest("textarea, input").ok().flatten().is_some());
        if !editable { e.prevent_default(); }
    });
    on_cleanup(move || ctx_menu_handle.remove());

    // Whether a column has a frame to show; a Memo, so that the column isn't
    // mounted again on every change of the settings
    let left = Memo::new(move |_| !ctx.pref(|p| p.column(false)).is_empty());
    let right = Memo::new(move |_| !ctx.pref(|p| p.column(true)).is_empty());
    let has_column = move |r: bool| if r { right.get() } else { left.get() };

    view! {
        <div class=move || if ctx.ui.drag.get().is_some() { "app resizing" } else { "app" }>
            <div
                class="app-main"
                style=move || format!(
                    "--sidebar-w: {}px; --panel-w: {}px; --broken-h: {}px;",
                    sidebar_w.get(), panel_w.get(), broken_h.get()
                )
            >
                {move || has_column(false).then(|| view! {
                    <Column right=false />
                    <Resizer which=Drag::Sidebar />
                })}
                <main class="workspace">
                    // Side by side, each pane has its own tabs under its title (see EditorArea)
                    {move || {
                        let split = ctx.work.split.get();
                        (split != Some(SplitKind::Vertical)).then(|| view! {
                            <div class="workspace-topbar">
                                {match split {
                                    None => view! { <TabBar /> }.into_any(),
                                    _ => view! { <TabBar pane=false /> }.into_any(),
                                }}
                            </div>
                        })
                    }}
                    <EditorSplit />
                </main>
                {move || has_column(true).then(|| view! {
                    <Resizer which=Drag::Panel />
                    <Column right=true />
                })}
            </div>
            {move || ctx.ui.show_quick_open.get().then(|| view! { <QuickOpenModal /> })}
            {move || ctx.ui.show_settings.get().then(|| view! { <SettingsModal /> })}
            {move || ctx.ui.confirm.get().is_some().then(|| view! { <dialogs::ConfirmDialog /> })}
            {move || ctx.ui.disk_conflicts.with(|q| !q.is_empty()).then(|| view! { <dialogs::DiskConflictDialog /> })}
            {move || ctx.ui.show_new_page.get().then(|| view! { <NewPageModal /> })}
            {move || ctx.ui.show_commit.get().then(|| view! { <crate::components::commit_modal::CommitModal /> })}
            {move || ctx.ui.git_history.get().map(|file| view! { <crate::components::git_history::GitHistoryModal file /> })}
            {move || ctx.ui.show_git_remote.get().then(|| view! { <crate::components::git_setup::RemoteModal /> })}
            {move || ctx.ui.show_clone.get().then(|| view! { <crate::components::git_setup::CloneModal /> })}
            {move || ctx.ui.show_git_identity.get().then(|| view! { <crate::components::git_setup::IdentityModal /> })}
            {move || ctx.ui.update.get().map(|info| view! { <crate::components::update_modal::UpdateModal info /> })}
            {move || ctx.ui.show_shortcuts.get().then(|| view! { <crate::components::shortcuts_help::ShortcutsHelp /> })}
            {move || ctx.ui.status.get().map(|msg| {
                // The "Undo" button of this message, if it has one
                let undo = ctx.ui.status_undo.get().filter(|(m, _)| *m == msg).map(|(_, f)| f);
                view! {
                <div class="status-bar">
                    <span>{msg}</span>
                    {undo.map(|f| view! {
                        <button class="status-undo" on:click=move |_| {
                            ctx.ui.status_undo.set(None);
                            ctx.ui.status.set(None);
                            f.run(());
                        }>{move || t("undo", ctx.lang.get())}</button>
                    })}
                    <button class="status-close" aria-label=move || t("close", ctx.lang.get()) on:click=move |_| ctx.ui.status.set(None)>"×"</button>
                </div>
            }})}
        </div>
    }
}
#[cfg(test)]
mod tests {
    use super::cycle;

    #[test]
    fn tabs_cycle_both_ways() {
        assert_eq!(cycle(Some(2), 3, 1), Some(0));
        assert_eq!(cycle(Some(0), 3, -1), Some(2));
        assert_eq!(cycle(None, 3, 1), Some(1));
        assert_eq!(cycle(Some(0), 0, 1), None);
    }
}
