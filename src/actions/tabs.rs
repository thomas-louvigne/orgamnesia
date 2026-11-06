//! The tabs: opening a page or following a link in one, creating a page, closing them.

use super::*;

/// Add `tab` and make it the active one; when its page got opened meanwhile
/// (two quick clicks), only activate the existing tab.
fn push_tab(ctx: AppCtx, tab: Tab) {
    if let Some(idx) = ctx.work.tab_index(&tab.path) {
        ctx.work.active_tab.set(Some(idx));
        return;
    }
    ctx.work.tabs.update(|tabs| tabs.push(tab));
    ctx.work.active_tab.set(Some(ctx.work.tabs.with_untracked(|t| t.len()) - 1));
}

/// Open `file` in a tab (or activate its tab); with `goto`, also move the cursor there.
pub fn open_file(ctx: AppCtx, file: FileEntry, goto: Option<Goto>) {
    // Also for the page already shown: it is below them
    ctx.ui.show_settings.set(false);
    let go = move |path: String| {
        if let Some(g) = goto { ctx.work.goto.set(Some((path, g))); }
    };
    if let Some(idx) = ctx.work.tab_index(&file.path) {
        ctx.work.active_tab.set(Some(idx));
        go(file.path);
        return;
    }
    // Signals are created here, in the reactive owner, not after the await
    let tab = Tab::new(&file, String::new(), false);
    spawn_local(async move {
        match read_page(&file.path).await {
            Ok(content) => {
                tab.content.set(content);
                push_tab(ctx, tab);
                go(file.path);
            }
            Err(e) => ctx.error("error", &e),
        }
    });
}

/// Open the existing page called `name` (per the case-sensitivity setting).
pub fn open_page_named(ctx: AppCtx, name: &str, goto: Option<Goto>) {
    if let Some(file) = ctx.find_page(name) {
        open_file(ctx, file, goto);
    }
}

/// Where a link points outside the project's pages: a web page, or a file
/// (`file:`, `./`, `../`, `/`, `~/`).
#[derive(Debug, PartialEq)]
pub enum Outside { Web(String), File(String) }

pub fn outside(target: &str) -> Option<Outside> {
    let t = target.trim();
    if t.contains("://") && !t.starts_with("file://") { return Some(Outside::Web(t.to_string())); }
    let path = t.strip_prefix("file://").or_else(|| t.strip_prefix("file:")).unwrap_or(t);
    // `C:/…` or `C:\…`: a Windows path
    let drive = path.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) && matches!(path.get(1..3), Some(":/" | ":\\"));
    (path != t || drive || ["./", "../", "/", "~/"].iter().any(|p| path.starts_with(p))).then(|| Outside::File(path.to_string()))
}

/// Open the page a link points to, creating it when it doesn't exist yet, at the
/// place the link names (`[[Page::*Titre]]`); a web link opens in the browser, a
/// link to a file with its application.
pub fn follow_link(ctx: AppCtx, name: String) {
    if let Some(target) = outside(&name) {
        spawn_local(async move {
            let done = match target {
                Outside::Web(url) => invoke::open_url(&url).await,
                Outside::File(path) => {
                    // From the folder of the pages (every page is in it)
                    let base = ctx.work.active_tab_data().map(|t| t.path)
                        .or_else(|| ctx.project.files.with_untracked(|f| f.first().map(|f| f.path.clone())))
                        .unwrap_or_default();
                    invoke::open_file_link(&crate::images::resolve(&base, &path)).await
                }
            };
            if let Err(e) = done { ctx.error("open_link_failed", &e); }
        });
        return;
    }
    // `[[Page::*Titre]]`: the headline `Titre` of `Page`; `[[*Titre]]`: of this page
    let (page, place) = orgamnesia_core::links::split(&name);
    let goto = place.map(|p| Goto::Place(p.to_string()));
    if page.is_empty() {
        if let (Some(tab), Some(goto)) = (ctx.work.active_tab_data(), goto) {
            ctx.work.goto.set(Some((tab.path, goto)));
        }
        return;
    }
    let file = ctx.find_page(page);
    let open = file.as_ref().and_then(|f| ctx.work.tabs.with_untracked(|tabs| tabs.iter().position(|t| t.path == f.path)));
    if let Some(idx) = open {
        ctx.work.active_tab.set(Some(idx));
        if let Some(goto) = goto {
            let path = ctx.work.tabs.with_untracked(|tabs| tabs[idx].path.clone());
            ctx.work.goto.set(Some((path, goto)));
        }
    } else if let Some(file) = file {
        open_file(ctx, file, goto);
    } else {
        create_and_open_page(ctx, page.to_string());
    }
}

/// Create the page `name`, add it to the list and open it in a new tab. When
/// the format of new pages is asked (and `name` doesn't tell it, as `Note.md`
/// does), the new page window opens with the name instead.
pub fn create_and_open_page(ctx: AppCtx, name: String) {
    if asks_format(ctx) && orgamnesia_core::format::split_ext(name.trim()).is_none() {
        ctx.ui.new_page_name.set(name);
        ctx.ui.show_new_page.set(true);
        return;
    }
    create_page_as(ctx, name, None);
}

/// Whether the format of each new page is asked (both formats being read).
pub fn asks_format(ctx: AppCtx) -> bool {
    ctx.pref_untracked(|p| p.ask_format && !p.single_format)
}

/// Create the page `name` in `format` (`None`: the default one), add it to the
/// list and open it in a new tab.
pub fn create_page_as(ctx: AppCtx, name: String, format: Option<orgamnesia_core::Format>) {
    // Signals are created here, in the reactive owner, not after the await
    let content = RwSignal::new(String::new());
    let dirty = RwSignal::new(true);
    spawn_local(async move {
        match invoke::create_page(&name, format).await {
            Ok(file) => {
                let format = orgamnesia_core::Format::of_page(&file.path);
                content.set(format.title_line(&file.name));
                ctx.project.files.update(|files| {
                    files.push(file.clone());
                    files.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.path.cmp(&b.path)));
                });
                ctx.warn_twin(&file);
                push_tab(ctx, Tab { path: file.path, name: file.name, content, dirty, history: StoredValue::new(Default::default()) });
            }
            Err(e) => ctx.error("error", &e),
        }
    });
}

/// Close the tab at `idx`; the one before it becomes active.
pub fn close_tab(ctx: AppCtx, idx: usize) {
    let len = ctx.work.tabs.with_untracked(|t| t.len());
    if idx >= len { return; }
    ctx.work.tabs.update(|tabs| { tabs.remove(idx); });
    ctx.work.active_tab.update(|active| *active = active_after_close(*active, idx, len));
}

/// Close every tab.
pub fn close_all_tabs(ctx: AppCtx) {
    ctx.work.tabs.set(vec![]);
    ctx.work.active_tab.set(None);
}

/// The active tab once tab `closed` of `len` is closed: the same tab (its index
/// shifts when it was after the closed one), or the one before the closed tab.
fn active_after_close(active: Option<usize>, closed: usize, len: usize) -> Option<usize> {
    match active? {
        i if i == closed => (len > 1).then(|| closed.saturating_sub(1)),
        i if i > closed => Some(i - 1),
        i => Some(i),
    }
}

/// Close the tab showing the page at `path`, if any.
pub fn close_tab_of(ctx: AppCtx, path: &str) {
    if let Some(idx) = ctx.work.tab_index(path) {
        close_tab(ctx, idx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_outside_the_pages() {
        assert_eq!(outside("https://example.com"), Some(Outside::Web("https://example.com".into())));
        assert_eq!(outside("../assets/doc_1_0.pdf"), Some(Outside::File("../assets/doc_1_0.pdf".into())));
        assert_eq!(outside("file:~/notes.txt"), Some(Outside::File("~/notes.txt".into())));
        assert_eq!(outside("file:///tmp/a.pdf"), Some(Outside::File("/tmp/a.pdf".into())));
        assert_eq!(outside("C:/Users/moi/doc.pdf"), Some(Outside::File("C:/Users/moi/doc.pdf".into())));
        assert_eq!(outside("Une page"), None);
    }

    #[test]
    fn closing_a_tab_keeps_a_sensible_active_one() {
        // Closing the active tab: the previous one, or the first when it was first
        assert_eq!(active_after_close(Some(2), 2, 4), Some(1));
        assert_eq!(active_after_close(Some(0), 0, 3), Some(0));
        // The last tab: none left
        assert_eq!(active_after_close(Some(0), 0, 1), None);
        // Another tab: the active one stays, shifted when it was after it
        assert_eq!(active_after_close(Some(3), 1, 4), Some(2));
        assert_eq!(active_after_close(Some(1), 2, 4), Some(1));
        assert_eq!(active_after_close(None, 0, 2), None);
    }

    #[test]
    fn project_names() {
        assert_eq!(project_name("/home/me/notes/"), "notes");
        assert_eq!(project_name("/home/me/notes"), "notes");
        assert_eq!(project_name("/"), "/");
    }
}
