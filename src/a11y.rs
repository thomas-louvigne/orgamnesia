//! Keyboard access to the panels around the editor: moving between the regions
//! of the window (F6) and through the items of their lists.

use wasm_bindgen::JsCast;
use web_sys::{Element, HtmlElement, KeyboardEvent};

/// The regions F6 cycles through, in the order of the window.
const REGIONS: &str = ".sidebar, .editor-area, .backlinks-panel";

/// What takes the focus when a region is entered: the open page in the list, else
/// the first item, the editor's text, or the first field.
const ENTRY: &[&str] = &[".file-item.active", "textarea", ".list-item", "input", "button"];

/// Give the focus to the next (or previous) region of the window: the pages, each
/// editor pane, the side panel.
pub fn cycle_region(forward: bool) {
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else { return };
    let Ok(list) = doc.query_selector_all(REGIONS) else { return };
    let regions: Vec<Element> = (0..list.length())
        .filter_map(|i| list.item(i)?.dyn_into::<Element>().ok())
        .collect();
    if regions.is_empty() { return; }
    let active = doc.active_element();
    let current = active.and_then(|a| regions.iter().position(|r| r.contains(Some(&a))));
    let n = regions.len();
    let next = match current {
        Some(i) if forward => (i + 1) % n,
        Some(i) => (i + n - 1) % n,
        None if forward => 0,
        None => n - 1,
    };
    let region = &regions[next];
    let target = ENTRY.iter()
        .find_map(|sel| region.query_selector(sel).ok().flatten())
        .and_then(|el| el.dyn_into::<HtmlElement>().ok());
    if let Some(el) = target { let _ = el.focus(); }
}

/// Keys on an item of a list (pages, backlinks, tags…): Enter or Space opens it
/// like a click, the arrows go to the item above or below.
pub fn list_item_keys(e: KeyboardEvent) {
    if e.ctrl_key() || e.alt_key() || e.meta_key() { return; }
    let Some(item) = e.current_target().and_then(|t| t.dyn_into::<HtmlElement>().ok()) else { return };
    // Keys typed in a field inside the item (renaming a page) are its own
    if e.target().and_then(|t| t.dyn_into::<HtmlElement>().ok()).is_some_and(|t| t != item) { return; }
    let sibling = |down: bool| {
        let mut el = if down { item.next_element_sibling() } else { item.previous_element_sibling() };
        while let Some(s) = el {
            if s.class_list().contains("list-item") { return s.dyn_into::<HtmlElement>().ok(); }
            el = if down { s.next_element_sibling() } else { s.previous_element_sibling() };
        }
        None
    };
    match e.key().as_str() {
        "Enter" | " " => { e.prevent_default(); item.click(); }
        "ArrowDown" => if let Some(s) = sibling(true) { e.prevent_default(); let _ = s.focus(); },
        "ArrowUp" => if let Some(s) = sibling(false) { e.prevent_default(); let _ = s.focus(); },
        _ => {}
    }
}
