//! The history of the pages visited (Alt+← / Alt+→).

use super::*;

/// Pages kept in each direction of the history.
const HISTORY_LEN: usize = 50;

/// Record a visit: `left` was shown, another page now is.
pub fn record_visit(ctx: AppCtx, left: String, now: Option<&str>) {
    let work = ctx.work;
    // Reached by going back or forward: the history already knows
    if let Some(arriving) = work.history_arriving.get_untracked() {
        work.history_arriving.set(None);
        if now == Some(arriving.as_str()) { return; }
    }
    work.history_back.update(|back| {
        back.retain(|p| *p != left);
        back.push(left);
        if back.len() > HISTORY_LEN { back.remove(0); }
    });
    work.history_forward.set(vec![]);
}

/// Go back (`back`) or forward in the pages visited. A page closed since is
/// reopened; one deleted since is skipped.
pub fn history_step(ctx: AppCtx, back: bool) {
    let work = ctx.work;
    let (from, to) = if back { (work.history_back, work.history_forward) } else { (work.history_forward, work.history_back) };
    let current = work.active_tab_data().map(|t| t.path);
    loop {
        let Some(path) = from.try_update(|v| v.pop()).flatten() else {
            ctx.notify_t("history_empty", "");
            return;
        };
        if current.as_deref() == Some(path.as_str()) { continue; }
        let target = match work.tab_index(&path) {
            Some(idx) => Ok(idx),
            None => match ctx.project.files.with_untracked(|f| f.iter().find(|f| f.path == path).cloned()) {
                Some(file) => Err(file),
                None => continue,
            },
        };
        if let Some(cur) = current {
            to.update(|v| {
                v.push(cur);
                if v.len() > HISTORY_LEN { v.remove(0); }
            });
        }
        work.history_arriving.set(Some(path));
        match target {
            Ok(idx) => work.active_tab.set(Some(idx)),
            Err(file) => open_file(ctx, file, None),
        }
        return;
    }
}
