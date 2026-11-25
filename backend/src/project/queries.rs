//! What the project tells of its pages, read from them in memory: backlinks,
//! unlinked mentions, the graph, broken links, tags, TODO headlines, the agenda.

use std::collections::HashMap;

use orgamnesia_core::{
    agenda, pages::Resolver, AgendaHit, FileEntry, Format, BrokenLink, Graph, MentionHit, OrgTags, TagCount, TagHit, TagSyntax, TodoHit, TodoKeywords,
};

use super::{leaf, Project};
use crate::tags;

impl Project {
    /// The names links give the pages `paths` (see `Resolver::link_name`), sorted.
    fn link_names(&self, resolver: &Resolver, paths: impl IntoIterator<Item = String>) -> Vec<String> {
        let mut names: Vec<String> = paths.into_iter()
            .map(|p| FileEntry { name: self.name_of(&p), path: p, ..Default::default() })
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

    /// The places other pages name the page `path` (its name without its folders,
    /// or an alias, of 3 letters at least) without a link, by page then place; at
    /// most `MAX_MENTIONS`.
    pub fn unlinked_mentions(&self, path: &str, tags: TagSyntax, ignore_case: bool) -> Vec<MentionHit> {
        const MAX_MENTIONS: usize = 200;
        let resolver = self.resolver(ignore_case);
        let files = self.files();
        let Some(file) = files.iter().find(|f| f.path == path) else { return vec![] };
        // What is looked for, and the name its link uses
        let mut names = vec![(leaf(&file.name).to_string(), resolver.link_name(file))];
        names.extend(file.aliases.iter().map(|a| (a.clone(), a.clone())));
        names.retain(|(n, _)| n.trim().chars().count() >= 3);
        let looked: Vec<String> = names.iter().map(|(n, _)| n.clone()).collect();
        let mut out = Vec::new();
        for f in files.iter().filter(|f| f.path != path) {
            let Some(content) = self.pages.get(&f.path) else { continue };
            let format = Format::of_page(&f.path);
            let chars: Vec<char> = content.chars().collect();
            for m in orgamnesia_core::mentions::find(content, &looked, tags.links(format)) {
                let line_start = chars[..m.start].iter().rposition(|&c| c == '\n').map_or(0, |i| i + 1);
                let line_end = chars[m.start..].iter().position(|&c| c == '\n').map_or(chars.len(), |i| m.start + i);
                out.push(MentionHit {
                    page: resolver.link_name(f),
                    path: f.path.clone(),
                    line: chars[..m.start].iter().filter(|&&c| c == '\n').count(),
                    start: m.start,
                    end: m.end,
                    text: chars[m.start..m.end].iter().collect(),
                    target: names[m.name].1.clone(),
                    context: chars[line_start..line_end].iter().collect::<String>().trim().to_string(),
                });
                if out.len() == MAX_MENTIONS { return out; }
            }
        }
        out
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
    /// settings, which a page's `#+TODO:` lines replace), by page then line;
    /// with a tag search (`travail-perso`), those it finds only.
    pub fn todos(&self, keywords: &TodoKeywords, query: &str, org: OrgTags) -> Vec<TodoHit> {
        let query = tags::Query::parse(query, org);
        self.files().into_iter().flat_map(|f| {
            let content = self.pages.get(&f.path).map(String::as_str).unwrap_or("");
            let format = Format::of_page(&f.path);
            let dated: Vec<usize> = agenda::entries(content, Some(keywords), format).into_iter()
                .filter(|e| e.scheduled.is_some() || e.deadline.is_some()).map(|e| e.line).collect();
            let found = query.as_ref().map(|q| tags::search(content, q, org, format));
            let levels: Vec<Option<usize>> = content.lines().map(|l| format.heading_level_of(l)).collect();
            // Found: the page, the headline, or a headline above it whose subtree it is in
            let tagged = |line: usize, level: usize| found.as_ref().is_none_or(|hits| hits.iter().any(|h| h.heading.is_none() || h.line == line
                || (h.line < line && h.level < level && !levels[h.line + 1..line].iter().any(|l| l.is_some_and(|l| l <= h.level)))));
            orgamnesia_core::todo::headlines(content, keywords, format).into_iter()
                .filter(|h| tagged(h.line, h.level))
                .map(|h| TodoHit {
                    page: f.name.clone(),
                    path: f.path.clone(),
                    dated: dated.contains(&h.line),
                    keyword: h.keyword,
                    done: h.done,
                    title: h.title,
                    level: h.level,
                    line: h.line,
                }).collect::<Vec<_>>()
        }).collect()
    }

    /// The agenda of the `days` days from `from` (days from 1970-01-01), `today`
    /// being today: the dated headlines of every page (`keywords`: those of the
    /// settings, `None` when off), by day.
    pub fn agenda(&self, keywords: Option<&TodoKeywords>, from: i64, days: i64, today: i64) -> Vec<AgendaHit> {
        let mut out: Vec<AgendaHit> = self.files().into_iter().flat_map(|f| {
            let content = self.pages.get(&f.path).map(String::as_str).unwrap_or("");
            let format = Format::of_page(&f.path);
            let entries = agenda::entries(content, keywords, format);
            // A page named after a day (the journal's) is in the agenda that day
            let day_page = agenda::day_of_name(&f.name).filter(|d| (from..from + days).contains(d)).map(|day| AgendaHit {
                page: f.name.clone(),
                path: f.path.clone(),
                line: 0,
                day,
                when: agenda::When::Page,
                time: None,
                late: 0,
                keyword: None,
                done: false,
                title: format.title(content).unwrap_or_else(|| f.name.rsplit('/').next().unwrap_or(&f.name).to_string()),
                stamp: 0,
            });
            day_page.into_iter().chain(agenda::agenda(&entries, from, days, today).into_iter().map(|i| {
                let e = &entries[i.entry];
                AgendaHit {
                    page: f.name.clone(),
                    path: f.path.clone(),
                    line: e.line,
                    day: i.day,
                    when: i.when,
                    time: i.time.map(|(h, m)| format!("{h:02}:{m:02}")),
                    late: i.late,
                    keyword: e.keyword.clone(),
                    done: e.done,
                    title: e.title.clone(),
                    stamp: i.stamp,
                }
            })).collect::<Vec<_>>()
        }).collect();
        // The day's page first, then the times of the day, then the rest
        out.sort_by(|a, b| (a.day, a.when != agenda::When::Page, a.time.is_none(), &a.time, a.when, a.late <= 0)
            .cmp(&(b.day, b.when != agenda::When::Page, b.time.is_none(), &b.time, b.when, b.late <= 0)));
        out
    }
}
