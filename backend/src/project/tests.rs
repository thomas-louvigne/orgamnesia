use super::*;
use orgamnesia_core::{Hashtags, OrgTags, Prefs, TagCount, TodoKeywords};

/// A fresh project folder holding the given pages.
pub(super) fn project(test: &str, pages: &[(&str, &str)]) -> (PathBuf, Project) {
    let dir = crate::testing::folder(&format!("project-{test}"));
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    for (name, content) in pages {
        std::fs::write(dir.join("pages").join(format!("{name}.org")), content).unwrap();
    }
    let p = Project::open(dir.to_str().unwrap(), Hashtags::Dashes.into(), Formats::default(), "journals", "templates").unwrap();
    (dir, p)
}

#[test]
fn images_are_copied_to_the_assets() {
    let (dir, p) = project("images", &[("a", "* a\n")]);
    let src = dir.join("Ma photo.PNG");
    std::fs::write(&src, b"png").unwrap();
    let link = p.import_image(&path_of(&p, "a"), src.to_str().unwrap()).unwrap();
    let file = link.strip_prefix("../assets/").unwrap();
    assert!(file.starts_with("Ma_photo_") && file.ends_with("_0.png"));
    assert_eq!(std::fs::read(dir.join("assets").join(file)).unwrap(), b"png");
    let text = dir.join("notes.txt");
    std::fs::write(&text, b"x").unwrap();
    assert!(matches!(p.import_image(&path_of(&p, "a"), text.to_str().unwrap()), Err(AppError::NotAnImage(_))));
    let _ = std::fs::remove_dir_all(dir);
}

pub(super) fn path_of(p: &Project, name: &str) -> String {
    p.files().into_iter().find(|f| f.name == name).unwrap().path
}

#[test]
fn refuses_paths_outside_the_pages() {
    let (dir, p) = project("outside", &[("a", "* a\n")]);
    assert!(p.read(&path_of(&p, "a")).is_ok());
    for bad in ["/etc/passwd", "/tmp/x.org", &format!("{}/pages/../secret.org", dir.display())] {
        assert!(matches!(p.read(bad), Err(AppError::NotAPage(_))), "{bad}");
    }
    assert!(matches!(valid_name("../x"), Err(AppError::InvalidName(_))));
    assert!(matches!(valid_name("  "), Err(AppError::InvalidName(_))));
    for bad in ["Réunion: 12h", "a?", "a*b", "a|b", "\"a\"", "<a>"] {
        assert!(matches!(valid_name(bad), Err(AppError::InvalidName(_))), "{bad}");
    }
    assert_eq!(valid_name(" Réunion de 12h ").unwrap(), "Réunion de 12h");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn pages_in_folders_named_by_their_path() {
    let (dir, mut p) = project("folders", &[("a", "voir [[Jeux/Quêtes/Dragon]] et [[Jeux/Règles]]\n")]);
    let prefs = Prefs::default();
    let tags: TagSyntax = Hashtags::Dashes.into();
    // Made with their folders, titled by their own name
    let dragon = p.create("Jeux/Quêtes/Dragon", None, tags).unwrap();
    assert_eq!(dragon.name, "Jeux/Quêtes/Dragon");
    assert_eq!(p.read(&dragon.path).unwrap(), "* Dragon\n");
    p.create("Jeux/Règles", None, tags).unwrap();
    assert!(p.broken_links(false).is_empty());
    assert_eq!(p.backlinks(&dragon.path, false), vec!["a"]);
    p.create_folder("Vide/Sous").unwrap();
    assert_eq!(p.folders(), ["Jeux", "Jeux/Quêtes", "Vide", "Vide/Sous"]);
    for bad in ["../x", "Jeux/../x", ".cache/x", "a//b", "a/ b"] {
        assert!(matches!(p.create(bad, None, tags), Err(AppError::InvalidName(_))), "{bad}");
    }
    // Read again from disk: the same names; the templates', journal's, assets' folders left out
    std::fs::create_dir_all(dir.join("templates")).unwrap();
    std::fs::write(dir.join("templates/T.org"), "").unwrap();
    std::fs::create_dir_all(dir.join("pages/.git")).unwrap();
    std::fs::write(dir.join("pages/.git/x.org"), "").unwrap();
    let again = Project::open(dir.to_str().unwrap(), tags, Formats::default(), "journals", "templates").unwrap();
    assert_eq!(again.files().iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["Jeux/Quêtes/Dragon", "Jeux/Règles", "a"]);
    assert!(matches!(p.read(&format!("{}/pages/.git/x.org", dir.display())), Err(AppError::NotAPage(_))));

    // A page moved: the links follow
    let moved = p.rename(&dragon.path, "Bêtes/Dragon", &prefs).unwrap();
    assert!(moved.path.ends_with("pages/Bêtes/Dragon.org"));
    assert_eq!(std::fs::read_to_string(path_of(&p, "a")).unwrap(), "voir [[Bêtes/Dragon]] et [[Jeux/Règles]]\n");
    // A folder renamed: its pages, their links, and what else it holds
    std::fs::write(dir.join("pages/Jeux/carte.png"), "png").unwrap();
    assert!(matches!(p.rename_folder("Jeux", "Bêtes", &prefs), Err(AppError::PageExists(_))));
    p.rename_folder("Jeux", "JdR/Jeux", &prefs).unwrap();
    assert_eq!(std::fs::read_to_string(path_of(&p, "a")).unwrap(), "voir [[Bêtes/Dragon]] et [[JdR/Jeux/Règles]]\n");
    assert!(dir.join("pages/JdR/Jeux/carte.png").is_file() && !dir.join("pages/Jeux").exists());
    // A folder deleted: its pages in the trash, back in their folder when restored
    let trashed = p.delete_folder("Bêtes").unwrap();
    assert_eq!(trashed.len(), 1);
    assert!(!dir.join("pages/Bêtes").exists() && p.files().iter().all(|f| f.name != "Bêtes/Dragon"));
    let back = p.restore(&trashed[0], tags).unwrap();
    assert_eq!(back.name, "Bêtes/Dragon");
    assert!(back.path.ends_with("pages/Bêtes/Dragon.org"));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn templates_are_read_and_written_but_are_no_pages() {
    let (dir, mut p) = project("templates", &[("a", "* a\n")]);
    assert!(p.templates().is_empty());
    let tpl = p.create_template("Réunion", Format::Org, "* Notes [[a]]\n").unwrap();
    assert!(matches!(p.create_template("Réunion", Format::Org, ""), Err(AppError::PageExists(_))));
    assert!(matches!(p.create_template("../x", Format::Org, ""), Err(AppError::InvalidName(_))));
    assert_eq!(p.templates(), vec![tpl.clone()]);
    assert_eq!(p.read(&tpl.path).unwrap(), "* Notes [[a]]\n");
    p.write(&tpl.path, "* Ordre du jour [[a]]\n".into(), Hashtags::Dashes.into()).unwrap();
    // Not a page: not listed, links from it not counted, no outside change seen
    assert_eq!(p.files().len(), 1);
    assert!(p.backlinks(&path_of(&p, "a"), false).is_empty());
    assert!(p.refresh(Hashtags::Dashes.into()).unwrap().is_none());
    // Another folder, as Obsidian's; a path leaving the project is not taken
    p.set_templates("_meta/Templates");
    assert!(p.templates().is_empty());
    let other = p.create_template("Jour", Format::Org, "").unwrap();
    assert!(other.path.ends_with("_meta/Templates/Jour.org") && p.read(&other.path).is_ok());
    assert!(matches!(p.read(&tpl.path), Err(AppError::NotAPage(_))));
    assert!(files::templates_dir("/p", "../x").ends_with("p/templates") && files::templates_dir("/p", "/etc").ends_with("p/templates"));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn links_follow_writes_creations_and_deletions() {
    let (dir, mut p) = project("links", &[("a", "voir [[b]] et [[c]]\n")]);
    assert_eq!(p.broken_links(false).len(), 2);

    p.create("b", None, Hashtags::Dashes.into()).unwrap();
    assert_eq!(p.backlinks(&path_of(&p, "b"), false), vec!["a"]);
    // a → b only: c doesn't exist
    let g = p.graph(false);
    assert_eq!(g.pages.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), vec!["a", "b"]);
    assert_eq!(g.links, vec![(0, 1)]);
    assert_eq!(p.broken_links(false).iter().map(|b| b.target.as_str()).collect::<Vec<_>>(), vec!["c"]);
    assert!(matches!(p.create("b", None, Hashtags::Dashes.into()), Err(AppError::PageExists(_))));

    p.write(&path_of(&p, "a"), "plus rien\n".into(), Hashtags::Dashes.into()).unwrap();
    assert!(p.backlinks(&path_of(&p, "b"), false).is_empty());

    p.delete(&path_of(&p, "b")).unwrap();
    assert_eq!(p.files().len(), 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn deleted_pages_go_to_the_trash_and_come_back() {
    let (dir, mut p) = project("trash", &[("a", "* a\n"), ("b", "[[a]]\n")]);
    let trashed = p.delete(&path_of(&p, "a")).unwrap();
    assert_eq!(p.files().len(), 1);
    assert!(Path::new(&trashed).exists() && trashed.ends_with("-a.org"));
    assert!(dir.join(".trash/.gitignore").exists());
    assert_eq!(p.broken_links(false).len(), 1);

    let back = p.restore(&trashed, Hashtags::Dashes.into()).unwrap();
    assert_eq!(back.name, "a");
    assert_eq!(p.read(&back.path).unwrap(), "* a\n");
    assert!(p.broken_links(false).is_empty());
    // Only from the trash, and not over a page of the same name
    assert!(matches!(p.restore(&back.path, Hashtags::Dashes.into()), Err(AppError::NotAPage(_))));
    let again = p.delete(&back.path).unwrap();
    p.create("a", None, Hashtags::Dashes.into()).unwrap();
    assert!(matches!(p.restore(&again, Hashtags::Dashes.into()), Err(AppError::PageExists(_))));

    // Emptying: the old ones, then the oldest beyond the size, then all
    std::fs::write(dir.join(".trash/1000-vieux.org"), "").unwrap();
    std::fs::write(dir.join(".trash/2000-moins-vieux.org"), "x".repeat(100)).unwrap();
    std::fs::write(dir.join(".trash/3000-recent.org"), "x".repeat(100)).unwrap();
    let recent = format!("{}-neuf.org", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis());
    std::fs::write(dir.join(".trash").join(recent), "x".repeat(10)).unwrap();
    assert_eq!(p.trash_count().unwrap(), 5);
    // No limit at all: nothing goes
    assert_eq!(p.prune_trash(None, 0).unwrap(), 0);
    // Over 150 bytes (214 here): the oldest go until it fits, whatever their size
    assert_eq!(p.prune_trash(None, 150).unwrap(), 2);
    assert!(dir.join(".trash/3000-recent.org").exists() && !dir.join(".trash/2000-moins-vieux.org").exists());
    assert_eq!(p.prune_trash(Some(30), 0).unwrap(), 1);
    assert_eq!(p.empty_trash().unwrap(), 2);
    assert_eq!(p.trash_count().unwrap(), 0);
    assert!(dir.join(".trash/.gitignore").exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn lists_the_todos_of_every_page() {
    let (dir, p) = project("todos", &[("a", "* TODO un\n** DONE deux\n"), ("b", "#+TODO: A | B\n* A trois\n* TODO pas ici\n")]);
    std::fs::write(dir.join("pages/c.md"), "# TODO quatre\n- [x] cinq\n").unwrap();
    let mut p = p;
    p.refresh(Hashtags::Dashes.into()).unwrap();
    let todos = p.todos(&TodoKeywords::parse("TODO | DONE"), "", OrgTags::default());
    let seen: Vec<(&str, &str, bool, usize)> = todos.iter().map(|t| (t.page.as_str(), t.title.as_str(), t.done, t.line)).collect();
    assert_eq!(seen, vec![("a", "un", false, 0), ("a", "deux", true, 1), ("b", "trois", false, 1), ("c", "quatre", false, 0), ("c", "cinq", true, 1)]);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn todos_of_a_tag_and_the_agenda() {
    let (dir, p) = project("agenda", &[
        ("a", "* Projet :travail:\n** TODO un\nSCHEDULED: <2026-11-24 Tue>\n* TODO deux :perso:\n"),
        ("b", "#+FILETAGS: :travail:\n* TODO trois\n"),
        ("2026-11-24", "- <2026-11-24 Tue 09:30> idée\n"),
    ]);
    let k = TodoKeywords::parse("TODO | DONE");
    let seen = |q: &str| p.todos(&k, q, OrgTags::default()).into_iter().map(|t| (t.title, t.dated)).collect::<Vec<_>>();
    assert_eq!(seen("travail"), vec![("un".to_string(), true), ("trois".to_string(), false)]);
    assert_eq!(seen("perso"), vec![("deux".to_string(), false)]);
    assert_eq!(seen("").len(), 3);
    let today = orgamnesia_core::agenda::day_of(2026, 11, 24);
    let a = p.agenda(Some(&k), today, 1, today);
    // The day's page first, the note captured at 09:30, then the rest
    assert_eq!(a.iter().map(|h| (h.page.as_str(), h.title.as_str(), h.line)).collect::<Vec<_>>(),
               vec![("2026-11-24", "2026-11-24", 0), ("2026-11-24", "idée", 0), ("a", "un", 1)]);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn rename_rewrites_links() {
    let (dir, mut p) = project("rename", &[("old", "* old\n"), ("other", "lien [[old]] #old\n")]);
    let nf = p.rename(&path_of(&p, "old"), "new", &Prefs::default()).unwrap();
    assert_eq!(nf.name, "new");
    assert_eq!(p.read(&path_of(&p, "other")).unwrap(), "lien [[new]] #new\n");
    assert_eq!(p.backlinks(&path_of(&p, "new"), false), vec!["other"]);
    assert!(p.broken_links(false).is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn aliases_are_names_of_the_page() {
    let (dir, mut p) = project("aliases", &[("Dragon", "#+ALIAS: Drake\n* Dragon\n"), ("other", "[[Drake]] [[Wyrm]]\n")]);
    let dragon = path_of(&p, "Dragon");
    assert_eq!(p.backlinks(&dragon, false), vec!["other"]);
    assert_eq!(p.broken_links(false).iter().map(|b| b.target.as_str()).collect::<Vec<_>>(), vec!["Wyrm"]);
    // An alias written later is taken in
    p.write(&dragon, "#+ALIAS: Drake, Wyrm\n* Dragon\n".into(), Hashtags::Dashes.into()).unwrap();
    assert!(p.broken_links(false).is_empty());
    // Renamed, the page keeps the links naming its aliases
    p.rename(&dragon, "Grand dragon", &Prefs::default()).unwrap();
    assert_eq!(p.read(&path_of(&p, "other")).unwrap(), "[[Drake]] [[Wyrm]]\n");
    assert_eq!(p.backlinks(&path_of(&p, "Grand dragon"), false), vec!["other"]);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn unlinked_mentions_of_a_page() {
    let (dir, p) = project("mentions", &[("Dragon", "#+ALIAS: Wyrm\n* Dragon\n"), ("other", "Un dragon.\n[[Dragon]] et le wyrm\n"), ("rien", "dragons\n")]);
    let hits = p.unlinked_mentions(&path_of(&p, "Dragon"), Hashtags::Dashes.into(), true);
    let seen: Vec<(&str, usize, &str, &str)> = hits.iter().map(|h| (h.page.as_str(), h.line, h.text.as_str(), h.target.as_str())).collect();
    assert_eq!(seen, [("other", 0, "dragon", "Dragon"), ("other", 1, "wyrm", "Wyrm")]);
    assert_eq!(hits[1].context, "[[Dragon]] et le wyrm");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn rename_follows_headline_tags_and_skips_the_rest() {
    let (dir, mut p) = project("rename-tags", &[("old", "* old\n"), ("tagged", "* Titre :old:\n"), ("plain", "* plain\n")]);
    let plain = path_of(&p, "plain");
    let before = std::fs::metadata(&plain).unwrap().modified().unwrap();
    p.rename(&path_of(&p, "old"), "new", &Prefs::default()).unwrap();
    assert_eq!(p.read(&path_of(&p, "tagged")).unwrap(), "* Titre :new:\n");
    assert_eq!(std::fs::metadata(&plain).unwrap().modified().unwrap(), before);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_page_changed_on_disk_is_not_overwritten() {
    let (dir, mut p) = project("changed-on-disk", &[("a", "* a\n")]);
    let a = path_of(&p, "a");
    let tags = Hashtags::Dashes.into();
    p.write(&a, "* a\nmine\n".into(), tags).unwrap();
    // Another program writes it (a different size: the stamp changes whatever the clock)
    std::fs::write(&a, "* a\ntheirs, longer\n").unwrap();
    assert!(matches!(p.write(&a, "* a\nmine again\n".into(), tags), Err(AppError::ChangedOnDisk(_))));
    assert_eq!(std::fs::read_to_string(&a).unwrap(), "* a\ntheirs, longer\n");
    // Seen by `refresh`: the interface asked, writing goes through again
    p.refresh(tags).unwrap();
    p.write(&a, "* a\nkept mine\n".into(), tags).unwrap();
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn rename_changing_only_the_case() {
    let (dir, mut p) = project("rename-case", &[("note", "* note\n")]);
    let nf = p.rename(&path_of(&p, "note"), "Note", &Prefs::default()).unwrap();
    assert!(nf.path.ends_with("/Note.org") && Path::new(&nf.path).exists());
    // A disk minding case (Linux): another page of that very name is left alone
    if cfg!(target_os = "linux") {
        std::fs::write(dir.join("pages/a.org"), "* a\n").unwrap();
        std::fs::write(dir.join("pages/A.org"), "* A\n").unwrap();
        p.refresh(Hashtags::Dashes.into()).unwrap();
        assert!(matches!(p.rename(&file_named(&p, "a"), "A", &Prefs::default()), Err(AppError::PageExists(_))));
        assert_eq!(std::fs::read_to_string(dir.join("pages/A.org")).unwrap(), "* A\n");
    }
    std::fs::remove_dir_all(dir).unwrap();
}

fn file_named(p: &Project, name: &str) -> String {
    p.files().into_iter().find(|f| f.path.ends_with(&format!("/{name}.org"))).unwrap().path
}

/// A project holding the given files (`Note.md`, `Note.org`…).
fn mixed(test: &str, files: &[(&str, &str)], formats: Formats) -> (PathBuf, Project) {
    let (dir, _) = project(test, &[]);
    for (file, content) in files {
        std::fs::write(dir.join("pages").join(file), content).unwrap();
    }
    let p = Project::open(dir.to_str().unwrap(), Hashtags::Dashes.into(), formats, "journals", "templates").unwrap();
    (dir, p)
}

fn file(p: &Project, file: &str) -> String {
    p.files().into_iter().find(|f| f.path.ends_with(&format!("/{file}"))).unwrap().path
}

#[test]
fn markdown_pages_and_twins() {
    let (dir, mut p) = mixed("markdown", &[
        ("Note.org", "* Note\n"),
        ("Note.md", "# Note\n[lien](Solo.md) [[Absente|x]]\n"),
        ("Solo.md", "# Solo\n[[Note]] [[Note.md#Titre]]\n"),
        ("Org.org", "[[Solo|pas un lien étendu]] [[Note.md]]\n"),
    ], Formats::default());
    assert_eq!(p.files().len(), 4);
    // `[[Note]]` leads to the org one (the default format)
    assert_eq!(p.backlinks(&file(&p, "Note.org"), false), vec!["Solo"]);
    assert_eq!(p.backlinks(&file(&p, "Note.md"), false), vec!["Org", "Solo"]);
    // The Markdown twin is named with its extension
    assert_eq!(p.backlinks(&file(&p, "Solo.md"), false), vec!["Note.md"]);
    let broken: Vec<String> = p.broken_links(false).into_iter().map(|b| b.target).collect();
    assert_eq!(broken, vec!["Absente", "Solo|pas un lien étendu"]);

    // Renaming the Markdown twin: only the links leading to it follow
    let moved = p.rename(&file(&p, "Note.md"), "Carnet", &Prefs::default()).unwrap();
    assert!(moved.path.ends_with("/Carnet.md"));
    assert_eq!(p.read(&file(&p, "Solo.md")).unwrap(), "# Solo\n[[Note]] [[Carnet.md#Titre]]\n");
    assert_eq!(p.read(&file(&p, "Org.org")).unwrap(), "[[Solo|pas un lien étendu]] [[Carnet.md]]\n");

    // Created in the format asked, its name telling it, or the default one
    let md = p.create("Neuve.md", None, Hashtags::Dashes.into()).unwrap();
    assert_eq!((md.name.as_str(), p.read(&md.path).unwrap().as_str()), ("Neuve", "# Neuve\n"));
    assert!(p.create("Neuve", Some(Format::Markdown), Hashtags::Dashes.into()).is_err());
    assert!(p.create("Neuve", None, Hashtags::Dashes.into()).unwrap().path.ends_with("/Neuve.org"));

    // The trash keeps the format
    let trashed = p.delete(&md.path).unwrap();
    assert!(trashed.ends_with("-Neuve.md"));
    assert!(p.restore(&trashed, Hashtags::Dashes.into()).unwrap().path.ends_with("/Neuve.md"));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn one_format_only_leaves_the_others_aside() {
    let md = Formats { default: Format::Markdown, single: true };
    let (dir, mut p) = mixed("single", &[("A.org", "* A\n"), ("B.md", "[[A]]\n")], md);
    assert_eq!(p.files().iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), vec!["B"]);
    assert!(p.read(&format!("{}/pages/A.org", dir.display())).is_err());
    assert_eq!(p.broken_links(false).len(), 1);
    // Both read again: A comes back at the next look
    p.set_formats(Formats::default());
    let changes = p.refresh(Hashtags::Dashes.into()).unwrap().unwrap();
    assert_eq!(changes.files.len(), 2);
    assert!(p.broken_links(false).is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn refresh_sees_outside_changes_only() {
    let (dir, mut p) = project("refresh", &[("a", "* a\n")]);
    p.write(&path_of(&p, "a"), "* a\n[[b]]\n".into(), Hashtags::Dashes.into()).unwrap();
    assert!(p.refresh(Hashtags::Dashes.into()).unwrap().is_none());

    std::fs::write(dir.join("pages/n.org"), "* n :projet:\n").unwrap();
    let changes = p.refresh(Hashtags::Dashes.into()).unwrap().unwrap();
    assert_eq!(changes.changed.len(), 1);
    assert_eq!(p.tags(OrgTags::default()), vec![TagCount { name: "projet".into(), count: 1 }]);
    assert_eq!(p.search_tags("projet", OrgTags::default()).len(), 1);
    std::fs::remove_dir_all(dir).unwrap();
}
