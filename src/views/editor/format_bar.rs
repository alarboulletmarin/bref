//! The formatting bar at the bottom of the note, and the colours of highlights.

use super::*;

/// The formatting bar of a note.
#[derive(Default)]
pub(super) struct BarState {
    /// Ce que la barre de mise en forme propose, avec la version et la sélection pour lesquelles on l'a compté.
    pub(super) tools: Option<((u64, Range<usize>), Tools)>,
    /// Bord gauche de la barre, quand elle se montre (voir `bar_place`).
    pub(super) at: Option<Bar>,
    /// Barre repliée en un bouton (note étroite) : ses boutons sont ouverts au-dessus de lui.
    pub(super) open: bool,
    /// Les pastilles de couleur du surlignage sont ouvertes.
    pub(super) swatches: bool,
}

/// Largeur de la barre de mise en forme : onze boutons de 26 px, le chevron des couleurs (14 px), deux
/// séparateurs, 1 px entre chacun des quatorze éléments, 3 px de marge et le trait de bordure.
pub(super) const BAR_WIDTH: f32 = 6. + 2. + 11. * 26. + 14. + 13. + 2. * 7.;

/// Fond du bouton qui replie la barre : toujours opaque, il passe sur du texte (la teinte de la sélection,
/// translucide, laisserait voir ce qu'il couvre).
pub fn bar_fill(t: &Theme, open: bool) -> Hsla {
    if open { crate::ui::theme::blend(t.accent, t.panel, 0.2) } else { t.panel }
}

/// Où et comment se montre la barre de mise en forme.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Bar {
    /// Dans la rangée du bas, son bord gauche à cette abscisse.
    Inline(f32),
    /// Repliée en un bouton, en bas à gauche, qui ouvre la barre au-dessus de lui.
    Compact,
}

/// Ce que la barre de mise en forme propose pour la sélection.
#[derive(Clone, Copy)]
pub(super) struct Tools {
    /// Les styles en ligne posés (`md::STYLES`).
    pub(super) styles: u16,
    /// Le genre de liste de la ligne du curseur.
    pub(super) list: Option<ListStyle>,
    /// Le curseur est dans un tableau.
    pub(super) table: bool,
}

/// Fond du texte surligné, de la couleur `color` (voir `md::MARKS`) : translucide, lisible sur le clair
/// comme sur le sombre, et distinct de la teinte d'accent des commentaires.
pub fn highlight(color: usize) -> Hsla {
    let hue = [0.14, 0.36, 0.58, 0.93, 0.76][color.min(md::MARKS.len() - 1)];
    Hsla { h: hue, s: 0.9, l: 0.55, a: 0.4 }
}

impl Editor {
    /// Les pastilles de couleur sont ouvertes.
    #[cfg(test)]
    pub fn swatches_open(&self) -> bool {
        self.bar.swatches
    }

    /// Surligne la sélection de la couleur `color` (voir `md::MARKS`) : de la même couleur qu'elle a déjà,
    /// le surlignage part ; d'une autre, il change de couleur ; sinon il se pose.
    pub(super) fn paint_with(&mut self, color: usize, cx: &mut Context<Self>) {
        let (open, close) = md::mark_signs(color);
        let parts = self.mark_parts();
        if parts.is_empty() {
            return;
        }
        let cuts: Option<Vec<_>> = parts.iter().map(|part| self.cut(part, md::HIGHLIGHT)).collect();
        self.push_undo(true);
        let Some(cuts) = cuts else {
            return self.wrap(&parts, &open, close, cx);
        };
        let same = cuts.iter().all(|(o, _)| self.content[o.clone()] == open);
        for (o, c) in cuts.into_iter().rev() {
            self.splice(c, if same { "" } else { close }, cx);
            self.splice(o, if same { "" } else { &open }, cx);
        }
    }

    #[cfg(test)]
    /// Les boutons de la barre de mise en forme sont à l'écran : dans la rangée du bas, ou ouverts au-dessus du bouton qui la replie.
    pub fn bar_shown(&self) -> bool {
        match self.bar.at {
            Some(Bar::Inline(_)) => true,
            Some(Bar::Compact) => self.bar.open,
            None => false,
        }
    }

    /// La note est trop étroite pour la barre : elle se replie en un bouton.
    #[cfg(test)]
    pub fn bar_folded(&self) -> bool {
        self.bar.at == Some(Bar::Compact)
    }

    /// Où poser la barre : au milieu de la page, plus à gauche quand le compteur et les boutons du coin
    /// la pousseraient, repliée en un bouton quand elle n'a plus la place ; `None` si elle est coupée ou
    /// que ce n'est pas le pane actif, ou tant que la largeur de la note n'est pas connue.
    // ponytail: la place du compteur est estimée (7,4 px par caractère à 12 px, les espaces et les points médians comptent plus que les lettres) plutôt que mesurée.
    pub(super) fn bar_place(&mut self) -> Option<Bar> {
        let width = f32::from(self.viewport.size.width);
        if !self.theme.bar || !self.bar_here || width == 0. {
            return None;
        }
        let counts = self.counts().chars().count() as f32 * 7.4 + 16.;
        let x = (width / 2. - BAR_WIDTH / 2.).min(width - (12. + 34. * self.corner as f32) - counts - 8. - BAR_WIDTH);
        Some(if x >= 8. { Bar::Inline(x) } else { Bar::Compact })
    }

    /// Ce que la barre propose pour la sélection, recalculé seulement quand le texte ou la sélection changent.
    // ponytail: rien n'est allumé pour une sélection sur plusieurs lignes (le geste, lui, les traite) :
    // la compter à chaque mouvement de souris pèserait sur une grande sélection.
    pub(super) fn tools(&mut self) -> Tools {
        let key = (self.version, self.sel.clone());
        if let Some((known, tools)) = &self.bar.tools
            && *known == key
        {
            return *tools;
        }
        let line = self.line_range(self.sel.start);
        let one_line = self.sel.end <= line.end;
        let parts = if one_line { self.mark_parts() } else { Vec::new() };
        let styles = match parts.as_slice() {
            [part] => md::STYLES.into_iter().filter(|&flag| self.cut(part, flag).is_some()).fold(0, |all, flag| all | flag),
            _ => 0,
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

    /// Les styles posés sur la sélection (`md::STYLES`), pour allumer les boutons de la barre de mise en forme.
    #[cfg(test)]
    pub fn styles(&mut self) -> u16 {
        self.tools().styles
    }

    /// Les pastilles de couleur du surlignage, au-dessus de la barre ; `aligned` : sous le bouton H.
    pub(super) fn swatches(&mut self, aligned: bool, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let t = self.theme;
        let names = [tr("Yellow", "Jaune"), tr("Green", "Vert"), tr("Blue", "Bleu"), tr("Pink", "Rose"), tr("Purple", "Violet")];
        div()
            .debug_selector(|| "format-swatches".into())
            .ml(px(if aligned { 4. + 4. * 27. } else { 0. }))
            .flex_none()
            .h(px(30.))
            .px(px(7.))
            .flex()
            .items_center()
            .gap(px(5.))
            .rounded(px(8.))
            .border_1()
            .border_color(t.border)
            .bg(t.panel)
            .occlude()
            .children((0..md::MARKS.len()).map(|i| {
                div()
                    .id(("swatch", i))
                    .debug_selector(move || format!("swatch-{i}"))
                    .size(px(18.))
                    .flex_none()
                    .rounded_full()
                    .cursor_pointer()
                    .bg(highlight(i).opacity(2.))
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

    /// La barre de mise en forme : gras, italique, souligné, barré, surligné, code, lien, puis les listes
    /// et l'insertion d'un tableau, ou, dans un tableau, ses lignes et ses colonnes. Un bouton s'allume
    /// quand la sélection a déjà le style, et le retire alors.
    pub(super) fn format_bar(&mut self, mode: Bar, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let t = self.theme;
        let tools = self.tools();
        let width = f32::from(self.viewport.size.width);
        let (left, wide, inline) = match mode {
            Bar::Inline(x) => (x, BAR_WIDTH, true),
            Bar::Compact => (8., BAR_WIDTH.min(width - 16.), false),
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
        // Un bouton : son visage, son état, son infobulle ; le geste se pose ensuite.
        let button = |i: usize, face: gpui::AnyElement, lit: bool| {
            let (label, keys) = tips[i].clone();
            div()
                .id(("format", i))
                .debug_selector(move || format!("format-{i}"))
                .size(px(26.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .cursor_pointer()
                .text_size(px(13.))
                .text_color(if lit { t.accent } else { t.soft })
                .when(lit, |d| d.bg(t.selection))
                .hover(|s| s.bg(t.border).text_color(t.text))
                .active(|s| s.bg(t.selection))
                .tooltip(crate::ui::widgets::tip(label, keys, t))
                // Ni la note ni son curseur ne bougent sous le clic : le geste porte sur la sélection.
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(face)
        };
        let icon = |path: &'static str, lit: bool| gpui::svg().path(path).size(px(16.)).flex_none().text_color(if lit { t.accent } else { t.soft }).into_any_element();
        let lit = |flag: u16| on & flag != 0;
        let styled = |i: usize, flag: u16, face: gpui::AnyElement| {
            button(i, face, lit(flag)).on_click(cx.listener(move |this, _, window, cx| {
                window.focus(&this.focus);
                this.each(cx, |this, cx| this.mark(flag, cx));
            }))
        };
        // Un genre de liste : un second clic, sur ses lignes, les remet en texte.
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
                // Le choix de la taille se fait sur la grille, comme avec `/table`.
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
        let separator = || div().w(px(1.)).h(px(14.)).mx(px(3.)).bg(t.border);
        let bar = div()
            .debug_selector(|| "format-bar".into())
            .w_full()
            .min_h(px(30.))
            .px(px(3.))
            .flex_none()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(1.))
            .rounded(px(8.))
            .border_1()
            .border_color(t.border)
            .bg(t.panel)
            .occlude()
            .child(styled(0, md::BOLD, div().font_weight(FontWeight::BOLD).child("B").into_any_element()))
            .child(styled(1, md::ITALIC, div().italic().child("I").into_any_element()))
            .child(styled(2, md::UNDER, div().underline().child("U").into_any_element()))
            .child(styled(3, md::STRIKE, div().line_through().child("S").into_any_element()))
            .child(styled(4, md::HIGHLIGHT, div().px(px(4.)).rounded(px(3.)).bg(highlight(0)).text_color(t.text).child("H").into_any_element()))
            // Le choix de la couleur : des pastilles au-dessus de la barre.
            .child(
                div()
                    .id("format-colors")
                    .debug_selector(|| "format-colors".into())
                    .w(px(14.))
                    .h(px(26.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.))
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
                    .child(gpui::svg().path("chevron-down.svg").size(px(10.)).flex_none().text_color(t.soft)),
            )
            .child(separator())
            .child(styled(5, md::CODE, icon("code.svg", lit(md::CODE))))
            .child(button(6, icon("link.svg", false), false).on_click(cx.listener(|this, _, window, cx| {
                window.focus(&this.focus);
                this.link(cx);
            })))
            .child(separator())
            .children(last);
        // Les pastilles de couleur au-dessus de la barre, le tout ancré au bas de la note.
        // Repliée, la barre s'ouvre au-dessus du bouton (30 px, 16 px du bas, 6 px d'écart).
        div().absolute().bottom(px(if inline { 16. } else { 52. })).left(px(left)).w(px(wide)).flex().flex_col().items_start().gap(px(6.)).children(swatches).child(bar)
    }

    /// Le bouton qui replie la barre : en bas à gauche, il l'ouvre au-dessus de lui et la referme.
    pub(super) fn bar_toggle(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let t = self.theme;
        let open = self.bar.open;
        div()
            .id("format-toggle")
            .debug_selector(|| "format-toggle".into())
            .absolute()
            .bottom_4()
            .left_2()
            .size(px(30.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(8.))
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
            .child(gpui::svg().path("format.svg").size(px(16.)).flex_none().text_color(if open { t.accent } else { t.soft }))
    }
}
