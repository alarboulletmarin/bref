//! Canvas: the diagram editor. Shapes and arrows are placed with the
//! mouse, moved, written in; tools are chosen with one
//! letter, as in Excalidraw. The drawing itself lives in `diagram`.

mod edit;
mod mouse;
mod paint;
mod render;

use std::ops::Range;

use gpui::{
    App, Bounds, ClipboardItem, ContentMask, Context, CursorStyle, ElementInputHandler, EntityInputHandler,
    EventEmitter, FocusHandle, Focusable, FontStyle, FontWeight, Hsla, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, PathBuilder, Pixels, Point, Rgba, ScrollWheelEvent, SharedString, StrikethroughStyle, TextRun, UTF16Selection, UnderlineStyle,
    Window, actions, canvas, div, fill, font, point, prelude::*, px, quad, rgb, size,
};

use crate::{
    ui::theme::Theme,
    logic::diagram::{self, Cmd, Diagram, End, Form, Head, Route},
    logic::richtext::{self, Span},
    ui::widgets::{button, tip},
    ui::assets::{mono, sans}, lang::tr,
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

    /// Makes this diagram read the marks of notes in its texts (`*italic*`): one step of undo.
    pub fn enrich(&mut self, cx: &mut Context<Self>) {
        self.remember();
        self.diagram.rich = true;
        self.changed(cx);
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


/// Theme text colour as `0xrrggbb`, for the neutral stroke of a diagram
/// drawn outside the canvas.
pub fn neutral(theme: Theme) -> u32 {
    let c = Rgba::from(theme.text);
    let byte = |v: f32| (v * 255.) as u32;
    byte(c.r) << 16 | byte(c.g) << 8 | byte(c.b)
}

