//! The front matter of a Markdown page: YAML between two `---` lines at its
//! top. Its `title:` is the page's title, its `tags:` tag the whole page (as
//! `#+FILETAGS:` does in org-mode). Only these simple forms are read:
//!
//! ```text
//! ---
//! title: Ma page
//! tags: [projet, idée]      (or `tags: projet, idée`, or one `- tag` per line below)
//! ---
//! ```

/// The lines of `text`'s front matter (between the `---` lines), and the index
/// of the line after it. `None` when the page has none.
pub fn lines(text: &str) -> Option<(Vec<&str>, usize)> {
    let mut all = text.split('\n');
    if all.next()?.trim_end() != "---" { return None; }
    let mut inside = Vec::new();
    for l in all {
        if matches!(l.trim_end(), "---" | "...") { let n = inside.len() + 2; return Some((inside, n)); }
        inside.push(l);
    }
    None
}

/// `"x"`, `'x'` → `x`.
fn unquote(v: &str) -> &str {
    let v = v.trim();
    for q in ['"', '\''] {
        if v.len() >= 2 && v.starts_with(q) && v.ends_with(q) { return &v[1..v.len() - 1]; }
    }
    v
}

/// The value of `key:` on its line, when it is a top-level key.
fn value<'a>(fields: &[&'a str], key: &str) -> Option<(usize, &'a str)> {
    fields.iter().enumerate().find_map(|(n, l)| {
        let (k, v) = l.split_once(':')?;
        (!l.starts_with([' ', '\t']) && k.trim().eq_ignore_ascii_case(key)).then_some((n, v))
    })
}

/// The `title:` of the front matter, if not empty.
pub fn title(text: &str) -> Option<String> {
    let (fields, _) = lines(text)?;
    let (_, v) = value(&fields, "title")?;
    Some(unquote(v).to_string()).filter(|t| !t.is_empty())
}

/// The `tags:` of the front matter (`[a, b]`, `a, b`, `a b`, or a `- a` list
/// under it), without a leading `#`.
pub fn tags(text: &str) -> Vec<String> {
    let Some((fields, _)) = lines(text) else { return vec![] };
    let Some((n, v)) = value(&fields, "tags").or_else(|| value(&fields, "tag")) else { return vec![] };
    let v = v.trim();
    let words: Vec<&str> = if v.is_empty() {
        fields[n + 1..].iter()
            .take_while(|l| l.trim_start().starts_with("- ") || l.trim().is_empty())
            .filter_map(|l| l.trim_start().strip_prefix("- "))
            .collect()
    } else {
        let v = v.strip_prefix('[').and_then(|v| v.strip_suffix(']')).unwrap_or(v);
        v.split([',', ' ']).collect()
    };
    let mut out: Vec<String> = Vec::new();
    for w in words {
        let w = unquote(w);
        let w = w.strip_prefix('#').unwrap_or(w).trim();
        if !w.is_empty() && !out.iter().any(|t| t == w) { out.push(w.to_string()); }
    }
    out
}

/// `text` with the `title:` of its front matter set to `title`; a front matter
/// is added when the page has none.
pub fn set_title(text: &str, title: &str) -> String {
    let quoted = if title.contains(':') || title.starts_with(['"', '\'', '[', '#']) {
        format!("\"{}\"", title.replace('"', "\\\""))
    } else {
        title.to_string()
    };
    let Some((fields, _)) = lines(text) else {
        return format!("---\ntitle: {quoted}\n---\n{text}");
    };
    let mut all: Vec<String> = text.split('\n').map(String::from).collect();
    match value(&fields, "title") {
        // The fields start on the second line
        Some((n, _)) => all[n + 1] = format!("title: {quoted}"),
        None => all.insert(1, format!("title: {quoted}")),
    }
    all.join("\n")
}

/// `text` with `tag` added to the `tags:` of its front matter (written as a
/// `[a, b]` list); a front matter is added when the page has none. `None` when
/// the page already has the tag.
pub fn add_tag(text: &str, tag: &str) -> Option<String> {
    let mut list = tags(text);
    if list.iter().any(|t| t == tag) { return None; }
    list.push(tag.to_string());
    let line = format!("tags: [{}]", list.join(", "));
    let Some((fields, _)) = lines(text) else {
        return Some(format!("---\n{line}\n---\n{text}"));
    };
    let mut all: Vec<String> = text.split('\n').map(String::from).collect();
    match value(&fields, "tags").or_else(|| value(&fields, "tag")) {
        Some((n, v)) => {
            // A `- tag` list under the key goes with it
            let below = if v.trim().is_empty() {
                fields[n + 1..].iter().take_while(|l| l.trim_start().starts_with("- ")).count()
            } else { 0 };
            all.splice(n + 1..n + 2 + below, [line]);
        }
        None => all.insert(fields.len() + 1, line),
    }
    Some(all.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_title_and_the_tags() {
        let text = "---\ntitle: \"Mon titre\"\ntags: [projet, \"idée\", #rust]\n---\n# Corps";
        assert_eq!(lines(text).map(|(_, n)| n), Some(4));
        assert_eq!(title(text).as_deref(), Some("Mon titre"));
        assert_eq!(tags(text), vec!["projet", "idée", "rust"]);
        assert_eq!(tags("---\ntags:\n  - a\n  - b\nautre: x\n---"), vec!["a", "b"]);
        assert_eq!(tags("---\ntags: a, b c\n---"), vec!["a", "b", "c"]);
        assert!(title("# Pas d'en-tête\n---\ntitle: x\n---").is_none());
        assert!(title("---\ntitle: x\n").is_none());
    }

    #[test]
    fn adds_a_tag() {
        assert_eq!(add_tag("# A", "x").unwrap(), "---\ntags: [x]\n---\n# A");
        assert_eq!(add_tag("---\ntitle: T\n---\n", "x").unwrap(), "---\ntitle: T\ntags: [x]\n---\n");
        assert_eq!(add_tag("---\ntags:\n  - a\nz: 1\n---", "b").unwrap(), "---\ntags: [a, b]\nz: 1\n---");
        assert!(add_tag("---\ntags: [a]\n---", "a").is_none());
    }

    #[test]
    fn sets_the_title() {
        assert_eq!(set_title("# A", "Neuf"), "---\ntitle: Neuf\n---\n# A");
        assert_eq!(set_title("---\ntags: a\ntitle: Vieux\n---\nx", "Un : deux"), "---\ntags: a\ntitle: \"Un : deux\"\n---\nx");
        assert_eq!(set_title("---\ntags: a\n---\nx", "T"), "---\ntitle: T\ntags: a\n---\nx");
    }
}
