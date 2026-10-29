use leptos::{ev, prelude::*};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

use crate::{
    actions,
    components::{
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
    state::{AppCtx, Drag, Session, SplitKind, Tab, VaultChanges},
    storage,
    text_field,
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

/// In-app yes/no dialog (or message with OK) driven by `ctx.ui.confirm`.
#[component]
fn ConfirmDialog() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();
    let close = move || ctx.ui.confirm.set(None);
    let accept = move || {
        if let Some(req) = ctx.ui.confirm.get_untracked() {
            ctx.ui.confirm.set(None);
            req.on_yes.run(());
        }
    };
    // A destructive action is not done by Enter: Cancel has the focus, Enter presses it
    let danger = ctx.ui.confirm.get_untracked().and_then(|r| r.danger);
    let info = ctx.ui.confirm.get_untracked().is_some_and(|r| r.info);
    // A choice: the first button, in place of Cancel, and the label of the second
    let choice = ctx.ui.confirm.get_untracked().and_then(|r| r.choice);
    let no = move || {
        close();
        if let Some((_, on_no, _)) = choice { on_no.run(()); }
    };
    let keys = window_event_listener(ev::keydown, move |e: web_sys::KeyboardEvent| {
        match e.key().as_str() {
            "Escape" => { e.stop_propagation(); close(); }
            "Enter" if danger.is_none() => { e.stop_propagation(); accept(); }
            _ => {}
        }
    });
    on_cleanup(move || keys.remove());
    let cancel_ref = NodeRef::<leptos::html::Button>::new();
    Effect::new(move |_| {
        if danger.is_some() && let Some(b) = cancel_ref.get() { let _ = b.focus(); }
    });

    view! {
        <div class="alert-overlay" on:click=move |_| close()>
            <div class="alert-box confirm" role="alertdialog" on:click=|e| e.stop_propagation()>
                <p>{move || ctx.ui.confirm.get().map(|r| r.message).unwrap_or_default()}</p>
                <div class="alert-buttons">
                    {(!info).then(|| view! {
                        <button class="btn-secondary" node_ref=cancel_ref on:click=move |_| no()>
                            {move || t(choice.map_or("cancel", |(label, _, _)| label), lang())}
                        </button>
                    })}
                    <button class=if danger.is_some() { "btn-danger" } else { "btn-primary" } on:click=move |_| accept()>
                        {move || t(if info { "ok" } else { choice.map(|(_, _, yes)| yes).or(danger).unwrap_or("confirm") }, lang())}
                    </button>
                </div>
            </div>
        </div>
    }
}

/// A page changed on disk while it has unsaved changes: keep the text being
/// edited, or take the one on disk. Escape keeps the text being edited.
#[component]
fn DiskConflictDialog() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();
    let settle = move |take_disk: bool| spawn_local(actions::settle_disk_conflict(ctx, take_disk));
    let keys = window_event_listener(ev::keydown, move |e: web_sys::KeyboardEvent| {
        if e.key() == "Escape" {
            e.stop_propagation();
            settle(false);
        }
    });
    on_cleanup(move || keys.remove());
    // The keyboard leaves the editor: typing on would change the text being decided on
    let keep_ref = NodeRef::<leptos::html::Button>::new();
    Effect::new(move |_| if let Some(b) = keep_ref.get() { let _ = b.focus(); });
    let name = move || {
        let path = ctx.ui.disk_conflicts.with(|q| q.first().cloned()).unwrap_or_default();
        ctx.work.tabs.with(|tabs| tabs.iter().find(|t| t.path == path).map(|t| t.name.clone())).unwrap_or_default()
    };
    view! {
        <div class="alert-overlay">
            <div class="alert-box confirm" role="alertdialog">
                <h3 class="disk-conflict-title">{move || t("disk_conflict_title", lang())}</h3>
                <p>{move || format!("« {} » {}", name(), t("disk_conflict", lang()))}</p>
                <div class="alert-buttons">
                    <button class="btn-secondary" node_ref=keep_ref on:click=move |_| settle(false)>{move || t("disk_keep_mine", lang())}</button>
                    <button class="btn-primary" on:click=move |_| settle(true)>{move || t("disk_take", lang())}</button>
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
    owner.with(|| crate::preferences::watch(ctx));
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
        // The "preference" project's files are no pages of the project: the session
        // stays the one of the project put aside
        if ctx.project.preference.get() { return; }
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
    // And a look every second, for the changes the watcher misses
    fn poll(ctx: AppCtx) {
        crate::keybindings::after_ms(1000, move || spawn_local(async move {
            if ctx.project.vault_path.try_get_untracked().flatten().is_some()
                && let Ok(Some(changes)) = invoke::check_disk().await
            {
                actions::apply_disk_changes(ctx, changes).await;
            }
            poll(ctx);
        }));
    }
    poll(ctx);
    // Files dropped on the window: the editor under them adds the images
    invoke::listen(invoke::DRAG_DROP, move |d: invoke::Dropped| {
        let n = ctx.ui.dropped.with_untracked(|d| d.as_ref().map_or(0, |(n, _)| n + 1));
        ctx.ui.dropped.set(Some((n, d)));
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
                // Unchanged, it isn't set again: the git frame would be updated for nothing
                if ctx.project.vault_path.get_untracked().as_deref() == Some(path.as_str())
                    && ctx.project.git.with_untracked(|g| *g != st)
                {
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

/// The remote fetched once the project is known to have one, then every few
/// minutes; the automatic sync every so many minutes, as set.
fn git_timers(ctx: AppCtx) {
    use std::time::Duration;
    // Another project: its sync isn't held back by the last one's failure
    Effect::new(move |_| {
        ctx.project.vault_path.track();
        ctx.project.git_sync_paused.set(false);
    });
    let fetchable = Memo::new(move |_| (
        ctx.project.vault_path.get(),
        ctx.project.git.with(|g| g.as_ref().is_some_and(|g| g.repo && g.remote)),
    ));
    let fetch_minutes = Memo::new(move |_| ctx.pref(|p| p.git_fetch));
    Effect::new(move |_| if fetchable.get().1 && fetch_minutes.get() > 0 { crate::git::fetch(ctx); });
    // Started again whenever the setting changes; 0: never
    let every = move |minutes: u64, job: fn(AppCtx)| (minutes > 0).then(|| {
        set_interval_with_handle(move || job(ctx), Duration::from_secs(minutes * 60)).ok()
    }).flatten();
    Effect::new(move |prev: Option<Option<IntervalHandle>>| {
        if let Some(Some(tick)) = prev { tick.clear(); }
        every(fetch_minutes.get(), crate::git::fetch)
    });
    Effect::new(move |prev: Option<Option<IntervalHandle>>| {
        if let Some(Some(tick)) = prev { tick.clear(); }
        every(ctx.pref(|p| p.git_auto_sync), crate::git::auto_sync)
    });
}

/// Another project: the project search starts afresh (its undo was the other project's).
fn reset_search(ctx: AppCtx) {
    Effect::new(move |_| {
        ctx.project.vault_path.track();
        let s = ctx.ui.search;
        s.results.set(Default::default());
        s.skip.set(Default::default());
        s.can_undo.set(false);
    });
}

/// Org-mode tags of the project, for the tags frame and the completion of
/// `:tags:`. Refreshed (debounced) when the project, its pages or their links change.
fn watch_tags(ctx: AppCtx) {
    let seq = StoredValue::new(0u32);
    Effect::new(move |_| {
        ctx.project.files.track();
        ctx.project.links_version.track();
        if ctx.project.vault_path.get().is_none() {
            ctx.project.tags.set(vec![]);
            return;
        }
        let n = seq.get_value() + 1;
        seq.set_value(n);
        crate::keybindings::after_ms(400, move || {
            if seq.try_get_value() != Some(n) { return; }
            spawn_local(async move {
                if let Ok(list) = invoke::list_tags().await
                    && ctx.project.tags.get_untracked() != list { ctx.project.tags.set(list); }
            });
        });
    });
}

/// Run an application action (shortcut).
pub fn run_app_action(ctx: AppCtx, action: AppAction) {
    // What acts on the pages of a project has nothing to act on in the "preference" one
    let project_only = matches!(action, AppAction::NewPage | AppAction::ProjectSearch | AppAction::ProjectReplace
        | AppAction::AddTag | AppAction::GitCommit | AppAction::GitPull | AppAction::GitPush);
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

    spawn_local(load_startup(ctx, Owner::current().expect("App owner")));
    watch_disk(ctx);
    watch_git(ctx);
    git_timers(ctx);
    crate::update::at_start(ctx);
    watch_tags(ctx);
    reset_search(ctx);

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
            {move || ctx.ui.confirm.get().is_some().then(|| view! { <ConfirmDialog /> })}
            {move || ctx.ui.disk_conflicts.with(|q| !q.is_empty()).then(|| view! { <DiskConflictDialog /> })}
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
