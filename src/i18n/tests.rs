use super::*;

/// The Rust sources under `dir` but the dictionary (`src/i18n.rs`, `src/i18n/`).
fn sources(dir: &std::path::Path, out: &mut String) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.ends_with("src/i18n") || path.ends_with("src/i18n.rs") { continue; }
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push_str(&std::fs::read_to_string(&path).unwrap());
        }
    }
}

/// Every key the sources use has a text in both languages.
#[test]
fn every_key_is_translated() {
    let mut code = String::new();
    sources(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut code);
    let sources = [code.as_str()];
    let mut keys: Vec<String> = Vec::new();
    for src in sources {
        for pat in ["t(\"", "label=\"", "hint=\"", "error(\"", "notify_t(\""] {
            for (at, _) in src.match_indices(pat) {
                // `pat` must start a word: not `expect("…")`
                let before = src[..at].chars().next_back();
                if before.is_some_and(|c| c.is_alphanumeric() || c == '_') { continue; }
                let part = &src[at + pat.len()..];
                let key: String = part.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                if !key.is_empty() && part[key.len()..].starts_with('"') { keys.push(key); }
            }
        }
    }
    let registry = crate::keybindings::APP_ACTIONS.iter().map(|a| a.label_key)
        .chain(crate::keybindings::EDITOR_ACTIONS.iter().map(|a| a.label_key));
    keys.extend(registry.map(String::from));
    assert!(keys.len() > 100);
    let missing: Vec<&String> = keys.iter()
        .filter(|k| [Lang::En, Lang::Fr].iter().any(|&l| {
            let k: &'static str = Box::leak(k.to_string().into_boxed_str());
            t(k, l) == k
        }))
        .collect();
    assert!(missing.is_empty(), "untranslated: {missing:?}");
}

/// Every key of the dictionary is used somewhere: a text no one shows is dead.
#[test]
fn every_key_is_used() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut code = String::new();
    sources(&root.join("src"), &mut code);
    sources(&root.join("core/src"), &mut code);
    let dictionary = [
        include_str!("panels.rs"), include_str!("settings.rs"), include_str!("editing.rs"),
        include_str!("git.rs"), include_str!("dialogs.rs"), include_str!("status.rs"),
    ].concat();
    let mut keys: Vec<&str> = dictionary.lines()
        .filter_map(|l| l.trim_start().strip_prefix("(\""))
        .filter_map(|l| l.split_once('"').filter(|(_, rest)| rest.trim_start().starts_with(", Lang::")).map(|(k, _)| k))
        .collect();
    keys.dedup();
    assert!(keys.len() > 100);
    let unused: Vec<&&str> = keys.iter().filter(|k| !code.contains(&format!("\"{k}\""))).collect();
    assert!(unused.is_empty(), "unused: {unused:?}");
}
