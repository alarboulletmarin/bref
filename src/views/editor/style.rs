//! Inline styles and line kinds: bold, italic, highlight…, lists, tasks, checkboxes.

use super::*;

impl Editor {
    /// The selection's lines (or the cursor's) written in style `to`: a single undo
    /// step, the selection keeps its text. A line the selection only touches
    /// at its start is not part of it.
    pub fn restyle(&mut self, to: ListStyle, cx: &mut Context<Self>) {
        let sel = self.sel.clone();
        let first = self.line_range(sel.start);
        let touches = !sel.is_empty() && self.content[..sel.end].ends_with('\n');
        let block = first.start..self.line_range(if touches { sel.end - 1 } else { sel.end }).end;
        if self.in_code(first.start) || block.is_empty() {
            return;
        }
        let old = self.content[block.clone()].to_string();
        let (new, spans) = md::restyle(&old, to);
        if new == old {
            return;
        }
        // Position `at` after the change: in a line's content, it follows it; in its
        // marker, it goes to the start of the new content.
        let moved = |at: usize| {
            let at = at.clamp(block.start, block.end) - block.start;
            let (mut before, mut after) = (0, 0);
            for ((was, now), &(was_at, now_at)) in old.split('\n').zip(new.split('\n')).zip(&spans) {
                if at <= before + was.len() {
                    let within = at - before;
                    return block.start + after + if within >= was_at { now_at + within - was_at } else { within.min(now_at) };
                }
                before += was.len() + 1;
                after += now.len() + 1;
            }
            block.start + new.len()
        };
        let sel = moved(sel.start)..moved(sel.end);
        self.push_undo(true);
        self.damaged(&block);
        self.content.replace_range(block, &new);
        self.sel = sel;
        self.changed(cx);
        self.renumber(cx);
    }

    /// Text, bullets, numbered, tasks, then text: the cursor's line (the first of the
    /// selection) says where we are, and the whole selection moves to the next.
    pub(super) fn cycle_list(&mut self, cx: &mut Context<Self>) {
        let line = self.line_range(self.sel.start);
        let kind = md::classify(&self.content[line.clone()], self.in_code(line.start)).0;
        let next = match kind {
            Kind::Bullet => ListStyle::Ordered,
            Kind::Ordered => ListStyle::Task,
            Kind::Task(_) => ListStyle::Text,
            _ => ListStyle::Bullet,
        };
        self.restyle(next, cx);
    }

    /// Checks or unchecks the `[ ]` box that starts at byte `at`.
    pub(super) fn flip_box(&mut self, at: usize, cx: &mut Context<Self>) {
        let done = self.content.as_bytes()[at + 1] != b' ';
        self.push_undo(true);
        self.splice(at + 1..at + 2, if done { " " } else { "x" }, cx);
    }

    /// Checkbox under the pointer: its start. It is on the pointer's visual line,
    /// between its two edges, and not in a code block.
    pub(super) fn checkbox_at(&self, pos: Point<Pixels>) -> Option<usize> {
        let row = &self.rows[self.row_at(self.index_at(pos))?];
        if matches!(row.kind, Kind::Code | Kind::Fence) {
            return None;
        }
        let line = &self.content[row.start..row.start + row.len];
        let local = point(pos.x - self.origin.x, pos.y - self.origin.y - row.y - row.pad);
        md::checkboxes(line, 0).into_iter().find_map(|b| {
            let (from, to) = (row.pos(b.start), row.pos(b.end));
            let inside = local.x >= from.x && local.x <= to.x && local.y >= from.y && local.y < from.y + row.lh;
            (inside && from.y == to.y).then_some(row.start + b.start)
        })
    }

    pub(super) fn toggle_task(&mut self, cx: &mut Context<Self>) {
        let lr = self.line_range(self.cursor());
        let line = &self.content[lr.clone()];
        let (kind, marker) = md::classify(line, self.in_code(lr.start));
        let at = lr.start + marker;
        // A box written in the text or in a cell, under the cursor or in its cell.
        let c = self.cursor();
        let cell = self.table_cells(c).map(|(cells, k)| cells[k].clone());
        let near = |b: &Range<usize>| match &cell {
            Some(cell) => b.start + lr.start >= cell.start && b.end + lr.start <= cell.end,
            None => (b.start + lr.start..=b.end + lr.start).contains(&c),
        };
        if !self.in_code(lr.start) && !matches!(kind, Kind::Task(_) | Kind::Bullet) {
            if let Some(b) = md::checkboxes(line, 0).into_iter().find(near) {
                return self.flip_box(lr.start + b.start, cx);
            }
            // A cell without a box gets one.
            if let Some(cell) = cell {
                self.push_undo(true);
                let text = if cell.is_empty() { "[ ]" } else { "[ ] " };
                self.edit(cell.start..cell.start, text, cx);
                return self.realign(cx);
            }
        }
        match kind {
            Kind::Task(done) => self.splice(at - 3..at - 2, if done { " " } else { "x" }, cx),
            Kind::Bullet => {
                self.splice(at..at, "[ ] ", cx);
                if self.sel.end == at {
                    self.move_to(at + 4, cx);
                }
            }
            _ => {}
        }
    }

    /// The spans `mark` wraps: the text of each line of the selection, without edge whitespace
    /// and bullet, without code blocks; the text of each cell on a table line, whatever the
    /// selection covers (a `|` is never inside a span), or of each cell a handle took; the lone
    /// cursor, outside a code block, is one, empty.
    pub(super) fn mark_parts(&self) -> Vec<Range<usize>> {
        if let Some(cells) = self.pick_cells() {
            return cells.into_iter().filter_map(|c| self.trimmed(c.start, c.end)).collect();
        }
        let sel = self.sel.clone();
        let first = self.line_range(sel.start);
        let mut code = self.in_code(first.start);
        let mut parts = Vec::new();
        if sel.is_empty() {
            if !code {
                parts.push(sel);
            }
            return parts;
        }
        let mut at = first.start;
        while at <= sel.end {
            let lr = self.line_range(at);
            let line = &self.content[lr.clone()];
            if md::is_fence(line) {
                code = !code && self.content[lr.end..].lines().any(md::is_fence);
            } else if !code && self.is_table_line(&lr) {
                if !self.is_dash_row(&lr) {
                    parts.extend(self.line_cells(&lr, &sel).into_iter().filter_map(|c| self.trimmed(c.start, c.end)));
                }
            } else if !code {
                let (a, b) = (sel.start.max(lr.start + md::classify(line, false).1), sel.end.min(lr.end));
                parts.extend(self.trimmed(a, b));
            }
            at = lr.end + 1;
        }
        parts
    }

    /// `a..b` without its edge whitespace; `None` if nothing is left.
    pub(super) fn trimmed(&self, a: usize, b: usize) -> Option<Range<usize>> {
        let text = self.content.get(a..b)?;
        let lead = text.len() - text.trim_start().len();
        let kept = text.trim().len();
        (kept > 0).then_some(a + lead..a + lead + kept)
    }

    /// The signs of style `flag` around `part` (text bytes), if it is wholly inside a span of that style.
    pub(super) fn cut(&self, part: &Range<usize>, flag: u16) -> Option<(Range<usize>, Range<usize>)> {
        let lr = self.line_range(part.start);
        let line = &self.content[lr.clone()];
        let flags = md::flags(line, md::classify(line, false).1);
        let (open, close) = md::unmark(line, &flags, part.start - lr.start..part.end - lr.start, flag)?;
        Some((lr.start + open.start..lr.start + open.end, lr.start + close.start..lr.start + close.end))
    }

    /// Puts style `flag` on the selection, or removes it when everything it holds already has it
    /// (see `md::unmark`). A lone cursor puts both signs and sits between them. Over
    /// several lines or cells, each gets its own signs, only where it lacks them: a style does
    /// not run from one line to the next, nor across a `|`.
    pub(super) fn mark(&mut self, flag: u16, cx: &mut Context<Self>) {
        let Some(&(open, close)) = md::delimiters(flag).first() else { return };
        self.aligned(cx, |this, cx| {
            let parts = this.mark_parts();
            if parts.is_empty() {
                return;
            }
            let cuts: Vec<_> = parts.iter().map(|part| this.cut(part, flag)).collect();
            this.push_undo(true);
            // Everything already has it: the signs go, closing ones first so the bytes stay right.
            if cuts.iter().all(Option::is_some) {
                for (open, close) in cuts.into_iter().flatten().rev() {
                    this.splice(close, "", cx);
                    this.splice(open, "", cx);
                }
                return;
            }
            this.restyle_parts(&parts, &cuts, open, close, false, cx);
        });
    }

    /// Puts `open` and `close` around each part that has no signs (`cuts`, see `cut`); with
    /// `replace`, the parts that have some get these instead. The selection goes from the
    /// first part to the last, signs outside.
    pub(super) fn restyle_parts(&mut self, parts: &[Range<usize>], cuts: &[Option<(Range<usize>, Range<usize>)>], open: &str, close: &str, replace: bool, cx: &mut Context<Self>) {
        for (part, cut) in parts.iter().zip(cuts).rev() {
            match cut {
                Some((o, c)) if replace => {
                    self.splice(c.clone(), close, cx);
                    self.splice(o.clone(), open, cx);
                }
                Some(_) => {}
                None => {
                    self.splice(part.end..part.end, close, cx);
                    self.splice(part.start..part.start, open, cx);
                }
            }
        }
        // Where the first and the last text land: each part moves by what the signs before it added.
        let (mut shift, mut moved) = (0isize, Vec::new());
        for cut in cuts {
            let (o, c) = match cut {
                Some((o, c)) if replace => (open.len() as isize - o.len() as isize, close.len() as isize - c.len() as isize),
                Some(_) => (0, 0),
                None => (open.len() as isize, close.len() as isize),
            };
            moved.push(shift + o);
            shift += o + c;
        }
        let at = |i: usize, p: usize| (p as isize + moved[i]) as usize;
        let last = parts.len() - 1;
        self.sel = at(0, parts[0].start)..at(last, parts[last].end);
        self.reversed = false;
    }
}
