use super::*;

#[test]
fn clean_and_tracking() {
    let s = parse("# branch.oid abc\n# branch.head main\n# branch.upstream origin/main\n# branch.ab +0 -0\n");
    assert_eq!(s, GitStatus { repo: true, branch: Some("main".into()), upstream: true, ..Default::default() });
}

#[test]
fn changes_ahead_behind() {
    let s = parse("# branch.head main\n# branch.upstream origin/main\n# branch.ab +2 -3\n1 .M N... 100644 100644 100644 a b x.org\n? y.org\nu UU N... 1 2 3 4 a b c z.org\n");
    assert_eq!((s.changes, s.ahead, s.behind, s.conflicts), (2, 2, 3, 1));
}

#[test]
fn no_upstream_detached() {
    let s = parse("# branch.oid abc\n# branch.head (detached)\n");
    assert_eq!((s.branch, s.upstream), (None, false));
}

#[test]
fn branch_names() {
    let out = "refs/heads/main\nrefs/heads/notes\nrefs/remotes/origin/HEAD\nrefs/remotes/origin/main\nrefs/remotes/origin/feat/x\n";
    assert_eq!(parse_branches(out), vec!["feat/x", "main", "notes"]);
}

#[test]
fn changes_from_porcelain() {
    let out = " M pages/a.org\0?? pages/b c.org\0D  pages/d.org\0R  pages/new.org\0pages/old.org\0A  assets/x.png\0";
    let got = parse_changes(out);
    let seen: Vec<(String, GitChangeKind, Option<String>)> = got.into_iter().map(|c| (c.path, c.kind, c.old_path)).collect();
    assert_eq!(seen, vec![
        ("pages/a.org".into(), GitChangeKind::Modified, None),
        ("pages/b c.org".into(), GitChangeKind::Added, None),
        ("pages/d.org".into(), GitChangeKind::Deleted, None),
        ("pages/new.org".into(), GitChangeKind::Renamed, Some("pages/old.org".into())),
        ("assets/x.png".into(), GitChangeKind::Added, None),
    ]);
}

#[test]
fn log_records() {
    let out = "\x1eabc\x1f1700000000\x1fT\x1fSecond\n\npages/b.org\n\x1edef\x1f1600000000\x1fT\x1fFirst\n\npages/a.org\n";
    let got = parse_log(out);
    assert_eq!(got.len(), 2);
    assert_eq!((got[0].hash.as_str(), got[0].date, got[0].path.as_str()), ("abc", 1_700_000_000, "pages/b.org"));
    assert_eq!((got[1].subject.as_str(), got[1].path.as_str()), ("First", "pages/a.org"));
}

#[test]
fn conflict_markers() {
    assert!(has_markers("a\n<<<<<<< HEAD\nx\n=======\ny\n>>>>>>> abc\n"));
    assert!(!has_markers("a\n=======\n<<< pas un marqueur\n"));
}

#[test]
fn clone_folder_names() {
    assert_eq!(repo_name("git@github.com:me/notes.git").as_deref(), Some("notes"));
    assert_eq!(repo_name("https://example.com/me/wiki/").as_deref(), Some("wiki"));
    assert_eq!(repo_name("/srv/git/notes.git").as_deref(), Some("notes"));
    assert_eq!(repo_name(""), None);
}

#[test]
fn ssh_never_asks() {
    let c = ssh_command("ssh", "", false);
    assert!(c.starts_with("ssh -o BatchMode=yes") && !c.contains("-i"));
    let c = ssh_command("ssh -p 2222", "/k/it's key", false);
    assert!(c.starts_with("ssh -p 2222 -o BatchMode=yes"));
    assert!(c.ends_with(" -i '/k/it'\\''s key' -o IdentitiesOnly=yes"), "{c}");
}

#[test]
fn with_a_passphrase_ssh_asks_for_nothing_else() {
    let c = ssh_command("ssh", "", true);
    assert!(!c.contains("BatchMode"), "{c}");
    assert!(c.contains("-o PasswordAuthentication=no") && c.contains("-o KbdInteractiveAuthentication=no"), "{c}");
    assert_eq!(askpass_reply("Enter passphrase for key '/k': ", "secret"), Some("secret"));
    assert_eq!(askpass_reply("git@host's password: ", "secret"), None);
    assert_eq!(askpass_reply("Username for 'https://github.com': ", "secret"), None);
}

#[test]
fn a_hash_is_never_an_option() {
    assert!(check_hash("0123abcDEF").is_ok());
    assert!(show(".", "--output=/tmp/x", "a.org").is_err());
    assert!(commit_diff(".", "-p", "a.org").is_err());
    assert!(check_hash("").is_err() && check_hash("HEAD~1").is_err());
}

fn sh(dir: &Path, args: &[&str]) {
    let out = Command::new("git").arg("-C").arg(dir).args(["-c", "core.autocrlf=false"]).args(args).output().unwrap();
    assert!(out.status.success(), "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

/// A bare repository `server.git` and a clone of it, `name`, in `base`.
fn work_tree(base: &Path, name: &str) -> std::path::PathBuf {
    let work = base.join(name);
    sh(base, &["clone", "-q", base.join("server.git").to_str().unwrap(), name]);
    sh(&work, &["config", "user.name", "t"]);
    sh(&work, &["config", "user.email", "t@example.com"]);
    sh(&work, &["config", "pull.rebase", "false"]);
    work
}

fn temp(test: &str) -> std::path::PathBuf {
    crate::testing::folder(&format!("git-{test}"))
}

#[test]
fn commit_pull_and_push_through_git() {
    let base = temp("cpp");
    let work = base.join("work");
    std::fs::create_dir_all(&work).unwrap();
    sh(&base, &["init", "-q", "--bare", "-b", "main", "server.git"]);
    let w = work.to_str().unwrap();
    assert!(!status(w).unwrap().repo && status(w).unwrap().error.is_none());
    init(w).unwrap();
    assert!(work.join(".gitignore").exists());
    sh(&work, &["checkout", "-q", "-b", "main"]);
    sh(&work, &["config", "user.name", "t"]);
    sh(&work, &["config", "user.email", "t@example.com"]);
    assert!(!status(w).unwrap().remote);
    add_remote(w, base.join("server.git").to_str().unwrap()).unwrap();
    assert!(status(w).unwrap().remote);

    std::fs::write(work.join("a.org"), "* a\n").unwrap();
    std::fs::remove_file(work.join(".gitignore")).unwrap();
    assert_eq!(changes(w).unwrap(), vec![GitChange { path: "a.org".into(), kind: GitChangeKind::Added, old_path: None }]);
    assert!(diff(w, "a.org", None).unwrap().starts_with("+* a"));
    // No remote branch: marked while it differs from the last commit
    assert_eq!(status(w).unwrap().unpushed, vec![format!("{w}/a.org")]);
    commit(w, "Premier\n\n- a.org", None).unwrap();
    assert!(changes(w).unwrap().is_empty());
    // No remote branch yet: pushed and tracked
    push(w, "").unwrap();
    let st = status(w).unwrap();
    assert!(st.upstream && st.unpushed.is_empty());
    // Committed, not pushed: marked
    std::fs::write(work.join("a.org"), "* a\n2\n").unwrap();
    commit(w, "2", None).unwrap();
    assert_eq!(status(w).unwrap().unpushed, vec![format!("{w}/a.org")]);
    push(w, "").unwrap();
    assert!(pull(w, "", true).is_ok());
    assert!(fetch(w, "").is_ok());
    // A new branch, back to main, and a name git can't take
    switch(w, "brouillon", true).unwrap();
    assert_eq!(status(w).unwrap().branch.as_deref(), Some("brouillon"));
    switch(w, "main", false).unwrap();
    assert_eq!(branches(w).unwrap(), vec!["brouillon", "main"]);
    assert!(switch(w, "-f", false).is_err() && switch(w, "a b", true).is_err());
    // Nothing to commit: git says so
    assert!(matches!(commit(w, "vide", None), Err(AppError::Git(_))));

    // Only the files chosen are committed
    std::fs::write(work.join("a.org"), "* a\n2\nplus\n").unwrap();
    std::fs::write(work.join("b.org"), "* b\n").unwrap();
    assert!(diff(w, "a.org", None).unwrap().contains("+plus"));
    commit(w, "b seul", Some(&["b.org".into()])).unwrap();
    assert_eq!(changes(w).unwrap().len(), 1);
    // A deletion too
    std::fs::remove_file(work.join("b.org")).unwrap();
    commit(w, "sans b", Some(&["b.org".into()])).unwrap();
    assert_eq!(changes(w).unwrap()[0].path, "a.org");

    // History of a page, and its text then
    let file = work.join("b.org");
    let hist = log(w, file.to_str().unwrap()).unwrap();
    assert_eq!(hist.iter().map(|c| c.subject.as_str()).collect::<Vec<_>>(), vec!["sans b", "b seul"]);
    assert_eq!(show(w, &hist[1].hash, &hist[1].path).unwrap(), "* b\n");
    assert!(commit_diff(w, &hist[1].hash, &hist[1].path).unwrap().contains("+* b"));

    // A clone, in a new folder named after the repository
    let cloned = clone(base.to_str().unwrap(), base.join("server.git").to_str().unwrap(), "").unwrap();
    assert!(cloned.ends_with("/server") && Path::new(&cloned).join("a.org").exists());
    assert!(clone(base.to_str().unwrap(), base.join("server.git").to_str().unwrap(), "").is_err());
    std::fs::remove_dir_all(base).unwrap();
}

#[test]
fn a_project_inside_a_repository_commits_only_its_folder() {
    let base = temp("sub");
    sh(&base, &["init", "-q", "-b", "main"]);
    sh(&base, &["config", "user.name", "t"]);
    sh(&base, &["config", "user.email", "t@example.com"]);
    let notes = base.join("notes");
    std::fs::create_dir_all(&notes).unwrap();
    std::fs::write(notes.join("a.org"), "* a\n").unwrap();
    std::fs::write(base.join(".env"), "SECRET=1\n").unwrap();
    let n = notes.to_str().unwrap();
    assert_eq!(changes(n).unwrap().iter().map(|c| c.path.as_str()).collect::<Vec<_>>(), ["notes/a.org"]);
    assert_eq!(status(n).unwrap().changes, 1);
    commit(n, "notes", None).unwrap();
    assert!(changes(n).unwrap().is_empty());
    // The file outside the project is neither committed nor staged
    let out = Command::new("git").arg("-C").arg(&base).args(["status", "--porcelain"]).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout), "?? .env\n");
    std::fs::remove_dir_all(base).unwrap();
}

#[test]
fn pull_over_local_changes_and_diverged_history() {
    let base = temp("pull");
    sh(&base, &["init", "-q", "--bare", "-b", "main", "server.git"]);
    let one = work_tree(&base, "one");
    std::fs::write(one.join("a.org"), "* a\n1\n2\n3\n").unwrap();
    std::fs::write(one.join("b.org"), "* b\n").unwrap();
    sh(&one, &["add", "-A"]);
    sh(&one, &["commit", "-qm", "base"]);
    sh(&one, &["push", "-q", "-u", "origin", "HEAD:main"]);
    let two = work_tree(&base, "two");
    let (o, t) = (one.to_str().unwrap(), two.to_str().unwrap());

    // The remote moves on; a page changed here, not committed: put aside, then back
    std::fs::write(one.join("a.org"), "* a\nun\n2\n3\n").unwrap();
    commit(o, "un", None).unwrap();
    push(o, "").unwrap();
    std::fs::write(two.join("b.org"), "* b\nlocal\n").unwrap();
    fetch(t, "").unwrap();
    assert_eq!(status(t).unwrap().behind, 1);
    pull(t, "", true).unwrap();
    assert_eq!(std::fs::read_to_string(two.join("a.org")).unwrap(), "* a\nun\n2\n3\n");
    assert_eq!(std::fs::read_to_string(two.join("b.org")).unwrap(), "* b\nlocal\n");

    // Both committed apart, on other lines: refused without the rebase, else
    // replayed on the remote's
    std::fs::write(one.join("a.org"), "* a\nun\n2\ntrois\n").unwrap();
    commit(o, "trois", None).unwrap();
    push(o, "").unwrap();
    commit(t, "local", None).unwrap();
    assert!(diverged(&pull(t, "", false).unwrap_err().to_string()));
    pull(t, "", true).unwrap();
    let st = status(t).unwrap();
    assert_eq!((st.ahead, st.behind, st.merging), (1, 0, false));
    push(t, "").unwrap();

    // On the same line: refused, and everything as before
    pull(o, "", true).unwrap();
    std::fs::write(one.join("a.org"), "* a\nUN\n2\ntrois\n").unwrap();
    commit(o, "UN", None).unwrap();
    push(o, "").unwrap();
    std::fs::write(two.join("a.org"), "* a\nOne\n2\ntrois\n").unwrap();
    commit(t, "One", None).unwrap();
    let err = pull(t, "", true).unwrap_err().to_string();
    assert!(err.to_lowercase().contains("conflict"), "{err}");
    let st = status(t).unwrap();
    assert_eq!((st.merging, st.conflicts, st.ahead), (false, 0, 1));
    assert_eq!(std::fs::read_to_string(two.join("a.org")).unwrap(), "* a\nOne\n2\ntrois\n");

    // A merge made in a terminal stops on the conflict: shown, then resolved
    // once the markers are gone
    let _ = Command::new("git").arg("-C").arg(&two).args(["merge", "-q", "origin/main"]).output();
    let st = status(t).unwrap();
    assert_eq!((st.conflicts, st.merging), (1, true));
    assert_eq!(st.conflict_files, vec![format!("{t}/a.org")]);
    assert_eq!(mark_resolved(t), 0);
    std::fs::write(two.join("a.org"), "* a\nOne, UN\n2\ntrois\n").unwrap();
    assert_eq!(mark_resolved(t), 1);
    let st = status(t).unwrap();
    assert_eq!((st.conflicts, st.changes), (0, 1));
    assert!(st.conflict_files.is_empty());
    std::fs::remove_dir_all(base).unwrap();
}
