//! The canvas as a view: its tool bar, its zoom buttons and its actions.

use super::*;

impl Render for Canvas {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (this, focus) = (cx.entity(), self.focus.clone());
        let drawing = canvas(
            |_, _, _| (),
            move |bounds, _, window, cx| {
                window.handle_input(&focus, ElementInputHandler::new(bounds, this.clone()), cx);
                window.with_content_mask(Some(ContentMask { bounds }), |window| {
                    this.update(cx, |canvas, cx| {
                        let before = canvas.bounds.size;
                        canvas.paint(bounds, window, cx);
                        // The bar over the selection is put by the size of the canvas: drawn again when it changed.
                        if before != bounds.size {
                            cx.notify();
                        }
                    })
                })
            },
        )
        .size_full();

        let (dock, cluster, zoom, drawer, bar, menu) = (self.bottom(cx), self.cluster(cx), self.zoom_box(cx), self.drawer(cx), self.selection_bar(cx), self.context_menu(cx));

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
            .on_action(cx.listener(|this, _: &NextText, _, cx| this.next_text(cx)))
            .on_action(cx.listener(|this, _: &Undo, _, cx| this.restore(false, cx)))
            .on_action(cx.listener(|this, _: &Redo, _, cx| this.restore(true, cx)))
            .on_action(cx.listener(|this, _: &Duplicate, _, cx| this.duplicate(cx)))
            .on_action(cx.listener(|this, _: &SelectAll, _, cx| this.select_all(cx)))
            .on_action(cx.listener(|this, _: &Paste, _, cx| {
                if let Some(text) = cx.read_from_clipboard().and_then(|item: ClipboardItem| item.text()) {
                    this.typed(&text, cx);
                }
            }))
            // Enter: write in the selected element; while writing, new line.
            .on_action(cx.listener(|this, _: &Confirm, _, cx| match (this.editing, this.selected.as_slice()) {
                // In the library: Enter picks the first shape that answers the search.
                (None, _) if this.library.is_some() => {
                    let query = this.library.take().unwrap_or_default();
                    if let Some(first) = library::presets().iter().position(|p| p.matches(&query)) {
                        this.choose_preset(first, cx);
                    }
                    cx.notify();
                }
                (Some(_), _) => this.typed("\n", cx),
                (None, &[id]) => this.start_editing(id, cx),
                _ => {}
            }))
            // Escape: finish writing, drop the tool, then the selection; otherwise the window decides.
            .on_action(cx.listener(|this, _: &Cancel, _, cx| {
                if this.editing.is_some() {
                    this.stop_editing(cx);
                } else if this.context.take().is_some() {
                    cx.notify();
                } else if this.library.is_some() {
                    this.library = None;
                    cx.notify();
                } else if this.relations || this.shapes {
                    (this.relations, this.shapes) = (false, false);
                    cx.notify();
                } else if this.tool != Tool::Select {
                    this.set_tool(Tool::Select, cx);
                } else if !this.selected.is_empty() {
                    this.selected.clear();
                    cx.notify();
                } else {
                    cx.propagate();
                }
            }))
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
                    .on_mouse_down(MouseButton::Right, cx.listener(Self::context_down))
                    .on_mouse_move(cx.listener(Self::mouse_move))
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
                    .on_mouse_up(MouseButton::Middle, cx.listener(|this, _, _, _| this.drag = Drag::None))
                    .on_scroll_wheel(cx.listener(Self::scroll))
                    .on_drop(cx.listener(|this, placing: &Placing, window, cx| {
                        this.place(placing.0, window.mouse_position(), cx)
                    }))
                    .relative()
                    .child(drawing)
                    .child(cluster)
                    .children(drawer)
                    .children(bar)
                    .child(zoom)
                    .child(dock)
                    .children(menu),
            )
    }
}
