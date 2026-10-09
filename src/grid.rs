//! Grille des tableaux : les colonnes se partagent la largeur de la page et le texte
//! des cellules passe à la ligne, comme sur le web. Le Markdown reste du texte brut :
//! seul le rendu change.

use std::{
    hash::{Hash, Hasher},
    ops::Range,
    rc::Rc,
};

use gpui::{
    App, Bounds, ContentMask, Pixels, Point, TextAlign, Window, WindowTextSystem, WrappedLine, fill, point, px, size,
};

use crate::{
    Theme,
    editor::{Fast, FastMap, RunStyle, text_runs},
    markdown as md,
};

/// Marge intérieure d'une cellule, et épaisseur des traits.
pub const PAD_X: Pixels = px(10.);
pub const PAD_Y: Pixels = px(5.);
pub const LINE: Pixels = px(1.);
/// Largeur de texte d'une colonne dont les cellules sont courtes ou vides, et la plus
/// étroite qu'on lui donne : en dessous, c'est le tableau qui défile.
const MIN_WIDE: f32 = 48.;
const NARROWEST: f32 = 36.;

/// Texte d'une cellule mis en forme (sur une ligne, ou à la largeur de sa colonne).
pub struct Shape {
    line: Option<WrappedLine>,
    /// Largeur du plus long morceau insécable, mesurée quand le texte est sur une ligne.
    narrow: Pixels,
    /// `\|` écrit, `|` affiché : (octet, longueur écrite, longueur affichée), comme
    /// pour les signes d'une ligne.
    subs: Vec<(usize, usize, usize)>,
}

/// Mises en forme de la dernière frame, reprises tant que leur texte et leur largeur
/// ne changent pas : taper dans une cellule ne remet en forme que celles qui bougent.
#[derive(Default)]
pub struct Cache {
    old: FastMap<u64, Rc<Shape>>,
    new: FastMap<u64, Rc<Shape>>,
    /// Les tableaux entiers, par empreinte de leur texte : un tableau qui ne change pas
    /// (le curseur ou la page bougent) n'est pas remis en page.
    tables_old: FastMap<u64, Tables>,
    tables_new: FastMap<u64, Tables>,
}

/// Les lignes d'un tableau mises en page.
pub type Tables = Rc<Vec<Option<Rc<TableRow>>>>;

impl Cache {
    /// Les mises en forme gardées pour la prochaine frame ne grossissent pas sans fin quand on fait
    /// défiler un très gros tableau : les lignes affichées gardent les leurs.
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

/// Ce qui, hors du texte, change la mise en forme.
pub struct Look<'a> {
    pub theme: &'a Theme,
    /// Empreinte du thème et des polices : elle change la clé de chaque cellule.
    pub style: u64,
}

/// Corps du texte d'une cellule, et sa hauteur de ligne.
pub fn font_size(theme: &Theme) -> Pixels {
    px(15.) * (theme.size / 16.)
}

pub fn line_height(theme: &Theme) -> Pixels {
    (font_size(theme) * 1.5).round()
}

/// Octet du texte → octet du texte affiché.
pub fn shown(subs: &[(usize, usize, usize)], i: usize) -> usize {
    let mut shift = 0isize;
    for &(at, src, dst) in subs {
        if i <= at {
            break;
        }
        // À l'intérieur d'un signe : on se place juste après lui.
        if i < at + src {
            return (at as isize + shift) as usize + dst;
        }
        shift += dst as isize - src as isize;
    }
    (i as isize + shift) as usize
}

/// Octet du texte affiché → octet du texte.
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

/// Les caractères que le retour à la ligne de GPUI ne sépare pas du mot qui précède.
fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(c, '\u{00C0}'..='\u{024F}' | '\u{0400}'..='\u{04FF}')
        || matches!(c, '-' | '_' | '.' | '\'' | '$' | '%' | '@' | '#' | '^' | '~' | ',' | '=' | ':' | '⋯')
}

/// Les morceaux que le retour à la ligne ne coupe pas, comme il les voit : un mot avec
/// les espaces qui le suivent ; chaque idéogramme, chaque signe est à part.
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

/// Abscisse de l'octet `i` dans la ligne sans retour.
fn x_at(line: &WrappedLine, i: usize) -> Pixels {
    line.unwrapped_layout.x_for_index(i)
}

/// Largeur du plus long morceau insécable.
fn longest(line: &WrappedLine) -> Pixels {
    let trimmed = |r: Range<usize>| r.start..line.text[r.clone()].trim_end().len() + r.start;
    let widths = units(&line.text).into_iter().map(trimmed).map(|r| x_at(line, r.end) - x_at(line, r.start));
    widths.fold(px(0.), |a, b| a.max(b))
}

/// Largeur de contenu de chaque colonne, comme un tableau HTML : tout le texte sur une
/// ligne si la page le permet ; sinon les colonnes courtes gardent leur largeur et les longues
/// se partagent le reste, sans passer sous leur plus long mot ; et si même les mots ne
/// tiennent pas, les plus longs sont coupés. `narrow` : plus long mot ; `wide` : texte sur une ligne.
pub fn column_widths(narrow: &[f32], wide: &[f32], avail: f32) -> Vec<f32> {
    let total = |v: &[f32]| v.iter().sum::<f32>();
    if total(wide) <= avail {
        return wide.to_vec();
    }
    let narrow: Vec<f32> = narrow.iter().zip(wide).map(|(n, w)| n.min(*w)).collect();
    if total(&narrow) <= avail {
        // Un même niveau pour les colonnes qui en ont besoin : les courtes gardent leur largeur, les
        // longues se partagent le reste, sans passer sous leur plus long mot.
        let take = |level: f32| narrow.iter().zip(wide).map(|(n, w)| level.clamp(*n, *w)).sum::<f32>();
        let (mut low, mut high) = (0., wide.iter().copied().fold(0., f32::max));
        for _ in 0..40 {
            let mid = (low + high) / 2.;
            if take(mid) > avail { high = mid } else { low = mid }
        }
        return narrow.iter().zip(wide).map(|(n, w)| low.clamp(*n, *w)).collect();
    }
    // On abaisse les colonnes les plus larges à un même niveau, comme de l'eau dans des vases.
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

/// Une cellule : son contenu dans la ligne et son texte mis en forme.
#[derive(Clone)]
pub struct Cell {
    /// Contenu, sans les espaces autour (octets de la ligne) ; vide au bout de la ligne
    /// pour une colonne qu'elle n'a pas.
    pub range: Range<usize>,
    shape: Rc<Shape>,
    align: TextAlign,
    /// Largeur du texte : celle de la colonne.
    width: Pixels,
    /// Le texte ne passe pas à la ligne (très gros tableau) : ce qui dépasse est coupé.
    clip: bool,
}

impl Cell {
    fn lines(&self) -> i32 {
        self.shape.line.as_ref().map_or(1, |l| l.wrap_boundaries.len() as i32 + 1)
    }

    /// Largeur de la ligne visuelle `k`.
    fn line_width(&self, k: usize) -> Pixels {
        let Some(line) = &self.shape.line else {
            return px(0.);
        };
        let edge = |b: &gpui::WrapBoundary| line.runs()[b.run_ix].glyphs[b.glyph_ix].position.x;
        let start = k.checked_sub(1).map_or(px(0.), |k| edge(&line.wrap_boundaries[k]));
        line.wrap_boundaries.get(k).map_or(line.unwrapped_layout.width, edge) - start
    }

    /// Décalage de la ligne visuelle `k` dû à l'alignement de la colonne.
    fn shift(&self, k: usize) -> Pixels {
        let room = (self.width - self.line_width(k)).max(px(0.));
        match self.align {
            TextAlign::Left => px(0.),
            TextAlign::Center => room / 2.,
            TextAlign::Right => room,
        }
    }

    /// Position de l'octet `i` du contenu, depuis le coin de la cellule.
    fn position(&self, i: usize, lh: Pixels) -> Point<Pixels> {
        let Some(line) = &self.shape.line else {
            return point(self.shift(0), px(0.));
        };
        let i = shown(&self.shape.subs, i);
        // À un retour à la ligne, le curseur est au début de la ligne qui suit, non au bout de la précédente.
        let first = |b: &gpui::WrapBoundary| line.runs()[b.run_ix].glyphs[b.glyph_ix].index;
        if let Some(k) = line.wrap_boundaries.iter().position(|b| first(b) == i) {
            return point(self.shift(k + 1), lh * (k + 1) as f32);
        }
        let p = line.position_for_index(i, lh).unwrap_or_default();
        point(p.x + self.shift((f32::from(p.y) / f32::from(lh)).round() as usize), p.y)
    }

    /// Octet du contenu le plus proche du point `p`, depuis le coin de la cellule.
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

    /// Bandes à teinter pour sélectionner `from..to` : (x, y, largeur) par ligne visuelle.
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

/// Une ligne du tableau. Les rangées sont des lignes du texte : les octets sont ceux de la ligne.
#[derive(Clone)]
pub struct TableRow {
    pub cells: Vec<Cell>,
    /// Abscisse des traits verticaux, depuis le bord gauche de la page : un de plus que de colonnes.
    xs: Rc<Vec<Pixels>>,
    /// Lignes visuelles de la plus haute cellule ; 0 pour la ligne de tirets, qu'on ne voit pas.
    pub lines: i32,
    header: bool,
    /// Épaisseur du trait au-dessus (plus épais sous l'en-tête) et de celui du dessous (dernière ligne).
    top: Pixels,
    bottom: Pixels,
    /// Positions des `|` dans la ligne, et nombre de cellules qu'elle écrit.
    bars: Vec<usize>,
    written: usize,
}

impl TableRow {
    pub fn hidden(&self) -> bool {
        self.lines == 0
    }

    /// Largeur du tableau, traits compris.
    pub fn total(&self) -> Pixels {
        self.xs.last().map_or(px(0.), |x| *x + LINE)
    }

    /// Bord gauche du texte de la colonne `c`.
    fn left(&self, c: usize) -> Pixels {
        self.xs[c] + LINE + PAD_X
    }

    /// Place au-dessus du texte.
    pub fn pad(&self) -> Pixels {
        self.top + PAD_Y
    }

    /// Hauteur de la ligne, pour des lignes visuelles de `lh`.
    pub fn height(&self, lh: Pixels) -> Pixels {
        if self.hidden() { px(0.) } else { self.pad() + lh * self.lines as f32 + PAD_Y + self.bottom }
    }

    /// Colonne où se trouve l'octet `i` : celle du `|` qui la précède.
    fn column(&self, i: usize) -> usize {
        let before = self.bars.iter().filter(|&&bar| bar < i).count();
        before.saturating_sub(1).min(self.written.saturating_sub(1))
    }

    /// Lignes visuelles de la cellule où se trouve l'octet `i`.
    pub fn lines_at(&self, i: usize) -> i32 {
        self.cells.get(self.column(i)).map_or(0, Cell::lines)
    }

    /// Position de l'octet `i`, depuis le coin du texte de la ligne. Dans les `|` ou
    /// les espaces entre cellules, celle de la cellule voisine.
    pub fn pos(&self, i: usize, lh: Pixels) -> Point<Pixels> {
        let c = self.column(i);
        let Some(cell) = self.cells.get(c) else {
            return Point::default();
        };
        let p = cell.position(i.clamp(cell.range.start, cell.range.end) - cell.range.start, lh);
        point(self.left(c) + p.x, p.y)
    }

    /// Octet le plus proche du point `p`, depuis le coin du texte de la ligne.
    pub fn index_at(&self, p: Point<Pixels>, lh: Pixels) -> usize {
        let Some(last) = self.cells.len().checked_sub(1) else {
            return 0;
        };
        let c = self.xs.partition_point(|&x| x <= p.x).saturating_sub(1).min(last);
        let cell = &self.cells[c];
        cell.range.start + cell.index(point(p.x - self.left(c), p.y), lh)
    }

    /// Bandes à teinter pour sélectionner `from..to` : (x, y, largeur), depuis le coin
    /// du texte de la ligne. Les `|` et les espaces entre les cellules ne sont pas teintés.
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

    /// Fond de l'en-tête, à dessiner sous le texte. `at` : coin de la ligne à l'écran ;
    /// `height` : sa hauteur, sans la place laissée sous le tableau.
    pub fn paint_back(&self, at: Point<Pixels>, height: Pixels, theme: &Theme, window: &mut Window) {
        if self.header {
            window.paint_quad(fill(Bounds::new(at, size(self.total(), height)), theme.code_bg));
        }
    }

    /// Le texte des cellules et les traits, à dessiner par-dessus.
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

/// Un morceau du texte à mettre en forme.
fn key(text: &str, wrap: Option<Pixels>, bold: bool, marked: &Option<Range<usize>>, look: &Look) -> u64 {
    let mut hasher = Fast::default();
    (text, wrap.map(|w| f32::from(w).to_bits()), bold, marked, look.style, f32::from(font_size(look.theme)).to_bits()).hash(&mut hasher);
    hasher.finish()
}

/// Met en forme le texte d'une cellule, sur une ligne ou à la largeur `wrap`.
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
    if bold {
        flags.iter_mut().for_each(|f| *f |= md::BOLD);
    }
    // `\|` s'écrit pour ne pas couper la cellule : on affiche `|`.
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

/// Alignement demandé par une cellule de la ligne de tirets : `:--`, `:-:` ou `--:`.
fn align_of(dashes: &str) -> TextAlign {
    match (dashes.starts_with(':'), dashes.len() > 1 && dashes.ends_with(':')) {
        (true, true) => TextAlign::Center,
        (false, true) => TextAlign::Right,
        _ => TextAlign::Left,
    }
}

/// Mise en page d'un tableau, ligne par ligne. `lines` : (début, texte) de chaque ligne
/// du tableau. `None` : la ligne de tirets, quand le curseur y est ; elle s'affiche
/// alors comme du texte. Sinon elle est masquée.
pub fn build(
    cache: &mut Cache,
    text_system: &WindowTextSystem,
    look: &Look,
    width: Pixels,
    lines: &[(usize, &str)],
    cursor: usize,
    marked: Option<&Range<usize>>,
) -> Tables {
    // Ce dont la mise en page dépend : le texte, la largeur, le thème, la ligne de tirets
    // quand le curseur y est, et le texte en cours de composition quand il touche le tableau.
    let touched = lines.get(1).is_some_and(|&(at, line)| (at..=at + line.len()).contains(&cursor));
    let (first, last) = (lines[0].0, lines[lines.len() - 1]);
    let composing = marked.filter(|m| m.start <= last.0 + last.1.len() && m.end >= first);
    let mut hasher = Fast::default();
    (look.style, f32::from(width).to_bits(), touched, composing.map(|m| (m.start - first.min(m.start), m.end)), lines.len()).hash(&mut hasher);
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
    // La ligne de tirets est la deuxième, quand elle n'a que des tirets et des `:`.
    let dash = (lines.len() > 1)
        .then(|| (0..parsed[1].0.len()).map(|c| text(1, c).to_string()).collect::<Vec<_>>())
        .filter(|row| md::is_dashes(row))
        .map(|_| 1);
    let aligns: Vec<TextAlign> =
        (0..cols).map(|c| dash.filter(|_| c < parsed[1].0.len()).map_or(TextAlign::Left, |d| align_of(text(d, c)))).collect();
    let shown_rows: Vec<usize> = (0..lines.len()).filter(|&r| Some(r) != dash).collect();

    // Largeurs : tout le texte sur une ligne, et le plus long mot, de chaque colonne.
    let mut wide = vec![MIN_WIDE; cols];
    let mut narrow = vec![NARROWEST; cols];
    let mut alone = vec![vec![px(0.); cols]; lines.len()];
    for &r in &shown_rows {
        for c in 0..cols {
            let (text, bold) = (text(r, c), r == 0);
            let one = cache.shape(key(text, None, bold, &None, look), || shape(text_system, text, None, bold, &None, look));
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
                // Sous le curseur, elle s'affiche comme du texte : on peut y changer les alignements.
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
                    // Dans le texte en cours de composition (méthode de saisie), relativement à la cellule.
                    let here = marked
                        .filter(|m| m.start <= at + range.end && m.end >= at + range.start)
                        .map(|m| m.start.saturating_sub(at + range.start)..m.end.saturating_sub(at + range.start));
                    // Un texte qui tient sur sa ligne n'est pas coupé : la mesure et le retour
                    // à la ligne de GPUI diffèrent de quelques fractions de pixel.
                    let wrap = (alone[r][c] > widths[c]).then_some(widths[c]);
                    let shape = cache.shape(key(text, wrap, bold, &here, look), || shape(text_system, text, wrap, bold, &here, look));
                    Cell { range, shape, align: aligns[c], width: widths[c], clip: false }
                })
                .collect();
            Some(TableRow {
                lines: cells.iter().map(Cell::lines).max().unwrap_or(1),
                cells,
                xs: xs.clone(),
                header: r == 0,
                // Un trait plus épais sépare l'en-tête du corps du tableau.
                top: if dash.is_some() && r == 2 { LINE * 2. } else { LINE },
                bottom: if Some(r) == last { LINE } else { px(0.) },
                bars,
                written: parsed[r].0.len(),
            })
        })
        .collect()
}

// ───────────────────────── Très gros tableaux ─────────────────────────

/// Au-delà de ce nombre de lignes, un tableau n'est plus mis en page d'un bloc : ses colonnes
/// viennent de ses premières lignes, ses cellules ne passent plus à la ligne (ce qui dépasse
/// est coupé), donc chaque ligne a la même hauteur, et seules les lignes proches de l'écran
/// reçoivent leurs cellules mises en forme. Un tableau de 300 000 lignes s'ouvre aussi vite
/// qu'un de 3 000.
pub const BIG: usize = 1500;
/// Lignes qui fixent les colonnes d'un très gros tableau.
pub const SAMPLE: usize = 300;

/// Les colonnes d'un très gros tableau, communes à toutes ses lignes.
pub struct Geometry {
    cols: usize,
    widths: Vec<Pixels>,
    xs: Rc<Vec<Pixels>>,
    aligns: Vec<TextAlign>,
    /// Indice de la ligne de tirets, quand il y en a une.
    dash: Option<usize>,
}

impl Geometry {
    /// Mêmes colonnes, mêmes alignements, même ligne de tirets.
    pub fn same(&self, other: &Geometry) -> bool {
        self.cols == other.cols
            && self.dash == other.dash
            && self.widths == other.widths
            && *self.xs == *other.xs
            && self.aligns.iter().zip(&other.aligns).all(|(a, b)| std::mem::discriminant(a) == std::mem::discriminant(b))
    }

    /// Largeur du tableau, traits compris.
    pub fn total(&self) -> Pixels {
        self.xs.last().map_or(px(0.), |x| *x + LINE)
    }

    /// Place au-dessus du texte et hauteur de la ligne `r` du tableau, sur une seule ligne
    /// visuelle ; `last` : c'est la dernière. La ligne de tirets n'a pas de hauteur : dans un très gros
    /// tableau, elle reste masquée même sous le curseur.
    pub fn metrics(&self, r: usize, last: bool, lh: Pixels) -> (Pixels, Pixels) {
        if Some(r) == self.dash {
            return (px(0.), px(0.));
        }
        let top = if self.dash.is_some() && r == 2 { LINE * 2. } else { LINE };
        let bottom = if last { LINE } else { px(0.) };
        (top + PAD_Y, top + PAD_Y + lh + PAD_Y + bottom)
    }
}

/// Fixe les colonnes d'un très gros tableau d'après ses premières lignes.
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
    // Pas de retour à la ligne : une colonne prend la largeur de son plus long texte, sans dépasser le tiers
    // de la page (au-delà, c'est coupé), plus une marge minimale.
    let frame = (cols + 1) as f32 * f32::from(LINE) + cols as f32 * 2. * f32::from(PAD_X);
    let cap = ((f32::from(width) - frame) / 3.).max(MIN_WIDE * 2.);
    let widths: Vec<Pixels> = wide.into_iter().map(|w| px(w.min(cap))).collect();
    let mut xs = vec![px(0.)];
    for w in &widths {
        xs.push(xs[xs.len() - 1] + LINE + PAD_X + *w + PAD_X);
    }
    Geometry { cols, widths, xs: Rc::new(xs), aligns, dash }
}

/// La ligne `r` d'un très gros tableau, ses cellules sur une seule ligne.
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
            Cell { range, shape, align: geom.aligns[c], width: geom.widths[c], clip }
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
        // Un signe ou un idéogramme peut commencer une ligne ; un mot avec ses tirets et ses apostrophes non.
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
        // `a\|b` s'affiche `a|b` : le signe `|` vaut deux octets écrits.
        let subs = [(1, 2, 1)];
        assert_eq!((0..=4).map(|i| shown(&subs, i)).collect::<Vec<_>>(), [0, 1, 2, 2, 3]);
        assert_eq!((0..=3).map(|i| source(&subs, i)).collect::<Vec<_>>(), [0, 1, 3, 4]);
    }

    /// Des centaines de tableaux au hasard : les colonnes ne dépassent jamais ce qu'elles
    /// demandent, tiennent dans la page quand c'est possible, et ne descendent pas sous leur plancher.
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
            assert!(total <= avail.max(floor) + 0.01, "cas {case}: {total} pour {avail}");
            for ((w, n), x) in widths.iter().zip(&narrow).zip(&wide) {
                assert!(*w <= x + 0.01, "cas {case}: {w} > {x}");
                assert!(*w >= n.min(NARROWEST).min(*x) - 0.01, "cas {case}: {w} < {n}");
            }
            // Tout tient sur une ligne si la page le permet.
            if wide.iter().sum::<f32>() <= avail {
                assert_eq!(widths, wide, "cas {case}");
            }
            // Si les mots les plus longs tiennent, aucun n'est coupé.
            if narrow.iter().sum::<f32>() <= avail {
                assert!(widths.iter().zip(&narrow).all(|(w, n)| w + 0.01 >= *n), "cas {case}");
            }
        }
    }
}
