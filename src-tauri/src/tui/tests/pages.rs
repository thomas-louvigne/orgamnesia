//! Pages changed on disk, the journal, mentions, the trash, replacements, git versions, the agenda.

use super::*;

    #[test]
    fn a_page_changed_on_disk_while_edited() {
        let dir = std::env::temp_dir().join(format!("orgamnesia-tui-disk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("pages")).unwrap();
        std::fs::write(dir.join("pages/Page.org"), "* Page\n").unwrap();
        let prefs = settings::Settings::default().prefs();
        let project = Project::open(&crate::slash(&dir), prefs.tags(), prefs.formats(), &prefs.journal_folder, &prefs.templates_folder).unwrap();
        let mut app = App::new(project, prefs, Keybindings::with_profiles(None, vec![]));
        let page = crate::slash(dir.join("pages/Page.org"));
        app.edit(&page);
        let text = |app: &App| app.editor.as_ref().unwrap().text();
        let answer = |app: &mut App, c: char| {
            assert!(matches!(&app.mode, Mode::Prompt(p) if p.asks_disk_conflict()));
            press(app, KeyCode::Char(c), KeyModifiers::NONE);
            press(app, KeyCode::Enter, KeyModifiers::NONE);
        };

        // No unsaved change: read again
        std::fs::write(&page, "* Page\ntheirs\n").unwrap();
        app.disk_changes(false);
        assert_eq!(text(&app), "* Page\ntheirs\n");

        // Unsaved changes: saving is refused and which text to keep asked; theirs taken
        press(&mut app, KeyCode::Char('!'), KeyModifiers::NONE);
        std::fs::write(&page, "* Page\ntheirs, longer\n").unwrap();
        press(&mut app, KeyCode::Char('s'), KeyModifiers::CONTROL);
        assert_eq!(std::fs::read_to_string(&page).unwrap(), "* Page\ntheirs, longer\n");
        answer(&mut app, 'n');
        assert_eq!(text(&app), "* Page\ntheirs, longer\n");
        assert!(!app.editor.as_ref().unwrap().dirty);

        // Seen while waiting: mine kept
        press(&mut app, KeyCode::Char('!'), KeyModifiers::NONE);
        std::fs::write(&page, "* Page\ntheirs, even longer\n").unwrap();
        app.disk_changes(false);
        answer(&mut app, 'y');
        assert_eq!(std::fs::read_to_string(&page).unwrap(), "*! Page\ntheirs, longer\n");

        // A folder made elsewhere shows in the list
        std::fs::create_dir_all(dir.join("pages/Jeux")).unwrap();
        std::fs::write(dir.join("pages/Jeux/Dragon.org"), "* Dragon\n").unwrap();
        app.disk_changes(false);
        assert!(app.folders.contains(&"Jeux".to_string()));
        std::fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn from_day_to_day_in_the_journal() {
        let (dir, mut app) = app_on("journal-days", &[("2026-11-20", "* a\n"), ("2026-11-22", "* b\n"), ("Note", "* n\n")]);
        app.prefs.journal = true;
        app.edit(&crate::slash(dir.join("pages/2026-11-22.org")));
        app.run_action("journal_previous");
        assert!(app.editor.as_ref().unwrap().path.ends_with("2026-11-20.org"));
        app.run_action("journal_previous");
        assert_eq!(app.status, "no earlier day in the journal");
        app.run_action("journal_next");
        assert!(app.editor.as_ref().unwrap().path.ends_with("2026-11-22.org"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn unlinked_mentions_made_links() {
        let (dir, mut app) = app_on("mentions", &[("Dragon", "#+ALIAS: Wyrm\n* Dragon\n"), ("Conte", "Un dragon et un wyrm.\n")]);
        app.edit(&crate::slash(dir.join("pages/Dragon.org")));
        app.right_column = false;
        press(&mut app, KeyCode::F(8), KeyModifiers::NONE);
        assert_eq!(app.popup().unwrap().lines, ["Unlinked mentions (2), l: link", "  Conte: Un dragon et un wyrm.", "  Conte: Un dragon et un wyrm."]);
        press(&mut app, KeyCode::Char('l'), KeyModifiers::NONE);
        let conte = crate::slash(dir.join("pages/Conte.org"));
        assert_eq!(std::fs::read_to_string(&conte).unwrap(), "Un [[Dragon][dragon]] et un wyrm.\n");
        // The list follows: the link is a backlink, the alias still to link
        assert_eq!(app.popup().unwrap().lines, ["Conte", "Unlinked mentions (1), l: link", "  Conte: Un [[Dragon][dragon]] et un wyrm."]);
        press(&mut app, KeyCode::Char('l'), KeyModifiers::NONE);
        assert_eq!(std::fs::read_to_string(&conte).unwrap(), "Un [[Dragon][dragon]] et un [[Wyrm][wyrm]].\n");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_page_deleted_then_brought_back_from_the_trash() {
        let (dir, mut app) = app_on("trash", &[("Note", "* Note\n"), ("Autre", "* Autre\n")]);
        let note = crate::slash(dir.join("pages/Note.org"));
        app.edit(&note);
        app.command("delete_page");
        answer(&mut app, "y");
        assert!(!Path::new(&note).exists() && app.editor.is_none());
        app.command("trash");
        let Some(popup) = app.popup() else { panic!("no trash") };
        assert_eq!(popup.lines, ["  Note"]);
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert!(Path::new(&note).exists(), "{}", app.status);
        app.command("trash");
        assert!(app.popup().unwrap().lines.is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_replacement_in_the_project_undone() {
        let (dir, mut app) = app_on("undo-replace", &[("A", "* A\nchat\n"), ("B", "* B\nchat chat\n")]);
        app.command("undo_replace");
        assert_eq!(app.status, "no replacement to undo");
        app.command("project_replace");
        for typed in ["chat", "chien", "y"] { answer(&mut app, typed); }
        assert_eq!(std::fs::read_to_string(dir.join("pages/B.org")).unwrap(), "* B\nchien chien\n");
        app.command("undo_replace");
        assert_eq!(app.status, "2 pages put back");
        assert_eq!(std::fs::read_to_string(dir.join("pages/B.org")).unwrap(), "* B\nchat chat\n");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_link_to_a_headline_reaches_it() {
        let (dir, mut app) = app_on("place", &[("A", "* A\n[[B::*Deux]] [[*Fin]]\n** Fin\n"), ("B", "* B\nun\n* Deux\ndeux\n")]);
        app.follow("B::*Deux");
        let e = app.editor.as_ref().unwrap();
        assert!(e.path.ends_with("B.org"));
        assert_eq!(e.cursor, "* B\nun\n".chars().count());
        app.edit(&crate::slash(dir.join("pages/A.org")));
        app.follow("*Fin");
        let e = app.editor.as_ref().unwrap();
        assert!(e.path.ends_with("A.org") && e.cursor == "* A\n[[B::*Deux]] [[*Fin]]\n".chars().count());
        app.follow("*Nulle part");
        assert!(app.status.contains("not found"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn commit_some_files_and_put_an_old_version_back() {
        let (dir, mut app) = app_on("git", &[("A", "* A\n"), ("B", "* B\n")]);
        let git = |args: &[&str]| assert!(std::process::Command::new("git").arg("-C").arg(&dir).args(args).output().unwrap().status.success(), "{args:?}");
        git(&["init", "-q"]);
        git(&["config", "user.name", "t"]);
        git(&["config", "user.email", "t@example.com"]);
        git(&["add", "-A"]);
        git(&["commit", "-qm", "first"]);
        std::fs::write(dir.join("pages/A.org"), "* A\nplus\n").unwrap();
        std::fs::write(dir.join("pages/B.org"), "* B\nplus\n").unwrap();

        // Both listed, ticked; B unticked, then its diff shown and closed back to the list
        app.command("git_commit");
        assert_eq!(app.popup().unwrap().lines.len(), 2);
        press(&mut app, KeyCode::Down, KeyModifiers::NONE);
        press(&mut app, KeyCode::Char(' '), KeyModifiers::NONE);
        assert!(app.popup().unwrap().lines[1].starts_with("[ ]"));
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        let diff = app.popup().unwrap();
        assert!(diff.diff && diff.lines.iter().any(|l| l == "+plus"), "{:?}", diff.lines);
        press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(app.popup().unwrap().title, "Commit");
        // A committed alone
        press(&mut app, KeyCode::Char('c'), KeyModifiers::NONE);
        for _ in 0..80 { press(&mut app, KeyCode::Backspace, KeyModifiers::NONE); }
        answer(&mut app, "only A");
        let changes = crate::git::changes(&crate::slash(&dir)).unwrap();
        assert_eq!(changes.len(), 1, "{changes:?}");
        assert!(changes[0].path.ends_with("B.org"));

        // The first version of A put back in the editor, undo taking it back
        let a = crate::slash(dir.join("pages/A.org"));
        app.edit(&a);
        app.command("git_history");
        assert_eq!(app.popup().unwrap().lines.len(), 2);
        press(&mut app, KeyCode::Down, KeyModifiers::NONE);
        press(&mut app, KeyCode::Char('r'), KeyModifiers::NONE);
        assert_eq!(app.editor.as_ref().unwrap().text(), "* A\n");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn plans_and_works_from_the_agenda() {
        let dir = std::env::temp_dir().join(format!("orgamnesia-tui-agenda-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("pages")).unwrap();
        std::fs::write(dir.join("pages/Plan.org"), "#+TODO: TODO | DONE\n* TODO Écrire\ntexte").unwrap();
        std::fs::write(dir.join("pages/Notes.md"), "## TODO Lire\n").unwrap();
        let prefs = settings::Settings::default().prefs();
        let project = Project::open(&crate::slash(&dir), prefs.tags(), prefs.formats(), &prefs.journal_folder, &prefs.templates_folder).unwrap();
        let mut app = App::new(project, prefs, Keybindings::with_profiles(None, vec![]));
        app.height = 10;
        let now = super::editor::now();
        let day = |n: i64| orgamnesia_core::edit::Stamp { time: None, active: true, ..now.on(now.day_number() + n) }.text();

        // Scheduled from the text of the headline: `.` is today
        let plan = dir.join("pages/Plan.org").to_string_lossy().replace('\\', "/");
        app.edit(&plan);
        press(&mut app, KeyCode::Down, KeyModifiers::NONE);
        press(&mut app, KeyCode::Down, KeyModifiers::NONE);
        app.run_action("schedule");
        press(&mut app, KeyCode::Char('.'), KeyModifiers::NONE);
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(app.editor.as_ref().unwrap().text(), format!("#+TODO: TODO | DONE\n* TODO Écrire\nSCHEDULED: {}\ntexte", day(0)));

        // The agenda shows it today; `t` marks it done (in the page open), S-→ moves it a day
        app.run_action("agenda");
        let shown = app.popup().unwrap().lines.join("\n");
        assert!(shown.contains("Scheduled: TODO Écrire"), "{shown}");
        press(&mut app, KeyCode::Char('t'), KeyModifiers::NONE);
        assert!(app.editor.as_ref().unwrap().text().starts_with("#+TODO: TODO | DONE\n* DONE Écrire\nCLOSED: ["), "{} / {}", app.status, app.editor.as_ref().unwrap().text());
        press(&mut app, KeyCode::Right, KeyModifiers::SHIFT);
        assert!(app.editor.as_ref().unwrap().text().contains(&format!("SCHEDULED: {}", day(1))));

        // A Markdown page not open: its deadline set from the agenda, on disk
        app.agenda(orgamnesia_core::agenda::week_start(now.day_number()), 0);
        assert!(app.popup().unwrap().agenda.is_some());
        let notes = dir.join("pages/Notes.md").to_string_lossy().replace('\\', "/");
        app.change_page(&notes, |t| orgamnesia_core::agenda::plan_typed(t, 0, orgamnesia_core::Format::Markdown, "DEADLINE:", "+2d", now.day_number()).unwrap().map(|e| vec![e]));
        assert_eq!(std::fs::read_to_string(&notes).unwrap(), format!("## TODO Lire\nDEADLINE: {}\n", day(2)));

        // The TODO frame: those with no date only
        app.todo_filter = "undated".into();
        assert!(app.frame_items(orgamnesia_core::Frame::Todos).1.is_empty());
        app.todo_filter = "dated".into();
        assert_eq!(app.frame_items(orgamnesia_core::Frame::Todos).1.len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }
