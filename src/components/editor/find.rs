//! Find / replace in the page: the bar over the editor and the matches.

use leptos::prelude::*;

use super::{
    dom::{after_tick, get_pos, reveal, set_cursor, set_selection, splice, Textarea},
    pane::Pane,
};
use crate::{
    edit,
    i18n::t,
    keybindings::{EditorAction, Resolution, Scope},
};

/// The find bar of an editor pane. Whether it is open is the pane's `find`.
#[derive(Clone, Copy)]
pub struct Find {
    pane: Pane,
    find_ref: NodeRef<leptos::html::Input>,
    replace_ref: NodeRef<leptos::html::Input>,
    query: RwSignal<String>,
    replacement: RwSignal<String>,
    match_case: RwSignal<bool>,
    /// Page ranges of the matches, and the one selected.
    pub matches: Memo<Vec<(usize, usize)>>,
    pub current: RwSignal<Option<usize>>,
    /// Where the search started: the first match shown is the one after it.
    anchor: StoredValue<usize>,
}

impl Find {
    pub fn new(pane: Pane) -> Self {
        let ctx = pane.ctx;
        let query = RwSignal::new(String::new());
        // Starts as the settings say, and follows them when they change
        let match_case = RwSignal::new(false);
        Effect::new(move |_| match_case.set(ctx.pref(|p| p.find_match_case)));
        let matches = Memo::new(move |_| {
            if pane.find.with(|f| f.is_none()) { return vec![]; }
            let q = query.get();
            pane.content.with(|c| edit::find_all(&c.chars().collect::<Vec<_>>(), &q, match_case.get()))
        });
        Self {
            pane,
            find_ref: NodeRef::new(),
            replace_ref: NodeRef::new(),
            query,
            replacement: RwSignal::new(String::new()),
            match_case,
            matches,
            current: RwSignal::new(None),
            anchor: StoredValue::new(0),
        }
    }

    pub fn is_open(self) -> bool {
        self.pane.find.get_untracked().is_some()
    }

    /// Select match `i` in the text and bring it into view.
    fn show_match(self, i: usize) {
        let Some(&(a, b)) = self.matches.with_untracked(|m| m.get(i).copied()).as_ref() else {
            self.current.set(None);
            return;
        };
        self.current.set(Some(i));
        self.anchor.set_value(a);
        self.pane.focus_pos.set(Some(a));
        if let Some(el) = self.pane.area.get_untracked() {
            let (a, b) = (self.pane.view_pos(a), self.pane.view_pos(b));
            set_selection(&el, a, b);
            reveal(&el, &el.value().chars().collect::<Vec<_>>(), a);
        }
    }

    /// The first match at or after the anchor.
    fn show_from_anchor(self) {
        let at = self.anchor.get_value();
        let n = self.matches.with_untracked(|m| m.iter().position(|&(a, _)| a >= at).or((!m.is_empty()).then_some(0)));
        match n {
            Some(i) => self.show_match(i),
            None => self.current.set(None),
        }
    }

    fn step(self, forward: bool) {
        let n = self.matches.with_untracked(|m| m.len());
        if n == 0 { return; }
        let i = match self.current.get_untracked() {
            Some(i) if forward => (i + 1) % n,
            Some(i) => (i + n - 1) % n,
            None if forward => 0,
            None => n - 1,
        };
        self.show_match(i);
    }

    /// Open the bar (with the replace row), looking for the selected text if any.
    pub fn open(self, with_replace: bool, el: &Textarea) {
        let (a, b) = get_pos(el);
        let selected = self.pane.page_text(el, a, b);
        if !selected.is_empty() && !selected.contains('\n') {
            self.query.set(selected);
        }
        self.anchor.set_value(self.pane.page_pos(el, a));
        // Matches may be hidden: everything is unfolded
        self.pane.folds.set(vec![]);
        self.pane.find.set(Some(with_replace || self.pane.find.get_untracked().unwrap_or(false)));
        // Once the textarea shows every table as text
        after_tick(move || {
            let Some(query) = self.query.try_get_untracked() else { return };
            self.show_from_anchor();
            let target = if with_replace && !query.is_empty() { self.replace_ref } else { self.find_ref };
            if let Some(input) = target.try_get_untracked().flatten() {
                let _ = input.focus();
                input.select();
            }
        });
    }

    pub fn close(self) {
        self.pane.find.set(None);
        self.current.set(None);
        if let Some(el) = self.pane.area.get_untracked() { let _ = el.focus(); }
    }

    fn replace_current(self) {
        let Some(el) = self.pane.area.get_untracked() else { return };
        let Some((a, b)) = self.current.get_untracked().and_then(|i| self.matches.with_untracked(|m| m.get(i).copied())) else {
            self.step(true);
            return;
        };
        let with = self.replacement.get_untracked();
        splice(&el, self.pane.view_pos(a), self.pane.view_pos(b), &with);
        self.pane.commit(&el);
        self.anchor.set_value(a + with.chars().count());
        self.show_from_anchor();
    }

    fn replace_all(self) {
        let ctx = self.pane.ctx;
        let Some(el) = self.pane.area.get_untracked() else { return };
        let found = self.matches.get_untracked();
        if found.is_empty() { return; }
        // Replaced in the page, which then takes the place of the whole view
        let chars: Vec<char> = self.pane.content.get_untracked().chars().collect();
        let all = el.value().chars().count();
        let out = edit::replace_ranges(&chars, &found, &self.replacement.get_untracked());
        // A single edit, so one undo brings everything back
        splice(&el, 0, all, &out);
        set_cursor(&el, 0);
        self.pane.commit(&el);
        self.current.set(None);
        ctx.notify(format!("{} × {}", found.len(), t("replace_all", ctx.lang.get_untracked())));
    }

    /// Keys typed in the find or the replace field.
    fn on_key(self, e: web_sys::KeyboardEvent, in_replace: bool) {
        let plain = !e.ctrl_key() && !e.alt_key() && !e.meta_key();
        let handled = match e.key().as_str() {
            "Escape" if plain => { self.close(); true }
            "Enter" if plain && in_replace && !e.shift_key() => { self.replace_current(); true }
            "Enter" if plain => { self.step(!e.shift_key()); true }
            _ => match crate::keybindings::resolve(&self.pane.ctx.keybindings.get_untracked(), &e) {
                Resolution::Action(Scope::Editor, id) => match EditorAction::from_id(&id) {
                    Some(EditorAction::Find) => { self.step(true); true }
                    Some(EditorAction::FindPrevious) => { self.step(false); true }
                    Some(EditorAction::KeyboardQuit) => { self.close(); true }
                    Some(EditorAction::Replace) => {
                        self.pane.find.set(Some(true));
                        let replace_ref = self.replace_ref;
                        after_tick(move || if let Some(i) = replace_ref.try_get_untracked().flatten() { let _ = i.focus(); });
                        true
                    }
                    _ => false,
                },
                _ => false,
            },
        };
        if handled {
            e.prevent_default();
            e.stop_propagation();
        }
    }

    /// The bar, when open.
    pub fn bar(self) -> impl IntoView {
        let ctx = self.pane.ctx;
        let Find { query, replacement, match_case, matches, current, .. } = self;
        move || self.pane.find.get().map(|with_replace| {
            let lang = ctx.lang.get();
            view! {
                <div class="find-bar" on:mousedown=|e: web_sys::MouseEvent| e.stop_propagation()>
                    <div class="find-row">
                        <button
                            class=if with_replace { "find-btn find-toggle open" } else { "find-btn find-toggle" }
                            title=t("find_replace_toggle", lang)
                            on:click=move |_| self.pane.find.set(Some(!with_replace))
                        >"›"</button>
                        <input
                            node_ref=self.find_ref
                            class="find-input"
                            type="text"
                            placeholder=t("find_placeholder", lang)
                            prop:value=move || query.get()
                            on:input=move |e| { query.set(event_target_value(&e)); self.show_from_anchor(); }
                            on:keydown=move |e| self.on_key(e, false)
                        />
                        <span class=move || if !query.get().is_empty() && matches.with(|m| m.is_empty()) {
                            "find-count none"
                        } else { "find-count" }>
                            {move || {
                                let n = matches.with(|m| m.len());
                                if query.get().is_empty() { String::new() }
                                else if n == 0 { t("find_none", ctx.lang.get()).to_string() }
                                else { format!("{}/{n}", current.get().map_or(0, |i| i + 1)) }
                            }}
                        </span>
                        <button
                            class=move || if match_case.get() { "find-btn active" } else { "find-btn" }
                            title=t("find_case_title", lang)
                            on:click=move |_| { match_case.update(|c| *c = !*c); self.show_from_anchor(); }
                        >"Aa"</button>
                        <button class="find-btn" title=t("find_prev_title", lang) on:click=move |_| self.step(false)>"↑"</button>
                        <button class="find-btn" title=t("find_next_title", lang) on:click=move |_| self.step(true)>"↓"</button>
                        <button class="find-btn" on:click=move |_| self.close()>"×"</button>
                    </div>
                    {with_replace.then(|| view! {
                        <div class="find-row find-replace-row">
                            <input
                                node_ref=self.replace_ref
                                class="find-input"
                                type="text"
                                placeholder=t("replace_placeholder", lang)
                                prop:value=move || replacement.get()
                                on:input=move |e| replacement.set(event_target_value(&e))
                                on:keydown=move |e| self.on_key(e, true)
                            />
                            <button class="find-btn find-text-btn" on:click=move |_| self.replace_current()>
                                {t("replace_one", lang)}
                            </button>
                            <button class="find-btn find-text-btn" on:click=move |_| self.replace_all()>
                                {t("replace_all", lang)}
                            </button>
                        </div>
                    })}
                </div>
            }
        })
    }
}
