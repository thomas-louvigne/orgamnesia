use leptos::{ev, prelude::*};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

use crate::{
    actions,
    components::{
        backlinks_panel::BacklinksPanel,
        editor::EditorSplit,
        quick_open::QuickOpenModal,
        settings_modal::SettingsModal,
        sidebar::{NewPageModal, Sidebar},
        tabs::TabBar,
    },
    i18n::t,
    invoke,
    keybindings::{AppAction, Resolution, Scope},
    state::{AppCtx, Drag, Session, SplitKind, Tab, VaultChanges},
    storage,
};

const SIDEBAR_KEY: &str = "sidebar_width";
const PANEL_KEY: &str = "panel_width";
const BROKEN_KEY: &str = "broken_height";
const MIN_SIDE: i32 = 140;
const MIN_BROKEN: i32 = 40;

fn load_size(key: &str, default: i32) -> i32 {
    storage::load(key).and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// Draggable bar between the pages menu / backlinks panel and the workspace.
#[component]
fn Resizer(which: Drag) -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");
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

/// In-app yes/no dialog driven by `ctx.ui.confirm`.
#[component]
fn ConfirmDialog() -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");
    let lang = move || ctx.lang.get();
    let close = move || ctx.ui.confirm.set(None);
    let accept = move || {
        if let Some(req) = ctx.ui.confirm.get_untracked() {
            ctx.ui.confirm.set(None);
            req.on_yes.run(());
        }
    };
    let keys = window_event_listener(ev::keydown, move |e: web_sys::KeyboardEvent| {
        match e.key().as_str() {
            "Escape" => { e.stop_propagation(); close(); }
            "Enter"  => { e.stop_propagation(); accept(); }
            _ => {}
        }
    });
    on_cleanup(move || keys.remove());

    view! {
        <div class="alert-overlay" on:click=move |_| close()>
            <div class="alert-box confirm" role="alertdialog" on:click=|e| e.stop_propagation()>
                <p>{move || ctx.ui.confirm.get().map(|r| r.message).unwrap_or_default()}</p>
                <div class="alert-buttons">
                    <button class="btn-secondary" on:click=move |_| close()>{move || t("cancel", lang())}</button>
                    <button class="btn-primary" on:click=move |_| accept()>{move || t("confirm", lang())}</button>
                </div>
            </div>
        </div>
    }
}

/// Load the saved settings, project and keybindings, and the pages and panes
/// open when the app was last quit. `owner` is the app's: the tabs' signals belong to it.
async fn load_startup(ctx: AppCtx, owner: Owner) {
    if let Ok(settings) = invoke::get_settings().await {
        let prefs = settings.prefs();
        ctx.prefs.set(prefs.clone());
        ctx.project.projects.set(settings.projects);
        if let Some(path) = settings.vault_path {
            match invoke::open_vault(&path).await {
                Ok(files) => {
                    ctx.project.vault_path.set(Some(path.clone()));
                    ctx.project.files.set(files);
                    if let Some(session) = settings.session.filter(|s| prefs.restore_session && s.project == path) {
                        restore_session(ctx, &owner, session).await;
                    }
                }
                Err(e) => ctx.error("open_project_error", &e),
            }
        }
    }
    if let Ok(mut kb) = invoke::get_keybindings().await {
        kb.upgrade();
        ctx.keybindings.set(kb);
    }
    track_session(ctx, &owner);
}

/// Open again the tabs and panes of `session`; pages deleted since are left out.
async fn restore_session(ctx: AppCtx, owner: &Owner, session: Session) {
    let files = ctx.project.files.get_untracked();
    let mut tabs = vec![];
    for path in &session.tabs {
        let Some(file) = files.iter().find(|f| &f.path == path) else { continue };
        if let Ok(content) = invoke::read_file(path).await {
            tabs.push(owner.with(|| Tab::new(file, content, false)));
        }
    }
    let index = |page: &Option<String>| page.as_ref().and_then(|p| tabs.iter().position(|t| &t.path == p));
    let active = index(&session.active).or((!tabs.is_empty()).then_some(0));
    let other = index(&session.other).map(|i| tabs[i].path.clone());
    let work = ctx.work;
    work.tabs.set(tabs);
    work.active_tab.set(active);
    if let (Some(kind), Some(other), Some(_)) = (session.split, other, active) {
        work.other_path.set(Some(other));
        work.focus_second.set(session.focus_second);
        work.split.set(Some(kind));
    }
}

/// Keep the backend told of the open pages and panes, which it saves on quit.
/// Started after the restore, so the saved session isn't replaced by an empty one.
fn track_session(ctx: AppCtx, owner: &Owner) {
    owner.with(|| Effect::new(move |_| {
        let work = ctx.work;
        let session = ctx.project.vault_path.get().map(|project| Session {
            project,
            tabs: work.tabs.with(|tabs| tabs.iter().map(|t| t.path.clone()).collect()),
            active: work.active_tab_data().map(|t| t.path),
            split: work.split.get(),
            other: work.other_path.get(),
            focus_second: work.focus_second.get(),
        });
        spawn_local(async move {
            let _ = invoke::set_session(session.as_ref()).await;
        });
    }));
}

/// Pages changed on disk by another program (another editor, a sync tool…),
/// as the backend's watcher reports them.
fn watch_disk(ctx: AppCtx) {
    invoke::listen(invoke::VAULT_CHANGED, move |changes: VaultChanges| {
        spawn_local(actions::apply_disk_changes(ctx, changes));
    });
}

/// Git extension: read the git state of the project when it is opened or the
/// extension enabled, then whenever the watcher sees the project or its
/// repository change (edits, commits, pushes made from elsewhere).
fn watch_git(ctx: AppCtx) {
    let busy = StoredValue::new(false);
    let again = StoredValue::new(false);
    let refresh = move || {
        let Some(path) = ctx.project.vault_path.get_untracked().filter(|_| ctx.pref_untracked(|p| p.git_ext)) else {
            ctx.project.git.set(None);
            return;
        };
        // One read at a time; a change during a read triggers one more
        if busy.get_value() { again.set_value(true); return; }
        busy.set_value(true);
        spawn_local(async move {
            loop {
                again.set_value(false);
                let st = invoke::git_status(&path).await.ok().flatten();
                // Another project may have been opened meanwhile
                if ctx.project.vault_path.get_untracked().as_deref() == Some(path.as_str()) {
                    ctx.project.git.set(st);
                }
                if !again.get_value() { break; }
            }
            busy.set_value(false);
        });
    };
    // Only when the extension is switched, not on every change of the settings
    let enabled = Memo::new(move |_| ctx.pref(|p| p.git_ext));
    Effect::new(move |_| {
        ctx.project.vault_path.track();
        enabled.track();
        ctx.project.git.set(None);
        refresh();
    });
    invoke::listen(invoke::GIT_CHANGED, move |()| refresh());
}

/// Run an application action (shortcut).
fn run_app_action(ctx: AppCtx, action: AppAction) {
    let tab_count = ctx.work.tabs.with_untracked(|t| t.len());
    let active = ctx.work.active_tab.get_untracked();
    match action {
        AppAction::NewPage      => ctx.ui.show_new_page.set(true),
        AppAction::OpenSettings => ctx.ui.show_settings.set(true),
        AppAction::QuickOpen    => ctx.ui.show_quick_open.set(true),
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
        AppAction::CloseSplit      => ctx.work.close_window(),
        AppAction::SingleWindow    => ctx.work.single_window(),
        AppAction::OtherWindow     => ctx.work.other_window(),
        AppAction::Quit => {
            let quit = move || spawn_local(async move {
                if let Err(err) = invoke::quit_app().await {
                    ctx.error("quit_error", &err);
                }
            });
            if ctx.work.has_unsaved_tabs() {
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
            Drag::Broken => {
                // Height of the list = what is left under the bar, minus the block's title
                let header = win.as_ref().and_then(|w| w.document())
                    .and_then(|d| d.query_selector(".broken-links-header").ok().flatten())
                    .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok())
                    .map(|el| el.offset_height()).unwrap_or(30);
                broken_h.set((win_h - e.client_y() - header).clamp(MIN_BROKEN, (win_h * 7 / 10).max(MIN_BROKEN)));
            }
        }
    });
    let drag_up = window_event_listener(ev::mouseup, move |_| {
        let Some(which) = ctx.ui.drag.get_untracked() else { return };
        let (key, size) = match which {
            Drag::Sidebar => (SIDEBAR_KEY, sidebar_w),
            Drag::Panel => (PANEL_KEY, panel_w),
            Drag::Broken => (BROKEN_KEY, broken_h),
        };
        storage::store(key, &size.get_untracked().to_string());
        ctx.ui.drag.set(None);
    });
    on_cleanup(move || { drag_move.remove(); drag_up.remove(); });

    spawn_local(load_startup(ctx, Owner::current().expect("App owner")));
    watch_disk(ctx);
    watch_git(ctx);

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

    let export = move |_| {
        spawn_local(async move {
            ctx.notify_t("export_running", "");
            match invoke::export_vault().await {
                Ok(_) => ctx.notify_t("export_done", ""),
                Err(e) => ctx.error("export_error", &e),
            }
        });
    };

    view! {
        <div class=move || if ctx.ui.drag.get().is_some() { "app resizing" } else { "app" }>
            <div
                class="app-main"
                style=move || format!(
                    "--sidebar-w: {}px; --panel-w: {}px; --broken-h: {}px;",
                    sidebar_w.get(), panel_w.get(), broken_h.get()
                )
            >
                {move || ctx.pref(|p| p.show_pages).then(|| view! {
                    <Sidebar />
                    <Resizer which=Drag::Sidebar />
                })}
                <div class="workspace">
                    // Side by side, each pane has its own tabs under its title (see EditorArea)
                    {move || {
                        let split = ctx.work.split.get();
                        (split != Some(SplitKind::Vertical) || ctx.pref(|p| p.site_builder)).then(|| view! {
                            <div class="workspace-topbar">
                                {match split {
                                    None => view! { <TabBar /> }.into_any(),
                                    Some(SplitKind::Horizontal) => view! { <TabBar pane=false /> }.into_any(),
                                    Some(SplitKind::Vertical) => ().into_any(),
                                }}
                                {move || ctx.pref(|p| p.site_builder).then(|| view! {
                                    <button
                                        class="btn-export"
                                        title={move || t("export_title", ctx.lang.get())}
                                        on:click=export
                                    >
                                        {move || t("export", ctx.lang.get())}
                                    </button>
                                })}
                            </div>
                        })
                    }}
                    <EditorSplit />
                </div>
                {move || ctx.has_right_panel().then(|| view! {
                    <Resizer which=Drag::Panel />
                    <BacklinksPanel />
                })}
            </div>
            // Quit, in the top right corner of the window (Display setting)
            {move || ctx.pref(|p| p.show_quit_button).then(|| view! {
                <button
                    class="btn-quit"
                    title=move || {
                        let label = t("quit_button_title", ctx.lang.get());
                        match ctx.keybindings.with(|kb| kb.app_binds("quit").first().cloned()) {
                            Some(b) => format!("{label} ({})", crate::keybindings::display(&b)),
                            None => label.to_string(),
                        }
                    }
                    on:click=move |_| run_app_action(ctx, AppAction::Quit)
                >
                    <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor"
                        stroke-width="2.2" stroke-linecap="round" aria-hidden="true">
                        <path d="M12 3v8" />
                        <path d="M6.3 6.8a8 8 0 1 0 11.4 0" />
                    </svg>
                </button>
            })}
            {move || ctx.ui.show_quick_open.get().then(|| view! { <QuickOpenModal /> })}
            {move || ctx.ui.show_settings.get().then(|| view! { <SettingsModal /> })}
            {move || ctx.ui.confirm.get().is_some().then(|| view! { <ConfirmDialog /> })}
            {move || ctx.ui.show_new_page.get().then(|| view! { <NewPageModal /> })}
            {move || ctx.ui.status.get().map(|msg| view! {
                <div class="status-bar">
                    <span>{msg}</span>
                    <button class="status-close" on:click=move |_| ctx.ui.status.set(None)>"×"</button>
                </div>
            })}
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
