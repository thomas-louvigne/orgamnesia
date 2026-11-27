//! The pages of the project, as a tree of their folders (see `orgamnesia_core::tree`).

use std::collections::HashSet;

use leptos::prelude::*;
use orgamnesia_core::tree::{self, Row};

use crate::{
    actions,
    i18n::t,
    state::{AppCtx, FileEntry},
};

/// The pages of the project, with their filter.
#[component]
pub fn PagesFrame() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let filter = RwSignal::new(String::new());
    // Right-click menu: position, and what was clicked (a page, a folder, or the empty area)
    let ctx_menu = RwSignal::new(None::<(i32, i32, Target)>);
    // The folders folded, kept for each project; the journal's folded at first
    let closed = RwSignal::new(HashSet::<String>::new());
    let closed_key = move || ctx.project.vault_path.get_untracked().map(|v| format!("pages-closed:{v}"));
    Effect::new(move |_| {
        ctx.project.vault_path.track();
        let stored = closed_key().and_then(|k| crate::storage::load(&k));
        closed.set(match stored {
            Some(s) => s.lines().filter(|l| !l.is_empty()).map(str::to_string).collect(),
            None => HashSet::from([tree::JOURNAL.to_string()]),
        });
    });
    let toggle = move |key: String| {
        closed.update(|c| if !c.remove(&key) { c.insert(key); });
        if let Some(k) = closed_key() {
            crate::storage::store(&k, &closed.with_untracked(|c| c.iter().cloned().collect::<Vec<_>>().join("\n")));
        }
    };
    // Path of the file being renamed inline, and the text in the input
    let renaming     = RwSignal::new(None::<String>);
    let rename_value = RwSignal::new(String::new());
    let rename_ref   = NodeRef::<leptos::html::Input>::new();

    Effect::new(move |_| {
        if renaming.get().is_some() && let Some(el) = rename_ref.get() {
            let _ = el.focus();
            el.select();
        }
    });

    let do_rename = move || {
        let Some(old_path) = renaming.get_untracked() else { return };
        renaming.set(None);
        actions::rename_page(ctx, old_path, rename_value.get_untracked());
    };

    // The page being dragged with the mouse, dropped on a folder (or a page: its
    // folder; the empty area: the top). Mouse events, not HTML5 drag: Tauri's file
    // drop takes it on Windows.
    let dragged = RwSignal::new(None::<FileEntry>);
    let drop_in = move |folder: &str| {
        let Some(file) = dragged.get_untracked() else { return };
        dragged.set(None);
        if actions::folder_of(&file.name) == folder { return; }
        let leaf = file.name.rsplit('/').next().unwrap_or(&file.name);
        let name = if folder.is_empty() { leaf.to_string() } else { format!("{folder}/{leaf}") };
        actions::rename_page(ctx, file.path, name);
    };

    let rows = move || {
        let journal = t("journal_folder_label", ctx.lang.get());
        ctx.project.files.with(|files| ctx.project.folders.with(|folders| closed.with(|closed| {
            tree::rows(files, folders, Some(journal), closed, &filter.get())
        })))
    };

    let active_path = Memo::new(move |_| ctx.work.active_tab_data().map(|t| t.path));
    // Pages with a twin (a page of the same name in the other format), and among
    // them those their name alone doesn't lead to: struck through
    let twins = Memo::new(move |_| {
        ctx.project.files.with(|fs| ctx.project.resolver.with(|r| {
            fs.iter().filter(|f| r.has_twin(f)).map(|f| (f.path.clone(), r.shadowed(f))).collect::<std::collections::HashMap<String, bool>>()
        }))
    });
    // Pages differing from the remote (git extension): marked, to push
    let unpushed = Memo::new(move |_| {
        if !ctx.pref(|p| p.git_ext && p.git_unpushed_marker) || ctx.project.preference.get() { return std::collections::HashSet::new(); }
        ctx.project.git.with(|g| g.iter().flat_map(|g| g.unpushed.iter().cloned()).collect::<std::collections::HashSet<String>>())
    });

    let trigger_new_page = move || {
        if ctx.project.vault_path.get_untracked().is_none() {
            ctx.ui.status.set(Some(t("no_vault", ctx.lang.get_untracked()).to_string()));
            return;
        }
        ctx.ui.show_new_page.set(true);
    };

    view! {
        <nav class="frame frame-grow pages-frame" data-frame="pages" aria-label=move || t("pages", ctx.lang.get())>
            <div class="sidebar-header">
                <span class="sidebar-title">{move || t("pages", ctx.lang.get())}</span>
                // The configuration files can't be created, renamed nor deleted
                {move || (!ctx.project.preference.get()).then(|| view! {
                    <button
                        class="btn-icon"
                        data-shortcut="new_page"
                        title={move || ctx.with_shortcut("new_page_title", "new_page")}
                        aria-label={move || t("new_page_title", ctx.lang.get())}
                        on:click=move |_| trigger_new_page()
                    >"+"</button>
                })}
            </div>
            <div class="sidebar-search">
                <input
                    type="text"
                    class="search-input"
                    aria-label={move || t("filter_hint", ctx.lang.get())}
                    placeholder={move || t("filter_hint", ctx.lang.get())}
                    prop:value=move || filter.get()
                    on:input=move |e| filter.set(event_target_value(&e))
                />
            </div>
            <div
                class=move || if dragged.get().is_some() { "sidebar-list pages-dragging" } else { "sidebar-list" }
                on:mouseup=move |_| drop_in("")
                on:mouseleave=move |_| dragged.set(None)
                on:contextmenu=move |e: web_sys::MouseEvent| {
                    e.prevent_default();
                    if !ctx.project.preference.get_untracked() { ctx_menu.set(Some((e.client_x(), e.client_y(), Target::Empty))); }
                }
            >
                <For
                    each=rows
                    key={|row| match row {
                        Row::Folder { key, open, pages, .. } => format!("{key}\u{0}{open}{pages}"),
                        Row::Page { file, depth, .. } => format!("{}\u{0}{depth}", file.path),
                    }}
                    let:row
                >
                    {match row {
                    Row::Folder { key, name, depth, open, pages } => {
                        let key_click = key.clone();
                        let key_drop = key.clone();
                        let is_journal = key == tree::JOURNAL;
                        view! {
                            <div
                                class="file-item list-item folder-item"
                                role="button"
                                tabindex="0"
                                aria-expanded=open.to_string()
                                style=format!("padding-left:{}px", 8 + depth * 14)
                                on:keydown=crate::a11y::list_item_keys
                                on:click=move |_| toggle(key_click.clone())
                                on:mouseup=move |e: web_sys::MouseEvent| {
                                    e.stop_propagation();
                                    if is_journal { dragged.set(None) } else { drop_in(&key_drop) }
                                }
                                on:contextmenu=move |e: web_sys::MouseEvent| {
                                    e.prevent_default();
                                    e.stop_propagation();
                                    if !ctx.project.preference.get_untracked() {
                                        let target = if is_journal { Target::Empty } else { Target::Folder(key.clone()) };
                                        ctx_menu.set(Some((e.client_x(), e.client_y(), target)));
                                    }
                                }
                            >
                                <span class="folder-arrow">{if open { "▾" } else { "▸" }}</span>
                                <span class="file-name">{name}</span>
                                <span class="folder-count">{pages}</span>
                            </div>
                        }.into_any()
                    }
                    Row::Page { file, name: leaf, depth } => {
                        let path = file.path.clone();
                        let path_r = file.path.clone();
                        let path_c = file.path.clone();
                        let path_u = file.path.clone();
                        let path_t = file.path.clone();
                        let path_s = file.path.clone();
                        let link = orgamnesia_core::pages::file_name(&file.path).to_string();
                        let name = leaf;
                        let file_open = file.clone();
                        let file_menu = file.clone();
                        let file_drag = file.clone();
                        let page_folder = actions::folder_of(&file.name).to_string();
                        view! {
                            <div
                                class=move || if active_path.get().as_deref() == Some(path.as_str()) {
                                    "file-item list-item active"
                                } else { "file-item list-item" }
                                style=format!("padding-left:{}px", 8 + depth * 14)
                                role="button"
                                tabindex="0"
                                aria-current=move || (active_path.get().as_deref() == Some(path_c.as_str())).then_some("page")
                                on:keydown=crate::a11y::list_item_keys
                                on:mousedown=move |e: web_sys::MouseEvent| {
                                    if e.button() == 0 && renaming.get_untracked().is_none() && !ctx.project.preference.get_untracked() {
                                        dragged.set(Some(file_drag.clone()));
                                    }
                                }
                                on:mouseup=move |e: web_sys::MouseEvent| {
                                    e.stop_propagation();
                                    drop_in(&page_folder);
                                }
                                on:click=move |_| {
                                    if renaming.get_untracked().is_none() {
                                        actions::open_file(ctx, file_open.clone(), None);
                                    }
                                }
                                on:contextmenu=move |e: web_sys::MouseEvent| {
                                    e.prevent_default();
                                    e.stop_propagation();
                                    if !ctx.project.preference.get_untracked() {
                                        ctx_menu.set(Some((e.client_x(), e.client_y(), Target::Page(file_menu.clone()))));
                                    }
                                }
                            >
                                {move || if renaming.get().as_deref() == Some(path_r.as_str()) {
                                    view! {
                                        <input
                                            node_ref=rename_ref
                                            class="search-input"
                                            type="text"
                                            prop:value=move || rename_value.get()
                                            on:input=move |e| rename_value.set(event_target_value(&e))
                                            on:click=|e: web_sys::MouseEvent| e.stop_propagation()
                                            on:keydown=move |e: web_sys::KeyboardEvent| {
                                                e.stop_propagation();
                                                match e.key().as_str() {
                                                    "Enter" => do_rename(),
                                                    "Escape" => renaming.set(None),
                                                    _ => {}
                                                }
                                            }
                                            on:blur=move |_| do_rename()
                                        />
                                    }.into_any()
                                } else {
                                    let path_s = path_s.clone();
                                    let shadowed = Signal::derive(move || twins.with(|t| t.get(&path_s) == Some(&true)));
                                    let link = link.clone();
                                    let name = name.clone();
                                    view! {
                                        <span
                                            class=move || if shadowed.get() { "file-name file-shadowed" } else { "file-name" }
                                            title=move || shadowed.get().then(|| format!("{} [[{}]]", t("twin_shadowed", ctx.lang.get()), link))
                                        >{name}</span>
                                    }.into_any()
                                }}
                                {move || twins.with(|t| t.contains_key(&path_t)).then(|| {
                                    let ext = orgamnesia_core::Format::of_path(&path_t).map_or("", |f| f.ext());
                                    view! { <span class="file-ext">{format!(".{ext}")}</span> }
                                })}
                                {move || unpushed.with(|u| u.contains(&path_u)).then(|| view! {
                                    <span class="file-unpushed"
                                        title=move || t("git_unpushed_file", ctx.lang.get())
                                        aria-label=move || t("git_unpushed_file", ctx.lang.get())>"↑"</span>
                                })}
                            </div>
                        }.into_any()
                    }
                    }}
                </For>
            </div>

            {move || ctx_menu.get().map(|(x, y, menu)| {
                let close = move || ctx_menu.set(None);
                let target = match &menu { Target::Page(f) => Some(f.clone()), _ => None };
                let folder = match &menu { Target::Folder(f) => Some(f.clone()), _ => None };
                view! {
                    <div
                        class="ctx-overlay"
                        on:click=move |_| close()
                        on:contextmenu=move |e: web_sys::MouseEvent| {
                            e.prevent_default();
                            close();
                        }
                    />
                    <div class="ctx-menu" style=format!("left:{x}px;top:{y}px")>
                        {target.clone().map(|f| view! {
                            <button class="ctx-menu-item" on:click=move |_| {
                                close();
                                rename_value.set(f.name.clone());
                                renaming.set(Some(f.path.clone()));
                            }>
                                {move || t("rename", ctx.lang.get())}
                            </button>
                        })}
                        {target.clone().map(|f| view! {
                            <button class="ctx-menu-item" on:click=move |_| {
                                close();
                                actions::move_page(ctx, f.clone());
                            }>
                                {move || t("move_page_menu", ctx.lang.get())}
                            </button>
                        })}
                        {folder.clone().map(|f| {
                            let (f1, f2, f3) = (f.clone(), f.clone(), f.clone());
                            view! {
                                <button class="ctx-menu-item" on:click=move |_| { close(); actions::new_page_in(ctx, f1.clone()); }>
                                    {move || t("folder_new_page", ctx.lang.get())}
                                </button>
                                <button class="ctx-menu-item" on:click=move |_| { close(); actions::new_folder(ctx, Some(f2.clone())); }>
                                    {move || t("folder_new_sub", ctx.lang.get())}
                                </button>
                                <button class="ctx-menu-item" on:click=move |_| { close(); actions::rename_folder(ctx, Some(f3.clone())); }>
                                    {move || t("folder_rename", ctx.lang.get())}
                                </button>
                                <div class="ctx-menu-sep" role="separator" />
                                <button class="ctx-menu-item ctx-menu-danger" on:click=move |_| { close(); actions::delete_folder(ctx, Some(f.clone())); }>
                                    {move || t("folder_delete", ctx.lang.get())}
                                </button>
                            }
                        })}
                        {target.clone().map(|f| view! {
                            <button
                                class="ctx-menu-item"
                                title=move || t("no_export_title", ctx.lang.get())
                                on:click=move |_| {
                                    close();
                                    actions::tag_page(ctx, f.clone(), "noexport");
                                }
                            >
                                {move || t("no_export", ctx.lang.get())}
                            </button>
                        })}
                        {target.clone().filter(|_| crate::git::history_available(ctx)).map(|f| view! {
                            <button class="ctx-menu-item" on:click=move |_| {
                                close();
                                crate::git::open_history(ctx, f.clone());
                            }>
                                {move || t("git_history_menu", ctx.lang.get())}
                            </button>
                        })}
                        {target.clone().map(|f| view! {
                            <div class="ctx-menu-sep" role="separator" />
                            {[(false, "export_pdf_menu"), (true, "export_html_menu")].map(|(html, label)| {
                                let f = f.clone();
                                view! {
                                    <button class="ctx-menu-item" on:click=move |_| {
                                        close();
                                        crate::export::export_file(ctx, f.clone(), html);
                                    }>
                                        {move || t(label, ctx.lang.get())}
                                    </button>
                                }
                            })}
                            <div class="ctx-menu-sep" role="separator" />
                        })}
                        {folder.is_none().then(|| view! {
                            <button class="ctx-menu-item" on:click=move |_| {
                                close();
                                trigger_new_page();
                            }>
                                <span>{move || t("new_page_title", ctx.lang.get())}</span>
                                {move || ctx.shortcut("new_page").map(|k| view! { <span class="ctx-menu-key">{k}</span> })}
                            </button>
                            <button class="ctx-menu-item" on:click=move |_| {
                                close();
                                actions::new_folder(ctx, None);
                            }>
                                {move || t("shortcut_new_folder", ctx.lang.get())}
                            </button>
                        })}
                        // Deleting comes last, set apart
                        {target.map(|f| view! {
                            <div class="ctx-menu-sep" role="separator" />
                            <button class="ctx-menu-item ctx-menu-danger" on:click=move |_| {
                                close();
                                actions::delete_page(ctx, f.clone());
                            }>
                                {move || t("delete_page", ctx.lang.get())}
                            </button>
                        })}
                    </div>
                }
            })}
        </nav>
    }
}

/// What a right click in the list was on.
#[derive(Clone)]
enum Target {
    Page(FileEntry),
    /// A folder of the pages, by its path (`Jeux/Quêtes`).
    Folder(String),
    /// The empty area, or the journal's folder.
    Empty,
}
