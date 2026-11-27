//! The action registry, single source of truth for the interfaces and the
//! presets: every action that can be given shortcuts, its id and its label.

/// An action that can be given shortcuts: its typed value, the id stored in
/// `keybindings.json` and the i18n key of its label.
pub struct ActionDef<A: 'static> {
    pub action: A,
    pub id: &'static str,
    pub label_key: &'static str,
}

/// Application actions (handled outside the editor).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppAction {
    NewPage, Save, OpenSettings, ShowShortcuts, QuickOpen, ProjectSearch, ProjectReplace, AddTag, CloseTab, NextTab, PrevTab, PageBack, PageForward,
    SplitVertical, SplitHorizontal, CloseSplit, SingleWindow, OtherWindow, NextRegion, PrevRegion, Quit,
    GitCommit, GitPull, GitPush, GitHistory, ExportPdf, ExportHtml, Graph, SwitchProject, Journal,
    /// Open a page of the project picked at random (not the one shown).
    RandomPage,
    /// Only the page, full screen: no columns, tabs nor bars.
    ZenMode,
    /// Rename the page; the TODO headlines, the tags, the pages linking here, the
    /// pages not created yet (the window's frames, the terminal's panels).
    RenamePage, TodosPanel, TagsPanel, BacklinksPanel, BrokenPanel,
    /// The list of the pages (its frame).
    PagesPanel,
    /// Quick capture (see `capture`): the page notes go to, beside the one in view.
    Capture,
    /// The folders of the pages: one made, the page open moved to one, one
    /// renamed (moved), one deleted. No shortcut by default.
    NewFolder, MovePage, RenameFolder, DeleteFolder,
    /// The agenda of the week (org-mode's `C-c a a`).
    Agenda,
    /// The page of the journal before or after the day shown (today, on
    /// another page), as org-journal's `C-c C-b` / `C-c C-f`.
    JournalPrevious, JournalNext,
}

/// Editor actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorAction {
    Copy, Cut, Paste, SelectAll, Undo, Redo, NewHeading,
    BeginningOfLine, EndOfLine, KillLine, KillWordForward, KillWordBackward, Yank,
    KillRegion, CopyRegion, DeleteCharForward, TransposeChars, TransposeWords,
    UpcaseWord, DowncaseWord, CapitalizeWord, UpcaseRegion, DowncaseRegion,
    DeleteHorizontalSpace, JustOneSpace, DeleteIndentation, ZapToChar, FillParagraph, YankPop,
    ForwardChar, BackwardChar, NextLine, PreviousLine, ForwardWord, BackwardWord,
    BeginningOfBuffer, EndOfBuffer, ScrollDown, ScrollUp, Recenter,
    BackwardSentence, ForwardSentence, BackwardParagraph, ForwardParagraph, BackwardElement, ForwardElement,
    BackToIndentation, PreviousHeading, NextHeading, BackwardHeading, ForwardHeading, UpHeading, OpenLink, SetMark, KeyboardQuit,
    Find, FindPrevious, Replace, TableNextCell, TablePrevCell, GlobalCycle,
    InsertLink, InsertDate, InsertInactiveDate, InsertFootnote,
    Bold, Italic, Underline, Strikethrough, Quote, Comment, TodoNext, TodoPrev, Schedule, Deadline,
    PromoteSubtree, DemoteSubtree, MoveLineUp, MoveLineDown, SpellNext,
}

/// The editor actions the command list (`>`) offers too, in the window and the
/// terminal: those worth a name. Not the moves, nor what only makes sense chained
/// at the keyboard (mark, zap, yank-pop), nor the clipboard (the right-click menu has it).
pub const PALETTE_EDITOR_ACTIONS: &[EditorAction] = {
    use EditorAction::*;
    &[
        InsertLink, InsertDate, InsertInactiveDate, InsertFootnote,
        Bold, Italic, Underline, Strikethrough, Quote, Comment,
        NewHeading, TodoNext, TodoPrev, Schedule, Deadline, PromoteSubtree, DemoteSubtree, GlobalCycle, MoveLineUp, MoveLineDown,
        Find, Replace, OpenLink, SpellNext,
        UpcaseWord, DowncaseWord, CapitalizeWord, UpcaseRegion, DowncaseRegion, FillParagraph,
        Undo, Redo, SelectAll, DeleteHorizontalSpace, JustOneSpace, DeleteIndentation,
    ]
};

impl AppAction {
    pub fn from_id(id: &str) -> Option<Self> {
        APP_ACTIONS.iter().find(|d| d.id == id).map(|d| d.action)
    }
}

impl EditorAction {
    pub fn from_id(id: &str) -> Option<Self> {
        EDITOR_ACTIONS.iter().find(|d| d.id == id).map(|d| d.action)
    }
}

pub const APP_ACTIONS: &[ActionDef<AppAction>] = &[
    ActionDef { action: AppAction::NewPage, id: "new_page",      label_key: "shortcut_new_page" },
    ActionDef { action: AppAction::Save, id: "save",          label_key: "shortcut_save" },
    ActionDef { action: AppAction::OpenSettings, id: "open_settings", label_key: "shortcut_settings" },
    ActionDef { action: AppAction::ShowShortcuts, id: "show_shortcuts", label_key: "shortcut_show_shortcuts" },
    ActionDef { action: AppAction::QuickOpen, id: "quick_open",    label_key: "shortcut_quick_open" },
    ActionDef { action: AppAction::ProjectSearch, id: "project_search",   label_key: "shortcut_project_search" },
    ActionDef { action: AppAction::ProjectReplace, id: "project_replace", label_key: "shortcut_project_replace" },
    ActionDef { action: AppAction::AddTag, id: "add_tag",                 label_key: "shortcut_add_tag" },
    ActionDef { action: AppAction::CloseTab, id: "close_tab",     label_key: "shortcut_close_tab" },
    ActionDef { action: AppAction::NextTab, id: "next_tab",      label_key: "shortcut_next_tab" },
    ActionDef { action: AppAction::PrevTab, id: "prev_tab",      label_key: "shortcut_prev_tab" },
    ActionDef { action: AppAction::PageBack, id: "page_back",     label_key: "shortcut_page_back" },
    ActionDef { action: AppAction::PageForward, id: "page_forward", label_key: "shortcut_page_forward" },
    ActionDef { action: AppAction::SplitVertical, id: "split_vertical",   label_key: "shortcut_split_vertical" },
    ActionDef { action: AppAction::SplitHorizontal, id: "split_horizontal", label_key: "shortcut_split_horizontal" },
    ActionDef { action: AppAction::CloseSplit, id: "close_split",      label_key: "shortcut_close_split" },
    ActionDef { action: AppAction::SingleWindow, id: "single_window",    label_key: "shortcut_single_window" },
    ActionDef { action: AppAction::OtherWindow, id: "other_window",     label_key: "shortcut_other_window" },
    ActionDef { action: AppAction::NextRegion, id: "next_region",      label_key: "shortcut_next_region" },
    ActionDef { action: AppAction::PrevRegion, id: "prev_region",      label_key: "shortcut_prev_region" },
    ActionDef { action: AppAction::Quit, id: "quit",             label_key: "shortcut_quit" },
    ActionDef { action: AppAction::GitCommit, id: "git_commit",  label_key: "shortcut_git_commit" },
    ActionDef { action: AppAction::GitPull, id: "git_pull",      label_key: "shortcut_git_pull" },
    ActionDef { action: AppAction::GitPush, id: "git_push",      label_key: "shortcut_git_push" },
    ActionDef { action: AppAction::GitHistory, id: "git_history", label_key: "shortcut_git_history" },
    ActionDef { action: AppAction::ExportPdf, id: "export_pdf",  label_key: "shortcut_export_pdf" },
    ActionDef { action: AppAction::ExportHtml, id: "export_html", label_key: "shortcut_export_html" },
    ActionDef { action: AppAction::Graph, id: "graph", label_key: "shortcut_graph" },
    ActionDef { action: AppAction::SwitchProject, id: "switch_project", label_key: "shortcut_switch_project" },
    ActionDef { action: AppAction::Journal, id: "journal", label_key: "shortcut_journal" },
    ActionDef { action: AppAction::RandomPage, id: "random_page", label_key: "shortcut_random_page" },
    ActionDef { action: AppAction::Capture, id: "capture", label_key: "shortcut_capture" },
    ActionDef { action: AppAction::NewFolder, id: "new_folder", label_key: "shortcut_new_folder" },
    ActionDef { action: AppAction::MovePage, id: "move_page", label_key: "shortcut_move_page" },
    ActionDef { action: AppAction::RenameFolder, id: "rename_folder", label_key: "shortcut_rename_folder" },
    ActionDef { action: AppAction::DeleteFolder, id: "delete_folder", label_key: "shortcut_delete_folder" },
    ActionDef { action: AppAction::ZenMode, id: "zen_mode", label_key: "shortcut_zen_mode" },
    ActionDef { action: AppAction::RenamePage, id: "rename_page", label_key: "shortcut_rename_page" },
    ActionDef { action: AppAction::TodosPanel, id: "panel_todos", label_key: "shortcut_panel_todos" },
    ActionDef { action: AppAction::TagsPanel, id: "panel_tags", label_key: "shortcut_panel_tags" },
    ActionDef { action: AppAction::BacklinksPanel, id: "panel_backlinks", label_key: "shortcut_panel_backlinks" },
    ActionDef { action: AppAction::BrokenPanel, id: "panel_broken", label_key: "shortcut_panel_broken" },
    ActionDef { action: AppAction::Agenda, id: "agenda", label_key: "shortcut_agenda" },
    ActionDef { action: AppAction::JournalPrevious, id: "journal_previous", label_key: "shortcut_journal_previous" },
    ActionDef { action: AppAction::JournalNext, id: "journal_next", label_key: "shortcut_journal_next" },
    ActionDef { action: AppAction::PagesPanel, id: "panel_pages", label_key: "shortcut_panel_pages" },
];

/// Every editor action, whatever the profile: a profile simply leaves the ones
/// it doesn't use unbound.
pub const EDITOR_ACTIONS: &[ActionDef<EditorAction>] = &[
    ActionDef { action: EditorAction::Copy, id: "copy",                label_key: "shortcut_copy" },
    ActionDef { action: EditorAction::Cut, id: "cut",                 label_key: "shortcut_cut" },
    ActionDef { action: EditorAction::Paste, id: "paste",               label_key: "shortcut_paste" },
    ActionDef { action: EditorAction::SelectAll, id: "select_all",          label_key: "shortcut_select_all" },
    ActionDef { action: EditorAction::Undo, id: "undo",                label_key: "shortcut_undo_edit" },
    ActionDef { action: EditorAction::Redo, id: "redo",                label_key: "shortcut_redo" },
    ActionDef { action: EditorAction::NewHeading, id: "new_heading",         label_key: "shortcut_new_heading" },
    ActionDef { action: EditorAction::BeginningOfLine, id: "beginning_of_line",   label_key: "shortcut_bol" },
    ActionDef { action: EditorAction::EndOfLine, id: "end_of_line",         label_key: "shortcut_eol" },
    ActionDef { action: EditorAction::KillLine, id: "kill_line",           label_key: "shortcut_kill_line" },
    ActionDef { action: EditorAction::KillWordForward, id: "kill_word_forward",   label_key: "shortcut_kill_word_fwd" },
    ActionDef { action: EditorAction::KillWordBackward, id: "kill_word_backward",  label_key: "shortcut_kill_word_bwd" },
    ActionDef { action: EditorAction::Yank, id: "yank",                label_key: "shortcut_yank" },
    ActionDef { action: EditorAction::KillRegion, id: "kill_region",         label_key: "shortcut_kill_region" },
    ActionDef { action: EditorAction::CopyRegion, id: "copy_region",         label_key: "shortcut_copy_region" },
    ActionDef { action: EditorAction::DeleteCharForward, id: "delete_char_forward", label_key: "shortcut_del_char_fwd" },
    ActionDef { action: EditorAction::TransposeChars, id: "transpose_chars",   label_key: "shortcut_transpose_chars" },
    ActionDef { action: EditorAction::TransposeWords, id: "transpose_words",   label_key: "shortcut_transpose_words" },
    ActionDef { action: EditorAction::UpcaseWord, id: "upcase_word",           label_key: "shortcut_upcase_word" },
    ActionDef { action: EditorAction::DowncaseWord, id: "downcase_word",       label_key: "shortcut_downcase_word" },
    ActionDef { action: EditorAction::CapitalizeWord, id: "capitalize_word",   label_key: "shortcut_capitalize_word" },
    ActionDef { action: EditorAction::UpcaseRegion, id: "upcase_region",       label_key: "shortcut_upcase_region" },
    ActionDef { action: EditorAction::DowncaseRegion, id: "downcase_region",   label_key: "shortcut_downcase_region" },
    ActionDef { action: EditorAction::DeleteHorizontalSpace, id: "delete_horizontal_space", label_key: "shortcut_delete_horizontal_space" },
    ActionDef { action: EditorAction::JustOneSpace, id: "just_one_space",      label_key: "shortcut_just_one_space" },
    ActionDef { action: EditorAction::DeleteIndentation, id: "delete_indentation", label_key: "shortcut_delete_indentation" },
    ActionDef { action: EditorAction::ZapToChar, id: "zap_to_char",            label_key: "shortcut_zap_to_char" },
    ActionDef { action: EditorAction::FillParagraph, id: "fill_paragraph",     label_key: "shortcut_fill_paragraph" },
    ActionDef { action: EditorAction::YankPop, id: "yank_pop",                 label_key: "shortcut_yank_pop" },
    ActionDef { action: EditorAction::ForwardChar, id: "forward_char",        label_key: "shortcut_forward_char" },
    ActionDef { action: EditorAction::BackwardChar, id: "backward_char",       label_key: "shortcut_backward_char" },
    ActionDef { action: EditorAction::NextLine, id: "next_line",           label_key: "shortcut_next_line" },
    ActionDef { action: EditorAction::PreviousLine, id: "previous_line",       label_key: "shortcut_previous_line" },
    ActionDef { action: EditorAction::ForwardWord, id: "forward_word",        label_key: "shortcut_forward_word" },
    ActionDef { action: EditorAction::BackwardWord, id: "backward_word",       label_key: "shortcut_backward_word" },
    ActionDef { action: EditorAction::BeginningOfBuffer, id: "beginning_of_buffer", label_key: "shortcut_bob" },
    ActionDef { action: EditorAction::EndOfBuffer, id: "end_of_buffer",       label_key: "shortcut_eob" },
    ActionDef { action: EditorAction::ScrollDown, id: "scroll_down",         label_key: "shortcut_scroll_down" },
    ActionDef { action: EditorAction::ScrollUp, id: "scroll_up",           label_key: "shortcut_scroll_up" },
    ActionDef { action: EditorAction::Recenter, id: "recenter",            label_key: "shortcut_recenter" },
    ActionDef { action: EditorAction::BackwardSentence, id: "backward_sentence",   label_key: "shortcut_backward_sentence" },
    ActionDef { action: EditorAction::ForwardSentence, id: "forward_sentence",    label_key: "shortcut_forward_sentence" },
    ActionDef { action: EditorAction::BackwardParagraph, id: "backward_paragraph",  label_key: "shortcut_backward_paragraph" },
    ActionDef { action: EditorAction::ForwardParagraph, id: "forward_paragraph",   label_key: "shortcut_forward_paragraph" },
    ActionDef { action: EditorAction::BackToIndentation, id: "back_to_indentation", label_key: "shortcut_back_to_indentation" },
    ActionDef { action: EditorAction::PreviousHeading, id: "previous_heading",    label_key: "shortcut_previous_heading" },
    ActionDef { action: EditorAction::NextHeading, id: "next_heading",        label_key: "shortcut_next_heading" },
    ActionDef { action: EditorAction::BackwardHeading, id: "backward_heading", label_key: "shortcut_backward_heading" },
    ActionDef { action: EditorAction::ForwardHeading, id: "forward_heading",   label_key: "shortcut_forward_heading" },
    ActionDef { action: EditorAction::BackwardElement, id: "backward_element", label_key: "shortcut_backward_element" },
    ActionDef { action: EditorAction::ForwardElement, id: "forward_element",   label_key: "shortcut_forward_element" },
    ActionDef { action: EditorAction::UpHeading, id: "up_heading",             label_key: "shortcut_up_heading" },
    ActionDef { action: EditorAction::OpenLink, id: "open_link",           label_key: "shortcut_open_link" },
    ActionDef { action: EditorAction::SetMark, id: "set_mark",            label_key: "shortcut_set_mark" },
    ActionDef { action: EditorAction::KeyboardQuit, id: "keyboard_quit",       label_key: "shortcut_keyboard_quit" },
    ActionDef { action: EditorAction::Find, id: "find",                label_key: "shortcut_find" },
    ActionDef { action: EditorAction::FindPrevious, id: "find_previous",       label_key: "shortcut_find_previous" },
    ActionDef { action: EditorAction::Replace, id: "replace",             label_key: "shortcut_replace" },
    ActionDef { action: EditorAction::TableNextCell, id: "table_next_cell",     label_key: "shortcut_table_next_cell" },
    ActionDef { action: EditorAction::TablePrevCell, id: "table_prev_cell",     label_key: "shortcut_table_prev_cell" },
    ActionDef { action: EditorAction::GlobalCycle, id: "global_cycle",         label_key: "shortcut_global_cycle" },
    ActionDef { action: EditorAction::InsertLink, id: "insert_link",           label_key: "shortcut_insert_link" },
    ActionDef { action: EditorAction::InsertDate, id: "insert_date",           label_key: "shortcut_insert_date" },
    ActionDef { action: EditorAction::InsertInactiveDate, id: "insert_inactive_date", label_key: "shortcut_insert_inactive_date" },
    ActionDef { action: EditorAction::InsertFootnote, id: "insert_footnote", label_key: "shortcut_insert_footnote" },
    ActionDef { action: EditorAction::Bold, id: "bold",                label_key: "shortcut_bold" },
    ActionDef { action: EditorAction::Italic, id: "italic",              label_key: "shortcut_italic" },
    ActionDef { action: EditorAction::Underline, id: "underline",           label_key: "shortcut_underline" },
    ActionDef { action: EditorAction::Strikethrough, id: "strikethrough",       label_key: "shortcut_strikethrough" },
    ActionDef { action: EditorAction::Quote, id: "quote",               label_key: "shortcut_quote" },
    ActionDef { action: EditorAction::Comment, id: "comment",           label_key: "shortcut_comment" },
    ActionDef { action: EditorAction::TodoNext, id: "todo_next",           label_key: "shortcut_todo_next" },
    ActionDef { action: EditorAction::TodoPrev, id: "todo_prev",           label_key: "shortcut_todo_prev" },
    ActionDef { action: EditorAction::Schedule, id: "schedule",            label_key: "shortcut_schedule" },
    ActionDef { action: EditorAction::Deadline, id: "deadline",            label_key: "shortcut_deadline" },
    ActionDef { action: EditorAction::PromoteSubtree, id: "promote_subtree", label_key: "shortcut_promote_subtree" },
    ActionDef { action: EditorAction::DemoteSubtree, id: "demote_subtree",   label_key: "shortcut_demote_subtree" },
    ActionDef { action: EditorAction::MoveLineUp, id: "move_line_up",       label_key: "shortcut_move_line_up" },
    ActionDef { action: EditorAction::MoveLineDown, id: "move_line_down",   label_key: "shortcut_move_line_down" },
    ActionDef { action: EditorAction::SpellNext, id: "spell_next",       label_key: "shortcut_spell_next" },
];

/// Preset ids available in the UI (VI intentionally left as a future addition).
pub const EDITOR_PRESETS: &[&str] = &["classic", "emacs"];

pub fn is_builtin(id: &str) -> bool {
    EDITOR_PRESETS.contains(&id)
}
