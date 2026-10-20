//! Diagrams: shapes, arrows between them, and their file. The file
//! is a plain SVG, readable anywhere, that also carries its source: that is what
//! the app rereads to continue the drawing. Pure functions, no UI dependency.

use std::fmt::Write;

use super::richtext::{self, Span, Style};

/// Stroke colors, as `0xrrggbb`: mid tones, readable on a light background
/// as on a dark one. The first is the neutral; in the app, the
/// theme replaces it with its text color.
pub const COLORS: [u32; 6] = [0x7d8590, 0xe5534b, 0xd98e2b, 0x3fa45b, 0x4a8fe0, 0xa371f7];
/// Step of the grid shapes snap to.
pub const GRID: f32 = 10.;
/// Text size and line height.
pub const FONT: f32 = 14.;
pub const LINE: f32 = 18.;
/// Text line that separates two compartments of a box (UML class, table).
pub const RULE: &str = "---";
/// Highest version of the file's source that this build reads. A file of a higher version
/// is never opened for editing: saving it would drop what this build does not know. A file is
/// written with the lowest version that can say what it holds (`Diagram::needed`), so a diagram
/// that uses nothing new still opens in the builds before.
///
/// 1: shapes and arrows. 2: `rich`, styled text.
pub const VERSION: u32 = 2;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Form {
    Rect,
    Round,
    Ellipse,
    Diamond,
    Text,
    Cylinder,
    Actor,
    Note,
}

impl Form {
    pub const ALL: [(Form, &str); 8] = [
        (Form::Rect, "rect"),
        (Form::Round, "round"),
        (Form::Ellipse, "ellipse"),
        (Form::Diamond, "diamond"),
        (Form::Text, "text"),
        (Form::Cylinder, "cylinder"),
        (Form::Actor, "actor"),
        (Form::Note, "note"),
    ];
}

/// Arrowhead. With solid or dashed stroke, they cover UML:
/// association, inheritance (triangle), aggregation and composition (diamonds).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Head {
    None,
    Arrow,
    Triangle,
    Diamond,
    DiamondFull,
}

impl Head {
    pub const ALL: [(Head, &str); 5] = [
        (Head::None, "none"),
        (Head::Arrow, "arrow"),
        (Head::Triangle, "triangle"),
        (Head::Diamond, "diamond"),
        (Head::DiamondFull, "fulldiamond"),
    ];

    /// The next head, to switch with one click.
    pub fn next(self) -> Head {
        let at = Head::ALL.iter().position(|(h, _)| *h == self).unwrap_or(0);
        Head::ALL[(at + 1) % Head::ALL.len()].0
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Shape {
    pub id: u32,
    pub form: Form,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub text: String,
    /// Index in `COLORS`.
    pub color: u8,
    /// Background tinted with the stroke color.
    pub fill: bool,
    pub dashed: bool,
}

/// End of an arrow: attached to a shape, which it follows, or set at a place.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum End {
    /// Attached to the shape, on the side facing the other end.
    Shape(u32),
    /// Attached to a precise point of the shape: its place in the rectangle that
    /// contains it, from 0 to 1 in width and height.
    Pin(u32, f32, f32),
    Point(f32, f32),
}

impl End {
    /// The shape this end is attached to.
    pub fn shape(self) -> Option<u32> {
        match self {
            End::Shape(id) | End::Pin(id, ..) => Some(id),
            End::Point(..) => None,
        }
    }
}

/// Route of an arrow between its two ends.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Route {
    Straight,
    /// Right-angled: it leaves the shape by a side, then turns.
    Elbow,
    Curve,
}

impl Route {
    pub const ALL: [(Route, &str); 3] = [(Route::Straight, "straight"), (Route::Elbow, "elbow"), (Route::Curve, "curve")];

    pub fn next(self) -> Route {
        let at = Route::ALL.iter().position(|(r, _)| *r == self).unwrap_or(0);
        Route::ALL[(at + 1) % Route::ALL.len()].0
    }
}

/// An arrow ready to draw.
pub struct Trace {
    /// The stroke, shortened where a closed head ends it.
    pub line: Vec<Cmd>,
    /// The heads: their path, and whether it is filled.
    pub heads: Vec<(Vec<Cmd>, bool)>,
    /// Middle of the stroke, where its text is written.
    pub middle: (f32, f32),
    /// The stroke is vertical at its middle: its text goes beside it, not on it.
    pub upright: bool,
    /// Points the stroke passes through, from one end to the other.
    pub points: Vec<(f32, f32)>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Link {
    pub id: u32,
    pub from: End,
    pub to: End,
    pub start: Head,
    pub end: Head,
    pub route: Route,
    pub text: String,
    pub color: u8,
    pub dashed: bool,
}

#[derive(Clone, Default, PartialEq, Debug)]
pub struct Diagram {
    /// In drawing order: the last shape is on top.
    pub shapes: Vec<Shape>,
    pub links: Vec<Link>,
    /// Texts are read for the marks of notes (`*italic*`, `<u>underline</u>`…). Off for a diagram
    /// made before they existed, whose texts keep their stars as they are.
    pub rich: bool,
    next: u32,
    /// Source lines this build cannot read, kept as they were and written back after the elements.
    kept: Vec<String>,
}

/// Path segment, as in the `d` attribute of a `<path>`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Cmd {
    Move(f32, f32),
    Line(f32, f32),
    /// Cubic Bézier curve: two control points, then the end point.
    Curve(f32, f32, f32, f32, f32, f32),
    Close,
}

/// A placed line of text: top-left corner or top middle, depending on `centered`.
#[derive(Clone, PartialEq, Debug)]
pub struct TextLine<'a> {
    pub x: f32,
    pub y: f32,
    pub text: &'a str,
    pub centered: bool,
    pub bold: bool,
}

pub fn snap(v: f32) -> f32 {
    (v / GRID).round() * GRID
}

fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32) -> Vec<Cmd> {
    // Four Bézier arcs: the usual circle approximation.
    let (kx, ky) = (rx * 0.5523, ry * 0.5523);
    vec![
        Cmd::Move(cx + rx, cy),
        Cmd::Curve(cx + rx, cy + ky, cx + kx, cy + ry, cx, cy + ry),
        Cmd::Curve(cx - kx, cy + ry, cx - rx, cy + ky, cx - rx, cy),
        Cmd::Curve(cx - rx, cy - ky, cx - kx, cy - ry, cx, cy - ry),
        Cmd::Curve(cx + kx, cy - ry, cx + rx, cy - ky, cx + rx, cy),
        Cmd::Close,
    ]
}

fn rounded(x: f32, y: f32, w: f32, h: f32, r: f32) -> Vec<Cmd> {
    let (r, k) = (r.min(w / 2.).min(h / 2.), 0.4477);
    let (x1, y1) = (x + w, y + h);
    vec![
        Cmd::Move(x + r, y),
        Cmd::Line(x1 - r, y),
        Cmd::Curve(x1 - r * k, y, x1, y + r * k, x1, y + r),
        Cmd::Line(x1, y1 - r),
        Cmd::Curve(x1, y1 - r * k, x1 - r * k, y1, x1 - r, y1),
        Cmd::Line(x + r, y1),
        Cmd::Curve(x + r * k, y1, x, y1 - r * k, x, y1 - r),
        Cmd::Line(x, y + r),
        Cmd::Curve(x, y + r * k, x + r * k, y, x + r, y),
        Cmd::Close,
    ]
}

impl Shape {
    pub fn center(&self) -> (f32, f32) {
        (self.x + self.w / 2., self.y + self.h / 2.)
    }

    pub fn contains(&self, x: f32, y: f32) -> bool {
        (self.x..=self.x + self.w).contains(&x) && (self.y..=self.y + self.h).contains(&y)
    }

    /// The text splits the box into compartments.
    fn boxed(&self) -> bool {
        matches!(self.form, Form::Rect | Form::Round) && self.text.lines().any(|l| l.trim() == RULE)
    }

    /// Paths of the shape. The first is its body, which can take a
    /// fill; the next ones are only strokes.
    pub fn outline(&self) -> Vec<Vec<Cmd>> {
        let (x, y, w, h) = (self.x, self.y, self.w, self.h);
        let (cx, cy) = self.center();
        match self.form {
            Form::Text => Vec::new(),
            Form::Rect => vec![rounded(x, y, w, h, 2.)],
            Form::Round => vec![rounded(x, y, w, h, 14.)],
            Form::Ellipse => vec![ellipse(cx, cy, w / 2., h / 2.)],
            Form::Diamond => vec![vec![
                Cmd::Move(cx, y),
                Cmd::Line(x + w, cy),
                Cmd::Line(cx, y + h),
                Cmd::Line(x, cy),
                Cmd::Close,
            ]],
            Form::Cylinder => {
                let (rx, ry) = (w / 2., (h * 0.12).min(12.));
                let (kx, ky) = (rx * 0.5523, ry * 0.5523);
                let (top, bottom) = (y + ry, y + h - ry);
                vec![
                    vec![
                        Cmd::Move(x, top),
                        Cmd::Curve(x, top - ky, cx - kx, y, cx, y),
                        Cmd::Curve(cx + kx, y, x + w, top - ky, x + w, top),
                        Cmd::Line(x + w, bottom),
                        Cmd::Curve(x + w, bottom + ky, cx + kx, y + h, cx, y + h),
                        Cmd::Curve(cx - kx, y + h, x, bottom + ky, x, bottom),
                        Cmd::Close,
                    ],
                    // The front edge of the lid.
                    vec![
                        Cmd::Move(x, top),
                        Cmd::Curve(x, top + ky, cx - kx, top + ry, cx, top + ry),
                        Cmd::Curve(cx + kx, top + ry, x + w, top + ky, x + w, top),
                    ],
                ]
            }
            Form::Actor => {
                let r = (h * 0.14).min(w / 2.);
                let (neck, hip) = (y + 2. * r, y + h * 0.62);
                vec![
                    ellipse(cx, y + r, r, r),
                    vec![Cmd::Move(cx, neck), Cmd::Line(cx, hip)],
                    vec![Cmd::Move(x, y + h * 0.38), Cmd::Line(x + w, y + h * 0.38)],
                    vec![Cmd::Move(x, y + h), Cmd::Line(cx, hip), Cmd::Line(x + w, y + h)],
                ]
            }
            Form::Note => {
                let fold = 12f32.min(w / 2.).min(h / 2.);
                vec![
                    vec![
                        Cmd::Move(x, y),
                        Cmd::Line(x + w - fold, y),
                        Cmd::Line(x + w, y + fold),
                        Cmd::Line(x + w, y + h),
                        Cmd::Line(x, y + h),
                        Cmd::Close,
                    ],
                    vec![Cmd::Move(x + w - fold, y), Cmd::Line(x + w - fold, y + fold), Cmd::Line(x + w, y + fold)],
                ]
            }
        }
    }

    /// Where each line of text is written, and the height of the separator strokes.
    /// A compartment box is written from the top, its first compartment
    /// centered and bold (the class name), the next ones left-aligned.
    pub fn text_lines(&self) -> (Vec<TextLine<'_>>, Vec<f32>) {
        let (cx, cy) = self.center();
        let lines: Vec<&str> = self.text.split('\n').collect();
        let mut rules = Vec::new();
        if self.boxed() {
            let (mut y, mut first) = (self.y + 6., true);
            let mut out = Vec::new();
            for text in lines {
                if text.trim() == RULE {
                    rules.push(y + 4.);
                    y += 8.;
                    first = false;
                } else {
                    let x = if first { cx } else { self.x + 8. };
                    out.push(TextLine { x, y, text, centered: first, bold: first });
                    y += LINE;
                }
            }
            return (out, rules);
        }
        let height = lines.len() as f32 * LINE;
        let (x, top, centered) = match self.form {
            Form::Text => (self.x, self.y, false),
            // Under the stick figure.
            Form::Actor => (cx, self.y + self.h + 4., true),
            _ => (cx, cy - height / 2., true),
        };
        let placed = lines.into_iter().enumerate();
        (placed.map(|(i, text)| TextLine { x, y: top + i as f32 * LINE, text, centered, bold: false }).collect(), rules)
    }

    /// Arrow end for a point of the shape: in the middle, the attachment follows
    /// the other end; near an edge, it sticks to that edge, at the nearest quarter.
    pub fn hook(&self, x: f32, y: f32) -> End {
        let (fx, fy) = (((x - self.x) / self.w.max(1.)).clamp(0., 1.), ((y - self.y) / self.h.max(1.)).clamp(0., 1.));
        if (fx - 0.5).abs() < 0.25 && (fy - 0.5).abs() < 0.25 {
            return End::Shape(self.id);
        }
        let quarter = |v: f32| (v * 4.).round() / 4.;
        let edges = [(fx, (0., quarter(fy))), (1. - fx, (1., quarter(fy))), (fy, (quarter(fx), 0.)), (1. - fy, (quarter(fx), 1.))];
        let (_, (px, py)) = edges.into_iter().min_by(|a, b| a.0.total_cmp(&b.0)).unwrap();
        End::Pin(self.id, px, py)
    }

    /// Attachment points offered around the shape.
    pub fn hooks(&self) -> Vec<(f32, f32)> {
        let quarters = [0., 0.25, 0.5, 0.75, 1.];
        let around = quarters.iter().flat_map(|&q| [(q, 0.), (q, 1.), (0., q), (1., q)]);
        around.map(|(fx, fy)| self.border((self.x + fx * self.w, self.y + fy * self.h))).collect()
    }

    /// Point of the outline on the ray from the center toward `toward`.
    pub fn border(&self, toward: (f32, f32)) -> (f32, f32) {
        let (cx, cy) = self.center();
        let (dx, dy) = (toward.0 - cx, toward.1 - cy);
        let (a, b) = ((self.w / 2.).max(1.), (self.h / 2.).max(1.));
        let reach = match self.form {
            Form::Ellipse => ((dx / a).powi(2) + (dy / b).powi(2)).sqrt(),
            Form::Diamond => dx.abs() / a + dy.abs() / b,
            _ => (dx.abs() / a).max(dy.abs() / b),
        };
        if reach < 1e-6 { (cx, cy) } else { (cx + dx / reach, cy + dy / reach) }
    }
}

/// Polygon of a head set at `tip`, for a stroke arriving in the
/// unit direction `u`: its points, whether it is closed, whether it is filled, and by
/// how much to shorten the stroke so it stops at its base.
pub fn head(head: Head, tip: (f32, f32), u: (f32, f32)) -> (Vec<(f32, f32)>, bool, bool, f32) {
    let n = (-u.1, u.0);
    let at = |back: f32, side: f32| (tip.0 - u.0 * back + n.0 * side, tip.1 - u.1 * back + n.1 * side);
    match head {
        Head::None => (Vec::new(), false, false, 0.),
        Head::Arrow => (vec![at(10., 5.), tip, at(10., -5.)], false, false, 0.),
        Head::Triangle => (vec![tip, at(12., 6.), at(12., -6.)], true, false, 12.),
        Head::Diamond => (vec![tip, at(8., 5.), at(16., 0.), at(8., -5.)], true, false, 16.),
        Head::DiamondFull => (vec![tip, at(8., 5.), at(16., 0.), at(8., -5.)], true, true, 16.),
    }
}

impl Diagram {
    /// An empty diagram of today: its texts are styled.
    pub fn blank() -> Diagram {
        Diagram { rich: true, ..Diagram::default() }
    }

    /// The runs of one line of text, in the styles its marks ask for, if this diagram reads them.
    pub fn styled(&self, text: &str, bold: bool) -> Vec<Span> {
        if self.rich { richtext::spans(text, bold) } else { richtext::plain(text, bold) }
    }

    /// The lowest version of the source that can say all this diagram holds.
    fn needed(&self) -> u32 {
        if self.rich { 2 } else { 1 }
    }

    pub fn shape(&self, id: u32) -> Option<&Shape> {
        self.shapes.iter().find(|s| s.id == id)
    }

    pub fn shape_mut(&mut self, id: u32) -> Option<&mut Shape> {
        self.shapes.iter_mut().find(|s| s.id == id)
    }

    pub fn link(&self, id: u32) -> Option<&Link> {
        self.links.iter().find(|l| l.id == id)
    }

    pub fn link_mut(&mut self, id: u32) -> Option<&mut Link> {
        self.links.iter_mut().find(|l| l.id == id)
    }

    fn fresh(&mut self) -> u32 {
        self.next += 1;
        self.next
    }

    pub fn add_shape(&mut self, form: Form, x: f32, y: f32, w: f32, h: f32) -> u32 {
        let id = self.fresh();
        self.shapes.push(Shape { id, form, x, y, w, h, text: String::new(), color: 0, fill: false, dashed: false });
        id
    }

    pub fn add_link(&mut self, from: End, to: End, end: Head, route: Route) -> u32 {
        let id = self.fresh();
        self.links.push(Link { id, from, to, start: Head::None, end, route, text: String::new(), color: 0, dashed: false });
        id
    }

    /// Copy of elements `ids`, offset; returns the ids of the copies.
    /// An arrow is only copied along with the shapes it links.
    pub fn duplicate(&mut self, ids: &[u32], by: f32) -> Vec<u32> {
        let mut twin = Vec::new();
        for shape in self.shapes.clone().into_iter().filter(|s| ids.contains(&s.id)) {
            let id = self.fresh();
            twin.push((shape.id, id));
            self.shapes.push(Shape { id, x: shape.x + by, y: shape.y + by, ..shape });
        }
        let copy = |id: u32| twin.iter().find(|(old, _)| *old == id).map(|(_, new)| *new);
        let moved = |end: End| match end {
            End::Shape(id) => copy(id).map(End::Shape),
            End::Pin(id, fx, fy) => copy(id).map(|id| End::Pin(id, fx, fy)),
            End::Point(x, y) => Some(End::Point(x + by, y + by)),
        };
        let mut made: Vec<u32> = twin.iter().map(|(_, new)| *new).collect();
        for link in self.links.clone().into_iter().filter(|l| ids.contains(&l.id)) {
            if let (Some(from), Some(to)) = (moved(link.from), moved(link.to)) {
                let id = self.fresh();
                made.push(id);
                self.links.push(Link { id, from, to, ..link });
            }
        }
        made
    }

    /// Removes elements `ids`, and the arrows attached to removed shapes.
    pub fn remove(&mut self, ids: &[u32]) {
        self.shapes.retain(|s| !ids.contains(&s.id));
        let gone = |end: End| end.shape().is_some_and(|id| ids.contains(&id));
        self.links.retain(|l| !ids.contains(&l.id) && !gone(l.from) && !gone(l.to));
    }

    /// The topmost shape under this point.
    pub fn shape_at(&self, x: f32, y: f32) -> Option<u32> {
        self.shapes.iter().rev().find(|s| s.contains(x, y)).map(|s| s.id)
    }

    /// The arrow passing within `reach` of this point.
    pub fn link_at(&self, x: f32, y: f32, reach: f32) -> Option<u32> {
        let near = |(ax, ay): (f32, f32), (bx, by): (f32, f32)| {
            let (dx, dy) = (bx - ax, by - ay);
            let t = (((x - ax) * dx + (y - ay) * dy) / (dx * dx + dy * dy).max(1e-6)).clamp(0., 1.);
            (x - ax - t * dx).hypot(y - ay - t * dy) <= reach
        };
        let hit = |l: &&Link| self.trace(l).is_some_and(|t| t.points.windows(2).any(|w| near(w[0], w[1])));
        self.links.iter().rev().find(hit).map(|l| l.id)
    }

    /// The two ends of the stroke, where it touches what it links. `None` if an
    /// attached shape is gone.
    pub fn ends(&self, link: &Link) -> Option<((f32, f32), (f32, f32))> {
        let points = self.trace(link)?.points;
        Some((*points.first()?, *points.last()?))
    }

    /// Where an end leaves what it holds, and in which direction: toward
    /// `toward` for a straight stroke, otherwise through the side facing it.
    fn leave(&self, end: End, toward: (f32, f32), straight: bool) -> Option<((f32, f32), (f32, f32))> {
        let side = |dx: f32, dy: f32, w: f32, h: f32| if dx.abs() * h >= dy.abs() * w { (dx.signum(), 0.) } else { (0., dy.signum()) };
        Some(match end {
            End::Point(x, y) => ((x, y), side(toward.0 - x, toward.1 - y, 1., 1.)),
            End::Shape(id) => {
                let shape = self.shape(id)?;
                let (cx, cy) = shape.center();
                let out = side(toward.0 - cx, toward.1 - cy, shape.w, shape.h);
                let aim = if straight { toward } else { (cx + out.0 * 1e4, cy + out.1 * 1e4) };
                (shape.border(aim), out)
            }
            End::Pin(id, fx, fy) => {
                let shape = self.shape(id)?;
                let out = if fx <= 0. { (-1., 0.) } else if fx >= 1. { (1., 0.) } else if fy <= 0. { (0., -1.) } else { (0., 1.) };
                (shape.border((shape.x + fx * shape.w, shape.y + fy * shape.h)), out)
            }
        })
    }

    /// Drawing of an arrow: its stroke along its route, its heads, its middle.
    // ponytail: the elbow route leaves by the chosen side and turns at a right angle,
    // without going around shapes in its way; the attachment point is what
    // lets it pass elsewhere. Search for a free path (A* on a
    // grid) if dense diagrams call for it.
    pub fn trace(&self, link: &Link) -> Option<Trace> {
        let aim = |end: End| match end {
            End::Shape(id) => self.shape(id).map(Shape::center),
            End::Pin(id, fx, fy) => self.shape(id).map(|s| (s.x + fx * s.w, s.y + fy * s.h)),
            End::Point(x, y) => Some((x, y)),
        };
        let straight = link.route == Route::Straight;
        let (a, da) = self.leave(link.from, aim(link.to)?, straight)?;
        let (b, db) = self.leave(link.to, aim(link.from)?, straight)?;
        let reach = ((b.0 - a.0).hypot(b.1 - a.1) * 0.4).max(30.);
        let (c1, c2) = ((a.0 + da.0 * reach, a.1 + da.1 * reach), (b.0 + db.0 * reach, b.1 + db.1 * reach));
        let mut points = match link.route {
            Route::Straight => vec![a, b],
            Route::Elbow => elbow(a, da, b, db),
            Route::Curve => (0..=16)
                .map(|i| {
                    let (t, u) = (i as f32 / 16., 1. - i as f32 / 16.);
                    let mix = |p: f32, q: f32, r: f32, s: f32| u * u * u * p + 3. * u * u * t * q + 3. * u * t * t * r + t * t * t * s;
                    (mix(a.0, c1.0, c2.0, b.0), mix(a.1, c1.1, c2.1, b.1))
                })
                .collect(),
        };
        let toward = |from: (f32, f32), to: (f32, f32)| {
            let len = (to.0 - from.0).hypot(to.1 - from.1).max(1e-3);
            ((to.0 - from.0) / len, (to.1 - from.1) / len)
        };
        let last = points.len() - 1;
        let (ua, ub) = (toward(points[1], points[0]), toward(points[last - 1], points[last]));
        let (start, end) = (head(link.start, a, ua), head(link.end, b, ub));
        // The stroke stops at the base of a closed head.
        let (a2, b2) = ((a.0 - ua.0 * start.3, a.1 - ua.1 * start.3), (b.0 - ub.0 * end.3, b.1 - ub.1 * end.3));
        let line = match link.route {
            Route::Curve => vec![Cmd::Move(a2.0, a2.1), Cmd::Curve(c1.0, c1.1, c2.0, c2.1, b2.0, b2.1)],
            _ => {
                let mut drawn = points.clone();
                (drawn[0], drawn[last]) = (a2, b2);
                drawn.iter().enumerate().map(|(i, p)| if i == 0 { Cmd::Move(p.0, p.1) } else { Cmd::Line(p.0, p.1) }).collect()
            }
        };
        let heads = [start, end].into_iter().filter(|(points, ..)| !points.is_empty()).map(|(points, closed, full, _)| {
            let mut path: Vec<Cmd> = points.iter().enumerate().map(|(i, p)| if i == 0 { Cmd::Move(p.0, p.1) } else { Cmd::Line(p.0, p.1) }).collect();
            if closed {
                path.push(Cmd::Close);
            }
            (path, full)
        });
        // The middle is found along the stroke, not between its two ends.
        let lengths: Vec<f32> = points.windows(2).map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1)).collect();
        let mut left = lengths.iter().sum::<f32>() / 2.;
        let (mut middle, mut upright) = (a, false);
        for (w, len) in points.windows(2).zip(&lengths) {
            if left <= *len {
                let t = left / len.max(1e-3);
                middle = (w[0].0 + (w[1].0 - w[0].0) * t, w[0].1 + (w[1].1 - w[0].1) * t);
                upright = (w[1].1 - w[0].1).abs() > (w[1].0 - w[0].0).abs();
                break;
            }
            left -= len;
        }
        points.dedup();
        Some(Trace { line, heads: heads.collect(), middle, upright, points })
    }

    /// Where line `i` of the `count` lines of an arrow's text is written: above
    /// a lying stroke, centered; right of a standing stroke. Also returns whether
    /// it is centered.
    pub fn label_at(trace: &Trace, i: usize, count: usize) -> (f32, f32, bool) {
        let (x, y) = trace.middle;
        match trace.upright {
            true => (x + 6., y - LINE * count as f32 / 2. + LINE * i as f32, false),
            false => (x, y - LINE * (count - i) as f32 - 2., true),
        }
    }

    /// Rectangle containing the whole diagram: (left, top, right, bottom).
    pub fn bounds(&self) -> Option<(f32, f32, f32, f32)> {
        // A stick figure's text is written under it.
        let boxes = self.shapes.iter().map(|s| {
            let below = if s.form == Form::Actor { 4. + LINE * s.text.split('\n').count() as f32 } else { 0. };
            (s.x, s.y, s.x + s.w, s.y + s.h + below)
        });
        let ends = self.links.iter().filter_map(|l| self.trace(l)).flat_map(|t| t.points);
        boxes.chain(ends.map(|(x, y)| (x, y, x, y))).reduce(|a, b| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)))
    }

    /// The diagram file: its drawing as SVG, and its source in `<metadata>`.
    /// `neutral` is the neutral stroke color, as `0xrrggbb`.
    pub fn to_svg(&self, neutral: u32) -> String {
        let color = |i: u8| format!("#{:06x}", if i == 0 { neutral } else { COLORS[i as usize % COLORS.len()] });
        let (x0, y0, x1, y1) = self.bounds().unwrap_or((0., 0., 100., 60.));
        let (x0, y0, w, h) = (x0 - 16., y0 - 16., x1 - x0 + 32., y1 - y0 + 32.);
        let mut out = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"{} {} {} {}\" fill=\"none\" stroke-width=\"1.5\" stroke-linecap=\"round\" stroke-linejoin=\"round\" font-family=\"sans-serif\" font-size=\"{FONT}\">\n",
            num(w), num(h), num(x0), num(y0), num(w), num(h),
        );
        let dash = |dashed: bool| if dashed { " stroke-dasharray=\"6 4\"" } else { "" };
        let label = |out: &mut String, x: f32, y: f32, spans: &[Span], centered: bool, ink: &str| {
            if spans.is_empty() {
                return;
            }
            let anchor = if centered { " text-anchor=\"middle\"" } else { "" };
            // One style for the whole line: on the `<text>`; several: a `<tspan>` for each run.
            let uniform = spans.windows(2).all(|w| w[0].style == w[1].style);
            let (attrs, body) = match uniform {
                true => (style_attrs(spans[0].style), xml(&spans.iter().map(|s| s.text.as_str()).collect::<String>())),
                false => (String::new(), spans.iter().map(|s| format!("<tspan{}>{}</tspan>", style_attrs(s.style), xml(&s.text))).collect()),
            };
            let _ = writeln!(out, "<text x=\"{}\" y=\"{}\"{anchor}{attrs} fill=\"{ink}\" stroke=\"none\">{body}</text>", num(x), num(y + FONT));
        };
        for shape in &self.shapes {
            let ink = color(shape.color);
            for (i, path) in shape.outline().iter().enumerate() {
                let fill = if i == 0 && shape.fill { format!(" fill=\"{ink}\" fill-opacity=\"0.15\"") } else { String::new() };
                let _ = writeln!(out, "<path d=\"{}\" stroke=\"{ink}\"{fill}{}/>", path_d(path), dash(shape.dashed));
            }
            let (lines, rules) = shape.text_lines();
            for y in rules {
                let _ = writeln!(out, "<path d=\"M{} {}H{}\" stroke=\"{ink}\"/>", num(shape.x), num(y), num(shape.x + shape.w));
            }
            for line in lines {
                label(&mut out, line.x, line.y, &self.styled(line.text, line.bold), line.centered, &ink);
            }
        }
        for link in &self.links {
            let Some(trace) = self.trace(link) else { continue };
            let ink = color(link.color);
            let _ = writeln!(out, "<path d=\"{}\" stroke=\"{ink}\"{}/>", path_d(&trace.line), dash(link.dashed));
            for (path, full) in &trace.heads {
                let fill = if *full { format!(" fill=\"{ink}\"") } else { String::new() };
                let _ = writeln!(out, "<path d=\"{}\" stroke=\"{ink}\"{fill}/>", path_d(path));
            }
            let lines: Vec<&str> = link.text.split('\n').collect();
            for (i, text) in lines.iter().enumerate() {
                let (x, y, centered) = Diagram::label_at(&trace, i, lines.len());
                label(&mut out, x, y, &self.styled(text, false), centered, &ink);
            }
        }
        let _ = write!(out, "<metadata id=\"bref-diagram\">\n{}</metadata>\n</svg>\n", xml(&self.source()));
        out
    }

    /// The source: its version, then one line per element, text last, then the lines kept as found.
    fn source(&self) -> String {
        let name = |form: Form| Form::ALL.iter().find(|(f, _)| *f == form).map_or("rect", |(_, n)| n);
        let tip = |head: Head| Head::ALL.iter().find(|(h, _)| *h == head).map_or("none", |(_, n)| n);
        let end = |end: End| match end {
            End::Shape(id) => format!("s{id}"),
            End::Pin(id, fx, fy) => format!("s{id}@{fx},{fy}"),
            End::Point(x, y) => format!("p{},{}", num(x), num(y)),
        };
        let text = |text: &str| text.replace('\\', "\\\\").replace('\n', "\\n");
        let mut out = format!("version {}\n{}", self.needed(), if self.rich { "rich 1\n" } else { "" });
        for s in &self.shapes {
            let _ = writeln!(out, "shape {} {} {} {} {} {} {} {} {} {}", s.id, name(s.form), num(s.x), num(s.y), num(s.w), num(s.h), s.color, s.fill as u8, s.dashed as u8, text(&s.text));
        }
        for l in &self.links {
            let route = Route::ALL.iter().find(|(r, _)| *r == l.route).map_or("straight", |(_, n)| n);
            let _ = writeln!(out, "link {} {} {} {} {} {route} {} {} {}", l.id, end(l.from), end(l.to), tip(l.start), tip(l.end), l.color, l.dashed as u8, text(&l.text));
        }
        for line in &self.kept {
            let _ = writeln!(out, "{line}");
        }
        out
    }

    /// The source a Bref diagram file carries, unescaped.
    fn source_of(svg: &str) -> Option<String> {
        let body = svg.split_once("<metadata id=\"bref-diagram\">")?.1.split_once("</metadata>")?.0;
        Some(body.replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&"))
    }

    /// Version declared by a source: the first `version` line, 0 without one. An
    /// unreadable number counts as the highest: what is not understood is not overwritten.
    fn version_of(source: &str) -> u32 {
        match source.lines().find(|l| is_version(l)) {
            Some(line) => line.strip_prefix("version ").and_then(|v| v.parse().ok()).unwrap_or(u32::MAX),
            None => 0,
        }
    }

    /// Whether this is a Bref diagram written by a version this one cannot read.
    pub fn newer(svg: &str) -> bool {
        Diagram::source_of(svg).is_some_and(|source| Diagram::version_of(&source) > VERSION)
    }

    /// The diagram this SVG file carries; `None` if it is not one, or if a newer version wrote it.
    pub fn from_svg(svg: &str) -> Option<Diagram> {
        let body = Diagram::source_of(svg)?;
        if Diagram::version_of(&body) > VERSION {
            return None;
        }
        let text = |raw: &str| {
            let (mut out, mut chars) = (String::new(), raw.chars());
            while let Some(c) = chars.next() {
                match (c == '\\').then(|| chars.next()).flatten() {
                    Some('n') => out.push('\n'),
                    Some(other) => out.push(other),
                    None => out.push(c),
                }
            }
            out
        };
        let end = |raw: &str| match raw.split_at_checked(1)? {
            ("s", id) => match id.split_once('@') {
                Some((id, at)) => at.split_once(',').and_then(|(fx, fy)| Some(End::Pin(id.parse().ok()?, fx.parse().ok()?, fy.parse().ok()?))),
                None => Some(End::Shape(id.parse().ok()?)),
            },
            ("p", at) => at.split_once(',').and_then(|(x, y)| Some(End::Point(x.parse().ok()?, y.parse().ok()?))),
            _ => None,
        };
        let mut diagram = Diagram::default();
        // A line this version cannot read is kept as it is: the rest of the diagram opens.
        let mut declared = false;
        for line in body.lines() {
            // The first `version` line is the file's; a second one is just another unknown line.
            let version = !declared && is_version(line);
            declared |= version;
            if version || line.trim().is_empty() {
                continue;
            }
            let mut read = || -> Option<()> {
                if let Some(rest) = line.strip_prefix("shape ") {
                    let f: Vec<&str> = rest.splitn(10, ' ').collect();
                    let n = |i: usize| f.get(i)?.parse::<f32>().ok();
                    diagram.shapes.push(Shape {
                        id: f.first()?.parse().ok()?,
                        form: Form::ALL.iter().find(|(_, name)| Some(name) == f.get(1)).map(|(form, _)| *form)?,
                        x: n(2)?,
                        y: n(3)?,
                        w: n(4)?,
                        h: n(5)?,
                        color: f.get(6)?.parse().ok()?,
                        fill: *f.get(7)? == "1",
                        dashed: *f.get(8)? == "1",
                        text: text(f.get(9).unwrap_or(&"")),
                    });
                    Some(())
                } else if line == "rich 1" {
                    diagram.rich = true;
                    Some(())
                } else if let Some(rest) = line.strip_prefix("link ") {
                    let f: Vec<&str> = rest.splitn(9, ' ').collect();
                    let tip = |i: usize| Head::ALL.iter().find(|(_, name)| Some(name) == f.get(i)).map(|(head, _)| *head);
                    diagram.links.push(Link {
                        id: f.first()?.parse().ok()?,
                        from: end(f.get(1)?)?,
                        to: end(f.get(2)?)?,
                        start: tip(3)?,
                        end: tip(4)?,
                        route: Route::ALL.iter().find(|(_, name)| Some(name) == f.get(5)).map(|(route, _)| *route)?,
                        color: f.get(6)?.parse().ok()?,
                        dashed: *f.get(7)? == "1",
                        text: text(f.get(8).unwrap_or(&"")),
                    });
                    Some(())
                } else {
                    None
                }
            };
            if read().is_none() {
                diagram.kept.push(line.to_string());
            }
        }
        // A kept line still owns its id (its second field), which no new element may take.
        let owned = diagram.kept.iter().filter_map(|l| l.split(' ').nth(1)?.parse().ok());
        let ids = diagram.shapes.iter().map(|s| s.id).chain(diagram.links.iter().map(|l| l.id)).chain(owned);
        diagram.next = ids.max().unwrap_or(0);
        Some(diagram)
    }
}

/// SVG attributes of a text style, with a leading space each.
fn style_attrs(style: Style) -> String {
    let mut out = String::new();
    if style.bold {
        out += " font-weight=\"bold\"";
    }
    if style.italic {
        out += " font-style=\"italic\"";
    }
    match (style.under, style.strike) {
        (true, true) => out += " text-decoration=\"underline line-through\"",
        (true, false) => out += " text-decoration=\"underline\"",
        (false, true) => out += " text-decoration=\"line-through\"",
        _ => {}
    }
    if style.code {
        out += " font-family=\"monospace\"";
    }
    out
}

fn is_version(line: &str) -> bool {
    line == "version" || line.starts_with("version ")
}

/// Right-angled stroke from `a` to `b`, leaving in direction `da` and
/// arriving by `db` (the sides by which it leaves what it links).
fn elbow(a: (f32, f32), da: (f32, f32), b: (f32, f32), db: (f32, f32)) -> Vec<(f32, f32)> {
    const STUB: f32 = 20.;
    let flat = |d: (f32, f32)| d.0 != 0.;
    let (a1, b1) = ((a.0 + da.0 * STUB, a.1 + da.1 * STUB), (b.0 + db.0 * STUB, b.1 + db.1 * STUB));
    let facing = da == (-db.0, -db.1) && (b.0 - a.0) * da.0 + (b.1 - a.1) * da.1 > 2. * STUB;
    let mut points = match (flat(da), flat(db)) {
        // Facing each other: a single jog, halfway.
        (true, true) if facing => vec![a, ((a.0 + b.0) / 2., a.1), ((a.0 + b.0) / 2., b.1), b],
        (false, false) if facing => vec![a, (a.0, (a.1 + b.1) / 2.), (b.0, (a.1 + b.1) / 2.), b],
        // On the same side: the stroke goes wide of both.
        (true, true) if da == db => {
            let x = if da.0 > 0. { a1.0.max(b1.0) } else { a1.0.min(b1.0) };
            vec![a, (x, a.1), (x, b.1), b]
        }
        (false, false) if da == db => {
            let y = if da.1 > 0. { a1.1.max(b1.1) } else { a1.1.min(b1.1) };
            vec![a, (a.0, y), (b.0, y), b]
        }
        // Back to back: it leaves, passes between the two, then comes back.
        (true, true) => vec![a, a1, (a1.0, (a.1 + b.1) / 2.), (b1.0, (a.1 + b.1) / 2.), b1, b],
        (false, false) => vec![a, a1, ((a.0 + b.0) / 2., a1.1), ((a.0 + b.0) / 2., b1.1), b1, b],
        (true, false) => vec![a, (b.0, a.1), b],
        (false, true) => vec![a, (a.0, b.1), b],
    };
    // A point aligned with its neighbors is useless.
    points.dedup();
    let mut i = 1;
    while i + 1 < points.len() {
        let (p, q, r) = (points[i - 1], points[i], points[i + 1]);
        if ((q.0 - p.0) * (r.1 - q.1) - (q.1 - p.1) * (r.0 - q.0)).abs() < 1e-3 {
            points.remove(i);
        } else {
            i += 1;
        }
    }
    points
}

/// Number written to the tenth, without a useless decimal.
fn num(v: f32) -> String {
    let v = (v * 10.).round() / 10.;
    if v.fract() == 0. { format!("{v:.0}") } else { format!("{v:.1}") }
}

fn xml(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn path_d(path: &[Cmd]) -> String {
    let mut d = String::new();
    for cmd in path {
        let _ = match *cmd {
            Cmd::Move(x, y) => write!(d, "M{} {}", num(x), num(y)),
            Cmd::Line(x, y) => write!(d, "L{} {}", num(x), num(y)),
            Cmd::Curve(a, b, c, e, x, y) => write!(d, "C{} {} {} {} {} {}", num(a), num(b), num(c), num(e), num(x), num(y)),
            Cmd::Close => write!(d, "Z"),
        };
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_its_file() {
        let mut d = Diagram::default();
        let a = d.add_shape(Form::Rect, 0., 0., 120., 60.);
        let b = d.add_shape(Form::Ellipse, 300., 0., 100., 100.);
        d.shape_mut(a).unwrap().text = "Compte\n---\n+ solde: f32 <€>\n\\n & fin".into();
        d.shape_mut(b).unwrap().fill = true;
        let link = d.add_link(End::Shape(a), End::Shape(b), Head::Triangle, Route::Straight);
        d.link_mut(link).unwrap().text = "hérite".into();
        d.add_link(End::Point(5.5, -20.), End::Pin(a, 0.25, 0.), Head::Arrow, Route::Elbow);
        let svg = d.to_svg(COLORS[0]);
        assert!(svg.starts_with("<svg") && svg.contains("font-weight=\"bold\"") && svg.contains("&lt;€&gt;"));
        assert_eq!(Diagram::from_svg(&svg), Some(d.clone()));
        assert_eq!(Diagram::from_svg("<svg></svg>"), None);
        // Ids resume after the largest ones read back.
        let mut back = Diagram::from_svg(&svg).unwrap();
        assert_eq!(back.add_shape(Form::Note, 0., 0., 10., 10.), 5);

        // An attached stroke stops at the shapes' outline, facing each other.
        let (from, to) = d.ends(&d.links[0]).unwrap();
        assert!((119.9..=120.1).contains(&from.0) && (299.0..=301.).contains(&to.0));
        assert_eq!((d.shape_at(10., 10.), d.shape_at(200., 10.)), (Some(a), None));
        assert_eq!(d.link_at(200., 36., 6.), Some(link));

        // Removing a shape takes its arrows along; copying an arrow follows its shapes.
        let copies = d.duplicate(&[a, b, link], 20.);
        assert_eq!((copies.len(), d.links.len()), (3, 3));
        d.remove(&[a]);
        assert_eq!((d.shapes.len(), d.links.len()), (3, 1));
    }

    /// A diagram file around `source`, as another version of Bref would have written it.
    fn file(source: &str) -> String {
        format!("<svg>\n<metadata id=\"bref-diagram\">\n{}</metadata>\n</svg>\n", xml(source))
    }

    #[test]
    fn keeps_the_lines_it_cannot_read() {
        let shape = "shape 1 rect 0 0 100 60 0 0 0 A";
        // An unknown form, an unknown kind of line, a short line, a second `version`, and text
        // that XML escapes: none is dropped, and none is changed.
        let kept = [
            "shape 9 hexagon 5 5 10 10 0 0 0 <&> é 日本 😀",
            "note 4 &lt; &amp; ]]> \\\\ \u{3000}",
            "link 3 s1",
            "version 1",
            &format!("shape 7 oval {}", "x".repeat(10_000)),
        ];
        let source = format!("version 1\n{shape}\n{}\nlink 2 s1 s9 none arrow straight 0 0 x\n\n   \n{}\n", kept[0], kept[1..].join("\n"));
        let mut d = Diagram::from_svg(&file(&source)).unwrap();
        assert_eq!((d.shapes.len(), d.links.len()), (1, 1));
        assert_eq!(d.kept, kept);
        // A link to a shape it cannot read stays on disk without being drawn.
        assert!(d.trace(&d.links[0]).is_none());
        // Written back: the same bytes, and the same again after another round.
        let svg = d.to_svg(COLORS[0]);
        let source = Diagram::source_of(&svg).unwrap();
        assert!(kept.iter().all(|line| source.lines().any(|l| l == *line)));
        let back = Diagram::from_svg(&svg).unwrap();
        assert_eq!((back.to_svg(COLORS[0]), &back), (svg, &d));
        // No new element takes the id of one it cannot read.
        assert_eq!(d.add_shape(Form::Rect, 0., 0., 10., 10.), 10);
    }

    #[test]
    fn refuses_what_a_newer_version_wrote() {
        let shape = "shape 1 rect 0 0 100 60 0 0 0 A\n";
        // Higher, unreadable, not a number, beyond `u32`, with a stray space, empty: all newer.
        for version in [&(VERSION + 1).to_string(), "99999999999", "-1", "1.5", "x", " 1", "", "1 "] {
            let svg = file(&format!("version {version}\n{shape}"));
            assert!((Diagram::from_svg(&svg), Diagram::newer(&svg)) == (None, true), "version {version:?}");
        }
        let svg = file(&format!("{shape}version\n"));
        assert!((Diagram::from_svg(&svg), Diagram::newer(&svg)) == (None, true));
        // The version may come after the elements.
        assert!(Diagram::newer(&file(&format!("{shape}version {}\n", VERSION + 1))));
        // Version 0, 1, none, or an empty source: opens. Not a Bref file: not "newer", just foreign.
        for source in ["version 0\n", "version 1\n", "", "\n"] {
            let svg = file(&format!("{source}{shape}"));
            assert!(Diagram::from_svg(&svg).is_some_and(|d| d.shapes.len() == 1) && !Diagram::newer(&svg), "{source:?}");
        }
        assert_eq!(Diagram::from_svg(&file("")), Some(Diagram::default()));
        assert!(!Diagram::newer("<svg></svg>") && !Diagram::newer(""));
    }

    #[test]
    fn an_old_file_opens_as_it_was() {
        // Written by 0.3.8: no `version` line.
        let old = "shape 1 rect 0 0 120 60 0 0 0 Compte\\n---\\n+ solde\nshape 2 ellipse 300 0 100 100 1 1 0 \nlink 3 s1 s2@0.5,1 none triangle elbow 0 1 hérite\n";
        let d = Diagram::from_svg(&file(old)).unwrap();
        assert!(d.kept.is_empty() && (d.shapes.len(), d.links.len()) == (2, 1));
        // Saving adds the version line, and nothing else.
        let svg = d.to_svg(COLORS[0]);
        assert_eq!(Diagram::source_of(&svg).unwrap().trim_start(), format!("version 1\n{old}"));
        assert_eq!(Diagram::from_svg(&svg).unwrap().to_svg(COLORS[0]), svg);
    }

    #[test]
    fn styled_text_is_written_and_read_back() {
        let mut d = Diagram::blank();
        let a = d.add_shape(Form::Rect, 0., 0., 200., 80.);
        d.shape_mut(a).unwrap().text = "*Shape*\n---\n+ <u>count</u>: int\n+ **area**(): f32\n~~old~~ `List<T>`".into();
        let b = d.add_shape(Form::Rect, 300., 0., 100., 40.);
        d.shape_mut(b).unwrap().text = "*whole line*".into();
        let link = d.add_link(End::Shape(a), End::Shape(b), Head::Arrow, Route::Straight);
        d.link_mut(link).unwrap().text = "*calls*".into();
        let svg = d.to_svg(COLORS[0]);
        // A line with several styles has a run for each; a line with one has it on the text itself.
        assert!(svg.contains("<tspan font-style=\"italic\">Shape</tspan>") || svg.contains("font-style=\"italic\" fill"), "{svg}");
        assert!(svg.contains("<tspan text-decoration=\"underline\">count</tspan>") && svg.contains("<tspan font-weight=\"bold\">area</tspan>"));
        assert!(svg.contains("text-decoration=\"line-through\"") && svg.contains("font-family=\"monospace\">List&lt;T&gt;"));
        assert!(svg.contains("font-style=\"italic\" fill=\"#7d8590\" stroke=\"none\">whole line</text>"));
        // The marks are not drawn, but they are in the source.
        assert!(!svg.contains(">*Shape*<") && Diagram::source_of(&svg).unwrap().contains("*Shape*"));
        assert_eq!(Diagram::source_of(&svg).unwrap().trim_start().lines().take(2).collect::<Vec<_>>(), ["version 2", "rich 1"]);
        let back = Diagram::from_svg(&svg).unwrap();
        assert!(back.rich && back == d && back.to_svg(COLORS[0]) == svg);
    }

    #[test]
    fn a_diagram_from_before_styles_keeps_its_stars() {
        let mut d = Diagram::default();
        let a = d.add_shape(Form::Rect, 0., 0., 100., 40.);
        d.shape_mut(a).unwrap().text = "a * b * c *d* <u>e</u>".into();
        let svg = d.to_svg(COLORS[0]);
        assert!(svg.contains(">a * b * c *d* &lt;u&gt;e&lt;/u&gt;</text>") && !svg.contains("italic") && !svg.contains("tspan"));
        // Plain, it still opens in the builds that know only version 1.
        assert_eq!(Diagram::source_of(&svg).unwrap().trim_start().lines().next(), Some("version 1"));
        assert!(!Diagram::from_svg(&svg).unwrap().rich);
    }

    #[test]
    fn lays_out_text_and_borders() {
        let mut shape = Shape { id: 1, form: Form::Rect, x: 0., y: 0., w: 100., h: 80., text: "A\nB".into(), color: 0, fill: false, dashed: false };
        let (lines, rules) = shape.text_lines();
        assert!(rules.is_empty() && lines[0].centered && (lines[0].x, lines[0].y, lines[1].y) == (50., 22., 40.));
        shape.text = "Nom\n---\nchamp".into();
        let (lines, rules) = shape.text_lines();
        assert!(lines[0].bold && !lines[1].centered && lines[1].x == 8. && rules == [28.]);
        assert_eq!(shape.border((200., 40.)), (100., 40.));
        shape.form = Form::Diamond;
        assert_eq!(shape.border((50., -100.)), (50., 0.));
        assert_eq!(head(Head::Triangle, (10., 0.), (1., 0.)).0, [(10., 0.), (-2., 6.), (-2., -6.)]);
        assert_eq!(snap(14.), 10.);

        // Near an edge, an end sticks to the nearest quarter; in the middle, it stays free.
        shape.form = Form::Rect;
        assert_eq!((shape.hook(50., 40.), shape.hook(97., 22.), shape.hook(30., 2.)), (End::Shape(1), End::Pin(1, 1., 0.25), End::Pin(1, 0.25, 0.)));
        // Elbow: facing each other, a jog halfway; otherwise a single corner.
        assert_eq!(elbow((0., 0.), (1., 0.), (100., 50.), (-1., 0.)), [(0., 0.), (50., 0.), (50., 50.), (100., 50.)]);
        assert_eq!(elbow((0., 0.), (1., 0.), (100., 0.), (-1., 0.)), [(0., 0.), (100., 0.)]);
        assert_eq!(elbow((0., 0.), (1., 0.), (100., 50.), (0., -1.)), [(0., 0.), (100., 0.), (100., 50.)]);
        let mut d = Diagram::default();
        let (a, b) = (d.add_shape(Form::Rect, 0., 0., 100., 60.), d.add_shape(Form::Rect, 300., 200., 100., 60.));
        let link = d.add_link(End::Pin(a, 0.5, 1.), End::Shape(b), Head::Triangle, Route::Elbow);
        let trace = d.trace(d.link(link).unwrap()).unwrap();
        // It leaves through the bottom of the first shape and arrives from the left of the second.
        assert_eq!(trace.points, [(50., 60.), (50., 230.), (300., 230.)]);
        assert_eq!((trace.heads.len(), trace.middle), (1, (90., 230.)));
        d.link_mut(link).unwrap().route = Route::Curve;
        assert_eq!(d.trace(d.link(link).unwrap()).unwrap().points.len(), 17);
    }
}
