//! Changing the text: one undoable edit at a time, applied to every cursor.

use super::*;

/// States of the text that can be restored, oldest to newest. Only the
/// last is kept whole: each of the others only keeps what sets it
/// apart from the next, i.e. a few bytes per keystroke rather than a copy of the note.
#[derive(Default)]
pub(super) struct History {
    pub(super) last: Option<(String, Range<usize>)>,
    /// (start of the difference, its length in the next state, its text here, selection).
    pub(super) earlier: Vec<(usize, usize, String, Range<usize>)>,
}

impl History {
    pub(super) fn push(&mut self, text: String, sel: Range<usize>) {
        if let Some((old, old_sel)) = self.last.replace((text, sel)) {
            let new = self.last.as_ref().unwrap().0.as_bytes();
            let old_bytes = old.as_bytes();
            let mut start = old_bytes.iter().zip(new).take_while(|(a, b)| a == b).count();
            while !old.is_char_boundary(start) {
                start -= 1;
            }
            let room = old.len().min(new.len()) - start;
            let mut tail = old_bytes.iter().rev().zip(new.iter().rev()).take(room).take_while(|(a, b)| a == b).count();
            while !old.is_char_boundary(old.len() - tail) {
                tail -= 1;
            }
            self.earlier.push((start, new.len() - tail - start, old[start..old.len() - tail].to_string(), old_sel));
            if self.earlier.len() >= 200 {
                self.earlier.remove(0);
            }
        }
    }

    pub(super) fn pop(&mut self) -> Option<(String, Range<usize>)> {
        let out = self.last.take()?;
        if let Some((start, len, own, sel)) = self.earlier.pop() {
            let mut text = out.0.clone();
            text.replace_range(start..start + len, &own);
            self.last = Some((text, sel));
        }
        Some(out)
    }
}

impl Editor {
    // ponytail: each undo step (200 max) copies the note once to
    // get the difference; move to an operation log if notes
    // exceed a MB.
    pub(super) fn push_undo(&mut self, boundary: bool) {
        let stale = self
            .last_edit
            .is_none_or(|t| t.elapsed() > Duration::from_millis(600));
        if (boundary || stale) && !self.held {
            self.undo.push(self.content.clone(), self.sel.clone());
        }
        self.redo = History::default();
        self.last_edit = Some(Instant::now());
    }

    /// To call before replacing `range` in the text: the layout will know what moved.
    pub(super) fn damaged(&mut self, range: &Range<usize>) {
        let len = self.content.len();
        let (prefix, suffix) = (range.start, len - range.end);
        self.edited_at = range.start;
        self.dmg = Some(match self.dmg {
            None => Damage { old_len: len, prefix, suffix },
            Some(d) => Damage { prefix: d.prefix.min(prefix), suffix: d.suffix.min(suffix), ..d },
        });
    }

    pub(super) fn changed(&mut self, cx: &mut Context<Self>) {
        self.bar.swatches = false;
        self.version += 1;
        self.revision += 1;
        self.marked = None;
        self.copied = None;
        self.grid = None;
        self.ac_index = 0;
        self.goal_x = None;
        self.reveal = true;
        cx.emit(EditorEvent::Changed);
        cx.notify();
    }

    /// Replaces `range` with `text`, keeping the selection as well as possible.
    pub(super) fn splice(&mut self, range: Range<usize>, text: &str, cx: &mut Context<Self>) {
        self.push_undo(text.contains('\n') || text == " ");
        let scopes = self.scopes_before(&range, text);
        self.damaged(&range);
        self.content.replace_range(range.clone(), text);
        let map = |p: usize| {
            if p <= range.start {
                p
            } else if p >= range.end {
                p + text.len() - range.len()
            } else {
                range.start + text.len()
            }
        };
        self.sel = map(self.sel.start)..map(self.sel.end);
        self.follow(map);
        self.changed(cx);
        if let Some((table, old)) = scopes {
            self.rename_scopes(table, old, cx);
        }
    }

    /// The other cursors follow a change of the text: `map` says where a byte of the old text
    /// lands. Every change calls it (`splice`, or the rewrites of a whole block: a realigned
    /// table, emptied cells, reindented lines), or the other cursors point into the wrong text.
    pub(super) fn follow(&mut self, map: impl Fn(usize) -> usize) {
        self.more.iter_mut().for_each(|(sel, _)| *sel = map(sel.start)..map(sel.end));
    }

    /// Makes each cursor do `act`: each in turn becomes the main cursor,
    /// while `splice` keeps the others in place. The whole is a single undo
    /// step, the first one's.
    // ponytail: the column aimed at by Up and Down (`goal_x`) is not kept per cursor: it
    // is recomputed at each step. Keep it per cursor if the drift bothers on lines
    // of uneven lengths.
    pub(super) fn each(&mut self, cx: &mut Context<Self>, act: impl Fn(&mut Self, &mut Context<Self>)) {
        if self.more.is_empty() {
            return act(self, cx);
        }
        self.goal_x = None;
        act(self, cx);
        self.held = true;
        for i in 0..self.more.len() {
            // A gesture that brings back a single cursor (a reloaded note) stops the round.
            if i >= self.more.len() {
                break;
            }
            let swap = |this: &mut Self| {
                std::mem::swap(&mut this.sel, &mut this.more[i].0);
                std::mem::swap(&mut this.reversed, &mut this.more[i].1);
            };
            swap(self);
            self.goal_x = None;
            act(self, cx);
            if i < self.more.len() {
                swap(self);
            }
        }
        self.held = false;
        self.goal_x = None;
        self.tidy();
        cx.notify();
    }

    /// The selected text.
    #[cfg(test)]
    pub fn selected_text(&self) -> String {
        self.content[self.sel.clone()].to_string()
    }

    /// Replaces `range` with `text` and puts the cursor right after.
    pub(super) fn edit(&mut self, range: Range<usize>, text: &str, cx: &mut Context<Self>) {
        let end = range.start + text.len();
        self.splice(range, text, cx);
        self.sel = end..end;
        self.reversed = false;
    }

    pub(super) fn renumber(&mut self, cx: &mut Context<Self>) {
        if let Some((text, cursor)) = md::renumber(&self.content, self.cursor()) {
            self.epoch += 1;
            self.content = text;
            self.edited_at = usize::MAX;
            self.sel = cursor..cursor;
            self.reversed = false;
            self.more.clear();
            self.changed(cx);
        }
    }

    pub fn restore(&mut self, redo: bool, cx: &mut Context<Self>) {
        let (from, to) = if redo {
            (&mut self.redo, &mut self.undo)
        } else {
            (&mut self.undo, &mut self.redo)
        };
        let Some((text, sel)) = from.pop() else {
            return;
        };
        self.epoch += 1;
        to.push(std::mem::replace(&mut self.content, text), self.sel.clone());
        self.sel = sel;
        self.reversed = false;
        self.more.clear();
        self.last_edit = None;
        self.edited_at = usize::MAX;
        self.changed(cx);
    }

    pub(super) fn delete(&mut self, m: Motion, cx: &mut Context<Self>) {
        if self.clear_pick(cx) {
            return;
        }
        if self.grid.is_some() {
            self.grid_typed.pop();
            self.type_grid("", cx);
            return;
        }
        if self.clear_cells(cx) {
            return;
        }
        let range = if self.sel.is_empty() {
            let (c, t) = (self.cursor(), self.plain_target(m, px(0.)));
            let mut range = c.min(t)..c.max(t);
            // In a table, deleting does not cross from one cell to another: that would merge
            // the columns. A `\|` is deleted as one block, as it is shown.
            if let Some((_, k)) = self.table_cells(c) {
                let line = self.line_range(c);
                let bars = md::bars(&self.content[line.clone()]);
                let (from, to) = (line.start + bars[k] + 1, bars.get(k + 1).map_or(line.end, |b| line.start + b));
                range = range.start.max(from)..range.end.min(to);
                if range.is_empty() {
                    return;
                }
                range = self.off_escape(range.start, false)..self.off_escape(range.end, true);
            }
            range
        } else {
            self.sel.clone()
        };
        self.edit(range, "", cx);
        self.renumber(cx);
        self.realign(cx);
    }

    pub(super) fn newline(&mut self, cx: &mut Context<Self>) {
        if self.accept_completion(cx) {
            return;
        }
        if let Some((cols, rows)) = self.grid {
            return self.insert_table(cols, rows, cx);
        }
        // In a table: one more row under the cursor's; on a
        // last row left empty, remove it and leave the table.
        // A very big table is not reparsed: Enter adds an empty row under the current one (under the dashes
        // when on the header), without touching the rest.
        if self.sel.is_empty() && self.in_big(self.cursor()) {
            let line = self.line_range(self.cursor());
            let cells = md::cells(&self.content[line.clone()]).len().max(1);
            let after = match self.row_at(self.cursor()).and_then(|r| self.big.iter().find(|b| b.rows.contains(&r)).map(|b| r - b.rows.start)) {
                Some(0) if line.end < self.content.len() => self.line_range(line.end + 1).end,
                _ => line.end,
            };
            self.push_undo(true);
            self.edit(after..after, &format!("\n|{}", " |".repeat(cells)), cx);
            return self.move_to(after + 3, cx);
        }
        if let Some((range, mut rows, (r, _))) = self.table_at(self.cursor()) {
            if r >= 2 && r + 1 == rows.len() && rows[r].iter().all(String::is_empty) {
                rows.pop();
                let text = format!("{}\n", self.format_table(&range, &rows));
                self.push_undo(true);
                return self.edit(range, &text, cx);
            }
            let at = r.max(1) + 1;
            rows.insert(at, vec![String::new(); rows[0].len()]);
            return self.set_table(range, &rows, (at, 0), cx);
        }
        if !self.sel.is_empty() {
            return self.edit(self.sel.clone(), "\n", cx);
        }
        let c = self.cursor();
        let lr = self.line_range(c);
        let line = self.content[lr.clone()].to_string();
        let in_code = self.in_code(lr.start);
        // Opening a code block: also put the closing fence.
        if md::is_fence(&line)
            && !in_code
            && c == lr.end
            && self.fences_before(self.content.len()) % 2 == 1
        {
            self.edit(c..c, "\n\n```", cx);
            return self.move_to(c + 1, cx);
        }
        match md::on_enter(&line, c - lr.start, in_code) {
            Enter::Insert(text) => self.edit(c..c, &text, cx),
            Enter::Clear => self.edit(lr, "", cx),
        }
        self.renumber(cx);
    }

    pub(super) fn shift_lines(&mut self, indent: bool, cx: &mut Context<Self>) {
        if indent && self.accept_completion(cx) {
            return;
        }
        if self.table_step(indent, cx) {
            return;
        }
        let first = self.line_range(self.sel.start);
        if indent && self.sel.is_empty() {
            let line = &self.content[first.clone()];
            let (kind, _) = md::classify(line, self.in_code(first.start));
            if !matches!(kind, Kind::Bullet | Kind::Ordered | Kind::Task(_)) {
                return self.edit(self.sel.clone(), md::INDENT, cx);
            }
        }
        let block = first.start..self.line_range(self.sel.end).end;
        let sel = self.sel.clone();
        let mut deltas = Vec::new();
        let shifted: Vec<String> = self.content[block.clone()]
            .split('\n')
            .map(|l| {
                let new = if indent {
                    format!("{}{l}", md::INDENT)
                } else if let Some(rest) = l.strip_prefix('\t') {
                    rest.to_string()
                } else {
                    let n = l.bytes().take(md::INDENT.len()).take_while(|&b| b == b' ').count();
                    l[n..].to_string()
                };
                deltas.push(new.len() as isize - l.len() as isize);
                new
            })
            .collect();
        self.push_undo(true);
        let total: isize = deltas.iter().sum();
        let old = self.content[block.clone()].to_string();
        // A byte keeps its place in its line; one in the removed indent goes to the line's start.
        self.follow(|p| {
            if p < block.start {
                return p;
            }
            if p > block.end {
                return (p as isize + total) as usize;
            }
            let l = old[..p - block.start].matches('\n').count();
            let start = block.start + old.split('\n').take(l).map(|x| x.len() + 1).sum::<usize>();
            let shift: isize = deltas[..l].iter().sum();
            ((start as isize + shift) + ((p - start) as isize + deltas[l]).max(0)) as usize
        });
        self.damaged(&block);
        self.content.replace_range(block.clone(), &shifted.join("\n"));
        let start = (sel.start as isize + deltas[0]).max(block.start as isize) as usize;
        let end = (sel.end as isize + total).max(start as isize) as usize;
        self.sel = start..end;
        self.changed(cx);
        self.renumber(cx);
    }

    /// A filled template, in place of the selection: the cursor goes to its mark, a single undo step.
    pub fn insert_filled(&mut self, text: &str, cursor: usize, cx: &mut Context<Self>) {
        let at = self.sel.start;
        self.push_undo(true);
        self.edit(self.sel.clone(), text, cx);
        self.move_to(at + cursor.min(text.len()), cx);
    }

    /// The number of text changes: a drop that took time to copy knows before writing whether the note moved.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Lines put at the start of line `at` (or after the last), typed or dropped:
    /// a single undo step, the cursor at their end.
    pub fn insert_lines(&mut self, at: usize, lines: &[String], cx: &mut Context<Self>) {
        let at = self.clamp(at);
        let before = if at > 0 && !self.content[..at].ends_with('\n') { "\n" } else { "" };
        let after = if at < self.content.len() { "\n" } else { "" };
        let text = format!("{before}{}{after}", lines.join("\n"));
        self.push_undo(true);
        self.edit(at..at, &text, cx);
        self.move_to(at + text.len() - after.len(), cx);
    }
}
