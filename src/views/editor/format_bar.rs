//! The formatting bar at the bottom of the note, and the colours of highlights.

use super::*;

/// The formatting bar of a note.
#[derive(Default)]
pub(super) struct BarState {
    /// What the formatting bar offers, with the version and selection it was computed for.
    pub(super) tools: Option<((u64, Range<usize>), Tools)>,
    /// Left edge of the bar, when it shows (see `bar_place`).
    pub(super) at: Option<Bar>,
    /// Bar collapsed into a button (narrow note): its buttons are open above it.
    pub(super) open: bool,
    /// The highlight color badges are open.
    pub(super) swatches: bool,
}

/// Width of the formatting bar: eleven 26 px buttons, the colors chevron (14 px), two
/// separators, 1 px between each of the fourteen items, 3 px of margin and the border line.
pub(super) const BAR_WIDTH: f32 = 6. + 2. + 11. * 26. + 14. + 13. + 2. * 7.;

/// Background of the button that collapses the bar: always opaque, it goes over text (the selection tint,
/// translucent, would show what it covers).
pub fn bar_fill(t: &Theme, open: bool) -> Hsla {
    if open { crate::ui::theme::blend(t.accent, t.panel, 0.2) } else { t.panel }
}

/// Where and how the formatting bar shows.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Bar {
    /// In the bottom row, its left edge at this x.
    Inline(f32),
    /// Collapsed into a button, bottom left, which opens the bar above it.
    Compact,
}

/// What the formatting bar offers for the selection.
#[derive(Clone, Copy)]
pub(super) struct Tools {
    /// The inline styles applied (`md::STYLES`).
    pub(super) styles: u16,
    /// The list kind of the cursor's line.
    pub(super) list: Option<ListStyle>,
    /// The cursor is in a table.
    pub(super) table: bool,
}

impl Editor {
    /// The color badges are open.
    #[cfg(test)]
    pub fn swatches_open(&self) -> bool {
        self.bar.swatches
    }

    /// Highlights the selection in color `color` (see `md::MARKS`): in the same color it already has,
    /// the highlight goes; in another, it changes color; otherwise it is applied.
    pub(super) fn paint_with(&mut self, color: usize, cx: &mut Context<Self>) {
        let (open, close) = md::mark_signs(color);
        self.aligned(cx, |this, cx| {
            let parts = this.mark_parts();
            if parts.is_empty() {
                return;
            }
            let cuts: Vec<_> = parts.iter().map(|part| this.cut(part, md::HIGHLIGHT)).collect();
            this.push_undo(true);
            if cuts.iter().any(Option::is_none) {
                return this.restyle_parts(&parts, &cuts, &open, close, true, cx);
            }
            let same = cuts.iter().flatten().all(|(o, _)| this.content[o.clone()] == open);
            for (o, c) in cuts.into_iter().flatten().rev() {
                this.splice(c, if same { "" } else { close }, cx);
                this.splice(o, if same { "" } else { &open }, cx);
            }
        });
    }

    #[cfg(test)]
    /// The formatting bar's buttons are on screen: in the bottom row, or open above the button that collapses it.
    pub fn bar_shown(&self) -> bool {
        match self.bar.at {
            Some(Bar::Inline(_)) => true,
            Some(Bar::Compact) => self.bar.open,
            None => false,
        }
    }

    /// The note is too narrow for the bar: it collapses into a button.
    #[cfg(test)]
    pub fn bar_folded(&self) -> bool {
        self.bar.at == Some(Bar::Compact)
    }

    /// Where to put the bar: in the middle of the page, further left when the counter and the corner buttons
    /// would push it, collapsed into a button when it no longer has room; `None` if it is off or
    /// this is not the active pane, or as long as the note's width is not known.
    // ponytail: the counter's room is estimated (7.4 px per character at 12 px, spaces and middle dots count more than letters) rather than measured.
    pub(super) fn bar_place(&mut self) -> Option<Bar> {
        let width = f32::from(self.viewport.size.width);
        if !self.theme.bar || !self.bar_here || width == 0. {
            return None;
        }
        // At the interface's scale, like the counter and the buttons around.
        let (u, bar) = (self.theme.ui, BAR_WIDTH * self.theme.ui);
        let counts = (self.counts().chars().count() as f32 * 7.4 + 16.) * u;
        let x = (width / 2. - bar / 2.).min(width - (12. + 34. * self.corner as f32) * u - counts - 8. * u - bar);
        Some(if x >= 8. * u { Bar::Inline(x) } else { Bar::Compact })
    }

    /// What the bar offers for the selection, recomputed only when the text or the selection change.
    // ponytail: nothing lights up for a selection over several lines (the gesture does handle them),
    // unless handles took cells: computing it at each mouse move would weigh on a large selection.
    pub(super) fn tools(&mut self) -> Tools {
        let key = (self.version, self.sel.clone());
        if let Some((known, tools)) = &self.bar.tools
            && *known == key
        {
            return *tools;
        }
        let line = self.line_range(self.sel.start);
        let one_line = self.sel.end <= line.end;
        let parts = if one_line || self.pick_cells().is_some() { self.mark_parts() } else { Vec::new() };
        // A button lights when every span taken has the style, as the gesture would remove it.
        let styles = if parts.is_empty() || parts.len() > 2000 {
            0
        } else {
            md::STYLES.into_iter().filter(|&flag| parts.iter().all(|part| self.cut(part, flag).is_some())).fold(0, |all, flag| all | flag)
        };
        let list = match md::classify(&self.content[line], false).0 {
            Kind::Bullet => Some(ListStyle::Bullet),
            Kind::Ordered => Some(ListStyle::Ordered),
            Kind::Task(_) => Some(ListStyle::Task),
            _ => None,
        };
        let tools = Tools { styles, list, table: self.in_table() };
        self.bar.tools = Some((key, tools));
        tools
    }

    /// The styles applied on the selection (`md::STYLES`), to light up the formatting bar's buttons.
    #[cfg(test)]
    pub fn styles(&mut self) -> u16 {
        self.tools().styles
    }

    /// The highlight color badges, above the bar; `aligned`: under the H button.
    pub(super) fn swatches(&mut self, aligned: bool, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let t = self.theme;
        let names = [tr("Yellow", "Jaune"), tr("Green", "Vert"), tr("Blue", "Bleu"), tr("Pink", "Rose"), tr("Purple", "Violet")];
        div()
            .debug_selector(|| "format-swatches".into())
            .ml(t.px(if aligned { 4. + 4. * 27. } else { 0. }))
            .flex_none()
            .h(t.px(30.))
            .px(t.px(7.))
            .flex()
            .items_center()
            .gap(t.px(5.))
            .rounded(t.px(8.))
            .border_1()
            .border_color(t.border)
            .bg(t.panel)
            .occlude()
            .children((0..md::MARKS.len()).map(|i| {
                div()
                    .id(("swatch", i))
                    .debug_selector(move || format!("swatch-{i}"))
                    .size(t.px(18.))
                    .flex_none()
                    .rounded_full()
                    .cursor_pointer()
                    .bg(crate::ui::theme::highlight_tint(i).opacity(0.8))
                    .border_1()
                    .border_color(t.border)
                    .hover(|s| s.border_color(t.accent))
                    .tooltip(crate::ui::widgets::tip(names[i], String::new(), t))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        window.focus(&this.focus);
                        this.bar.swatches = false;
                        this.each(cx, |this, cx| this.paint_with(i, cx));
                    }))
            }))
    }

    /// The formatting bar: bold, italic, underline, strikethrough, highlight, code, link, then lists
    /// and table insertion, or, in a table, its rows and columns. A button lights up
    /// when the selection already has the style, and then removes it.
    pub(super) fn format_bar(&mut self, mode: Bar, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let t = self.theme;
        let tools = self.tools();
        let width = f32::from(self.viewport.size.width);
        let (left, wide, inline) = match mode {
            Bar::Inline(x) => (x, BAR_WIDTH * t.ui, true),
            Bar::Compact => (8. * t.ui, (BAR_WIDTH * t.ui).min(width - 16. * t.ui), false),
        };
        let swatches = self.bar.swatches.then(|| self.swatches(inline, cx));
        let on = tools.styles;
        let keys = |action: &dyn gpui::Action| crate::ui::keys::of(cx, action);
        let mut tips = vec![
            (tr("Bold", "Gras"), keys(&Bold)),
            (tr("Italic", "Italique"), keys(&Italic)),
            (tr("Underline", "Souligné"), keys(&Underline)),
            (tr("Strikethrough", "Barré"), keys(&Strike)),
            (tr("Highlight", "Surligné"), keys(&Highlight)),
            (tr("Code", "Code"), keys(&InlineCode)),
            (tr("Link", "Lien"), keys(&InsertLink)),
        ];
        let ops = [TableOp::RowBelow, TableOp::ColRight, TableOp::DeleteRow, TableOp::DeleteCol];
        if tools.table {
            tips.extend(ops.map(|op| (op.label(), String::new())));
        } else {
            tips.extend([
                (tr("Bulleted list", "Liste à puces"), keys(&ListBullet)),
                (tr("Numbered list", "Liste numérotée"), keys(&ListOrdered)),
                (tr("Task list", "Liste de tâches"), keys(&ListTask)),
                (tr("Table", "Tableau"), String::new()),
            ]);
        }
        // A button: its face, its state, its tooltip; the gesture is attached next.
        let button = |i: usize, face: gpui::AnyElement, lit: bool| {
            let (label, keys) = tips[i].clone();
            div()
                .id(("format", i))
                .debug_selector(move || format!("format-{i}"))
                .size(t.px(26.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(t.px(6.))
                .cursor_pointer()
                .text_size(t.px(13.))
                .text_color(if lit { t.accent } else { t.soft })
                .when(lit, |d| d.bg(t.selection))
                .hover(|s| s.bg(t.border).text_color(t.text))
                .active(|s| s.bg(t.selection))
                .tooltip(crate::ui::widgets::tip(label, keys, t))
                // Neither the note nor its cursor move under the click: the gesture applies to the selection.
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(face)
        };
        let icon = |path: &'static str, lit: bool| gpui::svg().path(path).size(t.px(16.)).flex_none().text_color(if lit { t.accent } else { t.soft }).into_any_element();
        let lit = |flag: u16| on & flag != 0;
        let styled = |i: usize, flag: u16, face: gpui::AnyElement| {
            button(i, face, lit(flag)).on_click(cx.listener(move |this, _, window, cx| {
                window.focus(&this.focus);
                this.each(cx, |this, cx| this.mark(flag, cx));
            }))
        };
        // A list kind: a second click, on its lines, turns them back into text.
        let listed = |i: usize, style: ListStyle, path: &'static str| {
            let lit = tools.list == Some(style);
            button(i, icon(path, lit), lit).on_click(cx.listener(move |this, _, window, cx| {
                window.focus(&this.focus);
                this.restyle(if lit { ListStyle::Text } else { style }, cx);
            }))
        };
        let last = if tools.table {
            let paths = ["row-add.svg", "col-add.svg", "row-del.svg", "col-del.svg"];
            (0..4)
                .map(|n| {
                    button(7 + n, icon(paths[n], false), false)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            window.focus(&this.focus);
                            this.table_op(ops[n], cx);
                        }))
                        .into_any_element()
                })
                .collect::<Vec<_>>()
        } else {
            vec![
                listed(7, ListStyle::Bullet, "list-bullet.svg").into_any_element(),
                listed(8, ListStyle::Ordered, "list-number.svg").into_any_element(),
                listed(9, ListStyle::Task, "list-task.svg").into_any_element(),
                // The size is picked on the grid, as with `/table`.
                button(10, icon("table.svg", false), false)
                    .on_click(cx.listener(|this, _, window, cx| {
                        window.focus(&this.focus);
                        this.grid = Some((3, 3));
                        this.grid_typed.clear();
                        cx.notify();
                    }))
                    .into_any_element(),
            ]
        };
        let separator = || div().w(t.px(1.)).h(t.px(14.)).mx(t.px(3.)).bg(t.border);
        let bar = div()
            .debug_selector(|| "format-bar".into())
            .w_full()
            .min_h(t.px(30.))
            .px(t.px(3.))
            .flex_none()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(t.px(1.))
            .rounded(t.px(8.))
            .border_1()
            .border_color(t.border)
            .bg(t.panel)
            .occlude()
            .child(styled(0, md::BOLD, div().font_weight(FontWeight::BOLD).child("B").into_any_element()))
            .child(styled(1, md::ITALIC, div().italic().child("I").into_any_element()))
            .child(styled(2, md::UNDER, div().underline().child("U").into_any_element()))
            .child(styled(3, md::STRIKE, div().line_through().child("S").into_any_element()))
            .child(styled(4, md::HIGHLIGHT, div().px(t.px(4.)).rounded(t.px(3.)).bg(t.highlight(0)).text_color(t.text).child("H").into_any_element()))
            // Picking the color: badges above the bar.
            .child(
                div()
                    .id("format-colors")
                    .debug_selector(|| "format-colors".into())
                    .w(t.px(14.))
                    .h(t.px(26.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(t.px(6.))
                    .cursor_pointer()
                    .when(self.bar.swatches, |d| d.bg(t.selection))
                    .hover(|s| s.bg(t.border))
                    .tooltip(crate::ui::widgets::tip(tr("Highlight color", "Couleur du surlignage"), String::new(), t))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(|this, _, window, cx| {
                        window.focus(&this.focus);
                        this.bar.swatches = !this.bar.swatches;
                        cx.notify();
                    }))
                    .child(gpui::svg().path("chevron-down.svg").size(t.px(10.)).flex_none().text_color(t.soft)),
            )
            .child(separator())
            .child(styled(5, md::CODE, icon("code.svg", lit(md::CODE))))
            .child(button(6, icon("link.svg", false), false).on_click(cx.listener(|this, _, window, cx| {
                window.focus(&this.focus);
                this.link(cx);
            })))
            .child(separator())
            .children(last);
        // The color badges above the bar, all anchored to the bottom of the note.
        // Collapsed, the bar opens above the button (30 px, 16 px from the bottom, 6 px apart).
        div().absolute().bottom(t.px(if inline { 16. } else { 52. })).left(px(left)).w(px(wide)).flex().flex_col().items_start().gap(t.px(6.)).children(swatches).child(bar)
    }

    /// The button that collapses the bar: bottom left, it opens the bar above it and closes it.
    pub(super) fn bar_toggle(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let t = self.theme;
        let open = self.bar.open;
        div()
            .id("format-toggle")
            .debug_selector(|| "format-toggle".into())
            .absolute()
            .bottom_4()
            .left_2()
            .size(t.px(30.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(t.px(8.))
            .border_1()
            .border_color(t.border)
            .bg(bar_fill(&t, open))
            .cursor_pointer()
            .hover(|s| s.bg(t.border))
            .occlude()
            .tooltip(crate::ui::widgets::tip(tr("Formatting", "Mise en forme"), String::new(), t))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, window, cx| {
                window.focus(&this.focus);
                this.bar.open = !this.bar.open;
                this.bar.swatches = false;
                cx.notify();
            }))
            .child(gpui::svg().path("format.svg").size(t.px(16.)).flex_none().text_color(if open { t.accent } else { t.soft }))
    }
}
