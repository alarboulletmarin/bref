//! Kanban board: a note shown as columns. What a pane keeps of a board, the card being held and
//! the widgets of a card; reading and rewriting the Markdown is in `logic/kanban.rs`, and the
//! board is drawn by the shell (`shell/board.rs`).

pub use crate::logic::kanban::*;

gpui::actions!(kanban, [CardUp, CardDown, CardLeft, CardRight, CarryUp, CarryDown, CarryLeft, CarryRight, CardEdit, CardRemove]);

use gpui::{App, Context, Div, Entity, FocusHandle, Pixels, Window, div, prelude::*, px, svg};

use crate::{ui::theme::Theme, views::field::Field};

/// What a pane keeps of the board it shows.
pub struct Board {
        /// The open note is a kanban board whose text we are looking at: we asked for it, or we
        /// just wrote its key (typing `kanban: true` does not leave the text).
    pub text: bool,
        /// The note was a board at its last keystroke.
    pub was: bool,
        /// Card being moved in the board, and where it would land (column, rank).
    pub drag: Option<((usize, usize), (usize, usize))>,
        /// Width of a board card, measured when drawn.
    pub card: std::rc::Rc<std::cell::Cell<Pixels>>,
    pub scroll: gpui::ScrollHandle,
        /// What is being typed in the board, and its input field.
    pub field: Option<(Slot, Entity<Field>)>,
        /// The board receives input in place of the note it hides.
    pub focus: FocusHandle,
    /// The spot the keys are on (see `walk`); `None` until an arrow is pressed, and after a click.
    pub at: Option<(usize, usize)>,
}

impl Board {
    pub fn new(cx: &mut App) -> Self {
        Board { text: false, was: false, drag: None, card: Default::default(), scroll: Default::default(), field: None, focus: cx.focus_handle(), at: None }
    }
}

/// What is being typed in the board.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Slot {
        /// One more card at the bottom of this column.
    New(usize),
    Card(usize, usize),
    Column(usize),
        /// One more column.
    NewColumn,
}

/// Width of a column when the window has room; they shrink down to `NARROW` to
/// fit side by side, then the board scrolls.
pub const COLUMN: f32 = 250.;
pub const NARROW: f32 = 140.;

/// A card's frame, its checkbox and its text: the same for the placed card, the one being held
/// and the place it would take. The border is clear until the card is hovered, chosen or written in;
/// the placed card adds its own shadow.
pub fn card_box(t: Theme) -> Div {
    div().px(px(12.)).py(px(10.)).rounded(t.large()).bg(t.bg).border_1().border_color(gpui::transparent_black()).flex().items_start().gap(px(9.))
}

/// The round check box: a 1.5 px ring in `soft`, filled with `accent` and a check once done.
pub fn check_box(done: bool, t: Theme) -> Div {
    let mut ring = div().size(px(14.)).flex_none().mt(px(3.)).rounded_full().border_color(if done { t.accent } else { t.soft });
    let widths = &mut ring.style().border_widths;
    (widths.top, widths.right, widths.bottom, widths.left) = (Some(px(1.5).into()), Some(px(1.5).into()), Some(px(1.5).into()), Some(px(1.5).into()));
    ring.when(done, |d| d.bg(t.accent).flex().items_center().justify_center().child(svg().path("check.svg").size(px(10.)).text_color(t.bg)))
}

pub fn card_text(text: &str, done: bool, t: Theme) -> Div {
    div().flex_1().min_w_0().when(done, |d| d.text_color(t.dim).line_through()).child(text.to_string())
}

/// Card being held: the card itself, lifted, following the pointer from where it was grabbed.
#[derive(Clone)]
pub struct Dragged {
        /// The board pane the card comes from: another board ignores it.
    pub pane: usize,
    pub from: (usize, usize),
    pub text: String,
    pub done: bool,
        /// The card's width in its column.
    pub width: Pixels,
    pub theme: Theme,
}

impl Render for Dragged {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        card_box(t)
            .w(self.width)
            .border_color(t.accent)
            .shadow(t.floating())
            .text_size(px(13.))
            .text_color(t.text)
            .child(check_box(self.done, t))
            .child(card_text(&self.text, self.done, t))
    }
}

