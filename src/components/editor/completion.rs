//! Completion of page names (`#tag` and `[[link]]`) and org-mode `:tags:`
//! while they are typed: a popup under the caret.

use leptos::prelude::*;

use super::{
    dom::{caret_coords, get_pos, set_cursor, splice, Textarea},
    pane::Pane,
};
use crate::motion;

const MAX_SUGGESTIONS: usize = 8;
/// Characters to type after `[[` before pages are suggested.
const LINK_MIN_CHARS: usize = 2;
const POPUP_WIDTH: f64 = 260.0;
const ITEM_HEIGHT: f64 = 26.0;

/// What is being completed.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    /// A page name in `[[link`: accepting also closes the link with `]]`.
    Link,
    /// A page name after `#`.
    Hashtag,
    /// An org-mode tag (`* Title :tag`, `#+FILETAGS:`): accepting adds the closing `:`.
    OrgTag,
}

impl Kind {
    /// What closes the name once accepted.
    fn close(self) -> &'static str {
        match self {
            Kind::Link => "]]",
            Kind::OrgTag => ":",
            Kind::Hashtag => "",
        }
    }

    /// What the popup shows before each name.
    fn sigil(self) -> &'static str {
        match self {
            Kind::Link => "[[",
            Kind::Hashtag => "#",
            Kind::OrgTag => ":",
        }
    }
}

/// Suggestions shown while a `#tag`, a `[[link` or a `:tag:` is being typed.
#[derive(Clone, PartialEq)]
struct Suggestions {
    /// The typed part of the name lies between `start` and the caret.
    start: usize,
    caret: usize,
    kind: Kind,
    items: Vec<String>,
    selected: usize,
    /// Popup position in pixels, relative to the editor wrapper.
    x: f64,
    y: f64,
}

/// The completion of an editor pane.
#[derive(Clone, Copy)]
pub struct Completion {
    pane: Pane,
    shown: RwSignal<Option<Suggestions>>,
    /// A name is being put in the text: its input doesn't refresh the popup.
    accepting: StoredValue<bool>,
}

impl Completion {
    pub fn new(pane: Pane) -> Self {
        Self { pane, shown: RwSignal::new(None), accepting: StoredValue::new(false) }
    }

    pub fn close(self) {
        self.shown.set(None);
    }

    /// The text changed: suggest names for what is being typed at the caret.
    pub fn on_input(self, el: &Textarea) {
        if !self.accepting.get_value() { self.refresh(el); }
    }

    fn refresh(self, el: &Textarea) {
        let ctx = self.pane.ctx;
        let (caret, end) = get_pos(el);
        let chars: Vec<char> = el.value().chars().collect();
        let found = if caret != end { None } else {
            motion::link_prefix(&chars, caret)
                .filter(|(_, p)| p.trim().chars().count() >= LINK_MIN_CHARS)
                .map(|(start, p)| (start, p, Kind::Link))
                .or_else(|| motion::org_tag_prefix(&chars, caret, ctx.tags_untracked().org)
                    .map(|(start, p)| (start, p, Kind::OrgTag)))
                .or_else(|| motion::hashtag_prefix(&chars, caret, ctx.tags_untracked().hashtags)
                    .map(|(hash, p)| (hash + 1, p, Kind::Hashtag)))
        };
        let Some((start, prefix, kind)) = found else { self.close(); return };
        let items = match kind {
            Kind::OrgTag => {
                let names: Vec<String> = ctx.project.tags.get_untracked().into_iter().map(|t| t.name).collect();
                motion::complete_page(&names, &prefix, None, MAX_SUGGESTIONS)
            }
            _ => {
                let names: Vec<String> = ctx.project.files.get_untracked().into_iter().map(|f| f.name).collect();
                let tag = (kind == Kind::Hashtag).then(|| ctx.tags_untracked().hashtags);
                motion::complete_page(&names, &prefix, tag, MAX_SUGGESTIONS)
            }
        };
        if items.is_empty() { self.close(); return; }
        let Some((x, top, bottom)) = caret_coords(el, &chars, start) else { return };
        // Keep the popup inside the editor: flip above the line near the bottom edge
        let height = items.len() as f64 * ITEM_HEIGHT + 8.0;
        let y = if bottom + height > el.client_height() as f64 && top > height { top - height } else { bottom };
        let x = x.min(el.client_width() as f64 - POPUP_WIDTH).max(0.0);
        self.shown.set(Some(Suggestions { start, caret, kind, items, selected: 0, x, y }));
    }

    /// Put `name` in the text, in place of what was typed of it.
    fn accept(self, el: &Textarea, name: &str) {
        let Some(c) = self.shown.get_untracked() else { return };
        self.accepting.set_value(true);
        let mut cursor = splice(el, c.start, c.caret, name);
        // Close the link / tag: step over the closing characters if already there, else add them
        let close = c.kind.close();
        if !close.is_empty() {
            let n = close.chars().count();
            let after: String = el.value().chars().skip(cursor).take(n).collect();
            cursor = if after == close { cursor + n } else { splice(el, cursor, cursor, close) };
        }
        self.accepting.set_value(false);
        set_cursor(el, cursor);
        self.close();
        self.pane.commit(el);
    }

    /// With the popup open, the keys navigating, accepting or dismissing it,
    /// before any shortcut; true when the key was one of them.
    pub fn on_keydown(self, e: &web_sys::KeyboardEvent, el: &Textarea) -> bool {
        let Some(c) = self.shown.get_untracked() else { return false };
        let n = c.items.len();
        let plain = !e.ctrl_key() && !e.alt_key() && !e.meta_key() && !e.shift_key();
        let ctrl_only = e.ctrl_key() && !e.alt_key() && !e.meta_key() && !e.shift_key();
        let select = |i: usize| self.shown.set(Some(Suggestions { selected: i, ..c.clone() }));
        let key = e.key();
        let handled = match key.as_str() {
            "ArrowDown" if plain => { select((c.selected + 1) % n); true }
            "n" if ctrl_only     => { select((c.selected + 1) % n); true }
            "ArrowUp" if plain   => { select((c.selected + n - 1) % n); true }
            "p" if ctrl_only     => { select((c.selected + n - 1) % n); true }
            "Enter" | "Tab" if plain => { self.accept(el, &c.items[c.selected]); true }
            "Escape" if plain    => { self.close(); true }
            "g" if ctrl_only     => { self.close(); true }
            _ => false,
        };
        if handled {
            e.prevent_default();
            e.stop_propagation();
            return true;
        }
        // Moving the caret elsewhere closes the popup; typing refreshes it (on input)
        if matches!(key.as_str(), "ArrowLeft" | "ArrowRight" | "Home" | "End" | "PageUp" | "PageDown")
            || e.ctrl_key() || e.alt_key()
        {
            self.close();
        }
        false
    }

    /// The popup, when open.
    pub fn popup(self) -> impl IntoView {
        let area = self.pane.area;
        move || self.shown.get().map(|c| view! {
            <ul
                class="page-completion"
                style=format!("left: {}px; top: {}px; width: {}px;", c.x, c.y, POPUP_WIDTH)
            >
                {c.items.iter().enumerate().map(|(i, name)| {
                    let pick = name.clone();
                    view! {
                        <li
                            class=if i == c.selected { "page-completion-item selected" } else { "page-completion-item" }
                            // mousedown + preventDefault keeps the focus in the textarea
                            on:mousedown=move |e: web_sys::MouseEvent| {
                                e.prevent_default();
                                if let Some(el) = area.get() { self.accept(&el, &pick); }
                            }
                        >
                            <span class="page-completion-sigil">{c.kind.sigil()}</span>
                            {name.clone()}
                        </li>
                    }
                }).collect_view()}
            </ul>
        })
    }
}
