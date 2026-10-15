//! What an editor pane knows of its page: the page text, the view of it the
//! textarea holds (tables and images drawn, folds hidden), the caret and mark.

use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use super::{
    dom::{caret, get_pos, number_digits, set_cursor, table_lines, text_columns, Textarea, CHAR_WIDTH, LINE_HEIGHT},
    images::{image_src, image_url, load_size, ImageSizes},
};
use crate::{images, invoke, state::{AppCtx, Tab}, tables};
use orgamnesia_core::TodoKeywords;

/// The signals of an editor pane, shared by its event handlers.
#[derive(Clone, Copy)]
pub struct Pane {
    pub ctx: AppCtx,
    pub area: NodeRef<leptos::html::Textarea>,
    /// Text of the page, and whether it is unsaved.
    pub content: RwSignal<String>,
    pub dirty: RwSignal<bool>,
    history: StoredValue<crate::history::History>,
    /// None: find bar closed; Some(with_replace).
    pub find: RwSignal<Option<bool>>,
    /// Page position of the caret.
    pub focus_pos: RwSignal<Option<usize>>,
    /// Folded parts of the page (Tab on a headline), as page ranges: see `folding`.
    pub folds: RwSignal<Vec<(usize, usize)>>,
    /// The view the textarea holds: what its text is read with. Until the textarea
    /// gets a new view, an edit is still read with the one it shows.
    pub shown: StoredValue<tables::View>,
    /// Caret in the textarea after the last move.
    last_caret: StoredValue<usize>,
    /// Emacs mark (char index) and the caret ("point"); the region is what lies between.
    pub mark: RwSignal<Option<usize>>,
    pub point: RwSignal<usize>,
    /// TODO keywords of the page (its `#+TODO:` lines, else the settings'); None when off.
    pub todo_keywords: Memo<Option<TodoKeywords>>,
}

impl Pane {
    pub fn new(ctx: AppCtx, tab: &Tab) -> Self {
        let content = tab.content;
        Self {
            ctx,
            area: NodeRef::new(),
            content,
            dirty: tab.dirty,
            history: tab.history,
            find: RwSignal::new(None),
            focus_pos: RwSignal::new(None),
            folds: RwSignal::new(Vec::new()),
            shown: StoredValue::new(tables::View::default()),
            last_caret: StoredValue::new(0),
            mark: RwSignal::new(None),
            point: RwSignal::new(0),
            todo_keywords: Memo::new(move |_| {
                ctx.pref(|p| p.todo()).map(|d| content.with(|c| TodoKeywords::for_page(c, &d)))
            }),
        }
    }

    /// Region to act on: mark..point when a mark is set (and enabled), else the native selection.
    pub fn region_of(self, el: &Textarea) -> (usize, usize) {
        if self.ctx.pref_untracked(|p| p.emacs_mark) && let Some(m) = self.mark.get_untracked() {
            let p = caret(el);
            return (m.min(p), m.max(p));
        }
        get_pos(el)
    }

    /// Page position of the textarea position `pos`.
    pub fn page_pos(self, el: &Textarea, pos: usize) -> usize {
        let text = el.value();
        self.shown.with_value(|v| v.content_pos(&text, pos))
    }

    /// Textarea position of the page position `pos`.
    pub fn view_pos(self, pos: usize) -> usize {
        self.shown.with_value(|v| v.view_pos(pos))
    }

    /// Page text between two positions of the textarea.
    pub fn page_text(self, el: &Textarea, a: usize, b: usize) -> String {
        let text = el.value();
        let (a, b) = self.shown.with_value(|v| (v.content_pos(&text, a), v.content_pos(&text, b)));
        self.content.with_untracked(|c| c.chars().skip(a).take(b.saturating_sub(a)).collect())
    }

    /// The textarea was edited: the page follows.
    pub fn commit(self, el: &Textarea) {
        let text = el.value();
        let (page, pos, folded) = self.shown.with_value(|v| (
            v.to_content(&text),
            v.content_pos(&text, caret(el)),
            (!v.folds.is_empty()).then(|| v.fold_ranges(&text)),
        ));
        // The folds move with the text
        if let Some(f) = folded { self.folds.set(f); }
        self.record(&page);
        self.focus_pos.set(Some(pos));
        self.content.set(page);
        self.dirty.set(true);
    }

    /// The page changed without the textarea: it gets the new view.
    pub fn set_page(self, page: String, caret: usize) {
        self.record(&page);
        self.focus_pos.set(Some(caret));
        self.content.set(page);
        self.dirty.set(true);
    }

    /// The page is about to become `page`: a step of its undo history.
    fn record(self, page: &str) {
        let now = js_sys::Date::now();
        self.content.with_untracked(|old| self.history.update_value(|h| h.record(old, page, now)));
    }

    /// Undo (`redo` false) or redo the last step of the page; false when there is none.
    pub fn undo(self, redo: bool) -> bool {
        let current = self.content.get_untracked();
        let Some((page, caret)) = self.history.try_update_value(|h| if redo { h.redo(&current) } else { h.undo(&current) }).flatten() else {
            return false;
        };
        // Folds would no longer hold the same text
        self.folds.set(Vec::new());
        self.focus_pos.set(Some(caret));
        self.content.set(page);
        self.dirty.set(true);
        true
    }

    /// The caret moved: the table it enters is shown as text, the one it leaves drawn.
    pub fn track_caret(self, el: &Textarea) {
        let text = el.value();
        // Also runs a tick after a key press, when the pane may be closed (C-x 1)
        let Some(run) = self.shown.try_with_value(|v| v.indent_run(&text, caret(el))) else { return };
        // The caret doesn't stay in the indentation of a line: it goes after it, or
        // to the end of the line above when it came back from the start of the text
        let (start, end) = get_pos(el);
        if start == end && let Some((a, b)) = run && start < b {
            let back = self.last_caret.get_value() == b && start + 1 == b && a > 0;
            set_cursor(el, if back { a - 1 } else { b });
        }
        // Nor right after the fold character ending a folded headline: it goes before
        // it, or to the next line when it came from before it
        let p = caret(el);
        let chars: Vec<char> = text.chars().collect();
        if start == end && p > 0 && chars.get(p - 1).is_some_and(|&c| tables::fold_index(c).is_some()) {
            if self.last_caret.get_value() == p - 1 && p < chars.len() {
                let next = p + 1;
                let after = self.shown.with_value(|v| v.indent_run(&text, next)).map_or(next, |(_, b)| b);
                set_cursor(el, after);
            } else {
                set_cursor(el, p - 1);
            }
        }
        self.last_caret.set_value(caret(el));
        let pos = self.page_pos(el, caret(el));
        if self.focus_pos.get_untracked() != Some(pos) { self.focus_pos.set(Some(pos)); }
    }
}

/// Auto-save: re-runs on every content change while the tab is dirty.
/// A single write loop runs at a time, so writes never reach the disk out of order.
pub fn autosave(ctx: AppCtx, tab: Tab) {
    let saving = RwSignal::new(false);
    Effect::new(move |_| {
        tab.content.track();
        if !ctx.pref(|p| p.autosave) || !tab.dirty.get_untracked() || saving.get_untracked() {
            return;
        }
        saving.set(true);
        let tab = tab.clone();
        spawn_local(async move {
            loop {
                let content = tab.content.get_untracked();
                if let Err(e) = invoke::write_file(&tab.path, &content).await {
                    ctx.error("save_error", &e);
                    break;
                }
                ctx.bump_links();
                if tab.content.get_untracked() == content {
                    tab.dirty.set(false);
                    break;
                }
            }
            saving.set(false);
        });
    });
}

/// The view of the page the textarea shows: tables and images drawn, but the
/// table the caret is in (and all of them while finding), folds hidden. Kept in
/// the textarea, the caret where it was in the page.
pub fn drawn_view(pane: Pane, page_path: String, sizes: RwSignal<ImageSizes>) -> Memo<tables::View> {
    let ctx = pane.ctx;
    let area = pane.area;
    // Width of the textarea, which decides how high a drawn table is
    let width = RwSignal::new(0i32);
    let heights = StoredValue::new(std::collections::HashMap::<(String, i32, Option<usize>), usize>::new());
    // Images of the page whose size is being loaded
    let loading = StoredValue::new(std::collections::HashSet::<String>::new());
    let resize = window_event_listener(leptos::ev::resize, move |_| {
        if let Some(el) = area.get_untracked() { width.set(el.client_width()); }
    });
    on_cleanup(move || resize.remove());
    Effect::new(move |_| {
        if let Some(el) = area.get() { width.set(el.client_width()); }
    });
    let view = Memo::new(move |_| {
        let w = width.get();
        let content = pane.content.get();
        // While searching, tables are shown as text (matches may be in them)
        let collapse = w > 0 && pane.find.get().is_none();
        let indent = ctx.pref(|p| p.indent_headings);
        let digits = ctx.pref(|p| p.show_line_numbers).then(|| number_digits(&content));
        let el = area.get_untracked();
        let columns = text_columns(w.max(0), digits);
        let show_images = ctx.pref(|p| p.show_images);
        tables::View::build(&content, pane.focus_pos.get(), columns, |raw| {
            let key = (raw.to_string(), w, digits);
            if let Some(n) = heights.with_value(|h| h.get(&key).copied()) { return n; }
            let n = el.as_ref().and_then(|el| table_lines(el, raw, w, digits)).unwrap_or_else(|| raw.split('\n').count());
            heights.update_value(|h| { h.insert(key, n); });
            n
        }, |image, attr, before| {
            if !show_images { return None; }
            let src = image_src(&page_path, &image.target);
            // Shown as text until its size is known, and when it can't be loaded
            let Some(natural) = sizes.with(|s| s.get(&src).copied()) else {
                if loading.with_value(|l| !l.contains(&src)) {
                    loading.update_value(|l| { l.insert(src.clone()); });
                    load_size(&src, sizes);
                }
                return None;
            };
            let max = (columns.saturating_sub(before) as f64 * CHAR_WIDTH).max(16.0);
            let (width, height) = images::display_size(natural?, attr, image, max);
            let lines = ((height + 4.0) / LINE_HEIGHT).ceil() as usize;
            Some((tables::Shown { src: image_url(&src), width, height }, lines))
        }, collapse, indent, &pane.folds.get())
    });
    Effect::new(move |_| {
        let v = view.get();
        let Some(el) = area.get() else { return };
        if el.value() != v.text {
            let scroll = el.scroll_top();
            el.set_value(&v.text);
            if let Some(p) = pane.focus_pos.get_untracked() { set_cursor(&el, v.view_pos(p)); }
            el.set_scroll_top(scroll);
        }
        pane.shown.set_value(v);
    });
    view
}
