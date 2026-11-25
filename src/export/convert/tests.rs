use super::*;

fn html(text: &str) -> String {
    let k = TodoKeywords::parse("TODO | DONE");
    page_html(text, "Page", Some(&k), &|t| format!("url:{t}"), Format::Org, place::LinkSyntax::NONE)
}

fn md(text: &str) -> String {
    let k = TodoKeywords::parse("TODO | DONE");
    page_html(text, "Page", Some(&k), &|t| format!("url:{t}"), Format::Markdown, place::LinkSyntax::ALL)
}

#[test]
fn footnotes_at_the_end() {
    assert_eq!(html("* Page\nVoir[fn:a] et[fn:: en ligne].\n* Footnotes\n[fn:a] La\nnote[fn:b].\n\n[fn:b] Autre."),
        "<h1>Page</h1><p>Voir<sup class='fn'><a href='#fn-1' id='fnref-1'>1</a></sup> et<sup class='fn'><a href='#fn-2' id='fnref-2'>2</a></sup>.</p>\
         <section class='footnotes'><hr><ol><li id='fn-1'>La note<sup class='fn'><a href='#fn-3' id='fnref-3'>3</a></sup>. <a href='#fnref-1'>↩</a></li>\
         <li id='fn-2'>en ligne <a href='#fnref-2'>↩</a></li><li id='fn-3'>Autre. <a href='#fnref-3'>↩</a></li></ol></section>");
    assert!(md("Voir[^1].\n\n[^1]: La note.").ends_with("<li id='fn-1'>La note. <a href='#fnref-1'>↩</a></li></ol></section>"));
}

#[test]
fn markdown_pages() {
    assert_eq!(md("---\ntitle: Mon titre\ntags: [a]\n---\n## TODO Faire **vite** ##\ntexte *it* `co*de*`"),
        "<h1 class='title'>Mon titre</h1><h3><span class='todo'>TODO</span> Faire <b>vite</b></h3><p>texte <i>it</i> <code>co*de*</code></p>");
    assert_eq!(md("# Page\n* un\n* [x] deux\n\n> cité\n\n```\n# pas un titre\n```\n---"),
        "<h1>Page</h1><ul><li>un</li><li>☑ deux</li></ul><blockquote><p>cité</p></blockquote><pre># pas un titre</pre><hr>");
    assert_eq!(md("# Page\n| a | b |\n|---|:-:|\n| 1 | 2 |"),
        "<h1>Page</h1><table><tr><th>a</th><th>b</th></tr><tr><td>1</td><td>2</td></tr></table>");
    assert_eq!(md("# Page\n[[B|bé]] [lien](Ma%20page.md) [site](https://a.org) snake_case\n![](../assets/a%20b.png)"),
        "<h1>Page</h1><p><span class='link'>bé</span> <span class='link'>lien</span> <a href='https://a.org'>site</a> snake_case</p>\
        <p class='image'><img src='url:../assets/a b.png' alt=''></p>");
}

#[test]
fn title_from_the_page() {
    assert!(html("#+TITLE: Mon titre\ntexte").starts_with("<h1 class='title'>Mon titre</h1><p>texte</p>"));
    // A new page starts with a headline holding its name: not twice
    assert_eq!(html("* Page\ntexte"), "<h1>Page</h1><p>texte</p>");
    assert!(html("texte").starts_with("<h1 class='title'>Page</h1>"));
}

#[test]
fn headlines_with_keyword_and_tags() {
    assert_eq!(html("* Page\n** TODO Faire *vite* :a:b:"),
        "<h1>Page</h1><h2><span class='todo'>TODO</span> Faire <b>vite</b> <span class='tag'>a</span><span class='tag'>b</span></h2>");
}

#[test]
fn paragraphs_lists_and_blocks() {
    let out = html("* Page\nune\nligne\n\n- a\n  suite\n  - b\n- [X] c\n1. un\n#+BEGIN_SRC rust\nlet x = 1 < 2;\n#+END_SRC\n:PROPERTIES:\n:ID: x\n:END:\n# commentaire\n-----");
    assert_eq!(out, "<h1>Page</h1><p>une\nligne</p>\
        <ul><li>a suite<ul><li>b</li></ul></li><li>☑ c</li></ul><ol><li>un</li></ol>\
        <pre>let x = 1 &lt; 2;</pre><hr>");
}

#[test]
fn tables_with_a_header() {
    assert_eq!(html("* Page\n| a | b |\n|---+---|\n| 1 | =x= |"),
        "<h1>Page</h1><table><tr><th>a</th><th>b</th></tr><tr><td>1</td><td><code>x</code></td></tr></table>");
}

#[test]
fn links_and_images() {
    let out = html("* Page\nvoir [[Recettes::*Desserts]], [[https://a.org][site]], [[B][bé]] et https://b.org <<ici>>\n#+ATTR_ORG: :width 300\n[[../assets/a.png]]");
    assert_eq!(out, "<h1>Page</h1><p>voir <span class='link'>Recettes › Desserts</span>, \
        <a href='https://a.org'>site</a>, <span class='link'>bé</span> et <a href='https://b.org'>https://b.org</a> </p>\
        <p class='image'><img src='url:../assets/a.png' alt='' style='width: 300px'></p>");
}

#[test]
fn emphasis_as_org_reads_it() {
    let d = Doc { keywords: None, image: &|t| t.to_string(), shift: 0, format: Format::Org, extended: place::LinkSyntax::NONE, defs: vec![], notes: Default::default() };
    assert_eq!(d.inline("a /b/ c_d_e 2*3*4 +x+"), "a <i>b</i> c_d_e 2*3*4 <s>x</s>");
    assert_eq!(d.inline("=a*b*= ~<t>~"), "<code>a*b*</code> <code>&lt;t&gt;</code>");
}
