use leptos::{html, prelude::*};
use wasm_bindgen::JsCast;

use orgamnesia_core::names::{fold, search_order};

use crate::{actions, i18n::t, state::{AppCtx, FileEntry, Goto}};

#[derive(Clone, Debug)]
enum QuickItem {
    Tab { idx: usize, name: String, is_active: bool },
    File(FileEntry),
    /// An org-mode tag: shown in the tags frame.
    Tag(String),
    Create { name: String },
}

impl QuickItem {
    fn display_name(&self) -> String {
        match self {
            Self::Tab { name, .. } | Self::Create { name } => name.clone(),
            Self::File(f) => f.name.clone(),
            Self::Tag(tag) => format!(":{tag}:"),
        }
    }
}

fn do_open(ctx: AppCtx, item: QuickItem) {
    ctx.ui.show_quick_open.set(false);
    match item {
        QuickItem::Tab { idx, .. } => {
            ctx.close_panels();
            ctx.work.active_tab.set(Some(idx));
            if let Some(path) = ctx.work.tabs.with_untracked(|tabs| tabs.get(idx).map(|t| t.path.clone())) {
                ctx.work.goto.set(Some((path, Goto::Focus)));
            }
        }
        QuickItem::File(file) => actions::open_file(ctx, file, Some(Goto::Focus)),
        QuickItem::Tag(tag) => ctx.show_tag(tag),
        QuickItem::Create { name } => actions::create_and_open_page(ctx, name),
    }
}

/// Scrolls the list just enough to show its line `i`.
fn reveal(list: NodeRef<html::Div>, i: usize) {
    let Some(list) = list.get_untracked() else { return };
    let Ok(lines) = list.query_selector_all(".quick-open-item") else { return };
    let Some(line) = lines.item(i as u32) else { return };
    let line = line.unchecked_into::<web_sys::Element>().get_bounding_client_rect();
    let view = list.get_bounding_client_rect();
    let by = if line.top() < view.top() {
        line.top() - view.top()
    } else if line.bottom() > view.bottom() {
        line.bottom() - view.bottom()
    } else {
        return;
    };
    list.set_scroll_top(list.scroll_top() + by.round() as i32);
}

#[component]
pub fn QuickOpenModal() -> impl IntoView {
    let ctx      = expect_context::<AppCtx>();
    let query    = RwSignal::new(String::new());
    let selected = RwSignal::new(0usize);
    let input_ref = NodeRef::<html::Input>::new();
    let list_ref = NodeRef::<html::Div>::new();

    Effect::new(move |_| {
        if let Some(el) = input_ref.get() { let _ = el.focus(); }
    });

    let items = move || -> Vec<QuickItem> {
        let q     = fold(&query.get());
        let tabs  = ctx.work.tabs.get();
        let active = ctx.work.active_tab.get();
        // Case and accents ignored: "regle" finds "Règle"
        let matches = |name: &str| q.is_empty() || fold(name).contains(&q);
        let mut list = Vec::new();

        for (i, tab) in tabs.iter().enumerate() {
            if matches(&tab.name) {
                list.push(QuickItem::Tab {
                    idx: i,
                    name: tab.name.clone(),
                    is_active: Some(i) == active,
                });
            }
        }

        ctx.project.files.with(|files| {
            for file in files {
                let open = tabs.iter().any(|t| t.path == file.path);
                if !open && matches(&file.name) {
                    list.push(QuickItem::File(file.clone()));
                }
            }
        });
        if !q.is_empty() {
            list.sort_by_cached_key(|item| search_order(&item.display_name(), &q));
        }

        // The tags, when typing something and the tags frame is shown (`:pro` or `#pro` work too)
        let tag_q = q.trim_start_matches([':', '#']).trim_end_matches(':');
        if !tag_q.is_empty() && ctx.pref(|p| p.show_tags) {
            let names: Vec<String> = ctx.project.tags.with(|tags| tags.iter().map(|t| t.name.clone()).collect());
            let exact = names.iter().find(|n| fold(n) == tag_q).cloned();
            let found = crate::links::complete_page(&names, tag_q, None, 8);
            list.extend(exact.into_iter().chain(found).map(QuickItem::Tag));
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
                ctx.ui.show_quick_open.set(false);
            }
            "ArrowDown" => {
                e.prevent_default();
                if len > 0 { selected.update(|s| *s = (*s + 1).min(len - 1)); }
                reveal(list_ref, selected.get_untracked());
            }
            "ArrowUp" => {
                e.prevent_default();
                selected.update(|s| *s = s.saturating_sub(1));
                reveal(list_ref, selected.get_untracked());
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
        <div class="quick-open-overlay" on:click=move |_| ctx.ui.show_quick_open.set(false)>
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
                    let last = list.len().saturating_sub(1);
                    if list.is_empty() {
                        view! {
                            <div class="quick-open-empty">
                                {t("quick_open_empty", ctx.lang.get())}
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <div class="quick-open-list" node_ref=list_ref>
                                {list.into_iter().enumerate().map(|(i, item)| {
                                    let name   = item.display_name();
                                    let lang   = ctx.lang.get();
                                    let (icon, cls, hint): (&str, &str, Option<&str>) = match &item {
                                        QuickItem::Tab { is_active: true,  .. } => ("●", "qo-tab",    None),
                                        QuickItem::Tab { is_active: false, .. } => ("○", "qo-tab",    None),
                                        QuickItem::File(_)                      => ("▫", "qo-file",   None),
                                        QuickItem::Tag(_)                       => ("#", "qo-tag",    Some(t("quick_open_tag", lang))),
                                        QuickItem::Create { .. }                => ("+", "qo-create", Some(t("quick_open_new", lang))),
                                    };
                                    view! {
                                        <div
                                            class=move || if selected.get().min(last) == i { "quick-open-item selected" } else { "quick-open-item" }
                                            on:click=move |_| do_open(ctx, item.clone())
                                            // Only a mouse that moves: the list scrolling under a still one is not a choice
                                            on:mousemove=move |e: web_sys::MouseEvent| {
                                                if e.movement_x() != 0 || e.movement_y() != 0 { selected.set(i); }
                                            }
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
