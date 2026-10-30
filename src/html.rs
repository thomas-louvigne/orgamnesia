//! Text put in HTML.

/// `s` with the characters HTML reads as markup written as entities, so that it
/// shows as is, in the text and in attributes quoted with `'` or `"`.
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() { push_escaped(&mut out, c); }
    out
}

/// Add `c` to `out`, escaped as `escape` does.
pub fn push_escaped(out: &mut String, c: char) {
    match c {
        '&' => out.push_str("&amp;"),
        '<' => out.push_str("&lt;"),
        '>' => out.push_str("&gt;"),
        '"' => out.push_str("&quot;"),
        '\'' => out.push_str("&#39;"),
        c => out.push(c),
    }
}
