//! The settings: `Config` as stored in `settings.yaml`, `Settings` with what the
//! app keeps for itself (`state.json`), `Prefs` as used.

use serde::{Deserialize, Serialize};

use crate::{links::LinkSyntax, Format, Formats, Frame, Hashtags, Layout, OrgTags, TagSyntax, TodoKeywords};
mod prefs;
pub use prefs::*;

/// The TODO keywords when none are set (Emacs style: done states after the `|`).
pub const DEFAULT_TODO_KEYWORDS: &str = "TODO DOING HANGUP | DONE";

/// The settings and what the app keeps for itself, as the backend and the
/// interface exchange them.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Settings {
    /// What the user sets (`settings.yaml`).
    #[serde(flatten)]
    pub config: Config,
    /// The rest is kept by the app (`state.json`). The open project.
    pub vault_path: Option<String>,
    /// Every project (folder) the user has opened, most recent first.
    #[serde(default)]
    pub projects: Vec<String>,
    /// Pages and panes open when the app was last quit. Kept by the backend:
    /// the settings window never sends it.
    pub session: Option<Session>,
    /// Folders of `.org` files whose user didn't want an `assets/` folder made.
    #[serde(default)]
    pub assets_declined: Vec<String>,
    /// The user wants to keep the JSON settings files of older versions (now YAML):
    /// their deletion is not offered again.
    #[serde(default)]
    pub keep_json: bool,
    /// The widths of the columns of the terminal interface (in characters), as dragged.
    #[serde(default)]
    pub tui_left_width: Option<u16>,
    #[serde(default)]
    pub tui_right_width: Option<u16>,
    /// The app runs for the first time (no `settings.yaml` yet): the interface
    /// asks the format of the pages and the shortcuts. Not saved.
    #[serde(default)]
    pub first_run: bool,
}

/// The keys of `Settings` kept in `state.json`, not in `settings.yaml`.
pub const STATE_KEYS: [&str; 7] = ["vault_path", "projects", "session", "assets_declined", "keep_json", "tui_left_width", "tui_right_width"];

/// `settings.yaml`, what the user sets. A missing value (older file, or never
/// set) means the default: see `Prefs`, the only place defaults are decided.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Config {
    /// Command building the site (org-bloom extension); empty: `org-bloom`.
    pub org_bloom_path: Option<String>,
    pub language: Option<String>,
    /// Theme: `auto` (light or dark as the system is), `dark`, `light`, or the
    /// file name of one of the user's (`themes/<name>.css`).
    pub theme: Option<String>,
    /// Format of the new pages: `org` (`.org`) or `markdown` (`.md`); between two
    /// pages of the same name, the one `[[Name]]` leads to.
    pub default_format: Option<String>,
    /// Only the pages of the default format are read; the others are left aside.
    pub single_format: Option<bool>,
    /// Ask the format of each new page.
    pub ask_format: Option<bool>,
    /// In org-mode pages, also read as links `[[Page|texte]]` (`pipe`),
    /// `[[Page#Titre]]` (`hash`), `[texte](Page.md)` (`markdown`). All off by default.
    pub org_links: Option<LinkSyntax>,
    /// The same in Markdown pages. All on by default.
    pub md_links: Option<LinkSyntax>,
    /// Rewrite `[[links]]` in other pages when a page is renamed.
    pub update_links_on_rename: Option<bool>,
    /// Match `[[links]]` to pages ignoring case.
    pub case_insensitive_links: Option<bool>,
    /// A plain click on a link follows it, as Ctrl+click does; off by default
    /// (a click then places the cursor in the link).
    pub click_follows_links: Option<bool>,
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
    /// Hide the stars of a headline but the last (`  * Titre`), like Emacs'
    /// org-hide-leading-stars: the level reads as an indent.
    pub hide_leading_stars: Option<bool>,
    /// The markup (`*`, `/`, `[[target][`, `]]`, `**`, `#`…) fades away, except on
    /// the line of the caret: the text reads as it is rendered.
    pub live_preview: Option<bool>,
    /// The font's ligatures in the editor (`->` drawn as an arrow, `!=` as `≠`…).
    pub ligatures: Option<bool>,
    /// A link's text in bold, its target dimmed and underlined (colours: the theme's
    /// `--link-target`, `--link-text`, `--link-text-weight`).
    pub link_text_first: Option<bool>,
    /// Tab on a headline folds / unfolds it (org-mode cycle).
    pub tab_folds: Option<bool>,
    /// TODO keywords on headlines (`* TODO Titre`), stepped through with a shortcut.
    pub todo_enabled: Option<bool>,
    /// The TODO keywords, as in Emacs: `TODO DOING | DONE`.
    pub todo_keywords: Option<String>,
    /// A mark in the margin of the lines holding a link that leads nowhere (a page
    /// that doesn't exist, a missing file).
    pub mark_broken_links: Option<bool>,
    /// Draw the images of the page (a line holding only a link to an image).
    pub show_images: Option<bool>,
    /// A line holding only a link to a video (YouTube, Vimeo, Dailymotion, Twitch,
    /// PeerTube) is drawn as its thumbnail.
    pub show_video_thumbnails: Option<bool>,
    /// A click on a video's thumbnail plays it in the app, in the site's player
    /// (embedded), instead of opening it in the browser.
    pub embed_videos: Option<bool>,
    /// How a size given to an image is written: `org` (`#+ATTR_ORG: :width 300` on
    /// the line above, Emacs) or `logseq` (`{:height 200, :width 300}` after the link).
    pub image_size_format: Option<String>,
    /// A preview of the page a link leads to, shown over the link.
    pub link_preview: Option<bool>,
    /// When the preview shows: `ctrl` (the mouse over the link with Ctrl held) or
    /// `hover` (the mouse resting on the link).
    pub link_preview_trigger: Option<String>,
    /// Emacs mark: Ctrl+Space starts a region that follows the cursor.
    pub emacs_mark: Option<bool>,
    /// WebKitGTK's emoji picker opens on its own shortcuts (off: the app blocks them).
    pub webkit_emoji_shortcuts: Option<bool>,
    /// A button clicked whose action has a shortcut recalls it (a few times).
    pub shortcut_tips: Option<bool>,
    /// Electric mode: typing a bracket or quote around a selection wraps it.
    pub electric_mode: Option<bool>,
    /// With electric mode, typing an emphasis marker (`* / _`) around a selection wraps it.
    pub electric_emphasis: Option<bool>,
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
    /// App name and logo.
    pub show_brand: Option<bool>,
    /// Title of the page (its `#+TITLE:`, or its name) in large type above the editor.
    pub show_page_title: Option<bool>,
    /// Back / forward arrows around the page title.
    pub show_page_nav: Option<bool>,
    /// Quit button.
    pub show_quit_button: Option<bool>,
    /// Line numbers in the margin of the editor.
    pub show_line_numbers: Option<bool>,
    /// Frames shown around the editor: pages menu, backlinks, tags, pages not created.
    pub show_pages: Option<bool>,
    pub show_backlinks: Option<bool>,
    pub show_tags: Option<bool>,
    pub show_broken_links: Option<bool>,
    /// The "Did you know?" frame.
    pub show_tips: Option<bool>,
    /// "To do" frame: the headlines of the project with a TODO keyword.
    pub show_todos: Option<bool>,
    /// Name of the open project, with the menu of the projects.
    pub show_project: Option<bool>,
    /// Git state of the project (when the git extension is on).
    pub show_git: Option<bool>,
    /// Button opening the settings.
    pub show_settings_button: Option<bool>,
    /// Frames of the column left of the editor, top to bottom, by name (see
    /// `Frame::id`); the others are in the right column.
    pub frames_left: Option<Vec<String>>,
    /// Frames of the column right of the editor.
    pub frames_right: Option<Vec<String>>,
    /// Enable the org-bloom extension: a button building the site of the project.
    pub org_bloom_enabled: Option<bool>,
    /// Org-bloom button (when the extension is on).
    pub show_org_bloom: Option<bool>,
    /// Org-bloom also zips the site, into `<output_dir>.zip`.
    pub org_bloom_zip: Option<bool>,
    /// Org-bloom builds without the hand-made web folders of the project
    /// (`--no-external-deps`).
    pub org_bloom_no_external_deps: Option<bool>,
    /// Spelling extension: a red line under the words the dictionary doesn't know.
    pub spell_enabled: Option<bool>,
    /// Dictionary of the spelling extension (`fr_FR`, `en_US`…); empty: the one of
    /// the interface language.
    pub spell_language: Option<String>,
    /// A folder of Hunspell dictionaries (`.aff` + `.dic`) looked in before the
    /// system's.
    pub spell_dict_dir: Option<String>,
    /// Git extension: show the git state of the project under its name.
    pub git_status_enabled: Option<bool>,
    /// Graph extension: the pages and their links drawn in the main area.
    pub graph_enabled: Option<bool>,
    /// Graph button (when the extension is on).
    pub show_graph: Option<bool>,
    /// Journal extension: a shortcut opens the page of the day, named by its date.
    pub journal_enabled: Option<bool>,
    /// Journal extension: the folder of the pages of the days, next to `pages/`
    /// (`journals`; an existing `posts/` or `daily/` is taken instead).
    pub journal_folder: Option<String>,
    /// The `.excalidraw.svg` / `.excalidraw.png` drawings shown in the pages like
    /// images (when the images are).
    pub show_excalidraw: Option<bool>,
    /// Private SSH key used by git pull / push (empty: the system's SSH setup).
    pub git_ssh_key: Option<String>,
    /// Git extension: every so many minutes, commit the changes, pull and push
    /// (0: never).
    pub git_auto_sync_minutes: Option<u64>,
    /// Git extension: the same sync on quitting the app.
    pub git_sync_on_quit: Option<bool>,
    /// Git extension: fetch the remote every so many minutes, and when the
    /// project opens, so that the commits to pull are counted (0: never).
    pub git_fetch_minutes: Option<u64>,
    /// Git extension: a pull on a history that diverged from the remote replays
    /// the local commits on the remote's (`--rebase`); off, it is refused.
    pub git_pull_rebase: Option<bool>,
    /// Git extension: mark the pages differing from the remote in the pages list.
    pub git_unpushed_marker: Option<bool>,
    /// Git extension: the commit window's main button also pushes.
    pub git_commit_push: Option<bool>,
    /// Git extension: the commit message proposed, with `{date}`, `{time}`,
    /// `{count}` and `{files}` replaced; empty: a summary and the list of the files.
    pub git_commit_template: Option<String>,
    /// The folder of the page templates, in the project (Obsidian's "Template folder
    /// location"): `templates` when not set.
    pub templates_folder: Option<String>,
    /// Look for a new version of the app when it starts.
    pub check_updates: Option<bool>,
    /// The new version the user chose to skip: not offered again at start.
    pub skipped_update: Option<String>,
    /// The keybinding profile in effect: a built-in one (`classic`, `emacs`) or
    /// the name of one of the user's (`keybindings/<name>.yaml`). Kept apart from
    /// `Prefs`: the keybindings are saved on their own.
    pub keybinding_profile: Option<String>,
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
    #[serde(default)]
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

#[cfg(test)]
mod tests;
