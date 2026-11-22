//! "Did you know?": the tips on the features, shared by the window and the
//! terminal. `{key}` stands for the shortcut of the action, in the active profile.

use crate::Prefs;

pub struct Tip {
    /// The action whose shortcut replaces `{key}`; the tip is left out while it has none.
    pub action: Option<&'static str>,
    pub en: &'static str,
    pub fr: &'static str,
    /// Where it makes sense: the window, the terminal.
    pub window: bool,
    pub terminal: bool,
    /// Whether the feature is on with these settings.
    pub on: fn(&Prefs) -> bool,
}

const fn tip(action: Option<&'static str>, en: &'static str, fr: &'static str, window: bool, terminal: bool, on: fn(&Prefs) -> bool) -> Tip {
    Tip { action, en, fr, window, terminal, on }
}

pub const TIPS: &[Tip] = &[
    tip(Some("quick_open"), "Type > in {key}: every command, with its shortcut.",
        "Tapez > dans {key} : toutes les commandes, avec leur raccourci.", true, true, |_| true),
    tip(Some("quick_open"), "Type # in {key} to go to a tag.",
        "Tapez # dans {key} pour aller à un tag.", true, true, |p| p.show_tags),
    tip(Some("switch_project"), "{key} switches project; arrows and Enter choose it.",
        "{key} change de projet ; les flèches et Entrée le choisissent.", true, false, |p| p.show_project),
    tip(Some("switch_project"), "{key} switches project.",
        "{key} change de projet.", false, true, |_| true),
    tip(Some("show_shortcuts"), "{key} shows every shortcut, with a search.",
        "{key} affiche tous les raccourcis, avec une recherche.", true, false, |_| true),
    tip(Some("show_shortcuts"), "{key} shows every shortcut.",
        "{key} affiche tous les raccourcis.", false, true, |_| true),
    tip(Some("project_search"), "{key} searches the whole project (regular expressions too).",
        "{key} cherche dans tout le projet (expressions régulières aussi).", true, true, |_| true),
    tip(Some("add_tag"), "{key} adds a tag to the page.",
        "{key} ajoute un tag à la page.", true, false, |_| true),
    tip(Some("split_vertical"), "{key} splits the editor to see two pages side by side.",
        "{key} coupe l'éditeur pour voir deux pages côte à côte.", true, true, |_| true),
    tip(Some("page_back"), "{key} goes back to the page seen before.",
        "{key} revient à la page vue avant.", true, true, |_| true),
    tip(Some("graph"), "{key} opens the graph view: the pages and their links.",
        "{key} ouvre la vue graphe : les pages et leurs liens.", true, false, |p| p.graph),
    tip(Some("git_commit"), "{key} commits the changes of the project.",
        "{key} commite les changements du projet.", true, true, |p| p.git_ext),
    tip(Some("export_pdf"), "{key} exports the page to PDF.",
        "{key} exporte la page en PDF.", true, false, |_| true),
    tip(Some("random_page"), "{key} opens a page picked at random: old notes come back.",
        "{key} ouvre une page au hasard : de vieilles notes remontent.", true, true, |_| true),
    tip(Some("zen_mode"), "{key} switches to zen mode: only the page, full screen.",
        "{key} passe en mode zen : seulement la page, en plein écran.", true, false, |_| true),
    tip(Some("zen_mode"), "{key} switches to zen mode: only the page.",
        "{key} passe en mode zen : seulement la page.", false, true, |_| true),
    tip(None, "Orgamnesia also runs in a terminal: orgamnesia -nw [folder].",
        "Orgamnesia tourne aussi dans un terminal : orgamnesia -nw [dossier].", true, false, |_| true),
    tip(None, "The frames around the editor can be moved or hidden: Settings, Display.",
        "Les cadres autour de l'éditeur se déplacent ou se masquent : Réglages, Affichage.", true, false, |_| true),
    tip(None, "A button clicked recalls its shortcut in the status bar (Settings, Shortcuts).",
        "Un bouton cliqué rappelle son raccourci dans la barre d'état (Réglages, Raccourcis).", true, false, |p| p.shortcut_tips),
];

/// The texts of the tips that make sense here (`terminal` or the window) with
/// `prefs`, in French or English, `{key}` replaced by `shortcut` of their action.
pub fn available(prefs: &Prefs, terminal: bool, french: bool, shortcut: impl Fn(&str) -> Option<String>) -> Vec<String> {
    TIPS.iter()
        .filter(|t| (if terminal { t.terminal } else { t.window }) && (t.on)(prefs))
        .filter_map(|t| {
            let text = if french { t.fr } else { t.en };
            match t.action {
                Some(id) => shortcut(id).map(|k| text.replace("{key}", &k)),
                None => Some(text.to_string()),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keybindings::AppAction;

    /// Each tip naming an action says where its shortcut goes, in both languages.
    #[test]
    fn tips_name_real_actions() {
        for t in TIPS {
            for text in [t.en, t.fr] {
                assert_eq!(text.contains("{key}"), t.action.is_some(), "{text}");
            }
            if let Some(id) = t.action { assert!(AppAction::from_id(id).is_some(), "{id}"); }
        }
        let prefs = crate::Settings::default().prefs();
        let tui = available(&prefs, true, false, |id| Some(id.to_string()));
        assert!(tui.iter().any(|t| t.starts_with("random_page")) && !tui.iter().any(|t| t.contains("-nw")));
        assert!(available(&prefs, false, true, |_| None).iter().any(|t| t.contains("-nw")));
    }
}
