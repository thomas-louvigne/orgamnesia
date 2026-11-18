//! Completion of page names (`#tag` and `[[link]]`) and org-mode `:tags:`
//! while they are typed: a popup under the caret.

use leptos::prelude::*;

use super::{
    dom::{caret_coords, get_pos, set_cursor, splice, Textarea},
    pane::Pane,
};
use crate::{links, tags};
use orgamnesia_core::links::headlines;
use wasm_bindgen_futures::spawn_local;

const MAX_SUGGESTIONS: usize = 8;
/// Characters to type after `[[` before pages are suggested.
const LINK_MIN_CHARS: usize = 2;
const POPUP_WIDTH: f64 = 260.0;
const ITEM_HEIGHT: f64 = 26.0;

/// What is being completed.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    /// A page name in `[[link`: accepting also closes the link with `]]`.
    Link,
    /// A page name after `#`.
    Hashtag,
    /// An org-mode tag (`* Title :tag`, `#+FILETAGS:`): accepting adds the closing `:`.
    OrgTag,
    /// A headline of a page in `[[Page::*Titre` or `[[*Titre`: accepting also
    /// closes the link with `]]`.
    Place,
    /// A headline of a page in `[[Page#Titre` (when that syntax is read): the
    /// same, written without the `*`.
    HashPlace,
    /// The file of a page in a Markdown link `[texte](Pa`, written `%`-encoded:
    /// accepting also closes the link with `)`.
    MdLink,
}

impl Kind {
    /// What closes the name once accepted.
    fn close(self) -> &'static str {
        match self {
            Kind::Link | Kind::Place | Kind::HashPlace => "]]",
            Kind::MdLink => ")",
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
            Kind::Place => "::",
            Kind::HashPlace => "#",
            Kind::MdLink => "](",
        }
    }
}

/// What is being typed at `caret` that names can complete: where the typed part
/// starts, that part, and what it is.
fn being_typed(chars: &[char], caret: usize, tags: orgamnesia_core::TagSyntax, format: orgamnesia_core::Format) -> Option<(usize, String, Kind)> {
    links::link_prefix(chars, caret)
        .filter(|(_, p)| p.trim().chars().count() >= LINK_MIN_CHARS)
        .map(|(start, p)| (start, p, Kind::Link))
        .or_else(|| tags.links(format).markdown.then(|| md_link_prefix(chars, caret)).flatten()
            .filter(|(_, p)| p.trim().chars().count() >= LINK_MIN_CHARS)
            .map(|(start, p)| (start, p, Kind::MdLink)))
        .or_else(|| tags::org_tag_prefix(chars, caret, tags.org).map(|(start, p)| (start, p, Kind::OrgTag)))
        .or_else(|| tags::hashtag_prefix(chars, caret, tags.hashtags).map(|(hash, p)| (hash + 1, p, Kind::Hashtag)))
}

/// The target of a Markdown link being typed at `caret` (`[texte](Pa`): where it
/// starts, and what is typed of it; not an address, nor a place (`#titre`).
fn md_link_prefix(chars: &[char], caret: usize) -> Option<(usize, String)> {
    let start = caret - chars[..caret].iter().rev().take_while(|&&c| !matches!(c, '(' | ')' | '[' | ']' | '\n')).count();
    if start < 2 || chars[start - 1] != '(' || chars[start - 2] != ']' { return None; }
    let typed: String = chars[start..caret].iter().collect();
    (!typed.contains("://") && !typed.contains('#') && !typed.starts_with('<')).then_some((start, typed))
}

/// The change accepting a name makes: `insert` replaces the characters `from..to`,
/// then the caret goes to `cursor`.
#[derive(Debug, PartialEq)]
struct Accepted { from: usize, to: usize, insert: String, cursor: usize }

/// Accepting `name` for what was typed between `start` and `caret` in `chars`.
/// In a link already closed (`[[jeu de r|ôle][texte]]`) the name replaces the
/// whole target and the caret goes past `]]` (or before `][`, to the text shown);
/// otherwise the closing characters of `kind` are added, or stepped over when
/// already there.
fn accepted(chars: &[char], start: usize, caret: usize, kind: Kind, name: &str) -> Accepted {
    if kind == Kind::MdLink {
        // The file name, `Ma%20page.md`; a target already closed is replaced whole
        let name = orgamnesia_core::links::percent_encode(name);
        let target_end = caret + chars[caret..].iter().take_while(|&&ch| !matches!(ch, ')' | '\n')).count();
        let after_name = start + name.chars().count();
        return if chars.get(target_end) == Some(&')') {
            Accepted { from: start, to: target_end, insert: name, cursor: after_name + 1 }
        } else {
            Accepted { from: start, to: caret, insert: format!("{name})"), cursor: after_name + 1 }
        };
    }
    let target_end = caret + chars[caret..].iter().take_while(|&&ch| !matches!(ch, ']' | '[' | '\n')).count();
    let closed = matches!(kind, Kind::Link | Kind::Place | Kind::HashPlace)
        && matches!((chars.get(target_end), chars.get(target_end + 1)), (Some(']'), Some(']' | '[')));
    let after_name = start + name.chars().count();
    if closed {
        let past = if chars.get(target_end + 1) == Some(&']') { 2 } else { 0 };
        return Accepted { from: start, to: target_end, insert: name.to_string(), cursor: after_name + past };
    }
    let close = kind.close();
    let n = close.chars().count();
    let there = chars.iter().skip(caret).take(n).collect::<String>() == close;
    let insert = if there { name.to_string() } else { format!("{name}{close}") };
    Accepted { from: start, to: caret, insert, cursor: after_name + n }
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
        let tags = ctx.tags_untracked();
        let found = (caret == end).then(|| being_typed(&chars, caret, tags, self.pane.fmt())).flatten();
        let Some((start, prefix, kind)) = found else { self.close(); return };
        if kind == Kind::Link && let Some((page, typed, at)) = links::place_prefix(&prefix) {
            self.complete_place(el, start + at, caret, page, typed, Kind::Place);
            return;
        }
        if kind == Kind::Link && tags.links(self.pane.fmt()).hash && let Some((page, typed)) = prefix.split_once('#').filter(|(page, _)| !page.trim().is_empty()) {
            let at = page.chars().count() + 1;
            self.complete_place(el, start + at, caret, page.trim().to_string(), typed.to_string(), Kind::HashPlace);
            return;
        }
        let items = match kind {
            Kind::MdLink => {
                // The files of the pages, with their extension (`Ma page.md`)
                let names: Vec<String> = ctx.project.files.with_untracked(|fs| {
                    fs.iter().map(|f| orgamnesia_core::pages::file_name(&f.path).to_string()).collect()
                });
                let typed = orgamnesia_core::links::percent_decode(&prefix);
                links::complete_page(&names, &typed, None, MAX_SUGGESTIONS)
            }
            Kind::OrgTag => {
                let names: Vec<String> = ctx.project.tags.get_untracked().into_iter().map(|t| t.name).collect();
                links::complete_page(&names, &prefix, None, MAX_SUGGESTIONS)
            }
            _ => {
                let names: Vec<String> = ctx.link_names();
                let tag = (kind == Kind::Hashtag).then(|| ctx.tags_untracked().hashtags);
                links::complete_page(&names, &prefix, tag, MAX_SUGGESTIONS)
            }
        };
        self.show(el, &chars, start, caret, kind, items);
    }

    /// Suggest the headlines of `page` (this one when empty) for the place typed
    /// after `[[page::`, which starts at `start`. The page may have to be read first.
    fn complete_place(self, el: &Textarea, start: usize, caret: usize, page: String, typed: String, kind: Kind) {
        let el = el.clone();
        spawn_local(async move {
            let Some((text, keywords, format)) = self.pane.named_page_text(&page).await else { self.close(); return };
            let titles = headlines(&text, keywords.as_ref(), format);
            let typed = typed.strip_prefix('*').unwrap_or(&typed);
            let titles: Vec<String> = if typed.trim().is_empty() {
                titles.into_iter().take(MAX_SUGGESTIONS).collect()
            } else {
                links::complete_page(&titles, typed, None, MAX_SUGGESTIONS)
            };
            // The caret moved while the page was read: this is not the question any more
            if self.shown.try_get_untracked().is_none() || get_pos(&el) != (caret, caret) { return; }
            let star = if kind == Kind::Place { "*" } else { "" };
            let items = titles.into_iter().map(|t| format!("{star}{t}")).collect();
            let chars: Vec<char> = el.value().chars().collect();
            self.show(&el, &chars, start, caret, kind, items);
        });
    }

    /// Show `items` in the popup, for the name typed between `start` and `caret`.
    fn show(self, el: &Textarea, chars: &[char], start: usize, caret: usize, kind: Kind, items: Vec<String>) {
        if items.is_empty() { self.close(); return; }
        let Some((x, top, bottom)) = caret_coords(el, chars, start) else { return };
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
        let chars: Vec<char> = el.value().chars().collect();
        let edit = accepted(&chars, c.start, c.caret, c.kind, name);
        splice(el, edit.from, edit.to, &edit.insert);
        let cursor = edit.cursor;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn accept(text: &str, start: usize, caret: usize, kind: Kind, name: &str) -> (String, usize) {
        let mut chars: Vec<char> = text.chars().collect();
        let a = accepted(&chars, start, caret, kind, name);
        chars.splice(a.from..a.to, a.insert.chars());
        (chars.into_iter().collect(), a.cursor)
    }

    #[test]
    fn a_link_is_closed() {
        assert_eq!(accept("voir [[jeu", 7, 10, Kind::Link, "Jeu de rôle"), ("voir [[Jeu de rôle]]".into(), 20));
        // Its end already typed: stepped over
        assert_eq!(accept("[[je]] x", 2, 4, Kind::Link, "Jeu"), ("[[Jeu]] x".into(), 7));
    }

    #[test]
    fn a_closed_link_gets_its_whole_target_replaced() {
        assert_eq!(accept("[[jeu de r][texte]]", 2, 5, Kind::Link, "Jeu de rôle"), ("[[Jeu de rôle][texte]]".into(), 13));
        assert_eq!(accept("[[jeu de r]] x", 2, 5, Kind::Link, "Jeu"), ("[[Jeu]] x".into(), 7));
    }

    #[test]
    fn tags() {
        assert_eq!(accept("#pro", 1, 4, Kind::Hashtag, "projet"), ("#projet".into(), 7));
        assert_eq!(accept("* T :pro", 5, 8, Kind::OrgTag, "projet"), ("* T :projet:".into(), 12));
    }

    fn typed(text: &str, tags: orgamnesia_core::TagSyntax, format: orgamnesia_core::Format) -> Option<(usize, String, Kind)> {
        let chars: Vec<char> = text.chars().collect();
        being_typed(&chars, chars.len(), tags, format)
    }

    #[test]
    fn what_is_typed() {
        use orgamnesia_core::Format::{Markdown, Org};
        let tags = crate::state::Prefs::default().tags();
        assert_eq!(typed("voir [[je", tags, Org), Some((7, "je".into(), Kind::Link)));
        // One letter after `[[` is not enough
        assert!(typed("voir [[j", tags, Org).is_none_or(|(_, _, k)| k != Kind::Link));
        // A Markdown link, when that syntax is read in the page's format
        assert_eq!(typed("voir [texte](Ma pa", tags, Markdown), Some((13, "Ma pa".into(), Kind::MdLink)));
        let mut no_md = tags;
        no_md.md_links.markdown = false;
        assert!(typed("voir [texte](Ma pa", no_md, Markdown).is_none_or(|(_, _, k)| k != Kind::MdLink));
        // Not an address, a place, or a link already passed
        for text in ["[t](https://ex", "[t](Page.md#ti", "[t](Pa) suite", "f(xy"] {
            assert!(typed(text, tags, Markdown).is_none_or(|(_, _, k)| k != Kind::MdLink), "{text}");
        }
    }

    #[test]
    fn a_markdown_link_is_encoded_and_closed() {
        assert_eq!(accept("[t](Ma pa", 4, 9, Kind::MdLink, "Ma page.md"), ("[t](Ma%20page.md)".into(), 17));
        // Already closed: its whole target replaced, the caret past `)`
        assert_eq!(accept("[t](Ma pa.md) x", 4, 9, Kind::MdLink, "Ma page.md"), ("[t](Ma%20page.md) x".into(), 17));
    }

    #[test]
    fn a_hash_place_closes_the_link() {
        assert_eq!(accept("[[Page#Ti", 7, 9, Kind::HashPlace, "Titre"), ("[[Page#Titre]]".into(), 14));
    }
}
