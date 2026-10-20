//! Shapes that hold others: a package, a frame, a swimlane. A held shape has its holder as
//! `parent`; the holder comes first in the drawing order, so it is drawn under.

use super::*;

impl Diagram {
    /// The shapes held directly by `id`.
    pub fn children(&self, id: u32) -> Vec<u32> {
        self.shapes.iter().filter(|s| s.parent == Some(id)).map(|s| s.id).collect()
    }

    /// The shapes held by `id`, directly or through others.
    pub fn descendants(&self, id: u32) -> Vec<u32> {
        let mut found = Vec::new();
        let mut frontier = vec![id];
        while let Some(at) = frontier.pop() {
            for child in self.children(at) {
                // A loop in a damaged file does not make this one never end.
                if child != id && !found.contains(&child) {
                    found.push(child);
                    frontier.push(child);
                }
            }
        }
        found
    }

    /// Whether `id` is `ancestor`, or is held, however deep, by it.
    pub fn within(&self, id: u32, ancestor: u32) -> bool {
        let mut at = Some(id);
        for _ in 0..=self.shapes.len() {
            match at {
                Some(now) if now == ancestor => return true,
                Some(now) => at = self.shape(now).and_then(|s| s.parent),
                None => return false,
            }
        }
        false
    }

    /// The topmost shape that holds others and covers this point, apart from `except`.
    pub fn container_at(&self, x: f32, y: f32, except: &[u32]) -> Option<u32> {
        self.shapes.iter().rev().find(|s| s.form.holds() && s.contains(x, y) && !except.contains(&s.id)).map(|s| s.id)
    }

    /// Makes `parent` the holder of `id` (`None`: none). `false` if nothing changes: the same holder,
    /// or one that is `id` or inside it. A held shape moves to the top of the drawing order, above its holder.
    pub fn set_parent(&mut self, id: u32, parent: Option<u32>) -> bool {
        let Some(current) = self.shape(id).map(|s| s.parent) else { return false };
        if current == parent || parent.is_some_and(|p| self.shape(p).is_none() || self.within(p, id)) {
            return false;
        }
        if let Some(shape) = self.shape_mut(id) {
            shape.parent = parent;
        }
        if parent.is_some() {
            let mut family = self.descendants(id);
            family.push(id);
            let (moved, kept): (Vec<Shape>, Vec<Shape>) = std::mem::take(&mut self.shapes).into_iter().partition(|s| family.contains(&s.id));
            self.shapes = kept.into_iter().chain(moved).collect();
        }
        true
    }

    /// Gives each of the shapes `ids`, the ones no other of them holds, the holder it is over: the
    /// topmost container that covers its center. `true` if one changed.
    pub fn adopt(&mut self, ids: &[u32]) -> bool {
        let roots: Vec<u32> = ids.iter().copied().filter(|id| self.shape(*id).is_some() && !ids.iter().any(|other| other != id && self.shape(*id).and_then(|s| s.parent).is_some_and(|p| self.within(p, *other)))).collect();
        let mut changed = false;
        for id in roots {
            // A pin is not held like what a container holds: it sits on a border.
            if self.shape(id).is_some_and(|s| s.form == Form::Pin) {
                changed |= self.seat(id);
                continue;
            }
            let Some((cx, cy)) = self.shape(id).map(Shape::center) else { continue };
            let mut except = self.descendants(id);
            except.push(id);
            let holder = self.container_at(cx, cy, &except);
            changed |= self.set_parent(id, holder);
        }
        changed
    }

    /// Sits pin `id` on the border of the shape it was let go on (the topmost one that is not a pin, whose border
    /// is within `PIN` of its center), and has that shape hold it; far from every border it is let go. `true` if it changed.
    pub fn seat(&mut self, id: u32) -> bool {
        let Some(pin) = self.shape(id).cloned() else { return false };
        let (cx, cy) = pin.center();
        let reach = |s: &Shape| {
            // How far the center is from the border of the box: inside or outside.
            let inside = (s.x..=s.x + s.w).contains(&cx) && (s.y..=s.y + s.h).contains(&cy);
            if inside { (cx - s.x).min(s.x + s.w - cx).min(cy - s.y).min(s.y + s.h - cy) } else { (s.x - cx).max(cx - s.x - s.w).max(0.).hypot((s.y - cy).max(cy - s.y - s.h).max(0.)) }
        };
        let mut except = self.descendants(id);
        except.push(id);
        let holder = self.shapes.iter().rev().filter(|s| s.form != Form::Pin && s.form != Form::Text && !except.contains(&s.id) && reach(s) <= PIN).min_by(|a, b| reach(a).total_cmp(&reach(b))).map(|s| s.id);
        let changed = self.set_parent(id, holder);
        holder.is_some_and(|h| self.snap_to(id, h)) || changed
    }

    /// Moves pin `id` onto the nearest point of the border of `holder`. `true` if it moved.
    fn snap_to(&mut self, id: u32, holder: u32) -> bool {
        let (Some(h), Some(pin)) = (self.shape(holder).cloned(), self.shape(id).cloned()) else { return false };
        let (cx, cy) = pin.center();
        let (px, py) = (cx.clamp(h.x, h.x + h.w), cy.clamp(h.y, h.y + h.h));
        // On the side the point is nearest.
        let sides = [(px - h.x, (h.x, py)), (h.x + h.w - px, (h.x + h.w, py)), (py - h.y, (px, h.y)), (h.y + h.h - py, (px, h.y + h.h))];
        let (x, y) = sides.into_iter().min_by(|a, b| a.0.total_cmp(&b.0)).map_or((px, py), |(_, at)| at);
        let Some(shape) = self.shape_mut(id) else { return false };
        let (x, y) = (x - shape.w / 2., y - shape.h / 2.);
        let moved = (shape.x, shape.y) != (x, y);
        (shape.x, shape.y) = (x, y);
        moved
    }

    /// The name of a pin: outside the border it sits on, above and beside it at the sides, above or under it elsewhere.
    pub(super) fn pin_lines<'a>(&self, shape: &'a Shape) -> Vec<TextLine<'a>> {
        let (cx, cy) = shape.center();
        let lines: Vec<&str> = shape.text.split('\n').collect();
        let tall = LINE * lines.len() as f32;
        let wide = lines.iter().map(|l| Diagram::guess_width(l)).fold(0., f32::max);
        // 0 right, 1 left, 2 above, 3 under: the side of the holder the pin is nearest.
        let side = shape.parent.and_then(|p| self.shape(p)).map_or(3, |h| {
            let near = [h.x + h.w - cx, cx - h.x, cy - h.y, h.y + h.h - cy];
            (0..4).min_by(|a, b| near[*a].total_cmp(&near[*b])).unwrap_or(3)
        });
        let (x, y, centered) = match side {
            // Above the line an arrow leaves along, not on it.
            0 => (shape.x + shape.w + 2., shape.y - tall, false),
            1 => (shape.x - 2. - wide, shape.y - tall, false),
            2 => (cx, shape.y - 2. - tall, true),
            _ => (cx, shape.y + shape.h + 4., true),
        };
        lines.into_iter().enumerate().map(|(index, text)| TextLine { x, y: y + LINE * index as f32, text, centered, bold: false, index }).collect()
    }

    /// Keeps each pin on the border of its holder when that was moved or resized.
    pub fn seat_pins(&mut self) {
        let pins: Vec<(u32, u32)> = self.shapes.iter().filter(|s| s.form == Form::Pin).filter_map(|s| Some((s.id, s.parent?))).collect();
        for (id, holder) in pins {
            self.snap_to(id, holder);
        }
    }

    /// Resizes container `id` around what it holds, leaving room for its title.
    pub fn fit(&mut self, id: u32) {
        let inside: Vec<&Shape> = self.descendants(id).into_iter().filter_map(|d| self.shape(d)).collect();
        let Some(form) = self.shape(id).map(|s| (s.form, s.text.lines().count().max(1))) else { return };
        if inside.is_empty() {
            return;
        }
        let (x0, y0) = (inside.iter().map(|s| s.x).fold(f32::MAX, f32::min), inside.iter().map(|s| s.y).fold(f32::MAX, f32::min));
        let (x1, y1) = (inside.iter().map(|s| s.x + s.w).fold(f32::MIN, f32::max), inside.iter().map(|s| s.y + s.h).fold(f32::MIN, f32::max));
        let top = match form.0 {
            Form::Row => 20.,
            Form::Node => DEPTH + 6. + LINE * form.1 as f32 + 12.,
            Form::Package => 18. + 16.,
            Form::Lane | Form::Composite => LANE + 16.,
            _ => 6. + LINE * form.1 as f32 + 12.,
        };
        let grid = |v: f32, up: bool| if up { (v / GRID).ceil() * GRID } else { (v / GRID).floor() * GRID };
        // A horizontal swimlane has its header on the left.
        let (side, behind) = match form.0 {
            Form::Row => (ROW + 20., 20.),
            // A node has its right face behind its front face.
            Form::Node => (20., 20. + DEPTH),
            _ => (20., 20.),
        };
        let (left, above, right, below) = (grid(x0 - side, false), grid(y0 - top, false), grid(x1 + behind, true), grid(y1 + 20., true));
        if let Some(shape) = self.shape_mut(id) {
            (shape.x, shape.y, shape.w, shape.h) = (left, above, right - left, below - above);
        }
    }

    /// Puts holders before what they hold, and breaks the loops and the dangling holders of a damaged file.
    pub(super) fn arrange(&mut self) {
        for i in 0..self.shapes.len() {
            let id = self.shapes[i].id;
            let held = self.shapes[i].parent;
            if held.is_some_and(|p| self.shape(p).is_none() || self.within(p, id)) {
                self.shapes[i].parent = None;
            }
        }
        // Each pass lifts a holder over its first held shape; as many passes as shapes always suffice.
        for _ in 0..self.shapes.len() {
            let late = (0..self.shapes.len()).find_map(|i| {
                let p = self.shapes[i].parent?;
                let at = self.shapes.iter().position(|s| s.id == p)?;
                (at > i).then_some((at, i))
            });
            match late {
                Some((from, to)) => {
                    let holder = self.shapes.remove(from);
                    self.shapes.insert(to, holder);
                }
                None => break,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A package holding two classes, a frame beside it, and a loose note.
    fn sample() -> (Diagram, [u32; 5]) {
        let mut d = Diagram::default();
        let pack = d.add_shape(Form::Package, 0., 0., 300., 200.);
        let a = d.add_shape(Form::Rect, 20., 40., 100., 60.);
        let b = d.add_shape(Form::Rect, 160., 40., 100., 60.);
        let frame = d.add_shape(Form::Frame, 400., 0., 200., 200.);
        let note = d.add_shape(Form::Note, 700., 0., 60., 40.);
        for id in [a, b] {
            assert!(d.set_parent(id, Some(pack)));
        }
        (d, [pack, a, b, frame, note])
    }

    #[test]
    fn holders_come_first_and_loops_are_refused() {
        let (mut d, [pack, a, b, frame, _]) = sample();
        assert_eq!((d.children(pack), d.descendants(pack)), (vec![a, b], vec![a, b]));
        // A frame inside the package, a class inside the frame: any depth.
        assert!(d.set_parent(frame, Some(pack)) && d.set_parent(a, Some(frame)));
        assert_eq!(d.descendants(pack).len(), 3);
        assert!(d.within(a, pack) && d.within(a, a) && !d.within(pack, a));
        // Held shapes are after their holder, so drawn above it.
        let at = |d: &Diagram, id: u32| d.shapes.iter().position(|s| s.id == id).unwrap();
        assert!(at(&d, pack) < at(&d, frame) && at(&d, frame) < at(&d, a));
        // A holder cannot be put in what it holds, nor in itself; the same holder changes nothing.
        assert!(!d.set_parent(pack, Some(a)) && !d.set_parent(pack, Some(pack)) && !d.set_parent(a, Some(frame)) && !d.set_parent(a, Some(999)));
        assert!(d.set_parent(a, None) && d.shape(a).unwrap().parent.is_none());
        let _ = b;
    }

    #[test]
    fn follows_the_pointer_and_leaves_where_it_is_dropped() {
        let (mut d, [pack, a, _, frame, note]) = sample();
        assert_eq!((d.container_at(50., 50., &[]), d.container_at(450., 50., &[]), d.container_at(700., 20., &[]), d.container_at(50., 50., &[pack])), (Some(pack), Some(frame), None, None));
        // The note dragged over the frame is adopted; over nothing, it is let go.
        d.shape_mut(note).unwrap().x = 450.;
        assert!(d.adopt(&[note]) && d.shape(note).unwrap().parent == Some(frame));
        assert!(!d.adopt(&[note]));
        d.shape_mut(note).unwrap().x = 800.;
        assert!(d.adopt(&[note]) && d.shape(note).unwrap().parent.is_none());
        // A class dragged out of the package leaves it; dragged into the frame, it changes holder.
        d.shape_mut(a).unwrap().x = 420.;
        assert!(d.adopt(&[a]) && d.shape(a).unwrap().parent == Some(frame));
        // Dragged with its holder, a held shape stays with it: only the holder is looked at.
        let b = d.children(pack)[0];
        d.shape_mut(pack).unwrap().x = 1000.;
        d.shape_mut(b).unwrap().x += 1000.;
        assert!(!d.adopt(&[pack, b]) && d.shape(b).unwrap().parent == Some(pack));
        // A container is never adopted by what it holds.
        d.shape_mut(pack).unwrap().x = 420.;
        assert!(d.adopt(&[pack]) && d.shape(pack).unwrap().parent == Some(frame));
        assert!(d.container_at(430., 10., &[]).is_some());
    }

    #[test]
    fn copies_and_removes_what_a_holder_holds() {
        let (mut d, [pack, a, b, frame, _]) = sample();
        let inside = d.add_link(End::Shape(a), End::Shape(b), Head::Arrow, Route::Straight);
        let across = d.add_link(End::Shape(a), End::Shape(frame), Head::Arrow, Route::Straight);
        // Copying the package copies the classes, their arrow, and keeps the nesting; not the arrow that leaves.
        let made = d.duplicate(&[pack], 20.);
        assert_eq!((made.len(), d.shapes.len(), d.links.len()), (4, 8, 3));
        let copy = d.shape(made[0]).unwrap().clone();
        assert_eq!((copy.form, copy.x, copy.parent), (Form::Package, 20., None));
        assert!(made[1..3].iter().all(|id| d.shape(*id).unwrap().parent == Some(copy.id)));
        let held = d.links.iter().find(|l| l.id == made[3]).unwrap();
        assert_eq!((held.from, held.to), (End::Shape(made[1]), End::Shape(made[2])));
        // A held shape copied alone stays in its holder.
        let one = d.duplicate(&[a], 20.);
        assert_eq!(d.shape(one[0]).unwrap().parent, Some(pack));
        // Removing the package removes the classes, their copies and every arrow on them.
        d.remove(&[pack]);
        assert!(d.shape(a).is_none() && d.shape(b).is_none() && d.shape(one[0]).is_none());
        assert!(d.link(inside).is_none() && d.link(across).is_none());
        assert!(d.shape(frame).is_some() && d.shape(copy.id).is_some());
    }

    #[test]
    fn fits_around_what_it_holds() {
        let (mut d, [pack, ..]) = sample();
        d.fit(pack);
        let p = d.shape(pack).unwrap();
        // Classes at (20,40)-(260,100): 20 around, and the tab above.
        assert_eq!((p.x, p.y, p.w, p.h), (0., 0., 280., 120.));
        // Nothing held: nothing changes.
        let before = d.shape(pack).unwrap().clone();
        let frame = d.shapes.iter().find(|s| s.form == Form::Frame).unwrap().id;
        d.fit(frame);
        assert_eq!(d.shape(pack).unwrap(), &before);
        assert_eq!(d.shape(frame).unwrap().w, 200.);
    }

    #[test]
    fn a_damaged_file_is_put_in_order() {
        // A held shape before its holder, a holder that is not there, and two that hold each other.
        let mut d = Diagram::default();
        let (a, b, c, e) = (d.add_shape(Form::Rect, 0., 0., 10., 10.), d.add_shape(Form::Package, 0., 0., 10., 10.), d.add_shape(Form::Frame, 0., 0., 10., 10.), d.add_shape(Form::Frame, 0., 0., 10., 10.));
        d.shape_mut(a).unwrap().parent = Some(b);
        d.shape_mut(b).unwrap().parent = Some(99);
        d.shape_mut(c).unwrap().parent = Some(e);
        d.shape_mut(e).unwrap().parent = Some(c);
        d.arrange();
        let at = |id: u32| d.shapes.iter().position(|s| s.id == id).unwrap();
        assert!(at(b) < at(a) && d.shape(b).unwrap().parent.is_none());
        assert!(d.shape(c).unwrap().parent.is_none() || d.shape(e).unwrap().parent.is_none());
        assert_eq!(d.shapes.len(), 4);
        // Still finite: a descendant walk on what is left ends.
        assert!(d.descendants(c).len() <= 1);
    }

    #[test]
    fn a_pin_sits_on_the_border_of_the_shape_it_is_let_go_on() {
        let mut d = Diagram::blank();
        let action = d.add_shape(Form::Round, 100., 100., 160., 50.);
        let pin = d.add_shape(Form::Pin, 0., 0., PIN, PIN);
        let at = |d: &mut Diagram, x: f32, y: f32| {
            let shape = d.shape_mut(pin).unwrap();
            (shape.x, shape.y) = (x - PIN / 2., y - PIN / 2.);
        };
        let centre = |d: &Diagram| d.shape(pin).unwrap().center();
        // On the right side, a little inside or outside: it goes on the border, and the action holds it.
        for (x, y) in [(255., 120.), (265., 120.), (260., 120.)] {
            at(&mut d, x, y);
            d.adopt(&[pin]);
            assert_eq!((d.shape(pin).unwrap().parent, centre(&d)), (Some(action), (260., 120.)), "{x} {y}");
        }
        // The four sides, and a corner (the nearer side, never off the box).
        for ((x, y), want) in [((180., 98.), (180., 100.)), ((180., 152.), (180., 150.)), ((102., 125.), (100., 125.)), ((100., 100.), (100., 100.)), ((258., 148.), (260., 148.))] {
            at(&mut d, x, y);
            d.adopt(&[pin]);
            assert_eq!((d.shape(pin).unwrap().parent, centre(&d)), (Some(action), want), "{x} {y}");
        }
        // Far from every border, inside or outside, it is let go and stays where it is.
        for (x, y) in [(180., 125.), (400., 400.), (180., 60.)] {
            at(&mut d, x, y);
            d.adopt(&[pin]);
            assert_eq!((d.shape(pin).unwrap().parent, centre(&d)), (None, (x, y)), "{x} {y}");
        }
        // Held, it follows the action when that is moved or resized, staying on the border.
        at(&mut d, 260., 120.);
        d.adopt(&[pin]);
        assert!(d.descendants(action) == [pin]);
        d.shape_mut(action).unwrap().w = 100.;
        d.seat_pins();
        assert_eq!(centre(&d), (200., 120.));
        d.shape_mut(action).unwrap().h = 4.;
        d.seat_pins();
        let c = centre(&d);
        assert!(c.0 >= 100. && c.0 <= 200. && c.1 >= 100. && c.1 <= 104., "{c:?}");
        // A shape of no size, and two pins on the same spot, never panic or leave a pin off the box.
        let other = d.add_shape(Form::Pin, 0., 0., PIN, PIN);
        d.shape_mut(action).unwrap().w = 0.;
        d.shape_mut(action).unwrap().h = 0.;
        d.seat_pins();
        d.shape_mut(other).unwrap().x = 100. - PIN / 2.;
        d.shape_mut(other).unwrap().y = 100. - PIN / 2.;
        d.adopt(&[other]);
        assert!(d.shape(other).unwrap().center().0.is_finite());
        // Removing the action removes its pins, and the arrows on them.
        let arrow = d.add_link(End::Shape(pin), End::Shape(other), Head::Arrow, Route::Straight);
        d.remove(&[action]);
        assert!(d.shape(pin).is_none() && d.link(arrow).is_none());
    }

    #[test]
    fn the_name_of_a_pin_is_outside_the_border_it_sits_on() {
        let mut d = Diagram::blank();
        let action = d.add_shape(Form::Round, 100., 100., 160., 50.);
        let pin = d.add_shape(Form::Pin, 0., 0., PIN, PIN);
        d.shape_mut(pin).unwrap().text = "order".into();
        // Right and left: above and beside it, not on the arrow that leaves along its middle; top: above; bottom: under.
        for ((x, y), side) in [((260., 125.), "right"), ((100., 125.), "left"), ((180., 100.), "top"), ((180., 150.), "bottom")] {
            let shape = d.shape_mut(pin).unwrap();
            (shape.x, shape.y) = (x - PIN / 2., y - PIN / 2.);
            d.adopt(&[pin]);
            let pin_shape = d.shape(pin).unwrap();
            let (lines, _) = d.lines(pin_shape);
            let (cy, line) = (pin_shape.center().1, &lines[0]);
            let outside = match side {
                "right" => line.x > 260. && line.y + LINE <= cy + 1.,
                "left" => line.x + Diagram::guess_width("order") < 100. && line.y + LINE <= cy + 1.,
                "top" => line.y + LINE <= 100.,
                _ => line.y >= 150.,
            };
            assert!(outside, "{side}: {line:?}");
        }
        let _ = action;
    }

    #[test]
    fn a_row_holds_shapes_to_the_right_of_its_header() {
        let mut d = Diagram::blank();
        let row = d.add_shape(Form::Row, 0., 0., 500., 120.);
        d.shape_mut(row).unwrap().text = "Customer".into();
        let a = d.add_shape(Form::Round, 200., 30., 100., 50.);
        d.adopt(&[a]);
        assert_eq!(d.shape(a).unwrap().parent, Some(row));
        // Its name is in the header, on the left, in the middle of the height; the rest is in the body.
        let (lines, _) = d.lines(d.shape(row).unwrap());
        assert_eq!((lines[0].centered, lines[0].x, lines[0].y + LINE / 2.), (true, ROW / 2., 60.));
        // A name too long for the header starts at its left edge, inside the shape, whatever the text: empty, CJK, a long word.
        for name in ["", "Département des commandes internationales", &"x".repeat(300), "日本語日本語日本語日本語", "😀"] {
            d.shape_mut(row).unwrap().text = name.to_string();
            let shape = d.shape(row).unwrap();
            let (lines, _) = d.lines(shape);
            let line = &lines[0];
            assert!(line.x >= shape.x && (line.centered || line.x == shape.x + 6.), "{name:?}: {line:?}");
        }
        d.shape_mut(row).unwrap().text = "Customer".into();
        // It fits around what it holds, the header on the left.
        d.fit(row);
        let r = d.shape(row).unwrap();
        // Content at 200..300 by 30..80: 20 around it, and the header (100) on the left of it.
        assert_eq!((r.x, r.y, r.w, r.h), (80., 10., 240., 90.));
    }

    #[test]
    fn a_node_holds_what_runs_on_it_and_fits_around_it() {
        let mut d = Diagram::blank();
        let node = d.add_shape(Form::Node, 0., 0., 300., 200.);
        d.shape_mut(node).unwrap().text = "«device»\nServer".into();
        let artifact = d.add_shape(Form::Rect, 100., 100., 100., 50.);
        d.adopt(&[artifact]);
        assert_eq!(d.shape(artifact).unwrap().parent, Some(node));
        // The text is in the front face, under the top: 12 for the depth, 6 for the margin.
        let (lines, _) = d.lines(d.shape(node).unwrap());
        assert_eq!((lines[0].x, lines[0].y, lines[1].y), (8., 18., 36.));
        // Fitted around 100..200 by 100..150: the top (12 + 6 + two lines + 12 = 66 above), 20 left, 20 and the right face (12) on the right.
        d.fit(node);
        let n = d.shape(node).unwrap();
        assert_eq!((n.x, n.y, n.w, n.h), (80., 30., 160., 140.), "{n:?}");
        // A node smaller than its depth, or of no size, still draws: the faces never leave the box.
        for (w, h) in [(0., 0.), (5., 5.), (12., 200.), (200., 12.)] {
            let shape = Shape { id: 9, form: Form::Node, x: 10., y: 10., w, h, text: String::new(), color: 0, fill: false, dashed: false, parent: None, operand: 0 };
            let ok = |c: &Cmd| match *c { Cmd::Move(x, y) | Cmd::Line(x, y) => x >= 9.99 && x <= 10.01 + w && y >= 9.99 && y <= 10.01 + h, _ => true };
            assert!(shape.outline().iter().flatten().all(ok), "{w}x{h}");
        }
    }
}
