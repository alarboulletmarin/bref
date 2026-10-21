//! Blocks: the handle left of each block, its menu, and dragging blocks around.

use super::*;

/// Block handles: the block under the pointer (at its text version), the one being dragged, the
/// one the handle's menu targets, and the zones of the handle and of the « + ».
#[derive(Default)]
pub(super) struct BlockHandles {
    pub(super) hover: Option<(Range<usize>, u64)>,
    pub(super) drag: Option<BlockDrag>,
    pub(super) menu: Option<(Range<usize>, u64)>,
    pub(super) hitboxes: Option<(Hitbox, Option<Hitbox>)>,
}

/// A gesture on a block of the note, from its handle, the keyboard or the palette: transform it,
/// duplicate it, move it by one block, delete it (see `block`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum BlockOp {
    Turn(block::Turn),
    Duplicate,
    Up,
    Down,
    Delete,
}

impl BlockOp {
    pub const ALL: [BlockOp; 13] = [
        BlockOp::Turn(block::Turn::Text),
        BlockOp::Turn(block::Turn::Heading(1)),
        BlockOp::Turn(block::Turn::Heading(2)),
        BlockOp::Turn(block::Turn::Heading(3)),
        BlockOp::Turn(block::Turn::Bullet),
        BlockOp::Turn(block::Turn::Ordered),
        BlockOp::Turn(block::Turn::Task),
        BlockOp::Turn(block::Turn::Quote),
        BlockOp::Turn(block::Turn::Code),
        BlockOp::Duplicate,
        BlockOp::Up,
        BlockOp::Down,
        BlockOp::Delete,
    ];

    pub fn label(self) -> &'static str {
        use block::Turn;
        match self {
            BlockOp::Turn(Turn::Text) => tr("Text", "Texte"),
            BlockOp::Turn(Turn::Heading(1)) => tr("Heading 1", "Titre 1"),
            BlockOp::Turn(Turn::Heading(2)) => tr("Heading 2", "Titre 2"),
            BlockOp::Turn(Turn::Heading(_)) => tr("Heading 3", "Titre 3"),
            BlockOp::Turn(Turn::Bullet) => tr("Bulleted list", "Liste à puces"),
            BlockOp::Turn(Turn::Ordered) => tr("Numbered list", "Liste numérotée"),
            BlockOp::Turn(Turn::Task) => tr("Task list", "Liste de tâches"),
            BlockOp::Turn(Turn::Quote) => tr("Quote", "Citation"),
            BlockOp::Turn(Turn::Code) => tr("Code block", "Bloc de code"),
            BlockOp::Duplicate => tr("Duplicate", "Dupliquer"),
            BlockOp::Up => tr("Move up", "Monter"),
            BlockOp::Down => tr("Move down", "Descendre"),
            BlockOp::Delete => tr("Delete", "Supprimer"),
        }
    }

        /// What the help says: the gesture, and what it applies to.
    pub fn effect(self) -> &'static str {
        use block::Turn;
        match self {
            BlockOp::Turn(Turn::Text) => tr("Block: turn into text", "Bloc : en texte"),
            BlockOp::Turn(Turn::Heading(1)) => tr("Block: turn into heading 1", "Bloc : en titre 1"),
            BlockOp::Turn(Turn::Heading(2)) => tr("Block: turn into heading 2", "Bloc : en titre 2"),
            BlockOp::Turn(Turn::Heading(_)) => tr("Block: turn into heading 3", "Bloc : en titre 3"),
            BlockOp::Turn(Turn::Bullet) => tr("Block: turn into a bulleted list", "Bloc : en liste à puces"),
            BlockOp::Turn(Turn::Ordered) => tr("Block: turn into a numbered list", "Bloc : en liste numérotée"),
            BlockOp::Turn(Turn::Task) => tr("Block: turn into a task list", "Bloc : en liste de tâches"),
            BlockOp::Turn(Turn::Quote) => tr("Block: turn into a quote", "Bloc : en citation"),
            BlockOp::Turn(Turn::Code) => tr("Block: turn into a code block", "Bloc : en bloc de code"),
            BlockOp::Duplicate => tr("Block: duplicate", "Bloc : dupliquer"),
            BlockOp::Up => tr("Block: move up (in a table, the row)", "Bloc : monter (dans un tableau, la ligne)"),
            BlockOp::Down => tr("Block: move down (in a table, the row)", "Bloc : descendre (dans un tableau, la ligne)"),
            BlockOp::Delete => tr("Block: delete", "Bloc : supprimer"),
        }
    }

        /// The action that does the same from the keyboard.
    pub fn action(self) -> Box<dyn gpui::Action> {
        use block::Turn;
        match self {
            BlockOp::Turn(Turn::Text) => Box::new(BlockText),
            BlockOp::Turn(Turn::Heading(1)) => Box::new(BlockHeading1),
            BlockOp::Turn(Turn::Heading(2)) => Box::new(BlockHeading2),
            BlockOp::Turn(Turn::Heading(_)) => Box::new(BlockHeading3),
            BlockOp::Turn(Turn::Bullet) => Box::new(BlockBullet),
            BlockOp::Turn(Turn::Ordered) => Box::new(BlockOrdered),
            BlockOp::Turn(Turn::Task) => Box::new(BlockTask),
            BlockOp::Turn(Turn::Quote) => Box::new(BlockQuote),
            BlockOp::Turn(Turn::Code) => Box::new(BlockCode),
            BlockOp::Duplicate => Box::new(BlockDuplicate),
            BlockOp::Up => Box::new(BlockUp),
            BlockOp::Down => Box::new(BlockDown),
            BlockOp::Delete => Box::new(BlockDelete),
        }
    }
}

/// A block dragged by its handle: where it was grabbed, where it would land, and whether it has moved yet (otherwise,
/// dropped in place, it is a click, which opens its menu).
pub(super) struct BlockDrag {
    pub(super) block: Range<usize>,
    pub(super) from: Point<Pixels>,
    pub(super) to: Option<usize>,
    pub(super) moved: bool,
}

/// A handle is a 10 by 24 px badge; it floats 5 px from the table.
pub(super) const GRIP: Pixels = px(10.);

pub(super) const GRIP_LONG: Pixels = px(24.);

pub(super) const GRIP_GAP: Pixels = px(5.);

/// How far around a table the pointer makes the handle of its column or row appear.
pub(super) const GRIP_REACH: Pixels = px(24.);

impl Editor {
        /// What is clickable under the pointer takes over: a table handle (open, closed
        /// when dragged), a line to insert at, a checkbox, a link when `Ctrl` is held.
    pub(super) fn hand_at(&self, at: Point<Pixels>, held: gpui::Modifiers) -> Option<CursorStyle> {
        if self.stale() {
            return self.hand;
        }
        if self.moving.is_some() {
            return Some(CursorStyle::ClosedHand);
        }
        if !self.viewport.contains(&at) {
            return None;
        }
        // The source's name on an embed's frame opens it.
        if self.embed_at(at).is_some_and(|(.., on_label)| on_label) {
            return Some(CursorStyle::PointingHand);
        }
        if let Some((_, part)) = self.part_at(at) {
            return Some(match part {
                Part::JoinCol(_) | Part::JoinRow(_) => CursorStyle::PointingHand,
                Part::Col(_) | Part::Row(_) | Part::Corner => CursorStyle::OpenHand,
            });
        }
        if self.rows.is_empty() {
            return None;
        }
        if !held.secondary() && self.checkbox_at(at).is_some() {
            return Some(CursorStyle::PointingHand);
        }
        let i = self.index_at(at);
        let lr = self.line_range(i);
        let (line, col) = (&self.content[lr.clone()], i - lr.start);
        if held.secondary() && md::links(line).into_iter().any(|(r, _)| r.contains(&col)) {
            return Some(CursorStyle::PointingHand);
        }
        let (kind, marker) = md::classify(line, self.in_code(lr.start));
        (matches!(kind, Kind::Task(_)) && col + 4 >= marker && col < marker).then_some(CursorStyle::PointingHand)
    }

    pub(super) fn aim_hand(&mut self, at: Point<Pixels>, held: gpui::Modifiers, cx: &mut Context<Self>) {
        let hand = self.hand_at(at, held);
        if hand != self.hand {
            self.hand = hand;
            cx.notify();
        }
    }

        /// The note's top-level blocks, rebuilt once per text version.
    pub(super) fn top_blocks(&mut self) -> Rc<Vec<Range<usize>>> {
        match &self.blocks {
            Some((version, blocks)) if *version == self.version => blocks.clone(),
            _ => {
                let mut blocks = block::blocks(&self.content);
                                // The first line is the note's title, and its file name: it has no
                                // handle, and nothing goes before it.
                if !blocks.is_empty() {
                    blocks.remove(0);
                }
                let blocks = Rc::new(blocks);
                self.blocks = Some((self.version, blocks.clone()));
                blocks
            }
        }
    }

        /// The block the handle shows: the one being dragged, otherwise the one under the pointer, as long as the text
        /// has not changed since.
    pub(super) fn handled(&self) -> Option<Range<usize>> {
        if let Some(drag) = &self.handles.drag {
            return Some(drag.block.clone());
        }
        self.handles.hover.clone().filter(|(_, version)| *version == self.version).map(|(block, _)| block)
    }

        /// Left edge of what the line starting at `at` occupies: the page, or its table's frame
        /// (wider than the line, and whose handles already take the near margin).
        /// An indented item has its handle at its indent.
    pub(super) fn block_left(&self, at: usize) -> Pixels {
        if let Some((_, frame)) = self.tables.iter().find(|(range, _)| range.contains(&at)) {
            return frame.left() - px(20.);
        }
        let line = &self.content[self.line_range(at)];
        let indent = line.len() - line.trim_start().len();
        let lead = self.row_at(at).map_or(px(0.), |r| self.rows[r].pos(indent).x);
        self.origin.x + lead
    }

        /// The handle of the shown block and its "+" (absent when the margin is too narrow for it).
    pub(super) fn handle_bounds(&self) -> Option<(Bounds<Pixels>, Option<Bounds<Pixels>>)> {
        let block = self.handled()?;
        let row = &self.rows[self.row_at(block.start)?];
        if row.hidden() {
            return None;
        }
        let left = self.block_left(block.start);
        let mid = self.origin.y + row.y + row.pad + row.lh / 2.;
        let grip = Bounds::new(point(left - px(24.), mid - px(10.)), size(px(18.), px(20.)));
        let plus = Bounds::new(point(left - px(44.), mid - px(10.)), size(px(18.), px(20.)));
        if grip.left() < self.viewport.left() {
            return None;
        }
        Some((grip, (plus.left() >= self.viewport.left()).then_some(plus)))
    }

        /// The top of the line starting at `at` on screen; the end of the text: the bottom of the last line.
    pub(super) fn line_top(&self, at: usize) -> Option<Pixels> {
        if at >= self.content.len() {
            let row = self.rows.iter().rev().find(|r| !r.hidden())?;
            return Some(self.origin.y + row.y + row.height);
        }
        let row = &self.rows[self.row_at(at)?];
        Some(self.origin.y + row.y)
    }

        /// The block under the pointer gets a handle; the pointer on the handle keeps it.
    pub(super) fn aim_block(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        if self.handles.drag.is_some() || self.selecting || self.moving.is_some() {
            return;
        }
        if self.handle_bounds().is_some_and(|(grip, plus)| grip.dilate(px(4.)).contains(&at) || plus.is_some_and(|p| p.dilate(px(4.)).contains(&at))) {
            return;
        }
        let hover = self.block_under(at).map(|block| (block, self.version));
        if hover != self.handles.hover {
            self.handles.hover = hover;
            cx.notify();
        }
    }

        /// The block of the line under the pointer, if it is on the page or in its left margin.
    pub(super) fn block_under(&mut self, at: Point<Pixels>) -> Option<Range<usize>> {
        if self.rows.is_empty() || !self.viewport.contains(&at) || at.x < self.origin.x - px(140.) || at.x > self.origin.x + self.width + px(24.) {
            return None;
        }
        let i = self.index_at(at);
        let line = self.line_range(i).start;
        let row = &self.rows[self.row_at(line)?];
        let top = self.origin.y + row.y;
                // Below the last line, or in the space between two: none.
        let last = &self.rows[self.row_at(self.line_range(i).end)?];
        if at.y < top || at.y > self.origin.y + last.y + last.height {
            return None;
        }
        let top_blocks = self.top_blocks();
        block::at(&self.content, &top_blocks, line)
    }

        /// The block a gesture targets: the handle's as long as the cursor is there, otherwise the cursor's.
    pub(super) fn block_target(&mut self) -> Option<Range<usize>> {
        let cursor = self.cursor();
        if let Some((block, version)) = self.handles.menu.clone()
            && version == self.version
            && (block.start..=block.end).contains(&cursor)
        {
            return Some(block);
        }
        let top = self.top_blocks();
        block::at(&self.content, &top, self.line_range(cursor).start)
    }

        /// Performs a gesture on the targeted block; the resulting block stays selected, and targeted.
    pub fn block_op(&mut self, op: BlockOp, cx: &mut Context<Self>) {
        if !self.more.is_empty() {
            return;
        }
                // In a table, up and down move its row, as before.
        if matches!(op, BlockOp::Up | BlockOp::Down) && self.table_at(self.cursor()).is_some() {
            return self.table_op(if op == BlockOp::Up { TableOp::MoveRowUp } else { TableOp::MoveRowDown }, cx);
        }
        let Some(block) = self.block_target() else { return };
        let old = self.content.clone();
        let top = self.top_blocks();
        let (new, sel) = match op {
            BlockOp::Turn(turn) => {
                let turned = block::turned(&old[block.clone()], turn);
                let mut new = old.clone();
                new.replace_range(block.clone(), &turned);
                (new, block.start..block.start + turned.len())
            }
            BlockOp::Duplicate => block::duplicated(&old, block),
            BlockOp::Delete => {
                let (new, at) = block::removed(&old, block);
                (new, at..at)
            }
            BlockOp::Up | BlockOp::Down => {
                let targets = block::targets(&old, &top);
                let to = match op {
                                        // Before the preceding block.
                    BlockOp::Up => targets.iter().rev().find(|&&t| t < block.start).copied(),
                                        // After the following block: before the one after it, or at the end.
                    _ => targets.iter().find(|&&t| t > block.end).and_then(|&next| {
                        let next = block::at(&old, &top, next).map_or(next, |b| b.end);
                        targets.iter().find(|&&t| t > next).copied()
                    }),
                };
                let Some(moved) = to.and_then(|to| block::moved(&old, block, to)) else { return };
                moved
            }
        };
        if new == old {
            return;
        }
        self.replace_block(&old, &new, cx);
        self.sel = sel.clone();
        self.reversed = false;
        self.handles.menu = Some((sel, self.version));
        self.reveal = true;
        cx.notify();
    }

        /// Goes from `old` (the current text) to `new` replacing only what changes, in a single
        /// undo step: `splice` would add a second one for a multi-line text.
    pub(super) fn replace_block(&mut self, old: &str, new: &str, cx: &mut Context<Self>) {
        let (span, text) = block::diff(old, new);
        self.push_undo(true);
        self.held = true;
        self.edit(span, &text, cx);
        self.held = false;
    }

        /// A block's "+": a new line below it, separated like the surrounding blocks, where
        /// `/` already opens the list of components to insert.
    pub(super) fn add_block_after(&mut self, block: Range<usize>, cx: &mut Context<Self>) {
        let rest = &self.content[block.end..];
        let gap = rest.len() - rest.trim_start_matches('\n').len();
        let sep = "\n".repeat(gap.clamp(1, 2));
        self.push_undo(true);
        self.held = true;
        self.edit(block.end..block.end, &format!("{sep}/"), cx);
        self.held = false;
        self.ac_index = 0;
        self.ac_dismissed = None;
        self.handles.hover = None;
        self.reveal = true;
        cx.notify();
    }

        /// The dragged block follows the pointer: it would land at the nearest place.
    pub(super) fn drag_block(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(drag) = &self.handles.drag else { return };
        let moved = drag.moved || (at - drag.from).magnitude() > 4.;
        let top = self.top_blocks();
        let targets = block::targets(&self.content, &top);
        let to = targets
            .into_iter()
            .filter_map(|t| Some((t, self.line_top(t)?)))
            .min_by(|a, b| (a.1 - at.y).abs().partial_cmp(&(b.1 - at.y).abs()).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(t, _)| t);
        if let Some(drag) = &mut self.handles.drag {
            (drag.moved, drag.to) = (moved, to);
        }
        cx.notify();
    }

        /// Dropped: put at its new place; without having moved, it is a click, which takes the block and opens its menu.
    pub(super) fn drop_block(&mut self, cx: &mut Context<Self>) {
        let Some(drag) = self.handles.drag.take() else { return };
        if !drag.moved {
            let anchor = self.handle_bounds_of(&drag.block).map_or(drag.from, |grip| point(grip.left(), grip.bottom() + px(4.)));
            self.move_to(drag.block.start, cx);
            self.select_to(drag.block.end, cx);
            self.handles.menu = Some((drag.block, self.version));
            cx.emit(EditorEvent::BlockMenu(anchor));
            return cx.notify();
        }
        let old = self.content.clone();
        if let Some((new, sel)) = drag.to.and_then(|to| block::moved(&old, drag.block, to)) {
            self.replace_block(&old, &new, cx);
            self.sel = sel.clone();
            self.reversed = false;
            self.handles.menu = Some((sel, self.version));
        }
        cx.notify();
    }

    pub(super) fn handle_bounds_of(&self, block: &Range<usize>) -> Option<Bounds<Pixels>> {
        let row = &self.rows[self.row_at(block.start)?];
        let mid = self.origin.y + row.y + row.pad + row.lh / 2.;
        Some(Bounds::new(point(self.block_left(block.start) - px(24.), mid - px(10.)), size(px(18.), px(20.))))
    }

        /// Where the handle of the block under the pointer is, for tests.
    #[cfg(test)]
    pub fn block_handle(&self) -> Option<(Bounds<Pixels>, Option<Bounds<Pixels>>)> {
        self.handle_bounds()
    }

        /// The block under the handle, for tests.
    #[cfg(test)]
    pub fn hovered_block(&self) -> Option<&str> {
        self.handled().map(|block| &self.content[block])
    }
}
