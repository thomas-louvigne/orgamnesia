//! What `settings.yaml` says of each setting: its description, its section,
//! those the terminal ignores (and why), the notes at the top of the files.

/// The settings that do nothing in the terminal interface (`orgamnesia -nw`),
/// and why; written as comments above them in `settings.yaml`.
pub const TUI_UNSUPPORTED: &[(&str, &str)] = &[
    ("org_bloom_path", NOT_YET), ("org_bloom_enabled", NOT_YET), ("org_bloom_zip", NOT_YET), ("org_bloom_no_external_deps", NOT_YET),
    ("language", "the terminal is in English for now"),
    ("theme", "the terminal has its own colours"),
    ("ligatures", "the font is the terminal's"),
        ("indent_headings", NOT_YET),     ("show_images", NO_IMAGES), ("show_video_thumbnails", NO_IMAGES), ("embed_videos", NO_IMAGES),
    ("image_size_format", NO_IMAGES), ("show_excalidraw", NO_IMAGES),
    ("link_preview", MOUSE), ("link_preview_trigger", MOUSE),
    ("webkit_emoji_shortcuts", "the window's engine (WebKitGTK) only"),
    ("shortcut_tips", LAYOUT),
        ("show_brand", LAYOUT), ("show_page_title", LAYOUT), ("show_page_nav", LAYOUT), ("show_quit_button", LAYOUT),
    ("show_pages", LAYOUT), ("show_backlinks", LAYOUT), ("show_tags", LAYOUT), ("show_broken_links", LAYOUT),
    ("show_tips", LAYOUT), ("show_todos", LAYOUT), ("show_project", LAYOUT), ("show_git", LAYOUT),
    ("show_settings_button", LAYOUT), ("frames_left", LAYOUT), ("frames_right", LAYOUT),
    ("show_org_bloom", LAYOUT), ("show_graph", LAYOUT),
    ("spell_enabled", NOT_YET), ("spell_language", NOT_YET), ("spell_dict_dir", NOT_YET),
    ("git_unpushed_marker", NOT_YET),
    ("check_updates", UPDATES), ("skipped_update", UPDATES),
];
const NOT_YET: &str = "not in the terminal yet";
const NO_IMAGES: &str = "no images in a terminal";
const LAYOUT: &str = "the window's layout";
const MOUSE: &str = "on mouse hover, in the window";
const UPDATES: &str = "updates go through the window";

/// What each setting of `settings.yaml` does, written as a comment above it.
/// Every setting has one (see the test `every_setting_is_described`).
pub const DESCRIPTIONS: &[(&str, &str)] = &[
    ("language", "Language of the interface: fr or en."),
    ("default_format", "Format of the new pages: org (.org) or markdown (.md). When two pages share a name,\n[[Name]] leads to the one in this format."),
    ("single_format", "true: only the pages in the default format are read, the others are ignored."),
    ("ask_format", "true: the format (org or markdown) of each new page is asked."),
    ("restore_session", "On start, open again the pages and split windows open when the app was quit."),
    ("check_updates", "Look for a new version of the app when it starts."),
    ("skipped_update", "A new version you chose to skip: not offered again. Set by the app."),
    ("org_links", "In org pages, also read as links: [[Page|text]] (pipe), [[Page#Heading]] (hash),\n[text](Page.md) (markdown)."),
    ("md_links", "The same in Markdown pages."),
    ("case_insensitive_links", "Links ignore case and accents: [[elodie]] leads to the page Élodie."),
    ("update_links_on_rename", "When a page is renamed (or moved), the links to it in the other pages follow."),
    ("click_follows_links", "A plain click on a link follows it. false: Ctrl+click follows it, a click places the cursor."),
    ("hashtag_links", "A word after a hash (#idea) is a tag, and a link to the page of that name."),
    ("hashtag_dashes", "Tags may hold dashes: #my-tag."),
    ("org_tag_links", "Org tags (:tag: on headlines, #+FILETAGS:, :tag: in the text) are links to the page of\nthat name."),
    ("org_tag_dashes", "Org tags may hold dashes: :my-tag: (org-mode itself does not allow them)."),
    ("mark_broken_links", "A mark in the margin of the lines holding a link to a page or file that does not exist."),
    ("link_preview", "A preview of the page a link leads to, shown over the link."),
    ("link_preview_trigger", "When the preview shows: ctrl (mouse over the link, Ctrl held) or hover (mouse resting on it)."),
    ("embed_videos", "A click on a video's thumbnail plays it in the app instead of the browser."),
    ("autosave", "Save the page at every change (window and terminal). false: only with the save shortcut,\nor when leaving the page."),
    ("electric_mode", "Typing a bracket or a quote with text selected puts the selection inside."),
    ("electric_emphasis", "With electric_mode: typing * / or _ with text selected makes it bold, italic or underlined."),
    ("find_match_case", "The search in the page starts case and accent sensitive."),
    ("indent_headings", "The text under a headline indented to its title (on screen only: the file is unchanged)."),
    ("tab_folds", "Tab on a headline folds or unfolds it, as in org-mode."),
    ("todo_enabled", "TODO keywords on headlines (* TODO Title), stepped through with a shortcut."),
    ("todo_keywords", "The TODO keywords, in order; those after | mean done. Example: TODO DOING | DONE."),
    ("delete_empty_pages", "On quit, the pages with no text (only spaces or *) go to the trash."),
    ("delete_title_only_pages", "On quit, the pages holding only their title heading go to the trash."),
    ("trash_auto_empty", "When a project opens, its trash is emptied of the old pages (the two settings below)."),
    ("trash_keep_days", "Days a deleted page stays in the trash. 0: no limit."),
    ("trash_max_bytes", "Size of the trash in bytes beyond which its oldest pages go for good. 0: no limit."),
    ("templates_folder", "Folder of the page templates, in the project: templates, _meta/Templates…"),
    ("show_page_title", "The page's title (its #+TITLE:, else its name) in large type above the editor."),
    ("show_page_nav", "Back and forward arrows around the page title."),
    ("show_line_numbers", "Line numbers in the margin."),
    ("show_images", "Images drawn in the page (a line holding only a link to an image)."),
    ("image_size_format", "How an image's size is written: org (#+ATTR_ORG: :width 300 on the line above)\nor logseq ({:width 300} after the link)."),
    ("show_excalidraw", "Excalidraw drawings (.excalidraw.svg, .excalidraw.png) drawn like images."),
    ("show_video_thumbnails", "A line holding only a video link (YouTube, Vimeo, Dailymotion, Twitch, PeerTube) shows\nits thumbnail."),
    ("hide_leading_stars", "Only the last star of a headline shown (  * Title), as Emacs' org-hide-leading-stars."),
    ("live_preview", "The markup (* / [[ ]] # …) fades away but on the cursor's line: the text reads as rendered."),
    ("ligatures", "The font's ligatures in the editor: -> drawn as an arrow, != as ≠…"),
    ("link_text_first", "A link's text in bold, its target dimmed."),
    ("frames_left", "Frames of the column left of the editor, top to bottom. Names: brand, project, git, pages,\nsettings, backlinks, tags, todos, broken, quit, org_bloom, graph, tips."),
    ("frames_right", "Frames of the column right of the editor (same names)."),
    ("show_brand", "The app's name and logo."),
    ("show_project", "The name of the open project, with the menu of the projects."),
    ("show_git", "The git state of the project (with the git extension on)."),
    ("show_pages", "The frame listing the pages."),
    ("show_backlinks", "The frame of the pages linking to the open one."),
    ("show_tags", "The frame of the tags."),
    ("show_broken_links", "The frame of the links to pages not created yet."),
    ("show_todos", "The frame of the project's headlines with a TODO keyword."),
    ("show_org_bloom", "The button building the site (with the org-bloom extension on)."),
    ("show_graph", "The graph button (with the graph extension on)."),
    ("show_tips", "The “Did you know?” frame."),
    ("show_quit_button", "The quit button."),
    ("show_settings_button", "The button opening the settings."),
    ("theme", "Colours: auto (light or dark as the system is), dark, light, fjord, paper, dracula,\nor the name of one of yours (themes/<name>.css)."),
    ("keybinding_profile", "The shortcuts in use: classic, emacs, or the name of one of yours (keybindings/<name>.yaml)."),
    ("emacs_mark", "Ctrl+Space starts a selection that follows the cursor, as Emacs' mark."),
    ("webkit_emoji_shortcuts", "The window's engine (WebKitGTK) opens its emoji picker on its own shortcuts.\nfalse: the app blocks them."),
    ("shortcut_tips", "A button clicked whose action has a shortcut recalls the shortcut (a few times)."),
    ("spell_enabled", "Spelling: the words the dictionary does not know underlined in red."),
    ("spell_language", "Dictionary used: fr_FR, en_US… Empty: the one of the interface language."),
    ("spell_dict_dir", "A folder of Hunspell dictionaries (.aff + .dic) looked in before the system's."),
    ("org_bloom_enabled", "org-bloom extension: a button builds a website from the project."),
    ("org_bloom_path", "Command running org-bloom. Empty: org-bloom."),
    ("org_bloom_zip", "org-bloom also zips the site, into <output_dir>.zip."),
    ("org_bloom_no_external_deps", "org-bloom builds without the project's hand-made web folders (--no-external-deps)."),
    ("git_status_enabled", "Git extension: the git state of the project shown under its name."),
    ("git_ssh_key", "Private SSH key used to pull and push. Empty: your usual SSH setup."),
    ("git_fetch_minutes", "Fetch the remote every so many minutes and when the project opens, so that the commits\nto pull are counted. 0: never."),
    ("git_pull_rebase", "When your history and the remote's diverged, a pull replays your commits on the remote's\n(--rebase). false: such a pull is refused."),
    ("git_unpushed_marker", "The pages that differ from the remote marked in the list of the pages."),
    ("git_commit_push", "The commit window's main button also pushes."),
    ("git_commit_template", "The commit message proposed; {date}, {time}, {count} and {files} are replaced.\nEmpty: a summary and the list of the files."),
    ("git_auto_sync_minutes", "Every so many minutes: commit the changes, pull, then push. 0: never."),
    ("git_sync_on_quit", "The same sync (commit, pull, push) when the app quits."),
    ("graph_enabled", "Graph extension: the pages and their links drawn in 3D."),
    ("journal_enabled", "Journal extension: a shortcut opens the page of the day, named by its date (2026-11-24)."),
    ("journal_folder", "Folder of the journal's pages, next to pages/ (an existing posts/ or daily/ is taken instead)."),
];

/// The settings by the tab of the settings window showing them, in its order:
/// how `settings.yaml` is written, each tab as a comment above its settings.
pub const SECTIONS: &[(&str, &[&str])] = &[
    ("General", &["language", "default_format", "single_format", "ask_format", "restore_session", "check_updates", "skipped_update"]),
    ("Links", &["org_links", "md_links", "case_insensitive_links", "update_links_on_rename", "click_follows_links", "hashtag_links", "hashtag_dashes",
        "org_tag_links", "org_tag_dashes", "mark_broken_links", "link_preview", "link_preview_trigger", "embed_videos"]),
    ("Editing", &["autosave", "electric_mode", "electric_emphasis", "find_match_case", "indent_headings", "tab_folds",
        "todo_enabled", "todo_keywords", "delete_empty_pages", "delete_title_only_pages", "trash_auto_empty", "trash_keep_days", "trash_max_bytes"]),
    ("Templates", &["templates_folder"]),
    ("Display", &["show_page_title", "show_page_nav", "show_line_numbers", "show_images", "image_size_format", "show_excalidraw",
        "show_video_thumbnails", "hide_leading_stars", "live_preview", "ligatures", "link_text_first"]),
    ("Frames", &["frames_left", "frames_right", "show_brand", "show_project", "show_git", "show_pages", "show_backlinks", "show_tags",
        "show_broken_links", "show_todos", "show_org_bloom", "show_graph", "show_tips", "show_quit_button", "show_settings_button"]),
    ("Themes", &["theme"]),
    ("Shortcuts", &["keybinding_profile", "emacs_mark", "webkit_emoji_shortcuts", "shortcut_tips"]),
    ("Spelling", &["spell_enabled", "spell_language", "spell_dict_dir"]),
    ("Extensions", &["org_bloom_enabled", "org_bloom_path", "org_bloom_zip", "org_bloom_no_external_deps", "git_status_enabled",
        "git_ssh_key", "git_fetch_minutes", "git_pull_rebase", "git_unpushed_marker", "git_commit_push", "git_commit_template",
        "git_auto_sync_minutes", "git_sync_on_quit", "graph_enabled", "journal_enabled", "journal_folder"]),
];

/// What the app writes at the top of `settings.yaml`.
pub(super) const SETTINGS_NOTE: &str = "\
# Orgamnesia settings. Each setting is explained in the comment above it; a setting
# missing from this file (or null) takes its default value.
# Saved, a change applies at once in the window, at the next start in the terminal.
# The app writes this file again when a setting changes in the settings window:
# the comments you added are then lost.
# “# window only: …” after a setting: it does nothing in the terminal version
# (orgamnesia -nw), for the reason given.
";

/// What the app writes at the top of a keybinding profile: the shortcuts the
/// terminal interface can't do, and why.
pub const PROFILE_NOTE: &str = "\
# Terminal version (orgamnesia -nw): these shortcuts work in the window only.
# - export_pdf, export_html: the export needs the window's renderer (HTML, printing).
# - Ctrl+Shift+…, Ctrl+Tab, Ctrl+Shift+Tab: some terminals keep them (kitty: its tabs,
#   copy and paste…), the app never gets them; give these actions another key too.
# - Ctrl+, Ctrl+/ Ctrl+Enter: only terminals with kitty's keyboard protocol (kitty, foot,
#   WezTerm, Ghostty…) tell them from other keys.
";
