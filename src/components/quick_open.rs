use leptos::{html, prelude::*};
use wasm_bindgen_futures::spawn_local;

use crate::{i18n::t, invoke, state::{AppCtx, Tab}};

#[derive(Clone, Debug)]
enum QuickItem {
    Tab { idx: usize, name: String, is_active: bool },
    File { name: String, path: String },
    Create { name: String },
}

impl QuickItem {
    fn display_name(&self) -> String {
        match self {
            Self::Tab { name, .. } | Self::File { name, .. } | Self::Create { name } => name.clone(),
        }
    }
}

fn do_open(ctx: AppCtx, item: QuickItem) {
    ctx.show_quick_open.set(false);
    match item {
        QuickItem::Tab { idx, .. } => {
            ctx.active_tab.set(Some(idx));
        }
        QuickItem::File { name, path } => {
            let content_sig = RwSignal::new(String::new());
            let dirty_sig   = RwSignal::new(false);
            spawn_local(async move {
                match invoke::read_file(&path).await {
                    Ok(content) => {
                        content_sig.set(content);
                        ctx.tabs.update(|tabs| tabs.push(Tab {
                            path: path.clone(),
                            name,
                            content: content_sig,
                            dirty: dirty_sig,
                        }));
                        ctx.active_tab.set(Some(ctx.tabs.get().len() - 1));
                    }
                    Err(e) => ctx.status.set(Some(format!("Error: {e}"))),
                }
            });
        }
        QuickItem::Create { name } => {
            spawn_local(async move {
                match invoke::create_page(&name).await {
                    Ok(f) => {
                        let content_sig = RwSignal::new(format!("* {}\n", f.name));
                        let dirty_sig   = RwSignal::new(true);
                        ctx.files.update(|fs| {
                            fs.push(f.clone());
                            fs.sort_by(|a, b| a.name.cmp(&b.name));
                        });
                        ctx.tabs.update(|tabs| tabs.push(Tab {
                            path: f.path,
                            name: f.name,
                            content: content_sig,
                            dirty: dirty_sig,
                        }));
                        ctx.active_tab.set(Some(ctx.tabs.get().len() - 1));
                    }
                    Err(e) => ctx.status.set(Some(format!("Error: {e}"))),
                }
            });
        }
    }
}

#[component]
pub fn QuickOpenModal() -> impl IntoView {
    let ctx      = use_context::<AppCtx>().expect("AppCtx");
    let query    = RwSignal::new(String::new());
    let selected = RwSignal::new(0usize);
    let input_ref = NodeRef::<html::Input>::new();

    Effect::new(move |_| {
        if let Some(el) = input_ref.get() { let _ = el.focus(); }
    });

    let items = move || -> Vec<QuickItem> {
        let q     = query.get().to_lowercase();
        let tabs  = ctx.tabs.get();
        let files = ctx.files.get();
        let active = ctx.active_tab.get();
        let mut list = Vec::new();

        for (i, tab) in tabs.iter().enumerate() {
            if q.is_empty() || tab.name.to_lowercase().contains(&q) {
                list.push(QuickItem::Tab {
                    idx: i,
                    name: tab.name.clone(),
                    is_active: Some(i) == active,
                });
            }
        }

        let open_paths: Vec<&str> = tabs.iter().map(|t| t.path.as_str()).collect();
        for file in files.iter() {
            if open_paths.contains(&file.path.as_str()) { continue; }
            if q.is_empty() || file.name.to_lowercase().contains(&q) {
                list.push(QuickItem::File {
                    name: file.name.clone(),
                    path: file.path.clone(),
                });
            }
        }

        if !q.is_empty() {
            list.push(QuickItem::Create { name: query.get() });
        }

        list
    };

    let on_keydown = move |e: web_sys::KeyboardEvent| {
        let list = items();
        let len  = list.len();
        match e.key().as_str() {
            "Escape" => {
                e.prevent_default();
                ctx.show_quick_open.set(false);
            }
            "ArrowDown" => {
                e.prevent_default();
                if len > 0 { selected.update(|s| *s = (*s + 1).min(len - 1)); }
            }
            "ArrowUp" => {
                e.prevent_default();
                selected.update(|s| *s = s.saturating_sub(1));
            }
            "Enter" => {
                e.prevent_default();
                let idx = if len == 0 { return } else { selected.get().min(len - 1) };
                if let Some(item) = list.into_iter().nth(idx) {
                    do_open(ctx, item);
                }
            }
            _ => {}
        }
    };

    view! {
        <div class="quick-open-overlay" on:click=move |_| ctx.show_quick_open.set(false)>
            <div class="quick-open-modal" on:click=|e: web_sys::MouseEvent| e.stop_propagation()>
                <input
                    node_ref=input_ref
                    class="quick-open-input"
                    type="text"
                    placeholder=move || t("quick_open_ph", ctx.lang.get())
                    prop:value=move || query.get()
                    on:input=move |e| {
                        query.set(event_target_value(&e));
                        selected.set(0);
                    }
                    on:keydown=on_keydown
                />
                {move || {
                    let list = items();
                    let sel  = if list.is_empty() { 0 } else { selected.get().min(list.len() - 1) };
                    if list.is_empty() {
                        view! {
                            <div class="quick-open-empty">
                                {t("quick_open_empty", ctx.lang.get())}
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <div class="quick-open-list">
                                {list.into_iter().enumerate().map(|(i, item)| {
                                    let is_sel = i == sel;
                                    let name   = item.display_name();
                                    let lang   = ctx.lang.get();
                                    let (icon, cls, hint): (&str, &str, Option<&str>) = match &item {
                                        QuickItem::Tab { is_active: true,  .. } => ("●", "qo-tab",    None),
                                        QuickItem::Tab { is_active: false, .. } => ("○", "qo-tab",    None),
                                        QuickItem::File { .. }                  => ("▫", "qo-file",   None),
                                        QuickItem::Create { .. }                => ("+", "qo-create", Some(t("quick_open_new", lang))),
                                    };
                                    view! {
                                        <div
                                            class=if is_sel { "quick-open-item selected" } else { "quick-open-item" }
                                            on:click=move |_| do_open(ctx, item.clone())
                                            on:mouseenter=move |_| selected.set(i)
                                        >
                                            <span class=format!("qo-icon {}", cls)>{icon}</span>
                                            <span class="qo-name">{name}</span>
                                            {hint.map(|h| view! { <span class="qo-hint">{h}</span> })}
                                        </div>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        }.into_any()
                    }
                }}
            </div>
        </div>
    }
}
