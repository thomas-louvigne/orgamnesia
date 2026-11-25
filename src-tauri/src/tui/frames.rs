//! The window over the page (shortcuts, history, links…) and the frames of the
//! window shown in it (backlinks and unlinked mentions, tags, TODO, broken
//! links): what each line leads to, and opening it.

use orgamnesia_core::{AgendaHit, Frame};

use super::{git::short, App, Mode};

/// Text shown over the page (the shortcuts, the history of the page…); with
/// `targets`, a list to pick from (TODO, tags, links), the line `selected`.
#[derive(Default)]
pub struct Popup {
    /// The agenda (`agenda`): its first day, the headline of each line.
    pub agenda: Option<(i64, Vec<Option<AgendaHit>>)>,
    pub title: String,
    pub lines: Vec<String>,
    pub scroll: u16,
    pub targets: Vec<Target>,
    pub selected: usize,
    /// The keys it takes, shown in its title (`None`: those of any text or list).
    pub keys: Option<&'static str>,
    /// A diff: its added lines in green, its removed ones in red.
    pub diff: bool,
    /// The popup it was opened from, back when it closes (a diff from the commit's files).
    pub parent: Option<Box<Popup>>,
}

/// What a line of a popup leads to.
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    /// A line (from 0) of a page.
    Page(String, usize),
    /// The line of a page naming this (a link to a page): found when opened.
    Mention(String, String),
    /// The headlines and pages of a tag.
    Tag(String),
    /// An editing action on the page (the right-click menu).
    Action(&'static str),
    /// A page in the trash, by its path there: restored.
    Trashed(String),
    /// A file changed since the last commit (relative to the root of the
    /// repository; its path before a rename): its diff shown.
    Change { file: String, old: Option<String> },
    /// A commit of the page `page` (`file` in it): its diff shown, or that version put back.
    Commit { hash: String, file: String, page: String },
    /// The page's name written in another without a link: opened there, or made a link (`l`).
    Unlinked(orgamnesia_core::MentionHit),
    /// Nothing (a title, a blank line).
    None,
}

impl Popup {
    /// Text to read, scrolled.
    pub fn text(title: String, lines: Vec<String>) -> Self {
        Self { title, lines, ..Self::default() }
    }

    /// A list to pick from: each line and what it leads to.
    pub fn pick(title: String, items: Vec<(String, Target)>) -> Self {
        let (lines, targets): (Vec<String>, Vec<Target>) = items.into_iter().unzip();
        let selected = targets.iter().position(|t| *t != Target::None).unwrap_or(0);
        Self { title, lines, targets, selected, ..Self::default() }
    }

    /// The next (previous) line leading somewhere.
    pub fn step(&mut self, forward: bool) {
        let n = self.targets.len();
        let mut i = self.selected;
        for _ in 0..n {
            i = if forward { (i + 1).min(n.saturating_sub(1)) } else { i.saturating_sub(1) };
            if self.targets[i] != Target::None { self.selected = i; return; }
        }
    }
}

impl App {
    /// What a frame of the window lists (the TODO headlines, the tags, the pages
    /// linking to the page in view, the links to pages not created), and its title.
    pub(super) fn frame_items(&self, frame: Frame) -> (String, Vec<(String, Target)>) {
        let names = self.prefs.case_insensitive_links;
        match frame {
            Frame::Todos => {
                let Some(keywords) = self.prefs.todo() else { return ("To do".into(), vec![]) };
                // The first word: which ones (as the window's list offers); the rest, a tag search
                let filter = self.todo_filter.trim();
                let (state, tags) = match filter.split_once(' ').unwrap_or((filter, "")) {
                    (w, rest) if matches!(w, "open" | "done" | "all" | "dated" | "undated") || w.chars().all(|c| c.is_uppercase()) && !w.is_empty() => (w, rest),
                    _ => ("all", filter),
                };
                let mut hits = self.project.todos(&keywords, tags.trim(), self.prefs.tags().org);
                hits.retain(|h| match state {
                    "all" => true,
                    "open" => !h.done,
                    "done" => h.done,
                    "dated" => !h.done && h.dated,
                    "undated" => !h.done && !h.dated,
                    k => h.keyword == k,
                });
                // Those to do first, by page
                hits.sort_by_key(|a| (a.done, a.page.to_lowercase(), a.line));
                let title = if filter.is_empty() { "To do".to_string() } else { format!("To do: {filter}") };
                (title, hits.into_iter().map(|h| {
                    (format!("{} {} {}  — {}", if h.done { "✓" } else { "·" }, h.keyword, h.title, h.page), Target::Page(h.path, h.line))
                }).collect())
            }
            Frame::Tags => {
                let mut tags = self.project.tags(self.prefs.tags().org);
                tags.sort_by_key(|t| t.name.to_lowercase());
                ("Tags".into(), tags.into_iter().map(|t| (format!("{} ({})", t.name, t.count), Target::Tag(t.name))).collect())
            }
            Frame::Backlinks => {
                let Some(path) = self.current_path() else { return ("Backlinks".into(), vec![]) };
                let name = crate::project::files::page_name(&path);
                let mut items: Vec<(String, Target)> = self.project.backlinks(&path, names).into_iter().map(|p| {
                    (crate::project::files::page_name(&p), Target::Mention(p, name.clone()))
                }).collect();
                let mentions = self.project.unlinked_mentions(&path, self.prefs.tags(), names);
                if !mentions.is_empty() {
                    items.push((format!("Unlinked mentions ({}), l: link", mentions.len()), Target::None));
                    items.extend(mentions.into_iter().map(|m| (format!("  {}: {}", m.page, m.context), Target::Unlinked(m))));
                }
                (format!("Linking to {name}"), items)
            }
            Frame::Broken => {
                let mut items = vec![];
                for b in self.project.broken_links(names) {
                    items.push((format!("{} ({})", b.target, b.count.max(1)), Target::None));
                    for source in b.sources {
                        items.push((format!("  ← {}", crate::project::files::page_name(&source)), Target::Mention(source, b.target.clone())));
                    }
                }
                ("Pages not created".into(), items)
            }
            _ => (String::new(), vec![]),
        }
    }

    /// The shortcut of a frame (TODO, tags, backlinks, broken links): the focus
    /// on it when a column shows it, as the window does; else it opens over the page.
    pub(super) fn panel(&mut self, frame: Frame) {
        if frame == Frame::Todos && self.prefs.todo().is_none() { self.status = "TODO keywords are off (settings)".into(); return; }
        if self.frame_shown_here(frame) {
            self.focus_frame(frame);
            return;
        }
        let (title, items) = self.frame_items(frame);
        self.mode = Mode::Popup(Popup::pick(title, items));
    }

    /// The headlines and pages holding the tag `tag`.
    pub(super) fn tag_panel(&mut self, tag: &str) {
        let items = self.project.search_tags(tag, self.prefs.tags().org).into_iter().map(|h| {
            let place = h.heading.map(|t| format!(" › {t}")).unwrap_or_default();
            (format!("{}{place}", h.page), Target::Page(h.path, h.line))
        }).collect();
        self.mode = Mode::Popup(Popup::pick(format!("Tag {tag}"), items));
    }

    /// The first line of the page `path` naming `name` (a link to it), else its first line.
    pub(super) fn line_naming(&self, path: &str, name: &str) -> usize {
        let text = self.project.read(path).unwrap_or_default().to_lowercase();
        let name = name.to_lowercase();
        text.lines().position(|l| l.contains(&name)).unwrap_or(0)
    }

    /// The line chosen in the popup opened: a page at its line, or the headlines of a tag.
    pub(super) fn open_target(&mut self) {
        if let Some(target) = self.popup().and_then(|p| p.targets.get(p.selected).cloned()) { self.open(target); }
    }

    /// Open what a line of a list leads to: a page at its line, the headlines of a tag.
    pub(super) fn open(&mut self, target: Target) {
        let (path, line) = match target {
            Target::Page(path, line) => (path, line),
            Target::Mention(path, name) => { let line = self.line_naming(&path, &name); (path, line) }
            Target::Unlinked(m) => (m.path, m.line),
            Target::Tag(tag) => return self.tag_panel(&tag),
            Target::Trashed(trashed) => return self.restore_trashed(&trashed),
            Target::Change { file, old } => {
                let diff = crate::git::diff(&self.project.path, &file, old.as_deref());
                return self.diff_popup(file, diff);
            }
            Target::Commit { hash, file, .. } => {
                let diff = crate::git::commit_diff(&self.project.path, &hash, &file);
                return self.diff_popup(format!("{} {file}", short(&hash)), diff);
            }
            Target::Action(id) => {
                self.mode = Mode::Normal;
                self.run_action(id);
                return;
            }
            Target::None => return,
        };
        self.mode = Mode::Normal;
        self.hits = None;
        self.edit(&path);
        if let Some(e) = &mut self.editor && e.path == path { e.goto_line(line); }
    }

    /// The unlinked mention `m` made a link to its page; the popup listing it
    /// lists them again.
    pub(super) fn link_mention(&mut self, m: &orgamnesia_core::MentionHit) {
        let format = orgamnesia_core::Format::of_page(&m.path);
        let syntax = self.prefs.tags().links(format);
        self.change_page(&m.path, |text| orgamnesia_core::mentions::link(text, m.start, m.end, &m.text, &m.target, format, syntax));
        self.status = format!("{} linked in {}", m.text, m.page);
        let in_popup = self.popup().is_some_and(|p| p.targets.iter().any(|t| matches!(t, Target::Unlinked(_))));
        if in_popup {
            let (_, items) = self.frame_items(Frame::Backlinks);
            if let Mode::Popup(p) = &mut self.mode {
                let selected = p.selected;
                *p = Popup::pick(std::mem::take(&mut p.title), items);
                p.selected = selected.min(p.targets.len().saturating_sub(1));
                if p.targets.get(p.selected) == Some(&Target::None) { p.step(true); }
            }
        }
    }
}
