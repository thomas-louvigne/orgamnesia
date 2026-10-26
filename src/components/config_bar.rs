//! The bar above a configuration file of the "preference" project: whether its
//! text is valid (what is wrong, and where), applying it, going back to a valid
//! version of it, and coming back to the project.

use leptos::prelude::*;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::spawn_local;

use crate::{
    i18n::t,
    invoke,
    preferences::{self, File, Problem},
    state::{AppCtx, Goto, Tab},
};

#[component]
pub fn ConfigBar(tab: Tab, file: File) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();
    let checked = preferences::checked(ctx, tab.content, file.clone());
    // Shared by the buttons
    let file = StoredValue::new(file);
    let dirty = tab.dirty;
    let valid = move || checked.with(|c| c.is_ok());
    // What the status tells: the error, else the first warning
    let problem = move || checked.with(|c| match c {
        Err(p) => Some(p.clone()),
        Ok(c) => c.warnings.first().cloned(),
    });
    let status_class = move || checked.with(|c| match c {
        Err(_) => "config-status error",
        Ok(c) if c.warnings.is_empty() => "config-status ok",
        Ok(_) => "config-status warning",
    });
    let status_text = move || {
        let lang = lang();
        checked.with(|c| match c {
            Err(p) => preferences::describe(p, lang),
            Ok(c) if c.warnings.is_empty() => t("pref_valid", lang).to_string(),
            Ok(c) => {
                let more = c.warnings.len() - 1;
                let first = preferences::describe(&c.warnings[0], lang);
                if more == 0 { first } else { format!("{first} (+{more})") }
            }
        })
    };
    // The caret goes where the problem is
    let path = tab.path.clone();
    let go_to = move |p: Problem| {
        let chars: Vec<char> = tab.content.get_untracked().chars().collect();
        let at = preferences::char_index(&chars, p.line, p.column);
        ctx.work.goto.set(Some((path.clone(), Goto::Range(at, at))));
    };

    let apply_tab = tab.clone();
    // The versions kept, while their menu is open
    let versions = RwSignal::new(None::<Vec<u64>>);
    let toggle_versions = move || {
        if versions.get_untracked().is_some() {
            versions.set(None);
            return;
        }
        spawn_local(async move {
            match invoke::config_versions(&file.get_value()).await {
                Ok(list) => versions.set(Some(list)),
                Err(e) => ctx.error("error", &e),
            }
        });
    };
    let date = move |stamp: u64| {
        let d = js_sys::Date::new(&JsValue::from_f64(stamp as f64));
        String::from(d.to_locale_string(ctx.lang.get_untracked().as_str(), &JsValue::UNDEFINED))
    };

    view! {
        <div class="config-bar" role="region" aria-label=move || t("pref_bar", lang())>
            <button
                class="btn-secondary config-back"
                title=move || t("pref_back_title", lang())
                on:click=move |_| preferences::leave(ctx, || ())
            >
                {move || format!("← {}", preferences::aside_name(ctx))}
            </button>
            <button
                class=status_class
                title=move || problem().map(|_| t("pref_go_to_problem", lang()))
                on:click=move |_| if let Some(p) = problem() { go_to(p) }
            >
                <span class="config-status-icon" aria-hidden="true">
                    {move || checked.with(|c| match c {
                        Err(_) => "✗",
                        Ok(c) if c.warnings.is_empty() => "✓",
                        Ok(_) => "⚠",
                    })}
                </span>
                <span class="config-status-text">{status_text}</span>
            </button>
            <div class="config-actions">
                <button
                    class="btn-secondary"
                    title=move || t("pref_revert_title", lang())
                    disabled=move || !dirty.get() && valid()
                    on:click=move |_| preferences::restore(ctx, file.get_value(), None)
                >{move || t("pref_revert", lang())}</button>
                <div class="config-history">
                    <button
                        class="btn-secondary"
                        aria-expanded=move || versions.with(|v| v.is_some()).to_string()
                        on:click=move |_| toggle_versions()
                    >{move || format!("{} ▾", t("pref_history", lang()))}</button>
                    {move || versions.get().map(|list| view! {
                        <div class="ctx-overlay" on:click=move |_| versions.set(None) />
                        <div class="ctx-menu config-history-menu" role="menu">
                            {list.is_empty().then(|| view! {
                                <div class="config-history-empty">{move || t("pref_history_empty", lang())}</div>
                            })}
                            {list.into_iter().map(|stamp| {
                                let when = date(stamp);
                                let label = when.clone();
                                view! {
                                    <button class="ctx-menu-item" role="menuitem" on:click=move |_| {
                                        versions.set(None);
                                        let lang = ctx.lang.get_untracked();
                                        let msg = format!("{} {} ({}) ?", t("pref_restore_version_confirm", lang), file.get_value().name(), when);
                                        ctx.ask_confirm(msg, move || preferences::restore(ctx, file.get_value(), Some(stamp)));
                                    }>{label}</button>
                                }
                            }).collect_view()}
                        </div>
                    })}
                </div>
                <button
                    class="btn-primary"
                    title=move || ctx.with_shortcut("pref_apply_title", "save")
                    disabled=move || !valid() || !dirty.get()
                    on:click=move |_| preferences::apply(ctx, apply_tab.clone())
                >{move || t("apply", lang())}</button>
            </div>
        </div>
    }
}
