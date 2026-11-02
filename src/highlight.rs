use crate::motion::{Hashtags, OrgTags, TagSyntax};
use orgamnesia_core::{links::{Found, Kind}, Format, TodoKeywords};

thread_local! {
    /// The format of the page drawn (set by `render_view`).
    static FORMAT: std::cell::Cell<Format> = const { std::cell::Cell::new(Format::Org) };
}

fn markdown() -> bool {
    FORMAT.with(|f| f.get()) == Format::Markdown
}

thread_local! {
    /// How `#tags` and `:tags:` are drawn as links (set by `render_view`, read while
    /// highlighting a line).
    static TAGS: std::cell::Cell<TagSyntax> = const { std::cell::Cell::new(TagSyntax {
        hashtags: Hashtags::Off,
        org: OrgTags { links: true, dashes: true },
        org_links: orgamnesia_core::links::LinkSyntax::NONE,
        md_links: orgamnesia_core::links::LinkSyntax::ALL,
        markdown: false,
    }) };
}

fn tags() -> TagSyntax {
    TAGS.with(|t| t.get())
}

/// Whether a link target leads nowhere.
pub type Broken = Box<dyn Fn(&str) -> bool>;

thread_local! {
    /// Whether a link target leads nowhere, when such links are marked (set by `render_view`).
    static BROKEN: std::cell::RefCell<Option<Broken>> = const { std::cell::RefCell::new(None) };
}

/// Class of a `[[link]]` to `target`.
fn link_class(target: &str) -> &'static str {
    let broken = BROKEN.with(|b| b.borrow().as_ref().is_some_and(|f| f(target)));
    if broken { "link broken" } else { "link" }
}

thread_local! {
    /// The TODO keywords of the page, `None` when they are off (set by `render_view`).
    static TODO: std::cell::RefCell<Option<TodoKeywords>> = const { std::cell::RefCell::new(None) };
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
pub fn render_view(
    view: &crate::tables::View, tags: TagSyntax, todo: Option<TodoKeywords>, numbers: bool,
    broken: Option<Broken>, format: Format,
) -> String {
    FORMAT.with(|f| f.set(format));
    TAGS.with(|t| t.set(tags.for_format(Some(format))));
    TODO.with(|t| *t.borrow_mut() = todo);
    BROKEN.with(|b| *b.borrow_mut() = broken);
    let out = render_lines(view, numbers);
    BROKEN.with(|b| *b.borrow_mut() = None);
    out
}

fn render_lines(view: &crate::tables::View, numbers: bool) -> String {
    let mut out = String::with_capacity(view.text.len() * 2);
    let lines: Vec<&str> = view.text.split('\n').collect();
    let heads = header_lines(&lines);
    let code = if markdown() { markdown_code_lines(&lines) } else { Default::default() };
    // Number of the next line of the page
    let mut page_line = 1;
    // Where the HTML of the line just drawn starts, to mark it if a link of it is broken
    let mut drawn: Option<usize> = None;
    let mark_broken = |out: &mut String, from: Option<usize>| {
        if let Some(from) = from && out[from..].contains("'link broken'") {
            out.insert_str(from, "<span class='broken-mark'></span>");
        }
    };
    for (n, line) in lines.iter().enumerate() {
        mark_broken(&mut out, drawn.take());
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
            if !before.is_empty() { out.push_str(&highlight_line(before)); }
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
                out.push_str(&crate::tables::to_html(&view.tables[t].raw, view.width.saturating_sub(indent)));
                out.push_str("</span>");
            }
            _ if fill => {}
            _ => out.push_str(&highlight_line(line)),
        }
        // The fold character, invisible, followed by `…`
        if fold.is_some() {
            let c = view.text.split('\n').nth(n).and_then(|l| l.chars().last()).unwrap_or(' ');
            out.push_str(&format!("<span class='fold'>{c}</span>"));
        }
        out.push('\n');
    }
    mark_broken(&mut out, drawn);
    out
}

/// The lines of a Markdown page drawn as they are, with their class: those of
/// its fenced code blocks (```` ``` ```` or `~~~` to the same), and of its front
/// matter (`---` on the first line, to the next `---`).
fn markdown_code_lines(lines: &[&str]) -> std::collections::HashMap<usize, &'static str> {
    let mut out = std::collections::HashMap::new();
    let mut n = 0;
    if lines.first().is_some_and(|l| l.trim_end() == "---")
        && let Some(end) = lines.iter().skip(1).position(|l| matches!(l.trim_end(), "---" | "..."))
    {
        out.extend((0..=end + 1).map(|i| (i, "kw")));
        n = end + 2;
    }
    let mut fence: Option<&str> = None;
    while n < lines.len() {
        let t = lines[n].trim_start();
        match fence {
            None if t.starts_with("```") || t.starts_with("~~~") => {
                fence = Some(&t[..3]);
                out.insert(n, "md-code");
            }
            Some(f) => {
                out.insert(n, "md-code");
                if t.trim_end() == f || (t.starts_with(f) && t[3..].trim_start_matches(f.chars().next().unwrap_or('`')).trim().is_empty()) {
                    fence = None;
                }
            }
            None => {}
        }
        n += 1;
    }
    out
}

/// Lines that are table headers: in a table, the rows above its first rule
/// (`|---+---|`), as org-mode draws them.
fn header_lines(lines: &[&str]) -> std::collections::HashSet<usize> {
    let is_table = |l: &str| l.trim_start().starts_with('|');
    let is_rule = |l: &str| crate::tables::is_rule(l);
    let mut heads = std::collections::HashSet::new();
    let mut n = 0;
    while n < lines.len() {
        if !is_table(lines[n]) { n += 1; continue; }
        let start = n;
        while n < lines.len() && is_table(lines[n]) { n += 1; }
        if let Some(rule) = (start..n).find(|&i| is_rule(lines[i])) {
            heads.extend(start..rule);
        }
    }
    heads
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

fn highlight_line(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let text = |a: usize, b: usize| chars[a..b].iter().collect::<String>();

    // Headline: count leading '*' (`#` in Markdown)
    let format = FORMAT.with(|f| f.get());
    let mark = format.heading_mark();
    if let Some(stars) = format.heading_level(&chars) {
        let level = stars.min(6);
        // `TODO`, `DONE`…: drawn on their own, the title after them
        let keyword = TODO.with(|t| t.borrow().as_ref().and_then(|k| k.keyword_in(&chars, format)));
        let (keyword_html, title_start) = match keyword {
            Some(k) => (
                format!("<span class='{}'>{}</span>", if k.done { "done" } else { "todo" }, escape(&text(k.start, k.end))),
                k.end,
            ),
            None => (String::new(), (stars + 1).min(chars.len())),
        };
        let (title, tags_html) = match crate::motion::headline_tags_range(&chars, tags().org) {
            Some((a, b)) if a >= title_start => (text(title_start, a), format!(
                "<span class='tags'>{}</span>{}", tag_links(&text(a, b)), escape(&text(b, chars.len())))),
            _ => (text(title_start, chars.len()), String::new()),
        };
        return format!(
            "<span class='h{level}{}'>\
             <span class='h-stars'>{}{}</span>{}\
             {keyword_html}{}{}\
             </span>",
            if keyword.is_some_and(|k| k.done) { " h-done" } else { "" },
            // The stars before the last, hidden when the setting asks it
            if stars > 1 { format!("<span class='h-lead'>{}</span>", mark.to_string().repeat(stars - 1)) } else { String::new() },
            mark,
            if stars < chars.len() { " " } else { "" },
            inline_html(&title),
            tags_html,
        );
    }

    if format == Format::Markdown { return markdown_line(line); }

    // `#+FILETAGS: :a:b:`
    if let Some(v) = crate::motion::filetags_value_start(&chars) {
        return format!("<span class='kw'>{}</span><span class='tags'>{}</span>",
            escape(&text(0, v)), tag_links(&text(v, chars.len())));
    }

    // `#+TITLE: …`, drawn like `#+FILETAGS:`: the keyword dimmed, the value as text
    let indent = chars.iter().take_while(|c| c.is_whitespace()).count();
    if text(indent, chars.len()).get(..8).is_some_and(|k| k.eq_ignore_ascii_case("#+title:")) {
        return format!("<span class='kw'>{}</span>{}",
            escape(&text(0, indent + 8)), inline_html(&text(indent + 8, chars.len())));
    }

    // Table line
    if line.trim_start().starts_with('|') {
        return format!("<span class='tbl'>{}</span>", escape(line));
    }

    // Regular paragraph line
    inline_html(line)
}

/// A line of a Markdown page that is no headline: a quote (`> `), a rule
/// (`---`), a table line, or a paragraph line.
fn markdown_line(line: &str) -> String {
    let t = line.trim();
    if t.len() >= 3 && ["-", "*", "_"].iter().any(|c| t.chars().all(|x| x.to_string() == *c || x == ' ')) {
        return format!("<span class='kw'>{}</span>", escape(line));
    }
    if line.trim_start().starts_with('>') {
        return format!("<span class='md-quote'>{}</span>", inline_html(line));
    }
    if line.trim_start().starts_with('|') {
        return format!("<span class='tbl'>{}</span>", escape(line));
    }
    // A checkbox, drawn as a TODO keyword: `[ ]` to do, `[x]` done
    let chars: Vec<char> = line.chars().collect();
    if let Some((at, done)) = orgamnesia_core::todo::checkbox(&chars) {
        let part = |a: usize, b: usize| chars[a..b].iter().collect::<String>();
        return format!("{}<span class='{}'>{}</span>{}", escape(&part(0, at - 1)), if done { "done" } else { "todo" },
            escape(&part(at - 1, at + 2)), inline_html(&part(at + 2, chars.len())));
    }
    inline_html(line)
}

/// Org-mode tags (`:a:b:`) drawn as links to their pages (when they are links);
/// separators left as they are.
fn tag_links(s: &str) -> String {
    let org = tags().org;
    let mut out = String::new();
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        if org.links && !word.is_empty() {
            out.push_str(&format!("<span class='tag-t'>{}</span>", escape(word)));
        } else {
            out.push_str(&escape(word));
        }
        word.clear();
    };
    for c in s.chars() {
        if org.is_tag_char(c) { word.push(c); }
        else { flush(&mut word, &mut out); out.push_str(&escape_char(c)); }
    }
    flush(&mut word, &mut out);
    out
}

/// Convert inline org-mode markup to highlighted HTML.
/// Inline markup of a piece of text (links, emphasis…), as in a paragraph.
pub fn inline(s: &str) -> String {
    inline_html(s)
}

fn inline_html(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;

    let md = markdown();
    while i < chars.len() {
        // Wiki link [[...]] (and the extended ones: `[[a|b]]`, `[texte](Page.md)`)
        if chars[i] == '[' && let Some((html, len)) = try_link(&chars, i) {
            out.push_str(&html);
            i += len;
            continue;
        }
        // Markdown: `code`, **bold**, __bold__, *italic*, _italic_, ~~struck~~
        if md {
            if chars[i] == '`' && let Some(len) = closing(&chars, i, "`", true) {
                let inner: String = chars[i..i + len].iter().collect();
                out.push_str(&format!("<span class='md-code'>{}</span>", escape(&inner)));
                i += len;
                continue;
            }
            let found = [("**", "em-b"), ("__", "em-b"), ("~~", "em-s"), ("*", "em-i"), ("_", "em-i")].into_iter()
                .filter(|(d, _)| chars[i..].starts_with(&d.chars().collect::<Vec<_>>()))
                .find_map(|(d, class)| closing(&chars, i, d, false).map(|len| (d, class, len)));
            if let Some((d, class, len)) = found {
                let n = d.chars().count();
                let inner: String = chars[i + n..i + len - n].iter().collect();
                out.push_str(&format!("<span class='{class}'>{d}{}{d}</span>", inline_html(&inner)));
                i += len;
                continue;
            }
            out.push_str(&escape_char(chars[i]));
            i += 1;
            continue;
        }
        // #hashtag
        if chars[i] == '#'
            && let Some((tag, end)) = crate::motion::hashtag_at(&chars, i, tags().hashtags)
        {
            out.push_str(&format!(
                "<span class='link'>#<span class='link-t'>{}</span></span>", escape(&tag)));
            i = end;
            continue;
        }
        // :tag: in the text
        if chars[i] == ':' && tags().org.links
            && let Some((_, end)) = orgamnesia_core::hashtags::text_tags_at(&chars, i, tags().org)
        {
            let s: String = chars[i..end].iter().collect();
            out.push_str(&format!("<span class='tags'>{}</span>", tag_links(&s)));
            i = end;
            continue;
        }
        // Bold *...*
        if chars[i] == '*' && let Some((html, len)) = try_em(&chars, i, '*', "em-b") {
            out.push_str(&html);
            i += len;
            continue;
        }
        // Italic /.../
        if chars[i] == '/' && let Some((html, len)) = try_em(&chars, i, '/', "em-i") {
            out.push_str(&html);
            i += len;
            continue;
        }
        // Strikethrough +...+ (not inside a word: `1+2+3`)
        if chars[i] == '+' && (i == 0 || !chars[i - 1].is_alphanumeric()) && let Some((html, len)) = try_em(&chars, i, '+', "em-s") {
            out.push_str(&html);
            i += len;
            continue;
        }
        // Underline _..._
        if chars[i] == '_' && let Some((html, len)) = try_em(&chars, i, '_', "em-u") {
            out.push_str(&html);
            i += len;
            continue;
        }

        out.push_str(&escape_char(chars[i]));
        i += 1;
    }
    out
}

/// The Markdown markup `delim` opening at `start` (not followed by a space, not
/// inside a word for `_`): the length up to its closing `delim` (not after a
/// space) included. `verbatim`: nothing inside is markup.
fn closing(chars: &[char], start: usize, delim: &str, verbatim: bool) -> Option<usize> {
    let d: Vec<char> = delim.chars().collect();
    let n = d.len();
    let open = start + n;
    if open >= chars.len() || chars[open].is_whitespace() { return None; }
    if d[0] == '_' && start > 0 && chars[start - 1].is_alphanumeric() { return None; }
    let mut i = open;
    while i + n <= chars.len() {
        if !verbatim && chars[i] == '[' && let Some((_, len)) = try_link(chars, i) {
            i += len;
            continue;
        }
        if chars[i..i + n] == d[..] && i > open && (verbatim || !chars[i - 1].is_whitespace())
            // `**` does not close `*`
            && (verbatim || chars.get(i + n) != Some(&d[0]))
        {
            let after = chars.get(i + n);
            if d[0] == '_' && after.is_some_and(|c| c.is_alphanumeric()) { i += 1; continue; }
            return Some(i + n - start);
        }
        i += 1;
    }
    None
}

/// The link starting at `start` drawn: its target (`link-t`) and text
/// (`link-d`); the target it leads to in `data-t` when it is not the one written
/// (`[[Page|texte]]`, `[texte](Ma%20page.md)`).
fn try_link(chars: &[char], start: usize) -> Option<(String, usize)> {
    let f = orgamnesia_core::links::link_starting(chars, start, tags().links())?;
    Some((link_html(chars, &f), f.end - start))
}

fn link_html(chars: &[char], f: &Found) -> String {
    let text = |a: usize, b: usize| escape(&chars[a..b].iter().collect::<String>());
    let (written, html) = match f.kind {
        Kind::Org => {
            let inner_end = f.end - 2;
            let split = (f.start + 2..inner_end).find(|&j| chars[j] == ']' && chars[j + 1] == '[')
                .map(|j| (j, 2))
                .or_else(|| tags().links().pipe.then(|| (f.start + 2..inner_end).find(|&j| chars[j] == '|').map(|j| (j, 1))).flatten());
            match split {
                Some((j, n)) => (chars[f.start + 2..j].iter().collect::<String>(), format!(
                    "[[<span class='link-t'>{}</span>{}<span class='link-d'>{}</span>]]",
                    text(f.start + 2, j), text(j, j + n), text(j + n, inner_end))),
                None => (chars[f.start + 2..inner_end].iter().collect::<String>(),
                    format!("[[<span class='link-t'>{}</span>]]", text(f.start + 2, inner_end))),
            }
        }
        Kind::Markdown => {
            let close = (f.start + 1..f.end).find(|&j| chars[j] == ']' && chars.get(j + 1) == Some(&'(')).unwrap_or(f.start);
            (String::new(), format!("[<span class='link-d'>{}</span>](<span class='link-t'>{}</span>)",
                text(f.start + 1, close), text(close + 2, f.end - 1)))
        }
    };
    let data = if written.trim() == f.target { String::new() } else { format!(" data-t='{}'", escape(&f.target)) };
    format!("<span class='{}'{data}>{html}</span>", link_class(&f.target))
}

fn try_em(chars: &[char], start: usize, delim: char, class: &str) -> Option<(String, usize)> {
    if start + 2 >= chars.len() { return None; }
    let next = chars[start + 1];
    if next.is_whitespace() || next == delim { return None; }
    let mut i = start + 1;
    while i < chars.len() {
        // A link inside is drawn as a link: its target may hold the delimiter (`/[[a/b]]/`)
        if chars[i] == '[' && chars.get(i + 1) == Some(&'[') && let Some((_, len)) = try_link(chars, i) {
            i += len;
            continue;
        }
        if chars[i] == delim && !chars[i - 1].is_whitespace() && i > start + 1 {
            let inner: String = chars[start + 1..i].iter().collect();
            let html = format!(
                "<span class='{class}'>{delim}{}{delim}</span>",
                inline_html(&inner)
            );
            return Some((html, i + 1 - start));
        }
        i += 1;
    }
    None
}

/// Syntax-highlighted HTML of a JSON text (the configuration files of the
/// "preference" project): keys, strings, numbers, `true` / `false` / `null`.
/// With `numbers`, each line starts with its number, drawn in the margin.
pub fn render_json(text: &str, numbers: bool) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    for (n, line) in text.split('\n').enumerate() {
        if numbers { out.push_str(&format!("<span class='ln'>{}</span>", n + 1)); }
        out.push_str(&json_line(line));
        out.push('\n');
    }
    out
}

fn json_line(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let text = |a: usize, b: usize| escape(&chars[a..b].iter().collect::<String>());
    let span = |class: &str, a: usize, b: usize| format!("<span class='{class}'>{}</span>", text(a, b));
    let word = |c: char| c.is_alphanumeric() || matches!(c, '_' | '-' | '+' | '.');
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' {
            // To the closing quote (not an escaped one), or the end of the line
            let mut j = i + 1;
            while j < chars.len() && chars[j] != '"' { j += if chars[j] == '\\' { 2 } else { 1 }; }
            let end = (j + 1).min(chars.len());
            let key = chars[end..].iter().find(|c| !c.is_whitespace()) == Some(&':');
            out.push_str(&span(if key { "json-key" } else { "json-str" }, i, end));
            i = end;
        } else if word(c) {
            let end = chars[i..].iter().position(|&c| !word(c)).map_or(chars.len(), |n| i + n);
            let token: String = chars[i..end].iter().collect();
            let class = match token.as_str() {
                "true" | "false" | "null" => "json-lit",
                _ if token.parse::<f64>().is_ok() => "json-num",
                _ => "json-bad",
            };
            out.push_str(&span(class, i, end));
            i = end;
        } else if "{}[],:".contains(c) {
            out.push_str(&span("json-punct", i, i + 1));
            i += 1;
        } else {
            out.push_str(&escape_char(c));
            i += 1;
        }
    }
    out
}

/// The layer marking what is wrong (`error`) or odd in a configuration file:
/// its char range `start..end`.
pub fn render_problem(content: &str, start: usize, end: usize, error: bool) -> String {
    let class = if error { "class='cfg-error'" } else { "class='cfg-warning'" };
    render_marks(content, [(start, end.max(start + 1), class.to_string())])
}

fn escape(s: &str) -> String {
    s.chars().map(escape_char).collect()
}

fn escape_char(c: char) -> String {
    match c {
        '&' => "&amp;".to_string(),
        '<' => "&lt;".to_string(),
        '>' => "&gt;".to_string(),
        '"' => "&quot;".to_string(),
        _ => c.to_string(),
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn md(text: &str) -> String {
        let v = crate::tables::View { text: text.to_string(), ..Default::default() };
        render_view(&v, Hashtags::Off.into(), None, false, None, Format::Markdown)
    }

    #[test]
    fn markdown_is_highlighted() {
        let html = md("## Titre *it*\n**gras** et `co*de*` ~~barré~~\n> citation\n```\n# pas un titre\n```\n---");
        let lines: Vec<&str> = html.split('\n').collect();
        assert!(lines[0].starts_with("<span class='h2'><span class='h-stars'><span class='h-lead'>#</span>#</span> Titre <span class='em-i'>*it*</span>"), "{}", lines[0]);
        assert!(lines[1].contains("<span class='em-b'>**gras**</span>") && lines[1].contains("<span class='md-code'>`co*de*`</span>"));
        assert!(lines[1].contains("<span class='em-s'>~~barré~~</span>"));
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
    fn extended_links_are_drawn() {
        let html = md("[[Page|texte]] [lien](Ma%20page.md) [[Simple]]");
        assert!(html.contains("<span class='link'>[[<span class='link-t'>Page</span>|<span class='link-d'>texte</span>]]</span>"), "{html}");
        assert!(html.contains("<span class='link' data-t='Ma page.md'>[<span class='link-d'>lien</span>](<span class='link-t'>Ma%20page.md</span>)</span>"), "{html}");
        assert!(html.contains("<span class='link'>[[<span class='link-t'>Simple</span>]]</span>"));
    }

    #[test]
    fn json_is_highlighted() {
        let html = render_json("{\n  \"a\": [true, 1.5, \"x<\"],\n  \"b\": nul\n}", true);
        assert!(html.starts_with("<span class='ln'>1</span><span class='json-punct'>{</span>\n"), "{html}");
        assert!(html.contains("<span class='json-key'>&quot;a&quot;</span><span class='json-punct'>:</span>"));
        assert!(html.contains("<span class='json-lit'>true</span>"));
        assert!(html.contains("<span class='json-num'>1.5</span>"));
        assert!(html.contains("<span class='json-str'>&quot;x&lt;&quot;</span>"));
        assert!(html.contains("<span class='json-bad'>nul</span>"));
        assert_eq!(html.matches('\n').count(), 4);
    }

    #[test]
    fn broken_links_are_marked_in_the_margin() {
        let page = "a [[Existe]]\nb [[Absente]] et [[Existe]]\n[[x]]";
        let v = crate::tables::View::build(page, None, 80, |_| 1, |_, _, _| None, true, false, &[]);
        let html = render_view(&v, Hashtags::Off.into(), None, false, Some(Box::new(|t: &str| t != "Existe")), Format::Org);
        let lines: Vec<&str> = html.split('\n').collect();
        assert!(!lines[0].contains("broken"));
        assert!(lines[1].starts_with("<span class='broken-mark'></span>b "));
        assert!(lines[1].contains("<span class='link broken'>[[<span class='link-t'>Absente"));
        assert!(lines[2].starts_with("<span class='broken-mark'>"));
        // Not marked when not asked
        assert!(!render_view(&v, Hashtags::Off.into(), None, false, None, Format::Org).contains("broken"));
    }

    #[test]
    fn headline_h1() {
        let html = highlight_line("* Hello");
        assert!(html.contains("h1"));
        assert!(html.contains("Hello"));
    }

    #[test]
    fn headline_h2_with_tag() {
        let html = highlight_line("** Work :project:");
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
            assert_eq!(strip(highlight_line(l)), l);
        }
    }

    #[test]
    fn filetags_rendered() {
        let html = highlight_line("#+FILETAGS: :a:b:");
        assert!(html.contains("<span class='tags'> :<span class='tag-t'>a</span>:<span class='tag-t'>b</span>:</span>"));
    }

    #[test]
    fn table_line_wrapped() {
        let html = highlight_line("| A | B |");
        assert!(html.contains("tbl"));
    }

    #[test]
    fn bold_rendered() {
        let html = inline_html("say *hello* now");
        assert!(html.contains("em-b"));
        assert!(html.contains("*hello*"));
    }

    #[test]
    fn italic_rendered() {
        let html = inline_html("/world/");
        assert!(html.contains("em-i"));
    }

    #[test]
    fn underline_rendered() {
        let html = inline_html("_under_");
        assert!(html.contains("em-u"));
    }

    #[test]
    fn link_simple_rendered() {
        let html = inline_html("See [[My Page]] now");
        assert!(html.contains("link-t"));
        assert!(html.contains("My Page"));
    }

    #[test]
    fn link_with_display_rendered() {
        let html = inline_html("[[Target][label]]");
        assert!(html.contains("link-t"));
        assert!(html.contains("link-d"));
        assert!(html.contains("label"));
    }

    #[test]
    fn link_inside_emphasis_rendered() {
        for s in ["*[[test]]*", "/[[test]]/", "_voir [[a/b][c]]_", "/[[a/b]]/"] {
            let html = inline_html(s);
            assert!(html.contains("<span class='link'>[[<span class='link-t'>"), "{s}: {html}");
        }
        assert!(inline_html("/[[a/b]]/").starts_with("<span class='em-i'>/"));
        assert!(inline_html("*gras /et italique/*").contains("<span class='em-i'>/et italique/</span>*</span>"));
    }

    #[test]
    fn html_entities_escaped() {
        let html = inline_html("3 < 4 & 5 > 2");
        assert!(html.contains("&lt;"));
        assert!(html.contains("&amp;"));
        assert!(html.contains("&gt;"));
    }

    #[test]
    fn no_bold_with_space_after_star() {
        let html = inline_html("* not bold");
        assert!(!html.contains("em-b"));
    }

    #[test]
    fn org_tags_in_text_drawn_as_links() {
        let v = crate::tables::View { text: "voir :ex: à 10:30:".into(), tables: vec![], width: 80, indents: vec![], folds: vec![] };
        let html = render_view(&v, Hashtags::Off.into(), None, false, None, Format::Org);
        assert!(html.contains(":<span class='tag-t'>ex</span>:"));
        assert!(!html.contains("30</span>"));
        let off = TagSyntax { hashtags: Hashtags::Off, org: OrgTags { links: false, dashes: false }, ..Hashtags::Off.into() };
        assert!(!render_view(&v, off, None, false, None, Format::Org).contains("tag-t"));
    }

    #[test]
    fn line_numbers_follow_the_page() {
        // A collapsed table of 3 rows takes 4 lines of the view, numbered as its 3 lines
        let page = "a\n| x |\n|---|\n| y |\nb";
        let v = crate::tables::View::build(page, None, 80, |_| 4, |_, _, _| None, true, false, &[]);
        let html = render_view(&v, Hashtags::Off.into(), None, true, None, Format::Org);
        let nums: Vec<&str> = html.split("<span class='ln'>").skip(1).map(|s| &s[..s.find('<').unwrap()]).collect();
        assert_eq!(nums, vec!["1", "2", "5"]);
        assert!(!render_view(&v, Hashtags::Off.into(), None, false, None, Format::Org).contains("class='ln'"));
    }

    #[test]
    fn title_keyword_dimmed_like_filetags() {
        assert_eq!(highlight_line("#+TITLE: Ma page"), "<span class='kw'>#+TITLE:</span> Ma page");
        assert!(highlight_line("#+FILETAGS: :a:").starts_with("<span class='kw'>#+FILETAGS:</span>"));
    }

    #[test]
    fn todo_keywords_on_headlines() {
        let v = crate::tables::View { text: "** TODO a :x:\n* DONE b\n* TODOS c".into(), tables: vec![], width: 80, indents: vec![], folds: vec![] };
        let kw = Some(TodoKeywords::parse("TODO | DONE"));
        let html = render_view(&v, Hashtags::Off.into(), kw, false, None, Format::Org);
        assert!(html.contains("<span class='h-stars'><span class='h-lead'>*</span>*</span> <span class='todo'>TODO</span> a <span class='tags'>"));
        assert!(html.contains("<span class='h1 h-done'><span class='h-stars'>*</span> <span class='done'>DONE</span> b"));
        assert!(!html.contains("TODOS</span>"));
        // Off: plain titles
        assert!(!render_view(&v, Hashtags::Off.into(), None, false, None, Format::Org).contains("class='todo'"));
    }

    #[test]
    fn table_header_bold_while_edited() {
        let v = crate::tables::View { text: "x\n| a | b |\n|---+---|\n| c | d |".into(), tables: vec![], width: 80, indents: vec![], folds: vec![] };
        let html = render_view(&v, Hashtags::Off.into(), None, false, None, Format::Org);
        assert!(html.contains("<span class='tbl tbl-head'>| a | b |</span>"));
        assert!(!html.contains("tbl-head'>| c"));
    }
}
