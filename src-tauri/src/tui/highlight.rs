//! A page drawn in colour in the terminal, as the editor of the window draws it
//! (`src/highlight.rs` and `.h1`… in `app.css`): headlines by level, TODO
//! keywords, tags, links (those leading nowhere in red), emphasis, tables,
//! Markdown code blocks and front matter.

use orgamnesia_core::{hashtags, links, Format, TagSyntax, TodoKeywords};
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

/// Where the markup of a link is (its brackets, and its target when it has a
/// text), and its text: `[[target][text]]`, `[[target]]`, `[text](target)`.
fn link_parts(chars: &[char], a: usize, b: usize) -> (Vec<(usize, usize)>, (usize, usize)) {
    let find = |pat: [char; 2], from: usize| (from..b.saturating_sub(1)).find(|&i| chars[i] == pat[0] && chars[i + 1] == pat[1]);
    if chars.get(a + 1) == Some(&'[') && b >= a + 4 {
        return match find([']', '['], a + 2) {
            Some(k) => (vec![(a, k + 2), (b - 2, b)], (k + 2, b - 2)),
            None => (vec![(a, a + 2), (b - 2, b)], (a + 2, b - 2)),
        };
    }
    match find([']', '('], a + 1) {
        Some(k) => (vec![(a, a + 1), (k, b)], (a + 1, k)),
        None => (vec![], (a, b)),
    }
}

/// The colour of the markup faded away (live preview).
const FADED: Color = Color::Indexed(238);

const DIM: Style = Style::new().fg(Color::DarkGray);
const CODE: Style = Style::new().fg(Color::Green);

/// The style of a headline of `level` (`.h1` … `.h6`).
fn heading(level: usize) -> Style {
    match level {
        1 => Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
        2 => Style::new().fg(Color::Blue).add_modifier(Modifier::BOLD),
        3 => Style::new().fg(Color::Yellow),
        4 => Style::new().fg(Color::Cyan),
        5 => Style::new().fg(Color::Green),
        _ => Style::new().fg(Color::Indexed(208)),
    }
}

/// The style of each line that is code (`CODE`) or front matter (`DIM`) in
/// Markdown, drawn as it is, no link in it.
fn verbatim_lines(lines: &[&str], format: Format) -> Vec<Option<Style>> {
    let mut out = vec![None; lines.len()];
    if format != Format::Markdown { return out; }
    let (mut fence, mut front) = (false, lines.first() == Some(&"---"));
    for (n, line) in lines.iter().enumerate() {
        let t = line.trim_start();
        let rule = t.starts_with("```") || t.starts_with("~~~");
        out[n] = if front { Some(DIM) } else if fence || rule { Some(CODE) } else { None };
        if front && n > 0 && *line == "---" { front = false; }
        if rule && !front { fence = !fence; }
    }
    out
}

/// The links of the page `text`, in order.
pub fn links(text: &str, d: &Draw) -> Vec<Link> {
    let lines: Vec<&str> = text.split('\n').collect();
    let verbatim = verbatim_lines(&lines, d.format);
    lines.iter().enumerate().filter(|(n, _)| verbatim[*n].is_none()).flat_map(|(n, line)| {
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
    let verbatim = verbatim_lines(&lines, d.format);
    let all_links = links(text, d);
    let mut start = 0;
    lines.iter().enumerate().map(|(n, line)| {
        let chars: Vec<char> = line.chars().collect();
        let mut st = vec![Style::new(); chars.len()];
        // The markup of the line: what the live preview fades away
        let mut markup = vec![false; chars.len()];
        if let Some(s) = verbatim[n] {
            st.fill(s);
        } else {
            let inline_from = style_line(&chars, &mut st, d);
            if let Some(stars) = d.format.heading_level(&chars) { markup[..stars.min(chars.len())].fill(true); }
            let line_links: Vec<&Link> = all_links.iter().filter(|l| l.line == n).collect();
            emphasis(&chars, &mut st, &mut markup, inline_from, &line_links, d.format);
            for l in line_links {
                let (a, b) = (l.start, l.end.min(chars.len()));
                let color = if (d.broken)(&l.target) { Color::Red } else { Color::Cyan };
                let chosen = |s: Style| if selected == Some(l) { s.add_modifier(Modifier::REVERSED) } else { s };
                let (marks, (ta, tb)) = link_parts(&chars, a, b);
                for (x, y) in marks { markup[x..y].fill(true); }
                if d.text_first {
                    // The text in bold, the rest (brackets, target) dimmed and underlined
                    st[a..b].fill(chosen(Style::new().fg(Color::DarkGray).add_modifier(Modifier::UNDERLINED)));
                    st[ta..tb].fill(chosen(Style::new().fg(color).add_modifier(Modifier::BOLD)));
                } else {
                    st[a..b].fill(chosen(Style::new().fg(color).add_modifier(Modifier::UNDERLINED)));
                }
            }
            if d.live.is_some_and(|cur| cur != n) {
                for (s, _) in st.iter_mut().zip(&markup).filter(|(_, m)| **m) { *s = s.fg(FADED); }
            }
        }
        mark_found(&mut st, start, found);
        start += chars.len() + 1;
        to_line(&chars, &st)
    }).collect()
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
/// (`src/highlight/yaml.rs`, `.cfg-*` in `app.css`), its pieces cut by the same
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

/// Styles of the whole line (headline, table, keyword, quote); returns where its
/// inline markup starts.
fn style_line(chars: &[char], st: &mut [Style], d: &Draw) -> usize {
    let indent = chars.iter().take_while(|c| c.is_whitespace()).count();
    let rest: String = chars[indent..].iter().collect();
    if let Some(stars) = d.format.heading_level(chars) {
        let s = heading(stars);
        st.fill(s);
        st[..stars.min(chars.len())].fill(s.add_modifier(Modifier::DIM));
        // org-hide-leading-stars: the level reads as an indent
        if d.hide_stars { st[..stars.saturating_sub(1).min(chars.len())].fill(Style::new().fg(Color::Indexed(237))); }
        let mut from = (stars + 1).min(chars.len());
        if let Some(k) = d.todo.and_then(|t| t.keyword(chars, d.format)) {
            if k.done { st[k.end..].fill(DIM); }
            let color = if k.done { Color::Green } else { Color::Red };
            st[k.start..k.end].fill(Style::new().fg(color).add_modifier(Modifier::BOLD | Modifier::REVERSED));
            from = k.end;
        }
        if let Some((a, b)) = hashtags::headline_tags_range(chars, d.tags.org) {
            st[a..b].fill(Style::new().fg(Color::Magenta));
        }
        return from;
    }
    if rest.starts_with('|') {
        st.fill(Style::new().fg(Color::Cyan));
        return chars.len();
    }
    if d.format == Format::Org && rest.starts_with("#+") {
        let colon = chars.iter().position(|&c| c == ':').map_or(chars.len(), |p| p + 1);
        st[..colon].fill(DIM);
        if let Some(v) = hashtags::filetags_value_start(chars) { st[v..].fill(Style::new().fg(Color::Magenta)); }
        return colon;
    }
    if d.format == Format::Org && (rest == "#" || rest.starts_with("# ")) {
        st.fill(DIM);
        return chars.len();
    }
    if d.format == Format::Markdown && rest.starts_with('>') {
        st.fill(DIM.add_modifier(Modifier::ITALIC));
    }
    0
}

/// Bold, italic, underlined, struck and code spans from `from`, outside the links.
fn emphasis(chars: &[char], st: &mut [Style], markup: &mut [bool], from: usize, links: &[&Link], format: Format) {
    let marks: &[(&str, Style)] = if format == Format::Markdown {
        &[("**", Style::new().add_modifier(Modifier::BOLD)), ("__", Style::new().add_modifier(Modifier::BOLD)),
          ("~~", Style::new().add_modifier(Modifier::CROSSED_OUT)), ("`", CODE),
          ("*", Style::new().fg(Color::Green).add_modifier(Modifier::ITALIC)), ("_", Style::new().fg(Color::Green).add_modifier(Modifier::ITALIC))]
    } else {
        &[("*", Style::new().add_modifier(Modifier::BOLD)), ("/", Style::new().fg(Color::Green).add_modifier(Modifier::ITALIC)),
          ("_", Style::new().fg(Color::Blue).add_modifier(Modifier::UNDERLINED)), ("+", Style::new().add_modifier(Modifier::CROSSED_OUT)),
          ("=", CODE), ("~", CODE)]
    };
    let in_link = |i: usize| links.iter().find(|l| (l.start..l.end).contains(&i)).map(|l| l.end);
    let mut i = from;
    while i < chars.len() {
        if let Some(end) = in_link(i) { i = end; continue; }
        let opens = i == 0 || chars[i - 1].is_whitespace() || "-('\"{[".contains(chars[i - 1]);
        let found = opens.then(|| marks.iter().find_map(|(m, s)| close(chars, i, m).map(|end| (m.chars().count(), end, *s)))).flatten();
        match found {
            Some((n, end, s)) if (i..end).all(|j| in_link(j).is_none()) => {
                st[i..end].fill(s);
                st[i..i + n].fill(DIM);
                st[end - n..end].fill(DIM);
                markup[i..i + n].fill(true);
                markup[end - n..end].fill(true);
                i = end;
            }
            _ => i += 1,
        }
    }
}

/// End (past the closing mark) of the span opened by `mark` at `at`, if closed on the line.
fn close(chars: &[char], at: usize, mark: &str) -> Option<usize> {
    let m: Vec<char> = mark.chars().collect();
    let n = m.len();
    let is = |i: usize| chars.get(i..i + n) == Some(&m[..]);
    if !is(at) || chars.get(at + n).is_none_or(|c| c.is_whitespace()) { return None; }
    (at + n + 1..=chars.len().saturating_sub(n)).find(|&j| {
        is(j) && !chars[j - 1].is_whitespace() && chars.get(j + n).is_none_or(|c| !c.is_alphanumeric())
    }).map(|j| j + n)
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

    fn styled(line: &Line, text: &str) -> Style {
        line.spans.iter().find(|s| s.content == text).map(|s| s.style).unwrap_or_else(|| panic!("{text} not in {line:?}"))
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
        assert_eq!(styled(&lines[1], "[[Page]]").fg, Some(Color::Cyan));
        assert_eq!(styled(&lines[1], "[[Nulle]]").fg, Some(Color::Red));
        assert_eq!(styled(&lines[2], "| a | b |").fg, Some(Color::Cyan));
        let found = links(text, &d);
        assert_eq!(found.iter().map(|l| l.target.as_str()).collect::<Vec<_>>(), ["Page", "Nulle"]);
        let sel = render(text, &d, Some(&found[0]), &Found::default());
        assert!(styled(&sel[1], "[[Page]]").add_modifier.contains(Modifier::REVERSED));
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
        assert_eq!(styled(&lines[4], "code").fg, Some(Color::Green));
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
        // On it, the markup as it is; the text of a link in bold, its target dimmed
        assert_eq!(styled(&lines[1], "*").fg, Some(Color::DarkGray));
        assert!(styled(&lines[1], "texte").add_modifier.contains(Modifier::BOLD));
        assert_eq!(styled(&lines[1], "[[Page][").fg, Some(Color::DarkGray));
        // A link without text: its target is its text
        assert!(styled(&lines[2], "Autre").add_modifier.contains(Modifier::BOLD));
        assert_eq!(link_parts(&"[t](P.md)".chars().collect::<Vec<_>>(), 0, 9), (vec![(0, 1), (2, 9)], (1, 2)));
    }
}
