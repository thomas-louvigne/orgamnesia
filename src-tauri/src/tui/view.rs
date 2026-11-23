//! What the terminal interface draws: the columns of frames, the page (edited
//! or not, the screen split), the popups over it, the line at the bottom.

use orgamnesia_core::FileEntry;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Paragraph, Wrap};

use super::*;

impl App {
    pub(super) fn draw(&mut self, f: &mut ratatui::Frame) {
        let [main, help] = Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(f.area());
        self.total = main.width;
        // The columns of frames as the window's layout puts them, the page between them
        // Zen mode: no column, only the page
        let (left_frames, right_frames) = if self.zen { (vec![], vec![]) } else { (tui_frames(self.prefs.column(false)), tui_frames(self.prefs.column(true))) };
        self.right_column = !right_frames.is_empty() && main.width >= 100;
        let right_width = if self.right_column { self.right_split.unwrap_or((main.width / 5).clamp(24, 40)).clamp(16, main.width / 3) } else { 0 };
        self.right_x = main.width - right_width;
        let left_width = if left_frames.is_empty() { 0 } else { self.split_width(main.width) };
        let [left_col, right, right_col] = Layout::horizontal([Constraint::Length(left_width), Constraint::Fill(1), Constraint::Length(right_width)]).areas(main);
        self.frame_areas.clear();
        self.list_area = Rect::default();
        for (frames, col) in [(&left_frames, left_col), (&right_frames, right_col)] {
            if col.width == 0 { continue; }
            let heights: Vec<Constraint> = frames.iter().map(|f| match f {
                _ if f.grows() => Constraint::Fill(1),
                orgamnesia_core::Frame::Tips => Constraint::Length(6),
                _ => Constraint::Length(3),
            }).collect();
            for (frame, area) in frames.iter().zip(Layout::vertical(heights).split(col).iter()) {
                self.frame_areas.push((*frame, *area));
                self.draw_frame(f, *frame, *area);
            }
        }

        let resolver = self.resolver();
        let mark_broken = self.prefs.mark_broken_links;
        let broken = |t: &str| mark_broken && links::is_page_target(t) && resolver.resolve(links::split(t).0).is_none();
        let todo = self.prefs.todo();
        // The other window: on the same page, the text of the active one
        if let (Some(a), Some(o)) = (&self.editor, &mut self.other) && a.path == o.path { o.sync_from(a); }
        let (active, other) = match self.split_dir {
            Some(dir) if self.other.is_some() => {
                let halves = Layout::new(dir, [Constraint::Fill(1), Constraint::Fill(1)]).split(right);
                (halves[self.active_slot], halves[1 - self.active_slot])
            }
            _ => (right, Rect::default()),
        };
        self.page_area = active;
        self.other_area = other;
        self.height = active.height.saturating_sub(2);
        self.width = active.width.saturating_sub(2);
        let tags = self.prefs.tags();
        let (hide_stars, text_first, live) = (self.prefs.hide_leading_stars, self.prefs.link_text_first, self.prefs.live_preview);
        // Live preview: the markup shown on the line of the cursor only (`usize::MAX`: on none)
        let draw_of = |path: &str, cursor_line: usize| Draw {
            format: orgamnesia_core::Format::of_page(path), tags, todo: todo.as_ref(), broken: &broken, hide_stars,
            live: live.then_some(cursor_line), text_first,
        };
        if let Some(o) = &mut self.other {
            let d = draw_of(&o.path, o.line_col().0);
            draw_editor(f, o, other, &d, false);
        }
        let path = self.current_path();
        match (&mut self.editor, path) {
            (Some(e), Some(_)) => {
                let d = draw_of(&e.path, e.line_col().0);
                draw_editor(f, e, active, &d, true);
            }
            (None, Some(path)) => {
                let d = draw_of(&path, usize::MAX);
                let text = self.project.read(&path).unwrap_or_else(|e| e.to_string());
                let chosen = self.link.and_then(|i| highlight::links(&text, &d).into_iter().nth(i));
                let name = title_of(&path, &text);
                f.render_widget(Paragraph::new(highlight::render(&text, &d, chosen.as_ref(), &Default::default())).wrap(Wrap { trim: false })
                    .scroll((self.scroll, 0)).block(Block::bordered().title(format!(" {name} "))), active);
            }
            _ => f.render_widget(Block::bordered(), active),
        }
        if let (Some(s), Some(e)) = (self.suggest(), &self.editor) {
            let a = self.page_area;
            let (x, y) = ((e.screen_col() + e.gutter()).saturating_sub(e.left) as u16, e.row().saturating_sub(e.top) as u16);
            let width = s.items.iter().map(|i| i.chars().count()).max().unwrap_or(0) as u16 + 4;
            let height = s.items.len() as u16 + 2;
            // Under the cursor, else above it; within the screen
            let below = a.y + 2 + y;
            let top = if below + height <= main.y + main.height { below } else { (a.y + 1 + y).saturating_sub(height) };
            let left = (a.x + 1 + x).min((main.x + main.width).saturating_sub(width));
            let area = Rect { x: left, y: top, width: width.min(main.width), height: height.min(main.height) };
            f.render_widget(Clear, area);
            let mut state = ListState::default().with_selected(Some(s.selected));
            f.render_stateful_widget(List::new(s.items.clone()).block(Block::bordered().border_style(Style::new().fg(ratatui::style::Color::Cyan)))
                .highlight_style(Style::new().add_modifier(Modifier::REVERSED)), area, &mut state);
        }
        if let Mode::Popup(p) = &mut self.mode {
            f.render_widget(Clear, right);
            let list = !p.targets.is_empty();
            // The line chosen kept in view
            let rows = right.height.saturating_sub(2) as usize;
            if list && rows > 0 && (p.selected < p.scroll as usize || p.selected >= p.scroll as usize + rows) {
                p.scroll = p.selected.saturating_sub(rows / 2) as u16;
            }
            let text: Vec<Line> = p.lines.iter().enumerate().map(|(i, l)| {
                let s = if list && i == p.selected { Style::new().add_modifier(Modifier::REVERSED) } else { Style::new() };
                Line::from(l.as_str()).style(s)
            }).collect();
            let keys = if list { "↑↓ Enter, Esc closes" } else { "↑↓, any other key closes" };
            let empty = if p.lines.is_empty() { vec![Line::from("(nothing)")] } else { text };
            f.render_widget(Paragraph::new(empty).scroll((p.scroll, 0))
                .block(Block::bordered().title(format!(" {} — {keys} ", p.title))), right);
        }
        if let Mode::Quick(q) = &self.mode {
            let items = self.quick_list(&q.query.clone());
            let Mode::Quick(q) = &mut self.mode else { return };
            q.selected = q.selected.min(items.len().saturating_sub(1));
            let [_, area, _] = Layout::vertical([Constraint::Percentage(15), Constraint::Percentage(70), Constraint::Fill(1)]).areas(main);
            let [_, area, _] = Layout::horizontal([Constraint::Percentage(20), Constraint::Percentage(60), Constraint::Fill(1)]).areas(area);
            f.render_widget(Clear, area);
            let [input, rest] = Layout::vertical([Constraint::Length(3), Constraint::Fill(1)]).areas(area);
            f.render_widget(Paragraph::new(q.query.as_str()).block(Block::bordered().title(" Page or command (> commands only) ")), input);
            f.set_cursor_position(Position::new(input.x + 1 + q.query.chars().count() as u16, input.y + 1));
            // The commands in the accent colour, apart from the pages
            let lines: Vec<ListItem> = items.iter().map(|i| match i {
                commands::QuickItem::Page { name, open: true, .. } => ListItem::new(format!("● {name}")),
                commands::QuickItem::Page { name, .. } => ListItem::new(format!("  {name}")),
                commands::QuickItem::Create(name) => ListItem::new(format!("+ create “{name}”")).style(Style::new().fg(Color::Green)),
                commands::QuickItem::Command { id, keys } => ListItem::new(format!("› {:<28} {keys}", id.replace('_', " "))).style(Style::new().fg(Color::Cyan)),
                commands::QuickItem::Template { name, edit: false, .. } => ListItem::new(format!("▤ template: {name}")).style(Style::new().fg(Color::Cyan)),
                commands::QuickItem::Template { name, .. } => ListItem::new(format!("✎ edit template: {name}")).style(Style::new().fg(Color::Cyan)),
                commands::QuickItem::NewTemplate => ListItem::new("› new template").style(Style::new().fg(Color::Cyan)),
                commands::QuickItem::SuggestedTemplates => ListItem::new("› add suggested templates").style(Style::new().fg(Color::Cyan)),
            }).collect();
            let mut state = ListState::default().with_selected(Some(q.selected));
            f.render_stateful_widget(List::new(lines).block(Block::bordered().title(" ↑↓ Enter · Esc "))
                .highlight_style(Style::new().add_modifier(Modifier::REVERSED)), rest, &mut state);
            return;
        }
        if let Mode::Prompt(p) = &self.mode {
            let line = format!("{}{}", p.label, p.input);
            let hint = match (p.choices.is_empty(), p.completes()) {
                (false, _) => format!("   [{}]", p.choices.join(" | ")),
                (true, Some(_)) => "   (Tab: complete)".to_string(),
                _ => String::new(),
            };
            f.render_widget(Line::from(hint).style(Style::new().fg(ratatui::style::Color::DarkGray)),
                Rect { x: help.x + line.chars().count() as u16, width: help.width.saturating_sub(line.chars().count() as u16), ..help });
            f.set_cursor_position(Position::new(help.x + line.chars().count() as u16, help.y));
            f.render_widget(Line::from(line).style(Style::new().add_modifier(Modifier::BOLD)), help);
            return;
        }
        if let Mode::Replace(from, to) = &self.mode {
            let line = format!("Replace “{from}” with “{to}”? (y/Space: yes, n: no, !: all, other key: stop)");
            f.render_widget(Line::from(line).style(Style::new().add_modifier(Modifier::BOLD)), help);
            return;
        }

        // A search going on: what is searched, in place of the help
        if let Some(s) = self.editor.as_ref().and_then(|e| e.search.as_ref()) {
            let failing = !s.query.is_empty() && s.current.is_none();
            let line = format!("{}Search: {}{}   (Enter/Esc: stop, Ctrl+G: cancel)",
                if s.wrapped { "Wrapped " } else { "" }, s.query, if failing { "  [not found]" } else { "" });
            f.render_widget(Line::from(line).style(Style::new().add_modifier(Modifier::BOLD)), help);
            return;
        }
        // Zen mode: only the way out (and a message, when there is one)
        if self.zen {
            let keys = self.shortcut("zen_mode").unwrap_or_else(|| "> zen mode".into());
            let hint = format!("{keys}: leave zen mode");
            let line = if self.status.is_empty() { hint } else { format!("{}  │  {hint}", self.status) };
            f.render_widget(Line::from(line).style(Style::new().add_modifier(Modifier::DIM)), help);
            return;
        }
        let help_line = &self.help[self.editor.is_some() as usize];
        let line = if self.status.is_empty() { help_line.clone() } else { format!("{}  │  {help_line}", self.status) };
        // The git state at the right end: branch, files to commit, commits to push and to pull
        let git = self.git.as_ref().map(git_line).unwrap_or_default();
        let [help, right] = Layout::horizontal([Constraint::Fill(1), Constraint::Length(git.chars().count() as u16)]).areas(help);
        f.render_widget(Line::from(format!("Orgamnesia TUI (beta)  {line}")).style(Style::new().add_modifier(Modifier::DIM)), help);
        let warn = self.git.as_ref().is_some_and(|g| g.conflicts > 0 || g.merging);
        f.render_widget(Line::from(git).style(if warn { Style::new().fg(ratatui::style::Color::Red) } else { Style::new().fg(ratatui::style::Color::Cyan) }), right);
    }
    /// The frame `frame` of a column drawn in `area`.
    pub(super) fn draw_frame(&mut self, f: &mut ratatui::Frame, frame: orgamnesia_core::Frame, area: Rect) {
        use orgamnesia_core::Frame;
        let focused = self.frame_focus() == Some(frame);
        let cyan = Style::new().fg(ratatui::style::Color::Cyan);
        match frame {
            Frame::Pages => {
                let count = |p: &FileEntry| self.hits.as_ref().and_then(|(_, h)| h.iter().find(|(path, _)| *path == p.path)).map(|(_, n)| format!(" ({n})"));
                let names: Vec<String> = self.rows().into_iter().map(|r| match r {
                    orgamnesia_core::tree::Row::Folder { name, depth, open, pages, .. } =>
                        format!("{}{} {name}/ ({pages})", "  ".repeat(depth), if open { "▾" } else { "▸" }),
                    orgamnesia_core::tree::Row::Page { file, name, depth } =>
                        format!("{}  {name}{}", "  ".repeat(depth), count(&file).unwrap_or_default()),
                }).collect();
                let title = match (&self.hits, self.filter.is_empty()) {
                    (Some((q, _)), _) => format!(" “{q}” "),
                    (None, true) => " Pages ".to_string(),
                    (None, false) => format!(" /{} ", self.filter),
                };
                // The list has the keys when no page is edited (nor a frame chosen)
                let keys = self.editor.is_none() && self.frame_focus().is_none();
                let list = List::new(names).block(Block::bordered().title(frame_title(frame, title)).border_style(if keys { cyan } else { Style::new() }))
                    .highlight_style(Style::new().add_modifier(Modifier::REVERSED));
                self.list_area = area;
                f.render_stateful_widget(list, area, &mut self.list);
            }
            Frame::Project => {
                let name = crate::project::files::page_name(&self.project.path);
                f.render_widget(Paragraph::new(format!(" {name}")).block(Block::bordered().title(frame_title(frame, " Project ".into()))), area);
            }
            // One tip, wrapped; a click shows the next one
            Frame::Tips => {
                let tips = orgamnesia_core::tips::available(&self.prefs, true, false, |id| self.shortcut(id));
                let text = if tips.is_empty() { String::new() } else { tips[self.tip % tips.len()].clone() };
                f.render_widget(Paragraph::new(text).wrap(Wrap { trim: true }).block(Block::bordered().title(frame_title(frame, " Did you know? ".into()))), area);
            }
            Frame::Git => {
                let line = self.git.as_ref().map(git_line).unwrap_or_else(|| " not under git".into());
                f.render_widget(Paragraph::new(line).block(Block::bordered().title(frame_title(frame, " Git ".into()))), area);
            }
            _ => {
                let (title, items) = self.frame_items(frame);
                let lines: Vec<Line> = items.iter().map(|(l, t)| {
                    let s = if *t == commands::Target::None { Style::new().add_modifier(Modifier::BOLD) } else { Style::new() };
                    Line::from(l.clone()).style(s)
                }).collect();
                let lines = if lines.is_empty() { vec![Line::from("(none)").style(Style::new().fg(ratatui::style::Color::DarkGray))] } else { lines };
                let list = List::new(lines)
                    .block(Block::bordered().title(frame_title(frame, format!(" {title} "))).border_style(if focused { cyan } else { Style::new() }))
                    .highlight_style(if focused { Style::new().add_modifier(Modifier::REVERSED) } else { Style::new() });
                let state = self.frame_state.entry(frame).or_default();
                f.render_stateful_widget(list, area, state);
            }
        }
    }

    /// Where the popup is drawn: over the page, both halves of a split screen.
    pub(super) fn page_area_popup(&self) -> Rect {
        if self.other.is_some() { self.page_area.union(self.other_area) } else { self.page_area }
    }
}

/// The git state as the bottom line shows it: ` ⎇ main ●3 ↑1 ↓2`.
pub(super) fn git_line(g: &orgamnesia_core::GitStatus) -> String {
    let mut out = format!(" ⎇ {}", g.branch.as_deref().unwrap_or("(detached)"));
    if g.changes > 0 { out.push_str(&format!(" ●{}", g.changes)); }
    if g.ahead > 0 { out.push_str(&format!(" ↑{}", g.ahead)); }
    if g.behind > 0 { out.push_str(&format!(" ↓{}", g.behind)); }
    if g.conflicts > 0 { out.push_str(&format!(" ✗{} conflicts", g.conflicts)); }
    if g.merging { out.push_str(" (merging)"); }
    out.push(' ');
    out
}

/// The title of a frame in its colour, bold: each frame known at a glance.
pub(super) fn frame_title(frame: orgamnesia_core::Frame, title: String) -> Line<'static> {
    use orgamnesia_core::Frame;
    use ratatui::style::Color;
    let color = match frame {
        Frame::Pages => Color::Cyan,
        Frame::Project => Color::Magenta,
        Frame::Git => Color::Green,
        Frame::Backlinks => Color::Blue,
        Frame::Tags => Color::Indexed(141),
        Frame::Todos => Color::Red,
        Frame::Broken => Color::Yellow,
        _ => Color::Reset,
    };
    Line::from(title).style(Style::new().fg(color).add_modifier(Modifier::BOLD))
}

/// The frames of a column the terminal draws: the lists and the lines of the
/// window and its tips, not its buttons (brand, settings, quit, graph, org-bloom).
pub(super) fn tui_frames(frames: Vec<orgamnesia_core::Frame>) -> Vec<orgamnesia_core::Frame> {
    use orgamnesia_core::Frame;
    frames.into_iter().filter(|f| matches!(f, Frame::Project | Frame::Git | Frame::Pages | Frame::Backlinks | Frame::Tags | Frame::Todos | Frame::Broken | Frame::Tips)).collect()
}

/// What the title of a window shows of the page `path`: the title written in
/// it (`#+TITLE:`, Markdown's `title:`), as the window's title bar, else its name.
pub(super) fn title_of(path: &str, text: &str) -> String {
    orgamnesia_core::Format::of_page(path).title(text).unwrap_or_else(|| crate::project::files::page_name(path))
}

/// The editor `e` in `area`, with the cursor when it is the active window. A
/// file of the settings is drawn as YAML.
pub(super) fn draw_editor(f: &mut ratatui::Frame, e: &mut Editor, area: Rect, d: &Draw, active: bool) {
    let gutter = e.gutter();
    let (height, width) = (area.height.saturating_sub(2) as usize, (area.width.saturating_sub(2) as usize).saturating_sub(gutter));
    e.follow_cursor(height, width);
    let found = highlight::Found { all: e.matches(), current: e.search.as_ref().and_then(|s| s.current), region: e.region() };
    let text = e.text();
    let lines = if e.page { highlight::render(&text, d, None, &found) } else { highlight::render_yaml(&text, &found) };
    // The lines hidden by folds left out, the folded headlines ending with …
    let dim = Style::new().fg(ratatui::style::Color::DarkGray);
    let lines: Vec<Line> = lines.into_iter().zip(e.lines_shown()).enumerate()
        .filter(|(_, (_, (hidden, _)))| !hidden)
        .map(|(n, (mut l, (_, folded)))| {
            if folded { l.spans.push(Span::styled(" …", dim)); }
            if gutter > 0 { l.spans.insert(0, Span::styled(format!("{:>w$} ", n + 1, w = gutter - 1), dim)); }
            l
        })
        .collect();
    if active {
        let (x, y) = (e.screen_col() - e.left + gutter, e.row() - e.top);
        f.set_cursor_position(Position::new(area.x + 1 + x as u16, area.y + 1 + y as u16));
    }
    let name = if e.page { title_of(&e.path, &text) } else { crate::project::files::page_name(&e.path) };
    let title = format!(" {name}{} ", if e.dirty { " ●" } else { "" });
    let border = if active { Style::new() } else { Style::new().fg(ratatui::style::Color::DarkGray) };
    f.render_widget(Paragraph::new(lines).scroll((e.top as u16, e.left as u16)).block(Block::bordered().title(title).border_style(border)), area);
}
