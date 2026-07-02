use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::{spawn_local, JsFuture};

use crate::{highlight, i18n::t, invoke, state::{AppCtx, Tab}}; // FileEntry inferred

/// Returns the wiki-link target at char position `pos`, scanning the full text.
/// Handles both `[[target]]` and `[[target][display]]` forms.
fn find_link_at_pos(text: &str, pos: usize) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut i = 0;

    while i + 1 < n {
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

// ─── Components ──────────────────────────────────────────────────────────────

#[component]
pub fn EditorArea() -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");

    view! {
        <div class="editor-area">
            {move || match ctx.active_tab_data() {
                None => view! {
                    <div class="editor-empty">
                        <p>{t("no_file", ctx.lang.get())}</p>
                        <p>{t("open_hint", ctx.lang.get())}</p>
                    </div>
                }.into_any(),
                Some(tab) => view! { <Editor tab=tab /> }.into_any(),
            }}
        </div>
    }
}

#[component]
fn Editor(tab: Tab) -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");

    let tab_input = tab.clone();
    let tab_key   = tab.clone();
    let tab_hl    = tab.clone();
    let tab_click = tab.clone();

    let on_input = move |e: web_sys::Event| {
        let el = e.target().unwrap().dyn_into::<Textarea>().unwrap();
        tab_input.content.set(el.value());
        tab_input.dirty.set(true);
    };

    let on_keydown = move |e: web_sys::KeyboardEvent| {
        let el: Textarea = e.target().unwrap().dyn_into().unwrap();
        let kb = ctx.keybindings.get();

        // Helper: sync Leptos signal after DOM edit
        let sync = |new_val: String| {
            tab_key.content.set(new_val);
            tab_key.dirty.set(true);
        };

        // ── Auto-pairing ──────────────────────────────────────────────────────
        let close_char = match e.key().as_str() {
            "[" => Some("]"),
            "(" => Some(")"),
            "{" => Some("}"),
            "\"" => Some("\""),
            _ => None,
        };
        if let Some(close) = close_char {
            e.prevent_default();
            let (start, end) = get_pos(&el);
            let open = e.key();
            if start != end {
                let val = el.value();
                let chars: Vec<char> = val.chars().collect();
                let selected: String = chars[start..end].iter().collect();
                let wrapped = format!("{}{}{}", open, selected, close);
                let new_end = splice(&el, start, end, &wrapped);
                set_selection(&el, start + 1, new_end - 1);
            } else {
                let pair = format!("{}{}", open, close);
                let cursor = splice(&el, start, start, &pair);
                set_cursor(&el, cursor - 1);
            }
            sync(el.value());
            return;
        }

        // ── Configured editor action (classic or emacs preset) ────────────────
        let Some(action) = crate::keybindings::match_action(&kb.editor, &e) else { return };
        e.prevent_default();
        e.stop_propagation();

        match action {
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
                let (start, end) = get_pos(&el);
                if start < end {
                    let chars: Vec<char> = el.value().chars().collect();
                    ctx.kill_ring.set(chars[start..end].iter().collect());
                    let cursor = splice(&el, start, end, "");
                    set_cursor(&el, cursor);
                    sync(el.value());
                }
            }
            "copy_region" => {
                let (start, end) = get_pos(&el);
                if start < end {
                    let chars: Vec<char> = el.value().chars().collect();
                    ctx.kill_ring.set(chars[start..end].iter().collect());
                    set_selection(&el, start, end); // preserve selection
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

    // Ctrl+Click navigates to (or creates) the wiki link under the cursor
    let on_click = move |e: web_sys::MouseEvent| {
        if !e.ctrl_key() { return; }
        let el: Textarea = match e.target().and_then(|t| t.dyn_into().ok()) {
            Some(el) => el,
            None => return,
        };
        let pos = el.selection_start().ok().flatten().unwrap_or(0) as usize;
        let content = tab_click.content.get();
        let Some(link_name) = find_link_at_pos(&content, pos) else { return };

        e.prevent_default();
        // Pre-allocate signals before async boundary
        let content_sig = RwSignal::new(String::new());
        let dirty_sig   = RwSignal::new(false);

        spawn_local(async move {
            // Already open?
            if let Some(idx) = ctx.tabs.get().iter().position(|t| t.name == link_name) {
                ctx.active_tab.set(Some(idx));
                return;
            }
            // Resolve: existing file or create new one?
            let (file, init_content, is_dirty) = {
                let files = ctx.files.get();
                if let Some(f) = files.iter().find(|f| f.name == link_name).cloned() {
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
    };

    let highlighted = move || highlight::render(&tab_hl.content.get());

    view! {
        <div class="editor-wrap">
            <div class="hl-layer" aria-hidden="true" inner_html=highlighted />
            <textarea
                class="edit-layer"
                spellcheck=false
                prop:value=move || tab.content.get()
                on:input=on_input
                on:keydown=on_keydown
                on:click=on_click
            />
        </div>
    }
}
