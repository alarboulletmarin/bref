//! Markdown tables in the editor: cells, moving between them, rows and columns, alignment, the grid picker.

use super::*;

/// The table around a byte: its range in the text, its rows, and the cell (row, column) of that byte.
pub(super) type TableAt = (Range<usize>, Vec<Vec<String>>, (usize, usize));

/// What a test reads of the table on screen: tallest row, rows, frame, text width and view.
#[cfg(test)]
pub(super) type TableState = (Pixels, i32, Bounds<Pixels>, Pixels, Bounds<Pixels>);

/// Room kept under a table for the button that adds a row to it, and
/// thickness of that button.
pub(super) const TABLE_GAP: Pixels = px(18.);

pub(super) const PLUS: Pixels = px(12.);

/// Table picker: offered columns and rows, grid step.
pub(super) const GRID: usize = 8;

pub(super) const GRID_STEP: Pixels = px(20.);

/// Largest table that can be requested, from the keyboard: (columns, rows).
pub(super) const GRID_MAX: (usize, usize) = (20, 99);

/// `/name` command: (name, English label, French label, text put before
/// the cursor, text put after).
/// A gesture on the table the cursor is in: it applies to the rows and columns the
/// selection touches.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TableOp {
    RowAbove,
    RowBelow,
    ColLeft,
    ColRight,
    DuplicateRow,
    DuplicateCol,
    MoveRowUp,
    MoveRowDown,
    MoveColLeft,
    MoveColRight,
    DeleteRow,
    DeleteCol,
    DeleteTable,
    SortAsc,
    SortDesc,
    Transpose,
}

impl TableOp {
    pub const ALL: [TableOp; 16] = [
        TableOp::RowAbove,
        TableOp::RowBelow,
        TableOp::DuplicateRow,
        TableOp::MoveRowUp,
        TableOp::MoveRowDown,
        TableOp::DeleteRow,
        TableOp::ColLeft,
        TableOp::ColRight,
        TableOp::DuplicateCol,
        TableOp::MoveColLeft,
        TableOp::MoveColRight,
        TableOp::DeleteCol,
        TableOp::SortAsc,
        TableOp::SortDesc,
        TableOp::Transpose,
        TableOp::DeleteTable,
    ];

    pub fn label(self) -> &'static str {
        match self {
            TableOp::RowAbove => tr("Insert a row above", "Insérer une ligne au-dessus"),
            TableOp::RowBelow => tr("Insert a row below", "Insérer une ligne en dessous"),
            TableOp::ColLeft => tr("Insert a column on the left", "Insérer une colonne à gauche"),
            TableOp::ColRight => tr("Insert a column on the right", "Insérer une colonne à droite"),
            TableOp::DuplicateRow => tr("Duplicate the row", "Dupliquer la ligne"),
            TableOp::DuplicateCol => tr("Duplicate the column", "Dupliquer la colonne"),
            TableOp::MoveRowUp => tr("Move the row up", "Monter la ligne"),
            TableOp::MoveRowDown => tr("Move the row down", "Descendre la ligne"),
            TableOp::MoveColLeft => tr("Move the column left", "Décaler la colonne à gauche"),
            TableOp::MoveColRight => tr("Move the column right", "Décaler la colonne à droite"),
            TableOp::DeleteRow => tr("Delete the row", "Supprimer la ligne"),
            TableOp::DeleteCol => tr("Delete the column", "Supprimer la colonne"),
            TableOp::DeleteTable => tr("Delete the table", "Supprimer le tableau"),
            TableOp::SortAsc => tr("Sort the rows by this column, A to Z", "Trier les lignes sur cette colonne, de A à Z"),
            TableOp::SortDesc => tr("Sort the rows by this column, Z to A", "Trier les lignes sur cette colonne, de Z à A"),
            TableOp::Transpose => tr("Swap rows and columns", "Échanger lignes et colonnes"),
        }
    }

    /// A CSV's sheet can do it: everything but deleting the table and swapping rows and columns.
    pub fn in_sheet(self) -> bool {
        !matches!(self, TableOp::DeleteTable | TableOp::Transpose)
    }

    /// The action that does the same from the keyboard.
    pub fn action(self) -> Box<dyn gpui::Action> {
        match self {
            TableOp::RowAbove => Box::new(TableRowAbove),
            TableOp::RowBelow => Box::new(TableRowBelow),
            TableOp::ColLeft => Box::new(TableColLeft),
            TableOp::ColRight => Box::new(TableColRight),
            TableOp::DuplicateRow => Box::new(TableDuplicateRow),
            TableOp::DuplicateCol => Box::new(TableDuplicateCol),
            TableOp::MoveRowUp => Box::new(TableMoveRowUp),
            TableOp::MoveRowDown => Box::new(TableMoveRowDown),
            TableOp::MoveColLeft => Box::new(TableMoveColLeft),
            TableOp::MoveColRight => Box::new(TableMoveColRight),
            TableOp::DeleteRow => Box::new(TableDeleteRow),
            TableOp::DeleteCol => Box::new(TableDeleteCol),
            TableOp::DeleteTable => Box::new(TableDelete),
            TableOp::SortAsc => Box::new(TableSortAsc),
            TableOp::SortDesc => Box::new(TableSortDesc),
            TableOp::Transpose => Box::new(TableTranspose),
        }
    }
}

/// Width offered to tables in a pane `pane` wide, for a text `text` wide.
pub(super) fn table_room(wide: Wide, pane: Pixels, text: Pixels) -> Pixels {
    match wide {
        Wide::Column => text,
        Wide::Centered => (pane - SIDE * 2.).max(text),
    }
}

/// How far a table `frame` wide goes past the left edge of the text.
pub(super) fn table_lead(wide: Wide, frame: Pixels, text: Pixels) -> Pixels {
    match wide {
        Wide::Centered => ((frame - text) / 2.).max(px(0.)),
        Wide::Column => px(0.),
    }
}

/// "+" buttons of a table: under it for a row, to its right for a column.
pub(super) fn plus_bars(frame: Bounds<Pixels>) -> [Bounds<Pixels>; 2] {
    [
        Bounds::new(point(frame.left(), frame.bottom() + px(3.)), size(frame.size.width, PLUS)),
        Bounds::new(point(frame.right() + px(4.), frame.top()), size(PLUS, frame.size.height)),
    ]
}

pub(super) struct TableMeta {
    /// Bytes, from its start to the end of its last line.
    pub(super) range: Range<usize>,
    /// Indices of its lines.
    pub(super) rows: Range<usize>,
    /// Width of its widest line.
    pub(super) wide: Pixels,
}

impl Editor {
    /// First table: how far it scrolled, largest number of visual lines of one
    /// of its rows, its frame, the text width and the view.
    #[cfg(test)]
    pub fn table_state(&self) -> Option<TableState> {
        let rows = self.rows.iter().filter(|r| r.kind == Kind::Table);
        let tall = rows.clone().map(Row::visual_rows).max()?;
        let frame = self.tables.first()?.1;
        let scrolled = rows.map(|r| r.dx).next()? - table_lead(self.theme.wide, frame.size.width, self.width);
        Some((scrolled, tall, frame, self.width, self.viewport))
    }

    /// Center of handle `part` of the first table.
    #[cfg(test)]
    pub fn part_center(&self, part: Part) -> Option<Point<Pixels>> {
        self.geos.first()?.parts().into_iter().find(|(p, _)| *p == part).map(|(_, zone)| zone.center())
    }

    /// Center of a table picker cell, if it is open.
    #[cfg(test)]
    pub fn grid_cell(&self, col: usize, row: usize) -> Option<Point<Pixels>> {
        self.grid?;
        let step = |n: usize| px(16.) + GRID_STEP * (n - 1) as f32;
        Some(self.popup_at(Self::grid_size())? + point(step(col), step(row)))
    }

    pub(super) fn is_table_line(&self, line: &Range<usize>) -> bool {
        self.content[line.clone()].trim_start().starts_with('|') && !self.in_code(line.start)
    }

    /// Table line before the one starting at `start`, or after it.
    pub(super) fn next_line(&self, line: &Range<usize>, down: bool) -> Option<Range<usize>> {
        if down {
            (line.end < self.content.len()).then(|| self.line_range(line.end + 1))
        } else {
            (line.start > 0).then(|| self.line_range(line.start - 1))
        }
    }

    /// The line is a table's dash line: the second one, when it has only dashes.
    pub(super) fn is_dash_row(&self, line: &Range<usize>) -> bool {
        let first = self.next_line(line, false).filter(|l| self.is_table_line(l));
        let none_before = first.as_ref().is_some_and(|f| self.next_line(f, false).is_none_or(|l| !self.is_table_line(&l)));
        let text = &self.content[line.clone()];
        none_before && md::is_dashes(&md::cells(text).into_iter().map(|r| text[r].to_string()).collect::<Vec<_>>())
    }

    /// Cells (text bytes) of the table line containing `c`, and the one
    /// we are in: `None` outside a table, and on the dash line, which shows as text.
    pub(super) fn table_cells(&self, c: usize) -> Option<(Vec<Range<usize>>, usize)> {
        let c = self.clamp(c);
        let line = self.line_range(c);
        if !self.is_table_line(&line) || self.is_dash_row(&line) {
            return None;
        }
        let text = &self.content[line.clone()];
        let cells: Vec<Range<usize>> = md::cells(text).into_iter().map(|r| line.start + r.start..line.start + r.end).collect();
        let before = md::bars(text).into_iter().filter(|&bar| line.start + bar < c).count();
        let k = before.saturating_sub(1).min(cells.len().checked_sub(1)?);
        Some((cells, k))
    }

    /// Cells of the neighboring table line, the dash line skipped.
    pub(super) fn table_neighbour(&self, c: usize, down: bool) -> Option<Vec<Range<usize>>> {
        let mut line = self.next_line(&self.line_range(self.clamp(c)), down)?;
        if self.is_table_line(&line) && self.is_dash_row(&line) {
            line = self.next_line(&line, down)?;
        }
        self.table_cells(line.start).map(|(cells, _)| cells)
    }

    /// The selection covers several cells of a table (or `|`s): the text of each
    /// cell it touches. `None` if it fits in one cell, or goes out of the table.
    pub(super) fn selected_cells(&self) -> Option<Vec<Range<usize>>> {
        self.cells_in(self.sel.clone())
    }

    /// Like `selected_cells`, for selection `sel`.
    pub(super) fn cells_in(&self, sel: Range<usize>) -> Option<Vec<Range<usize>>> {
        if sel.is_empty() {
            return None;
        }
        let (first, last) = (self.line_range(sel.start), self.line_range(sel.end));
        let (mut found, mut bars, mut line) = (Vec::new(), false, first.clone());
        loop {
            if !self.is_table_line(&line) {
                return None;
            }
            if !self.is_dash_row(&line) {
                let inside = |at: usize| (sel.start..sel.end).contains(&at);
                bars |= md::bars(&self.content[line.clone()]).into_iter().any(|bar| inside(line.start + bar));
                found.extend(self.line_cells(&line, &sel));
            }
            if line.end >= last.end {
                break;
            }
            line = self.line_range(line.end + 1);
        }
        (first != last || bars || found.len() > 1).then_some(found)
    }

    /// The text of each cell of table `line` that `sel` touches, cut to the selection.
    pub(super) fn line_cells(&self, line: &Range<usize>, sel: &Range<usize>) -> Vec<Range<usize>> {
        md::cells(&self.content[line.clone()])
            .into_iter()
            .filter_map(|cell| {
                let (from, to) = ((line.start + cell.start).max(sel.start), (line.start + cell.end).min(sel.end));
                (from < to).then(|| self.off_escape(from, false)..self.off_escape(to, true))
            })
            .collect()
    }

    /// Runs a gesture that edits cells, then realigns the tables it touched: the selection
    /// follows its text, and the cells a handle took stay taken, if the gesture left a selection.
    pub(super) fn aligned(&mut self, cx: &mut Context<Self>, act: impl FnOnce(&mut Self, &mut Context<Self>)) {
        let pick = self.table_at(self.sel.start).and_then(|(range, ..)| self.live_pick(range.start).cloned());
        let version = self.version;
        act(self, cx);
        if self.version == version {
            return;
        }
        // The later table first: realigning it leaves the earlier one where it is.
        self.align_at(self.sel.end, cx);
        self.align_at(self.sel.start, cx);
        if let Some(p) = pick.filter(|_| !self.sel.is_empty()) {
            self.pick(p.table, p.rows, p.cols, cx);
        }
    }

    /// Realigns the table holding byte `at`, as `realign` does, but the whole selection follows
    /// its text (an end that lands in padding goes to the end of its cell).
    fn align_at(&mut self, at: usize, cx: &mut Context<Self>) {
        let Some((range, rows, _)) = self.table_at(at) else {
            return;
        };
        let text = self.format_table(&range, &rows);
        let old = self.content[range.clone()].to_string();
        // A table without its dash line yet is left alone, as in `realign`.
        if text == old || old.matches('\n').count() + 1 != rows.len() {
            return;
        }
        let start_of = |t: &str, l: usize| t.split('\n').take(l).map(|x| x.len() + 1).sum::<usize>();
        let map = |p: usize| -> usize {
            if p <= range.start {
                return p;
            }
            if p >= range.end {
                return p + text.len() - old.len();
            }
            let q = p - range.start;
            let l = old[..q].matches('\n').count();
            let (ol, nl) = (old.split('\n').nth(l).unwrap_or_default(), text.split('\n').nth(l).unwrap_or_default());
            let (col, base) = (q - start_of(&old, l), range.start + start_of(&text, l));
            let (oc, nc) = (md::cells(ol), md::cells(nl));
            let Some(k) = oc.iter().rposition(|c| c.start <= col) else {
                return base + col.min(nl.len());
            };
            let off = (col - oc[k].start).min(oc[k].len());
            base + nc.get(k).map_or(nl.len(), |c| c.start + off.min(c.len()))
        };
        let sel = map(self.sel.start)..map(self.sel.end);
        self.damaged(&range);
        self.content.replace_range(range, &text);
        self.sel = sel;
        self.changed(cx);
    }

    /// Empties the selection's cells, `|` included in the selection: they keep their
    /// place, which deleting a block would merge. False if the selection does not cover several.
    pub(super) fn clear_cells(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(ranges) = self.selected_cells() else {
            return false;
        };
        let at = ranges.first().map_or(self.sel.start, |r| r.start);
        let guard = self.name_guard(self.sel.start);
        if !ranges.is_empty() {
            self.push_undo(true);
            for r in ranges.iter().rev() {
                self.damaged(r);
                self.content.replace_range(r.clone(), "");
            }
        }
        self.sel = at..at;
        self.reversed = false;
        self.changed(cx);
        self.realign(cx);
        if let Some((table, old)) = guard {
            self.rename_scopes(table, old, cx);
        }
        true
    }

    /// A byte that does not split a `\|`: the grid shows it as a single sign.
    pub(super) fn off_escape(&self, t: usize, forward: bool) -> usize {
        let b = self.content.as_bytes();
        let backslashes = b[..t].iter().rev().take_while(|&&x| x == b'\\').count();
        if t < b.len() && b[t] == b'|' && backslashes % 2 == 1 {
            if forward { t + 1 } else { t - 1 }
        } else {
            t
        }
    }

    /// Horizontal move in a table line: skips the `|` and spaces
    /// between cells, and goes from one line to the next without stopping on the dashes.
    pub(super) fn table_move(&self, c: usize, m: Motion) -> Option<usize> {
        let (cells, k) = self.table_cells(c)?;
        let cell = cells[k].clone();
        let at = c.clamp(cell.start, cell.end);
        let line = self.line_range(self.clamp(c));
        let prev_cell = || match k {
            0 => self.table_neighbour(c, false).and_then(|cells| Some(cells.last()?.end)).unwrap_or(line.start.saturating_sub(1)),
            _ => cells[k - 1].end,
        };
        let next_cell = || match cells.get(k + 1) {
            Some(next) => next.start,
            None => {
                let first = self.table_neighbour(c, true).and_then(|cells| Some(cells.first()?.start));
                first.unwrap_or((line.end + 1).min(self.content.len()))
            }
        };
        let inside = &self.content[cell.clone()];
        Some(match m {
            Motion::Left if at > cell.start => {
                let t = cell.start + inside[..at - cell.start].grapheme_indices(true).next_back().map_or(0, |(i, _)| i);
                self.off_escape(t, false)
            }
            Motion::Right if at < cell.end => {
                let t = at + inside[at - cell.start..].graphemes(true).next().map_or(0, str::len);
                self.off_escape(t, true)
            }
            Motion::WordLeft if at > cell.start => cell.start
                + inside[..at - cell.start]
                    .split_word_bound_indices()
                    .rev()
                    .find(|(_, w)| !w.trim().is_empty())
                    .map_or(0, |(i, _)| i),
            Motion::WordRight if at < cell.end => at
                + inside[at - cell.start..]
                    .split_word_bound_indices()
                    .find(|(_, w)| !w.trim().is_empty())
                    .map_or(cell.end - at, |(i, w)| i + w.len()),
            Motion::Left | Motion::WordLeft => prev_cell(),
            Motion::Right | Motion::WordRight => next_cell(),
            Motion::Home => cells[0].start,
            Motion::End => cells[cells.len() - 1].end,
            _ => return None,
        })
    }

    pub(super) fn target(&self, m: Motion, gx: Pixels) -> usize {
        self.table_move(self.cursor(), m).unwrap_or_else(|| self.plain_target(m, gx))
    }

    /// Where move `m` leads, counting only the text.
    pub(super) fn plain_target(&self, m: Motion, gx: Pixels) -> usize {
        let c = self.cursor();
        let lr = self.line_range(c);
        let line = &self.content[lr.clone()];
        let col = c - lr.start;
        let page = (f32::from(self.viewport.size.height) / 26.) as usize;
        match m {
            Motion::Left if col == 0 => c.saturating_sub(1),
            Motion::Left => {
                lr.start + line[..col].grapheme_indices(true).next_back().map_or(0, |(i, _)| i)
            }
            Motion::Right if c == lr.end => (c + 1).min(self.content.len()),
            Motion::Right => c + line[col..].graphemes(true).next().map_or(0, str::len),
            Motion::WordLeft => self.content[..c]
                .split_word_bound_indices()
                .rev()
                .find(|(_, w)| !w.trim().is_empty())
                .map_or(0, |(i, _)| i),
            Motion::WordRight => self.content[c..]
                .split_word_bound_indices()
                .find(|(_, w)| !w.trim().is_empty())
                .map_or(self.content.len(), |(i, w)| c + i + w.len()),
            Motion::Home => lr.start,
            Motion::End => lr.end,
            Motion::DocStart => 0,
            Motion::DocEnd => self.content.len(),
            Motion::Up => self.vertical(c, false, gx),
            Motion::Down => self.vertical(c, true, gx),
            Motion::PageUp => (0..page).fold(c, |c, _| self.vertical(c, false, gx)),
            Motion::PageDown => (0..page).fold(c, |c, _| self.vertical(c, true, gx)),
        }
    }

    /// Table picker open: `12x5` typed on the keyboard gives its size, beyond
    /// the grid. Returns false if `text` is not a size.
    pub(super) fn type_grid(&mut self, text: &str, cx: &mut Context<Self>) -> bool {
        let Some((cols, rows)) = self.grid else {
            return false;
        };
        if !text.chars().all(|c| c.is_ascii_digit() || matches!(c, 'x' | 'X' | '×' | '*' | ' ')) {
            return false;
        }
        self.grid_typed.push_str(text);
        let mut sizes = self.grid_typed.split(|c: char| !c.is_ascii_digit()).filter_map(|n| n.parse::<usize>().ok());
        let cols = sizes.next().unwrap_or(cols).clamp(1, GRID_MAX.0);
        let rows = sizes.next().unwrap_or(rows).clamp(1, GRID_MAX.1);
        self.grid = Some((cols, rows));
        cx.notify();
        true
    }

    /// Table around byte `at`: its range, its lines, and the cell
    /// (row, column) where `at` is.
    /// Byte `at` is in a very big table (see `grid::BIG`).
    pub(super) fn in_big(&self, at: usize) -> bool {
        self.row_at(at).is_some_and(|r| self.big.iter().any(|b| b.rows.contains(&r)))
    }

    pub(super) fn table_at(&self, at: usize) -> Option<TableAt> {
        // ponytail: a very big table (thousands of rows) is not reread nor realigned as a whole on each
        // keystroke: no column alignment, no row added with Tab or Enter. Tab goes to the next cell.
        if self.in_big(at) {
            return None;
        }
        let is_row = |r: &Range<usize>| self.content[r.clone()].trim_start().starts_with('|');
        let line = self.line_range(at);
        if !is_row(&line) || self.in_code(line.start) {
            return None;
        }
        let (mut start, mut end) = (line.start, line.end);
        while start > 0 {
            let above = self.line_range(start - 1);
            if !is_row(&above) {
                break;
            }
            start = above.start;
        }
        while end < self.content.len() {
            let below = self.line_range(end + 1);
            if !is_row(&below) {
                break;
            }
            end = below.end;
        }
        let (rows, added) = md::table(&self.content[start..end]);
        let mut r = self.content[start..at].matches('\n').count();
        // The missing dash line shifts the ones after the header.
        r += (added && r > 0) as usize;
        let col = at - line.start;
        let bars = md::bars(&self.content[line]).into_iter().filter(|&bar| bar < col).count();
        let c = bars.saturating_sub(1).min(rows[0].len() - 1);
        Some((start..end, rows, (r, c)))
    }

    pub(super) fn format_table(&self, range: &Range<usize>, rows: &[Vec<String>]) -> String {
        let first = &self.content[range.clone()];
        md::format_table(rows, &first[..first.len() - first.trim_start().len()])
    }

    /// Rewrites table `range`, columns aligned, and selects the content
    /// of the cell: typing replaces it.
    pub(super) fn set_table(&mut self, range: Range<usize>, rows: &[Vec<String>], (r, c): (usize, usize), cx: &mut Context<Self>) {
        let text = self.format_table(&range, rows);
        if self.content[range.clone()] != text {
            self.push_undo(true);
            self.edit(range.clone(), &text, cx);
        }
        let line = text.split('\n').nth(r).unwrap_or_default();
        let at = range.start + text.split('\n').take(r).map(|l| l.len() + 1).sum::<usize>();
        let cell = md::cells(line).get(c).cloned().unwrap_or(0..0);
        self.move_to(at + cell.start, cx);
        self.select_to(at + cell.end, cx);
    }

    /// After a keystroke in an already formed table (it has its dash line): its
    /// columns realign and the cursor keeps its place in its cell. Like
    /// `renumber`, without its own undo step.
    pub(super) fn realign(&mut self, cx: &mut Context<Self>) {
        let c = self.cursor();
        let line = self.line_range(c);
        if !self.sel.is_empty() || !self.content[line.clone()].trim_start().starts_with('|') {
            return;
        }
        let Some((range, rows, (r, col))) = self.table_at(c) else {
            return;
        };
        let text = self.format_table(&range, &rows);
        let formed = self.content[range.clone()].matches('\n').count() + 1 == rows.len();
        if !formed || text == self.content[range.clone()] {
            return;
        }
        let old = md::cells(&self.content[line.clone()]).get(col).map_or(c, |cell| line.start + cell.start);
        let at = range.start + text.split('\n').take(r).map(|l| l.len() + 1).sum::<usize>();
        let new = text.split('\n').nth(r).unwrap_or_default();
        // At the furthest, just before the `|` that closes the cell.
        let (start, end) = (md::cells(new)[col].start, md::bars(new)[col + 1]);
        let cursor = at + (start + c.saturating_sub(old)).min(end);
        self.damaged(&range);
        self.content.replace_range(range, &text);
        self.sel = cursor..cursor;
        self.changed(cx);
    }

    /// Tab and Shift+Tab in a table: next or previous cell; at the end
    /// of the table, a row is added.
    /// Tab and Shift+Tab in a very big table: the neighboring cell, on the line or the next one,
    /// without rereading the table.
    pub(super) fn table_step_big(&mut self, forward: bool, cx: &mut Context<Self>) -> bool {
        let cursor = self.cursor();
        let line = self.line_range(cursor);
        let cells = md::cells(&self.content[line.clone()]);
        let here = cells.iter().position(|c| line.start + c.end >= cursor).unwrap_or(cells.len().saturating_sub(1));
        let next = |line: &Range<usize>, first: bool| {
            let cells = md::cells(&self.content[line.clone()]);
            let cell = if first { cells.first() } else { cells.last() }?;
            Some(line.start + cell.start..line.start + cell.end)
        };
        let target = match (forward, here) {
            (true, i) if i + 1 < cells.len() => Some(line.start + cells[i + 1].start..line.start + cells[i + 1].end),
            (false, i) if i > 0 => Some(line.start + cells[i - 1].start..line.start + cells[i - 1].end),
            (true, _) if line.end < self.content.len() => {
                let below = self.line_range(line.end + 1);
                self.is_table_line(&below).then(|| next(&below, true)).flatten()
            }
            (false, _) if line.start > 0 => {
                let above = self.line_range(line.start - 1);
                self.is_table_line(&above).then(|| next(&above, false)).flatten()
            }
            _ => None,
        };
        if let Some(cell) = target {
            self.move_to(cell.start, cx);
            self.select_to(cell.end, cx);
        }
        true
    }

    pub(super) fn table_step(&mut self, forward: bool, cx: &mut Context<Self>) -> bool {
        if self.in_big(self.cursor()) {
            return self.table_step_big(forward, cx);
        }
        let Some((range, mut rows, (r, c))) = self.table_at(self.cursor()) else {
            return false;
        };
        let cols = rows[0].len();
        let mut cell = r * cols + c;
        // The dash line (the second one) is skipped.
        loop {
            cell = if forward { cell + 1 } else { cell.saturating_sub(1) };
            if cell / cols != 1 {
                break;
            }
        }
        if cell / cols == rows.len() {
            rows.push(vec![String::new(); cols]);
        }
        self.set_table(range, &rows, (cell / cols, cell % cols), cx);
        true
    }

    /// The cursor is in a Markdown table that can be rewritten (not a very big one).
    pub fn in_table(&self) -> bool {
        self.table_at(self.sel.start).is_some()
    }

    /// A gesture on the cursor's table, for the rows and columns the selection touches
    /// (from the cell where it starts to the one where it ends). A single undo step; the cursor
    /// lands on the cell that follows the gesture.
    /// Columns grabbed by their handle, whole, go; otherwise the selection's rows.
    pub(super) fn table_delete_span(&mut self, cx: &mut Context<Self>) {
        let Some((range, rows, _)) = self.table_at(self.sel.start) else {
            return;
        };
        let cols = self.live_pick(range.start).is_some_and(|p| p.rows.start == 0 && p.rows.end >= md::visible_rows(&rows) && p.cols.len() < rows[0].len());
        self.table_op(if cols { TableOp::DeleteCol } else { TableOp::DeleteRow }, cx)
    }

    pub(super) fn table_op(&mut self, op: TableOp, cx: &mut Context<Self>) {
        let Some((range, rows, (r, c))) = self.table_at(self.sel.start) else {
            return;
        };
        // Visible rank: the dash line counts for the header.
        let seen = |r: usize| r.saturating_sub(1);
        let slot = |v: usize| if v == 0 { 0 } else { v + 1 };
        let end = self.table_at(self.sel.end).filter(|(other, ..)| *other == range).map_or((r, c), |(.., cell)| cell);
        // What a handle grabbed counts more than the selected text.
        let (r0, r1, c0, c1) = match self.live_pick(range.start) {
            Some(pick) => (pick.rows.start, pick.rows.end - 1, pick.cols.start, pick.cols.end - 1),
            None => (seen(r).min(seen(end.0)), seen(r).max(seen(end.0)), c.min(end.1), c.max(end.1)),
        };
        let (tall, wide) = (md::visible_rows(&rows), rows[0].len());
        let (grid, at) = match op {
            TableOp::RowAbove => (Some(md::insert_row(&rows, r0)), (slot(r0.max(1)), c0)),
            TableOp::RowBelow => (Some(md::insert_row(&rows, r1 + 1)), (slot(r1 + 1), c0)),
            TableOp::ColLeft => (Some(md::insert_col(&rows, c0)), (slot(r0), c0)),
            TableOp::ColRight => (Some(md::insert_col(&rows, c1 + 1)), (slot(r0), c1 + 1)),
            TableOp::DuplicateRow => (Some(md::duplicate_row(&rows, r0)), (slot(r0 + 1), c0)),
            TableOp::DuplicateCol => (Some(md::duplicate_col(&rows, c0)), (slot(r0), c0 + 1)),
            TableOp::MoveRowUp if r0 > 1 => (Some(md::move_row(&rows, r0 - 1, r1)), (slot(r0 - 1), c0)),
            TableOp::MoveRowDown if r0 > 0 && r1 + 1 < tall => (Some(md::move_row(&rows, r1 + 1, r0)), (slot(r0 + 1), c0)),
            TableOp::MoveColLeft if c0 > 0 => (Some(md::move_col(&rows, c0 - 1, c1)), (slot(r0), c0 - 1)),
            TableOp::MoveColRight if c1 + 1 < wide => (Some(md::move_col(&rows, c1 + 1, c0)), (slot(r0), c0 + 1)),
            TableOp::DeleteRow => (md::delete_rows(&rows, r0..r1 + 1), (slot(r0.min(tall.saturating_sub(r1 - r0 + 2))), c0)),
            TableOp::DeleteCol => (md::delete_cols(&rows, c0..c1 + 1), (slot(r0), c0.min(wide.saturating_sub(c1 - c0 + 2)))),
            TableOp::SortAsc => (Some(md::sort_rows(&rows, c0, true)), (0, c0)),
            TableOp::SortDesc => (Some(md::sort_rows(&rows, c0, false)), (0, c0)),
            TableOp::Transpose => (Some(md::transpose(&rows)), (0, 0)),
            TableOp::DeleteTable => (None, (0, 0)),
            // A gesture with nowhere to go: the header stays on top, the last row does not go down.
            _ => return,
        };
        // The comments under the table follow what they are about, or go with it, in the same undo step.
        let lines = self.comment_lines_after(range.end);
        let tracks = !lines.is_empty();
        let held = self.held;
        if tracks {
            let shift = |n: usize, gone: Range<usize>| -> Vec<Option<usize>> {
                (0..n).map(|i| (!gone.contains(&i)).then(|| i - gone.clone().filter(|g| *g < i).count())).collect()
            };
            let (old_rows, old_cols) = md::keys(&rows);
            let old = md::scopes(&rows);
            let (rows_to, cols_to, new, gone) = match (&grid, op) {
                (None, _) => (vec![None; tall], vec![None; wide], Default::default(), true),
                (Some(_), TableOp::DeleteRow) => (shift(tall, r0..r1 + 1), (0..wide).map(Some).collect(), md::scopes(grid.as_ref().unwrap()), false),
                (Some(_), TableOp::DeleteCol) => ((0..tall).map(Some).collect(), shift(wide, c0..c1 + 1), md::scopes(grid.as_ref().unwrap()), false),
                // Rows become columns: each is looked for among the other kind, and named by it.
                (Some(g), TableOp::Transpose) => {
                    let (new_rows, new_cols) = md::keys(g);
                    let (names, cols) = md::scopes(g);
                    (md::track(&old_rows, &new_cols), md::track(&old_cols, &new_rows), (cols, names), false)
                }
                // Moved, inserted, duplicated, sorted: found again by their content.
                (Some(g), _) => {
                    let (new_rows, new_cols) = md::keys(g);
                    (md::track(&old_rows, &new_rows), md::track(&old_cols, &new_cols), md::scopes(g), false)
                }
            };
            self.push_undo(true);
            self.held = true;
            self.retarget_comments(&lines, (&rows_to, &cols_to), &old, &new, gone, cx);
        }
        match grid {
            Some(grid) => self.set_table(range, &grid, at, cx),
            // Nothing left in the table: it goes with the line break that follows it.
            None => {
                let stop = if self.content[range.end..].starts_with('\n') { range.end + 1 } else { range.end };
                self.push_undo(true);
                self.edit(range.start..stop, "", cx);
                self.move_to(range.start.min(self.content.len()), cx);
            }
        }
        self.held = held;
    }

    /// Aligns the cursor's column: `:--`, `:-:` or `--:` under its header. The dash
    /// line is not visible, so this shortcut is what sets it.
    pub(super) fn align(&mut self, dashes: &str, cx: &mut Context<Self>) {
        let Some((range, mut rows, (r, c))) = self.table_at(self.cursor()) else {
            return;
        };
        rows[1][c] = dashes.to_string();
        self.set_table(range, &rows, (r, c), cx);
    }

    /// Puts an empty table at the cursor, on its own lines.
    pub(super) fn insert_table(&mut self, cols: usize, rows: usize, cx: &mut Context<Self>) {
        let c = self.cursor();
        let line = self.line_range(c);
        // An empty line separates it from what comes before: stuck to a quote, it would be part of it.
        let above = self.content[..line.start].trim_end_matches(' ');
        let apart = self.content[line.start..c].trim().is_empty() && (above.is_empty() || above.ends_with("\n\n"));
        let lead = if apart { "" } else { "\n" };
        let tail = if c == line.end { "" } else { "\n" };
        let table = md::new_table(cols, rows);
        self.push_undo(true);
        self.edit(c..c, &format!("{lead}{}{tail}", md::format_table(&table, "")), cx);
        if let Some((range, rows, _)) = self.table_at(c + lead.len()) {
            self.set_table(range, &rows, (0, 0), cx);
        }
    }

    /// "+" button under the pointer: (start of the table, 0 for a row or 1
    /// for a column).
    pub(super) fn plus_at(&self, at: Point<Pixels>) -> Option<(usize, usize)> {
        self.tables.iter().find_map(|(range, frame)| {
            Some((range.start, plus_bars(*frame).iter().position(|bar| bar.contains(&at))?))
        })
    }

    pub(super) fn grid_size() -> Size<Pixels> {
        let side = GRID_STEP * GRID as f32 + px(12.);
        size(side, side + px(22.))
    }

    /// Table picker cell under the pointer: (columns, rows).
    pub(super) fn grid_at(&self, at: Point<Pixels>) -> Option<(usize, usize)> {
        self.grid?;
        let local = at - self.popup_at(Self::grid_size())? - point(px(8.), px(8.));
        let (col, row) = ((local.x / GRID_STEP).floor(), (local.y / GRID_STEP).floor());
        let inside = |n: f32| (0. ..GRID as f32).contains(&n);
        (inside(col) && inside(row)).then_some((col as usize + 1, row as usize + 1))
    }

    /// Selected cells of a table, as a Markdown table: mail, issues and Markdown
    /// editors paste it as a table, and in a note, pasting rebuilds the cells.
    // ponytail: plain text only, no `text/html` flavor: gpui's clipboard item holds one text, and
    // `wl-copy` / `xclip` offer one type per call, so setting HTML would drop the text. A
    // spreadsheet pastes the Markdown lines in one column.
    pub(super) fn cells_text(&self) -> Option<String> {
        let mut rows: Vec<Vec<String>> = Vec::new();
        let mut line = None;
        // A "select all" ends after the last line break: outside the table.
        let end = self.content[..self.sel.end].trim_end_matches('\n').len().max(self.sel.start);
        for cell in self.cells_in(self.sel.start..end)? {
            let start = self.line_range(cell.start).start;
            if line != Some(start) {
                rows.push(Vec::new());
                line = Some(start);
            }
            rows.last_mut()?.push(self.content[cell].trim().replace("\\|", "|"));
        }
        Some(md::grid_markdown(&rows))
    }

    /// Pasting in a table: a cell keeps a single line and its `|`s
    /// escaped; spreadsheet or Markdown cells fill the grid
    /// from the cursor's cell, which grows as needed.
    pub(super) fn paste_in_table(&mut self, text: &str, cx: &mut Context<Self>) -> bool {
        self.clear_cells(cx);
        let sel = self.sel.clone();
        if sel.end > self.line_range(sel.start).end {
            return false;
        }
        let Some((range, mut rows, (r, c))) = self.table_at(sel.start) else {
            return false;
        };
        let Some(grid) = md::paste_grid(text) else {
            // Before the first `|` or after the last: in the neighboring cell, not on the line.
            let line = self.line_range(sel.start);
            let bars = md::bars(&self.content[line.clone()]);
            let col = sel.start - line.start;
            let cell = md::cells(&self.content[line.clone()]).get(c).cloned().unwrap_or(0..0);
            let sel = match (bars.first(), bars.last()) {
                _ if !sel.is_empty() => sel,
                (Some(&first), _) if col <= first => line.start + cell.start..line.start + cell.start,
                (Some(_), Some(&last)) if col > last && self.content[line.clone()].trim_end().ends_with('|') => line.start + cell.end..line.start + cell.end,
                _ => sel,
            };
            self.push_undo(true);
            self.edit(sel, &md::cell_text(text), cx);
            self.realign(cx);
            return true;
        };
        let width = rows[0].len().max(c + grid.iter().map(Vec::len).max().unwrap_or(0));
        for (i, row) in rows.iter_mut().enumerate() {
            row.resize(width, if i == 1 { "---" } else { "" }.to_string());
        }
        // The dash line (the second one) is skipped.
        let mut at = if r == 1 { 2 } else { r };
        let mut last = (at, c);
        for cells in &grid {
            at += (at == 1) as usize;
            if at == rows.len() {
                rows.push(vec![String::new(); width]);
            }
            rows[at][c..c + cells.len()].clone_from_slice(cells);
            last = (at, c + cells.len() - 1);
            at += 1;
        }
        let guard = self.name_guard(range.start);
        self.set_table(range, &rows, last, cx);
        if let Some((table, old)) = guard {
            self.rename_scopes(table, old, cx);
        }
        true
    }
}
