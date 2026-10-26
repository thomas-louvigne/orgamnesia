//! Git extension: the state of the project and the commit, pull and push buttons.

use leptos::prelude::*;

use crate::{
    i18n::t,
    state::AppCtx,
};

/// Git logo (simple-icons, CC0), drawn in the current text color.
#[component]
fn GitIcon() -> impl IntoView {
    view! {
        <svg class="git-logo" viewBox="0 0 24 24" aria-hidden="true">
            <path fill="currentColor" d="M23.546 10.93L13.067.452c-.604-.603-1.582-.603-2.188 0L8.708 2.627l2.76 2.76c.645-.215 1.379-.07 1.889.441.516.515.658 1.258.438 1.9l2.658 2.66c.645-.223 1.387-.078 1.9.435.721.72.721 1.884 0 2.604-.719.719-1.881.719-2.6 0-.539-.541-.674-1.337-.404-1.996L12.86 8.955v6.525c.176.086.342.203.488.348.713.721.713 1.883 0 2.6-.719.721-1.889.721-2.609 0-.719-.719-.719-1.879 0-2.598.182-.18.387-.316.605-.406V8.835c-.217-.091-.424-.222-.6-.401-.545-.545-.676-1.342-.396-2.009L7.636 3.7.45 10.881c-.6.605-.6 1.584 0 2.189l10.48 10.477c.604.604 1.582.604 2.186 0l10.43-10.43c.605-.603.605-1.582 0-2.187" />
        </svg>
    }
}

/// Pull, commit and push buttons of the git extension, with their shortcuts in
/// their tooltips.
#[component]
fn GitButtons() -> impl IntoView {
    use crate::state::GitOp;
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();
    let busy = move || ctx.project.git_busy.get();
    let git = move || ctx.project.git.get().unwrap_or_default();
    let tip = move |label: &'static str, id: &'static str| {
        let keys = ctx.keybindings.with(|kb| kb.app_binds(id).first().cloned());
        match keys {
            Some(k) => format!("{} ({})", t(label, lang()), crate::keybindings::display(&k)),
            None => t(label, lang()).to_string(),
        }
    };
    let button = move |op: GitOp, label: &'static str, id: &'static str, icon: &'static str,
                       enabled: fn(&orgamnesia_core::GitStatus) -> bool,
                       waiting: fn(&orgamnesia_core::GitStatus) -> u32, run: fn(AppCtx)| {
    // Commits waiting for this button (the ones to push): it stands out, with their count
    let waiting = move || if busy().is_some() { 0 } else { waiting(&git()) };
    let tip = move || match waiting() {
        0 => tip(label, id),
        n => format!("{} — {n} {}", tip(label, id), t("git_to_push", lang())),
    };
    view! {
        <button
            class=move || match (busy() == Some(op), waiting() > 0) {
                (true, _) => "git-btn running",
                (_, true) => "git-btn ready",
                _ => "git-btn",
            }
            title=tip
            aria-label=tip
            prop:disabled=move || busy().is_some() || !enabled(&git())
            aria-busy=move || (busy() == Some(op)).then_some("true")
            on:click=move |_| run(ctx)
        >
            // While it runs (a push can take a while): a spinner in place of its icon
            {move || if busy() == Some(op) {
                view! { <span class="git-spinner" aria-hidden="true"></span> }.into_any()
            } else {
                view! { <span aria-hidden="true">{icon}</span> }.into_any()
            }}
            {move || t(label, lang())}
            {move || (waiting() > 0).then(|| view! {
                <span class="git-count" aria-hidden="true">{waiting()}</span>
            })}
        </button>
    }};
    view! {
        <div class="git-buttons">
            {button(GitOp::Pull, "git_pull", "git_pull", "↓", |g| g.upstream, |_| 0, crate::git::pull)}
            // Also with no change seen yet: unsaved pages are saved first
            {button(GitOp::Commit, "git_commit", "git_commit", "●", |_| true, |_| 0, crate::git::open_commit)}
            {button(GitOp::Push, "git_push", "git_push", "↑", |g| g.ahead > 0 || !g.upstream, |g| g.ahead, crate::git::push)}
        </div>
    }
}

/// Git state of the open project (git extension).
#[component]
pub fn GitLine() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();
    // Built again only when the project becomes, or stops being, a repository: a
    // new state only updates the branch and the counts, so the buttons stay the
    // same elements (one replaced between the press and the release of the mouse
    // gets no click)
    let repo = Memo::new(move |_| {
        ctx.project.vault_path.with(Option::is_some).then(|| ctx.project.git.with(|g| g.as_ref().map(|g| g.repo))).flatten()
    });
    let git = move || ctx.project.git.get().unwrap_or_default();
    let branch = move || git().branch.unwrap_or_else(|| t("git_detached", lang()).to_string());
    let items = move || {
        let st = git();
        let mut items: Vec<(&'static str, String)> = vec![];
        if st.changes > 0 { items.push(("git-dirty", format!("● {} {}", st.changes, t("git_to_commit", lang())))); }
        if st.ahead > 0 { items.push(("git-ahead", format!("↑ {} {}", st.ahead, t("git_to_push", lang())))); }
        if st.behind > 0 { items.push(("git-behind", format!("↓ {} {}", st.behind, t("git_to_pull", lang())))); }
        if !st.upstream { items.push(("git-local", t("git_no_upstream", lang()).to_string())); }
        if items.is_empty() { items.push(("git-clean", format!("✓ {}", t("git_up_to_date", lang())))); }
        items
    };
    move || match repo.get()? {
        false => Some(view! {
            <div class="frame git-status git-off">
                <GitIcon />
                <span class="git-branch">{move || t("git_no_repo", lang())}</span>
            </div>
        }.into_any()),
        true => Some(view! {
            <div class="frame git-status">
                <div class="git-head">
                    <GitIcon />
                    <span class="git-branch" title=branch>{branch}</span>
                </div>
                <div class="git-items">
                    {move || items().into_iter().map(|(cls, text)| view! {
                        <span class=format!("git-item {cls}")>{text}</span>
                    }).collect_view()}
                </div>
                <GitButtons />
            </div>
        }.into_any()),
    }
}
