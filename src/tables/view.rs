//! The view of a page the editor's textarea holds: tables and images collapsed,
//! folds hidden, text under headlines indented (see the module above).

use super::{find_tables, fold_index, fold_tag, tag, tag_index, FILL, INDENT, MAX_FOLDS, MAX_TABLES};
use crate::images;

/// A table of the page drawn in place of its text.
#[derive(Debug, Clone, PartialEq)]
pub struct Collapsed {
    /// Its text in the page.
    pub raw: String,
    /// Where it starts in the page.
    pub start: usize,
    /// Its length in the page (chars).
    pub len: usize,
    /// Lines it takes in the view.
    pub lines: usize,
    /// Not a table but an image, drawn this way.
    pub image: Option<Shown>,
}

/// An image as the view draws it.
#[derive(Debug, Clone, PartialEq)]
pub struct Shown {
    /// URL of the image.
    pub src: String,
    /// Size drawn, in px.
    pub width: f64,
    pub height: f64,
}

/// What `View::build` replaces.
enum Block {
    Fold,
    Table,
    Image(Shown, usize),
}

/// Lines of the page hidden under a folded headline.
#[derive(Debug, Clone, PartialEq)]
pub struct Fold {
    /// Its text in the page: from the `\n` ending the headline to the end of the
    /// last hidden line.
    pub raw: String,
    /// Where it starts in the page.
    pub start: usize,
    /// Its length in the page (chars).
    pub len: usize,
}

/// A piece of the view, as `View::walk` reads it.
enum Piece {
    /// A character of the page.
    Char(char),
    /// A character of a collapsed table's block: the table, the line within the block.
    Table(usize, usize),
    /// The character standing for a fold.
    Fold(usize),
}

/// The page as the textarea shows it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct View {
    pub text: String,
    /// The collapsed tables and images, in order.
    pub tables: Vec<Collapsed>,
    /// Width of the text, in characters: how wide a drawn table can be.
    pub width: usize,
    /// The indented lines: where they start in the page, and by how many columns.
    pub indents: Vec<(usize, usize)>,
    /// The folded parts, in order.
    pub folds: Vec<Fold>,
}

impl View {
    /// The view of `content` where, with `collapse`, every table is collapsed but the one
    /// holding the page position `keep` (if any), the text being `width` characters
    /// wide. `lines_of` gives the lines a table takes once drawn. With `indent`, the
    /// text under a headline is indented to the column of its title. `folds` are the
    /// page ranges to hide (see `Fold`); those that don't fit the page are ignored.
    /// With `collapse`, image lines are drawn too, but the one holding `keep`: `image_of`
    /// gives how an image is drawn and the lines it takes, from its line, the width
    /// `#+ATTR_ORG:` gives it and the columns before its link (`None`: shown as text).
    #[allow(clippy::too_many_arguments)]
    pub fn build(
        content: &str, keep: Option<usize>, width: usize, mut lines_of: impl FnMut(&str) -> usize,
        mut image_of: impl FnMut(&images::ImageLine, Option<images::Width>, usize) -> Option<(Shown, usize)>,
        collapse: bool, indent: bool, folds: &[(usize, usize)],
    ) -> Self {
        let chars: Vec<char> = content.chars().collect();
        let mut all_indents = if indent { heading_indents(&chars) } else { vec![] };
        let mut indents = Vec::new();
        // The folds that fit the page (a `\n` at both ends), not inside another one
        let mut hidden: Vec<(usize, usize)> = Vec::new();
        let mut sorted = folds.to_vec();
        sorted.sort();
        for (a, b) in sorted {
            let fits = a < b && b <= chars.len() && chars[a] == '\n' && (b == chars.len() || chars[b] == '\n');
            if !fits || hidden.last().is_some_and(|&(_, end)| a < end) { continue; }
            if hidden.len() as u32 >= MAX_FOLDS { break; }
            hidden.push((a, b));
        }
        // The hidden lines get no indentation
        all_indents.retain(|&(at, _)| !hidden.iter().any(|&(a, b)| a < at && at <= b));
        let mut text = String::with_capacity(content.len() + all_indents.len() * 3);
        let mut next = 0;
        // Copy the page from `a` to `b`, each line with its indentation
        let mut copy = |text: &mut String, a: usize, b: usize| {
            for (p, c) in chars[a..b].iter().map(Some).chain([None]).enumerate() {
                let p = a + p;
                while next < all_indents.len() && all_indents[next].0 < p { next += 1; }
                if let Some(&(at, n)) = all_indents.get(next).filter(|&&(at, _)| at == p) {
                    text.extend(std::iter::repeat_n(INDENT, n));
                    indents.push((at, n));
                    next += 1;
                }
                if let Some(&c) = c { text.push(c); }
            }
        };
        // What the view replaces, in order: folds, and tables (but those folded)
        let mut blocks: Vec<(usize, usize, Block)> = hidden.iter().map(|&(a, b)| (a, b, Block::Fold)).collect();
        if collapse {
            for (start, end) in find_tables(&chars) {
                let folded = hidden.iter().any(|&(a, b)| start > a && start < b);
                if !folded && !keep.is_some_and(|k| (start..=end).contains(&k)) {
                    blocks.push((start, end, Block::Table));
                }
            }
            let mut start = 0;
            let mut above: Option<usize> = None;
            loop {
                let end = chars[start..].iter().position(|&c| c == '\n').map_or(chars.len(), |n| start + n);
                // Not in a fold, nor a folded headline (its fold starts at its end)
                let folded = hidden.iter().any(|&(a, b)| (start > a && start <= b) || a == end);
                if !folded && !keep.is_some_and(|k| (start..=end).contains(&k))
                    && let Some(image) = images::parse(&chars[start..end])
                {
                    let attr = above.and_then(|a| images::attr_width(&chars[a..start - 1].iter().collect::<String>()));
                    let indent = all_indents.iter().find(|&&(at, _)| at == start).map_or(0, |&(_, n)| n);
                    if let Some((shown, lines)) = image_of(&image, attr, indent + image.prefix) {
                        blocks.push((start + image.prefix, end, Block::Image(shown, lines)));
                    }
                }
                if end >= chars.len() { break; }
                above = Some(start);
                start = end + 1;
            }
        }
        blocks.sort_by_key(|&(start, end, _)| (start, end));
        let mut tables = Vec::new();
        let mut folded = Vec::new();
        let mut at = 0;
        for (start, end, block) in blocks {
            if !matches!(block, Block::Fold) && tables.len() as u32 >= MAX_TABLES { continue; }
            copy(&mut text, at, start);
            let raw: String = chars[start..end].iter().collect();
            let (lines, image) = match block {
                Block::Fold => {
                    text.push(fold_tag(folded.len()));
                    folded.push(Fold { raw, start, len: end - start });
                    at = end;
                    continue;
                }
                Block::Table => (lines_of(&raw).max(1), None),
                Block::Image(shown, lines) => (lines.max(1), Some(shown)),
            };
            text.push(tag(tables.len()));
            for _ in 1..lines {
                text.push('\n');
                text.push(FILL);
            }
            tables.push(Collapsed { raw, start, len: end - start, lines, image });
            at = end;
        }
        copy(&mut text, at, chars.len());
        View { text, tables, width, indents, folds: folded }
    }

    /// The page ranges folded in `text` (this view, possibly edited since): a fold
    /// whose character is no longer at the end of a line is opened.
    pub fn fold_ranges(&self, text: &str) -> Vec<(usize, usize)> {
        let chars: Vec<char> = text.chars().collect();
        let mut out = Vec::new();
        let mut content = 0;
        let mut last = None;
        self.walk(text, |i, piece| match piece {
            Piece::Char(_) => { content += 1; last = None; }
            Piece::Table(t, _) => {
                if last != Some(t) { content += self.tables[t].len; last = Some(t); }
            }
            Piece::Fold(f) => {
                let len = self.folds[f].len;
                if chars.get(i + 1).is_none_or(|&c| c == '\n') { out.push((content, content + len)); }
                content += len;
                last = None;
            }
        });
        out
    }

    /// The run of `INDENT`s starting the line of the view position `pos` in `text`,
    /// when `pos` is in it or right after it.
    pub fn indent_run(&self, text: &str, pos: usize) -> Option<(usize, usize)> {
        if self.indents.is_empty() { return None; }
        let chars: Vec<char> = text.chars().collect();
        let pos = pos.min(chars.len());
        let ls = chars[..pos].iter().rposition(|&c| c == '\n').map_or(0, |i| i + 1);
        let n = chars[ls..].iter().take_while(|&&c| c == INDENT).count();
        (n > 0 && pos <= ls + n).then_some((ls, ls + n))
    }

    /// Walk `text` (this view, possibly edited since) calling `on` for each piece:
    /// a page character, each char of a collapsed table's block (line within the
    /// block), or a fold. What is left of a block whose tag was deleted is skipped.
    fn walk(&self, text: &str, mut on: impl FnMut(usize, Piece)) {
        let chars: Vec<char> = text.chars().collect();
        let indented = !self.indents.is_empty();
        let mut line_start = true;
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if indented && line_start && c == INDENT {
                // Indentation of the view, not in the page
                i += 1;
                continue;
            }
            line_start = c == '\n';
            if let Some(f) = fold_index(c).filter(|&f| f < self.folds.len()) {
                on(i, Piece::Fold(f));
                i += 1;
            } else if let Some(t) = tag_index(c).filter(|&t| t < self.tables.len()) {
                on(i, Piece::Table(t, 0));
                i += 1;
                let mut line = 0;
                while i + 1 < chars.len() && chars[i] == '\n' && chars[i + 1] == FILL {
                    line += 1;
                    on(i, Piece::Table(t, line));
                    on(i + 1, Piece::Table(t, line));
                    i += 2;
                }
            } else if c == '\n' && chars.get(i + 1) == Some(&FILL) {
                // A line of a block whose first line was deleted
                line_start = false;
                i += 2;
            } else {
                if c != FILL && tag_index(c).is_none() && fold_index(c).is_none() { on(i, Piece::Char(c)); }
                i += 1;
            }
        }
    }

    /// The page text for `text`, this view after edits in the textarea.
    pub fn to_content(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut last = None;
        self.walk(text, |_, piece| match piece {
            Piece::Char(c) => { out.push(c); last = None; }
            Piece::Table(t, _) if last != Some(t) => { out.push_str(&self.tables[t].raw); last = Some(t); }
            Piece::Table(..) => {}
            Piece::Fold(f) => { out.push_str(&self.folds[f].raw); last = None; }
        });
        out
    }

    /// Page position of the view position `pos` in `text` (this view, possibly
    /// edited). Inside a collapsed table: the start of the row at about the same height.
    pub fn content_pos(&self, text: &str, pos: usize) -> usize {
        let mut content = 0;
        let mut found = None;
        let mut last = None;
        // Page position of the table being walked through
        let mut table_at = 0;
        self.walk(text, |i, piece| {
            if found.is_some() { return; }
            match piece {
                Piece::Char(_) => {
                    if i >= pos { found = Some(content); }
                    content += 1;
                    last = None;
                }
                // Before the fold: the end of the headline
                Piece::Fold(f) => {
                    if i >= pos { found = Some(content); }
                    content += self.folds[f].len;
                    last = None;
                }
                Piece::Table(t, line) => {
                    let table = &self.tables[t];
                    if last != Some(t) {
                        table_at = content;
                        content += table.len;
                        last = Some(t);
                    }
                    if i >= pos {
                        found = Some(table_at + row_start(&table.raw, line, table.lines));
                    }
                }
            }
        });
        found.unwrap_or(content)
    }

    /// View position of the page position `pos`; a position inside a collapsed
    /// table goes to the start of its block, one inside a fold before the fold.
    pub fn view_pos(&self, pos: usize) -> usize {
        // Indentation of the lines starting at or before `p` (a line start goes after it)
        let indent = |p: usize| -> isize {
            self.indents.iter().take_while(|&&(at, _)| at <= p).map(|&(_, n)| n as isize).sum()
        };
        // Collapsed tables and folds: where they start, their length, the chars they take
        let mut blocks: Vec<(usize, usize, usize)> = self.tables.iter()
            .map(|t| (t.start, t.len, 2 * t.lines - 1))
            .chain(self.folds.iter().map(|f| (f.start, f.len, 1)))
            .collect();
        blocks.sort();
        let mut shift: isize = 0;
        for (start, len, chars) in blocks {
            if pos <= start { break; }
            if pos < start + len {
                return (start as isize + shift + indent(start)) as usize;
            }
            shift += chars as isize - len as isize;
        }
        (pos as isize + shift + indent(pos)).max(0) as usize
    }
}

/// The lines under a headline: where they start and their indentation,
/// the width of the stars of the headline and the space after them.
fn heading_indents(chars: &[char]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut level = 0;
    let mut start = 0;
    loop {
        let end = chars[start..].iter().position(|&c| c == '\n').map_or(chars.len(), |p| start + p);
        let line = &chars[start..end];
        let stars = line.iter().take_while(|&&c| c == '*').count();
        if stars > 0 && matches!(line.get(stars), Some(' ') | None) {
            level = stars;
        } else if level > 0 {
            out.push((start, level + 1));
        }
        if end == chars.len() { break; }
        start = end + 1;
    }
    out
}

/// Position in `raw` of the start of the row shown at line `line` of `lines`.
fn row_start(raw: &str, line: usize, lines: usize) -> usize {
    let rows: Vec<&str> = raw.split('\n').collect();
    let row = (line * rows.len() / lines.max(1)).min(rows.len() - 1);
    rows[..row].iter().map(|r| r.chars().count() + 1).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "* Titre\n| a | b |\n|---+---|\n| c | d |\nfin\n| x |";

    fn view(keep: Option<usize>) -> View {
        View::build(PAGE, keep, 80, |raw| raw.split('\n').count() + 1, |_, _, _| None, true, false, &[])
    }

    #[test]
    fn collapsed_tables_map_back_to_the_page() {
        let v = view(None);
        assert_eq!(v.tables.len(), 2);
        assert_eq!(v.text.lines().count(), 1 + 4 + 1 + 2);
        assert_eq!(v.to_content(&v.text), PAGE);
        // An edit outside the tables
        let edited = v.text.replacen("fin", "FIN!", 1);
        assert_eq!(v.to_content(&edited), PAGE.replacen("fin", "FIN!", 1));
        // The table holding the kept position stays as text
        let v = view(Some(20));
        assert_eq!(v.tables.len(), 1);
        assert!(v.text.contains("| c | d |"));
        assert_eq!(v.to_content(&v.text), PAGE);
    }

    #[test]
    fn text_under_headlines_is_indented() {
        let page = "intro\n* A\ntexte\n- item\n** B\n\n| t |\nfin";
        let v = View::build(page, None, 80, |raw| raw.split('\n').count(), |_, _, _| None, true, true, &[]);
        let i = |n: usize| INDENT.to_string().repeat(n);
        assert_eq!(v.text, format!("intro\n* A\n{}texte\n{}- item\n** B\n{}\n{}\u{e000}\n{}fin", i(2), i(2), i(3), i(3), i(3)));
        assert_eq!(v.to_content(&v.text), page);
        // A line start goes after its indentation, and back
        let texte = page.find("texte").unwrap();
        assert_eq!(v.view_pos(texte), texte + 2);
        assert_eq!(v.content_pos(&v.text, texte + 2), texte);
        assert_eq!(v.content_pos(&v.text, texte + 1), texte);
        let fin = page.find("fin").unwrap();
        assert_eq!(v.content_pos(&v.text, v.view_pos(fin)), fin);
        // Typed in the indentation, a character goes to the start of the line
        let edited = v.text.replacen(&format!("{}texte", i(2)), &format!("{}X{}texte", i(1), i(1)), 1);
        assert_eq!(v.to_content(&edited), page.replacen("texte", "X\u{a0}texte", 1));
        // The indentation around a position
        assert_eq!(v.indent_run(&v.text, texte + 2), Some((texte, texte + 2)));
        assert_eq!(v.indent_run(&v.text, 2), None);
        // Not indented: the page as it is
        let plain = View::build(page, None, 80, |_| 1, |_, _, _| None, false, false, &[]);
        assert_eq!(plain.text, page);
    }

    #[test]
    fn image_lines_are_drawn_after_their_prefix() {
        let page = "a\n#+ATTR_ORG: :width 50%\n* [[x.png]]{:width 9}\nb";
        let mut seen = None;
        let build = |keep, seen: &mut Option<_>| View::build(page, keep, 80, |_| 1, |img, attr, before| {
            *seen = Some((img.target.clone(), attr, before));
            Some((Shown { src: "x".into(), width: 10.0, height: 40.0 }, 3))
        }, true, false, &[]);
        let v = build(None, &mut seen);
        assert_eq!(seen, Some(("x.png".into(), Some(images::Width::Part(0.5)), 2)));
        assert_eq!(v.text, "a\n#+ATTR_ORG: :width 50%\n* \u{e000}\n\u{2063}\n\u{2063}\nb");
        assert_eq!(v.to_content(&v.text), page);
        let link = page.find("[[").unwrap();
        assert_eq!(v.tables[0].start, link);
        // In the image's lines: its link; after them, the next line
        assert_eq!(v.content_pos(&v.text, v.text.chars().count() - 3), link);
        assert_eq!(v.view_pos(page.len() - 1), v.text.chars().count() - 1);
        // The caret on its line shows it as text
        assert_eq!(build(Some(link - 1), &mut seen).text, page);
    }

    #[test]
    fn folded_lines_are_hidden_and_kept() {
        let page = "* A\ntexte\n| t |\n* B\nfin";
        let fold = (3, 15); // from the end of `* A` to the end of `| t |`
        let v = View::build(page, None, 80, |_| 1, |_, _, _| None, true, false, &[fold]);
        assert_eq!(v.text, "* A\u{f800}\n* B\nfin");
        assert_eq!(v.to_content(&v.text), page);
        // Before the fold: the end of the headline; inside it: there too; after it: the next line
        assert_eq!(v.content_pos(&v.text, 3), 3);
        assert_eq!(v.view_pos(8), 3);
        let b = page.find("* B").unwrap();
        assert_eq!(v.view_pos(b), 5);
        assert_eq!(v.content_pos(&v.text, 5), b);
        // Typing above moves the fold along
        let edited = format!("x{}", v.text);
        assert_eq!(v.fold_ranges(&edited), vec![(4, 16)]);
        assert_eq!(v.to_content(&edited), format!("x{page}"));
        // A fold no longer at the end of a line is opened
        let joined = v.text.replacen("\u{f800}\n", "\u{f800}", 1);
        assert_eq!(v.fold_ranges(&joined), vec![]);
        // A range that doesn't fit the page is ignored
        assert_eq!(View::build(page, None, 80, |_| 1, |_, _, _| None, true, false, &[(2, 15)]).text, View::build(page, None, 80, |_| 1, |_, _, _| None, true, false, &[]).text);
    }

    #[test]
    fn positions_map_both_ways() {
        let v = view(None);
        let fin_page = PAGE.find("fin").unwrap();
        let fin_view = v.text.chars().collect::<String>().find("fin").map(|b| v.text[..b].chars().count()).unwrap();
        assert_eq!(v.view_pos(fin_page), fin_view);
        assert_eq!(v.content_pos(&v.text, fin_view), fin_page);
        assert_eq!(v.view_pos(3), 3);
        assert_eq!(v.content_pos(&v.text, 3), 3);
        // In a table's block: the start of a row of the table
        assert_eq!(v.content_pos(&v.text, 8), 8);
        // Third line of the block (4 lines for 3 rows): the start of the third row
        assert_eq!(v.content_pos(&v.text, 8 + 5), 28);
        assert_eq!(v.view_pos(20), 8);
    }

    #[test]
    fn deleting_across_a_table_keeps_or_drops_it_whole() {
        let v = view(None);
        let block_start = v.text.find(tag(0)).unwrap();
        let cut = format!("{}{}", &v.text[..block_start + tag(0).len_utf8()], &v.text[block_start + tag(0).len_utf8() + 4..]);
        assert!(v.to_content(&cut).contains("| a | b |\n|---+---|\n| c | d |"));
        assert_eq!(v.to_content(&v.text.replace(tag(0), "")), "* Titre\n\nfin\n| x |");
    }
}
