//! The tabs of the themes, the page templates and the app itself (About).

use super::*;

const PROJECT_URL: &str = "https://github.com/thomas-louvigne/orgamnesia";
const COMMIT_URL: &str = concat!("https://github.com/thomas-louvigne/orgamnesia/commit/", env!("GIT_COMMIT"));
const AUTHOR_GITHUB_URL: &str = "https://github.com/thomas-louvigne";
const AUTHOR_X_URL: &str = "https://x.com/thomas_louvigne";
const AUTHOR_MASTODON_URL: &str = "https://framapiaf.org/@zobi8225";

/// A web address, shown whole and opened in the browser.
#[component]
fn AboutLink(url: &'static str) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    view! {
        <button class="setting-doc-link"
            on:click=move |_| spawn_local(async move {
                if let Err(e) = invoke::open_url(url).await { ctx.error("open_url_failed", &e); }
            })>
            {url}
        </button>
    }
}

/// The theme: the built-in ones and the user's CSS files of `themes/`.
#[component]
pub fn ThemesTab(draft: RwSignal<Prefs>) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();
    let files = RwSignal::new(Vec::<String>::new());
    let list = move || spawn_local(async move {
        match invoke::list_themes().await {
            Ok(names) => files.set(names),
            Err(e) => ctx.error("theme_error", &e),
        }
    });
    list();
    // The chosen theme as a new file of the user's, to change it from there
    let duplicate = move |_| spawn_local(async move {
        let id = draft.with_untracked(|p| p.theme.clone());
        let base = crate::theme::resolve(&id).trim_end_matches(".css");
        let made = match crate::theme::css_of(&id).await {
            Ok(css) => invoke::create_theme(&format!("{base}-copy"), &css).await,
            Err(e) => Err(e),
        };
        match made {
            Ok(file) => {
                draft.update(|p| p.theme = file);
                list();
                let _ = invoke::open_themes_folder().await;
            }
            Err(e) => ctx.error("theme_error", &e),
        }
    });
    let open_folder = move |_| spawn_local(async move {
        if let Err(e) = invoke::open_themes_folder().await { ctx.error("theme_error", &e); }
    });
    // A theme file changed outside: read again, as is the one in effect
    let reload = move |_| { list(); crate::theme::reload(ctx); };
    view! {
        <div class="tab-content">
            <div class="setting-row">
                <label>{move || t("theme", lang())}</label>
                <select
                    // Once its options are there too
                    prop:value=move || { files.track(); draft.with(|p| p.theme.clone()) }
                    on:change=move |e| draft.update(|p| p.theme = event_target_value(&e))
                >
                    <option value="auto">{move || t("theme_auto", lang())}</option>
                    {crate::theme::BUILTINS.iter().map(|&(id, label, _)| {
                        view! { <option value=id>{move || t(label, lang())}</option> }
                    }).collect_view()}
                    {move || files.get().into_iter().map(|f| {
                        let name = f.trim_end_matches(".css").to_string();
                        view! { <option value=f>{name}</option> }
                    }).collect_view()}
                    // A theme set by hand whose file is not there (any more)
                    {move || draft.with(|p| {
                        let id = p.theme.clone();
                        (crate::theme::builtin(&id).is_none() && !files.with(|f| f.contains(&id)))
                            .then(|| view! { <option value=id.clone()>{id.clone()}</option> })
                    })}
                </select>
            </div>
            <div class="setting-row">
                <span class="setting-hint">{move || t("theme_hint", lang())}</span>
            </div>
            <div class="setting-row">
                <button class="btn-secondary" title=move || t("theme_duplicate_title", lang()) on:click=duplicate>
                    {move || t("theme_duplicate", lang())}
                </button>
                <button class="btn-secondary" on:click=open_folder>{move || t("theme_open_folder", lang())}</button>
                <button class="btn-secondary" title=move || t("theme_reload_title", lang()) on:click=reload>
                    {move || t("theme_reload", lang())}
                </button>
            </div>
        </div>
    }
}

/// The app's logo and name, its link, version and commit, and who makes it.
#[component]
pub fn AboutTab() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();
    view! {
        <div class="tab-content">
            <div class="about-head">
                <img class="empty-logo" src="app-icon.svg" alt="" />
                <h3 class="about-title">"Orgamnesia"</h3>
            </div>
            <div class="setting-row">
                <label>{move || t("about_website", lang())}</label>
                <AboutLink url=PROJECT_URL />
            </div>
            <div class="setting-row">
                <label>{move || t("about_docs", lang())}</label>
                // In the language of the app
                {move || view! { <AboutLink url=t("about_docs_url", lang()) /> }}
            </div>
            <div class="setting-row">
                <label>{move || t("about_version", lang())}</label>
                <code>{env!("CARGO_PKG_VERSION")}</code>
            </div>
            <div class="setting-row">
                <label>{move || t("about_commit", lang())}</label>
                // "?": built outside a git checkout, no commit to link to
                {if env!("GIT_COMMIT") == "?" {
                    view! { <code>"?"</code> }.into_any()
                } else {
                    view! { <AboutLink url=COMMIT_URL /> }.into_any()
                }}
            </div>
            <div class="setting-row">
                <label>{move || t("about_author", lang())}</label>
                <span>"Thomas Louvigne"</span>
                <AboutLink url=AUTHOR_GITHUB_URL />
                <AboutLink url=AUTHOR_X_URL />
                <AboutLink url=AUTHOR_MASTODON_URL />
            </div>
        </div>
    }
}

/// The page templates of the project (its templates folder): opened in the editor to be
/// changed, created empty, or the suggested ones added.
#[component]
pub fn TemplatesTab(draft: RwSignal<Prefs>) -> impl IntoView {
    use orgamnesia_core::{templates, Format};
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();
    let project = move || ctx.project.vault_path.get().is_some();
    let list = RwSignal::new(Vec::<crate::state::FileEntry>::new());
    let reload = move || spawn_local(async move {
        if let Ok(found) = invoke::list_templates().await { list.set(found); }
    });
    if ctx.project.vault_path.get_untracked().is_some() { reload(); }
    let name = RwSignal::new(String::new());
    let format = RwSignal::new(draft.with_untracked(|p| p.default_format));

    let create = move || {
        let n = name.get_untracked().trim().to_string();
        if n.is_empty() { return; }
        spawn_local(async move {
            match invoke::create_template(&n, format.get_untracked(), "").await {
                Ok(file) => { name.set(String::new()); crate::actions::open_file(ctx, file, Some(crate::state::Goto::Focus)); }
                Err(e) => ctx.error("error", &e),
            }
        });
    };
    // In the format of the new pages; those already there (same name) are left as they are
    let add_defaults = move |_| {
        let f = draft.with_untracked(|p| p.default_format);
        spawn_local(async move {
            for (n, text) in templates::defaults(ctx.lang.get_untracked().as_str(), f) {
                if let Err(e) = invoke::create_template(n, f, text).await
                    && !list.with_untracked(|l| l.iter().any(|t| t.name == n)) { ctx.error("error", &e); }
            }
            reload();
        });
    };

    view! {
        <div class="tab-content">
            <div class="setting-row"><span class="setting-hint">{move || t("templates_hint", lang())}</span></div>
            <div class="setting-row">
                <label for="templates-folder">{move || t("templates_folder", lang())}</label>
                <input
                    id="templates-folder"
                    type="text"
                    class="setting-input"
                    spellcheck="false"
                    placeholder="templates"
                    prop:value=move || draft.with(|p| p.templates_folder.clone())
                    on:input=move |e| draft.update(|p| p.templates_folder = event_target_value(&e))
                />
                <span class="setting-hint">{move || t("templates_folder_hint", lang())}</span>
            </div>
            <hr class="setting-separator" />
            {move || if !project() {
                view! { <div class="setting-row"><span class="setting-hint">{t("templates_no_project", lang())}</span></div> }.into_any()
            } else {
                view! {
                    <div class="setting-row">
                        {move || if list.with(Vec::is_empty) {
                            view! { <span class="setting-hint">{t("templates_none", lang())}</span> }.into_any()
                        } else {
                            view! {
                                <ul class="template-list">
                                    {list.get().into_iter().map(|file| {
                                        let label = format!("{} ({})", file.name, Format::of_page(&file.path).ext());
                                        view! { <li><button class="btn-link" on:click=move |_| crate::actions::open_file(ctx, file.clone(), Some(crate::state::Goto::Focus))>{label}</button></li> }
                                    }).collect_view()}
                                </ul>
                            }.into_any()
                        }}
                    </div>
                    <hr class="setting-separator" />
                    <div class="setting-row template-new">
                        <input
                            type="text"
                            class="setting-input"
                            placeholder=move || t("template_name", lang())
                            prop:value=move || name.get()
                            on:input=move |e| name.set(event_target_value(&e))
                            on:keydown=move |e| if e.key() == "Enter" { create(); }
                        />
                        {move || (!draft.with(|p| p.single_format)).then(|| view! {
                            <select
                                aria-label=move || t("default_format", lang())
                                prop:value=move || format.get().id()
                                on:change=move |e| format.set(Format::from_id(&event_target_value(&e)).unwrap_or_default())
                            >
                                <option value="org">{move || t("format_org", lang())}</option>
                                <option value="markdown">{move || t("format_markdown", lang())}</option>
                            </select>
                        })}
                        <button class="btn-secondary" on:click=move |_| create()>{move || t("template_add", lang())}</button>
                    </div>
                    <div class="setting-row">
                        <button class="btn-secondary" on:click=add_defaults>{move || t("templates_defaults", lang())}</button>
                    </div>
                }.into_any()
            }}
        </div>
    }
}
