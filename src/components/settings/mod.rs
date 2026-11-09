//! The settings, shown in the workspace over the pages (they stay mounted): a
//! draft of the settings and keybindings, edited in tabs, applied or saved together.

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
    state::{AppCtx, Prefs, Settings},
};
use shortcuts::{KbDraft, KbSnapshot, ShortcutsTab};
use tabs::{AboutTab, DisplayTab, ThemesTab, EditingTab, ExtensionsTab, GeneralTab, LinksTab, SpellingTab};

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
    Links,
    Editing,
    Display,
    Themes,
    Shortcuts,
    Spelling,
    Extensions,
    About,
}

impl Tab {
    const ALL: [Tab; 9] = [Tab::General, Tab::Links, Tab::Editing, Tab::Display, Tab::Themes, Tab::Shortcuts, Tab::Spelling, Tab::Extensions, Tab::About];

    fn label(self) -> &'static str {
        match self {
            Tab::General => "tab_general",
            Tab::Links => "tab_links",
            Tab::Editing => "tab_editing",
            Tab::Display => "tab_display",
            Tab::Themes => "tab_themes",
            Tab::Shortcuts => "tab_shortcuts",
            Tab::Spelling => "tab_spelling",
            Tab::Extensions => "tab_extensions",
            Tab::About => "tab_about",
        }
    }
}

/// Puts back the defaults (`d`) of the settings shown in `tab`; the project
/// folder stays, it is not a preference.
fn reset_tab(tab: Tab, p: &mut Prefs, d: Prefs) {
    macro_rules! take { ($($f:ident),*) => {{ $(p.$f = d.$f;)* }} }
    match tab {
        Tab::General => take!(restore_session, default_format, single_format, ask_format, check_updates, lang),
        Tab::Links => take!(org_links, md_links, case_insensitive_links, update_links, hashtag_links,
            hashtag_dashes, org_tag_links, org_tag_dashes, mark_broken_links, link_preview, link_preview_trigger),
        Tab::Editing => take!(autosave, electric_mode, find_match_case, indent_headings, tab_folds, todo_enabled,
            todo_keywords, delete_empty, delete_title_only, trash_auto_empty, trash_keep_days, trash_max_bytes),
        Tab::Display => take!(show_page_title, show_line_numbers, show_images, image_size_format,
            show_video_thumbnails, hide_leading_stars, layout, show_brand, show_project, show_git, show_pages,
            show_backlinks, show_tags, show_broken_links, show_todos, show_org_bloom, show_quit_button,
            show_settings_button),
        Tab::Themes => take!(theme),
        Tab::Shortcuts => take!(emacs_mark, webkit_emoji_shortcuts),
        Tab::Spelling => take!(spell, spell_lang, spell_dict_dir),
        Tab::Extensions => take!(org_bloom, org_bloom_path, org_bloom_zip, org_bloom_no_external_deps, git_ext,
            git_ssh_key, git_fetch, git_pull_rebase, git_unpushed_marker, git_commit_push, git_commit_template,
            git_auto_sync, git_sync_on_quit),
        Tab::About => {}
    }
}

#[component]
pub fn SettingsPage() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();

    let panel = RwSignal::new(Tab::General);

    // The settings being edited (the saved ones replace these once read)
    let draft = RwSignal::new(ctx.prefs_untracked());
    let kb = KbDraft::new(ctx);
    let conflicts = kb.conflicts;

    let esc_handle = window_event_listener(ev::keydown, move |e| {
        if e.key() == "Escape" { ctx.ui.show_settings.set(false); }
    });
    on_cleanup(move || esc_handle.remove());

    let page_ref = NodeRef::<html::Div>::new();
    super::panel::opened(ctx, page_ref, move || ctx.ui.show_settings.set(false));

    // What was last loaded/applied; the page differs from it when there is
    // something to apply. `None` until the saved settings have been read.
    let snapshot = move || Snapshot { prefs: draft.get(), keybindings: kb.snapshot() };
    let baseline = RwSignal::new(None::<Snapshot>);
    let has_changes = move || {
        baseline.with(|b| b.as_ref().is_some_and(|b| *b != snapshot()))
    };

    // Load current settings
    spawn_local(async move {
        if let Ok(s) = invoke::get_settings().await {
            draft.set(preferences::prefs_of(&s));
        }
        baseline.set(Some(untrack(snapshot)));
    });

    // Save everything; `close` tells whether the page closes afterwards ("Save")
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

    // The current tab back to its defaults, in the draft (applied like any edit)
    let reset = move |_| {
        let tab = panel.get_untracked();
        draft.update(|p| reset_tab(tab, p, preferences::prefs_of(&Settings::default())));
        if tab == Tab::Shortcuts { kb.reset(); }
    };

    view! {
        <div node_ref=page_ref class="main-panel" tabindex="-1">
            <div class="modal-header">
                <h2>{move || t("settings", lang())}</h2>
                <button
                    class="btn-close"
                    aria-label=move || t("close", lang())
                    on:click=move |_| ctx.ui.show_settings.set(false)
                >"×"</button>
            </div>

            <div class="modal-main">
                <div class="modal-tabs">
                    {Tab::ALL.into_iter()
                        .map(|tab| view! {
                            <button
                                class=move || if panel.get() == tab { "modal-tab active" } else { "modal-tab" }
                                on:click=move |_| panel.set(tab)
                            >{move || t(tab.label(), lang())}</button>
                        })
                        .collect_view()}
                </div>

                <div class="modal-body">
                    {move || match panel.get() {
                        Tab::General => view! { <GeneralTab draft /> }.into_any(),
                        Tab::Links => view! { <LinksTab draft /> }.into_any(),
                        Tab::Editing => view! { <EditingTab draft /> }.into_any(),
                        Tab::Display => view! { <DisplayTab draft /> }.into_any(),
                        Tab::Themes => view! { <ThemesTab draft /> }.into_any(),
                        Tab::Shortcuts => view! { <ShortcutsTab draft kb /> }.into_any(),
                        Tab::Spelling => view! { <SpellingTab draft /> }.into_any(),
                        Tab::Extensions => view! { <ExtensionsTab draft /> }.into_any(),
                        Tab::About => view! { <AboutTab /> }.into_any(),
                    }}
                </div>
            </div>

            <div class="modal-footer">
                <button
                    class="btn-link modal-footer-start"
                    title=move || t("pref_edit_text_title", lang())
                    on:click=edit_as_text
                >{move || t("pref_edit_text", lang())}</button>
                {move || (panel.get() != Tab::About).then(|| view! {
                    <button
                        class="btn-secondary"
                        title=move || t("reset_tab_title", lang())
                        on:click=reset
                    >{move || t("reset_tab", lang())}</button>
                })}
                <button
                    class="btn-secondary"
                    on:click=move |_| ctx.ui.show_settings.set(false)
                >{move || t("cancel", lang())}</button>
                <button
                    class=move || if has_changes() { "btn-secondary pending" } else { "btn-secondary" }
                    disabled=move || !has_changes() || !conflicts.get().is_empty()
                    title=move || if conflicts.get().is_empty() { "" } else { t("conflict_blocked", lang()) }
                    on:click=apply
                >
                    {move || has_changes().then(|| view! { <span class="pending-dot">"●"</span> })}
                    {move || t("apply", lang())}
                </button>
                <button
                    class="btn-primary"
                    disabled=move || !conflicts.get().is_empty()
                    title=move || if conflicts.get().is_empty() { "" } else { t("conflict_blocked", lang()) }
                    on:click=save
                >{move || t("save", lang())}</button>
            </div>
        </div>
    }
}
