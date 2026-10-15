//! Org-bloom extension: build the static site of the project with the
//! `org-bloom` command (https://github.com/thomas-louvigne/org-bloom).

use std::path::{Component, Path, PathBuf};
use std::process::Command;

use orgamnesia_core::SiteConfig;

use crate::error::AppError;

/// Config file of org-bloom, at the root of the project.
const CONFIG_FILE: &str = "org-bloom.toml";
/// Its name before org-bloom was renamed, read when the new one is missing.
const LEGACY_CONFIG_FILE: &str = "logseq-site-builder.toml";

/// Command run when none is set in the settings.
const DEFAULT_COMMAND: &str = "org-bloom";

/// Run `org-bloom build <project> <options>` (such as `--zip`, to also zip the
/// site); returns the folder the site was written to (the `output_dir` of the
/// project's `org-bloom.toml`) and the archive, or what org-bloom said.
pub fn build(project: &str, command: &str, options: &[&str]) -> Result<String, AppError> {
    let mut last_err = None;
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let path = child_path(home.as_deref(), std::env::var_os("PATH"));
    for bin in candidates(command, home) {
        let mut cmd = Command::new(&bin);
        cmd.arg("build").arg(project).args(options).current_dir(project);
        if let Some(path) = &path {
            cmd.env("PATH", path);
        }
        match cmd.output() {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                if !out.status.success() {
                    return Err(AppError::OrgBloom(failure(&String::from_utf8_lossy(&out.stderr), &stdout)));
                }
                let folder = site_folder(&stdout).unwrap_or_default();
                return Ok(match archive(&stdout) {
                    Some(archive) => format!("{folder} ({archive})"),
                    None => folder,
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => last_err = Some(bin),
            Err(e) => return Err(AppError::CannotRun { program: bin.display().to_string(), error: e.to_string() }),
        }
    }
    let tried = last_err.map(|b| b.display().to_string()).unwrap_or_else(|| command.to_string());
    Err(AppError::OrgBloomNotFound(tried))
}

/// Where org-bloom builds the site of the project: the `output_dir` of the
/// `[site]` table of its config file, resolved as org-bloom does (`~` is the
/// home folder, a relative path is from the project).
pub fn site_config(project: &str) -> SiteConfig {
    let project = Path::new(project);
    let mut config = project.join(CONFIG_FILE);
    if !config.exists() && project.join(LEGACY_CONFIG_FILE).exists() {
        config = project.join(LEGACY_CONFIG_FILE);
    }
    let text = std::fs::read_to_string(&config).ok();
    let output_dir = text.as_deref().and_then(output_dir_of).map(|dir| {
        resolve(&dir, project, std::env::var_os("HOME").map(PathBuf::from)).display().to_string()
    });
    let built_at = output_dir.as_deref().and_then(built_at);
    SiteConfig { config: config.display().to_string(), exists: text.is_some(), output_dir, built_at }
}

/// When the site in `dir` was built: the last change of the folder, which
/// org-bloom wipes and fills again on every build.
fn built_at(dir: &str) -> Option<u64> {
    let meta = std::fs::metadata(dir).ok().filter(|m| m.is_dir())?;
    let time = meta.modified().ok()?;
    time.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs())
}

/// `output_dir` of the `[site]` table of a config file.
fn output_dir_of(toml_text: &str) -> Option<String> {
    let table: toml::Table = toml_text.parse().ok()?;
    let dir = table.get("site")?.get("output_dir")?.as_str()?.trim();
    (!dir.is_empty()).then(|| dir.to_string())
}

/// The folder `dir` names, from the project, without its `.` and `..`.
fn resolve(dir: &str, project: &Path, home: Option<PathBuf>) -> PathBuf {
    let path = match (dir.strip_prefix("~/").or((dir == "~").then_some("")), home) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => PathBuf::from(dir),
    };
    let path = if path.is_absolute() { path } else { project.join(path) };
    let mut clean = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => { clean.pop(); }
            other => clean.push(other),
        }
    }
    clean
}

/// Show a folder in the file manager.
pub fn open_folder(path: &str) -> Result<(), AppError> {
    if !Path::new(path).is_dir() {
        return Err(AppError::FolderNotFound(path.to_string()));
    }
    crate::open_with_system(path)
}

/// The PATH given to org-bloom: `~/.local/bin` first, so that the tools it
/// runs (pandoc) are the ones installed there, not the older ones of the
/// system that an app started from the desktop menu would find instead. On
/// macOS, Homebrew's folders too: an app started from the Finder has only the
/// system's in its PATH.
fn child_path(home: Option<&Path>, path: Option<std::ffi::OsString>) -> Option<std::ffi::OsString> {
    let mut first = vec![home?.join(".local/bin")];
    if cfg!(target_os = "macos") {
        first.extend(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from));
    }
    let mut dirs = first.clone();
    if let Some(path) = path {
        dirs.extend(std::env::split_paths(&path).filter(|d| !first.contains(d)));
    }
    std::env::join_paths(dirs).ok()
}

/// The commands to try: the one set, else `org-bloom` from the PATH, then from
/// `~/.local/bin` (where `pip install --user` puts it, and which an app started
/// from the desktop menu may not have on its PATH).
fn candidates(command: &str, home: Option<PathBuf>) -> Vec<PathBuf> {
    let command = command.trim();
    if !command.is_empty() {
        let path = match (command.strip_prefix("~/"), &home) {
            (Some(rest), Some(home)) => home.join(rest),
            _ => PathBuf::from(command),
        };
        return vec![path];
    }
    let mut list = vec![PathBuf::from(DEFAULT_COMMAND)];
    if let Some(home) = home {
        list.push(home.join(".local/bin").join(DEFAULT_COMMAND));
    }
    list
}

/// The folder in org-bloom's last line `Done. Site written to <folder>`.
fn site_folder(stdout: &str) -> Option<String> {
    stdout.lines().rev().find_map(|l| l.trim().strip_prefix("Done. Site written to ").map(str::to_string))
}

/// The archive in org-bloom's line `Zipped to <file>` (with `--zip`).
fn archive(stdout: &str) -> Option<String> {
    stdout.lines().find_map(|l| l.trim().strip_prefix("Zipped to ").map(str::to_string))
}

/// What to show of a failed build: org-bloom's `Error: …` lines, else the end of
/// what it wrote.
fn failure(stderr: &str, stdout: &str) -> String {
    let errors: Vec<&str> = stderr.lines().filter_map(|l| l.trim().strip_prefix("Error: ")).collect();
    if !errors.is_empty() {
        return errors.join("\n");
    }
    let text = if stderr.trim().is_empty() { stdout } else { stderr };
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    lines[lines.len().saturating_sub(3)..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_bin_first_in_path() {
        let path = child_path(Some(Path::new("/home/me")), Some("/usr/bin:/home/me/.local/bin:/bin".into())).unwrap();
        assert_eq!(path, "/home/me/.local/bin:/usr/bin:/bin");
        assert_eq!(child_path(None, Some("/usr/bin".into())), None);
    }

    #[test]
    fn commands_tried() {
        let home = Some(PathBuf::from("/home/me"));
        assert_eq!(candidates("", home.clone()), vec![
            PathBuf::from("org-bloom"), PathBuf::from("/home/me/.local/bin/org-bloom"),
        ]);
        assert_eq!(candidates(" ~/bin/ob ", home), vec![PathBuf::from("/home/me/bin/ob")]);
        assert_eq!(candidates("/opt/ob", None), vec![PathBuf::from("/opt/ob")]);
    }

    #[test]
    fn site_folder_from_the_config() {
        assert_eq!(output_dir_of("[site]\ntitle = \"T\"\noutput_dir = \"../site\"\n").as_deref(), Some("../site"));
        assert_eq!(output_dir_of("[site]\n# output_dir = \"../site\"\n"), None);
        assert_eq!(output_dir_of("output_dir = \"x\"\n[site]\n"), None);
        assert_eq!(output_dir_of("not toml ["), None);
        let home = Some(PathBuf::from("/home/me"));
        let project = Path::new("/home/me/notes");
        assert_eq!(resolve("../site", project, home.clone()), PathBuf::from("/home/me/site"));
        assert_eq!(resolve("./out/", project, home.clone()), PathBuf::from("/home/me/notes/out"));
        assert_eq!(resolve("~/www", project, home.clone()), PathBuf::from("/home/me/www"));
        assert_eq!(resolve("/srv/site", project, home), PathBuf::from("/srv/site"));
    }

    #[test]
    fn reads_what_org_bloom_says() {
        let out = "Loaded config from /p/org-bloom.toml\nBuilding site from /p → /s\nDone. Site written to /s\n";
        assert_eq!(site_folder(out).as_deref(), Some("/s"));
        assert_eq!(site_folder("nothing"), None);
        assert_eq!(archive(&format!("{out}  Zipped to /s.zip\n")).as_deref(), Some("/s.zip"));
        assert_eq!(archive(out), None);
        let err = "Error: no output folder. Pass one (org-bloom PROJECT OUTPUT) or set output_dir.\n";
        assert_eq!(failure(err, ""), "no output folder. Pass one (org-bloom PROJECT OUTPUT) or set output_dir.");
        assert_eq!(failure("", "a\nb\nc\nd\n"), "b\nc\nd");
    }
}
