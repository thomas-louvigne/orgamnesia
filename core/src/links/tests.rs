use super::*;

fn kw() -> TodoKeywords { TodoKeywords::parse("TODO | DONE") }

fn scanned(text: &str, extended: bool) -> Vec<(String, Option<String>)> {
    let chars: Vec<char> = text.chars().collect();
    scan(&chars, if extended { LinkSyntax::ALL } else { LinkSyntax::NONE }).into_iter()
        .map(|f| (f.target, f.page.map(|(a, b)| chars[a..b].iter().collect())))
        .collect()
}

fn pair(t: &str, p: Option<&str>) -> (String, Option<String>) { (t.to_string(), p.map(String::from)) }

#[test]
fn scans_org_links() {
    assert_eq!(scanned("a [[ Page ][x]] [[P::*T]] [[*T]] [[b|c]] [[ouvert", false), vec![
        pair("Page", Some("Page")), pair("P::*T", Some("P")), pair("*T", None), pair("b|c", Some("b|c")),
    ]);
    // Not a Markdown link without the extended syntax
    assert!(scanned("[t](Page.md)", false).is_empty());
}

#[test]
fn each_syntax_on_its_own() {
    let text = "[[a|b]] [[c#d]] [e](f.md)";
    let chars: Vec<char> = text.chars().collect();
    let targets = |s| scan(&chars, s).into_iter().map(|f| f.target).collect::<Vec<_>>();
    assert_eq!(targets(LinkSyntax { pipe: true, ..LinkSyntax::NONE }), vec!["a", "c#d"]);
    assert_eq!(targets(LinkSyntax { hash: true, ..LinkSyntax::NONE }), vec!["a|b", "c::*d"]);
    assert_eq!(targets(LinkSyntax { markdown: true, ..LinkSyntax::NONE }), vec!["a|b", "c#d", "f.md"]);
}

#[test]
fn scans_extended_links() {
    assert_eq!(scanned("[[Page|texte]] [[Page#Mon titre]] [[#id]] [[https://a.b/#x]]", true), vec![
        pair("Page", Some("Page")), pair("Page::*Mon titre", Some("Page")), pair("#id", None),
        pair("https://a.b/#x", None),
    ]);
    assert_eq!(scanned("[t](Ma%20page.md) [u](./P.org#mon-titre) [v](<Deux mots.md>) ![i](a.png) [w](https://x.y) [z](assets/a.pdf)", true), vec![
        pair("Ma page.md", Some("Ma%20page.md")), pair("P.org::*mon titre", Some("P.org")),
        pair("Deux mots.md", Some("Deux mots.md")), pair("https://x.y", None), pair("./assets/a.pdf", None),
    ]);
    assert_eq!(percent_encode("a b(1)"), "a%20b%281%29");
    assert_eq!(write(crate::Format::Org, "../assets/a b.png", "", true), "[[../assets/a b.png]]");
    assert_eq!(write(crate::Format::Org, "f.pdf", "f", false), "[[f.pdf][f]]");
    assert_eq!(write(crate::Format::Markdown, "../assets/a b.png", "", true), "![](../assets/a%20b.png)");
    assert_eq!(write(crate::Format::Markdown, "f.pdf", "f", false), "[f](f.pdf)");
    assert_eq!(percent_decode("%C3%A9t%C3%A9%2"), "été%2");
}

#[test]
fn links_as_written() {
    let c: Vec<char> = "[[a]] [[b][c d]]".chars().collect();
    assert_eq!(link_at(&c, 0), Some(Link { start: 0, end: 5, target: (2, 3), description: None }));
    assert_eq!(link_at(&c, 6), Some(Link { start: 6, end: 16, target: (8, 9), description: Some((11, 14)) }));
    assert_eq!(link_at(&c, 1), None);
    let text = "voir [[A]] et [[../b.png][image]], pas [[c\nd]] ni [[e] ni [[f ni [[g[h]]";
    assert_eq!(targets(text), vec!["A", "../b.png"]);
    assert_eq!(targets("[[x]]{:width 3} [[[y]]] [[z][a [b] c]]"), vec!["x", "y", "z"]);
    assert_eq!(targets("[[ Page ::*Titre]]"), vec![" Page ::*Titre"]);
}

#[test]
fn targets_are_pages_or_files() {
    assert_eq!(file_target("../assets/a.png"), Some("../assets/a.png"));
    assert_eq!(file_target("file:notes.pdf"), Some("notes.pdf"));
    assert_eq!(file_target("~/doc.pdf"), Some("~/doc.pdf"));
    assert_eq!(file_target("Page"), None);
    assert!(is_page_target("Ma page"));
    assert!(is_page_target("Recettes::*Desserts"));
    for other in ["../assets/a.png", "file:../assets/a.svg", "file:a.org::*Titre", "/home/a.pdf",
        "https://orgmode.org", "mailto:a@b.c", "*Titre", "#id", "::cible"] {
        assert!(!is_page_target(other), "{other}");
    }
}

#[test]
fn splits_page_and_place() {
    assert_eq!(split("Recettes::*Desserts"), ("Recettes", Some("*Desserts")));
    assert_eq!(split(" Recettes :: cible "), ("Recettes", Some("cible")));
    assert_eq!(split("Recettes::"), ("Recettes", None));
    assert_eq!(split("Recettes"), ("Recettes", None));
    assert_eq!(split("*Desserts"), ("", Some("*Desserts")));
    assert_eq!(split("#id"), ("", Some("#id")));
}

#[test]
fn titles_as_links_name_them() {
    let k = kw();
    assert_eq!(headline_title("** TODO [#A] Faire [1/3] le pain :cuisine:", Some(&k), Format::Org).as_deref(), Some("Faire le pain"));
    assert_eq!(headline_title("* COMMENT Brouillon  [50%]", None, Format::Org).as_deref(), Some("Brouillon"));
    assert_eq!(headline_title("* TODO Titre", None, Format::Org).as_deref(), Some("TODO Titre"));
    assert_eq!(headline_title("*gras*", None, Format::Org), None);
    assert_eq!(headline_title("texte", None, Format::Org), None);
    assert_eq!(headline_title("*", None, Format::Org).as_deref(), Some(""));
}

#[test]
fn finds_headlines_and_targets() {
    let text = "#+TITLE: R\n* Entrées\n** DONE Desserts :sucre:\nla <<pâte>> ici\n";
    let k = kw();
    assert_eq!(find(text, "*desserts", Some(&k), Format::Org), Some((21, 45)));
    assert_eq!(find(text, "*Entrees", Some(&k), Format::Org), Some((11, 20)));
    assert_eq!(find(text, "pate", Some(&k), Format::Org), Some((49, 57)));
    // No target of that name: a headline
    assert_eq!(find(text, "Desserts", Some(&k), Format::Org), Some((21, 45)));
    assert_eq!(find(text, "*Plats", Some(&k), Format::Org), None);
    assert_eq!(find(text, "#pate", Some(&k), Format::Org), None);
    assert_eq!(find("<<<radio>>> x", "radio", None, Format::Org), Some((0, 11)));
}

#[test]
fn lists_headlines_once() {
    assert_eq!(headlines("* A\ntexte\n** TODO B :t:\n* A\n*\n", Some(&kw()), Format::Org), vec!["A", "B"]);
}

#[test]
fn markdown_headlines() {
    let text = "# Recettes\ntexte\n## TODO Pâte à crêpes ##\nla pâte\n# Fin";
    let k = kw();
    assert_eq!(headlines(text, Some(&k), Format::Markdown), vec!["Recettes", "Pâte à crêpes", "Fin"]);
    assert_eq!(find(text, "*pate a crepes", Some(&k), Format::Markdown), Some((17, 41)));
    assert_eq!(section_end(text, 17, Format::Markdown), 49);
    assert!(headlines("* pas un titre\n#tag", None, Format::Markdown).is_empty());
}

#[test]
fn sections_end_at_the_next_headline() {
    let text = "* A\na\n** B\nb\n* C\nc";
    assert_eq!(section_end(text, 0, Format::Org), 12);
    assert_eq!(section_end(text, 6, Format::Org), 12);
    assert_eq!(section_end(text, 4, Format::Org), 5);
    assert_eq!(section_end(text, 13, Format::Org), text.chars().count());
}
