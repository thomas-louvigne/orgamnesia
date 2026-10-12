//! The textarea of the editor and the browser around it: selection, undoable
//! edits, clipboard, and the sizes the drawn view is laid out with.

use wasm_bindgen::JsCast;

use crate::{keybindings::after_ms, tables};

pub type Textarea = web_sys::HtmlTextAreaElement;

fn exec_insert(text: &str) {
    if let Ok(encoded) = serde_json::to_string(text) {
        let _ = js_sys::eval(&format!("document.execCommand('insertText',false,{})", encoded));
    }
}

pub fn exec_undo() {
    let _ = js_sys::eval("document.execCommand('undo')");
}

pub fn exec_redo() {
    let _ = js_sys::eval("document.execCommand('redo')");
}

// ─── Clipboard (system) ──────────────────────────────────────────────────────

pub fn clipboard() -> Option<web_sys::Clipboard> {
    web_sys::window().map(|w| w.navigator().clipboard())
}

pub fn clipboard_write(text: &str) {
    if let Some(cb) = clipboard() {
        let _ = cb.write_text(text);
    }
}

// ─── Selection ───────────────────────────────────────────────────────────────

pub fn get_pos(el: &Textarea) -> (usize, usize) {
    let s = el.selection_start().ok().flatten().unwrap_or(0) as usize;
    let e = el.selection_end().ok().flatten().unwrap_or(0) as usize;
    (s, e)
}

/// Caret position: the moving end of the selection.
pub fn caret(el: &Textarea) -> usize {
    let (s, e) = get_pos(el);
    match el.selection_direction().ok().flatten().as_deref() {
        Some("backward") => s,
        _ => e,
    }
}

/// Run `f` once the browser has applied the default action of the current event.
pub fn after_tick(f: impl FnOnce() + 'static) {
    after_ms(0, f);
}

pub fn set_cursor(el: &Textarea, pos: usize) {
    let p = pos as u32;
    el.set_selection_start(Some(p)).ok();
    el.set_selection_end(Some(p)).ok();
}

pub fn set_selection(el: &Textarea, start: usize, end: usize) {
    el.set_selection_start(Some(start as u32)).ok();
    el.set_selection_end(Some(end as u32)).ok();
}

/// Replace [del_start..del_end) with `insert`, returns new cursor position.
/// Uses execCommand so the browser's undo stack (Ctrl+Z) stays intact.
pub fn splice(el: &Textarea, del_start: usize, del_end: usize, insert: &str) -> usize {
    el.set_selection_start(Some(del_start as u32)).ok();
    el.set_selection_end(Some(del_end as u32)).ok();
    exec_insert(insert);
    del_start + insert.chars().count()
}

// ─── Layout ──────────────────────────────────────────────────────────────────

/// Height of a line of the editor (14px font, 1.65 line height).
pub const LINE_HEIGHT: f64 = 14.0 * 1.65;

/// Width of a character of the editor font (monospace, 14px).
pub const CHAR_WIDTH: f64 = 14.0 * 0.6;

/// Width in characters of the text of a textarea `width` px wide (24px padding each
/// side). With line numbers of `digits` digits, the left padding is 4px and the
/// numbers and their margin take `digits + 1.5` columns (see `.line-numbers` in the CSS).
pub fn text_columns(width: i32, digits: Option<usize>) -> usize {
    let (padding, gutter) = digits.map_or((48.0, 0.0), |d| (28.0, d as f64 + 1.5));
    ((width as f64 - padding).max(0.0) / CHAR_WIDTH - gutter).max(0.0) as usize
}

/// Digits of the line numbers of `content`: those of its last line, at least 2.
pub fn number_digits(content: &str) -> usize {
    (content.matches('\n').count() + 1).to_string().len().max(2)
}

/// Lines of text a table takes once drawn, the text being `width` px wide.
pub fn table_lines(el: &Textarea, raw: &str, width: i32, digits: Option<usize>) -> Option<usize> {
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

/// Caret position in pixels (left, line top, line bottom) relative to the editor
/// wrapper, measured with a hidden copy of the text laid out like the highlight layer.
pub fn caret_coords(el: &Textarea, chars: &[char], pos: usize) -> Option<(f64, f64, f64)> {
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

/// Scroll the textarea so the char at `pos` is in view (in the middle when it was not).
pub fn reveal(el: &Textarea, chars: &[char], pos: usize) {
    let Some((_, top, bottom)) = caret_coords(el, chars, pos) else { return };
    let height = el.client_height() as f64;
    if top < 0.0 || bottom > height {
        el.set_scroll_top((el.scroll_top() as f64 + top - height / 2.0).max(0.0) as i32);
    }
}
