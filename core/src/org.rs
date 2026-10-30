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

/// Level of the headline `line` (its stars: `** Titre` → 2), if it is one: stars
/// followed by a space, or alone on the line. `line` may go on past its `\n`.
pub fn headline_level(line: &[char]) -> Option<usize> {
    let stars = line.iter().take_while(|&&c| c == '*').count();
    (stars > 0 && matches!(line.get(stars), Some(' ' | '\n') | None)).then_some(stars)
}

/// `headline_level` of a line given as text.
pub fn headline_level_str(line: &str) -> Option<usize> {
    let stars = line.chars().take_while(|&c| c == '*').count();
    (stars > 0 && matches!(line[stars..].chars().next(), Some(' ' | '\n') | None)).then_some(stars)
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
        assert_eq!(headline_level(&chars("** Titre")), Some(2));
        assert_eq!(headline_level(&chars("*")), Some(1));
        assert_eq!(headline_level(&chars("*\nx")), Some(1));
        assert_eq!(headline_level(&chars("*gras*")), None);
        assert_eq!(headline_level(&chars("texte")), None);
        assert_eq!(headline_level_str("*** a"), Some(3));
        assert_eq!(headline_level_str("**b"), None);
    }
}
