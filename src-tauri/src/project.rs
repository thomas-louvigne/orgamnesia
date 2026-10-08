//! The open project: its pages, kept in memory with the links between them,
//! and the state of the files as last seen on disk (to notice outside changes).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use orgamnesia_core::{BrokenLink, FileEntry, OrgTags, Prefs, TagCount, TagHit, TagSyntax, VaultChanges};

use crate::{error::AppError, index::backlinks::BacklinkIndex, parser, tags, vault};

pub struct Project {
    /// The folder chosen as the project.
    pub path: String,
    /// Where its pages are, canonical (see `vault::pages_dir`).
    pages_dir: PathBuf,
    /// Content of every page, by path.
    pages: HashMap<String, String>,
    links: BacklinkIndex,
    snapshot: vault::Snapshot,
}

impl Project {
    /// Open the folder `path` (creating `pages/` and `assets/` if needed) and read its pages.
    pub fn open(path: &str, tags: TagSyntax) -> Result<Self, AppError> {
        vault::ensure_structure(path)?;
        let mut project = Self {
            path: path.to_string(),
            pages_dir: std::fs::canonicalize(vault::pages_dir(path))?,
            pages: HashMap::new(),
            links: BacklinkIndex::new(),
            snapshot: vault::Snapshot::new(),
        };
        let files = vault::list_org_files(path)?;
        for f in &files {
            let content = std::fs::read_to_string(&f.path).unwrap_or_default();
            project.store(&f.path, content, tags);
        }
        project.snapshot = vault::snapshot(&files);
        Ok(project)
    }

    /// The pages, sorted by name.
    pub fn files(&self) -> Vec<FileEntry> {
        let mut files: Vec<FileEntry> = self.pages.keys()
            .map(|p| FileEntry { name: vault::page_name(p), path: p.clone() })
            .collect();
        files.sort_by(|a, b| a.name.cmp(&b.name));
        files
    }

    /// Ok when `path` is a page of the project: a `.org` file right in its pages folder.
    /// Every path received from the interface goes through here.
    pub fn check_page(&self, path: &str) -> Result<(), AppError> {
        let p = Path::new(path);
        let is_org = p.extension().and_then(|e| e.to_str()) == Some("org");
        let folder = p.parent().and_then(|d| std::fs::canonicalize(d).ok());
        if is_org && folder.as_deref() == Some(self.pages_dir.as_path()) {
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
        std::fs::write(path, &content)?;
        self.store(path, content, tags);
        self.remember(path);
        Ok(())
    }

    /// Create the page `name`, holding its title heading.
    pub fn create(&mut self, name: &str, tags: TagSyntax) -> Result<FileEntry, AppError> {
        let name = valid_name(name)?;
        let path = vault::page_path(&self.path, &name);
        if path.exists() {
            return Err(AppError::PageExists(name));
        }
        let content = format!("* {name}\n");
        std::fs::write(&path, &content)?;
        let path = path.to_string_lossy().to_string();
        self.store(&path, content, tags);
        self.remember(&path);
        Ok(FileEntry { name, path })
    }

    /// Delete a page (irreversible).
    pub fn delete(&mut self, path: &str) -> Result<(), AppError> {
        self.check_page(path)?;
        std::fs::remove_file(path)?;
        self.forget(path);
        Ok(())
    }

    /// Rename a page; when the settings say so, the links to it in every page follow.
    pub fn rename(&mut self, old_path: &str, new_name: &str, prefs: &Prefs) -> Result<FileEntry, AppError> {
        self.check_page(old_path)?;
        let new_name = valid_name(new_name)?;
        let new_path = self.pages_dir.join(format!("{new_name}.org"));
        let old_name = vault::page_name(old_path);
        if new_path.exists() && new_name != old_name {
            return Err(AppError::PageExists(new_name));
        }
        // Keep the folder as the interface knows it (not the canonical one)
        let new_path = Path::new(old_path).with_file_name(format!("{new_name}.org"));
        std::fs::rename(old_path, &new_path)?;
        let new_path = new_path.to_string_lossy().to_string();
        let tags = prefs.tags();

        self.forget(old_path);
        self.store(&new_path, std::fs::read_to_string(&new_path).unwrap_or_default(), tags);
        self.remember(&new_path);

        if prefs.update_links && old_name != new_name {
            let paths: Vec<String> = self.pages.keys().cloned().collect();
            for path in paths {
                let Ok(text) = std::fs::read_to_string(&path) else { continue };
                let updated = parser::rewrite_links(&text, &old_name, &new_name, prefs.case_insensitive_links, tags);
                if let Some(updated) = updated {
                    std::fs::write(&path, &updated)?;
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
        let files = vault::list_org_files(&self.path)?;
        let now = vault::snapshot(&files);
        let (changed, removed) = vault::diff(&self.snapshot, &now);
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
            self.links.index_file(&vault::page_name(path), &parser::extract_links(content, tags));
        }
    }

    /// Pages linking to `page`.
    pub fn backlinks(&self, page: &str, ignore_case: bool) -> Vec<String> {
        if ignore_case {
            self.links.get_backlinks_ignore_case(page)
        } else {
            self.links.get_backlinks(page)
        }
    }

    /// Links to pages that don't exist, most used first.
    pub fn broken_links(&self, ignore_case: bool) -> Vec<BrokenLink> {
        let norm = |s: &str| orgamnesia_core::names::page_key(s.trim(), ignore_case);
        let pages: HashSet<String> = self.pages.keys().map(|p| norm(&vault::page_name(p))).collect();

        // Group targets that only differ by case or accents when they are ignored
        let mut grouped: Vec<(String, BrokenLink)> = Vec::new();
        for (target, sources, count) in self.links.all_links() {
            let key = norm(&target);
            if !parser::is_page_link(&target) || pages.contains(&key) {
                continue;
            }
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
        for content in self.pages.values() {
            for t in tags::written_tags(content, org) { *counts.entry(t).or_default() += 1; }
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
            tags::search(content, &query, org).into_iter().map(move |h| TagHit {
                page: f.name.clone(),
                path: f.path.clone(),
                heading: h.heading,
                level: h.level,
                line: h.line,
                tags: h.tags,
            }).collect::<Vec<_>>()
        }).collect()
    }

    fn store(&mut self, path: &str, content: String, tags: TagSyntax) {
        self.links.index_file(&vault::page_name(path), &parser::extract_links(&content, tags));
        self.pages.insert(path.to_string(), content);
    }

    fn forget(&mut self, path: &str) {
        self.links.remove_source(&vault::page_name(path));
        self.pages.remove(path);
        self.snapshot.remove(path);
    }

    /// Record the stamp of a page written by the app, so `refresh` doesn't
    /// report it as changed outside.
    fn remember(&mut self, path: &str) {
        if let Some(s) = vault::stamp(path) {
            self.snapshot.insert(path.to_string(), s);
        }
    }
}

/// `name` trimmed, if it can be a page name (and so a file name).
fn valid_name(name: &str) -> Result<String, AppError> {
    let name = name.trim();
    if name.is_empty() || name.contains(['/', '\\']) || name.starts_with('.') {
        return Err(AppError::InvalidName(name.to_string()));
    }
    Ok(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use orgamnesia_core::Hashtags;

    /// A fresh project folder holding the given pages.
    fn project(test: &str, pages: &[(&str, &str)]) -> (PathBuf, Project) {
        let dir = std::env::temp_dir().join(format!("orgamnesia-project-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("pages")).unwrap();
        for (name, content) in pages {
            std::fs::write(dir.join("pages").join(format!("{name}.org")), content).unwrap();
        }
        let p = Project::open(dir.to_str().unwrap(), Hashtags::Dashes.into()).unwrap();
        (dir, p)
    }

    fn path_of(p: &Project, name: &str) -> String {
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
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn links_follow_writes_creations_and_deletions() {
        let (dir, mut p) = project("links", &[("a", "voir [[b]] et [[c]]\n")]);
        assert_eq!(p.backlinks("b", false), vec!["a"]);
        assert_eq!(p.broken_links(false).len(), 2);

        p.create("b", Hashtags::Dashes.into()).unwrap();
        assert_eq!(p.broken_links(false).iter().map(|b| b.target.as_str()).collect::<Vec<_>>(), vec!["c"]);
        assert!(matches!(p.create("b", Hashtags::Dashes.into()), Err(AppError::PageExists(_))));

        p.write(&path_of(&p, "a"), "plus rien\n".into(), Hashtags::Dashes.into()).unwrap();
        assert!(p.backlinks("b", false).is_empty());

        p.delete(&path_of(&p, "b")).unwrap();
        assert_eq!(p.files().len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rename_rewrites_links() {
        let (dir, mut p) = project("rename", &[("old", "* old\n"), ("other", "lien [[old]] #old\n")]);
        let nf = p.rename(&path_of(&p, "old"), "new", &Prefs::default()).unwrap();
        assert_eq!(nf.name, "new");
        assert_eq!(p.read(&path_of(&p, "other")).unwrap(), "lien [[new]] #new\n");
        assert_eq!(p.backlinks("new", false), vec!["other"]);
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
