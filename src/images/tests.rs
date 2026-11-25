use super::*;

#[test]
fn excalidraw_drawings() {
    assert!(is_excalidraw("../assets/Schema.Excalidraw.SVG"));
    assert!(is_excalidraw("file:a.excalidraw.png"));
    assert!(!is_excalidraw("../assets/a.svg") && !is_excalidraw("excalidraw.svg"));
}

#[test]
fn youtube_links() {
    let id = Some("dQw4w9WgXcQ".to_string());
    for url in [
        "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
        "https://youtube.com/watch?feature=share&v=dQw4w9WgXcQ&t=42",
        "https://m.youtube.com/watch?v=dQw4w9WgXcQ",
        "https://youtu.be/dQw4w9WgXcQ?si=abc",
        "https://www.youtube.com/shorts/dQw4w9WgXcQ",
        "https://www.youtube.com/embed/dQw4w9WgXcQ",
        "youtu.be/dQw4w9WgXcQ",
    ] {
        assert_eq!(youtube_id(url), id, "{url}");
    }
    for url in ["https://www.youtube.com/", "https://www.youtube.com/watch?v=short", "https://vimeo.com/123", "https://example.com/watch?v=dQw4w9WgXcQ"] {
        assert_eq!(youtube_id(url), None, "{url}");
    }
    let line: Vec<char> = "- [[https://youtu.be/dQw4w9WgXcQ]] {:width 400}".chars().collect();
    let image = parse(&line).unwrap();
    assert_eq!((image.target.as_str(), image.width), ("https://youtu.be/dQw4w9WgXcQ", Some(400.0)));
    // With a description, it stays a link
    let line: Vec<char> = "[[https://youtu.be/dQw4w9WgXcQ][Clip]]".chars().collect();
    assert!(parse(&line).is_none());
    assert!(!parse(&"[[https://youtu.be/dQw4w9WgXcQ]]".chars().collect::<Vec<_>>()).unwrap().logseq_macro);
}

#[test]
fn video_sites() {
    let v = |url: &str| video(url).map(|v| (v.platform, v.id, v.extra));
    let some = |p, id: &str, extra: Option<&str>| Some((p, id.to_string(), extra.map(String::from)));
    assert_eq!(v("https://youtu.be/dQw4w9WgXcQ"), some(Platform::YouTube, "dQw4w9WgXcQ", None));
    assert_eq!(v("https://vimeo.com/76979871"), some(Platform::Vimeo, "76979871", None));
    assert_eq!(v("https://vimeo.com/channels/staffpicks/76979871"), some(Platform::Vimeo, "76979871", None));
    assert_eq!(v("https://vimeo.com/76979871/8272103f6e"), some(Platform::Vimeo, "76979871", Some("8272103f6e")));
    assert_eq!(v("https://player.vimeo.com/video/76979871?h=8272103f6e"), some(Platform::Vimeo, "76979871", Some("8272103f6e")));
    assert_eq!(v("https://www.dailymotion.com/video/x8j2abc"), some(Platform::Dailymotion, "x8j2abc", None));
    assert_eq!(v("https://www.dailymotion.com/video/x8j2abc_un-titre"), some(Platform::Dailymotion, "x8j2abc", None));
    assert_eq!(v("https://dai.ly/x8j2abc"), some(Platform::Dailymotion, "x8j2abc", None));
    assert_eq!(v("https://www.twitch.tv/videos/1234567"), some(Platform::Twitch, "v1234567", None));
    assert_eq!(v("https://www.twitch.tv/chaine/clip/DrolePetitClip-abc_1"), some(Platform::Twitch, "clip:DrolePetitClip-abc_1", None));
    assert_eq!(v("https://clips.twitch.tv/DrolePetitClip"), some(Platform::Twitch, "clip:DrolePetitClip", None));
    assert_eq!(v("https://framatube.org/w/9c9de5e8-0a1e-484a-b099-e80766180a6d"),
        some(Platform::PeerTube, "9c9de5e8-0a1e-484a-b099-e80766180a6d", Some("framatube.org")));
    assert_eq!(v("https://framatube.org/w/kkGMgK9ZtnKfYAgnEtQxbv"), some(Platform::PeerTube, "kkGMgK9ZtnKfYAgnEtQxbv", Some("framatube.org")));
    assert_eq!(v("https://tube.example.org/videos/watch/9c9de5e8-0a1e-484a-b099-e80766180a6d"),
        some(Platform::PeerTube, "9c9de5e8-0a1e-484a-b099-e80766180a6d", Some("tube.example.org")));
    for url in ["https://vimeo.com/", "https://vimeo.com/channels/staffpicks", "https://www.dailymotion.com/fr",
        "https://www.twitch.tv/chaine", "https://example.com/w/abc", "https://example.com/videos/watch/abc", "../assets/photo.png"] {
        assert_eq!(v(url), None, "{url}");
    }
}

#[test]
fn video_players() {
    let embed = |url: &str| video(url).unwrap().embed_url("tauri.localhost");
    assert_eq!(embed("https://youtu.be/dQw4w9WgXcQ"), "https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ?autoplay=1&rel=0");
    assert_eq!(embed("https://vimeo.com/76979871/8272103f6e"), "https://player.vimeo.com/video/76979871?h=8272103f6e&autoplay=1");
    assert_eq!(embed("https://dai.ly/x8j2abc"), "https://www.dailymotion.com/embed/video/x8j2abc?autoplay=1");
    assert_eq!(embed("https://www.twitch.tv/videos/1234567"), "https://player.twitch.tv/?video=v1234567&parent=tauri.localhost&autoplay=true");
    assert_eq!(embed("https://clips.twitch.tv/DrolePetitClip"), "https://clips.twitch.tv/embed?clip=DrolePetitClip&parent=tauri.localhost&autoplay=true");
    assert_eq!(embed("https://framatube.org/w/kkGMgK9ZtnKfYAgnEtQxbv"), "https://framatube.org/videos/embed/kkGMgK9ZtnKfYAgnEtQxbv?autoplay=1");
    // A card for the sites without a thumbnail by address, written to go between `'`
    let card = video("https://vimeo.com/76979871").unwrap().thumbnail();
    assert!(card.starts_with("data:image/svg+xml,") && card.contains("Vimeo") && !card.contains(['\'', '"', '#', ' ']));
}

#[test]
fn logseq_video_macro() {
    let line: Vec<char> = "- {{video https://www.youtube.com/watch?v=dQw4w9WgXcQ}}".chars().collect();
    let image = parse(&line).unwrap();
    assert!(image.logseq_macro);
    assert_eq!((image.prefix, image.link_end, image.target.as_str()), (2, line.len(), "https://www.youtube.com/watch?v=dQw4w9WgXcQ"));
    assert!(parse(&"{{youtube https://youtu.be/dQw4w9WgXcQ}}".chars().collect::<Vec<_>>()).is_some());
    // Another video site
    assert!(parse(&"{{video https://vimeo.com/123}}".chars().collect::<Vec<_>>()).is_some());
    assert!(parse(&"{{video https://example.com/clip}}".chars().collect::<Vec<_>>()).is_none());
    assert!(parse(&"{{embed [[Page]]}}".chars().collect::<Vec<_>>()).is_none());
    let page = "* Vidéos\n- {{video https://youtu.be/dQw4w9WgXcQ}}\nfin\n";
    let (a, b, text) = macro_to_link(page, 12).unwrap();
    let mut fixed: Vec<char> = page.chars().collect();
    fixed.splice(a..b, text.chars());
    assert_eq!(fixed.into_iter().collect::<String>(), "* Vidéos\n- [[https://youtu.be/dQw4w9WgXcQ]]\nfin\n");
    assert!(macro_to_link("[[https://youtu.be/dQw4w9WgXcQ]]", 0).is_none());
}

fn line(s: &str) -> Option<ImageLine> {
    parse(&s.chars().collect::<Vec<_>>())
}

#[test]
fn markdown_images() {
    let img = line("![Une photo](../assets/a%20b.png)").unwrap();
    assert_eq!((img.prefix, img.link_end, img.target.as_str()), (0, 33, "../assets/a b.png"));
    assert_eq!(line("- ![](<../assets/x.jpg> \"titre\")").unwrap().target, "../assets/x.jpg");
    assert!(line("![lien](page.md)").is_none());
    assert!(line("![a](x.png) et du texte").is_none());
}

#[test]
fn image_lines_are_read_with_their_logseq_sizes() {
    let img = line("* [[../assets/ramchard_1759520598218_0.png]]{:height 572, :width 1008}{:height 321, :width 559}").unwrap();
    assert_eq!((img.prefix, img.target.as_str()), (2, "../assets/ramchard_1759520598218_0.png"));
    assert_eq!((img.width, img.height), (Some(559.0), Some(321.0)));
    assert_eq!(line("  - [[file:a.JPG]]").unwrap().target, "a.JPG");
    assert_eq!(line("[[./a.png]]").unwrap().width, None);
    assert!(line("[[a.png][desc]]").is_none());
    assert!(line("voir [[a.png]]").is_none());
    assert!(line("[[a.png]] et la suite").is_none());
    assert!(line("[[Page]]").is_none());
    assert!(line("** TODO [[a.png]]").is_none());
}

#[test]
fn attr_org_widths() {
    assert_eq!(attr_width("#+ATTR_ORG: :width 300"), Some(Width::Px(300.0)));
    assert_eq!(attr_width("#+attr_org: :align center :width 300px"), Some(Width::Px(300.0)));
    assert_eq!(attr_width("#+ATTR_ORG: :width 50%"), Some(Width::Part(0.5)));
    assert_eq!(attr_width("#+ATTR_ORG: :width 0.5"), Some(Width::Part(0.5)));
    assert_eq!(attr_width("#+ATTR_HTML: :width 300"), None);
    assert_eq!(attr_width("#+ATTR_ORG: :width"), None);
}

#[test]
fn sizes_keep_the_proportions_and_the_text_width() {
    let img = line("[[a.png]]{:height 321, :width 559}").unwrap();
    assert_eq!(display_size((1000.0, 500.0), None, &img, 800.0), (559.0, 279.5));
    assert_eq!(display_size((1000.0, 500.0), Some(Width::Px(200.0)), &img, 800.0), (200.0, 100.0));
    assert_eq!(display_size((1000.0, 500.0), Some(Width::Part(0.5)), &img, 800.0), (400.0, 200.0));
    let plain = line("[[a.png]]").unwrap();
    assert_eq!(display_size((1000.0, 500.0), None, &plain, 800.0), (800.0, 400.0));
    assert_eq!(display_size((100.0, 50.0), None, &plain, 800.0), (100.0, 50.0));
}

#[test]
fn resizing_writes_the_chosen_format() {
    let page = "* Titre\n* [[../assets/a.png]]{:height 572, :width 1008}\nfin";
    let at = 9;
    let (a, b, text) = resize(page, at, 300.0, 170.4, true).unwrap();
    assert_eq!((a, b, text.as_str()), (8, 55, "#+ATTR_ORG: :width 300\n* [[../assets/a.png]]"));
    let (a, b, text) = resize(page, at, 300.0, 170.4, false).unwrap();
    assert_eq!((a, b, text.as_str()), (8, 55, "* [[../assets/a.png]]{:height 170, :width 300}"));

    let page = "texte\n  #+ATTR_ORG: :width 100 :align left\n  [[a.png]]";
    let (a, _, text) = resize(page, 50, 250.0, 100.0, true).unwrap();
    assert_eq!((a, text.as_str()), (6, "  #+ATTR_ORG: :align left :width 250\n  [[a.png]]"));
    let (a, _, text) = resize(page, 50, 250.0, 100.0, false).unwrap();
    assert_eq!((a, text.as_str()), (6, "  #+ATTR_ORG: :align left\n  [[a.png]]{:height 100, :width 250}"));
    let page = "#+ATTR_ORG: :width 100\n[[a.png]]";
    assert_eq!(resize(page, 25, 250.0, 100.0, false).unwrap(), (0, 32, "[[a.png]]{:height 100, :width 250}".into()));
    assert_eq!(resize("du texte", 2, 1.0, 1.0, true), None);
}

#[test]
fn link_targets_resolve_from_the_page() {
    assert_eq!(resolve("/v/pages/a.org", "../assets/x.png"), "/v/assets/x.png");
    assert_eq!(resolve("/v/a.org", "./assets/x.png"), "/v/assets/x.png");
    assert_eq!(resolve("/v/pages/a.org", "/tmp/x.png"), "/tmp/x.png");
    assert_eq!(resolve("/v/pages/a.org", "https://e.org/x.png"), "https://e.org/x.png");
}
