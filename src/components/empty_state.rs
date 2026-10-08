//! What the editor shows when no page is open: the way in. Without a project,
//! opening a folder of notes (or one opened before); with one, creating or going
//! to a page, and the few keys that matter, read from the active shortcut profile.

use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::{
    actions::{self, project_name},
    i18n::t,
    keybindings::display,
    state::AppCtx,
};

/// Projects opened before, offered on the first screen.
const RECENT_PROJECTS: usize = 4;

#[component]
pub fn EmptyState() -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");
    let lang = move || ctx.lang.get();
    // First shortcut of an application or editor action, as shown to the user
    let app_key = move |id: &str| ctx.keybindings.with(|kb| kb.app_binds(id).first().map(|b| display(b)));
    let editor_key = move |id: &str| ctx.keybindings.with(|kb| {
        kb.editor.get(id).and_then(|b| b.first()).map(|b| display(b))
    });

    view! {
        <div class="editor-empty">
            <img class="empty-logo" src="app-icon.svg" alt="" />
            {move || match ctx.project.vault_path.get() {
                None => {
                    let recent: Vec<String> = ctx.project.projects.get().into_iter().take(RECENT_PROJECTS).collect();
                    view! {
                        <h2 class="empty-title">{t("empty_no_project", lang())}</h2>
                        <p class="empty-text">{t("empty_no_project_hint", lang())}</p>
                        <div class="empty-actions">
                            <button class="btn-primary" on:click=move |_| spawn_local(actions::pick_and_open_project(ctx))>
                                {t("open_project", lang())}
                            </button>
                        </div>
                        {(!recent.is_empty()).then(|| view! {
                            <div class="empty-recent">
                                <h3 class="empty-label">{t("empty_recent", lang())}</h3>
                                <ul>
                                    {recent.into_iter().map(|path| {
                                        let open = path.clone();
                                        view! {
                                            <li>
                                                <button class="empty-recent-item" title=path.clone()
                                                    on:click=move |_| spawn_local(actions::open_project(ctx, open.clone()))>
                                                    {project_name(&path)}
                                                </button>
                                            </li>
                                        }
                                    }).collect_view()}
                                </ul>
                            </div>
                        })}
                    }.into_any()
                }
                Some(_) => {
                    let has_pages = !ctx.project.files.with(|f| f.is_empty());
                    // The few keys worth knowing first, those of the active profile (going to
                    // a page and creating one are the buttons above)
                    let keys: Vec<(&'static str, String)> = [
                        ("empty_key_fold", editor_key("table_next_cell").filter(|_| ctx.pref(|p| p.tab_folds))),
                        ("empty_key_link", editor_key("open_link")),
                        ("empty_key_find", editor_key("find")),
                        ("empty_key_region", app_key("next_region")),
                    ].into_iter().filter_map(|(label, key)| key.map(|k| (label, k))).collect();
                    let new_key = app_key("new_page");
                    let open_key = app_key("quick_open");
                    view! {
                        <h2 class="empty-title">{t(if has_pages { "empty_no_page" } else { "empty_no_pages" }, lang())}</h2>
                        <p class="empty-text">{t(if has_pages { "empty_no_page_hint" } else { "empty_no_pages_hint" }, lang())}</p>
                        <div class="empty-actions">
                            <button class="btn-primary" on:click=move |_| ctx.ui.show_new_page.set(true)>
                                {t("new_page_title", lang())}
                                {new_key.map(|k| view! { <kbd>{k}</kbd> })}
                            </button>
                            {has_pages.then(|| view! {
                                <button class="btn-secondary" on:click=move |_| ctx.ui.show_quick_open.set(true)>
                                    {t("empty_go_to_page", lang())}
                                    {open_key.map(|k| view! { <kbd>{k}</kbd> })}
                                </button>
                            })}
                        </div>
                        {(!keys.is_empty()).then(|| view! {
                            <dl class="empty-keys" aria-label=t("empty_keys", lang())>
                                {keys.into_iter().map(|(label, key)| view! {
                                    <dt><kbd>{key}</kbd></dt>
                                    <dd>{t(label, lang())}</dd>
                                }).collect_view()}
                            </dl>
                        })}
                    }.into_any()
                }
            }}
        </div>
    }
}
