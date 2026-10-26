//! The windows inserting a link (org-mode's `C-c C-l`) or a date (`C-c .`,
//! `C-c !`) at the caret; on a link or a date, they edit it.

use leptos::prelude::*;

use super::{dom::{get_pos, set_cursor, splice, Textarea}, pane::Pane};
use crate::{edit::{self, Stamp}, i18n::t};

/// What the open window inserts.
#[derive(Clone)]
enum Kind {
    Link { target: String, description: String },
    Date(Stamp),
}

/// The window open, if any: the range of the textarea it replaces, and what it inserts.
#[derive(Clone, Copy)]
pub struct Insert {
    pane: Pane,
    open: RwSignal<Option<(usize, usize, Kind)>>,
}

impl Insert {
    pub fn new(pane: Pane) -> Self {
        Self { pane, open: RwSignal::new(None) }
    }

    /// Open the link window: on a link, to edit it; else the selected text is
    /// its description.
    pub fn link(self, el: &Textarea) {
        let chars: Vec<char> = el.value().chars().collect();
        let (start, end) = get_pos(el);
        let open = match edit::link_around(&chars, start) {
            Some(l) => (l.start, l.end, Kind::Link { target: l.target, description: l.description }),
            None => (start, end, Kind::Link { target: String::new(), description: chars[start..end].iter().collect() }),
        };
        self.open.set(Some(open));
    }

    /// Open the date window, `active` or not: on a date, to edit it; else today's.
    pub fn date(self, el: &Textarea, active: bool) {
        let chars: Vec<char> = el.value().chars().collect();
        let (_, end) = get_pos(el);
        let open = match edit::stamp_around(&chars, end) {
            Some((a, b, stamp)) => (a, b, Kind::Date(Stamp { active, ..stamp })),
            None => {
                let now = js_sys::Date::new_0();
                let today = Stamp {
                    year: now.get_full_year() as i32, month: now.get_month() + 1, day: now.get_date(),
                    time: None, active, rest: String::new(),
                };
                (end, end, Kind::Date(today))
            }
        };
        self.open.set(Some(open));
    }

    /// The window, while it is open.
    pub fn dialog(self) -> impl IntoView {
        move || self.open.get().map(|(a, b, kind)| dialog(self, a, b, kind))
    }

    /// Close the window; with `text`, it replaces the range it was opened on.
    fn close(self, range: (usize, usize), text: Option<String>) {
        self.open.set(None);
        let Some(el) = self.pane.area.get_untracked() else { return };
        let _ = el.focus();
        match text {
            Some(text) => {
                let cursor = splice(&el, range.0, range.1, &text);
                set_cursor(&el, cursor);
                self.pane.commit(&el);
            }
            None => set_cursor(&el, range.1),
        }
    }
}

fn dialog(insert: Insert, a: usize, b: usize, kind: Kind) -> impl IntoView {
    let ctx = insert.pane.ctx;
    let lang = move || ctx.lang.get();
    let first = NodeRef::<leptos::html::Input>::new();
    Effect::new(move |_| if let Some(el) = first.get() { let _ = el.focus(); });
    let cancel = move || insert.close((a, b), None);

    // The fields, and the text they make (`None` while it can't be made)
    let (title, body, text): (&'static str, AnyView, Box<dyn Fn() -> Option<String> + Send + Sync>) = match kind {
        Kind::Link { target, description } => {
            let target = RwSignal::new(target);
            let description = RwSignal::new(description);
            let pages: Vec<String> = ctx.project.files.get_untracked().into_iter().map(|f| f.name).collect();
            let body = view! {
                <div class="setting-row">
                    <label for="insert-link-target">{move || t("link_target", lang())}</label>
                    <input id="insert-link-target" node_ref=first type="text" class="setting-input" list="insert-link-pages"
                        prop:value=move || target.get() on:input=move |e| target.set(event_target_value(&e)) />
                    <datalist id="insert-link-pages">
                        {pages.into_iter().map(|p| view! { <option value=p /> }).collect_view()}
                    </datalist>
                </div>
                <div class="setting-row">
                    <label for="insert-link-description">{move || t("link_description", lang())}</label>
                    <input id="insert-link-description" type="text" class="setting-input"
                        prop:value=move || description.get() on:input=move |e| description.set(event_target_value(&e)) />
                </div>
            }.into_any();
            let text = move || target.with(|tg| (!tg.trim().is_empty()).then(|| edit::format_link(tg, &description.get())));
            ("link_title", body, Box::new(text))
        }
        Kind::Date(stamp) => {
            let date = RwSignal::new(stamp.date_field());
            let time = RwSignal::new(stamp.time_field());
            let active = RwSignal::new(stamp.active);
            let rest = stamp.rest;
            let body = view! {
                <div class="setting-input-row">
                    <div class="setting-row">
                        <label for="insert-date-day">{move || t("date_day", lang())}</label>
                        <input id="insert-date-day" node_ref=first type="date" class="setting-input"
                            prop:value=move || date.get() on:input=move |e| date.set(event_target_value(&e)) />
                    </div>
                    <div class="setting-row">
                        <label for="insert-date-time">{move || t("date_time", lang())}</label>
                        <input id="insert-date-time" type="time" class="setting-input"
                            prop:value=move || time.get() on:input=move |e| time.set(event_target_value(&e)) />
                    </div>
                </div>
                <label class="setting-check">
                    <input type="checkbox" prop:checked=move || active.get()
                        on:change=move |e| active.set(event_target_checked(&e)) />
                    {move || t("date_active", lang())}
                </label>
                <span class="setting-hint">{move || t("date_active_hint", lang())}</span>
            }.into_any();
            let text = move || Stamp::from_fields(&date.get(), &time.get(), active.get(), &rest).map(|s| s.text());
            ("date_title", body, Box::new(text))
        }
    };
    let text = StoredValue::new(text);
    let preview = move || text.with_value(|f| f());
    let confirm = move || if let Some(s) = preview() { insert.close((a, b), Some(s)); };
    let on_keydown = move |e: web_sys::KeyboardEvent| {
        // The editor's and the app's shortcuts don't run in the window
        e.stop_propagation();
        match e.key().as_str() {
            "Escape" => { e.prevent_default(); cancel(); }
            "Enter" => { e.prevent_default(); confirm(); }
            _ => { crate::text_field::on_keydown(ctx, &e); }
        }
    };

    view! {
        <div class="modal-overlay" on:click=move |_| cancel()>
            <div class="modal insert-modal" role="dialog" aria-label=move || t(title, lang())
                on:click=|e| e.stop_propagation() on:keydown=on_keydown>
                <div class="modal-header">
                    <h2>{move || t(title, lang())}</h2>
                    <button class="btn-close" aria-label=move || t("close", lang()) on:click=move |_| cancel()>"×"</button>
                </div>
                <div class="modal-body">
                    {body}
                    <code class="insert-preview">{move || preview().unwrap_or_default()}</code>
                </div>
                <div class="modal-footer">
                    <button class="btn-secondary" on:click=move |_| cancel()>{move || t("cancel", lang())}</button>
                    <button class="btn-primary" prop:disabled=move || preview().is_none() on:click=move |_| confirm()>
                        {move || t("ok", lang())}
                    </button>
                </div>
            </div>
        </div>
    }
}
