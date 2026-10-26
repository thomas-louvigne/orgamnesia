//! The configuration files the "preference" project edits as text: which they
//! are, and whether a text is a valid one (what is wrong, and where).

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

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
    /// `settings.json` (the keybinding profile in effect among them).
    Settings,
    /// `keybindings/<name>.json`: a keybinding profile of the user's own.
    Profile(String),
}

impl File {
    /// The files always there, in the order the "preference" project lists them
    /// (the profiles come after).
    pub const FIXED: [File; 1] = [File::Settings];

    /// Its path in the configuration folder.
    pub fn name(&self) -> String {
        match self {
            File::Settings => "settings.json".to_string(),
            File::Profile(name) => format!("{PROFILES_DIR}/{name}.json"),
        }
    }

    /// The file at `path` (in the configuration folder), if it is one.
    pub fn from_path(path: &str) -> Option<File> {
        let mut parts = path.rsplit(['/', '\\']);
        let name = parts.next()?;
        let folder = parts.next();
        if folder == Some(PROFILES_DIR) {
            let name = name.strip_suffix(".json").filter(|n| !n.is_empty())?;
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

/// The text the app writes for `content`.
pub fn to_text(content: &Content) -> String {
    let text = match content {
        Content::Settings(c) => serde_json::to_string_pretty(c),
        Content::Profile(p) => serde_json::to_string_pretty(p),
    };
    text.unwrap_or_default()
}

fn check_settings(text: &str, profiles: &[String]) -> Result<Checked, Problem> {
    let object = object(text)?;
    let known = keys(&Config::default());
    for key in object.keys() {
        if STATE_KEYS.contains(&key.as_str()) {
            return Err(at_key(text, key, ProblemKind::StateKey(key.clone())));
        }
        if !known.contains(key) {
            return Err(at_key(text, key, ProblemKind::UnknownKey(key.clone())));
        }
    }
    let config = serde_json::from_str::<Config>(text).map_err(|e| syntax(&e))?;
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
    unknown_keys(text, &object, &known)?;
    let profile = serde_json::from_str::<ProfileFile>(text).map_err(|e| syntax(&e))?;
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

/// The JSON object `text` holds.
fn object(text: &str) -> Result<Map<String, Value>, Problem> {
    match serde_json::from_str::<Value>(text).map_err(|e| syntax(&e))? {
        Value::Object(map) => Ok(map),
        _ => Err(Problem { line: 1, column: 1, kind: ProblemKind::NotAnObject }),
    }
}

/// The keys of the JSON object `value` is written as.
fn keys(value: &impl Serialize) -> Vec<String> {
    match serde_json::to_value(value) {
        Ok(Value::Object(map)) => map.into_iter().map(|(k, _)| k).collect(),
        _ => vec![],
    }
}

fn unknown_keys(text: &str, object: &Map<String, Value>, known: &[String]) -> Result<(), Problem> {
    match object.keys().find(|k| !known.contains(k)) {
        Some(key) => Err(at_key(text, key, ProblemKind::UnknownKey(key.clone()))),
        None => Ok(()),
    }
}

/// Shortcuts as the app compares them: canonical, without the empty ones.
fn normalized(bindings: &Bindings) -> Bindings {
    bindings.iter()
        .map(|(id, binds)| (id.clone(), binds.iter().filter(|b| !b.is_empty()).map(|b| keybindings::canonical(b)).collect::<Vec<_>>()))
        .filter(|(_, binds)| !binds.is_empty())
        .collect()
}

/// What the JSON reader said, without the position it adds (the problem has it).
fn syntax(e: &serde_json::Error) -> Problem {
    let message = e.to_string();
    let message = message.rsplit_once(" at line ").map_or(message.as_str(), |(m, _)| m);
    Problem { line: e.line().max(1), column: e.column().max(1), kind: ProblemKind::Syntax(message.to_string()) }
}

/// `kind`, at the first place the key `key` is written (else the start).
fn at_key(text: &str, key: &str, kind: ProblemKind) -> Problem {
    find_string(text, key, true).or_else(|| find_string(text, key, false)).map_or(
        Problem { line: 1, column: 1, kind: kind.clone() },
        |(line, column)| Problem { line, column, kind: kind.clone() },
    )
}

/// `kind`, at the first place the string `value` is written as a value (else the start).
fn at_value(text: &str, value: &str, kind: ProblemKind) -> Problem {
    let (line, column) = find_string(text, value, false).unwrap_or((1, 1));
    Problem { line, column, kind }
}

/// Line and column of the first `"s"` in `text`, as a key (followed by `:`) or not.
fn find_string(text: &str, s: &str, key: bool) -> Option<(usize, usize)> {
    let quoted = serde_json::to_string(s).ok()?;
    let mut from = 0;
    while let Some(i) = text[from..].find(&quoted) {
        let at = from + i;
        from = at + quoted.len();
        if text[from..].trim_start().starts_with(':') == key {
            let before = &text[..at];
            let line = before.matches('\n').count() + 1;
            let column = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
            return Some((line, column));
        }
    }
    None
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
        let settings = to_text(&Content::Settings(config));
        assert_eq!(check(&File::Settings, &settings, &["Mine".into()]).unwrap().warnings, vec![]);
        let mine = crate::keybindings::Profile {
            id: "x".into(), name: "Mine".into(), editor_preset: "emacs".into(),
            app: keybindings::preset_app("emacs"), editor: keybindings::preset_editor("emacs"),
        };
        let (active, profiles) = Keybindings::with_profiles(Some("x"), vec![mine]).to_files();
        assert_eq!(active, "Mine");
        for (name, p) in profiles {
            assert_eq!(check(&File::Profile(name), &to_text(&Content::Profile(p)), &[]).unwrap().warnings, vec![]);
        }
    }

    #[test]
    fn syntax_errors_say_where() {
        let p = problem(File::Settings, "{\n  \"autosave\": true\n  \"language\": \"fr\"\n}");
        assert_eq!((p.line, p.column), (3, 3));
        assert!(matches!(p.kind, ProblemKind::Syntax(ref m) if m.starts_with("expected `,`")), "{p:?}");
        let p = problem(File::Settings, "{\n  \"autosave\": \"yes\"\n}");
        assert_eq!(p.line, 2);
        assert!(matches!(p.kind, ProblemKind::Syntax(ref m) if m.contains("expected a boolean")), "{p:?}");
        assert_eq!(problem(File::Settings, "[]").kind, ProblemKind::NotAnObject);
    }

    #[test]
    fn unknown_and_state_keys_are_refused() {
        let p = problem(File::Settings, "{\n  \"autosav\": true\n}");
        assert_eq!(p, Problem { line: 2, column: 3, kind: ProblemKind::UnknownKey("autosav".into()) });
        let p = problem(File::Settings, "{\"projects\": []}");
        assert_eq!(p.kind, ProblemKind::StateKey("projects".into()));
        // A profile is named after its file
        let p = problem(File::Profile("mine".into()), r#"{"name": "M", "editor_preset": "emacs"}"#);
        assert_eq!(p.kind, ProblemKind::UnknownKey("name".into()));
    }

    #[test]
    fn keybindings_rules() {
        assert!(check(&File::Settings, r#"{"keybinding_profile": "emacs"}"#, &[]).is_ok());
        assert!(check(&File::Settings, r#"{"keybinding_profile": "mine"}"#, &["mine".into()]).is_ok());
        let p = problem(File::Settings, r#"{"keybinding_profile": "theirs"}"#);
        assert_eq!(p.kind, ProblemKind::UnknownProfile("theirs".into()));
        // A profile's shortcuts may not clash
        let text = "{\"editor_preset\": \"classic\",\n \"app\": {\"save\": [\"ctrl+s\"], \"quit\": [\"ctrl+s\"]}}";
        let p = problem(File::Profile("mine".into()), text);
        assert!(matches!(p.kind, ProblemKind::Conflict { ref combo, .. } if combo == "ctrl+s"), "{p:?}");
        assert_eq!((p.line, p.column), (2, 19));
        let p = problem(File::Profile("mine".into()), r#"{"editor_preset": "vi"}"#);
        assert_eq!(p.kind, ProblemKind::UnknownPreset("vi".into()));
        // The built-in profiles are in the app
        let p = problem(File::Profile("emacs".into()), r#"{"editor_preset": "emacs"}"#);
        assert_eq!(p.kind, ProblemKind::BuiltinId("emacs".into()));
        // An unknown action is worth a warning
        let warnings = check(&File::Profile("mine".into()), r#"{"editor_preset": "classic", "app": {"sav": ["ctrl+j"]}}"#, &[]).unwrap().warnings;
        let kinds: Vec<_> = warnings.into_iter().map(|w| w.kind).collect();
        assert_eq!(kinds, vec![ProblemKind::UnknownAction("sav".into())]);
    }

    #[test]
    fn files_by_path() {
        assert_eq!(File::from_path("/home/me/.config/x/keybindings.json"), None);
        assert_eq!(File::from_path("/home/me/.config/x/keybindings/Personnalisé.json"), Some(File::Profile("Personnalisé".into())));
        assert_eq!(File::Profile("mine".into()).name(), "keybindings/mine.json");
        assert_eq!(File::from_path("/x/history/keybindings-12.json"), None);
        assert_eq!(File::from_path("state.json"), None);
    }
}
