//! The terminal interface driven by its keys and its mouse, on a project made for each test.

use super::*;
use super::view::{frame_title, git_line};
use orgamnesia_core::keybindings::Keybindings;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Direction, Rect};
use ratatui::style::Modifier;


    fn press(app: &mut App, code: KeyCode, m: KeyModifiers) -> bool { app.key(KeyEvent::new(code, m)) }

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
        assert_eq!(app.split_width(100), 30);
        app.mouse(at(29, MouseEventKind::Down(MouseButton::Left)));
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
        assert!(app.quick_list("template").iter().all(|i| !matches!(i, commands::QuickItem::Template { edit: false, .. })));
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
        app.mode = Mode::Quick(commands::Quick::default());
        for c in ">split vert".chars() { press(&mut app, KeyCode::Char(c), KeyModifiers::NONE); }
        assert!(matches!(app.quick_list(">split vert").first(), Some(commands::QuickItem::Command { id, .. }) if id == "split_vertical"));
        assert!(app.quick_list(">export").is_empty(), "window-only commands left out");
        // The editing ones too, on a page edited; not the moves
        assert!(matches!(app.quick_list(">bold").first(), Some(commands::QuickItem::Command { id, .. }) if id == "bold"));
        assert!(app.quick_list(">forward char").is_empty());
        // Without `>`: the pages, then the commands, the page to create last
        let found = app.quick_list("split vert");
        assert!(matches!(found.first(), Some(commands::QuickItem::Command { id, .. }) if id == "split_vertical"));
        assert!(matches!(found.last(), Some(commands::QuickItem::Create(_))));
        assert!(matches!(app.quick_list("bet").first(), Some(commands::QuickItem::Page { .. })));
        assert!(app.quick_list(">bet").iter().all(|i| matches!(i, commands::QuickItem::Command { .. })));
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
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_git_state_reads_at_a_glance() {
        let g = orgamnesia_core::GitStatus { repo: true, branch: Some("main".into()), changes: 3, ahead: 1, behind: 2, ..Default::default() };
        assert_eq!(git_line(&g), " ⎇ main ●3 ↑1 ↓2 ");
        let clean = orgamnesia_core::GitStatus { repo: true, branch: Some("main".into()), ..Default::default() };
        assert_eq!(git_line(&clean), " ⎇ main ");
    }

    #[test]
    fn the_session_comes_back() {
        let dir = std::env::temp_dir().join(format!("orgamnesia-tui-session-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("pages")).unwrap();
        for name in ["A", "B", "C"] { std::fs::write(dir.join(format!("pages/{name}.org")), format!("* {name}")).unwrap(); }
        let prefs = settings::Settings::default().prefs();
        let open = || Project::open(&crate::slash(&dir), prefs.tags(), prefs.formats(), &prefs.journal_folder, &prefs.templates_folder).unwrap();
        let mut app = App::new(open(), prefs.clone(), Keybindings::with_profiles(Some("emacs"), vec![]));
        let path = |n: &str| format!("{}/pages/{n}.org", app.project.path);
        let (a, b, c) = (path("A"), path("B"), path("C"));
        app.edit(&a);
        app.edit(&b);
        app.split(Direction::Horizontal);
        app.swap();
        app.edit(&c);
        let session = app.session().unwrap();
        assert_eq!((session.active.as_deref(), session.other.as_deref()), (Some(b.as_str()), Some(c.as_str())));
        assert_eq!((session.split, session.focus_second), (Some(orgamnesia_core::SplitKind::Vertical), true));
        // Opened again: the same pages and windows, the focus where it was
        let mut again = App::new(open(), prefs, Keybindings::with_profiles(Some("emacs"), vec![]));
        again.restore_session(session.clone());
        assert_eq!(again.editor.as_ref().unwrap().path, c);
        assert_eq!(again.other.as_ref().unwrap().path, b);
        assert_eq!(again.session(), Some(session));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn panels_and_renaming() {
        let dir = std::env::temp_dir().join(format!("orgamnesia-tui-panels-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("pages")).unwrap();
        std::fs::write(dir.join("pages/A.org"), "* TODO Faire :projet:\nvoir [[B]] et [[Rien]]").unwrap();
        std::fs::write(dir.join("pages/B.org"), "* B").unwrap();
        let mut prefs = settings::Settings::default().prefs();
        prefs.update_links = true;
        let project = Project::open(&crate::slash(&dir), prefs.tags(), prefs.formats(), &prefs.journal_folder, &prefs.templates_folder).unwrap();
        let mut app = App::new(project, prefs, Keybindings::with_profiles(None, vec![]));
        let a = format!("{}/pages/A.org", app.project.path);
        // F4: the TODO frame of the right column takes the focus; Enter opens the page on its line
        press(&mut app, KeyCode::F(4), KeyModifiers::NONE);
        assert_eq!(app.frame_focus(), Some(orgamnesia_core::Frame::Todos));
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(app.editor.as_ref().unwrap().path, a);
        assert_eq!(app.frame_focus(), None);
        // F5: the tags, Enter on one its headlines (over the page)
        press(&mut app, KeyCode::F(5), KeyModifiers::NONE);
        assert!(app.frame_items(orgamnesia_core::Frame::Tags).1[0].0.starts_with("projet"));
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert!(app.popup().unwrap().title.contains("projet"));
        press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        // The pages not created (the tag :projet: is a link too), the pages linking here
        let broken = app.frame_items(orgamnesia_core::Frame::Broken).1;
        assert!(broken.iter().any(|(l, _)| l.starts_with("Rien")), "{broken:?}");
        app.edit(&format!("{}/pages/B.org", app.project.path));
        let back: Vec<String> = app.frame_items(orgamnesia_core::Frame::Backlinks).1.into_iter().map(|(l, _)| l).collect();
        assert_eq!(back, ["A"]);
        // A narrow screen has no right column: the shortcut opens the frame over the page
        app.right_column = false;
        press(&mut app, KeyCode::F(8), KeyModifiers::NONE);
        assert_eq!(app.popup().unwrap().lines, ["A"]);
        press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        // F10: the list of the pages
        press(&mut app, KeyCode::F(10), KeyModifiers::NONE);
        assert!(app.editor.is_none());
        app.edit(&format!("{}/pages/B.org", app.project.path));
        // F2: B renamed C, the link in A follows
        press(&mut app, KeyCode::F(2), KeyModifiers::NONE);
        app.prompt_mut().unwrap().input = "C".into();
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert!(app.editor.as_ref().unwrap().path.ends_with("C.org"));
        assert!(std::fs::read_to_string(&a).unwrap().contains("[[C]]"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_custom_profile_gets_the_new_actions() {
        // A profile of an older version, without the actions added since
        let file = orgamnesia_core::keybindings::ProfileFile {
            editor_preset: "emacs".into(),
            app: [("quick_open".to_string(), vec!["alt+k".to_string()])].into(),
            editor: Default::default(),
        };
        let kb = Keybindings::from_files(Some("Mine"), vec![("Mine".into(), file)]);
        for (id, key) in [("rename_page", "F2"), ("panel_todos", "F4"), ("panel_tags", "F5"), ("panel_backlinks", "F8"), ("panel_broken", "F9")] {
            assert_eq!(kb.app_binds(id), &[key.to_string()], "{id}");
        }
        assert_eq!(kb.app_binds("quick_open"), &["alt+k".to_string()]);
        assert!(orgamnesia_core::keybindings::conflicts(&kb.app, &kb.editor).is_empty());
        assert!(help(&kb, false).contains("F1 shortcuts"));
    }

    #[test]
    fn names_complete_as_in_the_window() {
        let dir = std::env::temp_dir().join(format!("orgamnesia-tui-complete-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("pages")).unwrap();
        std::fs::write(dir.join("pages/Alpha.org"), "* Alpha :projet:\n").unwrap();
        std::fs::write(dir.join("pages/Lugny.org"), "* Histoire\n* Rues").unwrap();
        std::fs::write(dir.join("pages/Lunes.org"), "x").unwrap();
        let prefs = settings::Settings::default().prefs();
        let project = Project::open(&crate::slash(&dir), prefs.tags(), prefs.formats(), &prefs.journal_folder, &prefs.templates_folder).unwrap();
        let mut app = App::new(project, prefs, Keybindings::with_profiles(None, vec![]));
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        press(&mut app, KeyCode::End, KeyModifiers::CONTROL);
        let typed = |app: &mut App, s: &str| for c in s.chars() { press(app, KeyCode::Char(c), KeyModifiers::NONE); };
        // [[lu: the pages, the link closed on accepting
        typed(&mut app, "voir [[lu");
        assert_eq!(app.suggest().unwrap().items, ["Lugny", "Lunes"]);
        press(&mut app, KeyCode::Down, KeyModifiers::NONE);
        press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
        assert!(app.editor.as_ref().unwrap().text().ends_with("voir [[Lunes]]"), "{}", app.editor.as_ref().unwrap().text());
        // [[Lugny::*: its headlines
        typed(&mut app, " [[Lugny::*R");
        // Those starting with what is typed first, then those holding it (as in the window)
        assert_eq!(app.suggest().unwrap().items, ["*Rues", "*Histoire"]);
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert!(app.editor.as_ref().unwrap().text().ends_with("[[Lugny::*Rues]]"));
        // #lu: the pages; :pro the org-mode tags
        typed(&mut app, " #lu");
        assert_eq!(app.suggest().unwrap().items, ["Lugny", "Lunes"]);
        press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        typed(&mut app, " :pro");
        assert_eq!(app.suggest().unwrap().items, ["projet"]);
        press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        // A headline's tags
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        typed(&mut app, "* Titre :pro");
        assert_eq!(app.suggest().map(|s| s.items.clone()), Some(vec!["projet".to_string()]));
        press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        // Ctrl+Left: word by word, as Alt+B
        let at = app.editor.as_ref().unwrap().cursor;
        press(&mut app, KeyCode::Left, KeyModifiers::CONTROL);
        assert_eq!(app.editor.as_ref().unwrap().cursor, at - 3);
        press(&mut app, KeyCode::Right, KeyModifiers::CONTROL);
        assert_eq!(app.editor.as_ref().unwrap().cursor, at);
        // The tag line: Tab completes with the tags of the project
        app.command("add_tag");
        app.prompt_mut().unwrap().input = "pro".into();
        press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(app.prompt().unwrap().input, "projet");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn typing_after_a_space_adds_nothing() {
        let dir = std::env::temp_dir().join(format!("orgamnesia-tui-space-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("pages")).unwrap();
        std::fs::write(dir.join("pages/Alpha.org"), "* Alpha :projet:\n").unwrap();
        std::fs::write(dir.join("pages/Lugny.org"), "x").unwrap();
        let prefs = settings::Settings::default().prefs();
        let project = Project::open(&crate::slash(&dir), prefs.tags(), prefs.formats(), &prefs.journal_folder, &prefs.templates_folder).unwrap();
        let mut app = App::new(project, prefs, Keybindings::with_profiles(None, vec![]));
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        press(&mut app, KeyCode::End, KeyModifiers::CONTROL);
        for s in ["\nun mot", "\n* Titre :pro", "\nvoir :pro et", "\n#lu x", "\nl'arbre", "\nx :a b", "\n* T :p:q r"] {
            for c in s.chars() {
                if c == '\n' { press(&mut app, KeyCode::Enter, KeyModifiers::NONE); } else { press(&mut app, KeyCode::Char(c), KeyModifiers::NONE); }
            }
            if app.suggest().is_some() { press(&mut app, KeyCode::Esc, KeyModifiers::NONE); }
        }
        let text = app.editor.as_ref().unwrap().text();
        assert_eq!(text, "* Alpha :projet:\n\nun mot\n* Titre :pro\nvoir :pro et\n#lu x\nl'arbre\nx :a b\n* T :p:q r");        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn each_frame_has_its_colour() {
        use orgamnesia_core::Frame;
        let colours: Vec<_> = [Frame::Pages, Frame::Project, Frame::Git, Frame::Backlinks, Frame::Tags, Frame::Todos, Frame::Broken]
            .into_iter().map(|f| frame_title(f, String::new()).style.fg).collect();
        let mut distinct = colours.clone();
        distinct.dedup();
        assert_eq!(distinct.len(), colours.len());
        assert!(frame_title(Frame::Pages, " Pages ".into()).style.add_modifier.contains(Modifier::BOLD));
    
}

#[test]
fn one_thing_has_the_keys_at_a_time() {
    let dir = std::env::temp_dir().join(format!("orgamnesia-tui-modes-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::write(dir.join("pages/Lugny.org"), "x").unwrap();
    std::fs::write(dir.join("pages/Lunes.org"), "x").unwrap();
    let prefs = settings::Settings::default().prefs();
    let project = Project::open(&crate::slash(&dir), prefs.tags(), prefs.formats(), &prefs.journal_folder, &prefs.templates_folder).unwrap();
    let mut app = App::new(project, prefs, Keybindings::with_profiles(None, vec![]));
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    for c in "[[lu".chars() { press(&mut app, KeyCode::Char(c), KeyModifiers::NONE); }
    assert!(app.suggest().is_some());
    // A shortcut takes the keys from the suggestions: the shortcuts window, alone
    press(&mut app, KeyCode::F(1), KeyModifiers::NONE);
    assert!(app.popup().is_some() && app.suggest().is_none());
    // Esc closes it: the page has the keys again, typing goes there
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(matches!(app.mode, Mode::Normal));
    press(&mut app, KeyCode::Char('x'), KeyModifiers::NONE);
    assert!(app.editor.as_ref().unwrap().text().contains("[[lux"));
    // The quick open over it; Ctrl+G closes it as Esc does
    press(&mut app, KeyCode::Char('k'), KeyModifiers::CONTROL);
    assert!(matches!(app.mode, Mode::Quick(_)));
    press(&mut app, KeyCode::Char('g'), KeyModifiers::CONTROL);
    assert!(matches!(app.mode, Mode::Normal));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn random_page_and_zen_mode() {
    let dir = std::env::temp_dir().join(format!("orgamnesia-tui-random-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    for name in ["One.org", "Two.org"] { std::fs::write(dir.join("pages").join(name), "x").unwrap(); }
    let prefs = settings::Settings::default().prefs();
    let project = Project::open(&crate::slash(&dir), prefs.tags(), prefs.formats(), &prefs.journal_folder, &prefs.templates_folder).unwrap();
    let mut app = App::new(project, prefs, Keybindings::with_profiles(None, vec![]));
    // Ctrl+Alt+R: the other page each time, never the one edited
    press(&mut app, KeyCode::Char('r'), KeyModifiers::CONTROL | KeyModifiers::ALT);
    let first = app.editor.as_ref().unwrap().path.clone();
    press(&mut app, KeyCode::Char('r'), KeyModifiers::CONTROL | KeyModifiers::ALT);
    assert_ne!(app.editor.as_ref().unwrap().path, first);
    // What the screen shows: the tips in their frame, none of it in zen mode, only the way out
    let screen = |app: &mut App| {
        let mut term = ratatui::Terminal::new(ratatui::backend::TestBackend::new(140, 40)).unwrap();
        term.draw(|f| app.draw(f)).unwrap();
        term.backend().buffer().content().iter().map(|c| c.symbol()).collect::<String>()
    };
    assert!(screen(&mut app).contains("Did you know?"));
    // F11 in and out of zen mode
    press(&mut app, KeyCode::F(11), KeyModifiers::NONE);
    assert!(app.zen);
    let zen = screen(&mut app);
    assert!(!zen.contains("Did you know?") && zen.contains("F11: leave zen mode"), "{zen}");
    press(&mut app, KeyCode::F(11), KeyModifiers::NONE);
    assert!(!app.zen);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn tab_and_enter_move_in_a_table() {
    let dir = std::env::temp_dir().join(format!("orgamnesia-tui-table-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::write(dir.join("pages/T.org"), "| a | b |\n| c | d |").unwrap();
    let prefs = settings::Settings::default().prefs();
    let project = Project::open(&crate::slash(&dir), prefs.tags(), prefs.formats(), &prefs.journal_folder, &prefs.templates_folder).unwrap();
    let mut app = App::new(project, prefs, Keybindings::with_profiles(None, vec![]));
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    app.editor.as_mut().unwrap().cursor = 2;
    // Tab: from "a" to "b"; Enter: from "b" to "d"
    press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(app.editor.as_ref().unwrap().cursor, 6);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(app.editor.as_ref().unwrap().cursor, 16);
    std::fs::remove_dir_all(&dir).unwrap();
}
