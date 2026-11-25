//! What the editor's own keys do to the page: the drawn view's keys, auto
//! pairs, table cells, folding, TODO and dates, moving lines and subtrees, zap.

use super::*;

/// Keys the drawn view handles itself, as the page would have them; true when
/// the key was one of them.
pub(super) fn view_keys(pane: Pane, e: &web_sys::KeyboardEvent, el: &Textarea) -> bool {
    let key = e.key();
    // The fold character of a folded headline is not deleted on its own (that would
    // delete the hidden text): Backspace or Delete next to it unfolds the headline
    if matches!(key.as_str(), "Backspace" | "Delete") && unmodified(e) {
        let (start, end) = get_pos(el);
        let chars: Vec<char> = el.value().chars().collect();
        let at = if key == "Delete" { start } else { start.wrapping_sub(1) };
        if start == end && chars.get(at).is_some_and(|&c| tables::fold_index(c).is_some()) {
            e.prevent_default();
            let pos = pane.page_pos(el, at);
            pane.folds.update(|f| f.retain(|&(a, _)| a != pos));
            return true;
        }
    }
    // Enter in a cell of a table: to the cell below (org-mode's `org-table-next-row`)
    if key == "Enter" && unmodified(e) && !e.shift_key() {
        let (start, end) = get_pos(el);
        let chars: Vec<char> = el.value().chars().collect();
        let to = (start == end).then(|| tables::next_row(&chars, start)).flatten();
        if let Some(to) = to {
            e.prevent_default();
            match to {
                tables::CellMove::Caret(p) => set_cursor(el, p),
                tables::CellMove::Edit { from, to, text, caret } => {
                    splice(el, from, to, &text);
                    set_cursor(el, caret);
                    pane.commit(el);
                }
            }
            pane.track_caret(el);
            return true;
        }
    }
    // Enter at the end of a folded headline: the new line comes after the hidden text
    if key == "Enter" && unmodified(e) && !e.shift_key() {
        let (start, end) = get_pos(el);
        if start == end && el.value().chars().nth(start).is_some_and(|c| tables::fold_index(c).is_some()) {
            set_cursor(el, start + 1);
        }
    }
    // Backspace at the start of an indented line joins it to the line above
    // (the indentation is not in the page: deleting it would change nothing)
    if key == "Backspace" && unmodified(e) {
        let (start, end) = get_pos(el);
        let text = el.value();
        if start == end && let Some((a, b)) = pane.shown.with_value(|v| v.indent_run(&text, start)) {
            e.prevent_default();
            if a > 0 {
                let cursor = splice(el, a - 1, b, "");
                set_cursor(el, cursor);
                pane.commit(el);
            }
            return true;
        }
    }
    false
}

/// Auto-pairing / electric wrapping: without a selection, ( [ { and " insert
/// their closing character too; with a selection, electric mode wraps it in the
/// typed pair, or in emphasis markers (`*gras*`) when the settings say so. True when the key did so.
pub(super) fn auto_pair(pane: Pane, e: &web_sys::KeyboardEvent, el: &Textarea) -> bool {
    let key = e.key();
    let (start, end) = get_pos(el);
    // `[` on a selection: a link to it, the region done with
    let selected: String = el.value().chars().skip(start).take(end - start).collect();
    if let Some(link) = edit::link_of(&key, &selected) {
        e.prevent_default();
        let caret = splice(el, start, end, &link);
        pane.mark.set(None);
        set_cursor(el, caret);
        pane.commit(el);
        return true;
    }
    let emphasis = || (start != end && pane.ctx.pref_untracked(|p| p.electric_emphasis))
        .then(|| edit::emphasis_pair(&key, pane.fmt())).flatten();
    let Some((open, close)) = edit::pair_of(&key).or_else(emphasis) else { return false };
    if start != end {
        if !pane.ctx.pref_untracked(|p| p.electric_mode) { return false; }
        e.prevent_default();
        let selected: String = el.value().chars().skip(start).take(end - start).collect();
        let new_end = splice(el, start, end, &format!("{open}{selected}{close}"));
        set_selection(el, start + 1, new_end - 1);
    } else {
        if !edit::auto_closes(&key) { return false; }
        e.prevent_default();
        let cursor = splice(el, start, start, &format!("{open}{close}"));
        set_cursor(el, cursor - 1);
    }
    pane.commit(el);
    true
}

/// Tab / Shift+Tab move between the cells of a table; elsewhere, when the
/// settings say so, they fold / unfold as org-mode's `org-cycle` / `org-shifttab`.
pub(super) fn table_cell(pane: Pane, e: &web_sys::KeyboardEvent, el: &Textarea, action: EditorAction) {
    let chars: Vec<char> = el.value().chars().collect();
    let forward = action == EditorAction::TableNextCell;
    let Some(to) = tables::move_cell(&chars, caret(el), forward) else {
        if !pane.ctx.pref_untracked(|p| p.tab_folds) { return; }
        if forward { cycle(pane, e, el); } else if global_cycle(pane, el) { swallow(e); }
        return;
    };
    swallow(e);
    match to {
        tables::CellMove::Caret(p) => set_cursor(el, p),
        tables::CellMove::Edit { from, to, text, caret } => {
            splice(el, from, to, &text);
            set_cursor(el, caret);
            pane.commit(el);
        }
    }
    pane.track_caret(el);
}

/// A headline folds / unfolds (FOLDED, CHILDREN, SUBTREE), a list item with
/// lines under it, a block or a drawer too (org-mode's `org-cycle`).
pub(super) fn cycle(pane: Pane, e: &web_sys::KeyboardEvent, el: &Textarea) {
    let pos = pane.page_pos(el, caret(el));
    let page: Vec<char> = pane.content.with_untracked(|c| c.chars().collect());
    let line = line_start(&page, pos);
    let next = if folding::is_headline(&page, pane.fmt(), line) {
        // On a headline, Tab never leaves the editor: with nothing under it, say so (Emacs: EMPTY ENTRY)
        swallow(e);
        let Some(next) = pane.folds.with_untracked(|f| folding::cycle(&page, pane.fmt(), line, f)) else {
            pane.ctx.notify_t("fold_empty", "");
            return;
        };
        next
    } else {
        let Some(next) = pane.folds.with_untracked(|f| folding::cycle_element(&page, pane.fmt(), line, f)) else { return };
        swallow(e);
        next
    };
    // The caret stays on the line
    pane.focus_pos.set(Some(pos.min(page[line..].iter().position(|&c| c == '\n').map_or(page.len(), |p| line + p))));
    pane.folds.set(next);
}

/// The whole page folds / unfolds: OVERVIEW, CONTENTS, SHOW ALL (org-mode's
/// `org-global-cycle`). False when the page has no headline.
pub(super) fn global_cycle(pane: Pane, el: &Textarea) -> bool {
    let page: Vec<char> = pane.content.with_untracked(|c| c.chars().collect());
    let Some((shown, folds)) = pane.folds.with_untracked(|f| folding::global_cycle(&page, pane.fmt(), f)) else { return false };
    // A caret now hidden goes to the end of its headline
    let pos = pane.page_pos(el, caret(el));
    let pos = folds.iter().find(|&&(a, b)| a < pos && pos <= b).map_or(pos, |&(a, _)| a);
    pane.focus_pos.set(Some(pos));
    pane.folds.set(folds);
    pane.ctx.notify_t(match shown {
        folding::Global::Overview => "fold_overview",
        folding::Global::Contents => "fold_contents",
        folding::Global::ShowAll => "fold_show_all",
    }, "");
    true
}

/// On a headline, the TODO keyword goes to its next / previous state (org-mode
/// Shift+arrows), when TODO keywords are on.
pub(super) fn todo_cycle(pane: Pane, e: &web_sys::KeyboardEvent, el: &Textarea, action: EditorAction) {
    let Some(keywords) = pane.todo_keywords.get_untracked() else { return };
    let page = pane.content.get_untracked();
    let pos = pane.page_pos(el, get_pos(el).1);
    let line = page.chars().take(pos).filter(|&c| c == '\n').count();
    let forward = action == EditorAction::TodoNext;
    // Done, a repeated headline goes to its next dates, another gets its CLOSED date
    let Some(edits) = agenda::cycle_todo(&page, line, &keywords, pane.fmt(), forward, &crate::actions::now()) else { return };
    swallow(e);
    page_edits(pane, el, &edits);
}

/// The `SCHEDULED:` or `DEADLINE:` date of the headline of the caret, asked
/// (org-mode's `C-c C-s`, `C-c C-d`): `+3d`, `ven`, `2026-12-01 14:30`; `-` takes it away.
pub(super) fn plan(pane: Pane, el: &Textarea, action: EditorAction) {
    let ctx = pane.ctx;
    let (key, when, label) = if action == EditorAction::Schedule {
        (agenda::SCHEDULED, agenda::When::Scheduled, "plan_scheduled")
    } else {
        (agenda::DEADLINE, agenda::When::Deadline, "plan_deadline")
    };
    let page = pane.content.get_untracked();
    let pos = pane.page_pos(el, get_pos(el).1);
    let line = page.chars().take(pos).filter(|&c| c == '\n').count();
    let Some(head) = agenda::headline_of(&page, line, pane.fmt()) else { return ctx.notify_t("plan_no_headline", "") };
    let typed = agenda::entries(&page, pane.todo_keywords.get_untracked().as_ref(), pane.fmt()).iter()
        .find(|e| e.line == head).map(|e| agenda::typed_date(e, when)).unwrap_or_default();
    ctx.ask_text(t(label, ctx.lang.get_untracked()).to_string(), typed, vec![], move |input| {
        let Some(el) = pane.area.get_untracked() else { return };
        let page = pane.content.get_untracked();
        match agenda::plan_typed(&page, head, pane.fmt(), key, &input, crate::actions::today()) {
            Ok(Some(edit)) => page_edits(pane, &el, &[edit]),
            Ok(None) => {}
            Err(_) => ctx.notify_t("not_a_date", &input),
        }
        pane.focus();
    });
}

/// The edits of the agenda (page positions, in the order of the page) made as one
/// change; the caret and the folds move with them.
pub(super) fn page_edits(pane: Pane, el: &Textarea, edits: &[agenda::Edit]) {
    let (Some(first), Some(last)) = (edits.first(), edits.last()) else { return };
    let page: Vec<char> = pane.content.with_untracked(|c| c.chars().collect());
    let (a, b) = (first.a, last.b.min(page.len()));
    let span: String = page[a..b].iter().collect();
    let local: Vec<agenda::Edit> = edits.iter().map(|e| agenda::Edit { a: e.a - a, b: e.b - a, text: e.text.clone() }).collect();
    let caret = agenda::moved(pane.page_pos(el, get_pos(el).1), edits);
    let folds = pane.folds.with_untracked(|f| f.iter().map(|&(x, y)| (agenda::moved(x, edits), agenda::moved(y, edits))).collect());
    replace_lines(pane, el, &page, a, b, &agenda::apply(&span, &local), (caret, caret), Some(folds));
    pane.track_caret(el);
}

/// On a headline, the headline and its sub-headlines go one level up / down
/// (org-mode M-S-<left> / M-S-<right>).
pub(super) fn shift_subtree(pane: Pane, e: &web_sys::KeyboardEvent, el: &Textarea, action: EditorAction) {
    let (_, end) = get_pos(el);
    let pos = pane.page_pos(el, end);
    let page: Vec<char> = pane.content.with_untracked(|c| c.chars().collect());
    let demote = action == EditorAction::DemoteSubtree;
    let lines = match folding::shift_subtree(&page, pane.fmt(), line_start(&page, pos), demote) {
        Ok(lines) => lines,
        Err(folding::ShiftError::NotHeadline) => return,
        Err(folding::ShiftError::TopLevel) => {
            swallow(e);
            pane.ctx.notify_t("promote_top_level", "");
            return;
        }
    };
    swallow(e);
    let folded = pane.folds.get_untracked();
    if !lines.iter().any(|&l| folded.iter().any(|&(a, b)| a < l && l <= b)) {
        // All the headlines are shown: edit the textarea, so the change can be undone
        let at: Vec<usize> = lines.iter().map(|&l| pane.view_pos(l)).collect();
        let mark = pane.fmt().heading_mark().to_string();
        for &p in at.iter().rev() {
            if demote { splice(el, p, p, &mark); } else { splice(el, p, p + 1, ""); }
        }
        set_cursor(el, folding::caret_after_shift(end, &at, demote));
        pane.commit(el);
        pane.track_caret(el);
    } else {
        // Some are hidden in a fold: change the page, the folds keep their place
        let (out, moved) = folding::shift_page(&page, pane.fmt(), &lines, demote, &folded);
        pane.folds.set(moved);
        pane.set_page(out, folding::caret_after_shift(pos, &lines, demote));
    }
}

/// Move the line of the caret, or the lines of the selection, one line up or down.
/// On a headline, its whole subtree goes past its sibling's (org-mode's `M-<up>`).
pub(super) fn move_line(pane: Pane, el: &Textarea, up: bool) {
    let (start, end) = get_pos(el);
    let (start, end) = (pane.page_pos(el, start), pane.page_pos(el, end));
    let page: Vec<char> = pane.content.with_untracked(|c| c.chars().collect());
    let line = line_start(&page, start);
    if line_start(&page, end) == line && folding::is_headline(&page, pane.fmt(), line) {
        let Some(s) = folding::move_subtree(&page, pane.fmt(), line, up) else { return };
        // The folds inside go with their text
        let folds = pane.folds.with_untracked(|f| s.moved_folds(f));
        replace_lines(pane, el, &page, s.a, s.b, &s.text(&page), (s.pos(start), s.pos(end)), Some(folds));
        return;
    }
    let Some(m) = edit::move_line(&page, start, end, up) else { return };
    // Not across a fold: the hidden text would be moved with no sight of it
    if pane.folds.with_untracked(|f| f.iter().any(|&(a, b)| a < m.b && m.a <= b)) { return; }
    replace_lines(pane, el, &page, m.a, m.b, &m.text, m.sel, None);
}

/// The page range `a..b` becomes `text`, the selection `sel` (page positions), and
/// the folds `folds` when they change.
#[allow(clippy::too_many_arguments)]
pub(super) fn replace_lines(pane: Pane, el: &Textarea, page: &[char], a: usize, b: usize, text: &str, sel: (usize, usize), folds: Option<Vec<(usize, usize)>>) {
    let (va, vb) = (pane.view_pos(a), pane.view_pos(b));
    let shown: String = el.value().chars().skip(va).take(vb.saturating_sub(va)).collect();
    let page_span: String = page[a..b].iter().collect();
    if shown == page_span {
        // The lines are shown as they are: edit the textarea, so the move can be undone
        splice(el, va, vb, text);
        set_selection(el, pane.view_pos(sel.0), pane.view_pos(sel.1));
        pane.commit(el);
        pane.track_caret(el);
    } else {
        // A table drawn or a fold among them: change the page
        if let Some(f) = folds { pane.folds.set(f); }
        let (before, after): (String, String) = (page[..a].iter().collect(), page[b..].iter().collect());
        pane.set_page(format!("{before}{text}{after}"), sel.1);
    }
}

/// The page range `a..b` becomes `text`, the caret at the page position `caret`.
/// Nothing when hidden text of a fold is in the range: it would change out of sight.
pub(super) fn replace_page(pane: Pane, el: &Textarea, page: &[char], a: usize, b: usize, text: &str, caret: usize) {
    let folds = pane.folds.get_untracked();
    if folds.iter().any(|&(x, y)| x < b && y > a) { return; }
    let (va, vb) = (pane.view_pos(a), pane.view_pos(b));
    let shown: String = el.value().chars().skip(va).take(vb.saturating_sub(va)).collect();
    if shown == page[a..b].iter().collect::<String>() {
        // Shown as it is: the textarea is edited, its undo stays right
        splice(el, va, vb, text);
        set_cursor(el, va + caret - a);
        pane.commit(el);
        pane.track_caret(el);
    } else {
        // Drawn otherwise (indented under a headline…): the page gets the text
        let grown = text.chars().count() as isize - (b - a) as isize;
        pane.folds.set(folds.iter().map(|&(x, y)| if x >= b {
            (x.saturating_add_signed(grown), y.saturating_add_signed(grown))
        } else { (x, y) }).collect());
        let (before, after): (String, String) = (page[..a].iter().collect(), page[b..].iter().collect());
        pane.set_page(format!("{before}{text}{after}"), caret);
    }
}

/// `M-z c`: kill up to and including the next `c`.
pub(super) fn zap_to(pane: Pane, el: &Textarea, c: char) {
    let chars: Vec<char> = el.value().chars().collect();
    let at = caret(el);
    let Some(b) = edit::zap_end(&chars, at, c) else {
        pane.ctx.notify_t("zap_not_found", &c.to_string());
        return;
    };
    let (pa, pb) = (pane.page_pos(el, at), pane.page_pos(el, b));
    let len = pane.content.with_untracked(|t| t.chars().count());
    crate::kill_ring::kill_range(pane.ctx, el, pane.page_text(el, at, b), pa, pb, len);
    let cursor = splice(el, at, b, "");
    set_cursor(el, cursor);
    pane.commit(el);
}
