//! The settings: `Settings` as stored in `settings.json`, `Prefs` as used.

use serde::{Deserialize, Serialize};

use crate::{Hashtags, OrgTags, TagSyntax, TodoKeywords};

/// The TODO keywords when none are set (Emacs style: done states after the `|`).
pub const DEFAULT_TODO_KEYWORDS: &str = "TODO DOING HANGUP | DONE";

/// `settings.json`. A missing value (older file, or never set) means the
/// default: see `Prefs`, the only place defaults are decided.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Settings {
    pub vault_path: Option<String>,
    pub logseq_site_builder_path: Option<String>,
    pub language: Option<String>,
    /// Rewrite `[[links]]` in other pages when a page is renamed.
    pub update_links_on_rename: Option<bool>,
    /// Match `[[links]]` to pages ignoring case.
    pub case_insensitive_links: Option<bool>,
    /// Treat `#tags` as links to pages.
    pub hashtag_links: Option<bool>,
    /// Allow `-` in `#tags` (`#mon-tag`); org-mode tags don't.
    pub hashtag_dashes: Option<bool>,
    /// Treat org-mode `:tags:` as links to pages, also when written in the text.
    pub org_tag_links: Option<bool>,
    /// Allow `-` in `:tags:` (`:mon-tag:`); org-mode doesn't.
    pub org_tag_dashes: Option<bool>,
    /// The search in the page matches case (and accents) when it opens.
    pub find_match_case: Option<bool>,
    /// Indent the text under a headline to the column of its title (on screen only).
    pub indent_headings: Option<bool>,
    /// Tab on a headline folds / unfolds it (org-mode cycle).
    pub tab_folds: Option<bool>,
    /// TODO keywords on headlines (`* TODO Titre`), stepped through with a shortcut.
    pub todo_enabled: Option<bool>,
    /// The TODO keywords, as in Emacs: `TODO DOING | DONE`.
    pub todo_keywords: Option<String>,
    /// Emacs mark: Ctrl+Space starts a region that follows the cursor.
    pub emacs_mark: Option<bool>,
    /// Electric mode: typing a bracket or quote around a selection wraps it.
    pub electric_mode: Option<bool>,
    /// Save the active page on every change.
    pub autosave: Option<bool>,
    /// On quit, delete pages with no text: only spaces, tabs or `*`.
    pub delete_empty_pages: Option<bool>,
    /// On quit, delete pages holding only their `* Title` heading.
    pub delete_title_only_pages: Option<bool>,
    /// On opening a project, empty its trash of the pages deleted for too long, or
    /// beyond its size (see the two below).
    pub trash_auto_empty: Option<bool>,
    /// Days a deleted page stays in the trash (0: no limit).
    pub trash_keep_days: Option<u64>,
    /// Size of the trash in bytes beyond which its oldest pages go (0: no limit).
    pub trash_max_bytes: Option<u64>,
    /// On start, show again the pages and panes open when the app was quit.
    pub restore_session: Option<bool>,
    /// App name and logo at the top left of the window.
    pub show_brand: Option<bool>,
    /// Title of the page (its `#+TITLE:`, or its name) in large type above the editor.
    pub show_page_title: Option<bool>,
    /// Quit button in the top right corner of the window.
    pub show_quit_button: Option<bool>,
    /// Line numbers in the margin of the editor.
    pub show_line_numbers: Option<bool>,
    /// Frames shown around the editor: pages menu, backlinks, tags, pages not created.
    pub show_pages: Option<bool>,
    pub show_backlinks: Option<bool>,
    pub show_tags: Option<bool>,
    pub show_broken_links: Option<bool>,
    /// "To do" frame: the headlines of the project with a TODO keyword.
    pub show_todos: Option<bool>,
    /// Enable the logseq-site-builder extension (Export button).
    pub site_builder_enabled: Option<bool>,
    /// Git extension: show the git state of the project under its name.
    pub git_status_enabled: Option<bool>,
    /// Private SSH key used by git pull / push (empty: the system's SSH setup).
    pub git_ssh_key: Option<String>,
    /// Every project (folder) the user has opened, most recent first.
    #[serde(default)]
    pub projects: Vec<String>,
    /// Pages and panes open when the app was last quit. Kept by the backend:
    /// the settings window never sends it.
    pub session: Option<Session>,
}

/// How the editor is split in two (Emacs windows).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SplitKind {
    /// Side by side (`C-x 3`).
    Vertical,
    /// One above the other (`C-x 2`).
    Horizontal,
}

/// The open pages and how the editor shows them, saved on quit.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Session {
    /// Folder of the project the pages belong to.
    pub project: String,
    /// Paths of the pages open in tabs, in tab order.
    #[serde(default)]
    pub tabs: Vec<String>,
    /// Page of the focused pane.
    pub active: Option<String>,
    pub split: Option<SplitKind>,
    /// Page of the other pane, when split.
    pub other: Option<String>,
    /// Whether the focused pane is the second one (right / bottom).
    #[serde(default)]
    pub focus_second: bool,
}

impl Settings {
    /// The settings with their defaults applied.
    pub fn prefs(&self) -> Prefs {
        Prefs::from_settings(self)
    }
}

/// The settings as the app uses them, defaults applied.
#[derive(Debug, Clone, PartialEq)]
pub struct Prefs {
    pub vault: String,
    pub builder: String,
    /// Interface language code (`fr`, `en`).
    pub lang: String,
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
    pub show_quit_button: bool,
    pub show_line_numbers: bool,
    pub show_pages: bool,
    pub show_backlinks: bool,
    pub show_tags: bool,
    pub show_broken_links: bool,
    pub show_todos: bool,
    pub site_builder: bool,
    pub git_ext: bool,
    pub git_ssh_key: String,
    pub case_insensitive_links: bool,
    pub hashtag_links: bool,
    pub hashtag_dashes: bool,
    pub org_tag_links: bool,
    pub org_tag_dashes: bool,
    pub find_match_case: bool,
    pub indent_headings: bool,
    pub tab_folds: bool,
    pub todo_enabled: bool,
    pub todo_keywords: String,
    pub emacs_mark: bool,
    pub electric_mode: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self::from_settings(&Settings::default())
    }
}

impl Prefs {
    pub fn from_settings(s: &Settings) -> Self {
        Self {
            vault: s.vault_path.clone().unwrap_or_default(),
            builder: s.logseq_site_builder_path.clone().unwrap_or_default(),
            lang: s.language.clone().unwrap_or_else(|| "fr".to_string()),
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
            show_quit_button: s.show_quit_button.unwrap_or(true),
            show_line_numbers: s.show_line_numbers.unwrap_or(false),
            show_pages: s.show_pages.unwrap_or(true),
            show_backlinks: s.show_backlinks.unwrap_or(true),
            show_tags: s.show_tags.unwrap_or(true),
            show_broken_links: s.show_broken_links.unwrap_or(true),
            show_todos: s.show_todos.unwrap_or(true),
            site_builder: s.site_builder_enabled.unwrap_or(false),
            git_ext: s.git_status_enabled.unwrap_or(true),
            git_ssh_key: s.git_ssh_key.clone().unwrap_or_default(),
            case_insensitive_links: s.case_insensitive_links.unwrap_or(true),
            hashtag_links: s.hashtag_links.unwrap_or(true),
            hashtag_dashes: s.hashtag_dashes.unwrap_or(true),
            org_tag_links: s.org_tag_links.unwrap_or(true),
            org_tag_dashes: s.org_tag_dashes.unwrap_or(false),
            find_match_case: s.find_match_case.unwrap_or(false),
            indent_headings: s.indent_headings.unwrap_or(true),
            tab_folds: s.tab_folds.unwrap_or(true),
            todo_enabled: s.todo_enabled.unwrap_or(true),
            todo_keywords: s.todo_keywords.clone().filter(|k| !k.trim().is_empty())
                .unwrap_or_else(|| DEFAULT_TODO_KEYWORDS.to_string()),
            emacs_mark: s.emacs_mark.unwrap_or(true),
            electric_mode: s.electric_mode.unwrap_or(true),
        }
    }

    pub fn to_settings(&self, projects: Vec<String>) -> Settings {
        let some = |s: &str| (!s.is_empty()).then(|| s.to_string());
        Settings {
            vault_path: some(&self.vault),
            logseq_site_builder_path: some(&self.builder),
            language: Some(self.lang.clone()),
            update_links_on_rename: Some(self.update_links),
            case_insensitive_links: Some(self.case_insensitive_links),
            hashtag_links: Some(self.hashtag_links),
            hashtag_dashes: Some(self.hashtag_dashes),
            org_tag_links: Some(self.org_tag_links),
            org_tag_dashes: Some(self.org_tag_dashes),
            find_match_case: Some(self.find_match_case),
            indent_headings: Some(self.indent_headings),
            tab_folds: Some(self.tab_folds),
            todo_enabled: Some(self.todo_enabled),
            todo_keywords: Some(self.todo_keywords.clone()),
            emacs_mark: Some(self.emacs_mark),
            electric_mode: Some(self.electric_mode),
            autosave: Some(self.autosave),
            delete_empty_pages: Some(self.delete_empty),
            delete_title_only_pages: Some(self.delete_title_only),
            trash_auto_empty: Some(self.trash_auto_empty),
            trash_keep_days: Some(self.trash_keep_days),
            trash_max_bytes: Some(self.trash_max_bytes),
            restore_session: Some(self.restore_session),
            show_brand: Some(self.show_brand),
            show_page_title: Some(self.show_page_title),
            show_quit_button: Some(self.show_quit_button),
            show_line_numbers: Some(self.show_line_numbers),
            show_pages: Some(self.show_pages),
            show_backlinks: Some(self.show_backlinks),
            show_tags: Some(self.show_tags),
            show_broken_links: Some(self.show_broken_links),
            show_todos: Some(self.show_todos),
            site_builder_enabled: Some(self.site_builder),
            git_status_enabled: Some(self.git_ext),
            git_ssh_key: some(&self.git_ssh_key),
            projects,
            session: None,
        }
    }

    /// How `#tags` and `:tags:` are read.
    pub fn tags(&self) -> TagSyntax {
        TagSyntax {
            hashtags: Hashtags::new(self.hashtag_links, self.hashtag_dashes),
            org: OrgTags { links: self.org_tag_links, dashes: self.org_tag_dashes },
        }
    }

    /// The TODO keywords of the settings; `None` when they are off.
    pub fn todo(&self) -> Option<TodoKeywords> {
        self.todo_enabled.then(|| TodoKeywords::parse(&self.todo_keywords))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_values_take_their_default() {
        let p = Settings::default().prefs();
        assert!(p.autosave && p.git_ext && !p.site_builder && !p.delete_empty && p.restore_session);
        assert_eq!(p.lang, "fr");
        assert_eq!(p.todo(), Some(TodoKeywords::parse(DEFAULT_TODO_KEYWORDS)));
        let blank = Settings { todo_keywords: Some(" ".into()), ..Settings::default() };
        assert_eq!(blank.prefs().todo_keywords, DEFAULT_TODO_KEYWORDS);
        assert_eq!(p.tags(), TagSyntax { hashtags: Hashtags::Dashes, org: OrgTags::default() });
    }

    #[test]
    fn prefs_round_trip_through_settings() {
        let p = Prefs { vault: "/x".into(), autosave: false, git_ext: false, ..Prefs::default() };
        let s = p.to_settings(vec!["/x".into()]);
        assert_eq!(s.prefs(), p);
        assert_eq!(s.projects, vec!["/x".to_string()]);
        assert_eq!(Prefs::default().to_settings(vec![]).vault_path, None);
    }
}
