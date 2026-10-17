//! Changing the text: one undoable edit at a time, applied to every cursor.

use super::*;

/// États du texte qu'on peut retrouver, du plus ancien au plus récent. Seul le
/// dernier est gardé en entier : chacun des autres ne retient que ce qui le
/// distingue du suivant, soit quelques octets par frappe plutôt qu'une copie de la note.
#[derive(Default)]
pub(super) struct History {
    pub(super) last: Option<(String, Range<usize>)>,
    /// (début de la différence, sa longueur dans l'état suivant, son texte ici, sélection).
    pub(super) earlier: Vec<(usize, usize, String, Range<usize>)>,
}

impl History {
    pub(super) fn push(&mut self, text: String, sel: Range<usize>) {
        if let Some((old, old_sel)) = self.last.replace((text, sel)) {
            let new = self.last.as_ref().unwrap().0.as_bytes();
            let old_bytes = old.as_bytes();
            let mut start = old_bytes.iter().zip(new).take_while(|(a, b)| a == b).count();
            while !old.is_char_boundary(start) {
                start -= 1;
            }
            let room = old.len().min(new.len()) - start;
            let mut tail = old_bytes.iter().rev().zip(new.iter().rev()).take(room).take_while(|(a, b)| a == b).count();
            while !old.is_char_boundary(old.len() - tail) {
                tail -= 1;
            }
            self.earlier.push((start, new.len() - tail - start, old[start..old.len() - tail].to_string(), old_sel));
            if self.earlier.len() >= 200 {
                self.earlier.remove(0);
            }
        }
    }

    pub(super) fn pop(&mut self) -> Option<(String, Range<usize>)> {
        let out = self.last.take()?;
        if let Some((start, len, own, sel)) = self.earlier.pop() {
            let mut text = out.0.clone();
            text.replace_range(start..start + len, &own);
            self.last = Some((text, sel));
        }
        Some(out)
    }
}

impl Editor {
    // ponytail: chaque étape d'annulation (200 max) recopie une fois la note pour
    // en tirer la différence ; passer à un journal d'opérations si les notes
    // dépassent le Mo.
    pub(super) fn push_undo(&mut self, boundary: bool) {
        let stale = self
            .last_edit
            .is_none_or(|t| t.elapsed() > Duration::from_millis(600));
        if (boundary || stale) && !self.held {
            self.undo.push(self.content.clone(), self.sel.clone());
        }
        self.redo = History::default();
        self.last_edit = Some(Instant::now());
    }

    /// À appeler avant de remplacer `range` dans le texte : la mise en page saura ce qui a bougé.
    pub(super) fn damaged(&mut self, range: &Range<usize>) {
        let len = self.content.len();
        let (prefix, suffix) = (range.start, len - range.end);
        self.dmg = Some(match self.dmg {
            None => Damage { old_len: len, prefix, suffix },
            Some(d) => Damage { prefix: d.prefix.min(prefix), suffix: d.suffix.min(suffix), ..d },
        });
    }

    pub(super) fn changed(&mut self, cx: &mut Context<Self>) {
        self.bar.swatches = false;
        self.version += 1;
        self.revision += 1;
        self.marked = None;
        self.copied = None;
        self.grid = None;
        self.ac_index = 0;
        self.goal_x = None;
        self.reveal = true;
        cx.emit(EditorEvent::Changed);
        cx.notify();
    }

    /// Remplace `range` par `text` en conservant la sélection au mieux.
    pub(super) fn splice(&mut self, range: Range<usize>, text: &str, cx: &mut Context<Self>) {
        self.push_undo(text.contains('\n') || text == " ");
        self.damaged(&range);
        self.content.replace_range(range.clone(), text);
        let map = |p: usize| {
            if p <= range.start {
                p
            } else if p >= range.end {
                p + text.len() - range.len()
            } else {
                range.start + text.len()
            }
        };
        self.sel = map(self.sel.start)..map(self.sel.end);
        // Les autres curseurs suivent le texte : c'est ici, et ici seulement, qu'il change.
        self.more.iter_mut().for_each(|(sel, _)| *sel = map(sel.start)..map(sel.end));
        self.changed(cx);
    }

    /// Fait faire `act` à chaque curseur : chacun devient à son tour le curseur principal,
    /// pendant que `splice` tient les autres à leur place. Le tout ne fait qu'une étape
    /// d'annulation, celle du premier.
    // ponytail: la colonne visée par Haut et Bas (`goal_x`) n'est pas gardée par curseur : elle
    // est recalculée à chaque pas. La retenir par curseur si la dérive gêne sur des lignes
    // de longueurs inégales.
    pub(super) fn each(&mut self, cx: &mut Context<Self>, act: impl Fn(&mut Self, &mut Context<Self>)) {
        if self.more.is_empty() {
            return act(self, cx);
        }
        self.goal_x = None;
        act(self, cx);
        self.held = true;
        for i in 0..self.more.len() {
            // Un geste qui ramène à un seul curseur (une note rechargée) arrête la ronde.
            if i >= self.more.len() {
                break;
            }
            let swap = |this: &mut Self| {
                std::mem::swap(&mut this.sel, &mut this.more[i].0);
                std::mem::swap(&mut this.reversed, &mut this.more[i].1);
            };
            swap(self);
            self.goal_x = None;
            act(self, cx);
            if i < self.more.len() {
                swap(self);
            }
        }
        self.held = false;
        self.goal_x = None;
        self.tidy();
        cx.notify();
    }

    /// Le texte sélectionné.
    #[cfg(test)]
    pub fn selected_text(&self) -> String {
        self.content[self.sel.clone()].to_string()
    }

    /// Remplace `range` par `text` et place le curseur juste après.
    pub(super) fn edit(&mut self, range: Range<usize>, text: &str, cx: &mut Context<Self>) {
        let end = range.start + text.len();
        self.splice(range, text, cx);
        self.sel = end..end;
        self.reversed = false;
    }

    pub(super) fn renumber(&mut self, cx: &mut Context<Self>) {
        if let Some((text, cursor)) = md::renumber(&self.content, self.cursor()) {
            self.epoch += 1;
            self.content = text;
            self.sel = cursor..cursor;
            self.reversed = false;
            self.more.clear();
            self.changed(cx);
        }
    }

    pub fn restore(&mut self, redo: bool, cx: &mut Context<Self>) {
        let (from, to) = if redo {
            (&mut self.redo, &mut self.undo)
        } else {
            (&mut self.undo, &mut self.redo)
        };
        let Some((text, sel)) = from.pop() else {
            return;
        };
        self.epoch += 1;
        to.push(std::mem::replace(&mut self.content, text), self.sel.clone());
        self.sel = sel;
        self.reversed = false;
        self.more.clear();
        self.last_edit = None;
        self.changed(cx);
    }

    pub(super) fn delete(&mut self, m: Motion, cx: &mut Context<Self>) {
        if self.clear_pick(cx) {
            return;
        }
        if self.grid.is_some() {
            self.grid_typed.pop();
            self.type_grid("", cx);
            return;
        }
        if self.clear_cells(cx) {
            return;
        }
        let range = if self.sel.is_empty() {
            let (c, t) = (self.cursor(), self.plain_target(m, px(0.)));
            let mut range = c.min(t)..c.max(t);
            // Dans un tableau, effacer ne passe pas d'une cellule à l'autre : cela fusionnerait
            // les colonnes. Un `\|` s'efface d'un bloc, comme il s'affiche.
            if let Some((_, k)) = self.table_cells(c) {
                let line = self.line_range(c);
                let bars = md::bars(&self.content[line.clone()]);
                let (from, to) = (line.start + bars[k] + 1, bars.get(k + 1).map_or(line.end, |b| line.start + b));
                range = range.start.max(from)..range.end.min(to);
                if range.is_empty() {
                    return;
                }
                range = self.off_escape(range.start, false)..self.off_escape(range.end, true);
            }
            range
        } else {
            self.sel.clone()
        };
        self.edit(range, "", cx);
        self.renumber(cx);
        self.realign(cx);
    }

    pub(super) fn newline(&mut self, cx: &mut Context<Self>) {
        if self.accept_completion(cx) {
            return;
        }
        if let Some((cols, rows)) = self.grid {
            return self.insert_table(cols, rows, cx);
        }
        // Dans un tableau : une ligne de plus sous celle du curseur ; sur une
        // dernière ligne restée vide, on la retire et on sort du tableau.
        // Un très gros tableau n'est pas relu : Entrée y ajoute une ligne vide sous la courante (sous les tirets
        // quand on est sur l'en-tête), sans toucher au reste.
        if self.sel.is_empty() && self.in_big(self.cursor()) {
            let line = self.line_range(self.cursor());
            let cells = md::cells(&self.content[line.clone()]).len().max(1);
            let after = match self.row_at(self.cursor()).and_then(|r| self.big.iter().find(|b| b.rows.contains(&r)).map(|b| r - b.rows.start)) {
                Some(0) if line.end < self.content.len() => self.line_range(line.end + 1).end,
                _ => line.end,
            };
            self.push_undo(true);
            self.edit(after..after, &format!("\n|{}", " |".repeat(cells)), cx);
            return self.move_to(after + 3, cx);
        }
        if let Some((range, mut rows, (r, _))) = self.table_at(self.cursor()) {
            if r >= 2 && r + 1 == rows.len() && rows[r].iter().all(String::is_empty) {
                rows.pop();
                let text = format!("{}\n", self.format_table(&range, &rows));
                self.push_undo(true);
                return self.edit(range, &text, cx);
            }
            let at = r.max(1) + 1;
            rows.insert(at, vec![String::new(); rows[0].len()]);
            return self.set_table(range, &rows, (at, 0), cx);
        }
        if !self.sel.is_empty() {
            return self.edit(self.sel.clone(), "\n", cx);
        }
        let c = self.cursor();
        let lr = self.line_range(c);
        let line = self.content[lr.clone()].to_string();
        let in_code = self.in_code(lr.start);
        // Ouverture d'un bloc de code : on pose aussi la clôture.
        if md::is_fence(&line)
            && !in_code
            && c == lr.end
            && self.fences_before(self.content.len()) % 2 == 1
        {
            self.edit(c..c, "\n\n```", cx);
            return self.move_to(c + 1, cx);
        }
        match md::on_enter(&line, c - lr.start, in_code) {
            Enter::Insert(text) => self.edit(c..c, &text, cx),
            Enter::Clear => self.edit(lr, "", cx),
        }
        self.renumber(cx);
    }

    pub(super) fn shift_lines(&mut self, indent: bool, cx: &mut Context<Self>) {
        if indent && self.accept_completion(cx) {
            return;
        }
        if self.table_step(indent, cx) {
            return;
        }
        let first = self.line_range(self.sel.start);
        if indent && self.sel.is_empty() {
            let line = &self.content[first.clone()];
            let (kind, _) = md::classify(line, self.in_code(first.start));
            if !matches!(kind, Kind::Bullet | Kind::Ordered | Kind::Task(_)) {
                return self.edit(self.sel.clone(), md::INDENT, cx);
            }
        }
        let block = first.start..self.line_range(self.sel.end).end;
        let sel = self.sel.clone();
        let mut deltas = Vec::new();
        let shifted: Vec<String> = self.content[block.clone()]
            .split('\n')
            .map(|l| {
                let new = if indent {
                    format!("{}{l}", md::INDENT)
                } else if let Some(rest) = l.strip_prefix('\t') {
                    rest.to_string()
                } else {
                    let n = l.bytes().take(md::INDENT.len()).take_while(|&b| b == b' ').count();
                    l[n..].to_string()
                };
                deltas.push(new.len() as isize - l.len() as isize);
                new
            })
            .collect();
        self.push_undo(true);
        self.damaged(&block);
        self.content.replace_range(block.clone(), &shifted.join("\n"));
        let total: isize = deltas.iter().sum();
        let start = (sel.start as isize + deltas[0]).max(block.start as isize) as usize;
        let end = (sel.end as isize + total).max(start as isize) as usize;
        self.sel = start..end;
        self.changed(cx);
        self.renumber(cx);
    }

    /// Un modèle rempli, à la place de la sélection : le curseur va à sa marque, un seul pas d'annulation.
    pub fn insert_filled(&mut self, text: &str, cursor: usize, cx: &mut Context<Self>) {
        let at = self.sel.start;
        self.push_undo(true);
        self.edit(self.sel.clone(), text, cx);
        self.move_to(at + cursor.min(text.len()), cx);
    }

    /// Le nombre de changements du texte : un dépôt qui a mis du temps à copier sait avant d'écrire si la note a bougé.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Des lignes posées au début de la ligne `at` (ou après la dernière), qu'on tape ou qu'on dépose :
    /// un seul pas d'annulation, le curseur à leur fin.
    pub fn insert_lines(&mut self, at: usize, lines: &[String], cx: &mut Context<Self>) {
        let at = self.clamp(at);
        let before = if at > 0 && !self.content[..at].ends_with('\n') { "\n" } else { "" };
        let after = if at < self.content.len() { "\n" } else { "" };
        let text = format!("{before}{}{after}", lines.join("\n"));
        self.push_undo(true);
        self.edit(at..at, &text, cx);
        self.move_to(at + text.len() - after.len(), cx);
    }
}
