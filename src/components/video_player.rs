//! The window playing a video in the app: a click on its thumbnail, when the
//! setting "Play the videos in the app" is on. The site's player in a frame, and
//! a button opening the video in the browser instead.

use leptos::{ev, prelude::*};
use wasm_bindgen_futures::spawn_local;

use crate::{i18n::t, invoke, state::AppCtx};

#[component]
pub fn VideoPlayer(
    /// Address of the site's player for the video.
    embed: String,
    /// The link to the video, as written in the page.
    url: String,
) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();
    let close = move || ctx.ui.video.set(None);
    let esc = window_event_listener(ev::keydown, move |e| if e.key() == "Escape" { close(); });
    on_cleanup(move || esc.remove());
    let open = move |_| {
        let url = url.clone();
        spawn_local(async move {
            if let Err(e) = invoke::open_url(&url).await { ctx.error("open_url_failed", &e); }
        });
        close();
    };
    view! {
        <div class="modal-overlay" on:click=move |_| close()>
            <div class="modal video-modal" role="dialog" aria-label=move || t("video_player", lang())
                on:click=|e| e.stop_propagation()>
                <div class="modal-header">
                    <h2>{move || t("video_player", lang())}</h2>
                    <button class="btn-close" aria-label=move || t("close", lang()) on:click=move |_| close()>"×"</button>
                </div>
                <div class="video-frame">
                    <iframe
                        src=embed
                        allow="autoplay; encrypted-media; fullscreen; picture-in-picture"
                        allowfullscreen="true"
                        referrerpolicy="strict-origin-when-cross-origin"
                    ></iframe>
                </div>
                <div class="modal-footer">
                    <button class="btn-secondary" on:click=open>{move || t("video_open_browser", lang())}</button>
                    <button class="btn-primary" on:click=move |_| close()>{move || t("close", lang())}</button>
                </div>
            </div>
        </div>
    }
}
