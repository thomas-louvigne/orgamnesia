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

/// Finds the page a link names, among the pages of the project (borrowed, or
/// owned to be kept: `Resolver<'static>`).
#[derive(Clone, PartialEq)]
pub struct Resolver<'a> {
    loose: bool,
    default: Format,
    files: Cow<'a, [FileEntry]>,
    /// Indexes in `files` of the pages, by the key of their name (without extension).
    by_name: HashMap<String, Vec<usize>>,
}

impl<'a> Resolver<'a> {
    /// `loose`: names compare ignoring case and accents.
    pub fn new(files: impl Into<Cow<'a, [FileEntry]>>, loose: bool, default: Format) -> Self {
        let files = files.into();
        let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, f) in files.iter().enumerate() {
            by_name.entry(page_key(f.name.trim(), loose)).or_default().push(i);
        }
        Self { loose, default, files, by_name }
    }

    fn pages(&self, key: &str) -> impl Iterator<Item = &FileEntry> {
        self.by_name.get(key).into_iter().flatten().map(|&i| &self.files[i])
    }

    /// The page `name` leads to: `Note.md` to that file when it exists; `Note` to
    /// the page of that name, the one of the default format when there are two.
    pub fn resolve(&self, name: &str) -> Option<&FileEntry> {
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

    /// The name links give `file`: its name, or with its extension when its
    /// name alone leads to its twin.
    pub fn link_name(&self, file: &FileEntry) -> String {
        if self.shadowed(file) { file_name(&file.path).to_string() } else { file.name.clone() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str) -> FileEntry {
        let name = file_name(path).rsplit_once('.').unwrap().0.to_string();
        FileEntry { name, path: path.to_string() }
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
}
