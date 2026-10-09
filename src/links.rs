//! What a `[[link]]` points to, to tell the links that lead nowhere: a page
//! that doesn't exist, a file missing from the disk.

/// The path a link target points to when it names a file: `file:x`, `./x`, `../x`,
/// `/x` or `~/x`.
pub fn file_target(target: &str) -> Option<&str> {
    let t = target.trim();
    let t = t.strip_prefix("file:").unwrap_or(t);
    let file = t.starts_with("./") || t.starts_with("../") || t.starts_with('/') || t.starts_with("~/");
    (file || target.trim().starts_with("file:")).then_some(t).filter(|t| !t.is_empty())
}

/// Whether a link target names a page, as the backend reads it (`parser::is_page_link`):
/// not a file, an address (`https://`, `mailto:`) nor a place in the page (`#id`, `*Titre`).
pub fn is_page_target(target: &str) -> bool {
    let t = target.trim();
    !(t.is_empty() || t.contains("://") || t.starts_with("mailto:") || t.starts_with('#')
        || t.starts_with('*') || file_target(t).is_some())
}

/// Targets of the `[[links]]` of `text` (`[[target]]`, `[[target][label]]`).
pub fn targets(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find("[[") {
        rest = &rest[open + 2..];
        let Some(end) = rest.find(']') else { break };
        let target = &rest[..end];
        if !target.contains(['\n', '[']) && matches!(rest[end + 1..].chars().next(), Some(']' | '[')) {
            out.push(target);
        }
        rest = &rest[end..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_are_pages_or_files() {
        assert_eq!(file_target("../assets/a.png"), Some("../assets/a.png"));
        assert_eq!(file_target("file:notes.pdf"), Some("notes.pdf"));
        assert_eq!(file_target("~/doc.pdf"), Some("~/doc.pdf"));
        assert_eq!(file_target("Page"), None);
        assert!(is_page_target("Ma page"));
        assert!(!is_page_target("../assets/a.png"));
        assert!(!is_page_target("https://example.org"));
        assert!(!is_page_target("*Titre"));
    }

    #[test]
    fn link_targets_of_a_text() {
        let text = "voir [[A]] et [[../b.png][image]], pas [[c\nd]] ni [[e]";
        assert_eq!(targets(text), vec!["A", "../b.png"]);
        assert_eq!(targets("[[x]]{:width 3}"), vec!["x"]);
    }
}
