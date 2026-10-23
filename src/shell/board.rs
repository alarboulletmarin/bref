//! The kanban board of a note, drawn and edited in place of its text.

use gpui::{
    App, ClickEvent, Context, CursorStyle, DragMoveEvent, Focusable, Window, canvas, div, prelude::*, px,
};
use crate::{
    views::editor::{Redo, Undo},
    lang::tr,
};

use super::*;
use crate::views::kanban::*;

impl Shell {
    /// The open note is shown as a board: it is one, and its text was not asked for.
    pub(super) fn board_shown(&self, cx: &gpui::App) -> bool {
        self.pane().file.is_none() && self.pane().listing.is_none() && !self.pane().board.text && is_board(self.pane().editor.read(cx).text())
    }

    /// Switches from the board to the note's text, and back; the choice holds while it stays open.
    pub(super) fn toggle_board(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pane_mut().board.field = None;
        self.pane_mut().board.text = !self.pane_mut().board.text;
        if self.pane().board.text {
            window.focus(&self.pane().editor.focus_handle(cx));
        }
        cx.notify();
    }

    /// Applies a gesture made on the board to the note's text: it is undone like a keystroke.
    pub(super) fn board_edit(&mut self, change: impl Fn(&str) -> Option<String>, cx: &mut Context<Self>) {
        if let Some(text) = change(self.pane().editor.read(cx).text()) {
            self.pane().editor.update(cx, |e, cx| e.rewrite(&text, cx));
        }
    }

    /// Opens an input field in the board: a card or a title to rewrite, one more card or
    /// column. The one that was open is confirmed first.
    pub(super) fn board_write(&mut self, slot: Slot, window: &mut Window, cx: &mut Context<Self>) {
        self.board_commit(false, window, cx);
        let text = {
            let all = columns(self.pane().editor.read(cx).text());
            match slot {
                Slot::Card(c, i) => all.get(c).and_then(|column| column.cards.get(i)).map(|card| card.text.to_string()),
                Slot::Column(c) => all.get(c).map(|column| column.title.to_string()),
                Slot::New(_) | Slot::NewColumn => Some(String::new()),
            }
        };
        let Some(text) = text else {
            return;
        };
        let theme = self.theme;
        let field = cx.new(|cx| Field::new(&text, theme, cx));
        cx.subscribe_in(&field, window, |this, _, event: &FieldEvent, window, cx| match event {
            FieldEvent::Done => this.board_commit(true, window, cx),
            FieldEvent::Cancel => {
                this.pane_mut().board.field = None;
                window.focus(&this.pane().board.focus);
                cx.notify();
            }
        })
        .detach();
        window.focus(&field.focus_handle(cx));
        // The column being added is at the end of the board: it scrolls to it.
        if slot == Slot::NewColumn {
            self.pane().board.scroll.scroll_to_item(columns(self.pane().editor.read(cx).text()).len());
        }
        self.pane_mut().board.field = Some((slot, field));
        cx.notify();
    }

    /// Confirms what is written in the open field. After a card added with Enter
    /// (`again`), the field reopens for the next one: chain them without reaching for the mouse.
    pub(super) fn board_commit(&mut self, again: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some((slot, field)) = self.pane_mut().board.field.take() else {
            return;
        };
        let typed = field.read(cx).line.text.clone();
        let written = !typed.trim().is_empty();
        self.board_edit(
            |text| match slot {
                Slot::New(c) if written => add_card(text, c, &typed),
                Slot::Card(c, i) => retitle(text, (c, i), &typed),
                Slot::Column(c) => rename_column(text, c, &typed),
                Slot::NewColumn => add_column(text, &typed),
                Slot::New(_) => None,
            },
            cx,
        );
        match slot {
            Slot::New(c) if again && written => self.board_write(Slot::New(c), window, cx),
            _ => window.focus(&self.pane().board.focus),
        }
        cx.notify();
    }

    /// An arrow on the board: the keys go from spot to spot, and the board scrolls to it.
    fn board_walk(&mut self, by: (isize, isize), cx: &mut Context<Self>) {
        if self.pane().board.field.is_some() {
            return;
        }
        let counts: Vec<usize> = columns(self.pane().editor.read(cx).text()).iter().map(|c| c.cards.len()).collect();
        // The first arrow lands on the first card, or the first title.
        let at = match self.pane().board.at {
            Some(at) => walk(&counts, at, by),
            None => walk(&counts, (0, 1), (0, 0)),
        };
        self.pane().board.scroll.scroll_to_item(at.0);
        self.pane_mut().board.at = Some(at);
        cx.notify();
    }

    /// Alt+arrow: the card the keys are on goes along, and the keys with it.
    fn board_carry(&mut self, by: (isize, isize), cx: &mut Context<Self>) {
        let Some(at) = self.pane().board.at.filter(|_| self.pane().board.field.is_none()) else { return };
        if let Some((text, spot)) = carry(self.pane().editor.read(cx).text(), at, by) {
            self.pane().editor.update(cx, |e, cx| e.rewrite(&text, cx));
            self.pane().board.scroll.scroll_to_item(spot.0);
            self.pane_mut().board.at = Some(spot);
            cx.notify();
        }
    }

    /// What the spot the keys are on is: a title, a card, "Add a card", "Add a column".
    fn board_slot(&self, cx: &App) -> Option<Slot> {
        let (c, r) = self.pane().board.at.filter(|_| self.pane().board.field.is_none())?;
        let all = columns(self.pane().editor.read(cx).text());
        let Some(column) = all.get(c) else { return Some(Slot::NewColumn) };
        Some(match r {
            0 => Slot::Column(c),
            r if r <= column.cards.len() => Slot::Card(c, r - 1),
            _ => Slot::New(c),
        })
    }

    /// Delete: the card the keys are on, or the column whose title they are on, with its cards.
    /// Undone like any gesture of the board.
    fn board_remove(&mut self, cx: &mut Context<Self>) {
        match self.board_slot(cx) {
            Some(Slot::Card(c, i)) => self.board_edit(|text| retitle(text, (c, i), ""), cx),
            Some(Slot::Column(c)) => self.board_edit(|text| remove_column(text, c), cx),
            _ => return,
        }
        // The keys stay where they were, inside what is left.
        let counts: Vec<usize> = columns(self.pane().editor.read(cx).text()).iter().map(|c| c.cards.len()).collect();
        let at = self.pane().board.at.map(|at| walk(&counts, at, (0, 0)));
        self.pane_mut().board.at = at;
        cx.notify();
    }

    /// Space on a card: checks it, or unchecks it.
    fn board_tick(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(Slot::Card(c, i)) = self.board_slot(cx) else { return false };
        self.board_edit(|text| tick(text, (c, i)), cx);
        true
    }

    /// The board, in place of the note. A grabbed card leaves its place and follows the
    /// pointer; a dotted slot opens where it would land, between two cards or
    /// at the bottom of a column. A click on its checkbox ticks it, a double-click rewrites it in place
    /// (emptied, it is removed). "+" adds a card, or a column. The keys do the same: arrows go from
    /// spot to spot, Alt+arrows carry the card, Enter writes, Space ticks, Delete removes.
    pub(super) fn render_board(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        // The pane this board occupies: gestures starting from it, later, target it.
        let pane = self.active;
        let t = self.theme;
        let text = self.pane().editor.read(cx).text().to_string();
        let writing = self.pane().board.field.as_ref().map(|(slot, field)| (*slot, field.clone()));
        let field_at = |slot: Slot| writing.as_ref().filter(|(open, _)| *open == slot).map(|(_, field)| field.clone());
        let all = columns(&text);
        let drag = self.pane().board.drag.filter(|_| cx.has_active_drag());
        // The spot the keys are on: the same look as a chosen row elsewhere.
        let keys = self.pane().board.at;
        let lit = |spot: (usize, usize)| keys == Some(spot);
        // The slot the held card would take: its exact size, empty.
        let held = drag.and_then(|(from, _)| all.get(from.0)?.cards.get(from.1)).map(|card| (card.text.to_string(), card.done));
        let place = || {
            let (text, done) = held.as_ref()?;
            let empty = card_box(t).bg(t.accent.opacity(0.08)).border_dashed().border_color(t.accent.opacity(0.6));
            Some(empty.child(check_box(*done, t).invisible()).child(card_text(text, *done, t).invisible()).into_any_element())
        };
        /// A double-click opens the input field at this spot.
        fn twice(slot: Slot, cx: &mut Context<Shell>) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
            cx.listener(move |this, e: &ClickEvent, window, cx| {
                if e.click_count() >= 2 {
                    this.board_write(slot, window, cx)
                }
            })
        }
        let mut shown = Vec::new();
        for (c, column) in all.iter().enumerate() {
            let count = column.cards.len();
            let mut cards = Vec::new();
            for (i, card) in column.cards.iter().enumerate() {
                let from = (c, i);
                if drag.is_some_and(|(_, to)| to == from) {
                    cards.extend(place());
                }
                // The held card has left its column: it is under the pointer.
                if drag.is_some_and(|(held, _)| held == from) {
                    continue;
                }
                if let Some(field) = field_at(Slot::Card(c, i)) {
                    cards.push(card_box(t).border_color(t.accent).child(field).into_any_element());
                    continue;
                }
                let check = check_box(card.done, t).id(("tick", c * 10_000 + i)).cursor_pointer().on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.board_edit(|text| tick(text, from), cx)
                }));
                let card = card_box(t)
                    .id(("card", c * 10_000 + i))
                    .debug_selector(|| format!("card-{c}-{i}"))
                    .cursor_grab()
                    .hover(|s| s.border_color(t.dim))
                    .when(lit((c, i + 1)), |d| d.bg(t.selection).border_color(t.accent))
                    .child(check)
                    .child(card_text(card.text, card.done, t))
                    .on_click(twice(Slot::Card(c, i), cx))
                    .on_drag(Dragged { pane, from, text: card.text.to_string(), done: card.done, width: self.pane().board.card.get(), theme: t }, |dragged, _, _, cx| cx.new(|_| dragged.clone()))
                    .on_drag_move(aim(pane, c, i, true, cx))
                    .into_any_element();
                cards.push(card);
            }
            if drag.is_some_and(|(_, to)| to.0 == c && to.1 >= count) {
                cards.extend(place());
            }
            let title = match field_at(Slot::Column(c)) {
                Some(field) => div().flex_1().min_w_0().flex().child(field).into_any_element(),
                None => div()
                    .id(("column-title", c))
                    .debug_selector(move || format!("column-title-{c}"))
                    .flex_1()
                    .min_w_0()
                    .px_1()
                    .rounded(px(4.))
                    .when(lit((c, 0)), |d| d.bg(t.selection))
                    .truncate()
                    .font_weight(gpui::FontWeight::BOLD)
                    .child(column.title.to_string())
                    .on_click(twice(Slot::Column(c), cx))
                    .into_any_element(),
            };
            // At the bottom of the column: the card being written, otherwise a way to add one.
            let add = match field_at(Slot::New(c)) {
                Some(field) => card_box(t).border_color(t.accent).child(field).into_any_element(),
                None => div()
                    .id(("card-add", c))
                    .px_2()
                    .py_1()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .text_color(t.soft)
                    .hover(|s| s.bg(t.bg).text_color(t.text))
                    .when(lit((c, count + 1)), |d| d.bg(t.selection).text_color(t.text))
                    .child(format!("+ {}", tr("Add a card", "Ajouter une carte")))
                    .on_click(cx.listener(move |this, _, window, cx| this.board_write(Slot::New(c), window, cx)))
                    .into_any_element(),
            };
            let head = div().flex().items_center().gap_2().child(title).child(div().text_color(t.soft).child(count.to_string()));
            // The width of a card, measured when drawing: the held one keeps its own.
            let width = self.pane().board.card.clone();
            let measure = canvas(move |bounds, _, _| width.set(bounds.size.width), |_, _, _, _| ()).absolute().top_0().left_2().right_2().h_0();
            let column = div()
                .relative()
                .when(c == 0, |d| d.child(measure))
                .p_2()
                .rounded(px(8.))
                .bg(t.panel)
                .flex()
                .flex_col()
                .gap_2()
                // On the title, the held card aims at the top of the column; below the cards, the bottom.
                .child(head.on_drag_move(aim(pane, c, 0, false, cx)))
                .children(cards)
                .child(div().child(add).on_drag_move(aim(pane, c, count, false, cx)));
            // The column's lane goes all the way down: a card dropped under a short
            // column still lands there.
            let rest = div().debug_selector(|| format!("column-rest-{c}")).flex_1().min_h(px(48.)).on_drag_move(aim(pane, c, count, false, cx));
            shown.push(div().flex_1().min_w(px(NARROW)).max_w(px(COLUMN)).flex().flex_col().child(column).child(rest));
        }
        let more = match field_at(Slot::NewColumn) {
            Some(field) => div().flex_1().min_w(px(NARROW)).max_w(px(COLUMN)).h(px(36.)).p_2().rounded(px(8.)).bg(t.panel).border_1().border_color(t.accent).flex().child(field).into_any_element(),
            // No column at all: a lone « + » would float in the empty page, so the board says what it lacks.
            None if all.is_empty() => div()
                .debug_selector(|| "board-empty".into())
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_2()
                .text_center()
                .text_color(t.soft)
                .child(tr("This board has no column yet", "Ce tableau n'a pas encore de colonne"))
                .child(
                    div()
                        .id("column-add")
                        .px_3()
                        .py_1()
                        .rounded(px(6.))
                        .cursor_pointer()
                        .text_color(t.accent)
                        .hover(|s| s.bg(t.border))
                        .child(format!("+ {}", tr("Add a column", "Ajouter une colonne")))
                        .on_click(cx.listener(|this, _, window, cx| this.board_write(Slot::NewColumn, window, cx))),
                )
                .into_any_element(),
            None => crate::ui::widgets::button("column-add", "plus.svg", lit((all.len(), 0)), t)
                .flex_none()
                .on_click(cx.listener(|this, _, window, cx| this.board_write(Slot::NewColumn, window, cx)))
                .into_any_element(),
        };
        div()
            .id("board")
            .key_context("Board")
            .track_focus(&self.pane().board.focus)
            .on_action(cx.listener(|this, _: &Undo, _, cx| this.pane().editor.update(cx, |e, cx| e.restore(false, cx))))
            .on_action(cx.listener(|this, _: &Redo, _, cx| this.pane().editor.update(cx, |e, cx| e.restore(true, cx))))
            .on_action(cx.listener(|this, _: &CardUp, _, cx| this.board_walk((0, -1), cx)))
            .on_action(cx.listener(|this, _: &CardDown, _, cx| this.board_walk((0, 1), cx)))
            .on_action(cx.listener(|this, _: &CardLeft, _, cx| this.board_walk((-1, 0), cx)))
            .on_action(cx.listener(|this, _: &CardRight, _, cx| this.board_walk((1, 0), cx)))
            .on_action(cx.listener(|this, _: &CarryUp, _, cx| this.board_carry((0, -1), cx)))
            .on_action(cx.listener(|this, _: &CarryDown, _, cx| this.board_carry((0, 1), cx)))
            .on_action(cx.listener(|this, _: &CarryLeft, _, cx| this.board_carry((-1, 0), cx)))
            .on_action(cx.listener(|this, _: &CarryRight, _, cx| this.board_carry((1, 0), cx)))
            .on_action(cx.listener(|this, _: &CardEdit, window, cx| {
                if let Some(slot) = this.board_slot(cx) {
                    this.board_write(slot, window, cx)
                }
            }))
            .on_action(cx.listener(|this, _: &CardRemove, _, cx| this.board_remove(cx)))
            // Space is a key, not a shortcut: typed in a card being written, it is a space.
            .on_key_down(cx.listener(|this, e: &gpui::KeyDownEvent, window, cx| {
                if e.keystroke.key == "space" && !e.keystroke.modifiers.modified() && this.pane().board.focus.is_focused(window) && this.board_tick(cx) {
                    cx.stop_propagation();
                }
            }))
            // The held card is out of the board: its slot goes back where it came from, and dropping it
            // there moves nothing.
            .on_drag_move(cx.listener(move |this, e: &DragMoveEvent<Dragged>, _, cx| {
                let from = e.drag(cx).from;
                if e.drag(cx).pane != pane || pane >= this.panes.len() {
                    return;
                }
                if !e.bounds.contains(&e.event.position) && this.panes[pane].board.drag != Some((from, from)) {
                    this.panes[pane].board.drag = Some((from, from));
                    cx.notify();
                }
                // Held near the left or right edge, the board scrolls toward it, faster closer to it.
                // ponytail: it scrolls as the pointer moves; held still at the edge, it waits for the
                // next move. A timer would keep it going.
                let (x, edge) = (e.event.position.x, px(48.));
                let step = if x < e.bounds.left() + edge { (e.bounds.left() + edge - x).min(edge) } else if x > e.bounds.right() - edge { -(x - e.bounds.right() + edge).min(edge) } else { px(0.) };
                if step != px(0.) && e.bounds.contains(&e.event.position) {
                    let scroll = &this.panes[pane].board.scroll;
                    let max = scroll.max_offset().width;
                    let offset = scroll.offset();
                    scroll.set_offset(gpui::point((offset.x + step / 4.).clamp(-max, px(0.)), offset.y));
                    cx.notify();
                }
            }))
            .size_full()
            .pt(px(52.))
            .px_6()
            .pb_4()
            .text_size(px(13.))
            .flex()
            .gap_3()
            .overflow_scroll()
            .track_scroll(&self.pane().board.scroll)
            // A click elsewhere confirms what was being written; the keys' spot goes with the mouse.
            .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, _, window, cx| {
                this.pane_mut().board.at = None;
                this.board_commit(false, window, cx)
            }))
            // The dropped card, wherever on the board, goes to the slot opened for it.
            .on_drop(cx.listener(move |this, dragged: &Dragged, _, cx| {
                if dragged.pane == pane && pane < this.panes.len() {
                    this.in_pane(pane, |this| {
                        if let Some((from, to)) = this.pane_mut().board.drag.take() {
                            this.board_edit(|text| move_card(text, from, to), cx);
                        }
                    });
                }
                cx.notify();
            }))
            .children(shown)
            .child(more)
    }
}
/// While a card is held, the pointer over this zone says where it would land: before the
/// card of rank `i` in column `c`, or after it if it is in its lower half (`halves`).
pub(super) fn aim(pane: usize, c: usize, i: usize, halves: bool, cx: &mut Context<Shell>) -> impl Fn(&DragMoveEvent<Dragged>, &mut Window, &mut App) + 'static {
    cx.listener(move |this, e: &DragMoveEvent<Dragged>, window, cx| {
        let at = e.event.position;
        if !e.bounds.contains(&at) || e.drag(cx).pane != pane || pane >= this.panes.len() {
            return;
        }
        let below = halves && at.y > e.bounds.origin.y + e.bounds.size.height / 2.;
        let drag = Some((e.drag(cx).from, (c, i + below as usize)));
        if this.panes[pane].board.drag != drag {
            this.panes[pane].board.drag = drag;
            cx.set_active_drag_cursor_style(CursorStyle::ClosedHand, window);
            cx.notify();
        }
    })
}
