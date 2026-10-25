//! The one-line input field shared by the views: a card of the board, the help's search, a cell
//! and the filter of a list page, the find bar and the vault search. Its text is a `ui::line::Line`
//! (cursor, selection, word moves, clipboard), and an input method composes in it as in the note.

use std::{cell::Cell, ops::Range, rc::Rc};

use gpui::{
    App, Bounds, Context, ElementInputHandler, EntityInputHandler, EventEmitter, FocusHandle, Focusable, Pixels, Point,
    TextLayout, UTF16Selection, Window, canvas, div, prelude::*,
};

use crate::{
    ui::line::{self, Line},
    ui::theme::Theme,
    views::palette::{Confirm, Dismiss},
};

pub enum FieldEvent {
    /// Enter: the text is confirmed.
    Done,
    Cancel,
    /// The text changed: typed, erased, pasted, composed.
    Changed,
}

pub struct Field {
    focus: FocusHandle,
    pub line: Line,
    theme: Theme,
    /// The text as last drawn, to place the cursor on a click and the input method's window.
    layout: Option<TextLayout>,
    /// One line that scrolls to keep the cursor in sight (a search field), instead of wrapping (a card).
    scrolls: bool,
    /// How far it has scrolled, set after drawing.
    scroll: Rc<Cell<Pixels>>,
}

impl EventEmitter<FieldEvent> for Field {}

impl Field {
    pub fn new(text: &str, theme: Theme, cx: &mut Context<Self>) -> Self {
        Self { focus: cx.focus_handle(), line: Line::new(text), theme, layout: None, scrolls: false, scroll: Rc::default() }
    }

    /// One line that scrolls to keep the cursor in sight: a long search never makes its bar grow.
    pub fn scrolling(mut self) -> Self {
        self.scrolls = true;
        self
    }

    /// The text, replaced, all selected: the next keystroke replaces it.
    pub fn set(&mut self, text: &str, cx: &mut Context<Self>) {
        self.line = Line::new(text);
        self.line.select_all();
        cx.notify();
    }

    /// How far a scrolling field has moved its text, for a test.
    #[cfg(test)]
    pub fn scrolled(&self) -> Pixels {
        self.scroll.get()
    }

    fn changed(&mut self, cx: &mut Context<Self>) {
        cx.emit(FieldEvent::Changed);
        cx.notify();
    }
}

impl Focusable for Field {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl EntityInputHandler for Field {
    fn text_for_range(&mut self, range: Range<usize>, _: &mut Option<Range<usize>>, _: &mut Window, _: &mut Context<Self>) -> Option<String> {
        Some(self.line.text_utf16(range))
    }

    fn selected_text_range(&mut self, _: bool, _: &mut Window, _: &mut Context<Self>) -> Option<UTF16Selection> {
        Some(self.line.utf16())
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.line.marked_utf16()
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        // What was composed stays, as typed text.
        let text = self.line.marked_utf16().map(|r| self.line.text_utf16(r));
        if let Some(text) = text {
            self.line.commit(None, &text);
        }
    }

    fn replace_text_in_range(&mut self, range: Option<Range<usize>>, text: &str, _: &mut Window, cx: &mut Context<Self>) {
        self.line.commit(range, text);
        self.changed(cx);
    }

    fn replace_and_mark_text_in_range(&mut self, range: Option<Range<usize>>, text: &str, selected: Option<Range<usize>>, _: &mut Window, cx: &mut Context<Self>) {
        self.line.compose(range, text, selected);
        self.changed(cx);
    }

    fn bounds_for_range(&mut self, range: Range<usize>, _: Bounds<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<Bounds<Pixels>> {
        // The input method's window opens under the composed text.
        let layout = self.layout.as_ref()?;
        let at = layout.position_for_index(self.line.text_utf16(0..range.start).len())?;
        Some(Bounds::new(at, gpui::size(self.theme.px(2.), layout.line_height())))
    }

    fn character_index_for_point(&mut self, point: Point<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        let (Ok(at) | Err(at)) = self.layout.as_ref()?.index_for_position(point);
        Some(self.line.text[..at.min(self.line.text.len())].encode_utf16().count())
    }
}

impl Render for Field {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        let (focus, entity) = (self.focus.clone(), cx.entity());
        let input = canvas(|_, _, _| (), move |bounds, _, window, cx| window.handle_input(&focus, ElementInputHandler::new(bounds, entity), cx)).size_0();
        let (shown, layout) = self.line.shown(t);
        self.layout = Some(layout.clone());
        let placed = layout.clone();
        let field = div()
            // The palette's keys (Enter confirms, Escape gives up) and those of a line.
            .key_context("Palette Line")
            .track_focus(&self.focus)
            .on_action(cx.listener(|_, _: &Confirm, _, cx| cx.emit(FieldEvent::Done)))
            .on_action(cx.listener(|_, _: &Dismiss, _, cx| cx.emit(FieldEvent::Cancel)))
            // A click in the text puts the cursor there.
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |this, e: &gpui::MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    window.focus(&this.focus);
                    let (Ok(at) | Err(at)) = placed.index_for_position(e.position);
                    this.line.place(at);
                    cx.notify();
                }),
            );
        let field = line::keys(field, cx, |this| Some(&mut this.line), Self::changed).flex_1().min_w_0().cursor_text().child(input);
        if !self.scrolls {
            return field.child(shown);
        }
        // The cursor kept in sight: after drawing, the line moves by what it went out of the frame.
        let (scroll, at, room) = (self.scroll.clone(), self.line.cursor(), t.px(6.));
        let follow = canvas(|_, _, _| (), move |frame, _, window, _| {
            let Some(caret) = layout.position_for_index(at) else { return };
            let x = caret.x - layout.bounds().left();
            let was = scroll.get();
            let now = if x - was > frame.size.width - room { x - frame.size.width + room } else if x < was { x } else { was };
            if now != was {
                scroll.set(now.max(gpui::px(0.)));
                window.refresh();
            }
        });
        field.relative().overflow_hidden().whitespace_nowrap().child(shown.flex_none().ml(-self.scroll.get())).child(follow.absolute().inset_0())
    }
}
