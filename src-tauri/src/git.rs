use std::process::Command;

pub use orgamnesia_core::GitStatus;

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
}
