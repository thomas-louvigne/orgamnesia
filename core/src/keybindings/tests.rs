use super::*;
use super::presets::{clash, m};

#[test]
fn old_files_hold_profiles() {
    // A custom profile active: its shortcuts as they are in effect (at the top)
    let old = r#"{"editor_preset": "emacs", "active_profile": "c1",
        "app": {"save": ["ctrl+j"]}, "editor": {},
        "profiles": [{"id": "c1", "name": "Mine", "editor_preset": "emacs", "app": {}, "editor": {}}]}"#;
    let (active, profiles) = Keybindings::from_old(old).unwrap().to_files();
    assert_eq!((active.as_str(), profiles.len(), profiles[0].0.as_str()), ("Mine", 1, "Mine"));
    assert_eq!(profiles[0].1.app.get("save"), Some(&vec!["ctrl+j".to_string()]));
    let kb = Keybindings::from_files(Some(&active), profiles);
    assert_eq!(kb.app_binds("save"), &["ctrl+j".to_string()]);
    // Older than the profiles: shortcuts of its own become a profile
    let (active, profiles) = Keybindings::from_old(r#"{"editor_preset": "classic", "app": {"save": ["ctrl+j"]}, "editor": {}}"#).unwrap().to_files();
    assert_eq!((active.as_str(), profiles.len()), (DEFAULT_PROFILE_NAME, 1));
    assert_eq!(Keybindings::from_old(r#"{"active_profile": "emacs"}"#), None);
}

#[test]
fn profile_files_are_named_after_the_profiles() {
    let profile = |id: &str, name: &str| Profile { id: id.into(), name: name.into(), editor_preset: "emacs".into(), app: Bindings::new(), editor: Bindings::new() };
    let kb = Keybindings::with_profiles(Some("b"), vec![profile("a", "Perso"), profile("b", "perso"), profile("c", "a/b"), profile("d", " "), profile("e", "emacs")]);
    let (active, files) = kb.to_files();
    let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, vec!["Perso", "perso 2", "a-b", DEFAULT_PROFILE_NAME, "emacs 2"]);
    assert_eq!(active, "perso 2");
}

#[test]
fn a_built_in_profile_is_its_preset() {
    let kb = Keybindings::from_files(Some("emacs"), vec![]);
    assert_eq!((kb.app.clone(), kb.editor_preset.as_str()), (preset_app("emacs"), "emacs"));
    // A profile that is not there: the default one
    let kb = Keybindings::from_files(Some("gone"), vec![]);
    assert_eq!(kb.active_profile, "classic");
    assert_eq!(kb.to_files().0, "classic");
}

#[test]
fn presets_only_bind_known_actions() {
    for preset in EDITOR_PRESETS {
        for id in preset_app(preset).keys() {
            assert!(AppAction::from_id(id).is_some(), "{preset}: {id}");
        }
        for id in preset_editor(preset).keys() {
            assert!(EditorAction::from_id(id).is_some(), "{preset}: {id}");
        }
    }
}

#[test]
fn presets_have_no_conflicts() {
    for preset in EDITOR_PRESETS {
        assert_eq!(conflicts(&preset_app(preset), &preset_editor(preset)), vec![], "{preset}");
    }
}

#[test]
fn preset_bindings_are_canonical() {
    for preset in EDITOR_PRESETS {
        for b in preset_app(preset).values().chain(preset_editor(preset).values()).flatten() {
            assert_eq!(&canonical(b), b, "{preset}");
        }
    }
    assert_eq!(canonical("alt+shift+<"), "shift+alt+<");
    assert_eq!(canonical("ctrl+c ctrl+x ctrl+f shift++"), "ctrl+c ctrl+x ctrl+f shift++");
    assert_eq!(canonical("alt+ctrl+x"), "ctrl+alt+x");

    let mut kb = Keybindings::defaults();
    kb.profiles.push(Profile {
        id: "c1".into(), name: "Mine".into(), editor_preset: "emacs".into(),
        app: Bindings::new(), editor: m(&[("replace", &["alt+shift+%"])]),
    });
    kb.upgrade();
    assert_eq!(kb.profiles[0].editor["replace"], vec!["shift+alt+%"]);
}

#[test]
fn upgrade_resets_builtin_profile_and_fills_custom_ones() {
    let mut kb = Keybindings { active_profile: "emacs".into(), ..Keybindings::defaults() };
    kb.app.clear();
    kb.upgrade();
    assert_eq!(kb.app, preset_app("emacs"));
    assert_eq!(kb.editor_preset, "emacs");

    let mut custom = Keybindings::defaults();
    custom.profiles.push(Profile {
        id: "c1".into(), name: "Mine".into(), editor_preset: "classic".into(),
        app: Bindings::new(), editor: Bindings::new(),
    });
    custom.active_profile = "c1".into();
    custom.app = Bindings::new();
    custom.upgrade();
    assert_eq!(custom.profiles[0].app, preset_app("classic"));
    assert_eq!(custom.app, preset_app("classic"));
}

#[test]
fn fill_missing_leaves_out_shortcuts_already_taken() {
    // A profile saved when the settings were on `ctrl+,` in Emacs, before `spell_next` took it
    let mut app = m(&[("open_settings", &["ctrl+,"]), ("quit", &["ctrl+x ctrl+c"])]);
    let mut editor = Bindings::new();
    fill_missing(&mut app, &mut editor, "emacs");
    assert_eq!(app["open_settings"], vec!["ctrl+,"]);
    assert!(editor["spell_next"].is_empty());
    // Still filled when nothing is in the way
    assert_eq!(editor["find"], vec!["ctrl+s"]);
    assert!(!clash("ctrl+x", "ctrl+xb") && clash("ctrl+x", "ctrl+x ctrl+s"));
}

#[test]
fn fill_missing_moves_shortcuts_the_preset_handed_over() {
    // An Emacs profile saved when Alt+{ / Alt+} were the paragraph motions
    let mut app = Bindings::new();
    let mut editor = m(&[("backward_paragraph", &["shift+alt+{", "ctrl+ArrowUp"])]);
    fill_missing(&mut app, &mut editor, "emacs");
    assert_eq!(editor["backward_element"], vec!["shift+alt+{"]);
    assert_eq!(editor["backward_paragraph"], vec!["ctrl+ArrowUp"]);
    // Left alone once the profile knows the new action
    let mut editor = m(&[("backward_paragraph", &["shift+alt+{"]), ("backward_element", &[])]);
    fill_missing(&mut app, &mut editor, "emacs");
    assert_eq!(editor["backward_paragraph"], vec!["shift+alt+{"]);
}
