//! The tabs of the settings window, but the shortcuts one (see `shortcuts`).

use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use super::{fields::{Check, NumberField, PathField}, layout::LayoutEditor};
use crate::{i18n::t, invoke, state::{AppCtx, Prefs}};
use orgamnesia_core::SiteConfig;

/// Project, links and language.
#[component]
pub fn GeneralTab(draft: RwSignal<Prefs>) -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");
    let lang = move || ctx.lang.get();
    view! {
        <div class="tab-content">
            <div class="setting-row">
                <label>{move || t("vault_folder", lang())}</label>
                <PathField
                    value=Signal::derive(move || draft.with(|p| p.vault.clone()))
                    on_change=Callback::new(move |v| draft.update(|p| p.vault = v))
                    placeholder=Signal::derive(move || t("vault_hint", lang()).to_string())
                />
            </div>
            <Check draft get=|p| p.restore_session set=|p, v| p.restore_session = v label="restore_session" />
            <Check draft get=|p| p.case_insensitive_links set=|p, v| p.case_insensitive_links = v label="ci_links" />
            <Check draft get=|p| p.hashtag_links set=|p, v| p.hashtag_links = v label="hashtag_links" />
            <Check draft get=|p| p.hashtag_dashes set=|p, v| p.hashtag_dashes = v
                label="hashtag_dashes" hint="hashtag_dashes_hint" sub=true
                disabled=|p| !p.hashtag_links />
            <Check draft get=|p| p.org_tag_links set=|p, v| p.org_tag_links = v
                label="org_tag_links" hint="org_tag_links_hint" />
            <Check draft get=|p| p.org_tag_dashes set=|p, v| p.org_tag_dashes = v
                label="org_tag_dashes" hint="org_tag_dashes_hint" sub=true />
            <Check draft get=|p| p.update_links set=|p, v| p.update_links = v label="update_links" />

            <div class="setting-row">
                <label>{move || t("language", lang())}</label>
                <select
                    prop:value=move || draft.with(|p| p.lang.clone())
                    on:change=move |e| draft.update(|p| p.lang = event_target_value(&e))
                >
                    <option value="fr">{move || t("lang_fr", lang())}</option>
                    <option value="en">{move || t("lang_en", lang())}</option>
                </select>
            </div>
        </div>
    }
}

/// Saving, editing helpers, TODO keywords, deleting pages and the trash.
#[component]
pub fn EditingTab(draft: RwSignal<Prefs>) -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");
    let lang = move || ctx.lang.get();
    view! {
        <div class="tab-content">
            <Check draft get=|p| p.autosave set=|p, v| p.autosave = v label="autosave" />
            <Check draft get=|p| p.electric_mode set=|p, v| p.electric_mode = v label="electric_mode" />
            <Check draft get=|p| p.find_match_case set=|p, v| p.find_match_case = v label="find_match_case" />
            <Check draft get=|p| p.indent_headings set=|p, v| p.indent_headings = v
                label="indent_headings" hint="indent_headings_hint" />
            <Check draft get=|p| p.tab_folds set=|p, v| p.tab_folds = v
                label="tab_folds" hint="tab_folds_hint" />
            <Check draft get=|p| p.todo_enabled set=|p, v| p.todo_enabled = v
                label="todo_enabled" hint="todo_enabled_hint" />
            <div class="setting-row setting-sub">
                <label>{move || t("todo_keywords", lang())}</label>
                <input
                    type="text"
                    class="setting-input"
                    spellcheck="false"
                    prop:disabled=move || !draft.with(|p| p.todo_enabled)
                    placeholder=orgamnesia_core::settings::DEFAULT_TODO_KEYWORDS
                    prop:value=move || draft.with(|p| p.todo_keywords.clone())
                    on:input=move |e| draft.update(|p| p.todo_keywords = event_target_value(&e))
                />
                <span class="setting-hint">{move || t("todo_keywords_hint", lang())}</span>
            </div>
            <Check draft get=|p| p.delete_empty set=|p, v| p.delete_empty = v label="delete_empty_pages" />
            <Check draft get=|p| p.delete_title_only set=|p, v| p.delete_title_only = v label="delete_title_only_pages" />
            <TrashSettings draft />
        </div>
    }
}

/// What the editor shows, and where the frames go around it.
#[component]
pub fn DisplayTab(draft: RwSignal<Prefs>) -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");
    let lang = move || ctx.lang.get();
    view! {
        <div class="tab-content">
            <div class="setting-section-title">
                {move || t("display_appearance", lang())}
            </div>
            <Check draft get=|p| p.show_page_title set=|p, v| p.show_page_title = v
                label="show_page_title" hint="show_page_title_hint" />
            <Check draft get=|p| p.show_line_numbers set=|p, v| p.show_line_numbers = v
                label="show_line_numbers" />
            <Check draft get=|p| p.mark_broken_links set=|p, v| p.mark_broken_links = v
                label="mark_broken_links" hint="mark_broken_links_hint" />
            <Check draft get=|p| p.show_images set=|p, v| p.show_images = v
                label="show_images" hint="show_images_hint" />
            <div class="setting-row setting-sub">
                <label>{move || t("image_size_format", lang())}</label>
                <select
                    prop:disabled=move || !draft.with(|p| p.show_images)
                    prop:value=move || draft.with(|p| p.image_size_format.clone())
                    on:change=move |e| draft.update(|p| p.image_size_format = event_target_value(&e))
                >
                    <option value="org">{move || t("image_size_org", lang())}</option>
                    <option value="logseq">{move || t("image_size_logseq", lang())}</option>
                </select>
            </div>
            <Check draft get=|p| p.hide_leading_stars set=|p, v| p.hide_leading_stars = v
                label="hide_leading_stars" hint="hide_leading_stars_hint" />
            <div class="setting-section-title">
                {move || t("display_frames", lang())}
            </div>
            <LayoutEditor draft />
        </div>
    }
}

/// The org-bloom and git extensions.
#[component]
pub fn ExtensionsTab(draft: RwSignal<Prefs>) -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");
    let lang = move || ctx.lang.get();
    view! {
        <div class="tab-content">
            <Check draft get=|p| p.org_bloom set=|p, v| p.org_bloom = v
                label="org_bloom_enabled" hint="org_bloom_hint" />
            <div class="setting-row setting-sub">
                <label>{move || t("org_bloom_path", lang())}</label>
                <PathField
                    value=Signal::derive(move || draft.with(|p| p.org_bloom_path.clone()))
                    on_change=Callback::new(move |v| draft.update(|p| p.org_bloom_path = v))
                    placeholder=Signal::derive(|| "org-bloom".to_string())
                    disabled=Signal::derive(move || !draft.with(|p| p.org_bloom))
                    file_in="~/.local/bin"
                />
            </div>
            <Check draft get=|p| p.org_bloom_zip set=|p, v| p.org_bloom_zip = v
                label="org_bloom_zip" hint="org_bloom_zip_hint" sub=true
                disabled=|p| !p.org_bloom />
            <Check draft get=|p| p.org_bloom_no_external_deps set=|p, v| p.org_bloom_no_external_deps = v
                label="org_bloom_no_external_deps" hint="org_bloom_no_external_deps_hint" sub=true
                disabled=|p| !p.org_bloom />
            <SiteFolder draft />
            <hr class="setting-separator" />
            <Check draft get=|p| p.git_ext set=|p, v| p.git_ext = v
                label="git_ext_enabled" hint="git_ext_hint" />
            <div class="setting-row setting-sub">
                <label>{move || t("git_ssh_key", lang())}</label>
                <PathField
                    value=Signal::derive(move || draft.with(|p| p.git_ssh_key.clone()))
                    on_change=Callback::new(move |v| draft.update(|p| p.git_ssh_key = v))
                    placeholder=Signal::derive(move || t("git_ssh_key_ph", lang()).to_string())
                    disabled=Signal::derive(move || !draft.with(|p| p.git_ext))
                    file_in="~/.ssh"
                />
                <div class="setting-hint ssh-help">
                    <p>{move || t("git_ssh_help_where", lang())}</p>
                    <p>{move || t("git_ssh_help_create", lang())}</p>
                    <code>"ssh-keygen -t ed25519 -C \"you@example.com\""</code>
                    <p>{move || t("git_ssh_help_add", lang())}</p>
                    <p>{move || t("git_ssh_help_agent", lang())}</p>
                    <code>"ssh-add ~/.ssh/id_ed25519"</code>
                    <p>{move || t("git_ssh_help_https", lang())}</p>
                </div>
            </div>
        </div>
    }
}

/// The project's trash: how many pages it holds, a button to empty it, and the
/// setting to empty it automatically.
#[component]
fn TrashSettings(draft: RwSignal<Prefs>) -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");
    let lang = move || ctx.lang.get();
    // None: no project open
    let count = RwSignal::new(None::<usize>);
    Effect::new(move |_| {
        if ctx.project.vault_path.get().is_none() { count.set(None); return; }
        spawn_local(async move {
            if let Ok(n) = invoke::trash_count().await { let _ = count.try_set(Some(n)); }
        });
    });
    let empty = move |_| {
        let Some(n) = count.get_untracked().filter(|&n| n > 0) else { return };
        let l = ctx.lang.get_untracked();
        let msg = format!("{} {n} {}", t("trash_empty_confirm", l), t("trash_empty_confirm_end", l));
        ctx.ask_confirm_danger(msg, "trash_empty", move || {
            spawn_local(async move {
                match invoke::empty_trash().await {
                    Ok(gone) => {
                        let _ = count.try_set(Some(0));
                        ctx.notify_t("trash_emptied", &gone.to_string());
                    }
                    Err(e) => ctx.error("trash_empty_error", &e),
                }
            });
        });
    };
    view! {
        <div class="setting-section-title">{move || t("trash_section", lang())}</div>
        {move || count.get().map(|n| view! {
            <div class="setting-row">
                <div class="setting-input-row trash-row">
                    <span>{move || format!("{} {n}", t("trash_count", lang()))}</span>
                    <button class="btn-pick btn-pick-danger" prop:disabled=n == 0 on:click=empty>
                        {move || t("trash_empty", lang())}
                    </button>
                </div>
            </div>
        })}
        <Check draft get=|p| p.trash_auto_empty set=|p, v| p.trash_auto_empty = v
            label="trash_auto_empty" hint="trash_auto_empty_hint" />
        <NumberField draft get=|p| p.trash_keep_days set=|p, v| p.trash_keep_days = v
            label="trash_keep_days" hint="trash_keep_days_hint" />
        <NumberField draft get=|p| p.trash_max_bytes set=|p, v| p.trash_max_bytes = v
            label="trash_max_bytes" hint="trash_max_bytes_hint" readable=true />
    }
}

/// Where org-bloom builds the site of the open project, as its `org-bloom.toml`
/// says; the folder opens in the file manager.
#[component]
fn SiteFolder(draft: RwSignal<Prefs>) -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");
    let lang = move || ctx.lang.get();
    let site = RwSignal::new(None::<SiteConfig>);
    spawn_local(async move {
        if let Ok(found) = invoke::site_config().await { site.set(found); }
    });
    let open = move |dir: String| spawn_local(async move {
        if let Err(e) = invoke::open_folder(&dir).await { ctx.error("bloom_folder_missing", &e); }
    });
    view! {
        <div class="setting-row setting-sub bloom-site" class:off=move || !draft.with(|p| p.org_bloom)>
            <label>{move || t("bloom_folder", lang())}</label>
            {move || match site.get() {
                None => view! { <div class="setting-hint">{t("bloom_no_project", lang())}</div> }.into_any(),
                Some(SiteConfig { output_dir: Some(dir), config, .. }) => view! {
                    <button class="bloom-folder" title=move || t("bloom_open_folder", lang())
                        on:click={ let dir = dir.clone(); move |_| open(dir.clone()) }>
                        <span aria-hidden="true">"📂"</span>
                        <span class="bloom-folder-path">{dir.clone()}</span>
                    </button>
                    <div class="setting-hint">{t("bloom_folder_from", lang())} " " <code>{config}</code></div>
                }.into_any(),
                Some(SiteConfig { config, exists, .. }) => view! {
                    <div class="setting-hint">
                        {t(if exists { "bloom_no_output_dir" } else { "bloom_no_config" }, lang())}
                        " " <code>{config}</code>
                        <br />
                        <code>"[site]"</code> " " <code>"output_dir = \"../mon-site\""</code>
                    </div>
                }.into_any(),
            }}
        </div>
    }
}
