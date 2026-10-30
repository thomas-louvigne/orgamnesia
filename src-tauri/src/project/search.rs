//! Search and replace across every page of the project, and the undo of the
//! last replacement.

use std::collections::{HashMap, HashSet};

use orgamnesia_core::{search, PageHits, ReplaceOutcome, ReplaceSkip, SearchOptions, SearchResults, TagSyntax};

use super::Project;
use super::files;
use crate::error::AppError;

/// Matches sent to the interface at most; the counts stay exact past it.
const MAX_HITS: usize = 1000;

/// A page as it was before and after a project replacement.
pub struct Replaced {
    path: String,
    before: String,
    after: String,
}

impl Project {
    /// The matches of `query` in every page, sorted by page name. `open` holds the
    /// text of the pages being edited, used in place of the one known on disk.
    pub fn search(&self, query: &str, opts: SearchOptions, open: &HashMap<String, String>) -> SearchResults {
        let mut paths: Vec<&String> = self.pages.keys().collect();
        paths.sort_by_cached_key(|p| files::page_name(p).to_lowercase());
        let mut results = SearchResults::default();
        for path in paths {
            let text = open.get(path).unwrap_or(&self.pages[path]);
            let chars: Vec<char> = text.chars().collect();
            let ranges = search::find_all(&chars, query, opts);
            if ranges.is_empty() { continue; }
            let room = MAX_HITS.saturating_sub(results.total);
            let shown = &ranges[..ranges.len().min(room)];
            results.truncated |= shown.len() < ranges.len();
            results.total += ranges.len();
            results.pages.push(PageHits {
                page: files::page_name(path),
                path: path.clone(),
                hits: search::hits(&chars, shown),
                count: ranges.len(),
            });
        }
        results
    }

    /// Replace `query` by `with` in every page, but the pages and matches of `skip`;
    /// what the pages were is kept for `undo_replace`. The pages are read from disk:
    /// the interface writes the ones being edited first.
    pub fn replace_all(
        &mut self, query: &str, opts: SearchOptions, with: &str, skip: &ReplaceSkip, tags: TagSyntax,
    ) -> Result<ReplaceOutcome, AppError> {
        let skip_pages: HashSet<&String> = skip.pages.iter().collect();
        let skip_hits: HashSet<(&String, usize)> = skip.hits.iter().map(|(p, s)| (p, *s)).collect();
        let mut paths: Vec<String> = self.pages.keys().filter(|p| !skip_pages.contains(p)).cloned().collect();
        paths.sort();
        let mut outcome = ReplaceOutcome::default();
        let mut done = Vec::new();
        let mut result = Ok(());
        for path in paths {
            let before = match std::fs::read_to_string(&path) {
                Ok(t) => t,
                Err(e) => { result = Err(e.into()); break; }
            };
            let chars: Vec<char> = before.chars().collect();
            let ranges: Vec<(usize, usize)> = search::find_all(&chars, query, opts).into_iter()
                .filter(|&(start, _)| !skip_hits.contains(&(&path, start)))
                .collect();
            if ranges.is_empty() { continue; }
            let after = search::replace_ranges(&chars, &ranges, with);
            if let Err(e) = std::fs::write(&path, &after) { result = Err(e.into()); break; }
            self.store(&path, after.clone(), tags);
            self.remember(&path);
            outcome.pages += 1;
            outcome.replacements += ranges.len();
            done.push(Replaced { path, before, after });
        }
        // Even cut short by an error, what was written can be undone
        if !done.is_empty() { self.last_replace = done; }
        result.map(|_| outcome)
    }

    /// Whether there is a project replacement to undo.
    pub fn can_undo_replace(&self) -> bool {
        !self.last_replace.is_empty()
    }

    /// Put back the pages of the last project replacement, but the ones changed
    /// since (counted as skipped: undoing would lose those changes).
    pub fn undo_replace(&mut self, tags: TagSyntax) -> Result<ReplaceOutcome, AppError> {
        let mut outcome = ReplaceOutcome::default();
        for page in std::mem::take(&mut self.last_replace) {
            if std::fs::read_to_string(&page.path).ok().as_deref() != Some(page.after.as_str()) {
                outcome.skipped += 1;
                continue;
            }
            std::fs::write(&page.path, &page.before)?;
            self.store(&page.path, page.before, tags);
            self.remember(&page.path);
            outcome.pages += 1;
        }
        Ok(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{path_of, project};
    use super::*;
    use orgamnesia_core::Hashtags;

    const ANY: SearchOptions = SearchOptions { match_case: false, whole_word: false };

    #[test]
    fn searches_every_page() {
        let (dir, p) = project("search", &[("b", "* b\nun Chat\n"), ("a", "* a\nchat, chat\n"), ("c", "* c\n")]);
        let r = p.search("chat", ANY, &HashMap::new());
        assert_eq!(r.total, 3);
        assert!(!r.truncated);
        let pages: Vec<(&str, usize)> = r.pages.iter().map(|h| (h.page.as_str(), h.count)).collect();
        assert_eq!(pages, vec![("a", 2), ("b", 1)]);
        assert_eq!(r.pages[1].hits[0].line, 1);
        // The text being edited counts, not the one on disk
        let open = HashMap::from([(path_of(&p, "c"), "* c\nchat\n".to_string())]);
        assert_eq!(p.search("chat", ANY, &open).total, 4);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn replaces_then_undoes() {
        let (dir, mut p) = project("replace", &[("a", "* a\nchat chat\n"), ("b", "* b\nchat\n"), ("c", "* c\nchat\n")]);
        let tags = Hashtags::Dashes.into();
        let (a, b, c) = (path_of(&p, "a"), path_of(&p, "b"), path_of(&p, "c"));
        let skip = ReplaceSkip { pages: vec![c.clone()], hits: vec![(a.clone(), 9)] };
        let done = p.replace_all("chat", ANY, "chien", &skip, tags).unwrap();
        assert_eq!(done, ReplaceOutcome { pages: 2, replacements: 2, skipped: 0 });
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "* a\nchien chat\n");
        assert_eq!(std::fs::read_to_string(&b).unwrap(), "* b\nchien\n");
        assert_eq!(std::fs::read_to_string(&c).unwrap(), "* c\nchat\n");
        assert_eq!(p.search("chien", ANY, &HashMap::new()).total, 2);
        assert!(p.can_undo_replace());

        // A page changed since is left alone by the undo
        std::fs::write(&b, "* b\nchien !\n").unwrap();
        let undone = p.undo_replace(tags).unwrap();
        assert_eq!(undone, ReplaceOutcome { pages: 1, replacements: 0, skipped: 1 });
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "* a\nchat chat\n");
        assert_eq!(std::fs::read_to_string(&b).unwrap(), "* b\nchien !\n");
        assert!(!p.can_undo_replace());
        let _ = std::fs::remove_dir_all(dir);
    }
}
