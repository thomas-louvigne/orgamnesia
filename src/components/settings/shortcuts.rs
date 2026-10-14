//! The shortcuts of the settings window: the keybinding profiles, and a row of
//! two slots per action to record its shortcuts in.

use leptos::{html, prelude::*};

use super::fields::Check;
use crate::{
    i18n::{t, Lang},
    keybindings::{self, AppAction, Bindings, EditorAction, Keybindings, Profile},
    state::{AppCtx, Prefs},
};

/// Application actions about the editor panes; shown in their own section.
const SPLIT_ACTIONS: &[AppAction] = &[
    AppAction::SplitVertical, AppAction::SplitHorizontal, AppAction::CloseSplit,
    AppAction::SingleWindow, AppAction::OtherWindow,
];

/// Editor actions of the selection marker; shown in their own section.
const MARK_ACTIONS: &[EditorAction] = &[EditorAction::SetMark, EditorAction::KeyboardQuit];

/// Application actions of the git extension; shown in their own section.
const GIT_ACTIONS: &[AppAction] = &[AppAction::GitCommit, AppAction::GitPull, AppAction::GitPush];

/// Editor actions of the TODO states; shown in their own section.
const TODO_ACTIONS: &[EditorAction] = &[EditorAction::TodoNext, EditorAction::TodoPrev];

/// Combinations bound to more than one action, with the label keys of those actions.
pub type Conflicts = Vec<(String, Vec<&'static str>)>;

/// Application and editor shortcuts share one keyboard, so a combination may
/// only be given to a single action across both lists. A shortcut that is the
/// first key(s) of another action's chord ("ctrl+x" vs "ctrl+x ctrl+s") is a
/// conflict too: the chord could never be completed. Giving the same
/// combination to both slots of one action is fine.
fn find_conflicts(app: &Bindings, editor: &Bindings) -> Conflicts {
    // (binding, action label), one entry per distinct binding of each action
    let mut entries: Vec<(String, &'static str)> = Vec::new();
    let app_ids = keybindings::APP_ACTIONS.iter().map(|a| (app, a.id, a.label_key));
    let editor_ids = keybindings::EDITOR_ACTIONS.iter().map(|a| (editor, a.id, a.label_key));
    for (map, id, label_key) in app_ids.chain(editor_ids) {
        let mut seen = std::collections::HashSet::new();
        for b in map.get(id).into_iter().flatten().filter(|b| !b.is_empty()) {
            if seen.insert(b.clone()) {
                entries.push((b.clone(), label_key));
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
        let conflicts = Memo::new(move |_| app.with(|a| editor.with(|e| find_conflicts(a, e))));
        provide_context(conflicts);
        let this = Self { active_id, profiles, preset: RwSignal::new(kb.editor_preset), app, editor, conflicts };
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
    let ctx = use_context::<AppCtx>().expect("AppCtx");
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
        let q = fold(&query.get());
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

/// Lowercase, without accents: what the search compares.
fn fold(s: &str) -> String {
    s.trim().chars().map(orgamnesia_core::names::fold_char).collect()
}

/// One action of the shortcuts tab: where its bindings are kept, its id and label.
#[derive(Clone, Copy)]
struct Row {
    map: RwSignal<Bindings>,
    id: &'static str,
    label_key: &'static str,
}

impl Row {
    /// Whether the folded search `q` is in the action's name or one of its shortcuts.
    fn matches(&self, q: &str, lang: Lang) -> bool {
        fold(t(self.label_key, lang)).contains(q)
            || self.map.with(|m| m.get(self.id).into_iter().flatten()
                .any(|b| !b.is_empty() && fold(&keybindings::display(b)).contains(q)))
    }
}

/// A setting shown as a checkbox: how to read it, how to set it, its label.
type CheckDef = (fn(&Prefs) -> bool, fn(&mut Prefs, bool), &'static str);

/// A group of actions under a title, with the setting that goes with them.
#[derive(Clone)]
struct Section {
    title: &'static str,
    check: Option<CheckDef>,
    rows: Vec<Row>,
}

impl Section {
    /// A search on the section's title shows all of it.
    fn title_matches(&self, q: &str, lang: Lang) -> bool {
        fold(t(self.title, lang)).contains(q)
    }

    fn matches(&self, q: &str, lang: Lang) -> bool {
        self.title_matches(q, lang) || self.rows.iter().any(|r| r.matches(q, lang))
    }
}

/// A section of the shortcuts tab, hidden with its rows when the search leaves nothing of it.
#[component]
fn ShortcutSection(draft: RwSignal<Prefs>, section: Section, query: RwSignal<String>) -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");
    let lang = move || ctx.lang.get();
    let title = section.title;
    let check = section.check;
    let rows = section.rows.clone();
    let shown = move || {
        let q = fold(&query.get());
        q.is_empty() || section.matches(&q, lang())
    };
    let whole = {
        let section = Section { title, check, rows: vec![] };
        move || {
            let q = fold(&query.get());
            q.is_empty() || section.title_matches(&q, lang())
        }
    };
    view! {
        <div class="shortcut-section" class:hidden=move || !shown()>
            <div class="setting-section-title">{move || t(title, lang())}</div>
            {check.map(|(get, set, label)| view! { <Check draft get set label /> })}
            <KeybindingHeader lang=ctx.lang />
            {rows.into_iter().map(|row| {
                let whole = whole.clone();
                let row_shown = move || whole() || row.matches(&fold(&query.get()), lang());
                view! {
                    <div class="shortcut-row" class:hidden=move || !row_shown()>
                        <KeybindingRow map=row.map id=row.id label_key=row.label_key lang=ctx.lang />
                    </div>
                }
            }).collect_view()}
        </div>
    }
}

/// Titles of the two shortcut columns.
#[component]
fn KeybindingHeader(lang: Memo<Lang>) -> impl IntoView {
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
    id: &'static str,
    label_key: &'static str,
    lang: Memo<Lang>,
) -> impl IntoView {
    view! {
        <div class="keybinding-row">
            <label>{move || t(label_key, lang.get())}</label>
            <div class="keybinding-slots">
                <KeybindingSlot map=map id=id index=0 lang=lang />
                <KeybindingSlot map=map id=id index=1 lang=lang />
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
    lang: Memo<Lang>,
) -> impl IntoView {
    let recording = RwSignal::new(false);
    let input_ref = NodeRef::<html::Input>::new();

    Effect::new(move |_| {
        if recording.get() && let Some(el) = input_ref.get() {
            let _ = el.focus();
        }
    });

    let current = move || {
        map.with(|m| m.get(id).and_then(|v| v.get(index)).cloned().unwrap_or_default())
    };
    let conflicts = use_context::<Memo<Conflicts>>();
    let in_conflict = move || {
        let c = current();
        !c.is_empty() && conflicts.is_some_and(|cf| cf.with(|l| {
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
            if let Some(v) = m.get_mut(id) && index < v.len() { v[index] = String::new(); }
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
                        title=move || keybindings::display(&current())
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
