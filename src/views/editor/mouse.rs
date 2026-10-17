//! The mouse: clicks, drags, the scroll wheel, and picking table rows and columns.

use super::*;

/// Lignes et colonnes (rangs visibles) prises par une poignée. Valable tant que le texte et la
/// sélection sont ceux d'alors : rien d'autre à défaire.
#[derive(Clone)]
pub(super) struct Pick {
    pub(super) table: usize,
    pub(super) rows: Range<usize>,
    pub(super) cols: Range<usize>,
    pub(super) sel: Range<usize>,
    pub(super) version: u64,
}

/// Une ligne ou une colonne qu'on déplace en tirant sa poignée.
pub(super) struct Moving {
    pub(super) table: usize,
    pub(super) part: Part,
    pub(super) from: Point<Pixels>,
    /// Le trait où elle se poserait.
    pub(super) to: Option<usize>,
}

/// Ce que le pointeur désigne autour d'un tableau : sa colonne, sa ligne, et si c'est le coin.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub(super) struct Near {
    pub(super) col: Option<usize>,
    pub(super) row: Option<usize>,
    pub(super) corner: bool,
}

impl Editor {
    /// Point de l'écran où se trouve l'octet `i`, au milieu de sa ligne.
    #[cfg(test)]
    pub fn point_of(&self, i: usize) -> Option<Point<Pixels>> {
        let row = &self.rows[self.row_at(i)?];
        Some(self.origin + point(px(0.), row.y + row.pad + row.lh / 2.) + row.pos(i - row.start))
    }

    #[cfg(test)]
    pub fn hand(&self) -> Option<CursorStyle> {
        self.hand
    }

    /// Où poser ce qu'on dépose au pointeur `at` : le début d'une ligne, celle du dessous quand le pointeur est
    /// dans la moitié basse de la sienne. Jamais dans un tableau, un bloc de code ni l'en-tête YAML : au
    /// bord du bloc, vers le bas.
    pub fn drop_point(&self, at: Point<Pixels>) -> usize {
        let y = at.y - self.origin.y;
        let Some(row) = self.rows.iter().find(|r| y < r.y + r.height).or(self.rows.last()) else {
            return 0;
        };
        let line = self.line_range(row.start);
        let mut point = if y > row.y + row.height / 2. { (line.end + 1).min(self.content.len()) } else { line.start };
        let front = md::front_matter(&self.content);
        if point > 0 && point < front {
            point = front;
        }
        // Dans un tableau (sa ligne et la précédente en sont) ou dans un bloc de code : plus bas.
        while point > 0 && point < self.content.len() {
            let (here, above) = (self.line_range(point), self.line_range(point - 1));
            if (self.is_table_line(&here) && self.is_table_line(&above)) || self.in_code(point) {
                point = (here.end + 1).min(self.content.len());
            } else {
                break;
            }
        }
        point
    }

    pub fn set_drop_mark(&mut self, mark: Option<usize>, cx: &mut Context<Self>) {
        if self.drop_mark != mark {
            self.drop_mark = mark;
            cx.notify();
        }
    }

    /// Le début de la ligne du curseur.
    pub fn caret_line_start(&self) -> usize {
        self.line_range(self.cursor()).start
    }

    /// La poignée sous le pointeur : (début du tableau, poignée). Seules celles qu'on voit se saisissent.
    pub(super) fn part_at(&self, at: Point<Pixels>) -> Option<(usize, Part)> {
        self.geos.iter().find_map(|geo| {
            let (near, pick) = (geo.near(at), self.live_pick(geo.start));
            let hit = geo.parts().into_iter().find(|(part, zone)| zone.dilate(px(2.)).contains(&at) && geo.visible(*part, near, pick))?;
            Some((geo.start, hit.0))
        })
    }

    /// Ce qu'une poignée a pris dans le tableau qui commence en `table`, tant que rien n'a bougé.
    pub(super) fn live_pick(&self, table: usize) -> Option<&Pick> {
        self.pick.as_ref().filter(|p| p.table == table && p.version == self.version && p.sel == self.sel)
    }

    /// Les cases `rows` × `cols` (rangs visibles) du tableau qui commence en `table` sont prises : le
    /// curseur va dans la première, et les gestes, la copie et l'effacement valent pour toutes.
    pub(super) fn pick(&mut self, table: usize, rows: Range<usize>, cols: Range<usize>, cx: &mut Context<Self>) {
        let Some((range, _, _)) = self.table_at(table) else {
            return;
        };
        let line = if rows.start == 0 { 0 } else { rows.start + 1 };
        let text = &self.content[range.clone()];
        let at = range.start + text.split('\n').take(line).map(|l| l.len() + 1).sum::<usize>();
        let cell = md::cells(text.split('\n').nth(line).unwrap_or_default()).get(cols.start).cloned().unwrap_or(0..0);
        self.move_to(at + cell.start, cx);
        self.select_to(at + cell.end, cx);
        self.pick = Some(Pick { table: range.start, rows, cols, sel: self.sel.clone(), version: self.version });
        cx.notify();
    }

    /// Prend ce que désigne une poignée.
    pub(super) fn pick_part(&mut self, table: usize, part: Part, cx: &mut Context<Self>) {
        let Some(geo) = self.geos.iter().find(|g| g.start == table) else {
            return;
        };
        let (tall, cols) = (geo.tall(), geo.cols());
        match part {
            Part::Col(c) => self.pick(table, 0..tall, c..c + 1, cx),
            Part::Row(r) => self.pick(table, r..r + 1, 0..cols, cx),
            Part::Corner => self.pick(table, 0..tall, 0..cols, cx),
            Part::JoinCol(_) | Part::JoinRow(_) => {}
        }
    }

    /// Les cases prises, en TSV.
    pub(super) fn pick_text(&self) -> Option<String> {
        let (range, rows, _) = self.table_at(self.sel.start)?;
        let pick = self.live_pick(range.start)?;
        let line = |v: usize| if v == 0 { 0 } else { v + 1 };
        let cells = |v: usize| rows[line(v)][pick.cols.clone()].iter().map(|c| c.replace("\\|", "|")).collect::<Vec<_>>().join("\t");
        Some(pick.rows.clone().map(cells).collect::<Vec<_>>().join("\n"))
    }

    /// Vide les cases prises, sans toucher à la forme du tableau. Faux s'il n'y en a pas.
    pub(super) fn clear_pick(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((range, mut rows, _)) = self.table_at(self.sel.start) else {
            return false;
        };
        let Some(pick) = self.live_pick(range.start).cloned() else {
            return false;
        };
        for v in pick.rows.clone() {
            for c in pick.cols.clone() {
                rows[if v == 0 { 0 } else { v + 1 }][c].clear();
            }
        }
        self.set_table(range.clone(), &rows, (if pick.rows.start == 0 { 0 } else { pick.rows.start + 1 }, pick.cols.start), cx);
        self.pick(range.start, pick.rows, pick.cols, cx);
        true
    }

    /// Un clic sur une poignée : « + » insère, une poignée prend sa ligne ou sa colonne (et la tire,
    /// si on la déplace), l'angle prend tout le tableau.
    pub(super) fn grip_down(&mut self, table: usize, part: Part, at: Point<Pixels>, cx: &mut Context<Self>) {
        match part {
            Part::JoinCol(b) | Part::JoinRow(b) => {
                let Some((range, rows, _)) = self.table_at(table) else {
                    return;
                };
                let cols = matches!(part, Part::JoinCol(_));
                let grid = if cols { md::insert_col(&rows, b) } else { md::insert_row(&rows, b) };
                self.set_table(range, &grid, if cols { (0, b) } else { (b + 1, 0) }, cx);
            }
            _ => {
                self.pick_part(table, part, cx);
                // L'en-tête reste en tête : sa ligne se prend, elle ne se tire pas.
                if matches!(part, Part::Col(_) | Part::Row(1..)) {
                    self.moving = Some(Moving { table, part, from: at, to: None });
                }
            }
        }
    }

    /// La poignée tirée est lâchée sur le trait `to` : la ligne ou la colonne s'y pose, et reste prise.
    pub(super) fn drop_part(&mut self, moving: Moving, cx: &mut Context<Self>) {
        let (Some(b), Some((range, rows, _))) = (moving.to, self.table_at(moving.table)) else {
            return;
        };
        let (from, grid) = match moving.part {
            Part::Col(n) | Part::Row(n) => (n, if b > n { b - 1 } else { b }),
            _ => return,
        };
        if from == grid {
            return;
        }
        let (cols, tall) = (rows[0].len(), md::visible_rows(&rows));
        let moved = if matches!(moving.part, Part::Col(_)) { md::move_col(&rows, from, grid) } else { md::move_row(&rows, from, grid) };
        self.set_table(range.clone(), &moved, (0, 0), cx);
        if matches!(moving.part, Part::Col(_)) {
            self.pick(range.start, 0..tall, grid..grid + 1, cx);
        } else {
            self.pick(range.start, grid..grid + 1, 0..cols, cx);
        }
    }

    pub(super) fn copy_bounds(&self, row: &Row) -> Bounds<Pixels> {
        let at = point(self.width - COPY_SIZE, row.y + row.pad + (row.lh - COPY_SIZE) / 2.);
        Bounds::new(self.origin + at, size(COPY_SIZE, COPY_SIZE))
    }

    /// Début de la ligne dont le bouton de copie est sous le pointeur.
    pub(super) fn copy_at(&self, at: Point<Pixels>) -> Option<usize> {
        let row = self.rows.iter().find(|r| r.opens && self.copy_bounds(r).contains(&at))?;
        Some(row.start)
    }

    /// La ligne ouvre un bloc Mermaid : elle porte aussi le bouton qui montre le diagramme en grand,
    /// à gauche de celui de copie.
    pub(super) fn views(&self, row: &Row) -> bool {
        let line = &self.content[row.start..row.start + row.len];
        row.opens && line.trim_start().get(3..).is_some_and(|lang| lang.trim().eq_ignore_ascii_case("mermaid"))
    }

    pub(super) fn view_bounds(&self, row: &Row) -> Bounds<Pixels> {
        let copy = self.copy_bounds(row);
        Bounds::new(copy.origin - point(COPY_SIZE + px(4.), px(0.)), copy.size)
    }

    /// Début de la ligne dont le bouton « en grand » est sous le pointeur.
    pub(super) fn view_at(&self, at: Point<Pixels>) -> Option<usize> {
        let row = self.rows.iter().find(|r| self.views(r) && self.view_bounds(r).contains(&at))?;
        Some(row.start)
    }

    pub(super) fn index_at(&self, pos: Point<Pixels>) -> usize {
        let y = pos.y - self.origin.y;
        let Some(row) = self.rows.iter().find(|r| y < r.y + r.height).or(self.rows.last()) else {
            return 0;
        };
        let local = point(pos.x - self.origin.x, (y - row.y - row.pad).max(px(0.)));
        self.clamp(row.start + row.index_at(local))
    }

    pub(super) fn mouse_down(&mut self, e: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(find) = &mut self.find {
            find.active = false;
        }
        // Diagramme Mermaid en grand, dans la visionneuse.
        if let Some(start) = self.view_at(e.position)
            && let Some(block) = md::code_block(&self.content, self.line_range(start).end + 1)
        {
            return cx.emit(EditorEvent::View(self.content[block].to_string()));
        }
        // Bouton de copie, à droite de l'ouverture d'un bloc de code.
        if let Some(start) = self.copy_at(e.position)
            && let Some(block) = md::code_block(&self.content, self.line_range(start).end + 1)
        {
            cx.write_to_clipboard(ClipboardItem::new_string(self.content[block].to_string()));
            self.copied = Some(start);
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(Duration::from_millis(1500)).await;
                this.update(cx, |this, cx| {
                    this.copied = None;
                    cx.notify();
                })
                .ok();
            })
            .detach();
            return cx.notify();
        }
        if let Some((cols, rows)) = self.grid_at(e.position) {
            return self.insert_table(cols, rows, cx);
        }
        self.grid = None;
        // Poignée d'un bloc : la tirer le déplace, un clic ouvre son menu ; le « + » ajoute un bloc dessous.
        if let Some((grip, plus)) = self.handle_bounds()
            && let Some(block) = self.handled()
        {
            if plus.is_some_and(|plus| plus.contains(&e.position)) {
                return self.add_block_after(block, cx);
            }
            if grip.contains(&e.position) {
                self.handles.drag = Some(BlockDrag { block, from: e.position, to: None, moved: false });
                return cx.notify();
            }
        }
        // Poignées d'un tableau : prendre une ligne, une colonne ou tout, insérer à un trait.
        if let Some((table, part)) = self.part_at(e.position) {
            return self.grip_down(table, part, e.position, cx);
        }
        // Boutons « + » d'un tableau : une ligne à la fin, ou une colonne.
        if let Some((at, bar)) = self.plus_at(e.position)
            && let Some((range, mut rows, _)) = self.table_at(at)
        {
            let cell = if bar == 0 {
                rows.push(vec![String::new(); rows[0].len()]);
                (rows.len() - 1, 0)
            } else {
                let cells = rows.iter_mut().enumerate();
                cells.for_each(|(i, row)| row.push(if i == 1 { "---" } else { "" }.to_string()));
                (0, rows[0].len() - 1)
            };
            return self.set_table(range, &rows, cell, cx);
        }
        let i = self.index_at(e.position);
        if e.modifiers.alt {
            return self.add_cursor(i, cx);
        }
        // Un clic ordinaire revient à un seul curseur.
        self.more.clear();
        let lr = self.line_range(i);
        let line = &self.content[lr.clone()];
        let col = i - lr.start;
        if e.modifiers.secondary()
            && let Some((_, link)) = md::links(line).into_iter().find(|(r, _)| r.contains(&col))
        {
            return cx.emit(if e.modifiers.shift { EditorEvent::OpenAside(link) } else { EditorEvent::Open(link) });
        }
        if !e.modifiers.secondary()
            && let Some(at) = self.checkbox_at(e.position)
        {
            return self.flip_box(at, cx);
        }
        let (kind, marker) = md::classify(line, self.in_code(lr.start));
        if matches!(kind, Kind::Task(_)) && col + 4 >= marker && col < marker {
            self.move_to(lr.end, cx);
            return self.toggle_task(cx);
        }
        match e.click_count {
            n @ (2 | 3) => {
                let unit = self.unit_at(i, n == 3);
                self.move_to(unit.start, cx);
                self.select_to(unit.end, cx);
                self.selecting = true;
                self.anchor = Some((unit, n == 3));
            }
            _ => {
                self.selecting = true;
                self.anchor = None;
                if e.modifiers.shift {
                    self.select_to(i, cx)
                } else {
                    self.move_to(i, cx)
                }
            }
        }
    }

    /// Clic droit : le menu de la note. Hors de la sélection, le curseur vient d'abord sous
    /// le pointeur, pour que « Coller » et « Lien » agissent là où l'on a cliqué.
    pub(super) fn menu_down(&mut self, e: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        // Sur une poignée de tableau : elle prend sa ligne ou sa colonne, et le menu en parle.
        if let Some((table, part)) = self.part_at(e.position)
            && matches!(part, Part::Col(_) | Part::Row(_) | Part::Corner)
        {
            self.pick_part(table, part, cx);
            return cx.emit(EditorEvent::Menu(e.position, None, false));
        }
        let i = self.index_at(e.position);
        if !(self.sel.start..=self.sel.end).contains(&i) {
            self.move_to(i, cx);
        }
        let lr = self.line_range(i);
        let link = md::links(&self.content[lr.clone()]).into_iter().find(|(r, _)| (r.start..=r.end).contains(&(i - lr.start)));
        let col = i - lr.start;
        let commented = md::comments(&self.content[lr]).iter().any(|(whole, ..)| (whole.start..=whole.end).contains(&col));
        cx.emit(EditorEvent::Menu(e.position, link.map(|(_, link)| link), commented));
    }

    /// Mot (ou ligne entière) autour de l'octet `i`.
    pub(super) fn unit_at(&self, i: usize, by_line: bool) -> Range<usize> {
        let lr = self.line_range(i);
        if by_line {
            return lr;
        }
        let col = i - lr.start;
        let word = self.content[lr.clone()]
            .split_word_bound_indices()
            .find(|(s, w)| (*s..=s + w.len()).contains(&col))
            .map_or(col..col, |(s, w)| s..s + w.len());
        lr.start + word.start..lr.start + word.end
    }

    /// Le glisser étend la sélection jusqu'à `i` ; après un double ou triple clic,
    /// par mots ou par lignes entières, le mot ou la ligne saisis restant sélectionnés.
    pub(super) fn drag_to(&mut self, i: usize, cx: &mut Context<Self>) {
        let Some((first, by_line)) = self.anchor.clone() else {
            return self.select_to(i, cx);
        };
        let unit = self.unit_at(i, by_line);
        self.reversed = unit.start < first.start;
        self.sel = if self.reversed { unit.start..first.end } else { first.start..unit.end.max(first.end) };
        self.goal_x = None;
        self.reveal = true;
        cx.notify();
    }

    pub(super) fn mouse_move(&mut self, e: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        // Un bouton relâché hors de la fenêtre n'envoie pas de `mouse_up` : on ne
        // continue pas à sélectionner pointeur libre.
        if self.selecting && e.pressed_button != Some(MouseButton::Left) {
            self.selecting = false;
        }
        if self.selecting {
            self.drag_to(self.index_at(e.position), cx);
        }
        self.aim_hand(e.position, e.modifiers, cx);
        if self.handles.drag.is_some() {
            if e.pressed_button != Some(MouseButton::Left) {
                self.handles.drag = None;
                return cx.notify();
            }
            return self.drag_block(e.position, cx);
        }
        self.aim_block(e.position, cx);
        // Une poignée tirée : le trait où elle se poserait suit le pointeur, après quelques pixels.
        if let Some(moving) = &mut self.moving {
            if e.pressed_button != Some(MouseButton::Left) {
                self.moving = None;
            } else if (e.position - moving.from).magnitude() > 4. {
                let at = self.geos.iter().find(|g| g.start == moving.table).map(|geo| geo.boundary(moving.part, e.position));
                moving.to = at;
            }
            return cx.notify();
        }
        let part = self.part_at(e.position);
        let near = self.geos.iter().map(|geo| geo.near(e.position)).collect::<Vec<_>>();
        let before = self.geos.iter().map(|geo| geo.near(self.pointer)).collect::<Vec<_>>();
        self.pointer = e.position;
        if part != self.part_hover || near != before {
            self.part_hover = part;
            cx.notify();
        }
        let hover = (self.copy_at(e.position), self.view_at(e.position));
        if hover != (self.hover, self.view_hover) {
            (self.hover, self.view_hover) = hover;
            cx.notify();
        }
        if let Some(cell) = self.grid_at(e.position)
            && self.grid != Some(cell)
        {
            self.grid = Some(cell);
            self.grid_typed.clear();
            cx.notify();
        }
        // Un tableau montre ses boutons « + » dès qu'on s'en approche.
        let bar = self.plus_at(e.position);
        let near = |(range, frame): &(Range<usize>, Bounds<Pixels>)| {
            frame.dilate(px(20.)).contains(&e.position).then_some((range.start, None))
        };
        let over = bar.map(|(at, bar)| (at, Some(bar))).or_else(|| self.tables.iter().find_map(near));
        if over != self.table_hover {
            self.table_hover = over;
            cx.notify();
        }
    }

    pub(super) fn mouse_up(&mut self, e: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.selecting = false;
        self.drop_block(cx);
        if let Some(moving) = self.moving.take() {
            self.drop_part(moving, cx);
            cx.notify();
        }
        self.aim_hand(e.position, e.modifiers, cx);
    }

    pub(super) fn scroll(&mut self, e: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        let delta = e.delta.pixel_delta(px(26.));
        // Maj + molette, ou geste horizontal du pavé tactile : un tableau trop large défile.
        let (dx, dy) = if e.modifiers.shift { (delta.y, delta.x) } else { (delta.x, delta.y) };
        if let Some((range, _)) = self.tables.iter().find(|(_, frame)| frame.contains(&e.position)) {
            *self.table_x.entry(range.start).or_default() -= dx;
        }
        self.scroll_y -= dy;
        cx.notify();
    }
}
