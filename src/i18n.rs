#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum Lang {
    #[default]
    Fr,
    En,
}

impl Lang {
    pub fn from_str(s: &str) -> Self {
        match s {
            "en" => Self::En,
            _ => Self::Fr,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Fr => "fr",
        }
    }
}

pub fn t(key: &'static str, lang: Lang) -> &'static str {
    match (key, lang) {
        // Sidebar
        ("pages",            Lang::En) => "Pages",
        ("pages",            Lang::Fr) => "Pages",
        ("filter_hint",      Lang::En) => "Filter...",
        ("filter_hint",      Lang::Fr) => "Filtrer...",
        ("new_page_title",   Lang::En) => "New page",
        ("new_page_title",   Lang::Fr) => "Nouvelle page",
        ("new_page_prompt",  Lang::En) => "New page name:",
        ("new_page_prompt",  Lang::Fr) => "Nom de la nouvelle page :",
        ("new_page_ph",      Lang::En) => "Page name...",
        ("new_page_ph",      Lang::Fr) => "Nom de la page...",
        ("create",           Lang::En) => "Create",
        ("create",           Lang::Fr) => "Créer",
        ("no_vault",         Lang::En) => "No vault selected — open Settings to configure a folder.",
        ("no_vault",         Lang::Fr) => "Aucun dossier sélectionné — ouvrez les Paramètres pour en configurer un.",
        ("settings_btn",     Lang::En) => "⚙ Settings",
        ("settings_btn",     Lang::Fr) => "⚙ Paramètres",
        // Editor
        ("no_file",          Lang::En) => "No file open.",
        ("no_file",          Lang::Fr) => "Aucun fichier ouvert.",
        ("open_hint",        Lang::En) => "Select a page in the sidebar.",
        ("open_hint",        Lang::Fr) => "Sélectionnez une page dans la barre latérale.",
        ("unsaved",          Lang::En) => "Unsaved changes",
        ("unsaved",          Lang::Fr) => "Modifications non sauvegardées",
        // Tabs / toolbar
        ("export",           Lang::En) => "Export",
        ("export",           Lang::Fr) => "Exporter",
        ("export_title",     Lang::En) => "Export with logseq-site-builder",
        ("export_title",     Lang::Fr) => "Exporter avec logseq-site-builder",
        // Backlinks
        ("backlinks",        Lang::En) => "Backlinks",
        ("backlinks",        Lang::Fr) => "Liens entrants",
        ("no_backlinks",     Lang::En) => "No backlinks",
        ("no_backlinks",     Lang::Fr) => "Aucun lien entrant",
        // Settings modal
        ("settings",         Lang::En) => "Settings",
        ("settings",         Lang::Fr) => "Paramètres",
        ("vault_folder",     Lang::En) => "Vault folder",
        ("vault_folder",     Lang::Fr) => "Dossier du coffre",
        ("vault_hint",       Lang::En) => "/path/to/vault",
        ("vault_hint",       Lang::Fr) => "/chemin/vers/coffre",
        ("builder_path",     Lang::En) => "logseq-site-builder path",
        ("builder_path",     Lang::Fr) => "Chemin logseq-site-builder",
        ("browse",           Lang::En) => "Browse",
        ("browse",           Lang::Fr) => "Parcourir",
        ("cancel",           Lang::En) => "Cancel",
        ("cancel",           Lang::Fr) => "Annuler",
        ("save",             Lang::En) => "Save",
        ("save",             Lang::Fr) => "Sauvegarder",
        ("settings_saved",   Lang::En) => "Settings saved",
        ("settings_saved",   Lang::Fr) => "Paramètres sauvegardés",
        ("language",         Lang::En) => "Language",
        ("language",         Lang::Fr) => "Langue",
        ("lang_en",          Lang::En) => "English",
        ("lang_en",          Lang::Fr) => "Anglais",
        ("lang_fr",          Lang::En) => "French",
        ("lang_fr",          Lang::Fr) => "Français",
        // Settings modal tabs
        ("tab_general",   Lang::En) => "General",
        ("tab_general",   Lang::Fr) => "Général",
        ("tab_shortcuts", Lang::En) => "Shortcuts",
        ("tab_shortcuts", Lang::Fr) => "Raccourcis",
        // Keyboard shortcuts section
        ("shortcuts",         Lang::En) => "Keyboard shortcuts",
        ("shortcuts",         Lang::Fr) => "Raccourcis clavier",
        ("shortcut_new_page", Lang::En) => "New page",
        ("shortcut_new_page", Lang::Fr) => "Nouvelle page",
        ("shortcut_save",     Lang::En) => "Save",
        ("shortcut_save",     Lang::Fr) => "Sauvegarder",
        ("shortcut_settings", Lang::En) => "Open settings",
        ("shortcut_settings", Lang::Fr) => "Ouvrir les paramètres",
        ("shortcut_close_tab",Lang::En) => "Close tab",
        ("shortcut_close_tab",Lang::Fr) => "Fermer l'onglet",
        ("shortcut_next_tab", Lang::En) => "Next tab",
        ("shortcut_next_tab", Lang::Fr) => "Onglet suivant",
        ("shortcut_prev_tab", Lang::En) => "Previous tab",
        ("shortcut_prev_tab", Lang::Fr) => "Onglet précédent",
        ("shortcut_quick_open", Lang::En) => "Quick open",
        ("shortcut_quick_open", Lang::Fr) => "Ouverture rapide",
        ("recording",              Lang::En) => "Press a key…",
        ("recording",              Lang::Fr) => "Appuyez sur une touche…",
        // App shortcuts section header
        ("shortcut_section_app",   Lang::En) => "Application",
        ("shortcut_section_app",   Lang::Fr) => "Application",
        // Editor shortcuts section header
        ("shortcut_section_editor",Lang::En) => "Editor",
        ("shortcut_section_editor",Lang::Fr) => "Éditeur",
        // Editor preset selector
        ("editor_preset",          Lang::En) => "Preset",
        ("editor_preset",          Lang::Fr) => "Préréglage",
        ("preset_classic",         Lang::En) => "Classic",
        ("preset_classic",         Lang::Fr) => "Classique",
        ("preset_emacs",           Lang::En) => "Emacs",
        ("preset_emacs",           Lang::Fr) => "Emacs",
        // Classic editor shortcut labels
        ("shortcut_copy",          Lang::En) => "Copy",
        ("shortcut_copy",          Lang::Fr) => "Copier",
        ("shortcut_cut",           Lang::En) => "Cut",
        ("shortcut_cut",           Lang::Fr) => "Couper",
        ("shortcut_paste",         Lang::En) => "Paste",
        ("shortcut_paste",         Lang::Fr) => "Coller",
        ("shortcut_select_all",    Lang::En) => "Select all",
        ("shortcut_select_all",    Lang::Fr) => "Tout sélectionner",
        ("shortcut_redo",          Lang::En) => "Redo",
        ("shortcut_redo",          Lang::Fr) => "Rétablir",
        ("shortcut_new_heading",   Lang::En) => "New heading (same level)",
        ("shortcut_new_heading",   Lang::Fr) => "Nouveau titre (même niveau)",
        // Editor shortcut labels
        ("shortcut_kill_line",     Lang::En) => "Kill to end of line",
        ("shortcut_kill_line",     Lang::Fr) => "Couper jusqu'à la fin de ligne",
        ("shortcut_kill_word_fwd", Lang::En) => "Kill word forward",
        ("shortcut_kill_word_fwd", Lang::Fr) => "Couper le mot suivant",
        ("shortcut_kill_word_bwd", Lang::En) => "Kill word backward",
        ("shortcut_kill_word_bwd", Lang::Fr) => "Couper le mot précédent",
        ("shortcut_yank",          Lang::En) => "Yank (paste kill ring)",
        ("shortcut_yank",          Lang::Fr) => "Coller (kill ring)",
        ("shortcut_kill_region",   Lang::En) => "Kill region (cut)",
        ("shortcut_kill_region",   Lang::Fr) => "Couper la sélection",
        ("shortcut_copy_region",   Lang::En) => "Copy region",
        ("shortcut_copy_region",   Lang::Fr) => "Copier la sélection",
        ("shortcut_bol",           Lang::En) => "Beginning of line",
        ("shortcut_bol",           Lang::Fr) => "Début de ligne",
        ("shortcut_eol",           Lang::En) => "End of line",
        ("shortcut_eol",           Lang::Fr) => "Fin de ligne",
        ("shortcut_del_char_fwd",  Lang::En) => "Delete char forward",
        ("shortcut_del_char_fwd",  Lang::Fr) => "Supprimer le caractère suivant",
        ("shortcut_undo_edit",     Lang::En) => "Undo",
        ("shortcut_undo_edit",     Lang::Fr) => "Annuler",
        // Quick open
        ("quick_open_ph",    Lang::En) => "Go to page…",
        ("quick_open_ph",    Lang::Fr) => "Aller à la page…",
        ("quick_open_empty", Lang::En) => "No pages found.",
        ("quick_open_empty", Lang::Fr) => "Aucune page trouvée.",
        ("quick_open_new",   Lang::En) => "new page",
        ("quick_open_new",   Lang::Fr) => "nouvelle page",
        // Tab context menu
        ("rename",                 Lang::En) => "Rename",
        ("rename",                 Lang::Fr) => "Renommer",
        _ => key,
    }
}
