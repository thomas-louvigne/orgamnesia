use super::*;

fn ed(text: &str, at: usize) -> Editor {
    let mut e = Editor::new("Note.org".into(), text, true);
    e.cursor = at;
    e
}

#[test]
fn types_moves_and_undoes() {
    let mut e = ed("ab\ncdef\ng", 1);
    e.insert("X");
    e.insert("Y");
    assert_eq!(e.text(), "aXYb\ncdef\ng");
    e.action("next_line", 10, None);
    assert_eq!(e.line_col(), (1, 3));
    e.action("next_line", 10, None);
    assert_eq!(e.line_col(), (2, 1));
    e.action("undo", 10, None);
    assert_eq!(e.text(), "ab\ncdef\ng");
    e.action("redo", 10, None);
    assert_eq!(e.text(), "aXYb\ncdef\ng");
    assert!(e.dirty);
}

#[test]
fn kills_yanks_and_pops() {
    let mut e = ed("un\ndeux\ntrois", 0);
    e.action("kill_line", 10, None);
    e.action("kill_line", 10, None);
    assert_eq!(e.text(), "deux\ntrois");
    // A move in between: the next kill is a new entry
    e.action("beginning_of_line", 10, None);
    e.action("kill_word_forward", 10, None);
    assert_eq!(e.text(), "\ntrois");
    e.action("end_of_buffer", 10, None);
    e.action("yank", 10, None);
    assert_eq!(e.text(), "\ntroisdeux");
    e.action("yank_pop", 10, None);
    assert_eq!(e.text(), "\ntroisun\n");
    assert_eq!(e.clipboard.as_deref(), Some("deux"));
}

#[test]
fn the_region_is_killed_copied_and_cased() {
    let mut e = ed("un deux trois", 3);
    e.action("set_mark", 10, None);
    e.action("forward_word", 10, None);
    assert_eq!(e.region(), Some((3, 7)));
    e.action("upcase_region", 10, None);
    assert_eq!(e.text(), "un DEUX trois");
    e.action("set_mark", 10, None);
    e.action("backward_word", 10, None);
    e.action("kill_region", 10, None);
    assert_eq!(e.text(), "un  trois");
    e.action("bold", 10, None);
    assert_eq!(e.text(), "un ** trois");
}

#[test]
fn edits_as_the_window_does() {
    let mut e = ed("ab cd", 1);
    e.action("transpose_chars", 10, None);
    assert_eq!(e.text(), "ba cd");
    e.action("capitalize_word", 10, None);
    e.action("zap_to_char", 10, None);
    assert!(e.zap);
    let mut e = ed("ligne un\nligne deux", 0);
    e.action("move_line_down", 10, None);
    assert_eq!(e.text(), "ligne deux\nligne un");
    let mut e = ed("x", 1);
    e.action("insert_date", 10, None);
    assert!(e.text().starts_with("x<20"), "{}", e.text());
}

#[test]
fn footnotes_inserted_and_followed() {
    let mut e = Editor::new("/p/a.org".into(), "* T\nTexte.", true);
    e.goto(9);
    assert!(e.action("insert_footnote", 10, None));
    assert_eq!(e.text(), "* T\nTexte[fn:1].\n\n* Footnotes\n[fn:1] ");
    e.insert("La note.");
    e.goto(10);
    assert!(e.footnote_jump(), "{:?}", e.text());
    assert_eq!(e.line(), 4);
    assert!(e.footnote_jump());
    assert_eq!(e.line_col(), (1, 5));
}

#[test]
fn headlines_fold_and_shift() {
    let todo = TodoKeywords::parse("TODO | DONE");
    let mut e = ed("* Titre\ntexte\n** Sous\nplus\n* Autre", 0);
    e.action("todo_next", 10, Some(&todo));
    assert_eq!(e.text(), "* TODO Titre\ntexte\n** Sous\nplus\n* Autre");
    e.action("table_next_cell", 10, None);
    assert_eq!(e.lines_shown().iter().filter(|s| s.0).count(), 3);
    e.action("next_line", 10, None);
    assert_eq!(e.line_col().0, 4);
    e.action("global_cycle", 10, None);
    e.action("global_cycle", 10, None);
    e.action("global_cycle", 10, None);
    e.action("next_heading", 10, None);
    e.cursor = 0;
    e.action("demote_subtree", 10, None);
    assert!(e.text().starts_with("** TODO Titre\ntexte\n*** Sous"));
    e.action("next_heading", 10, None);
    e.action("next_heading", 10, None);
    e.action("new_heading", 10, None);
    assert!(e.text().ends_with("* Autre\n* "), "{}", e.text());
}

#[test]
fn searches_as_one_types() {
    let mut e = ed("Média, media et MEDIA", 0);
    e.find(true);
    for c in "media".chars() { e.search_edit(Some(c)); }
    assert_eq!(e.matches().len(), 3);
    assert_eq!(e.cursor, 5);
    e.find(true);
    assert_eq!(e.cursor, 12);
    e.find(true);
    e.find(true);
    assert!(e.search.as_ref().unwrap().wrapped);
    assert_eq!(e.cursor, 5);
    e.find(false);
    assert_eq!(e.cursor, 16);
    e.end_search(false);
    assert_eq!(e.cursor, 0);
    // Searching again with nothing typed: the text last searched
    e.find(true);
    e.find(true);
    assert_eq!(e.cursor, 5);
    e.end_search(true);
    assert_eq!(e.cursor, 5);
}

#[test]
fn a_click_puts_the_cursor() {
    let mut e = ed("ab
漢字x
fin", 0);
    e.click(1, 2);
    assert_eq!(e.line_col(), (1, 1));
    e.click(1, 9);
    assert_eq!(e.line_col(), (1, 3));
    e.click(7, 0);
    assert_eq!(e.line_col(), (2, 0));
}

#[test]
fn follows_the_settings_of_the_editor() {
    let mut e = ed("ab", 2);
    e.type_char('(');
    assert_eq!((e.text().as_str(), e.cursor), ("ab()", 3));
    e.prefs.electric = true;
    e.mark = Some(0);
    e.cursor = 2;
    e.type_char('{');
    assert_eq!(e.text(), "{ab}()");
    // `[` on the region: a link, the region done with
    e.mark = Some(1);
    e.cursor = 3;
    e.type_char('[');
    assert_eq!((e.text().as_str(), e.cursor, e.mark), ("{[[ab]]}()", 7, None));
    e.prefs.emacs_mark = false;
    assert!(!e.action("set_mark", 10, None));
    e.prefs.match_case = true;
    assert_eq!(e.find_all("AB").len(), 0);
    e.prefs.match_case = false;
    assert_eq!(e.find_all("AB").len(), 1);
    e.prefs.numbers = true;
    assert_eq!(e.gutter(), 2);
    let mut e = ed("* Titre\ntexte", 0);
    e.prefs.tab_folds = false;
    assert!(!e.action("table_next_cell", 10, None));
    e.prefs.tab_folds = true;
    assert!(e.action("table_next_cell", 10, None));
    assert!(!e.folds.is_empty());
}

#[test]
fn the_wheel_moves_the_view() {
    let text: String = (0..50).map(|n| format!("ligne {n}\n")).collect();
    let mut e = ed(&text, 0);
    e.scroll(3, 10);
    assert_eq!((e.top, e.line_col().0), (3, 3));
    e.cursor = edit::nth_line_start(&e.text, 8);
    e.scroll(3, 10);
    // Still in view: the cursor stays
    assert_eq!((e.top, e.line_col().0), (6, 8));
    e.scroll(-6, 10);
    assert_eq!((e.top, e.line_col().0), (0, 8));
    e.scroll(-3, 10);
    assert_eq!(e.top, 0);
}
