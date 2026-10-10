//! Tableau : un fichier CSV ou TSV ouvert dans une grille qu'on lit et qu'on écrit
//! directement. Le délimiteur et l'encodage se détectent, se changent et se retiennent
//! par fichier. Seules les cellules visibles sont construites ; l'analyse du fichier vit
//! dans `table`.

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
    Theme,
    line::{self, Line},
    markdown, mono,
    nav::{button, tip},
    table::{self, Encoding, Format, Style, Table, WriteError},
    tr, vault,
};

actions!(
    sheet,
    [
        Left, Right, Up, Down, ExtendLeft, ExtendRight, ExtendUp, ExtendDown, PageUp, PageDown, RowStart, RowEnd,
        Origin, Corner, FirstRow, LastRow, Next, Previous, Enter, Edit, Cancel, Backspace, Delete, Copy, Cut,
        Paste, SelectAll, Undo, Redo, InsertBelow, InsertAbove, DeleteRows, PickDelimiter, PickEncoding, PickHeader,
        PickCopyAs, PickExport
    ]
);

const ROW: f32 = 26.;
const GUTTER: f32 = 52.;
const PAD: f32 = 8.;
/// Largeur d'une colonne : celle de ses plus longues cellules parmi les premières lignes.
const SAMPLE: usize = 200;
const MIN_CHARS: usize = 4;
const MAX_CHARS: usize = 40;
/// Épaisseur de la zone où l'on attrape une barre de défilement.
const EDGE: f32 = 12.;
const UNDO_DEPTH: usize = 200;
/// Un réglage ou une édition attend ce délai avant d'être écrit sur le disque.
const SAVE_DELAY: Duration = Duration::from_millis(400);

/// Réglage ou geste du tableau proposé par la palette et par la barre.
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

    /// L'action que l'app écoute pour ouvrir le choix : elle remonte jusqu'à elle depuis n'importe quel focus.
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
    /// Un fichier vient d'être écrit à côté du tableau.
    Exported,
}

/// Un geste d'édition, et son contraire : annuler applique le contraire et obtient le geste.
enum Change {
    /// Valeurs à poser : (ligne, colonne, texte).
    Set(Vec<(usize, usize, String)>),
    Insert(usize, Vec<Vec<String>>),
    Remove(Range<usize>),
}

#[derive(Clone, Copy, PartialEq)]
enum Drag {
    None,
    Cells,
    Rows,
    Cols,
    /// Barre de défilement : verticale ou non.
    Bar(bool),
}

enum Hit {
    Cell(usize, usize),
    Row(usize),
    Col(usize),
    Corner,
    Bar(bool),
}

/// Le fichier tel qu'il vient d'être lu.
pub struct Loaded {
    table: Table,
    format: Format,
    stamp: Option<(SystemTime, u64)>,
}

fn stamp_of(path: &Path) -> Option<(SystemTime, u64)> {
    let meta = fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

/// Lit le fichier. Sans `forced`, encodage, délimiteur et en-tête sont devinés, sauf
/// ceux que l'utilisateur a déjà choisis pour ce fichier.
pub fn load(path: &Path, forced: Option<Format>) -> Result<Loaded, String> {
    let fail = |e: std::io::Error| format!("{} {} : {e}", tr("Cannot open", "Impossible d'ouvrir"), path.display());
    let size = fs::metadata(path).map_err(fail)?.len();
    if size > table::MAX_BYTES as u64 {
        return Err(format!("{} : {}", path.display(), tr("file too large", "fichier trop gros")));
    }
    // ponytail: lecture et analyse synchrones (100 Mo : environ 0,3 s) ; passer en tâche
    // de fond avec un état « chargement » si l'on ouvre des fichiers de plusieurs centaines de Mo.
    let bytes = fs::read(path).map_err(fail)?;
    let format = forced.or_else(|| saved_format(path)).unwrap_or_else(|| table::detect(&bytes));
    let table = Table::parse(table::decode(bytes, format.encoding), format.delimiter);
    Ok(Loaded { table, format, stamp: stamp_of(path) })
}

/// Réglages que l'utilisateur a choisis pour ce fichier : `chemin ⇥ délimiteur ⇥ encodage ⇥ en-tête`.
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

/// Dernière génération écrite : une écriture en retard ne remplace jamais une plus récente.
#[derive(Default)]
struct Disk(Mutex<usize>);

impl Disk {
    fn write(&self, generation: usize, path: &Path, table: &Table, encoding: Encoding) -> Result<(), WriteError> {
        let mut last = self.0.lock().unwrap();
        if *last >= generation {
            return Ok(());
        }
        // Écriture atomique : un crash ne laisse jamais un tableau tronqué.
        let tmp = path.with_file_name(format!(".{}.tmp", path.file_name().unwrap_or_default().to_string_lossy()));
        let result = (|| {
            let file = fs::File::create(&tmp)?;
            if let Ok(meta) = fs::metadata(path) {
                fs::set_permissions(&tmp, meta.permissions()).ok();
            }
            let mut out = BufWriter::with_capacity(1 << 16, file);
            table.write_to(&mut out, encoding)?;
            let file = out.into_inner().map_err(|e| e.into_error())?;
            // Sans `fsync`, une coupure de courant peut laisser le renommage sans les données.
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

/// Délimiteur tel que la liste de choix et la barre le montrent : son nom, puis le signe.
pub fn delimiter_option(d: u8) -> String {
    let sign = match d {
        b'\t' => "⇥".to_string(),
        b' ' => "␣".to_string(),
        d => char::from(d).to_string(),
    };
    format!("{}  {sign}", table::delimiter_label(d))
}

/// Nom de colonne à la manière des tableurs : A, B… Z, AA, AB…
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
    /// Date et taille du fichier tel que lu ou écrit : un autre programme l'a touché si elles changent.
    stamp: Option<(SystemTime, u64)>,
    /// Cellule active, et ancre de la sélection (qui va de l'une à l'autre).
    cursor: (usize, usize),
    anchor: (usize, usize),
    /// Sélections gardées par Ctrl+clic (ancre, curseur), en plus de celle en cours.
    more: Vec<((usize, usize), (usize, usize))>,
    /// Texte en cours de frappe dans la cellule active.
    edit: Option<Line>,
    /// Largeur de chaque colonne, et abscisse de son bord gauche (une de plus, pour le total).
    widths: Vec<f32>,
    xs: Vec<f32>,
    /// Décalage de la vue, en pixels (x, y).
    scroll: (f32, f32),
    body: Bounds<Pixels>,
    drag: Drag,
    undo: Vec<Vec<Change>>,
    redo: Vec<Vec<Change>>,
    dirty: bool,
    generation: usize,
    disk: Arc<Disk>,
    /// La sélection vient d'être copiée : l'icône le montre un instant.
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

    /// Thème courant, et la fenêtre dessine-t-elle sa barre de titre (la pastille cache alors
    /// le coin de la barre du tableau, qui sert de poignée de déplacement).
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

    // ----- Fichier -----

    /// Un tableau vide garde une ligne vide où écrire.
    fn fresh_table(&mut self) {
        if self.table.rows() == 0 {
            self.table.insert_row(0);
        }
        self.measure();
    }

    /// Prend une version relue du disque, en gardant la place du curseur.
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

    /// Un autre programme a réécrit le fichier et rien n'attend ici : on le relit.
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

    /// Change les réglages de lecture de ce fichier (`None` : les deviner de nouveau) :
    /// ce qui attend est écrit, puis le fichier est relu ainsi, et le choix retenu.
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

    /// Une modification : le disque la reçoit après un court délai, ou tout de suite à `flush`.
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

    /// Écrit en tâche de fond : un gros fichier ne fige pas la fenêtre.
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

    /// Écrit tout de suite ce qui attend (changement de fichier, fermeture).
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

    // ----- Géométrie -----

    fn cols(&self) -> usize {
        self.table.cols().max(1)
    }

    /// Première ligne du fichier qui défile : la précédente est l'en-tête, fixe en haut.
    fn first(&self) -> usize {
        self.format.header as usize
    }

    fn text_px(&self) -> f32 {
        (self.theme.size * 0.9).round().max(11.)
    }

    fn measure(&mut self) {
        // Une lecture par ligne, jamais cellule par cellule : le temps reste linéaire sur une ligne très large.
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

    /// Une cellule modifiée peut élargir sa colonne, jusqu'à la limite.
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

    /// Remet la vue, le curseur et la sélection dans le tableau.
    fn clamp(&mut self) {
        let (rows, cols) = (self.table.rows().max(1), self.cols());
        let fit = |(r, c): (usize, usize)| (r.min(rows - 1), c.min(cols - 1));
        (self.cursor, self.anchor) = (fit(self.cursor), fit(self.anchor));
        self.more.iter_mut().for_each(|(a, b)| (*a, *b) = (fit(*a), fit(*b)));
        let (mx, my) = self.max_scroll();
        self.scroll = (self.scroll.0.clamp(0., mx), self.scroll.1.clamp(0., my));
    }

    /// Ordonnée du haut de la ligne dans la grille, en-tête compris.
    fn y_of(&self, row: usize) -> f32 {
        match row.checked_sub(self.first()) {
            Some(line) => ROW + line as f32 * ROW - self.scroll.1,
            None => 0.,
        }
    }

    fn col_at(&self, x: f32) -> usize {
        self.xs.partition_point(|&edge| edge <= x).saturating_sub(1).min(self.cols() - 1)
    }

    /// Fait défiler la vue jusqu'à la cellule.
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

    // ----- Sélection -----

    /// Lignes et colonnes de la sélection en cours : celle où l'on tape, colle et étend.
    fn selection(&self) -> (Range<usize>, Range<usize>) {
        Self::span(self.anchor, self.cursor)
    }

    fn span((r0, c0): (usize, usize), (r1, c1): (usize, usize)) -> (Range<usize>, Range<usize>) {
        (r0.min(r1)..r0.max(r1) + 1, c0.min(c1)..c0.max(c1) + 1)
    }

    /// Toutes les sélections, de haut en bas : celles gardées par Ctrl+clic et celle en cours.
    /// Copier, couper, effacer et supprimer des lignes les prennent toutes.
    fn selections(&self) -> Vec<(Range<usize>, Range<usize>)> {
        let mut all: Vec<_> = self.more.iter().map(|&(a, b)| Self::span(a, b)).chain([self.selection()]).collect();
        all.sort_by_key(|(rows, cols)| (rows.start, cols.start));
        all.dedup();
        all
    }

    /// Où l'on en est dans le tableau : la cellule du curseur et le défilement, pour y revenir.
    pub fn whereabouts(&self) -> ((usize, usize), (f32, f32)) {
        (self.cursor, self.scroll)
    }

    /// Revient où l'on en était. Le défilement est borné quand la grille connaît sa taille
    /// (`measured`), pas avant : elle n'a pas encore été dessinée.
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

    // ----- Saisie dans une cellule -----

    fn begin_edit(&mut self, text: Option<&str>, cx: &mut Context<Self>) {
        let (r, c) = self.cursor;
        let text = text.map_or_else(|| self.table.cell(r, c).into_owned(), str::to_string);
        self.edit = Some(Line::new(text));
        cx.notify();
    }

    /// Valide la saisie en cours.
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
            // Taper sur une cellule en remplace le contenu, comme dans un tableur.
            None if !text.chars().all(char::is_control) => self.begin_edit(Some(text), cx),
            None => {}
        }
        cx.notify();
    }

    // ----- Modifications -----

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
        }
    }

    /// Applique des gestes ensemble : un seul « annuler » les défait.
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

    /// Vide les cellules de la sélection.
    fn clear(&mut self, cx: &mut Context<Self>) {
        // Deux sélections peuvent se recouvrir : chaque cellule une seule fois.
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
        let (rows, _) = self.selection();
        let at = if below { rows.end } else { rows.start };
        self.commit(vec![Change::Insert(at, vec![Vec::new()])], cx);
        self.select_to((at, self.cursor.1), false, cx);
    }

    fn delete_rows(&mut self, cx: &mut Context<Self>) {
        self.finish(cx);
        // Les lignes de toutes les sélections, regroupées en plages et retirées du bas vers le
        // haut : chaque retrait laisse en place les lignes qui restent à retirer.
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

    // ----- Presse-papiers et export -----

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

    /// La sélection, ou tout le tableau si elle tient dans une seule cellule.
    fn scope(&self) -> (Range<usize>, Range<usize>, bool) {
        let (rows, cols) = self.selection();
        match rows.len() * cols.len() {
            1 => (0..self.table.rows(), 0..self.cols(), false),
            _ => (rows, cols, true),
        }
    }

    /// Ctrl+C : la sélection en TSV, que les tableurs et les tableaux des notes collent tels quels.
    fn copy(&mut self, cx: &mut Context<Self>) {
        // Plusieurs sélections : leurs blocs l'un sous l'autre, de haut en bas.
        let blocks: Vec<String> =
            self.selections().into_iter().map(|(rows, cols)| self.table.text_range(rows, cols, Style::Tsv)).collect();
        cx.write_to_clipboard(ClipboardItem::new_string(blocks.join("\n")));
        self.flash(cx);
    }

    /// « Copier le tableau en… » : la sélection, ou tout le tableau si elle tient dans une cellule.
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

    /// Texte collé : des cellules (tableur, tableau Markdown, CSV) remplissent la grille à partir
    /// du coin de la sélection, qui s'agrandit au besoin ; une seule remplit toute la sélection.
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

    /// Écrit le tableau (ou la sélection) à côté du fichier, dans ce format.
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

    // ----- Souris -----

    fn press(&mut self, e: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus);
        self.finish(cx);
        let extend = e.modifiers.shift;
        let hit = self.hit(e.position);
        // Ctrl+clic sur une cellule, une ligne ou une colonne : ce qui est sélectionné le reste,
        // une autre sélection commence. Un clic simple ne garde que la sienne.
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
                // Double-clic sur l'en-tête : on écrit dans sa cellule.
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

    /// La barre de défilement suit le pointeur.
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

    /// La molette fait défiler ; avec Maj, de côté.
    fn wheel(&mut self, e: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let mut delta = e.delta.pixel_delta(px(ROW));
        if e.modifiers.shift && delta.x == px(0.) {
            delta = Point { x: delta.y, y: px(0.) };
        }
        self.scroll = (self.scroll.0 - f32::from(delta.x), self.scroll.1 - f32::from(delta.y));
        self.clamp();
        cx.notify();
    }

    /// Toute la colonne `c` (avec `extend`, jusqu'à celle de l'ancre). La vue ne bouge pas : la cellule active est
    /// celle du haut, et non la dernière ligne, où un défilement automatique nous emmènerait.
    fn pick_col(&mut self, c: usize, extend: bool, cx: &mut Context<Self>) {
        self.finish(cx);
        let from = if extend { self.anchor.1 } else { c };
        (self.anchor, self.cursor) = ((self.last_row(), from), (0, c));
        cx.notify();
    }

    /// Toute la ligne `r`, sans faire défiler vers la dernière colonne.
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

    /// Lignes et colonnes à construire. La taille de la vue n'est connue qu'après un premier dessin :
    /// d'ici là, et pour ne jamais montrer une grille à moitié vide, on prend la fenêtre entière,
    /// qui la contient.
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
            // Pendant le dessin, `notify` ne programme aucune nouvelle image : la grille ne se
            // serait redessinée qu'au prochain événement (souris, clavier). On le reporte à après.
            let this = cx.entity();
            cx.defer(move |cx| this.update(cx, |_, cx| cx.notify()));
        }
    }
}

// ponytail: saisie au fil de l'eau, sans composition IME (le texte composé est inséré
// tel quel) ni sélection dans la cellule ; reprendre le gestionnaire de l'éditeur si l'on
// écrit des tableaux en japonais ou en chinois.
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

    fn render_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        let name = self.path.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let info = format!("{name} · {} × {}", self.table.rows(), self.table.cols());
        let delimiter = delimiter_option(self.format.delimiter);
        div()
            .flex_none()
            .h(px(40.))
            .pl_4()
            // Sous la pastille de la fenêtre, quand l'app dessine sa barre de titre.
            .pr(px(if self.client { 128. } else { 8. }))
            .flex()
            .items_center()
            .gap_1()
            .border_b_1()
            .border_color(t.border)
            .text_size(px(12.))
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .items_center()
                    .min_w_0()
                    .truncate()
                    .text_color(t.soft)
                    .when(self.client, |d| d.map(crate::drag_window))
                    .child(info),
            )
            .child(self.chip("sheet-delimiter", delimiter, false, Pick::Delimiter))
            .child(self.chip("sheet-encoding", self.format.encoding.label().to_string(), false, Pick::Encoding))
            .child(self.chip("sheet-header", tr("Header", "En-tête").to_string(), self.format.header, Pick::Header))
            .child(
                button("sheet-copy", if self.copied { "check.svg" } else { "copy.svg" }, false, t)
                    .tooltip(tip(tr("Copy the selection", "Copier la sélection"), crate::keys::of(cx, &Copy), t))
                    .on_click(cx.listener(|this, _, _, cx| this.copy(cx))),
            )
            .child(
                button("sheet-export", "export.svg", false, t)
                    .tooltip(tip(tr("Export the table…", "Exporter le tableau…"), String::new(), t))
                    .on_click(|_, window, cx| window.dispatch_action(Pick::Export.action(), cx)),
            )
    }

    /// Une ligne de cellules, de la colonne `cols.start` à `cols.end` exclue.
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
                d.child(div().absolute().right(px(2.)).top(px(ROW + at)).w(px(6.)).h(px(len)).rounded(px(3.)).bg(t.dim.opacity(0.45)))
            })
            .when(mx > 0., |d| {
                let (at, len) = thumb(w - GUTTER, self.scroll.0, mx, self.view().0);
                d.child(div().absolute().bottom(px(2.)).left(px(GUTTER + at)).h(px(6.)).w(px(len)).rounded(px(3.)).bg(t.dim.opacity(0.45)))
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

        // Les colonnes et les lignes qui débordent de la vue ne sont pas construites.
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
            .child(div().absolute().top(px(0.)).left(px(-self.scroll.0)).w_full().h(px(ROW)).child(head))
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
            // Une cellule en cours de frappe : les touches d'une ligne de saisie passent avant
            // celles de la grille (flèches, effacement, presse-papiers).
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
            // Tab passe à la cellule suivante, et à la ligne suivante au bout de celle-ci.
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
            // Entrée valide et descend ; hors saisie, elle ouvre la cellule.
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
            .on_action(cx.listener(|this, _: &DeleteRows, _, cx| this.delete_rows(cx)))
            .child(self.render_bar(cx))
            .child(body)
    }
}

#[cfg(test)]
impl Sheet {
    pub fn table(&self) -> &Table {
        &self.table
    }

    /// Milieu de la cellule (`r`, `c`) à l'écran.
    #[cfg(test)]
    pub fn spot(&self, r: usize, c: usize) -> Point<Pixels> {
        let x = GUTTER + (self.xs[c] + self.xs[c + 1]) / 2. - self.scroll.0;
        let y = ROW + (r - self.first()) as f32 * ROW + ROW / 2. - self.scroll.1;
        self.body.origin + gpui::point(px(x), px(y))
    }

    /// Toutes les sélections (lignes, colonnes), de haut en bas.
    #[cfg(test)]
    pub fn picked(&self) -> Vec<(Range<usize>, Range<usize>)> {
        self.selections()
    }

    /// Défilement de la vue (x, y), et sélection (lignes, colonnes).
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

    /// Comme au tout premier affichage : la vue n'a pas encore été mesurée.
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
