//! Find and replace in the page edited: the matches, the search typed
//! incrementally (as Emacs' C-s), a replacement one match at a time.

use super::*;

impl Editor {
    /// The matches of `query` (case and accents ignored).
    pub fn find_all(&self, query: &str) -> Vec<(usize, usize)> {
        if query.is_empty() { return vec![]; }
        find_all(&self.text, query, SearchOptions { match_case: self.prefs.match_case, ..SearchOptions::default() })
    }

    /// The matches of the search going on.
    pub fn matches(&self) -> Vec<(usize, usize)> {
        self.search.as_ref().map_or(vec![], |s| self.find_all(&s.query))
    }

    /// `find` (`find_previous` with `!forward`): start a search, else go to its
    /// next match (the text last searched when nothing is typed yet).
    pub fn find(&mut self, forward: bool) {
        let Some(s) = &mut self.search else {
            self.search = Some(Search { query: String::new(), origin: self.cursor, forward, current: None, wrapped: false });
            return;
        };
        if s.query.is_empty() { s.query = self.last_query.clone(); }
        s.forward = forward;
        // Past the match the cursor is on
        let from = match s.current { Some((a, _)) if forward => a + 1, Some((a, _)) => a, None => self.cursor };
        self.locate(from, true);
    }

    /// A search of `query` started and on its first match (a project search opening a page).
    pub fn search_for(&mut self, query: &str) {
        self.search = Some(Search { query: query.to_string(), origin: self.cursor, forward: true, current: None, wrapped: false });
        self.locate(0, true);
    }

    /// A replacement of `from` starting (Emacs' `query-replace`): on its first
    /// match from the cursor; false when there is none.
    pub fn start_replace(&mut self, from: &str) -> bool {
        self.search = Some(Search { query: from.to_string(), origin: self.cursor, forward: true, current: None, wrapped: false });
        let c = self.cursor;
        self.locate(c, false);
        let found = self.search.as_ref().is_some_and(|s| s.current.is_some());
        if !found { self.search = None; }
        found
    }

    /// The match of the replacement replaced with `with` (`None`: left as it
    /// is), then on to the next one; false when there is none.
    pub fn replace_match(&mut self, with: Option<&str>) -> bool {
        let Some((a, b)) = self.search.as_ref().and_then(|s| s.current) else { return false };
        let next = match with {
            Some(with) => {
                let end = a + with.chars().count();
                self.replace(a, b, with, end, Change::Other);
                end
            }
            None => b,
        };
        self.locate(next, false);
        self.search.as_ref().is_some_and(|s| s.current.is_some())
    }

    /// Put the cursor on the first match from `from` in the way of the search,
    /// going round the page when `wrap` and there is none.
    pub(super) fn locate(&mut self, from: usize, wrap: bool) {
        let all = self.matches();
        let Some(s) = &mut self.search else { return };
        let pick = |from: usize| if s.forward { all.iter().find(|m| m.0 >= from).copied() } else { all.iter().rev().find(|m| m.0 < from).copied() };
        let mut hit = pick(from);
        if hit.is_none() && wrap && !all.is_empty() {
            hit = if s.forward { all.first().copied() } else { all.last().copied() };
            s.wrapped = true;
        }
        s.current = hit;
        if let Some((a, b)) = hit {
            self.cursor = if s.forward { b } else { a };
            // A match in a fold shows it
            self.folds.retain(|&(x, y)| !(x < b && a <= y));
        }
    }

    /// The text searched typed (`Some`) or shortened (`None`): the first match
    /// from where the search started.
    pub fn search_edit(&mut self, c: Option<char>) {
        let Some(s) = &mut self.search else { return };
        match c { Some(c) => s.query.push(c), None => { s.query.pop(); } }
        let origin = s.origin;
        s.wrapped = false;
        // Backwards, the match may start at the origin
        let from = if s.forward { origin } else { origin + 1 };
        self.locate(from, false);
        if self.search.as_ref().is_some_and(|s| s.current.is_none()) { self.cursor = origin; }
    }

    /// End the search: the cursor left where it is, else (`!keep`) put back where it started.
    pub fn end_search(&mut self, keep: bool) {
        if let Some(s) = self.search.take() {
            if !s.query.is_empty() { self.last_query = s.query; }
            if !keep { self.cursor = s.origin; }
        }
        self.goal = None;
    }
}
