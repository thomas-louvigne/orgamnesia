//! A configuration file (YAML) highlighted, for the "preference" project: keys,
//! strings, numbers, `true` / `false` / `null`, punctuation, comments. The pieces
//! are those of `orgamnesia_core::config::yaml_tokens`, as the terminal draws them.

use crate::html::escape;
use orgamnesia_core::config::{yaml_tokens, Token};

/// Syntax-highlighted HTML of the configuration file `text`. With `numbers`,
/// each line starts with its number, drawn in the margin.
pub fn render_yaml(text: &str, numbers: bool) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    for (n, line) in text.split('\n').enumerate() {
        if numbers { out.push_str(&format!("<span class='ln'>{}</span>", n + 1)); }
        let chars: Vec<char> = line.chars().collect();
        let part = |a: usize, b: usize| escape(&chars[a..b].iter().collect::<String>());
        let mut at = 0;
        for (a, b, token) in yaml_tokens(line) {
            let class = match token {
                Token::Key => "cfg-key", Token::Str => "cfg-str", Token::Number => "cfg-num",
                Token::Literal => "cfg-lit", Token::Punct => "cfg-punct", Token::Comment => "cfg-comment",
            };
            out.push_str(&part(at, a));
            out.push_str(&format!("<span class='{class}'>{}</span>", part(a, b)));
            at = b;
        }
        out.push_str(&part(at, chars.len()));
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yaml_is_highlighted() {
        let html = render_yaml("# note <a>\napp:\n  save: [ctrl+s, 1]\n  on: true", true);
        assert!(html.starts_with("<span class='ln'>1</span><span class='cfg-comment'># note &lt;a&gt;</span>\n"), "{html}");
        assert!(html.contains("<span class='cfg-key'>app</span><span class='cfg-punct'>:</span>"));
        assert!(html.contains("  <span class='cfg-key'>save</span>"));
        assert!(html.contains("<span class='cfg-str'>ctrl+s</span>"));
        assert!(html.contains("<span class='cfg-num'>1</span>"));
        assert!(html.contains("<span class='cfg-lit'>true</span>"));
        assert_eq!(html.matches('\n').count(), 4);
    }
}
