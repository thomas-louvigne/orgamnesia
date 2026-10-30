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

use crate::{actions, components::editor::loaded_image_url, images, invoke, links, state::{AppCtx, FileEntry, Tab}};
use orgamnesia_core::{links as place, names::fold, TodoKeywords};

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
        urls.insert(target.to_string(), image(target).await);
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

/// The `#+TITLE:` of the page.
fn page_title(text: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let l = l.trim();
        l.get(..8).filter(|k| k.eq_ignore_ascii_case("#+title:")).map(|_| l[8..].trim().to_string())
    }).filter(|t| !t.is_empty())
}

/// The HTML of the page `text` called `name`. `image` gives the URL an image
/// link target is shown from.
pub fn page_html(text: &str, name: &str, keywords: Option<&TodoKeywords>, image: &dyn Fn(&str) -> String) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    // The title: the `#+TITLE:`, else the name, unless the page starts with a
    // headline holding it (as a new page does)
    let first_headline = lines.iter().find(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
        .and_then(|l| place::headline_title(l, keywords));
    let title = match page_title(text) {
        Some(t) => Some(t),
        None if first_headline.is_some_and(|h| fold(&h) == fold(name)) => None,
        None => Some(name.to_string()),
    };
    let doc = Doc { keywords, image, shift: usize::from(title.is_some()) };
    let mut out = String::new();
    if let Some(t) = title { out.push_str(&format!("<h1 class='title'>{}</h1>", escape(&t))); }
    out.push_str(&doc.body(&lines));
    out
}

/// What the HTML of a page is made with.
struct Doc<'a> {
    keywords: Option<&'a TodoKeywords>,
    image: &'a dyn Fn(&str) -> String,
    /// Levels the headlines go down by (one below the title, when there is one).
    shift: usize,
}

impl Doc<'_> {
    /// The HTML of `lines`: a page, or the inside of a block.
    fn body(&self, lines: &[&str]) -> String {
        let mut out = String::new();
        let mut para: Vec<&str> = Vec::new();
        // The width `#+ATTR_ORG: :width` gives the next image
        let mut width: Option<String> = None;
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];
            let t = line.trim();
            if t.is_empty() || is_structure(line) { self.paragraph(&mut out, &mut para); }
            if t.is_empty() { i += 1; continue; }
            // A block: `#+BEGIN_SRC` … `#+END_SRC`
            if let Some(kind) = block_kind(t)
                && let Some(end) = lines[i + 1..].iter().position(|l| l.trim().eq_ignore_ascii_case(&format!("#+end_{kind}")))
            {
                out.push_str(&self.block(&kind, &lines[i + 1..i + 1 + end]));
                i += end + 2;
                continue;
            }
            let lower = t.to_ascii_lowercase();
            if lower.starts_with("#+attr_org:") || lower.starts_with("#+attr_html:") {
                width = attr_width(t);
                i += 1;
                continue;
            }
            // Keywords and comments are not shown
            if t.starts_with("#+") || t == "#" || t.starts_with("# ") { i += 1; continue; }
            // A drawer (`:PROPERTIES:` … `:END:`)
            if is_drawer(t) && let Some(end) = lines[i + 1..].iter().position(|l| l.trim().eq_ignore_ascii_case(":end:")) {
                i += end + 2;
                continue;
            }
            if let Some(level) = headline_level(line) {
                out.push_str(&self.headline(line, level));
                i += 1;
                continue;
            }
            if t.starts_with('|') {
                let n = lines[i..].iter().take_while(|l| l.trim().starts_with('|')).count();
                out.push_str(&self.table(&lines[i..i + n]));
                i += n;
                continue;
            }
            if t == ":" || t.starts_with(": ") {
                let n = lines[i..].iter().take_while(|l| { let t = l.trim(); t == ":" || t.starts_with(": ") }).count();
                let text: Vec<&str> = lines[i..i + n].iter().map(|l| { let t = l.trim(); t.get(2..).unwrap_or("") }).collect();
                out.push_str(&format!("<pre>{}</pre>", escape(&text.join("\n"))));
                i += n;
                continue;
            }
            if t.len() >= 5 && t.chars().all(|c| c == '-') {
                out.push_str("<hr>");
                i += 1;
                continue;
            }
            if list_item(line).is_some() {
                let n = list_lines(&lines[i..]);
                out.push_str(&self.list(&lines[i..i + n]));
                i += n;
                continue;
            }
            if let Some((target, w)) = image_line(t) {
                out.push_str(&self.image(target, w.or(width.take())));
                i += 1;
                continue;
            }
            para.push(t);
            i += 1;
        }
        self.paragraph(&mut out, &mut para);
        out
    }

    /// The lines of text gathered so far, as a paragraph.
    fn paragraph(&self, out: &mut String, para: &mut Vec<&str>) {
        if para.is_empty() { return; }
        let html: Vec<String> = para.iter().map(|l| match l.strip_suffix("\\\\") {
            Some(l) => format!("{}<br>", self.inline(l.trim_end())),
            None => self.inline(l),
        }).collect();
        out.push_str(&format!("<p>{}</p>", html.join("\n")));
        para.clear();
    }

    fn headline(&self, line: &str, level: usize) -> String {
        let chars: Vec<char> = line.chars().collect();
        let mut rest = chars[level..].iter().collect::<String>().trim().to_string();
        let mut html = String::new();
        if let Some(k) = self.keywords.and_then(|k| k.keyword(&chars)) {
            let word: String = chars[k.start..k.end].iter().collect();
            html.push_str(&format!("<span class='{}'>{}</span> ", if k.done { "done" } else { "todo" }, escape(&word)));
            rest = chars[k.end..].iter().collect::<String>().trim().to_string();
        }
        let mut tags = String::new();
        if let Some((before, last)) = rest.rsplit_once(' ')
            && last.len() > 2 && last.starts_with(':') && last.ends_with(':') && !last.contains("::")
        {
            tags = last.trim_matches(':').split(':').map(|t| format!("<span class='tag'>{}</span>", escape(t))).collect();
            rest = before.trim_end().to_string();
        }
        let h = (level + self.shift).min(6);
        format!("<h{h}>{html}{}{}</h{h}>", self.inline(&rest), if tags.is_empty() { tags } else { format!(" {tags}") })
    }

    fn block(&self, kind: &str, lines: &[&str]) -> String {
        let text = || escape(&lines.join("\n"));
        match kind {
            "src" | "example" => format!("<pre>{}</pre>", text()),
            "comment" | "export" => String::new(),
            "quote" => format!("<blockquote>{}</blockquote>", self.body(lines)),
            "center" => format!("<div class='center'>{}</div>", self.body(lines)),
            "verse" => format!("<p class='verse'>{}</p>", lines.iter().map(|l| self.inline(l)).collect::<Vec<_>>().join("<br>")),
            _ => format!("<div class='{}'>{}</div>", escape(kind), self.body(lines)),
        }
    }

    /// Rows above the first `|---|` line are headers.
    fn table(&self, lines: &[&str]) -> String {
        let is_rule = |l: &str| l.trim().starts_with("|-");
        let head = lines.iter().position(|l| is_rule(l)).filter(|&n| n > 0 && n < lines.len() - 1).unwrap_or(0);
        let mut out = String::from("<table>");
        for (n, line) in lines.iter().enumerate().filter(|(_, l)| !is_rule(l)) {
            let t = line.trim();
            let t = t.strip_prefix('|').unwrap_or(t);
            let t = t.strip_suffix('|').unwrap_or(t);
            let cell = if n < head { "th" } else { "td" };
            out.push_str("<tr>");
            for c in t.split('|') { out.push_str(&format!("<{cell}>{}</{cell}>", self.inline(c.trim()))); }
            out.push_str("</tr>");
        }
        out.push_str("</table>");
        out
    }

    /// Nested lists: an item deeper than the one before starts a list in it.
    fn list(&self, lines: &[&str]) -> String {
        // The items, with the lines continuing them
        let mut items: Vec<(usize, bool, String)> = Vec::new();
        for line in lines {
            match list_item(line) {
                Some((indent, ordered, text)) => items.push((indent, ordered, text.to_string())),
                None => if let Some(last) = items.last_mut() {
                    last.2.push(' ');
                    last.2.push_str(line.trim());
                },
            }
        }
        let tag = |ordered: bool| if ordered { "ol" } else { "ul" };
        let mut out = String::new();
        let mut open: Vec<(usize, bool)> = Vec::new();
        for (indent, ordered, text) in &items {
            while open.last().is_some_and(|&(i, _)| *indent < i) {
                let (_, o) = open.pop().unwrap_or_default();
                out.push_str(&format!("</li></{}>", tag(o)));
            }
            // Numbered after bulleted (or the reverse): another list
            if open.last().is_some_and(|&(i, o)| i == *indent && o != *ordered) {
                open.pop();
                out.push_str(&format!("</li></{}>", tag(!*ordered)));
            }
            if open.last().is_some_and(|&(i, _)| i == *indent) {
                out.push_str("</li>");
            } else {
                open.push((*indent, *ordered));
                out.push_str(&format!("<{}>", tag(*ordered)));
            }
            out.push_str(&format!("<li>{}", self.item(text.trim())));
        }
        while let Some((_, o)) = open.pop() { out.push_str(&format!("</li></{}>", tag(o))); }
        out
    }

    /// An item's text: its checkbox, its `term ::` and the rest.
    fn item(&self, text: &str) -> String {
        let (check, text) = match text.get(..4) {
            Some("[ ] ") => ("☐ ", &text[4..]),
            Some("[X] " | "[x] ") => ("☑ ", &text[4..]),
            Some("[-] ") => ("◩ ", &text[4..]),
            _ => ("", text),
        };
        match text.split_once(" :: ") {
            Some((term, rest)) => format!("{check}<b>{}</b> : {}", self.inline(term), self.inline(rest)),
            None => format!("{check}{}", self.inline(text)),
        }
    }

    /// An image alone on its line.
    fn image(&self, target: &str, width: Option<String>) -> String {
        format!("<p class='image'>{}</p>", self.img(target, width))
    }

    fn img(&self, target: &str, width: Option<String>) -> String {
        let width = width.map(|w| format!(" style='width: {}'", escape(&w))).unwrap_or_default();
        format!("<img src='{}' alt=''{width}>", escape(&(self.image)(target)))
    }

    /// Inline markup: links, emphasis, verbatim, addresses.
    fn inline(&self, s: &str) -> String {
        let chars: Vec<char> = s.chars().collect();
        let mut out = String::new();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if c == '[' && chars.get(i + 1) == Some(&'[') && let Some((target, desc, end)) = link_at(&chars, i) {
                out.push_str(&self.link(&target, desc.as_deref()));
                i = end;
                continue;
            }
            // `<<cible>>` is not shown, `<<<radio>>>` shows its text
            if c == '<' && chars.get(i + 1) == Some(&'<') && let Some(end) = (i + 2..chars.len()).find(|&j| chars[j] == '>' && chars.get(j + 1) == Some(&'>')) {
                let inner: String = chars[i + 2..end].iter().collect();
                if let Some(radio) = inner.strip_prefix('<') { out.push_str(&escape(radio)); }
                i = end + 2;
                while chars.get(i) == Some(&'>') { i += 1; }
                continue;
            }
            if "*/_+=~".contains(c) && let Some(end) = emphasis_end(&chars, i) {
                let inner: String = chars[i + 1..end].iter().collect();
                out.push_str(&match c {
                    '*' => format!("<b>{}</b>", self.inline(&inner)),
                    '/' => format!("<i>{}</i>", self.inline(&inner)),
                    '_' => format!("<u>{}</u>", self.inline(&inner)),
                    '+' => format!("<s>{}</s>", self.inline(&inner)),
                    _ => format!("<code>{}</code>", escape(&inner)),
                });
                i = end + 1;
                continue;
            }
            // An address written as is
            if (i == 0 || chars[i - 1].is_whitespace()) && (starts_with(&chars[i..], "https://") || starts_with(&chars[i..], "http://")) {
                let end = (i..chars.len()).find(|&j| chars[j].is_whitespace()).unwrap_or(chars.len());
                let url: String = chars[i..end].iter().collect();
                out.push_str(&format!("<a href='{0}'>{0}</a>", escape(&url)));
                i = end;
                continue;
            }
            out.push_str(&escape(&c.to_string()));
            i += 1;
        }
        out
    }

    fn link(&self, target: &str, desc: Option<&str>) -> String {
        let target = target.trim();
        if desc.is_none() && images::is_image(target) { return self.img(target, None); }
        let shown = |default: String| desc.map_or_else(|| escape(&default), |d| self.inline(d));
        if target.contains("://") || target.starts_with("mailto:") {
            return format!("<a href='{}'>{}</a>", escape(target), shown(target.to_string()));
        }
        if links::file_target(target).is_some() {
            return format!("<span class='link'>{}</span>", shown(target.to_string()));
        }
        // A page, a place in one: `Page › Titre`
        let label = match place::split(target) {
            (page, None) => page.to_string(),
            ("", Some(at)) => at.trim_start_matches(['*', '#']).to_string(),
            (page, Some(at)) => format!("{page} › {}", at.trim_start_matches(['*', '#'])),
        };
        format!("<span class='link'>{}</span>", shown(label))
    }
}

/// A line that ends a paragraph: a headline, block, keyword, table, list item, rule…
fn is_structure(line: &str) -> bool {
    let t = line.trim();
    headline_level(line).is_some() || t.starts_with("#+") || t.starts_with('|') || t.starts_with(": ")
        || t == ":" || list_item(line).is_some() || image_line(t).is_some() || is_drawer(t)
        || (t.len() >= 5 && t.chars().all(|c| c == '-'))
}

/// Stars of a headline (`** Titre`).
fn headline_level(line: &str) -> Option<usize> {
    let stars = line.chars().take_while(|&c| c == '*').count();
    (stars > 0 && matches!(line.chars().nth(stars), Some(' ') | None)).then_some(stars)
}

/// `#+BEGIN_SRC python` → `src`.
fn block_kind(t: &str) -> Option<String> {
    let rest = t.get(..8).filter(|k| k.eq_ignore_ascii_case("#+begin_"))
        .map(|_| &t[8..])?;
    let kind = rest.split_whitespace().next()?.to_ascii_lowercase();
    (!kind.is_empty()).then_some(kind)
}

/// `:PROPERTIES:`, `:LOGBOOK:`…
fn is_drawer(t: &str) -> bool {
    t.len() > 2 && t.starts_with(':') && t.ends_with(':') && !t.eq_ignore_ascii_case(":end:")
        && t[1..t.len() - 1].chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

/// The `:width` of an `#+ATTR_ORG:` line, in pixels when it has no unit.
fn attr_width(t: &str) -> Option<String> {
    let mut words = t.split_whitespace();
    words.find(|w| *w == ":width")?;
    let w = words.next()?;
    Some(if w.chars().all(|c| c.is_ascii_digit()) { format!("{w}px") } else { w.to_string() })
}

/// A list item (`- a`, `+ a`, ` * a`, `1. a`, `1) a`): its indent, whether it is
/// numbered, and its text.
fn list_item(line: &str) -> Option<(usize, bool, &str)> {
    let indent = line.len() - line.trim_start().len();
    let t = line.trim_start();
    for bullet in ["- ", "+ ", "* "] {
        if (t.starts_with(bullet) || t == bullet.trim()) && (bullet != "* " || indent > 0) {
            return Some((indent, false, t.get(2..).unwrap_or("")));
        }
    }
    let digits = t.chars().take_while(char::is_ascii_digit).count();
    let after = t.get(digits..)?;
    (digits > 0 && (after.starts_with(". ") || after.starts_with(") "))).then(|| (indent, true, &after[2..]))
}

/// How many of `lines` the list starting at the first takes: its items, the
/// lines indented under them, and the blank lines between.
fn list_lines(lines: &[&str]) -> usize {
    let mut n = 0;
    for (i, line) in lines.iter().enumerate() {
        if line.trim().is_empty() {
            let next = lines[i + 1..].iter().find(|l| !l.trim().is_empty());
            if !next.is_some_and(|l| list_item(l).is_some() || l.starts_with([' ', '\t'])) { break; }
            continue;
        }
        let indented = line.starts_with([' ', '\t']) && headline_level(line).is_none();
        if list_item(line).is_none() && !indented { break; }
        n = i + 1;
    }
    n
}

/// A line holding only a link to an image, maybe with Logseq's size after it:
/// its target and width.
fn image_line(t: &str) -> Option<(&str, Option<String>)> {
    let inner = t.strip_prefix("[[")?;
    let end = inner.find("]]")?;
    let target = &inner[..end];
    let rest = inner[end + 2..].trim();
    if target.contains(['[', ']']) || !images::is_image(target) { return None; }
    if rest.is_empty() { return Some((target, None)); }
    // `{:height 200, :width 300}`
    let size = rest.strip_prefix('{')?.strip_suffix('}')?;
    let width = size.split(',').find_map(|p| p.trim().strip_prefix(":width").map(|w| format!("{}px", w.trim())));
    Some((target, width))
}

/// The link `[[target]]` / `[[target][desc]]` at `start`, and where it ends.
fn link_at(chars: &[char], start: usize) -> Option<(String, Option<String>, usize)> {
    let mut i = start + 2;
    let t0 = i;
    while i < chars.len() && chars[i] != ']' && chars[i] != '[' { i += 1; }
    let target: String = chars[t0..i].iter().collect();
    match (chars.get(i), chars.get(i + 1)) {
        (Some(']'), Some(']')) => Some((target, None, i + 2)),
        (Some(']'), Some('[')) => {
            let d0 = i + 2;
            let end = (d0..chars.len().saturating_sub(1)).find(|&j| chars[j] == ']' && chars[j + 1] == ']')?;
            Some((target, Some(chars[d0..end].iter().collect()), end + 2))
        }
        _ => None,
    }
}

/// Where the emphasis opened by the marker at `start` closes, as org-mode reads it:
/// the marker after a space or punctuation, the text not starting nor ending with
/// a space, the closing marker followed by a space or punctuation.
fn emphasis_end(chars: &[char], start: usize) -> Option<usize> {
    let m = chars[start];
    let before_ok = start == 0 || chars[start - 1].is_whitespace() || "-({'\"".contains(chars[start - 1]);
    if !before_ok || chars.get(start + 1).is_none_or(|c| c.is_whitespace() || *c == m) { return None; }
    (start + 2..chars.len()).find(|&j| {
        chars[j] == m && !chars[j - 1].is_whitespace()
            && chars.get(j + 1).is_none_or(|c| c.is_whitespace() || "-.,:!?;'\")}[".contains(*c))
    })
}

fn starts_with(chars: &[char], s: &str) -> bool {
    s.chars().enumerate().all(|(i, c)| chars.get(i) == Some(&c))
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('\'', "&#39;").replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn html(text: &str) -> String {
        let k = TodoKeywords::parse("TODO | DONE");
        page_html(text, "Page", Some(&k), &|t| format!("url:{t}"))
    }

    #[test]
    fn title_from_the_page() {
        assert!(html("#+TITLE: Mon titre\ntexte").starts_with("<h1 class='title'>Mon titre</h1><p>texte</p>"));
        // A new page starts with a headline holding its name: not twice
        assert_eq!(html("* Page\ntexte"), "<h1>Page</h1><p>texte</p>");
        assert!(html("texte").starts_with("<h1 class='title'>Page</h1>"));
    }

    #[test]
    fn headlines_with_keyword_and_tags() {
        assert_eq!(html("* Page\n** TODO Faire *vite* :a:b:"),
            "<h1>Page</h1><h2><span class='todo'>TODO</span> Faire <b>vite</b> <span class='tag'>a</span><span class='tag'>b</span></h2>");
    }

    #[test]
    fn paragraphs_lists_and_blocks() {
        let out = html("* Page\nune\nligne\n\n- a\n  suite\n  - b\n- [X] c\n1. un\n#+BEGIN_SRC rust\nlet x = 1 < 2;\n#+END_SRC\n:PROPERTIES:\n:ID: x\n:END:\n# commentaire\n-----");
        assert_eq!(out, "<h1>Page</h1><p>une\nligne</p>\
            <ul><li>a suite<ul><li>b</li></ul></li><li>☑ c</li></ul><ol><li>un</li></ol>\
            <pre>let x = 1 &lt; 2;</pre><hr>");
    }

    #[test]
    fn tables_with_a_header() {
        assert_eq!(html("* Page\n| a | b |\n|---+---|\n| 1 | =x= |"),
            "<h1>Page</h1><table><tr><th>a</th><th>b</th></tr><tr><td>1</td><td><code>x</code></td></tr></table>");
    }

    #[test]
    fn links_and_images() {
        let out = html("* Page\nvoir [[Recettes::*Desserts]], [[https://a.org][site]], [[B][bé]] et https://b.org <<ici>>\n#+ATTR_ORG: :width 300\n[[../assets/a.png]]");
        assert_eq!(out, "<h1>Page</h1><p>voir <span class='link'>Recettes › Desserts</span>, \
            <a href='https://a.org'>site</a>, <span class='link'>bé</span> et <a href='https://b.org'>https://b.org</a> </p>\
            <p class='image'><img src='url:../assets/a.png' alt='' style='width: 300px'></p>");
    }

    #[test]
    fn images_in_the_file() {
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
        assert_eq!(data_url("a.JPG", b"M"), "data:image/jpeg;base64,TQ==");
        assert_eq!(data_url("a.png", b""), "data:image/png;base64,");
    }

    #[test]
    fn emphasis_as_org_reads_it() {
        let d = Doc { keywords: None, image: &|t| t.to_string(), shift: 0 };
        assert_eq!(d.inline("a /b/ c_d_e 2*3*4 +x+"), "a <i>b</i> c_d_e 2*3*4 <s>x</s>");
        assert_eq!(d.inline("=a*b*= ~<t>~"), "<code>a*b*</code> <code>&lt;t&gt;</code>");
    }
}
