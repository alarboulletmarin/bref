//! The editor drawn as a gpui view: the page, its buttons and its popups.

use super::*;

impl Render for Editor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Left of the buttons the window puts in the corner (copy, comments).
        // The count is truncated in a narrow note, instead of going out on the left.
        let t = self.theme;
        let after = (12. + 34. * self.corner as f32) * t.ui;
        // The formatting bar, at the left of the strip, the counter at the right.
        self.bar.at = self.bar_place();
        let folded = self.bar.at == Some(Bar::Compact);
        let bar = match self.bar.at {
            Some(Bar::Inline) => self.bar.at.map(|mode| self.format_bar(mode, cx).into_any_element()),
            Some(Bar::Compact) if self.bar.open => Some(crate::ui::widgets::fade_in(self.format_bar(Bar::Compact, cx), "bar-in", &t)),
            _ => None,
        };
        let toggle = folded.then(|| self.bar_toggle(cx));
        // Collapsed, the bar leaves room for its button: the counter is truncated before going under it.
        let counts = div().absolute().bottom_4().left(t.px(if folded { 46. } else { 8. })).right(px(after)).flex().justify_end().child(
            div()
                .debug_selector(|| "counts".into())
                .h(t.px(30.))
                .min_w_0()
                .px_2()
                .rounded(t.large())
                .flex()
                .items_center()
                .bg(self.theme.bg)
                .text_size(t.px(12.))
                .text_color(self.theme.soft)
                .child(div().min_w_0().truncate().child(self.counts())),
        );
        // The text scrolls behind this band: it fades into the page, then hides what lies under
        // the counter and the bar.
        let t = self.theme;
        let band = div()
            .debug_selector(|| "note-band".into())
            .absolute()
            .left_0()
            .right_0()
            .bottom_0()
            .h(FOOTER)
            .bg(gpui::linear_gradient(0., gpui::linear_color_stop(t.bg, 0.7), gpui::linear_color_stop(t.bg.opacity(0.), 1.)));
        // Before drawing: matches follow a note changed while the bar is open.
        self.refresh_find(cx);
        let find_bar = self.find_bar(window, cx);
        macro_rules! motions {
            ($el:expr, $($action:ident => $motion:ident, $select:expr;)*) => {
                $el$(.on_action(cx.listener(|this, _: &$action, _, cx| {
                    this.each(cx, |this, cx| this.go(Motion::$motion, $select, cx))
                })))*
            };
        }
        let el = div()
            .size_full()
            .relative()
            .key_context("Editor")
            .track_focus(&self.focus)
            .cursor(CursorStyle::IBeam);
        let el = motions!(el,
            Left => Left, false; Right => Right, false; Up => Up, false; Down => Down, false;
            WordLeft => WordLeft, false; WordRight => WordRight, false;
            Home => Home, false; End => End, false;
            DocStart => DocStart, false; DocEnd => DocEnd, false;
            PageUp => PageUp, false; PageDown => PageDown, false;
            SelectLeft => Left, true; SelectRight => Right, true;
            SelectUp => Up, true; SelectDown => Down, true;
            SelectWordLeft => WordLeft, true; SelectWordRight => WordRight, true;
            SelectHome => Home, true; SelectEnd => End, true;
            SelectDocStart => DocStart, true; SelectDocEnd => DocEnd, true;
        )
        .on_action(cx.listener(|this, _: &Backspace, _, cx| this.each(cx, |this, cx| this.delete(Motion::Left, cx))))
        .on_action(cx.listener(|this, _: &Delete, _, cx| this.each(cx, |this, cx| this.delete(Motion::Right, cx))))
        .on_action(cx.listener(|this, _: &DeleteWordLeft, _, cx| this.each(cx, |this, cx| this.delete(Motion::WordLeft, cx))))
        .on_action(cx.listener(|this, _: &DeleteWordRight, _, cx| this.each(cx, |this, cx| this.delete(Motion::WordRight, cx))))
        .on_action(cx.listener(|this, _: &SelectNext, _, cx| this.select_next(cx)))
        .on_action(cx.listener(|this, _: &SelectAll, _, cx| {
            this.more.clear();
            this.move_to(0, cx);
            this.select_to(this.content.len(), cx);
        }))
        .on_action(cx.listener(|this, _: &Newline, _, cx| this.each(cx, |this, cx| this.newline(cx))))
        .on_action(cx.listener(|this, _: &Indent, _, cx| this.shift_lines(true, cx)))
        .on_action(cx.listener(|this, _: &Outdent, _, cx| this.shift_lines(false, cx)))
        .on_action(cx.listener(|this, _: &ToggleTask, _, cx| this.toggle_task(cx)))
        .on_action(cx.listener(|this, _: &ListBullet, _, cx| this.restyle(ListStyle::Bullet, cx)))
        .on_action(cx.listener(|this, _: &ListOrdered, _, cx| this.restyle(ListStyle::Ordered, cx)))
        .on_action(cx.listener(|this, _: &ListTask, _, cx| this.restyle(ListStyle::Task, cx)))
        .on_action(cx.listener(|this, _: &ListText, _, cx| this.restyle(ListStyle::Text, cx)))
        .on_action(cx.listener(|this, _: &ListCycle, _, cx| this.cycle_list(cx)))
        .on_action(cx.listener(|this, _: &TableRowAbove, _, cx| this.table_op(TableOp::RowAbove, cx)))
        .on_action(cx.listener(|this, _: &TableRowBelow, _, cx| this.table_op(TableOp::RowBelow, cx)))
        .on_action(cx.listener(|this, _: &TableColLeft, _, cx| this.table_op(TableOp::ColLeft, cx)))
        .on_action(cx.listener(|this, _: &TableColRight, _, cx| this.table_op(TableOp::ColRight, cx)))
        .on_action(cx.listener(|this, _: &TableDuplicateRow, _, cx| this.table_op(TableOp::DuplicateRow, cx)))
        .on_action(cx.listener(|this, _: &TableDuplicateCol, _, cx| this.table_op(TableOp::DuplicateCol, cx)))
        .on_action(cx.listener(|this, _: &TableMoveRowUp, _, cx| this.table_op(TableOp::MoveRowUp, cx)))
        .on_action(cx.listener(|this, _: &TableMoveRowDown, _, cx| this.table_op(TableOp::MoveRowDown, cx)))
        .on_action(cx.listener(|this, _: &TableMoveColLeft, _, cx| this.table_op(TableOp::MoveColLeft, cx)))
        .on_action(cx.listener(|this, _: &TableMoveColRight, _, cx| this.table_op(TableOp::MoveColRight, cx)))
        .on_action(cx.listener(|this, _: &TableDeleteRow, _, cx| this.table_op(TableOp::DeleteRow, cx)))
        .on_action(cx.listener(|this, _: &TableDeleteSpan, _, cx| this.table_delete_span(cx)))
        .on_action(cx.listener(|this, _: &BlockText, _, cx| this.block_op(BlockOp::Turn(block::Turn::Text), cx)))
        .on_action(cx.listener(|this, _: &BlockHeading1, _, cx| this.block_op(BlockOp::Turn(block::Turn::Heading(1)), cx)))
        .on_action(cx.listener(|this, _: &BlockHeading2, _, cx| this.block_op(BlockOp::Turn(block::Turn::Heading(2)), cx)))
        .on_action(cx.listener(|this, _: &BlockHeading3, _, cx| this.block_op(BlockOp::Turn(block::Turn::Heading(3)), cx)))
        .on_action(cx.listener(|this, _: &BlockBullet, _, cx| this.block_op(BlockOp::Turn(block::Turn::Bullet), cx)))
        .on_action(cx.listener(|this, _: &BlockOrdered, _, cx| this.block_op(BlockOp::Turn(block::Turn::Ordered), cx)))
        .on_action(cx.listener(|this, _: &BlockTask, _, cx| this.block_op(BlockOp::Turn(block::Turn::Task), cx)))
        .on_action(cx.listener(|this, _: &BlockQuote, _, cx| this.block_op(BlockOp::Turn(block::Turn::Quote), cx)))
        .on_action(cx.listener(|this, _: &BlockCode, _, cx| this.block_op(BlockOp::Turn(block::Turn::Code), cx)))
        .on_action(cx.listener(|this, _: &BlockDuplicate, _, cx| this.block_op(BlockOp::Duplicate, cx)))
        .on_action(cx.listener(|this, _: &BlockUp, _, cx| this.block_op(BlockOp::Up, cx)))
        .on_action(cx.listener(|this, _: &BlockDown, _, cx| this.block_op(BlockOp::Down, cx)))
        .on_action(cx.listener(|this, _: &BlockDelete, _, cx| this.block_op(BlockOp::Delete, cx)))
        .on_action(cx.listener(|this, _: &TableDeleteCol, _, cx| this.table_op(TableOp::DeleteCol, cx)))
        .on_action(cx.listener(|this, _: &TableDelete, _, cx| this.table_op(TableOp::DeleteTable, cx)))
        .on_action(cx.listener(|this, _: &TableSortAsc, _, cx| this.table_op(TableOp::SortAsc, cx)))
        .on_action(cx.listener(|this, _: &TableSortDesc, _, cx| this.table_op(TableOp::SortDesc, cx)))
        .on_action(cx.listener(|this, _: &TableTranspose, _, cx| this.table_op(TableOp::Transpose, cx)))
        .on_action(cx.listener(|this, _: &OpenEmbed, _, cx| this.open_embed_here(cx)))
        .on_action(cx.listener(|this, _: &TableAutoSum, _, cx| this.auto_sum(cx)))
        .on_action(cx.listener(|this, _: &TableCopyResults, _, cx| this.copy_results(cx)))
        .on_action(cx.listener(|this, _: &AlignLeft, _, cx| this.align(":--", cx)))
        .on_action(cx.listener(|this, _: &AlignCenter, _, cx| this.align(":-:", cx)))
        .on_action(cx.listener(|this, _: &AlignRight, _, cx| this.align("--:", cx)))
        .on_action(cx.listener(|this, _: &Bold, _, cx| this.each(cx, |this, cx| this.mark(md::BOLD, cx))))
        .on_action(cx.listener(|this, _: &Italic, _, cx| this.each(cx, |this, cx| this.mark(md::ITALIC, cx))))
        .on_action(cx.listener(|this, _: &Underline, _, cx| this.each(cx, |this, cx| this.mark(md::UNDER, cx))))
        .on_action(cx.listener(|this, _: &Strike, _, cx| this.each(cx, |this, cx| this.mark(md::STRIKE, cx))))
        .on_action(cx.listener(|this, _: &Highlight, _, cx| this.each(cx, |this, cx| this.mark(md::HIGHLIGHT, cx))))
        .on_action(cx.listener(|this, _: &InlineCode, _, cx| this.each(cx, |this, cx| this.mark(md::CODE, cx))))
        .on_action(cx.listener(|this, _: &InsertLink, _, cx| this.link(cx)))
        .on_action(cx.listener(|this, _: &Comment, _, cx| this.comment(cx)))
        .on_action(cx.listener(|this, _: &Copy, _, cx| this.copy(false, cx)))
        .on_action(cx.listener(|this, _: &Cut, _, cx| this.copy(true, cx)))
        .on_action(cx.listener(|this, _: &Paste, _, cx| this.each(cx, |this, cx| this.paste(cx))))
        .on_action(cx.listener(|this, _: &Undo, _, cx| this.restore(false, cx)))
        .on_action(cx.listener(|this, _: &Redo, _, cx| this.restore(true, cx)))
        // The bar's keys that also work from the note while it is open; its others are on the bar.
        .on_action(cx.listener(|this, _: &Find, window, cx| this.open_find(false, window, cx)))
        .on_action(cx.listener(|this, _: &FindReplace, window, cx| this.open_find(true, window, cx)))
        .on_action(cx.listener(|this, _: &FindNext, window, cx| this.find_step(true, window, cx)))
        .on_action(cx.listener(|this, _: &FindPrev, window, cx| this.find_step(false, window, cx)))
        .on_action(cx.listener(|this, _: &FindCase, _, cx| this.find_flip(0, cx)))
        .on_action(cx.listener(|this, _: &FindWord, _, cx| this.find_flip(1, cx)))
        .on_action(cx.listener(|this, _: &FindRegex, _, cx| this.find_flip(2, cx)))
        .on_action(cx.listener(|this, _: &Cancel, _, cx| {
            // The color badges close first.
            if std::mem::take(&mut this.bar.swatches) || std::mem::take(&mut this.bar.open) {
                return cx.notify();
            }
            // Bar open but typing in the note: Escape closes it first, keeping its search.
            if let Some(find) = this.find.take() {
                this.found = Some(find);
                return cx.notify();
            }
            // Several cursors: Escape goes back to one.
            if !this.more.is_empty() {
                this.more.clear();
                return cx.notify();
            }
            // A dragged handle is let go without moving anything; what it had grabbed is released.
            if this.moving.take().is_some() {
                return cx.notify();
            }
            if this.pick.as_ref().is_some_and(|p| p.version == this.version && p.sel == this.sel) {
                this.pick = None;
                return cx.notify();
            }
            if this.grid.take().is_some() {
                cx.notify();
            } else if let Some((start, _)) = this.completion() {
                this.ac_dismissed = Some(start);
                cx.notify();
            } else {
                // Nothing to close here: up to the window (its menu).
                cx.propagate();
            }
        }))
        .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
        .on_mouse_down(MouseButton::Right, cx.listener(Self::menu_down))
        .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
        .on_mouse_up_out(MouseButton::Left, cx.listener(Self::mouse_up))
        .on_mouse_move(cx.listener(Self::mouse_move))
        .on_modifiers_changed(cx.listener(|this, e: &gpui::ModifiersChangedEvent, _, cx| this.aim_hand(this.pointer, e.modifiers, cx)))
        .on_scroll_wheel(cx.listener(Self::scroll))
        .child(EditorElement(cx.entity()))
        .child(band)
        .child(counts)
        .children(toggle)
        .children(bar);
        // The find bar beside the note, not inside it: typing a search never reaches the note's keys.
        div().size_full().relative().child(el).children(find_bar)
    }
}
