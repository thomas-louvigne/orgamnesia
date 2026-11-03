//! Lines and headlines of an org text, read as chars (`&[char]`, positions in
//! chars), as the editor and the readers of pages need them.

/// Start of the line holding `pos`.
pub fn line_start(chars: &[char], pos: usize) -> usize {
    chars[..pos.min(chars.len())].iter().rposition(|&c| c == '\n').map_or(0, |i| i + 1)
}

/// End of the line holding `pos` (its `\n`, or the end of the text).
pub fn line_end(chars: &[char], pos: usize) -> usize {
    chars[pos..].iter().position(|&c| c == '\n').map_or(chars.len(), |i| pos + i)
}

/// Start and end (before the `\n`) of the line holding `pos`.
pub fn line_bounds(chars: &[char], pos: usize) -> (usize, usize) {
    (line_start(chars, pos), line_end(chars, pos))
}

/// Value of the page's `#+TITLE:` keyword, if it has a non-empty one.
pub fn title(content: &str) -> Option<String> {
    content.lines().find_map(title_value).map(str::trim).filter(|v| !v.is_empty()).map(String::from)
}

/// The text after `#+TITLE:` (any case) when `line` is that keyword.
fn title_value(line: &str) -> Option<&str> {
    let line = line.trim_start();
    let key = line.get(..8)?;
    key.eq_ignore_ascii_case("#+title:").then(|| &line[8..])
}

/// `content` with its `#+TITLE:` set to `title`: the keyword is rewritten in
/// place, or added as the first line when the page has none.
pub fn set_title(content: &str, title: &str) -> String {
    let mut done = false;
    let mut out: Vec<String> = content.split('\n').map(|line| {
        if !done && title_value(line).is_some() {
            done = true;
            let indent = &line[..line.len() - line.trim_start().len()];
            let key = &line.trim_start()[..8];
            format!("{indent}{key} {title}")
        } else {
            line.to_string()
        }
    }).collect();
    if !done {
        out.insert(0, format!("#+TITLE: {title}"));
    }
    out.join("\n")
}

/// `content` with the file tag `tag` added to its `#+FILETAGS:` line, which is
/// created after the leading `#+` keywords (the top of the page) when missing.
/// `None` when the page already has that file tag.
pub fn add_filetag(content: &str, tag: &str) -> Option<String> {
    let mut lines: Vec<String> = content.split('\n').map(String::from).collect();
    let found = lines.iter().position(|l| crate::hashtags::filetags_value_start(&l.chars().collect::<Vec<_>>()).is_some());
    match found {
        Some(i) => {
            let chars: Vec<char> = lines[i].chars().collect();
            let v = crate::hashtags::filetags_value_start(&chars)?;
            let head: String = chars[..v].iter().collect();
            let value: String = chars[v..].iter().collect();
            if value.split(':').any(|t| t.trim().eq_ignore_ascii_case(tag)) {
                return None;
            }
            let value = value.trim_end();
            lines[i] = if value.trim().is_empty() {
                format!("{head} :{tag}:")
            } else if value.ends_with(':') {
                format!("{head}{value}{tag}:")
            } else {
                format!("{head}{value} :{tag}:")
            };
        }
        None => {
            let at = lines.iter().take_while(|l| l.trim_start().starts_with("#+")).count();
            lines.insert(at, format!("#+FILETAGS: :{tag}:"));
        }
    }
    Some(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(s: &str) -> Vec<char> { s.chars().collect() }

    #[test]
    fn lines() {
        let c = chars("ab\ncd\n");
        assert_eq!(line_bounds(&c, 0), (0, 2));
        assert_eq!(line_bounds(&c, 2), (0, 2));
        assert_eq!(line_bounds(&c, 4), (3, 5));
        assert_eq!(line_bounds(&c, 6), (6, 6));
        assert_eq!(line_start(&c, 99), 6);
    }

    #[test]
    fn headlines() {
        let org = crate::Format::Org;
        assert_eq!(org.heading_level(&chars("** Titre")), Some(2));
        assert_eq!(org.heading_level(&chars("*")), Some(1));
        assert_eq!(org.heading_level(&chars("*\nx")), Some(1));
        assert_eq!(org.heading_level(&chars("*gras*")), None);
        assert_eq!(org.heading_level(&chars("texte")), None);
        assert_eq!(org.heading_level_of("*** a"), Some(3));
        assert_eq!(org.heading_level_of("**b"), None);
    }

    #[test]
    fn reads_the_title_keyword() {
        assert_eq!(title("#+TITLE: Mon titre\n* A"), Some("Mon titre".into()));
        assert_eq!(title("* A\n  #+title:  Bas  \n"), Some("Bas".into()));
        assert_eq!(title("#+TITLE:\n* A"), None);
        assert_eq!(title("* #+TITLE: x"), None);
        assert_eq!(title(""), None);
    }

    #[test]
    fn sets_the_title_keyword() {
        assert_eq!(set_title("* A", "T"), "#+TITLE: T\n* A");
        assert_eq!(set_title("", "T"), "#+TITLE: T\n");
        assert_eq!(set_title("x\n#+title: old\n#+TITLE: 2", "New"), "x\n#+title: New\n#+TITLE: 2");
    }

    #[test]
    fn filetag_added() {
        assert_eq!(add_filetag("* Page\n", "pro").as_deref(), Some("#+FILETAGS: :pro:\n* Page\n"));
        assert_eq!(add_filetag("", "pro").as_deref(), Some("#+FILETAGS: :pro:\n"));
        assert_eq!(add_filetag("#+title: P\n* P", "pro").as_deref(), Some("#+title: P\n#+FILETAGS: :pro:\n* P"));
        assert_eq!(add_filetag("#+FILETAGS: :a:\nx", "b").as_deref(), Some("#+FILETAGS: :a:b:\nx"));
        assert_eq!(add_filetag("#+filetags:\nx", "b").as_deref(), Some("#+filetags: :b:\nx"));
        assert_eq!(add_filetag("#+filetags: :a:  \nx", "b").as_deref(), Some("#+filetags: :a:b:\nx"));
        assert_eq!(add_filetag("x\n#+filetags: :A:\n", "a"), None);
    }
}
