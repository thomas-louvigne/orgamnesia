//! The pages as a tree: the folders of the pages (`Jeux/Quêtes/Dragon` is in
//! `Quêtes`, in `Jeux`), folded or not, and the days of the journal in a folder of
//! their own; folders and pages together in the order of their names. The window's pages frame and the terminal's list show these rows.

use std::collections::HashSet;

use crate::{names::fold, FileEntry};

/// The key of the journal's folder in the tree (no folder of the pages has a `:`).
pub const JOURNAL: &str = ":journal";

/// A line of the tree.
#[derive(Debug, Clone, PartialEq)]
pub enum Row {
    /// A folder: its path from the pages folder (`Jeux/Quêtes`, or `JOURNAL`), its
    /// own name, its depth, whether it is unfolded, how many pages it holds (its folders' too).
    Folder { key: String, name: String, depth: usize, open: bool, pages: usize },
    /// A page: its own name (`Dragon`), its depth.
    Page { file: FileEntry, name: String, depth: usize },
}

/// Whether the page `name` is a day of the journal: `2026-11-18`.
pub fn is_day(name: &str) -> bool {
    let b = name.as_bytes();
    b.len() == 10 && b[4] == b'-' && b[7] == b'-'
        && b.iter().enumerate().all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

/// The rows of the pages `files` and the folders `folders` (empty ones too), the
/// folders of `closed` folded. The days of the journal (with `journal`: the name
/// of its folder) go in a folder of their own, last first. With a `filter`, only
/// the pages whose name holds it (case and accents ignored), their folders unfolded.
pub fn rows(files: &[FileEntry], folders: &[String], journal: Option<&str>, closed: &HashSet<String>, filter: &str) -> Vec<Row> {
    let q = fold(filter.trim());
    let shown: Vec<&FileEntry> = files.iter().filter(|f| q.is_empty() || fold(&f.name).contains(&q)).collect();
    let (days, pages): (Vec<&FileEntry>, Vec<&FileEntry>) = shown.into_iter().partition(|f| journal.is_some() && is_day(&f.name));
    let mut out = vec![];
    // Every folder, those of the pages' names too (in case the list of the folders is late)
    let mut all: Vec<&str> = folders.iter().map(String::as_str).collect();
    for f in &pages {
        let mut at = 0;
        while let Some(i) = f.name[at..].find('/') { all.push(&f.name[..at + i]); at += i + 1; }
    }
    all.sort_unstable();
    all.dedup();
    let filtering = !q.is_empty();
    // The journal's folder, among the first ones
    let journal = journal.filter(|_| !days.is_empty()).map(|label| {
        let open = filtering || !closed.contains(JOURNAL);
        let mut rows = vec![Row::Folder { key: JOURNAL.into(), name: label.into(), depth: 0, open, pages: days.len() }];
        if open {
            let mut days = days;
            days.sort_by(|a, b| b.name.cmp(&a.name));
            rows.extend(days.into_iter().map(|f| Row::Page { file: f.clone(), name: f.name.clone(), depth: 1 }));
        }
        (fold(label), rows)
    });
    walk("", 0, &all, &pages, closed, filtering, journal, &mut out);
    out
}

/// The rows of the folder `dir` (`""`: the pages folder), at `depth`: its folders
/// and pages in the order of their names (case and accents ignored), with
/// `extra` (the journal's rows, by their name) among them.
#[allow(clippy::too_many_arguments)]
fn walk(dir: &str, depth: usize, folders: &[&str], pages: &[&FileEntry], closed: &HashSet<String>, filtering: bool,
        extra: Option<(String, Vec<Row>)>, out: &mut Vec<Row>) {
    let prefix = if dir.is_empty() { String::new() } else { format!("{dir}/") };
    let in_here = |name: &str| name.strip_prefix(prefix.as_str()).is_some_and(|rest| !rest.contains('/'));
    // What this folder holds: a folder (its path) or a page, by its own name
    enum Item<'a> { Folder(&'a str), Page(&'a FileEntry), Extra(Vec<Row>) }
    let mut items: Vec<(String, Item)> = folders.iter().filter(|f| in_here(f))
        .map(|f| (fold(&f[prefix.len()..]), Item::Folder(f)))
        .chain(pages.iter().filter(|p| in_here(&p.name)).map(|p| (fold(&p.name[prefix.len()..]), Item::Page(p))))
        .chain(extra.map(|(name, rows)| (name, Item::Extra(rows))))
        .collect();
    items.sort_by(|a, b| a.0.cmp(&b.0));
    for (_, item) in items {
        match item {
            Item::Folder(folder) => {
                let inner = format!("{folder}/");
                let count = pages.iter().filter(|p| p.name.starts_with(&inner)).count();
                // Filtering: only the folders holding a page found
                if filtering && count == 0 { continue; }
                let open = filtering || !closed.contains(folder);
                out.push(Row::Folder { key: folder.to_string(), name: folder[prefix.len()..].to_string(), depth, open, pages: count });
                if open { walk(folder, depth + 1, folders, pages, closed, filtering, None, out); }
            }
            Item::Page(p) => out.push(Row::Page { file: p.clone(), name: p.name[prefix.len()..].to_string(), depth }),
            Item::Extra(rows) => out.extend(rows),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str) -> FileEntry {
        FileEntry { name: name.into(), path: format!("/p/{name}.org"), ..Default::default() }
    }

    fn show(rows: &[Row]) -> Vec<String> {
        rows.iter().map(|r| match r {
            Row::Folder { name, depth, open, pages, .. } => format!("{}{}{name} ({pages})", "  ".repeat(*depth), if *open { "▾" } else { "▸" }),
            Row::Page { name, depth, .. } => format!("{}{name}", "  ".repeat(*depth)),
        }).collect()
    }

    #[test]
    fn folders_and_pages_in_the_order_of_their_names() {
        let files: Vec<FileEntry> = ["Accueil", "Jeux/Quêtes/Dragon", "Jeux/Règles", "2026-11-22", "2026-11-23", "inbox"].map(file).to_vec();
        let folders = vec!["Jeux".to_string(), "Jeux/Quêtes".to_string(), "Vide".to_string()];
        let mut closed = HashSet::new();
        assert_eq!(show(&rows(&files, &folders, Some("Journal"), &closed, "")),
            ["Accueil", "inbox", "▾Jeux (2)", "  ▾Quêtes (1)", "    Dragon", "  Règles", "▾Journal (2)", "  2026-11-23", "  2026-11-22", "▾Vide (0)"]);
        closed.insert("Jeux".to_string());
        closed.insert(JOURNAL.to_string());
        assert_eq!(show(&rows(&files, &folders, Some("Journal"), &closed, "")), ["Accueil", "inbox", "▸Jeux (2)", "▸Journal (2)", "▾Vide (0)"]);
        // Found by a filter: the folders holding them unfolded, the others left out
        assert_eq!(show(&rows(&files, &folders, Some("Journal"), &closed, "drag")), ["▾Jeux (1)", "  ▾Quêtes (1)", "    Dragon"]);
        // Without the journal, the days are pages as the others
        assert_eq!(show(&rows(&files, &[], None, &HashSet::new(), "2026")), ["2026-11-22", "2026-11-23"]);
    }
}
