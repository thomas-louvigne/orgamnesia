//! The git extension, on the interface side: the default commit message, the
//! hints added to git's errors, and running pull / commit / push.

use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::{
    actions,
    i18n::{t, Lang},
    invoke,
    state::{AppCtx, GitChange, GitChangeKind, GitOp},
};

/// The message proposed for a commit: a summary line, then one line per file.
///
/// ```text
/// Mise à jour de 3 pages
///
/// - Modifié : Roadmap.org
/// - Ajouté : Journal.org
/// ```
pub fn default_message(changes: &[GitChange], lang: Lang) -> String {
    let name = |path: &str| path.strip_prefix("pages/").unwrap_or(path).to_string();
    let pages = changes.iter().filter(|c| c.path.ends_with(".org")).count();
    let summary = match (changes, pages == changes.len()) {
        ([one], true) => format!("{} {}", t("git_msg_page", lang), name(&one.path).trim_end_matches(".org")),
        ([one], false) => format!("{} {}", t("git_msg_file", lang), name(&one.path)),
        (_, true) => format!("{} {} {}", t("git_msg_pages", lang), changes.len(), t("git_msg_pages_end", lang)),
        _ => format!("{} {} {}", t("git_msg_pages", lang), changes.len(), t("git_msg_files_end", lang)),
    };
    let lines: Vec<String> = changes.iter()
        .map(|c| format!("- {} {}", t(kind_key(c.kind), lang), name(&c.path)))
        .collect();
    format!("{summary}\n\n{}\n", lines.join("\n"))
}

/// Label key of what happened to a file.
pub fn kind_key(kind: GitChangeKind) -> &'static str {
    match kind {
        GitChangeKind::Added => "git_added",
        GitChangeKind::Modified => "git_modified",
        GitChangeKind::Deleted => "git_deleted",
        GitChangeKind::Renamed => "git_renamed",
    }
}

/// What to do about a git error, when it is a common one (git runs in English).
pub fn hint(err: &str) -> Option<&'static str> {
    let e = err.to_lowercase();
    Some(if e.contains("fast-forward") || e.contains("divergent") {
        "git_hint_diverged"
    } else if e.contains("[rejected]") || e.contains("fetch first") || e.contains("non-fast-forward") {
        "git_hint_pull_first"
    } else if e.contains("permission denied") || e.contains("publickey") || e.contains("host key") || e.contains("could not read from remote") {
        "git_hint_ssh"
    } else if e.contains("would be overwritten") {
        "git_hint_local_changes"
    } else if e.contains("tell me who you are") || e.contains("user.email") {
        "git_hint_identity"
    } else if e.contains("no remote") || e.contains("no configured push destination") {
        "git_hint_no_remote"
    } else {
        return None;
    })
}

/// Report a failed git command in the status bar, with a hint when there is one.
fn report(ctx: AppCtx, key: &'static str, err: &str) {
    let lang = ctx.lang.get_untracked();
    // Git's last line says the most
    let last = err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or(err).trim();
    let msg = match hint(err) {
        Some(h) => format!("{} : {last} — {}", t(key, lang), t(h, lang)),
        None => format!("{} : {last}", t(key, lang)),
    };
    ctx.notify(msg);
}

/// Run `op` unless another git command is running; the pages are saved first.
fn run<F: std::future::Future<Output = ()> + 'static>(ctx: AppCtx, op: GitOp, job: impl FnOnce() -> F + 'static) {
    if ctx.project.git_busy.get_untracked().is_some() || !git_ready(ctx) { return; }
    ctx.project.git_busy.set(Some(op));
    spawn_local(async move {
        if actions::save_dirty_tabs(ctx).await {
            job().await;
        }
        ctx.project.git_busy.set(None);
    });
}

/// The git extension is on and the project is a repository.
pub fn git_ready(ctx: AppCtx) -> bool {
    ctx.pref_untracked(|p| p.git_ext)
        && ctx.project.vault_path.get_untracked().is_some()
        && ctx.project.git.get_untracked().is_some_and(|g| g.repo)
}

pub fn pull(ctx: AppCtx) {
    run(ctx, GitOp::Pull, move || async move {
        ctx.notify_t("git_pulling", "");
        match invoke::git_pull().await {
            Ok(out) if out.contains("Already up to date") => ctx.notify_t("git_pull_up_to_date", ""),
            Ok(_) => ctx.notify_t("git_pulled", ""),
            Err(e) => report(ctx, "git_pull_error", &e),
        }
    });
}

pub fn push(ctx: AppCtx) {
    run(ctx, GitOp::Push, move || async move { push_now(ctx).await });
}

async fn push_now(ctx: AppCtx) {
    ctx.notify_t("git_pushing", "");
    match invoke::git_push().await {
        Ok(_) => ctx.notify_t("git_pushed", ""),
        Err(e) => report(ctx, "git_push_error", &e),
    }
}

/// Open the commit window (the pages are saved first, so their changes show).
pub fn open_commit(ctx: AppCtx) {
    if !git_ready(ctx) || ctx.project.git_busy.get_untracked().is_some() { return; }
    spawn_local(async move {
        if actions::save_dirty_tabs(ctx).await { ctx.ui.show_commit.set(true); }
    });
}

/// Commit every change with `message`, then push when `and_push`.
pub fn commit(ctx: AppCtx, message: String, and_push: bool) {
    run(ctx, GitOp::Commit, move || async move {
        match invoke::git_commit(&message).await {
            Ok(_) => {
                let first = message.lines().next().unwrap_or_default().to_string();
                ctx.notify_t("git_committed", &first);
                if and_push {
                    ctx.project.git_busy.set(Some(GitOp::Push));
                    push_now(ctx).await;
                }
            }
            Err(e) => report(ctx, "git_commit_error", &e),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ch(path: &str, kind: GitChangeKind) -> GitChange { GitChange { path: path.into(), kind } }

    #[test]
    fn default_message_lists_the_files() {
        let changes = [
            ch("pages/Roadmap.org", GitChangeKind::Modified),
            ch("pages/Journal.org", GitChangeKind::Added),
            ch("pages/Draft.org", GitChangeKind::Deleted),
        ];
        assert_eq!(default_message(&changes, Lang::Fr),
            "Mise à jour de 3 pages\n\n- Modifié : Roadmap.org\n- Ajouté : Journal.org\n- Supprimé : Draft.org\n");
        assert_eq!(default_message(&changes[..1], Lang::En),
            "Update page Roadmap\n\n- Modified: Roadmap.org\n");
        let mixed = [ch("pages/a.org", GitChangeKind::Modified), ch("assets/x.png", GitChangeKind::Added)];
        assert!(default_message(&mixed, Lang::En).starts_with("Update 2 files\n"));
        assert!(default_message(&mixed[1..], Lang::Fr).starts_with("Mise à jour du fichier assets/x.png\n"));
    }

    #[test]
    fn hints_for_common_errors() {
        assert_eq!(hint("fatal: Not possible to fast-forward, aborting."), Some("git_hint_diverged"));
        assert_eq!(hint(" ! [rejected]        main -> main (fetch first)"), Some("git_hint_pull_first"));
        assert_eq!(hint("git@github.com: Permission denied (publickey)."), Some("git_hint_ssh"));
        assert_eq!(hint("something else"), None);
    }
}
