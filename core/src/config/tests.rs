use super::*;

fn problem(file: File, text: &str) -> Problem {
    check(&file, text, &["mine".to_string()]).unwrap_err()
}

#[test]
fn what_the_app_writes_is_valid() {
    let mut config = crate::Prefs::default().to_settings(vec![]).config;
    config.keybinding_profile = Some("Mine".into());
    let settings = to_text(&Content::Settings(config.clone()));
    let checked = check(&File::Settings, &settings, &["Mine".into()]).unwrap();
    assert_eq!(checked.warnings, vec![]);
    assert_eq!(checked.content, Content::Settings(config));
    assert_eq!(read_settings(&settings).unwrap().keybinding_profile.as_deref(), Some("Mine"));
    let mine = crate::keybindings::Profile {
        id: "x".into(), name: "Mine".into(), editor_preset: "emacs".into(),
        app: keybindings::preset_app("emacs"), editor: keybindings::preset_editor("emacs"),
    };
    let (active, profiles) = Keybindings::with_profiles(Some("x"), vec![mine]).to_files();
    assert_eq!(active, "Mine");
    for (name, p) in profiles {
        let text = to_text(&Content::Profile(p.clone()));
        assert!(text.starts_with("# Terminal version"));
        assert_eq!(check(&File::Profile(name), &text, &[]).unwrap().warnings, vec![]);
        assert_eq!(read_profile(&text), Some(p));
    }
}

#[test]
fn the_settings_say_what_the_terminal_does_not_do() {
    let text = to_text(&Content::Settings(Config::default()));
    let lines: Vec<&str> = text.lines().collect();
    let line = |key: &str| lines.iter().find(|l| l.starts_with(&format!("{key}:"))).copied();
    assert_eq!(line("show_images"), Some("show_images: null  # window only: no images in a terminal"));
    assert!(!line("autosave").unwrap().contains('#'));
    assert!(TUI_UNSUPPORTED.iter().all(|(k, _)| line(k).is_some_and(|l| l.contains("# window only"))), "a key of TUI_UNSUPPORTED is no setting");
    // A value on several lines: the comment after its key
    let c = Config { frames_left: Some(vec!["pages".into()]), ..Config::default() };
    let text = to_text(&Content::Settings(c.clone()));
    assert!(text.contains("frames_left:  # window only: the window's layout\n- pages\n"), "{text}");
    assert_eq!(read_settings(&text), Some(c));
}

#[test]
fn every_setting_is_described() {
    let text = to_text(&Content::Settings(Config::default()));
    let lines: Vec<&str> = text.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        // A setting (not a value of one): the line above is its description
        let Some((key, _)) = line.split_once(':').filter(|_| !line.starts_with(['#', ' ', '-'])) else { continue };
        assert!(lines[i - 1].starts_with("# ") && !lines[i - 1].starts_with("# ──"), "{key} has no description");
    }
    assert!(DESCRIPTIONS.iter().all(|(k, _)| SECTIONS.iter().any(|(_, keys)| keys.contains(k))), "a description of no setting");
    assert!(!text.contains("Other"), "a setting in no section of SECTIONS");
}

#[test]
fn older_json_files_are_read() {
    let Some(Content::Settings(c)) = from_legacy(&File::Settings, r#"{"autosave": true, "language": "fr"}"#) else { panic!() };
    assert_eq!((c.autosave, c.language.as_deref()), (Some(true), Some("fr")));
    assert_eq!(File::Profile("Mine".into()).legacy_name(), "keybindings/Mine.json");
    assert_eq!(File::Settings.legacy_name(), "settings.json");
}

#[test]
fn syntax_errors_say_where() {
    let p = problem(File::Settings, "autosave: true\nlanguage: [fr\n");
    assert!(matches!(p.kind, ProblemKind::Syntax(_)), "{p:?}");
    assert!(p.line >= 2, "{p:?}");
    let p = problem(File::Settings, "# note\nautosave: oui\n");
    assert_eq!(p.line, 2);
    assert!(matches!(p.kind, ProblemKind::Syntax(ref m) if m.contains("boolean")), "{p:?}");
    assert_eq!(problem(File::Settings, "- a\n- b").kind, ProblemKind::NotAnObject);
    // Empty: all at their defaults
    assert!(check(&File::Settings, "# rien\n", &[]).is_ok());
}

#[test]
fn unknown_and_state_keys_are_refused() {
    let p = problem(File::Settings, "# autosav: in a comment\n  autosav: true\n");
    assert_eq!(p, Problem { line: 2, column: 3, kind: ProblemKind::UnknownKey("autosav".into()) });
    let p = problem(File::Settings, "projects: []");
    assert_eq!(p.kind, ProblemKind::StateKey("projects".into()));
    // A profile is named after its file
    let p = problem(File::Profile("mine".into()), "name: M\neditor_preset: emacs");
    assert_eq!(p.kind, ProblemKind::UnknownKey("name".into()));
}

#[test]
fn keybindings_rules() {
    assert!(check(&File::Settings, "keybinding_profile: emacs", &[]).is_ok());
    assert!(check(&File::Settings, "keybinding_profile: mine", &["mine".into()]).is_ok());
    let p = problem(File::Settings, "keybinding_profile: theirs");
    assert_eq!(p.kind, ProblemKind::UnknownProfile("theirs".into()));
    // A profile's shortcuts may not clash
    let text = "editor_preset: classic\napp:\n  save:\n  - ctrl+s\n  quit: [ctrl+s]\n";
    let p = problem(File::Profile("mine".into()), text);
    assert!(matches!(p.kind, ProblemKind::Conflict { ref combo, .. } if combo == "ctrl+s"), "{p:?}");
    assert_eq!((p.line, p.column), (4, 5));
    let p = problem(File::Profile("mine".into()), "editor_preset: vi");
    assert_eq!(p.kind, ProblemKind::UnknownPreset("vi".into()));
    // The built-in profiles are in the app
    let p = problem(File::Profile("emacs".into()), "editor_preset: emacs");
    assert_eq!(p.kind, ProblemKind::BuiltinId("emacs".into()));
    // An unknown action is worth a warning
    let warnings = check(&File::Profile("mine".into()), "editor_preset: classic\napp: {sav: [ctrl+j]}", &[]).unwrap().warnings;
    let kinds: Vec<_> = warnings.into_iter().map(|w| w.kind).collect();
    assert_eq!(kinds, vec![ProblemKind::UnknownAction("sav".into())]);
}

#[test]
fn files_by_path() {
    assert_eq!(File::from_path("/home/me/.config/x/keybindings.json"), None);
    assert_eq!(File::from_path("/home/me/.config/x/keybindings/Personnalisé.yaml"), Some(File::Profile("Personnalisé".into())));
    assert_eq!(File::from_path("/home/me/.config/x/keybindings/Personnalisé.json"), None);
    assert_eq!(File::Profile("mine".into()).name(), "keybindings/mine.yaml");
    assert_eq!(File::from_path("/x/history/keybindings-12.yaml"), None);
    assert_eq!(File::from_path("settings.yaml"), Some(File::Settings));
    assert_eq!(File::from_path("state.json"), None);
}

#[test]
fn yaml_lines_in_pieces() {
    let pieces = |line: &str| yaml_tokens(line).into_iter()
        .map(|(a, b, t)| (line.chars().skip(a).take(b - a).collect::<String>(), t)).collect::<Vec<_>>();
    use Token::*;
    assert_eq!(pieces("  autosave: true  # on"), [("autosave".into(), Key), (":".into(), Punct), ("true".into(), Literal), ("# on".into(), Comment)]);
    assert_eq!(pieces("- ctrl+x ctrl+s"), [("-".into(), Punct), ("ctrl+x ctrl+s".into(), Str)]);
    assert_eq!(pieces("quit: [ctrl+q, \"a#b\"]"), [("quit".into(), Key), (":".into(), Punct), ("[".into(), Punct),
        ("ctrl+q".into(), Str), (",".into(), Punct), ("\"a#b\"".into(), Str), ("]".into(), Punct)]);
    assert_eq!(pieces("trash_keep_days: 30"), [("trash_keep_days".into(), Key), (":".into(), Punct), ("30".into(), Number)]);
    assert_eq!(pieces("url: http://x # y"), [("url".into(), Key), (":".into(), Punct), ("http://x".into(), Str), ("# y".into(), Comment)]);
    assert_eq!(pieces("# terminal : non"), [("# terminal : non".into(), Comment)]);
}

#[test]
fn the_settings_are_written_by_tab() {
    let all = keys(&Config::default());
    let listed: Vec<&str> = SECTIONS.iter().flat_map(|(_, keys)| keys.iter().copied()).collect();
    let missing: Vec<&String> = all.iter().filter(|k| !listed.contains(&k.as_str())).collect();
    assert!(missing.is_empty(), "settings in no tab: {missing:?}");
    let mut sorted = listed.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), listed.len(), "a setting in two tabs");
    assert!(listed.iter().all(|k| all.iter().any(|a| a == k)), "a tab names no setting");
    let text = to_text(&Content::Settings(Config::default()));
    let (general, links) = (text.find("# ── General ──").unwrap(), text.find("Links ──").unwrap());
    let language = text.find("\nlanguage:").unwrap();
    assert!(general < language && language < links);
    assert!(!text.contains("Autres"));
}
