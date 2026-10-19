//! Table grid: columns share the page width and cell text
//! wraps, as on the web. The Markdown stays plain text:
//! only the rendering changes.

use std::{
    hash::{Hash, Hasher},
    ops::Range,
    rc::Rc,
};

use gpui::{
    App, Bounds, ContentMask, Pixels, Point, TextAlign, Window, WindowTextSystem, WrappedLine, fill, point, px, size,
};

use crate::{
    ui::theme::Theme,
    views::editor::{Fast, FastMap, RunStyle, text_runs},
    logic::{formula, markdown as md},
};

/// Inner padding of a cell, and stroke thickness.
pub const PAD_X: Pixels = px(10.);
pub const PAD_Y: Pixels = px(5.);
pub const LINE: Pixels = px(1.);
/// Text width of a column whose cells are short or empty, and the narrowest
/// it is given: below that, the table scrolls.
const MIN_WIDE: f32 = 48.;
const NARROWEST: f32 = 36.;

/// Text of a cell laid out (on one line, or at its column's width).
pub struct Shape {
    line: Option<WrappedLine>,
    /// Width of the longest unbreakable piece, measured when the text is on one line.
    narrow: Pixels,
    /// `\|` written, `|` shown: (byte, written length, shown length), as
    /// for the signs of a line.
    subs: Vec<(usize, usize, usize)>,
}

/// Layouts from the last frame, reused while their text and width
/// do not change: typing in a cell only re-lays out the ones that move.
#[derive(Default)]
pub struct Cache {
    old: FastMap<u64, Rc<Shape>>,
    new: FastMap<u64, Rc<Shape>>,
    /// Whole tables, by hash of their text: a table that does not change
    /// (the cursor or the page moves) is not laid out again.
    tables_old: FastMap<u64, Tables>,
    tables_new: FastMap<u64, Tables>,
}

/// The lines of a table, laid out.
pub type Tables = Rc<Vec<Option<Rc<TableRow>>>>;

impl Cache {
    /// The layouts kept for the next frame do not grow without end when scrolling
    /// a very big table: the lines shown keep theirs.
    pub fn trim(&mut self, limit: usize) {
        if self.new.len() > limit {
            self.new.clear();
        }
    }

    pub fn next_frame(&mut self) {
        self.old = std::mem::take(&mut self.new);
        self.tables_old = std::mem::take(&mut self.tables_new);
    }

    fn shape(&mut self, key: u64, make: impl FnOnce() -> Shape) -> Rc<Shape> {
        let shape = self.old.remove(&key).or_else(|| self.new.get(&key).cloned()).unwrap_or_else(|| Rc::new(make()));
        self.new.insert(key, shape.clone());
        shape
    }
}

/// What, besides the text, changes the layout.
pub struct Look<'a> {
    pub theme: &'a Theme,
    /// Hash of the theme and fonts: it changes the key of every cell.
    pub style: u64,
}

/// Text size of a cell, and its line height.
pub fn font_size(theme: &Theme) -> Pixels {
    px(15.) * (theme.size / 16.)
}

pub fn line_height(theme: &Theme) -> Pixels {
    (font_size(theme) * 1.5).round()
}

/// Text byte → shown text byte.
pub fn shown(subs: &[(usize, usize, usize)], i: usize) -> usize {
    let mut shift = 0isize;
    for &(at, src, dst) in subs {
        if i <= at {
            break;
        }
        // Inside a sign: go just after it.
        if i < at + src {
            return (at as isize + shift) as usize + dst;
        }
        shift += dst as isize - src as isize;
    }
    (i as isize + shift) as usize
}

/// Shown text byte → text byte.
pub fn source(subs: &[(usize, usize, usize)], i: usize) -> usize {
    let mut shift = 0isize;
    for &(at, src, dst) in subs {
        let start = (at as isize + shift) as usize;
        if i <= start {
            break;
        }
        if i < start + dst {
            return at + src;
        }
        shift += dst as isize - src as isize;
    }
    (i as isize - shift) as usize
}

/// The characters GPUI's wrapping does not separate from the preceding word.
fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(c, '\u{00C0}'..='\u{024F}' | '\u{0400}'..='\u{04FF}')
        || matches!(c, '-' | '_' | '.' | '\'' | '$' | '%' | '@' | '#' | '^' | '~' | ',' | '=' | ':' | '⋯')
}

/// The pieces wrapping does not cut, as it sees them: a word with
/// the spaces that follow it; each ideogram, each sign stands alone.
fn units(text: &str) -> Vec<Range<usize>> {
    let (mut found, mut start, mut prev) = (Vec::new(), 0, '\0');
    for (i, c) in text.char_indices() {
        if i > 0 && ((!is_word_char(c) && c != ' ') || (prev == ' ' && c != ' ')) {
            found.push(start..i);
            start = i;
        }
        prev = c;
    }
    found.push(start..text.len());
    found
}

/// X of byte `i` in the unwrapped line.
fn x_at(line: &WrappedLine, i: usize) -> Pixels {
    line.unwrapped_layout.x_for_index(i)
}

/// Width of the longest unbreakable piece.
fn longest(line: &WrappedLine) -> Pixels {
    let trimmed = |r: Range<usize>| r.start..line.text[r.clone()].trim_end().len() + r.start;
    let widths = units(&line.text).into_iter().map(trimmed).map(|r| x_at(line, r.end) - x_at(line, r.start));
    widths.fold(px(0.), |a, b| a.max(b))
}

/// Content width of each column, like an HTML table: all the text on one
/// line if the page allows it; otherwise short columns keep their width and long ones
/// share the rest, without going below their longest word; and if even the words do not
/// fit, the longest are cut. `narrow`: longest word; `wide`: text on one line.
pub fn column_widths(narrow: &[f32], wide: &[f32], avail: f32) -> Vec<f32> {
    let total = |v: &[f32]| v.iter().sum::<f32>();
    if total(wide) <= avail {
        return wide.to_vec();
    }
    let narrow: Vec<f32> = narrow.iter().zip(wide).map(|(n, w)| n.min(*w)).collect();
    if total(&narrow) <= avail {
        // One shared level for the columns that need it: short ones keep their width,
        // long ones share the rest, without going below their longest word.
        let take = |level: f32| narrow.iter().zip(wide).map(|(n, w)| level.clamp(*n, *w)).sum::<f32>();
        let (mut low, mut high) = (0., wide.iter().copied().fold(0., f32::max));
        for _ in 0..40 {
            let mid = (low + high) / 2.;
            if take(mid) > avail { high = mid } else { low = mid }
        }
        return narrow.iter().zip(wide).map(|(n, w)| low.clamp(*n, *w)).collect();
    }
    // Lower the widest columns to one shared level, like water in vessels.
    let mut sorted = narrow.clone();
    sorted.sort_by(f32::total_cmp);
    let (mut left, mut level) = (avail, f32::MAX);
    for (k, w) in sorted.iter().enumerate() {
        let share = left / (sorted.len() - k) as f32;
        if *w > share {
            level = share;
            break;
        }
        left -= w;
    }
    let level = level.max(NARROWEST);
    narrow.iter().map(|n| n.min(level)).collect()
}

/// A cell: its content in the line and its laid-out text.
#[derive(Clone)]
pub struct Cell {
    /// Content, without surrounding spaces (bytes of the line); empty at the end of the line
    /// for a column it does not have.
    pub range: Range<usize>,
    shape: Rc<Shape>,
    align: TextAlign,
    /// Text width: that of the column.
    width: Pixels,
    /// The text does not wrap (very big table): what overflows is cut.
    clip: bool,
    /// A formula without a result: why.
    pub reason: Option<Rc<str>>,
}

impl Cell {
    fn lines(&self) -> i32 {
        self.shape.line.as_ref().map_or(1, |l| l.wrap_boundaries.len() as i32 + 1)
    }

    /// Width of visual line `k`.
    fn line_width(&self, k: usize) -> Pixels {
        let Some(line) = &self.shape.line else {
            return px(0.);
        };
        let edge = |b: &gpui::WrapBoundary| line.runs()[b.run_ix].glyphs[b.glyph_ix].position.x;
        let start = k.checked_sub(1).map_or(px(0.), |k| edge(&line.wrap_boundaries[k]));
        line.wrap_boundaries.get(k).map_or(line.unwrapped_layout.width, edge) - start
    }

    /// Offset of visual line `k` due to the column's alignment.
    fn shift(&self, k: usize) -> Pixels {
        let room = (self.width - self.line_width(k)).max(px(0.));
        match self.align {
            TextAlign::Left => px(0.),
            TextAlign::Center => room / 2.,
            TextAlign::Right => room,
        }
    }

    /// Position of content byte `i`, from the cell's corner.
    fn position(&self, i: usize, lh: Pixels) -> Point<Pixels> {
        let Some(line) = &self.shape.line else {
            return point(self.shift(0), px(0.));
        };
        let i = shown(&self.shape.subs, i);
        // At a wrap, the cursor is at the start of the next line, not at the end of the previous one.
        let first = |b: &gpui::WrapBoundary| line.runs()[b.run_ix].glyphs[b.glyph_ix].index;
        if let Some(k) = line.wrap_boundaries.iter().position(|b| first(b) == i) {
            return point(self.shift(k + 1), lh * (k + 1) as f32);
        }
        let p = line.position_for_index(i, lh).unwrap_or_default();
        point(p.x + self.shift((f32::from(p.y) / f32::from(lh)).round() as usize), p.y)
    }

    /// Content byte closest to point `p`, from the cell's corner.
    fn index(&self, p: Point<Pixels>, lh: Pixels) -> usize {
        let Some(line) = &self.shape.line else {
            return 0;
        };
        let k = ((f32::from(p.y) / f32::from(lh)).floor().max(0.) as usize).min(line.wrap_boundaries.len());
        let at = point(p.x - self.shift(k), lh * (k as f32 + 0.5));
        match line.closest_index_for_position(at, lh) {
            Ok(i) | Err(i) => source(&self.shape.subs, i),
        }
    }

    /// Bands to tint to select `from..to`: (x, y, width) per visual line.
    fn spans(&self, from: usize, to: usize, lh: Pixels) -> Vec<(Pixels, Pixels, Pixels)> {
        let (a, b) = (self.position(from, lh), self.position(to, lh));
        let row = |p: Point<Pixels>| (f32::from(p.y) / f32::from(lh)).round() as usize;
        (row(a)..=row(b))
            .map(|k| {
                let start = if k == row(a) { a.x } else { self.shift(k) };
                let end = if k == row(b) { b.x } else { self.shift(k) + self.line_width(k) };
                (start, lh * k as f32, end - start)
            })
            .collect()
    }
}

/// A table line. Rows are lines of the text: bytes are those of the line.
#[derive(Clone)]
pub struct TableRow {
    pub cells: Vec<Cell>,
    /// X of the vertical strokes, from the page's left edge: one more than columns.
    xs: Rc<Vec<Pixels>>,
    /// Visual lines of the tallest cell; 0 for the dash line, which is not seen.
    pub lines: i32,
    header: bool,
    /// Thickness of the stroke above (thicker under the header) and of the one below (last line).
    top: Pixels,
    bottom: Pixels,
    /// Positions of the `|` in the line, and number of cells it writes.
    bars: Vec<usize>,
    written: usize,
}

impl TableRow {
    pub fn hidden(&self) -> bool {
        self.lines == 0
    }

    /// Table width, strokes included.
    pub fn total(&self) -> Pixels {
        self.xs.last().map_or(px(0.), |x| *x + LINE)
    }

    /// X of each vertical stroke, from the table's left edge: one more than columns.
    pub fn xs(&self) -> &[Pixels] {
        &self.xs
    }

    /// Left edge of the text of column `c`.
    fn left(&self, c: usize) -> Pixels {
        self.xs[c] + LINE + PAD_X
    }

    /// Space above the text.
    pub fn pad(&self) -> Pixels {
        self.top + PAD_Y
    }

    /// Line height, for visual lines of `lh`.
    pub fn height(&self, lh: Pixels) -> Pixels {
        if self.hidden() { px(0.) } else { self.pad() + lh * self.lines as f32 + PAD_Y + self.bottom }
    }

    /// Column where byte `i` is: that of the `|` before it.
    fn column(&self, i: usize) -> usize {
        let before = self.bars.iter().filter(|&&bar| bar < i).count();
        before.saturating_sub(1).min(self.written.saturating_sub(1))
    }

    /// Why the formula in the cell where byte `i` is has no result.
    pub fn reason_at(&self, i: usize) -> Option<Rc<str>> {
        self.cells.get(self.column(i))?.reason.clone()
    }

    /// What each cell shows, for the tests.
    #[cfg(test)]
    pub fn shown(&self) -> Vec<String> {
        self.cells.iter().map(|c| c.shape.line.as_ref().map_or(String::new(), |l| l.text.to_string())).collect()
    }

    /// Visual lines of the cell where byte `i` is.
    pub fn lines_at(&self, i: usize) -> i32 {
        self.cells.get(self.column(i)).map_or(0, Cell::lines)
    }

    /// Position of byte `i`, from the corner of the line's text. In the `|` or
    /// the spaces between cells, that of the neighbouring cell.
    pub fn pos(&self, i: usize, lh: Pixels) -> Point<Pixels> {
        let c = self.column(i);
        let Some(cell) = self.cells.get(c) else {
            return Point::default();
        };
        let p = cell.position(i.clamp(cell.range.start, cell.range.end) - cell.range.start, lh);
        point(self.left(c) + p.x, p.y)
    }

    /// Byte closest to point `p`, from the corner of the line's text.
    pub fn index_at(&self, p: Point<Pixels>, lh: Pixels) -> usize {
        let Some(last) = self.cells.len().checked_sub(1) else {
            return 0;
        };
        let c = self.xs.partition_point(|&x| x <= p.x).saturating_sub(1).min(last);
        let cell = &self.cells[c];
        cell.range.start + cell.index(point(p.x - self.left(c), p.y), lh)
    }

    /// Bands to tint to select `from..to`: (x, y, width), from the corner
    /// of the line's text. The `|` and spaces between cells are not tinted.
    pub fn spans(&self, from: usize, to: usize, lh: Pixels) -> Vec<(Pixels, Pixels, Pixels)> {
        let mut found = Vec::new();
        for (c, cell) in self.cells.iter().enumerate() {
            let (a, b) = (from.max(cell.range.start), to.min(cell.range.end));
            if a < b {
                let at = self.left(c);
                found.extend(cell.spans(a - cell.range.start, b - cell.range.start, lh).into_iter().map(|(x, y, w)| (at + x, y, w)));
            }
        }
        found
    }

    /// Header background, drawn under the text. `at`: corner of the line on screen;
    /// `height`: its height, without the space left under the table.
    pub fn paint_back(&self, at: Point<Pixels>, height: Pixels, theme: &Theme, window: &mut Window) {
        if self.header {
            window.paint_quad(fill(Bounds::new(at, size(self.total(), height)), theme.code_bg));
        }
    }

    /// The cells' text and the strokes, drawn on top.
    pub fn paint_front(&self, at: Point<Pixels>, height: Pixels, lh: Pixels, theme: &Theme, window: &mut Window, cx: &mut App) {
        if self.hidden() {
            return;
        }
        for (c, cell) in self.cells.iter().enumerate() {
            if let Some(line) = &cell.shape.line {
                let origin = point(at.x + self.left(c), at.y + self.pad());
                let area = Bounds::new(origin, size(cell.width, lh * cell.lines() as f32));
                if cell.clip {
                    window.with_content_mask(Some(ContentMask { bounds: area }), |window| {
                        line.paint(origin, lh, cell.align, Some(area), window, cx).ok();
                    });
                } else {
                    line.paint(origin, lh, cell.align, Some(area), window, cx).ok();
                }
            }
        }
        let mut rule = |x: Pixels, y: Pixels, w: Pixels, h: Pixels| {
            window.paint_quad(fill(Bounds::new(at + point(x, y), size(w, h)), theme.border));
        };
        rule(px(0.), px(0.), self.total(), self.top);
        if self.bottom > px(0.) {
            rule(px(0.), height - self.bottom, self.total(), self.bottom);
        }
        for x in self.xs.iter() {
            rule(*x, px(0.), LINE, height);
        }
    }
}

/// A piece of text to lay out.
fn key(text: &str, wrap: Option<Pixels>, bold: bool, marked: &Option<Range<usize>>, look: &Look) -> u64 {
    let mut hasher = Fast::default();
    (text, wrap.map(|w| f32::from(w).to_bits()), bold, marked, look.style, f32::from(font_size(look.theme)).to_bits()).hash(&mut hasher);
    hasher.finish()
}

/// Lays out a cell's text, on one line or at width `wrap`.
fn shape(
    text_system: &WindowTextSystem,
    text: &str,
    wrap: Option<Pixels>,
    bold: bool,
    marked: &Option<Range<usize>>,
    look: &Look,
) -> Shape {
    if text.is_empty() {
        return Shape { line: None, narrow: px(0.), subs: Vec::new() };
    }
    let mut flags = vec![0u16; text.len()];
    md::inline(text, 0, &mut flags);
    shape_flags(text_system, text, flags, wrap, bold, marked, look)
}

/// The result of a formula, shown in place of its text (`written` bytes): plain, in the style `flags`.
fn shape_computed(text_system: &WindowTextSystem, shown: &Shown, wrap: Option<Pixels>, bold: bool, look: &Look) -> Shape {
    if shown.text.is_empty() {
        return Shape { line: None, narrow: px(0.), subs: Vec::new() };
    }
    let flags = vec![shown.flags | if bold { md::BOLD } else { 0 }; shown.text.len()];
    let mut shape = shape_flags(text_system, &shown.text, flags, wrap, false, &None, look);
    // The whole cell is one sign: a click in it lands after the formula.
    shape.subs = vec![(0, shown.written, shown.text.len())];
    shape
}

fn shape_flags(
    text_system: &WindowTextSystem,
    text: &str,
    mut flags: Vec<u16>,
    wrap: Option<Pixels>,
    bold: bool,
    marked: &Option<Range<usize>>,
    look: &Look,
) -> Shape {
    if bold {
        flags.iter_mut().for_each(|f| *f |= md::BOLD);
    }
    // `\|` is written so as not to split the cell: show `|`.
    let (mut shown_text, mut styles, mut subs) = (String::with_capacity(text.len()), Vec::with_capacity(text.len()), Vec::new());
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let mut end = i + c.len_utf8();
        if c == '\\'
            && let Some(&(j, next)) = chars.peek()
        {
            chars.next();
            end = j + next.len_utf8();
            if next == '|' {
                subs.push((i, 2, 1));
                shown_text.push('|');
                styles.push(flags[j]);
                continue;
            }
            shown_text.push(c);
            shown_text.push(next);
        } else {
            shown_text.push(c);
        }
        styles.extend_from_slice(&flags[i..end]);
    }
    let is_marked = |i: usize| marked.as_ref().is_some_and(|m| m.contains(&source(&subs, i)));
    let style = RunStyle { heading: false, italic: false, mono: false, tint: None };
    let runs = text_runs(look.theme, &styles, is_marked, &style);
    let line = text_system
        .shape_text(shown_text.into(), font_size(look.theme), &runs, wrap, None)
        .ok()
        .and_then(|lines| lines.into_iter().next());
    let narrow = line.as_ref().filter(|_| wrap.is_none()).map_or(px(0.), longest);
    Shape { line, narrow, subs }
}

/// Alignment asked by a cell of the dash line: `:--`, `:-:` or `--:`.
fn align_of(dashes: &str) -> TextAlign {
    match (dashes.starts_with(':'), dashes.len() > 1 && dashes.ends_with(':')) {
        (true, true) => TextAlign::Center,
        (false, true) => TextAlign::Right,
        _ => TextAlign::Left,
    }
}

/// A formula's result as its cell shows it, in place of the formula.
struct Shown {
    text: String,
    /// Length of the formula it replaces.
    written: usize,
    flags: u16,
    right: bool,
    reason: Option<Rc<str>>,
}

/// The cell of a formula the cursor is in, as (line, cell): it shows its formula, to edit it.
fn editing(lines: &[(usize, &str)], cursor: usize) -> Option<(usize, usize)> {
    if !lines.iter().any(|(_, l)| l.contains('=')) {
        return None;
    }
    let r = lines.iter().position(|&(at, l)| (at..=at + l.len()).contains(&cursor))?;
    let (at, line) = lines[r];
    let cells = md::cells(line);
    let c = cells.iter().position(|cell| (at + cell.start..=at + cell.end).contains(&cursor))?;
    formula::is_formula(&line[cells[c].clone()]).then_some((r, c))
}

/// The result of every formula of the table, by line and cell; `None` if it has none.
fn results(lines: &[(usize, &str)], parsed: &[(Vec<Range<usize>>, Vec<usize>)], dash: Option<usize>) -> Option<Vec<Vec<Option<formula::Outcome>>>> {
    if !lines.iter().any(|(_, l)| l.contains('=')) {
        return None;
    }
    let seen: Vec<usize> = (0..lines.len()).filter(|&r| Some(r) != dash).collect();
    let grid: Vec<Vec<String>> = seen.iter().map(|&r| parsed[r].0.iter().map(|c| lines[r].1[c.clone()].to_string()).collect()).collect();
    if !formula::any(&grid) {
        return None;
    }
    let mut out = vec![Vec::new(); lines.len()];
    for (k, row) in formula::evaluate(&grid).into_iter().enumerate() {
        out[seen[k]] = row;
    }
    Some(out)
}

impl Shown {
    fn of(outcome: &formula::Outcome, written: usize) -> Shown {
        let decimal = crate::lang::tr(".", ",").chars().next().unwrap_or('.');
        match outcome {
            Ok(value @ formula::Value::Num(_)) => Shown { text: formula::show(value, decimal), written, flags: md::NUMBER, right: true, reason: None },
            Ok(value) => Shown { text: formula::show(value, decimal), written, flags: 0, right: false, reason: None },
            Err(e) => Shown { text: "#ERR".into(), written, flags: md::TAG | md::BOLD, right: true, reason: Some(e.reason().into()) },
        }
    }
}

/// Layout of a table, line by line. `lines`: (start, text) of each line
/// of the table. `None`: the dash line, when the cursor is on it; it then shows
/// as text. Otherwise it is hidden.
pub fn build(
    cache: &mut Cache,
    text_system: &WindowTextSystem,
    look: &Look,
    width: Pixels,
    lines: &[(usize, &str)],
    cursor: usize,
    marked: Option<&Range<usize>>,
) -> Tables {
    // What the layout depends on: the text, the width, the theme, the dash line
    // when the cursor is on it, and the text being composed when it touches the table.
    let touched = lines.get(1).is_some_and(|&(at, line)| (at..=at + line.len()).contains(&cursor));
    let (first, last) = (lines[0].0, lines[lines.len() - 1]);
    let composing = marked.filter(|m| m.start <= last.0 + last.1.len() && m.end >= first);
    let mut hasher = Fast::default();
    (look.style, f32::from(width).to_bits(), touched, composing.map(|m| (m.start - first.min(m.start), m.end)), lines.len(), editing(lines, cursor)).hash(&mut hasher);
    lines.iter().for_each(|(at, line)| (composing.is_some().then_some(at - first), line).hash(&mut hasher));
    let key = hasher.finish();
    if let Some(rows) = cache.tables_old.remove(&key).or_else(|| cache.tables_new.get(&key).cloned()) {
        cache.tables_new.insert(key, rows.clone());
        return rows;
    }
    let rows: Tables = Rc::new(rows(cache, text_system, look, width, lines, cursor, marked).into_iter().map(|r| r.map(Rc::new)).collect());
    cache.tables_new.insert(key, rows.clone());
    rows
}

fn rows(
    cache: &mut Cache,
    text_system: &WindowTextSystem,
    look: &Look,
    width: Pixels,
    lines: &[(usize, &str)],
    cursor: usize,
    marked: Option<&Range<usize>>,
) -> Vec<Option<TableRow>> {
    let parsed: Vec<(Vec<Range<usize>>, Vec<usize>)> = lines.iter().map(|(_, l)| (md::cells(l), md::bars(l))).collect();
    let cols = parsed.iter().map(|(cells, _)| cells.len()).max().unwrap_or(0).max(1);
    let cell = |r: usize, c: usize| parsed[r].0.get(c).cloned().unwrap_or(lines[r].1.len()..lines[r].1.len());
    let text = |r: usize, c: usize| &lines[r].1[cell(r, c)];
    // The dash line is the second, when it has only dashes and `:`.
    let dash = (lines.len() > 1)
        .then(|| (0..parsed[1].0.len()).map(|c| text(1, c).to_string()).collect::<Vec<_>>())
        .filter(|row| md::is_dashes(row))
        .map(|_| 1);
    let aligns: Vec<TextAlign> =
        (0..cols).map(|c| dash.filter(|_| c < parsed[1].0.len()).map_or(TextAlign::Left, |d| align_of(text(d, c)))).collect();
    let shown_rows: Vec<usize> = (0..lines.len()).filter(|&r| Some(r) != dash).collect();
    // Formulas show their result, except in the cell being edited. Only a table that has some is evaluated.
    let calc = results(lines, &parsed, dash);
    let edit = calc.as_ref().and_then(|_| editing(lines, cursor));
    let computed = |r: usize, c: usize| -> Option<Shown> {
        let outcome = calc.as_ref()?.get(r)?.get(c)?.as_ref().filter(|_| edit != Some((r, c)))?;
        Some(Shown::of(outcome, text(r, c).len()))
    };
    let computed_key = |shown: &Shown| format!("\u{1}{}\u{1}{}\u{1}{}", shown.flags, shown.written, shown.text);

    // Widths: all the text on one line, and the longest word, of each column.
    let mut wide = vec![MIN_WIDE; cols];
    let mut narrow = vec![NARROWEST; cols];
    let mut alone = vec![vec![px(0.); cols]; lines.len()];
    for &r in &shown_rows {
        for c in 0..cols {
            let (text, bold) = (text(r, c), r == 0);
            let one = match computed(r, c) {
                Some(shown) => cache.shape(key(&computed_key(&shown), None, bold, &None, look), || shape_computed(text_system, &shown, None, bold, look)),
                None => cache.shape(key(text, None, bold, &None, look), || shape(text_system, text, None, bold, &None, look)),
            };
            if let Some(line) = &one.line {
                alone[r][c] = line.unwrapped_layout.width;
                wide[c] = wide[c].max(f32::from(alone[r][c]));
                narrow[c] = narrow[c].max(f32::from(one.narrow));
            }
        }
    }
    let frame = (cols + 1) as f32 * f32::from(LINE) + cols as f32 * 2. * f32::from(PAD_X);
    let avail = (f32::from(width) - frame).max(0.);
    let widths: Vec<Pixels> = column_widths(&narrow, &wide, avail).into_iter().map(px).collect();
    let mut xs = vec![px(0.)];
    for w in &widths {
        xs.push(xs[xs.len() - 1] + LINE + PAD_X + *w + PAD_X);
    }
    let xs = Rc::new(xs);

    let last = shown_rows.last().copied();
    lines
        .iter()
        .enumerate()
        .map(|(r, &(at, line))| {
            let bars = parsed[r].1.clone();
            if Some(r) == dash {
                // Under the cursor, it shows as text: alignments can be changed there.
                return (!(at..=at + line.len()).contains(&cursor)).then(|| TableRow {
                    cells: Vec::new(),
                    xs: xs.clone(),
                    lines: 0,
                    header: false,
                    top: px(0.),
                    bottom: px(0.),
                    bars,
                    written: 0,
                });
            }
            let cells: Vec<Cell> = (0..cols)
                .map(|c| {
                    let range = cell(r, c);
                    let (text, bold) = (&line[range.clone()], r == 0);
                    // In the text being composed (input method), relative to the cell.
                    let here = marked
                        .filter(|m| m.start <= at + range.end && m.end >= at + range.start)
                        .map(|m| m.start.saturating_sub(at + range.start)..m.end.saturating_sub(at + range.start));
                    // A text that fits on its line is not cut: GPUI's measure and wrapping
                    // differ by a few fractions of a pixel.
                    let wrap = (alone[r][c] > widths[c]).then_some(widths[c]);
                    if let Some(shown) = computed(r, c) {
                        let shape = cache.shape(key(&computed_key(&shown), wrap, bold, &None, look), || shape_computed(text_system, &shown, wrap, bold, look));
                        let align = if shown.right { TextAlign::Right } else { aligns[c] };
                        return Cell { range, shape, align, width: widths[c], clip: false, reason: shown.reason };
                    }
                    let shape = cache.shape(key(text, wrap, bold, &here, look), || shape(text_system, text, wrap, bold, &here, look));
                    Cell { range, shape, align: aligns[c], width: widths[c], clip: false, reason: None }
                })
                .collect();
            Some(TableRow {
                lines: cells.iter().map(Cell::lines).max().unwrap_or(1),
                cells,
                xs: xs.clone(),
                header: r == 0,
                // A thicker stroke separates the header from the table body.
                top: if dash.is_some() && r == 2 { LINE * 2. } else { LINE },
                bottom: if Some(r) == last { LINE } else { px(0.) },
                bars,
                written: parsed[r].0.len(),
            })
        })
        .collect()
}

// ───────────────────────── Very big tables ─────────────────────────

/// Beyond this number of lines, a table is no longer laid out in one go: its columns
/// come from its first lines, its cells no longer wrap (what overflows
/// is cut), so every line has the same height, and only the lines near the screen
/// get their cells laid out. A 300,000-line table opens as fast
/// as a 3,000-line one.
pub const BIG: usize = 1500;
/// Lines that set the columns of a very big table.
pub const SAMPLE: usize = 300;

/// The columns of a very big table, shared by all its lines.
pub struct Geometry {
    cols: usize,
    widths: Vec<Pixels>,
    xs: Rc<Vec<Pixels>>,
    aligns: Vec<TextAlign>,
    /// Index of the dash line, when there is one.
    dash: Option<usize>,
}

impl Geometry {
    /// Same columns, same alignments, same dash line.
    pub fn same(&self, other: &Geometry) -> bool {
        self.cols == other.cols
            && self.dash == other.dash
            && self.widths == other.widths
            && *self.xs == *other.xs
            && self.aligns.iter().zip(&other.aligns).all(|(a, b)| std::mem::discriminant(a) == std::mem::discriminant(b))
    }

    /// Table width, strokes included.
    pub fn total(&self) -> Pixels {
        self.xs.last().map_or(px(0.), |x| *x + LINE)
    }

    /// Space above the text and height of table line `r`, on a single visual
    /// line; `last`: it is the last one. The dash line has no height: in a very big
    /// table, it stays hidden even under the cursor.
    pub fn metrics(&self, r: usize, last: bool, lh: Pixels) -> (Pixels, Pixels) {
        if Some(r) == self.dash {
            return (px(0.), px(0.));
        }
        let top = if self.dash.is_some() && r == 2 { LINE * 2. } else { LINE };
        let bottom = if last { LINE } else { px(0.) };
        (top + PAD_Y, top + PAD_Y + lh + PAD_Y + bottom)
    }
}

/// Sets the columns of a very big table from its first lines.
pub fn geometry(cache: &mut Cache, text_system: &WindowTextSystem, look: &Look, width: Pixels, lines: &[(usize, &str)]) -> Geometry {
    let parsed: Vec<Vec<Range<usize>>> = lines.iter().map(|(_, l)| md::cells(l)).collect();
    let cols = parsed.iter().map(Vec::len).max().unwrap_or(0).max(1);
    let text = |r: usize, c: usize| parsed[r].get(c).map_or("", |range| &lines[r].1[range.clone()]);
    let dash = (lines.len() > 1)
        .then(|| (0..parsed[1].len()).map(|c| text(1, c).to_string()).collect::<Vec<_>>())
        .filter(|row| md::is_dashes(row))
        .map(|_| 1);
    let aligns = (0..cols).map(|c| dash.filter(|_| c < parsed[1].len()).map_or(TextAlign::Left, |d| align_of(text(d, c)))).collect();
    let mut wide = vec![MIN_WIDE; cols];
    for r in (0..lines.len()).filter(|&r| Some(r) != dash) {
        for (c, w) in wide.iter_mut().enumerate() {
            let (text, bold) = (text(r, c), r == 0);
            let one = cache.shape(key(text, None, bold, &None, look), || shape(text_system, text, None, bold, &None, look));
            if let Some(line) = &one.line {
                *w = w.max(f32::from(line.unwrapped_layout.width));
            }
        }
    }
    // No wrapping: a column takes the width of its longest text, without exceeding a third
    // of the page (beyond, it is cut), plus a minimum margin.
    let frame = (cols + 1) as f32 * f32::from(LINE) + cols as f32 * 2. * f32::from(PAD_X);
    let cap = ((f32::from(width) - frame) / 3.).max(MIN_WIDE * 2.);
    let widths: Vec<Pixels> = wide.into_iter().map(|w| px(w.min(cap))).collect();
    let mut xs = vec![px(0.)];
    for w in &widths {
        xs.push(xs[xs.len() - 1] + LINE + PAD_X + *w + PAD_X);
    }
    Geometry { cols, widths, xs: Rc::new(xs), aligns, dash }
}

/// Line `r` of a very big table, its cells on a single line.
#[allow(clippy::too_many_arguments)]
pub fn big_row(
    cache: &mut Cache,
    text_system: &WindowTextSystem,
    look: &Look,
    geom: &Geometry,
    r: usize,
    last: bool,
    (at, line): (usize, &str),
    marked: Option<&Range<usize>>,
) -> TableRow {
    let (ranges, bars) = (md::cells(line), md::bars(line));
    if Some(r) == geom.dash {
        return TableRow {
            cells: Vec::new(),
            xs: geom.xs.clone(),
            lines: 0,
            header: false,
            top: px(0.),
            bottom: px(0.),
            bars,
            written: 0,
        };
    }
    let cells: Vec<Cell> = (0..geom.cols)
        .map(|c| {
            let range = ranges.get(c).cloned().unwrap_or(line.len()..line.len());
            let (text, bold) = (&line[range.clone()], r == 0);
            let here = marked
                .filter(|m| m.start <= at + range.end && m.end >= at + range.start)
                .map(|m| m.start.saturating_sub(at + range.start)..m.end.saturating_sub(at + range.start));
            let shape = cache.shape(key(text, None, bold, &here, look), || shape(text_system, text, None, bold, &here, look));
            let clip = shape.line.as_ref().is_some_and(|l| l.unwrapped_layout.width > geom.widths[c]);
            Cell { range, shape, align: geom.aligns[c], width: geom.widths[c], clip, reason: None }
        })
        .collect();
    TableRow {
        lines: 1,
        cells,
        xs: geom.xs.clone(),
        header: r == 0,
        top: if geom.dash.is_some() && r == 2 { LINE * 2. } else { LINE },
        bottom: if last { LINE } else { px(0.) },
        bars,
        written: ranges.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_where_wrapping_can() {
        assert_eq!(units("un deux"), [0..3, 3..7]);
        assert_eq!(units("a  b"), [0..3, 3..4]);
        // A sign or an ideogram can start a line; a word with its hyphens and apostrophes cannot.
        assert_eq!(units("x(y)"), [0..1, 1..3, 3..4]);
        assert_eq!(units("日本"), [0..3, 3..6]);
        assert_eq!(units("l'état-major"), vec![0..13]);
        assert_eq!(units(""), vec![0..0]);
    }

    #[test]
    fn dashes_give_alignment() {
        assert!(matches!(align_of("---"), TextAlign::Left));
        assert!(matches!(align_of(":--"), TextAlign::Left));
        assert!(matches!(align_of("--:"), TextAlign::Right));
        assert!(matches!(align_of(":-:"), TextAlign::Center));
        assert!(matches!(align_of(":"), TextAlign::Left));
    }

    #[test]
    fn substitutions_map_both_ways() {
        // `a\|b` shows as `a|b`: the `|` sign is worth two written bytes.
        let subs = [(1, 2, 1)];
        assert_eq!((0..=4).map(|i| shown(&subs, i)).collect::<Vec<_>>(), [0, 1, 2, 2, 3]);
        assert_eq!((0..=3).map(|i| source(&subs, i)).collect::<Vec<_>>(), [0, 1, 3, 4]);
    }

    /// Hundreds of random tables: columns never exceed what they
    /// ask for, fit in the page when possible, and do not go below their floor.
    #[test]
    fn columns_fit_the_page() {
        let mut seed = 0x2545F4914F6CDD1Du64;
        let mut roll = move |n: u64| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % n) as f32
        };
        for case in 0..800 {
            let cols = 1 + roll(12) as usize;
            let wide: Vec<f32> = (0..cols).map(|_| MIN_WIDE + roll(900)).collect();
            let narrow: Vec<f32> = wide.iter().map(|w| (NARROWEST + roll(200)).min(*w)).collect();
            let avail = roll(1200);
            let widths = column_widths(&narrow, &wide, avail);
            assert_eq!(widths.len(), cols);
            let total: f32 = widths.iter().sum();
            let floor = cols as f32 * NARROWEST;
            assert!(total <= avail.max(floor) + 0.01, "case {case}: {total} for {avail}");
            for ((w, n), x) in widths.iter().zip(&narrow).zip(&wide) {
                assert!(*w <= x + 0.01, "case {case}: {w} > {x}");
                assert!(*w >= n.min(NARROWEST).min(*x) - 0.01, "case {case}: {w} < {n}");
            }
            // Everything fits on one line if the page allows it.
            if wide.iter().sum::<f32>() <= avail {
                assert_eq!(widths, wide, "case {case}");
            }
            // If the longest words fit, none is cut.
            if narrow.iter().sum::<f32>() <= avail {
                assert!(widths.iter().zip(&narrow).all(|(w, n)| w + 0.01 >= *n), "case {case}");
            }
        }
    }
}
