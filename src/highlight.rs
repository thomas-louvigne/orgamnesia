use crate::motion::Hashtags;

thread_local! {
    /// How `#tags` are drawn as links (set by `render`, read while highlighting a line).
    static HASHTAGS: std::cell::Cell<Hashtags> = const { std::cell::Cell::new(Hashtags::Off) };
}

/// Render org-mode text as syntax-highlighted HTML for the editor overlay.
/// Pure function — no side effects, easy to test.
pub fn render(content: &str, hashtags: Hashtags) -> String {
    HASHTAGS.with(|h| h.set(hashtags));
    let mut out = String::with_capacity(content.len() * 2);
    for line in content.split('\n') {
        out.push_str(&highlight_line(line));
        out.push('\n');
    }
    out
}

/// Plain-text copy of the document with the char range `start..end` wrapped in
/// `<mark>`, drawn (transparent text) on top of the highlight layer to show the
/// Emacs region. Same trailing-newline handling as `render` so the layers line up.
pub fn render_region(content: &str, start: usize, end: usize) -> String {
    let chars: Vec<char> = content.chars().collect();
    let end = end.min(chars.len());
    let start = start.min(end);
    let part = |a: usize, b: usize| escape(&chars[a..b].iter().collect::<String>());
    format!("{}<mark class='region'>{}</mark>{}\n", part(0, start), part(start, end), part(end, chars.len()))
}

/// The page with the search matches (`(start, end)` char ranges, in order)
/// marked, `current` being the selected one; same layout as the text.
pub fn render_matches(content: &str, matches: &[(usize, usize)], current: Option<usize>) -> String {
    if matches.is_empty() { return String::new(); }
    let chars: Vec<char> = content.chars().collect();
    let part = |a: usize, b: usize| escape(&chars[a.min(chars.len())..b.min(chars.len())].iter().collect::<String>());
    let mut out = String::new();
    let mut at = 0;
    for (i, &(a, b)) in matches.iter().enumerate() {
        if a < at { continue; }
        let class = if Some(i) == current { "match current" } else { "match" };
        out.push_str(&part(at, a));
        out.push_str(&format!("<mark class='{class}'>{}</mark>", part(a, b)));
        at = b;
    }
    out.push_str(&part(at, chars.len()));
    out.push('\n');
    out
}

fn highlight_line(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let text = |a: usize, b: usize| chars[a..b].iter().collect::<String>();

    // Headline: count leading '*'
    let stars = line.bytes().take_while(|&b| b == b'*').count();
    if stars > 0 && matches!(line.as_bytes().get(stars), Some(b' ') | None) {
        let level = stars.min(6);
        let title_start = (stars + 1).min(chars.len());
        let (title, tags_html) = match crate::motion::headline_tags_range(&chars) {
            Some((a, b)) => (text(title_start, a), format!(
                "<span class='tags'>{}</span>{}", tag_links(&text(a, b)), escape(&text(b, chars.len())))),
            None => (text(title_start, chars.len()), String::new()),
        };
        return format!(
            "<span class='h{level}'>\
             <span class='h-stars'>{}</span>{}\
             {}{}\
             </span>",
            "*".repeat(stars),
            if stars < chars.len() { " " } else { "" },
            inline_html(&title),
            tags_html,
        );
    }

    // `#+FILETAGS: :a:b:`
    if let Some(v) = crate::motion::filetags_value_start(&chars) {
        return format!("<span class='kw'>{}</span><span class='tags'>{}</span>",
            escape(&text(0, v)), tag_links(&text(v, chars.len())));
    }

    // Table line
    if line.trim_start().starts_with('|') {
        return format!("<span class='tbl'>{}</span>", escape(line));
    }

    // Regular paragraph line
    inline_html(line)
}

/// Org-mode tags (`:a:b:`) drawn as links to their pages; separators left as they are.
fn tag_links(s: &str) -> String {
    let mut out = String::new();
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        if !word.is_empty() { out.push_str(&format!("<span class='tag-t'>{}</span>", escape(word))); }
        word.clear();
    };
    for c in s.chars() {
        if crate::motion::is_org_tag_char(c) { word.push(c); }
        else { flush(&mut word, &mut out); out.push_str(&escape_char(c)); }
    }
    flush(&mut word, &mut out);
    out
}

/// Convert inline org-mode markup to highlighted HTML.
fn inline_html(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;

    while i < chars.len() {
        // Wiki link [[...]]
        if chars[i] == '[' && chars.get(i + 1) == Some(&'[') && let Some((html, len)) = try_link(&chars, i) {
            out.push_str(&html);
            i += len;
            continue;
        }
        // #hashtag
        if chars[i] == '#'
            && let Some((tag, end)) = crate::motion::hashtag_at(&chars, i, HASHTAGS.with(|h| h.get()))
        {
            out.push_str(&format!(
                "<span class='link'>#<span class='link-t'>{}</span></span>", escape(&tag)));
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

fn try_link(chars: &[char], start: usize) -> Option<(String, usize)> {
    let mut i = start + 2;
    let mut target = String::new();

    while i < chars.len() {
        match (chars[i], chars.get(i + 1)) {
            (']', Some(&']')) => {
                let html = format!(
                    "<span class='link'>[[<span class='link-t'>{}</span>]]</span>",
                    escape(&target)
                );
                return Some((html, i + 2 - start));
            }
            (']', Some(&'[')) => {
                i += 2;
                let mut disp = String::new();
                while i < chars.len() {
                    if chars[i] == ']' && chars.get(i + 1) == Some(&']') {
                        let html = format!(
                            "<span class='link'>[[<span class='link-t'>{}</span>]\
                            [<span class='link-d'>{}</span>]]</span>",
                            escape(&target),
                            escape(&disp)
                        );
                        return Some((html, i + 2 - start));
                    }
                    disp.push(chars[i]);
                    i += 1;
                }
                return None;
            }
            _ => { target.push(chars[i]); i += 1; }
        }
    }
    None
}

fn try_em(chars: &[char], start: usize, delim: char, class: &str) -> Option<(String, usize)> {
    if start + 2 >= chars.len() { return None; }
    let next = chars[start + 1];
    if next.is_whitespace() || next == delim { return None; }
    let mut i = start + 1;
    while i < chars.len() {
        if chars[i] == delim && !chars[i - 1].is_whitespace() && i > start + 1 {
            let inner: String = chars[start + 1..i].iter().collect();
            let html = format!(
                "<span class='{class}'>{delim}{}{delim}</span>",
                escape(&inner)
            );
            return Some((html, i + 1 - start));
        }
        i += 1;
    }
    None
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
}
