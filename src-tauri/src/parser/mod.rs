pub mod types;

use types::*;

// ─── Public API ──────────────────────────────────────────────────────────────

pub fn parse_document(content: &str) -> Document {
    let lines: Vec<&str> = content.lines().collect();
    Document { blocks: parse_blocks(&lines, 0) }
}

pub use orgamnesia_core::hashtags::{hashtag_at, Hashtags};

/// Extract all `[[target]]` link targets — used for backlink indexing.
/// Unless `hashtags` is `Off`, every `#tag` counts as a link to the page `tag`.
/// Org-mode tags (`#+FILETAGS:`, `* Title :tag:`) are links to their page too.
pub fn extract_links(content: &str, hashtags: Hashtags) -> Vec<String> {
    let chars: Vec<char> = content.chars().collect();
    let mut links = Vec::new();
    let mut i = 0;
    while i + 1 < chars.len() {
        if chars[i] == '[' && chars[i + 1] == '[' {
            i += 2;
            let mut target = String::new();
            while i < chars.len() && chars[i] != ']' && chars[i] != '[' {
                target.push(chars[i]);
                i += 1;
            }
            // skip optional display text [display]]
            if i < chars.len() && chars[i] == '[' {
                while i < chars.len() && chars[i] != ']' { i += 1; }
                if i < chars.len() { i += 1; }
            }
            // skip closing ]]
            if i + 1 < chars.len() && chars[i] == ']' && chars[i + 1] == ']' {
                i += 2;
            }
            let t = target.trim().to_string();
            if !t.is_empty() { links.push(t); }
        } else if let Some((tag, end)) = hashtag_at(&chars, i, hashtags) {
            links.push(tag);
            i = end;
        } else {
            i += 1;
        }
    }
    links.extend(crate::tags::written_tags(content));
    links
}

/// True for `[[targets]]` that name a wiki page (not a URL, a file or an internal anchor).
pub fn is_page_link(target: &str) -> bool {
    let t = target.trim();
    !(t.is_empty()
        || t.contains("://")
        || t.starts_with("file:")
        || t.starts_with("mailto:")
        || t.starts_with('#')
        || t.starts_with('*')
        || t.starts_with("./")
        || t.starts_with("../"))
}

/// Rewrite every `[[old]]` / `[[old][label]]` link so it targets `new`.
/// With `ignore_case`, `[[pageMagique]]` also matches `PageMagique`, and `[[elody]]` `Élody`.
/// Org-mode tags naming `old` are renamed too, when `new` can be a tag.
/// Returns `None` when nothing changed. The display label is left untouched.
pub fn rewrite_links(content: &str, old_name: &str, new: &str, ignore_case: bool, hashtags: Hashtags) -> Option<String> {
    let chars: Vec<char> = content.chars().collect();
    let norm = |s: &str| orgamnesia_core::names::page_key(s, ignore_case);
    let old = norm(old_name);
    let mut out = String::with_capacity(content.len());
    let mut changed = false;
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '[' && i + 1 < chars.len() && chars[i + 1] == '[' {
            out.push_str("[[");
            i += 2;
            let mut target = String::new();
            while i < chars.len() && chars[i] != ']' && chars[i] != '[' {
                target.push(chars[i]);
                i += 1;
            }
            if norm(target.trim()) == old {
                let lead: String = target.chars().take_while(|c| c.is_whitespace()).collect();
                let trail: String = target.chars().rev().take_while(|c| c.is_whitespace()).collect();
                out.push_str(&lead);
                out.push_str(new);
                out.push_str(&trail);
                changed = true;
            } else {
                out.push_str(&target);
            }
        } else if let Some((tag, end)) = hashtag_at(&chars, i, hashtags) {
            if norm(&tag) == old {
                // A name that is not a valid tag (spaces…) becomes a regular link
                if new.chars().all(|c| hashtags.is_tag_char(c)) { out.push('#'); out.push_str(new); }
                else { out.push_str(&format!("[[{new}]]")); }
                changed = true;
            } else {
                out.extend(&chars[i..end]);
            }
            i = end;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    let text = if changed { out } else { content.to_string() };
    crate::tags::rename(&text, old_name, new, ignore_case).or(changed.then_some(text))
}

// ─── Block parsing ───────────────────────────────────────────────────────────

fn parse_blocks(lines: &[&str], parent_level: u8) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if let Some(level) = headline_level(line) {
            if level <= parent_level { break; }
            let (block, consumed) = parse_headline_block(lines, i, level);
            blocks.push(block);
            i += consumed;
            continue;
        }
        if is_table_line(line) {
            let (block, consumed) = parse_table_block(lines, i);
            blocks.push(block);
            i += consumed;
            continue;
        }
        if line.trim().is_empty() {
            blocks.push(Block::Empty);
            i += 1;
            continue;
        }
        // Paragraph: collect consecutive non-special lines
        let start = i;
        while i < lines.len()
            && !lines[i].trim().is_empty()
            && headline_level(lines[i]).is_none()
            && !is_table_line(lines[i])
        {
            i += 1;
        }
        let text = lines[start..i].join("\n");
        blocks.push(Block::Paragraph { inlines: parse_inline(&text) });
    }
    blocks
}

fn parse_headline_block(lines: &[&str], start: usize, level: u8) -> (Block, usize) {
    let rest = &lines[start][(level as usize + 1)..];
    let (title_str, tags) = split_title_tags(rest);
    let title = parse_inline(title_str.trim());
    // collect child lines until next headline of same/higher level
    let mut i = start + 1;
    while i < lines.len() {
        if let Some(l) = headline_level(lines[i]) && l <= level { break; }
        i += 1;
    }
    let children = parse_blocks(&lines[start + 1..i], level);
    (Block::Headline(Headline { level, title, tags, children }), i - start)
}

fn parse_table_block(lines: &[&str], start: usize) -> (Block, usize) {
    let mut rows = Vec::new();
    let mut i = start;
    while i < lines.len() && is_table_line(lines[i]) {
        rows.push(parse_table_row(lines[i]));
        i += 1;
    }
    (Block::Table(Table { rows }), i - start)
}

fn parse_table_row(line: &str) -> TableRow {
    let t = line.trim();
    // Separator: |---+---| — only pipes, dashes, pluses, colons
    if t.chars().all(|c| matches!(c, '|' | '-' | '+' | ':' | ' ')) {
        return TableRow { cells: vec![], is_separator: true };
    }
    let mut cells: Vec<String> = t.split('|')
        .skip(1)
        .map(|s| s.trim().to_string())
        .collect();
    // remove trailing empty from final '|'
    if cells.last().map(|s: &String| s.is_empty()).unwrap_or(false) {
        cells.pop();
    }
    TableRow { cells, is_separator: false }
}

fn headline_level(line: &str) -> Option<u8> {
    let n = line.chars().take_while(|&c| c == '*').count();
    if n == 0 { return None; }
    match line.as_bytes().get(n) {
        Some(b' ') | None => Some(n as u8),
        _ => None,
    }
}

fn is_table_line(line: &str) -> bool {
    line.trim_start().starts_with('|')
}

/// Split "Title :tag1:tag2:" → ("Title", vec!["tag1", "tag2"])
fn split_title_tags(s: &str) -> (&str, Vec<String>) {
    crate::tags::split_headline_tags(s)
}

// ─── Inline parsing ──────────────────────────────────────────────────────────

pub fn parse_inline(input: &str) -> Vec<Inline> {
    let chars: Vec<char> = input.chars().collect();
    let mut result = Vec::new();
    let mut text = String::new();
    let mut i = 0;

    macro_rules! flush_text {
        () => {
            if !text.is_empty() {
                result.push(Inline::Text { content: text.clone() });
                text.clear();
            }
        };
    }

    while i < chars.len() {
        if chars[i] == '[' && chars.get(i + 1) == Some(&'[')
            && let Some((link, consumed)) = parse_wiki_link(&chars, i)
        {
            flush_text!();
            result.push(link);
            i += consumed;
            continue;
        }
        if let Some((ctor, consumed)) = try_emphasis(&chars, i) {
            flush_text!();
            let inner: String = chars[i + 1..i + consumed - 1].iter().collect();
            let children = parse_inline(&inner);
            result.push(ctor(children));
            i += consumed;
            continue;
        }
        text.push(chars[i]);
        i += 1;
    }
    flush_text!();
    result
}

fn parse_wiki_link(chars: &[char], start: usize) -> Option<(Inline, usize)> {
    debug_assert!(chars[start] == '[' && chars[start + 1] == '[');
    let mut i = start + 2;
    let mut target = String::new();

    while i < chars.len() {
        match (chars[i], chars.get(i + 1)) {
            (']', Some(&']')) => {
                return Some((
                    Inline::WikiLink { target: target.trim().to_string(), display: None },
                    i + 2 - start,
                ));
            }
            (']', Some(&'[')) => {
                i += 2;
                let mut display = String::new();
                while i < chars.len() {
                    if chars[i] == ']' && chars.get(i + 1) == Some(&']') {
                        return Some((
                            Inline::WikiLink {
                                target: target.trim().to_string(),
                                display: Some(display.trim().to_string()),
                            },
                            i + 2 - start,
                        ));
                    }
                    display.push(chars[i]);
                    i += 1;
                }
                return None;
            }
            _ => { target.push(chars[i]); i += 1; }
        }
    }
    None
}

type InlineCtor = fn(Vec<Inline>) -> Inline;

fn try_emphasis(chars: &[char], start: usize) -> Option<(InlineCtor, usize)> {
    let (delim, ctor): (char, InlineCtor) = match chars[start] {
        '*' => ('*', |c| Inline::Bold { children: c }),
        '/' => ('/', |c| Inline::Italic { children: c }),
        '_' => ('_', |c| Inline::Underline { children: c }),
        _ => return None,
    };
    if start + 2 >= chars.len() { return None; }
    let after_open = chars[start + 1];
    // opening delimiter must not be followed by whitespace or the same delimiter
    if after_open.is_whitespace() || after_open == delim { return None; }

    let mut i = start + 1;
    while i < chars.len() {
        if chars[i] == delim && i > start + 1 && !chars[i - 1].is_whitespace() {
            return Some((ctor, i + 1 - start));
        }
        i += 1;
    }
    None
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── headline_level ──

    #[test]
    fn headline_level_one_star() {
        assert_eq!(headline_level("* Hello"), Some(1));
    }

    #[test]
    fn headline_level_three_stars() {
        assert_eq!(headline_level("*** Deep"), Some(3));
    }

    #[test]
    fn headline_level_not_headline_bold() {
        assert_eq!(headline_level("*bold*"), None);
    }

    #[test]
    fn headline_level_not_headline_no_space() {
        assert_eq!(headline_level("**nospace"), None);
    }

    #[test]
    fn headline_level_plain_text() {
        assert_eq!(headline_level("Normal text"), None);
    }

    // ── parse_inline ──

    #[test]
    fn inline_plain_text() {
        let r = parse_inline("hello world");
        assert_eq!(r.len(), 1);
        assert!(matches!(&r[0], Inline::Text { content } if content == "hello world"));
    }

    #[test]
    fn inline_bold() {
        let r = parse_inline("say *hello* now");
        assert_eq!(r.len(), 3);
        assert!(matches!(&r[1], Inline::Bold { .. }));
    }

    #[test]
    fn inline_italic() {
        let r = parse_inline("/italic/");
        assert_eq!(r.len(), 1);
        assert!(matches!(&r[0], Inline::Italic { .. }));
    }

    #[test]
    fn inline_underline() {
        let r = parse_inline("_under_");
        assert_eq!(r.len(), 1);
        assert!(matches!(&r[0], Inline::Underline { .. }));
    }

    #[test]
    fn inline_wiki_link_simple() {
        let r = parse_inline("see [[My Page]] here");
        assert_eq!(r.len(), 3);
        assert!(matches!(&r[1],
            Inline::WikiLink { target, display: None } if target == "My Page"
        ));
    }

    #[test]
    fn inline_wiki_link_with_display() {
        let r = parse_inline("[[Target][Click here]]");
        assert_eq!(r.len(), 1);
        assert!(matches!(&r[0],
            Inline::WikiLink { target, display: Some(d) }
            if target == "Target" && d == "Click here"
        ));
    }

    #[test]
    fn inline_no_bold_space_after_star() {
        let r = parse_inline("* not bold");
        // the * has a space after it — not emphasis
        assert!(matches!(&r[0], Inline::Text { .. }));
    }

    #[test]
    fn inline_nested_in_bold() {
        let r = parse_inline("*bold /and italic/*");
        // outer is bold, inner should have italic parsed inside
        assert!(matches!(&r[0], Inline::Bold { children } if !children.is_empty()));
    }

    // ── extract_links ──

    #[test]
    fn extract_links_finds_simple() {
        let links = extract_links("See [[Page One]] done", Hashtags::Off);
        assert_eq!(links, vec!["Page One"]);
    }

    #[test]
    fn extract_links_with_display() {
        let links = extract_links("[[Target][label]]", Hashtags::Off);
        assert_eq!(links, vec!["Target"]);
    }

    #[test]
    fn extract_links_multiple() {
        let links = extract_links("[[A]] and [[B]] and [[C][display]]", Hashtags::Off);
        assert_eq!(links, vec!["A", "B", "C"]);
    }

    #[test]
    fn page_links_vs_others() {
        assert!(is_page_link("My Page"));
        assert!(!is_page_link("https://orgmode.org"));
        assert!(!is_page_link("file:../assets/a.svg"));
        assert!(!is_page_link("../assets/a.svg"));
    }

    #[test]
    fn rewrite_links_simple_and_labeled() {
        let out = rewrite_links("a [[Old]] b [[Old][lbl]] c [[Other]]", "Old", "New", false, Hashtags::Off).unwrap();
        assert_eq!(out, "a [[New]] b [[New][lbl]] c [[Other]]");
    }

    #[test]
    fn rewrite_links_ignores_case() {
        let out = rewrite_links("[[pageMagique]] [[PAGEMAGIQUE][x]]", "PageMagique", "PageMagique3", true, Hashtags::Off).unwrap();
        assert_eq!(out, "[[PageMagique3]] [[PageMagique3][x]]");
    }

    #[test]
    fn rewrite_links_case_sensitive_when_asked() {
        assert!(rewrite_links("[[pageMagique]]", "PageMagique", "X", false, Hashtags::Off).is_none());
    }

    #[test]
    fn rewrite_links_no_match() {
        assert!(rewrite_links("[[Other]] and Old", "Old", "New", false, Hashtags::Off).is_none());
    }

    #[test]
    fn hashtags_are_links_only_when_enabled() {
        let t = "Voir #idée et (#Rust) #+TITLE: x # note a#b [[P]] #fin-";
        assert_eq!(extract_links(t, Hashtags::Dashes), vec!["idée", "Rust", "P", "fin"]);
        assert_eq!(extract_links(t, Hashtags::Off), vec!["P"]);
    }

    #[test]
    fn hashtag_dashes_only_when_allowed() {
        let t = "#mon-tag et #mon_tag";
        assert_eq!(extract_links(t, Hashtags::Dashes), vec!["mon-tag", "mon_tag"]);
        assert_eq!(extract_links(t, Hashtags::Org), vec!["mon", "mon_tag"]);
    }

    #[test]
    fn rewrite_hashtags() {
        let out = rewrite_links("a #Old b #Older", "old", "New", true, Hashtags::Dashes).unwrap();
        assert_eq!(out, "a #New b #Older");
        let out = rewrite_links("a #Old", "Old", "Deux mots", false, Hashtags::Dashes).unwrap();
        assert_eq!(out, "a [[Deux mots]]");
        // A dash is not allowed in an org-mode style tag
        let out = rewrite_links("a #Old", "Old", "New-name", false, Hashtags::Org).unwrap();
        assert_eq!(out, "a [[New-name]]");
        assert!(rewrite_links("a #Old", "Old", "New", false, Hashtags::Off).is_none());
    }

    #[test]
    fn org_tags_are_links() {
        let t = "#+FILETAGS: :projet:\n* T :urgent:projet:\n[[P]]";
        assert_eq!(extract_links(t, Hashtags::Off), vec!["P", "projet", "urgent", "projet"]);
        let out = rewrite_links(t, "projet", "Projet2", false, Hashtags::Off).unwrap();
        assert_eq!(out, "#+FILETAGS: :Projet2:\n* T :urgent:Projet2:\n[[P]]");
    }

    #[test]
    fn extract_links_empty() {
        assert!(extract_links("no links here", Hashtags::Off).is_empty());
    }

    // ── split_title_tags ──

    #[test]
    fn tags_none() {
        let (t, tags) = split_title_tags("Hello World");
        assert_eq!(t, "Hello World");
        assert!(tags.is_empty());
    }

    #[test]
    fn tags_single() {
        let (t, tags) = split_title_tags("My Title :work:");
        assert_eq!(t, "My Title");
        assert_eq!(tags, vec!["work"]);
    }

    #[test]
    fn tags_multiple() {
        let (t, tags) = split_title_tags("Headline :tag1:tag2:");
        assert_eq!(t, "Headline");
        assert_eq!(tags, vec!["tag1", "tag2"]);
    }

    // ── parse_table_row ──

    #[test]
    fn table_row_data() {
        let row = parse_table_row("| foo | bar | baz |");
        assert!(!row.is_separator);
        assert_eq!(row.cells, vec!["foo", "bar", "baz"]);
    }

    #[test]
    fn table_row_separator() {
        let row = parse_table_row("|---+---+---|");
        assert!(row.is_separator);
    }

    // ── parse_document ──

    #[test]
    fn document_headline_and_paragraph() {
        let doc = parse_document("* Intro\n\nSome text.\n");
        assert!(matches!(&doc.blocks[0], Block::Headline(h) if h.level == 1));
    }

    #[test]
    fn document_nested_headlines() {
        let doc = parse_document("* H1\n** H2\ntext\n");
        if let Block::Headline(h1) = &doc.blocks[0] {
            assert_eq!(h1.level, 1);
            assert!(matches!(&h1.children[0], Block::Headline(h2) if h2.level == 2));
        } else {
            panic!("expected Headline");
        }
    }

    #[test]
    fn document_table() {
        let content = "| A | B |\n|---+---|\n| 1 | 2 |\n";
        let doc = parse_document(content);
        assert!(matches!(&doc.blocks[0], Block::Table(t) if t.rows.len() == 3));
    }
}
