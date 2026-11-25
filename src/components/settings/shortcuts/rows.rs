//! A section of the shortcuts tab and its rows: an action, its label and its
//! two slots, each recorded by typing the keys.

use super::*;

/// One action of the shortcuts tab: where its bindings are kept, its id and label.
#[derive(Clone, Copy)]
pub(super) struct Row {
    pub(super) map: RwSignal<Bindings>,
    pub(super) id: &'static str,
    pub(super) label_key: &'static str,
}

impl Row {
    /// Whether the folded search `q` is in the action's name or one of its shortcuts.
    pub(super) fn matches(&self, q: &str, lang: Lang) -> bool {
        fold(t(self.label_key, lang)).contains(q)
            || self.map.with(|m| m.get(self.id).into_iter().flatten()
                .any(|b| !b.is_empty() && fold(&keybindings::display(b)).contains(q)))
    }
}

/// A setting shown as a checkbox: how to read it, how to set it, its label.
type CheckDef = (fn(&Prefs) -> bool, fn(&mut Prefs, bool), &'static str);

/// A group of actions under a title, with the setting that goes with them.
#[derive(Clone)]
pub(super) struct Section {
    pub(super) title: &'static str,
    pub(super) check: Option<CheckDef>,
    pub(super) rows: Vec<Row>,
}

impl Section {
    /// A search on the section's title shows all of it.
    pub(super) fn title_matches(&self, q: &str, lang: Lang) -> bool {
        fold(t(self.title, lang)).contains(q)
    }

    pub(super) fn matches(&self, q: &str, lang: Lang) -> bool {
        self.title_matches(q, lang) || self.rows.iter().any(|r| r.matches(q, lang))
    }
}

/// A section of the shortcuts tab, hidden with its rows when the search leaves nothing of it.
#[component]
pub(super) fn ShortcutSection(draft: RwSignal<Prefs>, section: Section, query: RwSignal<String>) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();
    let title = section.title;
    let check = section.check;
    let rows = section.rows.clone();
    let shown = move || {
        let q = fold(query.get().trim());
        q.is_empty() || section.matches(&q, lang())
    };
    let whole = {
        let section = Section { title, check, rows: vec![] };
        move || {
            let q = fold(query.get().trim());
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
                let row_shown = move || whole() || row.matches(&fold(query.get().trim()), lang());
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
pub(super) fn KeybindingHeader(lang: Memo<Lang>) -> impl IntoView {
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
pub(super) fn KeybindingRow(
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
pub(super) fn KeybindingSlot(
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
    let draft = use_context::<KbDraft>();
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
        let cmd_is_ctrl = keybindings::is_mac() && draft.is_none_or(|d| d.preset.get_untracked() != "emacs");
        if e.ctrl_key() || (cmd_is_ctrl && e.meta_key()) { s.push_str("Ctrl+"); }
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
        let emacs = draft.is_some_and(|d| d.preset.get_untracked() == "emacs");
        match keybindings::capture_from_event(&e, emacs) {
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
