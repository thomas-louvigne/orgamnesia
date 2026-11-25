//! A page's org-mode or Markdown text made into HTML: title, headlines, lists,
//! tables, blocks, images, links, footnotes.

use crate::{html::escape, images, links};
use orgamnesia_core::{footnotes, links as place, names::fold, Format, TodoKeywords};

mod markdown;
mod lines;
use lines::*;

/// The HTML of the page `text` called `name`. `image` gives the URL an image
/// link target is shown from.
/// `format`: the page's; `extended`: the extended links are read (`[[Page|texte]]`,
/// `[texte](Page.md)`; always in Markdown).
pub fn page_html(
    text: &str, name: &str, keywords: Option<&TodoKeywords>, image: &dyn Fn(&str) -> String, format: Format, extended: place::LinkSyntax,
) -> String {
    let md = format == Format::Markdown;
    let all: Vec<&str> = text.split('\n').collect();
    // A Markdown page's front matter is not shown
    let skip = if md { orgamnesia_core::frontmatter::lines(text).map_or(0, |(_, n)| n) } else { 0 };
    let lines = &all[skip.min(all.len())..];
    // The title: the `#+TITLE:` (`title:`), else the name, unless the page starts
    // with a headline holding it (as a new page does)
    let first_headline = lines.iter().find(|l| !l.trim().is_empty() && (md || !l.trim_start().starts_with('#')))
        .and_then(|l| place::headline_title(l, keywords, format));
    let written = format.title(text);
    let title = match written {
        Some(t) => Some(t),
        None if first_headline.is_some_and(|h| fold(&h) == fold(name)) => None,
        None => Some(name.to_string()),
    };
    let (lines, defs) = footnote_definitions(lines, format);
    let doc = Doc { keywords, image, shift: usize::from(title.is_some()), format, extended, defs, notes: Default::default() };
    let mut out = String::new();
    if let Some(t) = title { out.push_str(&format!("<h1 class='title'>{}</h1>", escape(&t))); }
    out.push_str(&doc.body(&lines));
    out.push_str(&doc.footnotes());
    out
}

/// `lines` without the definitions of footnotes (a marker starting a line, and
/// the lines after it up to a blank one) nor org-mode's `Footnotes` headline;
/// the definitions apart, by label.
fn footnote_definitions<'a>(lines: &[&'a str], format: Format) -> (Vec<&'a str>, Vec<(String, String)>) {
    let mut kept = Vec::new();
    let mut defs: Vec<(String, String)> = Vec::new();
    let mut open = false;
    for &line in lines {
        let chars: Vec<char> = line.chars().collect();
        let indent = chars.iter().take_while(|c| c.is_whitespace()).count();
        let def = (chars.get(indent) == Some(&'[')).then(|| footnotes::at(&chars, indent, format)).flatten().filter(|n| n.definition);
        if let Some(n) = def {
            defs.push((n.label, chars[n.end..].iter().collect::<String>().trim().to_string()));
            open = true;
        } else if open && !line.trim().is_empty() && format.heading_level_of(line).is_none() {
            let text = &mut defs.last_mut().expect("a definition is open").1;
            text.push(' ');
            text.push_str(line.trim());
        } else if format == Format::Org && format.heading_level_of(line).is_some() && line.trim_start_matches('*').trim() == "Footnotes" {
            open = false;
        } else {
            open = false;
            kept.push(line);
        }
    }
    (kept, defs)
}

/// What the HTML of a page is made with.
struct Doc<'a> {
    keywords: Option<&'a TodoKeywords>,
    image: &'a dyn Fn(&str) -> String,
    /// Levels the headlines go down by (one below the title, when there is one).
    shift: usize,
    /// The page's format.
    format: Format,
    /// The links not of org-mode read.
    extended: place::LinkSyntax,
    /// The definitions of the footnotes, by label.
    defs: Vec<(String, String)>,
    /// The footnotes referred to, in the order of their first reference: their
    /// label (none for an inline one) and the text of an inline one.
    notes: std::cell::RefCell<Vec<(String, Option<String>)>>,
}

impl Doc<'_> {
    /// A Markdown page.
    fn md(&self) -> bool {
        self.format == Format::Markdown
    }

    /// The HTML of `lines`: a page, or the inside of a block.
    fn body(&self, lines: &[&str]) -> String {
        let mut out = String::new();
        let mut para: Vec<&str> = Vec::new();
        // The width `#+ATTR_ORG: :width` gives the next image
        let mut width: Option<String> = None;
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];
            let t = line.trim();
            if t.is_empty() || is_structure(line, self.format) { self.paragraph(&mut out, &mut para); }
            if t.is_empty() { i += 1; continue; }
            if self.md() && let Some((html, n)) = self.markdown_block(lines, i) {
                out.push_str(&html);
                i += n;
                continue;
            }
            // A block: `#+BEGIN_SRC` … `#+END_SRC`
            if let Some(kind) = block_kind(t)
                && let Some(end) = lines[i + 1..].iter().position(|l| l.trim().eq_ignore_ascii_case(&format!("#+end_{kind}")))
            {
                out.push_str(&self.block(&kind, &lines[i + 1..i + 1 + end]));
                i += end + 2;
                continue;
            }
            let lower = t.to_ascii_lowercase();
            if lower.starts_with("#+attr_org:") || lower.starts_with("#+attr_html:") {
                width = attr_width(t);
                i += 1;
                continue;
            }
            // Keywords and comments are not shown
            if !self.md() && (t.starts_with("#+") || t == "#" || t.starts_with("# ")) { i += 1; continue; }
            // A drawer (`:PROPERTIES:` … `:END:`)
            if is_drawer(t) && let Some(end) = lines[i + 1..].iter().position(|l| l.trim().eq_ignore_ascii_case(":end:")) {
                i += end + 2;
                continue;
            }
            if let Some(level) = self.format.heading_level_of(line) {
                out.push_str(&self.headline(line, level));
                i += 1;
                continue;
            }
            if t.starts_with('|') {
                let n = lines[i..].iter().take_while(|l| l.trim().starts_with('|')).count();
                out.push_str(&self.table(&lines[i..i + n]));
                i += n;
                continue;
            }
            if t == ":" || t.starts_with(": ") {
                let n = lines[i..].iter().take_while(|l| { let t = l.trim(); t == ":" || t.starts_with(": ") }).count();
                let text: Vec<&str> = lines[i..i + n].iter().map(|l| { let t = l.trim(); t.get(2..).unwrap_or("") }).collect();
                out.push_str(&format!("<pre>{}</pre>", escape(&text.join("\n"))));
                i += n;
                continue;
            }
            if t.len() >= 5 && t.chars().all(|c| c == '-') {
                out.push_str("<hr>");
                i += 1;
                continue;
            }
            if list_item(line, self.format).is_some() {
                let n = list_lines(&lines[i..], self.format);
                out.push_str(&self.list(&lines[i..i + n]));
                i += n;
                continue;
            }
            if let Some((target, w)) = image_line(t) {
                out.push_str(&self.image(&target, w.or(width.take())));
                i += 1;
                continue;
            }
            para.push(t);
            i += 1;
        }
        self.paragraph(&mut out, &mut para);
        out
    }

    /// The reference to the footnote `n`, numbered in the order of the first ones.
    fn footnote_ref(&self, n: &footnotes::Note, chars: &[char]) -> String {
        // `[fn:` then the label: a `:` after it makes an inline note
        let after = n.start + 4 + n.label.chars().count();
        let inline = self.format == Format::Org && chars.get(after) == Some(&':');
        let mut notes = self.notes.borrow_mut();
        let k = if inline {
            // `[fn:: texte]`, `[fn:nom: texte]`: the text after the second `:`
            let text: String = chars[after + 1..n.end - 1].iter().collect();
            notes.push((n.label.clone(), Some(text.trim().to_string())));
            notes.len()
        } else {
            match notes.iter().position(|(l, t)| t.is_none() && *l == n.label) {
                Some(k) => k + 1,
                None => { notes.push((n.label.clone(), None)); notes.len() }
            }
        };
        format!("<sup class='fn'><a href='#fn-{k}' id='fnref-{k}'>{k}</a></sup>")
    }

    /// The footnotes referred to (then those defined without a reference), as a
    /// numbered list at the end of the page.
    fn footnotes(&self) -> String {
        let unused: Vec<String> = self.defs.iter().map(|(l, _)| l.clone())
            .filter(|l| !self.notes.borrow().iter().any(|(n, t)| t.is_none() && n == l)).collect();
        if self.notes.borrow().is_empty() && unused.is_empty() { return String::new(); }
        self.notes.borrow_mut().extend(unused.into_iter().map(|l| (l, None)));
        let mut items = String::new();
        // A note may refer to another one: the list grows as it is written
        let mut k = 0;
        loop {
            // Not borrowed while the note is written: it may add others
            let Some((label, inline)) = self.notes.borrow().get(k).cloned() else { break };
            k += 1;
            let text = inline.or_else(|| self.defs.iter().find(|(l, _)| *l == label).map(|(_, t)| t.clone())).unwrap_or_default();
            items.push_str(&format!("<li id='fn-{k}'>{} <a href='#fnref-{k}'>↩</a></li>", self.inline(&text)));
        }
        format!("<section class='footnotes'><hr><ol>{items}</ol></section>")
    }

    /// The lines of text gathered so far, as a paragraph.
    fn paragraph(&self, out: &mut String, para: &mut Vec<&str>) {
        if para.is_empty() { return; }
        let html: Vec<String> = para.iter().map(|l| match l.strip_suffix("\\\\") {
            Some(l) => format!("{}<br>", self.inline(l.trim_end())),
            None => self.inline(l),
        }).collect();
        out.push_str(&format!("<p>{}</p>", html.join("\n")));
        para.clear();
    }

    fn headline(&self, line: &str, level: usize) -> String {
        let chars: Vec<char> = line.chars().collect();
        let mut rest = chars[level..].iter().collect::<String>().trim().to_string();
        let mut html = String::new();
        let format = self.format;
        if let Some(k) = self.keywords.and_then(|k| k.keyword(&chars, format)) {
            let word: String = chars[k.start..k.end].iter().collect();
            html.push_str(&format!("<span class='{}'>{}</span> ", if k.done { "done" } else { "todo" }, escape(&word)));
            rest = chars[k.end..].iter().collect::<String>().trim().to_string();
        }
        // `## Titre ##`: the closing marks go
        if self.md() { rest = rest.trim_end_matches('#').trim_end().to_string(); }
        let mut tags = String::new();
        if let Some((before, last)) = rest.rsplit_once(' ').filter(|_| !self.md())
            && last.len() > 2 && last.starts_with(':') && last.ends_with(':') && !last.contains("::")
        {
            tags = last.trim_matches(':').split(':').map(|t| format!("<span class='tag'>{}</span>", escape(t))).collect();
            rest = before.trim_end().to_string();
        }
        let h = (level + self.shift).min(6);
        format!("<h{h}>{html}{}{}</h{h}>", self.inline(&rest), if tags.is_empty() { tags } else { format!(" {tags}") })
    }

    fn block(&self, kind: &str, lines: &[&str]) -> String {
        let text = || escape(&lines.join("\n"));
        match kind {
            "src" | "example" => format!("<pre>{}</pre>", text()),
            "comment" | "export" => String::new(),
            "quote" => format!("<blockquote>{}</blockquote>", self.body(lines)),
            "center" => format!("<div class='center'>{}</div>", self.body(lines)),
            "verse" => format!("<p class='verse'>{}</p>", lines.iter().map(|l| self.inline(l)).collect::<Vec<_>>().join("<br>")),
            _ => format!("<div class='{}'>{}</div>", escape(kind), self.body(lines)),
        }
    }

    /// Rows above the first `|---|` line are headers.
    fn table(&self, lines: &[&str]) -> String {
        let is_rule = |l: &str| crate::tables::is_rule(l);
        let head = lines.iter().position(|l| is_rule(l)).filter(|&n| n > 0 && n < lines.len() - 1).unwrap_or(0);
        let mut out = String::from("<table>");
        for (n, line) in lines.iter().enumerate().filter(|(_, l)| !is_rule(l)) {
            let t = line.trim();
            let t = t.strip_prefix('|').unwrap_or(t);
            let t = t.strip_suffix('|').unwrap_or(t);
            let cell = if n < head { "th" } else { "td" };
            out.push_str("<tr>");
            for c in t.split('|') { out.push_str(&format!("<{cell}>{}</{cell}>", self.inline(c.trim()))); }
            out.push_str("</tr>");
        }
        out.push_str("</table>");
        out
    }

    /// Nested lists: an item deeper than the one before starts a list in it.
    fn list(&self, lines: &[&str]) -> String {
        // The items, with the lines continuing them
        let mut items: Vec<(usize, bool, String)> = Vec::new();
        for line in lines {
            match list_item(line, self.format) {
                Some((indent, ordered, text)) => items.push((indent, ordered, text.to_string())),
                None => if let Some(last) = items.last_mut() {
                    last.2.push(' ');
                    last.2.push_str(line.trim());
                },
            }
        }
        let tag = |ordered: bool| if ordered { "ol" } else { "ul" };
        let mut out = String::new();
        let mut open: Vec<(usize, bool)> = Vec::new();
        for (indent, ordered, text) in &items {
            while open.last().is_some_and(|&(i, _)| *indent < i) {
                let (_, o) = open.pop().unwrap_or_default();
                out.push_str(&format!("</li></{}>", tag(o)));
            }
            // Numbered after bulleted (or the reverse): another list
            if open.last().is_some_and(|&(i, o)| i == *indent && o != *ordered) {
                open.pop();
                out.push_str(&format!("</li></{}>", tag(!*ordered)));
            }
            if open.last().is_some_and(|&(i, _)| i == *indent) {
                out.push_str("</li>");
            } else {
                open.push((*indent, *ordered));
                out.push_str(&format!("<{}>", tag(*ordered)));
            }
            out.push_str(&format!("<li>{}", self.item(text.trim())));
        }
        while let Some((_, o)) = open.pop() { out.push_str(&format!("</li></{}>", tag(o))); }
        out
    }

    /// An item's text: its checkbox, its `term ::` and the rest.
    fn item(&self, text: &str) -> String {
        let (check, text) = match text.get(..4) {
            Some("[ ] ") => ("☐ ", &text[4..]),
            Some("[X] " | "[x] ") => ("☑ ", &text[4..]),
            Some("[-] ") => ("◩ ", &text[4..]),
            _ => ("", text),
        };
        match text.split_once(" :: ") {
            Some((term, rest)) => format!("{check}<b>{}</b> : {}", self.inline(term), self.inline(rest)),
            None => format!("{check}{}", self.inline(text)),
        }
    }

    /// An image alone on its line.
    fn image(&self, target: &str, width: Option<String>) -> String {
        format!("<p class='image'>{}</p>", self.img(target, width))
    }

    fn img(&self, target: &str, width: Option<String>) -> String {
        let width = width.map(|w| format!(" style='width: {}'", escape(&w))).unwrap_or_default();
        format!("<img src='{}' alt=''{width}>", escape(&(self.image)(target)))
    }

    /// Inline markup: links, emphasis, verbatim, addresses.
    fn inline(&self, s: &str) -> String {
        let chars: Vec<char> = s.chars().collect();
        let mut out = String::new();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            // Markdown: an image in the text, `![texte](cible)`
            if self.md() && c == '!' && chars.get(i + 1) == Some(&'[') && let Some(f) = place::markdown_target(&chars, i + 1) {
                out.push_str(&self.img(&f.0, None));
                i = f.1;
                continue;
            }
            if c == '[' && let Some(n) = footnotes::at(&chars, i, self.format) {
                out.push_str(&self.footnote_ref(&n, &chars));
                i = n.end;
                continue;
            }
            if c == '[' && let Some((target, desc, end)) = self.link_at(&chars, i) {
                out.push_str(&self.link(&target, desc.as_deref()));
                i = end;
                continue;
            }
            if self.md() && let Some((html, end)) = self.markdown_inline(&chars, i) {
                out.push_str(&html);
                i = end;
                continue;
            }
            // `<<cible>>` is not shown, `<<<radio>>>` shows its text
            if c == '<' && chars.get(i + 1) == Some(&'<') && let Some(end) = (i + 2..chars.len()).find(|&j| chars[j] == '>' && chars.get(j + 1) == Some(&'>')) {
                let inner: String = chars[i + 2..end].iter().collect();
                if let Some(radio) = inner.strip_prefix('<') { out.push_str(&escape(radio)); }
                i = end + 2;
                while chars.get(i) == Some(&'>') { i += 1; }
                continue;
            }
            if !self.md() && "*/_+=~".contains(c) && let Some(end) = emphasis_end(&chars, i) {
                let inner: String = chars[i + 1..end].iter().collect();
                out.push_str(&match c {
                    '*' => format!("<b>{}</b>", self.inline(&inner)),
                    '/' => format!("<i>{}</i>", self.inline(&inner)),
                    '_' => format!("<u>{}</u>", self.inline(&inner)),
                    '+' => format!("<s>{}</s>", self.inline(&inner)),
                    _ => format!("<code>{}</code>", escape(&inner)),
                });
                i = end + 1;
                continue;
            }
            // An address written as is
            if (i == 0 || chars[i - 1].is_whitespace()) && (starts_with(&chars[i..], "https://") || starts_with(&chars[i..], "http://")) {
                let end = (i..chars.len()).find(|&j| chars[j].is_whitespace()).unwrap_or(chars.len());
                let url: String = chars[i..end].iter().collect();
                out.push_str(&format!("<a href='{0}'>{0}</a>", escape(&url)));
                i = end;
                continue;
            }
            out.push_str(&escape(&c.to_string()));
            i += 1;
        }
        out
    }

    /// The link at `start`: its target as org-mode writes it (`Page::*Titre`), its
    /// description, and where it ends; the extended ones too when they are read.
    fn link_at(&self, chars: &[char], start: usize) -> Option<(String, Option<String>, usize)> {
        let f = place::link_starting(chars, start, self.extended)?;
        let text = |a: usize, b: usize| chars[a..b].iter().collect::<String>();
        let desc = match f.kind {
            place::Kind::Org => (f.start + 2..f.end - 2).find(|&j| chars[j] == ']' && chars[j + 1] == '[')
                .map(|j| text(j + 2, f.end - 2))
                .or_else(|| self.extended.pipe.then(|| (f.start + 2..f.end - 2).find(|&j| chars[j] == '|').map(|j| text(j + 1, f.end - 2))).flatten()),
            place::Kind::Markdown => (f.start + 1..f.end).find(|&j| chars[j] == ']').map(|j| text(f.start + 1, j)),
        };
        Some((f.target, desc, f.end))
    }

    fn link(&self, target: &str, desc: Option<&str>) -> String {
        let target = target.trim();
        if desc.is_none() && images::is_image(target) { return self.img(target, None); }
        let shown = |default: String| desc.map_or_else(|| escape(&default), |d| self.inline(d));
        if target.contains("://") || target.starts_with("mailto:") {
            return format!("<a href='{}'>{}</a>", escape(target), shown(target.to_string()));
        }
        if links::file_target(target).is_some() {
            return format!("<span class='link'>{}</span>", shown(target.to_string()));
        }
        // A page, a place in one: `Page › Titre`
        let label = match place::split(target) {
            (page, None) => page.to_string(),
            ("", Some(at)) => at.trim_start_matches(['*', '#']).to_string(),
            (page, Some(at)) => format!("{page} › {}", at.trim_start_matches(['*', '#'])),
        };
        format!("<span class='link'>{}</span>", shown(label))
    }
}

#[cfg(test)]
mod tests;
