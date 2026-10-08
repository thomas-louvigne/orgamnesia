use leptos::prelude::*;

use crate::{actions, i18n::t, state::{AppCtx, Tab}};

/// Value of the page's `#+TITLE:` keyword, if it has a non-empty one.
pub fn org_title(content: &str) -> Option<String> {
    content.lines().find_map(title_value).map(str::trim).filter(|v| !v.is_empty()).map(String::from)
}

/// The text after `#+TITLE:` (any case) when `line` is that keyword.
fn title_value(line: &str) -> Option<&str> {
    let line = line.trim_start();
    let key = line.get(..8)?;
    key.eq_ignore_ascii_case("#+title:").then(|| &line[8..])
}

/// `content` with its `#+TITLE:` set to `title`: the keyword is rewritten in
/// place, or added as the first line when the page has none.
pub fn set_org_title(content: &str, title: &str) -> String {
    let mut done = false;
    let mut out: Vec<String> = content.split('\n').map(|line| {
        if !done && title_value(line).is_some() {
            done = true;
            let indent = &line[..line.len() - line.trim_start().len()];
            let key = &line.trim_start()[..8];
            format!("{indent}{key} {title}")
        } else {
            line.to_string()
        }
    }).collect();
    if !done {
        out.insert(0, format!("#+TITLE: {title}"));
    }
    out.join("\n")
}

/// What the title's inline field is editing.
#[derive(Clone, Copy, PartialEq)]
enum EditMode {
    /// The file name.
    Rename,
    /// The `#+TITLE:` keyword.
    Title,
}

/// Name of the page in large type above the editor: its `#+TITLE:` when it
/// has one, else its name. Right-click to rename the file or set the title.
#[component]
pub fn PageTitle(tab: Tab) -> impl IntoView {
    let ctx = use_context::<AppCtx>().expect("AppCtx");
    let lang = move || ctx.lang.get();
    let content = tab.content;
    let title = Memo::new(move |_| content.with(|c| org_title(c)));

    let menu = RwSignal::new(None::<(i32, i32)>);
    let editing = RwSignal::new(None::<EditMode>);
    let value = RwSignal::new(String::new());
    let input_ref = NodeRef::<leptos::html::Input>::new();

    Effect::new(move |_| {
        if editing.get().is_some() && let Some(el) = input_ref.get() {
            let _ = el.focus();
            el.select();
        }
    });

    let path = tab.path.clone();
    let name = tab.name.clone();
    let commit = move || {
        let Some(mode) = editing.get_untracked() else { return };
        editing.set(None);
        let v = value.get_untracked().trim().to_string();
        match mode {
            EditMode::Rename => actions::rename_page(ctx, path.clone(), v),
            EditMode::Title => {
                if v.is_empty() || title.get_untracked().as_deref() == Some(v.as_str()) { return; }
                tab.dirty.set(true);
                content.update(|c| *c = set_org_title(c, &v));
            }
        }
    };
    let start = move |mode: EditMode| {
        menu.set(None);
        value.set(match mode {
            EditMode::Rename => name.clone(),
            EditMode::Title => title.get_untracked().unwrap_or_else(|| name.clone()),
        });
        editing.set(Some(mode));
    };
    let start_rename = start.clone();
    let shown = {
        let name = tab.name.clone();
        move || title.get().unwrap_or_else(|| name.clone())
    };

    view! {
        <div
            class="page-title"
            on:contextmenu=move |e: web_sys::MouseEvent| {
                e.prevent_default();
                menu.set(Some((e.client_x(), e.client_y())));
            }
        >
            {move || if editing.get().is_some() {
                let commit_blur = commit.clone();
                let commit_key = commit.clone();
                view! {
                    <input
                        node_ref=input_ref
                        class="page-title-input"
                        type="text"
                        prop:value=move || value.get()
                        on:input=move |e| value.set(event_target_value(&e))
                        on:keydown=move |e: web_sys::KeyboardEvent| {
                            e.stop_propagation();
                            match e.key().as_str() {
                                "Enter" => commit_key(),
                                "Escape" => editing.set(None),
                                _ => {}
                            }
                        }
                        on:blur=move |_| commit_blur()
                    />
                }.into_any()
            } else {
                view! { <h1 class="page-title-text">{shown.clone()}</h1> }.into_any()
            }}
            {move || menu.get().map(|(x, y)| {
                let start_rename = start_rename.clone();
                let start_title = start.clone();
                view! {
                    <div
                        class="ctx-overlay"
                        on:click=move |_| menu.set(None)
                        on:contextmenu=move |e: web_sys::MouseEvent| {
                            e.prevent_default();
                            e.stop_propagation();
                            menu.set(None);
                        }
                    />
                    <div class="ctx-menu" style=format!("left:{x}px;top:{y}px")>
                        <button class="ctx-menu-item" on:click=move |_| start_rename(EditMode::Rename)>
                            {move || t("rename_file", lang())}
                        </button>
                        <button class="ctx-menu-item" on:click=move |_| start_title(EditMode::Title)>
                            {move || if title.get().is_some() {
                                t("edit_title_tag", lang())
                            } else {
                                t("add_title_tag", lang())
                            }}
                        </button>
                    </div>
                }
            })}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_title_keyword() {
        assert_eq!(org_title("#+TITLE: Mon titre\n* A"), Some("Mon titre".into()));
        assert_eq!(org_title("* A\n  #+title:  Bas  \n"), Some("Bas".into()));
        assert_eq!(org_title("#+TITLE:\n* A"), None);
        assert_eq!(org_title("* #+TITLE: x"), None);
        assert_eq!(org_title(""), None);
    }

    #[test]
    fn sets_the_title_keyword() {
        assert_eq!(set_org_title("* A", "T"), "#+TITLE: T\n* A");
        assert_eq!(set_org_title("", "T"), "#+TITLE: T\n");
        assert_eq!(set_org_title("x\n#+title: old\n#+TITLE: 2", "New"), "x\n#+title: New\n#+TITLE: 2");
    }
}
