//! The theme in effect: the CSS of a theme file put in a `<style id="theme">`
//! after the app's own. `index.html` puts back the last one before the app
//! starts (kept in the webview's storage), so no other colours flash first.

use leptos::prelude::*;
use wasm_bindgen::{closure::Closure, JsCast};
use wasm_bindgen_futures::spawn_local;

use crate::{invoke, state::AppCtx, storage};

const DARK: &str = include_str!("../public/css/themes/dark.css");
const LIGHT: &str = include_str!("../public/css/themes/light.css");
/// The built-in themes, as listed: id, i18n key of its label, CSS.
pub const BUILTINS: &[(&str, &str, &str)] = &[
    ("dark", "theme_dark", DARK),
    ("light", "theme_light", LIGHT),
    ("fjord", "theme_fjord", include_str!("../public/css/themes/fjord.css")),
    ("paper", "theme_paper", include_str!("../public/css/themes/paper.css")),
    ("dracula", "theme_dracula", include_str!("../public/css/themes/dracula.css")),
];
/// The webview's storage key of the CSS in effect (read by `index.html`).
const CACHE_KEY: &str = "theme_css";

/// The theme `id` stands for: `auto` is `light` or `dark` as the system is.
pub fn resolve(id: &str) -> &str {
    match id {
        "auto" if system_is_light() => "light",
        "auto" => "dark",
        other => other,
    }
}

/// The CSS of a built-in theme (`auto` too).
pub fn builtin(id: &str) -> Option<&'static str> {
    let id = resolve(id);
    BUILTINS.iter().find(|(b, _, _)| *b == id).map(|(_, _, css)| *css)
}

fn light_query() -> Option<web_sys::MediaQueryList> {
    web_sys::window()?.match_media("(prefers-color-scheme: light)").ok()?
}

fn system_is_light() -> bool {
    light_query().is_some_and(|q| q.matches())
}

fn set_css(css: &str) {
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else { return };
    let style = doc.get_element_by_id("theme").or_else(|| {
        let el = doc.create_element("style").ok()?;
        el.set_id("theme");
        doc.head()?.append_child(&el).ok()?;
        Some(el)
    });
    if let Some(style) = style { style.set_text_content(Some(css)); }
    storage::store(CACHE_KEY, css);
}

/// The CSS of the theme `id`: a built-in one, or a file of the user's.
pub async fn css_of(id: &str) -> Result<String, String> {
    match builtin(id) {
        Some(css) => Ok(css.to_string()),
        None => invoke::read_theme(id).await,
    }
}

/// Put the theme `id` in effect; one that can't be read leaves the system's built-in one.
fn apply(ctx: AppCtx, id: String) {
    spawn_local(async move {
        match css_of(&id).await {
            Ok(css) => set_css(&css),
            Err(e) => {
                set_css(builtin("auto").unwrap_or(DARK));
                ctx.error("theme_error", &e);
            }
        }
    });
}

/// Read again the theme in effect (its file changed).
pub fn reload(ctx: AppCtx) {
    apply(ctx, ctx.prefs.with_untracked(|p| p.theme.clone()));
}

/// Follow the `theme` setting, and the system's light or dark mode for `auto`.
pub fn start(ctx: AppCtx) {
    // Before the settings are read: what the system asks, unless a theme was kept
    if storage::load(CACHE_KEY).is_none() { set_css(builtin("auto").unwrap_or(DARK)); }
    // Its first run sees the default settings, not the saved ones yet
    Effect::new(move |prev: Option<()>| {
        let id = ctx.pref(|p| p.theme.clone());
        if prev.is_some() { apply(ctx, id); }
    });
    if let Some(query) = light_query() {
        let on_change = Closure::<dyn Fn()>::new(move || {
            if ctx.prefs.with_untracked(|p| p.theme == "auto") { apply(ctx, "auto".into()); }
        });
        let _ = query.add_event_listener_with_callback("change", on_change.as_ref().unchecked_ref());
        // Listens as long as the app runs
        on_change.forget();
    }
}
