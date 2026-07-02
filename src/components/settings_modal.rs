use leptos::{ev, html, prelude::*};
use wasm_bindgen_futures::spawn_local;

use crate::{
    i18n::{t, Lang},
    invoke,
    keybindings::{self, ActionDef, Bindings, Keybindings},
    state::AppCtx,
};

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

#[component]
pub fn SettingsModal() -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");
    let lang = move || ctx.lang.get();

    // Which tab is active: 0 = general, 1 = shortcuts
    let panel = RwSignal::new(0u8);

    let vault_input   = RwSignal::new(ctx.vault_path.get().unwrap_or_default());
    let builder_input = RwSignal::new(String::new());
    let lang_input    = RwSignal::new(ctx.lang.get().as_str().to_string());

    // Working copy of the keybindings (action id -> up to 2 bindings).
    let kb = ctx.keybindings.get();
    let preset      = RwSignal::new(kb.editor_preset);
    let app_work    = RwSignal::new(kb.app);
    let editor_work = RwSignal::new(kb.editor);

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
    let up_handle = window_event_listener(ev::mouseup, move |_| {
        dragging.set(false);
    });
    let esc_handle = window_event_listener(ev::keydown, move |e| {
        if e.key() == "Escape" {
            ctx.show_settings.set(false);
        }
    });
    on_cleanup(move || { drop(move_handle); drop(up_handle); drop(esc_handle); });

    // Load current settings
    spawn_local(async move {
        if let Ok(s) = invoke::get_settings().await {
            vault_input.set(s.vault_path.unwrap_or_default());
            builder_input.set(s.logseq_site_builder_path.unwrap_or_default());
            if let Some(l) = s.language {
                lang_input.set(l);
            }
        }
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
        let x = modal_x.get();
        if x >= 0 {
            format!("position: fixed; left: {}px; top: {}px; margin: 0;", x, modal_y.get())
        } else {
            String::new()
        }
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

    // Applying a preset overwrites the editor bindings only; app bindings are kept.
    let on_preset_change = move |e: web_sys::Event| {
        let p = event_target_value(&e);
        editor_work.set(keybindings::preset_editor(&p));
        preset.set(p);
    };

    let save = move |_| {
        let vault    = vault_input.get();
        let builder  = builder_input.get();
        let lang_str = lang_input.get();
        let kb = Keybindings {
            editor_preset: preset.get(),
            app:    clean_bindings(app_work.get()),
            editor: clean_bindings(editor_work.get()),
        };

        spawn_local(async move {
            let settings = crate::state::Settings {
                vault_path: if vault.is_empty() { None } else { Some(vault.clone()) },
                logseq_site_builder_path: if builder.is_empty() { None } else { Some(builder) },
                language: Some(lang_str.clone()),
            };
            match invoke::save_settings(&settings).await {
                Ok(_) => {
                    let new_lang = Lang::from_str(&lang_str);
                    ctx.lang.set(new_lang);
                    if let Some(ref path) = settings.vault_path {
                        if ctx.vault_path.get().as_deref() != Some(path.as_str()) {
                            if let Ok(files) = invoke::open_vault(path).await {
                                ctx.vault_path.set(Some(path.clone()));
                                ctx.files.set(files);
                                ctx.tabs.set(vec![]);
                                ctx.active_tab.set(None);
                                ctx.backlinks.set(vec![]);
                            }
                        }
                    }
                    ctx.status.set(Some(t("settings_saved", new_lang).to_string()));
                    ctx.show_settings.set(false);
                }
                Err(e) => ctx.status.set(Some(format!("Error: {e}"))),
            }
            match invoke::set_keybindings(&kb).await {
                Ok(_) => ctx.keybindings.set(kb),
                Err(e) => ctx.status.set(Some(format!("Keybindings error: {e}"))),
            }
        });
    };

    view! {
        <div class="modal-overlay" on:click=move |_| ctx.show_settings.set(false)>
            <div
                node_ref=modal_ref
                class="modal"
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

                    {move || (panel.get() == 1).then(|| view! {
                        <div class="tab-content">
                            <div class="setting-section-title">
                                {move || t("shortcut_section_app", lang())}
                            </div>
                            {keybindings::APP_ACTIONS.iter().map(|a| view! {
                                <KeybindingRow map=app_work action=a lang=ctx.lang />
                            }).collect_view()}

                            <div class="setting-section-title">
                                {move || t("shortcut_section_editor", lang())}
                            </div>
                            <div class="setting-row">
                                <label>{move || t("editor_preset", lang())}</label>
                                <select
                                    prop:value=move || preset.get()
                                    on:change=on_preset_change
                                >
                                    {keybindings::EDITOR_PRESETS.iter().map(|p| view! {
                                        <option value=*p>
                                            {move || t(preset_label_key(p), lang())}
                                        </option>
                                    }).collect_view()}
                                </select>
                            </div>
                            {move || keybindings::editor_actions(&preset.get()).iter().map(|a| view! {
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
                        class="btn-primary"
                        on:click=save
                        on:mousedown=|e: web_sys::MouseEvent| e.stop_propagation()
                    >{move || t("save", lang())}</button>
                </div>
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

    let on_keydown = move |e: web_sys::KeyboardEvent| {
        e.prevent_default();
        if e.key() != "Escape" {
            if let Some(binding) = keybindings::capture_from_event(&e) {
                map.update(|m| {
                    let v = m.entry(id.to_string()).or_default();
                    while v.len() <= index { v.push(String::new()); }
                    v[index] = binding;
                });
            }
        }
        recording.set(false);
    };

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
                        placeholder={move || t("recording", lang.get())}
                        on:keydown=on_keydown
                        on:blur=move |_| recording.set(false)
                    />
                }.into_any()
            } else {
                view! {
                    <button
                        class="keybinding-field"
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
