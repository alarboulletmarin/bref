//! Painting the laid-out text, and drawing Mermaid and SVG figures in the background (`Job`).

use super::*;

/// Une figure dessinée hors du fil de l'interface : la mise en page d'un gros diagramme
/// Mermaid peut prendre une seconde, et resvg autant pour un SVG chargé de texte.
#[derive(Clone)]
pub(super) struct Job {
    pub(super) key: u64,
    /// La clé de la source seule : une figure à la fois par source.
    pub(super) family: u64,
    /// Le SVG à dessiner ; `None` si la source est invalide.
    pub(super) make: Arc<dyn Fn() -> Option<String> + Send + Sync>,
    /// Largeur d'affichage au plus, densité de l'écran, et fond.
    pub(super) room: f32,
    pub(super) density: f32,
    pub(super) bg: [u8; 3],
}

impl Job {
    pub(super) fn id(&self) -> (u64, u32, u32, [u8; 3]) {
        (self.key, self.room.to_bits(), self.density.to_bits(), self.bg)
    }
}

impl Hash for Job {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id().hash(state);
    }
}

impl gpui::Asset for Job {
    type Source = Self;
    type Output = Drawing;

    fn load(job: Self, _: &mut App) -> impl Future<Output = Drawing> + Send + 'static {
        async move {
            let tree = svg::parse(&(job.make)()?)?;
            let (w, h) = (tree.size().width(), tree.size().height());
            // Pas plus fin que l'écran à la plus grande largeur d'affichage : un schéma très
            // large ou très haut ne pèse pas plus que ce qu'on en voit.
            let scale = svg::scale_for(w, h, job.density * (job.room / w).min(1.));
            let bytes = svg::bmp(&tree, scale, (0, 0, (w * scale).ceil() as u32, (h * scale).ceil() as u32), job.bg)?;
            Some((Arc::new(Image::from_bytes(ImageFormat::Bmp, bytes)), size(px(w), px(h))))
        }
    }
}

/// La figure de `job` si elle est prête ; sinon elle se dessine (`waiting`), et la note sera
/// remise en page. Une seule à la fois par source (`before` : celles qu'on attendait) : parcourir
/// vite les thèmes ne lance pas un dessin par thème.
pub(super) fn drawn(job: Job, before: &[Job], waiting: &mut Vec<Job>, window: &mut Window, cx: &mut App) -> Option<Drawing> {
    if let Some(running) = before.iter().find(|j| j.family == job.family && j.id() != job.id()) {
        if window.use_asset::<Job>(running, cx).is_none() {
            waiting.push(running.clone());
            return None;
        }
        cx.remove_asset::<Job>(running);
    }
    let drawing = window.use_asset::<Job>(&job, cx);
    // Prête quand son image est décodée : jusque-là, l'ancienne version reste à l'écran.
    let ready = match &drawing {
        Some(Some((image, _))) => image.clone().use_render_image(window, cx).is_some(),
        Some(None) => true,
        None => false,
    };
    if !ready {
        waiting.push(job);
        return None;
    }
    // La figure est gardée par l'éditeur : gpui n'a plus à la retenir.
    cx.remove_asset::<Job>(&job);
    drawing
}

pub(super) struct EditorElement(pub(super) Entity<Editor>);

impl IntoElement for EditorElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for EditorElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.0.update(cx, |editor, cx| {
            let before = editor.viewport.size.width;
            editor.layout(bounds, window, cx);
            // La place de la barre de mise en forme suit la largeur de la note : l'image suivante la déplace.
            if before != editor.viewport.size.width {
                cx.notify();
            }
        });
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus = self.0.read(cx).focus.clone();
        window.handle_input(&focus, ElementInputHandler::new(bounds, self.0.clone()), cx);
        let focused = focus.is_focused(window);
        self.0.update(cx, |editor, cx| {
            editor.paint(focused, window, cx);
            if let Some((hand, hitbox)) = editor.hand.zip(editor.view_hitbox.as_ref()) {
                window.set_cursor_style(hand, hitbox);
            }
        });
    }
}

impl Editor {
    pub(super) fn paint(&self, focused: bool, window: &mut Window, cx: &mut App) {
        let (t, o, w) = (self.theme, self.origin, self.width);
        let cursor = self.cursor();
        let sel = &self.sel;
        let label = |text: &str, color, window: &mut Window| {
            let run = TextRun {
                len: text.len(),
                font: font(sans()),
                color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            window
                .text_system()
                .shape_line(SharedString::from(text.to_string()), px(14.), &[run], None)
        };
        for row in &self.rows {
            let top = o.y + row.y;
            if top + row.height < self.viewport.top() || top > self.viewport.bottom() {
                continue;
            }
            let text_top = top + row.pad;
            let end = row.start + row.len;
            let has_cursor = (row.start..=end).contains(&cursor);
            let block = |x: Pixels, y: Pixels, w: Pixels, h: Pixels| {
                Bounds::new(point(o.x + x, y), size(w, h))
            };
            match row.kind {
                Kind::Code | Kind::Fence => {
                    window.paint_quad(fill(block(px(-12.), top, w + px(24.), row.height), t.code_bg))
                }
                Kind::Quote => {
                    if let Some(tint) = row.tint {
                        window.paint_quad(fill(block(px(-14.), top, w + px(28.), row.height), tint.opacity(0.1)));
                    }
                    window.paint_quad(fill(block(px(-14.), top, px(3.), row.height), row.tint.unwrap_or(t.border)))
                }
                Kind::Rule if !has_cursor => {
                    let mid = top + row.height / 2.;
                    window.paint_quad(fill(block(px(0.), mid, w, px(1.)), t.border));
                    continue;
                }
                _ => {}
            }
            // Un tableau plus large que la page est coupé à ses bords.
            let mask = (row.kind == Kind::Table).then(|| {
                let frame = self.tables.iter().find(|(range, _)| range.contains(&row.start)).map(|(_, frame)| *frame);
                let (left, wide) = frame.map_or((o.x, w), |f| (f.left(), f.size.width));
                ContentMask { bounds: Bounds::new(point(left - px(2.), top), size(wide + px(4.), row.height)) }
            });
            if row.hidden() {
                continue;
            }
            window.with_content_mask(mask, |window| {
                // Dans un tableau, le fond de l'en-tête passe sous la sélection.
                let table_top = point(o.x - row.dx, top);
                if let Some(grid) = &row.grid {
                    grid.paint_back(table_top, row.height - row.gap, &t, window);
                }
                // Fond d'un passage du texte sur cette ligne : la sélection, les passages trouvés.
                let mark = |range: &Range<usize>, color: Hsla, window: &mut Window| {
                    let a = row.pos(range.start.max(row.start) - row.start);
                    let b = row.pos(range.end.min(end) - row.start);
                    let tail = if range.end > end { px(6.) } else { px(0.) };
                    let mut rect = |x: Pixels, y: Pixels, w: Pixels, h: Pixels| {
                        window.paint_quad(fill(block(x, text_top + y, w, h), color))
                    };
                    if let Some(grid) = &row.grid {
                        for (x, y, w) in grid.spans(range.start.max(row.start) - row.start, range.end.min(end) - row.start, row.lh) {
                            rect(x - row.dx, y, w, row.lh);
                        }
                    } else if a.y == b.y {
                        rect(a.x, a.y, b.x - a.x + tail, row.lh);
                    } else {
                        rect(a.x, a.y, w - a.x, row.lh);
                        rect(px(0.), a.y + row.lh, w, b.y - a.y - row.lh);
                        rect(px(0.), b.y, b.x + tail, row.lh);
                    }
                };
                // Le texte surligné (`==texte==`), sous la sélection ; les trouvailles et commentaires passent dessus.
                if !matches!(row.kind, Kind::Code | Kind::Fence | Kind::Meta | Kind::Rule) {
                    for (r, color) in md::highlights(&self.content[row.start..end], row.kind == Kind::Table) {
                        mark(&(row.start + r.start..row.start + r.end), highlight(color), window);
                    }
                }
                if !sel.is_empty() && sel.start <= end && sel.end > row.start {
                    mark(sel, t.selection, window);
                }
                for (other, _) in self.more.iter().filter(|(other, _)| !other.is_empty() && other.start <= end && other.end > row.start) {
                    mark(other, t.selection, window);
                }
                // Le texte qui porte un commentaire est surligné.
                if !matches!(row.kind, Kind::Code | Kind::Fence) {
                    for (_, noted, _) in md::comments(&self.content[row.start..end]) {
                        mark(&(row.start + noted.start..row.start + noted.end), t.accent.opacity(0.22), window);
                    }
                }
                if let Some(find) = &self.find {
                    let from = find.hits.partition_point(|hit| hit.end <= row.start);
                    for (i, hit) in find.hits.iter().enumerate().skip(from).take_while(|(_, hit)| hit.start < end) {
                        mark(hit, t.accent.opacity(if i == find.at { 0.45 } else { 0.2 }), window);
                    }
                }
                if let Some(grid) = &row.grid {
                    grid.paint_front(table_top, row.height - row.gap, row.lh, &t, window, cx);
                } else if let Some(line) = &row.shaped.line {
                    line.paint(point(o.x - row.dx, text_top), row.lh, TextAlign::Left, None, window, cx)
                        .ok();
                }
                if focused && sel.is_empty() && has_cursor {
                    let p = row.pos(cursor - row.start);
                    let caret = block(p.x, text_top + p.y + row.lh * 0.14, px(2.), row.lh * 0.72);
                    window.paint_quad(fill(caret, t.accent));
                }
                // Les autres curseurs, quand la note a la saisie.
                for (other, back) in self.more.iter().filter(|_| focused && row.grid.is_none()) {
                    let at = if *back { other.start } else { other.end };
                    if other.is_empty() && (row.start..=end).contains(&at) {
                        let p = row.pos(at - row.start);
                        let caret = block(p.x, text_top + p.y + row.lh * 0.14, px(2.), row.lh * 0.72);
                        window.paint_quad(fill(caret, t.accent));
                    }
                }
            });
            if let Some((image, s)) = &row.image {
                let at = block(px(0.), top + row.height - s.height, s.width, s.height);
                window.paint_image(at, Corners::all(px(6.)), image.clone(), 0, false).ok();
            }
            // Bouton de copie du bloc : la même icône que pour copier la note.
            if row.opens {
                let bounds = self.copy_bounds(row);
                if self.hover == Some(row.start) {
                    window.paint_quad(fill(bounds, t.border).corner_radii(px(6.)));
                }
                let icon = if self.copied == Some(row.start) { "check.svg" } else { "copy.svg" };
                let inset = (COPY_SIZE - px(16.)) / 2.;
                let at = Bounds::new(bounds.origin + point(inset, inset), size(px(16.), px(16.)));
                window.paint_svg(at, icon.into(), TransformationMatrix::unit(), t.text, cx).ok();
                if let Some((_, hitbox)) = self.copy_hitboxes.iter().find(|(s, _)| *s == row.start) {
                    window.set_cursor_style(CursorStyle::PointingHand, hitbox);
                }
                if self.views(row) {
                    let bounds = self.view_bounds(row);
                    if self.view_hover == Some(row.start) {
                        window.paint_quad(fill(bounds, t.border).corner_radii(px(6.)));
                    }
                    let at = Bounds::new(bounds.origin + point(inset, inset), size(px(16.), px(16.)));
                    window.paint_svg(at, "c-zoom-in.svg".into(), TransformationMatrix::unit(), t.text, cx).ok();
                    if let Some((_, hitbox)) = self.view_hitboxes.iter().find(|(s, _)| *s == row.start) {
                        window.set_cursor_style(CursorStyle::PointingHand, hitbox);
                    }
                }
            }
        }
        if self.content.is_empty() {
            // Le reste de la page vide (date, touches, notes à relire) est à `Shell::render_empty`.
            label(tr("Write here…", "Écris ici…"), t.soft, window)
                .paint(point(o.x + px(8.), o.y + px(3.)), px(22.), window, cx)
                .ok();
        }
        for thumb in &self.thumbs {
            window.paint_quad(fill(*thumb, t.dim.opacity(0.5)).corner_radii(px(1.5)));
        }
        // Le trait où se poseraient des fichiers glissés sur la note.
        if let Some(mark) = self.drop_mark
            && let Some(last) = self.rows.last()
        {
            let y = match self.row_at(mark).filter(|_| mark < self.content.len()) {
                Some(r) => self.rows[r].y,
                None => last.y + last.height,
            };
            let line = Bounds::new(point(o.x - px(6.), o.y + y - px(1.)), size(w + px(12.), px(2.)));
            window.paint_quad(fill(line, t.accent).corner_radii(px(1.)));
        }
        for (n, (range, frame)) in self.tables.iter().enumerate() {
            let hover = self.table_hover.filter(|(at, _)| *at == range.start);
            if hover.is_none() && !(focused && (range.start..=range.end).contains(&cursor)) {
                continue;
            }
            for (i, bar) in plus_bars(*frame).into_iter().enumerate() {
                let hot = hover.is_some_and(|(_, over)| over == Some(i));
                window.paint_quad(fill(bar, if hot { t.selection } else { t.code_bg }).corner_radii(px(4.)));
                let icon = Bounds::new(bar.center() - point(px(5.), px(5.)), size(px(10.), px(10.)));
                let color = if hot { t.accent } else { t.soft };
                window.paint_svg(icon, "plus.svg".into(), TransformationMatrix::unit(), color, cx).ok();
                if let Some(hitbox) = self.table_hitboxes.get(n * 2 + i) {
                    window.set_cursor_style(CursorStyle::PointingHand, hitbox);
                }
            }
        }
        // Poignées d'un tableau : une pastille à trois points sur la colonne et la ligne que le pointeur
        // survole (et sur ce qu'on a pris), ce qu'elles ont pris, et le trait où l'on dépose ce qu'on tire.
        for geo in &self.geos {
            let hot = self.part_hover.filter(|(at, _)| *at == geo.start).map(|(_, part)| part);
            let (near, pick) = (geo.near(self.pointer), self.live_pick(geo.start));
            if let Some(pick) = pick {
                let area = geo.cells(&pick.rows, &pick.cols).intersect(&geo.frame);
                window.paint_quad(quad(area, px(2.), t.selection, px(1.), t.accent.opacity(0.7), BorderStyle::default()));
            }
            let held = |part: Part| {
                pick.is_some_and(|p| {
                    let (all_rows, all_cols) = (p.rows.len() == geo.tall(), p.cols.len() == geo.cols());
                    match part {
                        Part::Col(c) => all_rows && p.cols.contains(&c),
                        Part::Row(r) => all_cols && p.rows.contains(&r),
                        Part::Corner => all_rows && all_cols,
                        _ => false,
                    }
                })
            };
            for (part, zone) in geo.parts() {
                if !geo.visible(part, near, pick) {
                    continue;
                }
                if matches!(part, Part::JoinCol(_) | Part::JoinRow(_)) {
                    if hot == Some(part) {
                        let dot = Bounds::new(zone.center() - point(px(7.), px(7.)), size(px(14.), px(14.)));
                        window.paint_quad(fill(dot, t.accent).corner_radii(px(7.)));
                        let icon = Bounds::new(zone.center() - point(px(5.), px(5.)), size(px(10.), px(10.)));
                        window.paint_svg(icon, "plus.svg".into(), TransformationMatrix::unit(), t.bg, cx).ok();
                    }
                    continue;
                }
                // Au repos : un fond clair et un filet ; sous le pointeur : l'accent en transparence ; pris : l'accent.
                let (back, edge, dots) = if held(part) {
                    (t.accent, t.accent, t.bg)
                } else if hot == Some(part) {
                    (t.selection, t.accent.opacity(0.6), t.accent)
                } else {
                    (t.bg, t.border, t.soft)
                };
                window.paint_quad(quad(zone, zone.size.width.min(zone.size.height) / 2., back, px(1.), edge, BorderStyle::default()));
                // Trois points dans le sens de la poignée ; le coin n'en a pas.
                if part != Part::Corner {
                    let lengthwise = zone.size.width > zone.size.height;
                    for k in [-1., 0., 1.] {
                        let step = px(4.) * k;
                        let center = if lengthwise { zone.center() + point(step, px(0.)) } else { zone.center() + point(px(0.), step) };
                        let dot = Bounds::new(center - point(px(1.), px(1.)), size(px(2.), px(2.)));
                        window.paint_quad(fill(dot, dots).corner_radii(px(1.)));
                    }
                }
            }
            if let Some(Moving { table, part, to: Some(b), .. }) = &self.moving
                && *table == geo.start
            {
                let f = geo.frame;
                let line = match part {
                    Part::Col(_) => Bounds::new(point(geo.xs[*b] - px(1.), f.top()), size(px(2.), f.size.height)),
                    _ => Bounds::new(point(f.left(), geo.ys[*b] - px(1.)), size(f.size.width, px(2.))),
                };
                window.paint_quad(fill(line, t.accent));
            }
        }
        // Le bloc qu'on tire : teinté, et un trait là où il se poserait.
        if let Some(drag) = self.handles.drag.as_ref().filter(|drag| drag.moved) {
            if let Some(y) = drag.to.and_then(|to| self.line_top(to)) {
                let line = Bounds::new(point(self.origin.x - px(8.), y - px(1.)), size(self.width + px(16.), px(2.)));
                window.paint_quad(fill(line, t.accent).corner_radii(px(1.)));
            }
        }
        // La poignée du bloc sous le pointeur, et son « + ».
        if let Some((grip, plus)) = self.handle_bounds() {
            let on = |b: Bounds<Pixels>| b.contains(&self.pointer) || (self.handles.drag.is_some() && b == grip);
            for (spot, icon) in [(Some(grip), "grip.svg"), (plus, "plus.svg")] {
                let Some(spot) = spot else { continue };
                if on(spot) {
                    window.paint_quad(fill(spot, t.border).corner_radii(px(4.)));
                }
                let mark = Bounds::new(spot.center() - point(px(7.), px(7.)), size(px(14.), px(14.)));
                window.paint_svg(mark, icon.into(), TransformationMatrix::unit(), if on(spot) { t.text } else { t.soft }, cx).ok();
            }
            if let Some((grip, plus)) = &self.handles.hitboxes {
                window.set_cursor_style(if self.handles.drag.is_some() { CursorStyle::ClosedHand } else { CursorStyle::OpenHand }, grip);
                if let Some(plus) = plus {
                    window.set_cursor_style(CursorStyle::PointingHand, plus);
                }
            }
        }
        // Sélecteur de tableau : la grille, et la taille choisie dessous.
        if focused
            && let Some((cols, rows)) = self.grid
            && let Some(at) = self.popup_at(Self::grid_size())
        {
            let panel = Bounds::new(at, Self::grid_size());
            window.paint_quad(quad(panel, px(8.), t.panel, px(1.), t.border, BorderStyle::default()));
            for (col, row) in (0..GRID * GRID).map(|i| (i % GRID, i / GRID)) {
                let corner = at + point(px(8.) + GRID_STEP * col as f32, px(8.) + GRID_STEP * row as f32);
                let on = col < cols && row < rows;
                let (inside, edge) = if on { (t.selection, t.accent) } else { (t.code_bg, t.border) };
                let cell = Bounds::new(corner, size(px(16.), px(16.)));
                window.paint_quad(quad(cell, px(3.), inside, px(1.), edge, BorderStyle::default()));
            }
            if let Some(hitbox) = &self.grid_hitbox {
                window.set_cursor_style(CursorStyle::PointingHand, hitbox);
            }
            let size = format!("{cols} × {rows}");
            label(&size, t.text, window)
                .paint(point(at.x + px(8.), panel.bottom() - px(24.)), px(20.), window, cx)
                .ok();
            let hint = label(tr("or type 12x5", "ou tape 12x5"), t.soft, window);
            hint.paint(point(panel.right() - px(8.) - hint.width, panel.bottom() - px(24.)), px(20.), window, cx).ok();
        }
        if focused && let Some((_, items)) = self.completion() {
            let chosen = self.ac_index.min(items.len() - 1);
            // La liste suit le choix quand elle dépasse ce qu'elle montre.
            let first = chosen.saturating_sub(SHOWN - 1);
            let shown = &items[first..items.len().min(first + SHOWN)];
            let item_h = px(28.);
            let panel = size(px(280.), item_h * shown.len() as f32 + px(8.));
            let Some(at) = self.popup_at(panel) else {
                return;
            };
            window.paint_quad(quad(Bounds::new(at, panel), px(8.), t.panel, px(1.), t.border, BorderStyle::default()));
            for (i, choice) in shown.iter().enumerate() {
                let y = at.y + px(4.) + item_h * i as f32;
                if first + i == chosen {
                    let hl = Bounds::new(point(at.x + px(4.), y), size(px(272.), item_h));
                    window.paint_quad(fill(hl, t.selection).corner_radii(px(5.)));
                }
                // Le libellé à gauche, sa précision à droite, calée sur le bord : un libellé trop
                // long est coupé avant elle, jamais dessiné dessous.
                let mut write = |text: &str, detail: &str| {
                    let detail = label(detail, t.soft, window);
                    let right = at.x + panel.width - px(12.) - detail.width;
                    let room = right - at.x - px(24.);
                    let (mut cut, mut main) = (text.to_string(), label(text, t.text, window));
                    while main.width > room && cut.pop().is_some() {
                        main = label(&format!("{}…", cut.trim_end()), t.text, window);
                    }
                    main.paint(point(at.x + px(12.), y + px(4.)), px(20.), window, cx).ok();
                    detail.paint(point(right, y + px(4.)), px(20.), window, cx).ok();
                };
                match choice {
                    Choice::Name(name) => write(name, ""),
                    Choice::Command((name, en, fr, ..)) => write(tr(en, fr), &format!("/{name}")),
                    Choice::Date(en, fr, date) => write(tr(en, fr), &crate::lang::date_name(*date)),
                }
            }
        }
    }
}
