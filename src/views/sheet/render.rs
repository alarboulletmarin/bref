//! Drawing the grid: the toolbar, the rows, the cell being edited, the scroll bars.

use super::*;

impl Render for Sheet {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        let (this, focus) = (cx.entity(), self.focus.clone());
        let measure = canvas(
            |_, _, _| (),
            move |bounds, _, window, cx| {
                window.handle_input(&focus, ElementInputHandler::new(bounds, this.clone()), cx);
                this.update(cx, |sheet, cx| sheet.measured(bounds, cx));
            },
        )
        .absolute()
        .size_full();

        // Columns and rows that overflow the view are not built.
        let (lines, columns) = self.visible(window.viewport_size());
        let (r0, r1, c0, c1) = (lines.start, lines.end, columns.start, columns.end);
        let selected = self.selections();

        let head = if self.format.header {
            self.render_row(0, c0..c1, &selected, true)
        } else {
            div().absolute().top(px(0.)).left(px(0.)).h(px(ROW)).flex().text_size(px(self.text_px())).child(div().flex_none().w(px(self.xs[c0]))).children(
                (c0..c1).map(|c| {
                    let inside = selected.iter().any(|(_, cols)| cols.contains(&c));
                    div()
                        .flex_none()
                        .w(px(self.widths[c]))
                        .h(px(ROW))
                        .flex()
                        .items_center()
                        .justify_center()
                        .border_r_1()
                        .border_b_1()
                        .border_color(t.border)
                        .bg(t.code_bg)
                        .text_color(t.soft)
                        .when(inside, |d| d.bg(t.selection))
                        .child(letters(c))
                }),
            )
        };
        let rows = (r0..r1).map(|r| self.render_row(r, c0..c1, &selected, false));
        let grid = div()
            .absolute()
            .top(px(0.))
            .left(px(GUTTER))
            .right(px(0.))
            .bottom(px(0.))
            .overflow_hidden()
            .child(div().absolute().top(px(0.)).left(px(-self.scroll.0)).size_full().children(rows))
            .child(div().absolute().top(px(0.)).left(px(-self.scroll.0)).w_full().h(px(ROW)).cursor_pointer().child(head))
            .children(self.edit.as_ref().map(|edit| self.render_edit(edit)));

        let number = |r: usize, y: f32| {
            let inside = selected.iter().any(|(rows, _)| rows.contains(&r));
            div()
                .absolute()
                .top(px(y))
                .left(px(0.))
                .w(px(GUTTER))
                .h(px(ROW))
                .flex()
                .items_center()
                .justify_center()
                .border_r_1()
                .border_b_1()
                .border_color(t.border)
                .bg(t.code_bg)
                .text_color(if inside { t.text } else { t.soft })
                .when(inside, |d| d.bg(t.selection))
                .child((r + 1).to_string())
        };
        let gutter = div()
            .absolute()
            .top(px(0.))
            .left(px(0.))
            .w(px(GUTTER))
            .h_full()
            .overflow_hidden()
            // A click there takes a whole row, as a click on a header takes its column.
            .cursor_pointer()
            .bg(t.bg)
            .text_size(px(self.text_px()))
            .children((r0..r1).map(|r| number(r, self.y_of(r))))
            .child(div().absolute().top(px(0.)).left(px(0.)).w(px(GUTTER)).h(px(ROW)).border_r_1().border_b_1().border_color(t.border).bg(t.code_bg))
            .children(self.format.header.then(|| number(0, 0.)));

        let body = div()
            .flex_1()
            .min_h_0()
            .relative()
            .overflow_hidden()
            .font_family(mono())
            .child(measure)
            .child(grid)
            .child(gutter)
            .child(self.render_bars())
            .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, window, cx| this.press(e, window, cx)))
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| this.drag_to(e, cx)))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _: &MouseUpEvent, _, _| this.drag = Drag::None))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _: &MouseUpEvent, _, _| this.drag = Drag::None))
            .on_scroll_wheel(cx.listener(|this, e: &ScrollWheelEvent, _, cx| this.wheel(e, cx)));

        line::keys(div(), cx, |this| this.edit.as_mut(), |_, _| ())
            .size_full()
            .flex()
            .flex_col()
            .bg(t.bg)
            .text_color(t.text)
            // A cell being typed in: the keys of an input line come before
            // those of the grid (arrows, deletion, clipboard).
            .key_context(if self.edit.is_some() { "Sheet Line" } else { "Sheet" })
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &Left, _, cx| this.go(0, -1, false, cx)))
            .on_action(cx.listener(|this, _: &Right, _, cx| this.go(0, 1, false, cx)))
            .on_action(cx.listener(|this, _: &Up, _, cx| this.go(-1, 0, false, cx)))
            .on_action(cx.listener(|this, _: &Down, _, cx| this.go(1, 0, false, cx)))
            .on_action(cx.listener(|this, _: &ExtendLeft, _, cx| this.go(0, -1, true, cx)))
            .on_action(cx.listener(|this, _: &ExtendRight, _, cx| this.go(0, 1, true, cx)))
            .on_action(cx.listener(|this, _: &ExtendUp, _, cx| this.go(-1, 0, true, cx)))
            .on_action(cx.listener(|this, _: &ExtendDown, _, cx| this.go(1, 0, true, cx)))
            .on_action(cx.listener(|this, _: &PageUp, _, cx| this.go(-this.page(), 0, false, cx)))
            .on_action(cx.listener(|this, _: &PageDown, _, cx| this.go(this.page(), 0, false, cx)))
            .on_action(cx.listener(|this, _: &RowStart, _, cx| this.select_to((this.cursor.0, 0), false, cx)))
            .on_action(cx.listener(|this, _: &RowEnd, _, cx| this.select_to((this.cursor.0, this.cols() - 1), false, cx)))
            .on_action(cx.listener(|this, _: &Origin, _, cx| {
                this.finish(cx);
                this.select_to((0, 0), false, cx)
            }))
            .on_action(cx.listener(|this, _: &Corner, _, cx| {
                this.finish(cx);
                this.select_to((this.last_row(), this.cols() - 1), false, cx)
            }))
            .on_action(cx.listener(|this, _: &FirstRow, _, cx| {
                this.finish(cx);
                this.select_to((0, this.cursor.1), false, cx)
            }))
            .on_action(cx.listener(|this, _: &LastRow, _, cx| {
                this.finish(cx);
                this.select_to((this.last_row(), this.cursor.1), false, cx)
            }))
            // Tab moves to the next cell, and to the next row at the end of this one.
            .on_action(cx.listener(|this, _: &Next, _, cx| {
                this.finish(cx);
                let (r, c) = this.cursor;
                match c + 1 < this.cols() {
                    true => this.go(0, 1, false, cx),
                    false if r < this.last_row() => this.select_to((r + 1, 0), false, cx),
                    false => {}
                }
            }))
            .on_action(cx.listener(|this, _: &Previous, _, cx| {
                this.finish(cx);
                let (r, c) = this.cursor;
                match c {
                    0 if r > 0 => this.select_to((r - 1, this.cols() - 1), false, cx),
                    0 => {}
                    _ => this.go(0, -1, false, cx),
                }
            }))
            // Enter commits and moves down; outside input, it opens the cell.
            .on_action(cx.listener(|this, _: &Enter, _, cx| match this.edit {
                Some(_) => this.go(1, 0, false, cx),
                None => this.begin_edit(None, cx),
            }))
            .on_action(cx.listener(|this, _: &Edit, _, cx| this.begin_edit(None, cx)))
            .on_action(cx.listener(|this, _: &Cancel, _, cx| {
                if this.edit.take().is_none() {
                    this.anchor = this.cursor;
                }
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Backspace, _, cx| this.clear(cx)))
            .on_action(cx.listener(|this, _: &Delete, _, cx| this.clear(cx)))
            .on_action(cx.listener(|this, _: &Copy, _, cx| this.copy(cx)))
            .on_action(cx.listener(|this, _: &Cut, _, cx| this.cut(cx)))
            .on_action(cx.listener(|this, _: &Paste, _, cx| {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    this.paste(&text, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &SelectAll, _, cx| this.select_all(cx)))
            .on_action(cx.listener(|this, _: &Undo, _, cx| this.restore(false, cx)))
            .on_action(cx.listener(|this, _: &Redo, _, cx| this.restore(true, cx)))
            .on_action(cx.listener(|this, _: &InsertBelow, _, cx| this.insert_rows(true, cx)))
            .on_action(cx.listener(|this, _: &InsertAbove, _, cx| this.insert_rows(false, cx)))
            .on_action(cx.listener(|this, _: &DeleteRows, _, cx| this.delete_span(cx)))
            .on_action(cx.listener(|this, _: &editor::TableDeleteSpan, _, cx| this.delete_span(cx)))
            .on_action(cx.listener(|this, _: &SelectRow, _, cx| this.select_whole(true, cx)))
            .on_action(cx.listener(|this, _: &SelectCol, _, cx| this.select_whole(false, cx)))
            .on_action(cx.listener(|this, _: &editor::TableRowAbove, _, cx| this.table_op(TableOp::RowAbove, cx)))
            .on_action(cx.listener(|this, _: &editor::TableRowBelow, _, cx| this.table_op(TableOp::RowBelow, cx)))
            .on_action(cx.listener(|this, _: &editor::TableColLeft, _, cx| this.table_op(TableOp::ColLeft, cx)))
            .on_action(cx.listener(|this, _: &editor::TableColRight, _, cx| this.table_op(TableOp::ColRight, cx)))
            .on_action(cx.listener(|this, _: &editor::TableDuplicateRow, _, cx| this.table_op(TableOp::DuplicateRow, cx)))
            .on_action(cx.listener(|this, _: &editor::TableDuplicateCol, _, cx| this.table_op(TableOp::DuplicateCol, cx)))
            .on_action(cx.listener(|this, _: &editor::TableMoveRowUp, _, cx| this.table_op(TableOp::MoveRowUp, cx)))
            .on_action(cx.listener(|this, _: &editor::TableMoveRowDown, _, cx| this.table_op(TableOp::MoveRowDown, cx)))
            .on_action(cx.listener(|this, _: &editor::TableMoveColLeft, _, cx| this.table_op(TableOp::MoveColLeft, cx)))
            .on_action(cx.listener(|this, _: &editor::TableMoveColRight, _, cx| this.table_op(TableOp::MoveColRight, cx)))
            .on_action(cx.listener(|this, _: &editor::TableDeleteRow, _, cx| this.table_op(TableOp::DeleteRow, cx)))
            .on_action(cx.listener(|this, _: &editor::TableDeleteCol, _, cx| this.table_op(TableOp::DeleteCol, cx)))
            .on_action(cx.listener(|this, _: &editor::TableSortAsc, _, cx| this.table_op(TableOp::SortAsc, cx)))
            .on_action(cx.listener(|this, _: &editor::TableSortDesc, _, cx| this.table_op(TableOp::SortDesc, cx)))
            .child(self.render_bar(cx))
            .child(body)
    }
}

impl Sheet {
    pub(super) fn chip(&self, id: &'static str, text: String, on: bool, pick: Pick) -> Stateful<gpui::Div> {
        let t = self.theme;
        div()
            .id(id)
            .px_2()
            .py_0p5()
            .rounded(t.small())
            .cursor_pointer()
            .text_color(if on { t.accent } else { t.soft })
            .hover(|s| s.bg(t.hover))
            .child(text)
            .on_click(move |_, window, cx| window.dispatch_action(pick.action(), cx))
    }

    /// What is selected, when it is whole columns or rows, and its gestures: its name
    /// ("Column “age”", "2 rows"), then icons by group (insert, move, sort or
    /// duplicate, delete), each named with its keys in its tooltip.
    pub(super) fn span_tools(&self, cx: &mut Context<Self>) -> Vec<gpui::AnyElement> {
        let t = self.theme;
        let (rows, cols) = self.selection();
        let whole_cols = rows.start == 0 && rows.end >= self.table.rows() && cols.len() < self.cols();
        let whole_rows = cols.start == 0 && cols.end >= self.cols() && !whole_cols && rows.len() < self.table.rows();
        type Tool = (&'static str, &'static str, Box<dyn Action>);
        let (title, groups): (String, Vec<Vec<Tool>>) = if whole_cols {
            let named = self.table.cell(0, cols.start).trim().to_string();
            let title = match (cols.len(), self.format.header && !named.is_empty()) {
                (1, true) => format!("{} « {named} »", tr("Column", "Colonne")),
                (1, false) => format!("{} {}", tr("Column", "Colonne"), letters(cols.start)),
                (n, _) => format!("{n} {}", tr("columns", "colonnes")),
            };
            (title, vec![
                vec![
                    ("c-ins-left.svg", TableOp::ColLeft.label(), Box::new(editor::TableColLeft)),
                    ("c-ins-right.svg", TableOp::ColRight.label(), Box::new(editor::TableColRight)),
                ],
                vec![
                    ("c-left.svg", TableOp::MoveColLeft.label(), Box::new(editor::TableMoveColLeft)),
                    ("c-right.svg", TableOp::MoveColRight.label(), Box::new(editor::TableMoveColRight)),
                ],
                vec![
                    ("c-sort-az.svg", TableOp::SortAsc.label(), Box::new(editor::TableSortAsc)),
                    ("c-sort-za.svg", TableOp::SortDesc.label(), Box::new(editor::TableSortDesc)),
                ],
                vec![("trash.svg", TableOp::DeleteCol.label(), Box::new(editor::TableDeleteCol))],
            ])
        } else if whole_rows {
            let title = match rows.len() {
                1 => format!("{} {}", tr("Row", "Ligne"), rows.start + 1),
                n => format!("{n} {}", tr("rows", "lignes")),
            };
            (title, vec![
                vec![
                    ("c-ins-above.svg", tr("Insert a row above", "Insérer une ligne au-dessus"), Box::new(InsertAbove)),
                    ("c-ins-below.svg", tr("Insert a row below", "Insérer une ligne en dessous"), Box::new(InsertBelow)),
                ],
                vec![
                    ("c-up.svg", TableOp::MoveRowUp.label(), Box::new(editor::TableMoveRowUp)),
                    ("c-down.svg", TableOp::MoveRowDown.label(), Box::new(editor::TableMoveRowDown)),
                ],
                vec![("copy.svg", TableOp::DuplicateRow.label(), Box::new(editor::TableDuplicateRow))],
                vec![("trash.svg", tr("Delete the rows", "Supprimer les lignes"), Box::new(DeleteRows))],
            ])
        } else {
            return Vec::new();
        };
        let separator = || div().flex_none().w(px(1.)).h(px(16.)).mx(px(6.)).bg(t.border).into_any_element();
        let mut out = vec![separator(), div().flex_none().mr_1().font_weight(FontWeight::BOLD).text_color(t.text).child(title).into_any_element()];
        for (g, group) in groups.into_iter().enumerate() {
            if g > 0 {
                out.push(div().flex_none().w(px(1.)).h(px(14.)).mx(px(3.)).bg(t.border).into_any_element());
            }
            for (icon, label, action) in group {
                let name = action.name();
                out.push(
                    button(gpui::SharedString::from(format!("sheet-span-{name}")), icon, false, t)
                        .debug_selector(move || format!("sheet-span-{name}"))
                        .tooltip(tip(label, crate::ui::keys::of(cx, action.as_ref()), t))
                        .on_click(move |_, window, cx| window.dispatch_action(action.boxed_clone(), cx))
                        .into_any_element(),
                );
            }
        }
        out
    }

    pub(super) fn render_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        let name = self.path.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let info = format!("{name} · {} × {}", self.table.rows(), self.table.cols());
        let delimiter = delimiter_option(self.format.delimiter);
        div()
            .flex_none()
            .min_h(px(40.))
            .pl_4()
            // Under the window badge, when the app draws its title bar.
            .pr(px(if self.client { 128. } else { 8. }))
            .flex()
            // In a narrow window, settings wrap instead of sliding under the badge.
            .flex_wrap()
            .items_center()
            .gap_1()
            .border_b_1()
            .border_color(t.border)
            .text_size(px(12.))
            .child(
                div()
                    .flex_shrink()
                    .h(px(38.))
                    .flex()
                    .items_center()
                    .min_w_0()
                    .truncate()
                    .text_color(t.soft)
                    .when(self.client, |d| d.map(crate::ui::widgets::drag_window))
                    .child(info),
            )
            // Next to the table name: what is selected, and what can be done with it.
            .children(self.span_tools(cx))
            .child(div().flex_1().min_w(px(8.)).h(px(38.)).when(self.client, |d| d.map(crate::ui::widgets::drag_window)))
            .child(self.chip("sheet-delimiter", delimiter, false, Pick::Delimiter))
            .child(self.chip("sheet-encoding", self.format.encoding.label().to_string(), false, Pick::Encoding))
            .child(self.chip("sheet-header", tr("Header", "En-tête").to_string(), self.format.header, Pick::Header))
            .child(div().flex_none().w(px(1.)).h(px(16.)).mx(px(6.)).bg(t.border))
            .child(
                button("sheet-copy", if self.copied { "check.svg" } else { "copy.svg" }, false, t)
                    .tooltip(tip(tr("Copy the selection", "Copier la sélection"), crate::ui::keys::of(cx, &Copy), t))
                    .on_click(cx.listener(|this, _, _, cx| this.copy(cx))),
            )
            .child(
                button("sheet-export", "export.svg", false, t)
                    .tooltip(tip(tr("Export the table…", "Exporter le tableau…"), String::new(), t))
                    .on_click(|_, window, cx| window.dispatch_action(Pick::Export.action(), cx)),
            )
    }

    /// A row of cells, from column `cols.start` to `cols.end` excluded.
    pub(super) fn render_row(&self, row: usize, cols: Range<usize>, selected: &[(Range<usize>, Range<usize>)], head: bool) -> gpui::Div {
        let t = self.theme;
        let editing = self.edit.is_some();
        let size = px(self.text_px());
        div()
            .absolute()
            .top(px(self.y_of(row)))
            .left(px(0.))
            .h(px(ROW))
            .flex()
            .child(div().flex_none().w(px(self.xs[cols.start])))
            .children(cols.clone().zip(self.table.cells(row, cols)).map(|(c, field)| {
                let mut text = field.into_owned();
                if text.len() > 400 {
                    text.truncate(text.char_indices().nth(200).map_or(text.len(), |(i, _)| i));
                }
                let text = if text.contains(['\n', '\r']) { text.replace(['\r', '\n'], "↵") } else { text };
                let inside = selected.iter().any(|(rows, cols)| rows.contains(&row) && cols.contains(&c));
                div()
                    .relative()
                    .flex_none()
                    .w(px(self.widths[c]))
                    .h(px(ROW))
                    .px(px(PAD))
                    .flex()
                    .items_center()
                    .border_r_1()
                    .border_b_1()
                    .border_color(t.border)
                    .when(head, |d| d.bg(t.code_bg).font_weight(FontWeight::SEMIBOLD))
                    .when(inside, |d| d.bg(t.selection))
                    .child(div().min_w_0().truncate().child(text))
                    .when(self.cursor == (row, c) && !editing, |d| {
                        d.child(div().absolute().inset_0().border_2().border_color(t.accent))
                    })
            }))
            .text_size(size)
    }

    pub(super) fn render_edit(&self, edit: &Line) -> gpui::Div {
        let t = self.theme;
        let (row, col) = self.cursor;
        div()
            .absolute()
            .left(px(self.xs[col] - self.scroll.0))
            .top(px(self.y_of(row)))
            .min_w(px(self.widths[col]))
            .h(px(ROW))
            .px(px(PAD))
            .cursor_text()
            .flex()
            .items_center()
            .whitespace_nowrap()
            .bg(t.panel)
            .border_2()
            .border_color(t.accent)
            .text_size(px(self.text_px()))
            .child(edit.shown(t).0)
    }

    pub(super) fn render_bars(&self) -> impl IntoElement {
        let t = self.theme;
        let (mx, my) = self.max_scroll();
        let (w, h) = (f32::from(self.body.size.width), f32::from(self.body.size.height));
        let thumb = |track: f32, scroll: f32, max: f32, shown: f32| {
            let len = (track * shown / (shown + max)).max(24.);
            (scroll / max * (track - len), len)
        };
        div()
            .when(my > 0., |d| {
                let (at, len) = thumb(h - ROW, self.scroll.1, my, self.view().1);
                d.child(div().absolute().right(px(2.)).top(px(ROW + at)).w(px(6.)).h(px(len)).rounded_full().cursor_pointer().bg(t.dim.opacity(0.45)))
            })
            .when(mx > 0., |d| {
                let (at, len) = thumb(w - GUTTER, self.scroll.0, mx, self.view().0);
                d.child(div().absolute().bottom(px(2.)).left(px(GUTTER + at)).h(px(6.)).w(px(len)).rounded_full().cursor_pointer().bg(t.dim.opacity(0.45)))
            })
    }
}
