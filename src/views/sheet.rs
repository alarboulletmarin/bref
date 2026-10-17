//! Table: a CSV or TSV file open in a grid that is read and written
//! directly. Delimiter and encoding are detected, changed and remembered
//! per file. Only visible cells are built; parsing the file lives
//! in `table`.

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
    logic::markdown, ui::assets::mono,
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
const UNDO_DEPTH: usize = 200;
/// A setting or an edit waits this long before being written to disk.
const SAVE_DELAY: Duration = Duration::from_millis(400);

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

/// An edit gesture, and its inverse: undo applies the inverse and gets the gesture back.
enum Change {
    /// Values to set: (row, column, text).
    Set(Vec<(usize, usize, String)>),
    Insert(usize, Vec<Vec<String>>),
    Remove(Range<usize>),
    /// `n` columns before `at`, with their previous cells (none: empty).
    InsertCols(usize, usize, Vec<Vec<String>>),
    RemoveCols(Range<usize>),
    /// The columns will start at `to` (counted once removed).
    MoveCols(Range<usize>, usize),
    /// The rows of a window starting at `start`: row `i` becomes the old `perm[i]`.
    Reorder(usize, Vec<usize>),
}

#[derive(Clone, Copy, PartialEq)]
enum Drag {
    None,
    Cells,
    Rows,
    Cols,
    /// Scrollbar: vertical or not.
    Bar(bool),
}

enum Hit {
    Cell(usize, usize),
    Row(usize),
    Col(usize),
    Corner,
    Bar(bool),
}

/// The file as just read.
pub struct Loaded {
    table: Table,
    format: Format,
    stamp: Option<(SystemTime, u64)>,
}

fn stamp_of(path: &Path) -> Option<(SystemTime, u64)> {
    let meta = fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

/// Reads the file. Without `forced`, encoding, delimiter and header are guessed, except
/// those the user has already chosen for this file.
pub fn load(path: &Path, forced: Option<Format>) -> Result<Loaded, String> {
    let fail = |e: std::io::Error| format!("{} {} : {e}", tr("Cannot open", "Impossible d'ouvrir"), path.display());
    let size = fs::metadata(path).map_err(fail)?.len();
    if size > table::MAX_BYTES as u64 {
        return Err(format!("{} : {}", path.display(), tr("file too large", "fichier trop gros")));
    }
    // ponytail: synchronous read and parse (100 MB: about 0.3 s); move to a background
    // task with a "loading" state if files of several hundred MB get opened.
    let bytes = fs::read(path).map_err(fail)?;
    let format = forced.or_else(|| saved_format(path)).unwrap_or_else(|| table::detect(&bytes));
    let table = Table::parse(table::decode(bytes, format.encoding), format.delimiter);
    Ok(Loaded { table, format, stamp: stamp_of(path) })
}

/// Settings the user chose for this file: `path ⇥ delimiter ⇥ encoding ⇥ header`.
fn saved_format(path: &Path) -> Option<Format> {
    let key = path.to_str()?;
    vault::load_file("tables").lines().find_map(|line| {
        let mut parts = line.split('\t');
        (parts.next()? == key).then_some(())?;
        Some(Format {
            delimiter: parts.next()?.parse().ok()?,
            encoding: Encoding::from_label(parts.next()?)?,
            header: parts.next()? == "1",
        })
    })
}

fn remember_format(path: &Path, format: Option<Format>) {
    let Some(key) = path.to_str().filter(|k| !k.contains(['\t', '\n'])) else {
        return;
    };
    let mut lines: Vec<String> =
        vault::load_file("tables").lines().filter(|l| l.split('\t').next() != Some(key)).map(String::from).collect();
    if let Some(f) = format {
        lines.push(format!("{key}\t{}\t{}\t{}", f.delimiter, f.encoding.label(), f.header as u8));
    }
    vault::save_file("tables", &lines.join("\n"));
}

/// Last generation written: a late write never replaces a newer one.
#[derive(Default)]
struct Disk(Mutex<usize>);

impl Disk {
    fn write(&self, generation: usize, path: &Path, table: &Table, encoding: Encoding) -> Result<(), WriteError> {
        let mut last = self.0.lock().unwrap();
        if *last >= generation {
            return Ok(());
        }
        // Atomic write: a crash never leaves a truncated table.
        let tmp = path.with_file_name(format!(".{}.tmp", path.file_name().unwrap_or_default().to_string_lossy()));
        let result = (|| {
            let file = fs::File::create(&tmp)?;
            if let Ok(meta) = fs::metadata(path) {
                fs::set_permissions(&tmp, meta.permissions()).ok();
            }
            let mut out = BufWriter::with_capacity(1 << 16, file);
            table.write_to(&mut out, encoding)?;
            let file = out.into_inner().map_err(|e| e.into_error())?;
            // Without `fsync`, a power cut can leave the rename without the data.
            file.sync_all()?;
            fs::rename(&tmp, path)?;
            Ok(())
        })();
        if result.is_err() {
            fs::remove_file(&tmp).ok();
        } else {
            *last = generation;
        }
        result
    }
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

    pub fn moved(&mut self, path: PathBuf) {
        self.path = path;
    }

    // ----- File -----

    /// An empty table keeps one empty row to write in.
    fn fresh_table(&mut self) {
        if self.table.rows() == 0 {
            self.table.insert_row(0);
        }
        self.measure();
    }

    /// Takes a version reread from disk, keeping the cursor's place.
    fn replace(&mut self, loaded: Loaded) {
        self.format = loaded.format;
        self.table = loaded.table;
        self.stamp = loaded.stamp;
        self.undo.clear();
        self.redo.clear();
        self.edit = None;
        self.dirty = false;
        self.fresh_table();
        self.clamp();
    }

    /// Another program rewrote the file and nothing is pending here: reread it.
    pub fn reload_if_changed(&mut self, cx: &mut Context<Self>) {
        if self.dirty || self.stamp == stamp_of(&self.path) {
            return;
        }
        match load(&self.path, Some(self.format)) {
            Ok(loaded) => self.replace(loaded),
            Err(e) => cx.emit(SheetEvent::Error(e)),
        }
        cx.notify();
    }

    /// Changes the read settings of this file (`None`: guess them again):
    /// what is pending is written, then the file is reread that way, and the choice remembered.
    pub fn reformat(&mut self, format: Option<Format>, cx: &mut Context<Self>) {
        self.flush(cx);
        if self.dirty {
            return;
        }
        remember_format(&self.path, format);
        match load(&self.path, format) {
            Ok(loaded) => self.replace(loaded),
            Err(e) => cx.emit(SheetEvent::Error(e)),
        }
        cx.notify();
    }

    pub fn format(&self) -> Format {
        self.format
    }

    pub fn toggle_header(&mut self, cx: &mut Context<Self>) {
        self.format.header = !self.format.header;
        remember_format(&self.path, Some(self.format));
        self.clamp();
        cx.notify();
    }

    /// A change: the disk gets it after a short delay, or right away on `flush`.
    fn changed(&mut self, cx: &mut Context<Self>) {
        self.dirty = true;
        self.generation += 1;
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SAVE_DELAY).await;
            this.update(cx, |this, cx| {
                if this.generation == generation {
                    this.save(cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// Writes in the background: a big file does not freeze the window.
    fn save(&mut self, cx: &mut Context<Self>) {
        if !self.dirty {
            return;
        }
        let (generation, snapshot, path, encoding, disk) =
            (self.generation, self.table.snapshot(), self.path.clone(), self.format.encoding, self.disk.clone());
        cx.spawn(async move |this, cx| {
            let written = cx
                .background_executor()
                .spawn(async move { disk.write(generation, &path, &snapshot, encoding).map(|()| stamp_of(&path)) })
                .await;
            this.update(cx, |this, cx| match written {
                Ok(stamp) => {
                    this.stamp = stamp.or(this.stamp);
                    this.dirty &= this.generation != generation;
                }
                Err(e) => cx.emit(SheetEvent::Error(this.not_saved(e))),
            })
            .ok();
        })
        .detach();
    }

    fn not_saved(&self, e: WriteError) -> String {
        format!("{} : {e}", tr("Table not saved", "Tableau non enregistré"))
    }

    /// Writes what is pending right away (file change, close).
    pub fn flush(&mut self, cx: &mut Context<Self>) {
        self.finish(cx);
        if !self.dirty {
            return;
        }
        match self.disk.write(self.generation, &self.path, &self.table, self.format.encoding) {
            Ok(()) => {
                self.dirty = false;
                self.stamp = stamp_of(&self.path).or(self.stamp);
            }
            Err(e) => cx.emit(SheetEvent::Error(self.not_saved(e))),
        }
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

    fn hit(&self, p: Point<Pixels>) -> Hit {
        let (x, y) = (f32::from(p.x - self.body.origin.x), f32::from(p.y - self.body.origin.y));
        let (w, h) = (f32::from(self.body.size.width), f32::from(self.body.size.height));
        let (mx, my) = self.max_scroll();
        if my > 0. && x > w - EDGE && y > ROW {
            return Hit::Bar(true);
        }
        if mx > 0. && y > h - EDGE && x > GUTTER {
            return Hit::Bar(false);
        }
        let col = (x >= GUTTER).then(|| self.col_at(x - GUTTER + self.scroll.0));
        let last = self.table.rows().max(1) - 1;
        let row = (y >= ROW).then(|| (self.first() + ((y - ROW + self.scroll.1) / ROW) as usize).min(last));
        match (row, col) {
            (None, None) => Hit::Corner,
            (None, Some(c)) => Hit::Col(c),
            (Some(r), None) => Hit::Row(r),
            (Some(r), Some(c)) => Hit::Cell(r, c),
        }
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

    /// Number of gestures that can be undone.
    #[cfg(test)]
    pub fn undo_len(&self) -> usize {
        self.undo.len()
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

    fn begin_edit(&mut self, text: Option<&str>, cx: &mut Context<Self>) {
        let (r, c) = self.cursor;
        let text = text.map_or_else(|| self.table.cell(r, c).into_owned(), str::to_string);
        self.edit = Some(Line::new(text));
        cx.notify();
    }

    /// Commits the current input.
    fn finish(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = self.edit.take() else {
            return;
        };
        let (r, c) = self.cursor;
        if *self.table.cell(r, c) != *edit.text {
            self.commit(vec![Change::Set(vec![(r, c, edit.text)])], cx);
        }
        cx.notify();
    }

    fn typed(&mut self, text: &str, cx: &mut Context<Self>) {
        match &mut self.edit {
            Some(edit) => edit.insert(text),
            // Typing on a cell replaces its content, as in a spreadsheet.
            None if !text.chars().all(char::is_control) => self.begin_edit(Some(text), cx),
            None => {}
        }
        cx.notify();
    }

    // ----- Edits -----

    fn apply(&mut self, change: Change) -> Change {
        match change {
            Change::Set(cells) => {
                let before = cells.iter().map(|&(r, c, _)| (r, c, self.table.cell(r, c).into_owned())).collect();
                for (r, c, value) in cells {
                    self.widen(c, value.chars().count());
                    self.table.set_cell(r, c, value);
                }
                Change::Set(before)
            }
            Change::Insert(at, rows) => {
                let n = rows.len();
                for (i, row) in rows.into_iter().enumerate() {
                    self.table.insert_row(at + i);
                    for (c, value) in row.into_iter().enumerate().filter(|(_, v)| !v.is_empty()) {
                        self.table.set_cell(at + i, c, value);
                    }
                }
                Change::Remove(at..at + n)
            }
            Change::Remove(range) => {
                let rows = range.clone().map(|r| self.table.row(r).into_iter().map(|c| c.into_owned()).collect()).collect();
                let at = range.start;
                self.table.remove_rows(range);
                Change::Insert(at, rows)
            }
            // Widths are measured again: `settle` sees they no longer fit the table.
            Change::InsertCols(at, n, saved) => {
                self.table.insert_cols(at, n, &saved);
                self.widths.clear();
                Change::RemoveCols(at..at + n)
            }
            Change::RemoveCols(range) => {
                let (n, at) = (range.len(), range.start);
                let saved = self.table.remove_cols(range);
                self.widths.clear();
                Change::InsertCols(at, n, saved)
            }
            Change::MoveCols(range, to) => {
                let back = Change::MoveCols(to..to + range.len(), range.start);
                self.table.move_cols(range, to);
                self.widths.clear();
                back
            }
            Change::Reorder(start, perm) => {
                self.table.reorder(start, &perm);
                let mut back = vec![0; perm.len()];
                for (i, &from) in perm.iter().enumerate() {
                    back[from] = i;
                }
                Change::Reorder(start, back)
            }
        }
    }

    /// Applies gestures together: a single "undo" reverts them.
    fn commit(&mut self, changes: Vec<Change>, cx: &mut Context<Self>) {
        let mut back: Vec<Change> = changes.into_iter().map(|c| self.apply(c)).collect();
        back.reverse();
        self.undo.push(back);
        if self.undo.len() > UNDO_DEPTH {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.settle(cx);
    }

    fn restore(&mut self, redo: bool, cx: &mut Context<Self>) {
        self.finish(cx);
        let Some(batch) = (if redo { self.redo.pop() } else { self.undo.pop() }) else {
            return;
        };
        let mut back: Vec<Change> = batch.into_iter().map(|c| self.apply(c)).collect();
        back.reverse();
        if redo { &mut self.undo } else { &mut self.redo }.push(back);
        self.settle(cx);
    }

    fn settle(&mut self, cx: &mut Context<Self>) {
        if self.widths.len() != self.cols() {
            self.measure();
        }
        self.clamp();
        self.show(self.cursor);
        self.changed(cx);
        cx.notify();
    }

    /// Clears the cells of the selection.
    fn clear(&mut self, cx: &mut Context<Self>) {
        // Two selections can overlap: each cell only once.
        let all: std::collections::BTreeSet<(usize, usize)> =
            self.selections().into_iter().flat_map(|(rows, cols)| rows.flat_map(move |r| cols.clone().map(move |c| (r, c)))).collect();
        let cells: Vec<_> = all
            .into_iter()
            .filter(|&(r, c)| !self.table.cell(r, c).is_empty())
            .map(|(r, c)| (r, c, String::new()))
            .collect();
        if !cells.is_empty() {
            self.commit(vec![Change::Set(cells)], cx);
        }
    }

    fn insert_rows(&mut self, below: bool, cx: &mut Context<Self>) {
        self.finish(cx);
        self.more.clear();
        let (rows, cols) = self.selection();
        // Whole columns: as many empty columns, to their right or left, selected in turn.
        if self.whole_cols() {
            let at = if below { cols.end } else { cols.start };
            self.commit(vec![Change::InsertCols(at, cols.len(), Vec::new())], cx);
            (self.anchor, self.cursor) = ((self.last_row(), at + cols.len() - 1), (0, at));
            return cx.notify();
        }
        let at = if below { rows.end } else { rows.start };
        self.commit(vec![Change::Insert(at, vec![Vec::new()])], cx);
        self.select_to((at, self.cursor.1), false, cx);
    }

    /// All selections take columns top to bottom, without taking the whole table:
    /// insert and delete then act on columns, not rows.
    fn whole_cols(&self) -> bool {
        let (tall, wide) = (self.table.rows(), self.cols());
        let all = self.selections();
        all.iter().all(|(rows, _)| rows.start == 0 && rows.end >= tall) && !all.iter().any(|(_, cols)| cols.start == 0 && cols.end >= wide)
    }

    /// The whole rows or columns of the selection: Shift+Space and Ctrl+Space, as in a spreadsheet.
    fn select_whole(&mut self, rows: bool, cx: &mut Context<Self>) {
        self.finish(cx);
        self.more.clear();
        let (rs, cs) = self.selection();
        (self.anchor, self.cursor) = match rows {
            true => ((rs.end - 1, self.cols() - 1), (rs.start, 0)),
            false => ((self.last_row(), cs.end - 1), (0, cs.start)),
        };
        cx.notify();
    }

    /// Delete what is selected: the columns when they are taken whole, otherwise the rows.
    fn delete_span(&mut self, cx: &mut Context<Self>) {
        if !self.whole_cols() {
            return self.delete_rows(cx);
        }
        self.finish(cx);
        let cols: std::collections::BTreeSet<usize> = self.selections().into_iter().flat_map(|(_, cols)| cols).collect();
        let mut ranges: Vec<Range<usize>> = Vec::new();
        for c in cols {
            match ranges.last_mut() {
                Some(last) if last.end == c => last.end = c + 1,
                _ => ranges.push(c..c + 1),
            }
        }
        let first = ranges[0].start;
        self.more.clear();
        // Right to left: each removal leaves in place the columns still to remove.
        self.commit(ranges.into_iter().rev().map(Change::RemoveCols).collect(), cx);
        let c = first.min(self.cols() - 1);
        (self.anchor, self.cursor) = ((self.last_row(), c), (0, c));
        cx.notify();
    }

    /// A table gesture, as in a note: on the rows and columns of the selection. Columns
    /// and sorting read the whole file, and a gesture is undone in one go.
    pub fn table_op(&mut self, op: TableOp, cx: &mut Context<Self>) {
        self.finish(cx);
        self.more.clear();
        let (rows, cols) = self.selection();
        let (first, tall, wide) = (self.first(), self.table.rows(), self.cols());
        let (r, c) = self.cursor;
        match op {
            TableOp::RowAbove => self.insert_rows(false, cx),
            TableOp::RowBelow => self.insert_rows(true, cx),
            TableOp::DeleteRow => self.delete_rows(cx),
            TableOp::ColLeft | TableOp::ColRight => {
                let at = if op == TableOp::ColLeft { cols.start } else { cols.end };
                self.commit(vec![Change::InsertCols(at, 1, Vec::new())], cx);
                (self.anchor, self.cursor) = ((r, at), (r, at));
            }
            TableOp::DuplicateRow => {
                let copies = rows.clone().map(|r| self.table.row(r).into_iter().map(|c| c.into_owned()).collect()).collect();
                self.commit(vec![Change::Insert(rows.end, copies)], cx);
                self.select_to((rows.end, c), false, cx);
            }
            TableOp::DuplicateCol => {
                let saved = (0..tall).map(|r| self.table.cells(r, cols.clone()).into_iter().map(|c| c.into_owned()).collect()).collect();
                self.commit(vec![Change::InsertCols(cols.end, cols.len(), saved)], cx);
                (self.anchor, self.cursor) = ((r, cols.end), (r, cols.end));
            }
            // Only the body moves: the header stays at the top, and nothing leaves the table.
            TableOp::MoveRowUp if rows.start > first => {
                let perm: Vec<usize> = (1..=rows.len()).chain([0]).collect();
                self.commit(vec![Change::Reorder(rows.start - 1, perm)], cx);
                (self.anchor, self.cursor) = ((self.anchor.0 - 1, self.anchor.1), (r - 1, c));
                self.settle(cx);
            }
            TableOp::MoveRowDown if rows.end < tall => {
                let perm: Vec<usize> = [rows.len()].into_iter().chain(0..rows.len()).collect();
                self.commit(vec![Change::Reorder(rows.start, perm)], cx);
                (self.anchor, self.cursor) = ((self.anchor.0 + 1, self.anchor.1), (r + 1, c));
                self.settle(cx);
            }
            TableOp::MoveColLeft if cols.start > 0 => {
                self.commit(vec![Change::MoveCols(cols.clone(), cols.start - 1)], cx);
                (self.anchor, self.cursor) = ((self.anchor.0, self.anchor.1 - 1), (r, c - 1));
                self.settle(cx);
            }
            TableOp::MoveColRight if cols.end < wide => {
                self.commit(vec![Change::MoveCols(cols.clone(), cols.start + 1)], cx);
                (self.anchor, self.cursor) = ((self.anchor.0, self.anchor.1 + 1), (r, c + 1));
                self.settle(cx);
            }
            // Removing all columns would leave no table.
            TableOp::DeleteCol if cols.len() < wide => self.commit(vec![Change::RemoveCols(cols)], cx),
            TableOp::SortAsc | TableOp::SortDesc => self.sort(c, op == TableOp::SortAsc, cx),
            _ => {}
        }
    }

    /// The rows under the header, sorted on column `col`: natural sort, empty cells last in
    /// both directions, equal rows in their previous order. Rows are not copied.
    fn sort(&mut self, col: usize, ascending: bool, cx: &mut Context<Self>) {
        let (first, tall) = (self.first(), self.table.rows());
        let keys: Vec<String> = (first..tall).map(|r| self.table.cell(r, col).into_owned()).collect();
        let mut perm: Vec<usize> = (0..keys.len()).collect();
        perm.sort_by(|&a, &b| {
            match (keys[a].trim().is_empty(), keys[b].trim().is_empty()) {
                (true, true) => std::cmp::Ordering::Equal,
                (true, false) => std::cmp::Ordering::Greater,
                (false, true) => std::cmp::Ordering::Less,
                _ => {
                    let order = crate::logic::markdown::cell_order(&keys[a], &keys[b]);
                    if ascending { order } else { order.reverse() }
                }
            }
        });
        if perm.iter().enumerate().any(|(i, &p)| i != p) {
            self.commit(vec![Change::Reorder(first, perm)], cx);
        }
    }

    fn delete_rows(&mut self, cx: &mut Context<Self>) {
        self.finish(cx);
        // The rows of all selections, grouped into ranges and removed from bottom to
        // top: each removal leaves in place the rows still to remove.
        let rows: std::collections::BTreeSet<usize> = self.selections().into_iter().flat_map(|(rows, _)| rows).collect();
        let mut ranges: Vec<Range<usize>> = Vec::new();
        for r in rows.iter().copied() {
            match ranges.last_mut() {
                Some(last) if last.end == r => last.end = r + 1,
                _ => ranges.push(r..r + 1),
            }
        }
        let mut changes: Vec<Change> = ranges.into_iter().rev().map(Change::Remove).collect();
        if rows.len() >= self.table.rows() {
            changes.push(Change::Insert(0, vec![Vec::new()]));
        }
        self.more.clear();
        self.commit(changes, cx);
    }

    // ----- Clipboard and export -----

    fn flash(&mut self, cx: &mut Context<Self>) {
        self.copied = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(1500)).await;
            this.update(cx, |this, cx| {
                this.copied = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The selection, or the whole table if it fits in a single cell.
    fn scope(&self) -> (Range<usize>, Range<usize>, bool) {
        let (rows, cols) = self.selection();
        match rows.len() * cols.len() {
            1 => (0..self.table.rows(), 0..self.cols(), false),
            _ => (rows, cols, true),
        }
    }

    /// Ctrl+C: the selection as TSV, which spreadsheets and note tables paste as is.
    fn copy(&mut self, cx: &mut Context<Self>) {
        // Several selections: their blocks one under the other, top to bottom.
        let blocks: Vec<String> =
            self.selections().into_iter().map(|(rows, cols)| self.table.text_range(rows, cols, Style::Tsv)).collect();
        cx.write_to_clipboard(ClipboardItem::new_string(blocks.join("\n")));
        self.flash(cx);
    }

    /// "Copy table as…": the selection, or the whole table if it fits in one cell.
    pub fn copy_as(&mut self, style: Style, cx: &mut Context<Self>) {
        self.finish(cx);
        let (rows, cols, _) = self.scope();
        self.copy_range(rows, cols, style, cx);
    }

    fn copy_range(&mut self, rows: Range<usize>, cols: Range<usize>, style: Style, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(self.table.text_range(rows, cols, style)));
        self.flash(cx);
    }

    fn cut(&mut self, cx: &mut Context<Self>) {
        self.copy(cx);
        self.clear(cx);
    }

    /// Pasted text: cells (spreadsheet, Markdown table, CSV) fill the grid from
    /// the corner of the selection, which grows if needed; a single one fills the whole selection.
    fn paste(&mut self, text: &str, cx: &mut Context<Self>) {
        let grid = markdown::paste_grid(text).unwrap_or_else(|| table::parse_grid(text));
        if grid.is_empty() {
            return;
        }
        let (rows, cols) = self.selection();
        let (top, left) = (rows.start, cols.start);
        let one = grid.len() == 1 && grid[0].len() == 1;
        let (height, width) = if one { (rows.len(), cols.len()) } else { (grid.len(), grid.iter().map(Vec::len).max().unwrap_or(0)) };
        let mut changes = Vec::new();
        if top + height > self.table.rows() {
            changes.push(Change::Insert(self.table.rows(), vec![Vec::new(); top + height - self.table.rows()]));
        }
        let cells = (0..height)
            .flat_map(|i| (0..width).map(move |j| (i, j)))
            .filter_map(|(i, j)| {
                let value = if one { grid[0][0].clone() } else { grid[i].get(j)?.clone() };
                Some((top + i, left + j, value))
            })
            .collect();
        changes.push(Change::Set(cells));
        self.commit(changes, cx);
        self.more.clear();
        (self.anchor, self.cursor) = ((top, left), (top + height - 1, left + width.max(1) - 1));
        self.show(self.cursor);
    }

    /// Writes the table (or the selection) next to the file, in this format.
    pub fn export(&mut self, style: Style, cx: &mut Context<Self>) {
        self.finish(cx);
        let (rows, cols, partial) = self.scope();
        let text = self.table.text_range(rows, cols, style);
        let suffix = if partial { tr(" (selection)", " (sélection)") } else { "" };
        let name = format!("{}{suffix}", vault::stem(&self.path));
        let file = vault::free_path(self.path.parent().unwrap_or(Path::new("")), &name, style.extension());
        match vault::write(&file, &text) {
            Ok(()) => cx.emit(SheetEvent::Exported),
            Err(e) => cx.emit(SheetEvent::Error(format!("{} : {e}", tr("Not exported", "Export impossible")))),
        }
    }

    // ----- Mouse -----

    fn press(&mut self, e: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus);
        self.finish(cx);
        let extend = e.modifiers.shift;
        let hit = self.hit(e.position);
        // Ctrl+click on a cell, a row or a column: what is selected stays so,
        // another selection starts. A plain click keeps only its own.
        let adding = e.modifiers.secondary();
        let mut kept = std::mem::take(&mut self.more);
        match hit {
            Hit::Cell(..) | Hit::Row(_) | Hit::Col(_) if adding && !extend => kept.push((self.anchor, self.cursor)),
            Hit::Bar(_) => {}
            _ if adding || extend => {}
            _ => kept.clear(),
        }
        if matches!(hit, Hit::Corner) {
            kept.clear();
        }
        match hit {
            Hit::Cell(r, c) => {
                self.drag = Drag::Cells;
                self.select_to((r, c), extend, cx);
                if e.click_count >= 2 {
                    self.begin_edit(None, cx);
                }
            }
            Hit::Row(r) => {
                self.drag = Drag::Rows;
                self.pick_row(r, extend, cx);
            }
            Hit::Col(c) => {
                self.drag = Drag::Cols;
                self.pick_col(c, extend, cx);
                // Double-click on the header: write in its cell.
                if e.click_count >= 2 && self.format.header {
                    self.select_to((0, c), false, cx);
                    self.begin_edit(None, cx);
                }
            }
            Hit::Corner => self.select_all(cx),
            Hit::Bar(vertical) => {
                self.drag = Drag::Bar(vertical);
                self.drag_bar(e.position, vertical, cx);
            }
        }
        self.more = kept;
    }

    fn drag_to(&mut self, e: &MouseMoveEvent, cx: &mut Context<Self>) {
        if e.pressed_button != Some(MouseButton::Left) {
            self.drag = Drag::None;
            return;
        }
        let at = self.hit(e.position);
        match (self.drag, at) {
            (Drag::Cells, Hit::Cell(r, c)) => self.select_to((r, c), true, cx),
            (Drag::Rows, Hit::Cell(r, _) | Hit::Row(r)) => self.pick_row(r, true, cx),
            (Drag::Cols, Hit::Cell(_, c) | Hit::Col(c)) => self.pick_col(c, true, cx),
            (Drag::Bar(vertical), _) => self.drag_bar(e.position, vertical, cx),
            _ => {}
        }
    }

    /// The scrollbar follows the pointer.
    fn drag_bar(&mut self, p: Point<Pixels>, vertical: bool, cx: &mut Context<Self>) {
        let (mx, my) = self.max_scroll();
        let (w, h) = (f32::from(self.body.size.width), f32::from(self.body.size.height));
        if vertical {
            let y = f32::from(p.y - self.body.origin.y) - ROW;
            self.scroll.1 = (y / (h - ROW).max(1.)).clamp(0., 1.) * my;
        } else {
            let x = f32::from(p.x - self.body.origin.x) - GUTTER;
            self.scroll.0 = (x / (w - GUTTER).max(1.)).clamp(0., 1.) * mx;
        }
        cx.notify();
    }

    /// The wheel scrolls; with Shift, sideways.
    fn wheel(&mut self, e: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let mut delta = e.delta.pixel_delta(px(ROW));
        if e.modifiers.shift && delta.x == px(0.) {
            delta = Point { x: delta.y, y: px(0.) };
        }
        self.scroll = (self.scroll.0 - f32::from(delta.x), self.scroll.1 - f32::from(delta.y));
        self.clamp();
        cx.notify();
    }

    /// The whole column `c` (with `extend`, up to the anchor's). The view does not move: the active cell is
    /// the top one, not the last row, where autoscroll would take us.
    fn pick_col(&mut self, c: usize, extend: bool, cx: &mut Context<Self>) {
        self.finish(cx);
        let from = if extend { self.anchor.1 } else { c };
        (self.anchor, self.cursor) = ((self.last_row(), from), (0, c));
        cx.notify();
    }

    /// The whole row `r`, without scrolling to the last column.
    fn pick_row(&mut self, r: usize, extend: bool, cx: &mut Context<Self>) {
        self.finish(cx);
        let from = if extend { self.anchor.0 } else { r };
        (self.anchor, self.cursor) = ((from, self.cols() - 1), (r, 0));
        cx.notify();
    }

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

// ponytail: typing as it comes, without IME composition (composed text is inserted
// as is) nor selection inside the cell; reuse the editor's handler if tables
// get written in Japanese or Chinese.
impl EntityInputHandler for Sheet {
    fn text_for_range(&mut self, _: Range<usize>, _: &mut Option<Range<usize>>, _: &mut Window, _: &mut Context<Self>) -> Option<String> {
        None
    }

    fn selected_text_range(&mut self, _: bool, _: &mut Window, _: &mut Context<Self>) -> Option<UTF16Selection> {
        Some(UTF16Selection { range: 0..0, reversed: false })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        None
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {}

    fn replace_text_in_range(&mut self, _: Option<Range<usize>>, text: &str, _: &mut Window, cx: &mut Context<Self>) {
        self.typed(text, cx);
    }

    fn replace_and_mark_text_in_range(&mut self, _: Option<Range<usize>>, text: &str, _: Option<Range<usize>>, _: &mut Window, cx: &mut Context<Self>) {
        self.typed(text, cx);
    }

    fn bounds_for_range(&mut self, _: Range<usize>, _: Bounds<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<Bounds<Pixels>> {
        None
    }

    fn character_index_for_point(&mut self, _: Point<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        None
    }
}

impl Sheet {
    fn chip(&self, id: &'static str, text: String, on: bool, pick: Pick) -> Stateful<gpui::Div> {
        let t = self.theme;
        div()
            .id(id)
            .px_2()
            .py_0p5()
            .rounded(px(6.))
            .cursor_pointer()
            .text_color(if on { t.accent } else { t.soft })
            .hover(|s| s.bg(t.border))
            .child(text)
            .on_click(move |_, window, cx| window.dispatch_action(pick.action(), cx))
    }

    /// What is selected, when it is whole columns or rows, and its gestures: its name
    /// ("Column “age”", "2 rows"), then icons by group (insert, move, sort or
    /// duplicate, delete), each named with its keys in its tooltip.
    fn span_tools(&self, cx: &mut Context<Self>) -> Vec<gpui::AnyElement> {
        let t = self.theme;
        let (rows, cols) = self.selection();
        let whole_cols = rows.start == 0 && rows.end >= self.table.rows() && cols.len() < self.cols();
        let whole_rows = cols.start == 0 && cols.end >= self.cols() && !whole_cols && rows.len() < self.table.rows();
        type Tool = (&'static str, &'static str, Box<dyn Action>);
        let (title, groups): (String, Vec<Vec<Tool>>) = if whole_cols {
            let named = self.table.cell(0, cols.start).trim().to_string();
            let title = match (cols.len(), self.format.header && !named.is_empty()) {
                (1, true) => format!("{} « {named} »", tr("Column", "Colonne")),
                (1, false) => format!("{} {}", tr("Column", "Colonne"), letters(cols.start)),
                (n, _) => format!("{n} {}", tr("columns", "colonnes")),
            };
            (title, vec![
                vec![
                    ("c-ins-left.svg", TableOp::ColLeft.label(), Box::new(editor::TableColLeft)),
                    ("c-ins-right.svg", TableOp::ColRight.label(), Box::new(editor::TableColRight)),
                ],
                vec![
                    ("c-left.svg", TableOp::MoveColLeft.label(), Box::new(editor::TableMoveColLeft)),
                    ("c-right.svg", TableOp::MoveColRight.label(), Box::new(editor::TableMoveColRight)),
                ],
                vec![
                    ("c-sort-az.svg", TableOp::SortAsc.label(), Box::new(editor::TableSortAsc)),
                    ("c-sort-za.svg", TableOp::SortDesc.label(), Box::new(editor::TableSortDesc)),
                ],
                vec![("trash.svg", TableOp::DeleteCol.label(), Box::new(editor::TableDeleteCol))],
            ])
        } else if whole_rows {
            let title = match rows.len() {
                1 => format!("{} {}", tr("Row", "Ligne"), rows.start + 1),
                n => format!("{n} {}", tr("rows", "lignes")),
            };
            (title, vec![
                vec![
                    ("c-ins-above.svg", tr("Insert a row above", "Insérer une ligne au-dessus"), Box::new(InsertAbove)),
                    ("c-ins-below.svg", tr("Insert a row below", "Insérer une ligne en dessous"), Box::new(InsertBelow)),
                ],
                vec![
                    ("c-up.svg", TableOp::MoveRowUp.label(), Box::new(editor::TableMoveRowUp)),
                    ("c-down.svg", TableOp::MoveRowDown.label(), Box::new(editor::TableMoveRowDown)),
                ],
                vec![("copy.svg", TableOp::DuplicateRow.label(), Box::new(editor::TableDuplicateRow))],
                vec![("trash.svg", tr("Delete the rows", "Supprimer les lignes"), Box::new(DeleteRows))],
            ])
        } else {
            return Vec::new();
        };
        let separator = || div().flex_none().w(px(1.)).h(px(16.)).mx(px(6.)).bg(t.border).into_any_element();
        let mut out = vec![separator(), div().flex_none().mr_1().font_weight(FontWeight::BOLD).text_color(t.text).child(title).into_any_element()];
        for (g, group) in groups.into_iter().enumerate() {
            if g > 0 {
                out.push(div().flex_none().w(px(1.)).h(px(14.)).mx(px(3.)).bg(t.border).into_any_element());
            }
            for (icon, label, action) in group {
                let name = action.name();
                out.push(
                    button(gpui::SharedString::from(format!("sheet-span-{name}")), icon, false, t)
                        .debug_selector(move || format!("sheet-span-{name}"))
                        .tooltip(tip(label, crate::ui::keys::of(cx, action.as_ref()), t))
                        .on_click(move |_, window, cx| window.dispatch_action(action.boxed_clone(), cx))
                        .into_any_element(),
                );
            }
        }
        out
    }

    fn render_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        let name = self.path.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let info = format!("{name} · {} × {}", self.table.rows(), self.table.cols());
        let delimiter = delimiter_option(self.format.delimiter);
        div()
            .flex_none()
            .min_h(px(40.))
            .pl_4()
            // Under the window badge, when the app draws its title bar.
            .pr(px(if self.client { 128. } else { 8. }))
            .flex()
            // In a narrow window, settings wrap instead of sliding under the badge.
            .flex_wrap()
            .items_center()
            .gap_1()
            .border_b_1()
            .border_color(t.border)
            .text_size(px(12.))
            .child(
                div()
                    .flex_shrink()
                    .h(px(38.))
                    .flex()
                    .items_center()
                    .min_w_0()
                    .truncate()
                    .text_color(t.soft)
                    .when(self.client, |d| d.map(crate::ui::widgets::drag_window))
                    .child(info),
            )
            // Next to the table name: what is selected, and what can be done with it.
            .children(self.span_tools(cx))
            .child(div().flex_1().min_w(px(8.)).h(px(38.)).when(self.client, |d| d.map(crate::ui::widgets::drag_window)))
            .child(self.chip("sheet-delimiter", delimiter, false, Pick::Delimiter))
            .child(self.chip("sheet-encoding", self.format.encoding.label().to_string(), false, Pick::Encoding))
            .child(self.chip("sheet-header", tr("Header", "En-tête").to_string(), self.format.header, Pick::Header))
            .child(div().flex_none().w(px(1.)).h(px(16.)).mx(px(6.)).bg(t.border))
            .child(
                button("sheet-copy", if self.copied { "check.svg" } else { "copy.svg" }, false, t)
                    .tooltip(tip(tr("Copy the selection", "Copier la sélection"), crate::ui::keys::of(cx, &Copy), t))
                    .on_click(cx.listener(|this, _, _, cx| this.copy(cx))),
            )
            .child(
                button("sheet-export", "export.svg", false, t)
                    .tooltip(tip(tr("Export the table…", "Exporter le tableau…"), String::new(), t))
                    .on_click(|_, window, cx| window.dispatch_action(Pick::Export.action(), cx)),
            )
    }

    /// A row of cells, from column `cols.start` to `cols.end` excluded.
    fn render_row(&self, row: usize, cols: Range<usize>, selected: &[(Range<usize>, Range<usize>)], head: bool) -> gpui::Div {
        let t = self.theme;
        let editing = self.edit.is_some();
        let size = px(self.text_px());
        div()
            .absolute()
            .top(px(self.y_of(row)))
            .left(px(0.))
            .h(px(ROW))
            .flex()
            .child(div().flex_none().w(px(self.xs[cols.start])))
            .children(cols.clone().zip(self.table.cells(row, cols)).map(|(c, field)| {
                let mut text = field.into_owned();
                if text.len() > 400 {
                    text.truncate(text.char_indices().nth(200).map_or(text.len(), |(i, _)| i));
                }
                let text = if text.contains(['\n', '\r']) { text.replace(['\r', '\n'], "↵") } else { text };
                let inside = selected.iter().any(|(rows, cols)| rows.contains(&row) && cols.contains(&c));
                div()
                    .relative()
                    .flex_none()
                    .w(px(self.widths[c]))
                    .h(px(ROW))
                    .px(px(PAD))
                    .flex()
                    .items_center()
                    .border_r_1()
                    .border_b_1()
                    .border_color(t.border)
                    .when(head, |d| d.bg(t.code_bg).font_weight(FontWeight::SEMIBOLD))
                    .when(inside, |d| d.bg(t.selection))
                    .child(div().min_w_0().truncate().child(text))
                    .when(self.cursor == (row, c) && !editing, |d| {
                        d.child(div().absolute().inset_0().border_2().border_color(t.accent))
                    })
            }))
            .text_size(size)
    }

    fn render_edit(&self, edit: &Line) -> gpui::Div {
        let t = self.theme;
        let (row, col) = self.cursor;
        div()
            .absolute()
            .left(px(self.xs[col] - self.scroll.0))
            .top(px(self.y_of(row)))
            .min_w(px(self.widths[col]))
            .h(px(ROW))
            .px(px(PAD))
            .cursor_text()
            .flex()
            .items_center()
            .whitespace_nowrap()
            .bg(t.panel)
            .border_2()
            .border_color(t.accent)
            .text_size(px(self.text_px()))
            .child(edit.shown(t).0)
    }

    fn render_bars(&self) -> impl IntoElement {
        let t = self.theme;
        let (mx, my) = self.max_scroll();
        let (w, h) = (f32::from(self.body.size.width), f32::from(self.body.size.height));
        let thumb = |track: f32, scroll: f32, max: f32, shown: f32| {
            let len = (track * shown / (shown + max)).max(24.);
            (scroll / max * (track - len), len)
        };
        div()
            .when(my > 0., |d| {
                let (at, len) = thumb(h - ROW, self.scroll.1, my, self.view().1);
                d.child(div().absolute().right(px(2.)).top(px(ROW + at)).w(px(6.)).h(px(len)).rounded(px(3.)).cursor_pointer().bg(t.dim.opacity(0.45)))
            })
            .when(mx > 0., |d| {
                let (at, len) = thumb(w - GUTTER, self.scroll.0, mx, self.view().0);
                d.child(div().absolute().bottom(px(2.)).left(px(GUTTER + at)).h(px(6.)).w(px(len)).rounded(px(3.)).cursor_pointer().bg(t.dim.opacity(0.45)))
            })
    }
}

impl Render for Sheet {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        let (this, focus) = (cx.entity(), self.focus.clone());
        let measure = canvas(
            |_, _, _| (),
            move |bounds, _, window, cx| {
                window.handle_input(&focus, ElementInputHandler::new(bounds, this.clone()), cx);
                this.update(cx, |sheet, cx| sheet.measured(bounds, cx));
            },
        )
        .absolute()
        .size_full();

        // Columns and rows that overflow the view are not built.
        let (lines, columns) = self.visible(window.viewport_size());
        let (r0, r1, c0, c1) = (lines.start, lines.end, columns.start, columns.end);
        let selected = self.selections();

        let head = if self.format.header {
            self.render_row(0, c0..c1, &selected, true)
        } else {
            div().absolute().top(px(0.)).left(px(0.)).h(px(ROW)).flex().text_size(px(self.text_px())).child(div().flex_none().w(px(self.xs[c0]))).children(
                (c0..c1).map(|c| {
                    let inside = selected.iter().any(|(_, cols)| cols.contains(&c));
                    div()
                        .flex_none()
                        .w(px(self.widths[c]))
                        .h(px(ROW))
                        .flex()
                        .items_center()
                        .justify_center()
                        .border_r_1()
                        .border_b_1()
                        .border_color(t.border)
                        .bg(t.code_bg)
                        .text_color(t.soft)
                        .when(inside, |d| d.bg(t.selection))
                        .child(letters(c))
                }),
            )
        };
        let rows = (r0..r1).map(|r| self.render_row(r, c0..c1, &selected, false));
        let grid = div()
            .absolute()
            .top(px(0.))
            .left(px(GUTTER))
            .right(px(0.))
            .bottom(px(0.))
            .overflow_hidden()
            .child(div().absolute().top(px(0.)).left(px(-self.scroll.0)).size_full().children(rows))
            .child(div().absolute().top(px(0.)).left(px(-self.scroll.0)).w_full().h(px(ROW)).cursor_pointer().child(head))
            .children(self.edit.as_ref().map(|edit| self.render_edit(edit)));

        let number = |r: usize, y: f32| {
            let inside = selected.iter().any(|(rows, _)| rows.contains(&r));
            div()
                .absolute()
                .top(px(y))
                .left(px(0.))
                .w(px(GUTTER))
                .h(px(ROW))
                .flex()
                .items_center()
                .justify_center()
                .border_r_1()
                .border_b_1()
                .border_color(t.border)
                .bg(t.code_bg)
                .text_color(if inside { t.text } else { t.soft })
                .when(inside, |d| d.bg(t.selection))
                .child((r + 1).to_string())
        };
        let gutter = div()
            .absolute()
            .top(px(0.))
            .left(px(0.))
            .w(px(GUTTER))
            .h_full()
            .overflow_hidden()
            // A click there takes a whole row, as a click on a header takes its column.
            .cursor_pointer()
            .bg(t.bg)
            .text_size(px(self.text_px()))
            .children((r0..r1).map(|r| number(r, self.y_of(r))))
            .child(div().absolute().top(px(0.)).left(px(0.)).w(px(GUTTER)).h(px(ROW)).border_r_1().border_b_1().border_color(t.border).bg(t.code_bg))
            .children(self.format.header.then(|| number(0, 0.)));

        let body = div()
            .flex_1()
            .min_h_0()
            .relative()
            .overflow_hidden()
            .font_family(mono())
            .child(measure)
            .child(grid)
            .child(gutter)
            .child(self.render_bars())
            .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, window, cx| this.press(e, window, cx)))
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| this.drag_to(e, cx)))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _: &MouseUpEvent, _, _| this.drag = Drag::None))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _: &MouseUpEvent, _, _| this.drag = Drag::None))
            .on_scroll_wheel(cx.listener(|this, e: &ScrollWheelEvent, _, cx| this.wheel(e, cx)));

        line::keys(div(), cx, |this| this.edit.as_mut(), |_, _| ())
            .size_full()
            .flex()
            .flex_col()
            .bg(t.bg)
            .text_color(t.text)
            // A cell being typed in: the keys of an input line come before
            // those of the grid (arrows, deletion, clipboard).
            .key_context(if self.edit.is_some() { "Sheet Line" } else { "Sheet" })
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &Left, _, cx| this.go(0, -1, false, cx)))
            .on_action(cx.listener(|this, _: &Right, _, cx| this.go(0, 1, false, cx)))
            .on_action(cx.listener(|this, _: &Up, _, cx| this.go(-1, 0, false, cx)))
            .on_action(cx.listener(|this, _: &Down, _, cx| this.go(1, 0, false, cx)))
            .on_action(cx.listener(|this, _: &ExtendLeft, _, cx| this.go(0, -1, true, cx)))
            .on_action(cx.listener(|this, _: &ExtendRight, _, cx| this.go(0, 1, true, cx)))
            .on_action(cx.listener(|this, _: &ExtendUp, _, cx| this.go(-1, 0, true, cx)))
            .on_action(cx.listener(|this, _: &ExtendDown, _, cx| this.go(1, 0, true, cx)))
            .on_action(cx.listener(|this, _: &PageUp, _, cx| this.go(-this.page(), 0, false, cx)))
            .on_action(cx.listener(|this, _: &PageDown, _, cx| this.go(this.page(), 0, false, cx)))
            .on_action(cx.listener(|this, _: &RowStart, _, cx| this.select_to((this.cursor.0, 0), false, cx)))
            .on_action(cx.listener(|this, _: &RowEnd, _, cx| this.select_to((this.cursor.0, this.cols() - 1), false, cx)))
            .on_action(cx.listener(|this, _: &Origin, _, cx| {
                this.finish(cx);
                this.select_to((0, 0), false, cx)
            }))
            .on_action(cx.listener(|this, _: &Corner, _, cx| {
                this.finish(cx);
                this.select_to((this.last_row(), this.cols() - 1), false, cx)
            }))
            .on_action(cx.listener(|this, _: &FirstRow, _, cx| {
                this.finish(cx);
                this.select_to((0, this.cursor.1), false, cx)
            }))
            .on_action(cx.listener(|this, _: &LastRow, _, cx| {
                this.finish(cx);
                this.select_to((this.last_row(), this.cursor.1), false, cx)
            }))
            // Tab moves to the next cell, and to the next row at the end of this one.
            .on_action(cx.listener(|this, _: &Next, _, cx| {
                this.finish(cx);
                let (r, c) = this.cursor;
                match c + 1 < this.cols() {
                    true => this.go(0, 1, false, cx),
                    false if r < this.last_row() => this.select_to((r + 1, 0), false, cx),
                    false => {}
                }
            }))
            .on_action(cx.listener(|this, _: &Previous, _, cx| {
                this.finish(cx);
                let (r, c) = this.cursor;
                match c {
                    0 if r > 0 => this.select_to((r - 1, this.cols() - 1), false, cx),
                    0 => {}
                    _ => this.go(0, -1, false, cx),
                }
            }))
            // Enter commits and moves down; outside input, it opens the cell.
            .on_action(cx.listener(|this, _: &Enter, _, cx| match this.edit {
                Some(_) => this.go(1, 0, false, cx),
                None => this.begin_edit(None, cx),
            }))
            .on_action(cx.listener(|this, _: &Edit, _, cx| this.begin_edit(None, cx)))
            .on_action(cx.listener(|this, _: &Cancel, _, cx| {
                if this.edit.take().is_none() {
                    this.anchor = this.cursor;
                }
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Backspace, _, cx| this.clear(cx)))
            .on_action(cx.listener(|this, _: &Delete, _, cx| this.clear(cx)))
            .on_action(cx.listener(|this, _: &Copy, _, cx| this.copy(cx)))
            .on_action(cx.listener(|this, _: &Cut, _, cx| this.cut(cx)))
            .on_action(cx.listener(|this, _: &Paste, _, cx| {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    this.paste(&text, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &SelectAll, _, cx| this.select_all(cx)))
            .on_action(cx.listener(|this, _: &Undo, _, cx| this.restore(false, cx)))
            .on_action(cx.listener(|this, _: &Redo, _, cx| this.restore(true, cx)))
            .on_action(cx.listener(|this, _: &InsertBelow, _, cx| this.insert_rows(true, cx)))
            .on_action(cx.listener(|this, _: &InsertAbove, _, cx| this.insert_rows(false, cx)))
            .on_action(cx.listener(|this, _: &DeleteRows, _, cx| this.delete_span(cx)))
            .on_action(cx.listener(|this, _: &editor::TableDeleteSpan, _, cx| this.delete_span(cx)))
            .on_action(cx.listener(|this, _: &SelectRow, _, cx| this.select_whole(true, cx)))
            .on_action(cx.listener(|this, _: &SelectCol, _, cx| this.select_whole(false, cx)))
            .on_action(cx.listener(|this, _: &editor::TableRowAbove, _, cx| this.table_op(TableOp::RowAbove, cx)))
            .on_action(cx.listener(|this, _: &editor::TableRowBelow, _, cx| this.table_op(TableOp::RowBelow, cx)))
            .on_action(cx.listener(|this, _: &editor::TableColLeft, _, cx| this.table_op(TableOp::ColLeft, cx)))
            .on_action(cx.listener(|this, _: &editor::TableColRight, _, cx| this.table_op(TableOp::ColRight, cx)))
            .on_action(cx.listener(|this, _: &editor::TableDuplicateRow, _, cx| this.table_op(TableOp::DuplicateRow, cx)))
            .on_action(cx.listener(|this, _: &editor::TableDuplicateCol, _, cx| this.table_op(TableOp::DuplicateCol, cx)))
            .on_action(cx.listener(|this, _: &editor::TableMoveRowUp, _, cx| this.table_op(TableOp::MoveRowUp, cx)))
            .on_action(cx.listener(|this, _: &editor::TableMoveRowDown, _, cx| this.table_op(TableOp::MoveRowDown, cx)))
            .on_action(cx.listener(|this, _: &editor::TableMoveColLeft, _, cx| this.table_op(TableOp::MoveColLeft, cx)))
            .on_action(cx.listener(|this, _: &editor::TableMoveColRight, _, cx| this.table_op(TableOp::MoveColRight, cx)))
            .on_action(cx.listener(|this, _: &editor::TableDeleteRow, _, cx| this.table_op(TableOp::DeleteRow, cx)))
            .on_action(cx.listener(|this, _: &editor::TableDeleteCol, _, cx| this.table_op(TableOp::DeleteCol, cx)))
            .on_action(cx.listener(|this, _: &editor::TableSortAsc, _, cx| this.table_op(TableOp::SortAsc, cx)))
            .on_action(cx.listener(|this, _: &editor::TableSortDesc, _, cx| this.table_op(TableOp::SortDesc, cx)))
            .child(self.render_bar(cx))
            .child(body)
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
