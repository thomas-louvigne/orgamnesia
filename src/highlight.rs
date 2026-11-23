//! The HTML drawn under the editor's textarea: the org-mode or Markdown text
//! highlighted (headlines, links, tags, emphasis, tables, images), and the layers
//! marking spelling mistakes, find matches and problems of the configuration files.

use crate::html::escape;
use orgamnesia_core::highlight::{self as hl, Node};
use orgamnesia_core::{Format, TagSyntax, TodoKeywords};

mod yaml;
pub use yaml::render_yaml;

/// What drawing a page depends on (`orgamnesia_core::highlight`, which cuts its
/// lines into pieces; written here as HTML).
pub use orgamnesia_core::highlight::Draw;

/// Whether a link target leads nowhere.
pub type Broken = hl::Broken<'static>;

/// The pieces of a line as HTML: a span of its class for each.
fn html(nodes: &[Node], out: &mut String) {
    for n in nodes {
        match n {
            Node::Text(t) => out.push_str(&escape(t)),
            Node::Span { class, data, children } => {
                out.push_str(&format!("<span class='{class}'"));
                if let Some(d) = data { out.push_str(&format!(" data-t='{}'", escape(d))); }
                out.push('>');
                html(children, out);
                out.push_str("</span>");
            }
        }
    }
}

fn to_html(nodes: &[Node]) -> String {
    let mut out = String::new();
    html(nodes, &mut out);
    out
}

/// Plain-text copy of the document with the char range `start..end` wrapped in
/// `<mark>`, drawn (transparent text) on top of the highlight layer to show the
/// Emacs region. Same trailing-newline handling as `render_view` so the layers line up.
pub fn render_region(content: &str, start: usize, end: usize) -> String {
    let chars: Vec<char> = content.chars().collect();
    let end = end.min(chars.len());
    let start = start.min(end);
    let part = |a: usize, b: usize| escape(&chars[a..b].iter().collect::<String>());
    format!("{}<mark class='region'>{}</mark>{}\n", part(0, start), part(start, end), part(end, chars.len()))
}

/// Syntax-highlighted HTML for the view of the page where some tables are collapsed
/// (see `tables`): the first line of a collapsed table draws the whole table,
/// over the empty lines that follow it.
/// With `numbers`, each line of the page starts with its number, drawn in the margin.
/// `todo`: the TODO keywords drawn on headlines (`None` when they are off).
/// With `broken`, telling the link targets that lead nowhere, those links are drawn
/// as such and their lines get a mark in the margin.
/// `current`: the line of the view wrapped in `<span class='cur'>`, where the
/// markup (`mk`) stays visible in live preview.
pub fn render_view(
    view: &crate::tables::View, tags: TagSyntax, todo: Option<TodoKeywords>, numbers: bool,
    broken: Option<Broken>, format: Format, current: Option<usize>,
) -> String {
    let d = Draw { todo, broken, ..Draw::new(format, tags) };
    render_lines(view, numbers, current, &d)
}

fn render_lines(view: &crate::tables::View, numbers: bool, current: Option<usize>, d: &Draw) -> String {
    let mut out = String::with_capacity(view.text.len() * 2);
    let lines: Vec<&str> = view.text.split('\n').collect();
    let heads = hl::table_heads(&lines);
    let code = if d.format == Format::Markdown { hl::code_lines(&lines, &view.text) } else { Default::default() };
    // Number of the next line of the page
    let mut page_line = 1;
    // Where the HTML of the line just drawn starts, to mark it if a link of it is broken
    let mut drawn: Option<usize> = None;
    // Where the HTML of the current line starts, to wrap it once drawn (its `\n` left out)
    let mut cur: Option<usize> = None;
    let wrap_cur = |out: &mut String, from: Option<usize>| {
        if let Some(from) = from {
            out.insert_str(out.len() - 1, "</span>");
            out.insert_str(from, "<span class='cur'>");
        }
    };
    let mark_broken = |out: &mut String, from: Option<usize>| {
        if let Some(from) = from && out[from..].contains("'link broken'") {
            out.insert_str(from, "<span class='broken-mark'></span>");
        }
    };
    for (n, line) in lines.iter().enumerate() {
        mark_broken(&mut out, drawn.take());
        wrap_cur(&mut out, cur.take());
        if current == Some(n) { cur = Some(out.len()); }
        let line = *line;
        // The lines below a collapsed table's first one are not lines of the page
        let fill = !line.is_empty() && line.chars().all(crate::tables::is_fill);
        // A folded headline ends with its fold character
        let fold = line.chars().last().and_then(crate::tables::fold_index).and_then(|f| view.folds.get(f));
        let line = if fold.is_some() { &line[..line.len() - line.chars().last().map_or(0, char::len_utf8)] } else { line };
        if numbers && !fill {
            let class = if fold.is_some() { "ln folded" } else { "ln" };
            out.push_str(&format!("<span class='{class}'>{page_line}</span>"));
            let table = line.trim_start_matches(crate::tables::INDENT).chars().next()
                .and_then(crate::tables::tag_index)
                .and_then(|t| view.tables.get(t));
            page_line += table.map_or(1, |t| t.raw.split('\n').count());
            page_line += fold.map_or(0, |f| f.raw.matches('\n').count());
        }
        if fold.is_some() {
            out.push_str("<span class='fold-mark'></span>");
        } else {
            drawn = Some(out.len());
        }
        if heads.contains(&n) {
            out.push_str(&format!("<span class='tbl tbl-head'>{}</span>\n", escape(line)));
            continue;
        }
        // Markdown: a fenced code block, or the front matter, as it is
        if let Some(&class) = code.get(&n) {
            out.push_str(&format!("<span class='{class}'>{}</span>", escape(line)));
            if fold.is_some() {
                let c = view.text.split('\n').nth(n).and_then(|l| l.chars().last()).unwrap_or(' ');
                out.push_str(&format!("<span class='fold'>{c}</span>"));
            }
            out.push('\n');
            continue;
        }
        // An image, after what comes before its link (indentation, stars, bullet)
        let image = line.chars().last().and_then(crate::tables::tag_index)
            .and_then(|t| view.tables.get(t))
            .and_then(|t| t.image.as_ref().map(|i| (t.start, i)));
        if let Some((at, img)) = image {
            let before = &line[..line.len() - line.chars().last().map_or(0, char::len_utf8)];
            if !before.is_empty() { out.push_str(&highlight_line(before, d)); }
            // A video's thumbnail: a play button over it, and the video to open
            let (class, video, play) = match &img.video {
                // Logseq's `{{video url}}`: a red cross at the bottom left, to replace it
                Some(url) if img.logseq_macro => ("img-box img-video img-macro", format!(" data-video='{}' data-macro='1'", escape(url)),
                    "<span class='img-play'></span><span class='img-warn'>✕</span>"),
                Some(url) => ("img-box img-video", format!(" data-video='{}'", escape(url)), "<span class='img-play'></span>"),
                None => ("img-box", String::new(), ""),
            };
            out.push_str(&format!(
                "<span class='img-view'><span class='{class}' data-at='{at}'{video}>\
                 <img class='img-pic' src='{}' style='width: {}px; height: {}px' alt=''>{play}</span></span>\n",
                escape(&img.src), img.width, img.height,
            ));
            continue;
        }
        // A collapsed table, maybe indented under a headline
        let indent = line.chars().take_while(|&c| c == crate::tables::INDENT).count();
        let mut chars = line.chars().skip(indent);
        match chars.next().and_then(crate::tables::tag_index) {
            Some(t) if chars.next().is_none() && t < view.tables.len() => {
                out.push_str(&crate::tables::INDENT.to_string().repeat(indent));
                out.push_str(&format!("<span class='tbl-view' style='width: calc(100% - {indent}ch)'>"));
                out.push_str(&crate::tables::to_html(&view.tables[t].raw, view.width.saturating_sub(indent), d));
                out.push_str("</span>");
            }
            _ if fill => {}
            _ => out.push_str(&highlight_line(line, d)),
        }
        // The fold character, invisible, followed by `…`
        if fold.is_some() {
            let c = view.text.split('\n').nth(n).and_then(|l| l.chars().last()).unwrap_or(' ');
            out.push_str(&format!("<span class='fold'>{c}</span>"));
        }
        out.push('\n');
    }
    mark_broken(&mut out, drawn);
    wrap_cur(&mut out, cur);
    out
}

/// `content` with each of `marks` (a char range and the attributes of its tag)
/// drawn as a `<mark>`; a mark overlapping the one before is left out.
fn render_marks(content: &str, marks: impl IntoIterator<Item = (usize, usize, String)>) -> String {
    let chars: Vec<char> = content.chars().collect();
    let part = |a: usize, b: usize| escape(&chars[a.min(chars.len())..b.min(chars.len())].iter().collect::<String>());
    let mut out = String::new();
    let mut at = 0;
    for (a, b, attrs) in marks {
        if a < at { continue; }
        out.push_str(&part(at, a));
        out.push_str(&format!("<mark {attrs}>{}</mark>", part(a, b)));
        at = b;
    }
    if out.is_empty() { return out; }
    out.push_str(&part(at, chars.len()));
    out.push('\n');
    out
}

/// A misspelled word: its range in the view, and in the page.
pub type SpellMark = ((usize, usize), (usize, usize));

/// The spelling layer: the misspelled words of `content`, with the page range of
/// each (`data-a`, `data-b`) to find the word under the mouse.
pub fn render_spelling(content: &str, words: &[SpellMark]) -> String {
    render_marks(content, words.iter().map(|&((a, b), (pa, pb))| {
        (a, b, format!("class='spell' data-a='{pa}' data-b='{pb}'"))
    }))
}

pub fn render_matches(content: &str, matches: &[(usize, usize)], current: Option<usize>) -> String {
    render_marks(content, matches.iter().enumerate().map(|(i, &(a, b))| {
        (a, b, format!("class='{}'", if Some(i) == current { "match current" } else { "match" }))
    }))
}

fn highlight_line(line: &str, d: &Draw) -> String {
    to_html(&hl::line(line, d))
}

/// Inline markup of a piece of text (links, emphasis…), as in a paragraph.
pub fn inline(s: &str, d: &Draw) -> String {
    to_html(&hl::inline(s, d))
}

/// The layer marking what is wrong (`error`) or odd in a configuration file:
/// its char range `start..end`.
pub fn render_problem(content: &str, start: usize, end: usize, error: bool) -> String {
    let class = if error { "class='cfg-error'" } else { "class='cfg-warning'" };
    render_marks(content, [(start, end.max(start + 1), class.to_string())])
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use orgamnesia_core::{Hashtags, OrgTags};

    /// How the tests draw: org-mode, `:tags:` as links.
    fn dr() -> Draw<'static> {
        Draw::new(Format::Org, TagSyntax { org: OrgTags { links: true, dashes: true }, ..Hashtags::Off.into() })
    }

    fn md(text: &str) -> String {
        let v = crate::tables::View { text: text.to_string(), ..Default::default() };
        render_view(&v, Hashtags::Off.into(), None, false, None, Format::Markdown, None)
    }

    #[test]
    fn markdown_is_highlighted() {
        let html = md("## Titre *it*\n**gras** et `co*de*` ~~barré~~\n> citation\n```\n# pas un titre\n```\n---");
        let lines: Vec<&str> = html.split('\n').collect();
        assert!(lines[0].starts_with("<span class='h2'><span class='h-stars'><span class='h-lead'>#</span>#</span> Titre <span class='em-i'><span class='mk'>*</span>it<span class='mk'>*</span></span>"), "{}", lines[0]);
        assert!(lines[1].contains("<span class='em-b'><span class='mk'>**</span>gras<span class='mk'>**</span></span>") && lines[1].contains("<span class='md-code'>`co*de*`</span>"));
        assert!(lines[1].contains("<span class='em-s'><span class='mk'>~~</span>barré<span class='mk'>~~</span></span>"));
        assert!(lines[2].starts_with("<span class='md-quote'>"));
        assert_eq!(lines[4], "<span class='md-code'># pas un titre</span>");
        assert_eq!(lines[6], "<span class='kw'>---</span>");
        assert!(md("- [x] fait").contains("<span class='done'>[x]</span>"));
        // Org-mode markup is not Markdown's
        assert!(!md("/pas/ +pas+ snake_case_name").contains("em-"));
        // A front matter
        assert!(md("---\ntitle: x\n---\n# T").split('\n').nth(1).unwrap().contains("class='kw'"));
    }

    #[test]
    fn current_line_is_wrapped() {
        let v = crate::tables::View { text: "a\n*b*\nc".to_string(), ..Default::default() };
        let html = render_view(&v, Hashtags::Off.into(), None, false, None, Format::Org, Some(1));
        assert_eq!(html.split('\n').collect::<Vec<_>>(), ["a", "<span class='cur'><span class='em-b'><span class='mk'>*</span>b<span class='mk'>*</span></span></span>", "c", ""]);
        // The last line, without a newline of its own in the page
        assert!(render_view(&v, Hashtags::Off.into(), None, false, None, Format::Org, Some(2)).ends_with("<span class='cur'>c</span>\n"));
    }

    #[test]
    fn extended_links_are_drawn() {
        let html = md("[[Page|texte]] [lien](Ma%20page.md) [[Simple]]");
        assert!(html.contains("<span class='link'><span class='mk'>[[<span class='link-t'>Page</span>|</span><span class='link-d'>texte</span><span class='mk'>]]</span></span>"), "{html}");
        assert!(html.contains("<span class='link' data-t='Ma page.md'><span class='mk'>[</span><span class='link-d'>lien</span><span class='mk'>](<span class='link-t'>Ma%20page.md</span>)</span></span>"), "{html}");
        assert!(html.contains("<span class='link'><span class='mk'>[[</span><span class='link-t'>Simple</span><span class='mk'>]]</span></span>"));
    }

    #[test]
    fn broken_links_are_marked_in_the_margin() {
        let page = "a [[Existe]]\nb [[Absente]] et [[Existe]]\n[[x]]";
        let v = crate::tables::View::build(page, None, 80, |_| 1, |_, _, _| None, true, false, &[]);
        let html = render_view(&v, Hashtags::Off.into(), None, false, Some(Box::new(|t: &str| t != "Existe")), Format::Org, None);
        let lines: Vec<&str> = html.split('\n').collect();
        assert!(!lines[0].contains("broken"));
        assert!(lines[1].starts_with("<span class='broken-mark'></span>b "));
        assert!(lines[1].contains("<span class='link broken'><span class='mk'>[[</span><span class='link-t'>Absente"));
        assert!(lines[2].starts_with("<span class='broken-mark'>"));
        // Not marked when not asked
        assert!(!render_view(&v, Hashtags::Off.into(), None, false, None, Format::Org, None).contains("broken"));
    }

    #[test]
    fn headline_h1() {
        let html = highlight_line("* Hello", &dr());
        assert!(html.contains("h1"));
        assert!(html.contains("Hello"));
    }

    #[test]
    fn headline_h2_with_tag() {
        let html = highlight_line("** Work :project:", &dr());
        assert!(html.contains("h2"));
        assert!(html.contains("tags"));
        assert!(html.contains("<span class='tag-t'>project</span>"));
    }

    #[test]
    fn headline_text_is_kept_as_is() {
        // The overlay must show exactly the typed characters to line up with the text
        let strip = |h: String| {
            let mut out = String::new();
            let mut tag = false;
            for c in h.chars() {
                match c { '<' => tag = true, '>' => tag = false, _ if !tag => out.push(c), _ => {} }
            }
            out
        };
        for l in ["** Work :project:", "* T  :a:b:  ", "*", "* Idée :été:", "#+FILETAGS: :x:"] {
            assert_eq!(strip(highlight_line(l, &dr())), l);
        }
    }

    #[test]
    fn filetags_rendered() {
        let html = highlight_line("#+FILETAGS: :a:b:", &dr());
        assert!(html.contains("<span class='tags'> :<span class='tag-t'>a</span>:<span class='tag-t'>b</span>:</span>"));
    }

    #[test]
    fn table_line_wrapped() {
        let html = highlight_line("| A | B |", &dr());
        assert!(html.contains("tbl"));
    }

    #[test]
    fn bold_rendered() {
        let html = inline("say *hello* now", &dr());
        assert!(html.contains("em-b"));
        assert!(html.contains("<span class='mk'>*</span>hello<span class='mk'>*</span>"));
    }

    #[test]
    fn italic_rendered() {
        let html = inline("/world/", &dr());
        assert!(html.contains("em-i"));
    }

    #[test]
    fn underline_rendered() {
        let html = inline("_under_", &dr());
        assert!(html.contains("em-u"));
    }

    #[test]
    fn link_simple_rendered() {
        let html = inline("See [[My Page]] now", &dr());
        assert!(html.contains("link-t"));
        assert!(html.contains("My Page"));
    }

    #[test]
    fn link_with_display_rendered() {
        let html = inline("[[Target][label]]", &dr());
        assert!(html.contains("link-t"));
        assert!(html.contains("link-d"));
        assert!(html.contains("label"));
    }

    #[test]
    fn link_inside_emphasis_rendered() {
        for s in ["*[[test]]*", "/[[test]]/", "_voir [[a/b][c]]_", "/[[a/b]]/"] {
            let html = inline(s, &dr());
            assert!(html.contains("<span class='link'><span class='mk'>[["), "{s}: {html}");
        }
        assert!(inline("/[[a/b]]/", &dr()).starts_with("<span class='em-i'><span class='mk'>/"));
        assert!(inline("*gras /et italique/*", &dr()).contains("<span class='em-i'><span class='mk'>/</span>et italique<span class='mk'>/</span></span><span class='mk'>*</span></span>"));
    }

    #[test]
    fn html_entities_escaped() {
        let html = inline("3 < 4 & 5 > 2", &dr());
        assert!(html.contains("&lt;"));
        assert!(html.contains("&amp;"));
        assert!(html.contains("&gt;"));
    }

    #[test]
    fn no_bold_with_space_after_star() {
        let html = inline("* not bold", &dr());
        assert!(!html.contains("em-b"));
    }

    #[test]
    fn org_tags_in_text_drawn_as_links() {
        let v = crate::tables::View { text: "voir :ex: à 10:30:".into(), tables: vec![], width: 80, indents: vec![], folds: vec![] };
        let html = render_view(&v, Hashtags::Off.into(), None, false, None, Format::Org, None);
        assert!(html.contains(":<span class='tag-t'>ex</span>:"));
        assert!(!html.contains("30</span>"));
        let off = TagSyntax { hashtags: Hashtags::Off, org: OrgTags { links: false, dashes: false }, ..Hashtags::Off.into() };
        assert!(!render_view(&v, off, None, false, None, Format::Org, None).contains("tag-t"));
    }

    #[test]
    fn line_numbers_follow_the_page() {
        // A collapsed table of 3 rows takes 4 lines of the view, numbered as its 3 lines
        let page = "a\n| x |\n|---|\n| y |\nb";
        let v = crate::tables::View::build(page, None, 80, |_| 4, |_, _, _| None, true, false, &[]);
        let html = render_view(&v, Hashtags::Off.into(), None, true, None, Format::Org, None);
        let nums: Vec<&str> = html.split("<span class='ln'>").skip(1).map(|s| &s[..s.find('<').unwrap()]).collect();
        assert_eq!(nums, vec!["1", "2", "5"]);
        assert!(!render_view(&v, Hashtags::Off.into(), None, false, None, Format::Org, None).contains("class='ln'"));
    }

    #[test]
    fn title_keyword_dimmed_like_filetags() {
        assert_eq!(highlight_line("#+TITLE: Ma page", &dr()), "<span class='kw'>#+TITLE:</span> Ma page");
        assert!(highlight_line("#+FILETAGS: :a:", &dr()).starts_with("<span class='kw'>#+FILETAGS:</span>"));
    }

    #[test]
    fn todo_keywords_on_headlines() {
        let v = crate::tables::View { text: "** TODO a :x:\n* DONE b\n* TODOS c".into(), tables: vec![], width: 80, indents: vec![], folds: vec![] };
        let kw = Some(TodoKeywords::parse("TODO | DONE"));
        let html = render_view(&v, Hashtags::Off.into(), kw, false, None, Format::Org, None);
        assert!(html.contains("<span class='h-stars'><span class='h-lead'>*</span>*</span> <span class='todo'>TODO</span> a <span class='tags'>"));
        assert!(html.contains("<span class='h1 h-done'><span class='h-stars'>*</span> <span class='done'>DONE</span> b"));
        assert!(!html.contains("TODOS</span>"));
        // Off: plain titles
        assert!(!render_view(&v, Hashtags::Off.into(), None, false, None, Format::Org, None).contains("class='todo'"));
    }

    #[test]
    fn table_header_bold_while_edited() {
        let v = crate::tables::View { text: "x\n| a | b |\n|---+---|\n| c | d |".into(), tables: vec![], width: 80, indents: vec![], folds: vec![] };
        let html = render_view(&v, Hashtags::Off.into(), None, false, None, Format::Org, None);
        assert!(html.contains("<span class='tbl tbl-head'>| a | b |</span>"));
        assert!(!html.contains("tbl-head'>| c"));
    }
}
