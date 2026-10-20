//! Diagrams: shapes, arrows between them, and their file. The file
//! is a plain SVG, readable anywhere, that also carries its source: that is what
//! the app rereads to continue the drawing. Pure functions, no UI dependency.

use std::fmt::Write;

use super::{
    richtext::{self, Span, Style},
    state, uml,
};

mod file;
mod hold;
mod route;

/// Stroke colors, as `0xrrggbb`: mid tones, readable on a light background
/// as on a dark one. The first is the neutral; in the app, the
/// theme replaces it with its text color.
pub const COLORS: [u32; 6] = [0x7d8590, 0xe5534b, 0xd98e2b, 0x3fa45b, 0x4a8fe0, 0xa371f7];
/// Step of the grid shapes snap to.
pub const GRID: f32 = 10.;
/// Text size and line height.
pub const FONT: f32 = 14.;
pub const LINE: f32 = 18.;
/// Distance between arrows that run side by side between the same two shapes.
const SPREAD: f32 = 18.;
/// Height of the header of a swimlane.
pub const LANE: f32 = 26.;
/// Text line that separates two compartments of a box (UML class, table).
pub const RULE: &str = "---";
/// Highest version of the file's source that this build reads. A file of a higher version
/// is never opened for editing: saving it would drop what this build does not know. A file is
/// written with the lowest version that can say what it holds (`Diagram::needed`), so a diagram
/// that uses nothing new still opens in the builds before.
///
/// 1: shapes and arrows. 2: `rich`, styled text. 3: `ends`, the texts at the ends of an arrow.
/// 4: the heads `cross`, `ball`, `socket`, and an arrow held by another's middle (`m<id>`).
/// 5: the forms `package`, `frame`, `lane` and the `in` lines: what a shape holds.
/// 6: the forms of state machines: `dot`, `final`, `terminate`, `bar`, `exit`, `composite`, `divider`.
pub const VERSION: u32 = 6;

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
    /// A small diamond, its text under it: an association of three or more ends, a decision.
    Branch,
    /// A package: a folder, its name in the tab. Holds shapes.
    Package,
    /// A frame, its title at the top left: a system boundary, a composite state. Holds shapes.
    Frame,
    /// A swimlane: a column under a header with its name. Holds shapes.
    Lane,
    /// A filled dot, text under it: an initial state, a junction.
    Dot,
    /// A circle with a dot in it: a final state.
    Final,
    /// A cross: the end of a state machine's life.
    Terminate,
    /// A thick bar: a fork or a join.
    Bar,
    /// A circle with a cross in it: an exit point.
    Exit,
    /// A composite state: a rounded frame, its name in a header. Holds shapes.
    Composite,
    /// A dashed line across a composite state: the border of its regions.
    Divider,
}

impl Form {
    pub const ALL: [(Form, &str); 19] = [
        (Form::Rect, "rect"),
        (Form::Round, "round"),
        (Form::Ellipse, "ellipse"),
        (Form::Diamond, "diamond"),
        (Form::Text, "text"),
        (Form::Cylinder, "cylinder"),
        (Form::Actor, "actor"),
        (Form::Note, "note"),
        (Form::Branch, "branch"),
        (Form::Package, "package"),
        (Form::Frame, "frame"),
        (Form::Lane, "lane"),
        (Form::Dot, "dot"),
        (Form::Final, "final"),
        (Form::Terminate, "terminate"),
        (Form::Bar, "bar"),
        (Form::Exit, "exit"),
        (Form::Composite, "composite"),
        (Form::Divider, "divider"),
    ];

    /// Whether shapes dropped on it belong to it.
    pub fn holds(self) -> bool {
        matches!(self, Form::Package | Form::Frame | Form::Lane | Form::Composite)
    }

    /// The version of the file that first had this form.
    pub fn since(self) -> u32 {
        match self {
            Form::Branch => 4,
            Form::Package | Form::Frame | Form::Lane => 5,
            Form::Dot | Form::Final | Form::Terminate | Form::Bar | Form::Exit | Form::Composite | Form::Divider => 6,
            _ => 1,
        }
    }
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
    /// A small cross on the stroke: an association end that cannot be navigated.
    Cross,
    /// A ball (`-o`): a provided interface.
    Ball,
    /// A socket (`-(`): a required interface.
    Socket,
}

impl Head {
    pub const ALL: [(Head, &str); 8] = [
        (Head::None, "none"),
        (Head::Arrow, "arrow"),
        (Head::Triangle, "triangle"),
        (Head::Diamond, "diamond"),
        (Head::DiamondFull, "fulldiamond"),
        (Head::Cross, "cross"),
        (Head::Ball, "ball"),
        (Head::Socket, "socket"),
    ];

    /// The version of the file that first had this head.
    pub fn since(self) -> u32 {
        match self {
            Head::Cross | Head::Ball | Head::Socket => 4,
            _ => 1,
        }
    }

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
    /// The shape that holds this one: it moves, is copied and removed with it. It comes first in
    /// `Diagram::shapes`, so it is drawn under.
    pub parent: Option<u32>,
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
    /// The middle of another arrow, which it follows: the dotted line of an association class.
    Mid(u32),
}

impl End {
    /// The shape this end is attached to.
    pub fn shape(self) -> Option<u32> {
        match self {
            End::Shape(id) | End::Pin(id, ..) => Some(id),
            End::Point(..) | End::Mid(_) => None,
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

/// Which side of its text a line of text is attached by.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Anchor {
    Start,
    Middle,
    End,
}

impl From<bool> for Anchor {
    /// `centered`, as a placed line of text says it.
    fn from(centered: bool) -> Anchor {
        if centered { Anchor::Middle } else { Anchor::Start }
    }
}

/// The three texts of an arrow: at its start, in its middle, at its end.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Slot {
    Start,
    Middle,
    End,
}

impl Slot {
    /// The next one, to go through them with one key.
    pub fn next(self) -> Slot {
        match self {
            Slot::Start => Slot::Middle,
            Slot::Middle => Slot::End,
            Slot::End => Slot::Start,
        }
    }
}

/// One end of an arrow: where its stroke meets what it links, and which way it goes from there.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Tip {
    pub at: (f32, f32),
    /// Unit direction of the stroke, going away from the end.
    pub out: (f32, f32),
    /// How far the head reaches along the stroke.
    pub head: f32,
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
    /// Its two ends: start, then end.
    pub tips: [Tip; 2],
}

#[derive(Clone, PartialEq, Debug)]
pub struct Link {
    pub id: u32,
    pub from: End,
    pub to: End,
    pub start: Head,
    pub end: Head,
    pub route: Route,
    /// The text in the middle of the stroke.
    pub text: String,
    /// The texts at its ends (a role, a multiplicity), and how far from the end each was dragged.
    pub start_text: String,
    pub end_text: String,
    pub start_along: f32,
    pub end_along: f32,
    pub color: u8,
    pub dashed: bool,
}

impl Link {
    /// An arrow with nothing set, for the file's reader to fill.
    pub(super) fn blank() -> Link {
        Link {
            id: 0,
            from: End::Point(0., 0.),
            to: End::Point(0., 0.),
            start: Head::None,
            end: Head::None,
            route: Route::Straight,
            text: String::new(),
            start_text: String::new(),
            end_text: String::new(),
            start_along: 0.,
            end_along: 0.,
            color: 0,
            dashed: false,
        }
    }

    pub fn text_of(&self, slot: Slot) -> &str {
        match slot {
            Slot::Start => &self.start_text,
            Slot::Middle => &self.text,
            Slot::End => &self.end_text,
        }
    }

    pub fn text_mut(&mut self, slot: Slot) -> &mut String {
        match slot {
            Slot::Start => &mut self.start_text,
            Slot::Middle => &mut self.text,
            Slot::End => &mut self.end_text,
        }
    }

    /// Whether the arrow has anything at its ends: it then needs a file of version 3.
    pub fn has_end_texts(&self) -> bool {
        !self.start_text.is_empty() || !self.end_text.is_empty() || self.start_along != 0. || self.end_along != 0.
    }
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
    /// Index of the line in the shape's text (the rules between compartments count).
    pub index: usize,
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
    /// The paths of the outline that are filled solid, in the stroke's color: a dot, a bar, the dot of a final state.
    pub fn solid(&self) -> &'static [usize] {
        match self.form {
            Form::Dot | Form::Bar => &[0],
            Form::Final => &[1],
            _ => &[],
        }
    }

    /// Width of the tab of a package: what its name takes.
    fn tab_width(&self) -> f32 {
        let name = self.text.lines().next().unwrap_or("");
        // Never wider than the package, whatever its size: it is a point while it is being drawn.
        let room = self.w.max(1.);
        // The name is bold: a bit wider than the guess for plain text.
        (Diagram::guess_width(name) * 1.08 + 16.).max(48f32.min(room)).min(room)
    }

    pub fn center(&self) -> (f32, f32) {
        (self.x + self.w / 2., self.y + self.h / 2.)
    }

    pub fn contains(&self, x: f32, y: f32) -> bool {
        (self.x..=self.x + self.w).contains(&x) && (self.y..=self.y + self.h).contains(&y)
    }

    /// The text splits the box into compartments.
    /// (An ellipse only in a diagram that reads styles: a use case with its extension points.)
    pub(super) fn boxed(&self, rich: bool) -> bool {
        let form = matches!(self.form, Form::Rect | Form::Round) || (rich && self.form == Form::Ellipse);
        form && self.text.lines().any(|l| l.trim() == RULE)
    }

    /// Where a separator at height `y` starts and ends: the whole width, or the chord of an ellipse.
    pub fn rule_span(&self, y: f32) -> (f32, f32) {
        if self.form != Form::Ellipse {
            return (self.x, self.x + self.w);
        }
        let (cx, cy) = self.center();
        let half = self.w / 2. * (1. - ((y - cy) / (self.h / 2.).max(1.)).powi(2)).max(0.).sqrt();
        (cx - half, cx + half)
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
            Form::Diamond | Form::Branch => vec![vec![
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
            Form::Package => {
                let (tab, th) = (self.tab_width(), 18f32.min(h));
                vec![vec![
                    Cmd::Move(x, y),
                    Cmd::Line(x + tab, y),
                    Cmd::Line(x + tab, y + th),
                    Cmd::Line(x + w, y + th),
                    Cmd::Line(x + w, y + h),
                    Cmd::Line(x, y + h),
                    Cmd::Close,
                ]]
            }
            Form::Frame => vec![rounded(x, y, w, h, 2.)],
            Form::Dot => vec![ellipse(cx, cy, w / 2., h / 2.)],
            Form::Final => vec![ellipse(cx, cy, w / 2., h / 2.), ellipse(cx, cy, (w / 2. - 4.).max(1.), (h / 2. - 4.).max(1.))],
            Form::Terminate => vec![vec![Cmd::Move(x, y), Cmd::Line(x + w, y + h), Cmd::Move(x + w, y), Cmd::Line(x, y + h)]],
            Form::Bar => vec![vec![Cmd::Move(x, y), Cmd::Line(x + w, y), Cmd::Line(x + w, y + h), Cmd::Line(x, y + h), Cmd::Close]],
            Form::Exit => {
                let (dx, dy) = (w * 0.3, h * 0.3);
                vec![ellipse(cx, cy, w / 2., h / 2.), vec![Cmd::Move(x + dx, y + dy), Cmd::Line(x + w - dx, y + h - dy), Cmd::Move(x + w - dx, y + dy), Cmd::Line(x + dx, y + h - dy)]]
            }
            Form::Composite => vec![rounded(x, y, w, h, 14.), vec![Cmd::Move(x, y + LANE), Cmd::Line(x + w, y + LANE)]],
            Form::Divider => vec![vec![Cmd::Move(x, cy), Cmd::Line(x + w, cy)]],
            Form::Lane => vec![rounded(x, y, w, h, 2.), vec![Cmd::Move(x, y + LANE), Cmd::Line(x + w, y + LANE)]],
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
    #[cfg(test)]
    pub fn text_lines(&self) -> (Vec<TextLine<'_>>, Vec<f32>) {
        self.text_lines_for(false)
    }

    /// `text_lines`, for a diagram that reads styles (`rich`) or not.
    pub fn text_lines_for(&self, rich: bool) -> (Vec<TextLine<'_>>, Vec<f32>) {
        let (cx, cy) = self.center();
        let lines: Vec<&str> = self.text.split('\n').collect();
        let mut rules = Vec::new();
        if self.boxed(rich) {
            // An ellipse (a use case, with its extension points) has them all centered, as a block in the middle.
            let round = self.form == Form::Ellipse;
            let rule_lines = lines.iter().filter(|l| l.trim() == RULE).count() as f32;
            let block = (lines.len() as f32 - rule_lines) * LINE + rule_lines * 8.;
            let (mut y, mut first) = (if round { cy - block / 2. } else { self.y + 6. }, true);
            let mut out = Vec::new();
            for (index, text) in lines.into_iter().enumerate() {
                if text.trim() == RULE {
                    rules.push(y + 4.);
                    y += 8.;
                    first = false;
                } else {
                    let (x, centered) = if first || round { (cx, true) } else { (self.x + 8., false) };
                    out.push(TextLine { x, y, text, centered, bold: first && !round, index });
                    y += LINE;
                }
            }
            return (out, rules);
        }
        // What holds shapes has its title at the top: in the tab of a package, in the header of a lane.
        if self.form.holds() {
            let mut out = Vec::new();
            for (index, text) in lines.into_iter().enumerate() {
                let (x, y, centered) = match (self.form, index) {
                    (Form::Package, 0) => (self.x + 6., self.y + 1., false),
                    (Form::Package, i) => (self.x + 8., self.y + 22. + LINE * (i - 1) as f32, false),
                    (Form::Lane | Form::Composite, 0) => (cx, self.y + (LANE - LINE) / 2., true),
                    (Form::Lane | Form::Composite, i) => (self.x + 8., self.y + LANE + 6. + LINE * (i - 1) as f32, false),
                    (_, i) => (self.x + 8., self.y + 6. + LINE * i as f32, false),
                };
                out.push(TextLine { x, y, text, centered, bold: index == 0, index });
            }
            return (out, rules);
        }
        let height = lines.len() as f32 * LINE;
        let (x, top, centered) = match self.form {
            Form::Text => (self.x, self.y, false),
            // Under the stick figure.
            Form::Actor | Form::Branch | Form::Dot | Form::Final | Form::Terminate | Form::Bar | Form::Exit => (cx, self.y + self.h + 4., true),
            _ => (cx, cy - height / 2., true),
        };
        let placed = lines.into_iter().enumerate();
        (placed.map(|(i, text)| TextLine { x, y: top + i as f32 * LINE, text, centered, bold: false, index: i }).collect(), rules)
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

    /// Where a line parallel to side `out` (a unit direction), `spread` away from the middle, meets the
    /// outline: the arrow's way out when it shares the side with others. Never past the side's end.
    pub fn border_beside(&self, out: (f32, f32), spread: f32) -> (f32, f32) {
        let (cx, cy) = self.center();
        let (a, b) = ((self.w / 2.).max(1.), (self.h / 2.).max(1.));
        // Along the exit's axis: `depth` is the half size there, `wide` the half size across.
        let (depth, wide) = if out.0 != 0. { (a, b) } else { (b, a) };
        let off = spread.clamp(-(wide - 6.).max(0.), (wide - 6.).max(0.));
        let reach = match self.form {
            Form::Ellipse | Form::Dot | Form::Final | Form::Terminate | Form::Exit => depth * (1. - (off / wide).powi(2)).max(0.).sqrt(),
            Form::Diamond | Form::Branch => depth * (1. - off.abs() / wide).max(0.),
            _ => depth,
        };
        if out.0 != 0. { (cx + out.0 * reach, cy + off) } else { (cx + off, cy + out.1 * reach) }
    }

    /// Point of the outline on the ray from the center toward `toward`.
    pub fn border(&self, toward: (f32, f32)) -> (f32, f32) {
        let (cx, cy) = self.center();
        let (dx, dy) = (toward.0 - cx, toward.1 - cy);
        let (a, b) = ((self.w / 2.).max(1.), (self.h / 2.).max(1.));
        let reach = match self.form {
            Form::Ellipse | Form::Dot | Form::Final | Form::Terminate | Form::Exit => ((dx / a).powi(2) + (dy / b).powi(2)).sqrt(),
            Form::Diamond | Form::Branch => dx.abs() / a + dy.abs() / b,
            _ => (dx.abs() / a).max(dy.abs() / b),
        };
        if reach < 1e-6 { (cx, cy) } else { (cx + dx / reach, cy + dy / reach) }
    }
}

/// How far a head reaches along the stroke.
fn head_reach(head: Head) -> f32 {
    match head {
        Head::None => 0.,
        Head::Arrow => 10.,
        Head::Triangle => 12.,
        Head::Diamond | Head::DiamondFull => 16.,
        Head::Cross => 0.,
        Head::Ball => 12.,
        Head::Socket => 8.,
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
        Head::Cross | Head::Ball | Head::Socket => (Vec::new(), false, false, 0.),
    }
}

/// Path of a head set at `tip`, for a stroke arriving in the unit direction `u`: the path, whether
/// it is filled, and by how much to shorten the stroke so it stops at its base.
pub fn head_path(kind: Head, tip: (f32, f32), u: (f32, f32)) -> (Vec<Cmd>, bool, f32) {
    let n = (-u.1, u.0);
    let at = |back: f32, side: f32| (tip.0 - u.0 * back + n.0 * side, tip.1 - u.1 * back + n.1 * side);
    // An arc of a circle of radius `r` around `c`, from angle `a0` to `a1` (0 = along the stroke, backwards).
    let arc = |c: (f32, f32), r: f32, a0: f32, a1: f32, steps: usize| -> Vec<(f32, f32)> {
        (0..=steps)
            .map(|i| {
                let a = a0 + (a1 - a0) * i as f32 / steps as f32;
                (c.0 - u.0 * r * a.cos() + n.0 * r * a.sin(), c.1 - u.1 * r * a.cos() + n.1 * r * a.sin())
            })
            .collect()
    };
    let path = |points: &[(f32, f32)], close: bool| -> Vec<Cmd> {
        let mut path: Vec<Cmd> = points.iter().enumerate().map(|(i, p)| if i == 0 { Cmd::Move(p.0, p.1) } else { Cmd::Line(p.0, p.1) }).collect();
        if close {
            path.push(Cmd::Close);
        }
        path
    };
    match kind {
        Head::Cross => {
            let (a, b, c, d) = (at(3., 5.), at(13., -5.), at(3., -5.), at(13., 5.));
            (vec![Cmd::Move(a.0, a.1), Cmd::Line(b.0, b.1), Cmd::Move(c.0, c.1), Cmd::Line(d.0, d.1)], false, 0.)
        }
        // A circle touching the tip, the stroke stopping at its edge.
        Head::Ball => (path(&arc(at(6., 0.), 6., 0., std::f32::consts::TAU, 24), true), false, 12.),
        // A half circle around the tip, its back toward the stroke's start: the stroke stops at its apex.
        Head::Socket => (path(&arc(tip, 8., -std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_2, 12), false), false, 8.),
        other => {
            let (points, closed, full, shorten) = head(other, tip, u);
            (path(&points, closed), full, shorten)
        }
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

    /// Where each line of the text of `shape` is written, and its separators, as this diagram reads its texts.
    pub fn lines<'a>(&self, shape: &'a Shape) -> (Vec<TextLine<'a>>, Vec<f32>) {
        shape.text_lines_for(self.rich)
    }

    /// The runs of a line of the text in the middle of an arrow: a transition's `[guard]` is in italics.
    pub fn label_runs(&self, text: &str) -> Vec<Span> {
        if self.rich { state::runs(text) } else { richtext::plain(text, false) }
    }

    /// The runs of each line `shape.text_lines()` places, in the same order: styled as the diagram
    /// reads its texts, a class box by the notation of UML, or as typed (`plain`: while it is written).
    pub fn runs(&self, shape: &Shape, plain: bool) -> Vec<Vec<Span>> {
        let (lines, _) = self.lines(shape);
        if plain || !self.rich {
            return lines.iter().map(|l| richtext::plain(l.text, l.bold)).collect();
        }
        let class = (shape.boxed(true) && shape.form != Form::Ellipse).then(|| uml::class_runs(&shape.text, RULE)).flatten();
        match class {
            Some(mut by_line) => lines.iter().map(|l| std::mem::take(&mut by_line[l.index])).collect(),
            None => lines.iter().map(|l| richtext::spans(l.text, l.bold)).collect(),
        }
    }

    /// The lowest version of the source that can say all this diagram holds.
    pub(super) fn needed(&self) -> u32 {
        let links = self.links.iter().map(|l| {
            let mid = matches!(l.from, End::Mid(_)) || matches!(l.to, End::Mid(_));
            [l.start.since(), l.end.since(), if mid { 4 } else { 1 }, if l.has_end_texts() { 3 } else { 1 }].into_iter().max().unwrap_or(1)
        });
        let shapes = self.shapes.iter().map(|s| if s.parent.is_some() { 5 } else { s.form.since() }).max().unwrap_or(1);
        links.max().unwrap_or(1).max(shapes).max(if self.rich { 2 } else { 1 })
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
        self.shapes.push(Shape { id, form, x, y, w, h, text: String::new(), color: 0, fill: false, dashed: false, parent: None });
        id
    }

    pub fn add_link(&mut self, from: End, to: End, end: Head, route: Route) -> u32 {
        let id = self.fresh();
        self.links.push(Link { id, from, to, end, route, ..Link::blank() });
        id
    }

    /// Copy of elements `ids`, offset; returns the ids of the copies.
    /// An arrow is only copied along with the shapes it links.
    pub fn duplicate(&mut self, ids: &[u32], by: f32) -> Vec<u32> {
        // What a container holds comes with it, and so do the arrows between those shapes.
        let mut ids = ids.to_vec();
        let mut family: Vec<u32> = Vec::new();
        for id in ids.clone() {
            if self.shape(id).is_some_and(|s| s.form.holds()) {
                family.push(id);
                family.extend(self.descendants(id));
            }
        }
        for id in &family {
            if !ids.contains(id) {
                ids.push(*id);
            }
        }
        let inside = |end: End| end.shape().is_some_and(|id| family.contains(&id));
        let between: Vec<u32> = self.links.iter().filter(|l| inside(l.from) && inside(l.to) && !ids.contains(&l.id)).map(|l| l.id).collect();
        ids.extend(between);
        let before = self.shapes.clone();
        let mut twin = Vec::new();
        for shape in before.iter().filter(|s| ids.contains(&s.id)) {
            let id = self.fresh();
            twin.push((shape.id, id));
            self.shapes.push(Shape { id, x: shape.x + by, y: shape.y + by, ..shape.clone() });
        }
        // A copy is held by the copy of its holder, or else by the holder of the original.
        for (old, new) in &twin {
            let holder = before.iter().find(|s| s.id == *old).and_then(|s| s.parent);
            let copy = holder.and_then(|p| twin.iter().find(|(o, _)| *o == p).map(|(_, n)| *n));
            if let Some(shape) = self.shape_mut(*new) {
                shape.parent = copy.or(holder);
            }
        }
        let mut copies: Vec<(u32, u32)> = Vec::new();
        let mut made: Vec<u32> = twin.iter().map(|(_, new)| *new).collect();
        // An arrow that holds on to another's middle is copied after it, to find its copy.
        let mut wanted: Vec<Link> = self.links.clone().into_iter().filter(|l| ids.contains(&l.id)).collect();
        wanted.sort_by_key(|l| matches!(l.from, End::Mid(_)) || matches!(l.to, End::Mid(_)));
        for link in wanted {
            let moved = |end: End| match end {
                End::Shape(id) => twin.iter().find(|(old, _)| *old == id).map(|(_, new)| End::Shape(*new)),
                End::Pin(id, fx, fy) => twin.iter().find(|(old, _)| *old == id).map(|(_, new)| End::Pin(*new, fx, fy)),
                End::Point(x, y) => Some(End::Point(x + by, y + by)),
                End::Mid(id) => copies.iter().find(|(old, _)| *old == id).map(|(_, new)| End::Mid(*new)),
            };
            if let (Some(from), Some(to)) = (moved(link.from), moved(link.to)) {
                let id = self.fresh();
                made.push(id);
                copies.push((link.id, id));
                self.links.push(Link { id, from, to, ..link });
            }
        }
        made
    }

    /// Removes elements `ids`, the shapes they hold, the arrows attached to removed shapes, and those attached to removed arrows.
    pub fn remove(&mut self, ids: &[u32]) {
        let mut gone = ids.to_vec();
        for id in ids {
            gone.extend(self.descendants(*id));
        }
        self.shapes.retain(|s| !gone.contains(&s.id));
        loop {
            let held = |end: End, gone: &[u32]| match end {
                End::Mid(id) => gone.contains(&id),
                other => other.shape().is_some_and(|id| gone.contains(&id)),
            };
            let dropped: Vec<u32> = self.links.iter().filter(|l| gone.contains(&l.id) || held(l.from, &gone) || held(l.to, &gone)).map(|l| l.id).collect();
            if dropped.is_empty() {
                break;
            }
            self.links.retain(|l| !dropped.contains(&l.id));
            gone.extend(dropped);
        }
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

    /// The middle of arrow `id`, where an association class holds on. Not of an arrow that itself
    /// holds on to another's middle: the middle of a middle would never end.
    pub fn mid(&self, id: u32) -> Option<(f32, f32)> {
        let link = self.link(id)?;
        if matches!(link.from, End::Mid(_)) || matches!(link.to, End::Mid(_)) {
            return None;
        }
        Some(self.trace(link)?.middle)
    }

    /// The arrow whose middle is within `reach` of this point, other than `except`.
    pub fn mid_at(&self, x: f32, y: f32, reach: f32, except: Option<u32>) -> Option<u32> {
        let near = |id: u32| self.mid(id).is_some_and(|m| (m.0 - x).hypot(m.1 - y) <= reach);
        self.links.iter().rev().map(|l| l.id).find(|id| Some(*id) != except && near(*id))
    }

    /// Where an end leaves what it holds, and in which direction: toward
    /// `toward` for a straight stroke, otherwise through the side facing it.
    fn leave(&self, end: End, toward: (f32, f32), straight: bool, spread: f32) -> Option<((f32, f32), (f32, f32))> {
        let side = |dx: f32, dy: f32, w: f32, h: f32| if dx.abs() * h >= dy.abs() * w { (dx.signum(), 0.) } else { (0., dy.signum()) };
        Some(match end {
            End::Point(x, y) => ((x, y), side(toward.0 - x, toward.1 - y, 1., 1.)),
            End::Mid(id) => {
                let (x, y) = self.mid(id)?;
                ((x, y), side(toward.0 - x, toward.1 - y, 1., 1.))
            }
            End::Shape(id) => {
                let shape = self.shape(id)?;
                let (cx, cy) = shape.center();
                let out = side(toward.0 - cx, toward.1 - cy, shape.w, shape.h);
                // One of several arrows between the same two shapes: it leaves beside the others.
                if spread != 0. {
                    return Some((shape.border_beside(out, spread), out));
                }
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

    /// How far to one side an arrow leaves its shapes, when it is one of several between the same two
    /// (joined shape to shape, not by a pin): the first goes to one side of the middle, the last to the
    /// other, by order of making, so that they run side by side instead of stacked.
    fn spread(&self, link: &Link) -> f32 {
        let (End::Shape(a), End::Shape(b)) = (link.from, link.to) else { return 0. };
        let pair = |l: &Link| match (l.from, l.to) {
            (End::Shape(x), End::Shape(y)) if (x, y) == (a, b) || (x, y) == (b, a) => Some(x),
            _ => None,
        };
        let twins: Vec<u32> = self.links.iter().filter(|l| pair(l).is_some()).map(|l| l.id).collect();
        match (twins.len(), twins.iter().position(|id| *id == link.id)) {
            (n, Some(i)) if n > 1 && a != b => (i as f32 - (n - 1) as f32 / 2.) * SPREAD,
            _ => 0.,
        }
    }

    /// The way of an arrow from a shape to itself (a transition that stays in its state): a loop out of
    /// its right side and back into its top, wider for each further one on the same shape.
    fn self_loop(&self, link: &Link) -> Option<Vec<(f32, f32)>> {
        let (End::Shape(id), End::Shape(back)) = (link.from, link.to) else { return None };
        if id != back {
            return None;
        }
        let shape = self.shape(id)?;
        let mine: Vec<u32> = self.links.iter().filter(|l| l.from == End::Shape(id) && l.to == End::Shape(id)).map(|l| l.id).collect();
        let k = mine.iter().position(|l| *l == link.id)? as f32;
        let reach = 26. + 14. * k;
        // Down the right side, a little lower for each; in at the top, a little further left.
        let start = shape.border_beside((1., 0.), -shape.h * (0.2 - 0.12 * k.min(3.)).max(-0.2));
        let end = shape.border_beside((0., -1.), shape.w * (0.2 - 0.12 * k.min(3.)));
        Some(vec![start, (start.0 + reach, start.1), (start.0 + reach, end.1 - reach), (end.0, end.1 - reach), end])
    }

    /// The boxes a right-angled arrow goes around: those of the shapes that hold nothing (a frame is
    /// crossed, what is in it is not), and apart, the boxes of the two shapes it links.
    fn obstacles(&self, link: &Link) -> (Vec<route::Rect>, Vec<route::Rect>) {
        let own = [link.from.shape(), link.to.shape()];
        let boxed = |s: &Shape| (s.x, s.y, s.x + s.w, s.y + s.h);
        let others = self.shapes.iter().filter(|s| !s.form.holds() && !own.contains(&Some(s.id))).map(boxed).collect();
        let mine = self.shapes.iter().filter(|s| own.contains(&Some(s.id)) && !s.form.holds()).map(boxed).collect();
        (others, mine)
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
            End::Mid(id) => self.mid(id),
        };
        let straight = link.route == Route::Straight;
        let spread = self.spread(link);
        let looped = self.self_loop(link);
        let (a, da, b, db);
        if let Some(path) = &looped {
            (a, da, b, db) = (path[0], (1., 0.), path[path.len() - 1], (0., -1.));
        } else {
            (a, da) = self.leave(link.from, aim(link.to)?, straight, spread)?;
            (b, db) = self.leave(link.to, aim(link.from)?, straight, spread)?;
        }
        let reach = ((b.0 - a.0).hypot(b.1 - a.1) * 0.4).max(30.);
        let (c1, c2) = ((a.0 + da.0 * reach, a.1 + da.1 * reach), (b.0 + db.0 * reach, b.1 + db.1 * reach));
        // A loop is right-angled, whatever the route asked for.
        let route = if looped.is_some() { Route::Elbow } else { link.route };
        let mut points = match route {
            _ if looped.is_some() => looped.clone().unwrap_or_default(),
            Route::Straight => vec![a, b],
            Route::Elbow => {
                let (boxes, own) = self.obstacles(link);
                route::route(a, da, b, db, &boxes, &own)
            }
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
        let (start, end) = (head_path(link.start, a, ua), head_path(link.end, b, ub));
        // The stroke stops at the base of a closed head.
        let (a2, b2) = ((a.0 - ua.0 * start.2, a.1 - ua.1 * start.2), (b.0 - ub.0 * end.2, b.1 - ub.1 * end.2));
        let line = match route {
            Route::Curve => vec![Cmd::Move(a2.0, a2.1), Cmd::Curve(c1.0, c1.1, c2.0, c2.1, b2.0, b2.1)],
            _ => {
                let mut drawn = points.clone();
                (drawn[0], drawn[last]) = (a2, b2);
                drawn.iter().enumerate().map(|(i, p)| if i == 0 { Cmd::Move(p.0, p.1) } else { Cmd::Line(p.0, p.1) }).collect()
            }
        };
        let heads = [start, end].into_iter().filter(|(path, ..)| !path.is_empty()).map(|(path, full, _)| (path, full));
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
        let tips = [
            Tip { at: a, out: (-ua.0, -ua.1), head: head_reach(link.start) },
            Tip { at: b, out: (-ub.0, -ub.1), head: head_reach(link.end) },
        ];
        points.dedup();
        Some(Trace { line, heads: heads.collect(), middle, upright, points, tips })
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

    /// Where line `i` of the `count` lines of the text at an end of an arrow is written, `along`
    /// further down its stroke: beyond the head, above a lying stroke, right of a standing one,
    /// so that it covers neither the shape, nor the head, nor the stroke.
    pub fn tip_label_at(tip: &Tip, along: f32, i: usize, count: usize) -> (f32, f32, Anchor) {
        let gap = tip.head + 8. + along;
        let (x, y) = (tip.at.0 + tip.out.0 * gap, tip.at.1 + tip.out.1 * gap);
        let height = LINE * count as f32;
        if tip.out.0.abs() >= tip.out.1.abs() {
            let anchor = if tip.out.0 >= 0. { Anchor::Start } else { Anchor::End };
            (x, y - height - 2. + LINE * i as f32, anchor)
        } else {
            let top = if tip.out.1 >= 0. { y } else { y - height };
            (x + 8., top + LINE * i as f32, Anchor::Start)
        }
    }

    /// Width a text takes, as a guess for the places that cannot measure it (the size of a tab, of
    /// the file's drawing, finding a text under the pointer): the longest line, each letter by its
    /// usual share of an em in the app's font, CJK and emoji wide.
    pub fn guess_width(text: &str) -> f32 {
        let em = |c: char| match c {
            'i' | 'l' | 'j' | '!' | '|' | '.' | ',' | ':' | ';' | '\'' | '`' | ' ' => 0.27,
            'I' | 't' | 'f' | 'r' | '(' | ')' | '[' | ']' | '/' | '-' => 0.38,
            'W' | 'M' | '@' => 0.95,
            'm' | 'w' | 'O' | 'Q' | 'G' | 'D' | 'H' | 'N' | 'U' | 'C' | 'R' | 'A' | 'B' | 'K' | 'V' | 'X' | 'Y' | 'Z' => 0.72,
            c if c.is_ascii_uppercase() => 0.64,
            c if c.is_ascii_digit() => 0.6,
            c if c.is_ascii() => 0.56,
            c if c as u32 >= 0x1F000 => 1.2,
            c if c as u32 >= 0x2E80 => 1.,
            _ => 0.58,
        };
        text.split('\n').map(|line| line.chars().map(|c| em(c) * FONT).sum::<f32>()).fold(0., f32::max)
    }

    /// Rectangle (left, top, right, bottom) of the text at an end of an arrow.
    fn tip_label_box(&self, link: &Link, slot: Slot, trace: &Trace) -> Option<(f32, f32, f32, f32)> {
        let (text, along, tip) = match slot {
            Slot::Start => (&link.start_text, link.start_along, trace.tips[0]),
            Slot::End => (&link.end_text, link.end_along, trace.tips[1]),
            Slot::Middle => return None,
        };
        if text.is_empty() {
            return None;
        }
        let count = text.split('\n').count();
        let (x, y, anchor) = Diagram::tip_label_at(&tip, along, 0, count);
        let w = Diagram::guess_width(text);
        let left = match anchor {
            Anchor::Start => x,
            Anchor::Middle => x - w / 2.,
            Anchor::End => x - w,
        };
        Some((left, y, left + w, y + LINE * count as f32))
    }

    /// The arrow end text under this point.
    pub fn tip_label_hit(&self, x: f32, y: f32) -> Option<(u32, Slot)> {
        self.links.iter().rev().find_map(|link| {
            let trace = self.trace(link)?;
            [Slot::Start, Slot::End].into_iter().find_map(|slot| {
                let (x0, y0, x1, y1) = self.tip_label_box(link, slot, &trace)?;
                ((x0 - 4.)..=(x1 + 4.)).contains(&x).then_some(())?;
                ((y0 - 2.)..=(y1 + 2.)).contains(&y).then_some((link.id, slot))
            })
        })
    }

    /// How far down the stroke from its end a text dragged to this point goes.
    pub fn along_to(&self, link: &Link, slot: Slot, x: f32, y: f32) -> f32 {
        let Some(trace) = self.trace(link) else { return 0. };
        let tip = if slot == Slot::Start { trace.tips[0] } else { trace.tips[1] };
        let stroke: f32 = trace.points.windows(2).map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1)).sum();
        let dragged = (x - tip.at.0) * tip.out.0 + (y - tip.at.1) * tip.out.1;
        (dragged - tip.head - 8.).clamp(0., (stroke - tip.head - 16.).max(0.))
    }

    /// Rectangle containing the whole diagram: (left, top, right, bottom).
    pub fn bounds(&self) -> Option<(f32, f32, f32, f32)> {
        // A stick figure's text is written under it.
        let boxes = self.shapes.iter().map(|s| {
            let below = if matches!(s.form, Form::Actor | Form::Branch | Form::Dot | Form::Final | Form::Terminate | Form::Bar | Form::Exit) { 4. + LINE * s.text.split('\n').count() as f32 } else { 0. };
            (s.x, s.y, s.x + s.w, s.y + s.h + below)
        });
        let ends = self.links.iter().filter_map(|l| self.trace(l)).flat_map(|t| t.points);
        let texts = self.links.iter().filter_map(|l| Some((l, self.trace(l)?))).flat_map(|(l, t)| [Slot::Start, Slot::End].map(|slot| self.tip_label_box(l, slot, &t)).into_iter().flatten().collect::<Vec<_>>());
        boxes.chain(texts).chain(ends.map(|(x, y)| (x, y, x, y))).reduce(|a, b| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)))
    }
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

    #[test]
    fn lays_out_text_and_borders() {
        let mut shape = Shape { id: 1, form: Form::Rect, x: 0., y: 0., w: 100., h: 80., text: "A\nB".into(), color: 0, fill: false, dashed: false, parent: None };
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
        assert_eq!(route::elbow((0., 0.), (1., 0.), (100., 50.), (-1., 0.)), [(0., 0.), (50., 0.), (50., 50.), (100., 50.)]);
        assert_eq!(route::elbow((0., 0.), (1., 0.), (100., 0.), (-1., 0.)), [(0., 0.), (100., 0.)]);
        assert_eq!(route::elbow((0., 0.), (1., 0.), (100., 50.), (0., -1.)), [(0., 0.), (100., 0.), (100., 50.)]);
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

    #[test]
    fn places_the_texts_at_the_ends_of_an_arrow() {
        let link = |from: (f32, f32), to: (f32, f32)| {
            let mut d = Diagram::default();
            let a = d.add_shape(Form::Rect, from.0, from.1, 100., 60.);
            let b = d.add_shape(Form::Rect, to.0, to.1, 100., 60.);
            let id = d.add_link(End::Shape(a), End::Shape(b), Head::Arrow, Route::Straight);
            let trace = d.trace(d.link(id).unwrap()).unwrap();
            (d, trace)
        };
        let at = |trace: &Trace, i: usize| Diagram::tip_label_at(&trace.tips[i], 0., 0, 1);
        // Leaving by the right, arriving by the left: beyond the head, above the stroke.
        let (_, t) = link((0., 0.), (300., 0.));
        assert_eq!((at(&t, 0), at(&t, 1)), ((108., 10., Anchor::Start), (282., 10., Anchor::End)));
        // The other way round.
        let (_, t) = link((300., 0.), (0., 0.));
        assert_eq!((at(&t, 0), at(&t, 1)), ((292., 10., Anchor::End), (118., 10., Anchor::Start)));
        // Leaving by the bottom, arriving by the top: right of the stroke, beyond the head.
        let (_, t) = link((0., 0.), (0., 200.));
        assert_eq!((at(&t, 0), at(&t, 1)), ((58., 68., Anchor::Start), (58., 164., Anchor::Start)));
        let (_, t) = link((0., 200.), (0., 0.));
        assert_eq!((at(&t, 0), at(&t, 1)), ((58., 174., Anchor::Start), (58., 78., Anchor::Start)));
        // Several lines stack away from the end; dragging moves the text along the stroke.
        let lines: Vec<f32> = (0..3).map(|i| Diagram::tip_label_at(&t.tips[1], 0., i, 3).1).collect();
        assert_eq!(lines, [78., 96., 114.]);
        assert_eq!(Diagram::tip_label_at(&t.tips[1], 40., 0, 1).1, 118.);
    }

    #[test]
    fn end_texts_never_cover_the_shape_or_the_head() {
        // On the four sides, for every head and a text of one, three lines, and dragged away.
        for (from, to) in [((0., 0.), (300., 0.)), ((300., 0.), (0., 0.)), ((0., 0.), (0., 200.)), ((0., 200.), (0., 0.)), ((0., 0.), (300., 200.))] {
            for head in [Head::None, Head::Arrow, Head::Triangle, Head::Diamond, Head::DiamondFull] {
                let mut d = Diagram::default();
                let a = d.add_shape(Form::Rect, from.0, from.1, 100., 60.);
                let b = d.add_shape(Form::Rect, to.0, to.1, 100., 60.);
                let id = d.add_link(End::Shape(a), End::Shape(b), head, Route::Straight);
                let link = d.link_mut(id).unwrap();
                link.start = head;
                (link.start_text, link.end_text) = ("0..*".into(), "role\nname\n+items".into());
                for along in [0., 25.] {
                    d.link_mut(id).unwrap().start_along = along;
                    d.link_mut(id).unwrap().end_along = along;
                    let link = d.link(id).unwrap().clone();
                    let trace = d.trace(&link).unwrap();
                    for (slot, shape) in [(Slot::Start, a), (Slot::End, b)] {
                        let (x0, y0, x1, y1) = d.tip_label_box(&link, slot, &trace).unwrap();
                        let s = d.shape(shape).unwrap();
                        let apart = x1 <= s.x || x0 >= s.x + s.w || y1 <= s.y || y0 >= s.y + s.h;
                        assert!(apart, "{slot:?} text ({x0},{y0},{x1},{y1}) covers {:?} for {from:?} -> {to:?}, {head:?}", (s.x, s.y));
                        // Nor the head: a head is within its reach of the end, along the stroke.
                        let tip = trace.tips[if slot == Slot::Start { 0 } else { 1 }];
                        let corners = [(x0, y0), (x1, y0), (x0, y1), (x1, y1)];
                        let into = |c: (f32, f32)| (c.0 - tip.at.0) * tip.out.0 + (c.1 - tip.at.1) * tip.out.1;
                        let across = |c: (f32, f32)| ((c.0 - tip.at.0) * -tip.out.1 + (c.1 - tip.at.1) * tip.out.0).abs();
                        assert!(corners.iter().all(|c| into(*c) >= tip.head - 0.01 || across(*c) > 8.), "{slot:?} text touches its head");
                    }
                }
            }
        }
        // The text is found under the pointer, and dragging it goes along the stroke, never past it.
        let mut d = Diagram::default();
        let a = d.add_shape(Form::Rect, 0., 0., 100., 60.);
        let b = d.add_shape(Form::Rect, 300., 0., 100., 60.);
        let id = d.add_link(End::Shape(a), End::Shape(b), Head::Arrow, Route::Straight);
        d.link_mut(id).unwrap().end_text = "0..*".into();
        let box_ = d.tip_label_box(d.link(id).unwrap(), Slot::End, &d.trace(d.link(id).unwrap()).unwrap()).unwrap();
        assert_eq!(d.tip_label_hit((box_.0 + box_.2) / 2., (box_.1 + box_.3) / 2.), Some((id, Slot::End)));
        assert_eq!((d.tip_label_hit(200., 100.), d.tip_label_hit(150., 30.)), (None, None));
        let link = d.link(id).unwrap();
        assert_eq!((d.along_to(link, Slot::End, 290., 0.), d.along_to(link, Slot::End, 240., 0.), d.along_to(link, Slot::End, -500., 0.)), (0., 42., 174.));
    }

    #[test]
    fn an_arrow_can_hold_on_to_the_middle_of_another() {
        let mut d = Diagram::default();
        let a = d.add_shape(Form::Rect, 0., 0., 100., 60.);
        let b = d.add_shape(Form::Rect, 300., 0., 100., 60.);
        let c = d.add_shape(Form::Rect, 150., 200., 100., 60.);
        let assoc = d.add_link(End::Shape(a), End::Shape(b), Head::None, Route::Straight);
        let class = d.add_link(End::Shape(c), End::Mid(assoc), Head::None, Route::Straight);
        // The dotted line ends in the middle of the association, and follows it.
        assert_eq!(d.mid(assoc), Some((200., 30.)));
        assert_eq!(d.ends(d.link(class).unwrap()).unwrap().1, (200., 30.));
        d.shape_mut(b).unwrap().x = 500.;
        assert_eq!(d.ends(d.link(class).unwrap()).unwrap().1, (300., 30.));
        // Found by the pointer near the middle, but not for the arrow itself.
        assert_eq!((d.mid_at(305., 35., 16., None), d.mid_at(305., 35., 16., Some(assoc)), d.mid_at(200., 100., 16., None)), (Some(assoc), None, None));
        // An arrow held by a middle has none to offer: no middle of a middle.
        let again = d.add_link(End::Shape(c), End::Mid(class), Head::None, Route::Straight);
        assert!(d.mid(class).is_none() && d.trace(d.link(again).unwrap()).is_none());
        d.remove(&[again]);
        // Copying the association with its class copies the hold; copying the class alone does not.
        let made = d.duplicate(&[a, b, c, assoc, class], 20.);
        assert_eq!(made.len(), 5);
        let copy = d.links.iter().find(|l| l.id == made[4]).unwrap();
        assert_eq!(copy.to, End::Mid(made[3]));
        let before = d.links.len();
        assert_eq!(d.duplicate(&[class], 20.).len(), 0);
        assert_eq!(d.links.len(), before);
        // Removing the association takes the class along; removing a shape takes the association, then the class.
        d.remove(&[a]);
        assert!(d.link(assoc).is_none() && d.link(class).is_none() && d.link(made[3]).is_some());
    }

    #[test]
    fn the_new_heads_stop_the_stroke_where_they_begin() {
        let (u, tip) = ((1., 0.), (100., 0.));
        let ends = |head| {
            let (path, full, shorten) = head_path(head, tip, u);
            let xs: Vec<f32> = path.iter().filter_map(|c| match c { Cmd::Move(x, _) | Cmd::Line(x, _) => Some(*x), _ => None }).collect();
            (xs.iter().copied().fold(f32::MAX, f32::min), xs.iter().copied().fold(f32::MIN, f32::max), full, shorten)
        };
        // A ball touches the tip with its edge; the stroke stops 12 before, where the ball's far edge is.
        assert_eq!(ends(Head::Ball), (88., 100., false, 12.));
        // A socket spans 8 on each side of the tip, apex back; the stroke stops at the apex.
        let (lo, hi, _, shorten) = ends(Head::Socket);
        assert_eq!((lo.round(), hi.round(), shorten), (92., 100., 8.));
        // A cross sits on the stroke, 8 before the tip, and does not shorten it.
        let (lo, hi, _, shorten) = ends(Head::Cross);
        assert_eq!((lo, hi, shorten), (87., 97., 0.));
        // Pointing any way, the head keeps its size.
        for u in [(0., 1.), (-1., 0.), (0., -1.)] {
            let (path, ..) = head_path(Head::Ball, (50., 50.), u);
            let (xs, ys): (Vec<f32>, Vec<f32>) = path.iter().filter_map(|c| match c { Cmd::Move(x, y) | Cmd::Line(x, y) => Some((*x, *y)), _ => None }).unzip();
            let (w, h) = (xs.iter().copied().fold(f32::MIN, f32::max) - xs.iter().copied().fold(f32::MAX, f32::min), ys.iter().copied().fold(f32::MIN, f32::max) - ys.iter().copied().fold(f32::MAX, f32::min));
            assert!((w - 12.).abs() < 0.1 || (h - 12.).abs() < 0.1, "{u:?}: {w}x{h}");
        }
    }

    #[test]
    fn containers_of_any_size_have_an_outline_and_a_title() {
        // From a point, while it is drawn, to a very large one; with no text, a one-letter name or a very long one.
        for (w, h) in [(0., 0.), (1., 1.), (10., 10.), (47., 17.), (48., 18.), (100., 100.), (10_000., 10_000.)] {
            for form in [Form::Package, Form::Frame, Form::Lane] {
                for text in ["", "a", "billing\npkg::billing\nthird", &"x".repeat(500), "日本語 😀"] {
                    let shape = Shape { id: 1, form, x: 5., y: 5., w, h, text: text.into(), color: 0, fill: false, dashed: false, parent: None };
                    let (lines, rules) = shape.text_lines();
                    assert!(rules.is_empty() && lines.len() == text.split('\n').count() && !shape.outline().is_empty(), "{form:?} {w}x{h} {text:?}");
                    assert!(lines.iter().all(|l| l.x.is_finite() && l.y.is_finite()) && lines[0].bold && lines.iter().skip(1).all(|l| !l.bold));
                    if form == Form::Package {
                        let Cmd::Line(tab, _) = shape.outline()[0][1] else { panic!() };
                        assert!(tab - 5. <= w.max(1.) + 0.01 && tab > 5., "tab {tab} of {w}");
                    }
                }
            }
        }
    }

    #[test]
    fn a_use_case_has_its_extension_points_under_its_name() {
        let shape = Shape { id: 1, form: Form::Ellipse, x: 0., y: 0., w: 200., h: 100., text: "Pay\n---\nextension points\nby card".into(), color: 0, fill: false, dashed: false, parent: None };
        // In a diagram that reads styles: a block in the middle, all centered, a separator in the chord.
        let (lines, rules) = shape.text_lines_for(true);
        assert_eq!(lines.iter().map(|l| (l.text, l.y, l.centered, l.bold)).collect::<Vec<_>>(), [("Pay", 19., true, false), ("extension points", 45., true, false), ("by card", 63., true, false)]);
        assert_eq!(rules, [41.]);
        let (x0, x1) = shape.rule_span(41.);
        assert!((1.0..3.0).contains(&x0) && (197.0..199.0).contains(&x1), "{x0} {x1}");
        // Outside the ellipse the chord has no width; a rectangle has the whole width.
        assert_eq!(shape.rule_span(-500.), (100., 100.));
        assert_eq!(Shape { form: Form::Rect, ..shape.clone() }.rule_span(41.), (0., 200.));
        // In a diagram from before, the same text is as typed: a line of dashes among the others.
        let (lines, rules) = shape.text_lines_for(false);
        assert!(rules.is_empty() && lines.iter().map(|l| l.text).collect::<Vec<_>>() == ["Pay", "---", "extension points", "by card"]);
        // The rule of any chord of a flat or a tall ellipse stays inside it.
        for (w, h) in [(0., 0.), (10., 200.), (400., 20.), (1., 1.)] {
            let flat = Shape { w, h, ..shape.clone() };
            for y in [-10., 0., h / 2., h, h + 10.] {
                let (a, b) = flat.rule_span(y);
                assert!(a <= b && a >= flat.x - 0.01 && b <= flat.x + w.max(1.) + 0.01, "{w}x{h} at {y}: {a} {b}");
            }
        }
    }

    #[test]
    fn the_shapes_of_state_machines_have_an_outline_of_any_size() {
        for (w, h) in [(0., 0.), (1., 1.), (8., 8.), (12., 12.), (26., 26.), (90., 8.), (300., 10.), (10_000., 3.)] {
            for form in [Form::Dot, Form::Final, Form::Terminate, Form::Bar, Form::Exit, Form::Composite, Form::Divider] {
                for text in ["", "a", "a long name\nsecond line", "日本語 😀"] {
                    let shape = Shape { id: 1, form, x: 5., y: 5., w, h, text: text.into(), color: 0, fill: false, dashed: false, parent: None };
                    let outline = shape.outline();
                    assert!(!outline.is_empty() && shape.solid().iter().all(|i| *i < outline.len()), "{form:?} {w}x{h}");
                    let finite = |p: &Vec<Cmd>| p.iter().all(|c| match *c { Cmd::Move(x, y) | Cmd::Line(x, y) => x.is_finite() && y.is_finite(), _ => true });
                    assert!(outline.iter().all(finite), "{form:?} {w}x{h}");
                    let (lines, _) = shape.text_lines();
                    assert_eq!(lines.len(), text.split('\n').count());
                    // Where a link meets it is on its outline's box, never at the center of a point.
                    let border = shape.border((1e4, 5. + h / 2.));
                    assert!(border.0.is_finite() && border.0 >= 4. && border.0 <= 5. + w.max(2.) + 0.01, "{form:?} {w}x{h}: {border:?}");
                }
            }
        }
        // Filled solid: a dot, a bar, the dot of a final state; nothing else.
        let solid = |form| Shape { id: 1, form, x: 0., y: 0., w: 20., h: 20., text: String::new(), color: 0, fill: false, dashed: false, parent: None }.solid().to_vec();
        assert_eq!((solid(Form::Dot), solid(Form::Bar), solid(Form::Final), solid(Form::Rect), solid(Form::Exit)), (vec![0], vec![0], vec![1], vec![], vec![]));
        // A composite state holds shapes and has its name in a header, like a lane.
        assert!(Form::Composite.holds() && !Form::Dot.holds());
        let composite = Shape { id: 1, form: Form::Composite, x: 0., y: 0., w: 200., h: 100., text: "Active".into(), color: 0, fill: false, dashed: false, parent: None };
        let (lines, _) = composite.text_lines();
        assert_eq!((lines[0].centered, lines[0].bold, lines[0].y), (true, true, 4.));
    }

    #[test]
    fn a_transition_from_a_state_to_itself_is_a_loop() {
        for form in [Form::Rect, Form::Round, Form::Ellipse, Form::Diamond, Form::Dot] {
            let mut d = Diagram::default();
            let s = d.add_shape(form, 100., 100., 120., 60.);
            let ids: Vec<u32> = (0..4).map(|_| d.add_link(End::Shape(s), End::Shape(s), Head::Arrow, Route::Elbow)).collect();
            let mut reached: Vec<f32> = Vec::new();
            for id in &ids {
                let trace = d.trace(d.link(*id).unwrap()).unwrap();
                let (first, last) = (trace.points[0], *trace.points.last().unwrap());
                // It leaves by the right side and comes back into the top, outside the shape in between.
                assert!(first.0 >= 100. + 120. / 2. && last.1 <= 100. + 60. / 2., "{form:?}: {:?}", trace.points);
                assert!(trace.points.len() >= 4 && trace.points.windows(2).all(|w| w[0].0 == w[1].0 || w[0].1 == w[1].1), "{form:?}: {:?}", trace.points);
                let top = trace.points.iter().map(|p| p.1).fold(f32::MAX, f32::min);
                assert!(top < 100., "{form:?}: the loop rises above the shape");
                assert_eq!(trace.heads.len(), 1);
                reached.push(top);
            }
            // Each further loop is wider: none on top of another.
            assert!(reached.windows(2).all(|w| w[1] < w[0]), "{form:?}: {reached:?}");
        }
        // A loop on a shape inside a frame, and a plain arrow to another shape, are not confused with it.
        let mut d = Diagram::default();
        let (a, b) = (d.add_shape(Form::Rect, 0., 0., 100., 60.), d.add_shape(Form::Rect, 300., 0., 100., 60.));
        let own = d.add_link(End::Shape(a), End::Shape(a), Head::Arrow, Route::Straight);
        let other = d.add_link(End::Shape(a), End::Shape(b), Head::Arrow, Route::Straight);
        assert!(d.trace(d.link(own).unwrap()).unwrap().points.len() > 2);
        assert_eq!(d.trace(d.link(other).unwrap()).unwrap().points.len(), 2);
    }
}
