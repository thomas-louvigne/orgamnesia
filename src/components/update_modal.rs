//! The window offering a new version of the app: what changed in each version
//! since the running one, then install it, later, or skip it.

use leptos::{ev, prelude::*};
use wasm_bindgen_futures::spawn_local;

use crate::{
    actions,
    i18n::{t, Lang},
    invoke, update,
    state::{AppCtx, UpdateInfo, VersionNotes},
};

/// The notes of a version in `lang`, else in English.
fn items(notes: &VersionNotes, lang: Lang) -> Vec<String> {
    if matches!(lang, Lang::Fr) && !notes.fr.is_empty() { notes.fr.clone() } else { notes.en.clone() }
}

#[component]
pub fn UpdateModal(info: UpdateInfo) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();
    // Downloading and installing: the window stays, its buttons off
    let busy = RwSignal::new(false);
    let close = move || if !busy.get_untracked() { ctx.ui.update.set(None); };
    let esc = window_event_listener(ev::keydown, move |e| if e.key() == "Escape" { close(); });
    on_cleanup(move || esc.remove());

    let version = info.version.clone();
    let skip = move || {
        update::skip(ctx, version.clone());
        close();
    };
    let can_install = info.can_install;
    let install = move || {
        if !can_install {
            spawn_local(async move {
                if let Err(e) = invoke::open_url(update::RELEASES_URL).await { ctx.error("open_url_failed", &e); }
            });
            close();
            return;
        }
        busy.set(true);
        spawn_local(async move {
            // The pages are saved first: the app starts again on the new version
            if !actions::save_dirty_tabs(ctx).await {
                let _ = busy.try_set(false);
                return;
            }
            ctx.notify_t("update_installing", "");
            if let Err(e) = invoke::install_update().await {
                let _ = busy.try_set(false);
                ctx.error("update_error", &e);
            }
        });
    };

    let title = info.version.clone();
    let changes = info.changes.clone();
    let current = info.current.clone();
    view! {
        <div class="modal-overlay" on:click=move |_| close()>
            <div class="modal update-modal" role="dialog" aria-label=move || t("update_title", lang())
                on:click=|e| e.stop_propagation()>
                <div class="modal-header">
                    <h2>{move || format!("{} {title}", t("update_title", lang()))}</h2>
                    <button class="btn-close" aria-label=move || t("close", lang()) on:click=move |_| close()>"×"</button>
                </div>
                <div class="modal-body update-body">
                    <p class="setting-hint">{move || format!("{} {current}.", t("update_running", lang()))}</p>
                    {move || {
                        let lang = lang();
                        if changes.is_empty() {
                            return view! { <p>{t("update_no_notes", lang)}</p> }.into_any();
                        }
                        changes.iter().map(|v| {
                            let head = match &v.date {
                                Some(d) => format!("{} — {d}", v.version),
                                None => v.version.clone(),
                            };
                            view! {
                                <section class="update-version">
                                    <h3>{head}</h3>
                                    <ul>{items(v, lang).into_iter().map(|i| view! { <li>{i}</li> }).collect_view()}</ul>
                                </section>
                            }
                        }).collect_view().into_any()
                    }}
                    {(!can_install).then(|| view! {
                        <p class="setting-hint">{move || t("update_manual", lang())}</p>
                    })}
                </div>
                <div class="modal-footer">
                    <button class="btn-secondary" prop:disabled=move || busy.get() on:click=move |_| skip()>
                        {move || t("update_skip", lang())}
                    </button>
                    <button class="btn-secondary" prop:disabled=move || busy.get() on:click=move |_| close()>
                        {move || t("update_later", lang())}
                    </button>
                    <button class="btn-primary" prop:disabled=move || busy.get() on:click=move |_| install()>
                        {move || busy.get().then(|| view! { <span class="git-spinner" aria-hidden="true"></span> })}
                        {move || t(if can_install { "update_install" } else { "update_download" }, lang())}
                    </button>
                </div>
            </div>
        </div>
    }
}
