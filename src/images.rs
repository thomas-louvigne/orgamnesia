//! Images in the page: a line holding only a link to an image file
//! (`[[../assets/photo.png]]`, maybe after headline stars or a list bullet) is
//! drawn as the image while the caret is elsewhere (see `tables::View`). A link
//! to a YouTube video (`[[https://www.youtube.com/watch?v=…]]`) is drawn the same
//! way, as its thumbnail.
//!
//! Its size is read as Emacs and Logseq write it: `#+ATTR_ORG: :width 300` on
//! the line above (org-mode), or `{:height 200, :width 300}` after the link
//! (Logseq, which adds one at each resize: the last one counts). The first wins.

use orgamnesia_core::assets::EXTENSIONS;
use orgamnesia_core::links::link_at;
use orgamnesia_core::org::{line_bounds, line_start};

/// A line of the page that is an image.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageLine {
    /// Chars before the link: indentation, headline stars or list bullet.
    pub prefix: usize,
    /// Chars up to the end of the link (`]]`).
    pub link_end: usize,
    /// Where the link points, without `file:`.
    pub target: String,
    /// Size given after the link (Logseq), in px.
    pub width: Option<f64>,
    pub height: Option<f64>,
    /// Not a link but Logseq's `{{video url}}` macro: read, but to be replaced
    /// by a plain link, which other tools read.
    pub logseq_macro: bool,
}

/// A width given by `#+ATTR_ORG:`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Width {
    Px(f64),
    /// A part of the width of the text (`50%`, `0.5`).
    Part(f64),
}

/// Whether a link target is an image file (by its extension).
pub fn is_image(target: &str) -> bool {
    let name = target.rsplit('/').next().unwrap_or(target);
    name.rsplit_once('.').is_some_and(|(_, ext)| EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
}

/// The id of a YouTube video a link points to (`youtube.com/watch?v=ID`,
/// `youtu.be/ID`, `/shorts/ID`, `/embed/ID`, `/live/ID`).
pub fn youtube_id(target: &str) -> Option<String> {
    let rest = target.split_once("://").map_or(target, |(_, r)| r);
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = host.strip_prefix("www.").or_else(|| host.strip_prefix("m.")).or_else(|| host.strip_prefix("music.")).unwrap_or(host);
    let (path, query) = path.split_once('?').unwrap_or((path, ""));
    let id = match host {
        "youtu.be" => path.split('/').next(),
        "youtube.com" | "youtube-nocookie.com" => match path.split_once('/') {
            Some(("shorts" | "embed" | "live" | "v", id)) => id.split('/').next(),
            _ if path == "watch" => query.split('&').find_map(|kv| kv.strip_prefix("v=")),
            _ => None,
        },
        _ => None,
    }?;
    let id = id.split('#').next().unwrap_or(id);
    (id.len() == 11 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')).then(|| id.to_string())
}

/// The thumbnail of the YouTube video `id` (320 x 180, without black bars).
pub fn youtube_thumbnail(id: &str) -> String {
    format!("https://i.ytimg.com/vi/{id}/mqdefault.jpg")
}

/// The image of a line, when the line holds only that: a link with no description to an
/// image or a YouTube video, maybe after headline stars or a list bullet, maybe
/// followed by Logseq sizes.
pub fn parse(line: &[char]) -> Option<ImageLine> {
    let mut i = 0;
    let stars = line.iter().take_while(|&&c| c == '*').count();
    if stars > 0 && line.get(stars) == Some(&' ') {
        i = stars + 1;
    } else {
        i += line.iter().take_while(|c| c.is_whitespace()).count();
        if matches!(line.get(i), Some('-' | '+')) && line.get(i + 1) == Some(&' ') { i += 2; }
    }
    let prefix = i;
    let (target, link_end, logseq_macro) = if line.get(i..i + 2) == Some(&['{', '{'][..]) {
        let (target, end) = video_macro(&line[i..])?;
        (target, i + end, true)
    } else {
        let link = link_at(line, i).filter(|l| l.description.is_none())?;
        let inner = link.target(line);
        let target = inner.strip_prefix("file:").unwrap_or(&inner).to_string();
        if target.is_empty() || !(is_image(&target) || youtube_id(&target).is_some()) { return None; }
        (target, link.end, false)
    };
    let (mut width, mut height) = (None, None);
    let mut j = link_end;
    loop {
        while line.get(j).is_some_and(|c| c.is_whitespace()) { j += 1; }
        match line.get(j) {
            None => break,
            Some('{') => {
                let end = (j..line.len()).find(|&k| line[k] == '}')?;
                let map: String = line[j + 1..end].iter().collect();
                for entry in map.split(',') {
                    let mut kv = entry.split_whitespace();
                    let value = |v: Option<&str>| v.and_then(|v| v.parse::<f64>().ok()).filter(|v| *v > 0.0);
                    match kv.next() {
                        Some(":width") => width = value(kv.next()).or(width),
                        Some(":height") => height = value(kv.next()).or(height),
                        _ => {}
                    }
                }
                j = end + 1;
            }
            Some(_) => return None,
        }
    }
    Some(ImageLine { prefix, link_end, target, width, height, logseq_macro })
}

/// Logseq's `{{video url}}` (or `{{youtube url}}`) macro at the start of `chars`,
/// for a YouTube video: its url, and the chars it takes.
fn video_macro(chars: &[char]) -> Option<(String, usize)> {
    let close = (2..chars.len().saturating_sub(1)).find(|&j| chars[j] == '}' && chars[j + 1] == '}')?;
    let inner: String = chars[2..close].iter().collect();
    let (name, url) = inner.trim().split_once(char::is_whitespace)?;
    let url = url.trim();
    (matches!(name, "video" | "youtube") && youtube_id(url).is_some()).then(|| (url.to_string(), close + 2))
}

/// The edit turning Logseq's `{{video url}}` macro of the line at page position
/// `at` into the link `[[url]]`: the page range to replace and its new text.
pub fn macro_to_link(content: &str, at: usize) -> Option<(usize, usize, String)> {
    let chars: Vec<char> = content.chars().collect();
    let at = at.min(chars.len());
    let (start, end) = line_bounds(&chars, at);
    let image = parse(&chars[start..end]).filter(|i| i.logseq_macro)?;
    Some((start + image.prefix, start + image.link_end, format!("[[{}]]", image.target)))
}

/// Whether a line is an `#+ATTR_ORG:` keyword.
fn is_attr_org(line: &str) -> bool {
    line.trim_start().get(..11).is_some_and(|k| k.eq_ignore_ascii_case("#+attr_org:"))
}

/// The width an `#+ATTR_ORG:` line gives: `:width 300`, `300px`, `50%` or `0.5`.
pub fn attr_width(line: &str) -> Option<Width> {
    if !is_attr_org(line) { return None; }
    let mut words = line.split_whitespace().skip_while(|w| *w != ":width");
    let value = words.nth(1)?;
    if let Some(p) = value.strip_suffix('%') {
        return p.parse::<f64>().ok().filter(|p| *p > 0.0).map(|p| Width::Part(p / 100.0));
    }
    let n = value.strip_suffix("px").unwrap_or(value).parse::<f64>().ok().filter(|n| *n > 0.0)?;
    Some(if value.contains('.') && n <= 1.0 { Width::Part(n) } else { Width::Px(n) })
}

/// Size the image is drawn at, in px: its natural size `natural`, or the width asked
/// (`#+ATTR_ORG:` first, then Logseq's), never wider than `max`, its proportions kept.
pub fn display_size(natural: (f64, f64), attr: Option<Width>, image: &ImageLine, max: f64) -> (f64, f64) {
    let ratio = if natural.0 > 0.0 { natural.1 / natural.0 } else { 1.0 };
    let width = match attr {
        Some(Width::Px(w)) => w,
        Some(Width::Part(p)) => max * p,
        None => image.width.or(image.height.map(|h| h / ratio)).unwrap_or(natural.0),
    };
    let width = width.min(max).max(1.0);
    (width, width * ratio)
}

/// The edit giving the image of the line at page position `at` the size `width` x
/// `height`: the page range to replace and its new text. With `org`, the size goes to
/// the `#+ATTR_ORG:` line above (added when missing) and Logseq's sizes are removed;
/// else it goes after the link, Logseq's way, and `:width` leaves `#+ATTR_ORG:`.
pub fn resize(content: &str, at: usize, width: f64, height: f64, org: bool) -> Option<(usize, usize, String)> {
    let chars: Vec<char> = content.chars().collect();
    let at = at.min(chars.len());
    let (start, end) = line_bounds(&chars, at);
    let image = parse(&chars[start..end])?;
    let link: String = chars[start..start + image.link_end].iter().collect();
    let (w, h) = (width.round(), height.round());
    // The `#+ATTR_ORG:` line above, if any: where it starts and its text
    let above = (start > 0).then(|| {
        let a = line_start(&chars, start - 1);
        (a, chars[a..start - 1].iter().collect::<String>())
    }).filter(|(_, l)| is_attr_org(l));
    Some(if org {
        let attr = match &above {
            Some((_, l)) => set_width(l, Some(w)),
            None => {
                let indent: String = chars[start..].iter().take_while(|&&c| c == ' ' || c == '\t').collect();
                format!("{indent}#+ATTR_ORG: :width {w}")
            }
        };
        (above.as_ref().map_or(start, |(a, _)| *a), end, format!("{attr}\n{link}"))
    } else {
        let line = format!("{link}{{:height {h}, :width {w}}}");
        match above {
            Some((a, l)) => {
                let attr = set_width(&l, None);
                let empty = attr.trim().eq_ignore_ascii_case("#+attr_org:");
                (a, end, if empty { line } else { format!("{attr}\n{line}") })
            }
            None => (start, end, line),
        }
    })
}

/// An `#+ATTR_ORG:` line with its `:width` set to `width`, or removed with `None`.
fn set_width(line: &str, width: Option<f64>) -> String {
    let indent = &line[..line.len() - line.trim_start().len()];
    let mut words: Vec<String> = line.split_whitespace().map(String::from).collect();
    if let Some(i) = words.iter().position(|w| w == ":width") {
        let has_value = words.get(i + 1).is_some_and(|v| !v.starts_with(':'));
        words.drain(i..if has_value { i + 2 } else { i + 1 });
    }
    if let Some(w) = width {
        words.push(":width".into());
        words.push(w.to_string());
    }
    format!("{indent}{}", words.join(" "))
}

/// Path of the file a link `target` of the page at `page` points to: relative
/// targets are read from the page's folder (`..` and `.` resolved).
pub fn resolve(page: &str, target: &str) -> String {
    if target.starts_with('/') || target.starts_with("~/") || target.contains("://") { return target.to_string(); }
    let mut parts: Vec<&str> = page.split('/').collect();
    parts.pop();
    for part in target.split('/') {
        match part {
            "." | "" => {}
            ".." => { if parts.len() > 1 { parts.pop(); } }
            p => parts.push(p),
        }
    }
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn logseq_video_macro() {
        let line: Vec<char> = "- {{video https://www.youtube.com/watch?v=dQw4w9WgXcQ}}".chars().collect();
        let image = parse(&line).unwrap();
        assert!(image.logseq_macro);
        assert_eq!((image.prefix, image.link_end, image.target.as_str()), (2, line.len(), "https://www.youtube.com/watch?v=dQw4w9WgXcQ"));
        assert!(parse(&"{{youtube https://youtu.be/dQw4w9WgXcQ}}".chars().collect::<Vec<_>>()).is_some());
        // Another video site, another macro: text
        assert!(parse(&"{{video https://vimeo.com/123}}".chars().collect::<Vec<_>>()).is_none());
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
}
