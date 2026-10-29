//! The spelling extension: Hunspell dictionaries (`.aff` + `.dic`) found on the
//! system, read with spellbook, and the user's own words.

use std::path::{Path, PathBuf};

use spellbook::Dictionary;

use crate::{error::AppError, settings::Prefs};

/// The user's own words, one per line, in the config folder.
const PERSONAL: &str = "dictionary.txt";

/// Suggestions given for a word, at most.
const MAX_SUGGESTIONS: usize = 8;

/// Folders where dictionaries are looked for, first ones first: the one of the
/// settings, `$DICPATH`, the system's (Hunspell, Myspell, LibreOffice), then
/// those installed with the app (Windows, which has none).
pub fn dict_dirs(extra: &str) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if !extra.trim().is_empty() { dirs.push(PathBuf::from(extra.trim())); }
    if let Some(paths) = std::env::var_os("DICPATH") { dirs.extend(std::env::split_paths(&paths)); }
    let home = crate::home_dir();
    if let Some(home) = &home {
        dirs.push(home.join(".local/share/hunspell"));
        dirs.push(home.join("Library/Spelling"));
    }
    for d in ["/usr/share/hunspell", "/usr/share/myspell", "/usr/share/myspell/dicts",
              "/usr/local/share/hunspell", "/opt/homebrew/share/hunspell", "/Library/Spelling"] {
        dirs.push(PathBuf::from(d));
    }
    // LibreOffice's own dictionaries (Windows, macOS), one folder per language
    for root in ["C:\\Program Files\\LibreOffice\\share\\extensions",
                 "C:\\Program Files (x86)\\LibreOffice\\share\\extensions",
                 "/Applications/LibreOffice.app/Contents/Resources/extensions"] {
        if let Ok(entries) = std::fs::read_dir(root) {
            dirs.extend(entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()));
        }
    }
    if let Some(res) = crate::resource_dir() { dirs.push(res.join("dictionaries")); }
    dirs
}

/// The dictionaries of `dirs`: their name (`fr_FR`) and the path without extension,
/// the first found for each name.
fn find_dicts(dirs: &[PathBuf]) -> Vec<(String, PathBuf)> {
    let mut found: Vec<(String, PathBuf)> = Vec::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        let mut here: Vec<(String, PathBuf)> = entries.flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "dic") && p.with_extension("aff").is_file())
            .filter_map(|p| Some((p.file_stem()?.to_str()?.to_string(), p.with_extension(""))))
            .filter(|(name, _)| !name.starts_with("hyph"))
            .collect();
        here.sort();
        for (name, path) in here {
            if !found.iter().any(|(n, _)| *n == name) { found.push((name, path)); }
        }
    }
    found
}

/// Names of the dictionaries found, sorted.
pub fn languages(extra: &str) -> Vec<String> {
    let mut names: Vec<String> = find_dicts(&dict_dirs(extra)).into_iter().map(|(n, _)| n).collect();
    names.sort();
    names
}

/// The dictionary to use among `names`: `wanted`, or when empty the one of the
/// interface language `ui` (`fr` → `fr_FR`, else `fr`, else any `fr_…`).
fn pick<'a>(names: &'a [String], wanted: &str, ui: &str) -> Option<&'a String> {
    if !wanted.is_empty() { return names.iter().find(|n| *n == wanted); }
    let main = format!("{ui}_{}", ui.to_uppercase());
    names.iter().find(|n| **n == main)
        .or_else(|| names.iter().find(|n| *n == ui))
        .or_else(|| names.iter().find(|n| n.starts_with(&format!("{ui}_"))))
}

/// The settings choosing the dictionary: the one asked, the folder of
/// dictionaries, the interface language.
type Choice = (String, String, String);

/// The dictionary in use, read again when the settings choose another one.
#[derive(Default)]
pub struct Speller {
    /// The dictionary read for these settings, with the user's words.
    loaded: Option<(Choice, Dictionary)>,
}

impl Speller {
    /// The dictionary the settings choose, read if needed.
    fn dict(&mut self, prefs: &Prefs, config_dir: &Path) -> Result<&mut Dictionary, AppError> {
        let choice = (prefs.spell_lang.clone(), prefs.spell_dict_dir.clone(), prefs.lang.clone());
        if self.loaded.as_ref().is_none_or(|(c, _)| *c != choice) {
            let found = find_dicts(&dict_dirs(&prefs.spell_dict_dir));
            let names: Vec<String> = found.iter().map(|(n, _)| n.clone()).collect();
            let wanted = if prefs.spell_lang.is_empty() { &prefs.lang } else { &prefs.spell_lang };
            let name = pick(&names, &prefs.spell_lang, &prefs.lang).ok_or_else(|| AppError::NoDictionary(wanted.clone()))?;
            let path = found.iter().find(|(n, _)| n == name).map(|(_, p)| p.clone()).unwrap_or_default();
            let aff = std::fs::read_to_string(path.with_extension("aff"))?;
            let dic = std::fs::read_to_string(path.with_extension("dic"))?;
            let mut dict = Dictionary::new(&aff, &dic)
                .map_err(|e| AppError::BadDictionary(format!("{}: {e}", path.display())))?;
            for word in personal_words(config_dir) { let _ = dict.add(&word); }
            self.loaded = Some((choice, dict));
        }
        Ok(&mut self.loaded.as_mut().expect("just loaded").1)
    }

    /// Of `words`, those the dictionary doesn't know.
    pub fn misspelled(&mut self, prefs: &Prefs, config_dir: &Path, words: Vec<String>) -> Result<Vec<String>, AppError> {
        let dict = self.dict(prefs, config_dir)?;
        Ok(words.into_iter().filter(|w| !dict.check(&normalize(w))).collect())
    }

    /// Words the dictionary knows that `word` may have been meant as.
    pub fn suggest(&mut self, prefs: &Prefs, config_dir: &Path, word: &str) -> Result<Vec<String>, AppError> {
        let dict = self.dict(prefs, config_dir)?;
        let mut out = Vec::new();
        dict.suggest(&normalize(word), &mut out);
        out.truncate(MAX_SUGGESTIONS);
        Ok(out)
    }

    /// Add `word` to the user's words: the dictionary knows it from now on.
    pub fn add(&mut self, prefs: &Prefs, config_dir: &Path, word: &str) -> Result<(), AppError> {
        let word = normalize(word.trim());
        if word.is_empty() || word.contains(char::is_whitespace) { return Ok(()); }
        let mut words = personal_words(config_dir);
        if !words.contains(&word) {
            words.push(word.clone());
            std::fs::create_dir_all(config_dir)?;
            std::fs::write(config_dir.join(PERSONAL), words.join("\n") + "\n")?;
        }
        if let Ok(dict) = self.dict(prefs, config_dir) { let _ = dict.add(&word); }
        Ok(())
    }
}

/// The user's words, from the config folder.
fn personal_words(config_dir: &Path) -> Vec<String> {
    std::fs::read_to_string(config_dir.join(PERSONAL)).unwrap_or_default()
        .lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect()
}

/// The typographic apostrophe as dictionaries write it.
fn normalize(word: &str) -> String {
    word.replace(['’', 'ʼ'], "'")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_interface_language_picks_its_main_dictionary() {
        let names: Vec<String> = ["en_GB", "en_US", "fr", "fr_BE", "fr_FR"].map(String::from).into();
        assert_eq!(pick(&names, "", "fr").map(String::as_str), Some("fr_FR"));
        assert_eq!(pick(&names, "", "en").map(String::as_str), Some("en_GB"));
        assert_eq!(pick(&names, "fr_BE", "en").map(String::as_str), Some("fr_BE"));
        assert_eq!(pick(&names, "de_DE", "fr"), None);
        let only_fr: Vec<String> = vec!["fr".into()];
        assert_eq!(pick(&only_fr, "", "fr").map(String::as_str), Some("fr"));
    }

    #[test]
    fn dictionaries_are_found_once_by_name() {
        let tmp = std::env::temp_dir().join(format!("orgamnesia-spell-{}", std::process::id()));
        let (a, b) = (tmp.join("a"), tmp.join("b"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        for dir in [&a, &b] {
            std::fs::write(dir.join("xx_XX.aff"), "SET UTF-8\n").unwrap();
            std::fs::write(dir.join("xx_XX.dic"), "1\nmot\n").unwrap();
        }
        // A .dic without its .aff is no dictionary
        std::fs::write(b.join("yy.dic"), "1\nmot\n").unwrap();
        let found = find_dicts(&[a.clone(), b.clone()]);
        assert_eq!(found, vec![("xx_XX".to_string(), a.join("xx_XX"))]);
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn words_are_checked_and_added() {
        let tmp = std::env::temp_dir().join(format!("orgamnesia-spell-add-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("xx_XX.aff"), "SET UTF-8\nTRY abcdefghijklmnopqrstuvwxyz\n").unwrap();
        std::fs::write(tmp.join("xx_XX.dic"), "2\nmaison\nl'homme\n").unwrap();
        let prefs = Prefs { spell_dict_dir: tmp.display().to_string(), spell_lang: "xx_XX".into(), ..Prefs::default() };
        let config = tmp.join("config");
        let mut s = Speller::default();
        let words = |w: &[&str]| w.iter().map(|w| w.to_string()).collect::<Vec<_>>();
        assert_eq!(s.misspelled(&prefs, &config, words(&["maison", "Maison", "l’homme", "maisonn"])).unwrap(), ["maisonn"]);
        assert!(s.suggest(&prefs, &config, "maisonn").unwrap().contains(&"maison".to_string()));
        s.add(&prefs, &config, "orgamnesia").unwrap();
        assert!(s.misspelled(&prefs, &config, words(&["orgamnesia"])).unwrap().is_empty());
        // Kept for next time
        assert!(Speller::default().misspelled(&prefs, &config, words(&["orgamnesia"])).unwrap().is_empty());
        let other = Prefs { spell_lang: "zz".into(), ..prefs };
        assert!(matches!(s.misspelled(&other, &config, vec![]), Err(AppError::NoDictionary(_))));
        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
