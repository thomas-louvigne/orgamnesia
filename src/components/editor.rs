use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::{spawn_local, JsFuture};

use crate::{
    actions::follow_link,
    components::tabs::TabBar,
    edit,
    highlight,
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
            let textarea = area_ref.get_untracked()
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
                let name = path.get()
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
            <div class="editor-pane-body">
            {move || match path.get() {
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

    let tab_input = tab.clone();
    let tab_key   = tab.clone();
    let tab_hl    = tab.clone();
    let tab_click = tab.clone();

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
        if ctx.pref_untracked(|p| p.emacs_mark) {
            if let Some(m) = mark.get_untracked() {
                let p = caret(el);
                return (m.min(p), m.max(p));
            }
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
                .or_else(|| crate::motion::org_tag_prefix(&chars, caret)
                    .map(|(start, p)| (start, p, CompletionKind::OrgTag)))
                .or_else(|| crate::motion::hashtag_prefix(&chars, caret, ctx.hashtags_untracked())
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
                let tag = (kind == CompletionKind::Hashtag).then(|| ctx.hashtags_untracked());
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
        content_sig.set(el.value());
        dirty_sig.set(true);
    };

    let on_input = move |e: web_sys::Event| {
        let el = e.target().unwrap().dyn_into::<Textarea>().unwrap();
        // Typing ends the region
        mark.set(None);
        tab_input.content.set(el.value());
        tab_input.dirty.set(true);
        if !accepting.get_value() { refresh_completion(&el); }
    };

    let on_keydown = move |e: web_sys::KeyboardEvent| {
        let el: Textarea = e.target().unwrap().dyn_into().unwrap();

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

        // Helper: sync Leptos signal after DOM edit
        let sync = |new_val: String| {
            tab_key.content.set(new_val);
            tab_key.dirty.set(true);
        };

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
                    sync(el.value());
                    return;
                }
            } else if edit::auto_closes(&key) {
                e.prevent_default();
                let cursor = splice(&el, start, start, &format!("{open}{close}"));
                set_cursor(&el, cursor - 1);
                sync(el.value());
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
        e.prevent_default();
        e.stop_propagation();

        let chars: Vec<char> = el.value().chars().collect();
        let text = |a: usize, b: usize| -> String { chars[a..b].iter().collect() };
        // Replace a..b with `with`, caret after it; the page has changed
        let replace = |a: usize, b: usize, with: &str| {
            let cursor = splice(&el, a, b, with);
            set_cursor(&el, cursor);
            sync(el.value());
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
                let (content, dirty) = (tab_key.content, tab_key.dirty);
                let el = el.clone();
                if let Some(cb) = clipboard() {
                    let promise = cb.read_text();
                    spawn_local(async move {
                        let pasted = JsFuture::from(promise).await.ok().and_then(|v| v.as_string());
                        if let Some(pasted) = pasted.filter(|p| !p.is_empty()) {
                            let (start, end) = get_pos(&el);
                            let cursor = splice(&el, start, end, &pasted);
                            set_cursor(&el, cursor);
                            content.set(el.value());
                            dirty.set(true);
                        }
                    });
                }
            }
            EditorAction::SelectAll => el.select(),
            EditorAction::Undo => exec_undo(),
            EditorAction::Redo => exec_redo(),

            // ── Org: new heading at the same level as the current one ─────────
            EditorAction::NewHeading => {
                let (at, heading) = edit::new_heading(&chars, end);
                replace(at, at, &heading);
            }
            EditorAction::OpenLink => {
                if let Some(name) = edit::link_at(&chars, start, ctx.hashtags_untracked()) {
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
            EditorAction::KeyboardQuit => mark.set(None),
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
        let chars: Vec<char> = tab_click.content.get_untracked().chars().collect();
        if let Some(name) = edit::link_at(&chars, pos, ctx.hashtags_untracked()) {
            e.prevent_default();
            follow_link(ctx, name);
        }
    };

    // Jump to a link (from the "pages not created" list) or a line (from a tag search)
    let area_ref = NodeRef::<leptos::html::Textarea>::new();
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
                crate::motion::find_link(&chars, &target, ci, ctx.hashtags_untracked())
            }
            Goto::Line(n) => {
                let start = edit::line_start(&chars, n);
                Some((start, start))
            }
        };
        if let Some((start, end)) = range {
            let line = chars[..start].iter().filter(|&&c| c == '\n').count();
            let _ = el.focus();
            set_selection(&el, start, end);
            // Bring the line to the middle of the view (14px font, 1.65 line height)
            let y = (line as f64 * 14.0 * 1.65 - el.client_height() as f64 / 2.0).max(0.0);
            el.set_scroll_top(y as i32);
        }
    });

    let highlighted = move || highlight::render(&tab_hl.content.get(), ctx.hashtags());
    let tab_region = tab.clone();
    let region_html = move || {
        if !ctx.pref(|p| p.emacs_mark) { return String::new(); }
        mark.get().map(|m| {
            let p = point.get();
            highlight::render_region(&tab_region.content.get(), m.min(p), m.max(p))
        }).unwrap_or_default()
    };
    // Mouse moves the caret too
    let on_pointer = move |e: web_sys::MouseEvent| {
        if mark.get_untracked().is_some() {
            if let Some(el) = e.target().and_then(|t| t.dyn_into::<Textarea>().ok()) {
                after_tick(move || point.set(caret(&el)));
            }
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
            let Some(el) = area_ref.get_untracked() else { return };
            let chars: Vec<char> = el.value().chars().collect();
            let pos = caret(&el).min(chars.len());
            let Some(coords) = caret_coords(&el, &chars, pos) else { return };
            let seq = ping_seq.get_value() + 1;
            ping_seq.set_value(seq);
            ping.set(Some(coords));
            crate::keybindings::after_ms(800, move || {
                if ping_seq.get_value() == seq { ping.set(None); }
            });
        });
    };

    view! {
        <div class="editor-wrap">
            <div class="hl-layer" aria-hidden="true" inner_html=highlighted />
            <div class="hl-layer region-layer" aria-hidden="true" inner_html=region_html />
            <textarea
                node_ref=area_ref
                class="edit-layer"
                spellcheck=false
                prop:value=move || tab.content.get()
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
