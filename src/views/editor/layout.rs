//! Layout: lines shaped and placed, redone only where the text changed (`Damage`, `plan`).

use super::*;
use crate::ui::assets::needs_system_font;

/// What the layout depends on, apart from the text: if nothing changed, it is kept.
#[derive(PartialEq)]
pub(super) struct LayoutKey {
    pub(super) version: u64,
    pub(super) width: u32,
    pub(super) size: u32,
    pub(super) cursor: usize,
    pub(super) marked: Option<Range<usize>>,
}

/// What changed in the text since the last layout: its first `prefix` bytes
/// and its last `suffix` stayed as they were.
#[derive(Clone, Copy)]
pub(super) struct Damage {
    pub(super) old_len: usize,
    pub(super) prefix: usize,
    pub(super) suffix: usize,
}

/// The last layout, to reuse what has not changed.
pub(super) struct Laid {
    pub(super) epoch: u64,
    pub(super) width: u32,
    pub(super) room: u32,
    pub(super) size: u32,
    pub(super) len: usize,
    pub(super) cursor: usize,
    pub(super) fences: usize,
    pub(super) dollars: usize,
    /// Length of the front matter.
    pub(super) meta: usize,
}

/// A zone of the text to lay out again: the lines of the old layout `old` (indices),
/// the text replacing them, from `start` to `stop` (bytes, `stop` included: end of the last line).
pub(super) struct Region {
    pub(super) old: Range<usize>,
    pub(super) start: usize,
    pub(super) stop: usize,
    /// In a very big table: its columns, the rank of the first line in the table and the byte where
    /// the table starts.
    pub(super) big: Option<(Rc<grid::Geometry>, usize, usize)>,
    /// The zone touches the first lines, which set the columns: they must not have changed.
    pub(super) check: bool,
}

/// What is kept from the previous layout, and what is redone.
pub(super) struct Plan {
    pub(super) regions: Vec<Region>,
    /// Counts of fence lines and `$$`, unchanged.
    pub(super) fences: usize,
    pub(super) dollars: usize,
    /// Reused lines following `after` (bytes of the old text) shift by `delta`.
    pub(super) after: usize,
    pub(super) delta: isize,
}

/// Reuses `count` lines of the old layout, placed from `y`, their text shifted by `delta`.
pub(super) fn reuse(old: &mut std::vec::IntoIter<Row>, count: usize, delta: isize, y: &mut Pixels, rows: &mut Vec<Row>) {
    let mut by = None;
    for mut row in old.by_ref().take(count) {
        let by = *by.get_or_insert(*y - row.y);
        row.start = (row.start as isize + delta) as usize;
        row.y += by;
        *y = row.y + row.height;
        rows.push(row);
    }
}

/// A very big table: its lines only get their cells near the screen.
pub(super) struct BigBlock {
    /// Indices of its lines.
    pub(super) rows: Range<usize>,
    pub(super) geom: Rc<grid::Geometry>,
    /// Parts of its lines (indices in the table) that have their cells.
    pub(super) mat: Vec<Range<usize>>,
    /// Its lines come from another layout: which ones have their cells is no longer known.
    pub(super) rescan: bool,
}

/// A table of the note.
/// Where a table's lines are on screen, for its handles and what was grabbed in it.
pub(super) struct Geo {
    /// Start of the table in the text.
    pub(super) start: usize,
    pub(super) frame: Bounds<Pixels>,
    /// X of the vertical lines (one more than columns), y of the horizontal ones
    /// (one more than shown rows: the header, then the body).
    pub(super) xs: Vec<Pixels>,
    pub(super) ys: Vec<Pixels>,
    /// Its commented rows, columns and cells.
    pub(super) notes: Notes,
}

/// A table handle: a column, a row, the corner (the whole table), or the "+" of a line.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Part {
    Col(usize),
    Row(usize),
    Corner,
    /// The line before column or row `n`: a click there inserts.
    JoinCol(usize),
    JoinRow(usize),
}

impl Geo {
    pub(super) fn cols(&self) -> usize {
        self.xs.len() - 1
    }

    pub(super) fn tall(&self) -> usize {
        self.ys.len() - 1
    }

    /// The column and the row above or beside which `at` is, up to `GRIP_REACH` from the table.
    pub(super) fn near(&self, at: Point<Pixels>) -> Near {
        let f = self.frame;
        // The last line counts for the last column (or row): its "+" shows there.
        let along = |lines: &[Pixels], v: Pixels| {
            (0..lines.len() - 1).find(|&i| v >= lines[i] && (v < lines[i + 1] || (i + 2 == lines.len() && v <= lines[i + 1])))
        };
        let (above, aside) = (at.y >= f.top() - GRIP_REACH && at.y <= f.bottom(), at.x >= f.left() - GRIP_REACH && at.x <= f.right());
        Near {
            col: along(&self.xs, at.x).filter(|_| above && at.x >= f.left() && at.x <= f.right()),
            row: along(&self.ys, at.y).filter(|_| aside && at.y >= f.top() && at.y <= f.bottom()),
            corner: at.x >= f.left() - GRIP_REACH && at.x <= f.left() + px(16.) && at.y >= f.top() - GRIP_REACH && at.y <= f.top() + px(16.),
        }
    }

    /// The handles and their zone, in the order used to find the one that gets a click.
    pub(super) fn parts(&self) -> Vec<(Part, Bounds<Pixels>)> {
        let f = self.frame;
        let shown = |x: Pixels| x >= f.left() - px(1.) && x <= f.right() + px(1.);
        // The corner first: it wins over the "+" of the first line, which brushes it.
        let mut parts = vec![(Part::Corner, Bounds::new(f.origin - point(GRIP + GRIP_GAP, GRIP + GRIP_GAP), size(GRIP, GRIP)))];
        for (b, &x) in self.xs.iter().enumerate().filter(|(_, x)| shown(**x)) {
            parts.push((Part::JoinCol(b), Bounds::new(point(x - px(6.), f.top() - px(18.)), size(px(12.), px(16.)))));
        }
        for (b, &y) in self.ys.iter().enumerate().skip(1) {
            parts.push((Part::JoinRow(b), Bounds::new(point(f.left() - px(18.), y - px(6.)), size(px(16.), px(12.)))));
        }
        // A badge in the middle of its column (or row), smaller if room is short.
        for c in 0..self.cols() {
            let (from, to) = (self.xs[c].max(f.left()), self.xs[c + 1].min(f.right()));
            let long = GRIP_LONG.min(to - from - px(12.));
            if long >= px(10.) {
                let mid = (from + to) / 2.;
                parts.push((Part::Col(c), Bounds::new(point(mid - long / 2., f.top() - GRIP - GRIP_GAP), size(long, GRIP))));
            }
        }
        for r in 0..self.tall() {
            let (from, to) = (self.ys[r], self.ys[r + 1]);
            let long = GRIP_LONG.min(to - from - px(8.));
            if long >= px(10.) {
                let mid = (from + to) / 2.;
                parts.push((Part::Row(r), Bounds::new(point(f.left() - GRIP - GRIP_GAP, mid - long / 2.), size(GRIP, long))));
            }
        }
        parts
    }

    /// Handle `part` shows: the pointer is on its column or row, or it holds what was grabbed.
    pub(super) fn visible(&self, part: Part, near: Near, pick: Option<&Pick>) -> bool {
        let (all_rows, all_cols) = pick.map_or((false, false), |p| (p.rows.len() == self.tall(), p.cols.len() == self.cols()));
        let held = |hold: bool, n: usize, range: Option<&Range<usize>>| hold && range.is_some_and(|r| r.contains(&n));
        match part {
            Part::Col(c) => near.col == Some(c) || held(all_rows, c, pick.map(|p| &p.cols)),
            Part::Row(r) => near.row == Some(r) || held(all_cols, r, pick.map(|p| &p.rows)),
            Part::Corner => near.corner || (all_rows && all_cols),
            // A "+" only shows under the pointer: its zone, near the handles, is enough.
            Part::JoinCol(_) | Part::JoinRow(_) => near.col.is_some() || near.row.is_some() || near.corner,
        }
    }

    /// The line closest to `at`, among those where a column (or a row) can land.
    pub(super) fn boundary(&self, part: Part, at: Point<Pixels>) -> usize {
        let (lines, at, first) = match part {
            Part::Col(_) => (&self.xs, at.x, 0),
            _ => (&self.ys, at.y, 1),
        };
        (first..lines.len()).min_by_key(|&b| (f32::from((lines[b] - at).abs()) * 100.) as i64).unwrap_or(first)
    }

    /// The `rows` × `cols` cells, in pixels.
    pub(super) fn cells(&self, rows: &Range<usize>, cols: &Range<usize>) -> Bounds<Pixels> {
        let (x0, x1) = (self.xs[cols.start.min(self.cols())], self.xs[cols.end.min(self.cols())]);
        let (y0, y1) = (self.ys[rows.start.min(self.tall())], self.ys[rows.end.min(self.tall())]);
        Bounds::new(point(x0, y0), size(x1 - x0, y1 - y0))
    }
}

/// A laid out logical line (it may take several visual lines).
pub(super) struct Row {
    pub(super) start: usize,
    pub(super) len: usize,
    pub(super) y: Pixels,
    pub(super) pad: Pixels,
    pub(super) lh: Pixels,
    pub(super) height: Pixels,
    pub(super) kind: Kind,
    /// Its styled text, shared with the line cache.
    pub(super) shaped: Rc<Shaped>,
    /// Picture the line points to, or figure (diagram, formula) it
    /// ends, drawn below, and its size on screen.
    pub(super) image: Option<(Arc<RenderImage>, Size<Pixels>)>,
    /// What the viewer shows when that image is enlarged (its button appears on hover).
    pub(super) enlarge: Option<Source>,
    /// The line opens a code block: it carries the copy button.
    pub(super) opens: bool,
    /// Room left under the last line of a table, or inside the fill of a code block or callout.
    pub(super) gap: Pixels,
    /// The line starts / ends a code block, a callout or a quote: its shape is rounded there.
    pub(super) head: bool,
    pub(super) foot: bool,
    /// Color of the `> [!TYPE]` callout the line is part of.
    pub(super) tint: Option<Hsla>,
    /// Table line wider than the page: how far its text is shifted to the left.
    pub(super) dx: Pixels,
    /// Table line shown as a grid: its cells, which wrap.
    pub(super) grid: Option<Rc<TableRow>>,
    /// The section of another note this line embeds, framed under it.
    pub(super) embed: Option<Rc<EmbedRow>>,
    /// The figure or the picture of the line cannot be read: what is said under it, in place of the picture.
    pub(super) broken: Option<&'static str>,
}

impl Row {
    /// Position on screen, relative to the left edge of the page.
    pub(super) fn pos(&self, i: usize) -> Point<Pixels> {
        let p = match &self.grid {
            Some(grid) => grid.pos(i.min(self.len), self.lh),
            None => match &self.shaped.lead {
                Some(lead) if i < lead.len => lead.line.position_for_index(i, self.lh).unwrap_or_default(),
                lead => {
                    let (skip, at) = lead.as_ref().map_or((0, px(0.)), |l| (l.len, l.width));
                    let p = self.shaped.line.as_ref().and_then(|l| l.position_for_index(self.shown(i.min(self.len)) - skip, self.lh));
                    p.map_or_else(Default::default, |p| point(p.x + at, p.y))
                }
            },
        };
        point(p.x - self.dx, p.y)
    }

    /// Text byte → displayed text byte.
    pub(super) fn shown(&self, i: usize) -> usize {
        grid::shown(&self.shaped.subs, i)
    }

    /// Displayed text byte → text byte.
    pub(super) fn source(&self, i: usize) -> usize {
        grid::source(&self.shaped.subs, i)
    }

    pub(super) fn image_height(&self) -> Pixels {
        match (&self.image, self.broken) {
            (Some((_, s)), _) => s.height.max(COPY_SIZE) + IMAGE_GAP,
            (None, Some(_)) => BROKEN + IMAGE_GAP,
            _ => px(0.),
        }
    }

    /// Room taken under the line by its picture or its embedded section.
    pub(super) fn below(&self) -> Pixels {
        self.image_height() + self.embed.as_ref().map_or(px(0.), |e| e.height())
    }

    /// Top of the picture's band: at least as tall as the enlarge button, so that a tiny
    /// picture's button never lands on the next line.
    pub(super) fn image_top(&self) -> Pixels {
        self.y + self.height - self.gap - self.image_height() + IMAGE_GAP
    }

    pub(super) fn index_at(&self, p: Point<Pixels>) -> usize {
        let p = point(p.x + self.dx, p.y);
        if let Some(grid) = &self.grid {
            return grid.index_at(p, self.lh);
        }
        if let Some(lead) = &self.shaped.lead {
            // On the marker (first visual line, left of the title), or in the title shifted by it.
            if p.y < self.lh && p.x < lead.width {
                return lead.line.closest_index_for_position(p, self.lh).unwrap_or_else(|i| i).min(lead.len);
            }
            let p = point(p.x - lead.width, p.y);
            return match self.shaped.line.as_ref().map(|l| l.closest_index_for_position(p, self.lh)) {
                Some(Ok(i) | Err(i)) => self.source(i + lead.len),
                None => lead.len,
            };
        }
        match &self.shaped.line {
            Some(l) => match l.closest_index_for_position(p, self.lh) {
                Ok(i) | Err(i) => self.source(i),
            },
            None => 0,
        }
    }

    pub(super) fn visual_rows(&self) -> i32 {
        match &self.grid {
            Some(grid) => grid.lines,
            None => (f32::from(self.height - self.pad - self.below() - self.gap) / f32::from(self.lh)).round() as i32,
        }
    }

    /// Visual lines of what contains byte `i`: the line, or a table cell.
    pub(super) fn lines_at(&self, i: usize) -> i32 {
        self.grid.as_ref().map_or_else(|| self.visual_rows(), |grid| grid.lines_at(i))
    }

    /// A table's dash line, which is not seen.
    pub(super) fn hidden(&self) -> bool {
        self.grid.as_ref().is_some_and(|g| g.hidden())
    }

    /// Width of what is drawn.
    pub(super) fn content_width(&self) -> Pixels {
        match &self.grid {
            Some(grid) => grid.total(),
            None => self.shaped.line.as_ref().map_or(px(0.), |l| l.width()) + self.shaped.lead.as_ref().map_or(px(0.), |l| l.width),
        }
    }
}

/// Fast hash (FxHash, mixed at the end): every line of the note is hashed on every
/// frame, and SipHash, the standard library's, spent a third of the time there. The
/// keys only come from the note: no need to resist whoever would choose them.
#[derive(Default)]
pub struct Fast(u64);

impl Fast {
    pub(super) fn add(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
}

impl Hasher for Fast {
    fn write(&mut self, bytes: &[u8]) {
        let (words, rest) = bytes.as_chunks::<8>();
        for word in words {
            self.add(u64::from_le_bytes(*word));
        }
        let mut last = [0u8; 8];
        last[..rest.len()].copy_from_slice(rest);
        self.add(u64::from_le_bytes(last) ^ (rest.len() as u64) << 56);
    }

    fn write_u64(&mut self, n: u64) {
        self.add(n);
    }

    fn write_usize(&mut self, n: usize) {
        self.add(n as u64);
    }

    fn finish(&self) -> u64 {
        let mut h = self.0;
        h ^= h >> 33;
        h = h.wrapping_mul(0xff51_afd7_ed55_8ccd);
        h ^ (h >> 33)
    }
}

/// The styling of the line with hash `id`, reused from the kept ones, or made by `make`.
pub(super) fn cached_shape(
    map: &mut FastMap<u64, (Rc<Shaped>, u32)>,
    id: u64,
    frame: u32,
    make: impl FnOnce() -> Rc<Shaped>,
) -> Rc<Shaped> {
    if let Some((shape, used)) = map.get_mut(&id) {
        *used = frame;
        return shape.clone();
    }
    let shape = make();
    map.insert(id, (shape.clone(), frame));
    shape
}

pub type FastMap<K, V> = HashMap<K, V, BuildHasherDefault<Fast>>;

/// The marker of a heading, drawn before its title.
pub(super) struct Lead {
    pub(super) line: WrappedLine,
    /// Bytes of the line it covers, and the room it takes (the title starts after it).
    pub(super) len: usize,
    pub(super) width: Pixels,
    /// How far down it is drawn so that its baseline is the title's.
    pub(super) dy: Pixels,
}

/// What a line displays, as long as neither it nor its context change.
pub(super) struct Shaped {
    pub(super) line: Option<WrappedLine>,
    /// A heading's `#` marker: shaped on its own at the body size, since a line has a single font size.
    pub(super) lead: Option<Lead>,
    /// Signs displayed in place of their ASCII spelling: (start byte,
    /// length in the text, displayed length).
    pub(super) subs: Vec<(usize, usize, usize)>,
    /// Formula on the line: its source, and whether it is display (block) math.
    pub(super) formula: Option<(String, bool)>,
    /// Path of the picture the line points to.
    pub(super) picture: Option<String>,
}

/// Figure ready to draw (`None`: invalid source) and its size.
pub(super) type Drawing = Option<(Arc<Image>, Size<Pixels>)>;

/// Drawn figures, by key; each with the key of its source alone (without the theme),
/// to show the old version while the new one is drawn.
pub(super) type Figures = FastMap<u64, (u64, Drawing)>;

/// The figure with key `key`, reused from the previous frame or drawn by `make`; if `lazy`,
/// only reused. As long as `make` has nothing (drawing in a background task), the previous version
/// of the same source (`family`), if there is one.
pub(super) fn cached(old: &mut Figures, kept: &mut Figures, (key, family): (u64, u64), lazy: bool, make: impl FnOnce() -> Option<Drawing>) -> Drawing {
    if let Some(entry) = old.remove(&key).or_else(|| kept.get(&key).cloned()) {
        kept.insert(key, entry.clone());
        return entry.1;
    }
    if lazy {
        return None;
    }
    if let Some(drawing) = make() {
        kept.insert(key, (family, drawing.clone()));
        return drawing;
    }
    let stale = *old.iter().find(|(_, (f, d))| *f == family && d.is_some())?.0;
    let entry = old.remove(&stale)?;
    kept.insert(stale, entry.clone());
    entry.1
}

/// The figure with this key was drawn, and it is invalid (not one that is still being drawn, which has no entry).
fn failed(kept: &Figures, key: u64) -> bool {
    kept.get(&key).is_some_and(|(_, drawing)| drawing.is_none())
}

/// What, in a line's style, comes from its nature rather than its text.
pub struct RunStyle {
    pub heading: bool,
    /// The text is `soft` (a level 6 heading).
    pub soft: bool,
    pub italic: bool,
    pub mono: bool,
    /// Color of the callout the line is part of.
    pub tint: Option<Hsla>,
}

/// The styles of the line `text`, `flags` (one flag word per byte), in one piece
/// as long as nothing changes; `marked`: the byte is in the text being composed.
pub fn text_runs(t: &Theme, text: &str, flags: &[u16], marked: impl Fn(usize) -> bool, style: &RunStyle) -> Vec<TextRun> {
    let mut runs: Vec<TextRun> = Vec::new();
    let mut i = 0;
    while i < flags.len() {
        let (f, m) = (flags[i], marked(i));
        let mut j = i + 1;
        while j < flags.len() && flags[j] == f && marked(j) == m {
            j += 1;
        }
        let color = if f & md::QUIET != 0 {
            t.soft
        } else if f & md::DIM != 0 {
            t.dim
        } else if f & md::STRING != 0 {
            t.string
        } else if f & md::NUMBER != 0 {
            t.number
        } else if f & md::MARK != 0 && let Some(tint) = style.tint {
            t.callout_label(tint)
        } else if f & (md::LINK | md::TAG | md::MARK | md::KEYWORD) != 0 {
            t.accent
        } else if style.soft {
            t.soft
        } else {
            t.text
        };
        let (code, slanted) = (style.mono || f & md::CODE != 0, f & md::ITALIC != 0 || style.italic);
        let underline = (m || f & (md::LINK | md::UNDER) != 0).then_some(UnderlineStyle {
            thickness: px(1.),
            color: Some(if f & md::LINK != 0 { t.accent } else { color }.opacity(if f & md::UNDER != 0 { 1. } else { 0.5 })),
            wavy: false,
        });
        // `upright`: the characters the app's fonts lack, in the sans, not italic.
        let run = |len: usize, upright: bool| {
            let (code, slanted) = (code && !upright, slanted && !upright);
            TextRun {
                len,
                font: Font {
                    weight: if style.heading || f & md::BOLD != 0 { FontWeight::BOLD } else { FontWeight::NORMAL },
                    style: if slanted { FontStyle::Italic } else { FontStyle::Normal },
                    ..font(if code { mono() } else { sans() })
                },
                color,
                background_color: (f & md::CODE != 0).then_some(t.code_bg),
                // A single underline for the whole link, brackets included: gpui 0.2 leaves
                // a bit of underline in the margin if the underline changes style
                // right at a wrap.
                underline,
                strikethrough: (f & md::STRIKE != 0).then_some(StrikethroughStyle { thickness: px(1.), color: Some(color) }),
            }
        };
        if (code || slanted) && text.get(i..j).is_some_and(|s| s.chars().any(needs_system_font)) {
            // Cut into pieces of the same kind: with or without such characters.
            let mut piece = (i, needs_system_font(text[i..].chars().next().unwrap_or(' ')));
            for (k, c) in text[i..j].char_indices() {
                let (k, upright) = (i + k, needs_system_font(c));
                if upright != piece.1 {
                    runs.push(run(k - piece.0, piece.1));
                    piece = (k, upright);
                }
            }
            runs.push(run(j - piece.0, piece.1));
        } else {
            runs.push(run(j - i, false));
        }
        i = j;
    }
    runs
}

impl Editor {
    /// Full layouts and reused layouts, since the start.
    #[cfg(test)]
    pub fn layouts(&self) -> (usize, usize, usize) {
        self.counts
    }

    /// The zones to lay out again, when the rest of the last layout can be reused:
    /// the touched text, the line where the cursor was and the one where it is, widened to blocks that
    /// must stay whole (table, quote). `None`: redo everything.
    pub(super) fn plan(&self, width: Pixels, room: Pixels, cursor: usize) -> Option<Plan> {
        let laid = self.laid.as_ref()?;
        let (rows, len) = (&self.rows, self.content.len());
        if laid.epoch != self.epoch
            || laid.width != f32::from(width).to_bits()
            || laid.room != f32::from(room).to_bits()
            || laid.size != self.theme.size.to_bits()
            || laid.dollars > 0
            || self.marked.is_some()
            || self.loading
            || rows.is_empty()
            || self.force_full
        {
            return None;
        }
        // ponytail: the front matter changes the meaning of all its lines as soon as it opens or
        // closes; a keystroke inside it, or one that moves its end, therefore redoes the whole layout.
        // Treat it as a separate zone if long notes with front matter make you wait.
        let meta = md::front_matter(&self.content);
        if meta != laid.meta || self.dmg.is_some_and(|d| d.prefix < meta) {
            return None;
        }
        // Touched text, in bytes of the old layout (`p..=q`), and its shift.
        let (p, q, delta) = match self.dmg {
            Some(d) => {
                let (prefix, suffix) = (d.prefix.min(laid.len), d.suffix.min(laid.len));
                if d.old_len != laid.len || prefix + suffix > laid.len || prefix + suffix > len {
                    return None;
                }
                (prefix, laid.len - suffix, len as isize - laid.len as isize)
            }
            None if len == laid.len => (0, 0, 0),
            None => return None,
        };
        let n = rows.len();
        let at = |pos: usize| rows.partition_point(|r| r.start <= pos).saturating_sub(1);
        let mut dirty: Vec<Range<usize>> = Vec::new();
        if self.dmg.is_some() {
            let lo = rows.partition_point(|r| r.start + r.len < p);
            let hi = rows.partition_point(|r| r.start <= q + 1);
            dirty.push(lo.min(n - 1)..hi.max(lo + 1).min(n));
        }
        // The cursor before and the one after: their line does not display the same.
        let now = if cursor <= p {
            cursor
        } else if (cursor as isize - delta) as usize >= q && cursor as isize - delta >= 0 {
            (cursor as isize - delta) as usize
        } else {
            p
        };
        for c in [laid.cursor.min(laid.len), now.min(laid.len)] {
            dirty.push(at(c)..at(c) + 1);
        }
        // Blocks not to cut: a table holds in one piece (except the inside of a very big one),
        // a quote does not lose its callout's tint.
        let block_of = |i: usize| -> Range<usize> {
            if let Some(b) = self.big.iter().find(|b| b.rows.contains(&i)) {
                return b.rows.clone();
            }
            let (mut a, mut z) = (i, i + 1);
            while a > 0 && rows[a - 1].kind == Kind::Table {
                a -= 1;
            }
            while z < n && rows[z].kind == Kind::Table {
                z += 1;
            }
            a..z
        };
        let big_of = |i: usize| self.big.iter().find(|b| b.rows.contains(&i));
        // Bytes of a zone's new text: the start only moves if it is after the touched text; the
        // end, if it is at the end of the touched text or after.
        let touched = self.dmg.is_some();
        let bounds = |r: &Range<usize>| {
            let (first, last) = (&rows[r.start], &rows[r.end - 1]);
            let start = if first.start <= p { first.start } else { (first.start as isize + delta) as usize };
            let end = last.start + last.len;
            let stop = if touched && end >= q { (end as isize + delta) as usize } else { end };
            (start, stop)
        };
        loop {
            dirty.sort_by_key(|r| r.start);
            let mut merged: Vec<Range<usize>> = Vec::new();
            for r in dirty.drain(..) {
                match merged.last_mut() {
                    Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
                    _ => merged.push(r),
                }
            }
            let mut grown = false;
            for r in merged.iter_mut() {
                let before = r.clone();
                let whole = |r: &mut Range<usize>, block: Range<usize>| {
                    r.start = r.start.min(block.start);
                    r.end = r.end.max(block.end);
                };
                for i in before.clone() {
                    match rows[i].kind {
                        Kind::Code | Kind::Fence => return None,
                        Kind::Table => match big_of(i) {
                            // Inside a very big table, a line does not depend on the others.
                            Some(b) if i + 1 < b.rows.end => {}
                            _ => whole(r, block_of(i)),
                        },
                        _ => {}
                    }
                }
                // A very big table is cut into independent lines, except its first ones.
                let inside = |i: usize, j: usize| big_of(i).is_some_and(|b| b.rows.contains(&j) && i + 1 < b.rows.end);
                if r.start > 0 && rows[r.start - 1].kind == Kind::Table && !inside(r.start, r.start - 1) {
                    whole(r, block_of(r.start - 1));
                }
                // The new text, too, can stick to a neighboring table or quote.
                let (start, stop) = bounds(r);
                if start > stop || stop > len {
                    return None;
                }
                let text = &self.content[start..stop];
                let last = text.rsplit('\n').next().unwrap_or("");
                if r.end < n && rows[r.end].kind == Kind::Table && md::is_table_line(last) && !inside(r.end - 1, r.end) {
                    whole(r, block_of(r.end));
                }
                if r.start > 0 && rows[r.start - 1].kind == Kind::Quote {
                    r.start -= 1;
                }
                if r.end < n && rows[r.end].kind == Kind::Quote {
                    r.end += 1;
                }
                while r.start > 0 && rows[r.start].kind == Kind::Quote && rows[r.start - 1].kind == Kind::Quote {
                    r.start -= 1;
                }
                while r.end < n && rows[r.end - 1].kind == Kind::Quote && rows[r.end].kind == Kind::Quote {
                    r.end += 1;
                }
                // The line after a table gives it its margin: it is redone with it, whether it always was
                // one or becomes one.
                if r.end < n && (rows[r.end - 1].kind == Kind::Table || md::is_table_line(last)) && rows[r.end].kind != Kind::Table {
                    r.end += 1;
                }
                grown |= *r != before;
            }
            dirty = merged;
            if !grown {
                break;
            }
        }
        let mut regions = Vec::new();
        for r in dirty {
            let (start, stop) = bounds(&r);
            // Inside a very big table.
            let block = big_of(r.start).filter(|b| r.end < b.rows.end);
            let big = block.map(|b| (b.geom.clone(), r.start - b.rows.start, rows[b.rows.start].start));
            let check = block.is_some_and(|b| r.start < b.rows.start + grid::SAMPLE);
            // The new text must change nothing in the context of the reused lines: no fence, no `$$`,
            // and, in a very big table, only table lines.
            if start > stop || stop > len || (stop < len && self.content.as_bytes()[stop] != b'\n') {
                return None;
            }
            let mut lines = 0;
            for l in self.content[start..stop].split('\n') {
                lines += 1;
                if md::is_fence(l) || l.trim() == "$$" || (big.is_some() && !md::is_table_line(l)) {
                    return None;
                }
            }
            // A table going back under the threshold is laid out like a small one: redo everything.
            if let Some(b) = block.filter(|_| big.is_some()) && b.rows.len() + lines <= r.len() + grid::BIG {
                return None;
            }
            regions.push(Region { old: r, start, stop, big, check });
        }
        Some(Plan { regions, fences: laid.fences, dollars: laid.dollars, after: if touched { q } else { usize::MAX }, delta })
    }

    /// What describes each laid out line, to compare two layouts.
    #[cfg(test)]
    #[allow(clippy::type_complexity)]
    pub(super) fn signature(&self) -> Vec<(usize, usize, i64, i64, i64, bool, Kind, i64, (bool, bool))> {
        let px = |v: Pixels| (f32::from(v) * 100.).round() as i64;
        self.rows.iter().map(|r| (r.start, r.len, px(r.y), px(r.height), px(r.pad), r.opens, r.kind, px(r.gap), (r.head, r.foot))).collect()
    }

    /// Each frame: the heavy layout is only redone if the text, the width, the theme
    /// or the cursor changed; otherwise, only what depends on scrolling is recomputed.
    pub(super) fn layout(&mut self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        // A note shown in an embed changed on disk: every line is laid out again.
        if self.embeds.poll() {
            self.epoch += 1;
            self.laid_out = None;
        }
        let key = LayoutKey {
            version: self.version,
            width: f32::from(bounds.size.width).to_bits(),
            size: self.theme.size.to_bits(),
            cursor: self.cursor(),
            marked: self.marked.clone(),
        };
        if self.laid_out.as_ref() != Some(&key) {
            self.laid_out = None;
            self.relayout(bounds, window, cx);
            // Tests redo the whole layout on the side, to find the reused one in it.
            #[cfg(test)]
            if self.compare && !self.force_full {
                let reused = self.signature();
                // Every table of more than `grid::BIG` lines is recorded as very big, and only those.
                let mut i = 0;
                while i < self.rows.len() {
                    let n = self.rows[i..].iter().take_while(|r| r.kind == Kind::Table).count();
                    if n > 0 {
                        let registered = self.big.iter().any(|b| b.rows == (i..i + n));
                        assert_eq!(registered, n > grid::BIG, "table of {n} lines at line {i}, register {:?}", self.big.iter().map(|b| b.rows.clone()).collect::<Vec<_>>());
                    }
                    i += n.max(1);
                }
                let (laid, big, counts) = (self.laid.take(), std::mem::take(&mut self.big), self.counts);
                self.force_full = true;
                self.relayout(bounds, window, cx);
                self.force_full = false;
                self.counts = counts;
                assert_eq!(reused.len(), self.rows.len(), "number of lines");
                // Positions add up in floats: the order of additions changes the last digit.
                let near = |a: i64, b: i64| (a - b).abs() <= 20;
                for (i, (a, b)) in reused.iter().zip(self.signature()).enumerate() {
                    assert!(
                        a.0 == b.0 && a.1 == b.1 && near(a.2, b.2) && near(a.3, b.3) && near(a.4, b.4) && a.5 == b.5 && a.6 == b.6 && near(a.7, b.7) && a.8 == b.8,
                        "line {i}: {a:?} against {b:?}"
                    );
                }
                let _ = (laid, big);
            }
            // A picture still loading changes its line's height: we will come back to it.
            self.laid_out = (!self.loading).then_some(key);
        }
        self.place(bounds, window, cx);
    }

    // ponytail: the layout reuses the lines that did not move (`plan`) and only redoes the touched
    // zones. Everything is redone when the context changes (theme, another note, code block or `$$` touched, text being
    // composed): typing in a note containing a code block or formulas stays proportional to
    // its size. Reuse the code blocks' context, as for quotes, if it gets noticed.
    pub(super) fn relayout(&mut self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        let t = self.theme;
        let cursor = self.cursor();
        let width = (bounds.size.width - SIDE * 2.).min(t.page.measure(t.size)).max(px(40.));
        let room = table_room(t.wide, bounds.size.width, width);
        let text_system = window.text_system().clone();
        // What can be reused from the previous layout, when only a few lines changed.
        // The first lines of a very big table set its columns: if they move, redo everything.
        let plan = self.plan(width, room, cursor).filter(|plan| {
            plan.regions.iter().filter(|r| r.check).all(|r| {
                let Some((geom, _, start)) = &r.big else { return true };
                let lines: Vec<(usize, &str)> = self.content[*start..]
                    .split('\n')
                    .scan(*start, |at, l| {
                        let line = (*at, l);
                        *at += l.len() + 1;
                        Some(line)
                    })
                    .take(grid::SAMPLE)
                    .collect();
                let look = grid::Look { theme: &t, style: 0 };
                lines.iter().all(|(_, l)| md::is_table_line(l))
                    && grid::geometry(&mut grid::Cache::default(), &text_system, &look, room, &lines).same(geom)
            })
        });
        // A ``` without a closing fence opens no block: otherwise each ``` typed would
        // switch the whole rest of the note to code, hence restyle it. If
        // there is one too many, it is the one being typed (the cursor is on the line of the
        // last change), otherwise the last: a cursor merely resting on a fence of a closed
        // block must not shift the pairing of the others.
        let (mut fences, mut dollars) = match &plan {
            Some(plan) => (plan.fences, plan.dollars),
            None => md::count_fences_and_dollars(&self.content),
        };
        let (fences0, dollars0) = (fences, dollars);
        let typed = self.line_range(cursor);
        let orphan = (plan.is_none()
            && fences % 2 == 1
            && md::is_fence(&self.content[typed.clone()])
            && self.content.is_char_boundary(self.edited_at)
            && self.line_range(self.edited_at) == typed)
            .then_some(typed.start);
        fences -= orphan.is_some() as usize;
        // A font without `≠` composes it from a `=` and a misplaced bar: might as well
        // leave `!=`.
        let has_unequal = text_system
            .advance(text_system.resolve_font(&font(sans())), px(16.), '≠')
            .is_ok();
        let mut lang = String::new();
        // Mermaid diagrams and LaTeX formulas: drawn once, reused afterwards.
        let rgb = |c: Hsla| {
            let c = Rgba::from(c);
            [c.r, c.g, c.b].map(|v| (v * 255.) as u8)
        };
        let hex = |c: Hsla| {
            let [r, g, b] = rgb(c);
            format!("#{r:02x}{g:02x}{b:02x}")
        };
        let look = figure::Look::of(&t);
        // A figure's key, and the key of its source alone.
        let key = |kind: u8, source: &str| {
            let (mut hasher, mut family) = (Fast::default(), Fast::default());
            (kind, source, &look.bg, &look.fill, &look.text, &look.line, &look.font, t.size.to_bits())
                .hash(&mut hasher);
            (kind, source).hash(&mut family);
            (hasher.finish(), family.finish())
        };
        let style = {
            let mut hasher = Fast::default();
            (key(0, "").0, hex(t.accent), mono(), has_unequal).hash(&mut hasher);
            hasher.finish()
        };
        // Figures are drawn at the page's largest width, for this screen.
        let job = {
            let (room, density, bg) = (f32::from(t.page.measure(t.size)), window.scale_factor(), rgb(t.bg));
            move |(key, family): (u64, u64), make: Arc<dyn Fn() -> Option<String> + Send + Sync>| Job { key, family, make, room, density, bg }
        };
        let vector = |figure: Option<figure::Figure>| {
            Some(figure.map(|(svg, w, h)| (Arc::new(Image::from_bytes(ImageFormat::Svg, svg.into_bytes())), size(px(w), px(h)))))
        };
        let mut cells = std::mem::take(&mut self.cells);
        cells.next_frame();
        let mut embeds = std::mem::take(&mut self.embeds);
        // Lines of a table already laid out, waiting for their turn in the loop.
        let mut pending: VecDeque<Option<Rc<TableRow>>> = VecDeque::new();
        let empty = Rc::new(Shaped { line: None, lead: None, subs: Vec::new(), formula: None, picture: None });
        let mut shaped = std::mem::take(&mut self.shaped);
        self.frame = self.frame.wrapping_add(1);
        let frame = self.frame;
        let mut old = std::mem::take(&mut self.figures);
        let (mut waiting, before) = (Vec::new(), std::mem::take(&mut self.waiting));
        let mut kept = FastMap::default();
        // Mermaid or `$$` block in progress: its start and its source.
        let mut block: Option<(usize, String)> = None;
        let marked = self.marked.clone();
        let mut rows = Vec::with_capacity(self.rows.len() + 16);
        let mut y = px(0.);
        let mut offset;
        let mut in_code = false;
        // Front matter: its lines are written faded, monospaced, without being interpreted.
        let meta = md::front_matter(&self.content);
        // Color of a `[!TYPE]` callout: its tint tells its type.
        let tint_of = |label: &str| {
            let h = match label.to_ascii_lowercase().as_str() {
                "[!tip]" | "[!success]" => 0.38,
                "[!important]" => 0.75,
                "[!warning]" => 0.09,
                "[!caution]" | "[!danger]" | "[!error]" => 0.,
                _ => 0.58,
            };
            Hsla { h, s: 0.7, l: 0.5, a: 1. }
        };
        let mut tint;
        let mut loading = false;
        let mut big: Vec<BigBlock> = Vec::new();
        let text_len = self.content.len();
        let incremental = plan.is_some();
        #[cfg(test)]
        {
            self.counts.0 += !incremental as usize;
            self.counts.1 += incremental as usize;
        }
        #[cfg(test)]
        if let Some(plan) = &plan {
            self.counts.2 += plan.regions.iter().any(|r| r.big.is_some()) as usize;
        }
        let (old_total, old_big) = (self.rows.len(), std::mem::take(&mut self.big));
        let mut old_rows = std::mem::take(&mut self.rows).into_iter();
        let (after, delta) = plan.as_ref().map_or((usize::MAX, 0), |p| (p.after, p.delta));
        let regions = plan.map_or_else(|| vec![Region { old: 0..0, start: 0, stop: text_len, big: None, check: false }], |p| p.regions);
        // Very big tables reused as they are: their columns, and the place of their first line.
        let mut carried: Vec<(Rc<grid::Geometry>, usize)> = Vec::new();
        let mut next_old = 0;
        let mut regions = regions.into_iter();
        // Reused lines: first those before the zone, then, at the end, those after it.
        let keep = |upto: usize, next_old: &mut usize, old_rows: &mut std::vec::IntoIter<Row>, y: &mut Pixels, rows: &mut Vec<Row>, carried: &mut Vec<(Rc<grid::Geometry>, usize)>| {
            let count = upto - *next_old;
            let shift = if old_rows.as_slice().first().is_some_and(|r| r.start > after) { delta } else { 0 };
            carried.extend(
                old_big.iter().filter(|b| (*next_old..upto).contains(&b.rows.start)).map(|b| (b.geom.clone(), rows.len() + b.rows.start - *next_old)),
            );
            reuse(old_rows, count, shift, y, rows);
            *next_old = upto;
        };
        while let Some(region) = {
            let next = regions.next();
            if incremental {
                keep(next.as_ref().map_or(old_total, |r| r.old.start), &mut next_old, &mut old_rows, &mut y, &mut rows, &mut carried);
            }
            next
        } {
            // The zone replaces its old lines.
            old_rows.by_ref().take(region.old.len()).for_each(drop);
            next_old = region.old.end;
            offset = region.start;
            tint = None;
            let first_new = rows.len();
            // Zone starting at the first line of a very big table: the table stays recorded.
            if let Some((geom, 0, _)) = &region.big {
                carried.push((geom.clone(), first_new));
            }
            while offset <= region.stop {
            let line = &self.content[offset..self.content[offset..].find('\n').map_or(text_len, |i| offset + i)];
            // Inside a very big table, each line stands alone: same height, no cells.
            if let Some((geom, rank, _)) = &region.big {
                let (pad, height) = geom.metrics(rank + rows.len() - first_new, false, grid::line_height(&t));
                rows.push(Row {
                    start: offset,
                    len: line.len(),
                    y,
                    pad,
                    lh: grid::line_height(&t),
                    height,
                    kind: Kind::Table,
                    shaped: empty.clone(),
                    image: None,
                    enlarge: None,
                    opens: false,
                    gap: px(0.),
                    head: false,
                    foot: false,
                    tint: None,
                    dx: px(0.),
                    grid: None,
                    embed: None,
                    broken: None,
                });
                y += height;
                offset += line.len() + 1;
                continue;
            }
            let (mut kind, mut marker) = if offset < meta { (Kind::Meta, 0) } else { md::classify(line, in_code) };
            let mut opens = false;
            let mut drawing = None;
            let mut broken = None;
            let mut enlarge = None;
            let was_in_code = in_code;
            if kind == Kind::Fence && orphan != Some(offset) {
                opens = !in_code && fences > 1;
                in_code = opens;
                fences -= 1;
                if opens {
                    lang = line.trim_start()[3..].trim().to_lowercase();
                    block = (lang == "mermaid").then(|| (offset, String::new()));
                } else if let Some((start, source)) = block.take() {
                    // A diagram is drawn once the cursor has left the block, in a background task.
                    let editing = (start..=offset + line.len()).contains(&cursor);
                    let (k, look) = (key(0, &source), look.clone());
                    drawing = cached(&mut old, &mut kept, k, editing, || {
                        drawn(job(k, Arc::new(move || figure::mermaid(&source, &look))), &before, &mut waiting, window, cx)
                    });
                    broken = failed(&kept, k.0).then(|| tr("Unreadable figure", "Figure illisible"));
                }
            }
            let code = matches!(kind, Kind::Code | Kind::Fence | Kind::Meta);
            // Formula over several lines, between two `$$` lines.
            let mut math = false;
            if kind == Kind::Code {
                if let Some((_, source)) = &mut block {
                    source.push_str(line);
                    source.push('\n');
                }
            } else if !code && line.trim() == "$$" {
                math = true;
                dollars -= 1;
                match block.take() {
                    Some((_, source)) => {
                        let k = key(1, &source);
                        drawing = cached(&mut old, &mut kept, k, false, || {
                            vector(figure::latex(&source, true, rgb(t.text), t.size * MATH_SCALE))
                        });
                        broken = failed(&kept, k.0).then(|| tr("Unreadable formula", "Formule illisible"));
                        enlarge = drawing.is_some().then_some(Source::Formula(source, true));
                    }
                    // As for ```: a `$$` without a closing one opens nothing.
                    None if dollars > 0 => block = Some((offset, String::new())),
                    None => {}
                }
            } else if !code && let Some((_, source)) = &mut block {
                math = true;
                source.push_str(line);
                source.push('\n');
            }
            if math {
                (kind, marker) = (Kind::Para, 0);
            }
            // Callout: the `> [!TYPE]` quote and the ones that follow it.
            let own_tint = (kind == Kind::Quote).then(|| md::callout(line)).flatten().map(|r| tint_of(&line[r]));
            tint = own_tint.or(tint).filter(|_| kind == Kind::Quote);
            let tinted = tint.is_some();
            let last: Option<&mut Row> = rows.last_mut();
            let after_table = last.as_ref().is_some_and(|r| r.kind == Kind::Table);
            let header = kind == Kind::Table && !after_table;
            // A table is laid out in one block, at its first line: its columns
            // depend on all its cells.
            if header {
                let mut at = offset;
                let block: Vec<(usize, &str)> = self.content[offset..]
                    .split('\n')
                    .take_while(|l| md::is_table_line(l))
                    .map(|l| {
                        at += l.len() + 1;
                        (at - l.len() - 1, l)
                    })
                    .collect();
                // A very big table: lines all of the same height, no styling yet.
                if block.len() > grid::BIG {
                    let look = grid::Look { theme: &t, style };
                    let geom = Rc::new(grid::geometry(&mut cells, &text_system, &look, room, &block[..grid::SAMPLE]));
                    let (first, n, lh) = (rows.len(), block.len(), grid::line_height(&t));
                    for (k, &(at, text)) in block.iter().enumerate() {
                        let (pad, height) = geom.metrics(k, k + 1 == n, lh);
                        rows.push(Row {
                            start: at,
                            len: text.len(),
                            y,
                            pad,
                            lh,
                            height,
                            kind: Kind::Table,
                            shaped: empty.clone(),
                            image: None,
                            enlarge: None,
                            opens: false,
                            gap: px(0.),
                            head: false,
                            foot: false,
                            tint: None,
                            dx: px(0.),
                            grid: None,
                            embed: None,
                            broken: None,
                        });
                        y += height;
                    }
                    big.push(BigBlock { rows: first..first + n, geom, mat: Vec::new(), rescan: false });
                    offset = block[n - 1].0 + block[n - 1].1.len() + 1;
                    continue;
                }
                let look = grid::Look { theme: &t, style };
                pending = grid::build(&mut cells, &text_system, &look, room, &block, cursor, marked.as_ref()).iter().cloned().collect();
            }
            let grid_row = if kind == Kind::Table { pending.pop_front().flatten() } else { None };
            if kind != Kind::Table && after_table && let Some(last) = last {
                last.gap = TABLE_GAP;
                last.height += TABLE_GAP;
                y += TABLE_GAP;
            }
            // Titles: a scale of about 1.2, about one body line of room above the first two levels
            // and two thirds of it above the others.
            let (font_size, pad) = match kind {
                Kind::Heading(1) => (px(27.), px(24.)),
                Kind::Heading(2) => (px(22.), px(24.)),
                Kind::Heading(3) => (px(19.), px(16.)),
                Kind::Heading(4) => (px(17.), px(16.)),
                Kind::Heading(_) => (px(16.), px(16.)),
                Kind::Code | Kind::Fence | Kind::Table | Kind::Meta => (px(14.), px(0.)),
                _ => (px(16.), px(0.)),
            };
            // The sizes above hold for a 16 px body text.
            let font_size = font_size * (t.size / 16.);
            let pad = if offset == 0 { px(0.) } else { pad * (t.size / 16.) };

            // An unchanged line, in the same context, reuses its styling from
            // the previous frame: typing only reprocesses the touched line.
            let has_cursor = (offset..=offset + line.len()).contains(&cursor);
            let composed = marked.clone().filter(|m| m.start <= offset + line.len() && m.end >= offset);
            let id = {
                let near = composed.as_ref().map(|m| (m.start.wrapping_sub(offset), m.end.wrapping_sub(offset)));
                let lang = (kind == Kind::Code).then_some(&lang);
                let mut hasher = Fast::default();
                (style, line, kind, marker, math, lang, has_cursor, near, f32::from(width).to_bits(), f32::from(room).to_bits(), header, tinted)
                    .hash(&mut hasher);
                hasher.finish()
            };
            // A title's lines are closer than the body's, so that it belongs to what follows.
            let lh = (font_size * if matches!(kind, Kind::Heading(_)) { 1.35 } else { 1.65 }).round();
            let made = if grid_row.is_some() { empty.clone() } else { cached_shape(&mut shaped, id, frame, || {
                let mut formula = None;
                let mut flags = vec![0u16; line.len()];
                let list = matches!(kind, Kind::Bullet | Kind::Ordered | Kind::Task(_));
                flags[..marker].fill(if list { md::MARK } else { md::DIM });
                if kind == Kind::Meta {
                    flags.fill(md::DIM | md::QUIET);
                } else if math {
                    flags.fill(if line.trim() == "$$" { md::DIM } else { md::CODE });
                } else if kind == Kind::Code {
                    // With no language given, in plain text or in Mermaid, the block stays as it is.
                    if !matches!(lang.as_str(), "" | "text" | "txt" | "plain" | "mermaid") {
                        md::code(line, &lang, &mut flags);
                    }
                } else if !code && kind != Kind::Rule {
                    md::inline(line, marker, &mut flags);
                    // Formula on the line: its source stays readable, like code.
                    if let Some((range, source, display)) = md::math(line)
                        && range.start >= marker
                        && flags[range.start] & md::CODE == 0
                    {
                        flags[range].fill(md::CODE);
                        formula = Some((source.to_string(), display));
                    }
                }
                if kind == Kind::Task(true) {
                    flags[marker..].iter_mut().for_each(|f| *f |= md::STRIKE | md::DIM);
                    // Checked, its checkbox is bold, like a checkbox in the text or a table.
                    if let Some(at) = line[..marker].find('[') {
                        flags[at..(at + 3).min(marker)].iter_mut().for_each(|f| *f |= md::BOLD);
                    }
                }
                if let Some(label) = md::callout(line).filter(|_| own_tint.is_some()) {
                    // The type is tinted, its `[!` and `]` are markers like any other.
                    let (open, close) = (label.start + 2, label.end - 1);
                    flags[label.clone()].fill(md::DIM);
                    flags[open..close].fill(md::MARK);
                }
                // Table: header in bold, `|` and dashes dimmed.
                // A line never wraps, which would break the grid: a
                // table wider than the page scrolls horizontally.
                if kind == Kind::Table {
                    if header {
                        flags.iter_mut().for_each(|f| *f |= md::BOLD);
                    }
                    md::bars(line).into_iter().for_each(|bar| flags[bar] = md::DIM);
                    if line.chars().all(|c| matches!(c, '|' | '-' | ':' | ' ')) {
                        flags.fill(md::DIM);
                    }
                }
                let picture = (!code).then(|| md::image(line)).flatten().map(|(range, path)| {
                    flags[range].iter_mut().for_each(|f| *f |= md::DIM);
                    path
                });
                // Outside the cursor's line, `->`, `!=`… display as signs;
                // the text itself does not change.
                let mut signs = if code || has_cursor || matches!(kind, Kind::Rule | Kind::Table) {
                    Vec::new()
                } else {
                    md::symbols(line, marker, &flags)
                };
                signs.retain(|(_, _, sign)| has_unequal || *sign != "≠");
                let mut shown = Cow::Borrowed(line);
                let mut subs = Vec::new();
                if !signs.is_empty() {
                    let (mut text, mut styles, mut done) = (String::new(), Vec::new(), 0);
                    for (at, len, sign) in signs {
                        text.push_str(&line[done..at]);
                        styles.extend_from_slice(&flags[done..at]);
                        text.push_str(sign);
                        styles.resize(text.len(), flags[at]);
                        subs.push((at, len, sign.len()));
                        done = at + len;
                    }
                    text.push_str(&line[done..]);
                    styles.extend_from_slice(&flags[done..]);
                    shown = Cow::Owned(text);
                    flags = styles;
                }
                let is_marked = |i: usize| composed.as_ref().is_some_and(|m| m.contains(&(offset + i)));

                let style = RunStyle {
                    heading: matches!(kind, Kind::Heading(_)),
                    soft: kind == Kind::Heading(6),
                    italic: kind == Kind::Quote && !tinted,
                    mono: code || kind == Kind::Table,
                    tint: own_tint,
                };
                // A title's `#` marker is at the body size, `dim`, on the title's baseline.
                let cut = if matches!(kind, Kind::Heading(_)) { marker } else { 0 };
                let runs = text_runs(&t, &shown[cut..], &flags[cut..], |i| is_marked(i + cut), &style);
                let shape = |text: &str, size: Pixels, runs: &[TextRun], wrap: Option<Pixels>| {
                    window.text_system().shape_text(text.to_string().into(), size, runs, wrap, None).ok().and_then(|lines| lines.into_iter().next())
                };
                let marker_style = RunStyle { heading: false, soft: false, italic: false, mono: false, tint: None };
                let mut lead = (cut > 0)
                    .then(|| shape(&shown[..cut], px(t.size), &text_runs(&t, &shown[..cut], &flags[..cut], is_marked, &marker_style), None))
                    .flatten()
                    .map(|line| Lead { width: line.position_for_index(cut, lh).map_or(px(0.), |p| p.x), line, len: cut, dy: px(0.) });
                let wrap = (kind != Kind::Table).then(|| width - lead.as_ref().map_or(px(0.), |l| l.width));
                let line = shape(&shown[cut..], font_size, &runs, wrap);
                if let Some(lead) = &mut lead {
                    // An empty title has no metrics of its own: those of a letter will do.
                    let probe = if shown[cut..].is_empty() { shape("x", font_size, &text_runs(&t, "x", &[0], |_| false, &style), None) } else { None };
                    if let Some(title) = probe.as_ref().or(line.as_ref()) {
                        let baseline = |l: &WrappedLine| (lh - (l.ascent() + l.descent())) / 2. + l.ascent();
                        lead.dy = baseline(title) - baseline(&lead.line);
                    }
                }
                Rc::new(Shaped { line, lead, subs, formula, picture })
            }) };
            if let Some((source, display)) = &made.formula {
                drawing = cached(&mut old, &mut kept, key(1 + *display as u8, source), false, || {
                    vector(figure::latex(source, *display, rgb(t.text), t.size * MATH_SCALE))
                });
            }
            // ponytail: one picture per line, looked up in the note's folder, at the
            // vault root, then by its name in the whole vault; inline
            // pictures (http) are not loaded.
            let file = made.picture.as_ref().and_then(|path| {
                let root = self.dirs.last();
                let found = self.dirs.iter().map(|d| d.join(path)).find(|p| p.is_file() && root.is_some_and(|r| inside(r, p)));
                found.or_else(|| {
                    let name = path.rsplit('/').next()?.to_lowercase();
                    self.images.get(&name).cloned()
                })
            });
            let has_file = file.is_some();
            let svg = file.as_ref().is_some_and(|f| f.extension().is_some_and(|e| e.eq_ignore_ascii_case("svg")));
            // The picture wins over a formula on the same line: it is what is drawn.
            enlarge = file
                .as_ref()
                .map(|f| if svg { Source::File(f.clone(), None) } else { Source::Raster(f.clone()) })
                .or(enlarge)
                .or_else(|| made.formula.clone().map(|(source, display)| Source::Formula(source, display)));
            // An SVG is redrawn in the theme's colors, and every time its file
            // changes: its date is part of its key. A Bref diagram takes the ink of the
            // text; an SVG from elsewhere is made readable (HTML text, `light-dark()`).
            if svg
                && let Some(file) = &file
                && let Ok(stamp) = fs::metadata(file).and_then(|m| m.modified())
            {
                let (k, family) = (key(3, &format!("{}{stamp:?}", file.display())).0, key(3, &file.display().to_string()).1);
                let (path, ink, dark) = (file.clone(), crate::views::canvas::neutral(t), look.dark);
                let own = cached(&mut old, &mut kept, (k, family), false, || {
                    let make = move || {
                        let text = fs::read_to_string(&path).ok()?;
                        Some(Diagram::from_svg(&text).map_or_else(|| svg::readable(&text, dark), |d| d.to_svg(ink)))
                    };
                    drawn(job((k, family), Arc::new(make)), &before, &mut waiting, window, cx)
                });
                broken = broken.or(failed(&kept, k).then(|| tr("Unreadable figure", "Figure illisible")));
                drawing = own.or(drawing);
            }
            let image = file.filter(|_| drawing.is_none() && !svg).and_then(|file| {
                let loaded = window.use_asset::<ImgResourceLoader>(&Resource::Path(file.into()), cx);
                loading |= loaded.is_none();
                let image = match loaded? {
                    Ok(image) => image,
                    Err(_) => {
                        broken = Some(tr("Unreadable image", "Image illisible"));
                        return None;
                    }
                };
                let natural = |d: gpui::DevicePixels| d.0 as f32;
                let (w, h) = (natural(image.size(0).width), natural(image.size(0).height));
                let scale = (f32::from(width) / w.max(1.)).min(1.);
                Some((image, size(px(w * scale), px(h * scale))))
            });
            let image = image.or_else(|| {
                let (image, s) = drawing?;
                let rendered = image.use_render_image(window, cx);
                loading |= rendered.is_none();
                let image = rendered?;
                let scale = (width / s.width).min(1.);
                Some((image, size(s.width * scale, s.height * scale)))
            });
            let enlarge = enlarge.filter(|_| image.is_some());
            // `![[Note#Heading]]` alone on a line, that is no picture and no other file: the section it points at,
            // framed under it.
            let embed = (!code && !has_file && made.picture.is_some())
                .then(|| crate::logic::embed::parse(line))
                .flatten()
                .filter(|target| !vault::is_file_shown(Path::new(&target.note)))
                .map(|target| Rc::new(embeds.row(&target, width, self, frame as u64, window, cx)));

            let text_height = made.line.as_ref().map_or(lh, |l| l.size(lh).height);
            // A code block and a callout have room inside their fill, above the first line and under the last.
            let (head, foot) = match kind {
                Kind::Fence => (!was_in_code, was_in_code || !opens),
                Kind::Quote => {
                    let next = self.content.get(offset + line.len() + 1..).and_then(|rest| rest.split('\n').next()).unwrap_or("");
                    (own_tint.is_some() || !rows.last().is_some_and(|r| r.kind == Kind::Quote), md::classify(next, false).0 != Kind::Quote || md::callout(next).is_some())
                }
                _ => (false, false),
            };
            let room = if matches!(kind, Kind::Code | Kind::Fence) || tinted { BLOCK_ROOM * (t.size / 16.) } else { px(0.) };
            let (above, below) = (if head { room } else { px(0.) }, if foot { room } else { px(0.) });
            let (pad, lh, height) = match &grid_row {
                Some(grid) => {
                    let lh = grid::line_height(&t);
                    (grid.pad(), lh, grid.height(lh))
                }
                None => (pad + above, lh, pad + above + text_height),
            };
            let row = Row {
                start: offset,
                len: line.len(),
                y,
                pad,
                lh,
                height,
                kind,
                shaped: made,
                image,
                enlarge,
                opens,
                gap: below,
                head,
                foot,
                tint,
                dx: px(0.),
                grid: grid_row,
                embed: None,
                broken,
            };
            let height = row.height + row.image_height() + embed.as_ref().map_or(px(0.), |e| e.height()) + row.gap;
            rows.push(Row { height, embed, ..row });
            y += height;
            offset += line.len() + 1;
            }
        }
        if let Some(last) = rows.last_mut().filter(|r| r.kind == Kind::Table && r.gap == px(0.)) {
            last.gap = TABLE_GAP;
            last.height += TABLE_GAP;
            y += TABLE_GAP;
        }
        embeds.sweep(&rows, frame as u64);
        self.rows = rows;
        self.rows_revision = self.revision;
        loading |= embeds.loading(cx);
        self.embeds = embeds;
        // Reused very big tables: their lines are no longer the ones that were styled.
        for (geom, first) in carried {
            let end = first + self.rows[first..].iter().take_while(|r| r.kind == Kind::Table).count();
            big.push(BigBlock { rows: first..end, geom, mat: Vec::new(), rescan: true });
        }
        big.sort_by_key(|b| b.rows.start);
        self.big = big;
        self.big_style = style;
        self.total_y = y;
        if incremental {
            kept.extend(old.drain());
        }
        // A figure no longer requested (other theme, changed source) is no longer awaited.
        for job in before {
            if !waiting.iter().any(|w| w.id() == job.id()) {
                cx.remove_asset::<Job>(&job);
            }
        }
        self.loading = loading || !waiting.is_empty();
        self.waiting = waiting;
        // Figures no longer in use free their image and its room on the graphics card,
        // unless a kept figure has the same one (same source, back to the same theme).
        let used: Vec<u64> = kept.values().filter_map(|(_, d)| Some(d.as_ref()?.0.id())).collect();
        for (_, (_, drawing)) in old {
            if let Some((image, _)) = drawing.filter(|(image, _)| !used.contains(&image.id())) {
                if let Some(render) = image.clone().get_render_image(window, cx) {
                    window.drop_image(render).ok();
                }
                image.remove_asset(cx);
            }
        }
        self.figures = kept;
        self.dmg = None;
        self.laid = Some(Laid {
            epoch: self.epoch,
            width: f32::from(width).to_bits(),
            room: f32::from(room).to_bits(),
            size: t.size.to_bits(),
            len: text_len,
            cursor,
            fences: fences0,
            dollars: dollars0,
            meta,
        });
        // Lines no longer in use (deleted, changed) go when they outnumber the note.
        if shaped.len() > self.rows.len() * 2 + 256 {
            shaped.retain(|_, (_, used)| *used == frame);
        }
        self.shaped = shaped;
        self.cells = cells;
        self.width = width;
        self.room = room;
        self.opening = (0..self.rows.len()).filter(|&i| self.rows[i].opens).collect();
        self.table_meta.clear();
        let mut i = 0;
        while i < self.rows.len() {
            let n = self.rows[i..].iter().take_while(|r| r.kind == Kind::Table).count();
            if n > 0 {
                let wide = match self.big.iter().find(|b| b.rows.start == i) {
                    Some(block) => block.geom.total(),
                    None => self.rows[i..i + n].iter().map(Row::content_width).fold(px(0.), |a, b| a.max(b)),
                };
                let (first, last) = (&self.rows[i], &self.rows[i + n - 1]);
                self.table_meta.push(TableMeta { range: first.start..last.start + last.len, rows: i..i + n, wide });
            }
            i += n.max(1);
        }
    }

    /// Gives their cells to the lines of very big tables that are near the screen or the cursor,
    /// and removes them from those that moved away.
    pub(super) fn materialize(&mut self, bounds: Bounds<Pixels>, window: &mut Window) {
        if self.big.is_empty() {
            return;
        }
        let (cursor, t) = (self.cursor(), self.theme);
        let at = self.row_at(cursor);
        let look = grid::Look { theme: &t, style: self.big_style };
        let (page, top) = (bounds.size.height, self.scroll_y - self.top);
        let (lo, hi) = (top - page * 2., top + page * 3.);
        let text_system = window.text_system().clone();
        let mut cells = std::mem::take(&mut self.cells);
        for b in 0..self.big.len() {
            let range = self.big[b].rows.clone();
            let n = range.len();
            let rows = &self.rows[range.clone()];
            let (from, to) = (rows.partition_point(|r| r.y + r.height < lo), rows.partition_point(|r| r.y <= hi));
            let mut want = vec![0..3.min(n), from..to.max(from)];
            if let Some(k) = at.filter(|c| range.contains(c)).map(|c| c - range.start) {
                want.push(k.saturating_sub(40)..(k + 41).min(n));
            }
            if want == self.big[b].mat && !self.big[b].rescan {
                continue;
            }
            let wanted = |k: usize| want.iter().any(|w| w.contains(&k));
            if std::mem::take(&mut self.big[b].rescan) {
                // Lines from another layout: which ones have their cells is no longer known.
                for k in (0..n).filter(|&k| !wanted(k)) {
                    self.rows[range.start + k].grid = None;
                }
            }
            for old in std::mem::take(&mut self.big[b].mat) {
                for k in old.filter(|&k| !wanted(k)) {
                    self.rows[range.start + k].grid = None;
                }
            }
            let geom = self.big[b].geom.clone();
            for k in want.iter().flat_map(|w| w.clone()) {
                let row = &mut self.rows[range.start + k];
                if row.grid.is_none() {
                    let text = &self.content[row.start..row.start + row.len];
                    let built = grid::big_row(&mut cells, &text_system, &look, &geom, k, k + 1 == n, (row.start, text), self.marked.as_ref());
                    row.grid = Some(Rc::new(built));
                }
            }
            self.big[b].mat = want;
        }
        cells.trim(20_000);
        self.cells = cells;
    }

    /// What depends on scrolling and the window size, and is redone every frame:
    /// view, table frames, clickable zones.
    pub(super) fn place(&mut self, bounds: Bounds<Pixels>, window: &mut Window, _: &mut App) {
        let reveal = std::mem::take(&mut self.reveal);
        let to_top = std::mem::take(&mut self.reveal_top);
        // A narrow note (the pane, not the page measure, sets its width) that reaches under the window buttons starts lower.
        // Only the geometry counts, so the text never jumps while it is typed. ponytail: a long first line of a wide note
        // can still pass under them; that is worth no gap at the top of every note.
        let narrow = self.width + SIDE * 2. >= bounds.size.width;
        let text_right = bounds.left() + (bounds.size.width + self.width) / 2.;
        self.top = if narrow && self.pill.is_some_and(|left| text_right > left) { PILL_CLEAR } else { TOP.max(self.strip) };
        if reveal {
            let c = self.cursor();
            if let Some(r) = self.row_at(c) {
                let row = &self.rows[r];
                let top = self.top + row.y + row.pad + row.pos(c - row.start).y;
                let margin = px(24.);
                if to_top || top - self.scroll_y < margin {
                    self.scroll_y = top - margin;
                } else if top + row.lh - self.scroll_y > bounds.size.height - FOOTER - margin {
                    self.scroll_y = top + row.lh - bounds.size.height + FOOTER + margin;
                }
            }
        }
        let max_scroll = (self.top + self.total_y - bounds.size.height * 0.4).max(px(0.));
        self.scroll_y = self.scroll_y.max(px(0.)).min(max_scroll);
        self.origin = point(
            bounds.left() + (bounds.size.width - self.width) / 2.,
            bounds.top() + self.top - self.scroll_y,
        );
        self.viewport = bounds;
        self.materialize(bounds, window);
        if !self.readonly {
        self.view_hitbox = Some(window.insert_hitbox(bounds, HitboxBehavior::Normal));
        self.copy_hitboxes = self
            .opening
            .iter()
            .map(|&i| &self.rows[i])
            .map(|r| (r.start, window.insert_hitbox(self.copy_bounds(r), HitboxBehavior::Normal)))
            .collect();
        self.view_hitboxes = self
            .opening
            .iter()
            .map(|&i| &self.rows[i])
            .filter(|r| self.views(r))
            .map(|r| (r.start, window.insert_hitbox(self.view_bounds(r), HitboxBehavior::Normal)))
            .collect();
        self.big_hitboxes = self
            .rows
            .iter()
            .filter_map(|r| Some((r.start, window.insert_hitbox(self.big_bounds(r)?, HitboxBehavior::Normal))))
            .collect();
        let grid = self.grid.and_then(|_| self.popup_at(Self::grid_size()));
        self.grid_hitbox = grid.map(|at| window.insert_hitbox(Bounds::new(at, Self::grid_size()), HitboxBehavior::Normal));
        }
        self.frames(reveal, window);
    }

    /// What an embedded section shows: where it is on screen, and what is visible of it.
    pub(super) fn place_embedded(&mut self, origin: Point<Pixels>, clip: Bounds<Pixels>, window: &mut Window) {
        self.origin = origin;
        self.viewport = clip;
        self.scroll_y = px(0.);
        self.pointer = point(px(-1e5), px(-1e5));
        // Everything of it is near enough: a very big table of it has all its cells.
        self.materialize(Bounds::new(origin, size(self.width, self.total_y)), window);
        self.frames(false, window);
    }

    /// The frame of each table of the layout, and the zones of its handles.
    fn frames(&mut self, reveal: bool, window: &mut Window) {
        let (cursor, width, room) = (self.cursor(), self.width, self.room);
        // Frame of each table: its lines, at the width of the longest without
        // going past the page; beyond that, it scrolls and follows the cursor.
        let scrolled = std::mem::take(&mut self.table_x);
        self.tables.clear();
        self.geos.clear();
        self.thumbs.clear();
        let starts: Vec<usize> = self.table_meta.iter().map(|m| m.range.start).collect();
        let marks = self.note_marks(&starts);
        for meta in &self.table_meta {
            let (start, end) = (meta.range.start, meta.range.end);
            let wide = meta.wide;
            let frame_w = wide.min(room);
            let overflow = (wide - room).max(px(0.));
            let lead = table_lead(self.theme.wide, frame_w, width);
            let mut dx = scrolled.get(&start).copied().unwrap_or_default().max(px(0.)).min(overflow);
            if reveal && (start..=end).contains(&cursor) {
                let at = self.row_at(cursor).unwrap_or(meta.rows.start).clamp(meta.rows.start, meta.rows.end - 1);
                let row = &self.rows[at];
                // Where the cursor would be without scrolling.
                let x = row.pos(cursor - row.start).x + row.dx;
                let margin = px(24.);
                if x - dx < margin {
                    dx = x - margin;
                } else if x - dx > frame_w - margin {
                    dx = x - frame_w + margin;
                }
                dx = dx.max(px(0.)).min(overflow);
            }
            if overflow > px(0.) {
                self.table_x.insert(start, dx);
            }
            if self.rows[meta.rows.start].dx != dx + lead {
                self.rows[meta.rows.clone()].iter_mut().for_each(|r| r.dx = dx + lead);
            }
            let (first, last) = (&self.rows[meta.rows.start], &self.rows[meta.rows.end - 1]);
            let top = first.y + first.pad;
            let frame = Bounds::new(
                self.origin + point(-lead, top),
                size(frame_w, last.y + last.height - last.gap - top),
            );
            if overflow > px(0.) {
                let thumb = (frame_w * (frame_w / wide)).max(px(24.));
                let at = (frame_w - thumb) * (dx / overflow);
                let bar = Bounds::new(point(frame.left() + at, frame.bottom() - px(5.)), size(thumb, px(3.)));
                self.thumbs.push(bar);
            }
            self.tables.push((start..end, frame));
            // A very big table has no handles: its gestures do not rewrite it as a whole.
            let big = self.big.iter().any(|b| b.rows.start == meta.rows.start);
            if let Some(grid) = first.grid.as_ref().filter(|_| !big) {
                let xs = grid.xs().iter().map(|x| frame.left() - dx + *x).collect();
                let lines = self.rows[meta.rows.clone()].iter().filter(|r| !r.hidden()).map(|r| self.origin.y + r.y);
                let mut ys: Vec<Pixels> = std::iter::once(frame.top()).chain(lines.skip(1)).collect();
                ys.push(frame.bottom());
                self.geos.push(Geo { start, frame, xs, ys, notes: marks.get(&start).cloned().unwrap_or_default() });
            }
        }
        if self.readonly {
            return;
        }
        let bars = self.tables.iter().flat_map(|(_, frame)| plus_bars(*frame));
        self.table_hitboxes = bars.map(|bar| window.insert_hitbox(bar, HitboxBehavior::Normal)).collect();
        self.handles.hitboxes = self
            .handle_bounds()
            .map(|(grip, plus)| (window.insert_hitbox(grip, HitboxBehavior::Normal), plus.map(|plus| window.insert_hitbox(plus, HitboxBehavior::Normal))));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logic::prefs::Prefs;
    use gpui::WindowAppearance;

    /// The runs of `text` as `(piece, italic, mono)`, `flags` the same on every byte.
    fn pieces(text: &str, flag: u16, style: RunStyle) -> Vec<(String, bool, bool)> {
        let t = Theme::of(&Prefs::parse(""), WindowAppearance::Light);
        let (mut at, flags) = (0, vec![flag; text.len()]);
        text_runs(&t, text, &flags, |_| false, &style)
            .iter()
            .map(|r| {
                let piece = text[at..at + r.len].to_string();
                at += r.len;
                (piece, r.font.style == FontStyle::Italic, r.font.family == mono())
            })
            .collect()
    }

    fn plain() -> RunStyle {
        RunStyle { heading: false, soft: false, italic: false, mono: false, tint: None }
    }

    #[test]
    fn characters_missing_from_the_app_fonts_leave_the_italic_and_the_monospace() {
        // Italic: the CJK pieces are upright, the Latin ones keep their slant; every byte is in one run.
        let got = pieces("abc 中文 def", md::ITALIC, plain());
        assert_eq!(got, [("abc ".into(), true, false), ("中文".into(), false, false), (" def".into(), true, false)]);
        // Monospace (inline code, then a code block): the same, the piece in the sans.
        for (flag, style) in [(md::CODE, plain()), (0, RunStyle { mono: true, ..plain() })] {
            let got = pieces("a漢b", flag, style);
            assert_eq!(got, [("a".into(), false, true), ("漢".into(), false, false), ("b".into(), false, true)]);
        }
        // Edges: one character, only CJK, CJK at both ends, a quote (italic by its kind), Japanese, Korean, a second plane.
        assert_eq!(pieces("中", md::ITALIC, plain()), [("中".into(), false, false)]);
        assert_eq!(pieces("中a日", md::ITALIC, plain()).len(), 3);
        assert_eq!(pieces("かな한글𠀀", md::ITALIC, plain()), [("かな한글𠀀".into(), false, false)]);
        assert_eq!(pieces("引用 中", 0, RunStyle { italic: true, ..plain() }), [("引用".into(), false, false), (" ".into(), true, false), ("中".into(), false, false)]);
        // Nothing to cut: one run, unchanged; a plain run with CJK stays as it was.
        assert_eq!(pieces("abc", md::ITALIC, plain()), [("abc".into(), true, false)]);
        assert_eq!(pieces("中文 abc", 0, plain()), [("中文 abc".into(), false, false)]);
        assert_eq!(pieces("", md::ITALIC, plain()), []);
    }
}
