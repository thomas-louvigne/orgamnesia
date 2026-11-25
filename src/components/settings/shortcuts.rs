//! The shortcuts of the settings window: the keybinding profiles, and a row of
//! two slots per action to record its shortcuts in.

use leptos::{html, prelude::*};
use orgamnesia_core::names::fold;

use super::fields::Check;
use crate::{
    i18n::{t, Lang},
    keybindings::{
        self, preset_label_key, AppAction, Bindings, Conflicts, EditorAction, Keybindings, Profile,
        GIT_ACTIONS, MARK_ACTIONS, SPLIT_ACTIONS, TODO_ACTIONS,
    },
    state::{AppCtx, Prefs},
};
mod rows;
use rows::*;

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

/// `base`, or `base 2`, `base 3`… : the first name no profile of `profiles` has.
fn free_name(base: &str, profiles: &[Profile]) -> String {
    (1..).map(|n| if n == 1 { base.to_string() } else { format!("{base} {n}") })
        .find(|name| !profiles.iter().any(|p| &p.name == name))
        .expect("names never run out")
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
    let name = profiles.with_untracked(|ps| free_name(t("profile_custom", lang), ps));
    let id = format!("custom-{}", js_sys::Date::now() as u64);
    profiles.update(|ps| ps.push(Profile {
        id: id.clone(), name, editor_preset: base_preset, app, editor,
    }));
    active_id.set(id);
}

/// The keybindings being edited: the profiles, the active one and its working
/// copies (action id -> up to 2 bindings).
#[derive(Clone, Copy)]
pub struct KbDraft {
    active_id: RwSignal<String>,
    profiles: RwSignal<Vec<Profile>>,
    preset: RwSignal<String>,
    app: RwSignal<Bindings>,
    editor: RwSignal<Bindings>,
    /// Shortcuts given to several actions (blocks saving).
    pub conflicts: Memo<Conflicts>,
}

/// What the settings window compares to tell whether there is anything to apply.
#[derive(Clone, PartialEq)]
pub struct KbSnapshot {
    active: String,
    profiles: Vec<Profile>,
    app: Bindings,
    editor: Bindings,
}

impl KbDraft {
    pub fn new(ctx: AppCtx) -> Self {
        let kb = ctx.keybindings.get();
        let active_id = RwSignal::new(kb.resolved_active());
        let profiles = RwSignal::new(kb.profiles);
        let app = RwSignal::new(kb.app);
        let editor = RwSignal::new(kb.editor);
        let conflicts = Memo::new(move |_| app.with(|a| editor.with(|e| keybindings::conflicts(a, e))));
        provide_context(conflicts);
        let this = Self { active_id, profiles, preset: RwSignal::new(kb.editor_preset), app, editor, conflicts };
        // The slots record the keys as the profile's base preset reads them
        provide_context(this);
        this.fork_on_edit(ctx);
        this
    }

    /// Any edit made while a built-in profile is active forks it into a new custom
    /// profile; while a custom profile is active the edit updates that profile.
    /// Switching profiles sets the working copies to the profile's own bindings,
    /// so it never counts as an edit.
    fn fork_on_edit(self, ctx: AppCtx) {
        let Self { active_id, profiles, .. } = self;
        Effect::new(move |_| {
            let app = self.app.get();
            let editor = self.editor.get();
            let id = active_id.get_untracked();
            if keybindings::is_builtin(&id) {
                if app != keybindings::preset_app(&id) || editor != keybindings::preset_editor(&id) {
                    add_profile(active_id, profiles, ctx.lang.get_untracked(), id, app, editor);
                }
            } else {
                let unchanged = profiles.with_untracked(|ps| {
                    ps.iter().find(|p| p.id == id).is_none_or(|p| p.app == app && p.editor == editor)
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
    }

    pub fn snapshot(self) -> KbSnapshot {
        KbSnapshot {
            active: self.active_id.get(),
            profiles: self.profiles.get(),
            app: self.app.get(),
            editor: self.editor.get(),
        }
    }

    /// The keybindings to save; a profile without a name gets `default_name`.
    pub fn to_keybindings(self, default_name: &str) -> Keybindings {
        Keybindings {
            editor_preset: self.preset.get(),
            app:    clean_bindings(self.app.get()),
            editor: clean_bindings(self.editor.get()),
            active_profile: self.active_id.get(),
            profiles: self.profiles.get().into_iter().map(|mut p| {
                p.app = clean_bindings(p.app);
                p.editor = clean_bindings(p.editor);
                if p.name.trim().is_empty() { p.name = default_name.to_string(); }
                p
            }).collect(),
        }
    }

    /// Load a profile (built-in or custom) into the working copies.
    fn select_profile(self, id: String) {
        let (base, app, editor) = if keybindings::is_builtin(&id) {
            (id.clone(), keybindings::preset_app(&id), keybindings::preset_editor(&id))
        } else if let Some(p) = self.profiles.with_untracked(|ps| ps.iter().find(|p| p.id == id).cloned()) {
            (p.editor_preset, p.app, p.editor)
        } else {
            return;
        };
        self.active_id.set(id);
        self.preset.set(base);
        self.app.set(app);
        self.editor.set(editor);
    }

    /// Back to the default built-in profile; the user's profiles stay.
    pub fn reset(self) {
        self.select_profile(Keybindings::default().active_profile);
    }

    fn duplicate_profile(self, lang: Lang) {
        add_profile(
            self.active_id, self.profiles, lang, self.preset.get_untracked(),
            self.app.get_untracked(), self.editor.get_untracked(),
        );
    }

    fn delete_profile(self) {
        let id = self.active_id.get_untracked();
        let base = self.preset.get_untracked();
        self.profiles.update(|ps| ps.retain(|p| p.id != id));
        self.select_profile(base);
    }

    fn rename_profile(self, name: String) {
        let id = self.active_id.get_untracked();
        self.profiles.update(|ps| {
            if let Some(p) = ps.iter_mut().find(|p| p.id == id) { p.name = name; }
        });
    }
}

/// The shortcuts tab.
#[component]
pub fn ShortcutsTab(draft: RwSignal<Prefs>, kb: KbDraft) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();
    let KbDraft { active_id, profiles, app: app_work, editor: editor_work, conflicts, .. } = kb;
    let on_profile_change = move |e: web_sys::Event| kb.select_profile(event_target_value(&e));
    let duplicate_profile = move |_| kb.duplicate_profile(ctx.lang.get_untracked());
    let delete_profile = move |_| kb.delete_profile();
    let rename_profile = move |e: web_sys::Event| kb.rename_profile(event_target_value(&e));

    let query = RwSignal::new(String::new());
    let search_ref = NodeRef::<html::Input>::new();
    Effect::new(move |_| {
        if let Some(el) = search_ref.get() { let _ = el.focus(); }
    });
    let app_rows = |keep: fn(&AppAction) -> bool| keybindings::APP_ACTIONS.iter()
        .filter(|a| keep(&a.action))
        .map(|a| Row { map: app_work, id: a.id, label_key: a.label_key })
        .collect::<Vec<_>>();
    let editor_rows = |keep: fn(&EditorAction) -> bool| keybindings::EDITOR_ACTIONS.iter()
        .filter(|a| keep(&a.action))
        .map(|a| Row { map: editor_work, id: a.id, label_key: a.label_key })
        .collect::<Vec<_>>();
    let sections = vec![
        Section {
            title: "shortcut_section_app", check: None,
            rows: app_rows(|a| !SPLIT_ACTIONS.contains(a) && !GIT_ACTIONS.contains(a)),
        },
        Section { title: "shortcut_section_split", check: None, rows: app_rows(|a| SPLIT_ACTIONS.contains(a)) },
        Section {
            title: "shortcut_section_editor", check: None,
            rows: editor_rows(|a| !MARK_ACTIONS.contains(a) && !TODO_ACTIONS.contains(a)),
        },
        Section {
            title: "mark_section",
            check: Some((|p| p.emacs_mark, |p, v| p.emacs_mark = v, "emacs_mark")),
            rows: editor_rows(|a| MARK_ACTIONS.contains(a)),
        },
        Section { title: "git_section", check: None, rows: app_rows(|a| GIT_ACTIONS.contains(a)) },
        Section {
            title: "todo_section",
            check: Some((|p| p.todo_enabled, |p, v| p.todo_enabled = v, "todo_enabled")),
            rows: editor_rows(|a| TODO_ACTIONS.contains(a)),
        },
    ];
    let all = sections.clone();
    let nothing_found = move || {
        let q = fold(query.get().trim());
        !q.is_empty() && !all.iter().any(|s| s.matches(&q, lang()))
    };
    view! {
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
            <Check draft get=|p| p.webkit_emoji_shortcuts set=|p, v| p.webkit_emoji_shortcuts = v
                label="webkit_emoji_shortcuts" hint="webkit_emoji_shortcuts_hint" />
            <Check draft get=|p| p.shortcut_tips set=|p, v| p.shortcut_tips = v
                label="shortcut_tips" hint="shortcut_tips_hint" />
            <div class="shortcut-search">
                <input
                    node_ref=search_ref
                    type="search"
                    class="setting-input"
                    placeholder=move || t("shortcut_search", lang())
                    prop:value=move || query.get()
                    on:input=move |e| query.set(event_target_value(&e))
                    on:keydown=move |e: web_sys::KeyboardEvent| {
                        // Escape first empties the search, then closes the window
                        if e.key() == "Escape" && !query.get_untracked().is_empty() {
                            e.stop_propagation();
                            query.set(String::new());
                        }
                    }
                />
            </div>
            {move || nothing_found().then(|| view! {
                <div class="setting-hint">{move || t("shortcut_search_empty", lang())}</div>
            })}
            {sections.into_iter().map(|section| view! { <ShortcutSection draft section query /> }).collect_view()}
        </div>
    }
}

#[cfg(test)]
mod tests;
