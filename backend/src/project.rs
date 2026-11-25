//! The open project: its pages, kept in memory with the links between them,
//! and the state of the files as last seen on disk (to notice outside changes).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use orgamnesia_core::{
    format::split_ext, pages::Resolver, FileEntry, Format, Formats, TagSyntax, VaultChanges,
};

use crate::error::AppError;
use backlinks::BacklinkIndex;

pub mod backlinks;
pub mod files;
pub mod graph;
pub mod links;
mod queries;
mod rename;
mod search;
mod trash;

pub struct Project {
    /// The folder chosen as the project.
    pub path: String,
    /// Where its pages are, canonical (see `files::pages_dir`).
    pages_dir: PathBuf,
    /// Content of every page, by path.
    pages: HashMap<String, String>,
    /// The links of every page, by its path.
    links: BacklinkIndex,
    /// The aliases of the pages having some, by path (see `orgamnesia_core::aliases`).
    aliases: HashMap<String, Vec<String>>,
    /// Which files are pages, and which one a name leads to.
    formats: Formats,
    /// The journal folder from the settings (see `files::journal_dir`).
    journal: String,
    /// The templates folder from the settings (see `files::templates_dir`).
    templates: String,
    snapshot: files::Snapshot,
    /// The pages of the last project replacement, for its undo.
    last_replace: Vec<search::Replaced>,
    /// The pages by the names links give them, with whether names ignore case,
    /// kept until a page comes or goes (see `resolver`).
    resolver: std::cell::RefCell<Option<(bool, Resolver<'static>)>>,
}

impl Project {
    /// Open the folder `path` (creating `pages/` and `assets/` if needed) and read its pages.
    pub fn open(path: &str, tags: TagSyntax, formats: Formats, journal: &str, templates: &str) -> Result<Self, AppError> {
        files::ensure_structure(path)?;
        let path = &crate::slash(path);
        let mut project = Self {
            path: path.to_string(),
            pages_dir: std::fs::canonicalize(files::pages_dir(path))?,
            pages: HashMap::new(),
            links: BacklinkIndex::new(),
            aliases: HashMap::new(),
            formats,
            journal: journal.to_string(),
            templates: templates.to_string(),
            snapshot: files::Snapshot::new(),
            last_replace: Vec::new(),
            resolver: Default::default(),
        };
        let files = files::list_pages(path, formats, journal, templates)?;
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

    /// The templates folder now in effect (see `Prefs::templates_folder`).
    pub fn set_templates(&mut self, folder: &str) {
        self.templates = folder.to_string();
    }

    /// The journal folder now in effect: its pages come at the next `refresh`.
    pub fn set_journal(&mut self, journal: &str) {
        self.journal = journal.to_string();
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

    /// The name of the page `path`, as links name it (see `files::name_in`).
    pub fn name_of(&self, path: &str) -> String {
        files::name_in(&self.path, &self.journal, path)
    }

    /// The folders of the pages, by their path from the pages folder (see `files::list_folders`).
    pub fn folders(&self) -> Vec<String> {
        files::list_folders(&self.path, &self.journal, &self.templates)
    }

    /// The pages, sorted by name.
    pub fn files(&self) -> Vec<FileEntry> {
        let mut files: Vec<FileEntry> = self.pages.keys()
            .map(|p| FileEntry { name: self.name_of(p), path: p.clone(), aliases: self.aliases.get(p).cloned().unwrap_or_default() })
            .collect();
        files.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.path.cmp(&b.path)));
        files
    }

    /// Ok when `path` is a page of the project: a `.org` or `.md` file (of a format
    /// read) in its pages folder or one of its folders (not a hidden one), or in its
    /// journal folder. Every path received from the interface goes through here.
    pub fn check_page(&self, path: &str) -> Result<(), AppError> {
        let p = Path::new(path);
        let folder = p.parent().and_then(|d| std::fs::canonicalize(d).ok());
        let journal = std::fs::canonicalize(files::journal_dir(&self.path, &self.journal)).ok();
        let in_pages = folder.as_deref().and_then(|f| f.strip_prefix(&self.pages_dir).ok())
            .is_some_and(|rel| rel.components().all(|c| !c.as_os_str().to_string_lossy().starts_with('.')));
        let in_folder = folder.is_some() && (in_pages || folder == journal) || self.is_template(path);
        if self.formats.accepts_path(path) && in_folder {
            Ok(())
        } else {
            Err(AppError::NotAPage(path.to_string()))
        }
    }

    pub fn read(&self, path: &str) -> Result<String, AppError> {
        self.check_page(path)?;
        Ok(std::fs::read_to_string(path)?)
    }

    /// Write the page `path`; refused when it changed on disk since the app last
    /// read or wrote it (a sync tool, another editor): the interface then asks
    /// which text to keep, once `refresh` has seen the change.
    pub fn write(&mut self, path: &str, content: String, tags: TagSyntax) -> Result<(), AppError> {
        self.check_page(path)?;
        // shortcut: a change `refresh` took in just before this write still goes through; closing
        // that too means the interface sending the stamp its text was read at.
        if self.snapshot.get(path).is_some_and(|s| files::stamp(path).as_ref() != Some(s)) {
            return Err(AppError::ChangedOnDisk(path.to_string()));
        }
        crate::write_atomic(path, &content)?;
        self.store(path, content, tags);
        self.remember(path);
        Ok(())
    }

    /// Create the page `name`, holding its title heading, in `format` (else the
    /// default one); `Note.md` is the page `Note` in Markdown, `Jeux/Dragon` the
    /// page `Dragon` of the folder `Jeux` (made if needed).
    pub fn create(&mut self, name: &str, format: Option<Format>, tags: TagSyntax) -> Result<FileEntry, AppError> {
        let name = valid_path(name)?;
        let (name, format) = match split_ext(&name) {
            Some((stem, f)) if self.formats.accepts(f) => (valid_path(stem)?, f),
            _ => (name, format.filter(|f| self.formats.accepts(*f)).unwrap_or(self.formats.default)),
        };
        let path = files::page_path(&self.path, &name, format, &self.journal)?;
        if path.exists() {
            return Err(AppError::PageExists(name));
        }
        let content = format.title_line(leaf(&name));
        crate::write_atomic(&path, &content)?;
        let path = crate::slash(&path);
        self.store(&path, content, tags);
        self.remember(&path);
        Ok(FileEntry { name, path, ..Default::default() })
    }

    /// The page templates of the project.
    pub fn templates(&self) -> Vec<FileEntry> {
        files::list_templates(&self.path, &self.templates, self.formats)
    }

    /// Create the template `name` in `format` holding `text`, in `templates/` (made if needed).
    pub fn create_template(&mut self, name: &str, format: Format, text: &str) -> Result<FileEntry, AppError> {
        let name = valid_name(name)?;
        if !self.formats.accepts(format) {
            return Err(AppError::NotAPage(format!("{name}.{}", format.ext())));
        }
        let dir = files::templates_dir(&self.path, &self.templates);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{name}.{}", format.ext()));
        if path.exists() {
            return Err(AppError::PageExists(name));
        }
        crate::write_atomic(&path, text)?;
        Ok(FileEntry { name, path: crate::slash(&path), ..Default::default() })
    }

    /// Whether `path` is a page template (in the templates folder): read and written as a page,
    /// but no page of the project (no links, tags, nor place in the pages list).
    fn is_template(&self, path: &str) -> bool {
        let folder = Path::new(path).parent().and_then(|d| std::fs::canonicalize(d).ok());
        folder.is_some() && folder == std::fs::canonicalize(files::templates_dir(&self.path, &self.templates)).ok()
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
        // The page is in `pages/` or `journals/` (Logseq), or right in the project's folder
        let folder = Path::new(page).parent().map(std::fs::canonicalize).transpose()?;
        let up = folder != Some(std::fs::canonicalize(&self.path)?);
        Ok(format!("{}assets/{file}", if up { "../" } else { "./" }))
    }

    /// Pick up the pages created, modified or deleted outside the app since the
    /// last look. `None` when nothing changed.
    pub fn refresh(&mut self, tags: TagSyntax) -> Result<Option<VaultChanges>, AppError> {
        let files = files::list_pages(&self.path, self.formats, &self.journal, &self.templates)?;
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
        // With their aliases, which the listing doesn't read
        Ok(Some(VaultChanges { files: self.files(), changed, removed }))
    }

    /// Read the links again (after `#tags` started being read differently).
    pub fn reindex(&mut self, tags: TagSyntax) {
        self.links = BacklinkIndex::new();
        for (path, content) in &self.pages {
            self.links.index_file(path, &links::extract_links(content, tags, Format::of_page(path)));
        }
    }

    fn store(&mut self, path: &str, content: String, tags: TagSyntax) {
        if self.is_template(path) { return; }
        self.links.index_file(path, &links::extract_links(&content, tags, Format::of_page(path)));
        let aliases = orgamnesia_core::aliases::aliases(&content, Format::of_page(path));
        let aliases_changed = self.aliases.get(path).map_or(!aliases.is_empty(), |a| *a != aliases);
        if aliases.is_empty() { self.aliases.remove(path); } else { self.aliases.insert(path.to_string(), aliases); }
        if self.pages.insert(path.to_string(), content).is_none() || aliases_changed { self.resolver.take(); }
    }

    fn forget(&mut self, path: &str) {
        self.links.remove_source(path);
        self.aliases.remove(path);
        if self.pages.remove(path).is_some() { self.resolver.take(); }
        self.snapshot.remove(path);
    }

    /// Record the stamp of a page written by the app, so `refresh` doesn't
    /// report it as changed outside.
    fn remember(&mut self, path: &str) {
        if self.is_template(path) { return; }
        if let Some(s) = files::stamp(path) {
            self.snapshot.insert(path.to_string(), s);
        }
    }
}

/// The last part of the page name `name`: `Dragon` for `Jeux/Dragon`.
fn leaf(name: &str) -> &str {
    name.rsplit('/').next().unwrap_or(name)
}

/// `name` trimmed, if it can be the name of a page or folder in a folder:
/// `Jeux/Quêtes/Dragon`, each part a valid name (see `valid_name`), no `..`.
fn valid_path(name: &str) -> Result<String, AppError> {
    let name = name.trim().trim_matches('/');
    let bad = || AppError::InvalidName(name.to_string());
    if name.split('/').any(|part| part != part.trim() || valid_name(part).is_err()) { return Err(bad()); }
    Ok(name.to_string())
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
mod tests;
