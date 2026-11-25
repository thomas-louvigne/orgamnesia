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
mod descriptions;
pub use descriptions::*;
use descriptions::SETTINGS_NOTE;

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

/// The text the app writes for `content`: YAML, with comments.
pub fn to_text(content: &Content) -> String {
    match content {
        Content::Settings(c) => {
            let mut out = SETTINGS_NOTE.to_string();
            let Ok(serde_yaml_ng::Value::Mapping(mut map)) = serde_yaml_ng::to_value(c) else { return out };
            // One setting at a time: what it does above it, at the end of its line why
            // the terminal does not use it
            let write = |out: &mut String, key: serde_yaml_ng::Value, value: serde_yaml_ng::Value| {
                let find = |table: &[(&str, &'static str)]| table.iter().find(|(k, _)| key.as_str() == Some(k)).map(|(_, v)| *v);
                let (what, why) = (find(DESCRIPTIONS), find(TUI_UNSUPPORTED));
                if let Some(what) = what {
                    for line in what.lines() { out.push_str(&format!("# {line}\n")); }
                }
                let mut one = serde_yaml_ng::Mapping::new();
                one.insert(key, value);
                let text = serde_yaml_ng::to_string(&one).unwrap_or_default();
                match (why, text.split_once('\n')) {
                    (Some(why), Some((first, rest))) => out.push_str(&format!("{first}  # window only: {why}\n{rest}")),
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
            if !map.is_empty() { out.push_str("\n# ── Other ──\n"); }
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
mod tests;
