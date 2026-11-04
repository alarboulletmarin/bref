//! What a click grabs. A shape is grabbed firmly by what is drawn of it: its stroke, its text, its inside when
//! it is filled, all of it when it is small. The empty inside of a shape that is not filled only grabs it
//! softly: an arrow that passes there, or a shape seen through it, wins. That keeps the border and the name of
//! a frame easy to take, and what it surrounds reachable, whatever order they were drawn in.

use super::*;

/// A shape this narrow, or thinner, is grabbed anywhere in its box: there is no inside to see through.
const SMALL: f32 = 40.;

/// Distance from `p` to the segment from `a` to `b`.
fn distance(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let t = (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / (dx * dx + dy * dy).max(1e-6)).clamp(0., 1.);
    (p.0 - a.0 - t * dx).hypot(p.1 - a.1 - t * dy)
}

/// Whether `p` is within `reach` of the stroke of `path`; a curve is followed through eight points.
fn on_path(path: &[Cmd], p: (f32, f32), reach: f32) -> bool {
    let (mut at, mut start) = ((0., 0.), (0., 0.));
    for cmd in path {
        let next = match *cmd {
            Cmd::Move(x, y) => {
                (at, start) = ((x, y), (x, y));
                continue;
            }
            Cmd::Line(x, y) => vec![(x, y)],
            Cmd::Close => vec![start],
            Cmd::Curve(a, b, c, d, x, y) => (1..=8)
                .map(|i| {
                    let t = i as f32 / 8.;
                    let u = 1. - t;
                    let mix = |p0: f32, p1: f32, p2: f32, p3: f32| u * u * u * p0 + 3. * u * u * t * p1 + 3. * u * t * t * p2 + t * t * t * p3;
                    (mix(at.0, a, c, x), mix(at.1, b, d, y))
                })
                .collect(),
        };
        for point in next {
            if distance(p, at, point) <= reach {
                return true;
            }
            at = point;
        }
    }
    false
}

impl Diagram {
    /// How a click at this point takes `shape`: `Some(true)` firmly, `Some(false)` through its empty inside,
    /// `None` not at all.
    fn grip(&self, shape: &Shape, x: f32, y: f32, reach: f32) -> Option<bool> {
        if !shape.contains(x, y) {
            return None;
        }
        let small = shape.w.min(shape.h) <= SMALL;
        let stroke = || shape.outline().iter().any(|path| on_path(path, (x, y), reach));
        let text = || {
            self.lines(shape).0.iter().filter(|l| !l.text.trim().is_empty()).any(|l| {
                let w = Diagram::guess_width(l.text);
                let left = if l.centered { l.x - w / 2. } else { l.x };
                (left - reach..=left + w + reach).contains(&x) && (l.y..=l.y + LINE).contains(&y)
            })
        };
        Some(shape.fill || small || shape.form == Form::Text || stroke() || text())
    }

    /// The shapes under this point, in the order clicks go through them: those taken firmly, the one on top
    /// first; then those seen through, the smallest first (it is the one inside the others).
    pub fn shapes_under(&self, x: f32, y: f32, reach: f32) -> Vec<u32> {
        let (mut firm, mut soft) = (Vec::new(), Vec::new());
        for shape in self.shapes.iter().rev() {
            match self.grip(shape, x, y, reach) {
                Some(true) => firm.push(shape.id),
                Some(false) => soft.push(shape),
                None => {}
            }
        }
        soft.sort_by(|a, b| (a.w * a.h).total_cmp(&(b.w * b.h)));
        firm.into_iter().chain(soft.into_iter().map(|s| s.id)).collect()
    }

    /// Everything a click at this point can grab, in order: shapes taken firmly, then the arrows passing
    /// within `reach`, the one on top first, then the shapes seen through. The first is what a click takes;
    /// Alt+click goes on to the next.
    pub fn under(&self, x: f32, y: f32, reach: f32) -> Vec<u32> {
        let shapes = self.shapes_under(x, y, reach);
        let firm = shapes.iter().take_while(|id| self.shape(**id).is_some_and(|s| self.grip(s, x, y, reach) == Some(true))).count();
        let links = self.links_at(x, y, reach);
        shapes[..firm].iter().chain(&links).chain(&shapes[firm..]).copied().collect()
    }

    /// What a click at this point grabs: see `under`.
    pub fn hit(&self, x: f32, y: f32, reach: f32) -> Option<u32> {
        self.under(x, y, reach).first().copied()
    }

    /// The arrows passing within `reach` of this point, the one on top first.
    pub(super) fn links_at(&self, x: f32, y: f32, reach: f32) -> Vec<u32> {
        let flow = OnceCell::new();
        let hit = |l: &&Link| self.trace_in(l, &flow).is_some_and(|t| t.points.windows(2).any(|w| distance((x, y), w[0], w[1]) <= reach));
        self.links.iter().rev().filter(hit).map(|l| l.id).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An outer rectangle around an inner one, drawn in this order or the other.
    fn nested(outer_first: bool) -> (Diagram, u32, u32) {
        let mut d = Diagram::default();
        let (outer, inner) = if outer_first {
            let outer = d.add_shape(Form::Rect, 0., 0., 800., 600.);
            (outer, d.add_shape(Form::Rect, 200., 200., 200., 100.))
        } else {
            let inner = d.add_shape(Form::Rect, 200., 200., 200., 100.);
            (d.add_shape(Form::Rect, 0., 0., 800., 600.), inner)
        };
        (d, outer, inner)
    }

    #[test]
    fn what_is_inside_a_shape_is_reachable_whatever_the_order() {
        for outer_first in [true, false] {
            let (d, outer, inner) = nested(outer_first);
            assert_eq!(d.hit(300., 250., 7.), Some(inner), "outer first: {outer_first}");
            // The border of the outer one is still its own, and so is its empty inside.
            assert_eq!((d.hit(0., 300., 7.), d.hit(797., 300., 7.), d.hit(100., 100., 7.)), (Some(outer), Some(outer), Some(outer)));
            // Alt+click goes through: the inner one, then the outer one.
            assert_eq!(d.under(300., 250., 7.), vec![inner, outer]);
            assert_eq!(d.under(900., 250., 7.), Vec::<u32>::new());
        }
    }

    #[test]
    fn a_filled_shape_on_top_hides_what_is_under_it() {
        let (mut d, outer, inner) = nested(false);
        d.shape_mut(outer).unwrap().fill = true;
        assert_eq!(d.under(300., 250., 7.), vec![outer, inner]);
        // Filled under an empty one: the one on top is seen through.
        let (mut d, outer, inner) = nested(true);
        d.shape_mut(outer).unwrap().fill = true;
        assert_eq!(d.under(300., 250., 7.), vec![outer, inner]);
        d.shape_mut(inner).unwrap().fill = true;
        assert_eq!(d.under(300., 250., 7.), vec![inner, outer]);
    }

    #[test]
    fn the_name_and_the_stroke_grab_firmly() {
        let (mut d, outer, inner) = nested(false);
        // The text of the inner one is in its middle; the outer one's name is written over the inner one.
        d.shape_mut(inner).unwrap().text = "Inner".into();
        assert_eq!(d.hit(300., 250., 7.), Some(inner));
        let frame = d.add_shape(Form::Frame, 150., 150., 400., 300.);
        d.shape_mut(frame).unwrap().text = "System".into();
        let name = d.lines(d.shape(frame).unwrap()).0[0].clone();
        assert_eq!(d.hit(name.x + 4., name.y + 9., 7.), Some(frame));
        // Its stroke too, inside the outer one; its empty inside lets the outer one's stroke through.
        assert_eq!((d.hit(150., 300., 7.), d.hit(400., 447., 7.)), (Some(frame), Some(frame)));
        assert_eq!(d.hit(500., 400., 7.), Some(frame));
        assert_eq!(d.under(500., 400., 7.), vec![frame, outer]);
    }

    #[test]
    fn an_arrow_across_a_frame_is_reachable() {
        let mut d = Diagram::default();
        let frame = d.add_shape(Form::Frame, 0., 0., 800., 600.);
        let link = d.add_link(End::Point(50., 500.), End::Point(750., 500.), Head::None, Route::Straight);
        assert_eq!((d.links_at(400., 500., 7.), d.hit(400., 500., 7.)), (vec![link], Some(link)));
        assert_eq!(d.under(400., 500., 7.), vec![link, frame]);
        // A filled shape keeps the click, as before; Alt+click goes on to the arrow.
        d.shape_mut(frame).unwrap().fill = true;
        assert_eq!(d.under(400., 500., 7.), vec![frame, link]);
        // Away from it, the frame.
        d.shape_mut(frame).unwrap().fill = false;
        assert_eq!(d.hit(400., 300., 7.), Some(frame));
    }

    #[test]
    fn small_shapes_and_texts_are_grabbed_anywhere() {
        let mut d = Diagram::default();
        let frame = d.add_shape(Form::Frame, 0., 0., 400., 400.);
        let text = d.add_shape(Form::Text, 100., 100., 80., LINE);
        let dot = d.add_shape(Form::Dot, 200., 200., 20., 20.);
        assert_eq!((d.hit(110., 105., 7.), d.hit(211., 211., 7.), d.hit(300., 300., 7.)), (Some(text), Some(dot), Some(frame)));
    }

    #[test]
    fn four_levels_of_containers() {
        let mut d = Diagram::default();
        let boxes: Vec<u32> = (0..4).map(|i| d.add_shape(Form::Package, i as f32 * 50., i as f32 * 50., 800. - i as f32 * 100., 800. - i as f32 * 100.)).collect();
        // Drawn from the inside out or the outside in, the innermost wins in the middle, each one on its stroke.
        assert_eq!(d.under(400., 400., 7.), boxes.iter().rev().copied().collect::<Vec<_>>());
        for (i, id) in boxes.iter().enumerate() {
            assert_eq!(d.hit(i as f32 * 50., 400., 7.), Some(*id));
        }
        d.shapes.reverse();
        assert_eq!(d.hit(400., 400., 7.), Some(boxes[3]));
    }
}
