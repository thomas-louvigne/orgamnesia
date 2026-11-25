use super::*;

fn c(s: &str) -> Vec<char> { s.chars().collect() }

#[test]
fn sentences_forward_and_back() {
    let t = c("Un deux. Trois quatre! Cinq");
    assert_eq!(forward_sentence(&t, 0), 8);
    assert_eq!(forward_sentence(&t, 8), 22);
    assert_eq!(backward_sentence(&t, 15), 9);   // inside sentence 2 -> its start
    assert_eq!(backward_sentence(&t, 9), 0);    // at start of 2 -> start of 1
    assert_eq!(backward_sentence(&t, 3), 0);
}

#[test]
fn sentences_stop_at_headlines() {
    let t = c("* Titre\nUn. Deux\ntrois\n** Sous\nQuatre");
    // From the text under `** Sous`: up to it, not to the top
    assert_eq!(backward_sentence(&t, t.len()), 31);
    assert_eq!(backward_sentence(&t, 31), 26);      // the headline's text
    assert_eq!(backward_sentence(&t, 26), 12);      // "Deux\ntrois"
    assert_eq!(backward_sentence(&t, 12), 8);
    assert_eq!(backward_sentence(&t, 8), 2);
    assert_eq!(forward_sentence(&t, 0), 7);
    assert_eq!(forward_sentence(&t, 7), 11);
    assert_eq!(forward_sentence(&t, 11), 22);       // stops before `** Sous`
    assert_eq!(forward_sentence(&t, 22), 30);
}

#[test]
fn sentences_in_a_table_go_field_by_field() {
    let t = c("Avant. Fin\n| ab | c |\n|----+---|\n|    | d\napres.");
    // Fields: ab (13..15), c (18..19), empty (35..35), d (40..41)
    assert_eq!(forward_sentence_in_table(&t, 13), Some(15));
    assert_eq!(forward_sentence_in_table(&t, 15), Some(19));
    assert_eq!(forward_sentence_in_table(&t, 19), Some(35));
    assert_eq!(forward_sentence_in_table(&t, 35), Some(41));
    // Past the last field: the sentence after the table
    assert_eq!(forward_sentence_in_table(&t, 41), Some(48));
    assert_eq!(backward_sentence_in_table(&t, 41), Some(40));
    assert_eq!(backward_sentence_in_table(&t, 40), Some(35));
    assert_eq!(backward_sentence_in_table(&t, 35), Some(18));
    assert_eq!(backward_sentence_in_table(&t, 14), Some(13));
    // Before the first field: the sentence before the table
    assert_eq!(backward_sentence_in_table(&t, 13), Some(7));
    assert_eq!(forward_sentence_in_table(&t, 3), None);
}

#[test]
fn org_elements() {
    let t = c("* A\ntexte un\nsuite\n\n- un\n  - sous\n- deux\n\n| t |\nfin\n** B\n* C\n");
    let at = |s: &str| t.iter().collect::<String>().find(s).unwrap();
    let (texte, un, sous, deux, table, fin, b, cc) =
        (at("texte"), at("- un"), at("- sous") - 2, at("- deux"), at("| t"), at("fin"), at("** B"), at("* C"));
    assert_eq!(forward_element(&t, texte), un);         // paragraph -> the list
    assert_eq!(forward_element(&t, un), deux);          // item -> next item
    assert_eq!(forward_element(&t, un + 2), sous);      // text of an item -> its sub-list
    assert_eq!(forward_element(&t, sous), deux);        // last sub-item -> past its list
    assert_eq!(forward_element(&t, deux), table);       // last item -> past the list
    assert_eq!(forward_element(&t, table), fin);
    assert_eq!(forward_element(&t, fin), b);
    assert_eq!(forward_element(&t, 0), cc);             // headline -> past its subtree
    assert_eq!(forward_element(&t, b), cc);
    assert_eq!(forward_element(&t, cc), cc);            // nothing after
    assert_eq!(backward_element(&t, b), 0);             // no sibling: the parent
    assert_eq!(backward_element(&t, cc), 0);            // the sibling
    assert_eq!(backward_element(&t, cc + 2), 0);
    assert_eq!(backward_element(&t, fin), table);
    assert_eq!(backward_element(&t, table), un);        // over the whole list
    assert_eq!(backward_element(&t, deux + 2), deux);   // text of an item -> its bullet
    assert_eq!(backward_element(&t, deux), un);         // item -> previous item
    assert_eq!(backward_element(&t, texte + 7), texte);
    assert_eq!(backward_element(&t, texte), 0);         // first element -> its headline
}

#[test]
fn org_elements_blocks_and_drawers() {
    let t = c("* A\n\n:PROPERTIES:\n:ID: 1\n:END:\n#+begin_src\na\n\nb\n#+end_src\nx\n");
    let at = |s: &str| t.iter().collect::<String>().find(s).unwrap();
    assert_eq!(forward_element(&t, 4), at(":PROP"));    // blank under the headline
    assert_eq!(forward_element(&t, at(":PROP")), at("#+begin"));
    assert_eq!(forward_element(&t, at("#+begin")), at("x\n"));
    assert_eq!(backward_element(&t, at("x\n")), at("#+begin"));
    assert_eq!(backward_element(&t, at("b\n")), at("#+begin"));
}

#[test]
fn fills_paragraphs() {
    let fill = |t: &str, pos| fill_paragraph(&c(t), pos, 10).map(|(a, b, s)| {
        let t = c(t);
        format!("{}{s}{}", t[..a].iter().collect::<String>(), t[b..].iter().collect::<String>())
    });
    let s = |t: &str| Some(t.to_string());
    // Wrapped, and joined: only the paragraph of the caret
    assert_eq!(fill("aaa bbb ccc ddd\n\nzzz zzz zzz zzz", 1), s("aaa bbb\nccc ddd\n\nzzz zzz zzz zzz"));
    assert_eq!(fill("a\nb\n\n", 0), s("a b\n\n"));
    // Already so; a word longer than the width stays whole
    assert_eq!(fill("aaa bbb\nccc", 0), None);
    assert_eq!(fill("aaaaaaaaaaaaaa b", 0), s("aaaaaaaaaaaaaa\nb"));
    // Its indentation; a list item's text goes under the text
    assert_eq!(fill("  aaa bbb ccc", 3), s("  aaa bbb\n  ccc"));
    assert_eq!(fill("- aaa bbb ccc\n- x", 0), s("- aaa bbb\n  ccc\n- x"));
    assert_eq!(fill("- [ ] aaa bbb", 8), s("- [ ] aaa\n      bbb"));
    // A link stays whole
    assert_eq!(fill("a [[b][c d e f g]] h", 0), s("a\n[[b][c d e f g]]\nh"));
    // Not prose
    for t in ["* Un titre un peu long", "| a | b c d e f g h i j |", ":PROPERTIES:\n:ID: a b c d e f g\n:END:",
              "#+TITLE: a b c d e f g", "#+begin_src\naaa bbb ccc ddd\n#+end_src", "\n",
              "# aaa bbb ccc ddd", "SCHEDULED: <2026-01-01 a b c>", "- "] {
        let pos = t.find("aaa").unwrap_or(0);
        assert_eq!(fill(t, pos), None, "{t}");
    }
    // Markdown: a fence, a quote, front matter
    let _md = FormatGuard::set(Format::Markdown);
    assert_eq!(fill("aaa bbb ccc", 0), s("aaa bbb\nccc"));
    for t in ["```\naaa bbb ccc\n```", "> aaa bbb ccc", "---\naaa bbb ccc\n---"] {
        assert_eq!(fill(t, t.find("aaa").unwrap()), None, "{t}");
    }
    assert_eq!(fill("```\n\naaa bbb ccc\n\n```", 5), None);
}

#[test]
fn heading_siblings_and_parent() {
    let t = c("intro\n* A\ntexte\n** A1\n** A2\n*** A21\n* B\n** B1\n");
    // intro 0, * A 6, texte 10, ** A1 16, ** A2 22, *** A21 28, * B 36, ** B1 40
    assert_eq!(backward_heading(&t, 12), 6);        // from the text: its headline
    assert_eq!(backward_heading(&t, 24), 16);       // inside the line: the sibling above
    assert_eq!(backward_heading(&t, 22), 16);
    assert_eq!(backward_heading(&t, 16), 16);       // first of its parent: stays
    assert_eq!(backward_heading(&t, 36), 6);        // over the sub-headlines of A
    assert_eq!(backward_heading(&t, 6), 6);
    assert_eq!(backward_heading(&t, 3), 0);         // before the first headline
    assert_eq!(forward_heading(&t, 6), 36);         // over the sub-headlines of A
    assert_eq!(forward_heading(&t, 16), 22);
    assert_eq!(forward_heading(&t, 24), 22);        // last of its parent: its start
    assert_eq!(forward_heading(&t, 12), 36);        // from the text: its headline's sibling
    assert_eq!(forward_heading(&t, 0), 6);          // before the first headline
    assert_eq!(forward_heading(&t, 36), 36);
    assert_eq!(up_heading(&t, 30), 22);
    assert_eq!(up_heading(&t, 22), 6);
    assert_eq!(up_heading(&t, 12), 12);             // top level: stays
    assert_eq!(up_heading(&t, 41), 36);
    assert_eq!(up_heading(&t, 3), 3);
}

#[test]
fn sentence_stops_at_blank_line() {
    let t = c("Fin sans point\n\nSuite.");
    assert_eq!(forward_sentence(&t, 0), 14);
}

#[test]
fn paragraphs() {
    let t = c("a\nb\n\nc\nd\n\ne");
    assert_eq!(forward_paragraph(&t, 0), 4);
    assert_eq!(forward_paragraph(&t, 4), 9);
    assert_eq!(forward_paragraph(&t, 10), t.len());
    assert_eq!(backward_paragraph(&t, 6), 4);
    assert_eq!(backward_paragraph(&t, 5), 4);
    assert_eq!(backward_paragraph(&t, 10), 9);
    assert_eq!(backward_paragraph(&t, 2), 0);
}

#[test]
fn bare_stars_line_is_empty() {
    let t = c("a\n** \nb\n*\nc");
    assert_eq!(forward_paragraph(&t, 0), 5);
    assert_eq!(forward_paragraph(&t, 5), 9);
    assert_eq!(backward_paragraph(&t, 10), 9);
    assert_eq!(backward_paragraph(&t, 6), 5);
    let t = c("* Titre\ntexte\n\nsuite");
    assert_eq!(forward_paragraph(&t, 0), 14);
}

#[test]
fn indentation() {
    let t = c("x\n   texte");
    assert_eq!(back_to_indentation(&t, 8), 5);
}

#[test]
fn headings() {
    let t = c("* A\ntexte\n** B\nplus\n* C");
    assert_eq!(next_heading(&t, 0), 10);
    assert_eq!(next_heading(&t, 5), 10);
    assert_eq!(next_heading(&t, 16), 20);
    assert_eq!(previous_heading(&t, 16), 10);
    assert_eq!(previous_heading(&t, 5), 0);   // body text -> its own heading
    assert_eq!(previous_heading(&t, 12), 0);  // on "** B" -> the heading before it
    assert_eq!(previous_heading(&t, 2), 2);   // first heading: nowhere to go
}
