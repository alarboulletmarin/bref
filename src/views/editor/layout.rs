//! Layout: lines shaped and placed, redone only where the text changed (`Damage`, `plan`).

use super::*;

/// Ce dont la mise en page dépend, en dehors du texte : si rien n'a changé, on la garde.
#[derive(PartialEq)]
pub(super) struct LayoutKey {
    pub(super) version: u64,
    pub(super) width: u32,
    pub(super) size: u32,
    pub(super) cursor: usize,
    pub(super) marked: Option<Range<usize>>,
}

/// Ce qui a changé dans le texte depuis la dernière mise en page : ses `prefix` premiers octets
/// et ses `suffix` derniers sont restés tels quels.
#[derive(Clone, Copy)]
pub(super) struct Damage {
    pub(super) old_len: usize,
    pub(super) prefix: usize,
    pub(super) suffix: usize,
}

/// La dernière mise en page, pour en reprendre ce qui n'a pas changé.
pub(super) struct Laid {
    pub(super) epoch: u64,
    pub(super) width: u32,
    pub(super) room: u32,
    pub(super) size: u32,
    pub(super) len: usize,
    pub(super) cursor: usize,
    pub(super) fences: usize,
    pub(super) dollars: usize,
    /// Longueur de l'en-tête YAML.
    pub(super) meta: usize,
}

/// Une zone du texte à remettre en page : les lignes de l'ancienne mise en page `old` (indices),
/// le texte qui les remplace, de `start` à `stop` (octets, `stop` inclus : fin de la dernière ligne).
pub(super) struct Region {
    pub(super) old: Range<usize>,
    pub(super) start: usize,
    pub(super) stop: usize,
    /// Dans un très gros tableau : ses colonnes, le rang de la première ligne dans le tableau et l'octet où
    /// le tableau commence.
    pub(super) big: Option<(Rc<grid::Geometry>, usize, usize)>,
    /// La zone touche aux premières lignes, qui fixent les colonnes : elles ne doivent pas avoir changé.
    pub(super) check: bool,
}

/// Ce qu'on garde de la mise en page précédente, et ce qu'on refait.
pub(super) struct Plan {
    pub(super) regions: Vec<Region>,
    /// Comptes de lignes de clôture et de `$$`, inchangés.
    pub(super) fences: usize,
    pub(super) dollars: usize,
    /// Les lignes reprises qui suivent `after` (octets de l'ancien texte) se décalent de `delta`.
    pub(super) after: usize,
    pub(super) delta: isize,
}

/// Reprend `count` lignes de l'ancienne mise en page, posées à partir de `y`, leur texte décalé de `delta`.
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

/// Un très gros tableau : ses lignes ne reçoivent leurs cellules que près de l'écran.
pub(super) struct BigBlock {
    /// Indices de ses lignes.
    pub(super) rows: Range<usize>,
    pub(super) geom: Rc<grid::Geometry>,
    /// Parties de ses lignes (indices dans le tableau) qui ont leurs cellules.
    pub(super) mat: Vec<Range<usize>>,
    /// Ses lignes viennent d'une autre mise en page : on ne sait plus lesquelles ont leurs cellules.
    pub(super) rescan: bool,
}

/// Un tableau de la note.
/// Où sont à l'écran les traits d'un tableau, pour ses poignées et ce qu'on y a pris.
pub(super) struct Geo {
    /// Début du tableau dans le texte.
    pub(super) start: usize,
    pub(super) frame: Bounds<Pixels>,
    /// Abscisses des traits verticaux (une de plus que de colonnes), ordonnées des horizontaux
    /// (une de plus que de lignes vues : l'en-tête, puis le corps).
    pub(super) xs: Vec<Pixels>,
    pub(super) ys: Vec<Pixels>,
}

/// Une poignée d'un tableau : une colonne, une ligne, l'angle (tout le tableau), ou le « + » d'un trait.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Part {
    Col(usize),
    Row(usize),
    Corner,
    /// Le trait avant la colonne ou la ligne `n` : un clic y insère.
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

    /// La colonne et la ligne au-dessus ou à côté desquelles est `at`, jusqu'à `GRIP_REACH` du tableau.
    pub(super) fn near(&self, at: Point<Pixels>) -> Near {
        let f = self.frame;
        // Le dernier trait compte pour la dernière colonne (ou ligne) : son « + » s'y montre.
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

    /// Les poignées et leur zone, dans l'ordre où l'on cherche celle qui reçoit un clic.
    pub(super) fn parts(&self) -> Vec<(Part, Bounds<Pixels>)> {
        let f = self.frame;
        let shown = |x: Pixels| x >= f.left() - px(1.) && x <= f.right() + px(1.);
        // L'angle d'abord : il l'emporte sur le « + » du premier trait, qui le frôle.
        let mut parts = vec![(Part::Corner, Bounds::new(f.origin - point(GRIP + GRIP_GAP, GRIP + GRIP_GAP), size(GRIP, GRIP)))];
        for (b, &x) in self.xs.iter().enumerate().filter(|(_, x)| shown(**x)) {
            parts.push((Part::JoinCol(b), Bounds::new(point(x - px(6.), f.top() - px(18.)), size(px(12.), px(16.)))));
        }
        for (b, &y) in self.ys.iter().enumerate().skip(1) {
            parts.push((Part::JoinRow(b), Bounds::new(point(f.left() - px(18.), y - px(6.)), size(px(16.), px(12.)))));
        }
        // Une pastille au milieu de sa colonne (ou de sa ligne), plus petite si la place manque.
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

    /// La poignée `part` se montre : le pointeur est sur sa colonne ou sa ligne, ou elle tient ce qu'on a pris.
    pub(super) fn visible(&self, part: Part, near: Near, pick: Option<&Pick>) -> bool {
        let (all_rows, all_cols) = pick.map_or((false, false), |p| (p.rows.len() == self.tall(), p.cols.len() == self.cols()));
        let held = |hold: bool, n: usize, range: Option<&Range<usize>>| hold && range.is_some_and(|r| r.contains(&n));
        match part {
            Part::Col(c) => near.col == Some(c) || held(all_rows, c, pick.map(|p| &p.cols)),
            Part::Row(r) => near.row == Some(r) || held(all_cols, r, pick.map(|p| &p.rows)),
            Part::Corner => near.corner || (all_rows && all_cols),
            // Un « + » ne se montre que sous le pointeur : sa zone, près des poignées, suffit.
            Part::JoinCol(_) | Part::JoinRow(_) => near.col.is_some() || near.row.is_some() || near.corner,
        }
    }

    /// Le trait le plus proche de `at`, parmi ceux où une colonne (ou une ligne) peut se poser.
    pub(super) fn boundary(&self, part: Part, at: Point<Pixels>) -> usize {
        let (lines, at, first) = match part {
            Part::Col(_) => (&self.xs, at.x, 0),
            _ => (&self.ys, at.y, 1),
        };
        (first..lines.len()).min_by_key(|&b| (f32::from((lines[b] - at).abs()) * 100.) as i64).unwrap_or(first)
    }

    /// Les cases `rows` × `cols`, en pixels.
    pub(super) fn cells(&self, rows: &Range<usize>, cols: &Range<usize>) -> Bounds<Pixels> {
        let (x0, x1) = (self.xs[cols.start.min(self.cols())], self.xs[cols.end.min(self.cols())]);
        let (y0, y1) = (self.ys[rows.start.min(self.tall())], self.ys[rows.end.min(self.tall())]);
        Bounds::new(point(x0, y0), size(x1 - x0, y1 - y0))
    }
}

/// Une ligne logique mise en page (elle peut occuper plusieurs lignes visuelles).
pub(super) struct Row {
    pub(super) start: usize,
    pub(super) len: usize,
    pub(super) y: Pixels,
    pub(super) pad: Pixels,
    pub(super) lh: Pixels,
    pub(super) height: Pixels,
    pub(super) kind: Kind,
    /// Son texte mis en forme, partagé avec le cache des lignes.
    pub(super) shaped: Rc<Shaped>,
    /// Image désignée par la ligne, ou figure (diagramme, formule) qu'elle
    /// termine, dessinée dessous, et sa taille à l'écran.
    pub(super) image: Option<(Arc<RenderImage>, Size<Pixels>)>,
    /// La ligne ouvre un bloc de code : elle porte le bouton de copie.
    pub(super) opens: bool,
    /// Place laissée sous la dernière ligne d'un tableau.
    pub(super) gap: Pixels,
    /// Couleur du panneau `> [!TYPE]` dont la ligne fait partie.
    pub(super) tint: Option<Hsla>,
    /// Ligne de tableau plus large que la page : de combien son texte est décalé vers la gauche.
    pub(super) dx: Pixels,
    /// Ligne de tableau affichée en grille : ses cellules, qui passent à la ligne.
    pub(super) grid: Option<Rc<TableRow>>,
}

impl Row {
    /// Position à l'écran, par rapport au bord gauche de la page.
    pub(super) fn pos(&self, i: usize) -> Point<Pixels> {
        let p = match &self.grid {
            Some(grid) => grid.pos(i.min(self.len), self.lh),
            None => self
                .shaped
                .line
                .as_ref()
                .and_then(|l| l.position_for_index(self.shown(i.min(self.len)), self.lh))
                .unwrap_or_default(),
        };
        point(p.x - self.dx, p.y)
    }

    /// Octet du texte → octet du texte affiché.
    pub(super) fn shown(&self, i: usize) -> usize {
        grid::shown(&self.shaped.subs, i)
    }

    /// Octet du texte affiché → octet du texte.
    pub(super) fn source(&self, i: usize) -> usize {
        grid::source(&self.shaped.subs, i)
    }

    pub(super) fn image_height(&self) -> Pixels {
        self.image.as_ref().map_or(px(0.), |(_, s)| s.height + IMAGE_GAP)
    }

    pub(super) fn index_at(&self, p: Point<Pixels>) -> usize {
        let p = point(p.x + self.dx, p.y);
        if let Some(grid) = &self.grid {
            return grid.index_at(p, self.lh);
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
            None => (f32::from(self.height - self.pad - self.image_height() - self.gap) / f32::from(self.lh)).round() as i32,
        }
    }

    /// Lignes visuelles de ce qui contient l'octet `i` : la ligne, ou la cellule d'un tableau.
    pub(super) fn lines_at(&self, i: usize) -> i32 {
        self.grid.as_ref().map_or_else(|| self.visual_rows(), |grid| grid.lines_at(i))
    }

    /// Ligne de tirets d'un tableau, qu'on ne voit pas.
    pub(super) fn hidden(&self) -> bool {
        self.grid.as_ref().is_some_and(|g| g.hidden())
    }

    /// Largeur de ce qui est dessiné.
    pub(super) fn content_width(&self) -> Pixels {
        match &self.grid {
            Some(grid) => grid.total(),
            None => self.shaped.line.as_ref().map_or(px(0.), |l| l.width()),
        }
    }
}

/// Empreinte rapide (FxHash, brassée à la fin) : chaque ligne de la note est hachée à chaque
/// image, et SipHash, celui de la bibliothèque standard, y passait un tiers du temps. Les
/// clés ne viennent que de la note : pas besoin de résister à qui les choisirait.
#[derive(Default)]
pub struct Fast(u64);

impl Fast {
    pub(super) fn add(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
}

impl Hasher for Fast {
    fn write(&mut self, bytes: &[u8]) {
        let mut words = bytes.chunks_exact(8);
        for word in &mut words {
            self.add(u64::from_le_bytes(word.try_into().unwrap()));
        }
        let rest = words.remainder();
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

/// La mise en forme de la ligne d'empreinte `id`, reprise de celles gardées, ou faite par `make`.
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

/// Ce qu'une ligne affiche, tant que ni elle ni son contexte ne changent.
pub(super) struct Shaped {
    pub(super) line: Option<WrappedLine>,
    /// Signes affichés à la place de leur écriture ASCII : (octet de début,
    /// longueur dans le texte, longueur affichée).
    pub(super) subs: Vec<(usize, usize, usize)>,
    /// Formule sur la ligne : son source, et si elle est hors texte.
    pub(super) formula: Option<(String, bool)>,
    /// Chemin de l'image que la ligne désigne.
    pub(super) picture: Option<String>,
}

/// Figure prête à dessiner (`None` : source invalide) et sa taille.
pub(super) type Drawing = Option<(Arc<Image>, Size<Pixels>)>;

/// Les figures dessinées, par clé ; chacune avec la clé de sa source seule (sans le thème),
/// pour montrer l'ancienne version pendant que la nouvelle se dessine.
pub(super) type Figures = FastMap<u64, (u64, Drawing)>;

/// La figure de clé `key`, reprise de la frame précédente ou dessinée par `make` ; si `lazy`,
/// seulement reprise. Tant que `make` n'a rien (dessin en tâche de fond), la version d'avant
/// de la même source (`family`), s'il y en a une.
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

/// Ce qui, dans le style d'une ligne, vient de sa nature plutôt que de son texte.
pub struct RunStyle {
    pub heading: bool,
    pub italic: bool,
    pub mono: bool,
    /// Couleur du panneau dont la ligne fait partie.
    pub tint: Option<Hsla>,
}

/// Les styles de la ligne `flags` (un mot de drapeaux par octet), d'un seul tenant
/// tant que rien ne change ; `marked` : l'octet est dans le texte en cours de composition.
pub fn text_runs(t: &Theme, flags: &[u16], marked: impl Fn(usize) -> bool, style: &RunStyle) -> Vec<TextRun> {
    // ponytail: chaînes et nombres prennent la couleur d'accent décalée en teinte,
    // pour s'accorder à chaque thème sans palette dédiée ; donner ses couleurs de
    // code à chaque thème si l'une d'elles jure.
    let hue = |by: f32| Hsla { h: (t.accent.h + by).fract(), ..t.accent };
    let mut runs: Vec<TextRun> = Vec::new();
    let mut i = 0;
    while i < flags.len() {
        let (f, m) = (flags[i], marked(i));
        let mut j = i + 1;
        while j < flags.len() && flags[j] == f && marked(j) == m {
            j += 1;
        }
        let color = if f & md::DIM != 0 {
            t.dim
        } else if f & md::STRING != 0 {
            hue(0.33)
        } else if f & md::NUMBER != 0 {
            hue(0.6)
        } else if f & md::MARK != 0 && let Some(tint) = style.tint {
            // Sur le fond du panneau, sa teinte à 10 % : le titre se lit dans tous les thèmes.
            crate::ui::theme::legible(tint, crate::ui::theme::blend(tint, t.bg, 0.1))
        } else if f & (md::LINK | md::TAG | md::MARK | md::KEYWORD) != 0 {
            t.accent
        } else {
            t.text
        };
        runs.push(TextRun {
            len: j - i,
            font: Font {
                weight: if style.heading || f & md::BOLD != 0 { FontWeight::BOLD } else { FontWeight::NORMAL },
                style: if f & md::ITALIC != 0 || style.italic { FontStyle::Italic } else { FontStyle::Normal },
                ..font(if style.mono || f & md::CODE != 0 { mono() } else { sans() })
            },
            color,
            background_color: (f & md::CODE != 0).then_some(t.code_bg),
            // Un seul trait pour tout le lien, crochets compris : gpui 0.2 laisse
            // un bout de trait dans la marge si le soulignement change de style
            // juste à un retour à la ligne.
            underline: (m || f & (md::LINK | md::UNDER) != 0).then_some(UnderlineStyle {
                thickness: px(1.),
                color: Some(if f & md::LINK != 0 { t.accent } else { color }.opacity(if f & md::UNDER != 0 { 1. } else { 0.5 })),
                wavy: false,
            }),
            strikethrough: (f & md::STRIKE != 0).then_some(StrikethroughStyle { thickness: px(1.), color: Some(color) }),
        });
        i = j;
    }
    runs
}

impl Editor {
    /// Mises en page complètes et mises en page reprises, depuis le début.
    #[cfg(test)]
    pub fn layouts(&self) -> (usize, usize, usize) {
        self.counts
    }

    /// Les zones à remettre en page, quand le reste de la dernière mise en page peut être repris :
    /// le texte touché, la ligne où était le curseur et celle où il est, élargies aux blocs qui
    /// doivent rester entiers (tableau, citation). `None` : tout refaire.
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
        // ponytail: l'en-tête YAML change le sens de toutes ses lignes dès qu'il s'ouvre ou se
        // ferme ; une frappe dedans, ou qui déplace sa fin, refait donc toute la mise en page.
        // Le traiter comme une zone à part si de longues notes à en-tête font attendre.
        let meta = md::front_matter(&self.content);
        if meta != laid.meta || self.dmg.is_some_and(|d| d.prefix < meta) {
            return None;
        }
        // Texte touché, en octets de l'ancienne mise en page (`p..=q`), et son décalage.
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
        // Le curseur d'avant et celui d'après : leur ligne ne s'affiche pas pareil.
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
        // Blocs à ne pas couper : un tableau tient en un morceau (sauf l'intérieur d'un très gros),
        // une citation ne perd pas la teinte de son panneau.
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
        // Octets du nouveau texte d'une zone : le début ne bouge que s'il est après le texte touché ; la
        // fin, si elle est au bout du texte touché ou après.
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
                            // Dans l'intérieur d'un très gros tableau, une ligne ne dépend pas des autres.
                            Some(b) if i + 1 < b.rows.end => {}
                            _ => whole(r, block_of(i)),
                        },
                        _ => {}
                    }
                }
                // Un très gros tableau est découpé en lignes indépendantes, hors de ses premières.
                let inside = |i: usize, j: usize| big_of(i).is_some_and(|b| b.rows.contains(&j) && i + 1 < b.rows.end);
                if r.start > 0 && rows[r.start - 1].kind == Kind::Table && !inside(r.start, r.start - 1) {
                    whole(r, block_of(r.start - 1));
                }
                // Le nouveau texte peut, lui aussi, se coller à un tableau ou à une citation voisins.
                let (start, stop) = bounds(r);
                if start > stop || stop > len {
                    return None;
                }
                let text = &self.content[start..stop];
                let (first, last) = (text.split('\n').next().unwrap_or(""), text.rsplit('\n').next().unwrap_or(""));
                if r.end < n && rows[r.end].kind == Kind::Table && md::is_table_line(last) && !inside(r.end - 1, r.end) {
                    whole(r, block_of(r.end));
                }
                let quote = |l: &str| md::classify(l, false).0 == Kind::Quote;
                if r.start > 0 && rows[r.start - 1].kind == Kind::Quote && quote(first) {
                    r.start -= 1;
                }
                if r.end < n && rows[r.end].kind == Kind::Quote && quote(last) {
                    r.end += 1;
                }
                while r.start > 0 && rows[r.start].kind == Kind::Quote && rows[r.start - 1].kind == Kind::Quote {
                    r.start -= 1;
                }
                while r.end < n && rows[r.end - 1].kind == Kind::Quote && rows[r.end].kind == Kind::Quote {
                    r.end += 1;
                }
                // La ligne qui suit un tableau lui donne sa marge : elle se refait avec lui, qu'il le soit
                // depuis toujours ou qu'il le devienne.
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
            // Dans l'intérieur d'un très gros tableau.
            let block = big_of(r.start).filter(|b| r.end < b.rows.end);
            let big = block.map(|b| (b.geom.clone(), r.start - b.rows.start, rows[b.rows.start].start));
            let check = block.is_some_and(|b| r.start < b.rows.start + grid::SAMPLE);
            // Le nouveau texte ne doit rien changer au contexte des lignes reprises : ni clôture, ni `$$`,
            // et, dans un très gros tableau, que des lignes de tableau.
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
            // Un tableau qui repasse sous le seuil se met en page comme un petit : on refait tout.
            if let Some(b) = block.filter(|_| big.is_some()) && b.rows.len() + lines <= r.len() + grid::BIG {
                return None;
            }
            regions.push(Region { old: r, start, stop, big, check });
        }
        Some(Plan { regions, fences: laid.fences, dollars: laid.dollars, after: if touched { q } else { usize::MAX }, delta })
    }

    /// Ce qui décrit chaque ligne mise en page, pour comparer deux mises en page.
    #[cfg(test)]
    #[allow(clippy::type_complexity)]
    pub(super) fn signature(&self) -> Vec<(usize, usize, i64, i64, i64, bool, Kind, i64)> {
        let px = |v: Pixels| (f32::from(v) * 100.).round() as i64;
        self.rows.iter().map(|r| (r.start, r.len, px(r.y), px(r.height), px(r.pad), r.opens, r.kind, px(r.gap))).collect()
    }

    /// Chaque image : la mise en page lourde n'est refaite que si le texte, la largeur, le thème
    /// ou le curseur ont changé ; sinon, seul ce qui dépend du défilement est recalculé.
    pub(super) fn layout(&mut self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
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
            // Les tests refont toute la mise en page à côté, pour y retrouver celle qu'on a reprise.
            #[cfg(test)]
            if self.compare && !self.force_full {
                let reused = self.signature();
                // Tout tableau de plus de `grid::BIG` lignes est enregistré comme très gros, et lui seul.
                let mut i = 0;
                while i < self.rows.len() {
                    let n = self.rows[i..].iter().take_while(|r| r.kind == Kind::Table).count();
                    if n > 0 {
                        let registered = self.big.iter().any(|b| b.rows == (i..i + n));
                        assert_eq!(registered, n > grid::BIG, "tableau de {n} lignes à la ligne {i}, registre {:?}", self.big.iter().map(|b| b.rows.clone()).collect::<Vec<_>>());
                    }
                    i += n.max(1);
                }
                let (laid, big, counts) = (self.laid.take(), std::mem::take(&mut self.big), self.counts);
                self.force_full = true;
                self.relayout(bounds, window, cx);
                self.force_full = false;
                self.counts = counts;
                assert_eq!(reused.len(), self.rows.len(), "nombre de lignes");
                // Les positions s'additionnent en flottants : l'ordre des additions change le dernier chiffre.
                let near = |a: i64, b: i64| (a - b).abs() <= 20;
                for (i, (a, b)) in reused.iter().zip(self.signature()).enumerate() {
                    assert!(
                        a.0 == b.0 && a.1 == b.1 && near(a.2, b.2) && near(a.3, b.3) && near(a.4, b.4) && a.5 == b.5 && a.6 == b.6 && near(a.7, b.7),
                        "ligne {i} : {a:?} contre {b:?}"
                    );
                }
                let _ = (laid, big);
            }
            // Une image qui charge encore change la hauteur de sa ligne : on y reviendra.
            self.laid_out = (!self.loading).then_some(key);
        }
        self.place(bounds, window, cx);
    }

    // ponytail: la mise en page reprend les lignes qui n'ont pas bougé (`plan`) et ne refait que les zones
    // touchées. Tout est refait quand le contexte change (thème, autre note, bloc de code ou `$$` touché, texte en
    // composition) : la frappe dans une note qui contient un bloc de code ou des formules reste proportionnelle à
    // sa taille. Reprendre le contexte des blocs de code, comme pour les citations, si cela se remarque.
    pub(super) fn relayout(&mut self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        let t = self.theme;
        let cursor = self.cursor();
        let width = (bounds.size.width - SIDE * 2.).min(t.page.measure(t.size)).max(px(40.));
        let room = table_room(t.wide, bounds.size.width, width);
        let text_system = window.text_system().clone();
        // Ce qui peut être repris de la mise en page précédente, quand seules quelques lignes ont changé.
        // Les premières lignes d'un très gros tableau fixent ses colonnes : si elles bougent, on refait tout.
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
        // Un ``` sans clôture n'ouvre pas de bloc : sinon chaque ``` tapé ferait
        // basculer en code, donc remettre en forme, toute la suite de la note. S'il
        // y en a un de trop, c'est celui qu'on est en train de taper, sinon le dernier.
        let (mut fences, mut dollars) = match &plan {
            Some(plan) => (plan.fences, plan.dollars),
            None => md::count_fences_and_dollars(&self.content),
        };
        let (fences0, dollars0) = (fences, dollars);
        let typed = self.line_range(cursor);
        let orphan = (plan.is_none() && fences % 2 == 1 && md::is_fence(&self.content[typed.clone()]))
            .then_some(typed.start);
        fences -= orphan.is_some() as usize;
        // Une police sans `≠` le compose d'un `=` et d'une barre mal placée : autant
        // laisser `!=`.
        let has_unequal = text_system
            .advance(text_system.resolve_font(&font(sans())), px(16.), '≠')
            .is_ok();
        let mut lang = String::new();
        // Diagrammes Mermaid et formules LaTeX : dessinés une fois, repris ensuite.
        let rgb = |c: Hsla| {
            let c = Rgba::from(c);
            [c.r, c.g, c.b].map(|v| (v * 255.) as u8)
        };
        let hex = |c: Hsla| {
            let [r, g, b] = rgb(c);
            format!("#{r:02x}{g:02x}{b:02x}")
        };
        let look = figure::Look::of(&t);
        // La clé d'une figure, et celle de sa source seule.
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
        // Les figures se dessinent à la plus grande largeur de la page, pour cet écran.
        let job = {
            let (room, density, bg) = (f32::from(t.page.measure(t.size)), window.scale_factor(), rgb(t.bg));
            move |(key, family): (u64, u64), make: Arc<dyn Fn() -> Option<String> + Send + Sync>| Job { key, family, make, room, density, bg }
        };
        let vector = |figure: Option<figure::Figure>| {
            Some(figure.map(|(svg, w, h)| (Arc::new(Image::from_bytes(ImageFormat::Svg, svg.into_bytes())), size(px(w), px(h)))))
        };
        let mut cells = std::mem::take(&mut self.cells);
        cells.next_frame();
        // Lignes d'un tableau déjà mises en page, en attente de leur tour dans la boucle.
        let mut pending: VecDeque<Option<Rc<TableRow>>> = VecDeque::new();
        let empty = Rc::new(Shaped { line: None, subs: Vec::new(), formula: None, picture: None });
        let mut shaped = std::mem::take(&mut self.shaped);
        self.frame = self.frame.wrapping_add(1);
        let frame = self.frame;
        let mut old = std::mem::take(&mut self.figures);
        let (mut waiting, before) = (Vec::new(), std::mem::take(&mut self.waiting));
        let mut kept = FastMap::default();
        // Bloc Mermaid ou `$$` en cours : son début et son source.
        let mut block: Option<(usize, String)> = None;
        let marked = self.marked.clone();
        let mut rows = Vec::with_capacity(self.rows.len() + 16);
        let mut y = px(0.);
        let mut offset;
        let mut in_code = false;
        // En-tête YAML : ses lignes s'écrivent estompées, à chasse fixe, sans être interprétées.
        let meta = md::front_matter(&self.content);
        // Couleur d'un panneau `[!TYPE]` : sa teinte dit son type.
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
        // Très gros tableaux repris tels quels : leurs colonnes, et la place de leur première ligne.
        let mut carried: Vec<(Rc<grid::Geometry>, usize)> = Vec::new();
        let mut next_old = 0;
        let mut regions = regions.into_iter();
        // Les lignes reprises : d'abord celles qui précèdent la zone, puis, à la fin, celles qui la suivent.
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
            // La zone remplace ses anciennes lignes.
            old_rows.by_ref().take(region.old.len()).for_each(drop);
            next_old = region.old.end;
            offset = region.start;
            tint = None;
            let first_new = rows.len();
            // Zone qui commence à la première ligne d'un très gros tableau : le tableau reste enregistré.
            if let Some((geom, 0, _)) = &region.big {
                carried.push((geom.clone(), first_new));
            }
            while offset <= region.stop {
            let line = &self.content[offset..self.content[offset..].find('\n').map_or(text_len, |i| offset + i)];
            // Dans l'intérieur d'un très gros tableau, chaque ligne se suffit : même hauteur, sans cellules.
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
                    opens: false,
                    gap: px(0.),
                    tint: None,
                    dx: px(0.),
                    grid: None,
                });
                y += height;
                offset += line.len() + 1;
                continue;
            }
            let (mut kind, mut marker) = if offset < meta { (Kind::Meta, 0) } else { md::classify(line, in_code) };
            let mut opens = false;
            let mut drawing = None;
            if kind == Kind::Fence && orphan != Some(offset) {
                opens = !in_code && fences > 1;
                in_code = opens;
                fences -= 1;
                if opens {
                    lang = line.trim_start()[3..].trim().to_lowercase();
                    block = (lang == "mermaid").then(|| (offset, String::new()));
                } else if let Some((start, source)) = block.take() {
                    // Un diagramme se dessine une fois le curseur sorti du bloc, en tâche de fond.
                    let editing = (start..=offset + line.len()).contains(&cursor);
                    let (k, look) = (key(0, &source), look.clone());
                    drawing = cached(&mut old, &mut kept, k, editing, || {
                        drawn(job(k, Arc::new(move || figure::mermaid(&source, &look))), &before, &mut waiting, window, cx)
                    });
                }
            }
            let code = matches!(kind, Kind::Code | Kind::Fence | Kind::Meta);
            // Formule sur plusieurs lignes, entre deux lignes `$$`.
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
                        drawing = cached(&mut old, &mut kept, key(1, &source), false, || {
                            vector(figure::latex(&source, true, rgb(t.text), t.size * MATH_SCALE))
                        });
                    }
                    // Comme pour ``` : un `$$` sans clôture n'ouvre rien.
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
            // Panneau : la citation `> [!TYPE]` et celles qui la suivent.
            let own_tint = (kind == Kind::Quote).then(|| md::callout(line)).flatten().map(|r| tint_of(&line[r]));
            tint = own_tint.or(tint).filter(|_| kind == Kind::Quote);
            let tinted = tint.is_some();
            let last: Option<&mut Row> = rows.last_mut();
            let after_table = last.as_ref().is_some_and(|r| r.kind == Kind::Table);
            let header = kind == Kind::Table && !after_table;
            // Un tableau se met en page d'un bloc, à sa première ligne : ses colonnes
            // dépendent de toutes ses cellules.
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
                // Un très gros tableau : lignes toutes de la même hauteur, sans mise en forme encore.
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
                            opens: false,
                            gap: px(0.),
                            tint: None,
                            dx: px(0.),
                            grid: None,
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
            let (font_size, pad) = match kind {
                Kind::Heading(1) => (px(27.), px(14.)),
                Kind::Heading(2) => (px(21.), px(10.)),
                Kind::Heading(_) => (px(17.5), px(6.)),
                Kind::Code | Kind::Fence | Kind::Table | Kind::Meta => (px(14.), px(0.)),
                _ => (px(16.), px(0.)),
            };
            // Les tailles ci-dessus valent pour un texte courant de 16 px.
            let font_size = font_size * (t.size / 16.);
            let pad = if offset == 0 { px(0.) } else { pad };

            // Une ligne inchangée, dans le même contexte, reprend sa mise en forme de
            // la frame précédente : la frappe ne retraite que la ligne touchée.
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
            let lh = (font_size * 1.65).round();
            let made = if grid_row.is_some() { empty.clone() } else { cached_shape(&mut shaped, id, frame, || {
                let mut formula = None;
                let mut flags = vec![0u16; line.len()];
                let list = matches!(kind, Kind::Bullet | Kind::Ordered | Kind::Task(_));
                flags[..marker].fill(if list { md::MARK } else { md::DIM });
                if kind == Kind::Meta {
                    flags.fill(md::DIM);
                } else if math {
                    flags.fill(if line.trim() == "$$" { md::DIM } else { md::CODE });
                } else if kind == Kind::Code {
                    // Sans langage annoncé, en texte brut ou en Mermaid, le bloc reste tel quel.
                    if !matches!(lang.as_str(), "" | "text" | "txt" | "plain" | "mermaid") {
                        md::code(line, &lang, &mut flags);
                    }
                } else if !code && kind != Kind::Rule {
                    md::inline(line, marker, &mut flags);
                    // Formule sur la ligne : son source reste lisible, comme du code.
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
                    // Cochée, sa case est en gras, comme une case dans le texte ou un tableau.
                    if let Some(at) = line[..marker].find('[') {
                        flags[at..(at + 3).min(marker)].iter_mut().for_each(|f| *f |= md::BOLD);
                    }
                }
                if let Some(label) = md::callout(line).filter(|_| own_tint.is_some()) {
                    flags[label].fill(md::BOLD | md::MARK);
                }
                // Tableau : en-tête en gras, `|` et tirets en retrait.
                // Une ligne ne passe jamais à la ligne, ce qui casserait la grille : un
                // tableau plus large que la page défile horizontalement.
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
                // Hors de la ligne du curseur, `->`, `!=`… s'affichent comme des signes ;
                // le texte, lui, ne change pas.
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
                    italic: kind == Kind::Quote && !tinted,
                    mono: code || kind == Kind::Table,
                    tint: own_tint,
                };
                let runs = text_runs(&t, &flags, is_marked, &style);

                let line = window
                    .text_system()
                    .shape_text(shown.into_owned().into(), font_size, &runs, (kind != Kind::Table).then_some(width), None)
                    .ok()
                    .and_then(|lines| lines.into_iter().next());
                Rc::new(Shaped { line, subs, formula, picture })
            }) };
            if let Some((source, display)) = &made.formula {
                drawing = cached(&mut old, &mut kept, key(1 + *display as u8, source), false, || {
                    vector(figure::latex(source, *display, rgb(t.text), t.size * MATH_SCALE))
                });
            }
            // ponytail: une image par ligne, cherchée dans le dossier de la note, à la
            // racine du coffre, puis par son nom dans tout le coffre ; les images en
            // ligne (http) ne sont pas chargées.
            let file = made.picture.as_ref().and_then(|path| {
                let root = self.dirs.last();
                let found = self.dirs.iter().map(|d| d.join(path)).find(|p| p.is_file() && root.is_some_and(|r| inside(r, p)));
                found.or_else(|| {
                    let name = path.rsplit('/').next()?.to_lowercase();
                    self.images.get(&name).cloned()
                })
            });
            let svg = file.as_ref().is_some_and(|f| f.extension().is_some_and(|e| e.eq_ignore_ascii_case("svg")));
            // Un SVG est redessiné aux couleurs du thème, et à chaque fois que son fichier
            // change : sa date fait partie de sa clé. Un schéma de Bref prend l'encre du
            // texte ; un SVG d'ailleurs est rendu lisible (texte HTML, `light-dark()`).
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
                drawing = own.or(drawing);
            }
            let image = file.filter(|_| drawing.is_none() && !svg).and_then(|file| {
                let loaded = window.use_asset::<ImgResourceLoader>(&Resource::Path(file.into()), cx);
                loading |= loaded.is_none();
                let image = loaded?.ok()?;
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

            let text_height = made.line.as_ref().map_or(lh, |l| l.size(lh).height);
            let (pad, lh, height) = match &grid_row {
                Some(grid) => {
                    let lh = grid::line_height(&t);
                    (grid.pad(), lh, grid.height(lh))
                }
                None => (pad, lh, pad + text_height),
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
                opens,
                gap: px(0.),
                tint,
                dx: px(0.),
                grid: grid_row,
            };
            let height = row.height + row.image_height();
            rows.push(Row { height, ..row });
            y += height;
            offset += line.len() + 1;
            }
        }
        if let Some(last) = rows.last_mut().filter(|r| r.kind == Kind::Table && r.gap == px(0.)) {
            last.gap = TABLE_GAP;
            last.height += TABLE_GAP;
            y += TABLE_GAP;
        }
        self.rows = rows;
        // Les très gros tableaux repris : leurs lignes ne sont plus celles qu'on a mises en forme.
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
        // Une figure qui n'est plus demandée (autre thème, source changée) n'est plus attendue.
        for job in before {
            if !waiting.iter().any(|w| w.id() == job.id()) {
                cx.remove_asset::<Job>(&job);
            }
        }
        self.loading = loading || !waiting.is_empty();
        self.waiting = waiting;
        // Les figures qui ne servent plus libèrent leur image et sa place sur la carte graphique,
        // sauf si une figure gardée a la même (même source, revenue au même thème).
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
        // Les lignes qui ne servent plus (supprimées, modifiées) partent quand elles dépassent la note.
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

    /// Donne leurs cellules aux lignes des très gros tableaux qui sont près de l'écran ou du curseur,
    /// et les retire à celles qui s'en sont éloignées.
    pub(super) fn materialize(&mut self, bounds: Bounds<Pixels>, window: &mut Window) {
        if self.big.is_empty() {
            return;
        }
        let (cursor, t) = (self.cursor(), self.theme);
        let at = self.row_at(cursor);
        let look = grid::Look { theme: &t, style: self.big_style };
        let (page, top) = (bounds.size.height, self.scroll_y - TOP);
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
                // Lignes venues d'une autre mise en page : on ne sait plus lesquelles ont leurs cellules.
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

    /// Ce qui dépend du défilement et de la taille de la fenêtre, et se refait à chaque image :
    /// vue, cadres des tableaux, zones cliquables.
    pub(super) fn place(&mut self, bounds: Bounds<Pixels>, window: &mut Window, _: &mut App) {
        let (cursor, width, room) = (self.cursor(), self.width, self.room);
        let reveal = std::mem::take(&mut self.reveal);
        let to_top = std::mem::take(&mut self.reveal_top);
        if reveal {
            let c = self.cursor();
            if let Some(r) = self.row_at(c) {
                let row = &self.rows[r];
                let top = TOP + row.y + row.pad + row.pos(c - row.start).y;
                let margin = px(24.);
                if to_top || top - self.scroll_y < margin {
                    self.scroll_y = top - margin;
                } else if top + row.lh - self.scroll_y > bounds.size.height - margin {
                    self.scroll_y = top + row.lh - bounds.size.height + margin;
                }
            }
        }
        let max_scroll = (TOP + self.total_y - bounds.size.height * 0.4).max(px(0.));
        self.scroll_y = self.scroll_y.max(px(0.)).min(max_scroll);
        self.origin = point(
            bounds.left() + (bounds.size.width - width) / 2.,
            bounds.top() + TOP - self.scroll_y,
        );
        self.viewport = bounds;
        self.materialize(bounds, window);
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
        let grid = self.grid.and_then(|_| self.popup_at(Self::grid_size()));
        self.grid_hitbox = grid.map(|at| window.insert_hitbox(Bounds::new(at, Self::grid_size()), HitboxBehavior::Normal));
        // Cadre de chaque tableau : ses lignes, à la largeur de la plus longue sans
        // dépasser la page ; au-delà, il défile et suit le curseur.
        let scrolled = std::mem::take(&mut self.table_x);
        self.tables.clear();
        self.geos.clear();
        self.thumbs.clear();
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
                // Là où serait le curseur sans défilement.
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
            // Un très gros tableau n'a pas de poignées : ses gestes ne le réécrivent pas d'un bloc.
            let big = self.big.iter().any(|b| b.rows.start == meta.rows.start);
            if let Some(grid) = first.grid.as_ref().filter(|_| !big) {
                let xs = grid.xs().iter().map(|x| frame.left() - dx + *x).collect();
                let lines = self.rows[meta.rows.clone()].iter().filter(|r| !r.hidden()).map(|r| self.origin.y + r.y);
                let mut ys: Vec<Pixels> = std::iter::once(frame.top()).chain(lines.skip(1)).collect();
                ys.push(frame.bottom());
                self.geos.push(Geo { start, frame, xs, ys });
            }
        }
        let bars = self.tables.iter().flat_map(|(_, frame)| plus_bars(*frame));
        self.table_hitboxes = bars.map(|bar| window.insert_hitbox(bar, HitboxBehavior::Normal)).collect();
        self.handles.hitboxes = self
            .handle_bounds()
            .map(|(grip, plus)| (window.insert_hitbox(grip, HitboxBehavior::Normal), plus.map(|plus| window.insert_hitbox(plus, HitboxBehavior::Normal))));
    }
}
