//! The settings as the app uses them (`Prefs`): read from `Settings`, their
//! defaults applied, and what follows from them.

use super::*;

/// The settings as the app uses them, defaults applied.
#[derive(Debug, Clone, PartialEq)]
pub struct Prefs {
    pub vault: String,
    /// Command running org-bloom; empty: `org-bloom`.
    pub org_bloom_path: String,
    /// Interface language code (`fr`, `en`).
    pub lang: String,
    /// See `Config::theme`.
    pub theme: String,
    /// See `Config::default_format`.
    pub default_format: Format,
    pub single_format: bool,
    pub ask_format: bool,
    pub org_links: LinkSyntax,
    pub md_links: LinkSyntax,
    pub update_links: bool,
    pub autosave: bool,
    pub delete_empty: bool,
    pub delete_title_only: bool,
    pub trash_auto_empty: bool,
    pub trash_keep_days: u64,
    pub trash_max_bytes: u64,
    pub restore_session: bool,
    pub show_brand: bool,
    pub show_page_title: bool,
    pub show_page_nav: bool,
    pub show_quit_button: bool,
    pub show_line_numbers: bool,
    pub show_pages: bool,
    pub show_backlinks: bool,
    pub show_tags: bool,
    pub show_broken_links: bool,
    pub show_tips: bool,
    pub show_todos: bool,
    pub show_project: bool,
    pub show_git: bool,
    pub show_settings_button: bool,
    /// Where the frames are, shown or not.
    pub layout: Layout,
    pub org_bloom: bool,
    pub show_org_bloom: bool,
    pub org_bloom_zip: bool,
    pub org_bloom_no_external_deps: bool,
    pub git_ext: bool,
    pub graph: bool,
    pub show_graph: bool,
    pub journal: bool,
    /// See `Config::journal_folder`.
    pub journal_folder: String,
    /// See `Config::templates_folder`.
    pub templates_folder: String,
    pub show_excalidraw: bool,
    pub git_ssh_key: String,
    /// See `Settings::git_auto_sync_minutes`.
    pub git_auto_sync: u64,
    pub git_sync_on_quit: bool,
    /// See `Settings::git_fetch_minutes`.
    pub git_fetch: u64,
    pub git_pull_rebase: bool,
    pub git_unpushed_marker: bool,
    pub git_commit_push: bool,
    pub git_commit_template: String,
    pub check_updates: bool,
    /// See `Settings::skipped_update`; empty: none.
    pub skipped_update: String,
    pub spell: bool,
    /// See `Settings::spell_language`; empty: the interface language's.
    pub spell_lang: String,
    pub spell_dict_dir: String,
    pub case_insensitive_links: bool,
    pub click_follows_links: bool,
    pub hashtag_links: bool,
    pub hashtag_dashes: bool,
    pub org_tag_links: bool,
    pub org_tag_dashes: bool,
    pub find_match_case: bool,
    pub indent_headings: bool,
    pub hide_leading_stars: bool,
    pub live_preview: bool,
    pub ligatures: bool,
    pub link_text_first: bool,
    pub tab_folds: bool,
    pub todo_enabled: bool,
    pub todo_keywords: String,
    pub show_images: bool,
    pub show_video_thumbnails: bool,
    pub embed_videos: bool,
    pub mark_broken_links: bool,
    /// `org` or `logseq`: see `Settings::image_size_format`.
    pub image_size_format: String,
    pub link_preview: bool,
    /// `ctrl` or `hover`: see `Config::link_preview_trigger`.
    pub link_preview_trigger: String,
    pub emacs_mark: bool,
    pub webkit_emoji_shortcuts: bool,
    pub shortcut_tips: bool,
    pub electric_mode: bool,
    pub electric_emphasis: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self::from_settings(&Settings::default())
    }
}

impl Prefs {
    pub fn from_settings(settings: &Settings) -> Self {
        let s = &settings.config;
        Self {
            vault: settings.vault_path.clone().unwrap_or_default(),
            org_bloom_path: s.org_bloom_path.clone().unwrap_or_default(),
            lang: s.language.clone().unwrap_or_else(|| "fr".to_string()),
            theme: s.theme.clone().unwrap_or_else(|| "auto".to_string()),
            default_format: s.default_format.as_deref().and_then(Format::from_id).unwrap_or_default(),
            single_format: s.single_format.unwrap_or(false),
            ask_format: s.ask_format.unwrap_or(false),
            org_links: s.org_links.unwrap_or(LinkSyntax::NONE),
            md_links: s.md_links.unwrap_or(LinkSyntax::ALL),
            update_links: s.update_links_on_rename.unwrap_or(true),
            autosave: s.autosave.unwrap_or(true),
            delete_empty: s.delete_empty_pages.unwrap_or(false),
            delete_title_only: s.delete_title_only_pages.unwrap_or(false),
            trash_auto_empty: s.trash_auto_empty.unwrap_or(false),
            trash_keep_days: s.trash_keep_days.unwrap_or(30),
            trash_max_bytes: s.trash_max_bytes.unwrap_or(10_000_000),
            restore_session: s.restore_session.unwrap_or(true),
            show_brand: s.show_brand.unwrap_or(true),
            show_page_title: s.show_page_title.unwrap_or(true),
            show_page_nav: s.show_page_nav.unwrap_or(true),
            show_quit_button: s.show_quit_button.unwrap_or(true),
            show_line_numbers: s.show_line_numbers.unwrap_or(false),
            show_pages: s.show_pages.unwrap_or(true),
            show_backlinks: s.show_backlinks.unwrap_or(true),
            show_tags: s.show_tags.unwrap_or(true),
            show_broken_links: s.show_broken_links.unwrap_or(true),
            show_tips: s.show_tips.unwrap_or(true),
            show_todos: s.show_todos.unwrap_or(true),
            show_project: s.show_project.unwrap_or(true),
            show_git: s.show_git.unwrap_or(true),
            show_settings_button: s.show_settings_button.unwrap_or(true),
            layout: Layout::from_ids(s.frames_left.as_deref(), s.frames_right.as_deref()),
            org_bloom: s.org_bloom_enabled.unwrap_or(false),
            show_org_bloom: s.show_org_bloom.unwrap_or(true),
            org_bloom_zip: s.org_bloom_zip.unwrap_or(false),
            org_bloom_no_external_deps: s.org_bloom_no_external_deps.unwrap_or(false),
            git_ext: s.git_status_enabled.unwrap_or(true),
            graph: s.graph_enabled.unwrap_or(false),
            show_graph: s.show_graph.unwrap_or(true),
            journal: s.journal_enabled.unwrap_or(false),
            journal_folder: s.journal_folder.clone().unwrap_or_else(|| "journals".to_string()),
            templates_folder: s.templates_folder.clone().filter(|f| !f.trim().is_empty()).unwrap_or_else(|| "templates".to_string()),
            show_excalidraw: s.show_excalidraw.unwrap_or(true),
            git_ssh_key: s.git_ssh_key.clone().unwrap_or_default(),
            git_auto_sync: s.git_auto_sync_minutes.unwrap_or(0),
            git_sync_on_quit: s.git_sync_on_quit.unwrap_or(false),
            git_fetch: s.git_fetch_minutes.unwrap_or(5),
            git_pull_rebase: s.git_pull_rebase.unwrap_or(true),
            git_unpushed_marker: s.git_unpushed_marker.unwrap_or(true),
            git_commit_push: s.git_commit_push.unwrap_or(false),
            git_commit_template: s.git_commit_template.clone().unwrap_or_default(),
            check_updates: s.check_updates.unwrap_or(true),
            skipped_update: s.skipped_update.clone().unwrap_or_default(),
            spell: s.spell_enabled.unwrap_or(true),
            spell_lang: s.spell_language.clone().unwrap_or_default(),
            spell_dict_dir: s.spell_dict_dir.clone().unwrap_or_default(),
            case_insensitive_links: s.case_insensitive_links.unwrap_or(true),
            click_follows_links: s.click_follows_links.unwrap_or(false),
            hashtag_links: s.hashtag_links.unwrap_or(true),
            hashtag_dashes: s.hashtag_dashes.unwrap_or(true),
            org_tag_links: s.org_tag_links.unwrap_or(true),
            org_tag_dashes: s.org_tag_dashes.unwrap_or(true),
            find_match_case: s.find_match_case.unwrap_or(false),
            indent_headings: s.indent_headings.unwrap_or(true),
            hide_leading_stars: s.hide_leading_stars.unwrap_or(false),
            live_preview: s.live_preview.unwrap_or(false),
            ligatures: s.ligatures.unwrap_or(true),
            link_text_first: s.link_text_first.unwrap_or(false),
            tab_folds: s.tab_folds.unwrap_or(true),
            todo_enabled: s.todo_enabled.unwrap_or(true),
            todo_keywords: s.todo_keywords.clone().filter(|k| !k.trim().is_empty())
                .unwrap_or_else(|| DEFAULT_TODO_KEYWORDS.to_string()),
            show_images: s.show_images.unwrap_or(true),
            show_video_thumbnails: s.show_video_thumbnails.unwrap_or(true),
            embed_videos: s.embed_videos.unwrap_or(true),
            mark_broken_links: s.mark_broken_links.unwrap_or(true),
            image_size_format: s.image_size_format.clone().filter(|f| f == "logseq")
                .unwrap_or_else(|| "org".to_string()),
            link_preview: s.link_preview.unwrap_or(true),
            link_preview_trigger: s.link_preview_trigger.clone().filter(|t| t == "hover")
                .unwrap_or_else(|| "ctrl".to_string()),
            emacs_mark: s.emacs_mark.unwrap_or(true),
            webkit_emoji_shortcuts: s.webkit_emoji_shortcuts.unwrap_or(true),
            shortcut_tips: s.shortcut_tips.unwrap_or(true),
            electric_mode: s.electric_mode.unwrap_or(true),
            electric_emphasis: s.electric_emphasis.unwrap_or(true),
        }
    }

    pub fn to_settings(&self, projects: Vec<String>) -> Settings {
        let some = |s: &str| (!s.is_empty()).then(|| s.to_string());
        let config = Config {
            org_bloom_path: some(&self.org_bloom_path),
            language: Some(self.lang.clone()),
            default_format: Some(self.default_format.id().to_string()),
            single_format: Some(self.single_format),
            ask_format: Some(self.ask_format),
            org_links: Some(self.org_links),
            md_links: Some(self.md_links),
            update_links_on_rename: Some(self.update_links),
            case_insensitive_links: Some(self.case_insensitive_links),
            click_follows_links: Some(self.click_follows_links),
            hashtag_links: Some(self.hashtag_links),
            hashtag_dashes: Some(self.hashtag_dashes),
            org_tag_links: Some(self.org_tag_links),
            org_tag_dashes: Some(self.org_tag_dashes),
            find_match_case: Some(self.find_match_case),
            indent_headings: Some(self.indent_headings),
            hide_leading_stars: Some(self.hide_leading_stars),
            live_preview: Some(self.live_preview),
            ligatures: Some(self.ligatures),
            link_text_first: Some(self.link_text_first),
            tab_folds: Some(self.tab_folds),
            todo_enabled: Some(self.todo_enabled),
            todo_keywords: Some(self.todo_keywords.clone()),
            show_images: Some(self.show_images),
            show_video_thumbnails: Some(self.show_video_thumbnails),
            embed_videos: Some(self.embed_videos),
            mark_broken_links: Some(self.mark_broken_links),
            image_size_format: Some(self.image_size_format.clone()),
            link_preview: Some(self.link_preview),
            link_preview_trigger: Some(self.link_preview_trigger.clone()),
            emacs_mark: Some(self.emacs_mark),
            webkit_emoji_shortcuts: Some(self.webkit_emoji_shortcuts),
            shortcut_tips: Some(self.shortcut_tips),
            electric_mode: Some(self.electric_mode),
            electric_emphasis: Some(self.electric_emphasis),
            autosave: Some(self.autosave),
            delete_empty_pages: Some(self.delete_empty),
            delete_title_only_pages: Some(self.delete_title_only),
            trash_auto_empty: Some(self.trash_auto_empty),
            trash_keep_days: Some(self.trash_keep_days),
            trash_max_bytes: Some(self.trash_max_bytes),
            restore_session: Some(self.restore_session),
            show_brand: Some(self.show_brand),
            show_page_title: Some(self.show_page_title),
            show_page_nav: Some(self.show_page_nav),
            theme: Some(self.theme.clone()),
            show_quit_button: Some(self.show_quit_button),
            show_line_numbers: Some(self.show_line_numbers),
            show_pages: Some(self.show_pages),
            show_backlinks: Some(self.show_backlinks),
            show_tags: Some(self.show_tags),
            show_broken_links: Some(self.show_broken_links),
            show_tips: Some(self.show_tips),
            show_todos: Some(self.show_todos),
            show_project: Some(self.show_project),
            show_git: Some(self.show_git),
            show_settings_button: Some(self.show_settings_button),
            frames_left: Some(Layout::ids(&self.layout.left)),
            frames_right: Some(Layout::ids(&self.layout.right)),
            org_bloom_enabled: Some(self.org_bloom),
            show_org_bloom: Some(self.show_org_bloom),
            org_bloom_zip: Some(self.org_bloom_zip),
            org_bloom_no_external_deps: Some(self.org_bloom_no_external_deps),
            git_status_enabled: Some(self.git_ext),
            graph_enabled: Some(self.graph),
            show_graph: Some(self.show_graph),
            journal_enabled: Some(self.journal),
            journal_folder: Some(self.journal_folder.clone()),
            templates_folder: Some(self.templates_folder.clone()),
            show_excalidraw: Some(self.show_excalidraw),
            git_ssh_key: some(&self.git_ssh_key),
            git_auto_sync_minutes: Some(self.git_auto_sync),
            git_sync_on_quit: Some(self.git_sync_on_quit),
            git_fetch_minutes: Some(self.git_fetch),
            git_pull_rebase: Some(self.git_pull_rebase),
            git_unpushed_marker: Some(self.git_unpushed_marker),
            git_commit_push: Some(self.git_commit_push),
            git_commit_template: some(&self.git_commit_template),
            check_updates: Some(self.check_updates),
            skipped_update: some(&self.skipped_update),
            spell_enabled: Some(self.spell),
            spell_language: some(&self.spell_lang),
            spell_dict_dir: some(&self.spell_dict_dir),
            // Saved with the keybindings
            keybinding_profile: None,
        };
        Settings { config, vault_path: some(&self.vault), projects, session: None, assets_declined: vec![], keep_json: false, tui_left_width: None, tui_right_width: None, first_run: false }
    }

    /// How `#tags` and `:tags:` are read.
    pub fn tags(&self) -> TagSyntax {
        TagSyntax {
            hashtags: Hashtags::new(self.hashtag_links, self.hashtag_dashes),
            org: OrgTags { links: self.org_tag_links, dashes: self.org_tag_dashes },
            org_links: self.org_links,
            md_links: self.md_links,
        }
    }

    /// Which files are pages, and which one a name leads to.
    pub fn formats(&self) -> Formats {
        Formats { default: self.default_format, single: self.single_format }
    }

    /// Whether `frame` is shown: its own setting, and the feature it belongs to.
    pub fn frame_shown(&self, frame: Frame) -> bool {
        self.frame_on(frame) && match frame {
            Frame::Git => self.git_ext,
            Frame::Todos => self.todo_enabled,
            Frame::OrgBloom => self.org_bloom,
            Frame::Graph => self.graph,
            _ => true,
        }
    }

    /// The setting of `frame` alone (not the feature it belongs to).
    pub fn frame_on(&self, frame: Frame) -> bool {
        match frame {
            Frame::Brand => self.show_brand,
            Frame::Project => self.show_project,
            Frame::Git => self.show_git,
            Frame::Pages => self.show_pages,
            Frame::Settings => self.show_settings_button,
            Frame::Backlinks => self.show_backlinks,
            Frame::Tags => self.show_tags,
            Frame::Todos => self.show_todos,
            Frame::Broken => self.show_broken_links,
            Frame::Tips => self.show_tips,
            Frame::Quit => self.show_quit_button,
            Frame::OrgBloom => self.show_org_bloom,
            Frame::Graph => self.show_graph,
        }
    }

    /// The setting of `frame`, to change it.
    pub fn frame_setting(&mut self, frame: Frame) -> &mut bool {
        match frame {
            Frame::Brand => &mut self.show_brand,
            Frame::Project => &mut self.show_project,
            Frame::Git => &mut self.show_git,
            Frame::Pages => &mut self.show_pages,
            Frame::Settings => &mut self.show_settings_button,
            Frame::Backlinks => &mut self.show_backlinks,
            Frame::Tags => &mut self.show_tags,
            Frame::Todos => &mut self.show_todos,
            Frame::Broken => &mut self.show_broken_links,
            Frame::Tips => &mut self.show_tips,
            Frame::Quit => &mut self.show_quit_button,
            Frame::OrgBloom => &mut self.show_org_bloom,
            Frame::Graph => &mut self.show_graph,
        }
    }

    /// The frames shown in a column (true: the right one), top to bottom.
    pub fn column(&self, right: bool) -> Vec<Frame> {
        self.layout.column(right).iter().copied().filter(|f| self.frame_shown(*f)).collect()
    }

    /// The TODO keywords of the settings; `None` when they are off.
    pub fn todo(&self) -> Option<TodoKeywords> {
        self.todo_enabled.then(|| TodoKeywords::parse(&self.todo_keywords))
    }
}
