//! Editing the table: the cell being typed in, undo and redo, rows and columns, sorting, clearing.

use super::*;

pub(super) const UNDO_DEPTH: usize = 200;

/// An edit gesture, and its inverse: undo applies the inverse and gets the gesture back.
pub(super) enum Change {
    /// Values to set: (row, column, text).
    Set(Vec<(usize, usize, String)>),
    Insert(usize, Vec<Vec<String>>),
    Remove(Range<usize>),
    /// `n` columns before `at`, with their previous cells (none: empty).
    InsertCols(usize, usize, Vec<Vec<String>>),
    RemoveCols(Range<usize>),
    /// The columns will start at `to` (counted once removed).
    MoveCols(Range<usize>, usize),
    /// The rows of a window starting at `start`: row `i` becomes the old `perm[i]`.
    Reorder(usize, Vec<usize>),
}

impl Sheet {
    /// A change: the disk gets it after a short delay, or right away on `flush`.
    pub(super) fn changed(&mut self, cx: &mut Context<Self>) {
        self.dirty = true;
        self.generation += 1;
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SAVE_DELAY).await;
            this.update(cx, |this, cx| {
                if this.generation == generation {
                    this.save(cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// Number of gestures that can be undone.
    #[cfg(test)]
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    pub(super) fn begin_edit(&mut self, text: Option<&str>, cx: &mut Context<Self>) {
        let (r, c) = self.cursor;
        let text = text.map_or_else(|| self.table.cell(r, c).into_owned(), str::to_string);
        self.edit = Some(Line::new(text));
        cx.notify();
    }

    /// Commits the current input.
    pub(super) fn finish(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = self.edit.take() else {
            return;
        };
        let (r, c) = self.cursor;
        if *self.table.cell(r, c) != *edit.text {
            self.commit(vec![Change::Set(vec![(r, c, edit.text)])], cx);
        }
        cx.notify();
    }

    pub(super) fn typed(&mut self, text: &str, cx: &mut Context<Self>) {
        match &mut self.edit {
            Some(edit) => edit.insert(text),
            // Typing on a cell replaces its content, as in a spreadsheet.
            None if !text.chars().all(char::is_control) => self.begin_edit(Some(text), cx),
            None => {}
        }
        cx.notify();
    }

    pub(super) fn apply(&mut self, change: Change) -> Change {
        match change {
            Change::Set(cells) => {
                let before = cells.iter().map(|&(r, c, _)| (r, c, self.table.cell(r, c).into_owned())).collect();
                for (r, c, value) in cells {
                    self.widen(c, value.chars().count());
                    self.table.set_cell(r, c, value);
                }
                Change::Set(before)
            }
            Change::Insert(at, rows) => {
                let n = rows.len();
                for (i, row) in rows.into_iter().enumerate() {
                    self.table.insert_row(at + i);
                    for (c, value) in row.into_iter().enumerate().filter(|(_, v)| !v.is_empty()) {
                        self.table.set_cell(at + i, c, value);
                    }
                }
                Change::Remove(at..at + n)
            }
            Change::Remove(range) => {
                let rows = range.clone().map(|r| self.table.row(r).into_iter().map(|c| c.into_owned()).collect()).collect();
                let at = range.start;
                self.table.remove_rows(range);
                Change::Insert(at, rows)
            }
            // Widths are measured again: `settle` sees they no longer fit the table.
            Change::InsertCols(at, n, saved) => {
                self.table.insert_cols(at, n, &saved);
                self.widths.clear();
                Change::RemoveCols(at..at + n)
            }
            Change::RemoveCols(range) => {
                let (n, at) = (range.len(), range.start);
                let saved = self.table.remove_cols(range);
                self.widths.clear();
                Change::InsertCols(at, n, saved)
            }
            Change::MoveCols(range, to) => {
                let back = Change::MoveCols(to..to + range.len(), range.start);
                self.table.move_cols(range, to);
                self.widths.clear();
                back
            }
            Change::Reorder(start, perm) => {
                self.table.reorder(start, &perm);
                let mut back = vec![0; perm.len()];
                for (i, &from) in perm.iter().enumerate() {
                    back[from] = i;
                }
                Change::Reorder(start, back)
            }
        }
    }

    /// Applies gestures together: a single "undo" reverts them.
    pub(super) fn commit(&mut self, changes: Vec<Change>, cx: &mut Context<Self>) {
        let mut back: Vec<Change> = changes.into_iter().map(|c| self.apply(c)).collect();
        back.reverse();
        self.undo.push(back);
        if self.undo.len() > UNDO_DEPTH {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.settle(cx);
    }

    pub(super) fn restore(&mut self, redo: bool, cx: &mut Context<Self>) {
        self.finish(cx);
        let Some(batch) = (if redo { self.redo.pop() } else { self.undo.pop() }) else {
            return;
        };
        let mut back: Vec<Change> = batch.into_iter().map(|c| self.apply(c)).collect();
        back.reverse();
        if redo { &mut self.undo } else { &mut self.redo }.push(back);
        self.settle(cx);
    }

    pub(super) fn settle(&mut self, cx: &mut Context<Self>) {
        if self.widths.len() != self.cols() {
            self.measure();
        }
        self.clamp();
        self.show(self.cursor);
        self.changed(cx);
        cx.notify();
    }

    /// Clears the cells of the selection.
    pub(super) fn clear(&mut self, cx: &mut Context<Self>) {
        // Two selections can overlap: each cell only once.
        let all: std::collections::BTreeSet<(usize, usize)> =
            self.selections().into_iter().flat_map(|(rows, cols)| rows.flat_map(move |r| cols.clone().map(move |c| (r, c)))).collect();
        let cells: Vec<_> = all
            .into_iter()
            .filter(|&(r, c)| !self.table.cell(r, c).is_empty())
            .map(|(r, c)| (r, c, String::new()))
            .collect();
        if !cells.is_empty() {
            self.commit(vec![Change::Set(cells)], cx);
        }
    }

    pub(super) fn insert_rows(&mut self, below: bool, cx: &mut Context<Self>) {
        self.finish(cx);
        self.more.clear();
        let (rows, cols) = self.selection();
        // Whole columns: as many empty columns, to their right or left, selected in turn.
        if self.whole_cols() {
            let at = if below { cols.end } else { cols.start };
            self.commit(vec![Change::InsertCols(at, cols.len(), Vec::new())], cx);
            (self.anchor, self.cursor) = ((self.last_row(), at + cols.len() - 1), (0, at));
            return cx.notify();
        }
        let at = if below { rows.end } else { rows.start };
        self.commit(vec![Change::Insert(at, vec![Vec::new()])], cx);
        self.select_to((at, self.cursor.1), false, cx);
    }

    /// All selections take columns top to bottom, without taking the whole table:
    /// insert and delete then act on columns, not rows.
    pub(super) fn whole_cols(&self) -> bool {
        let (tall, wide) = (self.table.rows(), self.cols());
        let all = self.selections();
        all.iter().all(|(rows, _)| rows.start == 0 && rows.end >= tall) && !all.iter().any(|(_, cols)| cols.start == 0 && cols.end >= wide)
    }

    /// The whole rows or columns of the selection: Shift+Space and Ctrl+Space, as in a spreadsheet.
    pub(super) fn select_whole(&mut self, rows: bool, cx: &mut Context<Self>) {
        self.finish(cx);
        self.more.clear();
        let (rs, cs) = self.selection();
        (self.anchor, self.cursor) = match rows {
            true => ((rs.end - 1, self.cols() - 1), (rs.start, 0)),
            false => ((self.last_row(), cs.end - 1), (0, cs.start)),
        };
        cx.notify();
    }

    /// Delete what is selected: the columns when they are taken whole, otherwise the rows.
    pub(super) fn delete_span(&mut self, cx: &mut Context<Self>) {
        if !self.whole_cols() {
            return self.delete_rows(cx);
        }
        self.finish(cx);
        let cols: std::collections::BTreeSet<usize> = self.selections().into_iter().flat_map(|(_, cols)| cols).collect();
        let mut ranges: Vec<Range<usize>> = Vec::new();
        for c in cols {
            match ranges.last_mut() {
                Some(last) if last.end == c => last.end = c + 1,
                _ => ranges.push(c..c + 1),
            }
        }
        let first = ranges[0].start;
        self.more.clear();
        // Right to left: each removal leaves in place the columns still to remove.
        self.commit(ranges.into_iter().rev().map(Change::RemoveCols).collect(), cx);
        let c = first.min(self.cols() - 1);
        (self.anchor, self.cursor) = ((self.last_row(), c), (0, c));
        cx.notify();
    }

    /// A table gesture, as in a note: on the rows and columns of the selection. Columns
    /// and sorting read the whole file, and a gesture is undone in one go.
    pub fn table_op(&mut self, op: TableOp, cx: &mut Context<Self>) {
        self.finish(cx);
        self.more.clear();
        let (rows, cols) = self.selection();
        let (first, tall, wide) = (self.first(), self.table.rows(), self.cols());
        let (r, c) = self.cursor;
        match op {
            TableOp::RowAbove => self.insert_rows(false, cx),
            TableOp::RowBelow => self.insert_rows(true, cx),
            TableOp::DeleteRow => self.delete_rows(cx),
            TableOp::ColLeft | TableOp::ColRight => {
                let at = if op == TableOp::ColLeft { cols.start } else { cols.end };
                self.commit(vec![Change::InsertCols(at, 1, Vec::new())], cx);
                (self.anchor, self.cursor) = ((r, at), (r, at));
            }
            TableOp::DuplicateRow => {
                let copies = rows.clone().map(|r| self.table.row(r).into_iter().map(|c| c.into_owned()).collect()).collect();
                self.commit(vec![Change::Insert(rows.end, copies)], cx);
                self.select_to((rows.end, c), false, cx);
            }
            TableOp::DuplicateCol => {
                let saved = (0..tall).map(|r| self.table.cells(r, cols.clone()).into_iter().map(|c| c.into_owned()).collect()).collect();
                self.commit(vec![Change::InsertCols(cols.end, cols.len(), saved)], cx);
                (self.anchor, self.cursor) = ((r, cols.end), (r, cols.end));
            }
            // Only the body moves: the header stays at the top, and nothing leaves the table.
            TableOp::MoveRowUp if rows.start > first => {
                let perm: Vec<usize> = (1..=rows.len()).chain([0]).collect();
                self.commit(vec![Change::Reorder(rows.start - 1, perm)], cx);
                (self.anchor, self.cursor) = ((self.anchor.0 - 1, self.anchor.1), (r - 1, c));
                self.settle(cx);
            }
            TableOp::MoveRowDown if rows.end < tall => {
                let perm: Vec<usize> = [rows.len()].into_iter().chain(0..rows.len()).collect();
                self.commit(vec![Change::Reorder(rows.start, perm)], cx);
                (self.anchor, self.cursor) = ((self.anchor.0 + 1, self.anchor.1), (r + 1, c));
                self.settle(cx);
            }
            TableOp::MoveColLeft if cols.start > 0 => {
                self.commit(vec![Change::MoveCols(cols.clone(), cols.start - 1)], cx);
                (self.anchor, self.cursor) = ((self.anchor.0, self.anchor.1 - 1), (r, c - 1));
                self.settle(cx);
            }
            TableOp::MoveColRight if cols.end < wide => {
                self.commit(vec![Change::MoveCols(cols.clone(), cols.start + 1)], cx);
                (self.anchor, self.cursor) = ((self.anchor.0, self.anchor.1 + 1), (r, c + 1));
                self.settle(cx);
            }
            // Removing all columns would leave no table.
            TableOp::DeleteCol if cols.len() < wide => self.commit(vec![Change::RemoveCols(cols)], cx),
            TableOp::SortAsc | TableOp::SortDesc => self.sort(c, op == TableOp::SortAsc, cx),
            _ => {}
        }
    }

    /// The rows under the header, sorted on column `col`: natural sort, empty cells last in
    /// both directions, equal rows in their previous order. Rows are not copied.
    pub(super) fn sort(&mut self, col: usize, ascending: bool, cx: &mut Context<Self>) {
        let (first, tall) = (self.first(), self.table.rows());
        let keys: Vec<String> = (first..tall).map(|r| self.table.cell(r, col).into_owned()).collect();
        let mut perm: Vec<usize> = (0..keys.len()).collect();
        perm.sort_by(|&a, &b| {
            match (keys[a].trim().is_empty(), keys[b].trim().is_empty()) {
                (true, true) => std::cmp::Ordering::Equal,
                (true, false) => std::cmp::Ordering::Greater,
                (false, true) => std::cmp::Ordering::Less,
                _ => {
                    let order = crate::logic::markdown::cell_order(&keys[a], &keys[b]);
                    if ascending { order } else { order.reverse() }
                }
            }
        });
        if perm.iter().enumerate().any(|(i, &p)| i != p) {
            self.commit(vec![Change::Reorder(first, perm)], cx);
        }
    }

    pub(super) fn delete_rows(&mut self, cx: &mut Context<Self>) {
        self.finish(cx);
        // The rows of all selections, grouped into ranges and removed from bottom to
        // top: each removal leaves in place the rows still to remove.
        let rows: std::collections::BTreeSet<usize> = self.selections().into_iter().flat_map(|(rows, _)| rows).collect();
        let mut ranges: Vec<Range<usize>> = Vec::new();
        for r in rows.iter().copied() {
            match ranges.last_mut() {
                Some(last) if last.end == r => last.end = r + 1,
                _ => ranges.push(r..r + 1),
            }
        }
        let mut changes: Vec<Change> = ranges.into_iter().rev().map(Change::Remove).collect();
        if rows.len() >= self.table.rows() {
            changes.push(Change::Insert(0, vec![Vec::new()]));
        }
        self.more.clear();
        self.commit(changes, cx);
    }
}
