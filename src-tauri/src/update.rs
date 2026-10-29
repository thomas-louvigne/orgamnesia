//! New versions of the app (Tauri's updater): the `latest.json` of the latest
//! GitHub release says the version, the package of each system with its
//! signature, and the notes of every version (`packaging/release/changelog.py`).

use orgamnesia_core::VersionNotes;

/// `1.2.3` as numbers, to compare versions; `None` when it isn't one.
fn numbers(version: &str) -> Option<(u64, u64, u64)> {
    let mut n = version.trim().trim_start_matches('v').split('.').map(|p| p.parse::<u64>().ok());
    Some((n.next()??, n.next()??, n.next()??))
}

/// The notes of the versions after `current` up to `new`, newest first, from the
/// `changelog` field of `latest.json` (anything else: none).
pub fn notes_between(changelog: &serde_json::Value, current: &str, new: &str) -> Vec<VersionNotes> {
    let (Some(from), Some(to)) = (numbers(current), numbers(new)) else { return vec![] };
    let mut notes: Vec<VersionNotes> = serde_json::from_value::<Vec<VersionNotes>>(changelog.clone()).unwrap_or_default()
        .into_iter()
        .filter(|n| numbers(&n.version).is_some_and(|v| v > from && v <= to))
        .collect();
    notes.sort_by_key(|n| std::cmp::Reverse(numbers(&n.version)));
    notes
}

/// The app can replace itself with the new version: an installer or a package
/// the updater knows (Windows, macOS, AppImage, .deb). A program installed by
/// another package manager (Arch's) is only told about the new version.
pub fn can_install() -> bool {
    if cfg!(target_os = "linux") { tauri::utils::platform::bundle_type().is_some() } else { true }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_since_the_running_version() {
        let changelog = serde_json::json!([
            { "version": "0.22.0", "date": "2026-11-02", "en": ["C"], "fr": ["C fr"] },
            { "version": "0.21.0", "en": ["B"] },
            { "version": "0.20.0", "en": ["A"] },
        ]);
        let got = notes_between(&changelog, "0.20.0", "0.22.0");
        assert_eq!(got.iter().map(|n| n.version.as_str()).collect::<Vec<_>>(), ["0.22.0", "0.21.0"]);
        assert_eq!((got[0].fr.clone(), got[1].fr.clone()), (vec!["C fr".to_string()], vec![]));
        assert!(notes_between(&changelog, "0.22.0", "0.22.0").is_empty());
        assert!(notes_between(&serde_json::json!("old"), "0.20.0", "0.22.0").is_empty());
        assert_eq!(numbers("v0.10.2"), Some((0, 10, 2)));
        assert!(numbers("0.10") .is_none());
    }
}
