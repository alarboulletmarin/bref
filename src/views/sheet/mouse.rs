//! The mouse: clicking and dragging cells, column edges and scroll bars, the wheel.

use super::*;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Drag {
    None,
    Cells,
    Rows,
    Cols,
    /// Scrollbar: vertical or not.
    Bar(bool),
    /// The right edge of column `col`, grabbed at `from` (x in the window) when it was `start` wide and
    /// set to `before` (Escape puts it back).
    Width { col: usize, from: f32, start: f32, before: Option<f32>, at: f32 },
}

pub(super) enum Hit {
    Cell(usize, usize),
    Row(usize),
    Col(usize),
    Corner,
    Bar(bool),
    /// The handle on the right edge of a column, in the top row.
    Edge(usize),
}

impl Sheet {
    pub(super) fn hit(&self, p: Point<Pixels>) -> Hit {
        let (x, y) = (f32::from(p.x - self.body.origin.x), f32::from(p.y - self.body.origin.y));
        let (w, h) = (f32::from(self.body.size.width), f32::from(self.body.size.height));
        let (mx, my) = self.max_scroll();
        if my > 0. && x > w - EDGE && y > ROW {
            return Hit::Bar(true);
        }
        if mx > 0. && y > h - EDGE && x > GUTTER {
            return Hit::Bar(false);
        }
        if y < ROW && x >= GUTTER {
            // The handle wins over the column only in a narrow band around its right edge; the edge of
            // the gutter is not one.
            let at = x - GUTTER + self.scroll.0;
            let c = self.col_at(at);
            let near = |edge: f32| (edge - at).abs() <= HANDLE;
            if near(self.xs[c + 1]) {
                return Hit::Edge(c);
            }
            if c > 0 && near(self.xs[c]) {
                return Hit::Edge(c - 1);
            }
        }
        let col = (x >= GUTTER).then(|| self.col_at(x - GUTTER + self.scroll.0));
        let last = self.table.rows().max(1) - 1;
        let row = (y >= ROW).then(|| (self.first() + ((y - ROW + self.scroll.1) / ROW) as usize).min(last));
        match (row, col) {
            (None, None) => Hit::Corner,
            (None, Some(c)) => Hit::Col(c),
            (Some(r), None) => Hit::Row(r),
            (Some(r), Some(c)) => Hit::Cell(r, c),
        }
    }

    pub(super) fn press(&mut self, e: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus);
        self.finish(cx);
        let extend = e.modifiers.shift;
        let hit = self.hit(e.position);
        // Ctrl+click on a cell, a row or a column: what is selected stays so,
        // another selection starts. A plain click keeps only its own.
        let adding = e.modifiers.secondary();
        let mut kept = std::mem::take(&mut self.more);
        match hit {
            Hit::Cell(..) | Hit::Row(_) | Hit::Col(_) if adding && !extend => kept.push((self.anchor, self.cursor)),
            Hit::Bar(_) | Hit::Edge(_) => {}
            _ if adding || extend => {}
            _ => kept.clear(),
        }
        if matches!(hit, Hit::Corner) {
            kept.clear();
        }
        match hit {
            Hit::Cell(r, c) => {
                self.drag = Drag::Cells;
                self.select_to((r, c), extend, cx);
                if e.click_count >= 2 {
                    self.begin_edit(None, cx);
                }
            }
            Hit::Row(r) => {
                self.drag = Drag::Rows;
                self.pick_row(r, extend, cx);
            }
            Hit::Col(c) => {
                self.drag = Drag::Cols;
                self.pick_col(c, extend, cx);
                // Double-click on the header: write in its cell.
                if e.click_count >= 2 && self.format.header {
                    self.select_to((0, c), false, cx);
                    self.begin_edit(None, cx);
                }
            }
            Hit::Corner => self.select_all(cx),
            // Double-click: the column fits its text.
            Hit::Edge(c) if e.click_count >= 2 => {
                self.fit_cols(&[c]);
                cx.notify();
            }
            Hit::Edge(col) => {
                let x = f32::from(e.position.x);
                self.drag = Drag::Width { col, from: x, start: self.widths[col], before: self.chosen.get(col).copied().flatten(), at: x };
                cx.notify();
            }
            Hit::Bar(vertical) => {
                self.drag = Drag::Bar(vertical);
                self.drag_bar(e.position, vertical, cx);
            }
        }
        self.more = kept;
    }

    pub(super) fn drag_to(&mut self, e: &MouseMoveEvent, cx: &mut Context<Self>) {
        if e.pressed_button != Some(MouseButton::Left) {
            return self.let_go(cx);
        }
        if let Drag::Width { col, from, start, before, .. } = self.drag {
            let x = f32::from(e.position.x);
            self.drag = Drag::Width { col, from, start, before, at: x };
            // Only this column changes: no measure, the frame stays short on a big file.
            self.set_width(col, start + x - from, false);
            return cx.notify();
        }
        let at = self.hit(e.position);
        match (self.drag, at) {
            (Drag::Cells, Hit::Cell(r, c)) => self.select_to((r, c), true, cx),
            (Drag::Rows, Hit::Cell(r, _) | Hit::Row(r)) => self.pick_row(r, true, cx),
            (Drag::Cols, Hit::Cell(_, c) | Hit::Col(c)) => self.pick_col(c, true, cx),
            (Drag::Bar(vertical), _) => self.drag_bar(e.position, vertical, cx),
            _ => {}
        }
    }

    /// The button is released, here or outside the window: a width being set is kept where it is.
    pub(super) fn let_go(&mut self, cx: &mut Context<Self>) {
        if matches!(self.drag, Drag::Width { .. }) {
            self.keep_widths();
            cx.notify();
        }
        self.drag = Drag::None;
    }

    /// Escape during a drag of a column edge: the width goes back to what it was. False when no edge is dragged.
    pub(super) fn cancel_width(&mut self, cx: &mut Context<Self>) -> bool {
        let Drag::Width { col, before, .. } = self.drag else {
            return false;
        };
        self.drag = Drag::None;
        self.chosen[col] = before;
        self.measure();
        self.clamp();
        cx.notify();
        true
    }

    /// The scrollbar follows the pointer.
    pub(super) fn drag_bar(&mut self, p: Point<Pixels>, vertical: bool, cx: &mut Context<Self>) {
        let (mx, my) = self.max_scroll();
        let (w, h) = (f32::from(self.body.size.width), f32::from(self.body.size.height));
        if vertical {
            let y = f32::from(p.y - self.body.origin.y) - ROW;
            self.scroll.1 = (y / (h - ROW).max(1.)).clamp(0., 1.) * my;
        } else {
            let x = f32::from(p.x - self.body.origin.x) - GUTTER;
            self.scroll.0 = (x / (w - GUTTER).max(1.)).clamp(0., 1.) * mx;
        }
        cx.notify();
    }

    /// The wheel scrolls; with Shift, sideways.
    pub(super) fn wheel(&mut self, e: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let mut delta = e.delta.pixel_delta(px(ROW));
        if e.modifiers.shift && delta.x == px(0.) {
            delta = Point { x: delta.y, y: px(0.) };
        }
        self.scroll = (self.scroll.0 - f32::from(delta.x), self.scroll.1 - f32::from(delta.y));
        self.clamp();
        cx.notify();
    }

    /// The whole column `c` (with `extend`, up to the anchor's). The view does not move: the active cell is
    /// the top one, not the last row, where autoscroll would take us.
    pub(super) fn pick_col(&mut self, c: usize, extend: bool, cx: &mut Context<Self>) {
        self.finish(cx);
        let from = if extend { self.anchor.1 } else { c };
        (self.anchor, self.cursor) = ((self.last_row(), from), (0, c));
        cx.notify();
    }

    /// The whole row `r`, without scrolling to the last column.
    pub(super) fn pick_row(&mut self, r: usize, extend: bool, cx: &mut Context<Self>) {
        self.finish(cx);
        let from = if extend { self.anchor.0 } else { r };
        (self.anchor, self.cursor) = ((from, self.cols() - 1), (r, 0));
        cx.notify();
    }
}
