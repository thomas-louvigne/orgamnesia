//! Editing, links, templates, capture, folders, splits and settings, driven by the keys.

use super::*;

    #[test]
    fn edits_follows_links_and_saves() {
        let dir = std::env::temp_dir().join(format!("orgamnesia-tui-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("pages")).unwrap();
        std::fs::write(dir.join("pages/Alpha.org"), "* Alpha\n[[Beta]] [[Gamma]]").unwrap();
        for name in ["Beta.org", "alphabet.org"] { std::fs::write(dir.join("pages").join(name), "x").unwrap(); }
        let prefs = settings::Settings::default().prefs();
        let project = Project::open(&crate::slash(&dir), prefs.tags(), prefs.formats(), &prefs.journal_folder, &prefs.templates_folder).unwrap();
        let mut app = App::new(project, prefs, Keybindings::with_profiles(None, vec![]));
        app.height = 10;
        app.filter = "ALPHA".into();
        let names: Vec<String> = app.shown().into_iter().map(|f| f.name).collect();
        assert_eq!(names, ["Alpha", "alphabet"]);
        assert!(app.help[0].contains("Ctrl+K find"), "{}", app.help[0]);

        // Enter edits the page here; typing changes it, Ctrl+S writes it
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        let alpha = app.editor.as_ref().unwrap().path.clone();
        press(&mut app, KeyCode::End, KeyModifiers::NONE);
        press(&mut app, KeyCode::Char('!'), KeyModifiers::NONE);
        press(&mut app, KeyCode::Char('s'), KeyModifiers::CONTROL);
        assert_eq!(std::fs::read_to_string(&alpha).unwrap(), "* Alpha!\n[[Beta]] [[Gamma]]");

        // Ctrl+Enter on a link opens its page in the editor, the first one saved
        press(&mut app, KeyCode::Down, KeyModifiers::NONE);
        press(&mut app, KeyCode::Char('x'), KeyModifiers::NONE);
        press(&mut app, KeyCode::Home, KeyModifiers::NONE);
        press(&mut app, KeyCode::Enter, KeyModifiers::CONTROL);
        assert!(app.editor.as_ref().unwrap().path.ends_with("Beta.org"));
        assert_eq!(std::fs::read_to_string(&alpha).unwrap(), "* Alpha!\n[[Beta]]x [[Gamma]]");
        press(&mut app, KeyCode::Left, KeyModifiers::ALT);
        assert_eq!(app.editor.as_ref().unwrap().path, alpha);

        // The border of the pages dragged with the mouse, kept within the screen
        app.total = 100;
        let at = |column, kind| MouseEvent { kind, column, row: 3, modifiers: KeyModifiers::NONE };
        app.mouse(at(5, MouseEventKind::Down(MouseButton::Left)));
        app.mouse(at(50, MouseEventKind::Drag(MouseButton::Left)));
        assert_eq!(app.split_width(100), 24);
        app.mouse(at(23, MouseEventKind::Down(MouseButton::Left)));
        app.mouse(at(49, MouseEventKind::Drag(MouseButton::Left)));
        app.mouse(at(49, MouseEventKind::Up(MouseButton::Left)));
        assert_eq!(app.split_width(100), 50);
        app.mouse(at(99, MouseEventKind::Drag(MouseButton::Left)));
        assert_eq!(app.split_width(100), 50);
        app.split = Some(99);
        assert_eq!(app.split_width(100), 80);

        // A click on a page of the list leaves the editor (saved) for it
        app.list_area = Rect::new(0, 0, 30, 10);
        press(&mut app, KeyCode::Char('z'), KeyModifiers::NONE);
        app.mouse(at(3, MouseEventKind::Down(MouseButton::Left)));
        assert!(app.editor.is_none());
        assert!(std::fs::read_to_string(&alpha).unwrap().starts_with("z"));
        assert_eq!(app.list.selected(), Some(2));
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        press(&mut app, KeyCode::Esc, KeyModifiers::NONE);

        // Esc leaves the editor; quitting from the pages
        press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert!(app.editor.is_none());
        assert!(press(&mut app, KeyCode::Char('q'), KeyModifiers::CONTROL));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn templates_from_the_quick_open() {
        let dir = std::env::temp_dir().join(format!("orgamnesia-tui-tpl-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("pages")).unwrap();
        std::fs::write(dir.join("pages/Point.org"), "* Point\n").unwrap();
        std::fs::write(dir.join("pages/Notes.md"), "# Notes\n").unwrap();
        let prefs = settings::Settings::default().prefs();
        let project = Project::open(&crate::slash(&dir), prefs.tags(), prefs.formats(), &prefs.journal_folder, &prefs.templates_folder).unwrap();
        let mut app = App::new(project, prefs, Keybindings::with_profiles(None, vec![]));
        let pick = |app: &mut App, query: &str| {
            let item = app.quick_list(query).into_iter().next().expect(query);
            app.template_chosen(item);
        };
        // The suggested ones, in the default format (org-mode), in French
        pick(&mut app, "add suggested templates");
        assert!(dir.join("templates/Réunion.org").is_file() && dir.join("templates/Projet.org").is_file());
        // A page holding only its title: the template takes its place, filled
        app.edit(&crate::slash(dir.join("pages/Point.org")));
        pick(&mut app, "template reunion");
        let text = app.editor.as_ref().unwrap().text();
        assert!(text.starts_with("#+TITLE: Point\n#+DATE: ["), "{text}");
        // Else at the cursor, once said yes
        let len = text.chars().count();
        pick(&mut app, "template projet");
        assert!(matches!(app.mode, Mode::Prompt(_)) && app.editor.as_ref().unwrap().text().chars().count() == len);
        press(&mut app, KeyCode::Char('n'), KeyModifiers::NONE);
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(app.editor.as_ref().unwrap().text().chars().count(), len);
        pick(&mut app, "template projet");
        press(&mut app, KeyCode::Char('a'), KeyModifiers::NONE);
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        let text = app.editor.as_ref().unwrap().text();
        assert!(text.chars().count() > len && text.contains("#+FILETAGS: :projet:"));
        // A template applied already: the page replaced by the new one, when said so
        pick(&mut app, "template lecture");
        press(&mut app, KeyCode::Char('r'), KeyModifiers::NONE);
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        let text = app.editor.as_ref().unwrap().text();
        assert!(text.starts_with("#+TITLE: Point\n#+FILETAGS: :lecture:") && !text.contains(":projet:"), "{text}");
        // A Markdown page is offered none of the org-mode templates
        app.edit(&crate::slash(dir.join("pages/Notes.md")));
        assert!(app.quick_list("template").iter().all(|i| !matches!(i, quick::QuickItem::Template { edit: false, .. })));
        // A template edited like a page, and no page itself
        pick(&mut app, "edit template reunion");
        assert!(app.editor.as_ref().unwrap().path.ends_with("templates/Réunion.org"));
        assert!(app.project.files().iter().all(|f| !f.path.contains("templates/")));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn quick_capture_beside_the_page() {
        let dir = std::env::temp_dir().join(format!("orgamnesia-tui-capture-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("pages")).unwrap();
        std::fs::write(dir.join("pages/Alpha.org"), "* Alpha\n").unwrap();
        let prefs = settings::Settings::default().prefs();
        let project = Project::open(&crate::slash(&dir), prefs.tags(), prefs.formats(), &prefs.journal_folder, &prefs.templates_folder).unwrap();
        let mut app = App::new(project, prefs, Keybindings::with_profiles(None, vec![]));
        let alpha = crate::slash(dir.join("pages/Alpha.org"));
        app.edit(&alpha);
        // The journal off: `Inbox`, made, in the other window, the caret on a new note
        app.run_action("capture");
        assert!(app.split_dir.is_some() && app.capture == Some(true));
        assert_eq!(app.other.as_ref().unwrap().path, alpha);
        let e = app.editor.as_ref().unwrap();
        assert!(e.path.ends_with("pages/Inbox.org"));
        assert!(e.text().starts_with("* Inbox\n- ") && e.cursor == e.text().chars().count(), "{}", e.text());
        for c in "idée".chars() { press(&mut app, KeyCode::Char(c), KeyModifiers::NONE); }
        // Again: saved, closed, back to the page edited
        app.run_action("capture");
        assert!(app.other.is_none() && app.split_dir.is_none() && app.capture.is_none());
        assert_eq!(app.editor.as_ref().unwrap().path, alpha);
        let inbox = std::fs::read_to_string(dir.join("pages/Inbox.org")).unwrap();
        assert!(inbox.starts_with("* Inbox\n- ") && inbox.ends_with(" idée"), "{inbox}");
        // A second note goes on a line of its own
        app.run_action("capture");
        assert_eq!(app.editor.as_ref().unwrap().text().matches("\n- ").count(), 2);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn folders_in_the_list_and_their_actions() {
        let dir = std::env::temp_dir().join(format!("orgamnesia-tui-folders-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("pages/Jeux")).unwrap();
        std::fs::write(dir.join("pages/Accueil.org"), "* Accueil\n[[Jeux/Dragon]]\n").unwrap();
        std::fs::write(dir.join("pages/Jeux/Dragon.org"), "* Dragon\n").unwrap();
        let prefs = settings::Settings::default().prefs();
        let project = Project::open(&crate::slash(&dir), prefs.tags(), prefs.formats(), &prefs.journal_folder, &prefs.templates_folder).unwrap();
        let mut app = App::new(project, prefs, Keybindings::with_profiles(None, vec![]));
        let type_in = |app: &mut App, text: &str| {
            if let Mode::Prompt(p) = &mut app.mode { p.input.clear(); }
            for c in text.chars() { press(app, KeyCode::Char(c), KeyModifiers::NONE); }
            press(app, KeyCode::Enter, KeyModifiers::NONE);
        };
        let lines = |app: &App| app.rows().into_iter().map(|r| match r {
            orgamnesia_core::tree::Row::Folder { name, open, .. } => format!("{}{name}", if open { "▾" } else { "▸" }),
            orgamnesia_core::tree::Row::Page { name, depth, .. } => format!("{}{name}", "  ".repeat(depth)),
        }).collect::<Vec<_>>();
        assert_eq!(lines(&app), ["Accueil", "▾Jeux", "  Dragon"]);
        // Enter on a folder folds it
        app.list.select(Some(1));
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(lines(&app), ["Accueil", "▸Jeux"]);
        press(&mut app, KeyCode::Right, KeyModifiers::NONE);
        // A folder made in the one chosen
        app.run_action("new_folder");
        assert!(matches!(&app.mode, Mode::Prompt(p) if p.input == "Jeux/"));
        type_in(&mut app, "Jeux/Quêtes");
        assert!(dir.join("pages/Jeux/Quêtes").is_dir());
        // The page edited moved there, Tab completing the folder; the link follows
        let dragon = crate::slash(dir.join("pages/Jeux/Dragon.org"));
        app.edit(&dragon);
        app.run_action("move_page");
        if let Mode::Prompt(p) = &mut app.mode { p.input = "Jeux/Q".into(); }
        press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
        assert!(matches!(&app.mode, Mode::Prompt(p) if p.input == "Jeux/Quêtes"));
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert!(app.editor.as_ref().unwrap().path.ends_with("pages/Jeux/Quêtes/Dragon.org"));
        assert_eq!(std::fs::read_to_string(dir.join("pages/Accueil.org")).unwrap(), "* Accueil\n[[Jeux/Quêtes/Dragon]]\n");
        // A folder renamed: the page edited follows it
        app.run_action("rename_folder");
        type_in(&mut app, "Jeux");
        type_in(&mut app, "JdR");
        assert!(app.editor.as_ref().unwrap().path.ends_with("pages/JdR/Quêtes/Dragon.org"), "{}", app.status);
        assert_eq!(std::fs::read_to_string(dir.join("pages/Accueil.org")).unwrap(), "* Accueil\n[[JdR/Quêtes/Dragon]]\n");
        // A folder deleted: its pages in the trash, the editor closed on them
        app.run_action("delete_folder");
        type_in(&mut app, "JdR");
        type_in(&mut app, "y");
        assert!(app.editor.is_none() && !dir.join("pages/JdR").exists(), "{}", app.status);
        assert_eq!(lines(&app), ["Accueil"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_plain_click_follows_a_link_when_the_settings_say_so() {
        let dir = std::env::temp_dir().join(format!("orgamnesia-tui-click-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("pages")).unwrap();
        std::fs::write(dir.join("pages/Alpha.org"), "[[Beta]] alpha").unwrap();
        std::fs::write(dir.join("pages/Beta.org"), "beta").unwrap();
        let prefs = settings::Settings::default().prefs();
        let project = Project::open(&crate::slash(&dir), prefs.tags(), prefs.formats(), &prefs.journal_folder, &prefs.templates_folder).unwrap();
        let mut app = App::new(project, prefs, Keybindings::with_profiles(Some("emacs"), vec![]));
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        app.page_area = Rect::new(30, 0, 40, 10);
        let click = MouseEvent { kind: MouseEventKind::Down(MouseButton::Left), column: 33, row: 1, modifiers: KeyModifiers::NONE };
        // Off (the default): the cursor placed in the link only
        app.mouse(click);
        assert!(app.editor.as_ref().unwrap().path.ends_with("Alpha.org"));
        app.prefs.click_follows_links = true;
        app.mouse(click);
        assert!(app.editor.as_ref().unwrap().path.ends_with("Beta.org"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn splits_the_screen_as_emacs() {
        let dir = std::env::temp_dir().join(format!("orgamnesia-tui-split-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("pages")).unwrap();
        std::fs::write(dir.join("pages/Alpha.org"), "[[Beta]] alpha").unwrap();
        std::fs::write(dir.join("pages/Beta.org"), "beta").unwrap();
        let prefs = settings::Settings::default().prefs();
        let project = Project::open(&crate::slash(&dir), prefs.tags(), prefs.formats(), &prefs.journal_folder, &prefs.templates_folder).unwrap();
        let mut app = App::new(project, prefs, Keybindings::with_profiles(Some("emacs"), vec![]));
        let ctrl = KeyModifiers::CONTROL;
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        let alpha = app.editor.as_ref().unwrap().path.clone();

        // C-x 3: the page in both halves; what is typed in one is in the other
        press(&mut app, KeyCode::Char('x'), ctrl);
        press(&mut app, KeyCode::Char('3'), KeyModifiers::NONE);
        assert_eq!(app.split_dir, Some(Direction::Horizontal));
        press(&mut app, KeyCode::Char('!'), KeyModifiers::NONE);
        press(&mut app, KeyCode::Char('x'), ctrl);
        press(&mut app, KeyCode::Char('o'), KeyModifiers::NONE);
        assert_eq!(app.active_slot, 1);
        assert!(app.editor.as_ref().unwrap().text().starts_with('!'));

        // Ctrl+click on the link: Beta in this half, Alpha (saved) in the other
        app.page_area = Rect::new(30, 0, 40, 10);
        app.mouse(MouseEvent { kind: MouseEventKind::Down(MouseButton::Left), column: 33, row: 1, modifiers: ctrl });
        assert!(app.editor.as_ref().unwrap().path.ends_with("Beta.org"));
        assert_eq!(app.other.as_ref().unwrap().path, alpha);
        assert_eq!(std::fs::read_to_string(&alpha).unwrap(), "![[Beta]] alpha");

        // Esc: back to the pages, the screen still split; the page chosen in this half
        press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert!(app.editor.is_none() && app.other.is_some() && app.split_dir.is_some());
        press(&mut app, KeyCode::Up, KeyModifiers::NONE);
        press(&mut app, KeyCode::Down, KeyModifiers::NONE);
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert!(app.editor.as_ref().unwrap().path.ends_with("Beta.org"));
        assert_eq!(app.other.as_ref().unwrap().path, alpha);
        assert_eq!(app.active_slot, 1);

        // C-SPC starts a region, C-g lets go of it
        press(&mut app, KeyCode::Char(' '), ctrl);
        press(&mut app, KeyCode::Char('f'), ctrl);
        assert!(app.editor.as_ref().unwrap().region().is_some());
        press(&mut app, KeyCode::Char('g'), ctrl);
        assert_eq!(app.editor.as_ref().unwrap().region(), None);
        press(&mut app, KeyCode::Char('b'), ctrl);

        // C-x 0 closes this half, C-x 1 the other: one window again
        press(&mut app, KeyCode::Char('z'), KeyModifiers::NONE);
        press(&mut app, KeyCode::Char('x'), ctrl);
        press(&mut app, KeyCode::Char('0'), KeyModifiers::NONE);
        assert!(app.other.is_none() && app.split_dir.is_none());
        assert_eq!(app.editor.as_ref().unwrap().path, alpha);
        assert_eq!(std::fs::read_to_string(dir.join("pages/Beta.org")).unwrap(), "zbeta");

        // The quick open's `>`: the commands, Enter doing the one chosen
        app.mode = Mode::Quick(quick::Quick::default());
        for c in ">split vert".chars() { press(&mut app, KeyCode::Char(c), KeyModifiers::NONE); }
        assert!(matches!(app.quick_list(">split vert").first(), Some(quick::QuickItem::Command { id, .. }) if id == "split_vertical"));
        assert!(app.quick_list(">export").is_empty(), "window-only commands left out");
        // The editing ones too, on a page edited; not the moves
        assert!(matches!(app.quick_list(">bold").first(), Some(quick::QuickItem::Command { id, .. }) if id == "bold"));
        assert!(app.quick_list(">forward char").is_empty());
        // Without `>`: the pages, then the commands, the page to create last
        let found = app.quick_list("split vert");
        assert!(matches!(found.first(), Some(quick::QuickItem::Command { id, .. }) if id == "split_vertical"));
        assert!(matches!(found.last(), Some(quick::QuickItem::Create(_))));
        assert!(matches!(app.quick_list("bet").first(), Some(quick::QuickItem::Page { .. })));
        assert!(app.quick_list(">bet").iter().all(|i| matches!(i, quick::QuickItem::Command { .. })));
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert!(app.other.is_some() && matches!(app.mode, Mode::Normal));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn follows_the_settings() {
        let dir = std::env::temp_dir().join(format!("orgamnesia-tui-prefs-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("pages")).unwrap();
        std::fs::write(dir.join("pages/Alpha.org"), "alpha").unwrap();
        let mut prefs = settings::Settings::default().prefs();
        (prefs.autosave, prefs.journal, prefs.ask_format) = (true, false, true);
        let project = Project::open(&crate::slash(&dir), prefs.tags(), prefs.formats(), &prefs.journal_folder, &prefs.templates_folder).unwrap();
        let mut app = App::new(project, prefs, Keybindings::with_profiles(None, vec![]));
        // Saved on every change
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        press(&mut app, KeyCode::Char('!'), KeyModifiers::NONE);
        app.after_event();
        assert_eq!(std::fs::read_to_string(dir.join("pages/Alpha.org")).unwrap(), "!alpha");
        // No journal when its extension is off
        app.journal();
        assert!(app.status.contains("journal"));
        // The format of a new page asked
        app.open_or_create("Beta".into());
        assert!(app.prompt().is_some_and(|p| p.label.contains("Beta")));
        app.prompt_mut().unwrap().input = "md".into();
        app.submit();
        assert!(dir.join("pages/Beta.md").exists());
        // While editing, `new_page` asks the name (Emacs' find-file) and opens the page
        app.action("new_page");
        answer(&mut app, "Alpha");
        assert!(app.editor.as_ref().is_some_and(|e| e.path.ends_with("Alpha.org")));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn ctrl_l_recenters_the_view() {
        let text: String = (0..100).map(|n| format!("ligne {n}\n")).collect();
        let (dir, _) = app_on("recenter", &[("Alpha", &text)]);
        let prefs = settings::Settings::default().prefs();
        let project = Project::open(&crate::slash(&dir), prefs.tags(), prefs.formats(), &prefs.journal_folder, &prefs.templates_folder).unwrap();
        let mut app = App::new(project, prefs, Keybindings::with_profiles(Some("emacs"), vec![]));
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        let mut term = ratatui::Terminal::new(ratatui::backend::TestBackend::new(140, 40)).unwrap();
        for _ in 0..60 { press(&mut app, KeyCode::Down, KeyModifiers::NONE); term.draw(|f| app.draw(f)).unwrap(); }
        press(&mut app, KeyCode::Char('l'), KeyModifiers::CONTROL);
        term.draw(|f| app.draw(f)).unwrap();
        let e = app.editor.as_ref().unwrap();
        let height = app.page_area.height as usize - 2;
        assert_eq!(e.row() - e.top, height / 2, "status: {}", app.status);
        let _ = std::fs::remove_dir_all(&dir);
    }
