//! Canvas: the diagram editor. Shapes and arrows are placed with the
//! mouse, moved, written in; tools are chosen with one
//! letter, as in Excalidraw. The drawing itself lives in `diagram`.

use std::ops::Range;

use gpui::{
    App, Bounds, ClipboardItem, ContentMask, Context, CursorStyle, ElementInputHandler, EntityInputHandler,
    EventEmitter, FocusHandle, Focusable, FontWeight, Hsla, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, PathBuilder, Pixels, Point, Rgba, ScrollWheelEvent, SharedString, TextRun, UTF16Selection,
    Window, actions, canvas, div, fill, font, point, prelude::*, px, quad, rgb, size,
};

use crate::{
    ui::theme::Theme,
    logic::diagram::{self, Cmd, Diagram, End, Form, Head, Route},
    ui::widgets::{button, tip},
    ui::assets::sans, lang::tr,
};

actions!(canvas, [ZoomIn, ZoomOut, ZoomFit, Erase, EraseNext, Duplicate, SelectAll, Undo, Redo, Confirm, Cancel, Left, Right, Up, Down, Paste]);

pub enum CanvasEvent {
    /// The diagram changed: it needs saving.
    Changed,
    /// A PNG image of the diagram is requested.
    Export,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tool {
    Select,
    Shape(Form),
    /// Line or arrow, depending on its head.
    Link(Head),
}

/// Bar tools: (tool, icon, letter).
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

/// Name of a bar tool, for its tooltip.
fn tool_label(tool: Tool) -> &'static str {
    match tool {
        Tool::Select => tr("Select", "Sélection"),
        Tool::Shape(Form::Rect) => "Rectangle",
        Tool::Shape(Form::Round) => tr("Rounded rectangle", "Rectangle arrondi"),
        Tool::Shape(Form::Ellipse) => "Ellipse",
        Tool::Shape(Form::Diamond) => tr("Diamond", "Losange"),
        Tool::Shape(Form::Cylinder) => tr("Cylinder (a database)", "Cylindre (une base de données)"),
        Tool::Shape(Form::Actor) => tr("Person (an actor)", "Personnage (un acteur)"),
        Tool::Shape(Form::Note) => "Note",
        Tool::Shape(Form::Text) => tr("Text", "Texte"),
        Tool::Link(Head::None) => tr("Line", "Trait"),
        Tool::Link(_) => tr("Arrow", "Flèche"),
    }
}

/// Shape dragged from the toolbar, drawn under the pointer up to the canvas.
#[derive(Clone)]
struct Placing(Form, &'static str, Theme);

impl Render for Placing {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        button("d-placing", self.1, true, self.2)
    }
}

/// Size of a shape placed with a click or dropped from the bar.
fn usual(form: Form) -> (f32, f32) {
    match form {
        Form::Text => (80., diagram::LINE),
        Form::Actor => (40., 80.),
        Form::Cylinder => (80., 100.),
        Form::Ellipse | Form::Diamond => (120., 80.),
        _ => (140., 60.),
    }
}

/// Mouse gesture in progress; points are in the diagram's coordinates.
#[derive(Clone, Copy, PartialEq)]
enum Drag {
    None,
    /// Moving the selection, from `last`; `begun` once it has moved.
    Move { last: (f32, f32), begun: bool },
    /// Dragged corner of a shape: the opposite corner, `fixed`, does not move.
    Resize { id: u32, fixed: (f32, f32) },
    Marquee { from: (f32, f32), to: (f32, f32) },
    Create { id: u32, from: (f32, f32) },
    /// End of an arrow (`to`: its head) being dragged.
    Tip { id: u32, to: bool },
    Pan { last: Point<Pixels> },
}

pub struct Canvas {
    focus: FocusHandle,
    theme: Theme,
    /// The app draws the window buttons: the toolbar leaves room for them.
    client: bool,
    diagram: Diagram,
    selected: Vec<u32>,
    tool: Tool,
    drag: Drag,
    /// Offset (px) of the diagram's origin in the view, and pixels per unit.
    pan: (f32, f32),
    zoom: f32,
    /// Fit the whole diagram on the next frame.
    fit: bool,
    /// Element whose text is being written, and cursor position (byte).
    editing: Option<(u32, usize)>,
    undo: Vec<Diagram>,
    redo: Vec<Diagram>,
    /// The pointer's shape, depending on what it hovers and what is dragged.
    hand: CursorStyle,
    bounds: Bounds<Pixels>,
    /// The PNG image was just saved: the button shows it for a moment.
    exported: bool,
    /// Path of the next arrows: the last one chosen.
    route: Route,
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
            client: false,
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
            hand: CursorStyle::Arrow,
            bounds: Bounds::default(),
            exported: false,
            route: Route::Elbow,
        }
    }

    pub fn diagram(&self) -> &Diagram {
        &self.diagram
    }

    /// The theme, as the window knows it.
    pub fn sync(&mut self, theme: Theme, client: bool) {
        (self.theme, self.client) = (theme, client);
    }

    pub fn exported(&mut self, cx: &mut Context<Self>) {
        self.exported = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(std::time::Duration::from_millis(1500)).await;
            this.update(cx, |this, cx| {
                this.exported = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Screen position of a diagram point.
    /// Number of selected shapes and arrows.
    #[cfg(test)]
    pub fn picked(&self) -> usize {
        self.selected.len()
    }

    pub fn spot(&self, x: f32, y: f32) -> Point<Pixels> {
        let o = self.bounds.origin;
        point(o.x + px(self.pan.0 + x * self.zoom), o.y + px(self.pan.1 + y * self.zoom))
    }

    fn world(&self, at: Point<Pixels>) -> (f32, f32) {
        let o = self.bounds.origin;
        ((f32::from(at.x - o.x) - self.pan.0) / self.zoom, (f32::from(at.y - o.y) - self.pan.1) / self.zoom)
    }

    /// Colour of an element: neutral takes the theme's text colour.
    fn ink(&self, color: u8) -> Hsla {
        match color {
            0 => self.theme.text,
            i => rgb(diagram::COLORS[i as usize % diagram::COLORS.len()]).into(),
        }
    }

    // ponytail: undo keeps whole copies of the diagram (100 max): a
    // diagram weighs a few KB. Switch to an operation log if they grow.
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
        self.hand = if tool == Tool::Select { CursorStyle::Arrow } else { CursorStyle::Crosshair };
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
    fn typed(&mut self, text: &str, cx: &mut Context<Self>) {
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

    /// Arrow keys: the cursor while writing, otherwise the selection, by one grid step.
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

    /// Moves the selection; an arrow carries its free ends.
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

    /// Applies a setting to the selected elements.
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

    /// Corners of the shape, in order: top-left, top-right, bottom-right, bottom-left.
    fn corners(shape: &diagram::Shape) -> [(f32, f32); 4] {
        let (x, y, x1, y1) = (shape.x, shape.y, shape.x + shape.w, shape.y + shape.h);
        [(x, y), (x1, y), (x1, y1), (x, y1)]
    }

    /// Arrow end for this point: attached to the shape found there, at the
    /// edge if close to it; otherwise the snapped point.
    fn end_at(&self, p: (f32, f32), not: Option<u32>) -> End {
        let shape = self.diagram.shape_at(p.0, p.1).filter(|id| Some(*id) != not).and_then(|id| self.diagram.shape(id));
        match shape {
            Some(shape) => shape.hook(p.0, p.1),
            None => End::Point(diagram::snap(p.0), diagram::snap(p.1)),
        }
    }

    /// Zooms around the centre of the view.
    fn zoom_by(&mut self, by: f32, cx: &mut Context<Self>) {
        let zoom = (self.zoom * by).clamp(0.2, 4.);
        let (mx, my) = (f32::from(self.bounds.size.width) / 2., f32::from(self.bounds.size.height) / 2.);
        let k = zoom / self.zoom;
        self.pan = (mx - (mx - self.pan.0) * k, my - (my - self.pan.1) * k);
        self.zoom = zoom;
        cx.notify();
    }

    fn zoom_fit(&mut self, cx: &mut Context<Self>) {
        self.fit = true;
        cx.notify();
    }

    /// Places a shape at its usual size, centred on this screen point.
    fn place(&mut self, form: Form, at: Point<Pixels>, cx: &mut Context<Self>) {
        self.stop_editing(cx);
        self.remember();
        let (p, (w, h)) = (self.world(at), usual(form));
        let id = self.diagram.add_shape(form, diagram::snap(p.0 - w / 2.), diagram::snap(p.1 - h / 2.), w, h);
        (self.selected, self.tool) = (vec![id], Tool::Select);
        if form == Form::Text {
            self.editing = Some((id, 0));
        }
        self.changed(cx);
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
                let id = self.diagram.add_link(from, End::Point(p.0, p.1), head, self.route);
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
                            self.start_editing(id, cx);
                            Drag::None
                        } else {
                            Drag::Move { last: p, begun: false }
                        }
                    }
                    // Double-click on empty space: a text, at that spot.
                    (None, None) if e.click_count >= 2 => {
                        self.remember();
                        let id = self.diagram.add_shape(Form::Text, diagram::snap(p.0), diagram::snap(p.1), 80., diagram::LINE);
                        (self.editing, self.selected) = (Some((id, 0)), vec![id]);
                        Drag::None
                    }
                    (None, None) => {
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

    /// What the pointer announces: a shape or an arrow can be grabbed (hand), a corner drags
    /// diagonally, an arrow end reattaches, the dragged view is held, a tool draws.
    fn hand_at(&self, at: Point<Pixels>) -> CursorStyle {
        match self.drag {
            Drag::Pan { .. } | Drag::Move { begun: true, .. } => return CursorStyle::ClosedHand,
            Drag::Resize { .. } | Drag::Tip { .. } | Drag::Create { .. } | Drag::Marquee { .. } => return self.hand,
            _ => {}
        }
        if self.tool != Tool::Select {
            return CursorStyle::Crosshair;
        }
        let p = self.world(at);
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
        match self.diagram.shape_at(p.0, p.1).or_else(|| self.diagram.link_at(p.0, p.1, near)) {
            Some(_) => CursorStyle::PointingHand,
            None => CursorStyle::Arrow,
        }
    }

    fn aim_hand(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        let hand = self.hand_at(at);
        if hand != self.hand {
            self.hand = hand;
            cx.notify();
        }
    }

    fn mouse_move(&mut self, e: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        let held = e.pressed_button;
        let p = self.world(e.position);
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

    fn mouse_up(&mut self, e: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        let drag = std::mem::replace(&mut self.drag, Drag::None);
        self.aim_hand(e.position, cx);
        match drag {
            Drag::Create { id, .. } => {
                let Some(shape) = self.diagram.shape_mut(id) else { return };
                // A simple click places the shape at its usual size.
                if shape.w < diagram::GRID || shape.h < diagram::GRID {
                    (shape.w, shape.h) = usual(shape.form);
                }
                let text = shape.form == Form::Text;
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

    /// The wheel moves the view; with Ctrl (Cmd), it zooms around the pointer.
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

    /// The shape being written in grows to hold its text.
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

        // Cursor of the text being written: (line, byte in the line).
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
            // Separator strokes are not text lines: the cursor's line
            // is found by its place in the text.
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
            let Some(drawn) = self.diagram.trace(link) else { continue };
            let ink = self.ink(link.color);
            stroke(window, &drawn.line, link.dashed, ink);
            for (path, full) in &drawn.heads {
                if *full {
                    flood(window, path, ink);
                }
                stroke(window, path, false, ink);
            }
            let lines: Vec<&str> = link.text.split('\n').collect();
            for (i, text) in lines.iter().enumerate() {
                let (x, y, centered) = Diagram::label_at(&drawn, i, lines.len());
                write(self, window, cx, x, y, text, centered, false, ink);
                if let Some((id, row, col)) = caret
                    && id == link.id
                    && row == i
                {
                    let left = if centered { x - self.measure(text, false, 1., window) / 2. } else { x };
                    caret_at = Some((left + self.measure(&text[..col], false, 1., window), y));
                }
            }
        }

        // Selection: a frame, and handles at the corners of a single shape.
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
        // While dragging an end over a shape: its attachment points.
        if let Drag::Tip { id, to } = self.drag
            && let Some(link) = self.diagram.link(id)
            && let Some(shape) = (if to { link.to } else { link.from }).shape().and_then(|id| self.diagram.shape(id))
        {
            frame(window, shape.x - pad, shape.y - pad, shape.x + shape.w + pad, shape.y + shape.h + pad, t.accent);
            for hook in shape.hooks() {
                let c = at(hook);
                let dot = Bounds::new(point(c.x - px(2.5), c.y - px(2.5)), size(px(5.), px(5.)));
                window.paint_quad(quad(dot, px(2.5), t.accent, px(0.), t.accent, gpui::BorderStyle::default()));
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
            // Word by word: the hint wraps in a narrow canvas.
            let room = bounds.size.width - px(32.);
            let mut lines: Vec<String> = Vec::new();
            for word in hint.split(' ') {
                let longer = lines.last().map(|last| format!("{last} {word}"));
                match longer.filter(|longer| self.line(longer, false, t.soft, 0.93, window).width <= room) {
                    Some(longer) => *lines.last_mut().unwrap() = longer,
                    None => lines.push(word.to_string()),
                }
            }
            let top = bounds.center().y - px(10.) * lines.len() as f32;
            for (i, text) in lines.iter().enumerate() {
                let line = self.line(text, false, t.soft, 0.93, window);
                let origin = point(bounds.center().x - line.width / 2., top + px(20.) * i as f32);
                line.paint(origin, px(20.), window, cx).ok();
            }
        }
    }
}

// ponytail: input as it comes, without IME composition (composed text is
// inserted as is) nor selection in the text; reuse the editor's handler
// if diagrams are written in Japanese or Chinese.
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

        // A tool is chosen with a click; a shape can also be dragged to the canvas.
        let tools = TOOLS.iter().map(|&(tool, icon, key)| {
            let tool_button = button(icon, icon, self.tool == tool, t).tooltip(tip(tool_label(tool), key.to_uppercase(), t)).on_click(cx.listener(move |this, _, _, cx| this.set_tool(tool, cx)));
            match tool {
                Tool::Shape(form) => tool_button.on_drag(Placing(form, icon, t), |placing, _, _, cx| cx.new(|_| placing.clone())),
                _ => tool_button,
            }
        });
        let percent = format!("{} %", (self.zoom * 100.).round());
        let zoom = div()
            .absolute()
            .bottom_3()
            .right_3()
            .p(px(2.))
            .flex()
            .items_center()
            .rounded(px(8.))
            .bg(t.panel)
            .border_1()
            .border_color(t.border)
            .occlude()
            .child(button("d-zoom-out", "minimize.svg", false, t).tooltip(tip(tr("Zoom out", "Dézoomer"), "-".into(), t)).on_click(cx.listener(|this, _, _, cx| this.zoom_by(0.8, cx))))
            .child(
                div()
                    .id("d-zoom-fit")
                    .w(px(52.))
                    .text_center()
                    .text_size(px(12.))
                    .text_color(t.soft)
                    .cursor_pointer()
                    .child(percent)
                    .on_click(cx.listener(|this, _, _, cx| this.zoom_fit(cx))),
            )
            .child(button("d-zoom-in", "plus.svg", false, t).tooltip(tip(tr("Zoom in", "Zoomer"), "+".into(), t)).on_click(cx.listener(|this, _, _, cx| this.zoom_by(1.25, cx))));
        // Selection settings: colour, fill, dashes, arrow heads.
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
        // In a narrow window, the bar wraps: no tool is out of reach.
        let bar = div()
            .flex_none()
            .min_h(px(44.))
            .pl_2()
            .pr(px(if self.client { 128. } else { 8. }))
            .py_1()
            .flex()
            .flex_wrap()
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
                d.child(button("d-route", "d-route.svg", false, t).tooltip(tip(tr("Route: elbowed, straight or curved", "Tracé : coudé, droit ou courbe"), String::new(), t)).on_click(cx.listener(|this, _, _, cx| {
                    // The chosen path also applies to arrows to come.
                    let first = this.selected.iter().find_map(|id| this.diagram.link(*id)).map(|l| l.route.next());
                    if let Some(route) = first {
                        this.route = route;
                        this.style(cx, move |_, link| {
                            if let Some(link) = link {
                                link.route = route;
                            }
                        });
                    }
                })))
                .child(toggle("d-head-start", "d-head-start.svg", cx, |_, link| {
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
            .child(button("d-export", if self.exported { "check.svg" } else { "export.svg" }, false, t).tooltip(tip(tr("Export as PNG", "Exporter en PNG"), String::new(), t)).on_click(cx.listener(|_, _, _, cx| cx.emit(CanvasEvent::Export))));

        div()
            .size_full()
            .flex()
            .flex_col()
            .key_context("Canvas")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &ZoomIn, _, cx| this.zoom_by(1.25, cx)))
            .on_action(cx.listener(|this, _: &ZoomOut, _, cx| this.zoom_by(0.8, cx)))
            .on_action(cx.listener(|this, _: &ZoomFit, _, cx| this.zoom_fit(cx)))
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
            // Enter: write in the selected element; while writing, new line.
            .on_action(cx.listener(|this, _: &Confirm, _, cx| match (this.editing, this.selected.as_slice()) {
                (Some(_), _) => this.typed("\n", cx),
                (None, &[id]) => this.start_editing(id, cx),
                _ => {}
            }))
            // Escape: finish writing, drop the tool, then the selection; otherwise the window decides.
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
                    .cursor(self.hand)
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
                    .on_mouse_down(
                        MouseButton::Middle,
                        cx.listener(|this, e: &MouseDownEvent, _, _| this.drag = Drag::Pan { last: e.position }),
                    )
                    .on_mouse_move(cx.listener(Self::mouse_move))
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
                    .on_mouse_up(MouseButton::Middle, cx.listener(|this, _, _, _| this.drag = Drag::None))
                    .on_scroll_wheel(cx.listener(Self::scroll))
                    .on_drop(cx.listener(|this, placing: &Placing, window, cx| {
                        this.place(placing.0, window.mouse_position(), cx)
                    }))
                    .relative()
                    .child(drawing)
                    .child(zoom),
            )
    }
}

/// Theme text colour as `0xrrggbb`, for the neutral stroke of a diagram
/// drawn outside the canvas.
pub fn neutral(theme: Theme) -> u32 {
    let c = Rgba::from(theme.text);
    let byte = |v: f32| (v * 255.) as u32;
    byte(c.r) << 16 | byte(c.g) << 8 | byte(c.b)
}
