//! A page exported: its org text made into HTML (title, headlines, lists,
//! tables, blocks, images, links), then
//! - as a PDF: put in the window and the system's print dialog opened, where the
//!   page is saved as a PDF. On screen that HTML stays hidden: only printing
//!   shows it, in place of the app (`@media print`);
//! - as an HTML file holding everything (its style, its images), saved where
//!   the user chooses.

use std::collections::HashMap;

use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::{spawn_local, JsFuture};

use crate::{actions, components::editor::loaded_image_url, html::escape, images, invoke, links, state::{AppCtx, FileEntry, Tab}};
use convert::{page_html, page_title};
use orgamnesia_core::TodoKeywords;

mod convert;

/// Id of the element holding the page to print.
const PRINT_ROOT: &str = "print-root";

/// The look of an exported page (inside `.org-page`), printed or in its file.
const STYLE: &str = "\
.org-page { max-width: 46em; margin: 0 auto; padding: 1em; color: #222; background: #fff;
  font: 11pt/1.5 'Iosevka Aile', 'Inter', 'Segoe UI', 'Helvetica Neue', Arial, sans-serif; }
.org-page h1, .org-page h2, .org-page h3, .org-page h4, .org-page h5, .org-page h6 { line-height: 1.25; margin: 1.2em 0 .4em; break-after: avoid; }
.org-page h1.title { font-size: 2em; margin-top: 0; }
.org-page .todo { color: #b3261e; font-size: .8em; }
.org-page .done { color: #2e7d32; font-size: .8em; }
.org-page .tag { font-size: .7em; font-weight: normal; color: #666; border: 1px solid #ccc; border-radius: 3px; padding: 0 .3em; margin-left: .3em; }
.org-page .link { color: #1a5fb4; }
.org-page a { color: #1a5fb4; }
.org-page pre { background: #f4f4f4; border: 1px solid #ddd; border-radius: 4px; padding: .6em .8em; white-space: pre-wrap; font-size: .9em; }
.org-page code { background: #f4f4f4; border-radius: 3px; padding: 0 .2em; font-size: .9em; }
.org-page pre, .org-page code { font-family: 'Fira Code', 'DejaVu Sans Mono', Consolas, monospace; }
.org-page blockquote { margin: 1em 0; padding-left: 1em; border-left: 3px solid #ccc; color: #444; }
.org-page .center { text-align: center; }
.org-page .verse { white-space: pre-wrap; }
.org-page table { border-collapse: collapse; margin: 1em 0; }
.org-page th, .org-page td { border: 1px solid #bbb; padding: .25em .6em; text-align: left; }
.org-page th { background: #eee; }
.org-page img { max-width: 100%; }
.org-page .image { text-align: center; }
.org-page hr { border: none; border-top: 1px solid #ccc; }
.org-page li { margin: .15em 0; }
.org-page pre, .org-page table, .org-page img { break-inside: avoid; }
";

/// The page to export: its file and its text.
struct Page { path: String, name: String, text: String }

impl Page {
    fn of(tab: &Tab) -> Self {
        Self { path: tab.path.clone(), name: tab.name.clone(), text: tab.content.get_untracked() }
    }
}

/// The HTML of `page`, inside `.org-page`; `image` gives the URL each image
/// link target is shown from (read first).
async fn page_body(ctx: AppCtx, page: &Page, image: impl AsyncFn(&str) -> String) -> String {
    let text = &page.text;
    let mut urls = HashMap::new();
    for target in links::targets(text).into_iter().filter(|t| images::is_image(t)) {
        let url = image(&target).await;
        urls.insert(target, url);
    }
    let keywords = ctx.pref_untracked(|p| p.todo()).map(|d| TodoKeywords::for_page(text, &d));
    let url = |t: &str| urls.get(t).cloned().unwrap_or_else(|| t.to_string());
    format!("<div class='org-page'>{}</div>", page_html(text, &page.name, keywords.as_ref(), &url))
}

/// Export the page `file` (as HTML or as PDF), from the list of pages: the text
/// of its tab when it is open (saved or not), else the file's.
pub fn export_file(ctx: AppCtx, file: FileEntry, html: bool) {
    let open = ctx.work.tabs.with_untracked(|tabs| tabs.iter().find(|t| t.path == file.path).map(Page::of));
    spawn_local(async move {
        let page = match open {
            Some(page) => page,
            None => match actions::read_page(&file.path).await {
                Ok(text) => Page { path: file.path, name: file.name, text },
                Err(e) => return ctx.error("error", &e),
            },
        };
        if html { html_file(ctx, page).await } else { print(ctx, page).await }
    });
}

/// Export the page of `tab` as an HTML file holding its style and images.
pub fn export_html(ctx: AppCtx, tab: Tab) {
    let page = Page::of(&tab);
    spawn_local(html_file(ctx, page));
}

async fn html_file(ctx: AppCtx, page: Page) {
    // Local images go in the file
    let path = page.path.clone();
    let body = page_body(ctx, &page, async |target: &str| {
        if target.contains("://") { return target.to_string(); }
        match invoke::read_image(&images::resolve(&path, target)).await {
            Ok(bytes) => data_url(target, &bytes.to_vec()),
            Err(_) => target.to_string(),
        }
    }).await;
    let title = page_title(&page.text).unwrap_or_else(|| page.name.clone());
    let html = format!(
        "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n\
        <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
        <title>{}</title>\n<style>\nbody {{ margin: 0; background: #fff; }}\n{STYLE}</style>\n</head>\n\
        <body>\n{body}\n</body>\n</html>\n",
        escape(&title));
    match invoke::save_html(&page.name, &html).await {
        Ok(Some(path)) => ctx.notify_t("html_exported", &path),
        Ok(None) => {}
        Err(e) => ctx.error("html_export_error", &e),
    }
}

/// A `data:` URL holding the image file `name` read as `bytes`.
fn data_url(name: &str, bytes: &[u8]) -> String {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    let mime = match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "svg" => "image/svg+xml",
        "" => "application/octet-stream",
        e => return format!("data:image/{e};base64,{}", base64(bytes)),
    };
    format!("data:{mime};base64,{}", base64(bytes))
}

fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            out.push(if i <= chunk.len() { ABC[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    out
}

/// Export the page of `tab` as a PDF, from the print dialog.
pub fn export_pdf(ctx: AppCtx, tab: Tab) {
    let page = Page::of(&tab);
    spawn_local(print(ctx, page));
}

async fn print(ctx: AppCtx, page: Page) {
    // Local images are shown from `blob:` URLs
    let path = page.path.clone();
    let body = page_body(ctx, &page, async |target: &str| {
        let src = if target.contains("://") { target.to_string() } else { images::resolve(&path, target) };
        loaded_image_url(&src).await
    }).await;
    let html = format!("<style>{STYLE}</style>{body}");
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else { return };
    let Some(body) = doc.body() else { return };
    // Kept after printing (hidden): the dialog may draw the page once it is closed
    if let Some(old) = doc.get_element_by_id(PRINT_ROOT) { old.remove(); }
    let Ok(root) = doc.create_element("div") else { return };
    root.set_id(PRINT_ROOT);
    root.set_inner_html(&html);
    let _ = body.append_child(&root);
    // The images drawn before the page is
    if let Ok(imgs) = root.query_selector_all("img") {
        for i in 0..imgs.length() {
            if let Some(img) = imgs.item(i).and_then(|n| n.dyn_into::<web_sys::HtmlImageElement>().ok()) {
                let _ = JsFuture::from(img.decode()).await;
            }
        }
    }
    // The name the PDF is offered under
    doc.set_title(&page_title(&page.text).unwrap_or_else(|| page.name.clone()));
    let restore = wasm_bindgen::closure::Closure::once_into_js(move || {
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) { doc.set_title("Orgamnesia"); }
    });
    if let Some(w) = web_sys::window() {
        let _ = w.add_event_listener_with_callback("afterprint", restore.unchecked_ref());
    }
    if let Err(e) = invoke::print_window().await { ctx.error("pdf_export_error", &e); }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn images_in_the_file() {
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
        assert_eq!(data_url("a.JPG", b"M"), "data:image/jpeg;base64,TQ==");
        assert_eq!(data_url("a.png", b""), "data:image/png;base64,");
    }
}
