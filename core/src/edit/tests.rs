use super::*;

fn c(s: &str) -> Vec<char> { s.chars().collect() }

#[test]
fn links_to_insert_or_edit() {
    let t = c("voir [[Page]] et [[https://x.org][le site]] fin");
    assert_eq!(link_around(&t, 7), Some(Link { start: 5, end: 13, target: "Page".into(), description: String::new() }));
    let l = link_around(&t, 40).unwrap();
    assert_eq!((l.start, l.end, l.target.as_str(), l.description.as_str()), (17, 43, "https://x.org", "le site"));
    assert_eq!(link_around(&t, 13), None);
    assert_eq!(link_around(&c("[[a] b"), 1), None);
    assert_eq!(format_link(" Page ", ""), "[[Page]]");
    assert_eq!(format_link("Page", "Page"), "[[Page]]");
    assert_eq!(format_link("Page", "la page"), "[[Page][la page]]");
}

#[test]
fn timestamps_to_insert_or_edit() {
    let s = Stamp::from_fields("2026-10-26", "", true, "").unwrap();
    assert_eq!(s.text(), "<2026-10-26 Mon>");
    let s = Stamp::from_fields("2024-02-29", "9:05", false, "+1w").unwrap();
    assert_eq!(s.text(), "[2024-02-29 Thu 09:05 +1w]");
    assert_eq!((s.date_field(), s.time_field()), ("2024-02-29".into(), "09:05".into()));
    assert_eq!(Stamp::from_fields("2026-13-01", "", true, ""), None);
    assert_eq!(Stamp::from_fields("2026-10-01", "25:00", true, ""), None);
    let t = c("DEADLINE: <2026-01-05 lun. 10:00 -2d> et [2026-01-06 Tue] [x]");
    let (a, b, s) = stamp_around(&t, 15).unwrap();
    assert_eq!((a, b), (10, 37));
    assert_eq!(s, Stamp { year: 2026, month: 1, day: 5, time: Some((10, 0)), active: true, rest: "-2d".into() });
    assert_eq!(s.text(), "<2026-01-05 Mon 10:00 -2d>");
    assert!(stamp_around(&t, 45).unwrap().2.time.is_none());
    assert_eq!(stamp_around(&t, 60), None);
    assert_eq!(stamp_around(&t, 3), None);
}

#[test]
fn lines() {
    let t = c("ab\ncde\n\nf");
    assert_eq!(line_bounds(&t, 4), (3, 6));
    assert_eq!(move_lines(&t, 1, 1), 4);
    assert_eq!(move_lines(&t, 5, 1), 7);
    assert_eq!(move_lines(&t, 5, -1), 2);
    assert_eq!(move_lines(&t, 1, -1), 1);
    assert_eq!(move_lines(&t, 0, 50), t.len());
    assert_eq!(nth_line_start(&t, 2), 7);
    assert_eq!(nth_line_start(&t, 9), t.len());
}

#[test]
fn moves_lines() {
    let t = c("un\ndeux\ntrois");
    // The caret in "deux", moved up: before "un"
    let m = move_line(&t, 4, 4, true).unwrap();
    assert_eq!((m.a, m.b, m.text.as_str(), m.sel), (0, 7, "deux\nun", (1, 1)));
    // Down: after "trois", the last line (no line break after it)
    let m = move_line(&t, 4, 4, false).unwrap();
    assert_eq!((m.a, m.b, m.text.as_str(), m.sel), (3, 13, "trois\ndeux", (10, 10)));
    assert!(move_line(&t, 1, 1, true).is_none());
    assert!(move_line(&t, 9, 9, false).is_none());
    // Two lines selected (ending at the start of the third): both move, not the third
    let m = move_line(&t, 0, 8, false).unwrap();
    assert_eq!((m.a, m.b, m.text.as_str(), m.sel), (0, 13, "trois\nun\ndeux", (6, 14)));
}

#[test]
fn words() {
    let t = c("un, deux trois");
    assert_eq!(word_end_forward(&t, 2), 8);
    assert_eq!(word_start_backward(&t, 8), 4);
    assert_eq!(word_start_backward(&t, 0), 0);
}

#[test]
fn kill_line_takes_the_line_break_at_end_of_line() {
    let t = c("abc\ndef");
    assert_eq!(kill_line_range(&t, 1), (1, 3));
    assert_eq!(kill_line_range(&t, 3), (3, 4));
    assert_eq!(kill_line_range(&t, 7), (7, 7));
}

#[test]
fn json_tab_indents() {
    // The next step of two spaces
    assert_eq!(json_indent(&c("{\n\"a\""), 2, 2, false), (2, 2, "  ".into(), (4, 4)));
    assert_eq!(json_indent(&c("{\n \"a\""), 3, 3, false), (3, 3, " ".into(), (4, 4)));
    // Shift+Tab: one step off the line, the caret with its text
    assert_eq!(json_indent(&c("{\n    \"a\""), 7, 7, true), (2, 9, "  \"a\"".into(), (5, 5)));
    assert_eq!(json_indent(&c("x\n \"a\""), 3, 3, true), (2, 6, "\"a\"".into(), (2, 2)));
    // A selection: its lines, not the one it only reaches the start of
    let t = c("{\n\"a\",\n\"b\"\n}");
    assert_eq!(json_indent(&t, 2, 11, false), (2, 10, "  \"a\",\n  \"b\"".into(), (4, 15)));
    let t = c("  a\n  b\nc");
    assert_eq!(json_indent(&t, 0, 8, true), (0, 7, "a\nb".into(), (0, 4)));
}

#[test]
fn delete_forward() {
    let t = c("abc");
    assert_eq!(delete_forward_range(&t, 0, 2), (0, 2));
    assert_eq!(delete_forward_range(&t, 1, 1), (1, 2));
    assert_eq!(delete_forward_range(&t, 3, 3), (3, 3));
    // At the end, after a last newline: that newline
    assert_eq!(delete_forward_range(&c("ab\n"), 3, 3), (2, 3));
    assert_eq!(delete_forward_range(&c("ab\n"), 2, 2), (2, 3));
}

#[test]
fn text_edits() {
    use EditorAction::*;
    // The text once edited, the caret marked by `|`
    let edit = |action, t: &str| {
        let pos = t.chars().position(|c| c == '|').unwrap();
        let t = c(&t.replace('|', ""));
        text_edit(action, &t, pos).map(|(a, b, with)| {
            let caret = caret_after_edit(action, a, &with);
            let done: Vec<char> = t[..a].iter().copied().chain(with.chars()).chain(t[b..].iter().copied()).collect();
            let (x, y) = done.split_at(caret);
            format!("{}|{}", x.iter().collect::<String>(), y.iter().collect::<String>())
        })
    };
    let s = |t: &str| Some(t.to_string());
    // Words: around the caret, what is between them stays
    assert_eq!(edit(TransposeWords, "foo |bar baz"), s("bar foo| baz"));
    assert_eq!(edit(TransposeWords, "f|oo, bar"), s("bar, foo|"));
    assert_eq!(edit(TransposeWords, "|l'été"), s("été'l|"));
    assert_eq!(edit(TransposeWords, "foo bar|"), None);
    assert_eq!(edit(TransposeWords, "foo bar| ."), None);
    // Case: from the caret to the end of the word
    assert_eq!(edit(UpcaseWord, "un mo|t ici"), s("un moT| ici"));
    assert_eq!(edit(UpcaseWord, "un| été ici"), s("un ÉTÉ| ici"));
    assert_eq!(edit(DowncaseWord, "|MOT ICI"), s("mot| ICI"));
    assert_eq!(edit(CapitalizeWord, "|mOT ici"), s("Mot| ici"));
    assert_eq!(edit(CapitalizeWord, "...|  mOT"), s("...  Mot|"));
    assert_eq!(edit(UpcaseWord, "fin|  "), s("fin  |"));
    // Spaces
    assert_eq!(edit(DeleteHorizontalSpace, "a \t| \tb"), s("a|b"));
    assert_eq!(edit(DeleteHorizontalSpace, "a|b"), None);
    assert_eq!(edit(JustOneSpace, "a  |  b"), s("a |b"));
    assert_eq!(edit(JustOneSpace, "a|b"), s("a |b"));
    // A line break is not a space
    assert_eq!(edit(DeleteHorizontalSpace, "a\n | b"), s("a\n|b"));
    // Joining: one space, none with an empty line or inside brackets
    assert_eq!(edit(DeleteIndentation, "un  \n   de|ux"), s("un| deux"));
    assert_eq!(edit(DeleteIndentation, "\nde|ux"), s("|deux"));
    assert_eq!(edit(DeleteIndentation, "un\n|"), s("un|"));
    assert_eq!(edit(DeleteIndentation, "f(\n|x\n)"), s("f(|x\n)"));
    assert_eq!(edit(DeleteIndentation, "x\n|)"), s("x|)"));
    assert_eq!(edit(DeleteIndentation, "se|ule"), None);
    // Not one of these actions
    assert_eq!(edit(Yank, "a|b"), None);
    // Zap: the character goes too
    assert_eq!(zap_end(&c("a,b,c"), 0, ','), Some(2));
    assert_eq!(zap_end(&c("a,b,c"), 2, ','), Some(4));
    assert_eq!(zap_end(&c("ab"), 0, ','), None);
    assert_eq!(zap_end(&c("ab"), 9, 'a'), None);
    // Region case keeps what has no case
    assert_eq!(change_case(&c("ab\u{f800} é"), Case::Upper), "AB\u{f800} É");
    assert_eq!(change_case(&c("ÀB"), Case::Lower), "àb");
}

#[test]
fn transpose() {
    let swap = |t: &str, pos| transpose_chars(&c(t), pos);
    let s = |t: &str| t.to_string();
    // In a line: around the caret
    assert_eq!(swap("abc", 1), Some((0, 2, s("ba"))));
    // At the end of the text or of a line: the two before it
    assert_eq!(swap("abc", 3), Some((1, 3, s("cb"))));
    assert_eq!(swap("abc\nd", 3), Some((1, 3, s("cb"))));
    // At the start of a line: the line break goes across, as in Emacs
    assert_eq!(swap("ab\ncd", 3), Some((2, 4, s("c\n"))));
    assert_eq!(swap("a😀b", 2), Some((1, 3, s("b😀"))));
    // Not two characters
    assert_eq!(swap("abc", 0), None);
    assert_eq!(swap("a", 1), None);
    assert_eq!(swap("", 0), None);
}

#[test]
fn new_heading_uses_the_level_in_effect() {
    let t = c("* a\n** b\ntexte");
    assert_eq!(new_heading(&t, 14), (14, 14, "\n** ".to_string()));
    assert_eq!(new_heading(&c("rien"), 4), (4, 4, "\n* ".to_string()));
    let _md = motion::FormatGuard::set(crate::Format::Markdown);
    assert_eq!(new_heading(&c("# A\n## B\ntexte"), 14), (14, 14, "\n## ".to_string()));
    drop(_md);
    // On a list item: a new item
    assert_eq!(new_heading(&c("- a\n  suite"), 11), (11, 11, "\n- ".to_string()));
    assert_eq!(new_heading(&c("  9) [X] a"), 10), (10, 10, "\n  10) [ ] ".to_string()));
}

#[test]
fn new_heading_cuts_a_list_item_at_the_caret() {
    // In the text of an item, or of a line continuing it
    assert_eq!(new_heading(&c("- ab"), 3), (3, 3, "\n- ".to_string()));
    assert_eq!(new_heading(&c("  9) [X] un deux"), 12), (11, 12, "\n  10) [ ] ".to_string()));
    assert_eq!(new_heading(&c("- a\n  suite"), 7), (7, 7, "\n- ".to_string()));
    // At the start of a line continuing it: that line becomes the item
    assert_eq!(new_heading(&c("- a\n  suite"), 6), (4, 6, "- ".to_string()));
    // On the bullet, or just after it: the new item goes below
    assert_eq!(new_heading(&c("- ab"), 0), (4, 4, "\n- ".to_string()));
    assert_eq!(new_heading(&c("- ab"), 2), (4, 4, "\n- ".to_string()));
    assert_eq!(new_heading(&c("  9) [X] a"), 4), (10, 10, "\n  10) [ ] ".to_string()));
}

#[test]
fn new_heading_cuts_the_line_at_the_caret() {
    // In a text, in a heading: what follows the caret is the new heading's text
    assert_eq!(new_heading(&c("* a\nun deux"), 5), (5, 5, "\n* ".to_string()));
    assert_eq!(new_heading(&c("** titre suite"), 6), (6, 6, "\n** ".to_string()));
    // The blanks around the cut go
    assert_eq!(new_heading(&c("un deux"), 2), (2, 3, "\n* ".to_string()));
    assert_eq!(new_heading(&c("un deux"), 3), (2, 3, "\n* ".to_string()));
    // At the start of a line of text: that line becomes the heading
    assert_eq!(new_heading(&c("* a\nb"), 4), (4, 4, "* ".to_string()));
    // On a heading's stars, on an empty line: nothing to cut
    assert_eq!(new_heading(&c("** a"), 0), (4, 4, "\n** ".to_string()));
    assert_eq!(new_heading(&c("** a"), 3), (4, 4, "\n** ".to_string()));
    assert_eq!(new_heading(&c("* a\n"), 4), (4, 4, "\n* ".to_string()));
}

#[test]
fn pairs() {
    assert_eq!(pair_of(")"), Some(("(", ")")));
    assert_eq!(pair_of("a"), None);
    assert!(auto_closes("[") && !auto_closes("'") && !auto_closes(")"));
    assert_eq!(link_of("[", "Dragon"), Some("[[Dragon]]".into()));
    assert_eq!(link_of("[", "a\nb"), None);
    assert_eq!(link_of("(", "Dragon"), None);
    use crate::Format::{Markdown, Org};
    assert_eq!(emphasis_pair("/", Org), Some(("/", "/")));
    assert_eq!(emphasis_pair("/", Markdown), None);
    assert_eq!(emphasis_pair("_", Markdown), Some(("_", "_")));
}

#[test]
fn motions() {
    let t = c("ab\ncd");
    assert_eq!(motion_target(EditorAction::EndOfLine, &t, 3), Some(5));
    assert_eq!(motion_target(EditorAction::BeginningOfLine, &t, 4), Some(3));
    assert_eq!(motion_target(EditorAction::BackwardChar, &t, 0), Some(0));
    assert_eq!(motion_target(EditorAction::ForwardChar, &t, 5), Some(5));
    assert_eq!(motion_target(EditorAction::Copy, &t, 0), None);
}

#[test]
fn emphasis_toggles() {
    let t = c("un mot ici");
    assert_eq!(toggle_emphasis(&t, 3, 6, "*", "*"), (3, 6, "*mot*".into(), 4, 7));
    let t = c("un *mot* ici");
    assert_eq!(toggle_emphasis(&t, 4, 7, "*", "*"), (3, 8, "mot".into(), 3, 6));
    assert_eq!(toggle_emphasis(&t, 3, 8, "*", "*"), (3, 8, "mot".into(), 3, 6));
    assert_eq!(toggle_emphasis(&t, 4, 7, "_", "_"), (4, 7, "_mot_".into(), 5, 8));
    // No selection: a pair of markers, the caret inside; again, they go away
    assert_eq!(toggle_emphasis(&c("ab"), 1, 1, "+", "+"), (1, 1, "++".into(), 2, 2));
    assert_eq!(toggle_emphasis(&c("a++b"), 2, 2, "+", "+"), (1, 3, String::new(), 1, 1));
    // Markers of several chars, not the same at both ends
    assert_eq!(toggle_emphasis(&c("un mot"), 3, 6, "<u>", "</u>"), (3, 6, "<u>mot</u>".into(), 6, 9));
    assert_eq!(toggle_emphasis(&c("un <u>mot</u>"), 6, 9, "<u>", "</u>"), (3, 13, "mot".into(), 3, 6));
    assert_eq!(toggle_emphasis(&c("un **mot**"), 3, 10, "**", "**"), (3, 10, "mot".into(), 3, 6));
    assert_eq!(emphasis_marks(EditorAction::Bold, crate::Format::Markdown), Some(("**", "**")));
    assert_eq!(emphasis_marks(EditorAction::Strikethrough, crate::Format::Org), Some(("+", "+")));
}

#[test]
fn quote_toggles() {
    use crate::Format::{Markdown, Org};
    let t = c("avant\nun\ndeux\napres");
    let (a, b, with, s, e) = toggle_quote(&t, 7, 10, Org);
    assert_eq!((a, b, with.as_str()), (6, 13, "#+begin_quote\nun\ndeux\n#+end_quote"));
    assert_eq!(with.chars().skip(s - a).take(e - s).collect::<String>(), "un\ndeux");
    // Again, inside the block: back as it was
    let q = c("avant\n#+begin_quote\nun\ndeux\n#+end_quote\napres");
    assert_eq!(toggle_quote(&q, 20, 25, Org), (6, 39, "un\ndeux".into(), 6, 13));
    assert_eq!(toggle_quote(&t, 7, 14, Markdown), (6, 13, "> un\n> deux".into(), 6, 17));
    assert_eq!(toggle_quote(&c("> un\n>deux"), 0, 0, Markdown), (0, 4, "un".into(), 0, 2));
    assert_eq!(toggle_quote(&c("> un\n>deux"), 0, 10, Markdown), (0, 10, "un\ndeux".into(), 0, 7));
}

#[test]
fn comment_toggles() {
    use crate::Format::{Markdown, Org};
    let t = c("avant\nun\ndeux\napres");
    let (a, b, with, s, e) = toggle_comment(&t, 7, 10, Org);
    assert_eq!((a, b, with.as_str()), (6, 13, "#+begin_comment\nun\ndeux\n#+end_comment"));
    assert_eq!(with.chars().skip(s - a).take(e - s).collect::<String>(), "un\ndeux");
    let q = c("avant\n#+begin_comment\nun\ndeux\n#+end_comment\napres");
    assert_eq!(toggle_comment(&q, 22, 27, Org), (6, 43, "un\ndeux".into(), 6, 13));
    let (_, _, with, s, e) = toggle_comment(&t, 7, 14, Markdown);
    assert_eq!(with, "<!--\nun\ndeux\n-->");
    assert_eq!(with.chars().skip(s - 6).take(e - s).collect::<String>(), "un\ndeux");
    assert_eq!(toggle_comment(&c("<!--\nun\n-->"), 5, 7, Markdown), (0, 11, "un".into(), 0, 2));
}

#[test]
fn extended_links_under_the_cursor() {
    let t = c("[[Page|texte]] [[P#Titre]] [lien](Ma%20page.md)");
    let ext = TagSyntax { org_links: crate::links::LinkSyntax::ALL, ..Hashtags::Off.into() };
    assert_eq!(linked_page_at(&t, 9, ext, crate::Format::Org).as_deref(), Some("Page"));
    assert_eq!(linked_page_at(&t, 17, ext, crate::Format::Org).as_deref(), Some("P::*Titre"));
    assert_eq!(linked_page_at(&t, 30, ext, crate::Format::Org).as_deref(), Some("Ma page.md"));
    assert_eq!(linked_page_at(&t, 9, Hashtags::Off.into(), crate::Format::Org).as_deref(), Some("Page|texte"));
    assert_eq!(linked_page_at(&t, 30, Hashtags::Off.into(), crate::Format::Org), None);
}

#[test]
fn links_under_the_cursor() {
    let t = c("voir [[Ma page]] et [[cible][texte]] #tag fin");
    assert_eq!(linked_page_at(&t, 8, Hashtags::Dashes.into(), crate::Format::Org).as_deref(), Some("Ma page"));
    assert_eq!(linked_page_at(&t, 30, Hashtags::Dashes.into(), crate::Format::Org).as_deref(), Some("cible"));
    assert_eq!(linked_page_at(&t, 39, Hashtags::Dashes.into(), crate::Format::Org).as_deref(), Some("tag"));
    assert_eq!(linked_page_at(&t, 39, Hashtags::Off.into(), crate::Format::Org), None);
    assert_eq!(linked_page_at(&t, 1, Hashtags::Dashes.into(), crate::Format::Org), None);
    assert_eq!(linked_page_at(&c("* Titre :projet:"), 11, Hashtags::Off.into(), crate::Format::Org).as_deref(), Some("projet"));
    assert_eq!(linked_page_at(&c("voir :exemple: ici"), 7, Hashtags::Off.into(), crate::Format::Org).as_deref(), Some("exemple"));
    let off = TagSyntax { hashtags: Hashtags::Off, org: crate::OrgTags { links: false, dashes: false }, ..Hashtags::Off.into() };
    assert_eq!(linked_page_at(&c("voir :exemple: ici"), 7, off, crate::Format::Org), None);
}
