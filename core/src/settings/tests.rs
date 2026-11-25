use super::*;

#[test]
fn each_frame_has_its_own_setting() {
    for frame in crate::layout::Frame::ALL {
        let mut p = Prefs::default();
        *p.frame_setting(frame) = false;
        assert!(!p.frame_on(frame) && !p.frame_shown(frame), "{frame:?}");
        *p.frame_setting(frame) = true;
        assert!(p.frame_on(frame), "{frame:?}");
        // The others are untouched by it
        let other = Prefs::default();
        for f in crate::layout::Frame::ALL.into_iter().filter(|f| *f != frame) {
            assert_eq!(p.frame_on(f), other.frame_on(f), "{frame:?} changed {f:?}");
        }
    }
}

#[test]
fn frames_of_features_turned_off_are_hidden() {
    use crate::layout::Frame;
    let mut p = Prefs::default();
    for f in [Frame::Git, Frame::Todos, Frame::OrgBloom, Frame::Graph] { *p.frame_setting(f) = true; }
    (p.git_ext, p.todo_enabled, p.org_bloom, p.graph) = (false, false, false, false);
    for f in [Frame::Git, Frame::Todos, Frame::OrgBloom, Frame::Graph] {
        assert!(p.frame_on(f) && !p.frame_shown(f), "{f:?}");
        assert!(!p.column(false).contains(&f) && !p.column(true).contains(&f));
    }
}

#[test]
fn missing_values_take_their_default() {
    let p = Settings::default().prefs();
    assert!(p.autosave && p.git_ext && !p.org_bloom && !p.delete_empty && p.restore_session);
    assert_eq!(p.lang, "fr");
    assert_eq!(p.image_size_format, "org");
    assert!(p.link_preview && p.embed_videos);
    assert_eq!(p.link_preview_trigger, "ctrl");
    assert_eq!(p.formats(), Formats { default: Format::Org, single: false });
    assert!(!p.ask_format);
    assert_eq!((p.tags().org_links, p.tags().md_links), (LinkSyntax::NONE, LinkSyntax::ALL));
    assert_eq!(p.todo(), Some(TodoKeywords::parse(DEFAULT_TODO_KEYWORDS)));
    let blank = Settings { config: Config { todo_keywords: Some(" ".into()), ..Config::default() }, ..Settings::default() };
    assert_eq!(blank.prefs().todo_keywords, DEFAULT_TODO_KEYWORDS);
    assert_eq!(p.tags(), TagSyntax::from(Hashtags::Dashes));
}

#[test]
fn link_syntax_in_settings_json() {
    let c: Config = serde_json::from_str(r#"{"org_links": {"pipe": true}, "md_links": {"hash": true, "markdown": true}}"#).unwrap();
    assert_eq!(c.org_links, Some(LinkSyntax { pipe: true, ..LinkSyntax::NONE }));
    assert_eq!(c.md_links, Some(LinkSyntax { pipe: false, ..LinkSyntax::ALL }));
}

#[test]
fn missing_keys_of_a_file_take_their_default() {
    let profile: crate::keybindings::ProfileFile = serde_json::from_str("{}").unwrap();
    assert_eq!(profile.editor_preset, "classic");
    let session: Session = serde_json::from_str("{}").unwrap();
    assert_eq!(session, Session::default());
}

#[test]
fn prefs_round_trip_through_settings() {
    let mut p = Prefs { vault: "/x".into(), autosave: false, git_ext: false, default_format: Format::Markdown, single_format: true, ..Prefs::default() };
    p.layout.move_frame(Frame::Git, crate::Move::OtherColumn, |_| false);
    let s = p.to_settings(vec!["/x".into()]);
    assert_eq!(s.prefs(), p);
    assert_eq!(s.projects, vec!["/x".to_string()]);
    assert_eq!(Prefs::default().to_settings(vec![]).vault_path, None);
}
