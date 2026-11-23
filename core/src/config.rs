//! The configuration files the "preference" project edits as text: which they
//! are, and whether a text is a valid one (what is wrong, and where). They are
//! YAML, so that they may hold comments; those of older versions were JSON
//! (`File::legacy_name`), read once to write them as YAML.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    keybindings::{self, Bindings, Keybindings, ProfileFile, APP_ACTIONS, EDITOR_ACTIONS},
    settings::{Config, STATE_KEYS},
};

/// Folder of the configuration folder holding the user's own keybinding profiles.
pub const PROFILES_DIR: &str = "keybindings";

/// A configuration file of the app.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum File {
    /// `settings.yaml` (the keybinding profile in effect among them).
    Settings,
    /// `keybindings/<name>.yaml`: a keybinding profile of the user's own.
    Profile(String),
}

impl File {
    /// The files always there, in the order the "preference" project lists them
    /// (the profiles come after).
    pub const FIXED: [File; 1] = [File::Settings];

    /// Its path in the configuration folder.
    pub fn name(&self) -> String {
        match self {
            File::Settings => "settings.yaml".to_string(),
            File::Profile(name) => format!("{PROFILES_DIR}/{name}.yaml"),
        }
    }

    /// Its path when it was JSON (older versions): read when there is no YAML one.
    pub fn legacy_name(&self) -> String {
        self.name().replace(".yaml", ".json")
    }

    /// The file at `path` (in the configuration folder), if it is one.
    pub fn from_path(path: &str) -> Option<File> {
        let mut parts = path.rsplit(['/', '\\']);
        let name = parts.next()?;
        let folder = parts.next();
        if folder == Some(PROFILES_DIR) {
            let name = name.strip_suffix(".yaml").filter(|n| !n.is_empty())?;
            return Some(File::Profile(name.to_string()));
        }
        File::FIXED.into_iter().find(|f| f.name() == name)
    }

}

/// What a configuration file holds (only passed along: its size doesn't matter).
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Content {
    Settings(Config),
    Profile(ProfileFile),
}

/// The configuration in effect (after a file changed): the settings, and the
/// keybindings of the profile they name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InEffect {
    pub settings: Config,
    pub keybindings: Keybindings,
}

/// A valid configuration file, and what is odd in it without making it wrong.
#[derive(Debug, Clone, PartialEq)]
pub struct Checked {
    pub content: Content,
    pub warnings: Vec<Problem>,
}

/// What is wrong (or odd) in a configuration file, and where: line and column, from 1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Problem {
    pub line: usize,
    pub column: usize,
    pub kind: ProblemKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum ProblemKind {
    /// Not JSON, or a value of the wrong type: what the JSON reader said.
    Syntax(String),
    /// The file holds something else than a JSON object.
    NotAnObject,
    /// A key that is no setting (a typo?).
    UnknownKey(String),
    /// A key the app keeps for itself, in `state.json`.
    StateKey(String),
    /// A shortcut given to several actions: the shortcut, the label keys of the actions.
    Conflict { combo: String, labels: Vec<String> },
    /// `keybinding_profile` names neither a built-in profile nor one of the user's.
    UnknownProfile(String),
    /// A profile's `editor_preset` is no built-in profile.
    UnknownPreset(String),
    /// A profile file has the name of a built-in profile, whose shortcuts are in
    /// the app and can't be changed.
    BuiltinId(String),
    /// Warning: no action has this id, its shortcuts do nothing.
    UnknownAction(String),
}

/// Whether `text` is a valid `file`: what it holds, or the first thing wrong in
/// it. `profiles`: the names of the user's own keybinding profiles.
pub fn check(file: &File, text: &str, profiles: &[String]) -> Result<Checked, Problem> {
    match file {
        File::Settings => check_settings(text, profiles),
        File::Profile(name) => check_profile(name, text),
    }
}

/// The settings that do nothing in the terminal interface (`orgamnesia -nw`),
/// and why; written as comments above them in `settings.yaml`.
pub const TUI_UNSUPPORTED: &[(&str, &str)] = &[
    ("org_bloom_path", NOT_YET), ("org_bloom_enabled", NOT_YET), ("org_bloom_zip", NOT_YET), ("org_bloom_no_external_deps", NOT_YET),
    ("language", "the terminal is in English for now"),
    ("theme", "the terminal has its own colours"),
    ("ligatures", "the font is the terminal's"),
        ("indent_headings", NOT_YET),     ("show_images", NO_IMAGES), ("show_video_thumbnails", NO_IMAGES), ("embed_videos", NO_IMAGES),
    ("image_size_format", NO_IMAGES), ("show_excalidraw", NO_IMAGES),
    ("link_preview", MOUSE), ("link_preview_trigger", MOUSE),
    ("webkit_emoji_shortcuts", "the window's engine (WebKitGTK) only"),
    ("shortcut_tips", LAYOUT),
        ("show_brand", LAYOUT), ("show_page_title", LAYOUT), ("show_page_nav", LAYOUT), ("show_quit_button", LAYOUT),
    ("show_pages", LAYOUT), ("show_backlinks", LAYOUT), ("show_tags", LAYOUT), ("show_broken_links", LAYOUT),
    ("show_tips", LAYOUT), ("show_todos", LAYOUT), ("show_project", LAYOUT), ("show_git", LAYOUT),
    ("show_settings_button", LAYOUT), ("frames_left", LAYOUT), ("frames_right", LAYOUT),
    ("show_org_bloom", LAYOUT), ("show_graph", LAYOUT),
    ("spell_enabled", NOT_YET), ("spell_language", NOT_YET), ("spell_dict_dir", NOT_YET),
    ("git_unpushed_marker", NOT_YET),
    ("check_updates", UPDATES), ("skipped_update", UPDATES),
];
const NOT_YET: &str = "not in the terminal yet";
const NO_IMAGES: &str = "no images in a terminal";
const LAYOUT: &str = "the window's layout";
const MOUSE: &str = "on mouse hover, in the window";
const UPDATES: &str = "updates go through the window";

/// The settings by the tab of the settings window showing them, in its order:
/// how `settings.yaml` is written, each tab as a comment above its settings.
pub const SECTIONS: &[(&str, &[&str])] = &[
    ("General", &["language", "default_format", "single_format", "ask_format", "restore_session", "check_updates", "skipped_update"]),
    ("Links", &["org_links", "md_links", "case_insensitive_links", "update_links_on_rename", "click_follows_links", "hashtag_links", "hashtag_dashes",
        "org_tag_links", "org_tag_dashes", "mark_broken_links", "link_preview", "link_preview_trigger", "embed_videos"]),
    ("Editing", &["autosave", "electric_mode", "electric_emphasis", "find_match_case", "indent_headings", "tab_folds",
        "todo_enabled", "todo_keywords", "delete_empty_pages", "delete_title_only_pages", "trash_auto_empty", "trash_keep_days", "trash_max_bytes"]),
    ("Display", &["show_page_title", "show_page_nav", "show_line_numbers", "show_images", "image_size_format", "show_excalidraw",
        "show_video_thumbnails", "hide_leading_stars", "live_preview", "ligatures", "link_text_first"]),
    ("Frames", &["frames_left", "frames_right", "show_brand", "show_project", "show_git", "show_pages", "show_backlinks", "show_tags",
        "show_broken_links", "show_todos", "show_org_bloom", "show_graph", "show_tips", "show_quit_button", "show_settings_button"]),
    ("Themes", &["theme"]),
    ("Shortcuts", &["keybinding_profile", "emacs_mark", "webkit_emoji_shortcuts", "shortcut_tips"]),
    ("Spelling", &["spell_enabled", "spell_language", "spell_dict_dir"]),
    ("Extensions", &["org_bloom_enabled", "org_bloom_path", "org_bloom_zip", "org_bloom_no_external_deps", "git_status_enabled",
        "git_ssh_key", "git_fetch_minutes", "git_pull_rebase", "git_unpushed_marker", "git_commit_push", "git_commit_template",
        "git_auto_sync_minutes", "git_sync_on_quit", "graph_enabled", "journal_enabled", "journal_folder"]),
];

/// What the app writes at the top of `settings.yaml`.
const SETTINGS_NOTE: &str = "\
# Orgamnesia settings. Comments (#) are kept as long as the app does not rewrite
# this file (it does when the settings window changes a setting).
# TUI version (orgamnesia -nw): the settings followed by “# TUI version: …” do
# nothing there, for the reason given.
";

/// What the app writes at the top of a keybinding profile: the shortcuts the
/// terminal interface can't do, and why.
pub const PROFILE_NOTE: &str = "\
# TUI version (orgamnesia -nw): these shortcuts work in the window only.
# - export_pdf, export_html: the export needs the window's renderer (HTML, printing).
# - Ctrl+Shift+…, Ctrl+Tab, Ctrl+Shift+Tab: some terminals keep them (kitty: its tabs,
#   copy and paste…), the app never gets them; give these actions another key too.
# - Ctrl+, Ctrl+/ Ctrl+Enter: only terminals with kitty's keyboard protocol (kitty, foot,
#   WezTerm, Ghostty…) tell them from other keys.
";

/// The text the app writes for `content`: YAML, with comments.
pub fn to_text(content: &Content) -> String {
    match content {
        Content::Settings(c) => {
            let mut out = SETTINGS_NOTE.to_string();
            let Ok(serde_yaml_ng::Value::Mapping(mut map)) = serde_yaml_ng::to_value(c) else { return out };
            // One setting at a time, the comment of those the terminal does not use at the end of its line
            let write = |out: &mut String, key: serde_yaml_ng::Value, value: serde_yaml_ng::Value| {
                let why = TUI_UNSUPPORTED.iter().find(|(k, _)| key.as_str() == Some(k)).map(|(_, why)| *why);
                let mut one = serde_yaml_ng::Mapping::new();
                one.insert(key, value);
                let text = serde_yaml_ng::to_string(&one).unwrap_or_default();
                match (why, text.split_once('\n')) {
                    (Some(why), Some((first, rest))) => out.push_str(&format!("{first}  # TUI version: {why}\n{rest}")),
                    _ => out.push_str(&text),
                }
            };
            // By the tabs of the settings window
            for (title, keys) in SECTIONS {
                out.push_str(&format!("\n# ── {title} ──\n"));
                for key in *keys {
                    if let Some((k, v)) = map.remove_entry(*key) { write(&mut out, k, v); }
                }
            }
            if !map.is_empty() { out.push_str("Other ──\n"); }
            for (k, v) in map { write(&mut out, k, v); }
            out
        }
        Content::Profile(p) => format!("{PROFILE_NOTE}{}", serde_yaml_ng::to_string(p).unwrap_or_default()),
    }
}

/// The content of a file of an older version (JSON), when it is a valid one.
pub fn from_legacy(file: &File, json: &str) -> Option<Content> {
    match file {
        File::Settings => serde_json::from_str::<Config>(json).ok().map(Content::Settings),
        File::Profile(_) => serde_json::from_str::<ProfileFile>(json).ok().map(Content::Profile),
    }
}

/// The settings `text` holds, read as the app reads them (what is wrong left at its default).
pub fn read_settings(text: &str) -> Option<Config> {
    serde_yaml_ng::from_str::<Option<Config>>(text).ok().map(Option::unwrap_or_default)
}

/// The keybinding profile `text` holds.
pub fn read_profile(text: &str) -> Option<ProfileFile> {
    serde_yaml_ng::from_str::<ProfileFile>(text).ok()
}

fn check_settings(text: &str, profiles: &[String]) -> Result<Checked, Problem> {
    let object = object(text)?;
    let known = keys(&Config::default());
    for key in &object {
        if STATE_KEYS.contains(&key.as_str()) {
            return Err(at_key(text, key, ProblemKind::StateKey(key.clone())));
        }
        if !known.contains(key) {
            return Err(at_key(text, key, ProblemKind::UnknownKey(key.clone())));
        }
    }
    let config = serde_yaml_ng::from_str::<Option<Config>>(text).map_err(|e| syntax(&e))?.unwrap_or_default();
    if let Some(name) = &config.keybinding_profile && !keybindings::is_builtin(name) && !profiles.contains(name) {
        return Err(at_key(text, "keybinding_profile", ProblemKind::UnknownProfile(name.clone())));
    }
    Ok(Checked { content: Content::Settings(config), warnings: vec![] })
}

fn check_profile(name: &str, text: &str) -> Result<Checked, Problem> {
    if keybindings::is_builtin(name) {
        return Err(Problem { line: 1, column: 1, kind: ProblemKind::BuiltinId(name.to_string()) });
    }
    let object = object(text)?;
    let known = keys(&ProfileFile { editor_preset: String::new(), app: Bindings::new(), editor: Bindings::new() });
    if let Some(key) = object.iter().find(|k| !known.contains(k)) {
        return Err(at_key(text, key, ProblemKind::UnknownKey(key.clone())));
    }
    let profile = serde_yaml_ng::from_str::<ProfileFile>(text).map_err(|e| syntax(&e))?;
    if !keybindings::is_builtin(&profile.editor_preset) {
        return Err(at_key(text, "editor_preset", ProblemKind::UnknownPreset(profile.editor_preset.clone())));
    }
    let (app, editor) = (normalized(&profile.app), normalized(&profile.editor));
    if let Some((combo, labels)) = keybindings::conflicts(&app, &editor).into_iter().next() {
        let labels = labels.into_iter().map(str::to_string).collect();
        return Err(at_value(text, &combo, ProblemKind::Conflict { combo: combo.clone(), labels }));
    }
    let mut warnings = vec![];
    for (map, app) in [(&profile.app, true), (&profile.editor, false)] {
        for id in map.keys() {
            let known = if app { APP_ACTIONS.iter().any(|a| a.id == id) } else { EDITOR_ACTIONS.iter().any(|a| a.id == id) };
            if !known { warnings.push(at_key(text, id, ProblemKind::UnknownAction(id.clone()))); }
        }
    }
    Ok(Checked { content: Content::Profile(profile), warnings })
}

/// What a piece of a line of a configuration file is, to draw it in colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Token { Key, Str, Number, Literal, Punct, Comment }

/// The pieces of the line `line` of a YAML configuration file, as char ranges
/// (`key: value  # comment`, `- ctrl+s`, `[a, "b"]`); the spaces are left out.
/// The window and the terminal draw the files with them.
pub fn yaml_tokens(line: &str) -> Vec<(usize, usize, Token)> {
    let c: Vec<char> = line.chars().collect();
    let mut out = vec![];
    // A key: what is followed by `:` (and a space or the end)
    let is_key = |end: usize| {
        let mut j = end;
        while j < c.len() && c[j] == ' ' { j += 1; }
        j < c.len() && c[j] == ':' && c.get(j + 1).is_none_or(|n| n.is_whitespace())
    };
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        if ch.is_whitespace() { i += 1; continue; }
        if ch == '#' && (i == 0 || c[i - 1].is_whitespace()) {
            out.push((i, c.len(), Token::Comment));
            break;
        }
        if ch == '"' || ch == '\'' {
            // To the closing quote (`\"` escapes one in double quotes, `''` in single ones)
            let mut j = i + 1;
            while j < c.len() {
                if ch == '"' && c[j] == '\\' { j += 2; continue; }
                if c[j] == ch {
                    if ch == '\'' && c.get(j + 1) == Some(&'\'') { j += 2; continue; }
                    break;
                }
                j += 1;
            }
            let end = (j + 1).min(c.len());
            out.push((i, end, if is_key(end) { Token::Key } else { Token::Str }));
            i = end;
            continue;
        }
        let dash = ch == '-' && c.get(i + 1).is_none_or(|n| n.is_whitespace());
        let colon = ch == ':' && c.get(i + 1).is_none_or(|n| n.is_whitespace());
        if dash || colon || "[]{},".contains(ch) {
            out.push((i, i + 1, Token::Punct));
            i += 1;
            continue;
        }
        // A plain value, to `: `, ` #`, a bracket or a comma (in brackets)
        let mut j = i;
        while j < c.len() && !"[]{},".contains(c[j])
            && !(c[j] == ':' && c.get(j + 1).is_none_or(|n| n.is_whitespace()))
            && !(c[j] == '#' && c[j - 1].is_whitespace()) { j += 1; }
        let mut end = j;
        while end > i && c[end - 1].is_whitespace() { end -= 1; }
        let word: String = c[i..end].iter().collect();
        let token = if is_key(end) { Token::Key } else { match word.as_str() {
            "true" | "false" | "null" | "~" => Token::Literal,
            _ if word.parse::<f64>().is_ok() => Token::Number,
            _ => Token::Str,
        } };
        out.push((i, end, token));
        i = j;
    }
    out
}

/// The keys of the mapping `text` holds (an empty file holds none).
fn object(text: &str) -> Result<Vec<String>, Problem> {
    match serde_yaml_ng::from_str::<serde_yaml_ng::Value>(text).map_err(|e| syntax(&e))? {
        serde_yaml_ng::Value::Mapping(map) => Ok(map.keys().map(|k| k.as_str().map_or_else(|| format!("{k:?}"), str::to_string)).collect()),
        serde_yaml_ng::Value::Null => Ok(vec![]),
        _ => Err(Problem { line: 1, column: 1, kind: ProblemKind::NotAnObject }),
    }
}

/// The keys `value` is written with.
fn keys(value: &impl Serialize) -> Vec<String> {
    match serde_json::to_value(value) {
        Ok(Value::Object(map)) => map.into_iter().map(|(k, _)| k).collect(),
        _ => vec![],
    }
}

/// Shortcuts as the app compares them: canonical, without the empty ones.
fn normalized(bindings: &Bindings) -> Bindings {
    bindings.iter()
        .map(|(id, binds)| (id.clone(), binds.iter().filter(|b| !b.is_empty()).map(|b| keybindings::canonical(b)).collect::<Vec<_>>()))
        .filter(|(_, binds)| !binds.is_empty())
        .collect()
}

/// What the YAML reader said, without the position it adds (the problem has it).
fn syntax(e: &serde_yaml_ng::Error) -> Problem {
    let message = e.to_string();
    let message = message.split_once(" at line ").map_or(message.as_str(), |(m, _)| m);
    let (line, column) = e.location().map_or((1, 1), |l| (l.line(), l.column()));
    Problem { line: line.max(1), column: column.max(1), kind: ProblemKind::Syntax(message.to_string()) }
}

/// `kind`, at the first line where the key `key` is written (else the start).
fn at_key(text: &str, key: &str, kind: ProblemKind) -> Problem {
    let (line, column) = find(text, |t| {
        ["", "\"", "'"].iter().any(|q| t.strip_prefix(q).and_then(|t| t.strip_prefix(key)).and_then(|t| t.strip_prefix(q))
            .is_some_and(|t| t.trim_start().starts_with(':')))
            .then_some(0)
    }).unwrap_or((1, 1));
    Problem { line, column, kind }
}

/// `kind`, at the first place the value `value` is written (else the start).
fn at_value(text: &str, value: &str, kind: ProblemKind) -> Problem {
    let (line, column) = find(text, |t| {
        let mut from = 0;
        while let Some(i) = t[from..].find(value) {
            let at = from + i;
            from = at + value.len();
            let before = t[..at].chars().last();
            let after = t[from..].chars().next();
            if before.is_none_or(|c| " -\"'[,".contains(c)) && after.is_none_or(|c| " \"',]#".contains(c)) {
                return Some(t[..at].chars().count());
            }
        }
        None
    }).unwrap_or((1, 1));
    Problem { line, column, kind }
}

/// Line and column (from 1) of the first line, comments left out, where `at`
/// finds what it looks for in the line's text after its indentation (giving
/// where, in chars from there).
fn find(text: &str, at: impl Fn(&str) -> Option<usize>) -> Option<(usize, usize)> {
    text.lines().enumerate().find_map(|(n, line)| {
        let t = line.trim_start();
        if t.starts_with('#') { return None; }
        let indent = line.chars().count() - t.chars().count();
        at(t).map(|c| (n + 1, indent + c + 1))
    })
}

#[cfg(test)]
mod tests {
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
            assert!(text.starts_with("# TUI version"));
            assert_eq!(check(&File::Profile(name), &text, &[]).unwrap().warnings, vec![]);
            assert_eq!(read_profile(&text), Some(p));
        }
    }

    #[test]
    fn the_settings_say_what_the_terminal_does_not_do() {
        let text = to_text(&Content::Settings(Config::default()));
        let lines: Vec<&str> = text.lines().collect();
        let line = |key: &str| lines.iter().find(|l| l.starts_with(&format!("{key}:"))).copied();
        assert_eq!(line("show_images"), Some("show_images: null  # TUI version: no images in a terminal"));
        assert!(!line("autosave").unwrap().contains('#'));
        assert!(TUI_UNSUPPORTED.iter().all(|(k, _)| line(k).is_some_and(|l| l.contains("# TUI version"))), "a key of TUI_UNSUPPORTED is no setting");
        // A value on several lines: the comment after its key
        let c = Config { frames_left: Some(vec!["pages".into()]), ..Config::default() };
        let text = to_text(&Content::Settings(c.clone()));
        assert!(text.contains("frames_left:  # TUI version: the window's layout\n- pages\n"), "{text}");
        assert_eq!(read_settings(&text), Some(c));
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
}
