//! Fields of the settings window bound to a setting of its draft.

use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::{
    i18n::{t, Lang},
    invoke,
    state::{AppCtx, Prefs},
};

/// A checkbox bound to one setting of the window's draft.
#[component]
pub fn Check(
    draft: RwSignal<Prefs>,
    get: fn(&Prefs) -> bool,
    set: fn(&mut Prefs, bool),
    label: &'static str,
    #[prop(optional)] hint: Option<&'static str>,
    /// Indented under the setting it depends on.
    #[prop(optional)] sub: bool,
    /// Greyed out while this is true (the setting it depends on is off).
    #[prop(optional)] disabled: Option<fn(&Prefs) -> bool>,
) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();
    view! {
        <div class=if sub { "setting-row setting-sub" } else { "setting-row" }>
            <label class="setting-check">
                <input
                    type="checkbox"
                    prop:disabled=move || disabled.is_some_and(|d| draft.with(d))
                    prop:checked=move || draft.with(get)
                    on:change=move |e| draft.update(|p| set(p, event_target_checked(&e)))
                />
                {move || t(label, lang())}
            </label>
            {hint.map(|h| view! { <span class="setting-hint">{move || t(h, lang())}</span> })}
        </div>
    }
}

/// A whole number setting of the trash, greyed out while it is not emptied automatically.
/// With `readable`, the size in bytes is also given in kB / MB.
#[component]
pub fn NumberField(
    draft: RwSignal<Prefs>,
    get: fn(&Prefs) -> u64,
    set: fn(&mut Prefs, u64),
    label: &'static str,
    hint: &'static str,
    #[prop(optional)] readable: bool,
) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();
    view! {
        <div class="setting-row setting-sub">
            <label>{move || t(label, lang())}</label>
            <div class="setting-input-row number-row">
                <input
                    type="number"
                    min="0"
                    step="1"
                    class="setting-input setting-number"
                    prop:disabled=move || !draft.with(|p| p.trash_auto_empty)
                    prop:value=move || draft.with(get).to_string()
                    on:input=move |e| {
                        // A field left empty or invalid counts as 0 (no limit)
                        let v = event_target_value(&e).trim().parse().unwrap_or(0);
                        draft.update(|p| set(p, v));
                    }
                />
                {readable.then(|| view! {
                    <span class="setting-unit">{move || readable_size(draft.with(get), lang())}</span>
                })}
            </div>
            <span class="setting-hint">{move || t(hint, lang())}</span>
        </div>
    }
}

/// `10000000` → "≈ 10 Mo" / "≈ 10 MB" (decimal units, as file managers show them); "" for 0.
fn readable_size(bytes: u64, lang: Lang) -> String {
    let fr = lang == Lang::Fr;
    let units = if fr { ["o", "ko", "Mo", "Go"] } else { ["B", "kB", "MB", "GB"] };
    let (value, unit) = match bytes {
        0 => return String::new(),
        b if b >= 1_000_000_000 => (b as f64 / 1e9, units[3]),
        b if b >= 1_000_000 => (b as f64 / 1e6, units[2]),
        b if b >= 1_000 => (b as f64 / 1e3, units[1]),
        b => return format!("{b} {}", units[0]),
    };
    let text = format!("{value:.1}").replace(".0", "");
    let text = if fr { text.replace('.', ",") } else { text };
    format!("≈ {text} {unit}")
}

/// A folder (or file) path field with its "Browse…" button.
#[component]
pub fn PathField(
    value: Signal<String>,
    on_change: Callback<String>,
    #[prop(optional)] placeholder: Option<Signal<String>>,
    #[prop(optional, into)] disabled: Option<Signal<bool>>,
    /// Pick a file (opening in this folder) instead of a folder.
    #[prop(optional)] file_in: Option<&'static str>,
) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let disabled = move || disabled.is_some_and(|d| d.get());
    let pick = move |_| {
        spawn_local(async move {
            let picked = match file_in {
                Some(dir) => invoke::pick_file(Some(dir)).await,
                None => invoke::pick_folder(None).await,
            };
            match picked {
                Ok(Some(path)) => on_change.run(path),
                Ok(None) => {}
                Err(e) => ctx.error("error", &e),
            }
        });
    };
    view! {
        <div class="setting-input-row">
            <input
                type="text"
                class="setting-input"
                prop:disabled=disabled
                placeholder=move || placeholder.map(|p| p.get()).unwrap_or_default()
                prop:value=move || value.get()
                on:input=move |e| on_change.run(event_target_value(&e))
            />
            <button class="btn-pick" prop:disabled=disabled on:click=pick>
                {move || t("browse", ctx.lang.get())}
            </button>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::readable_size;
    use crate::i18n::Lang;

    #[test]
    fn sizes_read_in_decimal_units() {
        assert_eq!(readable_size(0, Lang::Fr), "");
        assert_eq!(readable_size(512, Lang::Fr), "512 o");
        assert_eq!(readable_size(1_500, Lang::Fr), "≈ 1,5 ko");
        assert_eq!(readable_size(10_000_000, Lang::Fr), "≈ 10 Mo");
        assert_eq!(readable_size(2_340_000_000, Lang::En), "≈ 2.3 GB");
    }
}
