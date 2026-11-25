//! Which page a link names. A page is named by its file name without the
//! extension (`Note` for `Note.org` or `Note.md`); when both exist, `[[Note]]`
//! leads to the one of the default format, `[[Note.md]]` / `[[Note.org]]` to
//! each of them.

use std::{borrow::Cow, collections::HashMap};

use crate::{format::split_ext, names::page_key, FileEntry, Format};

/// The file name of a page path: `Note.md` for `/x/pages/Note.md`.
pub fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// True when a page holds no text: only spaces, tabs, line breaks, `*` or `#`.
pub fn is_blank(content: &str) -> bool {
    content.chars().all(|c| c.is_whitespace() || c == '*' || c == '#')
}

/// True when a page holds nothing but its title heading (`* Name` or `# Name`,
/// as a new page starts).
pub fn is_title_only(content: &str) -> bool {
    let mut lines = content.lines().filter(|l| !l.trim().is_empty());
    let Some(first) = lines.next() else { return false };
    let first = first.trim_start();
    let mark = if first.starts_with('#') { '#' } else { '*' };
    let stars = first.len() - first.trim_start_matches(mark).len();
    stars > 0
        && (first.len() == stars || first[stars..].starts_with([' ', '\t']))
        && lines.next().is_none()
}

/// Finds the page a link names, among the pages of the project (borrowed, or
/// owned to be kept: `Resolver<'static>`).
#[derive(Clone, PartialEq)]
pub struct Resolver<'a> {
    loose: bool,
    default: Format,
    files: Cow<'a, [FileEntry]>,
    /// Indexes in `files` of the pages, by the key of their name (without extension).
    by_name: HashMap<String, Vec<usize>>,
    /// Index in `files` of the page, by the key of each of its aliases (the first
    /// page giving it).
    by_alias: HashMap<String, usize>,
}

impl<'a> Resolver<'a> {
    /// `loose`: names compare ignoring case and accents.
    pub fn new(files: impl Into<Cow<'a, [FileEntry]>>, loose: bool, default: Format) -> Self {
        let files = files.into();
        let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
        let mut by_alias: HashMap<String, usize> = HashMap::new();
        for (i, f) in files.iter().enumerate() {
            by_name.entry(page_key(f.name.trim(), loose)).or_default().push(i);
            for a in &f.aliases { by_alias.entry(page_key(a.trim(), loose)).or_insert(i); }
        }
        Self { loose, default, files, by_name, by_alias }
    }

    fn pages(&self, key: &str) -> impl Iterator<Item = &FileEntry> {
        self.by_name.get(key).into_iter().flatten().map(|&i| &self.files[i])
    }

    /// The page `name` leads to: `Note.md` to that file when it exists; `Note` to
    /// the page of that name, the one of the default format when there are two;
    /// else to the page having `Note` as an alias.
    pub fn resolve(&self, name: &str) -> Option<&FileEntry> {
        self.resolve_name(name).or_else(|| self.by_alias.get(&page_key(name.trim(), self.loose)).map(|&i| &self.files[i]))
    }

    /// The page `name` leads to by the page names alone, not their aliases.
    pub fn resolve_name(&self, name: &str) -> Option<&FileEntry> {
        let name = name.trim();
        if let Some((stem, format)) = split_ext(name)
            && let Some(f) = self.pages(&page_key(stem.trim(), self.loose)).find(|f| Format::of_path(&f.path) == Some(format))
        {
            return Some(f);
        }
        let key = page_key(name, self.loose);
        self.pages(&key).find(|f| Format::of_path(&f.path) == Some(self.default)).or_else(|| self.pages(&key).next())
    }

    /// Whether `[[name]]` (its name alone) leads to another page than `file`,
    /// of the same name in the other format.
    pub fn shadowed(&self, file: &FileEntry) -> bool {
        self.resolve(&file.name).is_some_and(|f| f.path != file.path)
    }

    /// Whether another page has the name of `file`, in the other format.
    pub fn has_twin(&self, file: &FileEntry) -> bool {
        self.by_name.get(&page_key(file.name.trim(), self.loose)).is_some_and(|fs| fs.len() > 1)
    }

    /// The names links can give the pages, to complete them: each one's link
    /// name (see `link_name`), then its aliases.
    pub fn link_names(&self) -> Vec<String> {
        self.files.iter().flat_map(|f| std::iter::once(self.link_name(f)).chain(f.aliases.iter().cloned())).collect()
    }

    /// The name links give `file`: its name, or with its extension when its
    /// name alone leads to its twin.
    pub fn link_name(&self, file: &FileEntry) -> String {
        if self.shadowed(file) { file_name(&file.path).to_string() } else { file.name.clone() }
    }
}

/// A page of `files` picked with `r` (in `[0, 1)`), never the one at `shown`
/// while there is another; `None` without any page.
pub fn random_page<'f>(files: &'f [FileEntry], shown: Option<&str>, r: f64) -> Option<&'f FileEntry> {
    let others: Vec<&FileEntry> = files.iter().filter(|f| Some(f.path.as_str()) != shown).collect();
    let pool: Vec<&FileEntry> = if others.is_empty() { files.iter().collect() } else { others };
    let i = ((r.clamp(0.0, 1.0) * pool.len() as f64) as usize).min(pool.len().checked_sub(1)?);
    Some(pool[i])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_page_skips_the_page_shown() {
        let files = vec![entry("/p/A.org"), entry("/p/B.org")];
        for r in [0.0, 0.5, 0.999, 1.0] {
            assert_eq!(random_page(&files, Some("/p/A.org"), r).unwrap().path, "/p/B.org");
        }
        assert_eq!(random_page(&files, None, 0.0).unwrap().path, "/p/A.org");
        assert_eq!(random_page(&files[..1], Some("/p/A.org"), 0.3).unwrap().path, "/p/A.org");
        assert!(random_page(&[], None, 0.3).is_none());
    }

    fn entry(path: &str) -> FileEntry {
        let name = file_name(path).rsplit_once('.').unwrap().0.to_string();
        FileEntry { name, path: path.to_string(), ..Default::default() }
    }

    #[test]
    fn names_lead_to_pages() {
        let files = vec![entry("/p/Note.org"), entry("/p/Note.md"), entry("/p/Solo.md"), entry("/p/v1.2.org")];
        let r = Resolver::new(&files, true, Format::Org);
        assert_eq!(r.resolve("note").unwrap().path, "/p/Note.org");
        assert_eq!(r.resolve("Note.md").unwrap().path, "/p/Note.md");
        assert_eq!(r.resolve("Note.org").unwrap().path, "/p/Note.org");
        assert_eq!(r.resolve("solo").unwrap().path, "/p/Solo.md");
        assert_eq!(r.resolve("Solo.md").unwrap().path, "/p/Solo.md");
        // No such file: the name as it is
        assert!(r.resolve("Solo.org").is_none());
        assert_eq!(r.resolve("v1.2").unwrap().path, "/p/v1.2.org");
        assert!(r.resolve("Autre").is_none());

        assert!(r.shadowed(&files[1]) && !r.shadowed(&files[0]) && !r.shadowed(&files[2]));
        assert!(r.has_twin(&files[0]) && !r.has_twin(&files[2]));
        assert_eq!(r.link_name(&files[1]), "Note.md");
        assert_eq!(r.link_name(&files[0]), "Note");

        let r = Resolver::new(&files, false, Format::Markdown);
        assert_eq!(r.resolve("Note").unwrap().path, "/p/Note.md");
        assert!(r.resolve("note").is_none());
        assert_eq!(r.link_name(&files[0]), "Note.org");
    }

    #[test]
    fn aliases_lead_to_their_page_after_the_names() {
        let mut dragon = entry("/p/Dragon.org");
        dragon.aliases = vec!["Le dragon".into(), "Note".into()];
        let files = vec![dragon, entry("/p/Note.org")];
        let r = Resolver::new(&files, true, Format::Org);
        assert_eq!(r.resolve("le Dragon").unwrap().path, "/p/Dragon.org");
        assert!(r.resolve_name("Le dragon").is_none());
        // A page's own name wins over another's alias
        assert_eq!(r.resolve("Note").unwrap().path, "/p/Note.org");
    }
}
