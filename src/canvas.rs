//! Canevas : l'éditeur de schémas. On y pose des formes et des flèches à la
//! souris, on les déplace, on écrit dedans ; les outils se choisissent d'une
//! lettre, comme dans Excalidraw. Le dessin lui-même vit dans `diagram`.

use std::ops::Range;

use gpui::{
    App, Bounds, ClipboardItem, ContentMask, Context, CursorStyle, ElementInputHandler, EntityInputHandler,
    EventEmitter, FocusHandle, Focusable, FontWeight, Hsla, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, PathBuilder, Pixels, Point, Rgba, ScrollWheelEvent, SharedString, TextRun, UTF16Selection,
    Window, actions, canvas, div, fill, font, point, prelude::*, px, quad, rgb, size,
};

use crate::{
    Theme,
    diagram::{self, Cmd, Diagram, End, Form, Head},
    nav::button,
    sans, tr,
};

actions!(canvas, [Erase, EraseNext, Duplicate, SelectAll, Undo, Redo, Confirm, Cancel, Left, Right, Up, Down, Paste]);

pub enum CanvasEvent {
    /// Le schéma a changé : il est à enregistrer.
    Changed,
    /// Une image PNG du schéma est demandée.
    Export,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tool {
    Select,
    Shape(Form),
    /// Trait ou flèche, selon sa pointe.
    Link(Head),
}

/// Outils de la barre : (outil, icône, lettre).
const TOOLS: [(Tool, &str, &str); 11] = [
    (Tool::Select, "d-select.svg", "v"),
    (Tool::Shape(Form::Rect), "d-rect.svg", "r"),
    (Tool::Shape(Form::Round), "d-round.svg", "u"),
    (Tool::Shape(Form::Ellipse), "d-ellipse.svg", "o"),
    (Tool::Shape(Form::Diamond), "d-diamond.svg", "d"),
    (Tool::Shape(Form::Cylinder), "d-cylinder.svg", "c"),
    (Tool::Shape(Form::Actor), "d-actor.svg", "p"),
    (Tool::Shape(Form::Note), "d-note.svg", "n"),
    (Tool::Shape(Form::Text), "d-text.svg", "t"),
    (Tool::Link(Head::Arrow), "d-arrow.svg", "a"),
    (Tool::Link(Head::None), "d-line.svg", "l"),
];

/// Geste en cours à la souris ; les points sont dans le repère du schéma.
#[derive(Clone, Copy, PartialEq)]
enum Drag {
    None,
    /// Déplacement de la sélection, depuis `last` ; `begun` dès qu'elle a bougé.
    Move { last: (f32, f32), begun: bool },
    /// Coin tiré d'une forme : le coin opposé, `fixed`, ne bouge pas.
    Resize { id: u32, fixed: (f32, f32) },
    Marquee { from: (f32, f32), to: (f32, f32) },
    Create { id: u32, from: (f32, f32) },
    /// Bout d'une flèche (`to` : son arrivée) qu'on tire.
    Tip { id: u32, to: bool },
    Pan { last: Point<Pixels> },
}

pub struct Canvas {
    focus: FocusHandle,
    theme: Theme,
    diagram: Diagram,
    selected: Vec<u32>,
    tool: Tool,
    drag: Drag,
    /// Décalage (px) de l'origine du schéma dans la vue, et pixels par unité.
    pan: (f32, f32),
    zoom: f32,
    /// Cadrer tout le schéma à la prochaine frame.
    fit: bool,
    /// Élément dont on écrit le texte, et place du curseur (octet).
    editing: Option<(u32, usize)>,
    undo: Vec<Diagram>,
    redo: Vec<Diagram>,
    bounds: Bounds<Pixels>,
}

impl EventEmitter<CanvasEvent> for Canvas {}

impl Focusable for Canvas {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Canvas {
    pub fn new(diagram: Diagram, theme: Theme, cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            theme,
            diagram,
            selected: Vec::new(),
            tool: Tool::Select,
            drag: Drag::None,
            pan: (0., 0.),
            zoom: 1.,
            fit: true,
            editing: None,
            undo: Vec::new(),
            redo: Vec::new(),
            bounds: Bounds::default(),
        }
    }

    pub fn diagram(&self) -> &Diagram {
        &self.diagram
    }

    /// Le thème, tel que la fenêtre le connaît.
    pub fn sync(&mut self, theme: Theme) {
        self.theme = theme;
    }

    /// Position à l'écran d'un point du schéma.
    pub fn spot(&self, x: f32, y: f32) -> Point<Pixels> {
        let o = self.bounds.origin;
        point(o.x + px(self.pan.0 + x * self.zoom), o.y + px(self.pan.1 + y * self.zoom))
    }

    fn world(&self, at: Point<Pixels>) -> (f32, f32) {
        let o = self.bounds.origin;
        ((f32::from(at.x - o.x) - self.pan.0) / self.zoom, (f32::from(at.y - o.y) - self.pan.1) / self.zoom)
    }

    /// Couleur d'un élément : le neutre prend la couleur de texte du thème.
    fn ink(&self, color: u8) -> Hsla {
        match color {
            0 => self.theme.text,
            i => rgb(diagram::COLORS[i as usize % diagram::COLORS.len()]).into(),
        }
    }

    // ponytail: l'annulation garde des copies entières du schéma (100 max) : un
    // schéma pèse quelques Ko. Passer à un journal d'opérations s'ils grossissent.
    fn remember(&mut self) {
        self.undo.push(self.diagram.clone());
        if self.undo.len() > 100 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn changed(&mut self, cx: &mut Context<Self>) {
        cx.emit(CanvasEvent::Changed);
        cx.notify();
    }

    fn restore(&mut self, redo: bool, cx: &mut Context<Self>) {
        let (from, to) = if redo { (&mut self.redo, &mut self.undo) } else { (&mut self.undo, &mut self.redo) };
        if let Some(diagram) = from.pop() {
            to.push(std::mem::replace(&mut self.diagram, diagram));
            (self.editing, self.drag) = (None, Drag::None);
            self.selected.clear();
            self.changed(cx);
        }
    }

    fn set_tool(&mut self, tool: Tool, cx: &mut Context<Self>) {
        self.stop_editing(cx);
        self.tool = tool;
        cx.notify();
    }

    fn text_of(&mut self, id: u32) -> Option<&mut String> {
        if self.diagram.shape(id).is_some() {
            return self.diagram.shape_mut(id).map(|s| &mut s.text);
        }
        self.diagram.link_mut(id).map(|l| &mut l.text)
    }

    fn start_editing(&mut self, id: u32, cx: &mut Context<Self>) {
        self.remember();
        let Some(len) = self.text_of(id).map(|t| t.len()) else { return };
        self.editing = Some((id, len));
        self.selected = vec![id];
        cx.notify();
    }

    fn stop_editing(&mut self, cx: &mut Context<Self>) {
        if let Some((id, _)) = self.editing.take() {
            // Un texte laissé vide ne laisse rien derrière lui.
            let empty = self.diagram.shape(id).is_some_and(|s| s.form == Form::Text && s.text.trim().is_empty());
            if empty {
                self.diagram.remove(&[id]);
                self.selected.clear();
            }
            self.changed(cx);
        }
    }

    /// Insère `text` au curseur ; hors écriture, une lettre choisit un outil.
    fn typed(&mut self, text: &str, cx: &mut Context<Self>) {
        let Some((id, at)) = self.editing else {
            if let Some((tool, ..)) = TOOLS.iter().find(|(_, _, key)| key.eq_ignore_ascii_case(text)) {
                self.set_tool(*tool, cx);
            }
            return;
        };
        if let Some(target) = self.text_of(id) {
            target.insert_str(at, text);
            self.editing = Some((id, at + text.len()));
            self.changed(cx);
        }
    }

    /// Déplace le curseur d'un caractère, ou l'efface si `erase`.
    fn step(&mut self, forward: bool, erase: bool, cx: &mut Context<Self>) {
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

    fn erase(&mut self, forward: bool, cx: &mut Context<Self>) {
        if self.editing.is_some() {
            return self.step(forward, true, cx);
        }
        if !self.selected.is_empty() {
            self.remember();
            self.diagram.remove(&std::mem::take(&mut self.selected));
            self.changed(cx);
        }
    }

    /// Flèches du clavier : le curseur en écriture, sinon la sélection, d'un pas de grille.
    fn nudge(&mut self, dx: f32, dy: f32, cx: &mut Context<Self>) {
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

    /// Déplace la sélection ; une flèche emporte ses bouts libres.
    fn shift(&mut self, dx: f32, dy: f32) {
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

    /// Applique un réglage aux éléments sélectionnés.
    fn style(&mut self, cx: &mut Context<Self>, set: impl Fn(Option<&mut diagram::Shape>, Option<&mut diagram::Link>)) {
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

    /// Coins de la forme, dans l'ordre : haut-gauche, haut-droit, bas-droit, bas-gauche.
    fn corners(shape: &diagram::Shape) -> [(f32, f32); 4] {
        let (x, y, x1, y1) = (shape.x, shape.y, shape.x + shape.w, shape.y + shape.h);
        [(x, y), (x1, y), (x1, y1), (x, y1)]
    }

    /// Bout de flèche pour ce point : la forme qui s'y trouve, sinon le point aimanté.
    fn end_at(&self, p: (f32, f32), not: Option<u32>) -> End {
        match self.diagram.shape_at(p.0, p.1).filter(|id| Some(*id) != not) {
            Some(id) => End::Shape(id),
            None => End::Point(diagram::snap(p.0), diagram::snap(p.1)),
        }
    }

    fn mouse_down(&mut self, e: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus);
        self.stop_editing(cx);
        let p = self.world(e.position);
        let near = 7. / self.zoom;
        let close = |a: (f32, f32)| (a.0 - p.0).hypot(a.1 - p.1) <= near;
        self.drag = match self.tool {
            Tool::Shape(form) => {
                self.remember();
                let from = (diagram::snap(p.0), diagram::snap(p.1));
                let id = self.diagram.add_shape(form, from.0, from.1, 0., 0.);
                Drag::Create { id, from }
            }
            Tool::Link(head) => {
                self.remember();
                let from = self.end_at(p, None);
                let id = self.diagram.add_link(from, End::Point(p.0, p.1), head);
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
                let hit = self.diagram.shape_at(p.0, p.1).or_else(|| self.diagram.link_at(p.0, p.1, near));
                match (corner.or(tip), hit) {
                    (Some(drag), _) => {
                        self.remember();
                        drag
                    }
                    (None, Some(id)) => {
                        if e.modifiers.shift {
                            match self.selected.iter().position(|s| *s == id) {
                                Some(i) => drop(self.selected.remove(i)),
                                None => self.selected.push(id),
                            }
                        } else if !self.selected.contains(&id) {
                            self.selected = vec![id];
                        }
                        if e.click_count >= 2 {
                            self.start_editing(id, cx);
                            Drag::None
                        } else {
                            Drag::Move { last: p, begun: false }
                        }
                    }
                    // Double-clic dans le vide : un texte, à cet endroit.
                    (None, None) if e.click_count >= 2 => {
                        self.remember();
                        let id = self.diagram.add_shape(Form::Text, diagram::snap(p.0), diagram::snap(p.1), 80., diagram::LINE);
                        (self.editing, self.selected) = (Some((id, 0)), vec![id]);
                        Drag::None
                    }
                    (None, None) => {
                        if !e.modifiers.shift {
                            self.selected.clear();
                        }
                        Drag::Marquee { from: p, to: p }
                    }
                }
            }
        };
        cx.notify();
    }

    fn mouse_move(&mut self, e: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        let held = e.pressed_button;
        let p = self.world(e.position);
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
                // Un bout ne s'accroche pas à la forme d'où part l'autre.
                let other = self.diagram.link(id).map(|l| if to { l.from } else { l.to });
                let not = match other {
                    Some(End::Shape(id)) => Some(id),
                    _ => None,
                };
                let end = self.end_at(p, not);
                if let Some(link) = self.diagram.link_mut(id) {
                    *(if to { &mut link.to } else { &mut link.from }) = end;
                }
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

    fn mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        match std::mem::replace(&mut self.drag, Drag::None) {
            Drag::Create { id, .. } => {
                let Some(shape) = self.diagram.shape_mut(id) else { return };
                // Un simple clic pose la forme à sa taille habituelle.
                if shape.w < diagram::GRID || shape.h < diagram::GRID {
                    (shape.w, shape.h) = match shape.form {
                        Form::Text => (80., diagram::LINE),
                        Form::Actor => (40., 80.),
                        Form::Cylinder => (80., 100.),
                        Form::Ellipse | Form::Diamond => (120., 80.),
                        _ => (140., 60.),
                    };
                }
                let text = shape.form == Form::Text;
                (self.selected, self.tool) = (vec![id], Tool::Select);
                if text {
                    self.editing = Some((id, 0));
                }
                self.changed(cx);
            }
            Drag::Tip { id, .. } => {
                // Un trait sans longueur n'a pas lieu d'être.
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
            Drag::Move { begun: true, .. } | Drag::Resize { .. } => self.changed(cx),
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

    /// La molette déplace la vue ; avec Ctrl (Cmd), elle zoome autour du pointeur.
    fn scroll(&mut self, e: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
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

    /// La forme en cours d'écriture s'agrandit pour contenir son texte.
    fn fit_text(&mut self, window: &mut Window) {
        let Some(shape) = self.editing.and_then(|(id, _)| self.diagram.shape(id)) else { return };
        let (lines, _) = shape.text_lines();
        let widest = lines.iter().map(|l| self.measure(l.text, l.bold, 1., window)).fold(0f32, f32::max);
        let tall = lines.last().map_or(0., |l| l.y + diagram::LINE - shape.y);
        let (form, id) = (shape.form, shape.id);
        let Some(shape) = self.diagram.shape_mut(id) else { return };
        match form {
            Form::Text => (shape.w, shape.h) = (widest.max(20.), tall.max(diagram::LINE)),
            Form::Actor => {}
            _ => {
                let grow = |have: f32, need: f32| if need > have { (need / diagram::GRID).ceil() * diagram::GRID } else { have };
                (shape.w, shape.h) = (grow(shape.w, widest + 20.), grow(shape.h, tall + 6.));
            }
        }
    }

    fn line(&self, text: &str, bold: bool, color: Hsla, scale: f32, window: &mut Window) -> gpui::ShapedLine {
        let weight = if bold { FontWeight::BOLD } else { FontWeight::NORMAL };
        let run = TextRun {
            len: text.len(),
            font: gpui::Font { weight, ..font(sans()) },
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        window.text_system().shape_line(SharedString::from(text.to_string()), px(diagram::FONT * scale), &[run], None)
    }

    fn measure(&self, text: &str, bold: bool, scale: f32, window: &mut Window) -> f32 {
        f32::from(self.line(text, bold, self.theme.text, scale, window).width)
    }

    fn paint(&mut self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        self.bounds = bounds;
        if std::mem::take(&mut self.fit) {
            let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
            match self.diagram.bounds() {
                Some((x0, y0, x1, y1)) => {
                    self.zoom = (w / (x1 - x0 + 80.)).min(h / (y1 - y0 + 80.)).clamp(0.2, 1.);
                    self.pan = (w / 2. - (x0 + x1) / 2. * self.zoom, h / 2. - (y0 + y1) / 2. * self.zoom);
                }
                None => (self.pan, self.zoom) = ((w / 2., h / 2.), 1.),
            }
        }
        self.fit_text(window);
        let (t, zoom) = (self.theme, self.zoom);
        let at = |p: (f32, f32)| self.spot(p.0, p.1);
        let trace = |builder: &mut PathBuilder, path: &[Cmd]| {
            for cmd in path {
                match *cmd {
                    Cmd::Move(x, y) => builder.move_to(at((x, y))),
                    Cmd::Line(x, y) => builder.line_to(at((x, y))),
                    Cmd::Curve(a, b, c, d, x, y) => builder.cubic_bezier_to(at((x, y)), at((a, b)), at((c, d))),
                    Cmd::Close => builder.close(),
                }
            }
        };
        let pen = |dashed: bool| {
            let pen = PathBuilder::stroke(px(1.5 * zoom.max(0.6)));
            if dashed { pen.dash_array(&[px(6. * zoom), px(4. * zoom)]) } else { pen }
        };
        let stroke = |window: &mut Window, path: &[Cmd], dashed: bool, color: Hsla| {
            let mut builder = pen(dashed);
            trace(&mut builder, path);
            if let Ok(path) = builder.build() {
                window.paint_path(path, color);
            }
        };
        let flood = |window: &mut Window, path: &[Cmd], color: Hsla| {
            let mut builder = PathBuilder::fill();
            trace(&mut builder, path);
            if let Ok(path) = builder.build() {
                window.paint_path(path, color);
            }
        };
        let write = |this: &Self, window: &mut Window, cx: &mut App, x: f32, y: f32, text: &str, centered: bool, bold: bool, color: Hsla| {
            if text.is_empty() {
                return;
            }
            let line = this.line(text, bold, color, zoom, window);
            let origin = at((x, y)) - point(if centered { line.width / 2. } else { px(0.) }, px(0.));
            line.paint(origin, px(diagram::LINE * zoom), window, cx).ok();
        };

        // Curseur du texte en cours d'écriture : (ligne, octet dans la ligne).
        let caret = self.editing.and_then(|(id, at)| {
            let text = self.diagram.shape(id).map(|s| &s.text).or_else(|| self.diagram.link(id).map(|l| &l.text))?;
            let before = &text[..at];
            Some((id, before.matches('\n').count(), before.len() - before.rfind('\n').map_or(0, |i| i + 1)))
        });
        let mut caret_at = None;

        for shape in &self.diagram.shapes {
            let ink = self.ink(shape.color);
            let outline = shape.outline();
            if shape.fill && let Some(body) = outline.first() {
                flood(window, body, ink.opacity(0.15));
            }
            for path in &outline {
                stroke(window, path, shape.dashed, ink);
            }
            let (lines, rules) = shape.text_lines();
            for y in rules {
                stroke(window, &[Cmd::Move(shape.x, y), Cmd::Line(shape.x + shape.w, y)], false, ink);
            }
            // Les traits de séparation ne sont pas des lignes de texte : on retrouve
            // celle du curseur par sa place dans le texte.
            let starts: Vec<usize> = shape.text.split('\n').scan(0, |at, l| Some(std::mem::replace(at, *at + l.len() + 1))).collect();
            for line in &lines {
                write(self, window, cx, line.x, line.y, line.text, line.centered, line.bold, ink);
                if let Some((id, row, col)) = caret
                    && id == shape.id
                    && starts.get(row) == Some(&(line.text.as_ptr() as usize - shape.text.as_ptr() as usize))
                {
                    let width = self.measure(line.text, line.bold, 1., window);
                    let left = if line.centered { line.x - width / 2. } else { line.x };
                    caret_at = Some((left + self.measure(&line.text[..col], line.bold, 1., window), line.y));
                }
            }
        }
        for link in &self.diagram.links {
            let Some((a, b)) = self.diagram.ends(link) else { continue };
            let ink = self.ink(link.color);
            let len = (b.0 - a.0).hypot(b.1 - a.1).max(1e-3);
            let u = ((b.0 - a.0) / len, (b.1 - a.1) / len);
            let (start, end) = (diagram::head(link.start, a, (-u.0, -u.1)), diagram::head(link.end, b, u));
            let (a2, b2) = ((a.0 + u.0 * start.3, a.1 + u.1 * start.3), (b.0 - u.0 * end.3, b.1 - u.1 * end.3));
            stroke(window, &[Cmd::Move(a2.0, a2.1), Cmd::Line(b2.0, b2.1)], link.dashed, ink);
            for (points, closed, full, _) in [start, end] {
                let mut path: Vec<Cmd> = points.iter().enumerate().map(|(i, p)| if i == 0 { Cmd::Move(p.0, p.1) } else { Cmd::Line(p.0, p.1) }).collect();
                if closed {
                    path.push(Cmd::Close);
                }
                if full {
                    flood(window, &path, ink);
                }
                if !path.is_empty() {
                    stroke(window, &path, false, ink);
                }
            }
            let mid = ((a.0 + b.0) / 2., (a.1 + b.1) / 2.);
            let lines: Vec<&str> = link.text.split('\n').collect();
            for (i, text) in lines.iter().enumerate() {
                let y = mid.1 - diagram::LINE * (lines.len() - i) as f32 - 2.;
                write(self, window, cx, mid.0, y, text, true, false, ink);
                if let Some((id, row, col)) = caret
                    && id == link.id
                    && row == i
                {
                    let width = self.measure(text, false, 1., window);
                    caret_at = Some((mid.0 - width / 2. + self.measure(&text[..col], false, 1., window), y));
                }
            }
        }

        // Sélection : un cadre, et des poignées aux coins d'une forme seule.
        let frame = |window: &mut Window, x0: f32, y0: f32, x1: f32, y1: f32, color: Hsla| {
            let path = [Cmd::Move(x0, y0), Cmd::Line(x1, y0), Cmd::Line(x1, y1), Cmd::Line(x0, y1), Cmd::Close];
            let mut builder = PathBuilder::stroke(px(1.)).dash_array(&[px(4.), px(3.)]);
            trace(&mut builder, &path);
            if let Ok(path) = builder.build() {
                window.paint_path(path, color);
            }
        };
        let knob = |window: &mut Window, p: (f32, f32)| {
            let c = at(p);
            let bounds = Bounds::new(point(c.x - px(4.), c.y - px(4.)), size(px(8.), px(8.)));
            window.paint_quad(quad(bounds, px(2.), t.bg, px(1.5), t.accent, gpui::BorderStyle::default()));
        };
        let pad = 4. / zoom;
        for &id in &self.selected {
            if let Some(shape) = self.diagram.shape(id) {
                frame(window, shape.x - pad, shape.y - pad, shape.x + shape.w + pad, shape.y + shape.h + pad, t.accent);
                if self.selected.len() == 1 && self.editing.is_none() {
                    Self::corners(shape).into_iter().for_each(|c| knob(window, c));
                }
            } else if let Some((a, b)) = self.diagram.link(id).and_then(|l| self.diagram.ends(l)) {
                knob(window, a);
                knob(window, b);
            }
        }
        if let Drag::Marquee { from, to } = self.drag {
            let (a, b) = (at((from.0.min(to.0), from.1.min(to.1))), at((from.0.max(to.0), from.1.max(to.1))));
            window.paint_quad(fill(Bounds::from_corners(a, b), t.selection));
        }
        if let Some((x, y)) = caret_at
            && self.focus.is_focused(window)
        {
            let top = at((x, y + diagram::LINE * 0.1));
            window.paint_quad(fill(Bounds::new(top, size(px(1.5), px(diagram::LINE * 0.8 * zoom))), t.accent));
        }
        if self.diagram.shapes.is_empty() && self.diagram.links.is_empty() {
            let hint = tr(
                "Pick a shape above, or press R, O, D, A, T…, then drag. Double-click to write.",
                "Choisis une forme ci-dessus, ou tape R, O, D, A, T…, puis fais glisser. Double-clic pour écrire.",
            );
            let line = self.line(hint, false, t.dim, 0.93, window);
            let origin = bounds.center() - point(line.width / 2., px(10.));
            line.paint(origin, px(20.), window, cx).ok();
        }
    }
}

// ponytail: saisie au fil de l'eau, sans composition IME (le texte composé est
// inséré tel quel) ni sélection dans le texte ; reprendre le gestionnaire de
// l'éditeur si l'on écrit des schémas en japonais ou en chinois.
impl EntityInputHandler for Canvas {
    fn text_for_range(&mut self, _: Range<usize>, _: &mut Option<Range<usize>>, _: &mut Window, _: &mut Context<Self>) -> Option<String> {
        None
    }

    fn selected_text_range(&mut self, _: bool, _: &mut Window, _: &mut Context<Self>) -> Option<UTF16Selection> {
        Some(UTF16Selection { range: 0..0, reversed: false })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        None
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {}

    fn replace_text_in_range(&mut self, _: Option<Range<usize>>, text: &str, _: &mut Window, cx: &mut Context<Self>) {
        self.typed(text, cx);
    }

    fn replace_and_mark_text_in_range(&mut self, _: Option<Range<usize>>, text: &str, _: Option<Range<usize>>, _: &mut Window, cx: &mut Context<Self>) {
        self.typed(text, cx);
    }

    fn bounds_for_range(&mut self, _: Range<usize>, _: Bounds<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<Bounds<Pixels>> {
        None
    }

    fn character_index_for_point(&mut self, _: Point<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        None
    }
}

impl Render for Canvas {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        let (this, focus) = (cx.entity(), self.focus.clone());
        let drawing = canvas(
            |_, _, _| (),
            move |bounds, _, window, cx| {
                window.handle_input(&focus, ElementInputHandler::new(bounds, this.clone()), cx);
                window.with_content_mask(Some(ContentMask { bounds }), |window| {
                    this.update(cx, |canvas, cx| canvas.paint(bounds, window, cx))
                })
            },
        )
        .size_full();

        let tools = TOOLS.iter().map(|&(tool, icon, _)| {
            button(icon, icon, self.tool == tool, t).on_click(cx.listener(move |this, _, _, cx| this.set_tool(tool, cx)))
        });
        // Réglages de la sélection : couleur, fond, pointillé, pointes des flèches.
        let any = !self.selected.is_empty();
        let links = self.selected.iter().any(|id| self.diagram.shape(*id).is_none());
        let swatches = (0..diagram::COLORS.len() as u8).map(|i| {
            let color = self.ink(i);
            div()
                .id(("d-color", i as usize))
                .size(px(16.))
                .flex_none()
                .rounded_full()
                .cursor_pointer()
                .bg(color)
                .hover(|s| s.opacity(0.75))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.style(cx, |shape, link| {
                        if let Some(shape) = shape {
                            shape.color = i;
                        }
                        if let Some(link) = link {
                            link.color = i;
                        }
                    })
                }))
        });
        let toggle = |id, icon, cx: &Context<Self>, set: fn(Option<&mut diagram::Shape>, Option<&mut diagram::Link>)| {
            button(id, icon, false, t).on_click(cx.listener(move |this, _, _, cx| this.style(cx, set)))
        };
        let bar = div()
            .flex_none()
            .h(px(44.))
            .px_2()
            .flex()
            .items_center()
            .gap_1()
            .children(tools)
            .when(any, |d| {
                d.child(div().w(px(1.)).h(px(16.)).mx_1().bg(t.border))
                    .children(swatches)
                    .child(div().w(px(4.)))
                    .child(toggle("d-fill", "d-fill.svg", cx, |shape, _| {
                        if let Some(shape) = shape {
                            shape.fill = !shape.fill;
                        }
                    }))
                    .child(toggle("d-dash", "d-dash.svg", cx, |shape, link| {
                        if let Some(shape) = shape {
                            shape.dashed = !shape.dashed;
                        }
                        if let Some(link) = link {
                            link.dashed = !link.dashed;
                        }
                    }))
            })
            .when(links, |d| {
                d.child(toggle("d-head-start", "d-head-start.svg", cx, |_, link| {
                    if let Some(link) = link {
                        link.start = link.start.next();
                    }
                }))
                .child(toggle("d-head-end", "d-head-end.svg", cx, |_, link| {
                    if let Some(link) = link {
                        link.end = link.end.next();
                    }
                }))
            })
            .child(div().flex_1())
            .child(button("d-export", "export.svg", false, t).on_click(cx.listener(|_, _, _, cx| cx.emit(CanvasEvent::Export))));

        div()
            .size_full()
            .flex()
            .flex_col()
            .key_context("Canvas")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &Erase, _, cx| this.erase(false, cx)))
            .on_action(cx.listener(|this, _: &EraseNext, _, cx| this.erase(true, cx)))
            .on_action(cx.listener(|this, _: &Left, _, cx| this.nudge(-1., 0., cx)))
            .on_action(cx.listener(|this, _: &Right, _, cx| this.nudge(1., 0., cx)))
            .on_action(cx.listener(|this, _: &Up, _, cx| this.nudge(0., -1., cx)))
            .on_action(cx.listener(|this, _: &Down, _, cx| this.nudge(0., 1., cx)))
            .on_action(cx.listener(|this, _: &Undo, _, cx| this.restore(false, cx)))
            .on_action(cx.listener(|this, _: &Redo, _, cx| this.restore(true, cx)))
            .on_action(cx.listener(|this, _: &Duplicate, _, cx| {
                if !this.selected.is_empty() {
                    this.remember();
                    this.selected = this.diagram.duplicate(&this.selected, 2. * diagram::GRID);
                    this.changed(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &SelectAll, _, cx| {
                this.selected = this.diagram.shapes.iter().map(|s| s.id).chain(this.diagram.links.iter().map(|l| l.id)).collect();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Paste, _, cx| {
                if let Some(text) = cx.read_from_clipboard().and_then(|item: ClipboardItem| item.text()) {
                    this.typed(&text, cx);
                }
            }))
            // Entrée : écrire dans l'élément sélectionné ; en écriture, aller à la ligne.
            .on_action(cx.listener(|this, _: &Confirm, _, cx| match (this.editing, this.selected.as_slice()) {
                (Some(_), _) => this.typed("\n", cx),
                (None, &[id]) => this.start_editing(id, cx),
                _ => {}
            }))
            // Échap : finir d'écrire, lâcher l'outil, puis la sélection ; sinon la fenêtre décide.
            .on_action(cx.listener(|this, _: &Cancel, _, cx| {
                if this.editing.is_some() {
                    this.stop_editing(cx);
                } else if this.tool != Tool::Select {
                    this.set_tool(Tool::Select, cx);
                } else if !this.selected.is_empty() {
                    this.selected.clear();
                    cx.notify();
                } else {
                    cx.propagate();
                }
            }))
            .child(bar)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .when(self.tool != Tool::Select, |d| d.cursor(CursorStyle::Crosshair))
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
                    .on_mouse_down(
                        MouseButton::Middle,
                        cx.listener(|this, e: &MouseDownEvent, _, _| this.drag = Drag::Pan { last: e.position }),
                    )
                    .on_mouse_move(cx.listener(Self::mouse_move))
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
                    .on_mouse_up(MouseButton::Middle, cx.listener(|this, _, _, _| this.drag = Drag::None))
                    .on_scroll_wheel(cx.listener(Self::scroll))
                    .child(drawing),
            )
    }
}

/// Couleur de texte du thème en `0xrrggbb`, pour le trait neutre d'un schéma
/// dessiné hors du canevas.
pub fn neutral(theme: Theme) -> u32 {
    let c = Rgba::from(theme.text);
    let byte = |v: f32| (v * 255.) as u32;
    byte(c.r) << 16 | byte(c.g) << 8 | byte(c.b)
}
