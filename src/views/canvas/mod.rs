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
    Window, actions, canvas, div, fill, font, point, prelude::*, px, quad, rgb, size, svg,
};

use crate::{
    ui::theme::Theme,
    logic::diagram::{self, Anchor, Cmd, Diagram, End, Form, Route, Slot},
    logic::library::{self, Preset},
    logic::relation::Relation,
    logic::richtext::{self, Span},
    ui::widgets::{button, tip},
    ui::assets::{mono, sans}, lang::tr,
};

actions!(canvas, [ZoomIn, ZoomOut, ZoomFit, Erase, EraseNext, Duplicate, SelectAll, Undo, Redo, Confirm, Cancel, Left, Right, Up, Down, Paste, NextText]);

pub enum CanvasEvent {
    /// The diagram changed: it needs saving.
    Changed,
    /// A PNG image of the diagram is requested.
    Export,
    /// A new diagram that starts from the library's template of this number.
    Template(usize),
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tool {
    Select,
    Shape(Form),
    /// An arrow of one of the relationships: a line, an arrow, a generalization…
    Link(Relation),
    /// A shape of the library, with its text and size (the number in `library::presets`).
    Preset(usize),
}

/// Bar tools: (tool, icon, letter).
const TOOLS: [(Tool, &str, &str); 14] = [
    (Tool::Select, "d-select.svg", "v"),
    (Tool::Shape(Form::Rect), "d-rect.svg", "r"),
    (Tool::Shape(Form::Round), "d-round.svg", "u"),
    (Tool::Shape(Form::Ellipse), "d-ellipse.svg", "o"),
    (Tool::Shape(Form::Diamond), "d-diamond.svg", "d"),
    (Tool::Shape(Form::Cylinder), "d-cylinder.svg", "c"),
    (Tool::Shape(Form::Actor), "d-actor.svg", "p"),
    (Tool::Shape(Form::Note), "d-note.svg", "n"),
    (Tool::Shape(Form::Text), "d-text.svg", "t"),
    (Tool::Shape(Form::Package), "d-package.svg", "g"),
    (Tool::Shape(Form::Frame), "d-frame.svg", "f"),
    (Tool::Shape(Form::Lane), "d-lane.svg", "w"),
    (Tool::Link(Relation::Navigable), "d-arrow.svg", "a"),
    (Tool::Link(Relation::Association), "d-line.svg", "l"),
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
        Tool::Shape(Form::Package) => tr("Package (holds shapes)", "Paquetage (contient des formes)"),
        Tool::Shape(Form::Frame) => tr("Frame: a system boundary, a composite state (holds shapes)", "Cadre : frontière d'un système, état composite (contient des formes)"),
        Tool::Shape(Form::Lane) => tr("Swimlane (holds shapes)", "Couloir (contient des formes)"),
        Tool::Shape(Form::Branch) => tr("N-ary association (a small diamond)", "Association n-aire (un petit losange)"),
        // The shapes of state machines and the like come from the library, by their preset.
        Tool::Shape(_) => "",
        Tool::Link(Relation::Association) => tr("Line", "Trait"),
        Tool::Link(Relation::Navigable) => tr("Arrow", "Flèche"),
        Tool::Link(relation) => relation.name(),
        Tool::Preset(i) => library::presets().get(i).map_or("", |p| p.name),
    }
}

/// Shape dragged from the toolbar or the library, drawn under the pointer up to the canvas; the last field is
/// where the pointer took hold of the row or the button it comes from.
#[derive(Clone)]
struct Placing(Tool, &'static str, Theme, Point<Pixels>);

/// Side of the button that is drawn under the pointer.
const GHOST: f32 = 28.;

impl Placing {
    /// The same, taken hold of at `at` in the element it is dragged from.
    fn grabbed(&self, at: Point<Pixels>) -> Self {
        Self(self.0, self.1, self.2, at)
    }

    /// How far the picture is moved so that it is centred on the pointer: gpui draws a dragged view at
    /// the pointer minus the place it was taken hold of at, which is far from the pointer for a wide row.
    fn shift(&self) -> (f32, f32) {
        (f32::from(self.3.x) - GHOST / 2., f32::from(self.3.y) - GHOST / 2.)
    }
}

impl Render for Placing {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let (dx, dy) = self.shift();
        div().ml(px(dx)).mt(px(dy)).child(button("d-placing", self.1, true, self.2))
    }
}

/// Size of a shape placed with a click or dropped from the bar or the library.
fn usual(tool: Tool) -> (f32, f32) {
    if let Tool::Preset(i) = tool {
        return library::presets().get(i).map_or((140., 60.), |p| (p.w, p.h));
    }
    let Tool::Shape(form) = tool else { return (140., 60.) };
    match form {
        Form::Text => (80., diagram::LINE),
        Form::Actor => (40., 80.),
        Form::Cylinder => (80., 100.),
        Form::Branch => (40., 40.),
        Form::Package => (220., 150.),
        Form::Frame => (260., 170.),
        Form::Lane => (160., 320.),
        Form::Ellipse | Form::Diamond => (120., 80.),
        _ => (140., 60.),
    }
}

/// The form a shape tool draws, and what is written in it to begin with.
fn made_by(tool: Tool) -> Option<(Form, Option<Preset>)> {
    match tool {
        Tool::Shape(form) => Some((form, None)),
        Tool::Preset(i) => library::presets().into_iter().nth(i).map(|p| (p.form, Some(p))),
        _ => None,
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
    /// A message of a sequence diagram dragged up or down: it takes the place among the others its height gives it.
    Reorder { id: u32, begun: bool },
    /// Text at an end of an arrow being dragged along its stroke; `begun` once it has moved.
    Along { id: u32, slot: Slot, begun: bool },
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
    /// Which of an arrow's three texts is being written.
    slot: Slot,
    /// The menu of relationships is open: a letter picks one.
    relations: bool,
    /// The library is open, with what is typed to search it.
    library: Option<String>,
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
            slot: Slot::Middle,
            relations: false,
            library: None,
            undo: Vec::new(),
            redo: Vec::new(),
            hand: CursorStyle::Arrow,
            bounds: Bounds::default(),
            exported: false,
            route: Route::Elbow,
        }
    }

    /// The tool picked.
    #[cfg(test)]
    pub fn tool(&self) -> Tool {
        self.tool
    }

    /// What is typed in the library, if it is open.
    #[cfg(test)]
    pub fn library_query(&self) -> Option<&str> {
        self.library.as_deref()
    }

    /// Drops the library's shape number `preset` at this screen point, as dragging it there does.
    #[cfg(test)]
    pub fn drop_preset(&mut self, preset: usize, at: Point<Pixels>, cx: &mut Context<Self>) {
        self.place(Tool::Preset(preset), at, cx);
    }

    /// Asks for a new diagram that starts from the library's template of this number.
    #[cfg(test)]
    pub fn start_template(&mut self, template: usize, cx: &mut Context<Self>) {
        cx.emit(CanvasEvent::Template(template));
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

    /// The selected shapes and arrows.
    #[cfg(test)]
    pub fn selected_ids(&self) -> Vec<u32> {
        self.selected.clone()
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
        // The lifelines of a sequence diagram follow their messages.
        self.diagram.fit_sequence();
        // A pin stays on the border of the shape it is on, when that was moved or resized.
        self.diagram.seat_pins();
        cx.emit(CanvasEvent::Changed);
        cx.notify();
    }

    fn restore(&mut self, redo: bool, cx: &mut Context<Self>) {
        let (from, to) = if redo { (&mut self.redo, &mut self.undo) } else { (&mut self.undo, &mut self.redo) };
        if let Some(diagram) = from.pop() {
            to.push(std::mem::replace(&mut self.diagram, diagram));
            (self.editing, self.drag, self.slot) = (None, Drag::None, Slot::Middle);
            self.selected.clear();
            self.changed(cx);
        }
    }

    /// The key that opens the menu of relationships, and the one that picks the n-ary association in it.
    const RELATIONS: &str = "k";
    const NARY: &str = "y";
    /// The key that opens the library.
    const LIBRARY: &str = "s";

    /// A relationship picked in the menu: it changes the selected arrows, or else it is the tool that draws the next ones.
    /// Opens or closes the library.
    pub(super) fn toggle_library(&mut self, cx: &mut Context<Self>) {
        self.library = if self.library.is_some() { None } else { Some(String::new()) };
        self.relations = false;
        cx.notify();
    }

    /// A shape of the library picked: it is the tool that draws the next one; a combined fragment, with
    /// messages selected, wraps them instead.
    pub(super) fn choose_preset(&mut self, preset: usize, cx: &mut Context<Self>) {
        let wrapper = library::presets().into_iter().nth(preset).filter(|p| p.form == Form::Fragment);
        let messages = self.diagram.messages();
        let ids: Vec<u32> = self.selected.iter().copied().filter(|id| messages.contains(id)).collect();
        if let (Some(wrapper), false, true) = (wrapper, ids.is_empty(), self.editing.is_none() && self.tool == Tool::Select) {
            let mut next = self.diagram.clone();
            if let Some(id) = next.wrap(&ids, wrapper.text) {
                self.remember();
                (self.diagram, self.selected) = (next, vec![id]);
                return self.changed(cx);
            }
            // Across the border of another fragment: nothing to wrap, the fragment is drawn by hand.
        }
        self.set_tool(Tool::Preset(preset), cx);
    }

    pub(super) fn choose_relation(&mut self, relation: Relation, cx: &mut Context<Self>) {
        self.relations = false;
        let links: Vec<u32> = self.selected.iter().copied().filter(|id| self.diagram.link(*id).is_some()).collect();
        if self.editing.is_none() && self.tool == Tool::Select && !links.is_empty() {
            self.remember();
            for id in links {
                if let Some(link) = self.diagram.link_mut(id) {
                    relation.apply(link);
                }
            }
            self.changed(cx);
        } else {
            self.set_tool(Tool::Link(relation), cx);
        }
    }

    fn set_tool(&mut self, tool: Tool, cx: &mut Context<Self>) {
        self.stop_editing(cx);
        self.relations = false;
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


#[cfg(test)]
mod ghost_tests {
    use super::*;

    #[test]
    fn the_picture_dragged_is_centred_on_the_pointer_wherever_it_was_taken_hold_of() {
        // gpui draws it at pointer - grab; moved by `shift`, its centre is the pointer, for a button and for a wide row.
        for grab in [(0., 0.), (14., 14.), (3., 25.), (240., 12.), (600., 30.), (-4., -2.)] {
            let placing = Placing(Tool::Select, "d-rect.svg", crate::ui::theme::Theme::of(&crate::logic::prefs::Prefs::parse("theme=Bref Light\n"), gpui::WindowAppearance::Light), point(px(grab.0), px(grab.1)));
            let (dx, dy) = placing.shift();
            let (left, top) = (-grab.0 + dx, -grab.1 + dy);
            assert_eq!((left + GHOST / 2., top + GHOST / 2.), (0., 0.), "{grab:?}");
        }
    }
}
