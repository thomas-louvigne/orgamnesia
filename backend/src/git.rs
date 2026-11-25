//! The git extension: the state of the project, pull / commit / push, the
//! history of a page, and setting up a repository, by running the `git` command
//! (no git library).

use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Mutex;

use crate::error::AppError;
pub use orgamnesia_core::{GitChange, GitChangeKind, GitCommit, GitStatus};

/// Held by the commands writing to the repository (fetch, pull, commit, push…):
/// one at a time, so that a fetch in the background never makes a pull fail on
/// a locked ref.
pub static WRITING: Mutex<()> = Mutex::new(());

/// The passphrase of the SSH key, as written in the app: in memory until it is
/// closed, never on disk.
pub static PASSPHRASE: Mutex<Option<String>> = Mutex::new(None);

/// How the passphrase reaches this program when ssh runs it as its `SSH_ASKPASS`.
const ASKPASS_VAR: &str = "ORGAMNESIA_SSH_PASSPHRASE";

/// What to answer ssh asking `prompt`: the passphrase of a key, and nothing else
/// (it must not go to a server asking for a password).
fn askpass_reply<'a>(prompt: &str, passphrase: &'a str) -> Option<&'a str> {
    prompt.to_lowercase().contains("passphrase").then_some(passphrase)
}

/// Run by ssh as its `SSH_ASKPASS` (see `remote`): answers, and the program stops
/// there. `false` when started as the app.
pub fn answer_askpass() -> bool {
    use std::io::Write;
    let Ok(passphrase) = std::env::var(ASKPASS_VAR) else { return false };
    let prompt = std::env::args().nth(1).unwrap_or_default();
    match askpass_reply(&prompt, &passphrase) {
        Some(reply) => { let _ = writeln!(std::io::stdout(), "{reply}"); }
        None => std::process::exit(1),
    }
    true
}

/// What `git init` writes in `.gitignore`: the backups of the editors.
const GITIGNORE: &str = "# Editor backups\n*~\n\\#*\\#\n.#*\n.DS_Store\n";

/// Git state of `path`; `None` when git itself can't be run.
pub fn status(path: &str) -> Option<GitStatus> {
    // `.`: the project's folder, also when it is only part of the repository
    let out = git(path).args(["status", "--porcelain=v2", "--branch", "--", "."])
        .env("GIT_OPTIONAL_LOCKS", "0").stdin(Stdio::null())
        .output().ok()?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        // Any other failure is said, not taken for a folder without git
        let error = (!err.to_lowercase().contains("not a git repository")).then_some(err);
        return Some(GitStatus { error, ..Default::default() });
    }
    let mut s = parse(&String::from_utf8_lossy(&out.stdout));
    s.merging = in_progress(path);
    s.remote = run(git(path).arg("remote")).is_ok_and(|r| !r.trim().is_empty());
    s.unpushed = unpushed(path, s.upstream);
    if s.conflicts > 0 {
        s.conflict_files = run(git(path).args(["diff", "--name-only", "--relative", "--diff-filter=U", "-z"]))
            .map(|o| split_z(&o)).unwrap_or_default()
            .into_iter().map(|r| format!("{}/{r}", path.trim_end_matches('/'))).collect();
    }
    Some(s)
}

/// The files in conflict on both sides (`UU`, `AA`) whose conflict markers are
/// all gone, fixed by hand, are marked resolved (`git add`). Returns how many.
pub fn mark_resolved(path: &str) -> usize {
    let Ok(out) = run(git(path).args(["status", "--porcelain=v2", "-z"]).env("GIT_OPTIONAL_LOCKS", "0")) else { return 0 };
    let fixed: Vec<&str> = out.split('\0')
        .filter_map(|e| {
            // u <XY> <sub> <m1> <m2> <m3> <mW> <h1> <h2> <h3> <path>, from the root
            let f: Vec<&str> = e.splitn(11, ' ').collect();
            (f.len() == 11 && f[0] == "u" && matches!(f[1], "UU" | "AA")).then(|| f[10])
        })
        .collect();
    if fixed.is_empty() { return 0; }
    let Ok(root) = toplevel(path) else { return 0 };
    let fixed: Vec<&str> = fixed.into_iter()
        .filter(|f| std::fs::read_to_string(Path::new(&root).join(f)).is_ok_and(|t| !has_markers(&t)))
        .collect();
    if fixed.is_empty() || run(git(&root).args(["add", "--"]).args(&fixed)).is_err() { return 0; }
    fixed.len()
}

/// `text` still holds conflict markers.
fn has_markers(text: &str) -> bool {
    text.lines().any(|l| l.starts_with("<<<<<<<") || l.starts_with(">>>>>>>"))
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
        } else if line.starts_with("u ") {
            s.conflicts += 1;
        } else if !line.starts_with('#') && !line.is_empty() {
            s.changes += 1;
        }
    }
    s
}

/// A merge or a rebase stopped in `path`'s repository.
fn in_progress(path: &str) -> bool {
    let Ok(out) = run(git(path).args(["rev-parse", "--git-path", "MERGE_HEAD", "--git-path", "rebase-merge", "--git-path", "rebase-apply"])) else {
        return false;
    };
    out.lines().any(|p| Path::new(path).join(p.trim()).exists())
}

/// Full paths of the files of `path` differing from the remote branch (from the
/// last commit without one), untracked ones included.
fn unpushed(path: &str, upstream: bool) -> Vec<String> {
    let base = if upstream { "@{u}" } else { "HEAD" };
    // Paths relative to `path`, the files outside it left out
    let mut rel: Vec<String> = run(git(path).args(["diff", "--name-only", "--relative", "-z", base]).env("GIT_OPTIONAL_LOCKS", "0"))
        .map(|o| split_z(&o)).unwrap_or_default();
    rel.extend(run(git(path).args(["ls-files", "--others", "--exclude-standard", "-z"])).map(|o| split_z(&o)).unwrap_or_default());
    rel.sort();
    rel.dedup();
    let root = path.trim_end_matches('/');
    rel.into_iter().map(|r| format!("{root}/{r}")).collect()
}

fn split_z(out: &str) -> Vec<String> {
    out.split('\0').map(str::trim).filter(|p| !p.is_empty()).map(String::from).collect()
}

/// Files of the project's folder changed since the last commit (untracked ones
/// included), relative to the root of the repository.
pub fn changes(path: &str) -> Result<Vec<GitChange>, AppError> {
    let out = run(git(path).args(["status", "--porcelain=v1", "-z", "--untracked-files=all", "--", "."]).env("GIT_OPTIONAL_LOCKS", "0"))?;
    Ok(parse_changes(&out))
}

/// What changed in `file` (relative to the root of the repository) since the
/// last commit, as a unified diff; a new file is shown whole, added.
pub fn diff(path: &str, file: &str, old_path: Option<&str>) -> Result<String, AppError> {
    let root = toplevel(path)?;
    let mut c = git(&root);
    c.args(["diff", "--no-color", "-M", "HEAD", "--"]).arg(file);
    if let Some(old) = old_path { c.arg(old); }
    // A failure (no commit yet aside) is said, not shown as the whole file added
    let out = match run(&mut c) {
        Ok(out) => out,
        Err(AppError::Git(e)) if no_commit_yet(&e) => String::new(),
        Err(e) => return Err(e),
    };
    if !out.is_empty() {
        return Ok(out);
    }
    // Untracked, or no commit yet
    let text = std::fs::read_to_string(Path::new(&root).join(file)).unwrap_or_default();
    Ok(text.lines().map(|l| format!("+{l}\n")).collect())
}

/// Git failed because the repository has no commit yet (`HEAD` names none).
fn no_commit_yet(err: &str) -> bool {
    err.contains("bad revision 'HEAD'") || err.contains("unknown revision")
}

/// The root of the repository holding `path`.
fn toplevel(path: &str) -> Result<String, AppError> {
    run(git(path).args(["rev-parse", "--show-toplevel"]))
}

/// Commit the changes of the project's folder with `message` (never the rest of
/// a repository it is only part of); with `files` (relative to the root of the
/// repository), only those. Returns what git says.
pub fn commit(path: &str, message: &str, files: Option<&[String]>) -> Result<String, AppError> {
    let mut c = git(path);
    match files {
        None => { run(git(path).args(["add", "-A", "--", "."]))?; c.args(["commit", "--file", "-"]); }
        Some(files) => {
            // Paths from the root, wherever the project is in the repository
            let root = toplevel(path)?;
            run(git(&root).args(["add", "-A", "--"]).args(files))?;
            c = git(&root);
            c.args(["commit", "--file", "-", "--"]).args(files);
        }
    }
    let mut child = c.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        stdin.write_all(message.as_bytes())?;
    }
    output(child.wait_with_output()?)
}

/// `git fetch`, so that the commits to pull are counted; nothing to do without
/// a remote.
pub fn fetch(path: &str, ssh_key: &str) -> Result<String, AppError> {
    if run(git(path).arg("remote"))?.trim().is_empty() {
        return Ok(String::new());
    }
    run(remote(git(path), path, ssh_key).args(["fetch", "--quiet"]))
}

/// Pull, local changes put aside meanwhile (`--autostash`). Moves forward when
/// it can; when the histories diverged and `rebase`, the local commits are
/// replayed on the remote ones (`--rebase`), and on a conflict everything is put
/// back as it was. Returns what git says.
pub fn pull(path: &str, ssh_key: &str, rebase: bool) -> Result<String, AppError> {
    match run(remote(git(path), path, ssh_key).args(["pull", "--ff-only", "--autostash"])) {
        Err(AppError::Git(e)) if rebase && diverged(&e) => {
            run(remote(git(path), path, ssh_key).args(["pull", "--rebase", "--autostash"])).inspect_err(|_| {
                if in_progress(path) { let _ = run(git(path).args(["rebase", "--abort"])); }
            })
        }
        r => r,
    }
}

/// Git refused to pull because the histories diverged.
fn diverged(err: &str) -> bool {
    let e = err.to_lowercase();
    e.contains("not possible to fast-forward") || e.contains("divergent") || e.contains("diverging")
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
        return Err(AppError::NoRemote);
    };
    run(remote(git(path), path, ssh_key).args(["push", "--set-upstream", name, "HEAD"]))
}

/// The branches to switch to, sorted: the local ones, and the remote ones not
/// here yet under their local name (`git switch` then makes it, tracking).
pub fn branches(path: &str) -> Result<Vec<String>, AppError> {
    let out = run(git(path).args(["for-each-ref", "--format=%(refname)", "refs/heads", "refs/remotes"]))?;
    Ok(parse_branches(&out))
}

fn parse_branches(out: &str) -> Vec<String> {
    let mut names: Vec<String> = out.lines().filter_map(|r| {
        if let Some(local) = r.strip_prefix("refs/heads/") { return Some(local.to_string()); }
        // refs/remotes/<remote>/<branch>; `<remote>/HEAD` is no branch
        let (_, branch) = r.strip_prefix("refs/remotes/")?.split_once('/')?;
        (branch != "HEAD").then(|| branch.to_string())
    }).collect();
    names.sort();
    names.dedup();
    names
}

/// Switch to the branch `name`; with `create`, a new one from here. The
/// uncommitted changes come along; git refuses when they would be overwritten.
pub fn switch(path: &str, name: &str, create: bool) -> Result<String, AppError> {
    let name = name.trim();
    // Never read as an option
    if name.is_empty() || name.starts_with('-') {
        return Err(AppError::Git(format!("not a valid branch name: '{name}'")));
    }
    let mut c = git(path);
    c.arg("switch");
    if create { c.arg("-c"); }
    run(c.arg(name))
}

/// The commits that changed `file` (a full path), newest first, following its renames.
pub fn log(path: &str, file: &str) -> Result<Vec<GitCommit>, AppError> {
    let root = toplevel(path)?;
    let out = run(git(&root).args(["log", "--follow", "--name-only", "--format=%x1e%H%x1f%at%x1f%an%x1f%s", "--"]).arg(file))?;
    Ok(parse_log(&out))
}

/// Read the output of `git log --name-only --format=%x1e%H%x1f%at%x1f%an%x1f%s`.
fn parse_log(out: &str) -> Vec<GitCommit> {
    out.split('\x1e').filter_map(|rec| {
        let mut lines = rec.lines().filter(|l| !l.trim().is_empty());
        let mut head = lines.next()?.split('\x1f');
        let (hash, date, author, subject) = (head.next()?, head.next()?, head.next()?, head.next().unwrap_or_default());
        Some(GitCommit {
            hash: hash.to_string(),
            date: date.parse().unwrap_or(0),
            author: author.to_string(),
            subject: subject.to_string(),
            path: lines.next_back()?.trim().to_string(),
        })
    }).collect()
}

/// What `commit` changed in `file` (relative to the root of the repository).
pub fn commit_diff(path: &str, hash: &str, file: &str) -> Result<String, AppError> {
    check_hash(hash)?;
    let root = toplevel(path)?;
    run(git(&root).args(["show", "--no-color", "--format=", "-M", hash, "--", file]))
}

/// The text of `file` (relative to the root of the repository) in `commit`.
pub fn show(path: &str, hash: &str, file: &str) -> Result<String, AppError> {
    check_hash(hash)?;
    let root = toplevel(path)?;
    let out = git(&root).arg("show").arg(format!("{hash}:{file}")).stdin(Stdio::null()).output()?;
    if !out.status.success() {
        return Err(AppError::Git(String::from_utf8_lossy(&out.stderr).trim().to_string()));
    }
    // Not trimmed: the page comes back as it was
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Ok when `hash` is a commit hash (hexadecimal): never read by git as an option
/// (`--output=<file>` would have `git show` write a file anywhere).
fn check_hash(hash: &str) -> Result<(), AppError> {
    if !hash.is_empty() && hash.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(AppError::Git(format!("not a commit hash: '{hash}'")))
    }
}

/// The name and email git signs the commits of `path` with (empty when not set).
pub fn identity(path: &str) -> (String, String) {
    let get = |key: &str| run(git(path).args(["config", "--get", key])).unwrap_or_default();
    (get("user.name"), get("user.email"))
}

/// Set the name and email git signs the commits with, for every repository of
/// the user (`git config --global`).
pub fn set_identity(path: &str, name: &str, email: &str) -> Result<(), AppError> {
    run(git(path).args(["config", "--global", "user.name", name.trim()]))?;
    run(git(path).args(["config", "--global", "user.email", email.trim()]))?;
    Ok(())
}

/// Make `path` a repository, with a `.gitignore` for the editors' backups when
/// it has none.
pub fn init(path: &str) -> Result<String, AppError> {
    let out = run(git(path).arg("init"))?;
    let ignore = Path::new(path).join(".gitignore");
    if !ignore.exists() {
        std::fs::write(ignore, GITIGNORE)?;
    }
    Ok(out)
}

/// Add the remote `origin` at `url`.
pub fn add_remote(path: &str, url: &str) -> Result<String, AppError> {
    run(git(path).args(["remote", "add", "origin", url.trim()]))
}

/// Clone `url` into a new folder of `parent`, named after the repository;
/// returns the folder.
pub fn clone(parent: &str, url: &str, ssh_key: &str) -> Result<String, AppError> {
    let url = url.trim();
    let name = repo_name(url).ok_or_else(|| AppError::Git(format!("not a repository address: {url}")))?;
    let parent = crate::expand_home(parent.trim());
    let dest = Path::new(&parent).join(&name);
    if dest.read_dir().is_ok_and(|mut d| d.next().is_some()) {
        return Err(AppError::Git(format!("destination path '{}' already exists and is not an empty directory", dest.display())));
    }
    std::fs::create_dir_all(&parent)?;
    run(remote(git(&parent), &parent, ssh_key).args(["clone", "--quiet", "--", url]).arg(&name))?;
    Ok(crate::slash(&dest))
}

/// The folder name of a clone of `url`: its last part, without `.git`.
fn repo_name(url: &str) -> Option<String> {
    let last = url.trim_end_matches('/').rsplit(['/', ':', '\\']).next()?;
    let name = last.strip_suffix(".git").unwrap_or(last);
    (!name.is_empty() && name != "." && name != "..").then(|| name.to_string())
}

fn git(path: &str) -> Command {
    let mut c = crate::command(crate::git_program());
    // The files as they are in the repository, never with the `\r\n` of
    // Windows that git there may be set to write (`core.autocrlf`)
    c.arg("-C").arg(path).args(["-c", "core.autocrlf=false"]).env("LC_ALL", "C");
    c
}

/// A git command that talks to the remote: it must never wait for an answer no
/// one can give (password, unknown host), so git and ssh run in batch mode; on
/// Windows, https identifiers are asked in a window. With `ssh_key`, ssh uses that
/// key only. With a passphrase written in the app, ssh gets it from this program.
fn remote(mut c: Command, path: &str, ssh_key: &str) -> Command {
    // The ssh command of the repository's settings, if any, else `ssh`; on
    // Windows, its own OpenSSH, which uses Windows' ssh agent (MinGit's doesn't)
    let windows_ssh = std::path::Path::new("C:/Windows/System32/OpenSSH/ssh.exe");
    let base = run(git(path).args(["config", "--get", "core.sshCommand"])).ok()
        .map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
        .unwrap_or_else(|| if cfg!(windows) && windows_ssh.is_file() { crate::slash(windows_ssh) } else { "ssh".into() });
    let passphrase = crate::lock(&PASSPHRASE).clone();
    let askpass = passphrase.zip(std::env::current_exe().ok());
    c.env("GIT_SSH_COMMAND", ssh_command(&base, ssh_key, askpass.is_some()));
    if let Some((passphrase, program)) = askpass {
        c.env("SSH_ASKPASS", program).env("SSH_ASKPASS_REQUIRE", "force").env(ASKPASS_VAR, passphrase);
    }
    if cfg!(windows) {
        // No terminal to wait on (no console window): an https identifier is
        // asked by Git Credential Manager in a window of its own, signing in
        // with the browser on GitHub, GitLab, Bitbucket; a name and password
        // (or token) elsewhere (Codeberg, a server of one's own…)
        c.env("GCM_INTERACTIVE", "always").env("GCM_GUI_PROMPT", "true");
    } else {
        c.env("GIT_TERMINAL_PROMPT", "0");
    }
    c
}

/// The command git runs for ssh (read by a shell, hence the quoting). With
/// `askpass`, the passphrase of a key is the one thing it may ask.
fn ssh_command(base: &str, ssh_key: &str, askpass: bool) -> String {
    let asks = if askpass {
        "-o PasswordAuthentication=no -o KbdInteractiveAuthentication=no -o NumberOfPasswordPrompts=1"
    } else {
        "-o BatchMode=yes"
    };
    let mut cmd = format!("{base} {asks} -o ConnectTimeout=20 -o StrictHostKeyChecking=accept-new");
    let key = ssh_key.trim();
    if !key.is_empty() {
        let key = crate::expand_home(key);
        cmd.push_str(&format!(" -i '{}' -o IdentitiesOnly=yes", key.replace('\'', "'\\''")));
    }
    cmd
}

/// Run `c`; its output, or its error message when it fails.
fn run(c: &mut Command) -> Result<String, AppError> {
    output(c.stdin(Stdio::null()).output()?)
}

fn output(out: std::process::Output) -> Result<String, AppError> {
    let text = |b: &[u8]| String::from_utf8_lossy(b).trim().to_string();
    if out.status.success() {
        // Only the end: `git status` starts with a column of its own (` M a.org`)
        Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
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
        let mut old_path = None;
        let kind = match xy {
            "??" => GitChangeKind::Added,
            _ if xy.contains('R') || xy.contains('C') => {
                old_path = entries.next().map(String::from);
                GitChangeKind::Renamed
            }
            _ if xy.contains('D') => GitChangeKind::Deleted,
            _ if xy.starts_with('A') => GitChangeKind::Added,
            _ => GitChangeKind::Modified,
        };
        changes.push(GitChange { path: path.to_string(), kind, old_path });
    }
    changes
}

#[cfg(test)]
mod tests;
