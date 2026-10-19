//! Formulas in the tables of a note (the evaluator is `logic/formula.rs`, the display is in `grid.rs`):
//! the automatic total, the results copied for a spreadsheet, and what completion offers after `=`.

use super::*;
use crate::logic::formula;

/// What completion offers in a formula: its name, English and French labels, and what follows it
/// (`(` for a function).
pub(super) type Word = (&'static str, &'static str, &'static str, &'static str);

pub(super) const WORDS: &[Word] = &[
    ("sum", "Sum of the numbers", "Somme des nombres", "("),
    ("avg", "Average", "Moyenne", "("),
    ("min", "Smallest number", "Plus petit nombre", "("),
    ("max", "Largest number", "Plus grand nombre", "("),
    ("count", "How many numbers", "Combien de nombres", "("),
    ("round", "Round (number, decimals)", "Arrondir (nombre, décimales)", "("),
    ("abs", "Absolute value", "Valeur absolue", "("),
    ("if", "If (test, then, else)", "Si (test, alors, sinon)", "("),
    ("above", "The cells above", "Les cellules au-dessus", ""),
    ("left", "The cells on the left", "Les cellules à gauche", ""),
    ("column", "The whole column", "Toute la colonne", ""),
];

/// Letters of the columns completion can name.
pub(super) const LETTERS: [&str; 26] = [
    "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z",
];

impl Editor {
    /// The cell holding byte `c`, if it is a formula.
    pub(super) fn formula_cell(&self, c: usize) -> Option<Range<usize>> {
        let (cells, k) = self.table_cells(c)?;
        let cell = cells[k].clone();
        (cell.start <= c && c <= cell.end && formula::is_formula(&self.content[cell.clone()])).then_some(cell)
    }

    /// In a formula, after a word with no digit: the functions and names that begin with it, then the
    /// columns whose letter or header does. Start of the word, and the choices.
    pub(super) fn formulas(&self, c: usize) -> Option<(usize, Vec<Choice<'_>>)> {
        let cell = self.formula_cell(c)?;
        let before = &self.content[cell.start..c];
        let word = before.len() - before.trim_end_matches(|ch: char| ch.is_alphabetic() || ch == '_').len();
        let query = before[before.len() - word..].to_lowercase();
        // Neither right after `=` nor on a word that is part of a reference (`B2`, `3x`).
        let glued = before[..before.len() - word].chars().next_back().is_some_and(|ch| ch.is_alphanumeric() || ch == '_');
        if query.is_empty() || glued {
            return None;
        }
        // What is already written whole is not offered again: it would only take the arrow keys.
        let mut items: Vec<Choice<'_>> =
            WORDS.iter().filter(|(name, _, _, after)| name.starts_with(&query) && !(after.is_empty() && *name == query)).map(Choice::Word).collect();
        let head = self.table_at(c).map(|(range, ..)| {
            let line = self.line_range(range.start);
            let text = &self.content[line.clone()];
            md::cells(text).into_iter().map(|r| &text[r]).collect::<Vec<_>>()
        });
        for (j, header) in head.unwrap_or_default().into_iter().enumerate().take(LETTERS.len()) {
            let letter = LETTERS[j].to_lowercase();
            if letter != query && (letter.starts_with(&query) || (query.len() > 1 && header.to_lowercase().contains(&query))) {
                items.push(Choice::Column(LETTERS[j], header));
            }
        }
        items.truncate(SHOWN);
        Some((c - query.len(), items))
    }

    /// Writes a total in the cell the cursor is in: the sum of the cells above, or of those on its
    /// left when only they hold numbers. A cell with something else in it is not overwritten.
    pub(super) fn auto_sum(&mut self, cx: &mut Context<Self>) {
        let Some((range, mut rows, (r, c))) = self.table_at(self.cursor()) else { return };
        // The header has nothing above it, and the dash line is not a cell.
        if r < 2 || c >= rows[r].len() || !(rows[r][c].trim().is_empty() || formula::is_formula(&rows[r][c])) {
            return;
        }
        let number = |text: &String| formula::read_number(text).is_some();
        let above = rows[2..r].iter().any(|row| row.get(c).is_some_and(number));
        let left = rows[r][..c].iter().any(number);
        rows[r][c] = if !above && left { "=sum(left)" } else { "=sum(above)" }.to_string();
        let guard = self.name_guard(range.start);
        self.set_table(range.clone(), &rows, (r, c), cx);
        let end = self.sel.end;
        self.move_to(end, cx);
        if let Some((table, old)) = guard {
            self.rename_scopes(table, old, cx);
        }
    }

    /// The results of the taken cells (the selected ones, or the whole table), as spreadsheet cells:
    /// tabs between the cells, a line break between the rows, formulas replaced by what they give.
    pub(super) fn copy_results(&mut self, cx: &mut Context<Self>) {
        let Some((range, rows, (r, c))) = self.table_at(self.cursor()) else { return };
        let seen = |r: usize| r.saturating_sub(1);
        let grid: Vec<Vec<String>> = rows.iter().enumerate().filter(|(i, _)| *i != 1).map(|(_, row)| row.clone()).collect();
        let (tall, wide) = (grid.len(), grid[0].len());
        let end = self.table_at(self.sel.end).filter(|(other, ..)| *other == range).map_or((r, c), |(.., cell)| cell);
        let (r0, r1, c0, c1) = match self.live_pick(range.start) {
            Some(pick) => (pick.rows.start, pick.rows.end - 1, pick.cols.start, pick.cols.end - 1),
            None if self.selected_cells().is_some() => (seen(r).min(seen(end.0)), seen(r).max(seen(end.0)), c.min(end.1), c.max(end.1)),
            None => (0, tall - 1, 0, wide - 1),
        };
        let shown = formula::display(&grid, crate::lang::tr(".", ",").chars().next().unwrap_or('.'));
        let text: Vec<String> = shown[r0..=r1.min(tall - 1)]
            .iter()
            .map(|row| row[c0.min(row.len())..(c1 + 1).min(row.len())].iter().map(|cell| cell.replace(['\t', '\n'], " ")).collect::<Vec<_>>().join("\t"))
            .collect();
        cx.write_to_clipboard(ClipboardItem::new_string(text.join("\n")));
    }
}

#[cfg(test)]
impl Editor {
    /// What each cell of the first table shows, line by line.
    pub fn shown_table(&self) -> Vec<Vec<String>> {
        self.rows.iter().filter_map(|r| r.grid.as_ref().filter(|g| !g.hidden())).map(|g| g.shown()).collect()
    }

    /// Center of cell (`v`, `c`) of the first table: `v` counts the header as 0.
    pub fn cell_point(&self, v: usize, c: usize) -> Option<Point<Pixels>> {
        let geo = self.geos.first()?;
        Some(point((geo.xs[c] + geo.xs[c + 1]) / 2., (geo.ys[v] + geo.ys[v + 1]) / 2.))
    }

    /// Why the formula under the pointer has no result.
    pub fn hint(&self) -> Option<String> {
        self.reason.as_ref().map(|r| r.to_string())
    }

    /// What completion offers, as the list shows it: label, then detail.
    pub fn completions(&self) -> Vec<String> {
        let Some((_, items)) = self.completion() else { return Vec::new() };
        items
            .iter()
            .map(|choice| match choice {
                Choice::Word((name, en, fr, after)) => format!("{} {name}{after}", tr(en, fr)),
                Choice::Column(letter, header) => format!("{header} {letter}"),
                Choice::Name(name) => name.to_string(),
                Choice::Command((name, ..)) => format!("/{name}"),
                Choice::Date(..) => "date".into(),
            })
            .collect()
    }
}
