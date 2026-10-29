//! Preview of the page a link leads to, in a bubble under the link: shown with
//! Ctrl held over the link, or once the mouse rests on it (per the settings).
//! For `[[Page::*Titre]]`, only that headline and its subtree.

use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

use super::pane::Pane;
use crate::{actions::follow_link, highlight, i18n::t, keybindings::after_ms, links, tables};
use orgamnesia_core::links as place;

/// How long the mouse rests on a link before its preview shows (without Ctrl).
const HOVER_DELAY_MS: i32 = 500;
/// Lines of the page shown at most.
const MAX_LINES: usize = 40;
const WIDTH: f64 = 480.0;
const MAX_HEIGHT: f64 = 300.0;

/// The bubble shown.
#[derive(Clone, PartialEq)]
struct Shown {
    target: String,
    /// The highlighted text, or a message when there is none.
    html: Option<String>,
    message: &'static str,
    /// Position in pixels relative to the editor wrapper: `y` is the top of the
    /// bubble, or its bottom when `above`.
    x: f64,
    y: f64,
    above: bool,
}

/// The preview of an editor pane.
#[derive(Clone, Copy)]
pub struct Preview {
    pane: Pane,
    wrap: NodeRef<leptos::html::Div>,
    shown: RwSignal<Option<Shown>>,
    /// The link the mouse is over, waiting for its preview.
    over: StoredValue<Option<String>>,
    seq: StoredValue<u32>,
}

impl Preview {
    pub fn new(pane: Pane, wrap: NodeRef<leptos::html::Div>) -> Self {
        Self { pane, wrap, shown: RwSignal::new(None), over: StoredValue::new(None), seq: StoredValue::new(0) }
    }

    pub fn hide(self) {
        self.over.set_value(None);
        if self.shown.get_untracked().is_some() { self.shown.set(None); }
    }

    /// The mouse moved over the editor: preview the link under it, if any.
    pub fn on_move(self, e: &web_sys::MouseEvent) {
        let ctx = self.pane.ctx;
        if self.pane.config || !ctx.pref_untracked(|p| p.link_preview) { return; }
        let hover = ctx.pref_untracked(|p| p.link_preview_trigger == "hover");
        let Some(wrap) = self.wrap.get_untracked() else { return };
        let Some((target, rect)) = link_at(&wrap, e.client_x() as f64, e.client_y() as f64) else {
            self.hide();
            return;
        };
        if self.shown.with_untracked(|s| s.as_ref().is_some_and(|s| s.target == target)) { return; }
        if !hover && !e.ctrl_key() { self.hide(); return; }
        if self.over.with_value(|o| o.as_ref() == Some(&target)) { return; }
        self.over.set_value(Some(target.clone()));
        let n = self.seq.get_value() + 1;
        self.seq.set_value(n);
        let wrap_rect = wrap.get_bounding_client_rect();
        let x = (rect.left() - wrap_rect.left()).min(wrap_rect.width() - WIDTH).max(0.0);
        // Under the link, or above it near the bottom of the editor
        let below = rect.bottom() - wrap_rect.top();
        let above = below + MAX_HEIGHT > wrap_rect.height() && rect.top() - wrap_rect.top() > MAX_HEIGHT;
        let y = if above { wrap_rect.bottom() - rect.top() } else { below };
        let show = move || {
            if self.seq.try_get_value() != Some(n) { return; }
            spawn_local(async move {
                let (html, message) = self.contents(&target).await;
                if self.seq.try_get_value() != Some(n) || self.over.with_value(|o| o.as_ref() != Some(&target)) { return; }
                self.shown.set(Some(Shown { target, html, message, x, y, above }));
            });
        };
        if hover { after_ms(HOVER_DELAY_MS, show) } else { show() }
    }

    /// What the bubble shows for a link to `target`: the highlighted text of the
    /// page (or of the place it names), else why there is none.
    async fn contents(self, target: &str) -> (Option<String>, &'static str) {
        let ctx = self.pane.ctx;
        let (page, at) = place::split(target);
        let Some((text, keywords)) = self.pane.named_page_text(page).await else { return (None, "link_preview_new") };
        let part = match at {
            None => text,
            Some(at) => match place::find(&text, at, keywords.as_ref()) {
                Some((a, _)) => text.chars().skip(a).take(place::section_end(&text, a) - a).collect(),
                None => return (None, "link_place_missing"),
            },
        };
        let mut lines: Vec<&str> = part.trim_end().split('\n').collect();
        if lines.iter().all(|l| l.trim().is_empty()) { return (None, "link_preview_empty"); }
        let cut = lines.len() > MAX_LINES;
        lines.truncate(MAX_LINES);
        let mut text = lines.join("\n");
        if cut { text.push_str("\n…"); }
        let view = tables::View { text, ..Default::default() };
        (Some(highlight::render_view(&view, ctx.tags_untracked(), keywords, false, None)), "")
    }

    /// The bubble, when shown.
    pub fn bubble(self) -> impl IntoView {
        let ctx = self.pane.ctx;
        move || self.shown.get().map(|s| {
            let vertical = if s.above { format!("bottom: {}px", s.y) } else { format!("top: {}px", s.y) };
            let target = s.target.clone();
            view! {
                <div
                    class="link-preview"
                    style=format!("left: {}px; {vertical}; width: {WIDTH}px; max-height: {MAX_HEIGHT}px;", s.x)
                    on:mouseleave=move |_| self.hide()
                >
                    <button
                        class="link-preview-title"
                        title=move || t("link_preview_open", ctx.lang.get())
                        on:click=move |_| { self.hide(); follow_link(ctx, target.clone()); }
                    >
                        {s.target.clone()}
                    </button>
                    {match s.html {
                        Some(html) => view! { <div class="link-preview-text" inner_html=html /> }.into_any(),
                        None => view! { <div class="link-preview-message">{move || t(s.message, ctx.lang.get())}</div> }.into_any(),
                    }}
                </div>
            }
        })
    }

    /// The mouse left the textarea: the preview goes, unless the mouse went into it.
    pub fn on_leave(self, e: &web_sys::MouseEvent) {
        let into = e.related_target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
            .and_then(|el| el.closest(".link-preview").ok().flatten());
        if into.is_none() { self.hide(); }
    }
}

/// The target of the link drawn under the point `x`, `y` of the window in the
/// editor `wrap`, with the box of the link; only links to pages or to a place in one.
fn link_at(wrap: &web_sys::Element, x: f64, y: f64) -> Option<(String, web_sys::DomRect)> {
    let found = wrap.query_selector_all(".hl-layer .link").ok()?;
    (0..found.length())
        .filter_map(|i| found.item(i)?.dyn_into::<web_sys::Element>().ok())
        .find_map(|link| {
            let rects = link.get_client_rects();
            let rect = (0..rects.length()).filter_map(|i| rects.item(i))
                .find(|r| r.left() <= x && x <= r.right() && r.top() <= y && y <= r.bottom())?;
            let target = link.query_selector(".link-t").ok()??.text_content()?.trim().to_string();
            let (page, at) = place::split(&target);
            let previewed = if page.is_empty() { at.is_some_and(|a| !a.starts_with('#')) } else { links::is_page_target(&target) };
            previewed.then_some((target, rect))
        })
}
