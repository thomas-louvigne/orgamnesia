//! What the app context does: the project, the tabs and the panels it opens,
//! reads and changes, for every component.

use super::*;

impl AppCtx {
    pub fn new() -> Self {
        let prefs = RwSignal::new(Prefs::default());
        let files = RwSignal::new(vec![]);
        Self {
            prefs,
            lang: Memo::new(move |_| prefs.with(|p| Lang::from_code(&p.lang))),
            keybindings: RwSignal::new(Keybindings::defaults()),
            spell_words: RwSignal::new(0),
            project: ProjectState {
                vault_path: RwSignal::new(None),
                projects: RwSignal::new(vec![]),
                files,
                folders: RwSignal::new(vec![]),
                resolver: Memo::new(move |_| {
                    let (loose, default) = prefs.with(|p| (p.case_insensitive_links, p.default_format));
                    orgamnesia_core::pages::Resolver::new(files.get(), loose, default)
                }),
                tags: RwSignal::new(vec![]),
                tag_query: RwSignal::new(String::new()),
                git: RwSignal::new(None),
                git_busy: RwSignal::new(None),
                git_sync_paused: RwSignal::new(false),
                bloom_busy: RwSignal::new(false),
                bloom_built: RwSignal::new(None),
                links_version: RwSignal::new(0),
                preference: RwSignal::new(false),
                aside: StoredValue::new(None),
            },
            work: Workspace {
                tabs: RwSignal::new(vec![]),
                active_tab: RwSignal::new(None),
                split: RwSignal::new(None),
                other_path: RwSignal::new(None),
                focus_second: RwSignal::new(false),
                capture: RwSignal::new(None),
                goto: RwSignal::new(None),
                kill_ring: RwSignal::new(String::new()),
                history_back: RwSignal::new(vec![]),
                history_forward: RwSignal::new(vec![]),
                history_arriving: RwSignal::new(None),
            },
            ui: Ui {
                show_projects: RwSignal::new(false),
                show_settings: RwSignal::new(false),
                git_passphrase: RwSignal::new(None),
                git_passphrase_sent: RwSignal::new(false),
                show_new_page: RwSignal::new(false),
                new_page_name: RwSignal::new(String::new()),
                show_welcome: RwSignal::new(false),
                show_quick_open: RwSignal::new(false),
                show_commit: RwSignal::new(false),
                show_graph: RwSignal::new(false),
                show_agenda: RwSignal::new(false),
                zen: RwSignal::new(false),
                git_history: RwSignal::new(None),
                show_git_remote: RwSignal::new(false),
                show_git_branch: RwSignal::new(false),
                show_clone: RwSignal::new(false),
                show_git_identity: RwSignal::new(false),
                update: RwSignal::new(None),
                new_version: RwSignal::new(None),
                show_shortcuts: RwSignal::new(false),
                rename_request: RwSignal::new(0),
                video: RwSignal::new(None),
                tag_add: RwSignal::new(false),
                disk_conflicts: RwSignal::new(vec![]),
                search: ProjectSearch {
                    open: RwSignal::new(None),
                    query: RwSignal::new(String::new()),
                    replacement: RwSignal::new(String::new()),
                    options: RwSignal::new(SearchOptions::default()),
                    results: RwSignal::new(SearchResults::default()),
                    skip: RwSignal::new(ReplaceSkip::default()),
                    can_undo: RwSignal::new(false),
                },
                confirm: RwSignal::new(None),
                prompt: RwSignal::new(None),
                status: RwSignal::new(None),
                status_undo: RwSignal::new(None),
                drag: RwSignal::new(None),
                dropped: RwSignal::new(None),
            },
        }
    }

    /// One setting, tracked (re-runs effects when the settings change).
    pub fn pref<T>(&self, f: impl FnOnce(&Prefs) -> T) -> T {
        self.prefs.with(f)
    }

    pub fn pref_untracked<T>(&self, f: impl FnOnce(&Prefs) -> T) -> T {
        self.prefs.with_untracked(f)
    }

    /// The settings in effect, with the open project as the project setting.
    pub fn prefs_untracked(&self) -> Prefs {
        Prefs {
            vault: self.project.vault_path.get_untracked().unwrap_or_default(),
            ..self.prefs.get_untracked()
        }
    }

    /// Tell the views derived from the pages' links (backlinks, tags…) to refresh.
    pub fn bump_links(&self) {
        self.project.links_version.update(|v| *v += 1);
    }

    /// The page `path` was written with `content`: its aliases, which links may
    /// name, are taken in, and the views derived from the links refresh.
    pub fn page_written(&self, path: &str, content: &str) {
        let aliases = orgamnesia_core::aliases::aliases(content, orgamnesia_core::Format::of_page(path));
        let stale = self.project.files.with_untracked(|fs| fs.iter().any(|f| f.path == path && f.aliases != aliases));
        if stale {
            self.project.files.update(|fs| for f in fs.iter_mut().filter(|f| f.path == path) { f.aliases = aliases.clone(); });
        }
        self.bump_links();
    }

    /// The page called `name`, per the setting ignoring case and accents: `Note.md`
    /// that file, `Note` the page of that name (of the default format when there
    /// are two, see `orgamnesia_core::pages`).
    pub fn find_page(&self, name: &str) -> Option<FileEntry> {
        self.project.resolver.with_untracked(|r| r.resolve(name).cloned())
    }

    /// The names links give the pages, to complete them: their name, with its
    /// extension for the one its name alone doesn't lead to (`Note.md` next to
    /// `Note.org`), and their aliases.
    pub fn link_names(&self) -> Vec<String> {
        self.project.resolver.with_untracked(|r| r.link_names())
    }

    /// Warn when the page `file` has a twin, a page of the same name in the other
    /// format: `[[Name]]` leads to only one of them.
    pub fn warn_twin(&self, file: &FileEntry) {
        let twin = self.project.resolver.with_untracked(|r| r.has_twin(file));
        if twin { self.notify_t("twin_warning", &file.name); }
    }

    /// Show the pages and headlines tagged `tag` in the tags frame.
    pub fn show_tag(&self, tag: String) {
        self.project.tag_query.set(tag);
    }

    /// How `#tags` and `:tags:` are read, from the settings (tracked: re-runs effects
    /// when they change).
    pub fn tags(&self) -> orgamnesia_core::TagSyntax {
        self.pref(|p| p.tags())
    }

    pub fn tags_untracked(&self) -> orgamnesia_core::TagSyntax {
        self.pref_untracked(|p| p.tags())
    }

    /// Split the editor in two panes showing the current page (Emacs `C-x 2` / `C-x 3`).
    /// Splitting again only changes the direction.
    pub fn split_window(&self, kind: SplitKind) {
        let work = self.work;
        if work.split.get_untracked().is_none() {
            let Some(tab) = work.active_tab_data() else {
                self.notify_t("split_needs_page", "");
                return;
            };
            work.other_path.set(Some(tab.path));
            work.focus_second.set(false);
        }
        work.split.set(Some(kind));
    }

    /// Ask the user to confirm; `on_yes` runs only if they accept.
    pub fn close_panels(&self) {
        self.ui.show_settings.set(false);
        self.ui.show_commit.set(false);
        self.ui.show_graph.set(false);
        self.ui.show_agenda.set(false);
        self.ui.git_history.set(None);
        self.ui.show_clone.set(false);
        self.ui.show_git_identity.set(false);
    }

    pub fn ask_confirm(&self, message: String, on_yes: impl Fn() + Send + Sync + 'static) {
        self.ui.confirm.set(Some(ConfirmReq { message, on_yes: Callback::new(move |_| on_yes()), danger: None, info: false, choice: None }));
    }

    /// Ask a value (`value` typed already, `choices` offered); `on_ok` gets it, trimmed, when not empty.
    pub fn ask_text(&self, label: String, value: String, choices: Vec<String>, on_ok: impl Fn(String) + Send + Sync + 'static) {
        self.ui.prompt.set(Some(PromptReq { label, value, choices, on_ok: Callback::new(on_ok) }));
    }

    /// Show a message in a dialog the user closes with OK.
    pub fn alert(&self, message: String) {
        self.ui.confirm.set(Some(ConfirmReq { message, on_yes: Callback::new(|_| ()), danger: None, info: true, choice: None }));
    }

    /// Ask the user to confirm a destructive action, done by the button `button` (label key).
    pub fn ask_confirm_danger(&self, message: String, button: &'static str, on_yes: impl Fn() + Send + Sync + 'static) {
        self.ui.confirm.set(Some(ConfirmReq { message, on_yes: Callback::new(move |_| on_yes()), danger: Some(button), info: false, choice: None }));
    }

    /// Ask the user to choose between two actions: the buttons `no` (`on_no`) and
    /// `yes` (`on_yes`), by their label keys. Closing the dialog does neither.
    pub fn ask_choice(
        &self, message: String,
        no: &'static str, on_no: impl Fn() + Send + Sync + 'static,
        yes: &'static str, on_yes: impl Fn() + Send + Sync + 'static,
    ) {
        self.ui.confirm.set(Some(ConfirmReq {
            message, on_yes: Callback::new(move |_| on_yes()), danger: None, info: false,
            choice: Some((no, Callback::new(move |_| on_no()), yes)),
        }));
    }

    /// The shortcut of the action `id` in the active profile, shown ("Ctrl+W"), if it has one.
    pub fn shortcut(&self, id: &str) -> Option<String> {
        self.keybindings.with(|kb| crate::keybindings::first_shortcut(kb, id))
    }

    /// The text `label_key`, followed by the shortcut of the action `id` if it has
    /// one: "Close tab (Ctrl+W)", for a tooltip.
    pub fn with_shortcut(&self, label_key: &'static str, id: &str) -> String {
        let label = t(label_key, self.lang.get());
        match self.shortcut(id) {
            Some(k) => format!("{label} ({k})"),
            None => label.to_string(),
        }
    }

    /// Show a message in the status bar.
    pub fn notify(&self, msg: impl Into<String>) {
        self.ui.status.set(Some(msg.into()));
    }

    /// Show a message in the status bar with an "Undo" button running `undo`.
    pub fn notify_undo(&self, msg: impl Into<String>, undo: impl Fn() + Send + Sync + 'static) {
        let msg = msg.into();
        self.ui.status_undo.set(Some((msg.clone(), Callback::new(move |_| undo()))));
        self.notify(msg);
    }

    /// Show the text of `key`, followed by `detail` (a page name…).
    pub fn notify_t(&self, key: &'static str, detail: &str) {
        let label = t(key, self.lang.get_untracked());
        self.notify(if detail.is_empty() { label.to_string() } else { format!("{label} {detail}") });
    }

    /// Show the failure `key`, with the error message.
    pub fn error(&self, key: &'static str, err: &str) {
        self.notify(format!("{} : {err}", t(key, self.lang.get_untracked())));
    }

    /// Show the keys of a chord in progress ("⌨ Ctrl+X …") in the status bar.
    pub fn set_chord_status(&self, keys: &str) {
        self.notify(format!("{CHORD_MARK}{keys} …"));
    }

    /// Remove the chord-in-progress message, if that is what the status bar shows.
    pub fn clear_chord_status(&self) {
        if self.ui.status.with_untracked(|s| s.as_deref().is_some_and(|s| s.starts_with(CHORD_MARK))) {
            self.ui.status.set(None);
        }
    }
}

impl Default for AppCtx {
    fn default() -> Self { Self::new() }
}
