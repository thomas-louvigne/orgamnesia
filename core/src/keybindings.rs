use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

mod actions;
mod presets;
pub use actions::*;
pub use presets::*;

/// An action maps to a list of bindings (0, 1 or 2 accelerators).
pub type Bindings = BTreeMap<String, Vec<String>>;

pub fn default_preset() -> String { "classic".to_string() }

/// A user-made shortcut profile. The built-in ones ("classic", "emacs") are not stored.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Profile {
    pub id: String,
    pub name: String,
    /// Built-in preset this profile derives from (decides which editor actions exist).
    pub editor_preset: String,
    pub app: Bindings,
    pub editor: Bindings,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Keybindings {
    /// Base preset of the active profile ("classic" | "emacs").
    #[serde(default = "default_preset")]
    pub editor_preset: String,
    /// Application-level shortcuts of the active profile (action id -> bindings).
    #[serde(default)]
    pub app: Bindings,
    /// Editor shortcuts of the active profile (action id -> bindings).
    #[serde(default)]
    pub editor: Bindings,
    /// Id of the active profile: a built-in preset id or a custom `Profile::id`.
    #[serde(default)]
    pub active_profile: String,
    /// Custom profiles.
    #[serde(default)]
    pub profiles: Vec<Profile>,
}

/// Name of a profile of the user's own when it has none (the French of the app's
/// default language, as the settings window names it).
pub const DEFAULT_PROFILE_NAME: &str = "Personnalisé";

/// `keybindings/<name>.yaml`: a profile of the user's own, named after its file.
/// Which profile is active is a setting (`keybinding_profile`); the built-in
/// profiles are in the code.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProfileFile {
    /// Built-in preset it derives from (decides which editor actions exist).
    #[serde(default = "default_preset")]
    pub editor_preset: String,
    #[serde(default)]
    pub app: Bindings,
    #[serde(default)]
    pub editor: Bindings,
}

/// `name` as a file name: without `/` nor `\`, not hidden, not empty.
fn file_name(name: &str) -> String {
    let name: String = name.trim().chars().map(|c| if matches!(c, '/' | '\\') { '-' } else { c }).collect();
    let name = name.trim_start_matches('.').trim().to_string();
    if name.is_empty() { DEFAULT_PROFILE_NAME.to_string() } else { name }
}

impl Keybindings {
    /// The keybindings with the user's `profiles`, `active` the active one
    /// (built-in or one of them): its shortcuts in effect. A profile that is not
    /// there gives way to the default one.
    pub fn with_profiles(active: Option<&str>, profiles: Vec<Profile>) -> Self {
        let profiles: Vec<Profile> = profiles.into_iter().filter(|p| !is_builtin(&p.id)).collect();
        let wanted = active.unwrap_or_default();
        let (active, preset, app, editor) = match profiles.iter().find(|p| p.id == wanted) {
            Some(p) => (p.id.clone(), p.editor_preset.clone(), p.app.clone(), p.editor.clone()),
            None => {
                let id = if is_builtin(wanted) { wanted.to_string() } else { default_preset() };
                (id.clone(), id.clone(), preset_app(&id), preset_editor(&id))
            }
        };
        let mut kb = Self { editor_preset: preset, app, editor, active_profile: active, profiles };
        kb.upgrade();
        kb
    }

    /// The keybindings from their files: the user's profiles by name, `active`
    /// the name of the active one (the setting).
    pub fn from_files(active: Option<&str>, profiles: Vec<(String, ProfileFile)>) -> Self {
        let profiles = profiles.into_iter()
            .map(|(name, p)| Profile { id: name.clone(), name, editor_preset: p.editor_preset, app: p.app, editor: p.editor })
            .collect();
        Self::with_profiles(active, profiles)
    }

    /// The files of these keybindings: the user's own profiles, by the names of
    /// their files (their own names, made unique and fit for a file), the
    /// shortcuts of the active one as they are now; and the name of the active
    /// profile (the setting).
    pub fn to_files(&self) -> (String, Vec<(String, ProfileFile)>) {
        let active = self.resolved_active();
        let mut active_name = active.clone();
        let mut taken: Vec<String> = EDITOR_PRESETS.iter().map(|s| s.to_string()).collect();
        let mut files = vec![];
        for p in self.profiles.iter().filter(|p| !is_builtin(&p.id)) {
            let base = file_name(&p.name);
            let mut name = base.clone();
            let mut n = 2;
            while taken.iter().any(|t| t.to_lowercase() == name.to_lowercase()) {
                name = format!("{base} {n}");
                n += 1;
            }
            taken.push(name.clone());
            if p.id == active { active_name = name.clone(); }
            let (app, editor) = if p.id == active { (self.app.clone(), self.editor.clone()) } else { (p.app.clone(), p.editor.clone()) };
            files.push((name, ProfileFile { editor_preset: p.editor_preset.clone(), app, editor }));
        }
        (active_name, files)
    }

    /// `text`, a `keybindings.json` of an older version (the profiles in it, the
    /// active one, its shortcuts at the top): the keybindings it held. Shortcuts
    /// of a file older than the profiles that are not those of its preset become
    /// a profile of their own. `None` when it holds no shortcuts.
    pub fn from_old(text: &str) -> Option<Self> {
        let map = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(text).ok()?;
        if !["app", "editor", "profiles", "editor_preset"].iter().any(|k| map.contains_key(*k)) { return None; }
        let mut kb = serde_json::from_str::<Keybindings>(text).ok()?;
        let base = kb.editor_preset.clone();
        if kb.active_profile.is_empty() {
            let same = kb.app == preset_app(&base) && kb.editor == preset_editor(&base);
            kb.active_profile = if same { base.clone() } else { "custom".to_string() };
            if !same && !kb.profiles.iter().any(|p| p.id == "custom") {
                kb.profiles.push(Profile {
                    id: "custom".into(), name: DEFAULT_PROFILE_NAME.into(), editor_preset: base,
                    app: kb.app.clone(), editor: kb.editor.clone(),
                });
            }
        }
        // The active profile's shortcuts are those at the top
        let active = kb.resolved_active();
        let (app, editor) = (kb.app.clone(), kb.editor.clone());
        if let Some(p) = kb.profiles.iter_mut().find(|p| p.id == active) {
            p.app = app;
            p.editor = editor;
        }
        Some(Self::with_profiles(Some(&active), kb.profiles))
    }
}

impl Default for Keybindings {
    fn default() -> Self { Self::defaults() }
}

impl Keybindings {
    pub fn defaults() -> Self {
        Self {
            editor_preset: default_preset(),
            app: default_app(),
            editor: preset_editor("classic"),
            active_profile: default_preset(),
            profiles: vec![],
        }
    }

    /// Id of the active profile, falling back to the base preset for files
    /// written before profiles existed.
    pub fn resolved_active(&self) -> String {
        if is_builtin(&self.active_profile)
            || self.profiles.iter().any(|p| p.id == self.active_profile)
        {
            self.active_profile.clone()
        } else {
            self.editor_preset.clone()
        }
    }

    /// Bring shortcuts read from the file up to date. A built-in profile is
    /// always its current definition, so improved defaults reach existing users.
    /// (Files written before profiles existed have no active profile and keep
    /// their bindings as they are.) Custom profiles get the shortcuts of their
    /// base preset for actions added since they were saved.
    pub fn upgrade(&mut self) {
        for p in self.profiles.iter_mut() {
            let base = p.editor_preset.clone();
            fill_missing(&mut p.app, &mut p.editor, &base);
            canonicalize(&mut p.app);
            canonicalize(&mut p.editor);
        }
        canonicalize(&mut self.app);
        canonicalize(&mut self.editor);
        if self.active_profile.is_empty() { return; }
        let active = self.resolved_active();
        if is_builtin(&active) {
            self.app = preset_app(&active);
            self.editor = preset_editor(&active);
            self.editor_preset = active.clone();
            self.active_profile = active;
        } else {
            let base = self.editor_preset.clone();
            fill_missing(&mut self.app, &mut self.editor, &base);
        }
    }

    /// Bindings for an app action (empty slice if unbound).
    pub fn app_binds(&self, id: &str) -> &[String] {
        self.app.get(id).map(|v| v.as_slice()).unwrap_or(&[])
    }
}

/// A binding with the modifiers of each key in the order key presses are
/// written in (`ctrl+shift+alt+key`), the only one they are matched with.
/// Bindings written otherwise ("alt+shift+<", once in the Emacs preset) never matched.
pub fn canonical(binding: &str) -> String {
    binding.split(' ').map(|combo| {
        let parts: Vec<&str> = combo.split('+').collect();
        // The key itself may be "+" ("ctrl+shift++"): the last part, or an empty one
        let (mods, key) = match parts.as_slice() {
            [mods @ .., "", ""] => (mods, "+"),
            [mods @ .., key] => (mods, *key),
            [] => return combo.to_string(),
        };
        let mut out: Vec<&str> = ["ctrl", "shift", "alt"].into_iter().filter(|m| mods.contains(m)).collect();
        out.push(key);
        out.join("+")
    }).collect::<Vec<_>>().join(" ")
}

fn canonicalize(map: &mut Bindings) {
    for binds in map.values_mut() {
        for b in binds.iter_mut() { *b = canonical(b); }
    }
}

#[cfg(test)]
mod tests;
