//! The commands that only take the state, run on Tauri's mock runtime over
//! projects made for the test.

use std::collections::HashMap;
use std::path::Path;

use orgamnesia_core::{ReplaceSkip, SearchOptions};
use tauri::Manager;

use super::testing::{block_on, folder, mock_app, open_project};
use super::{app, git, links, org_bloom, pages, search};
use crate::AppState;

#[test]
fn pages_are_read_written_renamed_deleted_and_restored() {
    let app = mock_app();
    let s = || app.state::<AppState>();
    let path = open_project(&s(), "cmd-pages", &[("Une", "* Une\nvers [[Deux]]\n")]);
    let une = format!("{path}/pages/Une.org");
    assert_eq!(block_on(pages::read_file(s(), une.clone())).unwrap(), "* Une\nvers [[Deux]]\n");
    let deux = block_on(pages::create_page(s(), "Deux".into())).unwrap();
    assert!(block_on(pages::create_page(s(), "Deux".into())).is_err());
    block_on(pages::write_file(s(), deux.path.clone(), "* Deux\n".into())).unwrap();
    // Renamed: the links to it follow
    let trois = block_on(pages::rename_page(s(), deux.path, "Trois".into())).unwrap();
    assert_eq!(block_on(pages::read_file(s(), une.clone())).unwrap(), "* Une\nvers [[Trois]]\n");
    let trashed = block_on(pages::delete_page(s(), trois.path.clone())).unwrap();
    assert_eq!(block_on(pages::trash_count(s())).unwrap(), 1);
    assert_eq!(block_on(pages::restore_page(s(), trashed)).unwrap().name, "Trois");
    block_on(pages::delete_page(s(), trois.path)).unwrap();
    assert_eq!(block_on(pages::empty_trash(s())).unwrap(), 1);
    // Not a page of the project
    assert!(block_on(pages::read_file(s(), "/etc/passwd".into())).is_err());
}

#[test]
fn links_tags_and_todos() {
    let app = mock_app();
    let s = || app.state::<AppState>();
    open_project(&s(), "cmd-links", &[
        ("Une", "* TODO Écrire :projet:\nvers [[Deux]] et [[Nulle part]]\n"),
        ("Deux", "* Deux\n"),
    ]);
    assert_eq!(block_on(links::get_backlinks(s(), "Deux".into())).unwrap(), vec!["Une".to_string()]);
    let broken = block_on(links::get_broken_links(s())).unwrap();
    // `:tags:` are links too, by default
    assert_eq!(broken.iter().map(|b| b.target.as_str()).collect::<Vec<_>>(), vec!["Nulle part", "projet"]);
    assert_eq!(block_on(links::list_tags(s())).unwrap()[0].name, "projet");
    assert_eq!(block_on(links::search_tags(s(), "projet".into())).unwrap().len(), 1);
    assert_eq!(block_on(links::list_todos(s())).unwrap()[0].keyword, "TODO");
    // TODO keywords off: none
    crate::lock(&s().settings).config.todo_enabled = Some(false);
    assert!(block_on(links::list_todos(s())).unwrap().is_empty());
}

#[test]
fn search_and_replace_in_the_project() {
    let app = mock_app();
    let s = || app.state::<AppState>();
    let path = open_project(&s(), "cmd-search", &[("Une", "* Une\nchat et chaton\n")]);
    let options = SearchOptions { match_case: false, whole_word: true };
    let found = block_on(search::search_project(s(), "chat".into(), options, HashMap::new())).unwrap();
    assert_eq!(found.total, 1);
    assert!(!block_on(search::can_undo_replace(s())).unwrap());
    let done = block_on(search::replace_in_project(s(), "chat".into(), options, "chien".into(), ReplaceSkip::default())).unwrap();
    assert_eq!((done.pages, done.replacements), (1, 1));
    let page = Path::new(&path).join("pages/Une.org");
    assert_eq!(std::fs::read_to_string(&page).unwrap(), "* Une\nchien et chaton\n");
    assert!(block_on(search::can_undo_replace(s())).unwrap());
    block_on(search::undo_replace(s())).unwrap();
    assert_eq!(std::fs::read_to_string(&page).unwrap(), "* Une\nchat et chaton\n");
}

#[test]
fn git_of_the_project() {
    let app = mock_app();
    let s = || app.state::<AppState>();
    // No project open
    assert!(block_on(git::git_changes(s())).is_err());
    let path = open_project(&s(), "cmd-git", &[("Une", "* Une\n")]);
    let sh = |args: &[&str]| assert!(std::process::Command::new("git").arg("-C").arg(&path).args(args).output().unwrap().status.success());
    sh(&["init", "-q"]);
    sh(&["config", "user.email", "t@example.com"]);
    sh(&["config", "user.name", "T"]);
    assert!(block_on(git::git_status(path.clone())).unwrap().is_some());
    assert_eq!(block_on(git::git_changes(s())).unwrap().len(), 1);
    block_on(git::git_commit(s(), "Première".into(), None)).unwrap();
    assert!(block_on(git::git_changes(s())).unwrap().is_empty());
    // No remote to pull from nor push to
    assert!(block_on(git::git_pull(s())).is_err());
    assert!(block_on(git::git_push(s())).is_err());
    // Nothing to fetch from
    assert!(block_on(git::git_fetch(s())).is_ok());
    let page = format!("{path}/pages/Une.org");
    let hist = block_on(git::git_log(s(), page.clone())).unwrap();
    assert_eq!(hist.len(), 1);
    assert_eq!(block_on(git::git_show(s(), hist[0].hash.clone(), hist[0].path.clone())).unwrap(), "* Une\n");
    assert!(block_on(git::git_commit_diff(s(), hist[0].hash.clone(), hist[0].path.clone())).unwrap().contains("+* Une"));
    std::fs::write(&page, "* Une\ndeux\n").unwrap();
    assert!(block_on(git::git_diff(s(), "pages/Une.org".into(), None)).unwrap().contains("+deux"));
    block_on(git::git_add_remote(s(), "/nowhere.git".into())).unwrap();
    // The repository's own identity is read (the global one is not touched here)
    assert_eq!(block_on(git::git_identity(s())).unwrap(), ("T".to_string(), "t@example.com".to_string()));
    assert!(block_on(git::git_status(path.clone())).unwrap().unwrap().remote);
}

#[test]
fn org_bloom_builds_with_the_command_set() {
    let app = mock_app();
    let s = || app.state::<AppState>();
    assert_eq!(org_bloom::site_config(s()).unwrap(), None);
    let path = open_project(&s(), "cmd-org-bloom", &[]);
    std::fs::write(Path::new(&path).join("org-bloom.toml"), "[site]\noutput_dir = \"public\"\n").unwrap();
    let config = org_bloom::site_config(s()).unwrap().unwrap();
    assert!(config.exists && config.output_dir.unwrap().ends_with("public"));
    assert!(org_bloom::open_folder("/no/such/folder".into()).is_err());
    // Not offered on Windows
    if cfg!(windows) {
        assert!(block_on(org_bloom::build_site(s())).is_err());
        return;
    }
    // A stand-in for org-bloom, telling the options it got
    let bin = folder("cmd-org-bloom-bin").join("org-bloom");
    std::fs::write(&bin, "#!/bin/sh\necho \"Done. Site written to $3$4\"\n").unwrap();
    std::process::Command::new("chmod").arg("+x").arg(&bin).status().unwrap();
    {
        let state = s();
        let mut settings = crate::lock(&state.settings);
        settings.config.org_bloom_path = Some(bin.to_string_lossy().to_string());
        settings.config.org_bloom_zip = Some(true);
    }
    assert_eq!(block_on(org_bloom::build_site(s())).unwrap(), "--zip");
}

#[test]
fn files_and_images() {
    let app = mock_app();
    let s = || app.state::<AppState>();
    let path = open_project(&s(), "cmd-images", &[("Une", "* Une\n")]);
    let missing = block_on(app::missing_files(vec![path.clone(), "/no/such/file".into()])).unwrap();
    assert_eq!(missing, vec!["/no/such/file".to_string()]);
    let png = Path::new(&path).join("photo.png");
    std::fs::write(&png, b"png").unwrap();
    assert!(block_on(app::read_image(png.to_string_lossy().to_string())).is_ok());
    assert!(block_on(app::read_image(format!("{path}/pages/Une.org"))).is_err());
    let link = block_on(app::import_image(s(), format!("{path}/pages/Une.org"), png.to_string_lossy().to_string())).unwrap();
    assert!(link.starts_with("../assets/") && link.ends_with(".png"), "{link}");
    assert!(app::open_url("file:///etc/passwd".into()).is_err());
    // Any file goes to the assets, linked from the page
    let doc = std::path::Path::new(&path).join("Compte rendu.pdf");
    std::fs::write(&doc, "%PDF").unwrap();
    let link = block_on(app::import_file(s(), format!("{path}/pages/Une.org"), doc.to_string_lossy().to_string())).unwrap();
    assert!(link.starts_with("../assets/Compte_rendu_") && link.ends_with("_0.pdf"), "{link}");
    // A program is never opened from a link
    let script = std::path::Path::new(&path).join("x.sh");
    std::fs::write(&script, "#!/bin/sh\n").unwrap();
    assert!(app::open_file_link(script.to_string_lossy().to_string()).is_err());
    assert!(app::open_file_link(format!("{path}/missing.pdf")).is_err());
}
