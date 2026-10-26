//! Search and replace in every page of the project: a pop-in over the focused
//! pane, at the height of the page title. What it shows is kept while it is
//! closed (`Ui::search`); the last replacement can be undone.

use leptos::{html, prelude::*};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

use crate::{
    actions,
    i18n::{t, Lang},
    invoke,
    keybindings::after_ms,
    state::{AppCtx, FileEntry, Goto, ReplaceOutcome, ReplaceSkip, SearchOptions, SearchResults},
};

/// "1 page", "3 pages": `n` and the word for it.
fn count(n: usize, one: &'static str, many: &'static str, lang: Lang) -> String {
    format!("{n} {}", t(if n > 1 { many } else { one }, lang))
}

/// Matches and pages a replacement will change, given what is left out.
fn included(results: &SearchResults, skip: &ReplaceSkip) -> (usize, usize) {
    let mut hits = 0;
    let mut pages = 0;
    for page in results.pages.iter().filter(|p| !skip.pages.contains(&p.path)) {
        let left = skip.hits.iter().filter(|(path, _)| *path == page.path).count();
        if page.count > left {
            hits += page.count - left;
            pages += 1;
        }
    }
    (hits, pages)
}

/// The text of the pages with unsaved changes: searched instead of the saved one.
fn edited_pages(ctx: AppCtx) -> Vec<(String, String)> {
    ctx.work.tabs.get_untracked().into_iter()
        .filter(|t| t.dirty.get_untracked())
        .map(|t| (t.path.clone(), t.content.get_untracked()))
        .collect()
}

/// Search again with what the pop-in holds.
fn run_search(ctx: AppCtx) {
    let s = ctx.ui.search;
    let query = s.query.get_untracked();
    if query.is_empty() {
        s.results.set(SearchResults::default());
        return;
    }
    let options = s.options.get_untracked();
    let open = edited_pages(ctx);
    spawn_local(async move {
        match invoke::search_project(&query, options, &open).await {
            // Only if it is still the search asked for
            Ok(r) => if s.query.get_untracked() == query && s.options.get_untracked() == options {
                s.results.set(r);
            },
            Err(e) => ctx.error("error", &e),
        }
    });
}

/// Put the editor of the focused pane back under the keyboard.
pub fn focus_editor() {
    let area = web_sys::window().and_then(|w| w.document())
        .and_then(|d| d.query_selector(".editor-area.focused textarea.edit-layer").ok().flatten())
        .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok());
    if let Some(el) = area { let _ = el.focus(); }
}

fn close(ctx: AppCtx) {
    ctx.ui.search.open.set(None);
    focus_editor();
}

/// Replace the matches not left out, after confirmation. The pages with unsaved
/// changes are written first, as the replacement is made on disk.
fn replace_all(ctx: AppCtx) {
    let s = ctx.ui.search;
    let lang = ctx.lang.get_untracked();
    let (query, with, options, skip) = (
        s.query.get_untracked(), s.replacement.get_untracked(), s.options.get_untracked(), s.skip.get_untracked(),
    );
    let (hits, pages) = s.results.with_untracked(|r| included(r, &skip));
    if query.is_empty() || hits == 0 { return; }
    let message = format!(
        "{} « {query} » → « {with} » : {}, {}. {}",
        t("ps_replace_confirm", lang),
        count(hits, "ps_hit", "ps_hits", lang),
        count(pages, "ps_page", "ps_pages", lang),
        t("ps_replace_saves", lang),
    );
    ctx.ask_confirm(message, move || {
        let (query, with, skip) = (query.clone(), with.clone(), skip.clone());
        spawn_local(async move {
            if !actions::save_dirty_tabs(ctx).await { return; }
            match invoke::replace_in_project(&query, options, &with, &skip).await {
                Ok(done) => {
                    after_change(ctx).await;
                    s.can_undo.set(true);
                    let lang = ctx.lang.get_untracked();
                    let msg = format!("{} : {}, {}", t("ps_replaced", lang),
                        count(done.replacements, "ps_hit", "ps_hits", lang),
                        count(done.pages, "ps_page", "ps_pages", lang));
                    ctx.notify_undo(msg, move || undo(ctx));
                }
                Err(e) => {
                    // Part of it may have been written: that part can be undone
                    after_change(ctx).await;
                    s.can_undo.set(invoke::can_undo_replace().await.unwrap_or(false));
                    ctx.error("error", &e);
                }
            }
        });
    });
}

/// Put the pages of the last project replacement back as they were.
pub fn undo(ctx: AppCtx) {
    spawn_local(async move {
        if !actions::save_dirty_tabs(ctx).await { return; }
        match invoke::undo_replace().await {
            Ok(ReplaceOutcome { pages, skipped, .. }) => {
                after_change(ctx).await;
                ctx.ui.search.can_undo.set(false);
                let lang = ctx.lang.get_untracked();
                let mut msg = format!("{} : {}", t("ps_undone", lang), count(pages, "ps_page", "ps_pages", lang));
                if skipped > 0 {
                    msg.push_str(&format!(" ({} {})", count(skipped, "ps_page", "ps_pages", lang), t("ps_undo_skipped", lang)));
                }
                ctx.notify(msg);
            }
            Err(e) => ctx.error("error", &e),
        }
    });
}

/// After pages were rewritten on disk: the open tabs, the links and the results follow.
async fn after_change(ctx: AppCtx) {
    actions::reload_clean_tabs(ctx).await;
    ctx.bump_links();
    run_search(ctx);
}

/// The pop-in, shown in the focused pane while `Ui::search.open` is set.
#[component]
pub fn ProjectSearchPopin() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();
    let s = ctx.ui.search;
    let with_replace = move || s.open.get() == Some(true);
    let query_ref = NodeRef::<html::Input>::new();
    let replace_ref = NodeRef::<html::Input>::new();
    let list_ref = NodeRef::<html::Div>::new();
    // The match chosen with the arrows, among all the matches shown
    let selected = RwSignal::new(0usize);

    // The field of the shortcut used gets the keyboard, its text selected
    Effect::new(move |_| {
        let field = if with_replace() && !s.query.get_untracked().is_empty() { replace_ref.get() } else { query_ref.get() };
        if let Some(el) = field {
            let _ = el.focus();
            el.select();
        }
    });
    spawn_local(async move {
        if let Ok(can) = invoke::can_undo_replace().await { s.can_undo.try_set(can); }
    });

    // Search as the text is typed (once it rests), and again when pages change
    let generation = StoredValue::new(0u32);
    Effect::new(move |first: Option<()>| {
        s.query.track();
        s.options.track();
        ctx.project.links_version.track();
        if first.is_none() && !s.results.with_untracked(|r| r.pages.is_empty()) {
            // Opened again: the results kept are shown, then refreshed
            run_search(ctx);
            return;
        }
        generation.update_value(|g| *g += 1);
        let g = generation.get_value();
        after_ms(200, move || {
            if generation.try_get_value() == Some(g) { run_search(ctx); }
        });
    });
    // A new search: nothing left out, the first match chosen
    Effect::new(move |prev: Option<(String, SearchOptions)>| {
        let now = (s.query.get(), s.options.get());
        if prev.is_some_and(|p| p != now) {
            s.skip.set(ReplaceSkip::default());
            selected.set(0);
        }
        now
    });

    let flat = move || s.results.with(|r| {
        r.pages.iter().flat_map(|p| p.hits.iter().map(move |h| (p.page.clone(), p.path.clone(), h.start, h.end)))
            .collect::<Vec<_>>()
    });
    let open_hit = move |page: String, path: String, start: usize, end: usize| {
        ctx.ui.search.open.set(None);
        actions::open_file(ctx, FileEntry { name: page, path }, Some(Goto::Range(start, end)));
    };
    let open_selected = move || {
        if let Some((page, path, a, b)) = flat().into_iter().nth(selected.get_untracked()) {
            open_hit(page, path, a, b);
        }
    };
    let move_selection = move |down: bool| {
        let n = s.results.with_untracked(|r| r.pages.iter().map(|p| p.hits.len()).sum::<usize>());
        if n == 0 { return; }
        selected.update(|i| *i = if down { (*i + 1).min(n - 1) } else { i.saturating_sub(1) });
        // Keep the chosen match in sight
        if let Some(list) = list_ref.get_untracked()
            && let Ok(Some(el)) = list.query_selector(".ps-hit.selected")
        {
            el.scroll_into_view_with_bool(false);
        }
    };
    let on_key = move |e: web_sys::KeyboardEvent, in_replace: bool| {
        let plain = !e.ctrl_key() && !e.alt_key() && !e.meta_key();
        match e.key().as_str() {
            "Escape" => close(ctx),
            "ArrowDown" if plain => move_selection(true),
            "ArrowUp" if plain => move_selection(false),
            "Enter" if plain && in_replace => replace_all(ctx),
            "Enter" if plain => open_selected(),
            _ => return,
        }
        e.prevent_default();
        e.stop_propagation();
    };

    let toggle_page = move |path: String| s.skip.update(|k| {
        if let Some(i) = k.pages.iter().position(|p| *p == path) { k.pages.remove(i); } else { k.pages.push(path); }
    });
    let toggle_hit = move |path: String, start: usize| s.skip.update(|k| {
        let hit = (path, start);
        if let Some(i) = k.hits.iter().position(|h| *h == hit) { k.hits.remove(i); } else { k.hits.push(hit); }
    });

    let summary = move || {
        let lang = lang();
        s.results.with(|r| {
            if s.query.with(|q| q.is_empty()) { return String::new(); }
            if r.total == 0 { return t("find_none", lang).to_string(); }
            let mut text = format!("{} · {}", count(r.total, "ps_hit", "ps_hits", lang), count(r.pages.len(), "ps_page", "ps_pages", lang));
            if r.truncated { text.push_str(&format!(" · {}", t("ps_truncated", lang))); }
            text
        })
    };

    view! {
        <div class="ps-backdrop" on:mousedown=move |_| close(ctx) />
        <div
            class="title-popin project-search"
            role="dialog"
            aria-label=move || t("shortcut_project_search", lang())
            // Escape from anywhere in it (a checkbox, a button), not only its fields
            on:keydown=move |e: web_sys::KeyboardEvent| if e.key() == "Escape" {
                e.stop_propagation();
                close(ctx);
            }
        >
            <div class="find-row">
                <button
                    class=move || if with_replace() { "find-btn find-toggle open" } else { "find-btn find-toggle" }
                    title=move || ctx.with_shortcut("find_replace_toggle", "project_replace")
                    on:click=move |_| s.open.set(Some(!with_replace()))
                >"›"</button>
                <input
                    node_ref=query_ref
                    class="find-input ps-input"
                    type="text"
                    placeholder=move || t("ps_placeholder", lang())
                    prop:value=move || s.query.get()
                    on:input=move |e| s.query.set(event_target_value(&e))
                    on:keydown=move |e| on_key(e, false)
                />
                <button
                    class=move || if s.options.get().match_case { "find-btn active" } else { "find-btn" }
                    title=move || t("ps_match_case", lang())
                    on:click=move |_| s.options.update(|o| o.match_case = !o.match_case)
                >"Aa"</button>
                <button
                    class=move || if s.options.get().whole_word { "find-btn active" } else { "find-btn" }
                    title=move || t("ps_whole_word", lang())
                    on:click=move |_| s.options.update(|o| o.whole_word = !o.whole_word)
                >"ab"</button>
                <button class="find-btn" title=move || t("close", lang()) on:click=move |_| close(ctx)>"×"</button>
            </div>
            {move || with_replace().then(|| view! {
                <div class="find-row find-replace-row">
                    <input
                        node_ref=replace_ref
                        class="find-input ps-input"
                        type="text"
                        placeholder=move || t("replace_placeholder", lang())
                        prop:value=move || s.replacement.get()
                        on:input=move |e| s.replacement.set(event_target_value(&e))
                        on:keydown=move |e| on_key(e, true)
                    />
                    <button
                        class="find-btn find-text-btn"
                        prop:disabled=move || s.results.with(|r| s.skip.with(|k| included(r, k).0 == 0))
                        on:click=move |_| replace_all(ctx)
                    >
                        {move || {
                            let n = s.results.with(|r| s.skip.with(|k| included(r, k).0));
                            format!("{} ({n})", t("ps_replace_all", lang()))
                        }}
                    </button>
                </div>
            })}
            <div class="ps-status">
                <span>{summary}</span>
                {move || s.can_undo.get().then(|| view! {
                    <button class="find-btn find-text-btn" on:click=move |_| undo(ctx)>
                        {move || t("ps_undo", lang())}
                    </button>
                })}
            </div>
            <div class="ps-results" node_ref=list_ref>
                {move || {
                    let replacing = with_replace();
                    let replacement = s.replacement.get();
                    let mut index = 0;
                    s.results.get().pages.into_iter().map(|page| {
                        let path = page.path.clone();
                        let page_path = path.clone();
                        let page_left_out = {
                            let path = path.clone();
                            move || s.skip.with(|k| k.pages.contains(&path))
                        };
                        let hits = page.hits.into_iter().map(|hit| {
                            let i = index;
                            index += 1;
                            let (name, hit_path, click_path) = (page.page.clone(), path.clone(), path.clone());
                            let left_out = {
                                let path = path.clone();
                                let page_left_out = page_left_out.clone();
                                move || page_left_out() || s.skip.with(|k| k.hits.contains(&(path.clone(), hit.start)))
                            };
                            let left_out_class = left_out.clone();
                            let (start, end) = (hit.start, hit.end);
                            let replacement = replacement.clone();
                            view! {
                                <div
                                    class=move || {
                                        let mut c = String::from("ps-hit");
                                        if selected.get() == i { c.push_str(" selected"); }
                                        if replacing && left_out_class() { c.push_str(" left-out"); }
                                        c
                                    }
                                    on:click=move |_| open_hit(name.clone(), click_path.clone(), start, end)
                                >
                                    {replacing.then(|| view! {
                                        <input
                                            type="checkbox"
                                            prop:checked=move || !left_out()
                                            on:click=|e: web_sys::MouseEvent| e.stop_propagation()
                                            on:change=move |_| toggle_hit(hit_path.clone(), start)
                                        />
                                    })}
                                    <span class="ps-line">{hit.line + 1}</span>
                                    <span class="ps-text">
                                        {hit.before}
                                        {if replacing {
                                            view! { <del>{hit.matched}</del><ins>{replacement}</ins> }.into_any()
                                        } else {
                                            view! { <mark>{hit.matched}</mark> }.into_any()
                                        }}
                                        {hit.after}
                                    </span>
                                </div>
                            }
                        }).collect_view();
                        view! {
                            <div class="ps-page">
                                {replacing.then(|| view! {
                                    <input
                                        type="checkbox"
                                        prop:checked=move || !page_left_out()
                                        on:change=move |_| toggle_page(page_path.clone())
                                    />
                                })}
                                <span class="ps-page-name">{page.page.clone()}</span>
                                <span class="ps-page-count">{page.count}</span>
                            </div>
                            {hits}
                        }
                    }).collect_view()
                }}
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orgamnesia_core::PageHits;

    fn page(path: &str, count: usize) -> PageHits {
        PageHits { page: path.into(), path: path.into(), hits: vec![], count }
    }

    #[test]
    fn counts_what_a_replacement_changes() {
        let r = SearchResults { pages: vec![page("a", 3), page("b", 1), page("c", 2)], total: 6, truncated: false };
        assert_eq!(included(&r, &ReplaceSkip::default()), (6, 3));
        let skip = ReplaceSkip { pages: vec!["c".into()], hits: vec![("a".into(), 0), ("b".into(), 4)] };
        assert_eq!(included(&r, &skip), (2, 1));
    }
}
