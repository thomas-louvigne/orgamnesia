//! The agenda of a week, as org-mode's (`C-c a a`): the dated headlines of the
//! project, day by day. On a line: `t` the next TODO state, `s` / `d` its
//! dates, Shift+arrows a day later or earlier; `f` / `b` the next or previous
//! week, `.` this one, Enter opens the page there. Beside it, the calendar of
//! the month: the days having a page in the journal marked; a click shows the
//! week of a day, a double click opens its page (made when there is none).

use leptos::prelude::*;
use orgamnesia_core::agenda::{self, When};
use orgamnesia_core::AgendaHit;
use wasm_bindgen_futures::spawn_local;

use super::panel;
use crate::{actions, i18n::{t, Lang}, invoke, state::{AppCtx, Goto}};

/// The name of the day `d` (`lundi 24 novembre`), in the words of `lang`.
fn day_name(d: agenda::Day, lang: Lang) -> String {
    let (y, m, day) = agenda::date_of(d);
    let date = js_sys::Date::new_with_year_month_day(y as u32, m as i32 - 1, day as i32);
    let options = js_sys::Object::new();
    for (k, v) in [("weekday", "long"), ("day", "numeric"), ("month", "long")] {
        let _ = js_sys::Reflect::set(&options, &k.into(), &v.into());
    }
    let locale = if matches!(lang, Lang::Fr) { "fr-FR" } else { "en-GB" };
    String::from(date.to_locale_date_string(locale, &options))
}

/// The month of the day `d` (`novembre 2026`), in the words of `lang`.
fn month_name(d: agenda::Day, lang: Lang) -> String {
    let (y, m, _) = agenda::date_of(d);
    let date = js_sys::Date::new_with_year_month_day(y as u32, m as i32 - 1, 1);
    let options = js_sys::Object::new();
    for (k, v) in [("month", "long"), ("year", "numeric")] {
        let _ = js_sys::Reflect::set(&options, &k.into(), &v.into());
    }
    let locale = if matches!(lang, Lang::Fr) { "fr-FR" } else { "en-GB" };
    String::from(date.to_locale_date_string(locale, &options))
}

/// The day `months` months from the first of the month of `d`.
fn month_moved(d: agenda::Day, months: i32) -> agenda::Day {
    let (y, m, _) = agenda::date_of(d);
    let n = y * 12 + m as i32 - 1 + months;
    agenda::day_of(n.div_euclid(12), n.rem_euclid(12) as u32 + 1, 1)
}

/// The calendar of the month of `month`: the days of the journal marked, the
/// week of `from` shown; a click shows the week of a day, a double click opens its page.
#[component]
fn Calendar(month: RwSignal<agenda::Day>, from: RwSignal<agenda::Day>, today: agenda::Day, show_week: Callback<agenda::Day>, open_day: Callback<agenda::Day>) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();
    let journal = Memo::new(move |_| ctx.project.files.with(|fs| {
        fs.iter().filter_map(|f| agenda::day_of_name(&f.name)).collect::<std::collections::HashSet<_>>()
    }));
    let weekdays = move || if matches!(lang(), Lang::Fr) { ["lu", "ma", "me", "je", "ve", "sa", "di"] } else { ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"] };
    view! {
        <div class="agenda-calendar">
            <div class="calendar-head">
                <button class="btn-secondary" aria-label=move || t("calendar_previous", lang()) title=move || t("calendar_previous", lang())
                    on:click=move |_| month.update(|m| *m = month_moved(*m, -1))>"‹"</button>
                <span class="calendar-month">{move || month_name(month.get(), lang())}</span>
                <button class="btn-secondary" aria-label=move || t("calendar_next", lang()) title=move || t("calendar_next", lang())
                    on:click=move |_| month.update(|m| *m = month_moved(*m, 1))>"›"</button>
            </div>
            <div class="calendar-grid" role="grid">
                {move || weekdays().into_iter().map(|w| view! { <span class="calendar-weekday">{w}</span> }).collect_view()}
                {move || {
                    let shown = agenda::date_of(month.get()).1;
                    let week = from.get();
                    agenda::month_grid(month.get()).into_iter().map(|day| {
                        let (_, m, n) = agenda::date_of(day);
                        let mut class = String::from("calendar-day");
                        if m != shown { class.push_str(" other-month"); }
                        if day == today { class.push_str(" today"); }
                        if (week..week + 7).contains(&day) { class.push_str(" in-week"); }
                        let page = journal.with(|j| j.contains(&day));
                        if page { class.push_str(" has-page"); }
                        let title = format!("{}{}", day_name(day, lang()), if page { format!(" — {}", t("calendar_has_page", lang())) } else { String::new() });
                        view! {
                            <button class=class title=title.clone() aria-label=title
                                on:click=move |_| show_week.run(day)
                                on:dblclick=move |_| open_day.run(day)>{n}</button>
                        }
                    }).collect_view()
                }}
            </div>
            <p class="setting-hint">{move || t("calendar_hint", lang())}</p>
        </div>
    }
}

/// What puts a headline on its day, as org-mode writes it (`Sched.3x:`, `In 2 d.:`).
fn when_label(h: &AgendaHit, lang: Lang) -> String {
    match (h.when, h.late) {
        (When::Scheduled, 0) => t("agenda_scheduled", lang).to_string(),
        (When::Scheduled, n) => format!("{} {n}×", t("agenda_scheduled", lang)),
        (When::Deadline, 0) => t("agenda_deadline", lang).to_string(),
        (When::Deadline, n) if n > 0 => format!("{} −{n} {}", t("agenda_deadline", lang), t("agenda_days", lang)),
        (When::Deadline, n) => format!("{} +{} {}", t("agenda_deadline", lang), -n, t("agenda_days", lang)),
        (When::Timestamp, _) => String::new(),
        (When::Page, _) => t("agenda_day_page", lang).to_string(),
    }
}

#[component]
pub fn AgendaPage() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();
    let close = move || ctx.ui.show_agenda.set(false);
    let root = NodeRef::<leptos::html::Div>::new();
    panel::opened(ctx, root, close);

    let today = actions::today();
    let from = RwSignal::new(agenda::week_start(today));
    // None while read
    let hits = RwSignal::new(None::<Vec<AgendaHit>>);
    let selected = RwSignal::new(0usize);
    let seq = StoredValue::new(0u32);
    // Read from the pages as written: those with unsaved changes are saved first
    let load = move || {
        let n = seq.get_value() + 1;
        seq.set_value(n);
        let first = from.get_untracked();
        spawn_local(async move {
            if !actions::save_dirty_tabs(ctx).await { return; }
            match invoke::get_agenda(first, 7).await {
                Ok(list) if seq.try_get_value() == Some(n) => {
                    let last = list.len().saturating_sub(1);
                    selected.update(|s| *s = (*s).min(last));
                    hits.try_set(Some(list));
                }
                Ok(_) => {}
                Err(e) => ctx.error("error", &e),
            }
        });
    };
    Effect::new(move |_| { from.track(); load(); });
    let week = move |days: i64| { selected.set(0); from.update(|f| *f += days); };
    // The calendar shows the month of the week shown, and can go to others
    let month = RwSignal::new(from.get_untracked());
    Effect::new(move |_| month.set(from.get()));
    let show_week = Callback::new(move |day| { selected.set(0); from.set(agenda::week_start(day)); });
    let open_day = Callback::new(move |day| { close(); actions::follow_link(ctx, actions::day_page_name(day)); });

    let current = move || hits.with_untracked(|h| h.as_ref().and_then(|h| h.get(selected.get_untracked()).cloned()));
    // The page of the day is only opened
    let actionable = move || current().filter(|h| h.when != When::Page);
    let open = move |h: AgendaHit| actions::open_page_named(ctx, &h.page, Some(Goto::Line(h.line)));
    let cycle = move || {
        let Some(h) = actionable() else { return };
        let Some(keywords) = ctx.pref_untracked(|p| p.todo()) else { return ctx.notify_t("agenda_todo_off", "") };
        let format = orgamnesia_core::Format::of_page(&h.path);
        actions::change_page(ctx, h.path.clone(), move |text| agenda::cycle_todo(text, h.line, &keywords, format, true, &actions::now()), load);
    };
    let shift = move |days: i64| {
        let Some(h) = actionable() else { return };
        let keywords = ctx.pref_untracked(|p| p.todo());
        let format = orgamnesia_core::Format::of_page(&h.path);
        actions::change_page(ctx, h.path.clone(), move |text| {
            let e = agenda::entries(text, keywords.as_ref(), format).into_iter().find(|e| e.line == h.line)?;
            agenda::shift(text, &e, h.when, h.stamp, days).map(|e| vec![e])
        }, load);
    };
    let plan = move |when: When| {
        let Some(h) = actionable() else { return };
        let (key, label) = if when == When::Scheduled { (agenda::SCHEDULED, "plan_scheduled") } else { (agenda::DEADLINE, "plan_deadline") };
        let keywords = ctx.pref_untracked(|p| p.todo());
        let format = orgamnesia_core::Format::of_page(&h.path);
        spawn_local(async move {
            let text = match ctx.work.tabs.with_untracked(|tabs| tabs.iter().find(|t| t.path == h.path).map(|t| t.content.get_untracked())) {
                Some(text) => text,
                None => invoke::read_file(&h.path).await.unwrap_or_default(),
            };
            let typed = agenda::entries(&text, keywords.as_ref(), format).iter().find(|e| e.line == h.line)
                .map(|e| agenda::typed_date(e, when)).unwrap_or_default();
            ctx.ask_text(t(label, ctx.lang.get_untracked()).to_string(), typed, vec![], move |input| {
                let line = h.line;
                actions::change_page(ctx, h.path.clone(), move |text| match agenda::plan_typed(text, line, format, key, &input, actions::today()) {
                    Ok(edit) => edit.map(|e| vec![e]),
                    Err(_) => { ctx.notify_t("not_a_date", &input); None }
                }, load);
            });
        });
    };

    let on_keydown = move |e: web_sys::KeyboardEvent| {
        let n = hits.with_untracked(|h| h.as_ref().map_or(0, Vec::len));
        let handled = match (e.key().as_str(), e.shift_key()) {
            ("ArrowDown", false) => { selected.update(|s| *s = (*s + 1).min(n.saturating_sub(1))); true }
            ("ArrowUp", false) => { selected.update(|s| *s = s.saturating_sub(1)); true }
            ("ArrowRight", true) => { shift(1); true }
            ("ArrowLeft", true) => { shift(-1); true }
            ("Enter", _) => { if let Some(h) = current() { open(h); } true }
            ("t", _) => { cycle(); true }
            ("s", _) => { plan(When::Scheduled); true }
            ("d", _) => { plan(When::Deadline); true }
            ("f", _) => { week(7); true }
            ("b", _) => { week(-7); true }
            (".", _) => { selected.set(0); from.set(agenda::week_start(today)); true }
            ("Escape" | "q", _) => { close(); true }
            _ => false,
        };
        if handled { e.prevent_default(); e.stop_propagation(); }
    };

    view! {
        <div node_ref=root class="main-panel" tabindex="-1" role="region" aria-label=move || t("agenda_title", lang())
            on:keydown=on_keydown>
            <div class="modal-header">
                <h2>{move || format!("{} — {}", t("agenda_title", lang()), day_name(from.get(), lang()))}</h2>
                <div class="agenda-nav">
                    <button class="btn-secondary" aria-label=move || t("agenda_previous", lang()) title=move || t("agenda_previous", lang()) on:click=move |_| week(-7)>"‹"</button>
                    <button class="btn-secondary" on:click=move |_| { selected.set(0); from.set(agenda::week_start(today)); }>{move || t("agenda_this_week", lang())}</button>
                    <button class="btn-secondary" aria-label=move || t("agenda_next", lang()) title=move || t("agenda_next", lang()) on:click=move |_| week(7)>"›"</button>
                </div>
                <button class="btn-close" aria-label=move || t("close", lang()) on:click=move |_| close()>"×"</button>
            </div>
            <div class="modal-body agenda-body">
                <Calendar month from today show_week open_day />
                <div class="agenda-week">
                <div class="agenda-actions">
                    <button class="btn-secondary" on:click=move |_| cycle()>{move || t("agenda_state", lang())}</button>
                    <button class="btn-secondary" on:click=move |_| plan(When::Scheduled)>{move || t("agenda_plan", lang())}</button>
                    <button class="btn-secondary" on:click=move |_| plan(When::Deadline)>{move || t("agenda_due", lang())}</button>
                    <button class="btn-secondary" on:click=move |_| shift(-1)>{move || t("agenda_earlier", lang())}</button>
                    <button class="btn-secondary" on:click=move |_| shift(1)>{move || t("agenda_later", lang())}</button>
                    <span class="setting-hint">{move || t("agenda_keys", lang())}</span>
                </div>
                {move || match hits.get() {
                    None => view! { <p class="setting-hint">{t("agenda_reading", lang())}</p> }.into_any(),
                    Some(list) => {
                        let first = from.get();
                        let mut i = 0;
                        (first..first + 7).map(|day| {
                            let items: Vec<(usize, AgendaHit)> = list.iter().filter(|h| h.day == day).map(|h| { i += 1; (i - 1, h.clone()) }).collect();
                            let class = if day == today { "agenda-day today" } else { "agenda-day" };
                            view! {
                                <h3 class=class>{day_name(day, lang())}</h3>
                                <ul class="agenda-list" role="listbox">
                                    {items.into_iter().map(|(n, h)| {
                                        let done = if h.done { " is-done" } else { "" };
                                        let late = if h.late > 0 { " is-late" } else { "" };
                                        let label = when_label(&h, lang());
                                        let hit = h.clone();
                                        view! {
                                            <li class=move || format!("agenda-item todo-item list-item{done}{late}{}", if selected.get() == n { " active" } else { "" })
                                                role="option" tabindex="0"
                                                aria-selected=move || (selected.get() == n).to_string()
                                                on:click=move |_| selected.set(n)
                                                on:dblclick=move |_| open(hit.clone())
                                            >
                                                <span class="agenda-page">{h.page.clone()}</span>
                                                <span class="agenda-time">{h.time.clone().unwrap_or_default()}</span>
                                                <span class="agenda-when">{label}</span>
                                                {h.keyword.clone().map(|k| view! { <span class=if h.done { "done" } else { "todo" }>{k}</span> })}
                                                <span class="todo-item-title">{h.title.clone()}</span>
                                            </li>
                                        }
                                    }).collect_view()}
                                </ul>
                            }
                        }).collect_view().into_any()
                    }
                }}
                </div>
            </div>
        </div>
    }
}
