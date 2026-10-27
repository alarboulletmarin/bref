//! Table: a CSV or TSV file open in a grid that is read and written
//! directly. Delimiter and encoding are detected, changed and remembered
//! per file. Only visible cells are built; parsing the file lives
//! in `table`.


mod clipboard;
mod disk;
mod edit;
mod input;
mod mouse;
mod render;

pub use disk::*;
use edit::*;
use mouse::*;

use std::{
    fs,
    io::BufWriter,
    ops::Range,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};

use gpui::{
    Action, App, Bounds, ClipboardItem, Context, ElementInputHandler, EntityInputHandler, EventEmitter, FocusHandle,
    Focusable, FontWeight, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point,
    ScrollWheelEvent, Stateful, UTF16Selection, Window, actions, canvas, div, prelude::*, px,
};

use crate::{
    ui::theme::Theme,
    views::editor::{self, TableOp},
    ui::line::{self, Line},
    logic::markdown, ui::assets::{mono, sans},
    ui::widgets::{button, tip},
    logic::table::{self, Encoding, Format, Style, Table, WriteError},
    lang::tr, platform::vault,
};

actions!(
    sheet,
    [
        Left, Right, Up, Down, ExtendLeft, ExtendRight, ExtendUp, ExtendDown, PageUp, PageDown, RowStart, RowEnd,
        Origin, Corner, FirstRow, LastRow, Next, Previous, Enter, Edit, Cancel, Backspace, Delete, Copy, Cut,
        Paste, SelectAll, SelectRow, SelectCol, Undo, Redo, InsertBelow, InsertAbove, DeleteRows, PickDelimiter,
        PickEncoding, PickHeader,
        PickCopyAs, PickExport
    ]
);

const ROW: f32 = 26.;
const GUTTER: f32 = 52.;
const PAD: f32 = 8.;
/// Width of a column: that of its longest cells among the first rows.
const SAMPLE: usize = 200;
const MIN_CHARS: usize = 4;
const MAX_CHARS: usize = 40;
/// Thickness of the area where a scrollbar is grabbed.
const EDGE: f32 = 12.;
/// Table setting or gesture offered by the palette and the bar.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Pick {
    Delimiter,
    Encoding,
    Header,
    CopyAs,
    Export,
}

impl Pick {
    pub const ALL: [Pick; 5] = [Pick::Delimiter, Pick::Encoding, Pick::Header, Pick::CopyAs, Pick::Export];

    /// The action the app listens to to open the choice: it bubbles up to it from any focus.
    pub fn action(self) -> Box<dyn Action> {
        match self {
            Pick::Delimiter => PickDelimiter.boxed_clone(),
            Pick::Encoding => PickEncoding.boxed_clone(),
            Pick::Header => PickHeader.boxed_clone(),
            Pick::CopyAs => PickCopyAs.boxed_clone(),
            Pick::Export => PickExport.boxed_clone(),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Pick::Delimiter => tr("Table delimiter…", "Délimiteur du tableau…"),
            Pick::Encoding => tr("Table encoding…", "Encodage du tableau…"),
            Pick::Header => tr("Table header row", "Ligne d'en-tête du tableau"),
            Pick::CopyAs => tr("Copy table as…", "Copier le tableau en…"),
            Pick::Export => tr("Export table…", "Exporter le tableau…"),
        }
    }
}

pub enum SheetEvent {
    Error(String),
    /// A file was just written next to the table.
    Exported,
}

/// Delimiter as the choice list and the bar show it: its name, then the sign.
pub fn delimiter_option(d: u8) -> String {
    let sign = match d {
        b'\t' => "⇥".to_string(),
        b' ' => "␣".to_string(),
        d => char::from(d).to_string(),
    };
    format!("{}  {sign}", table::delimiter_label(d))
}

/// Column name the spreadsheet way: A, B… Z, AA, AB…
fn letters(mut c: usize) -> String {
    let mut name = Vec::new();
    loop {
        name.push(b'A' + (c % 26) as u8);
        if c < 26 {
            break;
        }
        c = c / 26 - 1;
    }
    name.reverse();
    String::from_utf8(name).unwrap_or_default()
}

pub struct Sheet {
    focus: FocusHandle,
    theme: Theme,
    path: PathBuf,
    format: Format,
    table: Table,
    /// Date and size of the file as read or written: another program touched it if they change.
    stamp: Option<(SystemTime, u64)>,
    /// Active cell, and selection anchor (the selection goes from one to the other).
    cursor: (usize, usize),
    anchor: (usize, usize),
    /// Selections kept by Ctrl+click (anchor, cursor), on top of the current one.
    more: Vec<((usize, usize), (usize, usize))>,
    /// Text being typed in the active cell.
    edit: Option<Line>,
    /// Width of each column, and x of its left edge (one more, for the total).
    widths: Vec<f32>,
    xs: Vec<f32>,
    /// Columns of numbers, dates, IDs (monospace), decided from the sample rows when measured: an edit
    /// does not change it, a new measure (opening, a column inserted or moved) does.
    mono: Vec<bool>,
    /// View offset, in pixels (x, y).
    scroll: (f32, f32),
    body: Bounds<Pixels>,
    drag: Drag,
    undo: Vec<Vec<Change>>,
    redo: Vec<Vec<Change>>,
    dirty: bool,
    generation: usize,
    disk: Arc<Disk>,
    /// The selection was just copied: the icon shows it for a moment.
    copied: bool,
    client: bool,
}

impl EventEmitter<SheetEvent> for Sheet {}

impl Focusable for Sheet {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Sheet {
    pub fn new(path: PathBuf, loaded: Loaded, theme: Theme, cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            focus: cx.focus_handle(),
            theme,
            path,
            format: loaded.format,
            table: loaded.table,
            stamp: loaded.stamp,
            cursor: (0, 0),
            anchor: (0, 0),
            more: Vec::new(),
            edit: None,
            widths: Vec::new(),
            xs: Vec::new(),
            mono: Vec::new(),
            scroll: (0., 0.),
            body: Bounds::default(),
            drag: Drag::None,
            undo: Vec::new(),
            redo: Vec::new(),
            dirty: false,
            generation: 0,
            disk: Arc::default(),
            copied: false,
            client: false,
        };
        this.fresh_table();
        this
    }

    /// Current theme, and whether the window draws its title bar (the badge then hides
    /// the corner of the table bar, which serves as a drag handle).
    pub fn sync(&mut self, theme: Theme, client: bool) {
        let resized = theme.size != self.theme.size;
        (self.theme, self.client) = (theme, client);
        if resized {
            self.measure();
        }
    }

    // ----- File -----

    pub fn toggle_header(&mut self, cx: &mut Context<Self>) {
        self.format.header = !self.format.header;
        remember_format(&self.path, Some(self.format));
        self.clamp();
        cx.notify();
    }

    // ----- Geometry -----

    fn cols(&self) -> usize {
        self.table.cols().max(1)
    }

    /// First file row that scrolls: the one before is the header, fixed at the top.
    fn first(&self) -> usize {
        self.format.header as usize
    }

    fn text_px(&self) -> f32 {
        (self.theme.size * 0.9).round().max(11.)
    }

    fn measure(&mut self) {
        // One read per row, never cell by cell: time stays linear on a very wide row.
        let mut chars = vec![0; self.cols()];
        for r in 0..self.table.rows().min(SAMPLE) {
            for (c, field) in self.table.row(r).iter().enumerate() {
                chars[c] = chars[c].max(field.chars().take(MAX_CHARS + 1).count());
            }
        }
        let em = self.text_px() * 0.6;
        self.mono = self.table.code_columns(self.first(), SAMPLE);
        self.widths = chars.iter().map(|&n| n.clamp(MIN_CHARS, MAX_CHARS) as f32 * em + 2. * PAD).collect();
        self.place();
    }

    fn place(&mut self) {
        let mut x = 0.;
        self.xs = std::iter::once(0.)
            .chain(self.widths.iter().map(|w| {
                x += w;
                x
            }))
            .collect();
    }

    /// An edited cell can widen its column, up to the limit.
    fn widen(&mut self, col: usize, chars: usize) {
        let wanted = chars.min(MAX_CHARS) as f32 * self.text_px() * 0.6 + 2. * PAD;
        match self.widths.get_mut(col) {
            Some(w) if *w < wanted => *w = wanted,
            Some(_) => return,
            None => return self.measure(),
        }
        self.place();
    }

    fn view(&self) -> (f32, f32) {
        let size = self.body.size;
        ((f32::from(size.width) - GUTTER).max(0.), (f32::from(size.height) - ROW).max(0.))
    }

    fn max_scroll(&self) -> (f32, f32) {
        let (w, h) = self.view();
        let lines = self.table.rows().saturating_sub(self.first());
        ((self.xs.last().copied().unwrap_or(0.) - w).max(0.), (lines as f32 * ROW - h).max(0.))
    }

    /// Puts the view, cursor and selection back inside the table.
    fn clamp(&mut self) {
        let (rows, cols) = (self.table.rows().max(1), self.cols());
        let fit = |(r, c): (usize, usize)| (r.min(rows - 1), c.min(cols - 1));
        (self.cursor, self.anchor) = (fit(self.cursor), fit(self.anchor));
        self.more.iter_mut().for_each(|(a, b)| (*a, *b) = (fit(*a), fit(*b)));
        let (mx, my) = self.max_scroll();
        self.scroll = (self.scroll.0.clamp(0., mx), self.scroll.1.clamp(0., my));
    }

    /// Y of the top of the row in the grid, header included.
    fn y_of(&self, row: usize) -> f32 {
        match row.checked_sub(self.first()) {
            Some(line) => ROW + line as f32 * ROW - self.scroll.1,
            None => 0.,
        }
    }

    fn col_at(&self, x: f32) -> usize {
        self.xs.partition_point(|&edge| edge <= x).saturating_sub(1).min(self.cols() - 1)
    }

    /// Scrolls the view to the cell.
    fn show(&mut self, (row, col): (usize, usize)) {
        let (w, h) = self.view();
        let (left, right) = (self.xs[col], self.xs[col + 1]);
        if left < self.scroll.0 {
            self.scroll.0 = left;
        } else if right > self.scroll.0 + w {
            self.scroll.0 = right - w;
        }
        if let Some(line) = row.checked_sub(self.first()) {
            let (top, bottom) = (line as f32 * ROW, (line + 1) as f32 * ROW);
            if top < self.scroll.1 {
                self.scroll.1 = top;
            } else if bottom > self.scroll.1 + h {
                self.scroll.1 = bottom - h;
            }
        }
        self.clamp();
    }

    // ----- Selection -----

    /// Rows and columns of the current selection: the one typed, pasted and extended in.
    fn selection(&self) -> (Range<usize>, Range<usize>) {
        Self::span(self.anchor, self.cursor)
    }

    fn span((r0, c0): (usize, usize), (r1, c1): (usize, usize)) -> (Range<usize>, Range<usize>) {
        (r0.min(r1)..r0.max(r1) + 1, c0.min(c1)..c0.max(c1) + 1)
    }

    /// All selections, top to bottom: those kept by Ctrl+click and the current one.
    /// Copy, cut, clear and delete rows take them all.
    fn selections(&self) -> Vec<(Range<usize>, Range<usize>)> {
        let mut all: Vec<_> = self.more.iter().map(|&(a, b)| Self::span(a, b)).chain([self.selection()]).collect();
        all.sort_by_key(|(rows, cols)| (rows.start, cols.start));
        all.dedup();
        all
    }

    /// Where we are in the table: the cursor cell and the scroll, to come back to it.
    pub fn whereabouts(&self) -> ((usize, usize), (f32, f32)) {
        (self.cursor, self.scroll)
    }

    /// Returns to where we were. The scroll is clamped once the grid knows its size
    /// (`measured`), not before: it has not been drawn yet.
    pub fn set_view(&mut self, cursor: (usize, usize), scroll: (f32, f32), cx: &mut Context<Self>) {
        (self.cursor, self.anchor) = (cursor, cursor);
        self.more.clear();
        self.clamp();
        self.scroll = scroll;
        cx.notify();
    }

    fn select_to(&mut self, to: (usize, usize), extend: bool, cx: &mut Context<Self>) {
        self.cursor = to;
        if !extend {
            self.anchor = to;
            self.more.clear();
        }
        self.show(to);
        cx.notify();
    }

    fn go(&mut self, rows: isize, cols: isize, extend: bool, cx: &mut Context<Self>) {
        self.finish(cx);
        let (r, c) = self.cursor;
        let to = (
            r.saturating_add_signed(rows).min(self.table.rows().max(1) - 1),
            c.saturating_add_signed(cols).min(self.cols() - 1),
        );
        self.select_to(to, extend, cx);
    }

    fn page(&self) -> isize {
        ((self.view().1 / ROW) as isize - 1).max(1)
    }

    fn last_row(&self) -> usize {
        self.table.rows().max(1) - 1
    }

    // ----- Typing in a cell -----

    // ----- Edits -----

    // ----- Clipboard and export -----

    // ----- Mouse -----

    fn select_all(&mut self, cx: &mut Context<Self>) {
        self.finish(cx);
        self.more.clear();
        (self.anchor, self.cursor) = ((self.last_row(), self.cols() - 1), (0, 0));
        cx.notify();
    }

    /// Rows and columns to build. The view size is only known after a first draw:
    /// until then, and so as never to show a half-empty grid, take the whole window,
    /// which contains it.
    fn visible(&self, window: gpui::Size<Pixels>) -> (Range<usize>, Range<usize>) {
        let (vw, vh) = self.view();
        let (vw, vh) = (vw.max(f32::from(window.width) - GUTTER), vh.max(f32::from(window.height) - ROW));
        let c0 = self.col_at(self.scroll.0);
        let c1 = (self.col_at(self.scroll.0 + vw) + 1).min(self.cols());
        let r0 = self.first() + (self.scroll.1 / ROW) as usize;
        let r1 = (r0 + (vh / ROW).ceil() as usize + 2).min(self.table.rows());
        (r0..r1, c0..c1)
    }

    fn measured(&mut self, bounds: Bounds<Pixels>, cx: &mut Context<Self>) {
        if bounds != self.body {
            self.body = bounds;
            self.clamp();
            // During drawing, `notify` schedules no new frame: the grid would only
            // have been redrawn on the next event (mouse, keyboard). Defer it until after.
            let this = cx.entity();
            cx.defer(move |cx| this.update(cx, |_, cx| cx.notify()));
        }
    }
}

#[cfg(test)]
impl Sheet {
    pub fn table(&self) -> &Table {
        &self.table
    }

    /// Middle of cell (`r`, `c`) on screen.
    #[cfg(test)]
    pub fn spot(&self, r: usize, c: usize) -> Point<Pixels> {
        let x = GUTTER + (self.xs[c] + self.xs[c + 1]) / 2. - self.scroll.0;
        let y = ROW + (r - self.first()) as f32 * ROW + ROW / 2. - self.scroll.1;
        self.body.origin + gpui::point(px(x), px(y))
    }

    /// All selections (rows, columns), top to bottom.
    #[cfg(test)]
    pub fn picked(&self) -> Vec<(Range<usize>, Range<usize>)> {
        self.selections()
    }

    /// View scroll (x, y), and selection (rows, columns).
    pub fn view_state(&self) -> ((f32, f32), (Range<usize>, Range<usize>)) {
        (self.scroll, self.selection())
    }

    pub fn scroll_by(&mut self, dx: f32, dy: f32) {
        self.scroll = (self.scroll.0 + dx, self.scroll.1 + dy);
        self.clamp();
    }

    pub fn pick(&mut self, what: &str, n: usize, cx: &mut Context<Self>) {
        match what {
            "col" => self.pick_col(n, false, cx),
            "row" => self.pick_row(n, false, cx),
            _ => self.select_all(cx),
        }
    }

    /// As on the very first display: the view has not been measured yet.
    pub fn unmeasured(&mut self) -> (Range<usize>, Range<usize>) {
        self.body = Bounds::default();
        self.visible(gpui::size(px(800.), px(600.)))
    }
}

#[cfg(test)]
mod tests {
    use super::letters;

    #[test]
    fn names_columns_like_a_spreadsheet() {
        let names: Vec<_> = [0, 1, 25, 26, 27, 51, 52, 701, 702].map(letters).into();
        assert_eq!(names, ["A", "B", "Z", "AA", "AB", "AZ", "BA", "ZZ", "AAA"]);
    }
}
