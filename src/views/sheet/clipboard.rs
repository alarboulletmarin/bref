//! Copy, cut, paste and export of a selection, as TSV, CSV, Markdown or JSON.

use super::*;

impl Sheet {
    pub(super) fn flash(&mut self, cx: &mut Context<Self>) {
        self.copied = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(1500)).await;
            this.update(cx, |this, cx| {
                this.copied = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The selection, or the whole table if it fits in a single cell.
    pub(super) fn scope(&self) -> (Range<usize>, Range<usize>, bool) {
        let (rows, cols) = self.selection();
        match rows.len() * cols.len() {
            1 => (0..self.table.rows(), 0..self.cols(), false),
            _ => (rows, cols, true),
        }
    }

    /// Ctrl+C: the selection as TSV, which spreadsheets and note tables paste as is.
    pub(super) fn copy(&mut self, cx: &mut Context<Self>) {
        // Several selections: their blocks one under the other, top to bottom.
        let blocks: Vec<String> =
            self.selections().into_iter().map(|(rows, cols)| self.table.text_range(rows, cols, Style::Tsv)).collect();
        cx.write_to_clipboard(ClipboardItem::new_string(blocks.join("\n")));
        self.flash(cx);
    }

    /// "Copy table as…": the selection, or the whole table if it fits in one cell.
    pub fn copy_as(&mut self, style: Style, cx: &mut Context<Self>) {
        self.finish(cx);
        let (rows, cols, _) = self.scope();
        self.copy_range(rows, cols, style, cx);
    }

    pub(super) fn copy_range(&mut self, rows: Range<usize>, cols: Range<usize>, style: Style, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(self.table.text_range(rows, cols, style)));
        self.flash(cx);
    }

    pub(super) fn cut(&mut self, cx: &mut Context<Self>) {
        self.copy(cx);
        self.clear(cx);
    }

    /// Pasted text: cells (spreadsheet, Markdown table, CSV) fill the grid from
    /// the corner of the selection, which grows if needed; a single one fills the whole selection.
    pub(super) fn paste(&mut self, text: &str, cx: &mut Context<Self>) {
        let grid = markdown::paste_grid(text).unwrap_or_else(|| table::parse_grid(text));
        if grid.is_empty() {
            return;
        }
        let (rows, cols) = self.selection();
        let (top, left) = (rows.start, cols.start);
        let one = grid.len() == 1 && grid[0].len() == 1;
        let (height, width) = if one { (rows.len(), cols.len()) } else { (grid.len(), grid.iter().map(Vec::len).max().unwrap_or(0)) };
        let mut changes = Vec::new();
        if top + height > self.table.rows() {
            changes.push(Change::Insert(self.table.rows(), vec![Vec::new(); top + height - self.table.rows()]));
        }
        let cells = (0..height)
            .flat_map(|i| (0..width).map(move |j| (i, j)))
            .filter_map(|(i, j)| {
                let value = if one { grid[0][0].clone() } else { grid[i].get(j)?.clone() };
                Some((top + i, left + j, value))
            })
            .collect();
        changes.push(Change::Set(cells));
        self.commit(changes, cx);
        self.more.clear();
        (self.anchor, self.cursor) = ((top, left), (top + height - 1, left + width.max(1) - 1));
        self.show(self.cursor);
    }

    /// Writes the table (or the selection) next to the file, in this format.
    pub fn export(&mut self, style: Style, cx: &mut Context<Self>) {
        self.finish(cx);
        let (rows, cols, partial) = self.scope();
        let text = self.table.text_range(rows, cols, style);
        let suffix = if partial { tr(" (selection)", " (sélection)") } else { "" };
        let name = format!("{}{suffix}", vault::stem(&self.path));
        let file = vault::free_path(self.path.parent().unwrap_or(Path::new("")), &name, style.extension());
        match vault::write(&file, &text) {
            Ok(()) => cx.emit(SheetEvent::Exported),
            Err(e) => cx.emit(SheetEvent::Error(format!("{} : {e}", tr("Not exported", "Export impossible")))),
        }
    }
}
