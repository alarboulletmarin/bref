//! Links and comments: making a link from an address, following it, commenting a selection.

use super::*;

impl Editor {
    /// Address held by the clipboard, if that is all it holds.
    pub(super) fn copied_url(cx: &mut Context<Self>) -> Option<String> {
        let text = cx.read_from_clipboard()?.text()?;
        md::is_url(text.trim()).then(|| text.trim().to_string())
    }

    /// The selection, if it can become a link's text: on a single line, outside
    /// code, and not already an address.
    pub(super) fn linkable(&self) -> bool {
        let text = &self.content[self.sel.clone()];
        // Over several cells, a link would mean nothing: it would hold a `|`.
        let cells = self.selected_cells().is_some() || self.pick_cells().is_some_and(|c| c.len() > 1);
        !text.contains('\n') && !cells && !md::is_url(text.trim()) && !self.in_code(self.line_range(self.sel.start).start)
    }

    /// Makes a link `[text](address)`. On an existing link, its address is selected,
    /// ready to be replaced. Otherwise the selection becomes the link text, to the clipboard's
    /// address if it holds one; the cursor waits for what is missing, the address between the
    /// parentheses or the text between the brackets.
    pub(super) fn link(&mut self, cx: &mut Context<Self>) {
        let lr = self.line_range(self.sel.start);
        let (line, col) = (&self.content[lr.clone()], self.sel.start - lr.start);
        let on = md::links(line).into_iter().find(|(r, _)| (r.start..=r.end).contains(&col));
        if let Some((r, _)) = &on
            && let Some((_, url)) = md::web_link(&line[r.start..])
        {
            let at = lr.start + r.start;
            self.move_to(at + url.start, cx);
            return self.select_to(at + url.end, cx);
        }
        // On a bare address: it becomes the link.
        if let Some((r, md::Link::Url(_))) = on.filter(|_| self.sel.is_empty()) {
            self.sel = lr.start + r.start..lr.start + r.end;
        }
        let sel = self.sel.clone();
        // A selected address keeps its place: it lacks its text.
        let picked = self.content[sel.clone()].to_string();
        if !md::is_url(picked.trim()) && !self.linkable() {
            return;
        }
        let (text, url) = match md::is_url(picked.trim()) {
            true => (String::new(), picked.trim().to_string()),
            false => (picked, Self::copied_url(cx).unwrap_or_default()),
        };
        self.push_undo(true);
        self.edit(sel.clone(), &format!("[{text}]({url})"), cx);
        let at = match (text.is_empty(), url.is_empty()) {
            (true, _) => 1,
            (false, true) => text.len() + 3,
            (false, false) => text.len() + url.len() + 4,
        };
        self.move_to(sel.start + at, cx);
    }

    /// Replaces all the text, like a keystroke: one undo step, the note needs saving.
    pub fn rewrite(&mut self, text: &str, cx: &mut Context<Self>) {
        self.more.clear();
        // A single step: `splice` would add a second one for a multi-line text.
        self.push_undo(true);
        self.held = true;
        self.splice(0..self.content.len(), text, cx);
        self.held = false;
    }

    /// Resolves the comment starting at byte `at`.
    pub fn resolve_comment(&mut self, at: usize, cx: &mut Context<Self>) {
        self.move_to(self.clamp(at), cx);
        self.comment(cx);
    }

    /// Comments the selection, or the line without one: `{==text==}{>>…<<}`, the cursor in the
    /// comment to write. On a comment, resolves it: the markup goes, the text stays. In a table,
    /// a comment is on the text of a cell, and one per cell the selection covers; handles that took
    /// a row, a column or the table comment it on a line under the table.
    pub(super) fn comment(&mut self, cx: &mut Context<Self>) {
        self.aligned(cx, |this, cx| this.comment_here(cx));
    }

    fn comment_here(&mut self, cx: &mut Context<Self>) {
        let lr = self.line_range(self.sel.start);
        let (line, col) = (self.content[lr.clone()].to_string(), self.sel.start - lr.start);
        if let Some((whole, noted, _)) = md::comments(&line).into_iter().find(|(whole, ..)| (whole.start..=whole.end).contains(&col)) {
            self.push_undo(true);
            // A comment alone on its line takes the line with it.
            if noted.start == whole.start && line.trim() == &line[whole.clone()] {
                return self.remove_line(lr, cx);
            }
            let kept = line[noted].to_string();
            return self.edit(lr.start + whole.start..lr.start + whole.end, &kept, cx);
        }
        if self.in_code(lr.start) {
            return;
        }
        if self.pick_cells().is_some_and(|cells| cells.len() > 1) {
            return self.comment_scope(cx);
        }
        // ponytail: a comment on an empty cell is refused, there is nothing to highlight. Allow it
        // (`{====}{>>…<<}`) if the corner mark of the issue gets drawn.
        if let Some(cells) = self.comment_cells(&lr) {
            let Some(first) = cells.first().map(|c| c.start) else { return };
            let len = cells[0].len();
            self.push_undo(true);
            for cell in cells.iter().rev() {
                let text = self.content[cell.clone()].to_string();
                self.edit(cell.clone(), &format!("{{=={text}==}}{{>><<}}"), cx);
            }
            return self.move_to(first + len + 9, cx);
        }
        let marker = md::classify(&line, false).1;
        let sel = if self.sel.is_empty() { lr.start + marker..lr.end } else { self.sel.clone() };
        if sel.is_empty() || sel.end > lr.end {
            return;
        }
        let text = self.content[sel.clone()].to_string();
        self.push_undo(true);
        self.edit(sel.clone(), &format!("{{=={text}==}}{{>><<}}"), cx);
        self.move_to(sel.start + text.len() + 9, cx);
    }

    /// Byte `at` is in the comment of a table cell: a `|` typed there is written `\|`, or it would split the cell.
    pub(super) fn in_cell_comment(&self, at: usize) -> bool {
        let lr = self.line_range(at);
        self.is_table_line(&lr) && md::comments(&self.content[lr.clone()]).iter().any(|(_, _, said)| (said.start..=said.end).contains(&(at - lr.start)))
    }

    /// What a comment is on when the selection is in a table, in reading order: the text of the
    /// cell the cursor is in, of the one a handle took, or of each cell a selection touches (a `|`
    /// is never inside a comment). `None` outside a table; empty if no cell has text.
    fn comment_cells(&self, line: &Range<usize>) -> Option<Vec<Range<usize>>> {
        if let Some(cells) = self.pick_cells() {
            return Some(cells.into_iter().filter_map(|c| self.trimmed(c.start, c.end)).collect());
        }
        if self.sel.is_empty() {
            if !self.is_table_line(line) {
                return None;
            }
            let (cells, k) = self.table_cells(self.sel.start).unzip();
            return Some(cells.zip(k).and_then(|(cells, k)| cells.get(k).cloned()).and_then(|c| self.trimmed(c.start, c.end)).into_iter().collect());
        }
        let parts = self.mark_parts();
        parts.iter().any(|p| self.is_table_line(&self.line_range(p.start))).then_some(parts)
    }

    /// The taken row, column or table is commented on a line of its own under the table, as
    /// `{>>Row "Léa": …<<}`: named by its content, so that a sort leaves the comment where it belongs.
    fn comment_scope(&mut self, cx: &mut Context<Self>) {
        let Some((range, rows, _)) = self.table_at(self.sel.start) else { return };
        let Some(pick) = self.live_pick(range.start).cloned() else { return };
        let visible = md::visible_rows(&rows);
        let (names, cols) = md::scopes(&rows);
        let name = if pick.rows.len() >= visible && pick.cols.len() >= rows[0].len() {
            "Table".to_string()
        } else if pick.rows.len() >= visible {
            cols[pick.cols.start].clone()
        } else {
            names[pick.rows.start].clone()
        };
        let head = format!("{{>>{name}: ");
        // A blank line keeps other apps from reading the comment as one more row.
        let after = &self.content[range.end..];
        let tail = if after.is_empty() || after == "\n" || after.starts_with("\n\n") { "" } else { "\n" };
        self.push_undo(true);
        self.edit(range.end..range.end, &format!("\n\n{head}<<}}{tail}"), cx);
        self.move_to(range.end + 2 + head.len(), cx);
    }

    /// Deletes the line `lr` and the blank line before it, if any: the text around closes up.
    fn remove_line(&mut self, lr: Range<usize>, cx: &mut Context<Self>) {
        let at = self.drop_line(lr, cx);
        self.move_to(at, cx);
    }

    /// Like `remove_line`, the cursor left to follow the text; where the text closed up.
    fn drop_line(&mut self, lr: Range<usize>, cx: &mut Context<Self>) -> usize {
        let before = self.content[..lr.start].bytes().rev().take(2).take_while(|&b| b == b'\n').count();
        let after = &self.content[lr.end..];
        let range = if lr.start == 0 {
            lr.start..lr.end + after.starts_with('\n') as usize
        } else if before == 2 && !(after.is_empty() || after.starts_with("\n\n")) {
            lr.start - 1..lr.end
        } else {
            lr.start - before..lr.end
        };
        self.edit(range.clone(), "", cx);
        range.start
    }

    /// The comment lines right under a table ending at `end`: each alone on its line, a single
    /// blank line allowed before each.
    pub(super) fn comment_lines_after(&self, end: usize) -> Vec<Range<usize>> {
        let (mut found, mut at, mut blank) = (Vec::new(), end, false);
        while at < self.content.len() {
            let lr = self.line_range(at + 1);
            let line = self.content[lr.clone()].trim();
            if line.is_empty() && !blank && lr.end < self.content.len() {
                blank = true;
            } else if line.starts_with("{>>") && line.ends_with("<<}") {
                blank = false;
                found.push(lr.clone());
            } else {
                break;
            }
            at = lr.end;
        }
        found
    }

    /// Follows the rows and columns of a table that changed in the comment lines under it, in
    /// the same undo step: each is renamed after its row or column, or goes with it
    /// (see `md::retarget`).
    pub(super) fn retarget_comments(&mut self, lines: &[Range<usize>], moved: (&[Option<usize>], &[Option<usize>]), old: &md::Scopes, new: &md::Scopes, gone: bool, cx: &mut Context<Self>) {
        let texts: Vec<String> = lines.iter().map(|lr| self.content[lr.clone()].to_string()).collect();
        let edits = md::retarget(&texts.iter().map(String::as_str).collect::<Vec<_>>(), old, new, moved, gone);
        for ((lr, text), edit) in lines.iter().zip(&texts).zip(edits).rev() {
            match edit {
                None => {
                    self.drop_line(lr.clone(), cx);
                }
                Some(new) if new != *text => self.splice(lr.clone(), &new, cx),
                Some(_) => {}
            }
        }
    }

    /// Before the edit of `range` by `text`, the names that comments under the table could use,
    /// if it is a change inside a line of a table that has such comments.
    pub(super) fn scopes_before(&self, range: &Range<usize>, text: &str) -> Option<(usize, md::Scopes)> {
        let lr = self.line_range(range.start);
        if range.end > lr.end || text.contains('\n') || !self.is_table_line(&lr) {
            return None;
        }
        // Only a first cell or a header names something: typing elsewhere costs nothing.
        let first_cell = md::cells(&self.content[lr.clone()]).first().is_none_or(|c| range.start <= lr.start + c.end + 1);
        let header = lr.start == 0 || !self.is_table_line(&self.line_range(lr.start - 1));
        if !first_cell && !header {
            return None;
        }
        self.name_guard(range.start)
    }

    /// The table around `at`, if comments sit under it, and the names they could use: to give
    /// back to `rename_scopes` once an edit that does not go through `splice` is done.
    pub(super) fn name_guard(&self, at: usize) -> Option<(usize, md::Scopes)> {
        if !self.content.contains("{>>") {
            return None;
        }
        let (table, rows, _) = self.table_at(at)?;
        (!self.comment_lines_after(table.end).is_empty()).then(|| (table.start, md::scopes(&rows)))
    }

    /// After an edit in a cell: a row or a column that took another name carries its comments along.
    pub(super) fn rename_scopes(&mut self, table: usize, old: md::Scopes, cx: &mut Context<Self>) {
        let Some((range, rows, _)) = self.table_at(table) else { return };
        let new = md::scopes(&rows);
        if old == new || old.0.len() != new.0.len() || old.1.len() != new.1.len() {
            return;
        }
        let lines = self.comment_lines_after(range.end);
        // Same undo step as the edit that renamed.
        let held = std::mem::replace(&mut self.held, true);
        let same = |n: usize| (0..n).map(Some).collect::<Vec<_>>();
        self.retarget_comments(&lines, (&same(old.0.len()), &same(old.1.len())), &old, &new, false, cx);
        self.held = held;
    }
}
