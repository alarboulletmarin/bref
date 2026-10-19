//! Which parts of a table carry a comment: what the marks and outlines drawn on it show.

use super::*;

/// What a comment is about, in a table: visible row, column, the table, a cell (row, column).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Target {
    Row(usize),
    Col(usize),
    Table,
    Cell(usize, usize),
}

/// The commented parts of each table (by start), for a version of the text and a cursor.
pub type Marks = ((u64, usize), Rc<HashMap<usize, Notes>>);

/// The commented parts of a table, and the one whose comment holds the cursor.
#[derive(Clone, Default, Debug)]
pub struct Notes {
    pub found: Vec<Target>,
    pub active: Option<Target>,
}

impl Editor {
    /// The commented parts of each table that starts at one of `starts`, for the text and the
    /// cursor as they are: reread only when one of them changes.
    pub(super) fn note_marks(&mut self, starts: &[usize]) -> Rc<HashMap<usize, Notes>> {
        let key = (self.version, self.sel.start);
        if let Some((known, marks)) = &self.marks
            && *known == key
        {
            return marks.clone();
        }
        let commented = self.content.contains("{>>") || self.content.contains("{==");
        let marks: HashMap<usize, Notes> = starts.iter().filter(|_| commented).map(|&start| (start, self.table_notes(start))).collect();
        let marks = Rc::new(marks);
        self.marks = Some((key, marks.clone()));
        marks
    }

    /// The commented parts of the table starting at `start`: the cells whose whole text is a
    /// comment, and the rows, columns or table named by a comment line under it.
    pub(super) fn table_notes(&self, start: usize) -> Notes {
        let Some((range, rows, _)) = self.table_at(start) else { return Notes::default() };
        let (names, cols) = md::scopes(&rows);
        let cursor = self.sel.start;
        let mut notes = Notes::default();
        let mut note = |target: Target, lr: &Range<usize>| {
            notes.found.push(target);
            if (lr.start..=lr.end).contains(&cursor) {
                notes.active = Some(target);
            }
        };
        for lr in self.comment_lines_after(range.end) {
            let body = self.content[lr.clone()].trim().trim_start_matches("{>>");
            let named = |list: &[String]| list.iter().position(|name| body.starts_with(&format!("{name}: ")));
            if body.starts_with("Table: ") {
                note(Target::Table, &lr);
            } else if let Some(v) = named(&names) {
                note(Target::Row(v), &lr);
            } else if let Some(c) = named(&cols) {
                note(Target::Col(c), &lr);
            }
        }
        let mut at = range.start;
        for (l, line) in self.content[range].split('\n').enumerate() {
            if l != 1 {
                for (c, cell) in md::cells(line).into_iter().enumerate() {
                    let text = &line[cell.clone()];
                    if md::comments(text).first().is_some_and(|(whole, ..)| *whole == (0..text.len()) && text.starts_with("{==")) {
                        note(Target::Cell(l.saturating_sub(1), c), &(at + cell.start..at + cell.end));
                    }
                }
            }
            at += line.len() + 1;
        }
        notes
    }

    /// The commented parts of the first table, for the tests.
    #[cfg(test)]
    pub fn first_notes(&mut self) -> Notes {
        let starts: Vec<usize> = self.geos.iter().map(|g| g.start).collect();
        self.note_marks(&starts).get(&starts[0]).cloned().unwrap_or_default()
    }
}
