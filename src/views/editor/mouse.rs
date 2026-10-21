//! The mouse: clicks, drags, the scroll wheel, and picking table rows and columns.

use super::*;

/// Rows and columns (visible indices) taken by a handle. Valid as long as the text and the
/// selection are those of then: nothing else to undo.
#[derive(Clone)]
pub(super) struct Pick {
    pub(super) table: usize,
    pub(super) rows: Range<usize>,
    pub(super) cols: Range<usize>,
    pub(super) sel: Range<usize>,
    pub(super) version: u64,
}

/// A row or column being moved by dragging its handle.
pub(super) struct Moving {
    pub(super) table: usize,
    pub(super) part: Part,
    pub(super) from: Point<Pixels>,
    /// The line where it would land.
    pub(super) to: Option<usize>,
}

/// What the pointer points at around a table: its column, its row, and whether it is the corner.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub(super) struct Near {
    pub(super) col: Option<usize>,
    pub(super) row: Option<usize>,
    pub(super) corner: bool,
}

impl Editor {
    /// Screen point where byte `i` is, in the middle of its line.
    #[cfg(test)]
    pub fn point_of(&self, i: usize) -> Option<Point<Pixels>> {
        let row = &self.rows[self.row_at(i)?];
        Some(self.origin + point(px(0.), row.y + row.pad + row.lh / 2.) + row.pos(i - row.start))
    }

    /// The pointer's three entry points, called as they are: with the lines of the last paint, whatever the text became.
    #[cfg(test)]
    pub fn pointer(&mut self, at: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let none = gpui::Modifiers::none();
        self.mouse_move(&MouseMoveEvent { position: at, pressed_button: None, modifiers: none }, window, cx);
        for button in [MouseButton::Left, MouseButton::Right] {
            let down = MouseDownEvent { button, position: at, modifiers: none, click_count: 1, first_mouse: false };
            if button == MouseButton::Left { self.mouse_down(&down, window, cx) } else { self.menu_down(&down, window, cx) }
        }
    }

    #[cfg(test)]
    pub fn hand(&self) -> Option<CursorStyle> {
        self.hand
    }

    /// Where to put what is dropped at pointer `at`: the start of a line, the one below when the pointer is
    /// in the lower half of its own. Never in a table, a code block or the front matter: at the
    /// edge of the block, downwards.
    pub fn drop_point(&self, at: Point<Pixels>) -> usize {
        let y = at.y - self.origin.y;
        let Some(row) = self.rows.iter().find(|r| y < r.y + r.height).or(self.rows.last()) else {
            return 0;
        };
        let line = self.line_range(row.start);
        let mut point = if y > row.y + row.height / 2. { (line.end + 1).min(self.content.len()) } else { line.start };
        let front = md::front_matter(&self.content);
        if point > 0 && point < front {
            point = front;
        }
        // In a table (its line and the previous one belong to it) or in a code block: further down.
        while point > 0 && point < self.content.len() {
            let (here, above) = (self.line_range(point), self.line_range(point - 1));
            if (self.is_table_line(&here) && self.is_table_line(&above)) || self.in_code(point) {
                point = (here.end + 1).min(self.content.len());
            } else {
                break;
            }
        }
        point
    }

    pub fn set_drop_mark(&mut self, mark: Option<usize>, cx: &mut Context<Self>) {
        if self.drop_mark != mark {
            self.drop_mark = mark;
            cx.notify();
        }
    }

    /// The start of the cursor's line.
    pub fn caret_line_start(&self) -> usize {
        self.line_range(self.cursor()).start
    }

    /// The handle under the pointer: (table start, handle). Only visible ones can be grabbed.
    pub(super) fn part_at(&self, at: Point<Pixels>) -> Option<(usize, Part)> {
        self.geos.iter().find_map(|geo| {
            let (near, pick) = (geo.near(at), self.live_pick(geo.start));
            let hit = geo.parts().into_iter().find(|(part, zone)| zone.dilate(px(2.)).contains(&at) && geo.visible(*part, near, pick))?;
            Some((geo.start, hit.0))
        })
    }

    /// What a handle took in the table starting at `table`, as long as nothing has moved.
    pub(super) fn live_pick(&self, table: usize) -> Option<&Pick> {
        self.pick.as_ref().filter(|p| p.table == table && p.version == self.version && p.sel == self.sel)
    }

    /// Cells `rows` × `cols` (visible indices) of the table starting at `table` are taken: the
    /// cursor goes into the first, and gestures, copy and clearing apply to all of them.
    pub(super) fn pick(&mut self, table: usize, rows: Range<usize>, cols: Range<usize>, cx: &mut Context<Self>) {
        let Some((range, _, _)) = self.table_at(table) else {
            return;
        };
        let line = if rows.start == 0 { 0 } else { rows.start + 1 };
        let text = &self.content[range.clone()];
        let at = range.start + text.split('\n').take(line).map(|l| l.len() + 1).sum::<usize>();
        let cell = md::cells(text.split('\n').nth(line).unwrap_or_default()).get(cols.start).cloned().unwrap_or(0..0);
        self.move_to(at + cell.start, cx);
        self.select_to(at + cell.end, cx);
        self.pick = Some(Pick { table: range.start, rows, cols, sel: self.sel.clone(), version: self.version });
        cx.notify();
    }

    /// Takes what a handle points at.
    pub(super) fn pick_part(&mut self, table: usize, part: Part, cx: &mut Context<Self>) {
        let Some(geo) = self.geos.iter().find(|g| g.start == table) else {
            return;
        };
        let (tall, cols) = (geo.tall(), geo.cols());
        match part {
            Part::Col(c) => self.pick(table, 0..tall, c..c + 1, cx),
            Part::Row(r) => self.pick(table, r..r + 1, 0..cols, cx),
            Part::Corner => self.pick(table, 0..tall, 0..cols, cx),
            Part::JoinCol(_) | Part::JoinRow(_) => {}
        }
    }

    /// The taken cells, as a Markdown table (a single cell: its text).
    pub(super) fn pick_text(&self) -> Option<String> {
        let (range, rows, _) = self.table_at(self.sel.start)?;
        let pick = self.live_pick(range.start)?;
        let line = |v: usize| if v == 0 { 0 } else { v + 1 };
        let cells: Vec<Vec<String>> =
            pick.rows.clone().map(|v| rows[line(v)][pick.cols.clone()].iter().map(|c| c.replace("\\|", "|")).collect()).collect();
        match cells.as_slice() {
            [one] if one.len() == 1 => Some(one[0].clone()),
            _ => Some(md::grid_markdown(&cells)),
        }
    }

    /// The text of each cell a handle took, in reading order, edge blanks left out.
    pub(super) fn pick_cells(&self) -> Option<Vec<Range<usize>>> {
        let (range, ..) = self.table_at(self.sel.start)?;
        let pick = self.live_pick(range.start)?;
        let lines: Vec<(usize, &str)> = self.content[range.clone()]
            .split('\n')
            .scan(range.start, |at, l| {
                let start = *at;
                *at += l.len() + 1;
                Some((start, l))
            })
            .collect();
        let mut cells = Vec::new();
        for v in pick.rows.clone() {
            let (start, line) = lines[if v == 0 { 0 } else { v + 1 }];
            let found = md::cells(line);
            cells.extend(pick.cols.clone().filter_map(|c| found.get(c)).map(|r| start + r.start..start + r.end));
        }
        Some(cells)
    }

    /// Empties the taken cells, without touching the table's shape. False if there are none.
    pub(super) fn clear_pick(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((range, mut rows, _)) = self.table_at(self.sel.start) else {
            return false;
        };
        let Some(pick) = self.live_pick(range.start).cloned() else {
            return false;
        };
        let guard = self.name_guard(range.start);
        for v in pick.rows.clone() {
            for c in pick.cols.clone() {
                rows[if v == 0 { 0 } else { v + 1 }][c].clear();
            }
        }
        self.set_table(range.clone(), &rows, (if pick.rows.start == 0 { 0 } else { pick.rows.start + 1 }, pick.cols.start), cx);
        self.pick(range.start, pick.rows, pick.cols, cx);
        // A header or a first cell emptied: its comments take the new name.
        if let Some((table, old)) = guard {
            self.rename_scopes(table, old, cx);
        }
        true
    }

    /// A click on a handle: "+" inserts, a handle takes its row or column (and drags it,
    /// if moved), the corner takes the whole table.
    pub(super) fn grip_down(&mut self, table: usize, part: Part, at: Point<Pixels>, cx: &mut Context<Self>) {
        match part {
            Part::JoinCol(b) | Part::JoinRow(b) => {
                let Some((range, rows, _)) = self.table_at(table) else {
                    return;
                };
                let cols = matches!(part, Part::JoinCol(_));
                let grid = if cols { md::insert_col(&rows, b) } else { md::insert_row(&rows, b) };
                self.set_table(range, &grid, if cols { (0, b) } else { (b + 1, 0) }, cx);
            }
            _ => {
                self.pick_part(table, part, cx);
                // The header stays on top: its row can be taken, not dragged.
                if matches!(part, Part::Col(_) | Part::Row(1..)) {
                    self.moving = Some(Moving { table, part, from: at, to: None });
                }
            }
        }
    }

    /// The dragged handle is released on line `to`: the row or column lands there, and stays taken.
    pub(super) fn drop_part(&mut self, moving: Moving, cx: &mut Context<Self>) {
        let (Some(b), Some((range, rows, _))) = (moving.to, self.table_at(moving.table)) else {
            return;
        };
        let (from, grid) = match moving.part {
            Part::Col(n) | Part::Row(n) => (n, if b > n { b - 1 } else { b }),
            _ => return,
        };
        if from == grid {
            return;
        }
        let (cols, tall) = (rows[0].len(), md::visible_rows(&rows));
        let moved = if matches!(moving.part, Part::Col(_)) { md::move_col(&rows, from, grid) } else { md::move_row(&rows, from, grid) };
        self.set_table(range.clone(), &moved, (0, 0), cx);
        if matches!(moving.part, Part::Col(_)) {
            self.pick(range.start, 0..tall, grid..grid + 1, cx);
        } else {
            self.pick(range.start, grid..grid + 1, 0..cols, cx);
        }
    }

    pub(super) fn copy_bounds(&self, row: &Row) -> Bounds<Pixels> {
        let at = point(self.width - COPY_SIZE, row.y + row.pad + (row.lh - COPY_SIZE) / 2.);
        Bounds::new(self.origin + at, size(COPY_SIZE, COPY_SIZE))
    }

    /// Start of the line whose copy button is under the pointer.
    pub(super) fn copy_at(&self, at: Point<Pixels>) -> Option<usize> {
        let row = self.rows.iter().find(|r| r.opens && self.copy_bounds(r).contains(&at))?;
        Some(row.start)
    }

    /// The line opens a Mermaid block: it also carries the button that shows the diagram enlarged,
    /// left of the copy one.
    pub(super) fn views(&self, row: &Row) -> bool {
        let line = &self.content[row.start..row.start + row.len];
        row.opens && line.trim_start().get(3..).is_some_and(|lang| lang.trim().eq_ignore_ascii_case("mermaid"))
    }

    pub(super) fn view_bounds(&self, row: &Row) -> Bounds<Pixels> {
        let copy = self.copy_bounds(row);
        Bounds::new(copy.origin - point(COPY_SIZE + px(4.), px(0.)), copy.size)
    }

    /// Start of the line whose "enlarge" button is under the pointer.
    pub(super) fn view_at(&self, at: Point<Pixels>) -> Option<usize> {
        let row = self.rows.iter().find(|r| self.views(r) && self.view_bounds(r).contains(&at))?;
        Some(row.start)
    }

    /// Where the picture or figure of the row is drawn, in the window.
    pub(super) fn image_bounds(&self, row: &Row) -> Option<Bounds<Pixels>> {
        let (_, s) = row.image.as_ref()?;
        Some(Bounds::new(self.origin + point(px(0.), row.image_top()), *s))
    }

    /// The "enlarge" button of a picture or SVG: in its top right corner, or, if it is too small to
    /// hold it, beside it (at the right edge when the line is full). A formula has no background to
    /// hide the button's place: it is always beside.
    pub(super) fn big_bounds(&self, row: &Row) -> Option<Bounds<Pixels>> {
        row.enlarge.as_ref()?;
        let image = self.image_bounds(row)?;
        let (inset, s) = (px(6.), image.size);
        let roomy = s.width >= COPY_SIZE * 2. && s.height >= COPY_SIZE * 2. && !matches!(row.enlarge, Some(Source::Formula(..)));
        let at = if roomy {
            point(s.width - COPY_SIZE - inset, inset)
        } else {
            point((s.width + px(4.)).min(self.width - COPY_SIZE), ((s.height - COPY_SIZE) / 2.).max(px(0.)))
        };
        // A tall picture scrolled up keeps its button in view.
        let (low, high) = (self.viewport.top() + inset, image.top() + s.height.max(COPY_SIZE) - COPY_SIZE);
        let y = (image.origin.y + at.y).max(low).min(high);
        Some(Bounds::new(point(image.origin.x + at.x, y), size(COPY_SIZE, COPY_SIZE)))
    }

    /// Start of the line whose picture, or whose "enlarge" button, is under the pointer.
    pub(super) fn big_at(&self, at: Point<Pixels>) -> Option<usize> {
        let over = |r: &&Row| {
            self.image_bounds(r).is_some_and(|b| b.contains(&at)) || self.big_bounds(r).is_some_and(|b| b.contains(&at))
        };
        self.rows.iter().filter(|r| r.enlarge.is_some()).find(over).map(|r| r.start)
    }

    /// What the "enlarge" button under the pointer opens.
    fn big_source(&self, at: Point<Pixels>) -> Option<Source> {
        let row = self.rows.iter().find(|r| self.big_bounds(r).is_some_and(|b| b.contains(&at)))?;
        row.enlarge.clone()
    }

    pub(super) fn index_at(&self, pos: Point<Pixels>) -> usize {
        let y = pos.y - self.origin.y;
        let Some(row) = self.rows.iter().find(|r| y < r.y + r.height).or(self.rows.last()) else {
            return 0;
        };
        let local = point(pos.x - self.origin.x, (y - row.y - row.pad).max(px(0.)));
        self.clamp(row.start + row.index_at(local))
    }

    /// Why the formula under `at` has no result, if it is in a table.
    pub(super) fn reason_at(&self, at: Point<Pixels>) -> Option<Rc<str>> {
        if !self.tables.iter().any(|(_, frame)| frame.contains(&at)) {
            return None;
        }
        let i = self.index_at(at);
        let row = self.rows.get(self.row_at(i)?)?;
        row.grid.as_ref()?.reason_at(i - row.start)
    }

    pub(super) fn mouse_down(&mut self, e: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.rows_revision != self.revision {
            return;
        }
        if let Some(find) = &mut self.find {
            find.active = false;
        }
        // Picture, formula or figure enlarged, in the viewer.
        if let Some(source) = self.big_source(e.position) {
            return cx.emit(EditorEvent::View(source));
        }
        // Mermaid diagram enlarged, in the viewer.
        if let Some(start) = self.view_at(e.position)
            && let Some(block) = md::code_block(&self.content, self.line_range(start).end + 1)
        {
            return cx.emit(EditorEvent::View(Source::Mermaid(self.content[block].to_string())));
        }
        // Copy button, right of a code block's opening.
        if let Some(start) = self.copy_at(e.position)
            && let Some(block) = md::code_block(&self.content, self.line_range(start).end + 1)
        {
            cx.write_to_clipboard(ClipboardItem::new_string(self.content[block].to_string()));
            self.copied = Some(start);
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(Duration::from_millis(1500)).await;
                this.update(cx, |this, cx| {
                    this.copied = None;
                    cx.notify();
                })
                .ok();
            })
            .detach();
            return cx.notify();
        }
        // An embedded section: its source's name, or a double click, opens the note there; a click elsewhere in
        // the frame puts the cursor on the line that says what it embeds.
        if let Some((row, embed, on_label)) = self.embed_at(e.position) {
            let (end, target) = (row.start + row.len, embed.target.clone());
            if on_label || e.click_count >= 2 {
                return self.open_embed(target, e.modifiers.shift, cx);
            }
            self.more.clear();
            return self.move_to(end, cx);
        }
        if let Some((cols, rows)) = self.grid_at(e.position) {
            return self.insert_table(cols, rows, cx);
        }
        self.grid = None;
        // A block's handle: dragging it moves it, a click opens its menu; the "+" adds a block below.
        if let Some((grip, plus)) = self.handle_bounds()
            && let Some(block) = self.handled()
        {
            if plus.is_some_and(|plus| plus.contains(&e.position)) {
                return self.add_block_after(block, cx);
            }
            if grip.contains(&e.position) {
                self.handles.drag = Some(BlockDrag { block, from: e.position, to: None, moved: false });
                return cx.notify();
            }
        }
        // A table's handles: take a row, a column or everything, insert at a line.
        if let Some((table, part)) = self.part_at(e.position) {
            return self.grip_down(table, part, e.position, cx);
        }
        // A table's "+" buttons: a row at the end, or a column.
        if let Some((at, bar)) = self.plus_at(e.position)
            && let Some((range, mut rows, _)) = self.table_at(at)
        {
            let cell = if bar == 0 {
                rows.push(vec![String::new(); rows[0].len()]);
                (rows.len() - 1, 0)
            } else {
                let cells = rows.iter_mut().enumerate();
                cells.for_each(|(i, row)| row.push(if i == 1 { "---" } else { "" }.to_string()));
                (0, rows[0].len() - 1)
            };
            return self.set_table(range, &rows, cell, cx);
        }
        let i = self.index_at(e.position);
        if e.modifiers.alt {
            return self.add_cursor(i, cx);
        }
        // An ordinary click returns to a single cursor.
        self.more.clear();
        let lr = self.line_range(i);
        let line = &self.content[lr.clone()];
        let col = i - lr.start;
        if e.modifiers.secondary()
            && let Some((range, link)) = md::links(line).into_iter().find(|(r, _)| r.contains(&col))
        {
            // A link to a heading or a block of a note opens the note there.
            if let Some(target) = Some(&link).filter(|l| matches!(l, Link::Wiki(_))).and_then(|_| crate::logic::embed::link_target(&line[range.start + 2..range.end - 2])).filter(|t| t.anchor != crate::logic::embed::Anchor::Whole) {
                return cx.emit(EditorEvent::OpenAt(target, e.modifiers.shift));
            }
            return cx.emit(if e.modifiers.shift { EditorEvent::OpenAside(link) } else { EditorEvent::Open(link) });
        }
        if !e.modifiers.secondary()
            && let Some(at) = self.checkbox_at(e.position)
        {
            return self.flip_box(at, cx);
        }
        let (kind, marker) = md::classify(line, self.in_code(lr.start));
        if matches!(kind, Kind::Task(_)) && col + 4 >= marker && col < marker {
            self.move_to(lr.end, cx);
            return self.toggle_task(cx);
        }
        match e.click_count {
            n @ (2 | 3) => {
                let unit = self.unit_at(i, n == 3);
                self.move_to(unit.start, cx);
                self.select_to(unit.end, cx);
                self.selecting = true;
                self.anchor = Some((unit, n == 3));
            }
            _ => {
                self.selecting = true;
                self.anchor = None;
                if e.modifiers.shift {
                    self.select_to(i, cx)
                } else {
                    self.move_to(i, cx)
                }
            }
        }
    }

    /// Right click: the note's menu. Outside the selection, the cursor first moves under
    /// the pointer, so that "Paste" and "Link" act where the click was.
    pub(super) fn menu_down(&mut self, e: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.rows_revision != self.revision {
            return;
        }
        // On a table handle: it takes its row or column, and the menu speaks of it.
        if let Some((table, part)) = self.part_at(e.position)
            && matches!(part, Part::Col(_) | Part::Row(_) | Part::Corner)
        {
            self.pick_part(table, part, cx);
            return cx.emit(EditorEvent::Menu(e.position, None, false));
        }
        let i = self.index_at(e.position);
        if !(self.sel.start..=self.sel.end).contains(&i) {
            self.move_to(i, cx);
        }
        let lr = self.line_range(i);
        let link = md::links(&self.content[lr.clone()]).into_iter().find(|(r, _)| (r.start..=r.end).contains(&(i - lr.start)));
        let col = i - lr.start;
        let commented = md::comments(&self.content[lr]).iter().any(|(whole, ..)| (whole.start..=whole.end).contains(&col));
        cx.emit(EditorEvent::Menu(e.position, link.map(|(_, link)| link), commented));
    }

    /// Word (or whole line) around byte `i`.
    pub(super) fn unit_at(&self, i: usize, by_line: bool) -> Range<usize> {
        let lr = self.line_range(i);
        if by_line {
            return lr;
        }
        let col = i - lr.start;
        let word = self.content[lr.clone()]
            .split_word_bound_indices()
            .find(|(s, w)| (*s..=s + w.len()).contains(&col))
            .map_or(col..col, |(s, w)| s..s + w.len());
        lr.start + word.start..lr.start + word.end
    }

    /// Dragging extends the selection to `i`; after a double or triple click,
    /// by words or whole lines, the grabbed word or line staying selected.
    pub(super) fn drag_to(&mut self, i: usize, cx: &mut Context<Self>) {
        let Some((first, by_line)) = self.anchor.clone() else {
            return self.select_to(i, cx);
        };
        let unit = self.unit_at(i, by_line);
        self.reversed = unit.start < first.start;
        self.sel = if self.reversed { unit.start..first.end } else { first.start..unit.end.max(first.end) };
        self.goal_x = None;
        self.reveal = true;
        cx.notify();
    }

    pub(super) fn mouse_move(&mut self, e: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        // ponytail: `rows` is rebuilt at paint, so right after an edit its offsets are those of the old text and
        // slicing `content` with them panics (a panic inside a window callback aborts the process on Windows).
        // The event is dropped; the next frame lays out again and the pointer's next move is handled.
        if self.rows_revision != self.revision {
            return;
        }
        // A button released outside the window sends no `mouse_up`: we do not
        // keep selecting with the pointer free.
        if self.selecting && e.pressed_button != Some(MouseButton::Left) {
            self.selecting = false;
        }
        if self.selecting {
            self.drag_to(self.index_at(e.position), cx);
        }
        self.aim_hand(e.position, e.modifiers, cx);
        if self.handles.drag.is_some() {
            if e.pressed_button != Some(MouseButton::Left) {
                self.handles.drag = None;
                return cx.notify();
            }
            return self.drag_block(e.position, cx);
        }
        self.aim_block(e.position, cx);
        // A dragged handle: the line where it would land follows the pointer, after a few pixels.
        if let Some(moving) = &mut self.moving {
            if e.pressed_button != Some(MouseButton::Left) {
                self.moving = None;
            } else if (e.position - moving.from).magnitude() > 4. {
                let at = self.geos.iter().find(|g| g.start == moving.table).map(|geo| geo.boundary(moving.part, e.position));
                moving.to = at;
            }
            return cx.notify();
        }
        let part = self.part_at(e.position);
        let near = self.geos.iter().map(|geo| geo.near(e.position)).collect::<Vec<_>>();
        let before = self.geos.iter().map(|geo| geo.near(self.pointer)).collect::<Vec<_>>();
        self.pointer = e.position;
        if part != self.part_hover || near != before {
            self.part_hover = part;
            cx.notify();
        }
        let reason = self.reason_at(e.position);
        if reason != self.reason {
            self.reason = reason;
            cx.notify();
        }
        let hover = (self.copy_at(e.position), self.view_at(e.position), self.big_at(e.position));
        if hover != (self.hover, self.view_hover, self.big_hover) {
            (self.hover, self.view_hover, self.big_hover) = hover;
            cx.notify();
        }
        if let Some(cell) = self.grid_at(e.position)
            && self.grid != Some(cell)
        {
            self.grid = Some(cell);
            self.grid_typed.clear();
            cx.notify();
        }
        // A table shows its "+" buttons as soon as one gets close.
        let bar = self.plus_at(e.position);
        let near = |(range, frame): &(Range<usize>, Bounds<Pixels>)| {
            frame.dilate(px(20.)).contains(&e.position).then_some((range.start, None))
        };
        let over = bar.map(|(at, bar)| (at, Some(bar))).or_else(|| self.tables.iter().find_map(near));
        if over != self.table_hover {
            self.table_hover = over;
            cx.notify();
        }
    }

    pub(super) fn mouse_up(&mut self, e: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.selecting = false;
        self.drop_block(cx);
        if let Some(moving) = self.moving.take() {
            self.drop_part(moving, cx);
            cx.notify();
        }
        self.aim_hand(e.position, e.modifiers, cx);
    }

    pub(super) fn scroll(&mut self, e: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        let delta = e.delta.pixel_delta(px(26.));
        // Shift + wheel, or horizontal touchpad gesture: a table too wide scrolls.
        let (dx, dy) = if e.modifiers.shift { (delta.y, delta.x) } else { (delta.x, delta.y) };
        if let Some((range, _)) = self.tables.iter().find(|(_, frame)| frame.contains(&e.position)) {
            *self.table_x.entry(range.start).or_default() -= dx;
        }
        self.scroll_y -= dy;
        cx.notify();
    }
}
