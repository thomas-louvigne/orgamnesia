//! Git extension: the state of the project and the commit, pull and push buttons.

use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

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
            {button(GitOp::Push, "git_push", "git_push", "↑", |g| g.remote && (g.ahead > 0 || !g.upstream), |g| g.ahead, crate::git::push)}
        </div>
    }
}

/// The field asking the passphrase of the SSH key, to run `op` again. `wrong`:
/// the last one was refused.
#[component]
fn PassphraseField(op: crate::state::GitOp, wrong: bool) -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();
    let value = RwSignal::new(String::new());
    let input = NodeRef::<leptos::html::Input>::new();
    Effect::new(move |_| {
        if let Some(input) = input.get() { let _ = input.focus(); }
    });
    view! {
        <form class="git-passphrase"
            on:submit=move |e: web_sys::SubmitEvent| {
                e.prevent_default();
                let passphrase = value.get_untracked();
                if !passphrase.is_empty() { crate::git::send_passphrase(ctx, op, passphrase); }
            }
            on:keydown=move |e: web_sys::KeyboardEvent| {
                if e.key() == "Escape" { ctx.ui.git_passphrase.set(None); }
            }
        >
            <label for="git-passphrase-input">
                {move || t(if wrong { "git_passphrase_wrong" } else { "git_passphrase" }, lang())}
            </label>
            <input id="git-passphrase-input" type="password" autocomplete="off" node_ref=input
                prop:value=move || value.get()
                on:input=move |e| value.set(event_target_value(&e)) />
            <span class="setting-hint">{move || t("git_passphrase_hint", lang())}</span>
            <div class="git-passphrase-actions">
                <button type="button" class="btn-secondary"
                    on:click=move |_| ctx.ui.git_passphrase.set(None)>
                    {move || t("cancel", lang())}
                </button>
                <button type="submit" class="btn-primary"
                    prop:disabled=move || value.with(String::is_empty)>
                    {move || t(if op == crate::state::GitOp::Pull { "git_pull" } else { "git_push" }, lang())}
                </button>
            </div>
        </form>
    }
}

/// The value of the "New branch…" entry: no branch name can start with '-'.
const NEW_BRANCH: &str = "-new";

/// The branch, as a list to switch to another one, or to make a new one.
#[component]
fn BranchSelect() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();
    let current = move || ctx.project.git.with(|g| g.as_ref().and_then(|g| g.branch.clone()));
    let branches = RwSignal::new(Vec::<String>::new());
    let load = move || spawn_local(async move {
        if let Ok(b) = crate::invoke::git_branches().await { branches.set(b); }
    });
    // Again when the branch changes, and on focus (a fetch may have brought some)
    Effect::new(move |_| { current(); load(); });
    let select_ref = NodeRef::<leptos::html::Select>::new();
    let names = move || {
        let mut names = branches.get();
        // A branch with no commit yet has no ref
        if let Some(c) = current() && !names.contains(&c) { names.push(c); names.sort(); }
        names
    };
    let label = move || current().unwrap_or_else(|| t("git_detached", lang()).to_string());
    view! {
        <span class="git-branch-pick">
        <select class="git-branch" node_ref=select_ref title=label aria-label=move || t("git_branch", lang())
            prop:disabled=move || ctx.project.git_busy.get().is_some()
            on:focus=move |_| load()
            on:change=move |e| {
                let picked = event_target_value(&e);
                let here = current().unwrap_or_default();
                // Shown on the current branch until git has switched
                if let Some(s) = select_ref.get_untracked() { s.set_value(&here); }
                if picked == NEW_BRANCH { ctx.ui.show_git_branch.set(true); }
                else if picked != here { crate::git::switch_branch(ctx, picked, false); }
            }>
            {move || current().is_none().then(|| view! { <option value="" selected>{label()}</option> })}
            {move || names().into_iter().map(|n| {
                let selected = current().as_deref() == Some(n.as_str());
                view! { <option value=n.clone() selected=selected>{n.clone()}</option> }
            }).collect_view()}
            <option value=NEW_BRANCH>{move || t("git_new_branch", lang())}</option>
        </select>
        <span class="git-caret" aria-hidden="true">"▾"</span>
        </span>
    }
}

/// What the project folder is to git.
#[derive(Clone, Copy, PartialEq)]
enum Repo { Yes, No, Unreadable }

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
        ctx.project.vault_path.with(Option::is_some)
            .then(|| ctx.project.git.with(|g| g.as_ref().map(|g| if g.repo { Repo::Yes } else if g.error.is_some() { Repo::Unreadable } else { Repo::No })))
            .flatten()
    });
    let busy = move || ctx.project.git_busy.get();
    let git = move || ctx.project.git.get().unwrap_or_default();
    let items = move || {
        let st = git();
        let mut items: Vec<(&'static str, String)> = vec![];
        if busy() == Some(crate::state::GitOp::Sync) { items.push(("git-syncing", format!("⟳ {}", t("git_syncing", lang())))); }
        if st.merging { items.push(("git-conflict", format!("⚠ {}", t("git_merging", lang())))); }
        if st.changes > 0 { items.push(("git-dirty", format!("● {} {}", st.changes, t("git_to_commit", lang())))); }
        if st.ahead > 0 { items.push(("git-ahead", format!("↑ {} {}", st.ahead, t("git_to_push", lang())))); }
        if st.behind > 0 { items.push(("git-behind", format!("↓ {} {}", st.behind, t("git_to_pull", lang())))); }
        if !st.remote { items.push(("git-local", t("git_no_remote", lang()).to_string())); }
        else if !st.upstream { items.push(("git-local", t("git_no_upstream", lang()).to_string())); }
        if items.is_empty() { items.push(("git-clean", format!("✓ {}", t("git_up_to_date", lang())))); }
        items
    };
    let error = move || git().error.unwrap_or_default();
    // Built again only when conflicts appear or go: the button stays the same
    // element, and the file it opens next is remembered
    let in_conflict = Memo::new(move |_| ctx.project.git.with(|g| g.as_ref().is_some_and(|g| g.conflicts > 0)));
    let next = StoredValue::new(0usize);
    move || match repo.get()? {
        Repo::No => Some(view! {
            <div class="frame git-status git-off">
                <div class="git-head">
                    <GitIcon />
                    <span class="git-branch">{move || t("git_no_repo", lang())}</span>
                </div>
                <button class="git-setup-btn" prop:disabled=move || busy().is_some()
                    title=move || t("git_init_title", lang())
                    on:click=move |_| crate::git::init(ctx)>
                    {move || t("git_init", lang())}
                </button>
            </div>
        }.into_any()),
        Repo::Unreadable => Some(view! {
            <div class="frame git-status git-off">
                <div class="git-head">
                    <GitIcon />
                    <span class="git-branch">{move || t("git_unreadable", lang())}</span>
                </div>
                <span class="git-error" title=error>
                    {move || {
                        let e = error();
                        let last = e.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or_default().to_string();
                        match crate::git::hint(&e) {
                            Some(h) => format!("{last} — {}", t(h, lang())),
                            None => last,
                        }
                    }}
                </span>
            </div>
        }.into_any()),
        Repo::Yes => Some(view! {
            <div class="frame git-status">
                <div class="git-head">
                    <GitIcon />
                    <BranchSelect />
                </div>
                <div class="git-items">
                    // A click opens a file in conflict, the next one at each click
                    {move || in_conflict.get().then(|| {
                        view! {
                            <button class="git-item git-conflict git-conflict-btn"
                                title=move || t("git_open_conflict", lang())
                                on:click=move |_| {
                                    let n = next.get_value();
                                    next.set_value(n + 1);
                                    crate::git::open_conflict(ctx, n);
                                }>
                                {move || format!("⚠ {} {}", git().conflicts, t("git_in_conflict", lang()))}
                            </button>
                        }
                    })}
                    {move || items().into_iter().map(|(cls, text)| view! {
                        <span class=format!("git-item {cls}")>{text}</span>
                    }).collect_view()}
                </div>
                <GitButtons />
                {move || ctx.ui.git_passphrase.get().map(|(op, wrong)| view! { <PassphraseField op wrong /> })}
                {move || (!git().remote).then(|| view! {
                    <button class="git-setup-btn" prop:disabled=move || busy().is_some()
                        on:click=move |_| ctx.ui.show_git_remote.set(true)>
                        {move || t("git_remote_title", lang())}
                    </button>
                })}
            </div>
        }.into_any()),
    }
}
