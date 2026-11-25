use leptos::{html, prelude::*};
use wasm_bindgen::JsCast;

use orgamnesia_core::{names::{fold, search_order}, Format};
use wasm_bindgen_futures::spawn_local;

use crate::{
    actions,
    i18n::t,
    keybindings::{AppAction, EditorAction, APP_ACTIONS, EDITOR_ACTIONS, GIT_ACTIONS, JOURNAL_ACTIONS, PALETTE_EDITOR_ACTIONS},
    state::{AppCtx, FileEntry, Goto, Tab},
};

#[derive(Clone, Debug)]
enum QuickItem {
    Tab { idx: usize, name: String, is_active: bool },
    File(FileEntry),
    /// An org-mode tag: shown in the tags frame.
    Tag(String),
    Create { name: String },
    /// An application action (`>` typed first), with its shortcut if it has one.
    Command { action: AppAction, label: String, keys: Option<String> },
    /// An editing action on the page open, the same way.
    Edit { action: EditorAction, label: String, keys: Option<String> },
    /// A template of the project (`templates/`), applied to the page open.
    Template { file: FileEntry, label: String },
}

impl QuickItem {
    fn display_name(&self) -> String {
        match self {
            Self::Tab { name, .. } | Self::Create { name } | Self::Command { label: name, .. } | Self::Edit { label: name, .. } | Self::Template { label: name, .. } => name.clone(),
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
        QuickItem::Command { action, .. } => crate::app::run_app_action(ctx, action),
        QuickItem::Edit { action, .. } => if let Some(tab) = ctx.work.active_tab_data() {
            ctx.work.goto.set(Some((tab.path, Goto::Action(action))));
        },
        QuickItem::Template { file, .. } => if let Some(tab) = ctx.work.active_tab_data() {
            spawn_local(async move {
                match crate::invoke::read_file(&file.path).await {
                    Ok(text) => {
                        let now = js_sys::Date::new_0();
                        let now = orgamnesia_core::templates::Now {
                            year: now.get_full_year() as i32, month: now.get_month() + 1, day: now.get_date(),
                            hour: now.get_hours(), minute: now.get_minutes(),
                        };
                        let text = orgamnesia_core::templates::fill(&text, &tab.name, now);
                        let page = tab.content.get_untracked();
                        let path = tab.path.clone();
                        let apply = move |whole: bool| {
                            let (path, text) = (path.clone(), text.clone());
                            move || ctx.work.goto.set(Some((path.clone(), Goto::Insert { text: text.clone(), whole })))
                        };
                        if orgamnesia_core::templates::replaces(&page) { return apply(true)(); }
                        // On a page already written: a template applied there is offered to be
                        // replaced; else the new one adds to the page, asked first
                        let lang = ctx.lang.get_untracked();
                        match applied_template(&page, &file).await {
                            Some(old) => ctx.ask_choice(
                                t("template_replace_ask", lang).replacen("{}", &old, 1).replacen("{}", &file.name, 1),
                                "template_add_at_cursor", apply(false),
                                "template_replace", apply(true),
                            ),
                            None => ctx.ask_confirm(t("template_confirm", lang).replace("{}", &file.name), apply(false)),
                        }
                    }
                    Err(e) => ctx.error("error", &e),
                }
            });
        },
    }
}

/// The template of the project (of the format of `new`) the page `page` holds already, if any.
async fn applied_template(page: &str, new: &FileEntry) -> Option<String> {
    let format = Format::of_page(&new.path);
    let mut templates = vec![];
    for f in crate::invoke::list_templates().await.ok()?.into_iter().filter(|f| Format::of_page(&f.path) == format) {
        if let Ok(text) = crate::invoke::read_file(&f.path).await { templates.push((f.name, text)); }
    }
    orgamnesia_core::templates::applied(page, &templates).map(str::to_string)
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

/// The application actions available here, whose name holds `q` (folded), each with
/// its shortcut; and the `templates` of the format of the page open.
fn commands(ctx: AppCtx, q: &str, templates: &[FileEntry]) -> Vec<QuickItem> {
    let lang = ctx.lang.get();
    let (graph, git, journal) = ctx.pref(|p| (p.graph, p.git_ext, p.journal));
    let mut list: Vec<QuickItem> = APP_ACTIONS.iter()
        // Opening this very list is no command
        .filter(|a| a.action != AppAction::QuickOpen
            && (a.action != AppAction::Graph || graph)
            && (!JOURNAL_ACTIONS.contains(&a.action) || journal)
            && (!GIT_ACTIONS.contains(&a.action) || git))
        .map(|a| QuickItem::Command { action: a.action, label: t(a.label_key, lang).to_string(), keys: ctx.shortcut(a.id) })
        // The editing ones only with a page open
        .chain(EDITOR_ACTIONS.iter()
            .filter(|a| PALETTE_EDITOR_ACTIONS.contains(&a.action) && ctx.work.active_tab_data().is_some())
            .map(|a| QuickItem::Edit { action: a.action, label: t(a.label_key, lang).to_string(), keys: ctx.shortcut(a.id) }))
        .chain(ctx.work.active_tab_data().into_iter().flat_map(|tab| {
            let format = Format::of_page(&tab.path);
            templates.iter()
                .filter(move |f| Format::of_page(&f.path) == format)
                .map(move |f| QuickItem::Template { file: f.clone(), label: format!("{}{}", t("quick_template", lang), f.name) })
        }))
        .filter(|item| q.is_empty() || fold(&item.display_name()).contains(q))
        .collect();
    if !q.is_empty() { list.sort_by_cached_key(|item| search_order(&item.display_name(), q)); }
    list
}

/// The `pages` found, then the `commands`, the page to create last.
fn with_commands(mut pages: Vec<QuickItem>, commands: Vec<QuickItem>) -> Vec<QuickItem> {
    let create = matches!(pages.last(), Some(QuickItem::Create { .. })).then(|| pages.pop()).flatten();
    pages.extend(commands);
    pages.extend(create);
    pages
}

/// What the quick open lists for `query`: the open `tabs` (`active` marked), the
/// other `files`, then the `tags` (when the tags frame is shown) and the page to
/// create; case and accents ignored ("regle" finds "Règle").
fn page_items(query: &str, tabs: &[Tab], active: Option<usize>, files: &[FileEntry], tags: Option<&[String]>) -> Vec<QuickItem> {
    let q = fold(query);
    let matches = |name: &str| q.is_empty() || fold(name).contains(&q);
    let mut list: Vec<QuickItem> = tabs.iter().enumerate()
        .filter(|(_, tab)| matches(&tab.name))
        .map(|(i, tab)| QuickItem::Tab { idx: i, name: tab.name.clone(), is_active: Some(i) == active })
        .collect();
    list.extend(files.iter()
        .filter(|file| matches(&file.name) && !tabs.iter().any(|t| t.path == file.path))
        .map(|file| QuickItem::File(file.clone())));
    if !q.is_empty() {
        list.sort_by_cached_key(|item| search_order(&item.display_name(), &q));
    }
    // `:pro` or `#pro` find the tag too
    let tag_q = q.trim_start_matches([':', '#']).trim_end_matches(':');
    if let Some(names) = tags.filter(|_| !tag_q.is_empty()) {
        let exact = names.iter().find(|n| fold(n) == tag_q).cloned();
        let found = crate::links::complete_page(names, tag_q, None, 8);
        list.extend(exact.into_iter().chain(found).map(QuickItem::Tag));
    }
    if !q.is_empty() {
        list.push(QuickItem::Create { name: query.to_string() });
    }
    list
}

#[component]
pub fn QuickOpenModal() -> impl IntoView {
    let ctx      = expect_context::<AppCtx>();
    let query    = RwSignal::new(String::new());
    let selected = RwSignal::new(0usize);
    let input_ref = NodeRef::<html::Input>::new();
    let list_ref = NodeRef::<html::Div>::new();
    // The project's templates, read each time the list opens (they are files anyone may change)
    let templates = RwSignal::new(Vec::<FileEntry>::new());
    if ctx.project.vault_path.get_untracked().is_some() {
        spawn_local(async move { if let Ok(found) = crate::invoke::list_templates().await { templates.set(found); } });
    }

    Effect::new(move |_| {
        if let Some(el) = input_ref.get() { let _ = el.focus(); }
    });

    let items = move || -> Vec<QuickItem> {
        let templates = templates.get();
        if let Some(rest) = query.get().strip_prefix('>') { return commands(ctx, &fold(rest.trim()), &templates); }
        let tabs = ctx.work.tabs.get();
        let tags = ctx.pref(|p| p.show_tags)
            .then(|| ctx.project.tags.with(|tags| tags.iter().map(|t| t.name.clone()).collect::<Vec<_>>()));
        let q = query.get();
        let pages = ctx.project.files.with(|files| page_items(&q, &tabs, ctx.work.active_tab.get(), files, tags.as_deref()));
        // The commands whose name holds what is typed too, before the page to create
        let q = fold(q.trim());
        if q.is_empty() { pages } else { with_commands(pages, commands(ctx, &q, &templates)) }
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
                        let empty = if query.get().starts_with('>') { "quick_open_no_command" } else { "quick_open_empty" };
                        view! {
                            <div class="quick-open-empty">
                                {t(empty, ctx.lang.get())}
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <div class="quick-open-list" node_ref=list_ref>
                                {list.into_iter().enumerate().map(|(i, item)| {
                                    let name   = item.display_name();
                                    let command = matches!(item, QuickItem::Command { .. } | QuickItem::Edit { .. } | QuickItem::Template { .. });
                                    let lang   = ctx.lang.get();
                                    let (icon, cls, hint): (&str, &str, Option<String>) = match &item {
                                        QuickItem::Tab { is_active: true,  .. } => ("●", "qo-tab",    None),
                                        QuickItem::Tab { is_active: false, .. } => ("○", "qo-tab",    None),
                                        QuickItem::File(_)                      => ("▫", "qo-file",   None),
                                        QuickItem::Tag(_)                       => ("#", "qo-tag",    Some(t("quick_open_tag", lang).to_string())),
                                        QuickItem::Create { .. }                => ("+", "qo-create", Some(t("quick_open_new", lang).to_string())),
                                        QuickItem::Command { keys, .. } | QuickItem::Edit { keys, .. } => ("›", "qo-cmd", keys.clone()),
                                        QuickItem::Template { .. }              => ("▤", "qo-cmd", None),
                                    };
                                    view! {
                                        <div
                                            class=move || format!("quick-open-item{}{}", if command { " qo-command" } else { "" }, if selected.get().min(last) == i { " selected" } else { "" })
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

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str) -> FileEntry {
        FileEntry { name: name.into(), path: format!("/p/{name}.org"), ..Default::default() }
    }

    fn names(items: &[QuickItem]) -> Vec<String> {
        items.iter().map(QuickItem::display_name).collect()
    }

    #[test]
    fn empty_query_lists_tabs_then_other_pages() {
        let tabs = [Tab::new(&file("B"), String::new(), false)];
        let items = page_items("", &tabs, Some(0), &[file("A"), file("B")], Some(&["tag".into()]));
        assert_eq!(names(&items), ["B", "A"]);
        assert!(matches!(items[0], QuickItem::Tab { idx: 0, is_active: true, .. }));
    }

    #[test]
    fn query_filters_sorts_and_offers_creation() {
        let files = [file("Mes règles"), file("Règle"), file("Autre")];
        assert_eq!(names(&page_items("regle", &[], None, &files, None)), ["Règle", "Mes règles", "regle"]);
    }

    #[test]
    fn commands_after_the_pages_before_the_page_to_create() {
        let command = QuickItem::Command { action: AppAction::Save, label: "Save".into(), keys: None };
        let items = with_commands(page_items("sa", &[], None, &[file("Salade")], None), vec![command]);
        assert_eq!(names(&items), ["Salade", "Save", "sa"]);
    }

    #[test]
    fn tags_when_shown() {
        let tags = ["projet".to_string(), "perso".to_string()];
        let found = names(&page_items(":projet:", &[], None, &[], Some(&tags)));
        assert_eq!(found.first().map(String::as_str), Some(":projet:"));
        assert_eq!(found.last().map(String::as_str), Some(":projet:"));
        assert!(names(&page_items("projet", &[], None, &[], None)).iter().all(|n| !n.starts_with(':')));
    }
}
