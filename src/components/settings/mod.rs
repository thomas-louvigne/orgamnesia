//! The settings window: a draft of the settings and keybindings, edited in
//! tabs, applied or saved together.

pub mod fields;
mod layout;
mod shortcuts;
mod tabs;

use leptos::{ev, html, prelude::*};
use wasm_bindgen_futures::spawn_local;

use crate::{
    actions,
    i18n::{t, Lang},
    invoke,
    preferences::{self, File},
    state::{AppCtx, Prefs},
    storage,
};
use shortcuts::{KbDraft, KbSnapshot, ShortcutsTab};
use tabs::{DisplayTab, EditingTab, ExtensionsTab, GeneralTab, SpellingTab};

/// Everything the settings window edits, to tell whether there is anything to apply.
#[derive(Clone, PartialEq)]
struct Snapshot {
    prefs: Prefs,
    keybindings: KbSnapshot,
}

/// The tabs of the window, in the order they are shown.
#[derive(Clone, Copy, PartialEq)]
enum Tab {
    General,
    Editing,
    Display,
    Shortcuts,
    Spelling,
    Extensions,
}

impl Tab {
    const ALL: [Tab; 6] = [Tab::General, Tab::Editing, Tab::Display, Tab::Shortcuts, Tab::Spelling, Tab::Extensions];

    fn label(self) -> &'static str {
        match self {
            Tab::General => "tab_general",
            Tab::Editing => "tab_editing",
            Tab::Display => "tab_display",
            Tab::Shortcuts => "tab_shortcuts",
            Tab::Spelling => "tab_spelling",
            Tab::Extensions => "tab_extensions",
        }
    }
}

const SIZE_KEY: &str = "settings_modal_size";

/// Size (width, height in px) the settings window had last time, if remembered.
fn load_size() -> Option<(i32, i32)> {
    let s = storage::load(SIZE_KEY)?;
    let (w, h) = s.split_once(',')?;
    Some((w.parse().ok()?, h.parse().ok()?))
}

fn store_size(w: i32, h: i32) {
    storage::store(SIZE_KEY, &format!("{w},{h}"));
}

#[component]
pub fn SettingsModal() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();

    let panel = RwSignal::new(Tab::General);

    // The settings being edited (the saved ones replace these once read)
    let draft = RwSignal::new(ctx.prefs_untracked());
    let kb = KbDraft::new(ctx);
    let conflicts = kb.conflicts;

    // True when the current mouse press started on the overlay itself
    let overlay_down = RwSignal::new(false);

    // Drag state
    let modal_ref  = NodeRef::<html::Div>::new();
    let modal_x    = RwSignal::new(-1i32);
    let modal_y    = RwSignal::new(-1i32);
    let dragging   = RwSignal::new(false);
    let drag_off_x = RwSignal::new(0i32);
    let drag_off_y = RwSignal::new(0i32);

    let move_handle = window_event_listener(ev::mousemove, move |e: web_sys::MouseEvent| {
        if dragging.get() {
            modal_x.set(e.client_x() - drag_off_x.get());
            modal_y.set(e.client_y() - drag_off_y.get());
        }
    });
    // Remembered size: the CSS resize handle writes inline width/height on the
    // element, so read them back once the mouse is released and keep them.
    let size = RwSignal::new(load_size());
    let up_handle = window_event_listener(ev::mouseup, move |_| {
        dragging.set(false);
        if let Some(el) = modal_ref.get_untracked() {
            let rect = el.get_bounding_client_rect();
            let now = (rect.width().round() as i32, rect.height().round() as i32);
            if size.get_untracked() != Some(now) {
                size.set(Some(now));
                store_size(now.0, now.1);
            }
        }
    });
    let esc_handle = window_event_listener(ev::keydown, move |e| {
        if e.key() == "Escape" { ctx.ui.show_settings.set(false); }
    });
    on_cleanup(move || { move_handle.remove(); up_handle.remove(); esc_handle.remove(); });

    // What was last loaded/applied; the window differs from it when there is
    // something to apply. `None` until the saved settings have been read.
    let snapshot = move || Snapshot { prefs: draft.get(), keybindings: kb.snapshot() };
    let baseline = RwSignal::new(None::<Snapshot>);
    let has_changes = move || {
        baseline.with(|b| b.as_ref().is_some_and(|b| *b != snapshot()))
    };

    // Load current settings
    spawn_local(async move {
        if let Ok(s) = invoke::get_settings().await {
            draft.set(Prefs::from_settings(&s));
        }
        baseline.set(Some(untrack(snapshot)));
    });

    let on_header_mousedown = move |e: web_sys::MouseEvent| {
        if let Some(el) = modal_ref.get() {
            let rect = el.get_bounding_client_rect();
            drag_off_x.set(e.client_x() - rect.left() as i32);
            drag_off_y.set(e.client_y() - rect.top() as i32);
            modal_x.set(rect.left() as i32);
            modal_y.set(rect.top() as i32);
            dragging.set(true);
        }
        e.prevent_default();
    };

    let modal_style = move || {
        let mut style = size.get()
            .map(|(w, h)| format!("width: {w}px; height: {h}px; "))
            .unwrap_or_default();
        let x = modal_x.get();
        if x >= 0 {
            style.push_str(&format!("position: fixed; left: {}px; top: {}px; margin: 0;", x, modal_y.get()));
        }
        style
    };

    // Save everything; `close` tells whether the window closes afterwards ("Save")
    // or stays open ("Apply").
    let do_save = move |close: bool| {
        if !conflicts.get_untracked().is_empty() { return; }
        let applied = untrack(snapshot);
        let prefs = applied.prefs.clone();
        let keybindings = kb.to_keybindings(t("profile_custom", ctx.lang.get_untracked()));

        spawn_local(async move {
            let settings = prefs.to_settings(ctx.project.projects.get_untracked());
            match invoke::save_settings(&settings).await {
                Ok(_) => {
                    ctx.prefs.set(prefs.clone());
                    ctx.bump_links();
                    // Another project chosen (the same one would leave the "preference" project)
                    if let Some(path) = settings.vault_path.filter(|p| ctx.project.vault_path.get_untracked().as_ref() != Some(p)) {
                        actions::open_project(ctx, path).await;
                    }
                    baseline.set(Some(applied));
                    ctx.ui.status.set(Some(t("settings_saved", Lang::from_code(&prefs.lang)).to_string()));
                    if close { ctx.ui.show_settings.set(false); }
                }
                Err(e) => ctx.error("error", &e),
            }
            // The profiles come back named after their files
            match invoke::set_keybindings(&keybindings).await {
                Ok(mut kb) => {
                    kb.upgrade();
                    ctx.keybindings.set(kb);
                }
                Err(e) => ctx.error("keybindings_error", &e),
            }
            // The files open in the "preference" project show what was written
            preferences::refresh(ctx);
        });
    };

    let save  = move |_| do_save(true);
    let apply = move |_| do_save(false);

    // The settings as text, in the "preference" project: from the shortcuts' tab,
    // the file of the profile in effect when it is one of the user's (a built-in
    // one is in the app). What is not applied here is lost (asked first).
    let edit_as_text = move |_| {
        let active = ctx.keybindings.with_untracked(|kb| kb.resolved_active());
        let file = if panel.get_untracked() == Tab::Shortcuts && !crate::keybindings::is_builtin(&active) {
            File::Profile(active)
        } else {
            File::Settings
        };
        let open = move || {
            ctx.ui.show_settings.set(false);
            preferences::open(ctx, file.clone(), None);
        };
        if has_changes() {
            ctx.ask_confirm(t("pref_text_discard", ctx.lang.get_untracked()).to_string(), open);
        } else {
            open();
        }
    };

    view! {
        <div
            class="modal-overlay"
            on:mousedown=move |e: web_sys::MouseEvent| {
                overlay_down.set(e.target() == e.current_target());
            }
            on:click=move |_| {
                // Ignore clicks that started inside the modal (e.g. a resize drag
                // released outside of it)
                if overlay_down.get_untracked() { ctx.ui.show_settings.set(false) }
            }
        >
            <div
                node_ref=modal_ref
                class="modal modal-resizable"
                style=modal_style
                on:click=|e| e.stop_propagation()
            >
                <div
                    class="modal-header"
                    style=move || if dragging.get() {
                        "cursor: grabbing; user-select: none;"
                    } else {
                        "cursor: grab; user-select: none;"
                    }
                    on:mousedown=on_header_mousedown
                >
                    <h2>{move || t("settings", lang())}</h2>
                    <button
                        class="btn-close"
                        aria-label=move || t("close", lang())
                        on:click=move |_| ctx.ui.show_settings.set(false)
                        on:mousedown=|e: web_sys::MouseEvent| e.stop_propagation()
                    >"×"</button>
                </div>

                <div class="modal-main">
                <div class="modal-tabs">
                    {Tab::ALL.into_iter()
                        .map(|tab| view! {
                            <button
                                class=move || if panel.get() == tab { "modal-tab active" } else { "modal-tab" }
                                on:click=move |_| panel.set(tab)
                                on:mousedown=|e: web_sys::MouseEvent| e.stop_propagation()
                            >{move || t(tab.label(), lang())}</button>
                        })
                        .collect_view()}
                </div>

                <div class="modal-body">
                    {move || match panel.get() {
                        Tab::General => view! { <GeneralTab draft /> }.into_any(),
                        Tab::Editing => view! { <EditingTab draft /> }.into_any(),
                        Tab::Display => view! { <DisplayTab draft /> }.into_any(),
                        Tab::Shortcuts => view! { <ShortcutsTab draft kb /> }.into_any(),
                        Tab::Spelling => view! { <SpellingTab draft /> }.into_any(),
                        Tab::Extensions => view! { <ExtensionsTab draft /> }.into_any(),
                    }}
                </div>
                </div>

                <div class="modal-footer">
                    <button
                        class="btn-link modal-footer-start"
                        title=move || t("pref_edit_text_title", lang())
                        on:click=edit_as_text
                        on:mousedown=|e: web_sys::MouseEvent| e.stop_propagation()
                    >{move || t("pref_edit_text", lang())}</button>
                    <button
                        class="btn-secondary"
                        on:click=move |_| ctx.ui.show_settings.set(false)
                        on:mousedown=|e: web_sys::MouseEvent| e.stop_propagation()
                    >{move || t("cancel", lang())}</button>
                    <button
                        class=move || if has_changes() { "btn-secondary pending" } else { "btn-secondary" }
                        disabled=move || !has_changes() || !conflicts.get().is_empty()
                        title=move || if conflicts.get().is_empty() { "" } else { t("conflict_blocked", lang()) }
                        on:click=apply
                        on:mousedown=|e: web_sys::MouseEvent| e.stop_propagation()
                    >
                        {move || has_changes().then(|| view! { <span class="pending-dot">"●"</span> })}
                        {move || t("apply", lang())}
                    </button>
                    <button
                        class="btn-primary"
                        disabled=move || !conflicts.get().is_empty()
                        title=move || if conflicts.get().is_empty() { "" } else { t("conflict_blocked", lang()) }
                        on:click=save
                        on:mousedown=|e: web_sys::MouseEvent| e.stop_propagation()
                    >{move || t("save", lang())}</button>
                </div>
            </div>
        </div>
    }
}
