use leptos::{ev, html, prelude::*};
use wasm_bindgen_futures::spawn_local;

use crate::{
    i18n::{t, Lang},
    invoke,
    keybindings::{self, ActionDef, Bindings, Keybindings, Profile},
    state::AppCtx,
};

/// Everything the settings window edits, to tell whether there is anything to apply.
#[derive(Clone, PartialEq)]
struct Snapshot {
    vault: String,
    builder: String,
    lang: String,
    update_links: bool,
    autosave: bool,
    ci_links: bool,
    hashtags: bool,
    emacs_mark: bool,
    electric: bool,
    active: String,
    profiles: Vec<Profile>,
    app: Bindings,
    editor: Bindings,
}

const SIZE_KEY: &str = "settings_modal_size";

/// Size (width, height in px) the settings window had last time, if remembered.
fn load_size() -> Option<(i32, i32)> {
    let s = web_sys::window()?.local_storage().ok()??.get_item(SIZE_KEY).ok()??;
    let (w, h) = s.split_once(',')?;
    Some((w.parse().ok()?, h.parse().ok()?))
}

fn store_size(w: i32, h: i32) {
    if let Some(st) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = st.set_item(SIZE_KEY, &format!("{w},{h}"));
    }
}

/// Application actions about the editor panes; shown in their own section.
const SPLIT_ACTIONS: &[&str] = &[
    "split_vertical", "split_horizontal", "close_split", "single_window", "other_window",
];

/// Editor actions of the selection marker; shown in their own section.
const MARK_ACTIONS: &[&str] = &["set_mark", "keyboard_quit"];

/// Combinations bound to more than one action, with the label keys of those actions.
type Conflicts = Vec<(String, Vec<&'static str>)>;

/// Application and editor shortcuts share one keyboard, so a combination may
/// only be given to a single action across both lists. A shortcut that is the
/// first key(s) of another action's chord ("ctrl+x" vs "ctrl+x ctrl+s") is a
/// conflict too: the chord could never be completed. Giving the same
/// combination to both slots of one action is fine.
fn find_conflicts(app: &Bindings, editor: &Bindings) -> Conflicts {
    // (binding, action label), one entry per distinct binding of each action
    let mut entries: Vec<(String, &'static str)> = Vec::new();
    for (map, defs) in [(app, keybindings::APP_ACTIONS), (editor, keybindings::EDITOR_ACTIONS)] {
        for a in defs {
            let mut seen = std::collections::HashSet::new();
            for b in map.get(a.id).into_iter().flatten().filter(|b| !b.is_empty()) {
                if seen.insert(b.clone()) {
                    entries.push((b.clone(), a.label_key));
                }
            }
        }
    }
    let mut by_combo: std::collections::BTreeMap<String, Vec<&'static str>> = Default::default();
    for (bind, label) in &entries {
        let chord_prefix = format!("{bind} ");
        for (other, other_label) in &entries {
            if other_label != label && (other == bind || other.starts_with(&chord_prefix)) {
                let labels = by_combo.entry(bind.clone()).or_default();
                for l in [label, other_label] {
                    if !labels.contains(l) { labels.push(l); }
                }
            }
        }
    }
    by_combo.into_iter().collect()
}

fn describe_conflict(combo: &str, labels: &[&'static str], lang: Lang) -> String {
    let names: Vec<&str> = labels.iter().map(|k| t(k, lang)).collect();
    format!("{} : {}", keybindings::display(combo), names.join(", "))
}

/// Drop empty binding strings and actions that end up with no bindings.
fn clean_bindings(mut b: Bindings) -> Bindings {
    for v in b.values_mut() {
        v.retain(|s| !s.is_empty());
    }
    b.retain(|_, v| !v.is_empty());
    b
}

fn preset_label_key(preset: &str) -> &'static str {
    match preset {
        "emacs" => "preset_emacs",
        _       => "preset_classic",
    }
}

/// Create a custom profile from the given bindings and make it the active one.
/// Named "Custom", then "Custom 2", "Custom 3"… when the name is taken.
fn add_profile(
    active_id: RwSignal<String>,
    profiles: RwSignal<Vec<Profile>>,
    lang: Lang,
    base_preset: String,
    app: Bindings,
    editor: Bindings,
) {
    let base = t("profile_custom", lang);
    let name = profiles.with_untracked(|ps| {
        let mut n = 1;
        loop {
            let candidate = if n == 1 { base.to_string() } else { format!("{base} {n}") };
            if !ps.iter().any(|p| p.name == candidate) { break candidate; }
            n += 1;
        }
    });
    let id = format!("custom-{}", js_sys::Date::now() as u64);
    profiles.update(|ps| ps.push(Profile {
        id: id.clone(), name, editor_preset: base_preset, app, editor,
    }));
    active_id.set(id);
}

#[component]
pub fn SettingsModal() -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");
    let lang = move || ctx.lang.get();

    // Which tab is active: 0 = general, 1 = shortcuts
    let panel = RwSignal::new(0u8);

    let vault_input   = RwSignal::new(ctx.vault_path.get().unwrap_or_default());
    let builder_input = RwSignal::new(String::new());
    let lang_input    = RwSignal::new(ctx.lang.get().as_str().to_string());
    let update_links  = RwSignal::new(true);
    let autosave      = RwSignal::new(ctx.autosave.get_untracked());
    let ci_links      = RwSignal::new(ctx.case_insensitive_links.get_untracked());
    let hashtags      = RwSignal::new(ctx.hashtag_links.get_untracked());
    let emacs_mark    = RwSignal::new(ctx.emacs_mark.get_untracked());
    let electric      = RwSignal::new(ctx.electric_mode.get_untracked());

    // Working copy of the keybindings (action id -> up to 2 bindings).
    let kb = ctx.keybindings.get();
    let active_id   = RwSignal::new(kb.resolved_active());
    let profiles    = RwSignal::new(kb.profiles);
    let preset      = RwSignal::new(kb.editor_preset);
    let app_work    = RwSignal::new(kb.app);
    let editor_work = RwSignal::new(kb.editor);

    // Any edit made while a built-in profile is active forks it into a new custom
    // profile; while a custom profile is active the edit updates that profile.
    // Switching profiles sets the working copies to the profile's own bindings,
    // so it never counts as an edit.
    Effect::new(move |_| {
        let app = app_work.get();
        let editor = editor_work.get();
        let id = active_id.get_untracked();
        if keybindings::is_builtin(&id) {
            if app != keybindings::preset_app(&id) || editor != keybindings::preset_editor(&id) {
                add_profile(active_id, profiles, ctx.lang.get_untracked(), id, app, editor);
            }
        } else {
            let unchanged = profiles.with_untracked(|ps| {
                ps.iter().find(|p| p.id == id).map_or(true, |p| p.app == app && p.editor == editor)
            });
            if !unchanged {
                profiles.update(|ps| {
                    if let Some(p) = ps.iter_mut().find(|p| p.id == id) {
                        p.app = app;
                        p.editor = editor;
                    }
                });
            }
        }
    });

    // Shortcuts given to several actions (blocks saving) and the popup announcing them
    let conflicts = Memo::new(move |_| {
        app_work.with(|a| editor_work.with(|e| find_conflicts(a, e)))
    });
    provide_context(conflicts);
    let conflict_popup = RwSignal::new(None::<String>);
    let known_conflicts = StoredValue::new(Vec::<String>::new());
    Effect::new(move |_| {
        let lang = ctx.lang.get_untracked();
        let list = conflicts.get();
        let fresh: Vec<String> = known_conflicts.with_value(|known| {
            list.iter()
                .filter(|(combo, _)| !known.contains(combo))
                .map(|(combo, labels)| describe_conflict(combo, labels, lang))
                .collect()
        });
        known_conflicts.set_value(list.iter().map(|(c, _)| c.clone()).collect());
        if !fresh.is_empty() {
            conflict_popup.set(Some(fresh.join("\n")));
        }
    });

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
        if e.key() == "Escape" {
            if conflict_popup.get_untracked().is_some() {
                conflict_popup.set(None);
            } else {
                ctx.show_settings.set(false);
            }
        }
    });
    on_cleanup(move || { drop(move_handle); drop(up_handle); drop(esc_handle); });

    // What was last loaded/applied; the window differs from it when there is
    // something to apply. `None` until the saved settings have been read.
    let snapshot = move || Snapshot {
        vault: vault_input.get(),
        builder: builder_input.get(),
        lang: lang_input.get(),
        update_links: update_links.get(),
        autosave: autosave.get(),
        ci_links: ci_links.get(),
        hashtags: hashtags.get(),
        emacs_mark: emacs_mark.get(),
        electric: electric.get(),
        active: active_id.get(),
        profiles: profiles.get(),
        app: app_work.get(),
        editor: editor_work.get(),
    };
    let baseline = RwSignal::new(None::<Snapshot>);
    let has_changes = move || {
        baseline.with(|b| b.as_ref().map_or(false, |b| *b != snapshot()))
    };

    // Load current settings
    spawn_local(async move {
        if let Ok(s) = invoke::get_settings().await {
            vault_input.set(s.vault_path.unwrap_or_default());
            builder_input.set(s.logseq_site_builder_path.unwrap_or_default());
            update_links.set(s.update_links_on_rename.unwrap_or(true));
            autosave.set(s.autosave.unwrap_or(true));
            ci_links.set(s.case_insensitive_links.unwrap_or(true));
            hashtags.set(s.hashtag_links.unwrap_or(true));
            emacs_mark.set(s.emacs_mark.unwrap_or(true));
            electric.set(s.electric_mode.unwrap_or(true));
            if let Some(l) = s.language {
                lang_input.set(l);
            }
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

    let pick_vault = move |_| {
        spawn_local(async move {
            match invoke::pick_folder().await {
                Ok(Some(path)) => vault_input.set(path),
                Ok(None) => {}
                Err(e) => ctx.status.set(Some(format!("Error: {e}"))),
            }
        });
    };

    let pick_builder = move |_| {
        spawn_local(async move {
            match invoke::pick_folder().await {
                Ok(Some(path)) => builder_input.set(path),
                Ok(None) => {}
                Err(e) => ctx.status.set(Some(format!("Error: {e}"))),
            }
        });
    };

    // Load a profile (built-in or custom) into the working copies.
    let select_profile = move |id: String| {
        let (base, app, editor) = if keybindings::is_builtin(&id) {
            (id.clone(), keybindings::preset_app(&id), keybindings::preset_editor(&id))
        } else if let Some(p) = profiles.with_untracked(|ps| ps.iter().find(|p| p.id == id).cloned()) {
            (p.editor_preset, p.app, p.editor)
        } else {
            return;
        };
        active_id.set(id);
        preset.set(base);
        app_work.set(app);
        editor_work.set(editor);
    };
    let on_profile_change = move |e: web_sys::Event| select_profile(event_target_value(&e));

    let duplicate_profile = move |_| {
        add_profile(
            active_id, profiles, ctx.lang.get_untracked(), preset.get_untracked(),
            app_work.get_untracked(), editor_work.get_untracked(),
        );
    };
    let delete_profile = move |_| {
        let id = active_id.get_untracked();
        let base = preset.get_untracked();
        profiles.update(|ps| ps.retain(|p| p.id != id));
        select_profile(base);
    };
    let rename_profile = move |e: web_sys::Event| {
        let name = event_target_value(&e);
        let id = active_id.get_untracked();
        profiles.update(|ps| {
            if let Some(p) = ps.iter_mut().find(|p| p.id == id) { p.name = name; }
        });
    };

    // Save everything; `close` tells whether the window closes afterwards ("Save")
    // or stays open ("Apply").
    let do_save = move |close: bool| {
        if !conflicts.get_untracked().is_empty() { return; }
        let applied = untrack(snapshot);
        let update_links_val = update_links.get();
        let autosave_val = autosave.get();
        let ci_links_val = ci_links.get();
        let hashtags_val = hashtags.get();
        let emacs_mark_val = emacs_mark.get();
        let electric_val = electric.get();
        let vault    = vault_input.get();
        let builder  = builder_input.get();
        let lang_str = lang_input.get();
        let default_name = t("profile_custom", ctx.lang.get_untracked());
        let kb = Keybindings {
            editor_preset: preset.get(),
            app:    clean_bindings(app_work.get()),
            editor: clean_bindings(editor_work.get()),
            active_profile: active_id.get(),
            profiles: profiles.get().into_iter().map(|mut p| {
                p.app = clean_bindings(p.app);
                p.editor = clean_bindings(p.editor);
                if p.name.trim().is_empty() { p.name = default_name.to_string(); }
                p
            }).collect(),
        };

        spawn_local(async move {
            let settings = crate::state::Settings {
                vault_path: if vault.is_empty() { None } else { Some(vault.clone()) },
                logseq_site_builder_path: if builder.is_empty() { None } else { Some(builder) },
                language: Some(lang_str.clone()),
                update_links_on_rename: Some(update_links_val),
                autosave: Some(autosave_val),
                case_insensitive_links: Some(ci_links_val),
                hashtag_links: Some(hashtags_val),
                emacs_mark: Some(emacs_mark_val),
                electric_mode: Some(electric_val),
                projects: ctx.projects.get_untracked(),
            };
            match invoke::save_settings(&settings).await {
                Ok(_) => {
                    let new_lang = Lang::from_str(&lang_str);
                    ctx.lang.set(new_lang);
                    ctx.autosave.set(autosave_val);
                    ctx.case_insensitive_links.set(ci_links_val);
                    ctx.hashtag_links.set(hashtags_val);
                    ctx.links_version.update(|v| *v += 1);
                    ctx.emacs_mark.set(emacs_mark_val);
                    ctx.electric_mode.set(electric_val);
                    if let Some(ref path) = settings.vault_path {
                        if ctx.vault_path.get().as_deref() != Some(path.as_str()) {
                            crate::components::sidebar::open_project(ctx, path.clone()).await;
                        }
                    }
                    baseline.set(Some(applied));
                    ctx.status.set(Some(t("settings_saved", new_lang).to_string()));
                    if close { ctx.show_settings.set(false); }
                }
                Err(e) => ctx.status.set(Some(format!("Error: {e}"))),
            }
            match invoke::set_keybindings(&kb).await {
                Ok(_) => ctx.keybindings.set(kb),
                Err(e) => ctx.status.set(Some(format!("Keybindings error: {e}"))),
            }
        });
    };

    let save  = move |_| do_save(true);
    let apply = move |_| do_save(false);

    view! {
        <div
            class="modal-overlay"
            on:mousedown=move |e: web_sys::MouseEvent| {
                overlay_down.set(e.target() == e.current_target());
            }
            on:click=move |_| {
                // Ignore clicks that started inside the modal (e.g. a resize drag
                // released outside of it)
                if overlay_down.get_untracked() { ctx.show_settings.set(false) }
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
                        on:click=move |_| ctx.show_settings.set(false)
                        on:mousedown=|e: web_sys::MouseEvent| e.stop_propagation()
                    >"×"</button>
                </div>

                <div class="modal-tabs">
                    <button
                        class=move || if panel.get() == 0 { "modal-tab active" } else { "modal-tab" }
                        on:click=move |_| panel.set(0)
                        on:mousedown=|e: web_sys::MouseEvent| e.stop_propagation()
                    >{move || t("tab_general", lang())}</button>
                    <button
                        class=move || if panel.get() == 1 { "modal-tab active" } else { "modal-tab" }
                        on:click=move |_| panel.set(1)
                        on:mousedown=|e: web_sys::MouseEvent| e.stop_propagation()
                    >{move || t("tab_shortcuts", lang())}</button>
                    <button
                        class=move || if panel.get() == 2 { "modal-tab active" } else { "modal-tab" }
                        on:click=move |_| panel.set(2)
                        on:mousedown=|e: web_sys::MouseEvent| e.stop_propagation()
                    >{move || t("tab_extensions", lang())}</button>
                </div>

                <div class="modal-body">
                    {move || (panel.get() == 0).then(|| view! {
                        <div class="tab-content">
                            <div class="setting-row">
                                <label>{move || t("vault_folder", lang())}</label>
                                <div class="setting-input-row">
                                    <input
                                        type="text"
                                        class="setting-input"
                                        placeholder={move || t("vault_hint", lang())}
                                        prop:value=move || vault_input.get()
                                        on:input=move |e| vault_input.set(event_target_value(&e))
                                    />
                                    <button class="btn-pick" on:click=pick_vault>
                                        {move || t("browse", lang())}
                                    </button>
                                </div>
                            </div>

                            <div class="setting-row">
                                <label class="setting-check">
                                    <input
                                        type="checkbox"
                                        prop:checked=move || autosave.get()
                                        on:change=move |e| autosave.set(event_target_checked(&e))
                                    />
                                    {move || t("autosave", lang())}
                                </label>
                            </div>

                            <div class="setting-row">
                                <label class="setting-check">
                                    <input
                                        type="checkbox"
                                        prop:checked=move || electric.get()
                                        on:change=move |e| electric.set(event_target_checked(&e))
                                    />
                                    {move || t("electric_mode", lang())}
                                </label>
                            </div>

                            <div class="setting-row">
                                <label class="setting-check">
                                    <input
                                        type="checkbox"
                                        prop:checked=move || ci_links.get()
                                        on:change=move |e| ci_links.set(event_target_checked(&e))
                                    />
                                    {move || t("ci_links", lang())}
                                </label>
                            </div>

                            <div class="setting-row">
                                <label class="setting-check">
                                    <input
                                        type="checkbox"
                                        prop:checked=move || hashtags.get()
                                        on:change=move |e| hashtags.set(event_target_checked(&e))
                                    />
                                    {move || t("hashtag_links", lang())}
                                </label>
                            </div>

                            <div class="setting-row">
                                <label class="setting-check">
                                    <input
                                        type="checkbox"
                                        prop:checked=move || update_links.get()
                                        on:change=move |e| update_links.set(event_target_checked(&e))
                                    />
                                    {move || t("update_links", lang())}
                                </label>
                            </div>

                            <div class="setting-row">
                                <label>{move || t("language", lang())}</label>
                                <select
                                    prop:value=move || lang_input.get()
                                    on:change=move |e| lang_input.set(event_target_value(&e))
                                >
                                    <option value="fr">{move || t("lang_fr", lang())}</option>
                                    <option value="en">{move || t("lang_en", lang())}</option>
                                </select>
                            </div>
                        </div>
                    })}

                    {move || (panel.get() == 2).then(|| view! {
                        <div class="tab-content">
                            <div class="setting-row">
                                <label>{move || t("builder_path", lang())}</label>
                                <div class="setting-input-row">
                                    <input
                                        type="text"
                                        class="setting-input"
                                        placeholder="logseq-site-builder"
                                        prop:value=move || builder_input.get()
                                        on:input=move |e| builder_input.set(event_target_value(&e))
                                    />
                                    <button class="btn-pick" on:click=pick_builder>
                                        {move || t("browse", lang())}
                                    </button>
                                </div>
                            </div>
                        </div>
                    })}

                    {move || (panel.get() == 1).then(|| view! {
                        <div class="tab-content">
                            <div class="setting-row">
                                <label>{move || t("kb_profile", lang())}</label>
                                <div class="setting-input-row">
                                    <select on:change=on_profile_change>
                                        {keybindings::EDITOR_PRESETS.iter().map(|p| view! {
                                            <option value=*p selected=move || active_id.get() == *p>
                                                {move || t(preset_label_key(p), lang())}
                                            </option>
                                        }).collect_view()}
                                        {move || profiles.get().into_iter().map(|p| {
                                            let id = p.id.clone();
                                            view! {
                                                <option value=p.id.clone() selected=move || active_id.get() == id>
                                                    {p.name}
                                                </option>
                                            }
                                        }).collect_view()}
                                    </select>
                                    <button class="btn-pick" on:click=duplicate_profile>
                                        {move || t("profile_duplicate", lang())}
                                    </button>
                                </div>
                                <span class="setting-hint">{move || t("preset_hint", lang())}</span>
                            </div>
                            {move || (!keybindings::is_builtin(&active_id.get())).then(|| view! {
                                <div class="setting-row">
                                    <label>{move || t("profile_name", lang())}</label>
                                    <div class="setting-input-row">
                                        <input
                                            type="text"
                                            class="setting-input"
                                            prop:value=move || {
                                                let id = active_id.get();
                                                profiles.with(|ps| ps.iter().find(|p| p.id == id)
                                                    .map(|p| p.name.clone()).unwrap_or_default())
                                            }
                                            on:input=rename_profile
                                        />
                                        <button class="btn-pick" on:click=delete_profile>
                                            {move || t("profile_delete", lang())}
                                        </button>
                                    </div>
                                </div>
                            })}
                            {move || {
                                let list = conflicts.get();
                                (!list.is_empty()).then(|| view! {
                                    <div class="conflict-banner">
                                        <strong>{move || t("conflict_title", lang())}</strong>
                                        {list.iter()
                                            .map(|(c, l)| describe_conflict(c, l, lang()))
                                            .map(|line| view! { <div>{line}</div> })
                                            .collect_view()}
                                        <div>{move || t("conflict_blocked", lang())}</div>
                                    </div>
                                })
                            }}
                            <div class="setting-section-title">
                                {move || t("shortcut_section_app", lang())}
                            </div>
                            <KeybindingHeader lang=ctx.lang />
                            {keybindings::APP_ACTIONS.iter()
                                .filter(|a| !SPLIT_ACTIONS.contains(&a.id))
                                .map(|a| view! {
                                    <KeybindingRow map=app_work action=a lang=ctx.lang />
                                }).collect_view()}

                            <div class="setting-section-title">
                                {move || t("shortcut_section_split", lang())}
                            </div>
                            <KeybindingHeader lang=ctx.lang />
                            {keybindings::APP_ACTIONS.iter()
                                .filter(|a| SPLIT_ACTIONS.contains(&a.id))
                                .map(|a| view! {
                                    <KeybindingRow map=app_work action=a lang=ctx.lang />
                                }).collect_view()}

                            <div class="setting-section-title">
                                {move || t("shortcut_section_editor", lang())}
                            </div>
                            <KeybindingHeader lang=ctx.lang />
                            {keybindings::EDITOR_ACTIONS.iter()
                                .filter(|a| !MARK_ACTIONS.contains(&a.id))
                                .map(|a| view! {
                                    <KeybindingRow map=editor_work action=a lang=ctx.lang />
                                }).collect_view()}

                            <div class="setting-section-title">
                                {move || t("mark_section", lang())}
                            </div>
                            <div class="setting-row">
                                <label class="setting-check">
                                    <input
                                        type="checkbox"
                                        prop:checked=move || emacs_mark.get()
                                        on:change=move |e| emacs_mark.set(event_target_checked(&e))
                                    />
                                    {move || t("emacs_mark", lang())}
                                </label>
                            </div>
                            <KeybindingHeader lang=ctx.lang />
                            {keybindings::EDITOR_ACTIONS.iter()
                                .filter(|a| MARK_ACTIONS.contains(&a.id))
                                .map(|a| view! {
                                    <KeybindingRow map=editor_work action=a lang=ctx.lang />
                                }).collect_view()}
                        </div>
                    })}
                </div>

                <div class="modal-footer">
                    <button
                        class="btn-secondary"
                        on:click=move |_| ctx.show_settings.set(false)
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

/// Titles of the two shortcut columns.
#[component]
fn KeybindingHeader(lang: RwSignal<Lang>) -> impl IntoView {
    view! {
        <div class="keybinding-row keybinding-header">
            <label></label>
            <div class="keybinding-slots">
                <span class="keybinding-col-title">{move || t("kb_col1", lang.get())}</span>
                <span class="keybinding-col-title">{move || t("kb_col2", lang.get())}</span>
            </div>
        </div>
    }
}

/// One action row: its label + two assignable binding slots.
#[component]
fn KeybindingRow(
    map: RwSignal<Bindings>,
    action: &'static ActionDef,
    lang: RwSignal<Lang>,
) -> impl IntoView {
    view! {
        <div class="keybinding-row">
            <label>{move || t(action.label_key, lang.get())}</label>
            <div class="keybinding-slots">
                <KeybindingSlot map=map id=action.id index=0 lang=lang />
                <KeybindingSlot map=map id=action.id index=1 lang=lang />
            </div>
        </div>
    }
}

/// A single binding slot (one accelerator) for `id` at position `index`.
#[component]
fn KeybindingSlot(
    map: RwSignal<Bindings>,
    id: &'static str,
    index: usize,
    lang: RwSignal<Lang>,
) -> impl IntoView {
    let recording = RwSignal::new(false);
    let input_ref = NodeRef::<html::Input>::new();

    Effect::new(move |_| {
        if recording.get() {
            if let Some(el) = input_ref.get() {
                let _ = el.focus();
            }
        }
    });

    let current = move || {
        map.with(|m| m.get(id).and_then(|v| v.get(index)).cloned().unwrap_or_default())
    };
    let conflicts = use_context::<Memo<Conflicts>>();
    let in_conflict = move || {
        let c = current();
        !c.is_empty() && conflicts.map_or(false, |cf| cf.with(|l| {
            l.iter().any(|(k, _)| *k == c || c.starts_with(&format!("{k} ")))
        }))
    };

    // Keys of the shortcut being recorded, and modifiers currently held (feedback).
    // A shortcut may be a chord of two keys ("Ctrl+X Ctrl+S"): after the first
    // key we wait a moment for a second one before committing.
    let seq = RwSignal::new(Vec::<String>::new());
    let held = RwSignal::new(String::new());
    let generation = StoredValue::new(0u32);
    let show_held = move |e: &web_sys::KeyboardEvent| {
        let mut s = String::new();
        if e.ctrl_key()  { s.push_str("Ctrl+"); }
        if e.shift_key() { s.push_str("Shift+"); }
        if e.alt_key()   { s.push_str("Alt+"); }
        held.set(s);
    };
    let stop_recording = move || {
        generation.update_value(|g| *g += 1); // cancels a pending commit
        seq.set(vec![]);
        held.set(String::new());
        recording.set(false);
    };
    let commit = move || {
        let binding = seq.get_untracked().join(" ");
        if !binding.is_empty() {
            map.update(|m| {
                let v = m.entry(id.to_string()).or_default();
                while v.len() <= index { v.push(String::new()); }
                v[index] = binding;
            });
        }
        stop_recording();
    };

    let on_keydown = move |e: web_sys::KeyboardEvent| {
        e.prevent_default();
        // Keep the press away from the global shortcut handlers while recording
        e.stop_propagation();
        if e.key() == "Escape" {
            stop_recording();
            return;
        }
        // A bare modifier (Ctrl, Shift…) is only the start of a combination:
        // keep recording until the actual key arrives.
        match keybindings::capture_from_event(&e) {
            Some(key) => {
                seq.update(|s| s.push(key));
                held.set(String::new());
                generation.update_value(|g| *g += 1);
                if seq.with_untracked(|s| s.len()) >= 2 {
                    commit();
                } else {
                    let g = generation.get_value();
                    keybindings::after_ms(800, move || {
                        if generation.get_value() == g { commit(); }
                    });
                }
            }
            None => show_held(&e),
        }
    };
    let on_keyup = move |e: web_sys::KeyboardEvent| show_held(&e);

    let clear = move |_| {
        map.update(|m| {
            if let Some(v) = m.get_mut(id) {
                if index < v.len() { v[index] = String::new(); }
            }
        });
        recording.set(false);
    };

    view! {
        <div class="keybinding-field-row">
            {move || if recording.get() {
                view! {
                    <input
                        node_ref=input_ref
                        class="keybinding-field recording"
                        type="text"
                        readonly=true
                        placeholder={move || {
                            let keys = seq.get();
                            let h = held.get();
                            if !keys.is_empty() {
                                format!("{} {h}…", keybindings::display(&keys.join(" ")))
                            } else if h.is_empty() {
                                t("recording", lang.get()).to_string()
                            } else {
                                format!("{h}…")
                            }
                        }}
                        on:keydown=on_keydown
                        on:keyup=on_keyup
                        on:blur=move |_| stop_recording()
                    />
                }.into_any()
            } else {
                view! {
                    <button
                        class=move || if in_conflict() { "keybinding-field conflict" } else { "keybinding-field" }
                        on:click=move |_| recording.set(true)
                    >
                        {move || keybindings::display(&current())}
                    </button>
                }.into_any()
            }}
            <button
                class="keybinding-clear"
                on:click=clear
            >"×"</button>
        </div>
    }
}
