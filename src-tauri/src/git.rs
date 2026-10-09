//! The git extension: the state of the project and pull / commit / push, by
//! running the `git` command (no git library).

use std::process::{Command, Stdio};

use crate::error::AppError;
pub use orgamnesia_core::{GitChange, GitChangeKind, GitStatus};

/// Git state of `path`; `None` when git itself can't be run.
pub fn status(path: &str) -> Option<GitStatus> {
    let out = Command::new("git")
        .args(["-C", path, "status", "--porcelain=v2", "--branch"])
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .ok()?;
    if !out.status.success() {
        return Some(GitStatus::default());
    }
    Some(parse(&String::from_utf8_lossy(&out.stdout)))
}

/// Read the output of `git status --porcelain=v2 --branch`.
fn parse(out: &str) -> GitStatus {
    let mut s = GitStatus { repo: true, ..Default::default() };
    for line in out.lines() {
        if let Some(head) = line.strip_prefix("# branch.head ") {
            s.branch = (head != "(detached)").then(|| head.to_string());
        } else if line.starts_with("# branch.upstream ") {
            s.upstream = true;
        } else if let Some(ab) = line.strip_prefix("# branch.ab ") {
            for n in ab.split_whitespace() {
                if let Some(a) = n.strip_prefix('+') { s.ahead = a.parse().unwrap_or(0); }
                if let Some(b) = n.strip_prefix('-') { s.behind = b.parse().unwrap_or(0); }
            }
        } else if !line.starts_with('#') && !line.is_empty() {
            s.changes += 1;
        }
    }
    s
}

/// Files changed since the last commit (untracked ones included).
pub fn changes(path: &str) -> Result<Vec<GitChange>, AppError> {
    let out = run(git(path).args(["status", "--porcelain=v1", "-z", "--untracked-files=all"]).env("GIT_OPTIONAL_LOCKS", "0"))?;
    Ok(parse_changes(&out))
}

/// Stage every change of the project (`git add -A`) and commit it with `message`.
/// Returns what git says.
pub fn commit(path: &str, message: &str) -> Result<String, AppError> {
    run(git(path).args(["add", "-A"]))?;
    let mut child = git(path).args(["commit", "--file", "-"])
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        stdin.write_all(message.as_bytes())?;
    }
    output(child.wait_with_output()?)
}

/// `git pull --ff-only`: only moves forward, never merges. Returns what git says.
pub fn pull(path: &str, ssh_key: &str) -> Result<String, AppError> {
    run(remote(git(path), path, ssh_key).args(["pull", "--ff-only"]))
}

/// `git push`; a branch with no remote branch yet is pushed to `origin` (or the
/// only remote) and set to track it. Returns what git says.
pub fn push(path: &str, ssh_key: &str) -> Result<String, AppError> {
    let tracking = run(git(path).args(["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"])).is_ok();
    if tracking {
        return run(remote(git(path), path, ssh_key).arg("push"));
    }
    let remotes = run(git(path).arg("remote"))?;
    let names: Vec<&str> = remotes.lines().map(str::trim).filter(|r| !r.is_empty()).collect();
    let Some(name) = names.iter().find(|&&r| r == "origin").or(names.first()) else {
        return Err(AppError::Git("no remote: add one with git remote add origin <url>".into()));
    };
    run(remote(git(path), path, ssh_key).args(["push", "--set-upstream", name, "HEAD"]))
}

fn git(path: &str) -> Command {
    let mut c = Command::new("git");
    c.arg("-C").arg(path).env("LC_ALL", "C");
    c
}

/// A git command that talks to the remote: it must never wait for an answer no
/// one can give (password, passphrase, unknown host), so git and ssh run in
/// batch mode. With `ssh_key`, ssh uses that key only.
fn remote(mut c: Command, path: &str, ssh_key: &str) -> Command {
    // The ssh command of the repository's settings, if any, else `ssh`
    let base = run(git(path).args(["config", "--get", "core.sshCommand"])).ok()
        .map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
        .unwrap_or_else(|| "ssh".into());
    c.env("GIT_TERMINAL_PROMPT", "0").env("GIT_SSH_COMMAND", ssh_command(&base, ssh_key));
    c
}

/// The command git runs for ssh (read by a shell, hence the quoting).
fn ssh_command(base: &str, ssh_key: &str) -> String {
    let mut cmd = format!("{base} -o BatchMode=yes -o ConnectTimeout=20 -o StrictHostKeyChecking=accept-new");
    let key = ssh_key.trim();
    if !key.is_empty() {
        let key = shellexpand_home(key);
        cmd.push_str(&format!(" -i '{}' -o IdentitiesOnly=yes", key.replace('\'', "'\\''")));
    }
    cmd
}

/// `~/x` → `/home/me/x`.
fn shellexpand_home(path: &str) -> String {
    match (path.strip_prefix("~/"), std::env::var("HOME")) {
        (Some(rest), Ok(home)) => format!("{home}/{rest}"),
        _ => path.to_string(),
    }
}

/// Run `c`; its output, or its error message when it fails.
fn run(c: &mut Command) -> Result<String, AppError> {
    output(c.stdin(Stdio::null()).output()?)
}

fn output(out: std::process::Output) -> Result<String, AppError> {
    let text = |b: &[u8]| String::from_utf8_lossy(b).trim().to_string();
    if out.status.success() {
        Ok(text(&out.stdout))
    } else {
        let err = text(&out.stderr);
        Err(AppError::Git(if err.is_empty() { text(&out.stdout) } else { err }))
    }
}

/// Read the output of `git status --porcelain=v1 -z`.
fn parse_changes(out: &str) -> Vec<GitChange> {
    let mut changes = Vec::new();
    let mut entries = out.split('\0').filter(|e| !e.is_empty());
    while let Some(entry) = entries.next() {
        let (Some(xy), Some(path)) = (entry.get(..2), entry.get(3..)) else { continue };
        let kind = match xy {
            "??" => GitChangeKind::Added,
            _ if xy.contains('R') || xy.contains('C') => {
                entries.next(); // the old path
                GitChangeKind::Renamed
            }
            _ if xy.contains('D') => GitChangeKind::Deleted,
            _ if xy.starts_with('A') => GitChangeKind::Added,
            _ => GitChangeKind::Modified,
        };
        changes.push(GitChange { path: path.to_string(), kind });
    }
    changes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_and_tracking() {
        let s = parse("# branch.oid abc\n# branch.head main\n# branch.upstream origin/main\n# branch.ab +0 -0\n");
        assert_eq!(s, GitStatus { repo: true, branch: Some("main".into()), upstream: true, ..Default::default() });
    }

    #[test]
    fn changes_ahead_behind() {
        let s = parse("# branch.head main\n# branch.upstream origin/main\n# branch.ab +2 -3\n1 .M N... 100644 100644 100644 a b x.org\n? y.org\n");
        assert_eq!((s.changes, s.ahead, s.behind), (2, 2, 3));
    }

    #[test]
    fn no_upstream_detached() {
        let s = parse("# branch.oid abc\n# branch.head (detached)\n");
        assert_eq!((s.branch, s.upstream), (None, false));
    }

    #[test]
    fn changes_from_porcelain() {
        let out = " M pages/a.org\0?? pages/b c.org\0D  pages/d.org\0R  pages/new.org\0pages/old.org\0A  assets/x.png\0";
        let got = parse_changes(out);
        let seen: Vec<(String, GitChangeKind)> = got.into_iter().map(|c| (c.path, c.kind)).collect();
        assert_eq!(seen, vec![
            ("pages/a.org".into(), GitChangeKind::Modified),
            ("pages/b c.org".into(), GitChangeKind::Added),
            ("pages/d.org".into(), GitChangeKind::Deleted),
            ("pages/new.org".into(), GitChangeKind::Renamed),
            ("assets/x.png".into(), GitChangeKind::Added),
        ]);
    }

    #[test]
    fn ssh_never_asks() {
        let c = ssh_command("ssh", "");
        assert!(c.starts_with("ssh -o BatchMode=yes") && !c.contains("-i"));
        let c = ssh_command("ssh -p 2222", "/k/it's key");
        assert!(c.starts_with("ssh -p 2222 -o BatchMode=yes"));
        assert!(c.ends_with(" -i '/k/it'\\''s key' -o IdentitiesOnly=yes"), "{c}");
    }

    #[test]
    fn commit_pull_and_push_through_git() {
        let base = std::env::temp_dir().join(format!("orgamnesia-git-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (server, work) = (base.join("server.git"), base.join("work"));
        std::fs::create_dir_all(&work).unwrap();
        let sh = |dir: &std::path::Path, args: &[&str]| {
            assert!(Command::new("git").arg("-C").arg(dir).args(args).output().unwrap().status.success(), "{args:?}");
        };
        sh(&base, &["init", "-q", "--bare", "-b", "main", server.to_str().unwrap()]);
        sh(&work, &["init", "-q", "-b", "main"]);
        sh(&work, &["config", "user.name", "t"]);
        sh(&work, &["config", "user.email", "t@example.com"]);
        sh(&work, &["remote", "add", "origin", server.to_str().unwrap()]);
        let w = work.to_str().unwrap();

        std::fs::write(work.join("a.org"), "* a\n").unwrap();
        assert_eq!(changes(w).unwrap(), vec![GitChange { path: "a.org".into(), kind: GitChangeKind::Added }]);
        commit(w, "Premier\n\n- a.org").unwrap();
        assert!(changes(w).unwrap().is_empty());
        // No remote branch yet: pushed and tracked
        push(w, "").unwrap();
        assert!(status(w).unwrap().upstream);
        assert!(pull(w, "").is_ok());
        // Nothing to commit: git says so
        assert!(matches!(commit(w, "vide"), Err(AppError::Git(_))));
        std::fs::remove_dir_all(base).unwrap();
    }
}
