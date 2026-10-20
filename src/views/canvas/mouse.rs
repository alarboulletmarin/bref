//! The pointer: selecting, creating, moving, resizing, hooking arrows; the view's pan and zoom.

use super::*;

/// How far out of a shape its handles are, and how near the pointer must be to take one (pixels).
const HANDLE_OUT: f32 = 16.;
const HANDLE_REACH: f32 = 9.;

impl Canvas {
    /// Corners of the shape, in order: top-left, top-right, bottom-right, bottom-left.
    pub(super) fn corners(shape: &diagram::Shape) -> [(f32, f32); 4] {
        let (x, y, x1, y1) = (shape.x, shape.y, shape.x + shape.w, shape.y + shape.h);
        [(x, y), (x1, y), (x1, y1), (x, y1)]
    }

    /// The handles of a shape from which an arrow is drawn: the middle of each side, a little way out, and the
    /// place of the border an arrow from there leaves by (fractions of the width and the height).
    pub(super) fn handles(shape: &diagram::Shape, zoom: f32) -> [((f32, f32), (f32, f32)); 4] {
        let (cx, cy, off) = (shape.x + shape.w / 2., shape.y + shape.h / 2., HANDLE_OUT / zoom);
        [
            ((cx, shape.y - off), (0.5, 0.)),
            ((shape.x + shape.w + off, cy), (1., 0.5)),
            ((cx, shape.y + shape.h + off), (0.5, 1.)),
            ((shape.x - off, cy), (0., 0.5)),
        ]
    }

    /// Whether an arrow can be drawn from a handle of this shape: a lifeline is joined by messages, a pin
    /// sits on a border, a fragment is drawn around messages.
    pub(super) fn drawable_from(shape: &diagram::Shape) -> bool {
        !matches!(shape.form, Form::Lifeline | Form::Pin | Form::Fragment | Form::Divider)
    }

    /// The handle under this point, on the shape under the pointer: the shape, and where on its border it leaves from.
    pub(super) fn handle_at(&self, p: (f32, f32)) -> Option<(u32, (f32, f32))> {
        let shape = self.hover.and_then(|id| self.diagram.shape(id)).filter(|s| Self::drawable_from(s))?;
        Self::handles(shape, self.zoom).into_iter().find(|(h, _)| (h.0 - p.0).hypot(h.1 - p.1) <= HANDLE_REACH / self.zoom).map(|(_, at)| (shape.id, at))
    }

    /// What is under the pointer, when nothing is being dragged: it is lit, and a shape shows its handles.
    pub(super) fn aim_hover(&mut self, p: (f32, f32), cx: &mut Context<Self>) {
        let idle = self.tool == Tool::Select && self.drag == Drag::None && self.editing.is_none() && self.context.is_none();
        let next = if !idle {
            None
        } else if self.handle_at(p).is_some() {
            self.hover
        } else {
            self.diagram.hit(p.0, p.1, 7. / self.zoom)
        };
        if next != self.hover {
            self.hover = next;
            cx.notify();
        }
    }

    /// Arrow end for this point: attached to the shape found there, at the
    /// edge if close to it; otherwise the snapped point. With `mid` (an association class), near
    /// the middle of another arrow than `except`, it holds on to that.
    pub(super) fn end_at(&self, p: (f32, f32), not: Option<u32>, mid: bool, except: Option<u32>) -> End {
        // A lifeline is joined whole, even by the arrow that began on it: a message to itself.
        let shape = self.diagram.shape_at(p.0, p.1).and_then(|id| self.diagram.shape(id)).filter(|s| Some(s.id) != not || s.form == Form::Lifeline);
        match shape {
            Some(shape) if shape.form == Form::Lifeline => End::Life(shape.id),
            Some(shape) => shape.hook(p.0, p.1),
            None => match self.diagram.mid_at(p.0, p.1, 16. / self.zoom, except).filter(|_| mid) {
                Some(id) => End::Mid(id),
                None => End::Point(diagram::snap(p.0), diagram::snap(p.1)),
            },
        }
    }

    /// Zooms around the centre of the view.
    pub(super) fn zoom_by(&mut self, by: f32, cx: &mut Context<Self>) {
        let zoom = (self.zoom * by).clamp(0.2, 4.);
        let (mx, my) = (f32::from(self.bounds.size.width) / 2., f32::from(self.bounds.size.height) / 2.);
        let k = zoom / self.zoom;
        self.pan = (mx - (mx - self.pan.0) * k, my - (my - self.pan.1) * k);
        self.zoom = zoom;
        cx.notify();
    }

    pub(super) fn zoom_fit(&mut self, cx: &mut Context<Self>) {
        self.fit = true;
        cx.notify();
    }

    /// Places a shape at its usual size, centred on this screen point.
    pub(super) fn place(&mut self, tool: Tool, at: Point<Pixels>, cx: &mut Context<Self>) {
        let Some((form, preset)) = made_by(tool) else { return };
        self.stop_editing(cx);
        (self.shapes, self.relations) = (false, false);
        self.remember();
        let (p, (w, h)) = (self.world(at), usual(tool));
        let id = self.diagram.add_shape(form, diagram::snap(p.0 - w / 2.), diagram::snap(p.1 - h / 2.), w, h);
        self.dress(id, preset);
        self.diagram.adopt(&[id]);
        (self.selected, self.tool) = (vec![id], Tool::Select);
        if form == Form::Text {
            self.editing = Some((id, 0));
        }
        self.changed(cx);
    }

    /// Writes in a shape what its preset begins with.
    pub(super) fn dress(&mut self, id: u32, preset: Option<Preset>) {
        if let (Some(preset), Some(shape)) = (preset, self.diagram.shape_mut(id)) {
            (shape.text, shape.fill, shape.dashed) = (preset.text.to_string(), preset.fill, preset.dashed);
        }
    }

    /// A right click: what is under it is selected, and the menu of what can be done with it opens there.
    pub(super) fn context_down(&mut self, e: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus);
        self.set_tool(Tool::Select, cx);
        self.library = None;
        let p = self.world(e.position);
        match self.diagram.hit(p.0, p.1, 7. / self.zoom) {
            Some(id) if !self.selected.contains(&id) => self.selected = vec![id],
            Some(_) => {}
            None => self.selected.clear(),
        }
        (self.drag, self.hover, self.context) = (Drag::None, None, Some(e.position));
        cx.notify();
    }

    pub(super) fn mouse_down(&mut self, e: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus);
        self.stop_editing(cx);
        self.context = None;
        let p = self.world(e.position);
        let near = 7. / self.zoom;
        let close = |a: (f32, f32)| (a.0 - p.0).hypot(a.1 - p.1) <= near;
        let handle = if self.tool == Tool::Select { self.handle_at(p) } else { None };
        self.hover = None;
        self.drag = match self.tool {
            // A handle of a shape: the arrow that is drawn from it, with the look of the last relationship picked.
            _ if handle.is_some() => {
                let (from, (fx, fy)) = handle.unwrap_or_default();
                self.remember();
                // A message needs lifelines, an association class a middle: those are not drawn from a handle.
                let relation = match self.link_pick {
                    Relation::AssociationClass | Relation::Sync | Relation::Create | Relation::Destroy | Relation::Found | Relation::Lost => Relation::Navigable,
                    other => other,
                };
                let id = self.diagram.add_link(End::Pin(from, fx, fy), End::Point(p.0, p.1), diagram::Head::None, self.route);
                if let Some(link) = self.diagram.link_mut(id) {
                    relation.apply(link);
                }
                Drag::Tip { id, to: true }
            }
            Tool::Shape(_) | Tool::Preset(_) => {
                self.remember();
                let (form, preset) = made_by(self.tool).unwrap_or((Form::Rect, None));
                let from = (diagram::snap(p.0), diagram::snap(p.1));
                let id = self.diagram.add_shape(form, from.0, from.1, 0., 0.);
                self.dress(id, preset);
                Drag::Create { id, from }
            }
            Tool::Link(relation) => {
                self.remember();
                let held = relation == Relation::AssociationClass;
                let from = self.end_at(p, None, held, None);
                // The dotted line of an association class is straight, to the middle of the association.
                let route = if held { Route::Straight } else { self.route };
                let id = self.diagram.add_link(from, End::Point(p.0, p.1), diagram::Head::None, route);
                if let Some(link) = self.diagram.link_mut(id) {
                    relation.apply(link);
                }
                Drag::Tip { id, to: true }
            }
            Tool::Select => {
                let only = (self.selected.len() == 1).then(|| self.selected[0]);
                let corner = only.and_then(|id| self.diagram.shape(id)).and_then(|shape| {
                    let corners = Self::corners(shape);
                    let i = corners.iter().position(|c| close(*c))?;
                    Some(Drag::Resize { id: shape.id, fixed: corners[(i + 2) % 4] })
                });
                let tip = only.and_then(|id| self.diagram.link(id)).and_then(|link| {
                    let (a, b) = self.diagram.ends(link)?;
                    let to = if close(b) { true } else if close(a) { false } else { return None };
                    Some(Drag::Tip { id: link.id, to })
                });
                let hit = self.diagram.hit(p.0, p.1, near);
                let label = self.diagram.tip_label_hit(p.0, p.1).filter(|_| hit.is_none_or(|id| self.diagram.link(id).is_some()));
                match (corner.or(tip), label, hit) {
                    (Some(drag), ..) => {
                        self.remember();
                        drag
                    }
                    // The text at an end of an arrow: a double click writes in it; once the arrow is
                    // selected, it can be dragged along the stroke.
                    (None, Some((id, slot)), _) => {
                        if e.click_count >= 2 {
                            self.start_editing_slot(id, slot, cx);
                            Drag::None
                        } else if self.selected == [id] {
                            Drag::Along { id, slot, begun: false }
                        } else {
                            self.selected = vec![id];
                            Drag::None
                        }
                    }
                    (None, None, Some(id)) => {
                        // Shift+click or Ctrl+click: the shape joins the selection, or leaves it.
                        if e.modifiers.shift || e.modifiers.secondary() {
                            match self.selected.iter().position(|s| *s == id) {
                                Some(i) => drop(self.selected.remove(i)),
                                None => self.selected.push(id),
                            }
                        } else if !self.selected.contains(&id) {
                            self.selected = vec![id];
                        }
                        if e.click_count >= 2 {
                            // On an arrow, near one of its ends: the text of that end.
                            let slot = self.slot_near(id, p);
                            self.start_editing_slot(id, slot, cx);
                            Drag::None
                        } else if self.diagram.messages().contains(&id) && self.selected == [id] {
                            // A message goes up or down, among the others.
                            Drag::Reorder { id, begun: false }
                        } else {
                            Drag::Move { last: p, begun: false }
                        }
                    }
                    // Double-click on empty space: a text, at that spot.
                    (None, None, None) if e.click_count >= 2 => {
                        self.remember();
                        let id = self.diagram.add_shape(Form::Text, diagram::snap(p.0), diagram::snap(p.1), 80., diagram::LINE);
                        (self.editing, self.selected) = (Some((id, 0)), vec![id]);
                        Drag::None
                    }
                    (None, None, None) => {
                        if !(e.modifiers.shift || e.modifiers.secondary()) {
                            self.selected.clear();
                        }
                        Drag::Marquee { from: p, to: p }
                    }
                }
            }
        };
        cx.notify();
    }

    /// The text of arrow `id` a double click at `p` asks for: that of an end it is near, or the middle.
    pub(super) fn slot_near(&self, id: u32, p: (f32, f32)) -> Slot {
        let Some(trace) = self.diagram.link(id).and_then(|l| self.diagram.trace(l)) else { return Slot::Middle };
        let close = |tip: &diagram::Tip| (tip.at.0 - p.0).hypot(tip.at.1 - p.1) <= 36.;
        match (close(&trace.tips[0]), close(&trace.tips[1])) {
            (true, false) => Slot::Start,
            (false, true) => Slot::End,
            _ => Slot::Middle,
        }
    }

    /// What the pointer announces: a shape or an arrow can be grabbed (hand), a corner drags
    /// diagonally, an arrow end reattaches, the dragged view is held, a tool draws.
    pub(super) fn hand_at(&self, at: Point<Pixels>) -> CursorStyle {
        match self.drag {
            Drag::Pan { .. } | Drag::Move { begun: true, .. } | Drag::Along { begun: true, .. } | Drag::Reorder { begun: true, .. } => return CursorStyle::ClosedHand,
            Drag::Resize { .. } | Drag::Tip { .. } | Drag::Create { .. } | Drag::Marquee { .. } | Drag::Along { .. } | Drag::Reorder { .. } => return self.hand,
            _ => {}
        }
        if self.tool != Tool::Select {
            return CursorStyle::Crosshair;
        }
        let p = self.world(at);
        if self.handle_at(p).is_some() {
            return CursorStyle::Crosshair;
        }
        let near = 7. / self.zoom;
        let close = |a: (f32, f32)| (a.0 - p.0).hypot(a.1 - p.1) <= near;
        let only = (self.selected.len() == 1).then(|| self.selected[0]);
        if let Some(shape) = only.and_then(|id| self.diagram.shape(id))
            && let Some(i) = Self::corners(shape).iter().position(|c| close(*c))
        {
            return if i % 2 == 0 { CursorStyle::ResizeUpLeftDownRight } else { CursorStyle::ResizeUpRightDownLeft };
        }
        if let Some((a, b)) = only.and_then(|id| self.diagram.link(id)).and_then(|link| self.diagram.ends(link))
            && (close(a) || close(b))
        {
            return CursorStyle::Crosshair;
        }
        if let Some((id, _)) = self.editing
            && self.diagram.shape(id).is_some_and(|shape| shape.contains(p.0, p.1))
        {
            return CursorStyle::IBeam;
        }
        match self.diagram.hit(p.0, p.1, near).or_else(|| self.diagram.tip_label_hit(p.0, p.1).map(|(id, _)| id)) {
            Some(_) => CursorStyle::PointingHand,
            None => CursorStyle::Arrow,
        }
    }

    pub(super) fn aim_hand(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        let hand = self.hand_at(at);
        if hand != self.hand {
            self.hand = hand;
            cx.notify();
        }
    }

    pub(super) fn mouse_move(&mut self, e: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        let held = e.pressed_button;
        let p = self.world(e.position);
        self.aim_hover(p, cx);
        self.aim_hand(e.position, cx);
        match self.drag {
            Drag::Pan { last } if held == Some(MouseButton::Middle) => {
                self.pan.0 += f32::from(e.position.x - last.x);
                self.pan.1 += f32::from(e.position.y - last.y);
                self.drag = Drag::Pan { last: e.position };
            }
            _ if held != Some(MouseButton::Left) => return,
            Drag::Create { id, from } => {
                let to = (diagram::snap(p.0), diagram::snap(p.1));
                if let Some(shape) = self.diagram.shape_mut(id) {
                    (shape.x, shape.y) = (from.0.min(to.0), from.1.min(to.1));
                    (shape.w, shape.h) = ((to.0 - from.0).abs(), (to.1 - from.1).abs());
                }
            }
            Drag::Tip { id, to } => {
                // An end does not attach to the shape the other end starts from.
                let other = self.diagram.link(id).map(|l| if to { l.from } else { l.to });
                let not = other.and_then(End::shape);
                let held = self.diagram.link(id).is_some_and(|l| Relation::of(l) == Some(Relation::AssociationClass));
                let end = self.end_at(p, not, held, Some(id));
                if let Some(link) = self.diagram.link_mut(id) {
                    *(if to { &mut link.to } else { &mut link.from }) = end;
                }
            }
            Drag::Reorder { id, begun } => {
                let to = self.diagram.rank_at(p.1);
                if self.diagram.messages().iter().position(|m| *m == id) == Some(to) {
                    return;
                }
                if !begun {
                    self.remember();
                }
                self.diagram.reorder(id, to);
                self.diagram.fit_sequence();
                self.drag = Drag::Reorder { id, begun: true };
            }
            Drag::Along { id, slot, begun } => {
                let along = self.diagram.link(id).map_or(0., |l| self.diagram.along_to(l, slot, p.0, p.1).round());
                let held = self.diagram.link(id).map_or(0., |l| if slot == Slot::Start { l.start_along } else { l.end_along });
                if along == held {
                    return;
                }
                if !begun {
                    self.remember();
                }
                if let Some(link) = self.diagram.link_mut(id) {
                    *(if slot == Slot::Start { &mut link.start_along } else { &mut link.end_along }) = along;
                }
                self.drag = Drag::Along { id, slot, begun: true };
            }
            Drag::Move { last, begun } => {
                let (dx, dy) = (diagram::snap(p.0 - last.0), diagram::snap(p.1 - last.1));
                if dx == 0. && dy == 0. {
                    return;
                }
                if !begun {
                    self.remember();
                }
                self.shift(dx, dy);
                self.drag = Drag::Move { last: (last.0 + dx, last.1 + dy), begun: true };
            }
            Drag::Resize { id, fixed } => {
                let to = (diagram::snap(p.0), diagram::snap(p.1));
                if let Some(shape) = self.diagram.shape_mut(id) {
                    (shape.x, shape.y) = (fixed.0.min(to.0), fixed.1.min(to.1));
                    (shape.w, shape.h) = ((to.0 - fixed.0).abs().max(diagram::GRID), (to.1 - fixed.1).abs().max(diagram::GRID));
                }
            }
            Drag::Marquee { from, .. } => self.drag = Drag::Marquee { from, to: p },
            Drag::None | Drag::Pan { .. } => return,
        }
        cx.notify();
    }

    pub(super) fn mouse_up(&mut self, e: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        let drag = std::mem::replace(&mut self.drag, Drag::None);
        self.aim_hand(e.position, cx);
        match drag {
            Drag::Create { id, .. } => {
                let Some(shape) = self.diagram.shape_mut(id) else { return };
                // A simple click places the shape at its usual size.
                if shape.w < diagram::GRID || shape.h < diagram::GRID {
                    (shape.w, shape.h) = usual(self.tool);
                }
                let text = shape.form == Form::Text;
                self.diagram.adopt(&[id]);
                (self.selected, self.tool) = (vec![id], Tool::Select);
                if text {
                    self.editing = Some((id, 0));
                }
                self.changed(cx);
            }
            Drag::Tip { id, .. } => {
                // A line with no length has no reason to exist.
                let flat = self.diagram.link(id).and_then(|l| self.diagram.ends(l));
                if flat.is_none_or(|(a, b)| (a.0 - b.0).hypot(a.1 - b.1) < diagram::GRID) {
                    self.diagram.remove(&[id]);
                    self.selected.clear();
                } else {
                    self.selected = vec![id];
                }
                self.tool = Tool::Select;
                self.changed(cx);
            }
            Drag::Move { begun: true, .. } => {
                // Dropped on a container, the shapes dragged are held by it; dropped elsewhere, they are let go.
                let moved = self.selected.clone();
                self.diagram.adopt(&moved);
                self.changed(cx);
            }
            Drag::Reorder { id, begun: true } => {
                // Let go inside a combined fragment, the message is in it; outside, it is not.
                self.diagram.settle(id, self.world(e.position).1);
                self.changed(cx);
            }
            Drag::Resize { .. } | Drag::Along { begun: true, .. } => self.changed(cx),
            Drag::Marquee { from, to } => {
                let (x0, y0, x1, y1) = (from.0.min(to.0), from.1.min(to.1), from.0.max(to.0), from.1.max(to.1));
                let inside = |p: (f32, f32)| (x0..=x1).contains(&p.0) && (y0..=y1).contains(&p.1);
                let shapes = self.diagram.shapes.iter().filter(|s| inside((s.x, s.y)) && inside((s.x + s.w, s.y + s.h)));
                let links = self.diagram.links.iter().filter(|l| self.diagram.ends(l).is_some_and(|(a, b)| inside(a) && inside(b)));
                let caught: Vec<u32> = shapes.map(|s| s.id).chain(links.map(|l| l.id)).collect();
                for id in caught {
                    if !self.selected.contains(&id) {
                        self.selected.push(id);
                    }
                }
                cx.notify();
            }
            _ => cx.notify(),
        }
    }

    /// The wheel moves the view; with Ctrl (Cmd), it zooms around the pointer.
    pub(super) fn scroll(&mut self, e: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        let delta = e.delta.pixel_delta(px(26.));
        if e.modifiers.secondary() {
            let zoom = (self.zoom * (f32::from(delta.y) * 0.004).exp()).clamp(0.2, 4.);
            let o = self.bounds.origin;
            let (mx, my) = (f32::from(e.position.x - o.x), f32::from(e.position.y - o.y));
            let k = zoom / self.zoom;
            self.pan = (mx - (mx - self.pan.0) * k, my - (my - self.pan.1) * k);
            self.zoom = zoom;
        } else {
            self.pan.0 += f32::from(delta.x);
            self.pan.1 += f32::from(delta.y);
        }
        cx.notify();
    }
}
