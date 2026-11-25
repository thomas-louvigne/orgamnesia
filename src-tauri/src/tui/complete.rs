//! Completion in the terminal, as in the window's editor (the same
//! `orgamnesia_core::complete`): the names completing a `[[link`, a `#tag`, an
//! org-mode `:tag`, the file of a Markdown link or a headline (`[[Page::*Ti`),
//! under the cursor; and in the lines typed at the bottom (a tag, a link), Tab.

use orgamnesia_core::complete::{accepted, being_typed, complete_page, place_prefix, Kind};
use orgamnesia_core::links::headlines;
use orgamnesia_core::pages::file_name;

use super::{App, Mode};

/// Names shown at most.
const MAX: usize = 8;

/// The names completing what is typed between `start` and `caret` of the page edited.
pub struct Suggest {
    pub start: usize,
    pub caret: usize,
    pub kind: Kind,
    pub items: Vec<String>,
    pub selected: usize,
}

impl App {
    /// The page names the links of the project use (a twin page by its file
    /// name), and their aliases.
    pub(super) fn link_names(&self) -> Vec<String> {
        self.resolver().link_names()
    }

    /// The tags of the project.
    pub(super) fn tag_names(&self) -> Vec<String> {
        self.project.tags(self.prefs.tags().org).into_iter().map(|t| t.name).collect()
    }

    /// What is typed at the cursor of the page edited: the names completing it, or none.
    pub(super) fn refresh_suggest(&mut self) {
        match self.suggestions() {
            Some(s) => self.mode = Mode::Suggest(s),
            None => if matches!(self.mode, Mode::Suggest(_)) { self.mode = Mode::Normal; },
        }
    }

    fn suggestions(&self) -> Option<Suggest> {
        let e = self.editor.as_ref().filter(|e| e.page && e.mark.is_none())?;
        let chars: Vec<char> = e.text().chars().collect();
        let (caret, format, tags) = (e.cursor, e.format(), self.prefs.tags());
        let (start, prefix, kind) = being_typed(&chars, caret, tags, format)?;
        // A headline of a page: `[[Page::*Ti`, `[[*Ti`, `[[Page#Ti`
        let place = match kind {
            Kind::Link => place_prefix(&prefix).map(|(page, typed, at)| (page, typed, at, Kind::Place))
                .or_else(|| tags.links(format).hash.then(|| prefix.split_once('#')).flatten()
                    .filter(|(page, _)| !page.trim().is_empty())
                    .map(|(page, typed)| (page.trim().to_string(), typed.to_string(), page.chars().count() + 1, Kind::HashPlace))),
            _ => None,
        };
        if let Some((page, typed, at, kind)) = place {
            let text = if page.is_empty() { e.text() } else {
                let file = self.resolver().resolve(&page).cloned()?;
                self.project.read(&file.path).ok()?
            };
            let titles = headlines(&text, self.prefs.todo().as_ref(), orgamnesia_core::Format::of_page(&e.path));
            let typed = typed.strip_prefix('*').unwrap_or(&typed);
            let titles = if typed.trim().is_empty() { titles.into_iter().take(MAX).collect() } else { complete_page(&titles, typed, None, MAX) };
            let star = if kind == Kind::Place { "*" } else { "" };
            let items: Vec<String> = titles.into_iter().map(|t| format!("{star}{t}")).collect();
            return (!items.is_empty()).then_some(Suggest { start: start + at, caret, kind, items, selected: 0 });
        }
        let items = match kind {
            Kind::MdLink => {
                let files: Vec<String> = self.project.files().iter().map(|f| file_name(&f.path).to_string()).collect();
                complete_page(&files, &orgamnesia_core::links::percent_decode(&prefix), None, MAX)
            }
            Kind::OrgTag => complete_page(&self.tag_names(), &prefix, None, MAX),
            Kind::Hashtag => complete_page(&self.link_names(), &prefix, Some(tags.hashtags), MAX),
            _ => complete_page(&self.link_names(), &prefix, None, MAX),
        };
        (!items.is_empty()).then_some(Suggest { start, caret, kind, items, selected: 0 })
    }

    /// The name chosen put in place of what was typed of it (the link closed).
    pub(super) fn accept_suggest(&mut self) {
        if !matches!(self.mode, Mode::Suggest(_)) { return; }
        let (Mode::Suggest(s), Some(e)) = (self.normal(), &mut self.editor) else { return };
        let chars: Vec<char> = e.text().chars().collect();
        let edit = accepted(&chars, s.start, s.caret, s.kind, &s.items[s.selected]);
        e.put(edit.from, edit.to, &edit.insert);
        e.cursor = edit.cursor;
    }

    /// Tab in a line typed at the bottom: the next name completing what is typed
    /// (`names`), the same Tab again giving the one after.
    pub(super) fn complete_prompt(&mut self, names: impl FnOnce(&App) -> Vec<String>) {
        let names = names(self);
        let Mode::Prompt(p) = &mut self.mode else { return };
        if p.choices.is_empty() {
            p.choices = complete_page(&names, &p.input, None, MAX);
            // What is typed is a name already: the others after it
            if p.choices.is_empty() { return; }
            p.choice = 0;
        } else {
            p.choice = (p.choice + 1) % p.choices.len();
        }
        p.input = p.choices[p.choice].clone();
    }
}
