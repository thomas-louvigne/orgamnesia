use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::{i18n::t, invoke, state::{AppCtx, Tab}};

#[component]
pub fn BacklinksPanel() -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");

    let open_backlink = move |name: String| {
        // Find file by name
        let file = ctx.files.get().into_iter().find(|f| f.name == name);
        if let Some(file) = file {
            let already = ctx.tabs.get().iter().position(|t| t.path == file.path);
            if let Some(idx) = already {
                ctx.active_tab.set(Some(idx));
                return;
            }
            spawn_local(async move {
                match invoke::read_file(&file.path).await {
                    Ok(content) => {
                        ctx.tabs.update(|tabs| {
                            tabs.push(Tab {
                                path: file.path.clone(),
                                name: file.name.clone(),
                                content: RwSignal::new(content),
                                dirty: RwSignal::new(false),
                            });
                        });
                        let idx = ctx.tabs.get().len() - 1;
                        ctx.active_tab.set(Some(idx));
                        if let Ok(bl) = invoke::get_backlinks(&file.name).await {
                            ctx.backlinks.set(bl);
                        }
                    }
                    Err(e) => ctx.status.set(Some(format!("Error: {e}"))),
                }
            });
        }
    };

    view! {
        <div class="backlinks-panel">
            <div class="panel-header">{move || t("backlinks", ctx.lang.get())}</div>
            <div class="panel-body">
                {move || {
                    let bl = ctx.backlinks.get();
                    if bl.is_empty() {
                        let msg = t("no_backlinks", ctx.lang.get());
                        view! { <p class="no-backlinks">{msg}</p> }.into_any()
                    } else {
                        let items = bl.into_iter().map(|name| {
                            let name2 = name.clone();
                            view! {
                                <div class="backlink-item" on:click=move |_| open_backlink(name2.clone())>
                                    {name.clone()}
                                </div>
                            }
                        }).collect_view();
                        view! { <div>{items}</div> }.into_any()
                    }
                }}
            </div>
        </div>
    }
}
