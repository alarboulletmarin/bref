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
        self.context = None;
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
            // The library is open: what is typed searches it.
            if let Some(query) = &mut self.library {
                query.push_str(text);
                self.reset_cursor();
                cx.notify();
                return;
            }
            match text {
                "+" | "=" => self.zoom_by(1.25, cx),
                "-" => self.zoom_by(0.8, cx),
                "0" => self.zoom_fit(cx),
                // The menu of relationships: `k` opens it, then a letter picks one.
                // The menu is open: `k` closes it, a letter picks a relationship.
                _ if self.relations && text.eq_ignore_ascii_case(Self::RELATIONS) => self.toggle_relations(cx),
                _ if self.relations => {
                    let relation = Relation::ALL.iter().find(|r| text.chars().next().is_some_and(|c| c.eq_ignore_ascii_case(&r.key())));
                    match relation {
                        Some(relation) => self.choose_relation(*relation, cx),
                        None if text.eq_ignore_ascii_case(Self::NARY) => self.set_tool(Tool::Shape(Form::Branch), cx),
                        // Not in the menu: it closes, and the letter acts as it would have.
                        None => {
                            self.relations = false;
                            if let Some((tool, ..)) = TOOLS.iter().find(|(_, _, key)| key.eq_ignore_ascii_case(text)) {
                                self.set_tool(*tool, cx);
                            }
                        }
                    }
                    cx.notify();
                }
                _ if text.eq_ignore_ascii_case(Self::RELATIONS) => self.toggle_relations(cx),
                _ if text.eq_ignore_ascii_case(Self::LIBRARY) => self.toggle_library(cx),
                // 1 to 6: the colour of the selection, as the palette gives them.
                _ if !self.selected.is_empty() && matches!(text.as_bytes(), [b'1'..=b'6']) => {
                    let color = text.as_bytes()[0] - b'1';
                    self.style(cx, move |shape, link| {
                        if let Some(shape) = shape {
                            shape.color = color;
                        }
                        if let Some(link) = link {
                            link.color = color;
                        }
                    })
                }
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
        if let Some(query) = &mut self.library {
            query.pop();
            self.reset_cursor();
            return cx.notify();
        }
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
        // A menu or the library is open: up and down go along its rows, the other keys are not for the selection.
        if self.menu_open() {
            if dy != 0. {
                self.walk(dy as isize, cx);
                self.show_cursor();
            }
            return;
        }
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

    /// Moves the selection, and what its containers hold; an arrow carries its free ends.
    pub(super) fn shift(&mut self, dx: f32, dy: f32) {
        let moved = |end: &mut End| {
            if let End::Point(x, y) = end {
                (*x, *y) = (*x + dx, *y + dy);
            }
        };
        let mut shapes: Vec<u32> = Vec::new();
        for &id in &self.selected {
            if self.diagram.shape(id).is_some() {
                shapes.push(id);
                shapes.extend(self.diagram.descendants(id));
            }
        }
        shapes.sort_unstable();
        shapes.dedup();
        for id in shapes {
            if let Some(shape) = self.diagram.shape_mut(id) {
                (shape.x, shape.y) = (shape.x + dx, shape.y + dy);
            }
        }
        for &id in &self.selected {
            if let Some(link) = self.diagram.link_mut(id) {
                moved(&mut link.from);
                moved(&mut link.to);
            }
        }
    }

    /// A transition from the selected shape to itself, a loop: its text is written at once.
    pub fn self_link(&mut self, cx: &mut Context<Self>) {
        let [id] = self.selected[..] else { return };
        if self.diagram.shape(id).is_none() {
            return;
        }
        self.remember();
        let link = self.diagram.add_link(End::Shape(id), End::Shape(id), diagram::Head::Arrow, Route::Elbow);
        self.slot = Slot::Middle;
        self.editing = Some((link, 0));
        self.selected = vec![link];
        self.changed(cx);
    }

    /// Shows the messages with their number, or not: one step of undo.
    pub fn toggle_numbers(&mut self, cx: &mut Context<Self>) {
        self.remember();
        self.diagram.numbered = !self.diagram.numbered;
        self.changed(cx);
    }

    /// Makes the container selected fit what it holds.
    pub fn fit_container(&mut self, cx: &mut Context<Self>) {
        if let [id] = self.selected[..] {
            self.remember();
            self.diagram.fit(id);
            self.changed(cx);
        }
    }

    /// Selects everything.
    pub(super) fn select_all(&mut self, cx: &mut Context<Self>) {
        self.selected = self.diagram.shapes.iter().map(|s| s.id).chain(self.diagram.links.iter().map(|l| l.id)).collect();
        cx.notify();
    }

    /// Turns the selected arrows around: one step of undo.
    pub(super) fn reverse(&mut self, cx: &mut Context<Self>) {
        let links: Vec<u32> = self.selected.iter().copied().filter(|id| self.diagram.link(*id).is_some()).collect();
        self.remember();
        let turned = links.iter().fold(false, |any, id| self.diagram.reverse(*id) | any);
        if turned {
            self.changed(cx);
        } else {
            self.undo.pop();
        }
    }

    /// The route of the selected arrows, and of those to come.
    pub(super) fn set_route(&mut self, route: Route, cx: &mut Context<Self>) {
        self.route = route;
        self.style(cx, move |_, link| {
            if let Some(link) = link {
                link.route = route;
            }
        });
    }

    /// A text, written at once, at this point of the window.
    pub(super) fn text_at(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        self.remember();
        let p = self.world(at);
        let id = self.diagram.add_shape(Form::Text, diagram::snap(p.0), diagram::snap(p.1), 80., diagram::LINE);
        (self.editing, self.selected) = (Some((id, 0)), vec![id]);
        self.changed(cx);
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
