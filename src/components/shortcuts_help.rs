//! The cheat sheet (F1): the shortcuts of the active profile, by theme, with a search.

use leptos::{ev, html, prelude::*};

use crate::{
    i18n::{t, Lang},
    keybindings::{
        self, AppAction, Bindings, EditorAction, Keybindings,
        GIT_ACTIONS, MARK_ACTIONS, SPLIT_ACTIONS, TODO_ACTIONS,
    },
    state::{AppCtx, Prefs},
};

/// An action of the cheat sheet: its label key and its shortcuts, shown.
struct Entry {
    label_key: &'static str,
    keys: Vec<String>,
}

/// The actions with a shortcut, by section (title key), as the shortcuts tab of
/// the settings groups them. Sections of an extension that is off are left out.
fn sections(kb: &Keybindings, prefs: &Prefs) -> Vec<(&'static str, Vec<Entry>)> {
    let entry = |map: &Bindings, id: &str, label_key: &'static str| {
        let keys: Vec<String> = map.get(id).into_iter().flatten()
            .filter(|b| !b.is_empty()).map(|b| keybindings::display(b)).collect();
        (!keys.is_empty()).then_some(Entry { label_key, keys })
    };
    let app = |keep: &dyn Fn(&AppAction) -> bool| keybindings::APP_ACTIONS.iter()
        .filter(|a| keep(&a.action))
        .filter_map(|a| entry(&kb.app, a.id, a.label_key))
        .collect::<Vec<_>>();
    let editor = |keep: &dyn Fn(&EditorAction) -> bool| keybindings::EDITOR_ACTIONS.iter()
        .filter(|a| keep(&a.action))
        .filter_map(|a| entry(&kb.editor, a.id, a.label_key))
        .collect::<Vec<_>>();
    let mut out = vec![
        ("shortcut_section_app", app(&|a| !SPLIT_ACTIONS.contains(a) && !GIT_ACTIONS.contains(a))),
        ("shortcut_section_split", app(&|a| SPLIT_ACTIONS.contains(a))),
        ("shortcut_section_editor", editor(&|a| !MARK_ACTIONS.contains(a) && !TODO_ACTIONS.contains(a))),
    ];
    if prefs.emacs_mark { out.push(("mark_section", editor(&|a| MARK_ACTIONS.contains(a)))); }
    if prefs.git_ext { out.push(("git_section", app(&|a| GIT_ACTIONS.contains(a)))); }
    if prefs.todo_enabled { out.push(("todo_section", editor(&|a| TODO_ACTIONS.contains(a)))); }
    out
}

/// Lowercase, without accents: what the search compares.
fn fold(s: &str) -> String {
    s.trim().chars().map(orgamnesia_core::names::fold_char).collect()
}

/// The sections narrowed to the folded search `q`: a match on a section's
/// title keeps all of it, otherwise the actions whose name or shortcut match.
fn filter(sections: Vec<(&'static str, Vec<Entry>)>, q: &str, lang: Lang) -> Vec<(&'static str, Vec<Entry>)> {
    sections.into_iter()
        .map(|(title, entries)| {
            if q.is_empty() || fold(t(title, lang)).contains(q) { return (title, entries); }
            let entries = entries.into_iter()
                .filter(|e| fold(t(e.label_key, lang)).contains(q) || e.keys.iter().any(|k| fold(k).contains(q)))
                .collect();
            (title, entries)
        })
        .filter(|(_, entries)| !entries.is_empty())
        .collect()
}

#[component]
pub fn ShortcutsHelp() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();
    let close = move || ctx.ui.show_shortcuts.set(false);

    let query = RwSignal::new(String::new());
    let input_ref = NodeRef::<html::Input>::new();
    Effect::new(move |_| {
        if let Some(el) = input_ref.get() { let _ = el.focus(); }
    });
    let esc_handle = window_event_listener(ev::keydown, move |e| {
        if e.key() == "Escape" { close(); }
    });
    on_cleanup(move || esc_handle.remove());

    let shown = move || {
        let all = ctx.keybindings.with(|kb| ctx.prefs.with(|p| sections(kb, p)));
        filter(all, &fold(&query.get()), lang())
    };

    view! {
        <div class="quick-open-overlay" on:click=move |_| close()>
            <div
                class="quick-open-modal shortcuts-help"
                role="dialog"
                aria-label=move || t("shortcuts_help_title", lang())
                on:click=|e: web_sys::MouseEvent| e.stop_propagation()
            >
                <div class="shortcuts-help-header">
                    <h2>{move || t("shortcuts_help_title", lang())}</h2>
                    <span class="shortcuts-help-profile">
                        {move || ctx.keybindings.with(|kb| keybindings::profile_name(kb, lang()))}
                    </span>
                    <button class="btn-close" aria-label=move || t("close", lang()) on:click=move |_| close()>"×"</button>
                </div>
                <input
                    node_ref=input_ref
                    class="quick-open-input"
                    type="search"
                    placeholder=move || t("shortcut_search", lang())
                    prop:value=move || query.get()
                    on:input=move |e| query.set(event_target_value(&e))
                    on:keydown=move |e: web_sys::KeyboardEvent| {
                        // Escape first empties the search, then closes
                        if e.key() == "Escape" && !query.get_untracked().is_empty() {
                            e.stop_propagation();
                            query.set(String::new());
                        }
                    }
                />
                <div class="shortcuts-help-list">
                    {move || {
                        let list = shown();
                        if list.is_empty() {
                            return view! {
                                <div class="quick-open-empty">{t("shortcut_search_empty", lang())}</div>
                            }.into_any();
                        }
                        list.into_iter().map(|(title, entries)| view! {
                            <section class="shortcuts-help-section">
                                <h3 class="setting-section-title">{t(title, lang())}</h3>
                                <dl>
                                    {entries.into_iter().map(|e| view! {
                                        <dt>{t(e.label_key, lang())}</dt>
                                        <dd>{e.keys.into_iter().map(|k| view! { <kbd>{k}</kbd> }).collect_view()}</dd>
                                    }).collect_view()}
                                </dl>
                            </section>
                        }).collect_view().into_any()
                    }}
                </div>
                <div class="shortcuts-help-footer">
                    {move || t("shortcuts_help_edit", lang())}
                </div>
            </div>
        </div>
    }
}
