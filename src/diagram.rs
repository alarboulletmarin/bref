//! Schémas : des formes, des flèches entre elles, et leur fichier. Le fichier
//! est un SVG ordinaire, lisible partout, qui porte aussi sa source : c'est elle
//! que l'app relit pour continuer le dessin. Fonctions pures, sans dépendance à l'UI.

use std::fmt::Write;

/// Couleurs du trait, en `0xrrggbb` : des teintes moyennes, lisibles sur fond
/// clair comme sur fond sombre. La première est le neutre ; dans l'app, le
/// thème la remplace par sa couleur de texte.
pub const COLORS: [u32; 6] = [0x7d8590, 0xe5534b, 0xd98e2b, 0x3fa45b, 0x4a8fe0, 0xa371f7];
/// Pas de la grille sur laquelle les formes s'alignent.
pub const GRID: f32 = 10.;
/// Corps du texte et hauteur d'une ligne.
pub const FONT: f32 = 14.;
pub const LINE: f32 = 18.;
/// Ligne de texte qui sépare deux compartiments d'une boîte (classe UML, table).
pub const RULE: &str = "---";

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

/// Pointe d'une flèche. Avec le trait plein ou pointillé, elles couvrent l'UML :
/// association, héritage (triangle), agrégation et composition (losanges).
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

    /// La pointe suivante, pour en changer d'un clic.
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
    /// Rang dans `COLORS`.
    pub color: u8,
    /// Fond teinté de la couleur du trait.
    pub fill: bool,
    pub dashed: bool,
}

/// Bout d'une flèche : accroché à une forme, qu'il suit, ou posé à un endroit.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum End {
    Shape(u32),
    Point(f32, f32),
}

#[derive(Clone, PartialEq, Debug)]
pub struct Link {
    pub id: u32,
    pub from: End,
    pub to: End,
    pub start: Head,
    pub end: Head,
    pub text: String,
    pub color: u8,
    pub dashed: bool,
}

#[derive(Clone, Default, PartialEq, Debug)]
pub struct Diagram {
    /// Dans l'ordre du dessin : la dernière forme est au-dessus.
    pub shapes: Vec<Shape>,
    pub links: Vec<Link>,
    next: u32,
}

/// Morceau de tracé, comme dans l'attribut `d` d'un `<path>`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Cmd {
    Move(f32, f32),
    Line(f32, f32),
    /// Courbe de Bézier cubique : deux points de contrôle, puis l'arrivée.
    Curve(f32, f32, f32, f32, f32, f32),
    Close,
}

/// Une ligne de texte placée : coin haut-gauche ou milieu haut, selon `centered`.
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
    // Quatre arcs de Bézier : l'approximation habituelle du cercle.
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

    /// Le texte découpe la boîte en compartiments.
    fn boxed(&self) -> bool {
        matches!(self.form, Form::Rect | Form::Round) && self.text.lines().any(|l| l.trim() == RULE)
    }

    /// Tracés de la forme. Le premier en est le corps, qui peut recevoir un
    /// fond ; les suivants ne sont que des traits.
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
                    // Le bord avant du couvercle.
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

    /// Où s'écrit chaque ligne du texte, et la hauteur des traits de séparation.
    /// Une boîte à compartiments s'écrit depuis le haut, son premier compartiment
    /// centré et en gras (le nom de la classe), les suivants alignés à gauche.
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
            // Sous le personnage.
            Form::Actor => (cx, self.y + self.h + 4., true),
            _ => (cx, cy - height / 2., true),
        };
        let placed = lines.into_iter().enumerate();
        (placed.map(|(i, text)| TextLine { x, y: top + i as f32 * LINE, text, centered, bold: false }).collect(), rules)
    }

    /// Point du contour sur la demi-droite qui part du centre vers `toward`.
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

/// Polygone d'une pointe posée en `tip`, pour un trait qui arrive dans la
/// direction unitaire `u` : ses points, s'il est fermé, s'il est plein, et de
/// combien raccourcir le trait pour qu'il s'arrête à sa base.
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

    pub fn add_link(&mut self, from: End, to: End, end: Head) -> u32 {
        let id = self.fresh();
        self.links.push(Link { id, from, to, start: Head::None, end, text: String::new(), color: 0, dashed: false });
        id
    }

    /// Copie des éléments `ids`, décalée ; renvoie les identifiants des copies.
    /// Une flèche n'est copiée qu'avec les formes qu'elle relie.
    pub fn duplicate(&mut self, ids: &[u32], by: f32) -> Vec<u32> {
        let mut twin = Vec::new();
        for shape in self.shapes.clone().into_iter().filter(|s| ids.contains(&s.id)) {
            let id = self.fresh();
            twin.push((shape.id, id));
            self.shapes.push(Shape { id, x: shape.x + by, y: shape.y + by, ..shape });
        }
        let moved = |end: End| match end {
            End::Shape(id) => twin.iter().find(|(old, _)| *old == id).map(|(_, new)| End::Shape(*new)),
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

    /// Retire les éléments `ids`, et les flèches accrochées aux formes retirées.
    pub fn remove(&mut self, ids: &[u32]) {
        self.shapes.retain(|s| !ids.contains(&s.id));
        let gone = |end: End| matches!(end, End::Shape(id) if ids.contains(&id));
        self.links.retain(|l| !ids.contains(&l.id) && !gone(l.from) && !gone(l.to));
    }

    /// La forme du dessus sous ce point.
    pub fn shape_at(&self, x: f32, y: f32) -> Option<u32> {
        self.shapes.iter().rev().find(|s| s.contains(x, y)).map(|s| s.id)
    }

    /// La flèche qui passe à moins de `reach` de ce point.
    pub fn link_at(&self, x: f32, y: f32, reach: f32) -> Option<u32> {
        self.links.iter().rev().find(|l| {
            self.ends(l).is_some_and(|((ax, ay), (bx, by))| {
                let (dx, dy) = (bx - ax, by - ay);
                let t = (((x - ax) * dx + (y - ay) * dy) / (dx * dx + dy * dy).max(1e-6)).clamp(0., 1.);
                (x - ax - t * dx).hypot(y - ay - t * dy) <= reach
            })
        })
        .map(|l| l.id)
    }

    /// Les deux bouts du trait : un bout accroché à une forme s'arrête à son
    /// contour, du côté de l'autre bout. `None` si une forme accrochée a disparu.
    pub fn ends(&self, link: &Link) -> Option<((f32, f32), (f32, f32))> {
        let aim = |end: End| match end {
            End::Shape(id) => self.shape(id).map(Shape::center),
            End::Point(x, y) => Some((x, y)),
        };
        let (a, b) = (aim(link.from)?, aim(link.to)?);
        let clip = |end: End, toward| match end {
            End::Shape(id) => self.shape(id).map(|s| s.border(toward)),
            End::Point(x, y) => Some((x, y)),
        };
        Some((clip(link.from, b)?, clip(link.to, a)?))
    }

    /// Rectangle qui contient tout le schéma : (gauche, haut, droite, bas).
    pub fn bounds(&self) -> Option<(f32, f32, f32, f32)> {
        // Le texte d'un personnage s'écrit dessous.
        let boxes = self.shapes.iter().map(|s| {
            let below = if s.form == Form::Actor { 4. + LINE * s.text.split('\n').count() as f32 } else { 0. };
            (s.x, s.y, s.x + s.w, s.y + s.h + below)
        });
        let ends = self.links.iter().filter_map(|l| self.ends(l)).flat_map(|(a, b)| [a, b]);
        boxes.chain(ends.map(|(x, y)| (x, y, x, y))).reduce(|a, b| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)))
    }

    /// Le fichier du schéma : son dessin en SVG, et sa source dans `<metadata>`.
    /// `neutral` est la couleur du trait neutre, en `0xrrggbb`.
    pub fn to_svg(&self, neutral: u32) -> String {
        let color = |i: u8| format!("#{:06x}", if i == 0 { neutral } else { COLORS[i as usize % COLORS.len()] });
        let (x0, y0, x1, y1) = self.bounds().unwrap_or((0., 0., 100., 60.));
        let (x0, y0, w, h) = (x0 - 16., y0 - 16., x1 - x0 + 32., y1 - y0 + 32.);
        let mut out = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"{} {} {} {}\" fill=\"none\" stroke-width=\"1.5\" stroke-linecap=\"round\" stroke-linejoin=\"round\" font-family=\"sans-serif\" font-size=\"{FONT}\">\n",
            num(w), num(h), num(x0), num(y0), num(w), num(h),
        );
        let dash = |dashed: bool| if dashed { " stroke-dasharray=\"6 4\"" } else { "" };
        let label = |out: &mut String, x: f32, y: f32, text: &str, centered: bool, bold: bool, ink: &str| {
            if !text.is_empty() {
                let anchor = if centered { " text-anchor=\"middle\"" } else { "" };
                let weight = if bold { " font-weight=\"bold\"" } else { "" };
                let _ = writeln!(out, "<text x=\"{}\" y=\"{}\"{anchor}{weight} fill=\"{ink}\" stroke=\"none\">{}</text>", num(x), num(y + FONT), xml(text));
            }
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
                label(&mut out, line.x, line.y, line.text, line.centered, line.bold, &ink);
            }
        }
        for link in &self.links {
            let Some((a, b)) = self.ends(link) else { continue };
            let ink = color(link.color);
            let len = (b.0 - a.0).hypot(b.1 - a.1).max(1e-3);
            let u = ((b.0 - a.0) / len, (b.1 - a.1) / len);
            let (start, end) = (head(link.start, a, (-u.0, -u.1)), head(link.end, b, u));
            let (a2, b2) = ((a.0 + u.0 * start.3, a.1 + u.1 * start.3), (b.0 - u.0 * end.3, b.1 - u.1 * end.3));
            let _ = writeln!(out, "<path d=\"M{} {}L{} {}\" stroke=\"{ink}\"{}/>", num(a2.0), num(a2.1), num(b2.0), num(b2.1), dash(link.dashed));
            for (points, closed, full, _) in [start, end] {
                if let Some((first, rest)) = points.split_first() {
                    let mut d = format!("M{} {}", num(first.0), num(first.1));
                    rest.iter().for_each(|p| { let _ = write!(d, "L{} {}", num(p.0), num(p.1)); });
                    let fill = if full { format!(" fill=\"{ink}\"") } else { String::new() };
                    let _ = writeln!(out, "<path d=\"{d}{}\" stroke=\"{ink}\"{fill}/>", if closed { "Z" } else { "" });
                }
            }
            let mid = ((a.0 + b.0) / 2., (a.1 + b.1) / 2.);
            let lines: Vec<&str> = link.text.split('\n').collect();
            for (i, text) in lines.iter().enumerate() {
                label(&mut out, mid.0, mid.1 - LINE * lines.len() as f32 - 2. + LINE * i as f32, text, true, false, &ink);
            }
        }
        let _ = write!(out, "<metadata id=\"bref-diagram\">\n{}</metadata>\n</svg>\n", xml(&self.source()));
        out
    }

    /// La source : une ligne par élément, le texte en dernier.
    fn source(&self) -> String {
        let name = |form: Form| Form::ALL.iter().find(|(f, _)| *f == form).map_or("rect", |(_, n)| n);
        let tip = |head: Head| Head::ALL.iter().find(|(h, _)| *h == head).map_or("none", |(_, n)| n);
        let end = |end: End| match end {
            End::Shape(id) => format!("s{id}"),
            End::Point(x, y) => format!("p{},{}", num(x), num(y)),
        };
        let text = |text: &str| text.replace('\\', "\\\\").replace('\n', "\\n");
        let mut out = String::new();
        for s in &self.shapes {
            let _ = writeln!(out, "shape {} {} {} {} {} {} {} {} {} {}", s.id, name(s.form), num(s.x), num(s.y), num(s.w), num(s.h), s.color, s.fill as u8, s.dashed as u8, text(&s.text));
        }
        for l in &self.links {
            let _ = writeln!(out, "link {} {} {} {} {} {} {} {}", l.id, end(l.from), end(l.to), tip(l.start), tip(l.end), l.color, l.dashed as u8, text(&l.text));
        }
        out
    }

    /// Le schéma que porte ce fichier SVG ; `None` si ce n'en est pas un.
    pub fn from_svg(svg: &str) -> Option<Diagram> {
        let body = svg.split_once("<metadata id=\"bref-diagram\">")?.1.split_once("</metadata>")?.0;
        let body = body.replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&");
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
            ("s", id) => Some(End::Shape(id.parse().ok()?)),
            ("p", at) => at.split_once(',').and_then(|(x, y)| Some(End::Point(x.parse().ok()?, y.parse().ok()?))),
            _ => None,
        };
        let mut diagram = Diagram::default();
        // Une ligne illisible est laissée de côté : le reste du schéma s'ouvre.
        for line in body.lines() {
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
                } else if let Some(rest) = line.strip_prefix("link ") {
                    let f: Vec<&str> = rest.splitn(8, ' ').collect();
                    let tip = |i: usize| Head::ALL.iter().find(|(_, name)| Some(name) == f.get(i)).map(|(head, _)| *head);
                    diagram.links.push(Link {
                        id: f.first()?.parse().ok()?,
                        from: end(f.get(1)?)?,
                        to: end(f.get(2)?)?,
                        start: tip(3)?,
                        end: tip(4)?,
                        color: f.get(5)?.parse().ok()?,
                        dashed: *f.get(6)? == "1",
                        text: text(f.get(7).unwrap_or(&"")),
                    });
                }
                Some(())
            };
            read();
        }
        let ids = diagram.shapes.iter().map(|s| s.id).chain(diagram.links.iter().map(|l| l.id));
        diagram.next = ids.max().unwrap_or(0);
        Some(diagram)
    }
}

/// Nombre écrit au dixième, sans décimale inutile.
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
        let link = d.add_link(End::Shape(a), End::Shape(b), Head::Triangle);
        d.link_mut(link).unwrap().text = "hérite".into();
        d.add_link(End::Point(5.5, -20.), End::Shape(a), Head::Arrow);
        let svg = d.to_svg(COLORS[0]);
        assert!(svg.starts_with("<svg") && svg.contains("font-weight=\"bold\"") && svg.contains("&lt;€&gt;"));
        assert_eq!(Diagram::from_svg(&svg), Some(d.clone()));
        assert_eq!(Diagram::from_svg("<svg></svg>"), None);
        // Les identifiants repartent après les plus grands relus.
        let mut back = Diagram::from_svg(&svg).unwrap();
        assert_eq!(back.add_shape(Form::Note, 0., 0., 10., 10.), 5);

        // Un trait accroché s'arrête au contour des formes, face à face.
        let (from, to) = d.ends(&d.links[0]).unwrap();
        assert!((119.9..=120.1).contains(&from.0) && (299.0..=301.).contains(&to.0));
        assert_eq!((d.shape_at(10., 10.), d.shape_at(200., 10.)), (Some(a), None));
        assert_eq!(d.link_at(200., 36., 6.), Some(link));

        // Retirer une forme emporte ses flèches ; la copie d'une flèche suit ses formes.
        let copies = d.duplicate(&[a, b, link], 20.);
        assert_eq!((copies.len(), d.links.len()), (3, 3));
        d.remove(&[a]);
        assert_eq!((d.shapes.len(), d.links.len()), (3, 1));
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
    }
}
