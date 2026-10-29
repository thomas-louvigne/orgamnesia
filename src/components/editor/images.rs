//! Images in the editor: their sizes, added with the button or dropped, resized
//! by their corner.

use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

use super::{
    dom::{after_tick, caret, set_cursor, splice, Textarea},
    pane::Pane,
};
use crate::{images, invoke};

/// Natural size of an image, by `image_src`; `None` when it could not be loaded.
pub type ImageSizes = std::collections::HashMap<String, Option<(f64, f64)>>;

thread_local! {
    /// `blob:` URL of each file image read so far, by its path.
    static BLOBS: std::cell::RefCell<std::collections::HashMap<String, String>> = Default::default();
}

/// The image a link `target` of the page `page` points to: the path of the file, or
/// the address of an image on the web.
pub fn image_src(page: &str, target: &str) -> String {
    if target.contains("://") { target.to_string() } else { images::resolve(page, target) }
}

/// URL the page shows the image `src` (an `image_src`) from: file images are read by
/// the backend and shown from a `blob:` URL, as the webview may refuse to load them
/// from Tauri's asset protocol (WebKitGTK, when the page comes from `http://localhost`).
pub fn image_url(src: &str) -> String {
    BLOBS.with(|b| b.borrow().get(src).cloned()).unwrap_or_else(|| src.to_string())
}

/// URL the image `src` (an `image_src`) is shown from, its file read first when
/// it hasn't been yet (for a page exported as a PDF).
pub async fn loaded_image_url(src: &str) -> String {
    if !src.contains("://") && BLOBS.with(|b| !b.borrow().contains_key(src))
        && let Some(url) = invoke::read_image(src).await.ok().and_then(|bytes| blob_url(src, &bytes))
    {
        BLOBS.with(|b| { b.borrow_mut().insert(src.to_string(), url); });
    }
    image_url(src)
}

/// Load the image `src` (an `image_src`) to learn its natural size, into `sizes`.
pub fn load_size(src: &str, sizes: RwSignal<ImageSizes>) {
    let src = src.to_string();
    spawn_local(async move {
        if !src.contains("://") && BLOBS.with(|b| !b.borrow().contains_key(&src)) {
            match invoke::read_image(&src).await.ok().and_then(|bytes| blob_url(&src, &bytes)) {
                Some(url) => BLOBS.with(|b| { b.borrow_mut().insert(src.clone(), url); }),
                None => {
                    let _ = sizes.try_update(|s| { s.insert(src, None); });
                    return;
                }
            }
        }
        let Ok(img) = web_sys::HtmlImageElement::new() else { return };
        let (key, el) = (src.clone(), img.clone());
        let done = wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
            let size = (e.type_() == "load" && el.natural_width() > 0)
                .then(|| (el.natural_width() as f64, el.natural_height() as f64));
            let _ = sizes.try_update(|s| { s.insert(key.clone(), size); });
        });
        img.set_onload(Some(done.as_ref().unchecked_ref()));
        img.set_onerror(Some(done.as_ref().unchecked_ref()));
        done.forget();
        img.set_src(&image_url(&src));
    });
}

/// A `blob:` URL holding the image file `path` read as `bytes`.
fn blob_url(path: &str, bytes: &js_sys::Uint8Array) -> Option<String> {
    let parts = js_sys::Array::of1(bytes);
    let options = web_sys::BlobPropertyBag::new();
    // An SVG is only drawn with its type; the others are recognized by their content
    if path.to_ascii_lowercase().ends_with(".svg") { options.set_type("image/svg+xml"); }
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options).ok()?;
    web_sys::Url::create_object_url_with_blob(&blob).ok()
}

/// Distance from the bottom right corner of an image within which it is resized.
const HANDLE: f64 = 14.0;

/// The image drawn in `wrap` under the point `x`, `y` (client px): its box, page
/// position (see `.img-box`), and whether the point is on its resize corner.
fn image_at(wrap: &web_sys::Element, x: f64, y: f64) -> Option<(web_sys::HtmlElement, usize, bool)> {
    let boxes = wrap.query_selector_all(".img-box").ok()?;
    (0..boxes.length()).filter_map(|i| boxes.item(i)?.dyn_into::<web_sys::HtmlElement>().ok()).find_map(|b| {
        let r = b.get_bounding_client_rect();
        let inside = x >= r.left() && x <= r.right() + 2.0 && y >= r.top() && y <= r.bottom() + 2.0;
        let corner = x >= r.right() - HANDLE && y >= r.bottom() - HANDLE;
        let at = b.get_attribute("data-at")?.parse().ok()?;
        inside.then_some((b, at, corner))
    })
}

/// Size of the red cross of Logseq's `{{video url}}`, at the bottom left of its thumbnail.
const CROSS: f64 = 26.0;

/// The point `x`, `y` is on the red cross of the thumbnail box `frame`, if it has one.
fn on_cross(frame: &web_sys::HtmlElement, x: f64, y: f64) -> bool {
    if !frame.has_attribute("data-macro") { return false; }
    let r = frame.get_bounding_client_rect();
    x <= r.left() + CROSS && y >= r.bottom() - CROSS
}

/// An image being resized with the mouse.
#[derive(Clone)]
struct ImageDrag {
    /// Its box (`.img-box`) and picture.
    frame: web_sys::HtmlElement,
    img: web_sys::HtmlElement,
    /// Page position of its link.
    at: usize,
    /// Pointer x and image width when the drag started, the width it may reach.
    x0: f64,
    w0: f64,
    max: f64,
    /// Height / width.
    ratio: f64,
}

/// The images of an editor pane.
#[derive(Clone, Copy)]
pub struct Images {
    pane: Pane,
    /// The editor wrapper, where the images are drawn.
    wrap: NodeRef<leptos::html::Div>,
    /// The page, whose folder image links start from.
    page: StoredValue<String>,
    /// The textarea gets the focus from the mouse: no caret ping (see `Editor`).
    mouse_focus: StoredValue<bool>,
    drag: StoredValue<Option<ImageDrag>>,
    hovered: StoredValue<Option<web_sys::HtmlElement>>,
    /// The pointer is over the red cross of a `{{video url}}` macro.
    on_cross: StoredValue<bool>,
}

impl Images {
    /// Also listens to the window: files dropped on the pane, drags of an image's corner.
    pub fn new(pane: Pane, wrap: NodeRef<leptos::html::Div>, page: String, mouse_focus: StoredValue<bool>) -> Self {
        let this = Self {
            pane,
            wrap,
            page: StoredValue::new(page),
            mouse_focus,
            drag: StoredValue::new(None),
            hovered: StoredValue::new(None),
            on_cross: StoredValue::new(false),
        };
        this.watch_drops();
        let drag = this.drag;
        let drag_move = window_event_listener(leptos::ev::mousemove, move |e: web_sys::MouseEvent| {
            let Some(d) = drag.get_value() else { return };
            let w = (d.w0 + e.client_x() as f64 - d.x0).clamp(24.0, d.max);
            let _ = d.img.style().set_property("width", &format!("{w}px"));
            let _ = d.img.style().set_property("height", &format!("{}px", w * d.ratio));
        });
        let drag_up = window_event_listener(leptos::ev::mouseup, move |_| {
            let Some(d) = drag.get_value() else { return };
            drag.set_value(None);
            d.frame.class_list().remove_1("resizing").ok();
            let r = d.img.get_bounding_client_rect();
            if (r.width() - d.w0).abs() >= 1.0 { this.resize(d.at, r.width(), r.width() * d.ratio); }
        });
        on_cleanup(move || { drag_move.remove(); drag_up.remove(); });
        this
    }

    /// Put links to images on lines of their own, after the caret's line (or in it
    /// when empty); the caret goes to the line after them, so they are drawn.
    fn insert_links(self, el: &Textarea, links: &[String]) {
        let chars: Vec<char> = el.value().chars().collect();
        let p = caret(el).min(chars.len());
        let ls = chars[..p].iter().rposition(|&c| c == '\n').map_or(0, |i| i + 1);
        let le = chars[p..].iter().position(|&c| c == '\n').map_or(chars.len(), |i| p + i);
        let empty = chars[ls..le].iter().all(|c| c.is_whitespace());
        let block = links.join("\n");
        let last = if le == chars.len() { "\n" } else { "" };
        let (from, text) = if empty { (ls, format!("{block}{last}")) } else { (le, format!("\n{block}{last}")) };
        let end = splice(el, from, le, &text);
        set_cursor(el, if last.is_empty() { end + 1 } else { end });
        self.pane.track_caret(el);
    }

    /// Copy the image files into the project and link them from the page.
    fn add(self, paths: Vec<String>) {
        let ctx = self.pane.ctx;
        let page = self.page.get_value();
        spawn_local(async move {
            let mut links = Vec::new();
            for path in paths {
                match invoke::import_image(&page, &path).await {
                    Ok(link) => links.push(format!("[[{link}]]")),
                    Err(e) => ctx.error("image_error", &e),
                }
            }
            let Some(el) = self.pane.area.try_get_untracked().flatten() else { return };
            if links.is_empty() { return; }
            let _ = el.focus();
            self.insert_links(&el, &links);
        });
    }

    /// Copy other files into the project and link them from the page, by their name.
    fn add_files(self, paths: Vec<String>) {
        let ctx = self.pane.ctx;
        let page = self.page.get_value();
        spawn_local(async move {
            let mut links = Vec::new();
            for path in paths {
                let name = path.rsplit(['/', '\\']).next().unwrap_or(&path).replace(['[', ']'], "");
                match invoke::import_file(&page, &path).await {
                    Ok(link) => links.push(format!("[[{link}][{name}]]")),
                    Err(e) => ctx.error("file_import_error", &e),
                }
            }
            let Some(el) = self.pane.area.try_get_untracked().flatten() else { return };
            if links.is_empty() { return; }
            let _ = el.focus();
            self.insert_links(&el, &links);
        });
    }

    /// Ask for a file, whatever it is, and add it: an image is drawn, another
    /// file linked by its name.
    pub fn pick_file(self) {
        let ctx = self.pane.ctx;
        spawn_local(async move {
            match invoke::pick_file(None).await {
                Ok(Some(path)) if images::is_image(&path) => self.add(vec![path]),
                Ok(Some(path)) => self.add_files(vec![path]),
                Ok(None) => {}
                Err(e) => ctx.error("file_import_error", &e),
            }
        });
    }

    /// Ask for an image file and add it.
    pub fn pick(self) {
        let ctx = self.pane.ctx;
        spawn_local(async move {
            match invoke::pick_image().await {
                Ok(Some(path)) => self.add(vec![path]),
                Ok(None) => {}
                Err(e) => ctx.error("image_error", &e),
            }
        });
    }

    /// Files dropped on this editor (the window reports every drop, see `app`).
    fn watch_drops(self) {
        let ctx = self.pane.ctx;
        let seen = StoredValue::new(ctx.ui.dropped.with_untracked(|d| d.as_ref().map(|(n, _)| *n)));
        Effect::new(move |_| {
            let Some((n, drop)) = ctx.ui.dropped.get() else { return };
            if seen.get_value() == Some(n) { return; }
            seen.set_value(Some(n));
            let Some(wrap) = self.wrap.get_untracked() else { return };
            let (x, y) = drop.client_pos();
            let r = wrap.get_bounding_client_rect();
            if x < r.left() || x > r.right() || y < r.top() || y > r.bottom() { return; }
            let (pictures, others): (Vec<String>, Vec<String>) = drop.paths.into_iter().partition(|p| images::is_image(p));
            if !pictures.is_empty() { self.add(pictures); }
            if !others.is_empty() { self.add_files(others); }
        });
    }

    /// Replace Logseq's `{{video url}}` of the line at page position `at` with `[[url]]`.
    fn macro_to_link(self, at: usize) {
        let pane = self.pane;
        let Some(el) = pane.area.get_untracked() else { return };
        let Some((from, to, text)) = pane.content.with_untracked(|c| images::macro_to_link(c, at)) else { return };
        let before = pane.focus_pos.get_untracked();
        let (a, b) = (pane.view_pos(from), pane.view_pos(to));
        let mouse_focus = self.mouse_focus;
        mouse_focus.set_value(true);
        let _ = el.focus();
        after_tick(move || mouse_focus.set_value(false));
        splice(&el, a, b, &text);
        // The caret back where it was, out of the video's line: the thumbnail is drawn again
        let grown = text.chars().count() as isize - (to - from) as isize;
        let back = before.map_or(0, |p| if p >= to { (p as isize + grown) as usize } else { p.min(from) });
        pane.focus_pos.set(Some(back));
        pane.ctx.notify_t("video_macro_fixed", "");
    }

    /// Give the image of the link at page position `at` the size `w` x `h`, in the file.
    fn resize(self, at: usize, w: f64, h: f64) {
        let pane = self.pane;
        let Some(el) = pane.area.get_untracked() else { return };
        let org = pane.ctx.pref_untracked(|p| p.image_size_format != "logseq");
        let Some((from, to, text)) = pane.content.with_untracked(|c| images::resize(c, at, w, h, org)) else { return };
        let before = pane.focus_pos.get_untracked();
        let (a, b) = (pane.view_pos(from), pane.view_pos(to));
        let mouse_focus = self.mouse_focus;
        mouse_focus.set_value(true);
        let _ = el.focus();
        after_tick(move || mouse_focus.set_value(false));
        splice(&el, a, b, &text);
        // The caret back where it was, out of the image's lines: the image is drawn again
        let grown = text.chars().count() as isize - (to - from) as isize;
        let back = before.map_or(0, |p| if p >= to { (p as isize + grown) as usize } else { p.min(from) });
        pane.focus_pos.set(Some(back));
    }

    /// The pointer over an image shows its corner; on the corner, it resizes it.
    pub fn on_hover(self, e: &web_sys::MouseEvent) {
        if self.drag.with_value(Option::is_some) { return; }
        let Some(wrap) = self.wrap.get_untracked() else { return };
        let hit = image_at(&wrap, e.client_x() as f64, e.client_y() as f64);
        if let Some(old) = self.hovered.get_value() && hit.as_ref().is_none_or(|(b, ..)| b != &old) {
            old.class_list().remove_1("hover").ok();
        }
        if let Some((b, ..)) = &hit { b.class_list().add_1("hover").ok(); }
        let corner = hit.as_ref().is_some_and(|&(_, _, c)| c);
        let video = hit.as_ref().is_some_and(|(b, ..)| b.has_attribute("data-video"));
        let cross = hit.as_ref().is_some_and(|(b, ..)| on_cross(b, e.client_x() as f64, e.client_y() as f64));
        // Over the red cross, what it is about and what a click does
        if cross && !self.on_cross.get_value() {
            self.pane.ctx.notify_t("video_macro_warning", "");
        }
        self.on_cross.set_value(cross);
        self.hovered.set_value(hit.map(|(b, ..)| b));
        if let Some(el) = self.pane.area.get_untracked() {
            let cursor = if corner { "nwse-resize" } else if video { "pointer" } else { "" };
            let _ = web_sys::HtmlElement::style(&el).set_property("cursor", cursor);
        }
    }

    /// A press on a video's thumbnail, off its corner, opens the video in the
    /// browser; on the red cross of Logseq's `{{video url}}`, the macro becomes the
    /// link `[[url]]`. True when it did.
    pub fn open_video(self, e: &web_sys::MouseEvent) -> bool {
        let Some(wrap) = self.wrap.get_untracked() else { return false };
        let (x, y) = (e.client_x() as f64, e.client_y() as f64);
        let Some((frame, at, false)) = image_at(&wrap, x, y) else { return false };
        let Some(url) = frame.get_attribute("data-video") else { return false };
        e.prevent_default();
        if on_cross(&frame, x, y) {
            self.macro_to_link(at);
            return true;
        }
        let ctx = self.pane.ctx;
        spawn_local(async move {
            if let Err(err) = invoke::open_url(&url).await { ctx.error("error", &err); }
        });
        true
    }

    /// A press on an image's corner starts resizing it; true when it did.
    pub fn start_drag(self, e: &web_sys::MouseEvent) -> bool {
        let Some(wrap) = self.wrap.get_untracked() else { return false };
        let Some((frame, at, true)) = image_at(&wrap, e.client_x() as f64, e.client_y() as f64) else { return false };
        let Some(img) = frame.first_element_child().and_then(|i| i.dyn_into::<web_sys::HtmlElement>().ok()) else { return false };
        let Some(area) = self.pane.area.get_untracked() else { return false };
        e.prevent_default();
        let r = img.get_bounding_client_rect();
        let right = area.get_bounding_client_rect().right() - 24.0;
        frame.class_list().add_1("resizing").ok();
        self.drag.set_value(Some(ImageDrag {
            frame, img, at,
            x0: e.client_x() as f64,
            w0: r.width(),
            max: (right - r.left()).max(24.0),
            ratio: if r.width() > 0.0 { r.height() / r.width() } else { 1.0 },
        }));
        true
    }
}
