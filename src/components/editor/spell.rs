//! The spelling extension in the editor: the words of the page the dictionary
//! doesn't know, underlined in red; their suggestions in the right-click menu;
//! the next one (`spell_next`, Emacs' flyspell-goto-next-error).

use std::collections::HashSet;

use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

use super::{
    dom::{caret, set_cursor, splice, Textarea},
    menu_item,
    pane::Pane,
};
use crate::{highlight, i18n::t, invoke, keybindings::after_ms, tables};

/// Wait after the last key before checking the page again.
const CHECK_DELAY_MS: i32 = 300;

/// A misspelled word of the page: its page range and its text.
#[derive(Debug, Clone, PartialEq)]
pub struct Word {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

/// Keep `pane.misspelled` up to date: the words of the page are sent to the
/// dictionary once typing stops, again when the dictionary changes.
pub fn watch(pane: Pane) {
    let ctx = pane.ctx;
    // The words of the page to check, with their text
    let words = Memo::new(move |_| {
        if !ctx.pref(|p| p.spell) { return Vec::new(); }
        pane.content.with(|c| {
            let chars: Vec<char> = c.chars().collect();
            orgamnesia_core::spell::words(c).into_iter()
                .map(|(a, b)| (a, b, chars[a..b].iter().collect::<String>()))
                .collect::<Vec<_>>()
        })
    });
    // What chooses the dictionary: the page is checked again when it changes,
    // not on every setting changed
    let dictionary = Memo::new(move |_| ctx.pref(|p| (p.spell_lang.clone(), p.spell_dict_dir.clone(), p.lang.clone())));
    // Of them, those the dictionary doesn't know
    let unknown = RwSignal::new(HashSet::<String>::new());
    let seq = StoredValue::new(0u32);
    // The last error shown: not shown again on every check
    let shown_error = StoredValue::new(None::<String>);
    Effect::new(move |checked_before: Option<()>| {
        let mut list: Vec<String> = words.with(|w| w.iter().map(|(_, _, text)| text.clone()).collect());
        list.sort();
        list.dedup();
        // Another dictionary, or a word added to it
        ctx.spell_words.track();
        dictionary.track();
        let n = seq.get_value() + 1;
        seq.set_value(n);
        if list.is_empty() {
            unknown.set(HashSet::new());
            return;
        }
        let delay = if checked_before.is_some() { CHECK_DELAY_MS } else { 0 };
        after_ms(delay, move || {
            if seq.try_get_value() != Some(n) { return; }
            spawn_local(async move {
                let found = invoke::spell_check(&list).await;
                // Only the answer to the last question counts
                if seq.try_get_value() != Some(n) { return; }
                match found {
                    Ok(bad) => {
                        unknown.set(bad.into_iter().collect());
                        shown_error.set_value(None);
                    }
                    Err(e) => {
                        unknown.set(HashSet::new());
                        if shown_error.get_value().as_ref() != Some(&e) { ctx.notify(e.clone()); }
                        shown_error.set_value(Some(e));
                    }
                }
            });
        });
    });
    Effect::new(move |_| {
        let bad = unknown.with(|u| words.with(|w| {
            w.iter().filter(|(_, _, text)| u.contains(text)).map(|&(a, b, _)| (a, b)).collect::<Vec<_>>()
        }));
        if pane.misspelled.with_untracked(|m| *m != bad) { pane.misspelled.set(bad); }
    });
}

/// The spelling layer of the view: the misspelled words drawn there as text (not in
/// a drawn table nor a fold), but the one being typed (the caret at its end).
pub fn layer_html(pane: Pane, view: Memo<tables::View>) -> String {
    if !pane.ctx.pref(|p| p.spell) { return String::new(); }
    let typing = pane.focus_pos.get();
    pane.misspelled.with(|bad| view.with(|v| {
        let words: Vec<_> = bad.iter()
            .filter(|&&(_, b)| typing != Some(b))
            .filter_map(|&(a, b)| {
                let (va, vb) = (v.view_pos(a), v.view_pos(b));
                (vb.checked_sub(va) == Some(b - a)).then_some(((va, vb), (a, b)))
            })
            .collect();
        highlight::render_spelling(&v.text, &words)
    }))
}

/// The misspelled word drawn under the point `x`, `y` of the window in the editor `wrap`.
pub fn word_under(pane: Pane, wrap: &web_sys::Element, x: i32, y: i32) -> Option<Word> {
    let (x, y) = (x as f64, y as f64);
    let marks = wrap.query_selector_all(".spell-layer mark.spell").ok()?;
    let mark = (0..marks.length())
        .filter_map(|i| marks.item(i)?.dyn_into::<web_sys::Element>().ok())
        .find(|m| {
            let rects = m.get_client_rects();
            (0..rects.length()).filter_map(|i| rects.item(i)).any(|r| {
                r.left() <= x && x <= r.right() && r.top() <= y && y <= r.bottom()
            })
        })?;
    let start: usize = mark.get_attribute("data-a")?.parse().ok()?;
    let end: usize = mark.get_attribute("data-b")?.parse().ok()?;
    if !pane.misspelled.with_untracked(|m| m.contains(&(start, end))) { return None; }
    let text = pane.content.with_untracked(|c| c.chars().skip(start).take(end - start).collect());
    Some(Word { start, end, text })
}

/// Put `with` in place of the misspelled `word`, if the page still holds it there.
fn replace(pane: Pane, word: &Word, with: &str) {
    let Some(el) = pane.area.get_untracked() else { return };
    let (a, b) = (pane.view_pos(word.start), pane.view_pos(word.end));
    let shown: String = el.value().chars().skip(a).take(b.saturating_sub(a)).collect();
    if shown != word.text { return; }
    let _ = el.focus();
    let end = splice(&el, a, b, with);
    set_cursor(&el, end);
    pane.commit(&el);
}

/// The first items of the right-click menu on a misspelled word: its suggestions,
/// and adding it to the dictionary.
pub fn menu_items(pane: Pane, word: Word, menu: RwSignal<Option<(i32, i32)>>) -> impl IntoView {
    let ctx = pane.ctx;
    let lang = move || ctx.lang.get();
    // None: still asked to the dictionary
    let suggestions = RwSignal::new(None::<Vec<String>>);
    {
        let text = word.text.clone();
        spawn_local(async move {
            let found = invoke::spell_suggest(&text).await.unwrap_or_default();
            let _ = suggestions.try_set(Some(found));
        });
    }
    let word = StoredValue::new(word);
    let add = move || {
        let text = word.with_value(|w| w.text.clone());
        spawn_local(async move {
            match invoke::spell_add(&text).await {
                Ok(()) => ctx.spell_words.update(|n| *n += 1),
                Err(e) => ctx.error("spell_add_error", &e),
            }
        });
    };
    view! {
        {move || match suggestions.get() {
            None => menu_item(move || t("spell_searching", lang()), false, menu, || ()).into_any(),
            Some(list) if list.is_empty() => {
                menu_item(move || t("spell_no_suggestion", lang()), false, menu, || ()).into_any()
            }
            Some(list) => list.into_iter().map(|s| {
                let with = s.clone();
                menu_item(s, true, menu, move || word.with_value(|w| replace(pane, w, &with)))
            }).collect_view().into_any(),
        }}
        {menu_item(move || t("spell_add_word", lang()), true, menu, add)}
        <div class="ctx-menu-sep" role="separator" />
    }
}

/// Move the caret to the start of the next misspelled word after it, back to the
/// first one past the last.
pub fn next(pane: Pane, el: &Textarea) {
    let ctx = pane.ctx;
    if !ctx.pref_untracked(|p| p.spell) { return; }
    let pos = pane.page_pos(el, caret(el));
    let target = pane.misspelled.with_untracked(|bad| {
        bad.iter().find(|&&(a, _)| a > pos).or(bad.first()).copied()
    });
    match target {
        Some((start, _)) => pane.select_page_range(start, start, false),
        None => ctx.notify_t("spell_no_error", ""),
    }
}
