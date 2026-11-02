//! Which page a link names. A page is named by its file name without the
//! extension (`Note` for `Note.org` or `Note.md`); when both exist, `[[Note]]`
//! leads to the one of the default format, `[[Note.md]]` / `[[Note.org]]` to
//! each of them.

use std::collections::HashMap;

use crate::{format::split_ext, names::page_key, FileEntry, Format};

/// The file name of a page path: `Note.md` for `/x/pages/Note.md`.
pub fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// Finds the page a link names, among the pages of the project.
pub struct Resolver<'a> {
    loose: bool,
    default: Format,
    /// Pages by the key of their name (without extension).
    by_name: HashMap<String, Vec<&'a FileEntry>>,
}

impl<'a> Resolver<'a> {
    /// `loose`: names compare ignoring case and accents.
    pub fn new(files: &'a [FileEntry], loose: bool, default: Format) -> Self {
        let mut by_name: HashMap<String, Vec<&FileEntry>> = HashMap::new();
        for f in files {
            by_name.entry(page_key(f.name.trim(), loose)).or_default().push(f);
        }
        Self { loose, default, by_name }
    }

    /// The page `name` leads to: `Note.md` to that file when it exists; `Note` to
    /// the page of that name, the one of the default format when there are two.
    pub fn resolve(&self, name: &str) -> Option<&'a FileEntry> {
        let name = name.trim();
        if let Some((stem, format)) = split_ext(name)
            && let Some(f) = self.by_name.get(&page_key(stem.trim(), self.loose))
                .and_then(|fs| fs.iter().find(|f| Format::of_path(&f.path) == Some(format)))
        {
            return Some(f);
        }
        let pages = self.by_name.get(&page_key(name, self.loose))?;
        pages.iter().find(|f| Format::of_path(&f.path) == Some(self.default)).or(pages.first()).copied()
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
