//! Kanban board: a note shown as columns. Each `##` heading is a column, each
//! `- [ ]` task under it a card; the file stays Markdown. Pure functions first, then what a
//! pane keeps of a board; the board is drawn by the shell (`shell/board.rs`).

use std::ops::Range;

gpui::actions!(kanban, [CardUp, CardDown, CardLeft, CardRight, CarryUp, CarryDown, CarryLeft, CarryRight, CardEdit, CardRemove]);

use gpui::{App, Context, Div, Entity, FocusHandle, Pixels, Window, div, prelude::*, px, svg};

use crate::{
    ui::theme::Theme,
    logic::markdown::{self, Kind},
    views::field::Field,
    lang::{number, tr},
};

#[derive(Debug, PartialEq)]
pub struct Card<'a> {
    pub text: &'a str,
    pub done: bool,
        /// Its line in the note, line ending included.
    pub line: Range<usize>,
}

#[derive(Debug, PartialEq)]
pub struct Column<'a> {
    pub title: &'a str,
        /// The line of its heading, line ending included.
    pub line: Range<usize>,
        /// Where to put one more card: after the last one, or after the heading.
    pub end: usize,
    pub cards: Vec<Card<'a>>,
}

/// What a pane keeps of the board it shows.
pub struct Board {
        /// The open note is a kanban board whose text we are looking at: we asked for it, or we
        /// just wrote its key (typing `kanban: true` does not leave the text).
    pub text: bool,
        /// The note was a board at its last keystroke.
    pub was: bool,
        /// Card being moved in the board, and where it would land (column, rank).
    pub drag: Option<((usize, usize), (usize, usize))>,
        /// Width of a board card, measured when drawn.
    pub card: std::rc::Rc<std::cell::Cell<Pixels>>,
    pub scroll: gpui::ScrollHandle,
        /// What is being typed in the board, and its input field.
    pub field: Option<(Slot, Entity<Field>)>,
        /// The board receives input in place of the note it hides.
    pub focus: FocusHandle,
    /// The spot the keys are on (see `walk`); `None` until an arrow is pressed, and after a click.
    pub at: Option<(usize, usize)>,
}

impl Board {
    pub fn new(cx: &mut App) -> Self {
        Board { text: false, was: false, drag: None, card: Default::default(), scroll: Default::default(), field: None, focus: cx.focus_handle(), at: None }
    }
}

/// The note opens as a board: its front matter carries the `kanban` key (or `kanban-plugin`).
pub fn is_board(text: &str) -> bool {
    markdown::front_keys(text).iter().any(|key| matches!(*key, "kanban" | "kanban-plugin"))
}

/// The `# ` title at the top of the note, as written.
pub fn heading(text: &str) -> Option<&str> {
    let first = text[markdown::front_matter(text)..].lines().find(|l| !l.trim().is_empty())?;
    Some(first.strip_prefix("# ")?.trim()).filter(|t| !t.is_empty())
}

/// How many cards are still open and how many are done, over all the columns.
pub fn tally(all: &[Column]) -> (usize, usize) {
    let done = all.iter().flat_map(|c| &c.cards).filter(|k| k.done).count();
    (all.iter().map(|c| c.cards.len()).sum::<usize>() - done, done)
}

/// `6 open · 2 done`. French agrees with « carte »: « 0 ouverte », « 1 faite », « 2 faites ».
pub fn summary(open: usize, done: usize) -> String {
    let (o, d) = (if open < 2 { "ouverte" } else { "ouvertes" }, if done < 2 { "faite" } else { "faites" });
    format!("{} {} · {} {}", number(open), tr("open", o), number(done), tr("done", d))
}

/// The note's columns and their cards. An indented task is a card's detail, not
/// a card; what comes before the first `##` is in no column.
pub fn columns(text: &str) -> Vec<Column<'_>> {
    let mut found: Vec<Column> = Vec::new();
    let mut at = markdown::front_matter(text);
    for line in text[at..].split_inclusive('\n') {
        let end = at + line.len();
        let bare = line.trim_end_matches(['\n', '\r']);
        match (markdown::classify(bare, false), found.last_mut()) {
            ((Kind::Heading(_), marker), _) if bare.starts_with("## ") => found.push(Column { title: bare[marker..].trim(), line: at..end, end, cards: Vec::new() }),
            ((Kind::Task(done), marker), Some(column)) if bare.starts_with('-') => {
                column.cards.push(Card { text: bare[marker..].trim(), done, line: at..end });
                column.end = end;
            }
            _ => {}
        }
        at = end;
    }
    found
}

/// The note once card `from` (column, rank) is put in column `to.0`, before the card
/// of rank `to.1` (or at the end if that rank does not exist).
pub fn move_card(text: &str, from: (usize, usize), to: (usize, usize)) -> Option<String> {
    let all = columns(text);
    let card = all.get(from.0)?.cards.get(from.1)?;
    let column = all.get(to.0)?;
    let into = column.cards.get(to.1).map_or(column.end, |next| next.line.start);
        // Dropped on itself or just after itself: nothing moves.
    if (card.line.start..=card.line.end).contains(&into) {
        return None;
    }
    let mut line = text[card.line.clone()].to_string();
    if !line.ends_with('\n') {
        line.push('\n');
    }
    let mut moved = String::with_capacity(text.len() + 1);
    let (first, second) = (card.line.start.min(into), card.line.start.max(into));
    moved.push_str(&text[..first]);
    if into < card.line.start {
        moved.push_str(&line);
        moved.push_str(&text[into..card.line.start]);
        moved.push_str(&text[card.line.end..]);
    } else {
        moved.push_str(&text[card.line.end..second]);
                // The target column ends the file without a line ending: the card starts one.
        if !moved.ends_with('\n') {
            moved.push('\n');
        }
        moved.push_str(&line);
        moved.push_str(&text[second..]);
    }
    Some(moved)
}

/// The note once the card is checked, or unchecked.
pub fn tick(text: &str, at: (usize, usize)) -> Option<String> {
    let all = columns(text);
    let card = all.get(at.0)?.cards.get(at.1)?;
    let open = card.line.start + text[card.line.clone()].find('[')?;
    let mark = if card.done { " " } else { "x" };
    Some(format!("{}{mark}{}", &text[..open + 1], &text[open + 2..]))
}

/// The note with card `card` added at the bottom of the column.
pub fn add_card(text: &str, column: usize, card: &str) -> Option<String> {
    let end = columns(text).get(column)?.end;
    let lead = if text[..end].ends_with('\n') { "" } else { "\n" };
    Some(format!("{}{lead}- [ ] {}\n{}", &text[..end], card.trim(), &text[end..]))
}

/// The note once the card is rewritten; rewritten empty, it is removed.
pub fn retitle(text: &str, at: (usize, usize), card: &str) -> Option<String> {
    let all = columns(text);
    let old = all.get(at.0)?.cards.get(at.1)?;
    if card.trim().is_empty() {
        return Some(format!("{}{}", &text[..old.line.start], &text[old.line.end..]));
    }
        // The card's text is a slice of the note: its place follows from it.
    let start = old.text.as_ptr() as usize - text.as_ptr() as usize;
    Some(format!("{}{}{}", &text[..start], card.trim(), &text[start + old.text.len()..]))
}

/// The note once the column is renamed; an empty heading changes nothing.
pub fn rename_column(text: &str, column: usize, title: &str) -> Option<String> {
    let all = columns(text);
    let old = all.get(column)?;
    let start = old.title.as_ptr() as usize - text.as_ptr() as usize;
    (!title.trim().is_empty()).then(|| format!("{}{}{}", &text[..start], title.trim(), &text[start + old.title.len()..]))
}

/// The note with one more column, at the end.
pub fn add_column(text: &str, title: &str) -> Option<String> {
    let lead = if text.is_empty() || text.ends_with("\n\n") { "" } else if text.ends_with('\n') { "\n" } else { "\n\n" };
    (!title.trim().is_empty()).then(|| format!("{text}{lead}## {}\n", title.trim()))
}

/// The note without the column, its cards and what is under its heading.
pub fn remove_column(text: &str, column: usize) -> Option<String> {
    let all = columns(text);
    let start = all.get(column)?.line.start;
    let end = all.get(column + 1).map_or(text.len(), |next| next.line.start);
    Some(format!("{}{}", &text[..start], &text[end..]))
}

/// Where the keys are on the board, moved by `by` (columns, rows): a spot is (column, row), row 0
/// the column's title, then its cards, then "Add a card"; the column after the last is "Add a column".
/// `counts`: the cards of each column.
pub fn walk(counts: &[usize], at: (usize, usize), by: (isize, isize)) -> (usize, usize) {
    let n = counts.len();
    let c = (at.0.min(n) as isize + by.0).clamp(0, n as isize) as usize;
    let last = counts.get(c).map_or(0, |k| k + 1);
    let r = (at.1.min(last) as isize + by.1).clamp(0, last as isize) as usize;
    (c, r)
}

/// The note once the card on spot `at` is carried one step along `by` (up, down: in its column;
/// sideways: into the next column, at the same rank or at its end), and the card's new spot.
pub fn carry(text: &str, at: (usize, usize), by: (isize, isize)) -> Option<(String, (usize, usize))> {
    let all = columns(text);
    let (c, r) = at;
    let len = all.get(c)?.cards.len();
    let i = r.checked_sub(1).filter(|&i| i < len)?;
    let (to, spot) = match by {
        (0, -1) => ((c, i.checked_sub(1)?), (c, r - 1)),
        (0, 1) if i + 1 < len => ((c, i + 2), (c, r + 1)),
        (dx, 0) if dx != 0 => {
            let next = c.checked_add_signed(dx)?;
            let k = i.min(all.get(next)?.cards.len());
            ((next, k), (next, k + 1))
        }
        _ => return None,
    };
    Some((move_card(text, (c, i), to)?, spot))
}

/// The text of a new board: its three columns, and the key that makes it open as a board.
pub fn new_board() -> String {
    let columns = [tr("To do", "À faire"), tr("Doing", "En cours"), tr("Done", "Fait")].map(|title| format!("## {title}\n\n")).concat();
    format!("---\nkanban: true\n---\n# {}\n\n{}", tr("Board", "Tableau"), columns.trim_end_matches('\n').to_string() + "\n")
}

/// What is being typed in the board.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Slot {
        /// One more card at the bottom of this column.
    New(usize),
    Card(usize, usize),
    Column(usize),
        /// One more column.
    NewColumn,
}

/// Width of a column when the window has room; they shrink down to `NARROW` to
/// fit side by side, then the board scrolls.
pub const COLUMN: f32 = 250.;
pub const NARROW: f32 = 140.;

/// A card's frame, its checkbox and its text: the same for the placed card, the one being held
/// and the place it would take. The border is clear until the card is hovered, chosen or written in;
/// the placed card adds its own shadow.
pub fn card_box(t: Theme) -> Div {
    div().px(px(12.)).py(px(10.)).rounded(t.large()).bg(t.bg).border_1().border_color(gpui::transparent_black()).flex().items_start().gap(px(9.))
}

/// The round check box: a 1.5 px ring in `soft`, filled with `accent` and a check once done.
pub fn check_box(done: bool, t: Theme) -> Div {
    let mut ring = div().size(px(14.)).flex_none().mt(px(3.)).rounded_full().border_color(if done { t.accent } else { t.soft });
    let widths = &mut ring.style().border_widths;
    (widths.top, widths.right, widths.bottom, widths.left) = (Some(px(1.5).into()), Some(px(1.5).into()), Some(px(1.5).into()), Some(px(1.5).into()));
    ring.when(done, |d| d.bg(t.accent).flex().items_center().justify_center().child(svg().path("check.svg").size(px(10.)).text_color(t.bg)))
}

pub fn card_text(text: &str, done: bool, t: Theme) -> Div {
    div().flex_1().min_w_0().when(done, |d| d.text_color(t.dim).line_through()).child(text.to_string())
}

/// Card being held: the card itself, lifted, following the pointer from where it was grabbed.
#[derive(Clone)]
pub struct Dragged {
        /// The board pane the card comes from: another board ignores it.
    pub pane: usize,
    pub from: (usize, usize),
    pub text: String,
    pub done: bool,
        /// The card's width in its column.
    pub width: Pixels,
    pub theme: Theme,
}

impl Render for Dragged {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        card_box(t)
            .w(self.width)
            .border_color(t.accent)
            .shadow(t.floating())
            .text_size(px(13.))
            .text_color(t.text)
            .child(check_box(self.done, t))
            .child(card_text(&self.text, self.done, t))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOARD: &str = "---\nkanban: true\n---\n# Projet\n\n## À faire\n\n- [ ] Écrire\n    - [ ] un détail\n- [ ] Relire\n\n## En cours\n\n## Fait\n- [x] Planifier";

    #[test]
    fn reads_and_moves_cards() {
        assert!(is_board(BOARD) && is_board("---\nkanban-plugin: basic\n---\n") && !is_board("## À faire\n- [ ] x"));
        let all = columns(BOARD);
        let shape: Vec<(&str, Vec<(&str, bool)>)> = all.iter().map(|c| (c.title, c.cards.iter().map(|k| (k.text, k.done)).collect())).collect();
        assert_eq!(shape, [("À faire", vec![("Écrire", false), ("Relire", false)]), ("En cours", vec![]), ("Fait", vec![("Planifier", true)])]);
        let titles = |text: &str| columns(text).iter().map(|c| c.cards.iter().map(|k| k.text.to_string()).collect::<Vec<_>>()).collect::<Vec<_>>();
                // Into an empty column, then at the end of the last one (the file has no line ending).
        let moved = move_card(BOARD, (0, 0), (1, usize::MAX)).unwrap();
        assert_eq!(titles(&moved), [vec!["Relire"], vec!["Écrire"], vec!["Planifier"]]);
        assert!(moved.contains("## En cours\n- [ ] Écrire\n\n## Fait") && moved.contains("    - [ ] un détail\n- [ ] Relire"));
        let moved = move_card(&moved, (1, 0), (2, usize::MAX)).unwrap();
        assert!(moved.ends_with("- [x] Planifier\n- [ ] Écrire\n"));
                // Before another card, upward as well as downward; onto itself, nothing.
        assert_eq!(titles(&move_card(BOARD, (2, 0), (0, 1)).unwrap()), [vec!["Écrire", "Planifier", "Relire"], vec![], vec![]]);
        assert_eq!(titles(&move_card(BOARD, (0, 0), (0, 2)).unwrap())[0], ["Relire", "Écrire"]);
        assert_eq!((move_card(BOARD, (0, 1), (0, 1)), move_card(BOARD, (0, 1), (0, 2)), move_card(BOARD, (5, 0), (0, 0))), (None, None, None));
                // Check, uncheck, add.
        assert!(tick(BOARD, (0, 1)).unwrap().contains("- [x] Relire") && tick(BOARD, (2, 0)).unwrap().ends_with("- [ ] Planifier"));
        assert!(add_card(BOARD, 1, " Coder ").unwrap().contains("## En cours\n- [ ] Coder\n\n## Fait"));
        assert!(add_card(BOARD, 2, "Fêter").unwrap().ends_with("- [x] Planifier\n- [ ] Fêter\n"));
                // Rewrite a card (emptied: it goes), rename a column, add one.
        assert!(retitle(BOARD, (0, 1), "Relire deux fois").unwrap().contains("- [ ] Relire deux fois\n\n## En cours"));
        assert!(retitle(BOARD, (2, 0), "Prévoir").unwrap().ends_with("- [x] Prévoir"));
        assert_eq!(titles(&retitle(BOARD, (0, 0), " ").unwrap())[0], ["Relire"]);
        assert!(rename_column(BOARD, 1, "En route").unwrap().contains("\n## En route\n\n## Fait") && rename_column(BOARD, 1, "").is_none());
        assert!(add_column(BOARD, "Plus tard").unwrap().ends_with("- [x] Planifier\n\n## Plus tard\n") && add_column(BOARD, " ").is_none());
        // A column goes with its cards and what is under its heading; the last one too.
        assert_eq!(titles(&remove_column(BOARD, 0).unwrap()), [vec![], vec!["Planifier"]]);
        assert!(remove_column(BOARD, 0).unwrap().starts_with("---\nkanban: true\n---\n# Projet\n\n## En cours"));
        assert!(remove_column(BOARD, 2).unwrap().ends_with("## En cours\n\n") && remove_column(BOARD, 3).is_none());
        let fresh = new_board();
        assert!(is_board(&fresh) && columns(&fresh).iter().map(|c| c.title).collect::<Vec<_>>() == ["À faire", "En cours", "Fait"] && fresh.ends_with("## Fait\n"));
    }

    /// The header counts: French agrees for 0 and 1 (singular) and 2 and more, thousands are set apart,
    /// done cards are not open ones, and the title is the `# ` line as written.
    #[test]
    fn the_header_counts_the_cards() {
        assert_eq!(tally(&columns(BOARD)), (2, 1));
        assert_eq!(tally(&columns("---\nkanban: true\n---\n")), (0, 0));
        assert_eq!((summary(0, 0), summary(1, 1), summary(2, 2)), ("0 ouverte · 0 faite".into(), "1 ouverte · 1 faite".into(), "2 ouvertes · 2 faites".into()));
        assert_eq!(summary(1000, 100_000), "1\u{202f}000 ouvertes · 100\u{202f}000 faites");
        assert_eq!((number(0), number(999), number(1_234_567)), ("0".into(), "999".into(), "1\u{202f}234\u{202f}567".into()));
        assert_eq!((heading(BOARD), heading("# Un `titre`  \n"), heading("## Non"), heading("---\nkanban: true\n---\n\n#  \n")), (Some("Projet"), Some("Un `titre`"), None, None));
    }

    /// Spots are (column, row): row 0 the title, then the cards, then "Add a card"; the column after the
    /// last is "Add a column", alone on its row.
    #[test]
    fn walks_the_board() {
        let counts = [2, 0, 1];
        let go = |at, by| walk(&counts, at, by);
        assert_eq!((go((0, 0), (0, 1)), go((0, 3), (0, 1)), go((0, 0), (0, -1))), ((0, 1), (0, 3), (0, 0)));
        // Sideways, the row stays when it can, else the nearest one; then "Add a column", then nothing more.
        assert_eq!((go((0, 2), (1, 0)), go((0, 3), (1, 0)), go((1, 1), (1, 0))), ((1, 1), (1, 1), (2, 1)));
        assert_eq!((go((2, 2), (1, 0)), go((3, 0), (1, 0)), go((3, 0), (0, 1)), go((3, 0), (-1, 0))), ((3, 0), (3, 0), (3, 0), (2, 0)));
        assert_eq!(go((0, 1), (-1, 0)), (0, 1));
        // A board without columns: only "Add a column"; a spot left behind by a removed card comes back inside.
        assert_eq!((walk(&[], (0, 0), (1, 0)), walk(&[], (2, 5), (0, 1))), ((0, 0), (0, 0)));
        assert_eq!(walk(&counts, (1, 7), (0, 0)), (1, 1));
    }

    /// Alt+arrows carry the card on a spot: up and down in its column, sideways at the same rank.
    #[test]
    fn carries_cards() {
        let titles = |text: &str| columns(text).iter().map(|c| c.cards.iter().map(|k| k.text.to_string()).collect::<Vec<_>>()).collect::<Vec<_>>();
        let (text, at) = carry(BOARD, (0, 1), (0, 1)).unwrap();
        assert_eq!((titles(&text)[0].clone(), at), (vec!["Relire".to_string(), "Écrire".to_string()], (0, 2)));
        let (text, at) = carry(&text, (0, 2), (1, 0)).unwrap();
        assert_eq!((titles(&text), at), (vec![vec!["Relire".to_string()], vec!["Écrire".to_string()], vec!["Planifier".to_string()]], (1, 1)));
        let (text, at) = carry(&text, (1, 1), (1, 0)).unwrap();
        assert_eq!((titles(&text)[2].clone(), at), (vec!["Écrire".to_string(), "Planifier".to_string()], (2, 1)));
        // At an edge, on a title or on "Add a card": nothing moves.
        assert!(carry(BOARD, (0, 1), (0, -1)).is_none() && carry(BOARD, (0, 2), (0, 1)).is_none() && carry(BOARD, (2, 1), (1, 0)).is_none());
        assert!(carry(BOARD, (0, 1), (-1, 0)).is_none() && carry(BOARD, (0, 0), (0, 1)).is_none() && carry(BOARD, (0, 3), (1, 0)).is_none());
    }
}
