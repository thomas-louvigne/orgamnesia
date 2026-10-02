use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::{spawn_local, JsFuture};

use crate::{
    highlight,
    i18n::t,
    invoke,
    keybindings::{Resolution, Scope},
    state::{AppCtx, Goto, SplitKind, Tab},
}; // FileEntry inferred

/// Returns the wiki-link target at char position `pos`, scanning the full text.
/// Handles both `[[target]]` and `[[target][display]]` forms.
fn find_link_at_pos(text: &str, pos: usize, hashtags: crate::motion::Hashtags) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut i = 0;

    while i < n {
        if let Some((tag, end)) = crate::motion::hashtag_at(&chars, i, hashtags) {
            if pos >= i && pos < end { return Some(tag); }
            i = end;
            continue;
        }
        if i + 1 >= n { break; }
        if chars[i] != '[' || chars[i + 1] != '[' {
            i += 1;
            continue;
        }
        let link_start = i;
        let content_start = i + 2;
        // Find end of target (]] or ][)
        let mut j = content_start;
        while j < n && !(chars[j] == ']' && j + 1 < n && (chars[j + 1] == ']' || chars[j + 1] == '[')) {
            j += 1;
        }
        if j + 1 >= n { break; }
        let target: String = chars[content_start..j].iter().collect();
        if chars[j + 1] == ']' {
            // [[target]]
            let link_end = j + 2;
            if pos >= link_start && pos < link_end {
                return Some(target.trim().to_string());
            }
            i = link_end;
        } else {
            // [[target][display]]
            let mut k = j + 2;
            while k + 1 < n && !(chars[k] == ']' && chars[k + 1] == ']') {
                k += 1;
            }
            if k + 1 < n {
                let link_end = k + 2;
                if pos >= link_start && pos < link_end {
                    return Some(target.trim().to_string());
                }
                i = link_end;
            } else {
                i += 1;
            }
        }
    }
    None
}

/// Open the page named `link_name` in a tab, creating it when it doesn't exist yet.
fn follow_link(ctx: AppCtx, link_name: String) {
    // Pre-allocate signals before async boundary
    let content_sig = RwSignal::new(String::new());
    let dirty_sig   = RwSignal::new(false);

    spawn_local(async move {
        // Already open?
        if let Some(idx) = ctx.tabs.get().iter().position(|t| ctx.same_page(&t.name, &link_name)) {
            ctx.active_tab.set(Some(idx));
            return;
        }
        // Resolve: existing file or create new one?
        let (file, init_content, is_dirty) = {
            let files = ctx.files.get();
            if let Some(f) = files.iter().find(|f| ctx.same_page(&f.name, &link_name)).cloned() {
                match invoke::read_file(&f.path).await {
                    Ok(c)    => (f, c, false),
                    Err(err) => { ctx.status.set(Some(format!("Error: {err}"))); return; }
                }
            } else {
                match invoke::create_page(&link_name).await {
                    Ok(f) => {
                        let init = format!("* {}\n", f.name);
                        ctx.files.update(|fs| {
                            fs.push(f.clone());
                            fs.sort_by(|a, b| a.name.cmp(&b.name));
                        });
                        (f, init, true)
                    }
                    Err(err) => { ctx.status.set(Some(format!("Error: {err}"))); return; }
                }
            }
        };
        content_sig.set(init_content);
        dirty_sig.set(is_dirty);
        ctx.tabs.update(|tabs| {
            tabs.push(Tab {
                path: file.path.clone(),
                name: file.name.clone(),
                content: content_sig,
                dirty: dirty_sig,
            });
        });
        let idx = ctx.tabs.get().len() - 1;
        ctx.active_tab.set(Some(idx));
    });
}

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

// ─── Emacs editing helpers ────────────────────────────────────────────────────

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
    if let Some(w) = web_sys::window() {
        let cb = wasm_bindgen::closure::Closure::once_into_js(f);
        let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(cb.unchecked_ref(), 0);
    }
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

fn line_bounds(chars: &[char], pos: usize) -> (usize, usize) {
    let start = chars[..pos].iter().rposition(|&c| c == '\n').map(|i| i + 1).unwrap_or(0);
    let end   = chars[pos..].iter().position(|&c| c == '\n').map(|i| pos + i).unwrap_or(chars.len());
    (start, end)
}

/// Position `delta` lines down (negative: up) keeping the column when possible.
fn move_lines(chars: &[char], pos: usize, delta: i32) -> usize {
    let (mut start, _) = line_bounds(chars, pos);
    let col = pos - start;
    for _ in 0..delta.unsigned_abs() {
        if delta > 0 {
            let (_, end) = line_bounds(chars, start);
            if end >= chars.len() { return chars.len(); }
            start = end + 1;
        } else {
            if start == 0 { return pos; }
            start = line_bounds(chars, start - 1).0;
        }
    }
    let (s, e) = line_bounds(chars, start);
    (s + col).min(e)
}

/// Lines moved by the page-up / page-down actions.
const PAGE_LINES: i32 = 20;

fn word_end_forward(chars: &[char], pos: usize) -> usize {
    let mut i = pos;
    while i < chars.len() && !chars[i].is_alphanumeric() { i += 1; }
    while i < chars.len() &&  chars[i].is_alphanumeric() { i += 1; }
    i
}

fn word_start_backward(chars: &[char], pos: usize) -> usize {
    let mut i = pos;
    while i > 0 && !chars[i - 1].is_alphanumeric() { i -= 1; }
    while i > 0 &&  chars[i - 1].is_alphanumeric() { i -= 1; }
    i
}

/// If the line `[start..end)` is an org heading (`*`… followed by a space or EOL),
/// return its level (number of leading `*`).
fn heading_level(chars: &[char], start: usize, end: usize) -> Option<usize> {
    let mut i = start;
    while i < end && chars[i] == '*' { i += 1; }
    let level = i - start;
    if level > 0 && (i >= end || chars[i] == ' ') { Some(level) } else { None }
}

/// Heading level in effect at `caret`: the current line if it is a heading,
/// otherwise the nearest heading above it. Defaults to 1 when none is found.
fn nearest_heading_level(chars: &[char], caret: usize) -> usize {
    let mut line_start = chars[..caret].iter().rposition(|&c| c == '\n').map(|i| i + 1).unwrap_or(0);
    loop {
        let line_end = chars[line_start..].iter().position(|&c| c == '\n').map(|i| line_start + i).unwrap_or(chars.len());
        if let Some(l) = heading_level(chars, line_start, line_end) { return l; }
        if line_start == 0 { return 1; }
        line_start = chars[..line_start - 1].iter().rposition(|&c| c == '\n').map(|i| i + 1).unwrap_or(0);
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
        <div class=move || match ctx.split.get() {
            None => "editor-split",
            Some(SplitKind::Vertical) => "editor-split vertical",
            Some(SplitKind::Horizontal) => "editor-split horizontal",
        }>
            <EditorArea second=false />
            {move || ctx.split.get().is_some().then(|| view! { <EditorArea second=true /> })}
        </div>
    }
}

#[component]
pub fn EditorArea(second: bool) -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");

    // The focused pane shows the active tab, the other one `other_path`
    let path = Memo::new(move |_| -> Option<String> {
        if ctx.focus_second.get() == second {
            ctx.active_tab_data().map(|t| t.path)
        } else {
            ctx.other_path.get()
        }
    });
    let focused = move || ctx.split.get().is_none() || ctx.focus_second.get() == second;

    // When this pane takes the focus (C-x o, C-o…), the keyboard must follow:
    // put the text cursor in its editor.
    let area_ref = NodeRef::<leptos::html::Div>::new();
    Effect::new(move |_| {
        let has_focus = ctx.focus_second.get() == second;
        if ctx.split.get().is_none() || !has_focus { return; }
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
            on:mousedown=move |_| ctx.focus_pane(second)
        >
            {move || (ctx.split.get() == Some(SplitKind::Vertical)).then(|| {
                let name = path.get()
                    .and_then(|p| ctx.tabs.get().into_iter().find(|t| t.path == p))
                    .map(|t| t.name)
                    .unwrap_or_default();
                view! { <div class="pane-tab"><span class="pane-tab-name">{name}</span></div> }
            })}
            <div class="editor-pane-body">
            {move || match path.get() {
                None => view! {
                    <div class="editor-empty">
                        {move || ctx.vault_path.get().is_none().then(|| view! {
                            <img class="empty-logo" src="app-icon.svg" alt="" />
                        })}
                        <p>{t("no_file", ctx.lang.get())}</p>
                        <p>{t("open_hint", ctx.lang.get())}</p>
                    </div>
                }.into_any(),
                Some(p) => {
                    let tab = ctx.tabs.get_untracked().into_iter().find(|t| t.path == p);
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
        if !ctx.autosave.get() || !tab_auto.dirty.get_untracked() || saving.get_untracked() {
            return;
        }
        saving.set(true);
        let tab = tab_auto.clone();
        spawn_local(async move {
            loop {
                let content = tab.content.get_untracked();
                if let Err(e) = invoke::write_file(&tab.path, &content).await {
                    ctx.status.set(Some(format!("Save error: {e}")));
                    break;
                }
                ctx.links_version.update(|v| *v += 1);
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
        if ctx.emacs_mark.get_untracked() {
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
                let names: Vec<String> = ctx.tags.get_untracked().into_iter().map(|t| t.name).collect();
                crate::motion::complete_page(&names, &prefix, None, MAX_SUGGESTIONS)
            }
            _ => {
                let names: Vec<String> = ctx.files.get_untracked().into_iter().map(|f| f.name).collect();
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
        let pair = match key.as_str() {
            "(" | ")" => Some(("(", ")")),
            "[" | "]" => Some(("[", "]")),
            "{" | "}" => Some(("{", "}")),
            "\"" => Some(("\"", "\"")),
            "'" => Some(("'", "'")),
            _ => None,
        };
        if let Some((open, close)) = pair {
            if start != end {
                if ctx.electric_mode.get_untracked() {
                    e.prevent_default();
                    let chars: Vec<char> = el.value().chars().collect();
                    let selected: String = chars[start..end].iter().collect();
                    let new_end = splice(&el, start, end, &format!("{open}{selected}{close}"));
                    set_selection(&el, start + 1, new_end - 1);
                    sync(el.value());
                    return;
                }
            } else if matches!(key.as_str(), "(" | "[" | "{" | "\"") {
                e.prevent_default();
                let cursor = splice(&el, start, start, &format!("{open}{close}"));
                set_cursor(&el, cursor - 1);
                sync(el.value());
                return;
            }
        }

        // ── Configured editor action (classic or emacs preset) ────────────────
        let action = match resolution {
            Resolution::Action(Scope::Editor, id) => id,
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
        e.prevent_default();
        e.stop_propagation();

        match action.as_str() {
            // ── Classic clipboard / editing ───────────────────────────────────
            "copy" => {
                let (start, end) = get_pos(&el);
                if start < end {
                    let chars: Vec<char> = el.value().chars().collect();
                    let selected: String = chars[start..end].iter().collect();
                    clipboard_write(&selected);
                    set_selection(&el, start, end); // preserve selection
                }
            }
            "cut" => {
                let (start, end) = get_pos(&el);
                if start < end {
                    let chars: Vec<char> = el.value().chars().collect();
                    let selected: String = chars[start..end].iter().collect();
                    clipboard_write(&selected);
                    let cursor = splice(&el, start, end, "");
                    set_cursor(&el, cursor);
                    sync(el.value());
                }
            }
            "paste" => {
                let content_sig = tab_key.content;
                let dirty_sig   = tab_key.dirty;
                let el2 = el.clone();
                if let Some(cb) = clipboard() {
                    let promise = cb.read_text();
                    spawn_local(async move {
                        if let Ok(v) = JsFuture::from(promise).await {
                            if let Some(text) = v.as_string() {
                                if !text.is_empty() {
                                    let (start, end) = get_pos(&el2);
                                    let cursor = splice(&el2, start, end, &text);
                                    set_cursor(&el2, cursor);
                                    content_sig.set(el2.value());
                                    dirty_sig.set(true);
                                }
                            }
                        }
                    });
                }
            }
            "select_all" => {
                el.select();
            }
            "undo" => exec_undo(),
            "redo" => exec_redo(),

            // ── Org: new heading at the same level as the current one ─────────
            "new_heading" => {
                let (_, caret) = get_pos(&el);
                let chars: Vec<char> = el.value().chars().collect();
                let level = nearest_heading_level(&chars, caret);
                let (_, line_end) = line_bounds(&chars, caret);
                let insert = format!("\n{} ", "*".repeat(level));
                let cursor = splice(&el, line_end, line_end, &insert);
                set_cursor(&el, cursor);
                sync(el.value());
            }

            // ── Emacs editing ─────────────────────────────────────────────────
            "kill_line" => {
                let (pos, _) = get_pos(&el);
                let chars: Vec<char> = el.value().chars().collect();
                let (_, line_end) = line_bounds(&chars, pos);
                let (del_start, del_end) = if pos == line_end && pos < chars.len() {
                    (pos, pos + 1) // kill the newline itself when at EOL
                } else {
                    (pos, line_end)
                };
                let killed: String = chars[del_start..del_end].iter().collect();
                ctx.kill_ring.set(killed);
                let cursor = splice(&el, del_start, del_end, "");
                set_cursor(&el, cursor);
                sync(el.value());
            }
            "kill_region" => {
                let (start, end) = region_of(&el);
                if start < end {
                    let chars: Vec<char> = el.value().chars().collect();
                    ctx.kill_ring.set(chars[start..end].iter().collect());
                    let cursor = splice(&el, start, end, "");
                    set_cursor(&el, cursor);
                    sync(el.value());
                }
                mark.set(None);
            }
            "copy_region" => {
                let (start, end) = region_of(&el);
                if start < end {
                    let chars: Vec<char> = el.value().chars().collect();
                    ctx.kill_ring.set(chars[start..end].iter().collect());
                    if mark.get_untracked().is_none() {
                        set_selection(&el, start, end); // preserve native selection
                    }
                }
                mark.set(None);
            }
            "set_mark" => {
                if ctx.emacs_mark.get_untracked() {
                    let c = caret(&el);
                    if mark.get_untracked() == Some(c) {
                        mark.set(None); // Ctrl+Space twice at the same spot cancels
                    } else {
                        mark.set(Some(c));
                        point.set(c);
                    }
                }
            }
            "keyboard_quit" => mark.set(None),
            "open_link" => {
                let content = tab_key.content.get_untracked();
                let pos = el.selection_start().ok().flatten().unwrap_or(0) as usize;
                if let Some(name) = find_link_at_pos(&content, pos, ctx.hashtags_untracked()) {
                    follow_link(ctx, name);
                } else if let Some(tag) = crate::motion::org_tag_at(&content.chars().collect::<Vec<_>>(), pos) {
                    follow_link(ctx, tag);
                }
            }
            "yank" => {
                let text = ctx.kill_ring.get();
                if !text.is_empty() {
                    let (start, end) = get_pos(&el);
                    let cursor = splice(&el, start, end, &text);
                    set_cursor(&el, cursor);
                    sync(el.value());
                }
            }
            "kill_word_forward" => {
                let (pos, _) = get_pos(&el);
                let chars: Vec<char> = el.value().chars().collect();
                let word_end = word_end_forward(&chars, pos);
                ctx.kill_ring.set(chars[pos..word_end].iter().collect());
                let cursor = splice(&el, pos, word_end, "");
                set_cursor(&el, cursor);
                sync(el.value());
            }
            "kill_word_backward" => {
                let (pos, _) = get_pos(&el);
                let chars: Vec<char> = el.value().chars().collect();
                let word_start = word_start_backward(&chars, pos);
                ctx.kill_ring.set(chars[word_start..pos].iter().collect());
                let cursor = splice(&el, word_start, pos, "");
                set_cursor(&el, cursor);
                sync(el.value());
            }
            // ── Emacs cursor movement ─────────────────────────────────────────
            "forward_char" | "backward_char" | "next_line" | "previous_line"
            | "forward_word" | "backward_word" | "beginning_of_buffer" | "end_of_buffer"
            | "scroll_down" | "scroll_up" | "forward_sentence" | "backward_sentence"
            | "forward_paragraph" | "backward_paragraph" | "back_to_indentation"
            | "next_heading" | "previous_heading" => {
                let pos = caret(&el);
                let chars: Vec<char> = el.value().chars().collect();
                let target = match action.as_str() {
                    "forward_char" => (pos + 1).min(chars.len()),
                    "backward_char" => pos.saturating_sub(1),
                    "next_line" => move_lines(&chars, pos, 1),
                    "previous_line" => move_lines(&chars, pos, -1),
                    "forward_word" => word_end_forward(&chars, pos),
                    "backward_word" => word_start_backward(&chars, pos),
                    "beginning_of_buffer" => 0,
                    "end_of_buffer" => chars.len(),
                    "forward_sentence" => crate::motion::forward_sentence(&chars, pos),
                    "backward_sentence" => crate::motion::backward_sentence(&chars, pos),
                    "forward_paragraph" => crate::motion::forward_paragraph(&chars, pos),
                    "backward_paragraph" => crate::motion::backward_paragraph(&chars, pos),
                    "back_to_indentation" => crate::motion::back_to_indentation(&chars, pos),
                    "next_heading" => crate::motion::next_heading(&chars, pos),
                    "previous_heading" => crate::motion::previous_heading(&chars, pos),
                    "scroll_down" => move_lines(&chars, pos, PAGE_LINES),
                    _ => move_lines(&chars, pos, -PAGE_LINES),
                };
                set_cursor(&el, target);
            }
            "beginning_of_line" => {
                let (pos, _) = get_pos(&el);
                let chars: Vec<char> = el.value().chars().collect();
                let (line_start, _) = line_bounds(&chars, pos);
                set_cursor(&el, line_start);
            }
            "end_of_line" => {
                let (pos, _) = get_pos(&el);
                let chars: Vec<char> = el.value().chars().collect();
                let (_, line_end) = line_bounds(&chars, pos);
                set_cursor(&el, line_end);
            }
            "delete_char_forward" => {
                let (start, end) = get_pos(&el);
                let chars: Vec<char> = el.value().chars().collect();
                let del_end = if start != end { end } else { (start + 1).min(chars.len()) };
                let cursor = splice(&el, start, del_end, "");
                set_cursor(&el, cursor);
                sync(el.value());
            }
            _ => {}
        }
    };

    // Ctrl+Click navigates to (or creates) the page of the wiki link, #tag or
    // org-mode :tag: under the cursor
    let on_click = move |e: web_sys::MouseEvent| {
        if !e.ctrl_key() { return; }
        let el: Textarea = match e.target().and_then(|t| t.dyn_into().ok()) {
            Some(el) => el,
            None => return,
        };
        let pos = el.selection_start().ok().flatten().unwrap_or(0) as usize;
        let content = tab_click.content.get();
        if let Some(link_name) = find_link_at_pos(&content, pos, ctx.hashtags_untracked()) {
            e.prevent_default();
            follow_link(ctx, link_name);
        } else if let Some(tag) = crate::motion::org_tag_at(&content.chars().collect::<Vec<_>>(), pos) {
            e.prevent_default();
            follow_link(ctx, tag);
        }
    };

    // Jump to a link (from the "pages not created" list) or a line (from a tag search)
    let area_ref = NodeRef::<leptos::html::Textarea>::new();
    let tab_goto = tab.clone();
    Effect::new(move |_| {
        let Some((path, goto)) = ctx.goto.get() else { return };
        if path != tab_goto.path || ctx.focus_second.get_untracked() != second { return; }
        let Some(el) = area_ref.get() else { return };
        ctx.goto.set(None);
        let chars: Vec<char> = tab_goto.content.get_untracked().chars().collect();
        let range = match goto {
            Goto::Link(target) => {
                let ci = ctx.case_insensitive_links.get_untracked();
                crate::motion::find_link(&chars, &target, ci, ctx.hashtags_untracked())
            }
            Goto::Line(n) => {
                let start = if n == 0 { 0 } else {
                    chars.iter().enumerate().filter(|(_, &c)| c == '\n').nth(n - 1)
                        .map_or(chars.len(), |(i, _)| i + 1)
                };
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
        if !ctx.emacs_mark.get() { return String::new(); }
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
                on:mousedown=move |_| completion.set(None)
                on:blur=move |_| completion.set(None)
                on:scroll=move |_| completion.set(None)
            />
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
