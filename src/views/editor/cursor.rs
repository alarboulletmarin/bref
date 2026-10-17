//! Cursors and selections: moving, extending, several cursors at once.

use super::*;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Motion {
    Left,
    Right,
    Up,
    Down,
    WordLeft,
    WordRight,
    Home,
    End,
    DocStart,
    DocEnd,
    PageUp,
    PageDown,
}

impl Editor {
    #[cfg(test)]
    pub fn caret(&self) -> usize {
        self.cursor()
    }

    /// Met le curseur à l'octet `at` (le plus proche qui soit valide).
    #[cfg(test)]
    pub fn place_cursor(&mut self, at: usize, cx: &mut Context<Self>) {
        let at = self.clamp(at);
        self.move_to(at, cx);
    }

    /// Place le curseur à `at` ; avec `top`, sa ligne monte en haut de la vue (un titre garde
    /// sa section sous lui), sinon elle défile juste assez pour se voir.
    pub fn jump(&mut self, at: usize, top: bool, cx: &mut Context<Self>) {
        self.more.clear();
        self.move_to(self.clamp(at), cx);
        self.reveal_top = top;
    }

    /// Résultat de la recherche du coffre : à la ligne de rang `row`, sélectionne le premier
    /// passage qui vaut `query` (sans la casse) ; à défaut, le curseur va au début de la ligne.
    pub fn select_in_row(&mut self, row: usize, query: &str, cx: &mut Context<Self>) {
        let start: usize = self.content.split_inclusive('\n').take(row).map(str::len).sum();
        let line = self.line_range(start);
        self.jump(line.start, false, cx);
        if let Some(hit) = md::find(&self.content[line.clone()], query, false, false).first() {
            self.sel = line.start + hit.start..line.start + hit.end;
        }
    }

    pub(super) fn cursor(&self) -> usize {
        if self.reversed { self.sel.start } else { self.sel.end }
    }

    pub(super) fn clamp(&self, mut i: usize) -> usize {
        i = i.min(self.content.len());
        while !self.content.is_char_boundary(i) {
            i -= 1;
        }
        i
    }

    pub(super) fn line_range(&self, at: usize) -> Range<usize> {
        let start = self.content[..at].rfind('\n').map_or(0, |i| i + 1);
        let end = self.content[at..].find('\n').map_or(self.content.len(), |i| at + i);
        start..end
    }

    pub(super) fn move_to(&mut self, to: usize, cx: &mut Context<Self>) {
        self.bar.swatches = false;
        self.bar.open = false;
        self.sel = to..to;
        self.reversed = false;
        self.goal_x = None;
        self.grid = None;
        self.reveal = true;
        cx.notify();
    }

    pub(super) fn select_to(&mut self, to: usize, cx: &mut Context<Self>) {
        if self.reversed {
            self.sel.start = to
        } else {
            self.sel.end = to
        }
        if self.sel.end < self.sel.start {
            self.reversed = !self.reversed;
            self.sel = self.sel.end..self.sel.start;
        }
        self.goal_x = None;
        self.reveal = true;
        cx.notify();
    }

    /// Nombre de curseurs, le principal compris.
    #[cfg(test)]
    pub fn cursors(&self) -> usize {
        self.more.len() + 1
    }

    /// Range les autres curseurs dans l'ordre du texte, et retire ceux qui en touchent un autre.
    pub(super) fn tidy(&mut self) {
        self.more.sort_by_key(|(sel, _)| sel.start);
        let (mut kept, main): (Vec<(Range<usize>, bool)>, _) = (Vec::new(), self.sel.clone());
        let touch = |a: &Range<usize>, b: &Range<usize>| a.start <= b.end && b.start <= a.end;
        for cursor in std::mem::take(&mut self.more) {
            if !touch(&cursor.0, &main) && kept.last().is_none_or(|last| !touch(&last.0, &cursor.0)) {
                kept.push(cursor);
            }
        }
        self.more = kept;
    }

    /// Alt+clic : un curseur de plus à l'octet `at`, qui devient le principal. Pas dans un
    /// tableau, qui a ses propres gestes.
    pub(super) fn add_cursor(&mut self, at: usize, cx: &mut Context<Self>) {
        if self.table_at(at).is_some() || self.table_at(self.sel.start).is_some() {
            return self.move_to(at, cx);
        }
        self.more.push((self.sel.clone(), self.reversed));
        self.move_to(at, cx);
        self.tidy();
    }

    /// Sélectionne le mot sous le curseur ; puis, à chaque appel, l'occurrence suivante de la
    /// sélection reçoit un curseur de plus (elle se cherche en boucle, la casse compte).
    pub(super) fn select_next(&mut self, cx: &mut Context<Self>) {
        if self.sel.is_empty() {
            let word = self.unit_at(self.sel.start, false);
            self.move_to(word.start, cx);
            return self.select_to(word.end, cx);
        }
        let text = self.content[self.sel.clone()].to_string();
        let taken = |at: usize| self.sel.start == at || self.more.iter().any(|(sel, _)| sel.start == at);
        let after = self.content[self.sel.end..].match_indices(&text).map(|(i, _)| i + self.sel.end);
        let before = self.content[..self.sel.start].match_indices(&text).map(|(i, _)| i);
        let Some(at) = after.chain(before).find(|at| !taken(*at)) else {
            return;
        };
        self.more.push((self.sel.clone(), self.reversed));
        self.move_to(at, cx);
        self.select_to(at + text.len(), cx);
        self.tidy();
    }

    pub(super) fn row_at(&self, i: usize) -> Option<usize> {
        self.rows
            .partition_point(|r| r.start <= i)
            .checked_sub(1)
    }

    /// Ligne voisine qu'on voit : la ligne de tirets d'un tableau se saute.
    pub(super) fn neighbour(&self, r: usize, down: bool) -> Option<usize> {
        let step = |r: usize| if down { Some(r + 1) } else { r.checked_sub(1) };
        let mut r = step(r)?;
        while self.rows.get(r)?.hidden() {
            r = step(r)?;
        }
        Some(r)
    }

    pub(super) fn vertical(&self, from: usize, down: bool, gx: Pixels) -> usize {
        let Some(r) = self.row_at(from) else {
            return from;
        };
        let row = &self.rows[r];
        let v = (f32::from(row.pos(from - row.start).y) / f32::from(row.lh)).round() as i32;
        // Dans une cellule, on descend ligne visuelle après ligne visuelle, puis on passe à la ligne suivante.
        let (r, v) = if down {
            if v + 1 < row.lines_at(from - row.start) {
                (r, v + 1)
            } else if let Some(next) = self.neighbour(r, true) {
                (next, 0)
            } else {
                return self.content.len();
            }
        } else if v > 0 {
            (r, v - 1)
        } else if let Some(prev) = self.neighbour(r, false) {
            (prev, self.rows[prev].visual_rows() - 1)
        } else {
            return 0;
        };
        let row = &self.rows[r];
        self.clamp(row.start + row.index_at(point(gx, row.lh * (v as f32 + 0.5))))
    }

    pub(super) fn go(&mut self, m: Motion, select: bool, cx: &mut Context<Self>) {
        // Sélecteur de tableau : les flèches règlent sa taille.
        if let Some((cols, rows)) = &mut self.grid {
            match m {
                Motion::Left => *cols = (*cols - 1).max(1),
                Motion::Right => *cols = (*cols + 1).min(GRID_MAX.0),
                Motion::Up => *rows = (*rows - 1).max(1),
                Motion::Down => *rows = (*rows + 1).min(GRID_MAX.1),
                _ => self.grid = None,
            }
            self.grid_typed.clear();
            return cx.notify();
        }
        if !select
            && matches!(m, Motion::Up | Motion::Down)
            && let Some((_, items)) = self.completion()
        {
            let n = items.len();
            let i = self.ac_index.min(n - 1);
            self.ac_index = if m == Motion::Down { (i + 1) % n } else { (i + n - 1) % n };
            cx.notify();
            return;
        }
        let c = self.cursor();
        let gx = self
            .goal_x
            .unwrap_or_else(|| self.row_at(c).map_or(px(0.), |r| {
                let row = &self.rows[r];
                row.pos(c.saturating_sub(row.start)).x
            }));
        let to = match m {
            Motion::Left if !select && !self.sel.is_empty() => self.sel.start,
            Motion::Right if !select && !self.sel.is_empty() => self.sel.end,
            _ => self.target(m, gx),
        };
        if select {
            self.select_to(to, cx)
        } else {
            self.move_to(to, cx)
        }
        if matches!(m, Motion::Up | Motion::Down | Motion::PageUp | Motion::PageDown) {
            self.goal_x = Some(gx);
        }
    }
}
