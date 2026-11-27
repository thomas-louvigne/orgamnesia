//! The tabs of the settings window, but the shortcuts one (see `shortcuts`).

use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use super::{fields::{Check, NumberField, PathField}, layout::LayoutEditor};
use crate::{i18n::t, invoke, state::{AppCtx, Prefs}};
use orgamnesia_core::SiteConfig;
mod about;
pub use about::*;
mod extensions;
pub use extensions::*;
use extensions::TrashSettings;

/// Project, format of the pages, updates and language.
#[component]
pub fn GeneralTab(draft: RwSignal<Prefs>) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
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
            <hr class="setting-separator" />
            <div class="setting-row">
                <label>{move || t("default_format", lang())}</label>
                <select
                    prop:value=move || draft.with(|p| p.default_format.id())
                    on:change=move |e| {
                        let f = orgamnesia_core::Format::from_id(&event_target_value(&e)).unwrap_or_default();
                        draft.update(|p| p.default_format = f);
                    }
                >
                    <option value="org">{move || t("format_org", lang())}</option>
                    <option value="markdown">{move || t("format_markdown", lang())}</option>
                </select>
                <span class="setting-hint">{move || t("default_format_hint", lang())}</span>
            </div>
            <Check draft get=|p| p.single_format set=|p, v| p.single_format = v
                label="single_format" hint="single_format_hint" sub=true />
            <Check draft get=|p| p.ask_format set=|p, v| p.ask_format = v
                label="ask_format" sub=true disabled=|p| p.single_format />
            <hr class="setting-separator" />
            {if orgamnesia_core::Package::current() == orgamnesia_core::Package::Flatpak {
                view! { <div class="setting-row"><span class="setting-hint">{move || t("update_flatpak", lang())}</span></div> }.into_any()
            } else {
                view! {
                    <Check draft get=|p| p.check_updates set=|p, v| p.check_updates = v
                        label="check_updates" hint="check_updates_hint" />
                    <div class="setting-row setting-sub">
                        <button class="btn-secondary update-check-now" on:click=move |_| crate::update::check_now(ctx)>
                            {move || t("update_check_now", lang())}
                        </button>
                    </div>
                }.into_any()
            }}
            <hr class="setting-separator" />
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

/// The links: their syntaxes in each format, page names, tags, how links are shown.
#[component]
pub fn LinksTab(draft: RwSignal<Prefs>) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();
    view! {
        <div class="tab-content">
            <div class="setting-section-title">{move || t("links_syntaxes", lang())}</div>
            <div class="setting-row"><label>{move || t("org_links", lang())}</label>
                <span class="setting-hint">{move || t("org_links_hint", lang())}</span></div>
            <Check draft get=|p| p.org_links.pipe set=|p, v| p.org_links.pipe = v label="link_pipe" sub=true />
            <Check draft get=|p| p.org_links.hash set=|p, v| p.org_links.hash = v label="link_hash" sub=true />
            <Check draft get=|p| p.org_links.markdown set=|p, v| p.org_links.markdown = v label="link_markdown" sub=true />
            <div class="setting-row"><label>{move || t("md_links", lang())}</label></div>
            <Check draft get=|p| p.md_links.pipe set=|p, v| p.md_links.pipe = v label="link_pipe" sub=true />
            <Check draft get=|p| p.md_links.hash set=|p, v| p.md_links.hash = v label="link_hash" sub=true />
            <Check draft get=|p| p.md_links.markdown set=|p, v| p.md_links.markdown = v label="link_markdown" sub=true />
            <div class="setting-section-title">{move || t("links_pages", lang())}</div>
            <Check draft get=|p| p.case_insensitive_links set=|p, v| p.case_insensitive_links = v label="ci_links" />
            <Check draft get=|p| p.update_links set=|p, v| p.update_links = v label="update_links" />
            <Check draft get=|p| p.click_follows_links set=|p, v| p.click_follows_links = v
                label="click_follows_links" hint="click_follows_links_hint" />
            <div class="setting-section-title">{move || t("links_tags", lang())}</div>
            <Check draft get=|p| p.hashtag_links set=|p, v| p.hashtag_links = v label="hashtag_links" />
            <Check draft get=|p| p.hashtag_dashes set=|p, v| p.hashtag_dashes = v
                label="hashtag_dashes" hint="hashtag_dashes_hint" sub=true
                disabled=|p| !p.hashtag_links />
            <Check draft get=|p| p.org_tag_links set=|p, v| p.org_tag_links = v
                label="org_tag_links" hint="org_tag_links_hint" />
            <Check draft get=|p| p.org_tag_dashes set=|p, v| p.org_tag_dashes = v
                label="org_tag_dashes" hint="org_tag_dashes_hint" sub=true />
            <div class="setting-row setting-sub">
                <button class="setting-doc-link"
                    on:click=move |_| spawn_local(async move {
                        let url = t("org_tag_dashes_doc_url", ctx.lang.get_untracked());
                        if let Err(e) = invoke::open_url(url).await { ctx.error("open_url_failed", &e); }
                    })>
                    {move || t("org_tag_dashes_doc", lang())}
                </button>
            </div>
            <div class="setting-section-title">{move || t("links_display", lang())}</div>
            <Check draft get=|p| p.mark_broken_links set=|p, v| p.mark_broken_links = v
                label="mark_broken_links" hint="mark_broken_links_hint" />
            <Check draft get=|p| p.link_preview set=|p, v| p.link_preview = v
                label="link_preview" hint="link_preview_hint" />
            <div class="setting-row setting-sub">
                <label>{move || t("link_preview_trigger", lang())}</label>
                <select
                    prop:disabled=move || !draft.with(|p| p.link_preview)
                    prop:value=move || draft.with(|p| p.link_preview_trigger.clone())
                    on:change=move |e| draft.update(|p| p.link_preview_trigger = event_target_value(&e))
                >
                    <option value="ctrl">{move || t("link_preview_ctrl", lang())}</option>
                    <option value="hover">{move || t("link_preview_hover", lang())}</option>
                </select>
            </div>
            <div class="setting-section-title">{move || t("links_videos", lang())}</div>
            <Check draft get=|p| p.show_video_thumbnails set=|p, v| p.show_video_thumbnails = v
                label="show_video_thumbnails" hint="show_video_thumbnails_hint" />
            <Check draft get=|p| p.embed_videos set=|p, v| p.embed_videos = v
                label="embed_videos" hint="embed_videos_hint" sub=true
                disabled=|p| !p.show_video_thumbnails />
        </div>
    }
}

/// Saving, editing helpers, TODO keywords, deleting pages and the trash.
#[component]
pub fn EditingTab(draft: RwSignal<Prefs>) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();
    view! {
        <div class="tab-content">
            <Check draft get=|p| p.autosave set=|p, v| p.autosave = v label="autosave" />
            <Check draft get=|p| p.electric_mode set=|p, v| p.electric_mode = v label="electric_mode" />
            <Check draft get=|p| p.electric_emphasis set=|p, v| p.electric_emphasis = v
                label="electric_emphasis" hint="electric_emphasis_hint" sub=true disabled=|p| !p.electric_mode />
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

/// What the editor shows.
#[component]
pub fn DisplayTab(draft: RwSignal<Prefs>) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();
    view! {
        <div class="tab-content">
            <div class="setting-section-title">
                {move || t("display_appearance", lang())}
            </div>
            <Check draft get=|p| p.show_page_title set=|p, v| p.show_page_title = v
                label="show_page_title" hint="show_page_title_hint" />
            <Check draft get=|p| p.show_page_nav set=|p, v| p.show_page_nav = v
                label="show_page_nav" hint="show_page_nav_hint" sub=true disabled=|p| !p.show_page_title />
            <Check draft get=|p| p.show_line_numbers set=|p, v| p.show_line_numbers = v
                label="show_line_numbers" />
            <Check draft get=|p| p.show_extensions set=|p, v| p.show_extensions = v
                label="show_extensions" />
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
            <Check draft get=|p| p.show_excalidraw set=|p, v| p.show_excalidraw = v
                label="show_excalidraw" hint="show_excalidraw_hint" sub=true disabled=|p| !p.show_images />
            <Check draft get=|p| p.hide_leading_stars set=|p, v| p.hide_leading_stars = v
                label="hide_leading_stars" hint="hide_leading_stars_hint" />
            <Check draft get=|p| p.live_preview set=|p, v| p.live_preview = v
                label="live_preview" hint="live_preview_hint" />
            <Check draft get=|p| p.ligatures set=|p, v| p.ligatures = v
                label="ligatures" hint="ligatures_hint" />
            <Check draft get=|p| p.link_text_first set=|p, v| p.link_text_first = v
                label="link_text_first" hint="link_text_first_hint" />
        </div>
    }
}

/// Which frames go around the editor, in which column and order.
#[component]
pub fn FramesTab(draft: RwSignal<Prefs>) -> impl IntoView {
    view! {
        <div class="tab-content">
            <LayoutEditor draft />
        </div>
    }
}

