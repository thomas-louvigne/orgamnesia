use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::{spawn_local, JsFuture};

use crate::{
    actions::follow_link,
    components::{page_title::PageTitle, tabs::TabBar},
    edit,
    highlight,
    tables,
    i18n::t,
    invoke,
    keybindings::{after_ms, EditorAction, Resolution, Scope},
    state::{AppCtx, Goto, SplitKind, Tab},
};

fn exec_insert(text: &str) {
    if let Ok(encoded) = serde_json::to_string(text) {
        let _ = js_sys::eval(&format!("document.execCommand('insertText',false,{})", encoded));
    }
}

fn exec_undo() {
    let _ = js_sys::eval("document.execCommand('undo')");
}

fn exec_redo() {
    let _ = js_sys::eval("document.execCommand('redo')");
}

// ─── Clipboard (system) helpers ────────────────────────────────────────────────

fn clipboard() -> Option<web_sys::Clipboard> {
    web_sys::window().map(|w| w.navigator().clipboard())
}

fn clipboard_write(text: &str) {
    if let Some(cb) = clipboard() {
        let _ = cb.write_text(text);
    }
}

// ─── Textarea helpers ────────────────────────────────────────────────────

type Textarea = web_sys::HtmlTextAreaElement;

fn get_pos(el: &Textarea) -> (usize, usize) {
    let s = el.selection_start().ok().flatten().unwrap_or(0) as usize;
    let e = el.selection_end().ok().flatten().unwrap_or(0) as usize;
    (s, e)
}

/// Shortcuts a textarea handles natively; they only work when the active profile binds them.
const NATIVE_EDIT_SHORTCUTS: &[&str] = &[
    "ctrl+c", "ctrl+x", "ctrl+v", "ctrl+a", "ctrl+z", "ctrl+y", "ctrl+shift+z",
    "ctrl+Insert", "shift+Insert", "shift+Delete",
];

/// Caret position: the moving end of the selection.
fn caret(el: &Textarea) -> usize {
    let (s, e) = get_pos(el);
    match el.selection_direction().ok().flatten().as_deref() {
        Some("backward") => s,
        _ => e,
    }
}

/// Run `f` once the browser has applied the default action of the current event.
fn after_tick(f: impl FnOnce() + 'static) {
    after_ms(0, f);
}

fn set_cursor(el: &Textarea, pos: usize) {
    let p = pos as u32;
    el.set_selection_start(Some(p)).ok();
    el.set_selection_end(Some(p)).ok();
}

fn set_selection(el: &Textarea, start: usize, end: usize) {
    el.set_selection_start(Some(start as u32)).ok();
    el.set_selection_end(Some(end as u32)).ok();
}

/// Replace [del_start..del_end) with `insert`, returns new cursor position.
/// Uses execCommand so the browser's undo stack (Ctrl+Z) stays intact.
fn splice(el: &Textarea, del_start: usize, del_end: usize, insert: &str) -> usize {
    el.set_selection_start(Some(del_start as u32)).ok();
    el.set_selection_end(Some(del_end as u32)).ok();
    exec_insert(insert);
    del_start + insert.chars().count()
}

/// Height of a line of the editor (14px font, 1.65 line height).
const LINE_HEIGHT: f64 = 14.0 * 1.65;

/// Width of a character of the editor font (monospace, 14px).
const CHAR_WIDTH: f64 = 14.0 * 0.6;

/// Width in characters of the text of a textarea `width` px wide (24px padding each
/// side). With line numbers of `digits` digits, the left padding is 4px and the
/// numbers and their margin take `digits + 1.5` columns (see `.line-numbers` in the CSS).
fn text_columns(width: i32, digits: Option<usize>) -> usize {
    let (padding, gutter) = digits.map_or((48.0, 0.0), |d| (28.0, d as f64 + 1.5));
    ((width as f64 - padding).max(0.0) / CHAR_WIDTH - gutter).max(0.0) as usize
}

/// Digits of the line numbers of `content`: those of its last line, at least 2.
fn number_digits(content: &str) -> usize {
    (content.matches('\n').count() + 1).to_string().len().max(2)
}

/// Lines of text a table takes once drawn, the text being `width` px wide.
fn table_lines(el: &Textarea, raw: &str, width: i32, digits: Option<usize>) -> Option<usize> {
    let doc = web_sys::window()?.document()?;
    let parent = el.parent_element()?;
    let probe = doc.create_element("div").ok()?;
    probe.set_class_name("hl-layer tbl-probe");
    probe.set_attribute("style", &format!("width: {width}px")).ok()?;
    probe.set_inner_html(&format!("<div class='tbl-view'>{}</div>", tables::to_html(raw, text_columns(width, digits))));
    parent.append_child(&probe).ok()?;
    let height = probe.query_selector("table").ok()??.dyn_into::<web_sys::HtmlElement>().ok()?.offset_height();
    probe.remove();
    Some((height as f64 / LINE_HEIGHT).ceil() as usize)
}

/// Scroll the textarea so the char at `pos` is in view (in the middle when it was not).
fn reveal(el: &Textarea, chars: &[char], pos: usize) {
    let Some((_, top, bottom)) = caret_coords(el, chars, pos) else { return };
    let height = el.client_height() as f64;
    if top < 0.0 || bottom > height {
        el.set_scroll_top((el.scroll_top() as f64 + top - height / 2.0).max(0.0) as i32);
    }
}

// ─── Completion of page names (#tag and [[link]]) and org-mode :tags: ─────────

/// What is being completed.
#[derive(Clone, Copy, PartialEq)]
enum CompletionKind {
    /// A page name in `[[link`: accepting also closes the link with `]]`.
    Link,
    /// A page name after `#`.
    Hashtag,
    /// An org-mode tag (`* Title :tag`, `#+FILETAGS:`): accepting adds the closing `:`.
    OrgTag,
}

/// Suggestions shown while a `#tag`, a `[[link` or a `:tag:` is being typed.
#[derive(Clone, PartialEq)]
struct PageCompletion {
    /// The typed part of the name lies between `start` and the caret.
    start: usize,
    caret: usize,
    kind: CompletionKind,
    items: Vec<String>,
    selected: usize,
    /// Popup position in pixels, relative to the editor wrapper.
    x: f64,
    y: f64,
}

const MAX_SUGGESTIONS: usize = 8;
/// Characters to type after `[[` before pages are suggested.
const LINK_MIN_CHARS: usize = 2;
const POPUP_WIDTH: f64 = 260.0;
const ITEM_HEIGHT: f64 = 26.0;

/// Caret position in pixels (left, line top, line bottom) relative to the editor
/// wrapper, measured with a hidden copy of the text laid out like the highlight layer.
fn caret_coords(el: &Textarea, chars: &[char], pos: usize) -> Option<(f64, f64, f64)> {
    let doc = web_sys::window()?.document()?;
    let parent = el.parent_element()?;
    let mirror = doc.create_element("div").ok()?;
    mirror.set_class_name("hl-layer caret-mirror");
    mirror.set_text_content(Some(&chars[..pos].iter().collect::<String>()));
    let marker = doc.create_element("span").ok()?;
    marker.set_text_content(Some("\u{200b}"));
    mirror.append_child(&marker).ok()?;
    parent.append_child(&mirror).ok()?;
    let m: web_sys::HtmlElement = marker.dyn_into().ok()?;
    let x = (m.offset_left() - el.scroll_left()) as f64;
    let top = (m.offset_top() - el.scroll_top()) as f64;
    let bottom = top + m.offset_height() as f64;
    mirror.remove();
    Some((x, top, bottom))
}

// ─── Components ──────────────────────────────────────────────────────────────

/// The editor: one pane, or two when split (Emacs `C-x 2` / `C-x 3`).
#[component]
pub fn EditorSplit() -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");
    view! {
        <div class=move || match ctx.work.split.get() {
            None => "editor-split",
            Some(SplitKind::Vertical) => "editor-split vertical",
            Some(SplitKind::Horizontal) => "editor-split horizontal",
        }>
            <EditorArea second=false />
            {move || ctx.work.split.get().is_some().then(|| view! { <EditorArea second=true /> })}
        </div>
    }
}

#[component]
pub fn EditorArea(second: bool) -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");

    // The focused pane shows the active tab, the other one `other_path`
    let path = Memo::new(move |_| ctx.work.pane_path(second));
    let focused = move || ctx.work.split.get().is_none() || ctx.work.focus_second.get() == second;

    // When this pane takes the focus (C-x o, C-o…), the keyboard must follow:
    // put the text cursor in its editor.
    let area_ref = NodeRef::<leptos::html::Div>::new();
    Effect::new(move |_| {
        let has_focus = ctx.work.focus_second.get() == second;
        if ctx.work.split.get().is_none() || !has_focus { return; }
        crate::keybindings::after_ms(0, move || {
            let textarea = area_ref.try_get_untracked().flatten()
                .and_then(|div| div.query_selector("textarea").ok().flatten())
                .and_then(|el| el.dyn_into::<Textarea>().ok());
            if let Some(t) = textarea { let _ = t.focus(); }
        });
    });

    view! {
        <div
            node_ref=area_ref
            class=move || if focused() { "editor-area focused" } else { "editor-area" }
            on:mousedown=move |_| ctx.work.focus_pane(second)
        >
            {move || (ctx.work.split.get() == Some(SplitKind::Vertical)).then(|| {
                let name = path.try_get().flatten()
                    .and_then(|p| ctx.work.tabs.get().into_iter().find(|t| t.path == p))
                    .map(|t| t.name)
                    .unwrap_or_default();
                view! { <div class="pane-tab"><span class="pane-tab-name">{name}</span></div> }
            })}
            // Side by side: under each pane's title, its own tabs to choose its page.
            // One above the other: the bottom pane has its own tabs (the top bar is the top pane's).
            {move || {
                let split = ctx.work.split.get();
                (split == Some(SplitKind::Vertical) || (second && split == Some(SplitKind::Horizontal)))
                    .then(|| view! { <div class="pane-tabs"><TabBar pane=second /></div> })
            }}
            {move || ctx.pref(|p| p.show_page_title).then(|| {
                let p = path.try_get().flatten()?;
                let tab = ctx.work.tabs.with(|tabs| tabs.iter().find(|t| t.path == p).cloned())?;
                Some(view! { <PageTitle tab=tab /> })
            }).flatten()}
            <div class="editor-pane-body">
            // `try_get`: these also re-run when the pane is closed (C-x 0), after `path` is disposed
            {move || match path.try_get().flatten() {
                None => view! {
                    <div class="editor-empty">
                        <p>{t("no_file", ctx.lang.get())}</p>
                        <p>{t("open_hint", ctx.lang.get())}</p>
                        <img class="empty-logo" src="app-icon.svg" alt="" />
                    </div>
                }.into_any(),
                Some(p) => {
                    let tab = ctx.work.tabs.get_untracked().into_iter().find(|t| t.path == p);
                    match tab {
                        Some(tab) => view! { <Editor tab=tab second=second /> }.into_any(),
                        None => ().into_any(),
                    }
                }
            }}
            </div>
        </div>
    }
}

#[component]
fn Editor(tab: Tab, second: bool) -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");


    // Auto-save: re-runs on every content change while the tab is dirty.
    // A single write loop runs at a time, so writes never reach the disk out of order.
    let saving = RwSignal::new(false);
    let tab_auto = tab.clone();
    Effect::new(move |_| {
        tab_auto.content.track();
        if !ctx.pref(|p| p.autosave) || !tab_auto.dirty.get_untracked() || saving.get_untracked() {
            return;
        }
        saving.set(true);
        let tab = tab_auto.clone();
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

    // Emacs mark (char index) and the caret ("point"); the region is what lies between.
    let mark  = RwSignal::new(None::<usize>);
    let point = RwSignal::new(0usize);

    // Region to act on: mark..point when a mark is set (and enabled), else the native selection.
    let region_of = move |el: &Textarea| -> (usize, usize) {
        if ctx.pref_untracked(|p| p.emacs_mark) && let Some(m) = mark.get_untracked() {
            let p = caret(el);
            return (m.min(p), m.max(p));
        }
        get_pos(el)
    };

    // Page name completion while a `#tag` or a `[[link` is being typed
    let completion = RwSignal::new(None::<PageCompletion>);
    let accepting  = StoredValue::new(false);
    let refresh_completion = move |el: &Textarea| {
        let (caret, end) = get_pos(el);
        let chars: Vec<char> = el.value().chars().collect();
        let found = if caret != end { None } else {
            crate::motion::link_prefix(&chars, caret)
                .filter(|(_, p)| p.trim().chars().count() >= LINK_MIN_CHARS)
                .map(|(start, p)| (start, p, CompletionKind::Link))
                .or_else(|| crate::motion::org_tag_prefix(&chars, caret, ctx.tags_untracked().org)
                    .map(|(start, p)| (start, p, CompletionKind::OrgTag)))
                .or_else(|| crate::motion::hashtag_prefix(&chars, caret, ctx.tags_untracked().hashtags)
                    .map(|(hash, p)| (hash + 1, p, CompletionKind::Hashtag)))
        };
        let Some((start, prefix, kind)) = found else { completion.set(None); return };
        let items = match kind {
            CompletionKind::OrgTag => {
                let names: Vec<String> = ctx.project.tags.get_untracked().into_iter().map(|t| t.name).collect();
                crate::motion::complete_page(&names, &prefix, None, MAX_SUGGESTIONS)
            }
            _ => {
                let names: Vec<String> = ctx.project.files.get_untracked().into_iter().map(|f| f.name).collect();
                let tag = (kind == CompletionKind::Hashtag).then(|| ctx.tags_untracked().hashtags);
                crate::motion::complete_page(&names, &prefix, tag, MAX_SUGGESTIONS)
            }
        };
        if items.is_empty() { completion.set(None); return; }
        let Some((x, top, bottom)) = caret_coords(el, &chars, start) else { return };
        // Keep the popup inside the editor: flip above the line near the bottom edge
        let height = items.len() as f64 * ITEM_HEIGHT + 8.0;
        let y = if bottom + height > el.client_height() as f64 && top > height { top - height } else { bottom };
        let x = x.min(el.client_width() as f64 - POPUP_WIDTH).max(0.0);
        completion.set(Some(PageCompletion { start, caret, kind, items, selected: 0, x, y }));
    };
    let (content_sig, dirty_sig) = (tab.content, tab.dirty);

    // ── Tables drawn as tables (see `tables`) ─────────────────────────────────
    // The textarea holds a view of the page where the tables are drawn, but the
    // one the caret is in (and all of them while finding).
    let area_ref = NodeRef::<leptos::html::Textarea>::new();
    // None: find bar closed; Some(with_replace)
    let find = RwSignal::new(None::<bool>);
    // Page position of the caret
    let focus_pos = RwSignal::new(None::<usize>);
    // Folded parts of the page (Tab on a headline), as page ranges: see `folding`
    let folds = RwSignal::new(Vec::<(usize, usize)>::new());
    // Width of the textarea, which decides how high a drawn table is
    let width = RwSignal::new(0i32);
    let heights = StoredValue::new(std::collections::HashMap::<(String, i32, Option<usize>), usize>::new());
    let resize = window_event_listener(leptos::ev::resize, move |_| {
        if let Some(el) = area_ref.get_untracked() { width.set(el.client_width()); }
    });
    on_cleanup(move || resize.remove());
    Effect::new(move |_| {
        if let Some(el) = area_ref.get() { width.set(el.client_width()); }
    });
    let view = Memo::new(move |_| {
        let w = width.get();
        let content = content_sig.get();
        // While searching, tables are shown as text (matches may be in them)
        let collapse = w > 0 && find.get().is_none();
        let indent = ctx.pref(|p| p.indent_headings);
        let digits = ctx.pref(|p| p.show_line_numbers).then(|| number_digits(&content));
        let el = area_ref.get_untracked();
        tables::View::build(&content, focus_pos.get(), text_columns(w.max(0), digits), |raw| {
            let key = (raw.to_string(), w, digits);
            if let Some(n) = heights.with_value(|h| h.get(&key).copied()) { return n; }
            let n = el.as_ref().and_then(|el| table_lines(el, raw, w, digits)).unwrap_or_else(|| raw.split('\n').count());
            heights.update_value(|h| { h.insert(key, n); });
            n
        }, collapse, indent, &folds.get())
    });
    // The view the textarea holds: what its text is read with. Until the textarea
    // gets a new view, an edit is still read with the one it shows.
    let shown = StoredValue::new(tables::View::default());
    // Show the view in the textarea, the caret where it was in the page
    Effect::new(move |_| {
        let v = view.get();
        let Some(el) = area_ref.get() else { return };
        if el.value() != v.text {
            let scroll = el.scroll_top();
            el.set_value(&v.text);
            if let Some(p) = focus_pos.get_untracked() { set_cursor(&el, v.view_pos(p)); }
            el.set_scroll_top(scroll);
        }
        shown.set_value(v);
    });
    // The textarea was edited: the page follows
    let commit = move |el: &Textarea| {
        let text = el.value();
        let (page, pos, folded) = shown.with_value(|v| (
            v.to_content(&text),
            v.content_pos(&text, caret(el)),
            (!v.folds.is_empty()).then(|| v.fold_ranges(&text)),
        ));
        // The folds move with the text
        if let Some(f) = folded { folds.set(f); }
        focus_pos.set(Some(pos));
        content_sig.set(page);
        dirty_sig.set(true);
    };
    // The caret moved: the table it enters is shown as text, the one it leaves drawn
    // Caret in the textarea after the last move
    let last_caret = StoredValue::new(0usize);
    let track_caret = move |el: &Textarea| {
        let text = el.value();
        // Also runs a tick after a key press, when the pane may be closed (C-x 1)
        let Some(run) = shown.try_with_value(|v| v.indent_run(&text, caret(el))) else { return };
        // The caret doesn't stay in the indentation of a line: it goes after it, or
        // to the end of the line above when it came back from the start of the text
        let (start, end) = get_pos(el);
        if start == end && let Some((a, b)) = run && start < b {
            let back = last_caret.get_value() == b && start + 1 == b && a > 0;
            set_cursor(el, if back { a - 1 } else { b });
        }
        // Nor right after the fold character ending a folded headline: it goes before
        // it, or to the next line when it came from before it
        let p = caret(el);
        let chars: Vec<char> = text.chars().collect();
        if start == end && p > 0 && chars.get(p - 1).is_some_and(|&c| tables::fold_index(c).is_some()) {
            if last_caret.get_value() == p - 1 && p < chars.len() {
                let next = p + 1;
                let after = shown.with_value(|v| v.indent_run(&text, next)).map_or(next, |(_, b)| b);
                set_cursor(el, after);
            } else {
                set_cursor(el, p - 1);
            }
        }
        last_caret.set_value(caret(el));
        let pos = shown.with_value(|v| v.content_pos(&text, caret(el)));
        if focus_pos.get_untracked() != Some(pos) { focus_pos.set(Some(pos)); }
    };
    // Page text between two positions of the textarea
    let page_text = move |el: &Textarea, a: usize, b: usize| -> String {
        let text = el.value();
        let (a, b) = shown.with_value(|v| (v.content_pos(&text, a), v.content_pos(&text, b)));
        content_sig.with_untracked(|c| c.chars().skip(a).take(b.saturating_sub(a)).collect())
    };
    let accept_completion = move |el: &Textarea, name: &str| {
        let Some(c) = completion.get_untracked() else { return };
        accepting.set_value(true);
        let mut cursor = splice(el, c.start, c.caret, name);
        // Close the link / tag: step over the closing characters if already there, else add them
        let close = match c.kind {
            CompletionKind::Link => "]]",
            CompletionKind::OrgTag => ":",
            CompletionKind::Hashtag => "",
        };
        if !close.is_empty() {
            let n = close.chars().count();
            let after: String = el.value().chars().skip(cursor).take(n).collect();
            cursor = if after == close { cursor + n } else { splice(el, cursor, cursor, close) };
        }
        accepting.set_value(false);
        set_cursor(el, cursor);
        completion.set(None);
        commit(el);
    };

    // ── Find / replace in the page ────────────────────────────────────────────
    let find_ref = NodeRef::<leptos::html::Input>::new();
    let replace_ref = NodeRef::<leptos::html::Input>::new();
    let query = RwSignal::new(String::new());
    let replacement = RwSignal::new(String::new());
    // Starts as the settings say, and follows them when they change
    let match_case = RwSignal::new(false);
    Effect::new(move |_| match_case.set(ctx.pref(|p| p.find_match_case)));
    let current = RwSignal::new(None::<usize>);
    // Where the search started: the first match shown is the one after it
    let anchor = StoredValue::new(0usize);
    let matches = Memo::new(move |_| {
        if find.with(|f| f.is_none()) { return vec![]; }
        let q = query.get();
        content_sig.with(|c| edit::find_all(&c.chars().collect::<Vec<_>>(), &q, match_case.get()))
    });
    // Select match `i` in the text and bring it into view
    let show_match = move |i: usize| {
        let Some(&(a, b)) = matches.with_untracked(|m| m.get(i).copied()).as_ref() else {
            current.set(None);
            return;
        };
        current.set(Some(i));
        anchor.set_value(a);
        focus_pos.set(Some(a));
        if let Some(el) = area_ref.get_untracked() {
            let (a, b) = shown.with_value(|v| (v.view_pos(a), v.view_pos(b)));
            set_selection(&el, a, b);
            reveal(&el, &el.value().chars().collect::<Vec<_>>(), a);
        }
    };
    // The first match at or after the anchor
    let show_from_anchor = move || {
        let at = anchor.get_value();
        let n = matches.with_untracked(|m| m.iter().position(|&(a, _)| a >= at).or((!m.is_empty()).then_some(0)));
        match n {
            Some(i) => show_match(i),
            None => current.set(None),
        }
    };
    let step = move |forward: bool| {
        let n = matches.with_untracked(|m| m.len());
        if n == 0 { return; }
        let i = match current.get_untracked() {
            Some(i) if forward => (i + 1) % n,
            Some(i) => (i + n - 1) % n,
            None if forward => 0,
            None => n - 1,
        };
        show_match(i);
    };
    let open_find = move |with_replace: bool, el: &Textarea| {
        let (a, b) = get_pos(el);
        let selected = page_text(el, a, b);
        if !selected.is_empty() && !selected.contains('\n') {
            query.set(selected);
        }
        let text = el.value();
        anchor.set_value(shown.with_value(|v| v.content_pos(&text, a)));
        // Matches may be hidden: everything is unfolded
        folds.set(vec![]);
        find.set(Some(with_replace || find.get_untracked().unwrap_or(false)));
        // Once the textarea shows every table as text
        after_tick(move || {
            let Some(query) = query.try_get_untracked() else { return };
            show_from_anchor();
            let target = if with_replace && !query.is_empty() { replace_ref } else { find_ref };
            if let Some(input) = target.try_get_untracked().flatten() {
                let _ = input.focus();
                input.select();
            }
        });
    };
    let close_find = move || {
        find.set(None);
        current.set(None);
        if let Some(el) = area_ref.get_untracked() { let _ = el.focus(); }
    };
    let replace_current = move || {
        let Some(el) = area_ref.get_untracked() else { return };
        let Some((a, b)) = current.get_untracked().and_then(|i| matches.with_untracked(|m| m.get(i).copied())) else {
            step(true);
            return;
        };
        let with = replacement.get_untracked();
        let (va, vb) = shown.with_value(|v| (v.view_pos(a), v.view_pos(b)));
        splice(&el, va, vb, &with);
        commit(&el);
        anchor.set_value(a + with.chars().count());
        show_from_anchor();
    };
    let replace_all = move || {
        let Some(el) = area_ref.get_untracked() else { return };
        let found = matches.get_untracked();
        if found.is_empty() { return; }
        // Replaced in the page, which then takes the place of the whole view
        let chars: Vec<char> = content_sig.get_untracked().chars().collect();
        let all = el.value().chars().count();
        let with = replacement.get_untracked();
        let mut out = String::new();
        let mut at = 0;
        for &(a, b) in &found {
            out.extend(&chars[at..a]);
            out.push_str(&with);
            at = b;
        }
        out.extend(&chars[at..]);
        // A single edit, so one undo brings everything back
        splice(&el, 0, all, &out);
        set_cursor(&el, 0);
        commit(&el);
        current.set(None);
        ctx.notify(format!("{} × {}", found.len(), t("replace_all", ctx.lang.get_untracked())));
    };
    let on_find_key = move |e: web_sys::KeyboardEvent, in_replace: bool| {
        let plain = !e.ctrl_key() && !e.alt_key() && !e.meta_key();
        let handled = match e.key().as_str() {
            "Escape" if plain => { close_find(); true }
            "Enter" if plain && in_replace && !e.shift_key() => { replace_current(); true }
            "Enter" if plain => { step(!e.shift_key()); true }
            _ => match crate::keybindings::resolve(&ctx.keybindings.get_untracked(), &e) {
                Resolution::Action(Scope::Editor, id) => match EditorAction::from_id(&id) {
                    Some(EditorAction::Find) => { step(true); true }
                    Some(EditorAction::FindPrevious) => { step(false); true }
                    Some(EditorAction::KeyboardQuit) => { close_find(); true }
                    Some(EditorAction::Replace) => {
                        find.set(Some(true));
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
    };

    let on_input = move |e: web_sys::Event| {
        let el = e.target().unwrap().dyn_into::<Textarea>().unwrap();
        // Typing ends the region
        mark.set(None);
        commit(&el);
        if !accepting.get_value() { refresh_completion(&el); }
    };

    let on_keydown = move |e: web_sys::KeyboardEvent| {
        let el: Textarea = e.target().unwrap().dyn_into().unwrap();
        {
            let el = el.clone();
            after_tick(move || track_caret(&el));
        }

        // Completion popup: navigate, accept or dismiss before any shortcut
        if let Some(c) = completion.get_untracked() {
            let n = c.items.len();
            let plain = !e.ctrl_key() && !e.alt_key() && !e.meta_key() && !e.shift_key();
            let ctrl_only = e.ctrl_key() && !e.alt_key() && !e.meta_key() && !e.shift_key();
            let key = e.key();
            let handled = match key.as_str() {
                "ArrowDown" if plain => { completion.set(Some(PageCompletion { selected: (c.selected + 1) % n, ..c })); true }
                "n" if ctrl_only     => { completion.set(Some(PageCompletion { selected: (c.selected + 1) % n, ..c })); true }
                "ArrowUp" if plain   => { completion.set(Some(PageCompletion { selected: (c.selected + n - 1) % n, ..c })); true }
                "p" if ctrl_only     => { completion.set(Some(PageCompletion { selected: (c.selected + n - 1) % n, ..c })); true }
                "Enter" | "Tab" if plain => { accept_completion(&el, &c.items[c.selected]); true }
                "Escape" if plain    => { completion.set(None); true }
                "g" if ctrl_only     => { completion.set(None); true }
                _ => false,
            };
            if handled {
                e.prevent_default();
                e.stop_propagation();
                return;
            }
            // Moving the caret elsewhere closes the popup; typing refreshes it (on input)
            if matches!(key.as_str(), "ArrowLeft" | "ArrowRight" | "Home" | "End" | "PageUp" | "PageDown")
                || e.ctrl_key() || e.alt_key()
            {
                completion.set(None);
            }
        }

        if find.get_untracked().is_some() && e.key() == "Escape"
            && !e.ctrl_key() && !e.alt_key() && !e.meta_key() && !e.shift_key()
        {
            e.prevent_default();
            close_find();
            return;
        }

        let kb = ctx.keybindings.get();

        // Multi-key chords ("ctrl+x ctrl+s"): swallow the keys of a chord in progress
        let resolution = crate::keybindings::resolve(&kb, &e);
        match &resolution {
            Resolution::Pending(keys) => {
                e.prevent_default();
                e.stop_propagation();
                ctx.set_chord_status(keys);
                return;
            }
            Resolution::Aborted => {
                e.prevent_default();
                e.stop_propagation();
                ctx.clear_chord_status();
                return;
            }
            _ => ctx.clear_chord_status(),
        }

        // With a mark set, the region follows the caret after each key press
        if mark.get_untracked().is_some() {
            let el2 = el.clone();
            after_tick(move || point.set(caret(&el2)));
        }

        // The fold character of a folded headline is not deleted on its own (that would
        // delete the hidden text): Backspace or Delete next to it unfolds the headline
        if matches!(e.key().as_str(), "Backspace" | "Delete") && !e.ctrl_key() && !e.alt_key() && !e.meta_key() {
            let (start, end) = get_pos(&el);
            let chars: Vec<char> = el.value().chars().collect();
            let at = if e.key() == "Delete" { start } else { start.wrapping_sub(1) };
            if start == end && chars.get(at).is_some_and(|&c| tables::fold_index(c).is_some()) {
                e.prevent_default();
                let text = el.value();
                let pos = shown.with_value(|v| v.content_pos(&text, at));
                folds.update(|f| f.retain(|&(a, _)| a != pos));
                return;
            }
        }
        // Enter at the end of a folded headline: the new line comes after the hidden text
        if e.key() == "Enter" && !e.ctrl_key() && !e.alt_key() && !e.meta_key() && !e.shift_key() {
            let (start, end) = get_pos(&el);
            if start == end && el.value().chars().nth(start).is_some_and(|c| tables::fold_index(c).is_some()) {
                set_cursor(&el, start + 1);
            }
        }

        // Backspace at the start of an indented line joins it to the line above
        // (the indentation is not in the page: deleting it would change nothing)
        if e.key() == "Backspace" && !e.ctrl_key() && !e.alt_key() && !e.meta_key() {
            let (start, end) = get_pos(&el);
            let text = el.value();
            if start == end && let Some((a, b)) = shown.with_value(|v| v.indent_run(&text, start)) {
                e.prevent_default();
                if a > 0 {
                    let cursor = splice(&el, a - 1, b, "");
                    set_cursor(&el, cursor);
                    commit(&el);
                }
                return;
            }
        }

        // ── Auto-pairing / electric wrapping ──────────────────────────────────
        // Without a selection, ( [ { and " insert their closing character too.
        // With a selection, electric mode wraps it in the typed pair.
        let key = e.key();
        let (start, end) = get_pos(&el);
        if let Some((open, close)) = edit::pair_of(&key) {
            if start != end {
                if ctx.pref_untracked(|p| p.electric_mode) {
                    e.prevent_default();
                    let selected: String = el.value().chars().skip(start).take(end - start).collect();
                    let new_end = splice(&el, start, end, &format!("{open}{selected}{close}"));
                    set_selection(&el, start + 1, new_end - 1);
                    commit(&el);
                    return;
                }
            } else if edit::auto_closes(&key) {
                e.prevent_default();
                let cursor = splice(&el, start, start, &format!("{open}{close}"));
                set_cursor(&el, cursor - 1);
                commit(&el);
                return;
            }
        }

        // ── Configured editor action (classic or emacs preset) ────────────────
        let action = match resolution {
            Resolution::Action(Scope::Editor, id) => EditorAction::from_id(&id),
            other => {
                // Not an editor shortcut of this profile: the browser's built-in
                // clipboard/undo shortcuts must not work either (unless the
                // combination belongs to an application shortcut).
                if other == Resolution::None
                    && NATIVE_EDIT_SHORTCUTS.iter().any(|b| crate::keybindings::key_matches(&e, b))
                {
                    e.prevent_default();
                }
                return;
            }
        };
        let Some(action) = action else { return };
        // Tab / Shift+Tab only move between cells in a table; elsewhere the key does what it usually does
        if matches!(action, EditorAction::TableNextCell | EditorAction::TablePrevCell) {
            let chars: Vec<char> = el.value().chars().collect();
            let forward = action == EditorAction::TableNextCell;
            let Some(to) = tables::move_cell(&chars, caret(&el), forward) else {
                // Tab on a headline folds / unfolds it (org-mode cycle)
                if !forward { return; }
                let text = el.value();
                let pos = shown.with_value(|v| v.content_pos(&text, caret(&el)));
                let page: Vec<char> = content_sig.with_untracked(|c| c.chars().collect());
                let line = crate::folding::line_start(&page, pos);
                let Some(next) = folds.with_untracked(|f| crate::folding::cycle(&page, line, f)) else { return };
                e.prevent_default();
                e.stop_propagation();
                // The caret stays on the headline
                focus_pos.set(Some(pos.min(page[line..].iter().position(|&c| c == '\n').map_or(page.len(), |p| line + p))));
                folds.set(next);
                return;
            };
            e.prevent_default();
            e.stop_propagation();
            match to {
                tables::CellMove::Caret(p) => set_cursor(&el, p),
                tables::CellMove::Edit { from, to, text, caret } => {
                    splice(&el, from, to, &text);
                    set_cursor(&el, caret);
                    commit(&el);
                }
            }
            track_caret(&el);
            return;
        }
        e.prevent_default();
        e.stop_propagation();

        let chars: Vec<char> = el.value().chars().collect();
        let text = |a: usize, b: usize| -> String { page_text(&el, a, b) };
        // Replace a..b with `with`, caret after it; the page has changed
        let replace = |a: usize, b: usize, with: &str| {
            let cursor = splice(&el, a, b, with);
            set_cursor(&el, cursor);
            commit(&el);
        };

        if let Some(target) = edit::motion_target(action, &chars, caret(&el)) {
            set_cursor(&el, target);
            return;
        }
        let (start, end) = get_pos(&el);
        match action {
            // ── Classic clipboard / editing ───────────────────────────────────
            EditorAction::Copy if start < end => {
                clipboard_write(&text(start, end));
                set_selection(&el, start, end); // keep the selection
            }
            EditorAction::Cut if start < end => {
                clipboard_write(&text(start, end));
                replace(start, end, "");
            }
            EditorAction::Paste => {
                let el = el.clone();
                if let Some(cb) = clipboard() {
                    let promise = cb.read_text();
                    spawn_local(async move {
                        let pasted = JsFuture::from(promise).await.ok().and_then(|v| v.as_string());
                        if let Some(pasted) = pasted.filter(|p| !p.is_empty()) {
                            let (start, end) = get_pos(&el);
                            let cursor = splice(&el, start, end, &pasted);
                            set_cursor(&el, cursor);
                            commit(&el);
                        }
                    });
                }
            }
            EditorAction::SelectAll => el.select(),
            EditorAction::Undo => exec_undo(),
            EditorAction::Redo => exec_redo(),

            // ── Org emphasis: *bold*, /italic/, _underline_, +strikethrough+ ──
            EditorAction::Bold | EditorAction::Italic | EditorAction::Underline | EditorAction::Strikethrough => {
                let marker = match action {
                    EditorAction::Bold => '*',
                    EditorAction::Italic => '/',
                    EditorAction::Underline => '_',
                    _ => '+',
                };
                let (a, b, with, sel_start, sel_end) = edit::toggle_emphasis(&chars, start, end, marker);
                splice(&el, a, b, &with);
                set_selection(&el, sel_start, sel_end);
                commit(&el);
            }

            // ── Org: new heading at the same level as the current one ─────────
            EditorAction::NewHeading => {
                let (at, heading) = edit::new_heading(&chars, end);
                replace(at, at, &heading);
            }
            EditorAction::OpenLink => {
                if let Some(name) = edit::link_at(&chars, start, ctx.tags_untracked()) {
                    follow_link(ctx, name);
                }
            }

            // ── Emacs editing ─────────────────────────────────────────────────
            EditorAction::KillLine => {
                let (a, b) = edit::kill_line_range(&chars, start);
                ctx.work.kill_ring.set(text(a, b));
                replace(a, b, "");
            }
            EditorAction::KillWordForward => {
                let b = edit::word_end_forward(&chars, start);
                ctx.work.kill_ring.set(text(start, b));
                replace(start, b, "");
            }
            EditorAction::KillWordBackward => {
                let a = edit::word_start_backward(&chars, start);
                ctx.work.kill_ring.set(text(a, start));
                replace(a, start, "");
            }
            EditorAction::KillRegion | EditorAction::CopyRegion => {
                let (a, b) = region_of(&el);
                if a < b {
                    ctx.work.kill_ring.set(text(a, b));
                    if action == EditorAction::KillRegion {
                        replace(a, b, "");
                    } else if mark.get_untracked().is_none() {
                        set_selection(&el, a, b); // keep the native selection
                    }
                }
                mark.set(None);
            }
            EditorAction::Yank => {
                let killed = ctx.work.kill_ring.get_untracked();
                if !killed.is_empty() { replace(start, end, &killed); }
            }
            EditorAction::DeleteCharForward => {
                let (a, b) = edit::delete_forward_range(&chars, start, end);
                replace(a, b, "");
            }
            EditorAction::SetMark => {
                if ctx.pref_untracked(|p| p.emacs_mark) {
                    let c = caret(&el);
                    if mark.get_untracked() == Some(c) {
                        mark.set(None); // Ctrl+Space twice at the same spot cancels
                    } else {
                        mark.set(Some(c));
                        point.set(c);
                    }
                }
            }
            EditorAction::KeyboardQuit => {
                mark.set(None);
                if find.get_untracked().is_some() { close_find(); }
            }
            EditorAction::Find | EditorAction::FindPrevious => open_find(false, &el),
            EditorAction::Replace => open_find(true, &el),
            // Movements are handled above; copy / cut without a selection do nothing
            _ => {}
        }
    };

    // Ctrl+Click navigates to (or creates) the page of the wiki link, #tag or
    // org-mode :tag: under the cursor
    let on_click = move |e: web_sys::MouseEvent| {
        if !e.ctrl_key() { return; }
        let Some(el) = e.target().and_then(|t| t.dyn_into::<Textarea>().ok()) else { return };
        let pos = el.selection_start().ok().flatten().unwrap_or(0) as usize;
        let text = el.value();
        let pos = shown.with_value(|v| v.content_pos(&text, pos));
        let chars: Vec<char> = content_sig.get_untracked().chars().collect();
        if let Some(name) = edit::link_at(&chars, pos, ctx.tags_untracked()) {
            e.prevent_default();
            follow_link(ctx, name);
        }
    };

    // Jump to a link (from the "pages not created" list) or a line (from a tag search)
    let tab_goto = tab.clone();
    Effect::new(move |_| {
        let Some((path, goto)) = ctx.work.goto.get() else { return };
        if path != tab_goto.path || ctx.work.focus_second.get_untracked() != second { return; }
        let Some(el) = area_ref.get() else { return };
        ctx.work.goto.set(None);
        let chars: Vec<char> = tab_goto.content.get_untracked().chars().collect();
        let range = match goto {
            Goto::Link(target) => {
                let ci = ctx.pref_untracked(|p| p.case_insensitive_links);
                crate::motion::find_link(&chars, &target, ci, ctx.tags_untracked())
            }
            Goto::Line(n) => {
                let start = edit::line_start(&chars, n);
                Some((start, start))
            }
        };
        if let Some((start, end)) = range {
            // Show the target's table as text first, if it is in one, and unfold it
            focus_pos.set(Some(start));
            folds.update(|f| f.retain(|&(a, b)| !(a < start && start <= b)));
            after_tick(move || {
                let Some((a, b)) = shown.try_with_value(|v| (v.view_pos(start), v.view_pos(end))) else { return };
                let line = el.value().chars().take(a).filter(|&c| c == '\n').count();
                let _ = el.focus();
                set_selection(&el, a, b);
                // Bring the line to the middle of the view
                let y = (line as f64 * LINE_HEIGHT - el.client_height() as f64 / 2.0).max(0.0);
                el.set_scroll_top(y as i32);
            });
        }
    });

    let highlighted = move || view.with(|v| highlight::render_view(v, ctx.tags(), ctx.pref(|p| p.show_line_numbers)));
    let region_html = move || {
        if !ctx.pref(|p| p.emacs_mark) { return String::new(); }
        mark.get().map(|m| {
            let p = point.get();
            view.with(|v| highlight::render_region(&v.text, m.min(p), m.max(p)))
        }).unwrap_or_default()
    };
    let match_html = move || matches.with(|m| view.with(|v| {
        let m: Vec<(usize, usize)> = m.iter().map(|&(a, b)| (v.view_pos(a), v.view_pos(b))).collect();
        highlight::render_matches(&v.text, &m, current.get())
    }));
    // Mouse moves the caret too
    let on_pointer = move |e: web_sys::MouseEvent| {
        let Some(el) = e.target().and_then(|t| t.dyn_into::<Textarea>().ok()) else { return };
        // A click on a drawn table shows it as text
        if !e.ctrl_key() { track_caret(&el); }
        if mark.get_untracked().is_some() {
            after_tick(move || point.set(caret(&el)));
        }
    };

    // Brief pulse at the caret when the keyboard focus lands in this editor
    // (C-x o, Tab from another panel…), so the eye finds the cursor.
    // Not on a click: the cursor is then where the mouse is.
    let ping = RwSignal::new(None::<(f64, f64, f64)>);
    let ping_seq = StoredValue::new(0u32);
    let mouse_focus = StoredValue::new(false);
    let on_focus = move |_| {
        if mouse_focus.get_value() { return; }
        after_tick(move || {
            let Some(el) = area_ref.try_get_untracked().flatten() else { return };
            let chars: Vec<char> = el.value().chars().collect();
            let pos = caret(&el).min(chars.len());
            let Some(coords) = caret_coords(&el, &chars, pos) else { return };
            let Some(seq) = ping_seq.try_get_value().map(|n| n + 1) else { return };
            ping_seq.set_value(seq);
            ping.set(Some(coords));
            crate::keybindings::after_ms(800, move || {
                if ping_seq.try_get_value() == Some(seq) { ping.set(None); }
            });
        });
    };

    view! {
        <div
            class=move || if ctx.pref(|p| p.show_line_numbers) { "editor-wrap line-numbers" } else { "editor-wrap" }
            style=move || content_sig.with(|c| format!("--ln-w: {}ch", number_digits(c)))
        >
            <div class="hl-layer" aria-hidden="true" inner_html=highlighted />
            <div class="hl-layer region-layer" aria-hidden="true" inner_html=region_html />
            <div class="hl-layer match-layer" aria-hidden="true" inner_html=match_html />
            <textarea
                node_ref=area_ref
                class="edit-layer"
                spellcheck=false
                on:input=on_input
                on:keydown=on_keydown
                on:click=on_click
                on:mouseup=on_pointer
                on:mousedown=move |_| {
                    completion.set(None);
                    mouse_focus.set_value(true);
                    after_tick(move || mouse_focus.set_value(false));
                }
                on:focus=on_focus
                on:blur=move |_| completion.set(None)
                on:scroll=move |_| completion.set(None)
            />
            {move || find.get().map(|with_replace| {
                let lang = ctx.lang.get();
                view! {
                    <div class="find-bar" on:mousedown=|e: web_sys::MouseEvent| e.stop_propagation()>
                        <div class="find-row">
                            <button
                                class=if with_replace { "find-btn find-toggle open" } else { "find-btn find-toggle" }
                                title=t("find_replace_toggle", lang)
                                on:click=move |_| find.set(Some(!with_replace))
                            >"›"</button>
                            <input
                                node_ref=find_ref
                                class="find-input"
                                type="text"
                                placeholder=t("find_placeholder", lang)
                                prop:value=move || query.get()
                                on:input=move |e| { query.set(event_target_value(&e)); show_from_anchor(); }
                                on:keydown=move |e| on_find_key(e, false)
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
                                on:click=move |_| { match_case.update(|c| *c = !*c); show_from_anchor(); }
                            >"Aa"</button>
                            <button class="find-btn" title=t("find_prev_title", lang) on:click=move |_| step(false)>"↑"</button>
                            <button class="find-btn" title=t("find_next_title", lang) on:click=move |_| step(true)>"↓"</button>
                            <button class="find-btn" on:click=move |_| close_find()>"×"</button>
                        </div>
                        {with_replace.then(|| view! {
                            <div class="find-row find-replace-row">
                                <input
                                    node_ref=replace_ref
                                    class="find-input"
                                    type="text"
                                    placeholder=t("replace_placeholder", lang)
                                    prop:value=move || replacement.get()
                                    on:input=move |e| replacement.set(event_target_value(&e))
                                    on:keydown=move |e| on_find_key(e, true)
                                />
                                <button class="find-btn find-text-btn" on:click=move |_| replace_current()>
                                    {t("replace_one", lang)}
                                </button>
                                <button class="find-btn find-text-btn" on:click=move |_| replace_all()>
                                    {t("replace_all", lang)}
                                </button>
                            </div>
                        })}
                    </div>
                }
            })}
            {move || ping.get().map(|(x, top, bottom)| view! {
                <div
                    class="caret-ping"
                    aria-hidden="true"
                    style=format!("left: {x}px; top: {top}px; height: {}px;", bottom - top)
                />
            })}
            {move || completion.get().map(|c| view! {
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
                                    if let Some(el) = area_ref.get() { accept_completion(&el, &pick); }
                                }
                            >
                                <span class="page-completion-sigil">{match c.kind {
                                    CompletionKind::Link => "[[",
                                    CompletionKind::Hashtag => "#",
                                    CompletionKind::OrgTag => ":",
                                }}</span>
                                {name.clone()}
                            </li>
                        }
                    }).collect_view()}
                </ul>
            })}
        </div>
    }
}
