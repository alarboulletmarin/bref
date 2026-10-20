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
            let Some((cx, cy)) = self.shape(id).map(Shape::center) else { continue };
            let mut except = self.descendants(id);
            except.push(id);
            let holder = self.container_at(cx, cy, &except);
            changed |= self.set_parent(id, holder);
        }
        changed
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
            Form::Package => 18. + 16.,
            Form::Lane => LANE + 16.,
            _ => 6. + LINE * form.1 as f32 + 12.,
        };
        let grid = |v: f32, up: bool| if up { (v / GRID).ceil() * GRID } else { (v / GRID).floor() * GRID };
        let (left, above, right, below) = (grid(x0 - 20., false), grid(y0 - top, false), grid(x1 + 20., true), grid(y1 + 20., true));
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
}
