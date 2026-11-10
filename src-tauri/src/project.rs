//! The open project: its pages, kept in memory with the links between them,
//! and the state of the files as last seen on disk (to notice outside changes).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use orgamnesia_core::{
    format::split_ext, pages::Resolver, BrokenLink, FileEntry, Format, Formats, Graph, OrgTags, Prefs, TagCount, TagHit,
    TagSyntax, TodoHit, TodoKeywords, VaultChanges,
};

use crate::{error::AppError, tags};
use backlinks::BacklinkIndex;

pub mod backlinks;
pub mod files;
pub mod graph;
pub mod links;
mod search;

pub struct Project {
    /// The folder chosen as the project.
    pub path: String,
    /// Where its pages are, canonical (see `files::pages_dir`).
    pages_dir: PathBuf,
    /// Content of every page, by path.
    pages: HashMap<String, String>,
    /// The links of every page, by its path.
    links: BacklinkIndex,
    /// Which files are pages, and which one a name leads to.
    formats: Formats,
    snapshot: files::Snapshot,
    /// The pages of the last project replacement, for its undo.
    last_replace: Vec<search::Replaced>,
    /// The pages by the names links give them, with whether names ignore case,
    /// kept until a page comes or goes (see `resolver`).
    resolver: std::cell::RefCell<Option<(bool, Resolver<'static>)>>,
}

impl Project {
    /// Open the folder `path` (creating `pages/` and `assets/` if needed) and read its pages.
    pub fn open(path: &str, tags: TagSyntax, formats: Formats) -> Result<Self, AppError> {
        files::ensure_structure(path)?;
        let path = &crate::slash(path);
        let mut project = Self {
            path: path.to_string(),
            pages_dir: std::fs::canonicalize(files::pages_dir(path))?,
            pages: HashMap::new(),
            links: BacklinkIndex::new(),
            formats,
            snapshot: files::Snapshot::new(),
            last_replace: Vec::new(),
            resolver: Default::default(),
        };
        let files = files::list_pages(path, formats)?;
        for f in &files {
            let content = std::fs::read_to_string(&f.path).unwrap_or_default();
            project.store(&f.path, content, tags);
        }
        project.snapshot = files::snapshot(&files);
        Ok(project)
    }

    /// The formats now in effect: the pages of a format no longer read go at the
    /// next `refresh`, those of a format read again come back.
    pub fn set_formats(&mut self, formats: Formats) {
        self.formats = formats;
        self.resolver.take();
    }

    /// Finds the page a link names (names ignoring case with `ignore_case`); built
    /// again only when the pages or this setting change.
    fn resolver(&self, ignore_case: bool) -> std::cell::Ref<'_, Resolver<'static>> {
        if self.resolver.borrow().as_ref().is_none_or(|(ic, _)| *ic != ignore_case) {
            let r = Resolver::new(self.files(), ignore_case, self.formats.default);
            *self.resolver.borrow_mut() = Some((ignore_case, r));
        }
        std::cell::Ref::map(self.resolver.borrow(), |r| &r.as_ref().expect("just built").1)
    }

    /// The pages, sorted by name.
    pub fn files(&self) -> Vec<FileEntry> {
        let mut files: Vec<FileEntry> = self.pages.keys()
            .map(|p| FileEntry { name: files::page_name(p), path: p.clone() })
            .collect();
        files.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.path.cmp(&b.path)));
        files
    }

    /// Ok when `path` is a page of the project: a `.org` or `.md` file (of a format
    /// read) right in its pages folder. Every path received from the interface goes through here.
    pub fn check_page(&self, path: &str) -> Result<(), AppError> {
        let p = Path::new(path);
        let folder = p.parent().and_then(|d| std::fs::canonicalize(d).ok());
        if self.formats.accepts_path(path) && folder.as_deref() == Some(self.pages_dir.as_path()) {
            Ok(())
        } else {
            Err(AppError::NotAPage(path.to_string()))
        }
    }

    pub fn read(&self, path: &str) -> Result<String, AppError> {
        self.check_page(path)?;
        Ok(std::fs::read_to_string(path)?)
    }

    pub fn write(&mut self, path: &str, content: String, tags: TagSyntax) -> Result<(), AppError> {
        self.check_page(path)?;
        crate::write_atomic(path, &content)?;
        self.store(path, content, tags);
        self.remember(path);
        Ok(())
    }

    /// Create the page `name`, holding its title heading, in `format` (else the
    /// default one); `Note.md` is the page `Note` in Markdown.
    pub fn create(&mut self, name: &str, format: Option<Format>, tags: TagSyntax) -> Result<FileEntry, AppError> {
        let name = valid_name(name)?;
        let (name, format) = match split_ext(&name) {
            Some((stem, f)) if self.formats.accepts(f) => (valid_name(stem)?, f),
            _ => (name, format.filter(|f| self.formats.accepts(*f)).unwrap_or(self.formats.default)),
        };
        let path = files::page_path(&self.path, &name, format);
        if path.exists() {
            return Err(AppError::PageExists(name));
        }
        let content = format.title_line(&name);
        crate::write_atomic(&path, &content)?;
        let path = crate::slash(&path);
        self.store(&path, content, tags);
        self.remember(&path);
        Ok(FileEntry { name, path })
    }

    /// Copy the image `source` into the project's `assets/` folder, named as Logseq
    /// names them; returns the link to it from the page `page`.
    pub fn import_image(&self, page: &str, source: &str) -> Result<String, AppError> {
        let ext = Path::new(source).extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
        if !ext.is_some_and(|e| orgamnesia_core::assets::EXTENSIONS.contains(&e.as_str())) {
            return Err(AppError::NotAnImage(source.to_string()));
        }
        self.import_file(page, source)
    }

    /// Copy the file `source`, whatever it is, into the project's `assets/`, named
    /// as Logseq names them; returns the link to it from the page `page`.
    pub fn import_file(&self, page: &str, source: &str) -> Result<String, AppError> {
        self.check_page(page)?;
        let src = Path::new(source);
        let name = src.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !src.is_file() {
            return Err(AppError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, format!("not a file: {source}"))));
        }
        let dir = Path::new(&self.path).join("assets");
        std::fs::create_dir_all(&dir)?;
        let millis = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
        let file = orgamnesia_core::assets::asset_name(name, millis);
        std::fs::copy(src, dir.join(&file))?;
        // The pages are in `pages/` (Logseq), or right in the project's folder
        let up = std::fs::canonicalize(&self.path)? != self.pages_dir;
        Ok(format!("{}assets/{file}", if up { "../" } else { "./" }))
    }

    /// Delete a page: it goes to the project's trash (`.trash/`, named
    /// `<time>-<page>.org` or `.md`), from where `restore` brings it back. Returns its path there.
    pub fn delete(&mut self, path: &str) -> Result<String, AppError> {
        self.check_page(path)?;
        let to = files::to_trash(&self.path, path)?;
        self.forget(path);
        Ok(crate::slash(&to))
    }

    /// Bring back a page `delete` put in the trash, under its name.
    pub fn restore(&mut self, trashed: &str, tags: TagSyntax) -> Result<FileEntry, AppError> {
        let p = Path::new(trashed);
        let in_trash = p.parent().and_then(|d| std::fs::canonicalize(d).ok())
            == std::fs::canonicalize(self.trash_dir()?).ok();
        let name = p.file_stem().and_then(|s| s.to_str()).and_then(|s| s.split_once('-')).map(|(_, n)| n);
        let format = Format::of_path(trashed).filter(|f| self.formats.accepts(*f));
        let (Some(name), true, Some(format)) = (name, in_trash, format) else {
            return Err(AppError::NotAPage(trashed.to_string()));
        };
        let path = files::page_path(&self.path, name, format);
        if path.exists() {
            return Err(AppError::PageExists(name.to_string()));
        }
        std::fs::rename(p, &path)?;
        let path = crate::slash(&path);
        self.store(&path, std::fs::read_to_string(&path).unwrap_or_default(), tags);
        self.remember(&path);
        Ok(FileEntry { name: name.to_string(), path })
    }

    /// Number of pages in the trash.
    pub fn trash_count(&self) -> Result<usize, AppError> {
        Ok(self.trashed()?.len())
    }

    /// Delete for good every page in the trash. Returns how many went.
    pub fn empty_trash(&self) -> Result<usize, AppError> {
        self.prune_trash(Some(0), 0)
    }

    /// Delete for good the pages of the trash deleted more than `keep_days` days
    /// ago (`Some(0)`: all of them; `None`: no limit), then the oldest ones while
    /// the trash holds more than `max_bytes` bytes (0: no limit). Returns how many went.
    pub fn prune_trash(&self, keep_days: Option<u64>, max_bytes: u64) -> Result<usize, AppError> {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
        let limit = keep_days.map(|d| now.saturating_sub(u128::from(d) * 86_400_000));
        let mut pages = self.trashed()?;
        // Oldest first; a page with no time in its name counts as just deleted
        pages.sort_by_key(|p| p.deleted_at.unwrap_or(now));
        let mut size: u64 = pages.iter().map(|p| p.size).sum();
        let mut gone = 0;
        for p in pages {
            let too_old = limit.is_some_and(|l| p.deleted_at.unwrap_or(now) <= l);
            let too_big = max_bytes > 0 && size > max_bytes;
            if !too_old && !too_big { continue; }
            std::fs::remove_file(&p.path)?;
            size -= p.size;
            gone += 1;
        }
        Ok(gone)
    }

    /// The pages in the trash.
    fn trashed(&self) -> Result<Vec<Trashed>, AppError> {
        Ok(std::fs::read_dir(self.trash_dir()?)?
            .filter_map(|e| e.ok())
            .filter(|e| Format::of_path(&e.path().to_string_lossy()).is_some())
            .map(|e| {
                let path = e.path();
                let deleted_at = path.file_stem().and_then(|s| s.to_str()).and_then(|s| s.split_once('-'))
                    .and_then(|(t, _)| t.parse().ok());
                Trashed { size: e.metadata().map_or(0, |m| m.len()), path, deleted_at }
            })
            .collect())
    }

    /// The trash folder of the project (see `files::trash_dir`).
    fn trash_dir(&self) -> Result<PathBuf, AppError> {
        Ok(files::trash_dir(&self.path)?)
    }

    /// Rename a page; when the settings say so, the links to it in every page follow.
    pub fn rename(&mut self, old_path: &str, new_name: &str, prefs: &Prefs) -> Result<FileEntry, AppError> {
        self.check_page(old_path)?;
        let new_name = valid_name(new_name)?;
        let ext = Format::of_page(old_path).ext();
        let new_path = self.pages_dir.join(format!("{new_name}.{ext}"));
        let old_name = files::page_name(old_path);
        if new_name != old_name && taken(&new_path, &old_name, &new_name) {
            return Err(AppError::PageExists(new_name));
        }
        // The links leading to the page, as they did before
        let files = self.files();
        let resolver = Resolver::new(&files, prefs.case_insensitive_links, self.formats.default);
        let leads_here = |target: &str| resolver.resolve(target).is_some_and(|f| f.path == old_path);
        let by_name = leads_here(&old_name);
        // Keep the folder as the interface knows it (not the canonical one)
        let new_path = Path::new(old_path).with_file_name(format!("{new_name}.{ext}"));
        std::fs::rename(old_path, &new_path)?;
        let new_path = crate::slash(&new_path);
        let tags = prefs.tags();

        self.forget(old_path);
        self.store(&new_path, std::fs::read_to_string(&new_path).unwrap_or_default(), tags);
        self.remember(&new_path);

        if prefs.update_links && old_name != new_name {
            let paths: Vec<String> = self.pages.keys().cloned().collect();
            for path in paths {
                let Ok(text) = std::fs::read_to_string(&path) else { continue };
                let rename = links::Rename { leads_here: &leads_here, old: &old_name, by_name, new: &new_name };
                let updated = links::rewrite_links(&text, &rename, prefs.case_insensitive_links, tags, Format::of_page(&path));
                if let Some(updated) = updated {
                    crate::write_atomic(&path, &updated)?;
                    self.store(&path, updated, tags);
                    self.remember(&path);
                }
            }
        }
        Ok(FileEntry { name: new_name, path: new_path })
    }

    /// Pick up the pages created, modified or deleted outside the app since the
    /// last look. `None` when nothing changed.
    pub fn refresh(&mut self, tags: TagSyntax) -> Result<Option<VaultChanges>, AppError> {
        let files = files::list_pages(&self.path, self.formats)?;
        let now = files::snapshot(&files);
        let (changed, removed) = files::diff(&self.snapshot, &now);
        if changed.is_empty() && removed.is_empty() {
            return Ok(None);
        }
        for path in &removed {
            self.forget(path);
        }
        for path in &changed {
            let content = std::fs::read_to_string(path).unwrap_or_default();
            self.store(path, content, tags);
        }
        self.snapshot = now;
        Ok(Some(VaultChanges { files, changed, removed }))
    }

    /// Read the links again (after `#tags` started being read differently).
    pub fn reindex(&mut self, tags: TagSyntax) {
        self.links = BacklinkIndex::new();
        for (path, content) in &self.pages {
            self.links.index_file(path, &links::extract_links(content, tags, Format::of_page(path)));
        }
    }

    /// The names links give the pages `paths` (see `Resolver::link_name`), sorted.
    fn link_names(&self, resolver: &Resolver, paths: impl IntoIterator<Item = String>) -> Vec<String> {
        let mut names: Vec<String> = paths.into_iter()
            .map(|p| FileEntry { name: files::page_name(&p), path: p })
            .map(|f| resolver.link_name(&f))
            .collect();
        names.sort();
        names.dedup();
        names
    }

    /// Pages linking to the page `path`, by the names links give them.
    pub fn backlinks(&self, path: &str, ignore_case: bool) -> Vec<String> {
        let resolver = self.resolver(ignore_case);
        let sources = self.links.sources_where(|target| resolver.resolve(target).is_some_and(|f| f.path == path));
        self.link_names(&resolver, sources)
    }

    /// Every page, with the links between pages that exist.
    pub fn graph(&self, ignore_case: bool) -> Graph {
        let resolver = self.resolver(ignore_case);
        let pages = self.files();
        let index: HashMap<&str, usize> = pages.iter().enumerate().map(|(i, f)| (f.path.as_str(), i)).collect();
        let mut links = std::collections::BTreeSet::new();
        for (target, sources, _) in self.links.all_links() {
            let Some(to) = resolver.resolve(&target).and_then(|f| index.get(f.path.as_str()).copied()) else { continue };
            for from in sources.iter().filter_map(|s| index.get(s.as_str()).copied()) {
                if from != to { links.insert((from, to)); }
            }
        }
        Graph { links: links.into_iter().collect(), pages, pos: vec![] }
    }

    /// Links to pages that don't exist, most used first.
    pub fn broken_links(&self, ignore_case: bool) -> Vec<BrokenLink> {
        let norm = |s: &str| orgamnesia_core::names::page_key(s.trim(), ignore_case);
        let resolver = self.resolver(ignore_case);

        // Group targets that only differ by case or accents when they are ignored
        let mut grouped: Vec<(String, BrokenLink)> = Vec::new();
        for (target, sources, count) in self.links.all_links() {
            let key = norm(&target);
            if !orgamnesia_core::links::is_page_target(&target) || resolver.resolve(&target).is_some() {
                continue;
            }
            let sources = self.link_names(&resolver, sources);
            match grouped.iter_mut().find(|(k, _)| *k == key) {
                Some((_, b)) => {
                    b.count += count;
                    for s in sources {
                        if !b.sources.contains(&s) { b.sources.push(s); }
                    }
                    b.sources.sort();
                }
                None => grouped.push((key, BrokenLink { target: target.trim().to_string(), sources, count })),
            }
        }
        let mut links: Vec<BrokenLink> = grouped.into_iter().map(|(_, b)| b).collect();
        links.sort_by(|a, b| {
            b.count.cmp(&a.count).then_with(|| a.target.to_lowercase().cmp(&b.target.to_lowercase()))
        });
        links
    }

    /// Every `:tag:` and `#+FILETAGS:` tag, with the number of pages and headlines carrying it.
    pub fn tags(&self, org: OrgTags) -> Vec<TagCount> {
        let mut counts = HashMap::<String, usize>::new();
        for (path, content) in &self.pages {
            for t in tags::written_tags(content, org, Format::of_page(path)) { *counts.entry(t).or_default() += 1; }
        }
        let mut list: Vec<TagCount> = counts.into_iter().map(|(name, count)| TagCount { name, count }).collect();
        list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then_with(|| a.name.cmp(&b.name)));
        list
    }

    /// Pages and headlines matching an org-mode tag search (`projet+urgent-perso|idée`).
    pub fn search_tags(&self, query: &str, org: OrgTags) -> Vec<TagHit> {
        let Some(query) = tags::Query::parse(query, org) else { return vec![] };
        self.files().into_iter().flat_map(|f| {
            let content = self.pages.get(&f.path).map(String::as_str).unwrap_or("");
            tags::search(content, &query, org, Format::of_page(&f.path)).into_iter().map(move |h| TagHit {
                page: f.name.clone(),
                path: f.path.clone(),
                heading: h.heading,
                level: h.level,
                line: h.line,
                tags: h.tags,
            }).collect::<Vec<_>>()
        }).collect()
    }

    /// Every headline of the project with a TODO keyword (`keywords`: those of the
    /// settings, which a page's `#+TODO:` lines replace), by page then line.
    pub fn todos(&self, keywords: &TodoKeywords) -> Vec<TodoHit> {
        self.files().into_iter().flat_map(|f| {
            let content = self.pages.get(&f.path).map(String::as_str).unwrap_or("");
            let format = Format::of_page(&f.path);
            orgamnesia_core::todo::headlines(content, keywords, format).into_iter().map(move |h| TodoHit {
                page: f.name.clone(),
                path: f.path.clone(),
                keyword: h.keyword,
                done: h.done,
                title: h.title,
                level: h.level,
                line: h.line,
            }).collect::<Vec<_>>()
        }).collect()
    }

    fn store(&mut self, path: &str, content: String, tags: TagSyntax) {
        self.links.index_file(path, &links::extract_links(&content, tags, Format::of_page(path)));
        if self.pages.insert(path.to_string(), content).is_none() { self.resolver.take(); }
    }

    fn forget(&mut self, path: &str) {
        self.links.remove_source(path);
        if self.pages.remove(path).is_some() { self.resolver.take(); }
        self.snapshot.remove(path);
    }

    /// Record the stamp of a page written by the app, so `refresh` doesn't
    /// report it as changed outside.
    fn remember(&mut self, path: &str) {
        if let Some(s) = files::stamp(path) {
            self.snapshot.insert(path.to_string(), s);
        }
    }
}

/// A page in the trash.
struct Trashed {
    path: PathBuf,
    /// When it was deleted (ms since 1970), from its name.
    deleted_at: Option<u128>,
    size: u64,
}

/// Whether renaming the page `old` to `new` would land on another page at
/// `new_path`. Only the case changing (`note` → `Note`), the page itself is found
/// there on a disk ignoring case (macOS, Windows): not another one, unless a
/// file of exactly that name is there too (a disk minding case).
fn taken(new_path: &Path, old: &str, new: &str) -> bool {
    if !new_path.exists() { return false; }
    if old.to_lowercase() != new.to_lowercase() { return true; }
    let exact = new_path.file_name();
    new_path.parent().and_then(|d| std::fs::read_dir(d).ok())
        .is_some_and(|mut entries| entries.any(|e| e.is_ok_and(|e| Some(e.file_name().as_os_str()) == exact)))
}

/// `name` trimmed, if it can be a page name (and so a file name, on every system:
/// Windows takes none of `: * ? " < > |`, and reads `a:b` as a hidden stream of `a`).
fn valid_name(name: &str) -> Result<String, AppError> {
    let name = name.trim();
    if name.is_empty() || name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) || name.starts_with('.') {
        return Err(AppError::InvalidName(name.to_string()));
    }
    Ok(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use orgamnesia_core::Hashtags;

    /// A fresh project folder holding the given pages.
    pub(super) fn project(test: &str, pages: &[(&str, &str)]) -> (PathBuf, Project) {
        let dir = crate::commands::testing::folder(&format!("project-{test}"));
        std::fs::create_dir_all(dir.join("pages")).unwrap();
        for (name, content) in pages {
            std::fs::write(dir.join("pages").join(format!("{name}.org")), content).unwrap();
        }
        let p = Project::open(dir.to_str().unwrap(), Hashtags::Dashes.into(), Formats::default()).unwrap();
        (dir, p)
    }

    #[test]
    fn images_are_copied_to_the_assets() {
        let (dir, p) = project("images", &[("a", "* a\n")]);
        let src = dir.join("Ma photo.PNG");
        std::fs::write(&src, b"png").unwrap();
        let link = p.import_image(&path_of(&p, "a"), src.to_str().unwrap()).unwrap();
        let file = link.strip_prefix("../assets/").unwrap();
        assert!(file.starts_with("Ma_photo_") && file.ends_with("_0.png"));
        assert_eq!(std::fs::read(dir.join("assets").join(file)).unwrap(), b"png");
        let text = dir.join("notes.txt");
        std::fs::write(&text, b"x").unwrap();
        assert!(matches!(p.import_image(&path_of(&p, "a"), text.to_str().unwrap()), Err(AppError::NotAnImage(_))));
        let _ = std::fs::remove_dir_all(dir);
    }

    pub(super) fn path_of(p: &Project, name: &str) -> String {
        p.files().into_iter().find(|f| f.name == name).unwrap().path
    }

    #[test]
    fn refuses_paths_outside_the_pages() {
        let (dir, p) = project("outside", &[("a", "* a\n")]);
        assert!(p.read(&path_of(&p, "a")).is_ok());
        for bad in ["/etc/passwd", "/tmp/x.org", &format!("{}/pages/../secret.org", dir.display())] {
            assert!(matches!(p.read(bad), Err(AppError::NotAPage(_))), "{bad}");
        }
        assert!(matches!(valid_name("../x"), Err(AppError::InvalidName(_))));
        assert!(matches!(valid_name("  "), Err(AppError::InvalidName(_))));
        for bad in ["Réunion: 12h", "a?", "a*b", "a|b", "\"a\"", "<a>"] {
            assert!(matches!(valid_name(bad), Err(AppError::InvalidName(_))), "{bad}");
        }
        assert_eq!(valid_name(" Réunion de 12h ").unwrap(), "Réunion de 12h");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn links_follow_writes_creations_and_deletions() {
        let (dir, mut p) = project("links", &[("a", "voir [[b]] et [[c]]\n")]);
        assert_eq!(p.broken_links(false).len(), 2);

        p.create("b", None, Hashtags::Dashes.into()).unwrap();
        assert_eq!(p.backlinks(&path_of(&p, "b"), false), vec!["a"]);
        // a → b only: c doesn't exist
        let g = p.graph(false);
        assert_eq!(g.pages.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), vec!["a", "b"]);
        assert_eq!(g.links, vec![(0, 1)]);
        assert_eq!(p.broken_links(false).iter().map(|b| b.target.as_str()).collect::<Vec<_>>(), vec!["c"]);
        assert!(matches!(p.create("b", None, Hashtags::Dashes.into()), Err(AppError::PageExists(_))));

        p.write(&path_of(&p, "a"), "plus rien\n".into(), Hashtags::Dashes.into()).unwrap();
        assert!(p.backlinks(&path_of(&p, "b"), false).is_empty());

        p.delete(&path_of(&p, "b")).unwrap();
        assert_eq!(p.files().len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn deleted_pages_go_to_the_trash_and_come_back() {
        let (dir, mut p) = project("trash", &[("a", "* a\n"), ("b", "[[a]]\n")]);
        let trashed = p.delete(&path_of(&p, "a")).unwrap();
        assert_eq!(p.files().len(), 1);
        assert!(Path::new(&trashed).exists() && trashed.ends_with("-a.org"));
        assert!(dir.join(".trash/.gitignore").exists());
        assert_eq!(p.broken_links(false).len(), 1);

        let back = p.restore(&trashed, Hashtags::Dashes.into()).unwrap();
        assert_eq!(back.name, "a");
        assert_eq!(p.read(&back.path).unwrap(), "* a\n");
        assert!(p.broken_links(false).is_empty());
        // Only from the trash, and not over a page of the same name
        assert!(matches!(p.restore(&back.path, Hashtags::Dashes.into()), Err(AppError::NotAPage(_))));
        let again = p.delete(&back.path).unwrap();
        p.create("a", None, Hashtags::Dashes.into()).unwrap();
        assert!(matches!(p.restore(&again, Hashtags::Dashes.into()), Err(AppError::PageExists(_))));

        // Emptying: the old ones, then the oldest beyond the size, then all
        std::fs::write(dir.join(".trash/1000-vieux.org"), "").unwrap();
        std::fs::write(dir.join(".trash/2000-moins-vieux.org"), "x".repeat(100)).unwrap();
        std::fs::write(dir.join(".trash/3000-recent.org"), "x".repeat(100)).unwrap();
        let recent = format!("{}-neuf.org", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis());
        std::fs::write(dir.join(".trash").join(recent), "x".repeat(10)).unwrap();
        assert_eq!(p.trash_count().unwrap(), 5);
        // No limit at all: nothing goes
        assert_eq!(p.prune_trash(None, 0).unwrap(), 0);
        // Over 150 bytes (214 here): the oldest go until it fits, whatever their size
        assert_eq!(p.prune_trash(None, 150).unwrap(), 2);
        assert!(dir.join(".trash/3000-recent.org").exists() && !dir.join(".trash/2000-moins-vieux.org").exists());
        assert_eq!(p.prune_trash(Some(30), 0).unwrap(), 1);
        assert_eq!(p.empty_trash().unwrap(), 2);
        assert_eq!(p.trash_count().unwrap(), 0);
        assert!(dir.join(".trash/.gitignore").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn lists_the_todos_of_every_page() {
        let (dir, p) = project("todos", &[("a", "* TODO un\n** DONE deux\n"), ("b", "#+TODO: A | B\n* A trois\n* TODO pas ici\n")]);
        std::fs::write(dir.join("pages/c.md"), "# TODO quatre\n- [x] cinq\n").unwrap();
        let mut p = p;
        p.refresh(Hashtags::Dashes.into()).unwrap();
        let todos = p.todos(&TodoKeywords::parse("TODO | DONE"));
        let seen: Vec<(&str, &str, bool, usize)> = todos.iter().map(|t| (t.page.as_str(), t.title.as_str(), t.done, t.line)).collect();
        assert_eq!(seen, vec![("a", "un", false, 0), ("a", "deux", true, 1), ("b", "trois", false, 1), ("c", "quatre", false, 0), ("c", "cinq", true, 1)]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rename_rewrites_links() {
        let (dir, mut p) = project("rename", &[("old", "* old\n"), ("other", "lien [[old]] #old\n")]);
        let nf = p.rename(&path_of(&p, "old"), "new", &Prefs::default()).unwrap();
        assert_eq!(nf.name, "new");
        assert_eq!(p.read(&path_of(&p, "other")).unwrap(), "lien [[new]] #new\n");
        assert_eq!(p.backlinks(&path_of(&p, "new"), false), vec!["other"]);
        assert!(p.broken_links(false).is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rename_changing_only_the_case() {
        let (dir, mut p) = project("rename-case", &[("note", "* note\n")]);
        let nf = p.rename(&path_of(&p, "note"), "Note", &Prefs::default()).unwrap();
        assert!(nf.path.ends_with("/Note.org") && Path::new(&nf.path).exists());
        // A disk minding case (Linux): another page of that very name is left alone
        if cfg!(target_os = "linux") {
            std::fs::write(dir.join("pages/a.org"), "* a\n").unwrap();
            std::fs::write(dir.join("pages/A.org"), "* A\n").unwrap();
            p.refresh(Hashtags::Dashes.into()).unwrap();
            assert!(matches!(p.rename(&file_named(&p, "a"), "A", &Prefs::default()), Err(AppError::PageExists(_))));
            assert_eq!(std::fs::read_to_string(dir.join("pages/A.org")).unwrap(), "* A\n");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn file_named(p: &Project, name: &str) -> String {
        p.files().into_iter().find(|f| f.path.ends_with(&format!("/{name}.org"))).unwrap().path
    }

    /// A project holding the given files (`Note.md`, `Note.org`…).
    fn mixed(test: &str, files: &[(&str, &str)], formats: Formats) -> (PathBuf, Project) {
        let (dir, _) = project(test, &[]);
        for (file, content) in files {
            std::fs::write(dir.join("pages").join(file), content).unwrap();
        }
        let p = Project::open(dir.to_str().unwrap(), Hashtags::Dashes.into(), formats).unwrap();
        (dir, p)
    }

    fn file(p: &Project, file: &str) -> String {
        p.files().into_iter().find(|f| f.path.ends_with(&format!("/{file}"))).unwrap().path
    }

    #[test]
    fn markdown_pages_and_twins() {
        let (dir, mut p) = mixed("markdown", &[
            ("Note.org", "* Note\n"),
            ("Note.md", "# Note\n[lien](Solo.md) [[Absente|x]]\n"),
            ("Solo.md", "# Solo\n[[Note]] [[Note.md#Titre]]\n"),
            ("Org.org", "[[Solo|pas un lien étendu]] [[Note.md]]\n"),
        ], Formats::default());
        assert_eq!(p.files().len(), 4);
        // `[[Note]]` leads to the org one (the default format)
        assert_eq!(p.backlinks(&file(&p, "Note.org"), false), vec!["Solo"]);
        assert_eq!(p.backlinks(&file(&p, "Note.md"), false), vec!["Org", "Solo"]);
        // The Markdown twin is named with its extension
        assert_eq!(p.backlinks(&file(&p, "Solo.md"), false), vec!["Note.md"]);
        let broken: Vec<String> = p.broken_links(false).into_iter().map(|b| b.target).collect();
        assert_eq!(broken, vec!["Absente", "Solo|pas un lien étendu"]);

        // Renaming the Markdown twin: only the links leading to it follow
        let moved = p.rename(&file(&p, "Note.md"), "Carnet", &Prefs::default()).unwrap();
        assert!(moved.path.ends_with("/Carnet.md"));
        assert_eq!(p.read(&file(&p, "Solo.md")).unwrap(), "# Solo\n[[Note]] [[Carnet.md#Titre]]\n");
        assert_eq!(p.read(&file(&p, "Org.org")).unwrap(), "[[Solo|pas un lien étendu]] [[Carnet.md]]\n");

        // Created in the format asked, its name telling it, or the default one
        let md = p.create("Neuve.md", None, Hashtags::Dashes.into()).unwrap();
        assert_eq!((md.name.as_str(), p.read(&md.path).unwrap().as_str()), ("Neuve", "# Neuve\n"));
        assert!(p.create("Neuve", Some(Format::Markdown), Hashtags::Dashes.into()).is_err());
        assert!(p.create("Neuve", None, Hashtags::Dashes.into()).unwrap().path.ends_with("/Neuve.org"));

        // The trash keeps the format
        let trashed = p.delete(&md.path).unwrap();
        assert!(trashed.ends_with("-Neuve.md"));
        assert!(p.restore(&trashed, Hashtags::Dashes.into()).unwrap().path.ends_with("/Neuve.md"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn one_format_only_leaves_the_others_aside() {
        let md = Formats { default: Format::Markdown, single: true };
        let (dir, mut p) = mixed("single", &[("A.org", "* A\n"), ("B.md", "[[A]]\n")], md);
        assert_eq!(p.files().iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), vec!["B"]);
        assert!(p.read(&format!("{}/pages/A.org", dir.display())).is_err());
        assert_eq!(p.broken_links(false).len(), 1);
        // Both read again: A comes back at the next look
        p.set_formats(Formats::default());
        let changes = p.refresh(Hashtags::Dashes.into()).unwrap().unwrap();
        assert_eq!(changes.files.len(), 2);
        assert!(p.broken_links(false).is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn refresh_sees_outside_changes_only() {
        let (dir, mut p) = project("refresh", &[("a", "* a\n")]);
        p.write(&path_of(&p, "a"), "* a\n[[b]]\n".into(), Hashtags::Dashes.into()).unwrap();
        assert!(p.refresh(Hashtags::Dashes.into()).unwrap().is_none());

        std::fs::write(dir.join("pages/n.org"), "* n :projet:\n").unwrap();
        let changes = p.refresh(Hashtags::Dashes.into()).unwrap().unwrap();
        assert_eq!(changes.changed.len(), 1);
        assert_eq!(p.tags(OrgTags::default()), vec![TagCount { name: "projet".into(), count: 1 }]);
        assert_eq!(p.search_tags("projet", OrgTags::default()).len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
