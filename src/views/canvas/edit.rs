//! Writing in a shape or an arrow, erasing, moving and styling the selection.

use super::*;

impl Canvas {
    pub(super) fn text_of(&mut self, id: u32) -> Option<&mut String> {
        if self.diagram.shape(id).is_some() {
            return self.diagram.shape_mut(id).map(|s| &mut s.text);
        }
        let slot = self.slot;
        self.diagram.link_mut(id).map(|l| l.text_mut(slot))
    }

    pub(super) fn start_editing(&mut self, id: u32, cx: &mut Context<Self>) {
        self.start_editing_slot(id, Slot::Middle, cx);
    }

    /// Writes in an arrow's text at `slot` (a shape has only one).
    pub(super) fn start_editing_slot(&mut self, id: u32, slot: Slot, cx: &mut Context<Self>) {
        self.remember();
        self.slot = slot;
        let Some(len) = self.text_of(id).map(|t| t.len()) else { return };
        self.editing = Some((id, len));
        self.selected = vec![id];
        cx.notify();
    }

    /// Tab: from one text of an arrow to the next; on a selected arrow, starts with its first.
    pub(super) fn next_text(&mut self, cx: &mut Context<Self>) {
        match (self.editing, self.selected.as_slice()) {
            (Some((id, _)), _) if self.diagram.link(id).is_some() => {
                self.slot = self.slot.next();
                let len = self.text_of(id).map_or(0, |t| t.len());
                self.editing = Some((id, len));
                cx.notify();
            }
            (None, &[id]) if self.diagram.link(id).is_some() => self.start_editing_slot(id, Slot::Start, cx),
            (None, _) if self.tool == Tool::Select => cx.propagate(),
            _ => {}
        }
    }

    pub(super) fn stop_editing(&mut self, cx: &mut Context<Self>) {
        self.slot = Slot::Middle;
        if let Some((id, _)) = self.editing.take() {
            // A text left empty leaves nothing behind.
            let empty = self.diagram.shape(id).is_some_and(|s| s.form == Form::Text && s.text.trim().is_empty());
            if empty {
                self.diagram.remove(&[id]);
                self.selected.clear();
            }
            self.changed(cx);
        }
    }

    /// Inserts `text` at the cursor; outside writing, a letter picks a tool.
    pub(super) fn typed(&mut self, text: &str, cx: &mut Context<Self>) {
        let Some((id, at)) = self.editing else {
            match text {
                "+" | "=" => self.zoom_by(1.25, cx),
                "-" => self.zoom_by(0.8, cx),
                "0" => self.zoom_fit(cx),
                _ => {
                    if let Some((tool, ..)) = TOOLS.iter().find(|(_, _, key)| key.eq_ignore_ascii_case(text)) {
                        self.set_tool(*tool, cx);
                    }
                }
            }
            return;
        };
        if let Some(target) = self.text_of(id) {
            target.insert_str(at, text);
            self.editing = Some((id, at + text.len()));
            self.changed(cx);
        }
    }

    /// Moves the cursor by one character, or erases it if `erase`.
    pub(super) fn step(&mut self, forward: bool, erase: bool, cx: &mut Context<Self>) {
        let Some((id, at)) = self.editing else { return };
        let Some(text) = self.text_of(id) else { return };
        let to = if forward {
            text[at..].chars().next().map_or(at, |c| at + c.len_utf8())
        } else {
            text[..at].chars().next_back().map_or(at, |c| at - c.len_utf8())
        };
        if erase {
            text.replace_range(at.min(to)..at.max(to), "");
            self.editing = Some((id, at.min(to)));
            self.changed(cx);
        } else {
            self.editing = Some((id, to));
            cx.notify();
        }
    }

    pub(super) fn erase(&mut self, forward: bool, cx: &mut Context<Self>) {
        if self.editing.is_some() {
            return self.step(forward, true, cx);
        }
        if !self.selected.is_empty() {
            self.remember();
            self.diagram.remove(&std::mem::take(&mut self.selected));
            self.changed(cx);
        }
    }

    /// Arrow keys: the cursor while writing, otherwise the selection, by one grid step.
    pub(super) fn nudge(&mut self, dx: f32, dy: f32, cx: &mut Context<Self>) {
        if self.editing.is_some() {
            if dx != 0. {
                self.step(dx > 0., false, cx);
            }
        } else if !self.selected.is_empty() {
            self.remember();
            self.shift(dx * diagram::GRID, dy * diagram::GRID);
            self.changed(cx);
        }
    }

    /// Moves the selection; an arrow carries its free ends.
    pub(super) fn shift(&mut self, dx: f32, dy: f32) {
        let moved = |end: &mut End| {
            if let End::Point(x, y) = end {
                (*x, *y) = (*x + dx, *y + dy);
            }
        };
        for &id in &self.selected {
            if let Some(shape) = self.diagram.shape_mut(id) {
                (shape.x, shape.y) = (shape.x + dx, shape.y + dy);
            } else if let Some(link) = self.diagram.link_mut(id) {
                moved(&mut link.from);
                moved(&mut link.to);
            }
        }
    }

    /// Applies a setting to the selected elements.
    pub(super) fn style(&mut self, cx: &mut Context<Self>, set: impl Fn(Option<&mut diagram::Shape>, Option<&mut diagram::Link>)) {
        self.remember();
        for id in self.selected.clone() {
            if self.diagram.shape(id).is_some() {
                set(self.diagram.shape_mut(id), None);
            } else {
                set(None, self.diagram.link_mut(id));
            }
        }
        self.changed(cx);
    }
}
