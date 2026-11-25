use super::*;

/// The pieces as the window's HTML writes them, to read them at a glance.
fn html(nodes: &[Node]) -> String {
    nodes.iter().map(|n| match n {
        Node::Text(t) => t.clone(),
        Node::Span { class, data, children } => format!("<{class}{}>{}</>", data.as_ref().map(|d| format!(" →{d}")).unwrap_or_default(), html(children)),
    }).collect()
}

#[test]
fn a_headline_in_pieces() {
    let d = Draw { todo: Some(TodoKeywords::parse("TODO | DONE")), ..Draw::new(Format::Org, crate::Hashtags::Off.into()) };
    assert_eq!(html(&line("** TODO Écrire *vite* :a:", &d)),
        "<h2><h-stars><h-lead>*</>*</> <todo>TODO</> Écrire <em-b><mk>*</>vite<mk>*</></> <tags>:<tag-t>a</>:</></>");
}

#[test]
fn links_in_pieces() {
    let broken = |t: &str| t == "Rien";
    let d = Draw { broken: Some(Box::new(broken)), ..Draw::new(Format::Org, crate::Hashtags::Off.into()) };
    assert_eq!(html(&line("voir [[Page][texte]] [[Rien]]", &d)),
        "voir <link><mk>[[<link-t>Page</>][</><link-d>texte</><mk>]]</></> <link broken><mk>[[</><link-t>Rien</><mk>]]</></>");
    let md = Draw::new(Format::Markdown, crate::Hashtags::Off.into());
    assert_eq!(html(&line("[t](Ma%20page.md) `x`", &md)),
        "<link →Ma page.md><mk>[</><link-d>t</><mk>](<link-t>Ma%20page.md</>)</></> <md-code>`x`</>");
}

#[test]
fn footnotes_in_pieces() {
    let d = Draw::new(Format::Org, crate::Hashtags::Off.into());
    assert_eq!(html(&line("Voir[fn:1].", &d)), "Voir<fn>[fn:1]</>.");
    assert_eq!(html(&line("[fn:1] La *note*", &d)), "<fn fn-def>[fn:1]</> La <em-b><mk>*</>note<mk>*</></>");
    let md = Draw::new(Format::Markdown, crate::Hashtags::Off.into());
    assert_eq!(html(&line("[^1]: Note[^2]", &md)), "<fn fn-def>[^1]:</> Note<fn>[^2]</>");
}

#[test]
fn timestamps_in_pieces() {
    let d = Draw::new(Format::Org, crate::Hashtags::Off.into());
    assert_eq!(html(&line("- <2026-11-24 Tue 14:05-15:00 +1w> [[P]] [2026-11-24 mar.]", &d)),
        "- <ts><mk><</><ts-date>2026-11-24</> <ts-day>Tue</> <ts-time>14:05-15:00</> <ts-rest>+1w</><mk>></></> \
         <link><mk>[[</><link-t>P</><mk>]]</></> <ts ts-off><mk>[</><ts-date>2026-11-24</> <ts-day>mar.</><mk>]</></>");
    assert_eq!(html(&line("  SCHEDULED: <2026-11-24> DEADLINE:x", &d)),
        "  <kw>SCHEDULED:</> <ts><mk><</><ts-date>2026-11-24</><mk>></></> <kw>DEADLINE:</>x");
    assert_eq!(html(&line("NOTSCHEDULED:", &d)), "NOTSCHEDULED:");
    let md = Draw::new(Format::Markdown, crate::Hashtags::Off.into());
    assert!(html(&line("# a <2026-11-24>", &md)).contains("<ts><mk><</><ts-date>2026-11-24</><mk>></></>"));
}

#[test]
fn markdown_lines() {
    let md = Draw::new(Format::Markdown, crate::Hashtags::Off.into());
    assert_eq!(html(&line("---", &md)), "<kw>---</>");
    assert_eq!(html(&line("- [x] fait", &md)), "- <done>[x]</> fait");
    let lines = ["| a |", "|---|", "| b |"];
    assert_eq!(table_heads(&lines).into_iter().collect::<Vec<_>>(), [0]);
    let text = "---\ntitle: x\n---\n```\ncode\n```\nfin";
    let code = code_lines(&text.split('\n').collect::<Vec<_>>(), text);
    assert_eq!((code.get(&1), code.get(&4), code.get(&6)), (Some(&"kw"), Some(&"md-code"), None));
}
