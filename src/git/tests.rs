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
