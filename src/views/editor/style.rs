//! Inline styles and line kinds: bold, italic, highlight…, lists, tasks, checkboxes.

use super::*;

impl Editor {
    /// Les lignes de la sélection (ou celle du curseur) écrites dans le style `to` : une seule étape
    /// d'annulation, la sélection garde son texte. Une ligne que la sélection effleure seulement
    /// par son début n'en fait pas partie.
    pub fn restyle(&mut self, to: ListStyle, cx: &mut Context<Self>) {
        let sel = self.sel.clone();
        let first = self.line_range(sel.start);
        let touches = !sel.is_empty() && self.content[..sel.end].ends_with('\n');
        let block = first.start..self.line_range(if touches { sel.end - 1 } else { sel.end }).end;
        if self.in_code(first.start) || block.is_empty() {
            return;
        }
        let old = self.content[block.clone()].to_string();
        let (new, spans) = md::restyle(&old, to);
        if new == old {
            return;
        }
        // La position `at` après le changement : dans le contenu d'une ligne, elle le suit ; dans son
        // marqueur, elle va au début du nouveau contenu.
        let moved = |at: usize| {
            let at = at.clamp(block.start, block.end) - block.start;
            let (mut before, mut after) = (0, 0);
            for ((was, now), &(was_at, now_at)) in old.split('\n').zip(new.split('\n')).zip(&spans) {
                if at <= before + was.len() {
                    let within = at - before;
                    return block.start + after + if within >= was_at { now_at + within - was_at } else { within.min(now_at) };
                }
                before += was.len() + 1;
                after += now.len() + 1;
            }
            block.start + new.len()
        };
        let sel = moved(sel.start)..moved(sel.end);
        self.push_undo(true);
        self.damaged(&block);
        self.content.replace_range(block, &new);
        self.sel = sel;
        self.changed(cx);
        self.renumber(cx);
    }

    /// Texte, puces, numérotée, tâches, puis texte : la ligne du curseur (la première de la
    /// sélection) dit où l'on en est, et la sélection entière passe au suivant.
    pub(super) fn cycle_list(&mut self, cx: &mut Context<Self>) {
        let line = self.line_range(self.sel.start);
        let kind = md::classify(&self.content[line.clone()], self.in_code(line.start)).0;
        let next = match kind {
            Kind::Bullet => ListStyle::Ordered,
            Kind::Ordered => ListStyle::Task,
            Kind::Task(_) => ListStyle::Text,
            _ => ListStyle::Bullet,
        };
        self.restyle(next, cx);
    }

    /// Coche ou décoche la case `[ ]` qui commence à l'octet `at`.
    pub(super) fn flip_box(&mut self, at: usize, cx: &mut Context<Self>) {
        let done = self.content.as_bytes()[at + 1] != b' ';
        self.push_undo(true);
        self.splice(at + 1..at + 2, if done { " " } else { "x" }, cx);
    }

    /// Case à cocher sous le pointeur : son début. Elle est sur la ligne visuelle du pointeur,
    /// entre ses deux bords, et n'est pas dans un bloc de code.
    pub(super) fn checkbox_at(&self, pos: Point<Pixels>) -> Option<usize> {
        let row = &self.rows[self.row_at(self.index_at(pos))?];
        if matches!(row.kind, Kind::Code | Kind::Fence) {
            return None;
        }
        let line = &self.content[row.start..row.start + row.len];
        let local = point(pos.x - self.origin.x, pos.y - self.origin.y - row.y - row.pad);
        md::checkboxes(line, 0).into_iter().find_map(|b| {
            let (from, to) = (row.pos(b.start), row.pos(b.end));
            let inside = local.x >= from.x && local.x <= to.x && local.y >= from.y && local.y < from.y + row.lh;
            (inside && from.y == to.y).then_some(row.start + b.start)
        })
    }

    pub(super) fn toggle_task(&mut self, cx: &mut Context<Self>) {
        let lr = self.line_range(self.cursor());
        let line = &self.content[lr.clone()];
        let (kind, marker) = md::classify(line, self.in_code(lr.start));
        let at = lr.start + marker;
        // Une case écrite dans le texte ou dans une cellule, sous le curseur ou dans sa cellule.
        let c = self.cursor();
        let cell = self.table_cells(c).map(|(cells, k)| cells[k].clone());
        let near = |b: &Range<usize>| match &cell {
            Some(cell) => b.start + lr.start >= cell.start && b.end + lr.start <= cell.end,
            None => (b.start + lr.start..=b.end + lr.start).contains(&c),
        };
        if !self.in_code(lr.start) && !matches!(kind, Kind::Task(_) | Kind::Bullet) {
            if let Some(b) = md::checkboxes(line, 0).into_iter().find(near) {
                return self.flip_box(lr.start + b.start, cx);
            }
            // Une cellule sans case en reçoit une.
            if let Some(cell) = cell {
                self.push_undo(true);
                let text = if cell.is_empty() { "[ ]" } else { "[ ] " };
                self.edit(cell.start..cell.start, text, cx);
                return self.realign(cx);
            }
        }
        match kind {
            Kind::Task(done) => self.splice(at - 3..at - 2, if done { " " } else { "x" }, cx),
            Kind::Bullet => {
                self.splice(at..at, "[ ] ", cx);
                if self.sel.end == at {
                    self.move_to(at + 4, cx);
                }
            }
            _ => {}
        }
    }

    /// Les passages que `mark` entoure : le texte de chaque ligne de la sélection, hors blancs de
    /// bord et puce, sans les blocs de code ; le curseur seul, hors d'un bloc de code, en est un, vide.
    pub(super) fn mark_parts(&self) -> Vec<Range<usize>> {
        let sel = self.sel.clone();
        let first = self.line_range(sel.start);
        let mut code = self.in_code(first.start);
        let mut parts = Vec::new();
        if sel.is_empty() {
            if !code {
                parts.push(sel);
            }
            return parts;
        }
        let mut at = first.start;
        while at <= sel.end {
            let lr = self.line_range(at);
            let line = &self.content[lr.clone()];
            if md::is_fence(line) {
                code = !code && self.content[lr.end..].lines().any(md::is_fence);
            } else if !code {
                let (a, b) = (sel.start.max(lr.start + md::classify(line, false).1), sel.end.min(lr.end));
                if a < b {
                    let text = &self.content[a..b];
                    let lead = text.len() - text.trim_start().len();
                    let kept = text.trim().len();
                    if kept > 0 {
                        parts.push(a + lead..a + lead + kept);
                    }
                }
            }
            at = lr.end + 1;
        }
        parts
    }

    /// Les signes du style `flag` autour de `part` (octets du texte), s'il est tout entier dans un passage de ce style.
    pub(super) fn cut(&self, part: &Range<usize>, flag: u16) -> Option<(Range<usize>, Range<usize>)> {
        let lr = self.line_range(part.start);
        let line = &self.content[lr.clone()];
        let flags = md::flags(line, md::classify(line, false).1);
        let (open, close) = md::unmark(line, &flags, part.start - lr.start..part.end - lr.start, flag)?;
        Some((lr.start + open.start..lr.start + open.end, lr.start + close.start..lr.start + close.end))
    }

    /// Pose le style `flag` sur la sélection, ou le retire quand tout ce qu'elle tient l'a déjà
    /// (voir `md::unmark`). Un curseur seul pose les deux signes et se place entre eux. Sur
    /// plusieurs lignes, chacune reçoit ses propres signes : un style ne passe pas d'une ligne à l'autre.
    pub(super) fn mark(&mut self, flag: u16, cx: &mut Context<Self>) {
        let Some(&(open, close)) = md::delimiters(flag).first() else { return };
        let parts = self.mark_parts();
        if parts.is_empty() {
            return;
        }
        let cuts: Option<Vec<_>> = parts.iter().map(|part| self.cut(part, flag)).collect();
        self.push_undo(true);
        // Tout y est déjà : les signes partent, ceux de fermeture d'abord pour que les octets restent justes.
        if let Some(cuts) = cuts {
            for (open, close) in cuts.into_iter().rev() {
                self.splice(close, "", cx);
                self.splice(open, "", cx);
            }
            return;
        }
        self.wrap(&parts, open, close, cx);
    }

    /// Entoure chaque passage de `open` et `close` ; la sélection va du premier au dernier, signes dehors.
    pub(super) fn wrap(&mut self, parts: &[Range<usize>], open: &str, close: &str, cx: &mut Context<Self>) {
        let (Some(head), Some(tail)) = (parts.first().map(|p| p.start), parts.last().map(|p| p.end)) else { return };
        for part in parts.iter().rev() {
            self.splice(part.end..part.end, close, cx);
            self.splice(part.start..part.start, open, cx);
        }
        let grown = parts.len() * open.len() + (parts.len() - 1) * close.len();
        self.sel = head + open.len()..tail + grown;
        self.reversed = false;
    }
}
