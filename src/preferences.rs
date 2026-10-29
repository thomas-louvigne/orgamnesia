//! The "preference" project: the configuration files (`settings.json`,
//! `keybindings.json`) edited as text in the editor. It takes the place of the
//! open project, which waits aside, its tabs and panes as they were, to be come
//! back to.

use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

pub use orgamnesia_core::config::{check, Checked, File, InEffect, Problem, ProblemKind};

use crate::{
    actions,
    i18n::{t, Lang},
    invoke::{self, ConfigInvalid},
    keybindings,
    state::{AppCtx, FileEntry, Goto, Prefs, Settings, SplitKind, Tab, VaultChanges},
};

/// Name of the project, as the project menu shows it.
pub const NAME: &str = "preference";

/// Whether the app runs on Windows, where org-bloom is not offered.
pub fn on_windows() -> bool {
    web_sys::window().and_then(|w| w.navigator().platform().ok()).is_some_and(|p| p.starts_with("Win"))
}

/// The settings as the app uses them on this system: org-bloom off on Windows,
/// whatever `settings.json` says.
pub fn prefs_of(settings: &Settings) -> Prefs {
    let mut prefs = settings.prefs();
    prefs.org_bloom &= !on_windows();
    prefs
}

/// The open project, put aside while the "preference" project is shown.
#[derive(Clone, Debug)]
pub struct Aside {
    files: Vec<FileEntry>,
    tabs: Vec<Tab>,
    active_tab: Option<usize>,
    split: Option<SplitKind>,
    other_path: Option<String>,
    focus_second: bool,
    history_back: Vec<String>,
    history_forward: Vec<String>,
    /// Its pages changed on disk meanwhile, applied on coming back.
    changes: Vec<VaultChanges>,
}

/// The configuration file at `path`, when it is one (a page of the "preference" project).
pub fn config_file(path: &str) -> Option<File> {
    path.ends_with(".json").then(|| File::from_path(path)).flatten()
}

/// The names of the user's own keybinding profiles: the profile files of the project.
fn profile_names(files: &[FileEntry]) -> Vec<String> {
    files.iter().filter_map(|f| match config_file(&f.path) {
        Some(File::Profile(id)) => Some(id),
        _ => None,
    }).collect()
}

/// Whether the text of the file `file` is valid, as it changes.
pub fn checked(ctx: AppCtx, content: RwSignal<String>, file: File) -> Memo<Result<Checked, Problem>> {
    Memo::new(move |_| {
        let profiles = ctx.project.files.with(|f| profile_names(f));
        content.with(|c| check(&file, c, &profiles))
    })
}

/// Show the "preference" project, `file` open (on its line `line`, from 1).
pub fn open(ctx: AppCtx, file: File, line: Option<usize>) {
    spawn_local(async move {
        if !ctx.project.preference.get_untracked() {
            let files = match invoke::config_files().await {
                Ok(files) => files,
                Err(e) => return ctx.error("error", &e),
            };
            let work = ctx.work;
            ctx.project.aside.set_value(Some(Aside {
                files: ctx.project.files.get_untracked(),
                tabs: work.tabs.get_untracked(),
                active_tab: work.active_tab.get_untracked(),
                split: work.split.get_untracked(),
                other_path: work.other_path.get_untracked(),
                focus_second: work.focus_second.get_untracked(),
                history_back: work.history_back.get_untracked(),
                history_forward: work.history_forward.get_untracked(),
                changes: vec![],
            }));
            work.split.set(None);
            work.other_path.set(None);
            work.focus_second.set(false);
            work.tabs.set(vec![]);
            work.active_tab.set(None);
            work.history_back.set(vec![]);
            work.history_forward.set(vec![]);
            ctx.project.files.set(files);
            ctx.project.preference.set(true);
        }
        // A profile no longer there: the settings, which name the one in effect
        let entry = ctx.project.files.with_untracked(|fs| {
            let find = |file: &File| fs.iter().find(|f| config_file(&f.path).as_ref() == Some(file)).cloned();
            find(&file).or_else(|| find(&File::Settings))
        });
        if let Some(entry) = entry {
            actions::open_file(ctx, entry, line.map(|l| Goto::Line(l.saturating_sub(1))));
        }
    });
}

/// Come back from the "preference" project to the project put aside, then run
/// `then`. Asks first when a file has changes not applied: they would be lost.
pub fn leave(ctx: AppCtx, then: impl Fn() + Send + Sync + 'static) {
    if !ctx.project.preference.get_untracked() {
        then();
        return;
    }
    let back = move || {
        come_back(ctx);
        then();
    };
    if ctx.work.has_unsaved_tabs() {
        ctx.ask_confirm(t("pref_leave_unapplied", ctx.lang.get_untracked()).to_string(), back);
    } else {
        back();
    }
}

fn come_back(ctx: AppCtx) {
    let Some(aside) = ctx.project.aside.try_update_value(|a| a.take()).flatten() else { return };
    let work = ctx.work;
    ctx.project.preference.set(false);
    ctx.project.files.set(aside.files);
    work.tabs.set(aside.tabs);
    work.active_tab.set(aside.active_tab);
    work.other_path.set(aside.other_path);
    work.focus_second.set(aside.focus_second);
    work.split.set(aside.split);
    work.history_back.set(aside.history_back);
    work.history_forward.set(aside.history_forward);
    if !aside.changes.is_empty() {
        spawn_local(async move {
            for changes in aside.changes { actions::apply_disk_changes(ctx, changes).await; }
        });
    }
}

/// Name of the project put aside, to come back to.
pub fn aside_name(ctx: AppCtx) -> String {
    ctx.project.vault_path.get().map(|p| actions::project_name(&p))
        .unwrap_or_else(|| t("no_project", ctx.lang.get()).to_string())
}

/// Whether a page of the project put aside has unsaved changes.
pub fn aside_unsaved(ctx: AppCtx) -> bool {
    ctx.project.aside.with_value(|a| a.as_ref().is_some_and(|a| a.tabs.iter().any(|t| t.dirty.get_untracked())))
}

/// While the "preference" project is shown, the pages of the project put aside
/// changed on disk: kept to be applied on coming back. Returns them otherwise.
pub fn defer_disk_changes(ctx: AppCtx, changes: VaultChanges) -> Option<VaultChanges> {
    if !ctx.project.preference.get_untracked() { return Some(changes); }
    ctx.project.aside.update_value(|a| if let Some(a) = a { a.changes.push(changes) });
    None
}

/// The open tab showing `file`.
fn tab_of(ctx: AppCtx, file: &File) -> Option<Tab> {
    if !ctx.project.preference.get_untracked() { return None; }
    ctx.work.tabs.with_untracked(|tabs| tabs.iter().find(|t| config_file(&t.path).as_ref() == Some(file)).cloned())
}

/// Apply the configuration file of `tab`: written and put in effect, when valid.
pub fn apply(ctx: AppCtx, tab: Tab) {
    let Some(file) = config_file(&tab.path) else { return };
    let lang = ctx.lang.get_untracked();
    let content = tab.content.get_untracked();
    let profiles = ctx.project.files.with_untracked(|f| profile_names(f));
    if let Err(p) = check(&file, &content, &profiles) {
        ctx.notify(format!("{} {} : {}", t("pref_not_applied", lang), file.name(), describe(&p, lang)));
        return;
    }
    spawn_local(async move {
        match invoke::apply_config(&file, &content).await {
            Ok(effect) => {
                put_in_effect(ctx, effect);
                if tab.content.get_untracked() == content { tab.dirty.set(false); }
                ctx.notify_t("pref_applied", &file.name());
            }
            Err(e) => ctx.error("pref_apply_error", &e),
        }
    });
}

/// Put back in effect the version of `file` replaced at `version` (`None`: the
/// last valid one); its tab, when open, shows it.
pub fn restore(ctx: AppCtx, file: File, version: Option<u64>) {
    spawn_local(async move {
        match invoke::restore_config(&file, version).await {
            Ok(restored) => {
                put_in_effect(ctx, restored.effect);
                if let Some(tab) = tab_of(ctx, &file) {
                    tab.content.set(restored.content);
                    tab.dirty.set(false);
                }
                ctx.notify_t("pref_restored", &file.name());
            }
            Err(e) => ctx.error("pref_restore_error", &e),
        }
    });
}

/// Put the configuration `effect` in effect in the interface (what changed only:
/// the views depending on the settings are not drawn again for nothing).
fn put_in_effect(ctx: AppCtx, effect: InEffect) {
    let vault = ctx.prefs.with_untracked(|p| p.vault.clone());
    let prefs = Prefs { vault, ..prefs_of(&Settings { config: effect.settings, ..Settings::default() }) };
    if ctx.prefs.with_untracked(|p| *p != prefs) {
        ctx.prefs.set(prefs);
        ctx.bump_links();
    }
    let mut kb = effect.keybindings;
    kb.upgrade();
    if ctx.keybindings.with_untracked(|k| *k != kb) { ctx.keybindings.set(kb); }
}

/// Listen to the configuration files changed outside the app, and tell what is
/// wrong with those found wrong as the app started.
pub fn watch(ctx: AppCtx) {
    invoke::listen(invoke::CONFIG_CHANGED, move |c: invoke::ConfigChanged| {
        put_in_effect(ctx, c.effect);
        ctx.notify_t("pref_changed_outside", &c.file.name());
        // A profile may have been added or deleted
        refresh(ctx);
    });
    invoke::listen(invoke::CONFIG_INVALID, move |c: ConfigInvalid| tell_invalid(ctx, vec![c]));
    spawn_local(async move {
        if let Ok(list) = invoke::config_problems().await && !list.is_empty() {
            tell_invalid(ctx, list);
        }
    });
}

/// The configuration files changed (from the settings window, or outside the
/// app): the project lists them as they are (a profile added or deleted), and
/// their tabs show them.
pub fn refresh(ctx: AppCtx) {
    if !ctx.project.preference.get_untracked() { return; }
    spawn_local(async move {
        let Ok(files) = invoke::config_files().await else { return };
        // The project may have been left meanwhile
        if !ctx.project.preference.get_untracked() { return; }
        let gone: Vec<String> = ctx.work.tabs.with_untracked(|tabs| tabs.iter()
            .filter(|t| !files.iter().any(|f| f.path == t.path))
            .map(|t| t.path.clone())
            .collect());
        for path in gone { actions::close_tab_of(ctx, &path); }
        if ctx.project.files.get_untracked() != files { ctx.project.files.set(files); }
        for tab in ctx.work.tabs.get_untracked() {
            if let Some(file) = config_file(&tab.path) { reload(ctx, tab, file).await; }
        }
    });
}

/// `file`, open in `tab`, changed on disk: the tab shows it, unless it has changes
/// not applied (then which text to keep is asked).
async fn reload(ctx: AppCtx, tab: Tab, file: File) {
    let Ok(content) = invoke::read_config(&file).await else { return };
    if tab.content.try_get_untracked().is_none_or(|c| c == content) {
        tab.dirty.try_set(false);
    } else if tab.dirty.try_get_untracked() == Some(true) {
        ctx.ui.disk_conflicts.update(|q| if !q.contains(&tab.path) { q.push(tab.path.clone()) });
    } else {
        tab.content.try_set(content);
    }
}

/// Configuration files found wrong: the ones before stay in effect. Offers to
/// correct them in the "preference" project, or to put back the last valid ones.
fn tell_invalid(ctx: AppCtx, list: Vec<ConfigInvalid>) {
    for c in &list {
        if let Some(tab) = tab_of(ctx, &c.file) { spawn_local(reload(ctx, tab, c.file.clone())); }
    }
    let Some(first) = list.first().cloned() else { return };
    let lang = ctx.lang.get_untracked();
    let problems: Vec<String> = list.iter()
        .map(|c| format!("{} — {}", c.file.name(), describe(&c.problem, lang)))
        .collect();
    let message = format!("{}\n\n{}", t("pref_invalid_outside", lang), problems.join("\n"));
    let files: Vec<File> = list.iter().map(|c| c.file.clone()).collect();
    ctx.ask_choice(
        message,
        "pref_fix", move || open(ctx, first.file.clone(), Some(first.problem.line)),
        "pref_restore_valid", move || for f in &files { restore(ctx, f.clone(), None) },
    );
}

/// What is wrong with a configuration file, and where, in `lang`.
pub fn describe(p: &Problem, lang: Lang) -> String {
    let what = match &p.kind {
        ProblemKind::Syntax(m) => format!("{} {m}", t("cfg_syntax", lang)),
        ProblemKind::NotAnObject => t("cfg_not_object", lang).to_string(),
        ProblemKind::UnknownKey(k) => format!("{} « {k} »", t("cfg_unknown_key", lang)),
        ProblemKind::StateKey(k) => format!("« {k} » {}", t("cfg_state_key", lang)),
        ProblemKind::Conflict { combo, labels } => {
            let names: Vec<&str> = labels.iter().map(|l| action_label(l, lang)).collect();
            format!("{} {} ({})", t("cfg_conflict", lang), keybindings::display(combo), names.join(", "))
        }
        ProblemKind::UnknownProfile(id) => format!("{} « {id} »", t("cfg_unknown_profile", lang)),
        ProblemKind::UnknownAction(id) => format!("{} « {id} »", t("cfg_unknown_action", lang)),
        ProblemKind::UnknownPreset(id) => format!("{} « {id} »", t("cfg_unknown_preset", lang)),
        ProblemKind::BuiltinId(id) => format!("« {id} » {}", t("cfg_builtin_id", lang)),
    };
    format!("{} {}, {} {} : {what}", t("cfg_line", lang), p.line, t("cfg_column", lang), p.column)
}

/// The name of the action whose label key is `key`.
fn action_label(key: &str, lang: Lang) -> &'static str {
    keybindings::APP_ACTIONS.iter().map(|a| a.label_key)
        .chain(keybindings::EDITOR_ACTIONS.iter().map(|a| a.label_key))
        .find(|k| *k == key)
        .map_or("?", |k| t(k, lang))
}

/// Char index of `line`, `column` (from 1) in `chars`, kept within the line.
pub fn char_index(chars: &[char], line: usize, column: usize) -> usize {
    let mut start = 0;
    for _ in 1..line {
        match chars[start..].iter().position(|&c| c == '\n') {
            Some(i) => start += i + 1,
            None => return chars.len(),
        }
    }
    let end = chars[start..].iter().position(|&c| c == '\n').map_or(chars.len(), |i| start + i);
    (start + column.saturating_sub(1)).min(end)
}

/// The char range to mark for a problem at `line`, `column` of `text`: the
/// string or word there, else the one just before (the JSON reader points after
/// a value of the wrong type, or at the end of the text).
pub fn problem_range(text: &str, line: usize, column: usize) -> (usize, usize) {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() { return (0, 0); }
    let word = |c: char| c.is_alphanumeric() || matches!(c, '_' | '-' | '+' | '.');
    let token_at = |i: usize| chars.get(i).is_some_and(|&c| c == '"' || word(c));
    let at = char_index(&chars, line, column);
    // Right after a string or a word: that one
    if at > 0 && token_at(at - 1) && !token_at(at) {
        let end = at - 1;
        let start = if chars[end] == '"' {
            chars[..end].iter().rposition(|&c| c == '"' || c == '\n').map_or(0, |i| if chars[i] == '"' { i } else { i + 1 })
        } else {
            chars[..end].iter().rposition(|&c| !word(c)).map_or(0, |i| i + 1)
        };
        return (start, at);
    }
    match chars.get(at) {
        Some('"') => {
            let end = chars[at + 1..].iter().position(|&c| c == '"' || c == '\n')
                .map_or(chars.len(), |i| at + 1 + i + usize::from(chars[at + 1 + i] == '"'));
            (at, end)
        }
        Some(&c) if word(c) => (at, chars[at..].iter().position(|&c| !word(c)).map_or(chars.len(), |i| at + i)),
        Some(&c) if !c.is_whitespace() => (at, at + 1),
        // Nothing there: the last character before that is not a space
        _ => {
            let last = chars[..at].iter().rposition(|c| !c.is_whitespace()).unwrap_or(0);
            (last, last + 1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marked(text: &str, line: usize, column: usize) -> String {
        let (a, b) = problem_range(text, line, column);
        text.chars().skip(a).take(b - a).collect()
    }

    #[test]
    fn problems_mark_what_they_are_about() {
        // A key, a word
        assert_eq!(marked("{\n  \"autosav\": true\n}", 2, 3), "\"autosav\"");
        assert_eq!(marked("{\"a\": tru }", 1, 7), "tru");
        // After a value of the wrong type: that value
        assert_eq!(marked("{\"autosave\": \"yes\"}", 1, 19), "\"yes\"");
        assert_eq!(marked("{\"autosave\": \"yes\"\n}", 1, 19), "\"yes\"");
        // A missing comma: the string where one was expected
        assert_eq!(marked("{\n  \"a\": true\n  \"b\": 1\n}", 3, 3), "\"b\"");
        // The end of the text: its last character
        assert_eq!(marked("{\"a\": 1", 1, 8), "1");
        assert_eq!(char_index(&"ab\ncd".chars().collect::<Vec<_>>(), 2, 9), 5);
    }

    #[test]
    fn config_pages() {
        assert_eq!(config_file("/home/me/.config/x/settings.json"), Some(File::Settings));
        assert_eq!(config_file("/home/me/.config/x/keybindings/mine.json"), Some(File::Profile("mine".into())));
        assert_eq!(config_file("/notes/pages/settings.org"), None);
    }
}
