//! The git line, the session, the frames and panels, completion, tables, zen mode.

use super::*;

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
    // The quick open over it; Ctrl+G, no shortcut of the classic keys, leaves it; Esc closes it
    press(&mut app, KeyCode::Char('k'), KeyModifiers::CONTROL);
    assert!(matches!(app.mode, Mode::Quick(_)));
    press(&mut app, KeyCode::Char('g'), KeyModifiers::CONTROL);
    assert!(matches!(app.mode, Mode::Quick(_)));
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
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

#[test]
fn the_pages_opened_and_the_scrollbar_are_drawn() {
    let dir = std::env::temp_dir().join(format!("orgamnesia-tui-tabs-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    std::fs::write(dir.join("pages/Short.org"), "x").unwrap();
    std::fs::write(dir.join("pages/Long.org"), "line\n".repeat(200)).unwrap();
    let prefs = settings::Settings::default().prefs();
    let project = Project::open(&crate::slash(&dir), prefs.tags(), prefs.formats(), &prefs.journal_folder, &prefs.templates_folder).unwrap();
    let mut app = App::new(project, prefs, Keybindings::with_profiles(None, vec![]));
    let screen = |app: &mut App| {
        let mut term = ratatui::Terminal::new(ratatui::backend::TestBackend::new(140, 40)).unwrap();
        term.draw(|f| app.draw(f)).unwrap();
        term.backend().buffer().content().iter().map(|c| c.symbol()).collect::<String>()
    };
    // One page: no tab bar, no thumb
    app.edit(&crate::slash(dir.join("pages/Short.org")));
    assert!(!screen(&mut app).contains('█'));
    // Two pages: their names side by side; the long one has a thumb
    app.edit(&crate::slash(dir.join("pages/Long.org")));
    let s = screen(&mut app);
    assert!(s.contains("Short │ Long") && s.contains('█'), "{s}");
    // A click on a name of the bar: that page edited
    let bar = app.tab_bar;
    app.mouse(MouseEvent { kind: MouseEventKind::Down(MouseButton::Left), column: bar.x + 2, row: bar.y, modifiers: KeyModifiers::NONE });
    assert!(app.editor.as_ref().unwrap().path.ends_with("Short.org"));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn an_image_is_drawn_under_its_link_when_the_page_is_read() {
    let dir = std::env::temp_dir().join(format!("orgamnesia-tui-image-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("assets")).unwrap();
    std::fs::create_dir_all(dir.join("pages")).unwrap();
    image::RgbImage::from_pixel(40, 40, image::Rgb([255, 0, 0])).save(dir.join("assets/red.png")).unwrap();
    std::fs::write(dir.join("pages/P.org"), "before\n[[../assets/red.png]]\nafter").unwrap();
    let prefs = settings::Settings::default().prefs();
    let project = Project::open(&crate::slash(&dir), prefs.tags(), prefs.formats(), &prefs.journal_folder, &prefs.templates_folder).unwrap();
    let mut app = App::new(project, prefs, Keybindings::with_profiles(None, vec![]));
    app.images.picker = Some(ratatui_image::picker::Picker::halfblocks());
    let mut term = ratatui::Terminal::new(ratatui::backend::TestBackend::new(140, 40)).unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    // Under the link, a row of red half blocks; "after" pushed down below the image
    let buf = term.backend().buffer();
    let a = app.page_area;
    let red = |y| (a.x..a.right()).any(|x| buf[(x, y)].fg == ratatui::style::Color::Rgb(255, 0, 0));
    let row = |y| (a.x..a.right()).map(|x| buf[(x, y)].symbol()).collect::<String>();
    assert!(row(a.y + 2).contains("red.png") && red(a.y + 3), "{}", row(a.y + 3));
    assert!(!row(a.y + 4).contains("after"));
    std::fs::remove_dir_all(&dir).unwrap();
}
