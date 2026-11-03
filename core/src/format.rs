//! The two formats a page can be written in, told by its extension: org-mode
//! (`.org`) and Markdown (`.md`).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    #[default]
    Org,
    Markdown,
}

impl Format {
    pub const ALL: [Format; 2] = [Format::Org, Format::Markdown];

    /// The extension of its files, without the dot.
    pub fn ext(self) -> &'static str {
        match self {
            Format::Org => "org",
            Format::Markdown => "md",
        }
    }

    /// The format of the extension `ext` (without the dot, any case).
    pub fn from_ext(ext: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.ext().eq_ignore_ascii_case(ext))
    }

    /// The format of the file `path`, from its extension.
    pub fn of_path(path: &str) -> Option<Self> {
        let file = crate::pages::file_name(path);
        let (_, ext) = file.rsplit_once('.')?;
        Self::from_ext(ext)
    }

    /// As the settings write it: `org` or `markdown`.
    pub fn id(self) -> &'static str {
        match self {
            Format::Org => "org",
            Format::Markdown => "markdown",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.id() == id)
    }

    /// The character a headline starts with, once per level: `*` (org-mode) or `#`.
    pub fn heading_mark(self) -> char {
        match self {
            Format::Org => '*',
            Format::Markdown => '#',
        }
    }

    /// The level of the headline starting `line` (`** Titre`, `## Titre`), if it is
    /// one: its marks followed by a space, the end of the line or of the text.
    /// An org-mode line starting with `# ` is a comment, not a headline.
    pub fn heading_level(self, line: &[char]) -> Option<usize> {
        let mark = self.heading_mark();
        let n = line.iter().take_while(|&&c| c == mark).count();
        (n > 0 && matches!(line.get(n), Some(' ' | '\n') | None)).then_some(n)
    }

    /// `heading_level` of a line given as a string.
    pub fn heading_level_of(self, line: &str) -> Option<usize> {
        let mark = self.heading_mark();
        let n = line.chars().take_while(|&c| c == mark).count();
        (n > 0 && matches!(line.chars().nth(n), Some(' ' | '\n') | None)).then_some(n)
    }

    /// The first line of a new page called `name`: its title as a headline.
    pub fn title_line(self, name: &str) -> String {
        format!("{} {name}\n", self.heading_mark())
    }
}

/// `name` without a page extension (`Note.md` → (`Note`, Markdown)), when it has one.
pub fn split_ext(name: &str) -> Option<(&str, Format)> {
    let (stem, ext) = name.rsplit_once('.')?;
    let format = Format::from_ext(ext)?;
    (!stem.trim().is_empty()).then_some((stem, format))
}

/// Which files are pages, and which one `[[Note]]` leads to when there are both
/// `Note.org` and `Note.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Formats {
    /// The format of new pages, preferred between two pages of the same name.
    pub default: Format,
    /// Only the pages of `default` are read; the others are left aside.
    pub single: bool,
}

impl Formats {
    pub fn accepts(self, format: Format) -> bool {
        !self.single || format == self.default
    }

    /// Whether the file `path` is a page.
    pub fn accepts_path(self, path: &str) -> bool {
        Format::of_path(path).is_some_and(|f| self.accepts(f))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_of_files() {
        assert_eq!(Format::of_path("/a/b/Note.md"), Some(Format::Markdown));
        assert_eq!(Format::of_path("C:\\x\\Note.ORG"), Some(Format::Org));
        assert_eq!(Format::of_path("/a/b.md/Note"), None);
        assert_eq!(Format::of_path("notes.txt"), None);
        assert_eq!(split_ext("Note.md"), Some(("Note", Format::Markdown)));
        assert_eq!(split_ext("v1.2"), None);
        assert_eq!(split_ext(".md"), None);
        let c = |s: &str| s.chars().collect::<Vec<char>>();
        assert_eq!(Format::Markdown.heading_level(&c("## Titre")), Some(2));
        assert_eq!(Format::Markdown.heading_level(&c("#tag")), None);
        assert_eq!(Format::Org.heading_level(&c("# commentaire")), None);
        assert_eq!(Format::Org.heading_level_of("*** x"), Some(3));
        assert_eq!(Format::Org.heading_level_of("*gras*"), None);
    }

    #[test]
    fn one_format_only() {
        let f = Formats { default: Format::Markdown, single: true };
        assert!(f.accepts_path("a.md") && !f.accepts_path("a.org"));
        let f = Formats { single: false, ..f };
        assert!(f.accepts_path("a.md") && f.accepts_path("a.org") && !f.accepts_path("a.txt"));
    }
}
