//! A page drawn in colour in the terminal: its lines cut into pieces by
//! `orgamnesia_core::highlight`, as the window's editor cuts them, each piece
//! given the colours of its class in the window's stylesheet (`.h1`, `.link-t`,
//! `.em-b`… in `public/css/panels.css`).

use orgamnesia_core::highlight::{self as hl, Node};
use orgamnesia_core::{links, Format, TagSyntax, TodoKeywords};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// A link of the page: its line, its char range in it, what it leads to.
#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub line: usize,
    pub start: usize,
    pub end: usize,
    pub target: String,
}

/// How the page is read and drawn.
pub struct Draw<'a> {
    pub format: Format,
    pub tags: TagSyntax,
    pub todo: Option<&'a TodoKeywords>,
    /// Whether a link target leads nowhere.
    pub broken: &'a dyn Fn(&str) -> bool,
    /// The stars of a headline but the last nearly hidden (`hide_leading_stars`).
    pub hide_stars: bool,
    /// Live preview (`live_preview`): the markup fades away but on this line
    /// (`usize::MAX`: on every line).
    pub live: Option<usize>,
    /// A link's text in bold, its target dimmed (`link_text_first`).
    pub text_first: bool,
}

/// The colour of the markup faded away (live preview).
const FADED: Color = Color::Indexed(238);

const DIM: Style = Style::new().fg(Color::DarkGray);

/// The lines of a Markdown page drawn as they are (code blocks, front matter), with their class.
fn verbatim(lines: &[&str], text: &str, format: Format) -> std::collections::HashMap<usize, &'static str> {
    if format == Format::Markdown { hl::code_lines(lines, text) } else { Default::default() }
}

/// The links of the page `text`, in order.
pub fn links(text: &str, d: &Draw) -> Vec<Link> {
    let lines: Vec<&str> = text.split('\n').collect();
    let code = verbatim(&lines, text, d.format);
    lines.iter().enumerate().filter(|(n, _)| !code.contains_key(n)).flat_map(|(n, line)| {
        let chars: Vec<char> = line.chars().collect();
        links::scan(&chars, d.tags.links(d.format)).into_iter()
            .map(move |f| Link { line: n, start: f.start, end: f.end, target: f.target })
    }).collect()
}

/// What a search found in the page: the char ranges of the matches, and the
/// one the cursor is on.
#[derive(Default)]
pub struct Found {
    pub all: Vec<(usize, usize)>,
    pub current: Option<(usize, usize)>,
    /// The region of the editor (from the mark to the cursor).
    pub region: Option<(usize, usize)>,
}

/// The page `text` in colour; `selected`, a link of it, drawn reversed; the
/// matches of a search marked.
pub fn render(text: &str, d: &Draw, selected: Option<&Link>, found: &Found) -> Vec<Line<'static>> {
    let lines: Vec<&str> = text.split('\n').collect();
    let code = verbatim(&lines, text, d.format);
    let heads = hl::table_heads(&lines);
    let cut = hl::Draw { format: d.format, tags: d.tags, todo: d.todo.cloned(), broken: Some(Box::new(|t: &str| (d.broken)(t))) };
    let mut start = 0;
    lines.iter().enumerate().map(|(n, line)| {
        let chars: Vec<char> = line.chars().collect();
        let mut paint = Paint { d, st: vec![Style::new(); chars.len()], markup: vec![false; chars.len()], at: 0 };
        match code.get(&n) {
            Some(class) => paint.nodes(&[Node::Span { class: class.to_string(), data: None, children: vec![Node::Text(line.to_string())] }], &mut vec![], false),
            None if heads.contains(&n) => paint.nodes(&[Node::Span { class: "tbl tbl-head".into(), data: None, children: vec![Node::Text(line.to_string())] }], &mut vec![], false),
            None => paint.nodes(&hl::line(line, &cut), &mut vec![], false),
        }
        let Paint { mut st, markup, .. } = paint;
        if let Some(l) = selected.filter(|l| l.line == n) {
            for s in &mut st[l.start.min(chars.len())..l.end.min(chars.len())] { *s = s.add_modifier(Modifier::REVERSED); }
        }
        // Live preview: the markup faded away out of the line of the cursor
        if d.live.is_some_and(|cur| cur != n) {
            for (s, _) in st.iter_mut().zip(&markup).filter(|(_, m)| **m) { *s = s.fg(FADED); }
        }
        mark_found(&mut st, start, found);
        start += chars.len() + 1;
        to_line(&chars, &st)
    }).collect()
}

/// The pieces of a line painted: the style of each char, and whether it is markup.
struct Paint<'a, 'b> {
    d: &'a Draw<'b>,
    st: Vec<Style>,
    markup: Vec<bool>,
    at: usize,
}

impl Paint<'_, '_> {
    /// `nodes` within the spans of the classes `classes` (outer first); `text`:
    /// within a link that has a text (`[[target][text]]`).
    fn nodes(&mut self, nodes: &[Node], classes: &mut Vec<String>, text: bool) {
        for node in nodes {
            match node {
                Node::Text(t) => {
                    let style = classes.iter().fold(Style::new(), |s, c| self.style(s, c, text));
                    let markup = classes.iter().any(|c| c == "mk" || c == "h-stars");
                    for _ in t.chars() {
                        if self.at < self.st.len() { (self.st[self.at], self.markup[self.at]) = (style, markup); }
                        self.at += 1;
                    }
                }
                Node::Span { class, children, .. } => {
                    let has_text = text || (class.starts_with("link") && children.iter().any(|c| matches!(c, Node::Span { class, .. } if class == "link-d")));
                    let n = classes.len();
                    classes.extend(class.split(' ').map(str::to_string));
                    self.nodes(children, classes, has_text);
                    classes.truncate(n);
                }
            }
        }
    }

    /// `s` with the colours of the class `class` (`public/css/panels.css`).
    fn style(&self, s: Style, class: &str, link_text: bool) -> Style {
        let first = self.d.text_first;
        match class {
            "h1" => s.fg(Color::Red).add_modifier(Modifier::BOLD),
            "h2" => s.fg(Color::Blue).add_modifier(Modifier::BOLD),
            "h3" => s.fg(Color::Yellow),
            "h4" => s.fg(Color::Cyan),
            "h5" => s.fg(Color::Green),
            "h6" => s.fg(Color::Indexed(208)),
            "h-done" => s.fg(Color::DarkGray),
            "h-stars" => s.add_modifier(Modifier::DIM),
            // org-hide-leading-stars: the level reads as an indent
            "h-lead" if self.d.hide_stars => s.fg(Color::Indexed(237)),
            "todo" => s.fg(Color::Red).add_modifier(Modifier::BOLD | Modifier::REVERSED),
            "done" => s.fg(Color::Green).add_modifier(Modifier::BOLD | Modifier::REVERSED),
            "tags" => s.fg(Color::Magenta),
            "tag-t" => s.add_modifier(Modifier::UNDERLINED),
            "kw" => s.fg(Color::DarkGray),
            "fn" => s.fg(Color::Magenta),
            "fn-def" => s.add_modifier(Modifier::BOLD),
            "tbl" => s.fg(Color::Cyan),
            "tbl-head" => s.add_modifier(Modifier::BOLD),
            "em-b" => s.add_modifier(Modifier::BOLD),
            "em-i" => s.fg(Color::Green).add_modifier(Modifier::ITALIC),
            "em-u" => s.fg(Color::Blue).add_modifier(Modifier::UNDERLINED),
            "em-s" => s.add_modifier(Modifier::CROSSED_OUT),
            "md-code" => s.fg(Color::Green),
            "ts" => s.fg(Color::DarkGray),
            "ts-off" => s.add_modifier(Modifier::DIM),
            "ts-date" => s.fg(Color::Indexed(208)),
            "ts-day" => s.fg(Color::Magenta),
            "ts-time" => s.fg(Color::Green).add_modifier(Modifier::BOLD),
            "md-quote" => s.fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
            // A link: its text first (link_text_first) dims the rest of it
            "link" => s.fg(if first { Color::DarkGray } else { Color::Cyan }),
            "broken" => s.fg(Color::Red),
            "link-t" if first && !link_text => s.fg(Color::Cyan).add_modifier(Modifier::BOLD),
            "link-t" => s.add_modifier(Modifier::UNDERLINED),
            "link-d" if first => s.fg(Color::Cyan).add_modifier(Modifier::BOLD),
            "link-d" => s.add_modifier(Modifier::UNDERLINED),
            _ => s,
        }
    }
}

/// The region, then the matches of a line starting at `start` of the text, by their range in it.
fn mark_found(st: &mut [Style], start: usize, found: &Found) {
    let end = start + st.len();
    if let Some((a, b)) = found.region.filter(|(a, b)| *a < end && *b > start) {
        for s in &mut st[a.max(start) - start..b.min(end) - start] { *s = s.bg(Color::Indexed(238)); }
    }
    for &(a, b) in found.all.iter().filter(|(a, b)| *a < end && *b > start) {
        let bg = if found.current == Some((a, b)) { Color::LightRed } else { Color::Yellow };
        for s in &mut st[a.max(start) - start..b.min(end) - start] { *s = s.bg(bg).fg(Color::Black); }
    }
}

/// A configuration file (YAML) in colour, as the window draws it
/// (`src/highlight/yaml.rs`, `.cfg-*` in `public/css/lists.css`), its pieces cut by the same
/// `yaml_tokens`: keys, strings, numbers, `true` / `false` / `null`,
/// punctuation, comments.
pub fn render_yaml(text: &str, found: &Found) -> Vec<Line<'static>> {
    use orgamnesia_core::config::{yaml_tokens, Token};
    let mut start = 0;
    text.split('\n').map(|line| {
        let chars: Vec<char> = line.chars().collect();
        let mut st = vec![Style::new(); chars.len()];
        for (a, b, token) in yaml_tokens(line) {
            st[a..b].fill(match token {
                Token::Key => Style::new().fg(Color::Blue),
                Token::Str => Style::new().fg(Color::Green),
                Token::Number => Style::new().fg(Color::Indexed(208)),
                Token::Literal => Style::new().fg(Color::Cyan),
                Token::Punct => DIM,
                Token::Comment => DIM.add_modifier(Modifier::ITALIC),
            });
        }
        mark_found(&mut st, start, found);
        start += chars.len() + 1;
        to_line(&chars, &st)
    }).collect()
}

/// The chars of a line with their styles, as spans of the same style.
fn to_line(chars: &[char], st: &[Style]) -> Line<'static> {
    let mut spans: Vec<Span> = vec![];
    let mut i = 0;
    while i < chars.len() {
        let j = (i..chars.len()).find(|&j| st[j] != st[i]).unwrap_or(chars.len());
        spans.push(Span::styled(chars[i..j].iter().collect::<String>(), st[i]));
        i = j;
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draw() -> (TagSyntax, TodoKeywords) {
        let p = orgamnesia_core::Settings::default().prefs();
        (p.tags(), p.todo().unwrap_or_else(|| TodoKeywords::parse("TODO | DONE")))
    }

    /// The style of the first piece of `line` holding `text`.
    fn styled(line: &Line, text: &str) -> Style {
        line.spans.iter().find(|s| s.content.contains(text)).map(|s| s.style).unwrap_or_else(|| panic!("{text} not in {line:?}"))
    }

    #[test]
    fn draws_org_as_the_window() {
        let (tags, todo) = draw();
        let broken = |t: &str| t == "Nulle";
        let d = Draw { format: Format::Org, tags, todo: Some(&todo), broken: &broken, hide_stars: false, live: None, text_first: false };
        let text = "* TODO Titre\nun *gras* et [[Page]] [[Nulle]]\n| a | b |";
        let lines = render(text, &d, None, &Found::default());
        assert_eq!(styled(&lines[0], "TODO").fg, Some(Color::Red));
        assert_eq!(styled(&lines[0], " Titre").fg, Some(Color::Red));
        assert!(styled(&lines[1], "gras").add_modifier.contains(Modifier::BOLD));
        assert_eq!(styled(&lines[1], "Page").fg, Some(Color::Cyan));
        assert!(styled(&lines[1], "Page").add_modifier.contains(Modifier::UNDERLINED));
        assert_eq!(styled(&lines[1], "Nulle").fg, Some(Color::Red));
        assert_eq!(styled(&lines[2], "| a | b |").fg, Some(Color::Cyan));
        let found = links(text, &d);
        assert_eq!(found.iter().map(|l| l.target.as_str()).collect::<Vec<_>>(), ["Page", "Nulle"]);
        let sel = render(text, &d, Some(&found[0]), &Found::default());
        assert!(styled(&sel[1], "Page").add_modifier.contains(Modifier::REVERSED));
    }

    #[test]
    fn markdown_code_is_left_alone() {
        let (tags, _) = draw();
        let d = Draw { format: Format::Markdown, tags, todo: None, broken: &|_| false, hide_stars: false, live: None, text_first: false };
        let text = "## Titre\n```\n**pas gras** [[X]]\n```\n`code` **gras**";
        let lines = render(text, &d, None, &Found::default());
        assert_eq!(styled(&lines[0], " Titre").fg, Some(Color::Blue));
        assert_eq!(styled(&lines[2], "**pas gras** [[X]]").fg, Some(Color::Green));
        assert!(links(text, &d).is_empty());
        assert_eq!(styled(&lines[4], "`code`").fg, Some(Color::Green));
        assert!(styled(&lines[4], "gras").add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn marks_the_matches() {
        let (tags, _) = draw();
        let d = Draw { format: Format::Org, tags, todo: None, broken: &|_| false, hide_stars: false, live: None, text_first: false };
        let found = Found { all: vec![(0, 2), (6, 8)], current: Some((6, 8)), region: None };
        let lines = render("ab cd\nab", &d, None, &found);
        assert_eq!(styled(&lines[0], "ab").bg, Some(Color::Yellow));
        assert_eq!(styled(&lines[1], "ab").bg, Some(Color::LightRed));
    }

    #[test]
    fn yaml_as_the_window_draws_it() {
        let lines = render_yaml("# terminal : non\ntheme: day\ntrash_keep_days: 14\nautosave: true", &Found::default());
        assert_eq!(styled(&lines[0], "# terminal : non").fg, Some(Color::DarkGray));
        assert_eq!(styled(&lines[1], "theme").fg, Some(Color::Blue));
        assert_eq!(styled(&lines[1], "day").fg, Some(Color::Green));
        assert_eq!(styled(&lines[2], "14").fg, Some(Color::Indexed(208)));
        assert_eq!(styled(&lines[3], "true").fg, Some(Color::Cyan));
    }

    #[test]
    fn live_preview_and_link_text_first() {
        let (tags, _) = draw();
        let d = Draw { format: Format::Org, tags, todo: None, broken: &|_| false, hide_stars: false, live: Some(1), text_first: true };
        let text = "** Titre\nun *gras* [[Page][texte]]\nvoir [[Autre]]";
        let lines = render(text, &d, None, &Found::default());
        // Faded away out of the line of the cursor
        assert_eq!(styled(&lines[0], "**").fg, Some(FADED));
        assert_eq!(styled(&lines[2], "[[").fg, Some(FADED));
        // On it, the markup as it is (in the colours of what it marks)
        assert!(styled(&lines[1], "*gras*").add_modifier.contains(Modifier::BOLD));
        // The text of a link in bold, its target dimmed
        assert!(styled(&lines[1], "texte").add_modifier.contains(Modifier::BOLD));
        assert_eq!(styled(&lines[1], "Page").fg, Some(Color::DarkGray));
        // A link without text: its target is its text
        assert!(styled(&lines[2], "Autre").add_modifier.contains(Modifier::BOLD));
    }
}
