use std::collections::{HashMap, HashSet};

/// In-memory reverse index: page → set of pages that link TO it.
#[derive(Debug, Default)]
pub struct BacklinkIndex {
    /// reverse_map[target] = {source, ...}
    reverse_map: HashMap<String, HashSet<String>>,
    /// forward_map[source] = {target, ...}  — needed to remove stale links on re-index
    forward_map: HashMap<String, HashSet<String>>,
    /// link_counts[source][target] = how many times `source` links to `target`
    link_counts: HashMap<String, HashMap<String, usize>>,
}

impl BacklinkIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Index (or re-index) one file. Replaces any previously stored links for this source.
    pub fn index_file(&mut self, source: &str, links: &[String]) {
        self.remove_source(source);
        let targets: HashSet<String> = links.iter().cloned().collect();
        for target in &targets {
            self.reverse_map
                .entry(target.clone())
                .or_default()
                .insert(source.to_string());
        }
        self.forward_map.insert(source.to_string(), targets);
        let mut counts: HashMap<String, usize> = HashMap::new();
        for l in links { *counts.entry(l.clone()).or_default() += 1; }
        self.link_counts.insert(source.to_string(), counts);
    }

    /// Remove all outgoing links from a source page (e.g. on file deletion).
    pub fn remove_source(&mut self, source: &str) {
        self.link_counts.remove(source);
        if let Some(old) = self.forward_map.remove(source) {
            for target in old {
                if let Some(set) = self.reverse_map.get_mut(&target) {
                    set.remove(source);
                    if set.is_empty() {
                        self.reverse_map.remove(&target);
                    }
                }
            }
        }
    }

    /// Pages that link TO `page`, sorted alphabetically.
    pub fn get_backlinks(&self, page: &str) -> Vec<String> {
        let mut v: Vec<String> = self
            .reverse_map
            .get(page)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default();
        v.sort();
        v
    }

    /// Every link target with the pages that link to it and the total number of
    /// times it is linked, sorted by target.
    pub fn all_links(&self) -> Vec<(String, Vec<String>, usize)> {
        let mut v: Vec<(String, Vec<String>, usize)> = self
            .reverse_map
            .iter()
            .map(|(t, s)| {
                let mut s: Vec<String> = s.iter().cloned().collect();
                s.sort();
                let count = s
                    .iter()
                    .filter_map(|src| self.link_counts.get(src).and_then(|m| m.get(t)))
                    .sum();
                (t.clone(), s, count)
            })
            .collect();
        v.sort_by_key(|a| a.0.to_lowercase());
        v
    }

    /// Like `get_backlinks`, but link targets match `page` ignoring case and accents.
    pub fn get_backlinks_ignore_case(&self, page: &str) -> Vec<String> {
        let page = orgamnesia_core::names::fold(page);
        let mut v: Vec<String> = self
            .reverse_map
            .iter()
            .filter(|(target, _)| orgamnesia_core::names::fold(target) == page)
            .flat_map(|(_, sources)| sources.iter().cloned())
            .collect();
        v.sort();
        v.dedup();
        v
    }

    /// Rebuild the whole index from scratch.
    pub fn rebuild<'a>(&mut self, entries: impl IntoIterator<Item = (&'a str, Vec<String>)>) {
        self.reverse_map.clear();
        self.forward_map.clear();
        for (source, links) in entries {
            self.index_file(source, &links);
        }
    }

    pub fn page_count(&self) -> usize {
        self.forward_map.len()
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn idx(entries: &[(&str, &[&str])]) -> BacklinkIndex {
        let mut b = BacklinkIndex::new();
        for (src, targets) in entries {
            let links: Vec<String> = targets.iter().map(|s| s.to_string()).collect();
            b.index_file(src, &links);
        }
        b
    }

    #[test]
    fn basic_backlinks() {
        let b = idx(&[("a", &["b", "c"]), ("d", &["b"])]);
        let mut bl = b.get_backlinks("b");
        bl.sort();
        assert_eq!(bl, vec!["a", "d"]);
        assert_eq!(b.get_backlinks("c"), vec!["a"]);
    }

    #[test]
    fn backlinks_ignore_case() {
        let b = idx(&[("a", &["Page"]), ("b", &["page"]), ("c", &["other"])]);
        assert_eq!(b.get_backlinks("Page"), vec!["a"]);
        assert_eq!(b.get_backlinks_ignore_case("PAGE"), vec!["a", "b"]);
    }

    #[test]
    fn backlinks_ignore_accents() {
        let b = idx(&[("a", &["élody"]), ("b", &["Elody"]), ("c", &["elodie"])]);
        assert_eq!(b.get_backlinks("élody"), vec!["a"]);
        assert_eq!(b.get_backlinks_ignore_case("Élody"), vec!["a", "b"]);
    }

    #[test]
    fn all_links_lists_targets_with_sources() {
        let b = idx(&[("a", &["x", "y", "x"]), ("b", &["x"])]);
        assert_eq!(
            b.all_links(),
            vec![("x".to_string(), vec!["a".to_string(), "b".to_string()], 3),
                 ("y".to_string(), vec!["a".to_string()], 1)]
        );
    }

    #[test]
    fn empty_index_returns_empty() {
        let b = BacklinkIndex::new();
        assert!(b.get_backlinks("anything").is_empty());
    }

    #[test]
    fn remove_source_cleans_reverse_map() {
        let mut b = idx(&[("a", &["b"]), ("c", &["b"])]);
        b.remove_source("a");
        assert_eq!(b.get_backlinks("b"), vec!["c"]);
    }

    #[test]
    fn remove_last_source_clears_entry() {
        let mut b = idx(&[("a", &["b"])]);
        b.remove_source("a");
        assert!(b.get_backlinks("b").is_empty());
    }

    #[test]
    fn reindex_removes_stale_links() {
        let mut b = idx(&[("a", &["b", "c"])]);
        b.index_file("a", &["d".to_string()]);
        assert!(b.get_backlinks("b").is_empty());
        assert!(b.get_backlinks("c").is_empty());
        assert_eq!(b.get_backlinks("d"), vec!["a"]);
    }

    #[test]
    fn rebuild_resets_state() {
        let mut b = idx(&[("x", &["y"])]);
        b.rebuild([("new", vec!["z".to_string()])]);
        assert!(b.get_backlinks("y").is_empty());
        assert_eq!(b.get_backlinks("z"), vec!["new"]);
        assert_eq!(b.page_count(), 1);
    }

    #[test]
    fn duplicate_links_deduplicated() {
        let b = idx(&[("a", &["b", "b", "b"])]);
        assert_eq!(b.get_backlinks("b"), vec!["a"]);
    }

    #[test]
    fn backlinks_sorted() {
        let b = idx(&[("z", &["x"]), ("a", &["x"]), ("m", &["x"])]);
        assert_eq!(b.get_backlinks("x"), vec!["a", "m", "z"]);
    }

    #[test]
    fn page_count() {
        let b = idx(&[("a", &["b"]), ("c", &["d"])]);
        assert_eq!(b.page_count(), 2);
    }

    #[test]
    fn remove_nonexistent_is_noop() {
        let mut b = BacklinkIndex::new();
        b.remove_source("ghost"); // should not panic
    }
}
