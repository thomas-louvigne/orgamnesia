//! "Did you know?": one tip at a time on a feature, with its shortcut in the
//! active profile, and a button for the next one.

use leptos::prelude::*;

use crate::{i18n::t, state::{AppCtx, Prefs}};

/// The text of a tip (`{key}` replaced by the shortcut of the action), the
/// action, and whether it is available with these settings.
type Tip = (&'static str, Option<&'static str>, fn(&Prefs) -> bool);

const TIPS: &[Tip] = &[
    ("tip_commands", Some("quick_open"), |_| true),
    ("tip_tags_quick", Some("quick_open"), |p| p.show_tags),
    ("tip_switch_project", Some("switch_project"), |p| p.show_project),
    ("tip_shortcuts", Some("show_shortcuts"), |_| true),
    ("tip_project_search", Some("project_search"), |_| true),
    ("tip_add_tag", Some("add_tag"), |_| true),
    ("tip_split", Some("split_vertical"), |_| true),
    ("tip_page_back", Some("page_back"), |_| true),
    ("tip_graph", Some("graph"), |p| p.graph),
    ("tip_git_commit", Some("git_commit"), |p| p.git_ext),
    ("tip_export_pdf", Some("export_pdf"), |_| true),
    ("tip_layout", None, |_| true),
    ("tip_mouse_tips", None, |p| p.shortcut_tips),
];

/// The tips that make sense now: the action has a shortcut, the feature is on.
fn available(ctx: AppCtx) -> Vec<String> {
    let lang = ctx.lang.get();
    let prefs = ctx.prefs.get();
    TIPS.iter().filter(|(_, _, on)| on(&prefs)).filter_map(|&(key, id, _)| {
        let text = t(key, lang);
        match id {
            Some(id) => ctx.shortcut(id).map(|k| text.replace("{key}", &k)),
            None => Some(text.to_string()),
        }
    }).collect()
}

#[component]
pub fn TipsFrame() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    // A different one at each start
    let index = RwSignal::new((js_sys::Math::random() * TIPS.len() as f64) as usize);
    let tip = move || {
        let tips = available(ctx);
        (!tips.is_empty()).then(|| tips[index.get() % tips.len()].clone())
    };
    view! {
        <div class="frame panel-section tips-frame">
            <div class="panel-header panel-header-row">
                <span>{move || t("tips_title", ctx.lang.get())}</span>
                <button
                    class="panel-add"
                    title=move || t("tips_next", ctx.lang.get())
                    aria-label=move || t("tips_next", ctx.lang.get())
                    on:click=move |_| index.update(|i| *i += 1)
                >"›"</button>
            </div>
            <p class="tips-text" aria-live="polite">{tip}</p>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each tip naming an action says where its shortcut goes.
    #[test]
    fn tips_with_an_action_show_its_shortcut() {
        for (key, id, _) in TIPS {
            for lang in [crate::i18n::Lang::En, crate::i18n::Lang::Fr] {
                assert_eq!(t(key, lang).contains("{key}"), id.is_some(), "{key}");
            }
        }
    }
}
