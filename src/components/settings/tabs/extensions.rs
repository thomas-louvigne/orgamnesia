//! The tabs of the extensions (org-bloom, git, graph), of the spelling (its
//! dictionaries) and of the trash.

use super::*;

/// The org-bloom, git and graph extensions.
#[component]
pub fn ExtensionsTab(draft: RwSignal<Prefs>) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();
    view! {
        <div class="tab-content">
            <Check draft get=|p| p.org_bloom set=|p, v| p.org_bloom = v
                label="org_bloom_enabled"
                hint=if crate::preferences::on_windows() { "org_bloom_windows" } else { "org_bloom_hint" }
                disabled=|_| crate::preferences::on_windows() />
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
            <NumberField draft get=|p| p.git_fetch set=|p, v| p.git_fetch = v
                label="git_fetch_minutes" hint="git_fetch_minutes_hint" disabled=|p| !p.git_ext />
            <Check draft get=|p| p.git_pull_rebase set=|p, v| p.git_pull_rebase = v
                label="git_pull_rebase" hint="git_pull_rebase_hint" sub=true disabled=|p| !p.git_ext />
            <Check draft get=|p| p.git_unpushed_marker set=|p, v| p.git_unpushed_marker = v
                label="git_unpushed_marker" hint="git_unpushed_marker_hint" sub=true disabled=|p| !p.git_ext />
            <Check draft get=|p| p.git_commit_push set=|p, v| p.git_commit_push = v
                label="git_commit_push_setting" hint="git_commit_push_hint" sub=true disabled=|p| !p.git_ext />
            <div class="setting-row setting-sub">
                <label for="git-commit-template">{move || t("git_commit_template", lang())}</label>
                <input
                    id="git-commit-template"
                    type="text"
                    class="setting-input"
                    spellcheck="false"
                    prop:disabled=move || !draft.with(|p| p.git_ext)
                    placeholder=move || t("git_commit_template_ph", lang())
                    prop:value=move || draft.with(|p| p.git_commit_template.clone())
                    on:input=move |e| draft.update(|p| p.git_commit_template = event_target_value(&e))
                />
                <span class="setting-hint">{move || t("git_commit_template_hint", lang())}</span>
            </div>
            <NumberField draft get=|p| p.git_auto_sync set=|p, v| p.git_auto_sync = v
                label="git_auto_sync" hint="git_auto_sync_hint" disabled=|p| !p.git_ext />
            <Check draft get=|p| p.git_sync_on_quit set=|p, v| p.git_sync_on_quit = v
                label="git_sync_on_quit" hint="git_sync_on_quit_hint" sub=true disabled=|p| !p.git_ext />
            <hr class="setting-separator" />
            <Check draft get=|p| p.graph set=|p, v| p.graph = v
                label="graph_enabled" hint="graph_hint" shortcut="graph" />
            <hr class="setting-separator" />
            <Check draft get=|p| p.journal set=|p, v| p.journal = v
                label="journal_enabled" hint="journal_hint" shortcut="journal" />
            <div class="setting-row setting-sub">
                <label for="journal-folder">{move || t("journal_folder", lang())}</label>
                <input
                    id="journal-folder"
                    type="text"
                    class="setting-input"
                    spellcheck="false"
                    prop:disabled=move || !draft.with(|p| p.journal)
                    placeholder="journals"
                    prop:value=move || draft.with(|p| p.journal_folder.clone())
                    on:input=move |e| draft.update(|p| p.journal_folder = event_target_value(&e))
                />
                <span class="setting-hint">{move || t("journal_folder_hint", lang())}</span>
            </div>
        </div>
    }
}

/// The spelling extension: on or off, the dictionary, and a folder of dictionaries.
#[component]
pub fn SpellingTab(draft: RwSignal<Prefs>) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();
    let off = move || !draft.with(|p| p.spell);
    // The dictionaries found, with the folder of the settings looked in first; None: not known yet
    let found = RwSignal::new(None::<Vec<String>>);
    Effect::new(move |_| {
        let dir = draft.with(|p| p.spell_dict_dir.clone());
        spawn_local(async move {
            if let Ok(names) = invoke::spell_languages(&dir).await { let _ = found.try_set(Some(names)); }
        });
    });
    view! {
        <div class="tab-content">
            <Check draft get=|p| p.spell set=|p, v| p.spell = v
                label="spell_enabled" hint="spell_hint" />
            {move || found.with(|f| f.as_ref().is_some_and(Vec::is_empty)).then(|| view! { <SpellInstallHelp draft /> })}
            <div class="setting-row setting-sub">
                <label>{move || t("spell_language", lang())}</label>
                <select
                    prop:disabled=off
                    prop:value=move || draft.with(|p| p.spell_lang.clone())
                    on:change=move |e| draft.update(|p| p.spell_lang = event_target_value(&e))
                >
                    <option value="">{move || t("spell_lang_auto", lang())}</option>
                    // A dictionary chosen before, missing now, stays shown
                    {move || {
                        let chosen = draft.with(|p| p.spell_lang.clone());
                        let mut names = found.get().unwrap_or_default();
                        if !chosen.is_empty() && !names.contains(&chosen) { names.insert(0, chosen.clone()); }
                        names.into_iter().map(|n| view! {
                            <option value=n.clone() selected=n == chosen>{n.clone()}</option>
                        }).collect_view()
                    }}
                </select>
            </div>
            <div class="setting-row setting-sub">
                <label>{move || t("spell_dict_dir", lang())}</label>
                <PathField
                    value=Signal::derive(move || draft.with(|p| p.spell_dict_dir.clone()))
                    on_change=Callback::new(move |v| draft.update(|p| p.spell_dict_dir = v))
                    placeholder=Signal::derive(move || t("spell_dict_dir_ph", lang()).to_string())
                    disabled=Signal::derive(off)
                />
            </div>
        </div>
    }
}

/// Where Hunspell dictionaries can be downloaded, for the systems without packages.
const DICTIONARIES_URL: &str = "https://github.com/LibreOffice/dictionaries";

/// The packages of the dictionary for `code` (`fr`, `en_GB`…), as Debian, Fedora
/// and Arch name them.
fn dict_packages(code: &str) -> [String; 3] {
    let (lang, region) = code.split_once('_').unwrap_or((code, ""));
    let lang = lang.to_lowercase();
    // English comes in one package per country, the other languages in one package
    if lang == "en" {
        let region = if region.is_empty() { "us".to_string() } else { region.to_lowercase() };
        return [format!("hunspell-en-{region}"), format!("hunspell-en-{}", region.to_uppercase()),
                format!("hunspell-en_{region}")];
    }
    [0, 1, 2].map(|_| format!("hunspell-{lang}"))
}

/// No dictionary on the system: how to install one, for the system the application runs on.
#[component]
fn SpellInstallHelp(draft: RwSignal<Prefs>) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();
    let platform = web_sys::window().and_then(|w| w.navigator().platform().ok()).unwrap_or_default();
    let (mac, windows) = (platform.starts_with("Mac"), platform.starts_with("Win"));
    let help = if mac { "spell_install_mac" } else if windows { "spell_install_windows" } else { "spell_install_linux" };
    // The dictionary wanted: the one of the settings, else the interface language's
    let commands = move || {
        let code = draft.with(|p| if p.spell_lang.is_empty() { p.lang.clone() } else { p.spell_lang.clone() });
        let [deb, fedora, arch] = dict_packages(&code);
        [("Debian, Ubuntu", format!("sudo apt install {deb}")),
         ("Fedora", format!("sudo dnf install {fedora}")),
         ("Arch", format!("sudo pacman -S {arch}"))]
    };
    view! {
        <div class="setting-row setting-sub">
            <div class="setting-hint install-help">
                <p class="install-help-title">{move || t("spell_lang_none", lang())}</p>
                <p>{move || t(help, lang())}</p>
                {(!mac && !windows).then(|| view! {
                    {move || commands().into_iter().map(|(distro, cmd)| view! {
                        <div class="install-cmd"><span>{distro}</span><code>{cmd}</code></div>
                    }).collect_view()}
                })}
                {(mac || windows).then(|| view! {
                    <button class="setting-doc-link"
                        on:click=move |_| spawn_local(async move {
                            if let Err(e) = invoke::open_url(DICTIONARIES_URL).await { ctx.error("open_url_failed", &e); }
                        })>
                        {move || t("spell_install_download", lang())}
                    </button>
                })}
                <p>{move || t("spell_install_then", lang())}</p>
            </div>
        </div>
    }
}

/// The project's trash: how many pages it holds, a button to empty it, and the
/// setting to empty it automatically.
#[component]
pub(super) fn TrashSettings(draft: RwSignal<Prefs>) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
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
            label="trash_keep_days" hint="trash_keep_days_hint" disabled=|p| !p.trash_auto_empty />
        <NumberField draft get=|p| p.trash_max_bytes set=|p, v| p.trash_max_bytes = v
            label="trash_max_bytes" hint="trash_max_bytes_hint" disabled=|p| !p.trash_auto_empty readable=true />
    }
}

/// Where org-bloom builds the site of the open project, as its `org-bloom.toml`
/// says; the folder opens in the file manager.
#[component]
fn SiteFolder(draft: RwSignal<Prefs>) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
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
                        <code>"[site]"</code> " " <code>"output_dir = \"../site\""</code>
                    </div>
                }.into_any(),
            }}
        </div>
    }
}
