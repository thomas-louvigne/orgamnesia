//! The git extension, on the interface side: the default commit message, the
//! hints added to git's errors, running pull / commit / push, the automatic
//! sync, and setting up a repository.

use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::{
    actions,
    i18n::{t, Lang},
    invoke,
    state::{AppCtx, FileEntry, GitChange, GitChangeKind, GitOp},
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

/// The message `template` with its fields filled: `{date}` (2025-10-29),
/// `{time}` (14:05), `{count}` (number of files) and `{files}` (their names).
pub fn fill_template(template: &str, changes: &[GitChange], date: &str, time: &str) -> String {
    let files: Vec<&str> = changes.iter().map(|c| {
        let p = c.path.strip_prefix("pages/").unwrap_or(&c.path);
        p.strip_suffix(".org").unwrap_or(p)
    }).collect();
    template.replace("{date}", date).replace("{time}", time)
        .replace("{count}", &changes.len().to_string()).replace("{files}", &files.join(", "))
}

/// The commit message proposed for `changes`: from the template of the
/// settings, or the summary and the list of the files.
pub fn proposed_message(ctx: AppCtx, changes: &[GitChange]) -> String {
    let template = ctx.pref_untracked(|p| p.git_commit_template.clone());
    if template.trim().is_empty() {
        return default_message(changes, ctx.lang.get_untracked());
    }
    let now = js_sys::Date::new_0();
    let two = |n: u32| format!("{n:02}");
    let date = format!("{}-{}-{}", now.get_full_year(), two(now.get_month() + 1), two(now.get_date()));
    let time = format!("{}:{}", two(now.get_hours()), two(now.get_minutes()));
    fill_template(&template, changes, &date, &time)
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
    Some(if e.contains("autostash") && e.contains("conflict") {
        "git_hint_autostash"
    } else if e.contains("conflict") {
        "git_hint_conflict"
    } else if e.contains("fast-forward") || e.contains("divergent") {
        "git_hint_diverged"
    } else if e.contains("[rejected]") || e.contains("fetch first") || e.contains("non-fast-forward") {
        "git_hint_pull_first"
    } else if e.contains("permission denied") || e.contains("publickey") || e.contains("host key") || e.contains("could not read from remote") {
        "git_hint_ssh"
    } else if e.contains("could not read username") || e.contains("authentication failed") || e.contains("403") {
        "git_hint_https"
    } else if e.contains("would be overwritten") {
        "git_hint_local_changes"
    } else if e.contains("tell me who you are") || e.contains("user.email") {
        "git_hint_identity"
    } else if e.contains("no configured push destination") || e.contains("no remote") {
        "git_hint_no_remote"
    } else if e.contains("dubious ownership") {
        "git_hint_ownership"
    } else {
        return None;
    })
}

/// The pull was refused because the local and remote changes touch the same
/// lines, or a merge is in progress with conflicts: the app never resolves
/// conflicts, it is to be done elsewhere.
pub fn needs_merge(err: &str) -> bool {
    let e = err.to_lowercase();
    matches!(hint(err), Some("git_hint_diverged" | "git_hint_conflict"))
        || ["unmerged", "merge_head", "conclude your merge"].iter().any(|w| e.contains(w))
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

/// Read the git state of the open project again, now (the watcher would, a bit later).
pub async fn refresh(ctx: AppCtx) {
    let Some(path) = ctx.project.vault_path.get_untracked() else { return };
    let st = invoke::git_status(&path).await.ok().flatten();
    if ctx.project.vault_path.get_untracked().as_deref() == Some(path.as_str()) {
        ctx.project.git.set(st);
    }
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
    git_on(ctx) && ctx.project.git.get_untracked().is_some_and(|g| g.repo)
}

/// The git extension is on and a project is open.
fn git_on(ctx: AppCtx) -> bool {
    ctx.pref_untracked(|p| p.git_ext)
        && ctx.project.vault_path.get_untracked().is_some()
        && !ctx.project.preference.get_untracked()
}

pub fn pull(ctx: AppCtx) {
    run(ctx, GitOp::Pull, move || async move {
        ctx.notify_t("git_pulling", "");
        match invoke::git_pull().await {
            Ok(out) if out.contains("Already up to date") || out.contains("is up to date") => {
                ctx.project.git_sync_paused.set(false);
                ctx.notify_t("git_pull_up_to_date", "");
            }
            Ok(_) => {
                ctx.project.git_sync_paused.set(false);
                ctx.notify_t("git_pulled", "");
            }
            Err(e) => {
                report(ctx, "git_pull_error", &e);
                if needs_merge(&e) { ctx.alert(t("git_merge_outside", ctx.lang.get_untracked()).to_string()); }
            }
        }
    });
}

pub fn push(ctx: AppCtx) {
    run(ctx, GitOp::Push, move || async move { push_now(ctx).await; });
}

async fn push_now(ctx: AppCtx) -> bool {
    ctx.notify_t("git_pushing", "");
    match invoke::git_push().await {
        Ok(_) => {
            ctx.project.git_sync_paused.set(false);
            ctx.notify_t("git_pushed", "");
            true
        }
        Err(e) => { report(ctx, "git_push_error", &e); false }
    }
}

/// Fetch the remote in the background, so that the commits to pull are counted.
/// Says nothing: offline, it simply fails until the next time.
pub fn fetch(ctx: AppCtx) {
    let ready = git_ready(ctx) && ctx.project.git.get_untracked().is_some_and(|g| g.remote);
    if !ready || ctx.project.git_busy.get_untracked().is_some() { return; }
    spawn_local(async move { let _ = invoke::git_fetch().await; });
}

/// Open the commit window (the pages are saved first, so their changes show).
/// When git doesn't know who signs the commits yet, their name and email are
/// asked first.
pub fn open_commit(ctx: AppCtx) {
    if !git_ready(ctx) || ctx.project.git_busy.get_untracked().is_some() { return; }
    spawn_local(async move {
        let known = invoke::git_identity().await.is_ok_and(|(name, email)| !name.is_empty() && !email.is_empty());
        if !known {
            ctx.ui.show_git_identity.set(true);
            return;
        }
        if actions::save_dirty_tabs(ctx).await { ctx.ui.show_commit.set(true); }
    });
}

/// Commit the changes with `message` (with `files`, only those), then push when `and_push`.
pub fn commit(ctx: AppCtx, message: String, files: Option<Vec<String>>, and_push: bool) {
    run(ctx, GitOp::Commit, move || async move {
        match invoke::git_commit(&message, files.as_deref()).await {
            Ok(_) => {
                let first = message.lines().next().unwrap_or_default().to_string();
                ctx.notify_t("git_committed", &first);
                if and_push {
                    ctx.project.git_busy.set(Some(GitOp::Push));
                    push_now(ctx).await;
                }
            }
            Err(e) => {
                report(ctx, "git_commit_error", &e);
                if hint(&e) == Some("git_hint_identity") { ctx.ui.show_git_identity.set(true); }
            }
        }
    });
}

// ─── Automatic sync ──────────────────────────────────────────────────────────

/// What the automatic sync did.
#[derive(Default)]
struct Synced { committed: bool, pulled: bool, pushed: bool }

/// Commit every change with the proposed message, pull, then push. Stops at
/// the first failure, said in the status bar; `Err` then.
async fn sync_now(ctx: AppCtx) -> Result<Synced, ()> {
    let mut done = Synced::default();
    let fail = |key: &'static str, e: &str| {
        report(ctx, key, e);
        ctx.project.git_sync_paused.set(true);
    };
    let changes = invoke::git_changes().await.map_err(|e| fail("git_sync_error", &e))?;
    if !changes.is_empty() {
        invoke::git_commit(&proposed_message(ctx, &changes), None).await.map_err(|e| fail("git_commit_error", &e))?;
        done.committed = true;
    }
    refresh(ctx).await;
    let st = ctx.project.git.get_untracked().unwrap_or_default();
    if st.upstream {
        let out = invoke::git_pull().await.map_err(|e| fail("git_pull_error", &e))?;
        done.pulled = !(out.contains("Already up to date") || out.contains("is up to date"));
    }
    if st.remote && (st.ahead > 0 || !st.upstream || done.committed) {
        invoke::git_push().await.map_err(|e| fail("git_push_error", &e))?;
        done.pushed = true;
    }
    Ok(done)
}

/// The sync can run: the project is a repository, not in the middle of a merge,
/// and no git command runs.
fn can_sync(ctx: AppCtx) -> bool {
    git_ready(ctx)
        && ctx.project.git_busy.get_untracked().is_none()
        && ctx.project.git.get_untracked().is_some_and(|g| g.conflicts == 0 && !g.merging)
}

/// The automatic sync, every so many minutes: quiet when there was nothing to do.
pub fn auto_sync(ctx: AppCtx) {
    if ctx.project.git_sync_paused.get_untracked() || !can_sync(ctx) { return; }
    run(ctx, GitOp::Sync, move || async move {
        if let Ok(done) = sync_now(ctx).await && (done.committed || done.pulled || done.pushed) {
            ctx.notify_t("git_synced", "");
        }
    });
}

/// Before quitting, with the sync on quitting on: sync (the pages as on disk),
/// then `quit`. When the sync fails, ask whether to quit all the same.
pub fn sync_then(ctx: AppCtx, quit: impl Fn() + Send + Sync + Clone + 'static) {
    if !ctx.pref_untracked(|p| p.git_sync_on_quit) || !can_sync(ctx) {
        quit();
        return;
    }
    ctx.project.git_busy.set(Some(GitOp::Sync));
    ctx.notify_t("git_syncing_quit", "");
    spawn_local(async move {
        let ok = sync_now(ctx).await.is_ok();
        ctx.project.git_busy.set(None);
        if ok {
            quit();
        } else {
            ctx.ask_confirm(t("git_sync_quit_failed", ctx.lang.get_untracked()).to_string(), quit);
        }
    });
}

// ─── Setting up ──────────────────────────────────────────────────────────────

/// Make the open project a git repository.
pub fn init(ctx: AppCtx) {
    if !git_on(ctx) || ctx.project.git_busy.get_untracked().is_some() { return; }
    ctx.project.git_busy.set(Some(GitOp::Setup));
    spawn_local(async move {
        match invoke::git_init().await {
            Ok(_) => ctx.notify_t("git_inited", ""),
            Err(e) => report(ctx, "git_init_error", &e),
        }
        refresh(ctx).await;
        ctx.project.git_busy.set(None);
    });
}

/// Give the project's repository the remote `origin` at `url`.
pub fn add_remote(ctx: AppCtx, url: String) {
    if !git_ready(ctx) || ctx.project.git_busy.get_untracked().is_some() { return; }
    ctx.project.git_busy.set(Some(GitOp::Setup));
    spawn_local(async move {
        match invoke::git_add_remote(&url).await {
            Ok(_) => ctx.notify_t("git_remote_added", &url),
            Err(e) => report(ctx, "git_remote_error", &e),
        }
        refresh(ctx).await;
        ctx.project.git_busy.set(None);
    });
}

// ─── Conflicts ───────────────────────────────────────────────────────────────

/// Open the `n`-th file in conflict (cycling through them), the cursor on its
/// first conflict marker. Saved without its markers, it is marked resolved (by
/// the backend, as the page changes on disk) and the count goes down.
pub fn open_conflict(ctx: AppCtx, n: usize) {
    let files = ctx.project.git.with_untracked(|g| g.as_ref().map(|g| g.conflict_files.clone()).unwrap_or_default());
    if files.is_empty() {
        ctx.notify_t("git_conflict_outside", "");
        return;
    }
    let path = files[n % files.len()].clone();
    let Some(file) = ctx.project.files.with_untracked(|fs| fs.iter().find(|f| f.path == path).cloned()) else {
        // Not a page (an image…): to resolve elsewhere
        ctx.notify_t("git_conflict_not_page", &path);
        return;
    };
    spawn_local(async move {
        // The open tab's text, else the page on disk
        let text = match ctx.work.tabs.with_untracked(|tabs| tabs.iter().find(|t| t.path == file.path).map(|t| t.content.get_untracked())) {
            Some(text) => text,
            None => invoke::read_file(&file.path).await.unwrap_or_default(),
        };
        let line = text.lines().position(|l| l.starts_with("<<<<<<<")).unwrap_or(0);
        ctx.notify_t("git_conflict_help", "");
        actions::open_file(ctx, file, Some(crate::state::Goto::Line(line)));
    });
}

// ─── History of a page ───────────────────────────────────────────────────────

/// Open the history window of `file`.
pub fn open_history(ctx: AppCtx, file: FileEntry) {
    if git_ready(ctx) { ctx.ui.git_history.set(Some(file)); }
}

/// The history of the pages can be shown (tracked): the extension is on and
/// the project is a repository.
pub fn history_available(ctx: AppCtx) -> bool {
    ctx.pref(|p| p.git_ext)
        && !ctx.project.preference.get()
        && ctx.project.git.with(|g| g.as_ref().is_some_and(|g| g.repo))
}

/// Make `content` (an older version) the text of the page `file`: in its tab when
/// open (undone with the editor's undo), else written to disk, then opened.
pub fn restore(ctx: AppCtx, file: FileEntry, content: String) {
    let open = ctx.work.tabs.with_untracked(|tabs| tabs.iter().position(|t| t.path == file.path));
    if let Some(idx) = open {
        let tab = ctx.work.tabs.with_untracked(|tabs| tabs[idx].clone());
        let now = js_sys::Date::now();
        tab.content.with_untracked(|old| tab.history.update_value(|h| h.record(old, &content, now)));
        tab.content.set(content);
        // Marked unsaved, then auto-saved if enabled
        tab.dirty.set(true);
        ctx.work.active_tab.set(Some(idx));
        ctx.notify_t("git_restored", &file.name);
        return;
    }
    spawn_local(async move {
        match invoke::write_file(&file.path, &content).await {
            Ok(()) => {
                ctx.bump_links();
                ctx.notify_t("git_restored", &file.name);
                actions::open_file(ctx, file, None);
            }
            Err(e) => ctx.error("error", &e),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ch(path: &str, kind: GitChangeKind) -> GitChange { GitChange { path: path.into(), kind, old_path: None } }

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
    fn template_fields() {
        let changes = [ch("pages/Roadmap.org", GitChangeKind::Modified), ch("assets/x.png", GitChangeKind::Added)];
        assert_eq!(fill_template("Notes {date} {time} ({count}) : {files}", &changes, "2025-10-29", "14:05"),
            "Notes 2025-10-29 14:05 (2) : Roadmap, assets/x.png");
        assert_eq!(fill_template("Sans champ", &changes, "d", "t"), "Sans champ");
    }

    #[test]
    fn hints_for_common_errors() {
        assert_eq!(hint("fatal: Not possible to fast-forward, aborting."), Some("git_hint_diverged"));
        assert_eq!(hint(" ! [rejected]        main -> main (fetch first)"), Some("git_hint_pull_first"));
        assert_eq!(hint("git@github.com: Permission denied (publickey)."), Some("git_hint_ssh"));
        assert_eq!(hint("CONFLICT (content): Merge conflict in a.org"), Some("git_hint_conflict"));
        assert_eq!(hint("Applying autostash resulted in conflicts."), Some("git_hint_autostash"));
        assert_eq!(hint("fatal: could not read Username for 'https://github.com'"), Some("git_hint_https"));
        assert_eq!(hint("fatal: detected dubious ownership in repository at '/x'"), Some("git_hint_ownership"));
        assert_eq!(hint("something else"), None);
        assert!(needs_merge("fatal: Not possible to fast-forward, aborting."));
        assert!(needs_merge("CONFLICT (content): Merge conflict in a.org\nerror: could not apply abc"));
        assert!(needs_merge("error: Pulling is not possible because you have unmerged files."));
        assert!(needs_merge("fatal: You have not concluded your merge (MERGE_HEAD exists)."));
        assert!(!needs_merge("git@github.com: Permission denied (publickey)."));
        assert!(!needs_merge("Applying autostash resulted in conflicts."));
    }
}
