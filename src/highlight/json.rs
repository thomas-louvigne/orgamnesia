//! JSON highlighted, for the configuration files of the "preference" project.

use crate::html::{escape, push_escaped};

/// Syntax-highlighted HTML of a JSON text (the configuration files of the
/// "preference" project): keys, strings, numbers, `true` / `false` / `null`.
/// With `numbers`, each line starts with its number, drawn in the margin.
pub fn render_json(text: &str, numbers: bool) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    for (n, line) in text.split('\n').enumerate() {
        if numbers { out.push_str(&format!("<span class='ln'>{}</span>", n + 1)); }
        out.push_str(&json_line(line));
        out.push('\n');
    }
    out
}

fn json_line(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let text = |a: usize, b: usize| escape(&chars[a..b].iter().collect::<String>());
    let span = |class: &str, a: usize, b: usize| format!("<span class='{class}'>{}</span>", text(a, b));
    let word = |c: char| c.is_alphanumeric() || matches!(c, '_' | '-' | '+' | '.');
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' {
            // To the closing quote (not an escaped one), or the end of the line
            let mut j = i + 1;
            while j < chars.len() && chars[j] != '"' { j += if chars[j] == '\\' { 2 } else { 1 }; }
            let end = (j + 1).min(chars.len());
            let key = chars[end..].iter().find(|c| !c.is_whitespace()) == Some(&':');
            out.push_str(&span(if key { "json-key" } else { "json-str" }, i, end));
            i = end;
        } else if word(c) {
            let end = chars[i..].iter().position(|&c| !word(c)).map_or(chars.len(), |n| i + n);
            let token: String = chars[i..end].iter().collect();
            let class = match token.as_str() {
                "true" | "false" | "null" => "json-lit",
                _ if token.parse::<f64>().is_ok() => "json-num",
                _ => "json-bad",
            };
            out.push_str(&span(class, i, end));
            i = end;
        } else if "{}[],:".contains(c) {
            out.push_str(&span("json-punct", i, i + 1));
            i += 1;
        } else {
            push_escaped(&mut out, c);
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_is_highlighted() {
        let html = render_json("{\n  \"a\": [true, 1.5, \"x<\"],\n  \"b\": nul\n}", true);
        assert!(html.starts_with("<span class='ln'>1</span><span class='json-punct'>{</span>\n"), "{html}");
        assert!(html.contains("<span class='json-key'>&quot;a&quot;</span><span class='json-punct'>:</span>"));
        assert!(html.contains("<span class='json-lit'>true</span>"));
        assert!(html.contains("<span class='json-num'>1.5</span>"));
        assert!(html.contains("<span class='json-str'>&quot;x&lt;&quot;</span>"));
        assert!(html.contains("<span class='json-bad'>nul</span>"));
        assert_eq!(html.matches('\n').count(), 4);
    }
}
