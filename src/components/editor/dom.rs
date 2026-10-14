//! The textarea of the editor and the browser around it: selection, undoable
//! edits, clipboard, and the sizes the drawn view is laid out with.

use wasm_bindgen::JsCast;

use crate::{keybindings::after_ms, tables};

pub type Textarea = web_sys::HtmlTextAreaElement;

/// Run an editing command of the document: done this way, the edit goes in the
/// browser's undo stack (Ctrl+Z).
fn exec(command: &str, value: &str) {
    let doc = web_sys::window().and_then(|w| w.document()).and_then(|d| d.dyn_into::<web_sys::HtmlDocument>().ok());
    if let Some(doc) = doc {
        let _ = doc.exec_command_with_show_ui_and_value(command, false, value);
    }
}

fn exec_insert(text: &str) {
    exec("insertText", text);
}


// ─── Clipboard (system) ──────────────────────────────────────────────────────

pub fn clipboard_write(text: &str) {
    let text = text.to_string();
    wasm_bindgen_futures::spawn_local(async move { crate::kill_ring::write_clipboard(&text).await; });
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

/// Where the view of a textarea scrolled by `scroll` goes so the line from `top`
/// to `bottom` (relative to the view, `height` high) is in the middle (`step` 0),
/// at the top (1) or at the bottom (2).
fn recenter_scroll(scroll: f64, top: f64, bottom: f64, height: f64, step: u8) -> f64 {
    let to = match step {
        0 => scroll + (top + bottom - height) / 2.0,
        1 => scroll + top,
        _ => scroll + bottom - height,
    };
    to.max(0.0)
}

thread_local! {
    /// The last recenter: the textarea, its caret, where it left the view, and its step.
    static RECENTERED: std::cell::RefCell<Option<(Textarea, usize, i32, u8)>> = const { std::cell::RefCell::new(None) };
}

/// Emacs's `recenter-top-bottom`: scroll the textarea so the line of the char at
/// `pos` is in the middle; again (caret and view left as they were), at the top,
/// then at the bottom, then in the middle again… The caret doesn't move.
pub fn recenter(el: &Textarea, chars: &[char], pos: usize) {
    let Some((_, top, bottom)) = caret_coords(el, chars, pos) else { return };
    let step = RECENTERED.with(|r| match &*r.borrow() {
        Some((field, at, scroll, step)) if field == el && *at == pos && *scroll == el.scroll_top() => (step + 1) % 3,
        _ => 0,
    });
    let to = recenter_scroll(el.scroll_top() as f64, top, bottom, el.client_height() as f64, step);
    el.set_scroll_top(to.round() as i32);
    RECENTERED.with(|r| *r.borrow_mut() = Some((el.clone(), pos, el.scroll_top(), step)));
}

/// Scroll the textarea so the char at `pos` is in view (in the middle when it was not).
pub fn reveal(el: &Textarea, chars: &[char], pos: usize) {
    let Some((_, top, bottom)) = caret_coords(el, chars, pos) else { return };
    let height = el.client_height() as f64;
    if top < 0.0 || bottom > height {
        el.set_scroll_top((el.scroll_top() as f64 + top - height / 2.0).max(0.0) as i32);
    }
}

#[cfg(test)]
mod tests {
    use super::recenter_scroll;

    #[test]
    fn recenter_puts_the_line_in_the_middle_then_top_then_bottom() {
        // View 400 px high scrolled by 1000; the line is 20 px high, 300 px down the view
        assert_eq!(recenter_scroll(1000.0, 300.0, 320.0, 400.0, 0), 1110.0);
        assert_eq!(recenter_scroll(1000.0, 300.0, 320.0, 400.0, 1), 1300.0);
        assert_eq!(recenter_scroll(1000.0, 300.0, 320.0, 400.0, 2), 920.0);
        // Near the start of the page, the view doesn't go above it
        assert_eq!(recenter_scroll(0.0, 20.0, 40.0, 400.0, 0), 0.0);
    }
}
