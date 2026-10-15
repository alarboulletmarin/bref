//! Éditeur Markdown stylé en direct : le texte reste du Markdown brut, seul le
//! rendu change (tailles, graisses, couleurs).

use std::{
    borrow::Cow,
    collections::{HashMap, VecDeque},
    fs,
    hash::{BuildHasherDefault, Hash, Hasher},
    ops::Range,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

use gpui::{
    App, BorderStyle, Bounds, ClipboardEntry, ClipboardItem, ContentMask, Context, Corners, CursorStyle,
    ElementId, ElementInputHandler, Entity, EntityInputHandler, EventEmitter, FocusHandle,
    Focusable, Font, FontStyle, FontWeight, GlobalElementId, Hitbox, HitboxBehavior, Hsla, Image,
    ImageFormat, ImgResourceLoader, InspectorElementId, LayoutId, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, Point, RenderImage, Resource, Rgba, ScrollWheelEvent,
    SharedString, Size, StrikethroughStyle, Style, TextAlign, TextRun, TransformationMatrix,
    UTF16Selection, UnderlineStyle, Window, WrappedLine, actions, div, fill, font, point,
    prelude::*, px, quad, relative, size,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::{
    Theme, diagram::Diagram, figure, graph, grid::{self, TableRow}, mono, sans, tr, vault,
    markdown::{self as md, Enter, Kind, Link, ListStyle},
};

actions!(
    editor,
    [
        AlignLeft,
        AlignCenter,
        AlignRight,
        Backspace,
        Delete,
        DeleteWordLeft,
        DeleteWordRight,
        Left,
        Right,
        Up,
        Down,
        WordLeft,
        WordRight,
        Home,
        End,
        DocStart,
        DocEnd,
        PageUp,
        PageDown,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        SelectWordLeft,
        SelectWordRight,
        SelectHome,
        SelectEnd,
        SelectDocStart,
        SelectDocEnd,
        SelectAll,
        Newline,
        Indent,
        Outdent,
        ToggleTask,
        ListBullet,
        ListOrdered,
        ListTask,
        ListText,
        ListCycle,
        TableRowAbove,
        TableRowBelow,
        TableColLeft,
        TableColRight,
        TableDuplicateRow,
        TableDuplicateCol,
        TableMoveRowUp,
        TableMoveRowDown,
        TableMoveColLeft,
        TableMoveColRight,
        TableDeleteRow,
        TableDeleteCol,
        TableDeleteSpan,
        TableDelete,
        TableSortAsc,
        TableSortDesc,
        TableTranspose,
        Bold,
        Italic,
        Underline,
        Strike,
        Highlight,
        InlineCode,
        SelectNext,
        InsertLink,
        Comment,
        Copy,
        Cut,
        Paste,
        Undo,
        Redo,
        Cancel,
        Find,
        FindReplace,
        FindNext,
        FindPrev,
        FindEnter,
        FindClose,
        FindErase,
        FindSwitch,
        FindCase,
        FindWord,
        FindRegex,
        ReplaceInVault,
        FindPaste,
        ReplaceAll,
    ]
);

/// Recherche dans la note : la barre, ses réglages et les passages trouvés.
#[derive(Default)]
struct Finder {
    query: String,
    /// Texte de remplacement ; `None` tant que la ligne « Remplacer » est fermée.
    with: Option<String>,
    /// La recherche vient de la sélection : elle est comme sélectionnée, la première frappe
    /// la remplace.
    fresh: bool,
    /// La saisie va au champ de remplacement.
    on_with: bool,
    /// La barre reçoit la saisie ; un clic dans la note la lui reprend sans la fermer.
    active: bool,
    case: bool,
    word: bool,
    /// La recherche est une expression régulière.
    regex: bool,
    hits: Vec<Range<usize>>,
    /// Passage courant dans `hits`.
    at: usize,
    /// Texte et réglages pour lesquels `hits` a été calculé.
    key: Option<(u64, String, bool, bool, bool)>,
}

const TOP: Pixels = px(20.);
pub const MAX_WIDTH: Pixels = px(720.);
/// Marge gardée de chaque côté de la page, qu'elle soit étroite ou pleine.
const SIDE: Pixels = px(24.);

/// Largeur de la colonne de texte (réglage « Largeur de page »).
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Page {
    Narrow,
    #[default]
    Normal,
    Wide,
    /// Toute la place de la fenêtre.
    Full,
}

impl Page {
    pub const ALL: [Page; 4] = [Page::Narrow, Page::Normal, Page::Wide, Page::Full];

    pub fn key(self) -> &'static str {
        match self {
            Page::Narrow => "narrow",
            Page::Normal => "normal",
            Page::Wide => "wide",
            Page::Full => "full",
        }
    }

    /// Un réglage inconnu ou absent vaut la largeur normale.
    pub fn of(key: &str) -> Self {
        Self::ALL.into_iter().find(|p| p.key() == key).unwrap_or_default()
    }

    pub fn label(self) -> &'static str {
        match self {
            Page::Narrow => tr("Narrow", "Étroite"),
            Page::Normal => tr("Normal", "Normale"),
            Page::Wide => tr("Wide", "Large"),
            Page::Full => tr("Full width", "Pleine largeur"),
        }
    }

    /// La colonne la plus large, pour un texte de `size` px : elle suit le corps du texte, pour que
    /// le nombre de caractères par ligne reste le même quand on agrandit.
    pub fn measure(self, size: f32) -> Pixels {
        let base = match self {
            Page::Narrow => 560.,
            Page::Normal => 720.,
            Page::Wide => 960.,
            Page::Full => return px(f32::MAX),
        };
        px(base * size / 16.)
    }
}

/// Où se place un tableau plus large que la colonne de texte (réglage « Grands tableaux »).
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Wide {
    /// Il reste dans la colonne et défile de côté.
    Column,
    /// Il déborde des deux côtés, centré sur la fenêtre.
    #[default]
    Centered,
}

impl Wide {
    pub const ALL: [Wide; 2] = [Wide::Column, Wide::Centered];

    pub fn key(self) -> &'static str {
        match self {
            Wide::Column => "column",
            Wide::Centered => "center",
        }
    }

    pub fn of(key: &str) -> Self {
        Self::ALL.into_iter().find(|w| w.key() == key).unwrap_or_default()
    }

    pub fn label(self) -> &'static str {
        match self {
            Wide::Column => tr("In the text column", "Dans la colonne du texte"),
            Wide::Centered => tr("Centered, wider than the text", "Centrés, plus larges que le texte"),
        }
    }
}
/// Côté du bouton de copie, à droite de l'ouverture d'un bloc de code.
const COPY_SIZE: Pixels = px(22.);
/// Corps des formules, par rapport au texte : la police mathématique paraît petite à taille égale.
const MATH_SCALE: f32 = 1.2;
/// Espace entre une ligne et l'image qu'elle désigne.
const IMAGE_GAP: Pixels = px(8.);
/// Place gardée sous un tableau pour le bouton qui lui ajoute une ligne, et
/// épaisseur de ce bouton.
const TABLE_GAP: Pixels = px(18.);
const PLUS: Pixels = px(12.);
/// Sélecteur de tableau : colonnes et lignes proposées, pas de la grille.
const GRID: usize = 8;
const GRID_STEP: Pixels = px(20.);
/// Plus grand tableau qu'on puisse demander, au clavier : (colonnes, lignes).
const GRID_MAX: (usize, usize) = (20, 99);
/// Choix affichés à la fois dans la liste de complétion.
const SHOWN: usize = 8;

/// `file` est dans `root`, une fois les `..` et les liens symboliques résolus : une note
/// reçue ne doit pas faire afficher un fichier du disque hors du coffre.
// ponytail: un dossier d'images relié par un lien symbolique vers l'extérieur du coffre n'est
// pas suivi ; autoriser la cible de ce lien si quelqu'un en a besoin.
fn inside(root: &Path, file: &Path) -> bool {
    let (Ok(root), Ok(file)) = (fs::canonicalize(root), fs::canonicalize(file)) else {
        return false;
    };
    file.starts_with(root)
}

/// Commande `/nom` : (nom, libellé anglais, libellé français, texte posé avant
/// le curseur, texte posé après).
/// Un geste sur le tableau où est le curseur : il vaut pour les lignes et les colonnes que la
/// sélection touche.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TableOp {
    RowAbove,
    RowBelow,
    ColLeft,
    ColRight,
    DuplicateRow,
    DuplicateCol,
    MoveRowUp,
    MoveRowDown,
    MoveColLeft,
    MoveColRight,
    DeleteRow,
    DeleteCol,
    DeleteTable,
    SortAsc,
    SortDesc,
    Transpose,
}

impl TableOp {
    pub const ALL: [TableOp; 16] = [
        TableOp::RowAbove,
        TableOp::RowBelow,
        TableOp::DuplicateRow,
        TableOp::MoveRowUp,
        TableOp::MoveRowDown,
        TableOp::DeleteRow,
        TableOp::ColLeft,
        TableOp::ColRight,
        TableOp::DuplicateCol,
        TableOp::MoveColLeft,
        TableOp::MoveColRight,
        TableOp::DeleteCol,
        TableOp::SortAsc,
        TableOp::SortDesc,
        TableOp::Transpose,
        TableOp::DeleteTable,
    ];

    pub fn label(self) -> &'static str {
        match self {
            TableOp::RowAbove => tr("Insert a row above", "Insérer une ligne au-dessus"),
            TableOp::RowBelow => tr("Insert a row below", "Insérer une ligne en dessous"),
            TableOp::ColLeft => tr("Insert a column on the left", "Insérer une colonne à gauche"),
            TableOp::ColRight => tr("Insert a column on the right", "Insérer une colonne à droite"),
            TableOp::DuplicateRow => tr("Duplicate the row", "Dupliquer la ligne"),
            TableOp::DuplicateCol => tr("Duplicate the column", "Dupliquer la colonne"),
            TableOp::MoveRowUp => tr("Move the row up", "Monter la ligne"),
            TableOp::MoveRowDown => tr("Move the row down", "Descendre la ligne"),
            TableOp::MoveColLeft => tr("Move the column left", "Décaler la colonne à gauche"),
            TableOp::MoveColRight => tr("Move the column right", "Décaler la colonne à droite"),
            TableOp::DeleteRow => tr("Delete the row", "Supprimer la ligne"),
            TableOp::DeleteCol => tr("Delete the column", "Supprimer la colonne"),
            TableOp::DeleteTable => tr("Delete the table", "Supprimer le tableau"),
            TableOp::SortAsc => tr("Sort the rows by this column, A to Z", "Trier les lignes sur cette colonne, de A à Z"),
            TableOp::SortDesc => tr("Sort the rows by this column, Z to A", "Trier les lignes sur cette colonne, de Z à A"),
            TableOp::Transpose => tr("Swap rows and columns", "Échanger lignes et colonnes"),
        }
    }

    /// La feuille d'un CSV sait le faire : tout sauf supprimer le tableau et échanger lignes et colonnes.
    pub fn in_sheet(self) -> bool {
        !matches!(self, TableOp::DeleteTable | TableOp::Transpose)
    }

    /// L'action qui fait la même chose au clavier.
    pub fn action(self) -> Box<dyn gpui::Action> {
        match self {
            TableOp::RowAbove => Box::new(TableRowAbove),
            TableOp::RowBelow => Box::new(TableRowBelow),
            TableOp::ColLeft => Box::new(TableColLeft),
            TableOp::ColRight => Box::new(TableColRight),
            TableOp::DuplicateRow => Box::new(TableDuplicateRow),
            TableOp::DuplicateCol => Box::new(TableDuplicateCol),
            TableOp::MoveRowUp => Box::new(TableMoveRowUp),
            TableOp::MoveRowDown => Box::new(TableMoveRowDown),
            TableOp::MoveColLeft => Box::new(TableMoveColLeft),
            TableOp::MoveColRight => Box::new(TableMoveColRight),
            TableOp::DeleteRow => Box::new(TableDeleteRow),
            TableOp::DeleteCol => Box::new(TableDeleteCol),
            TableOp::DeleteTable => Box::new(TableDelete),
            TableOp::SortAsc => Box::new(TableSortAsc),
            TableOp::SortDesc => Box::new(TableSortDesc),
            TableOp::Transpose => Box::new(TableTranspose),
        }
    }
}

/// Largeur offerte aux tableaux dans un volet large de `pane`, pour un texte large de `text`.
fn table_room(wide: Wide, pane: Pixels, text: Pixels) -> Pixels {
    match wide {
        Wide::Column => text,
        Wide::Centered => (pane - SIDE * 2.).max(text),
    }
}

/// De combien un tableau large de `frame` dépasse le bord gauche du texte.
fn table_lead(wide: Wide, frame: Pixels, text: Pixels) -> Pixels {
    match wide {
        Wide::Centered => ((frame - text) / 2.).max(px(0.)),
        Wide::Column => px(0.),
    }
}

type Command = (&'static str, &'static str, &'static str, &'static str, &'static str);

const COMMANDS: &[Command] = &[
    ("h1", "Heading 1", "Titre 1", "# ", ""),
    ("h2", "Heading 2", "Titre 2", "## ", ""),
    ("h3", "Heading 3", "Titre 3", "### ", ""),
    ("list", "Bullet list", "Liste à puces", "- ", ""),
    ("num", "Numbered list", "Liste numérotée", "1. ", ""),
    ("todo", "Task", "Tâche à cocher", "- [ ] ", ""),
    ("table", "Table", "Tableau", "", ""),
    ("date", "Today's date", "Date du jour", "", ""),
    ("meta", "Front matter: tags, aliases", "En-tête : tags, alias", "", ""),
    ("note", "Panel: note", "Panneau : note", "> [!NOTE]\n> ", ""),
    ("tip", "Panel: tip", "Panneau : astuce", "> [!TIP]\n> ", ""),
    ("important", "Panel: important", "Panneau : important", "> [!IMPORTANT]\n> ", ""),
    ("warning", "Panel: warning", "Panneau : attention", "> [!WARNING]\n> ", ""),
    ("caution", "Panel: danger", "Panneau : danger", "> [!CAUTION]\n> ", ""),
    ("quote", "Quote", "Citation", "> ", ""),
    ("code", "Code block", "Bloc de code", "```", "\n\n```"),
    ("mermaid", "Mermaid diagram", "Diagramme Mermaid", "```mermaid\n", "\n```"),
    ("math", "Formula", "Formule", "$$\n", "\n$$"),
    ("rule", "Divider", "Séparateur", "---\n", ""),
    ("link", "Link to a note", "Lien vers une note", "[[", ""),
    ("image", "Picture or diagram", "Image ou schéma", "![[", ""),
];

/// Ce que la complétion propose : un nom à poser après `[[`, ou une commande.
#[derive(Clone, Copy)]
enum Choice<'a> {
    Name(&'a str),
    Command(&'static Command),
    /// Jour proposé après `@` : son nom anglais et français, sa date.
    Date(&'static str, &'static str, md::Date),
}

/// Boutons « + » d'un tableau : sous lui pour une ligne, à sa droite pour une colonne.
fn plus_bars(frame: Bounds<Pixels>) -> [Bounds<Pixels>; 2] {
    [
        Bounds::new(point(frame.left(), frame.bottom() + px(3.)), size(frame.size.width, PLUS)),
        Bounds::new(point(frame.right() + px(4.), frame.top()), size(PLUS, frame.size.height)),
    ]
}

#[derive(Clone, Copy, PartialEq)]
enum Motion {
    Left,
    Right,
    Up,
    Down,
    WordLeft,
    WordRight,
    Home,
    End,
    DocStart,
    DocEnd,
    PageUp,
    PageDown,
}

pub enum EditorEvent {
    Changed,
    Open(Link),
    /// Le même, avec Maj : à ouvrir dans l'autre pane.
    OpenAside(Link),
    /// Remplacer dans tout le coffre ce que la barre de recherche cherche.
    Swap(md::Swap),
    /// Clic droit dans la note : où, le lien qui s'y trouve, et s'il vise un commentaire.
    Menu(Point<Pixels>, Option<Link>, bool),
}

/// Ce dont la mise en page dépend, en dehors du texte : si rien n'a changé, on la garde.
#[derive(PartialEq)]
struct LayoutKey {
    version: u64,
    width: u32,
    size: u32,
    cursor: usize,
    marked: Option<Range<usize>>,
}

/// Ce qui a changé dans le texte depuis la dernière mise en page : ses `prefix` premiers octets
/// et ses `suffix` derniers sont restés tels quels.
#[derive(Clone, Copy)]
struct Damage {
    old_len: usize,
    prefix: usize,
    suffix: usize,
}

/// La dernière mise en page, pour en reprendre ce qui n'a pas changé.
struct Laid {
    epoch: u64,
    width: u32,
    room: u32,
    size: u32,
    len: usize,
    cursor: usize,
    fences: usize,
    dollars: usize,
    /// Longueur de l'en-tête YAML.
    meta: usize,
}

/// Une zone du texte à remettre en page : les lignes de l'ancienne mise en page `old` (indices),
/// le texte qui les remplace, de `start` à `stop` (octets, `stop` inclus : fin de la dernière ligne).
struct Region {
    old: Range<usize>,
    start: usize,
    stop: usize,
    /// Dans un très gros tableau : ses colonnes, le rang de la première ligne dans le tableau et l'octet où
    /// le tableau commence.
    big: Option<(Rc<grid::Geometry>, usize, usize)>,
    /// La zone touche aux premières lignes, qui fixent les colonnes : elles ne doivent pas avoir changé.
    check: bool,
}

/// Ce qu'on garde de la mise en page précédente, et ce qu'on refait.
struct Plan {
    regions: Vec<Region>,
    /// Comptes de lignes de clôture et de `$$`, inchangés.
    fences: usize,
    dollars: usize,
    /// Les lignes reprises qui suivent `after` (octets de l'ancien texte) se décalent de `delta`.
    after: usize,
    delta: isize,
}

/// Reprend `count` lignes de l'ancienne mise en page, posées à partir de `y`, leur texte décalé de `delta`.
fn reuse(old: &mut std::vec::IntoIter<Row>, count: usize, delta: isize, y: &mut Pixels, rows: &mut Vec<Row>) {
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
struct BigBlock {
    /// Indices de ses lignes.
    rows: Range<usize>,
    geom: Rc<grid::Geometry>,
    /// Parties de ses lignes (indices dans le tableau) qui ont leurs cellules.
    mat: Vec<Range<usize>>,
    /// Ses lignes viennent d'une autre mise en page : on ne sait plus lesquelles ont leurs cellules.
    rescan: bool,
}

/// Un tableau de la note.
/// Où sont à l'écran les traits d'un tableau, pour ses poignées et ce qu'on y a pris.
struct Geo {
    /// Début du tableau dans le texte.
    start: usize,
    frame: Bounds<Pixels>,
    /// Abscisses des traits verticaux (une de plus que de colonnes), ordonnées des horizontaux
    /// (une de plus que de lignes vues : l'en-tête, puis le corps).
    xs: Vec<Pixels>,
    ys: Vec<Pixels>,
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

/// Lignes et colonnes (rangs visibles) prises par une poignée. Valable tant que le texte et la
/// sélection sont ceux d'alors : rien d'autre à défaire.
#[derive(Clone)]
struct Pick {
    table: usize,
    rows: Range<usize>,
    cols: Range<usize>,
    sel: Range<usize>,
    version: u64,
}

/// Une ligne ou une colonne qu'on déplace en tirant sa poignée.
struct Moving {
    table: usize,
    part: Part,
    from: Point<Pixels>,
    /// Le trait où elle se poserait.
    to: Option<usize>,
}

/// Une poignée est une pastille de 10 px sur 24 ; elle flotte à 5 px du tableau.
const GRIP: Pixels = px(10.);
const GRIP_LONG: Pixels = px(24.);
const GRIP_GAP: Pixels = px(5.);
/// Jusqu'où, autour d'un tableau, le pointeur fait paraître la poignée de sa colonne ou de sa ligne.
const GRIP_REACH: Pixels = px(24.);

/// Ce que le pointeur désigne autour d'un tableau : sa colonne, sa ligne, et si c'est le coin.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
struct Near {
    col: Option<usize>,
    row: Option<usize>,
    corner: bool,
}

impl Geo {
    fn cols(&self) -> usize {
        self.xs.len() - 1
    }

    fn tall(&self) -> usize {
        self.ys.len() - 1
    }

    /// La colonne et la ligne au-dessus ou à côté desquelles est `at`, jusqu'à `GRIP_REACH` du tableau.
    fn near(&self, at: Point<Pixels>) -> Near {
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
    fn parts(&self) -> Vec<(Part, Bounds<Pixels>)> {
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
    fn visible(&self, part: Part, near: Near, pick: Option<&Pick>) -> bool {
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
    fn boundary(&self, part: Part, at: Point<Pixels>) -> usize {
        let (lines, at, first) = match part {
            Part::Col(_) => (&self.xs, at.x, 0),
            _ => (&self.ys, at.y, 1),
        };
        (first..lines.len()).min_by_key(|&b| (f32::from((lines[b] - at).abs()) * 100.) as i64).unwrap_or(first)
    }

    /// Les cases `rows` × `cols`, en pixels.
    fn cells(&self, rows: &Range<usize>, cols: &Range<usize>) -> Bounds<Pixels> {
        let (x0, x1) = (self.xs[cols.start.min(self.cols())], self.xs[cols.end.min(self.cols())]);
        let (y0, y1) = (self.ys[rows.start.min(self.tall())], self.ys[rows.end.min(self.tall())]);
        Bounds::new(point(x0, y0), size(x1 - x0, y1 - y0))
    }
}

struct TableMeta {
    /// Octets, de son début au bout de sa dernière ligne.
    range: Range<usize>,
    /// Indices de ses lignes.
    rows: Range<usize>,
    /// Largeur de sa plus large ligne.
    wide: Pixels,
}

/// Une ligne logique mise en page (elle peut occuper plusieurs lignes visuelles).
struct Row {
    start: usize,
    len: usize,
    y: Pixels,
    pad: Pixels,
    lh: Pixels,
    height: Pixels,
    kind: Kind,
    /// Son texte mis en forme, partagé avec le cache des lignes.
    shaped: Rc<Shaped>,
    /// Image désignée par la ligne, ou figure (diagramme, formule) qu'elle
    /// termine, dessinée dessous, et sa taille à l'écran.
    image: Option<(Arc<RenderImage>, Size<Pixels>)>,
    /// La ligne ouvre un bloc de code : elle porte le bouton de copie.
    opens: bool,
    /// Place laissée sous la dernière ligne d'un tableau.
    gap: Pixels,
    /// Couleur du panneau `> [!TYPE]` dont la ligne fait partie.
    tint: Option<Hsla>,
    /// Ligne de tableau plus large que la page : de combien son texte est décalé vers la gauche.
    dx: Pixels,
    /// Ligne de tableau affichée en grille : ses cellules, qui passent à la ligne.
    grid: Option<Rc<TableRow>>,
}

impl Row {
    /// Position à l'écran, par rapport au bord gauche de la page.
    fn pos(&self, i: usize) -> Point<Pixels> {
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
    fn shown(&self, i: usize) -> usize {
        grid::shown(&self.shaped.subs, i)
    }

    /// Octet du texte affiché → octet du texte.
    fn source(&self, i: usize) -> usize {
        grid::source(&self.shaped.subs, i)
    }

    fn image_height(&self) -> Pixels {
        self.image.as_ref().map_or(px(0.), |(_, s)| s.height + IMAGE_GAP)
    }

    fn index_at(&self, p: Point<Pixels>) -> usize {
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

    fn visual_rows(&self) -> i32 {
        match &self.grid {
            Some(grid) => grid.lines,
            None => (f32::from(self.height - self.pad - self.image_height() - self.gap) / f32::from(self.lh)).round() as i32,
        }
    }

    /// Lignes visuelles de ce qui contient l'octet `i` : la ligne, ou la cellule d'un tableau.
    fn lines_at(&self, i: usize) -> i32 {
        self.grid.as_ref().map_or_else(|| self.visual_rows(), |grid| grid.lines_at(i))
    }

    /// Ligne de tirets d'un tableau, qu'on ne voit pas.
    fn hidden(&self) -> bool {
        self.grid.as_ref().is_some_and(|g| g.hidden())
    }

    /// Largeur de ce qui est dessiné.
    fn content_width(&self) -> Pixels {
        match &self.grid {
            Some(grid) => grid.total(),
            None => self.shaped.line.as_ref().map_or(px(0.), |l| l.width()),
        }
    }
}

/// États du texte qu'on peut retrouver, du plus ancien au plus récent. Seul le
/// dernier est gardé en entier : chacun des autres ne retient que ce qui le
/// distingue du suivant, soit quelques octets par frappe plutôt qu'une copie de la note.
#[derive(Default)]
struct History {
    last: Option<(String, Range<usize>)>,
    /// (début de la différence, sa longueur dans l'état suivant, son texte ici, sélection).
    earlier: Vec<(usize, usize, String, Range<usize>)>,
}

impl History {
    fn push(&mut self, text: String, sel: Range<usize>) {
        if let Some((old, old_sel)) = self.last.replace((text, sel)) {
            let new = self.last.as_ref().unwrap().0.as_bytes();
            let old_bytes = old.as_bytes();
            let mut start = old_bytes.iter().zip(new).take_while(|(a, b)| a == b).count();
            while !old.is_char_boundary(start) {
                start -= 1;
            }
            let room = old.len().min(new.len()) - start;
            let mut tail = old_bytes.iter().rev().zip(new.iter().rev()).take(room).take_while(|(a, b)| a == b).count();
            while !old.is_char_boundary(old.len() - tail) {
                tail -= 1;
            }
            self.earlier.push((start, new.len() - tail - start, old[start..old.len() - tail].to_string(), old_sel));
            if self.earlier.len() >= 200 {
                self.earlier.remove(0);
            }
        }
    }

    fn pop(&mut self) -> Option<(String, Range<usize>)> {
        let out = self.last.take()?;
        if let Some((start, len, own, sel)) = self.earlier.pop() {
            let mut text = out.0.clone();
            text.replace_range(start..start + len, &own);
            self.last = Some((text, sel));
        }
        Some(out)
    }
}

/// Empreinte rapide (FxHash, brassée à la fin) : chaque ligne de la note est hachée à chaque
/// image, et SipHash, celui de la bibliothèque standard, y passait un tiers du temps. Les
/// clés ne viennent que de la note : pas besoin de résister à qui les choisirait.
#[derive(Default)]
pub struct Fast(u64);

impl Fast {
    fn add(&mut self, word: u64) {
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
fn cached_shape(
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
struct Shaped {
    line: Option<WrappedLine>,
    /// Signes affichés à la place de leur écriture ASCII : (octet de début,
    /// longueur dans le texte, longueur affichée).
    subs: Vec<(usize, usize, usize)>,
    /// Formule sur la ligne : son source, et si elle est hors texte.
    formula: Option<(String, bool)>,
    /// Chemin de l'image que la ligne désigne.
    picture: Option<String>,
}

/// Figure prête à dessiner (`None` : source invalide) et sa taille.
type Drawing = Option<(Arc<Image>, Size<Pixels>)>;

/// La figure de clé `key`, reprise de la frame précédente ou dessinée par `make` ;
/// si `lazy`, seulement reprise.
fn cached(
    old: &mut FastMap<u64, Drawing>,
    kept: &mut FastMap<u64, Drawing>,
    key: u64,
    lazy: bool,
    make: impl FnOnce() -> Option<figure::Figure>,
) -> Drawing {
    let drawing = match old.remove(&key).or_else(|| kept.get(&key).cloned()) {
        Some(drawing) => drawing,
        None if lazy => return None,
        None => make().map(|(svg, w, h)| {
            let image = Image::from_bytes(ImageFormat::Svg, svg.into_bytes());
            (Arc::new(image), size(px(w), px(h)))
        }),
    };
    kept.insert(key, drawing.clone());
    drawing
}

/// Largeur de la barre de mise en forme : onze boutons de 26 px, le chevron des couleurs (14 px), deux
/// séparateurs, 1 px entre chacun des quatorze éléments, 3 px de marge et le trait de bordure.
const BAR_WIDTH: f32 = 6. + 2. + 11. * 26. + 14. + 13. + 2. * 7.;

/// Fond du bouton qui replie la barre : toujours opaque, il passe sur du texte (la teinte de la sélection,
/// translucide, laisserait voir ce qu'il couvre).
pub fn bar_fill(t: &Theme, open: bool) -> Hsla {
    if open { crate::blend(t.accent, t.panel, 0.2) } else { t.panel }
}

/// Où et comment se montre la barre de mise en forme.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Bar {
    /// Dans la rangée du bas, son bord gauche à cette abscisse.
    Inline(f32),
    /// Repliée en un bouton, en bas à gauche, qui ouvre la barre au-dessus de lui.
    Compact,
}

/// Ce que la barre de mise en forme propose pour la sélection.
#[derive(Clone, Copy)]
struct Tools {
    /// Les styles en ligne posés (`md::STYLES`).
    styles: u16,
    /// Le genre de liste de la ligne du curseur.
    list: Option<ListStyle>,
    /// Le curseur est dans un tableau.
    table: bool,
}

/// Fond du texte surligné, de la couleur `color` (voir `md::MARKS`) : translucide, lisible sur le clair
/// comme sur le sombre, et distinct de la teinte d'accent des commentaires.
pub fn highlight(color: usize) -> Hsla {
    let hue = [0.14, 0.36, 0.58, 0.93, 0.76][color.min(md::MARKS.len() - 1)];
    Hsla { h: hue, s: 0.9, l: 0.55, a: 0.4 }
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
            crate::legible(tint, crate::blend(tint, t.bg, 0.1))
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

pub struct Editor {
    focus: FocusHandle,
    content: String,
    sel: Range<usize>,
    reversed: bool,
    /// Les autres curseurs (Alt+clic, occurrence suivante) : leur sélection, et si elle est à
    /// rebours. Ce que fait le curseur principal, ils le font aussi (voir `each`).
    more: Vec<(Range<usize>, bool)>,
    /// Les curseurs suivants rejouent le geste du premier : une seule étape d'annulation.
    held: bool,
    marked: Option<Range<usize>>,
    selecting: bool,
    /// Mot ou ligne saisi par un double ou triple clic (et s'il s'agit d'une ligne) :
    /// glisser l'étend de mot en mot ou de ligne en ligne.
    anchor: Option<(Range<usize>, bool)>,
    goal_x: Option<Pixels>,
    undo: History,
    redo: History,
    last_edit: Option<Instant>,
    notes: Vec<String>,
    /// Dossiers où chercher les images : celui de la note, puis le coffre.
    dirs: Vec<PathBuf>,
    /// Images du coffre par nom de fichier (en minuscules) : où qu'elles soient
    /// rangées, une note les trouve.
    images: HashMap<String, PathBuf>,
    /// Début de la ligne dont le bloc de code vient d'être copié.
    copied: Option<usize>,
    /// Début de la ligne dont le bouton de copie est survolé.
    hover: Option<usize>,
    /// Zones des boutons de copie, par début de ligne : elles portent le curseur en main.
    copy_hitboxes: Vec<(usize, Hitbox)>,
    /// Diagrammes et formules de la dernière frame, par empreinte de leur source.
    figures: FastMap<u64, Drawing>,
    /// Lignes mises en forme à la dernière frame, par empreinte de leur texte et
    /// de leur contexte.
    shaped: FastMap<u64, (Rc<Shaped>, u32)>,
    /// Numéro de la dernière mise en page : une ligne y est marquée quand elle sert.
    frame: u32,
    /// Cellules des tableaux mises en forme à la dernière frame.
    cells: grid::Cache,
    ac_index: usize,
    ac_dismissed: Option<usize>,
    /// Sélecteur de tableau ouvert sous le curseur : colonnes et lignes choisies.
    grid: Option<(usize, usize)>,
    /// Taille tapée pendant qu'il est ouvert (`12x5`) : elle dépasse la grille.
    grid_typed: String,
    /// Sa zone : le pointeur y prend la forme d'une main.
    grid_hitbox: Option<Hitbox>,
    /// Étendue et cadre de chaque tableau à la dernière frame, puis les zones de
    /// leurs boutons « + », deux par tableau.
    tables: Vec<(Range<usize>, Bounds<Pixels>)>,
    table_hitboxes: Vec<Hitbox>,
    /// Tableau survolé (son début), et celui de ses boutons qui l'est.
    table_hover: Option<(usize, Option<usize>)>,
    /// Où sont les traits de chaque tableau affiché, à la dernière frame.
    geos: Vec<Geo>,
    /// La poignée sous le pointeur : (début du tableau, poignée).
    part_hover: Option<(usize, Part)>,
    /// Où était le pointeur à son dernier mouvement.
    pointer: Point<Pixels>,
    /// La forme du pointeur sur ce qui se clique dans le texte (poignée, case, lien sous `Ctrl`), et la
    /// zone de la note où elle vaut.
    hand: Option<CursorStyle>,
    view_hitbox: Option<Hitbox>,
    /// Où des fichiers glissés se poseraient : un trait entre deux lignes.
    drop_mark: Option<usize>,
    pick: Option<Pick>,
    moving: Option<Moving>,
    /// Défilement horizontal des tableaux plus larges que la page, par début de tableau.
    table_x: HashMap<usize, Pixels>,
    /// Curseur de défilement de chaque tableau trop large, à la dernière frame.
    thumbs: Vec<Bounds<Pixels>>,
    theme: Theme,
    // Mise en page de la dernière frame.
    rows: Vec<Row>,
    /// Change à chaque modification du texte ou de ce qui sert à le mettre en page
    /// (thème, images) : tant qu'il et le reste de `LayoutKey` ne bougent pas, la mise
    /// en page est reprise telle quelle, pour un simple défilement.
    version: u64,
    /// Compte les changements du texte, et eux seuls : une image qui charge ou un thème qui change ne le font pas bouger.
    revision: u64,
    /// Compteur du bas de page, gardé tant que le texte et la sélection ne bougent pas.
    counted: Option<((u64, Option<Range<usize>>), String)>,
    /// Le texte compté en dernier, sa version et ses comptes : à la frappe, seules les lignes
    /// changées sont recomptées (`md::recount`).
    tally: Option<(u64, String, (usize, usize))>,
    /// Nombre de boutons que la fenêtre pose dans le coin, à droite du compteur.
    pub corner: usize,
    /// Ce que la barre de mise en forme propose, avec la version et la sélection pour lesquelles on l'a compté.
    tooled: Option<((u64, Range<usize>), Tools)>,
    /// La note est celle du pane actif : une seule barre de mise en forme pour toute la fenêtre.
    pub bar_here: bool,
    /// Bord gauche de la barre, quand elle se montre (voir `bar_place`).
    bar_at: Option<Bar>,
    /// Barre repliée en un bouton (note étroite) : ses boutons sont ouverts au-dessus de lui.
    bar_open: bool,
    /// Les pastilles de couleur du surlignage sont ouvertes.
    swatches: bool,
    /// Change quand tout est à refaire (autre note, thème, images) : rien n'est repris.
    epoch: u64,
    dmg: Option<Damage>,
    laid: Option<Laid>,
    /// Les tests refont toute la mise en page à chaque fois, pour la comparer à celle qu'on a reprise.
    force_full: bool,
    /// Test : comparer chaque mise en page reprise à une mise en page complète.
    #[cfg(test)]
    compare: bool,
    /// Test : mises en page complètes et mises en page reprises.
    #[cfg(test)]
    counts: (usize, usize, usize),
    laid_out: Option<LayoutKey>,
    /// Une image ou un diagramme se charge encore : la mise en page sera refaite.
    loading: bool,
    /// Hauteur totale des lignes, et indices des lignes qui portent un bouton de copie.
    total_y: Pixels,
    opening: Vec<usize>,
    /// Chaque tableau de la note : son étendue, ses lignes et sa largeur.
    table_meta: Vec<TableMeta>,
    big: Vec<BigBlock>,
    /// Empreinte du thème et des polices de la dernière mise en page.
    big_style: u64,
    origin: Point<Pixels>,
    width: Pixels,
    /// Largeur offerte à un tableau : celle du texte, ou plus si les réglages le laissent déborder.
    room: Pixels,
    viewport: Bounds<Pixels>,
    scroll_y: Pixels,
    reveal: bool,
    /// Avec `reveal` : la ligne du curseur se place en haut de la vue (saut vers un titre).
    reveal_top: bool,
    find: Option<Finder>,
}

impl EventEmitter<EditorEvent> for Editor {}

impl Focusable for Editor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Editor {
    pub fn new(theme: Theme, cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            content: String::new(),
            sel: 0..0,
            reversed: false,
            more: Vec::new(),
            held: false,
            marked: None,
            selecting: false,
            anchor: None,
            goal_x: None,
            undo: History::default(),
            redo: History::default(),
            last_edit: None,
            notes: Vec::new(),
            dirs: Vec::new(),
            images: HashMap::new(),
            copied: None,
            hover: None,
            copy_hitboxes: Vec::new(),
            figures: FastMap::default(),
            shaped: FastMap::default(),
            frame: 0,
            cells: grid::Cache::default(),
            ac_index: 0,
            ac_dismissed: None,
            grid: None,
            grid_typed: String::new(),
            grid_hitbox: None,
            tables: Vec::new(),
            table_hitboxes: Vec::new(),
            table_hover: None,
            geos: Vec::new(),
            part_hover: None,
            pointer: Point::default(),
            hand: None,
            view_hitbox: None,
            drop_mark: None,
            pick: None,
            moving: None,
            table_x: HashMap::new(),
            thumbs: Vec::new(),
            theme,
            rows: Vec::new(),
            version: 0,
            revision: 0,
            counted: None,
            tally: None,
            corner: 1,
            tooled: None,
            bar_here: true,
            bar_at: None,
            bar_open: false,
            swatches: false,
            epoch: 0,
            dmg: None,
            laid: None,
            force_full: false,
            #[cfg(test)]
            compare: std::env::var("BREF_NO_COMPARE").is_err(),
            #[cfg(test)]
            counts: (0, 0, 0),
            laid_out: None,
            loading: false,
            total_y: px(0.),
            opening: Vec::new(),
            table_meta: Vec::new(),
            big: Vec::new(),
            big_style: 0,
            origin: Point::default(),
            width: MAX_WIDTH,
            room: MAX_WIDTH,
            viewport: Bounds::default(),
            scroll_y: px(0.),
            reveal: false,
            reveal_top: false,
            find: None,
        }
    }

    pub fn text(&self) -> &str {
        &self.content
    }

    /// Remplace tout le contenu (changement de note) ; curseur à l'octet `cursor`.
    pub fn load(&mut self, text: String, cursor: usize, cx: &mut Context<Self>) {
        let cursor = cursor.min(text.len());
        self.version += 1;
        self.revision += 1;
        self.epoch += 1;
        self.content = text;
        self.sel = cursor..cursor;
        self.reversed = false;
        self.more.clear();
        self.marked = None;
        self.undo = History::default();
        self.redo = History::default();
        self.last_edit = None;
        self.scroll_y = px(0.);
        self.table_x.clear();
        self.selecting = false;
        self.reveal = true;
        // Une autre note : la barre reste, la saisie revient au texte.
        if let Some(find) = &mut self.find {
            find.active = false;
        }
        cx.notify();
    }

    pub fn set_theme(&mut self, theme: Theme, cx: &mut Context<Self>) {
        self.version += 1;
        self.epoch += 1;
        self.theme = theme;
        cx.notify();
    }

    /// Noms des notes du coffre, pour compléter les `[[wikiliens]]`.
    pub fn set_notes(&mut self, notes: Vec<String>) {
        self.notes = notes;
    }

    pub fn set_images(&mut self, images: &[PathBuf]) {
        let named = images.iter().map(|p| (graph::image_name(p).to_lowercase(), p.clone()));
        self.images = named.collect();
        self.version += 1;
        self.epoch += 1;
    }

    pub fn set_dirs(&mut self, dirs: Vec<PathBuf>) {
        self.version += 1;
        self.epoch += 1;
        self.dirs = dirs;
    }

    /// Contenu du bloc de code où se trouve le curseur.
    pub fn code_at_cursor(&self) -> Option<&str> {
        md::code_block(&self.content, self.cursor()).map(|r| &self.content[r])
    }

    /// Centre du premier bouton « Copier » affiché.
    #[cfg(test)]
    pub fn copy_button(&self) -> Option<Point<Pixels>> {
        Some(self.copy_bounds(self.rows.iter().find(|r| r.opens)?).center())
    }

    /// Nombre de signes et taille de l'image affichés sur la ligne qui commence par `prefix`.
    #[cfg(test)]
    pub fn decorations(&self, prefix: &str) -> (usize, Option<(f32, f32)>) {
        let row = self.rows.iter().find(|r| self.content[r.start..].starts_with(prefix)).unwrap();
        let image = row.image.as_ref().map(|(_, s)| (s.width.into(), s.height.into()));
        (row.shaped.subs.len(), image)
    }

    /// Centre des boutons « + » (ligne, colonne) du premier tableau affiché.
    #[cfg(test)]
    pub fn plus_buttons(&self) -> Option<[Point<Pixels>; 2]> {
        Some(plus_bars(self.tables.first()?.1).map(|bar| bar.center()))
    }

    /// Point de l'écran où se trouve l'octet `i`, au milieu de sa ligne.
    #[cfg(test)]
    pub fn point_of(&self, i: usize) -> Option<Point<Pixels>> {
        let row = &self.rows[self.row_at(i)?];
        Some(self.origin + point(px(0.), row.y + row.pad + row.lh / 2.) + row.pos(i - row.start))
    }

    #[cfg(test)]
    pub fn hand(&self) -> Option<CursorStyle> {
        self.hand
    }

    #[cfg(test)]
    pub fn caret(&self) -> usize {
        self.cursor()
    }

    /// Met le curseur à l'octet `at` (le plus proche qui soit valide).
    #[cfg(test)]
    pub fn place_cursor(&mut self, at: usize, cx: &mut Context<Self>) {
        let at = self.clamp(at);
        self.move_to(at, cx);
    }

    /// Mises en page complètes et mises en page reprises, depuis le début.
    #[cfg(test)]
    pub fn layouts(&self) -> (usize, usize, usize) {
        self.counts
    }

    /// Compteur du bas de page : mots et caractères de la note, ou ceux de la sélection. Recompté seulement quand le texte ou la sélection changent.
    pub fn counts(&mut self) -> String {
        // Sans sélection, déplacer le curseur ne change rien au compte.
        let key = (self.version, (!self.sel.is_empty()).then(|| self.sel.clone()));
        if let Some((_, label)) = self.counted.as_ref().filter(|(known, _)| *known == key) {
            return label.clone();
        }
        let plural = |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
        let both = |(words, chars): (usize, usize)| {
            format!(
                "{} · {}",
                plural(words, tr("word", "mot"), tr("words", "mots")),
                plural(chars, tr("character", "caractère"), tr("characters", "caractères"))
            )
        };
        let label = if self.sel.is_empty() {
            let whole = match &self.tally {
                Some((version, _, whole)) if *version == self.version => *whole,
                Some((_, old, was)) => md::recount(old, *was, &self.content),
                None => md::counts(&self.content),
            };
            if self.tally.as_ref().is_none_or(|(version, ..)| *version != self.version) {
                // ponytail: une copie du texte pour le prochain recompte ; tenir les comptes dans `splice` si elle pèse.
                self.tally = Some((self.version, self.content.clone(), whole));
            }
            both(whole)
        } else {
            format!("{} {}", tr("Selection:", "Sélection :"), both(md::counts(&self.content[self.sel.clone()])))
        };
        self.counted = Some((key, label.clone()));
        label
    }

    /// Type de la ligne qui porte l'octet `i`, à la dernière mise en page.
    #[cfg(test)]
    pub fn kind_at(&self, i: usize) -> Option<Kind> {
        Some(self.rows[self.row_at(i)?].kind)
    }

    /// Position du curseur dans le texte.
    pub fn position(&self) -> usize {
        self.cursor()
    }

    /// Où l'on en est dans la note : le curseur et le défilement, pour y revenir.
    pub fn view(&self) -> (usize, f32) {
        (self.cursor(), self.scroll_y.into())
    }

    /// Revient où l'on en était : le défilement est repris tel quel, pas recalculé d'après le
    /// curseur.
    pub fn set_view(&mut self, at: usize, scroll: f32, cx: &mut Context<Self>) {
        self.more.clear();
        self.move_to(self.clamp(at), cx);
        self.scroll_y = px(scroll);
        self.reveal = false;
    }

    /// Place le curseur à `at` ; avec `top`, sa ligne monte en haut de la vue (un titre garde
    /// sa section sous lui), sinon elle défile juste assez pour se voir.
    pub fn jump(&mut self, at: usize, top: bool, cx: &mut Context<Self>) {
        self.more.clear();
        self.move_to(self.clamp(at), cx);
        self.reveal_top = top;
    }

    /// Résultat de la recherche du coffre : à la ligne de rang `row`, sélectionne le premier
    /// passage qui vaut `query` (sans la casse) ; à défaut, le curseur va au début de la ligne.
    pub fn select_in_row(&mut self, row: usize, query: &str, cx: &mut Context<Self>) {
        let start: usize = self.content.split_inclusive('\n').take(row).map(str::len).sum();
        let line = self.line_range(start);
        self.jump(line.start, false, cx);
        if let Some(hit) = md::find(&self.content[line.clone()], query, false, false).first() {
            self.sel = line.start + hit.start..line.start + hit.end;
        }
    }

    /// Recherche, numéro du passage courant (à partir de 1) et nombre de passages.
    #[cfg(test)]
    pub fn finding(&self) -> Option<(&str, usize, usize)> {
        let find = self.find.as_ref()?;
        Some((&find.query, if find.hits.is_empty() { 0 } else { find.at + 1 }, find.hits.len()))
    }

    /// Lignes de tirets de tableau qu'on ne voit pas.
    #[cfg(test)]
    pub fn hidden_rows(&self) -> usize {
        self.rows.iter().filter(|r| r.hidden()).count()
    }

    #[cfg(test)]
    pub fn span(&self) -> Range<usize> {
        self.sel.clone()
    }

    #[cfg(test)]
    pub fn selected(&self) -> &str {
        &self.content[self.sel.clone()]
    }

    /// Premier tableau : de combien il a défilé, plus grand nombre de lignes visuelles d'une
    /// des siennes, son cadre, la largeur du texte et la vue.
    #[cfg(test)]
    pub fn table_state(&self) -> Option<(Pixels, i32, Bounds<Pixels>, Pixels, Bounds<Pixels>)> {
        let rows = self.rows.iter().filter(|r| r.kind == Kind::Table);
        let tall = rows.clone().map(Row::visual_rows).max()?;
        let frame = self.tables.first()?.1;
        let scrolled = rows.map(|r| r.dx).next()? - table_lead(self.theme.wide, frame.size.width, self.width);
        Some((scrolled, tall, frame, self.width, self.viewport))
    }

    /// Centre de la poignée `part` du premier tableau.
    #[cfg(test)]
    pub fn part_center(&self, part: Part) -> Option<Point<Pixels>> {
        self.geos.first()?.parts().into_iter().find(|(p, _)| *p == part).map(|(_, zone)| zone.center())
    }

    /// Centre d'une case du sélecteur de tableau, s'il est ouvert.
    #[cfg(test)]
    pub fn grid_cell(&self, col: usize, row: usize) -> Option<Point<Pixels>> {
        self.grid?;
        let step = |n: usize| px(16.) + GRID_STEP * (n - 1) as f32;
        Some(self.popup_at(Self::grid_size())? + point(step(col), step(row)))
    }

    fn cursor(&self) -> usize {
        if self.reversed { self.sel.start } else { self.sel.end }
    }

    fn clamp(&self, mut i: usize) -> usize {
        i = i.min(self.content.len());
        while !self.content.is_char_boundary(i) {
            i -= 1;
        }
        i
    }

    fn line_range(&self, at: usize) -> Range<usize> {
        let start = self.content[..at].rfind('\n').map_or(0, |i| i + 1);
        let end = self.content[at..].find('\n').map_or(self.content.len(), |i| at + i);
        start..end
    }

    fn fences_before(&self, at: usize) -> usize {
        self.content[..at].lines().filter(|l| md::is_fence(l)).count()
    }

    /// Un ``` sans clôture n'ouvre pas de bloc, comme dans `layout`.
    fn in_code(&self, line_start: usize) -> bool {
        self.fences_before(line_start) % 2 == 1
            && self.content[line_start..].lines().any(md::is_fence)
    }

    fn move_to(&mut self, to: usize, cx: &mut Context<Self>) {
        self.swatches = false;
        self.bar_open = false;
        self.sel = to..to;
        self.reversed = false;
        self.goal_x = None;
        self.grid = None;
        self.reveal = true;
        cx.notify();
    }

    fn select_to(&mut self, to: usize, cx: &mut Context<Self>) {
        if self.reversed {
            self.sel.start = to
        } else {
            self.sel.end = to
        }
        if self.sel.end < self.sel.start {
            self.reversed = !self.reversed;
            self.sel = self.sel.end..self.sel.start;
        }
        self.goal_x = None;
        self.reveal = true;
        cx.notify();
    }

    // ----- Recherche dans la note -----

    /// Ouvre la barre de recherche, ou lui rend la saisie ; `replace` montre aussi la ligne
    /// « Remplacer ». Une sélection tenue sur une ligne devient la recherche.
    fn open_find(&mut self, replace: bool, cx: &mut Context<Self>) {
        let picked = Some(&self.content[self.sel.clone()]).filter(|s| !s.is_empty() && !s.contains('\n'));
        let picked = picked.map(String::from);
        let find = self.find.get_or_insert_with(Finder::default);
        find.active = true;
        find.on_with = false;
        if let Some(picked) = picked {
            (find.query, find.fresh) = (picked, true);
        }
        if replace {
            find.with.get_or_insert_with(String::new);
        }
        self.refresh_find();
        cx.notify();
    }

    /// Refait la liste des passages si le texte, la recherche ou ses réglages ont changé (et
    /// le dit) ; le passage courant est alors le premier à partir du curseur.
    fn refresh_find(&mut self) -> bool {
        let (version, from) = (self.version, self.sel.start);
        let Some(find) = &mut self.find else {
            return false;
        };
        let key = (version, find.query.clone(), find.case, find.word, find.regex);
        if find.key.as_ref() == Some(&key) {
            return false;
        }
        find.hits = md::replacements(&self.content, &find.query, "", find.case, find.word, find.regex).into_iter().map(|(hit, _)| hit).collect();
        find.at = find.hits.iter().position(|hit| hit.start >= from).unwrap_or(0);
        find.key = Some(key);
        true
    }

    /// Sélectionne le passage courant et le fait défiler à l'écran.
    fn show_hit(&mut self, cx: &mut Context<Self>) {
        if let Some(hit) = self.find.as_ref().and_then(|find| find.hits.get(find.at).cloned()) {
            self.sel = hit;
            self.reversed = false;
            self.more.clear();
            self.goal_x = None;
            self.grid = None;
            self.reveal = true;
        }
        cx.notify();
    }

    /// Passage suivant ou précédent, en repartant de l'autre bout une fois au bord.
    fn find_step(&mut self, forward: bool, cx: &mut Context<Self>) {
        // F3 sans barre : elle s'ouvre, comme avec Ctrl+F.
        if self.find.is_none() {
            return self.open_find(false, cx);
        }
        self.refresh_find();
        if let Some(find) = &mut self.find
            && let n @ 1.. = find.hits.len()
        {
            find.at = if forward { (find.at + 1) % n } else { (find.at + n - 1) % n };
        }
        self.show_hit(cx);
    }

    /// Entrée dans la barre : passage suivant, ou remplacement du passage courant quand la
    /// saisie est dans le champ « Remplacer ».
    fn find_enter(&mut self, cx: &mut Context<Self>) {
        self.refresh_find();
        let current = self.find.as_ref().filter(|find| find.on_with).and_then(|find| Some((find.hits.get(find.at)?.clone(), self.replacements()?)));
        // Le texte qui remplace ce passage : les groupes d'une expression régulière en font partie.
        let Some((hit, with)) = current.and_then(|(hit, all)| Some((hit.clone(), all.into_iter().find(|(range, _)| *range == hit)?.1))) else {
            return self.find_step(true, cx);
        };
        self.edit(hit, &with, cx);
        self.realign(cx);
        self.refresh_find();
        self.show_hit(cx);
    }

    /// Les passages trouvés et ce qui remplace chacun ; `None` sans champ « Remplacer ».
    fn replacements(&self) -> Option<Vec<(Range<usize>, String)>> {
        let find = self.find.as_ref()?;
        Some(md::replacements(&self.content, &find.query, find.with.as_ref()?, find.case, find.word, find.regex))
    }

    /// Demande à la fenêtre de remplacer dans tout le coffre : la recherche, le remplacement et
    /// les réglages de la barre.
    fn replace_in_vault(&mut self, cx: &mut Context<Self>) {
        if let Some(find) = self.find.as_ref().filter(|find| !find.query.is_empty())
            && let Some(with) = find.with.clone()
        {
            cx.emit(EditorEvent::Swap(md::Swap { query: find.query.clone(), with, case: find.case, word: find.word, regex: find.regex }));
        }
    }

    /// Remplace tous les passages d'un coup : une seule étape d'annulation.
    fn replace_all(&mut self, cx: &mut Context<Self>) {
        self.refresh_find();
        let Some(hits) = self.replacements() else {
            return;
        };
        let Some(first) = hits.first().map(|(hit, _)| hit.start) else {
            return;
        };
        let mut text = String::with_capacity(self.content.len());
        let mut done = 0;
        for (hit, with) in &hits {
            text.push_str(&self.content[done..hit.start]);
            text.push_str(with);
            done = hit.end;
        }
        text.push_str(&self.content[done..]);
        self.last_edit = None;
        self.splice(0..self.content.len(), &text, cx);
        self.sel = first..first;
        self.reversed = false;
        self.refresh_find();
    }

    /// Champ de la barre qui reçoit la saisie.
    fn find_field(&mut self) -> Option<&mut String> {
        let find = self.find.as_mut()?;
        if find.on_with {
            return find.with.as_mut();
        }
        if std::mem::take(&mut find.fresh) {
            find.query.clear();
        }
        Some(&mut find.query)
    }

    // ponytail: comme la palette, la saisie de la barre se réduit à « ajouter à la fin » (pas
    // de curseur mobile ni de composition IME) ; à remplacer par un vrai champ de texte si
    // les recherches s'allongent.
    fn find_type(&mut self, text: &str, cx: &mut Context<Self>) {
        if let Some(field) = self.find_field() {
            field.extend(text.chars().filter(|c| !c.is_control()));
        }
        self.find_changed(cx);
    }

    fn find_erase(&mut self, cx: &mut Context<Self>) {
        if let Some(field) = self.find_field() {
            let last = field.graphemes(true).next_back().map_or(0, str::len);
            field.truncate(field.len() - last);
        }
        self.find_changed(cx);
    }

    /// Après un changement de la recherche ou de ses réglages : le premier passage se montre.
    fn find_changed(&mut self, cx: &mut Context<Self>) {
        if self.refresh_find() { self.show_hit(cx) } else { cx.notify() }
    }

    /// La barre de recherche, en haut à droite de la note (sous les boutons de la fenêtre).
    fn find_bar(&self, cx: &mut Context<Self>) -> Option<gpui::Div> {
        let (t, find) = (self.theme, self.find.as_ref()?);
        let field = |text: &str, hint: &'static str, on: bool, fresh: bool| {
            let caret = on.then(|| div().flex_none().w(px(1.)).h(px(15.)).bg(t.text));
            let shown = match text.is_empty() {
                true => div().text_color(t.soft).child(hint),
                false => div().when(fresh, |d| d.bg(t.selection)).child(text.to_string()),
            };
            let field = div().flex_1().min_w(px(90.)).h(px(26.)).px_2().rounded(px(6.)).border_1().bg(t.bg).cursor(CursorStyle::IBeam);
            let field = field.border_color(if on { t.accent } else { t.border }).flex().items_center().overflow_hidden();
            match text.is_empty() {
                true => field.children(caret).child(shown),
                false => field.child(shown).children(caret),
            }
        };
        let toggle = |label: &'static str, on: bool| {
            let toggle = div().flex_none().h(px(26.)).px_1p5().rounded(px(6.)).flex().items_center().cursor_pointer();
            toggle.text_color(if on { t.accent } else { t.soft }).when(on, |d| d.bg(t.selection)).child(label)
        };
        let count = match (find.query.is_empty(), find.hits.len()) {
            (true, _) => String::new(),
            (false, 0) => tr("No results", "Aucun résultat").into(),
            (false, n) => format!("{} / {n}", find.at + 1),
        };
        let pick = |on_with: bool| {
            cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                if let Some(find) = &mut this.find {
                    (find.active, find.on_with) = (true, on_with);
                }
                cx.notify();
            })
        };
        let flip = |which: u8| {
            cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                if let Some(find) = &mut this.find {
                    *[&mut find.case, &mut find.word, &mut find.regex][which as usize] ^= true;
                }
                this.find_changed(cx);
            })
        };
        let bar = div()
            .debug_selector(|| "find-bar".into())
            .w(px(340.))
            .max_w_full()
            .p_1p5()
            .rounded(px(8.))
            .bg(t.panel)
            .border_1()
            .border_color(t.border)
            .text_size(px(13.))
            .text_color(t.text)
            .cursor(CursorStyle::Arrow)
            .flex()
            .flex_col()
            .gap_1p5()
            .occlude();
        // Dans une note étroite, le compte et les réglages passent sous le champ.
        let first = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_1p5()
            .child(field(&find.query, tr("Find", "Chercher"), find.active && !find.on_with, find.fresh).on_mouse_down(MouseButton::Left, pick(false)))
            .child(div().flex_none().text_size(px(12.)).text_color(t.soft).child(count))
            .child(toggle("Aa", find.case).on_mouse_down(MouseButton::Left, flip(0)))
            .child(toggle("\u{201c}ab\u{201d}", find.word).on_mouse_down(MouseButton::Left, flip(1)))
            .child(toggle(".*", find.regex).on_mouse_down(MouseButton::Left, flip(2)));
        let second = find.with.as_ref().map(|with| {
            let everywhere = cx.listener(|this, _: &MouseDownEvent, _, cx| this.replace_in_vault(cx));
            div()
                .flex()
                .items_center()
                .gap_1p5()
                .child(
                    field(with, tr("Replace with", "Remplacer par"), find.active && find.on_with, false)
                        .on_mouse_down(MouseButton::Left, pick(true)),
                )
                .child(toggle(tr("Vault…", "Coffre…"), false).on_mouse_down(MouseButton::Left, everywhere))
        });
        // Calée à droite, dans les marges de la note : une note étroite la rétrécit au lieu de la laisser sortir.
        Some(div().absolute().top(px(52.)).left_4().right_4().flex().justify_end().child(bar.child(first).children(second)))
    }

    // ponytail: chaque étape d'annulation (200 max) recopie une fois la note pour
    // en tirer la différence ; passer à un journal d'opérations si les notes
    // dépassent le Mo.
    fn push_undo(&mut self, boundary: bool) {
        let stale = self
            .last_edit
            .is_none_or(|t| t.elapsed() > Duration::from_millis(600));
        if (boundary || stale) && !self.held {
            self.undo.push(self.content.clone(), self.sel.clone());
        }
        self.redo = History::default();
        self.last_edit = Some(Instant::now());
    }

    /// À appeler avant de remplacer `range` dans le texte : la mise en page saura ce qui a bougé.
    fn damaged(&mut self, range: &Range<usize>) {
        let len = self.content.len();
        let (prefix, suffix) = (range.start, len - range.end);
        self.dmg = Some(match self.dmg {
            None => Damage { old_len: len, prefix, suffix },
            Some(d) => Damage { prefix: d.prefix.min(prefix), suffix: d.suffix.min(suffix), ..d },
        });
    }

    fn changed(&mut self, cx: &mut Context<Self>) {
        self.swatches = false;
        self.version += 1;
        self.revision += 1;
        self.marked = None;
        self.copied = None;
        self.grid = None;
        self.ac_index = 0;
        self.goal_x = None;
        self.reveal = true;
        cx.emit(EditorEvent::Changed);
        cx.notify();
    }

    /// Remplace `range` par `text` en conservant la sélection au mieux.
    fn splice(&mut self, range: Range<usize>, text: &str, cx: &mut Context<Self>) {
        self.push_undo(text.contains('\n') || text == " ");
        self.damaged(&range);
        self.content.replace_range(range.clone(), text);
        let map = |p: usize| {
            if p <= range.start {
                p
            } else if p >= range.end {
                p + text.len() - range.len()
            } else {
                range.start + text.len()
            }
        };
        self.sel = map(self.sel.start)..map(self.sel.end);
        // Les autres curseurs suivent le texte : c'est ici, et ici seulement, qu'il change.
        self.more.iter_mut().for_each(|(sel, _)| *sel = map(sel.start)..map(sel.end));
        self.changed(cx);
    }

    // ----- Plusieurs curseurs -----

    /// Fait faire `act` à chaque curseur : chacun devient à son tour le curseur principal,
    /// pendant que `splice` tient les autres à leur place. Le tout ne fait qu'une étape
    /// d'annulation, celle du premier.
    // ponytail: la colonne visée par Haut et Bas (`goal_x`) n'est pas gardée par curseur : elle
    // est recalculée à chaque pas. La retenir par curseur si la dérive gêne sur des lignes
    // de longueurs inégales.
    fn each(&mut self, cx: &mut Context<Self>, act: impl Fn(&mut Self, &mut Context<Self>)) {
        if self.more.is_empty() {
            return act(self, cx);
        }
        self.goal_x = None;
        act(self, cx);
        self.held = true;
        for i in 0..self.more.len() {
            // Un geste qui ramène à un seul curseur (une note rechargée) arrête la ronde.
            if i >= self.more.len() {
                break;
            }
            let swap = |this: &mut Self| {
                std::mem::swap(&mut this.sel, &mut this.more[i].0);
                std::mem::swap(&mut this.reversed, &mut this.more[i].1);
            };
            swap(self);
            self.goal_x = None;
            act(self, cx);
            if i < self.more.len() {
                swap(self);
            }
        }
        self.held = false;
        self.goal_x = None;
        self.tidy();
        cx.notify();
    }

    /// Les pastilles de couleur sont ouvertes.
    #[cfg(test)]
    pub fn swatches_open(&self) -> bool {
        self.swatches
    }

    /// Le texte sélectionné.
    #[cfg(test)]
    pub fn selected_text(&self) -> String {
        self.content[self.sel.clone()].to_string()
    }

    /// Nombre de curseurs, le principal compris.
    #[cfg(test)]
    pub fn cursors(&self) -> usize {
        self.more.len() + 1
    }

    /// Range les autres curseurs dans l'ordre du texte, et retire ceux qui en touchent un autre.
    fn tidy(&mut self) {
        self.more.sort_by_key(|(sel, _)| sel.start);
        let (mut kept, main): (Vec<(Range<usize>, bool)>, _) = (Vec::new(), self.sel.clone());
        let touch = |a: &Range<usize>, b: &Range<usize>| a.start <= b.end && b.start <= a.end;
        for cursor in std::mem::take(&mut self.more) {
            if !touch(&cursor.0, &main) && kept.last().is_none_or(|last| !touch(&last.0, &cursor.0)) {
                kept.push(cursor);
            }
        }
        self.more = kept;
    }

    /// Alt+clic : un curseur de plus à l'octet `at`, qui devient le principal. Pas dans un
    /// tableau, qui a ses propres gestes.
    fn add_cursor(&mut self, at: usize, cx: &mut Context<Self>) {
        if self.table_at(at).is_some() || self.table_at(self.sel.start).is_some() {
            return self.move_to(at, cx);
        }
        self.more.push((self.sel.clone(), self.reversed));
        self.move_to(at, cx);
        self.tidy();
    }

    /// Sélectionne le mot sous le curseur ; puis, à chaque appel, l'occurrence suivante de la
    /// sélection reçoit un curseur de plus (elle se cherche en boucle, la casse compte).
    fn select_next(&mut self, cx: &mut Context<Self>) {
        if self.sel.is_empty() {
            let word = self.unit_at(self.sel.start, false);
            self.move_to(word.start, cx);
            return self.select_to(word.end, cx);
        }
        let text = self.content[self.sel.clone()].to_string();
        let taken = |at: usize| self.sel.start == at || self.more.iter().any(|(sel, _)| sel.start == at);
        let after = self.content[self.sel.end..].match_indices(&text).map(|(i, _)| i + self.sel.end);
        let before = self.content[..self.sel.start].match_indices(&text).map(|(i, _)| i);
        let Some(at) = after.chain(before).find(|at| !taken(*at)) else {
            return;
        };
        self.more.push((self.sel.clone(), self.reversed));
        self.move_to(at, cx);
        self.select_to(at + text.len(), cx);
        self.tidy();
    }

    /// Remplace `range` par `text` et place le curseur juste après.
    fn edit(&mut self, range: Range<usize>, text: &str, cx: &mut Context<Self>) {
        let end = range.start + text.len();
        self.splice(range, text, cx);
        self.sel = end..end;
        self.reversed = false;
    }

    fn renumber(&mut self, cx: &mut Context<Self>) {
        if let Some((text, cursor)) = md::renumber(&self.content, self.cursor()) {
            self.epoch += 1;
            self.content = text;
            self.sel = cursor..cursor;
            self.reversed = false;
            self.more.clear();
            self.changed(cx);
        }
    }

    pub fn restore(&mut self, redo: bool, cx: &mut Context<Self>) {
        let (from, to) = if redo {
            (&mut self.redo, &mut self.undo)
        } else {
            (&mut self.undo, &mut self.redo)
        };
        let Some((text, sel)) = from.pop() else {
            return;
        };
        self.epoch += 1;
        to.push(std::mem::replace(&mut self.content, text), self.sel.clone());
        self.sel = sel;
        self.reversed = false;
        self.more.clear();
        self.last_edit = None;
        self.changed(cx);
    }

    // ----- Déplacements -----

    fn row_at(&self, i: usize) -> Option<usize> {
        self.rows
            .partition_point(|r| r.start <= i)
            .checked_sub(1)
    }

    /// Ligne voisine qu'on voit : la ligne de tirets d'un tableau se saute.
    fn neighbour(&self, r: usize, down: bool) -> Option<usize> {
        let step = |r: usize| if down { Some(r + 1) } else { r.checked_sub(1) };
        let mut r = step(r)?;
        while self.rows.get(r)?.hidden() {
            r = step(r)?;
        }
        Some(r)
    }

    fn vertical(&self, from: usize, down: bool, gx: Pixels) -> usize {
        let Some(r) = self.row_at(from) else {
            return from;
        };
        let row = &self.rows[r];
        let v = (f32::from(row.pos(from - row.start).y) / f32::from(row.lh)).round() as i32;
        // Dans une cellule, on descend ligne visuelle après ligne visuelle, puis on passe à la ligne suivante.
        let (r, v) = if down {
            if v + 1 < row.lines_at(from - row.start) {
                (r, v + 1)
            } else if let Some(next) = self.neighbour(r, true) {
                (next, 0)
            } else {
                return self.content.len();
            }
        } else if v > 0 {
            (r, v - 1)
        } else if let Some(prev) = self.neighbour(r, false) {
            (prev, self.rows[prev].visual_rows() - 1)
        } else {
            return 0;
        };
        let row = &self.rows[r];
        self.clamp(row.start + row.index_at(point(gx, row.lh * (v as f32 + 0.5))))
    }

    fn is_table_line(&self, line: &Range<usize>) -> bool {
        self.content[line.clone()].trim_start().starts_with('|') && !self.in_code(line.start)
    }

    /// Ligne de tableau qui précède celle qui commence en `start`, ou la suit.
    fn next_line(&self, line: &Range<usize>, down: bool) -> Option<Range<usize>> {
        if down {
            (line.end < self.content.len()).then(|| self.line_range(line.end + 1))
        } else {
            (line.start > 0).then(|| self.line_range(line.start - 1))
        }
    }

    /// La ligne est la ligne de tirets d'un tableau : la deuxième, quand elle n'a que des tirets.
    fn is_dash_row(&self, line: &Range<usize>) -> bool {
        let first = self.next_line(line, false).filter(|l| self.is_table_line(l));
        let none_before = first.as_ref().is_some_and(|f| self.next_line(f, false).is_none_or(|l| !self.is_table_line(&l)));
        let text = &self.content[line.clone()];
        none_before && md::is_dashes(&md::cells(text).into_iter().map(|r| text[r].to_string()).collect::<Vec<_>>())
    }

    /// Cellules (octets du texte) de la ligne de tableau qui contient `c`, et celle où
    /// l'on est : `None` hors d'un tableau, et sur la ligne de tirets, qui s'affiche comme du texte.
    fn table_cells(&self, c: usize) -> Option<(Vec<Range<usize>>, usize)> {
        let c = self.clamp(c);
        let line = self.line_range(c);
        if !self.is_table_line(&line) || self.is_dash_row(&line) {
            return None;
        }
        let text = &self.content[line.clone()];
        let cells: Vec<Range<usize>> = md::cells(text).into_iter().map(|r| line.start + r.start..line.start + r.end).collect();
        let before = md::bars(text).into_iter().filter(|&bar| line.start + bar < c).count();
        let k = before.saturating_sub(1).min(cells.len().checked_sub(1)?);
        Some((cells, k))
    }

    /// Cellules de la ligne de tableau voisine, la ligne de tirets sautée.
    fn table_neighbour(&self, c: usize, down: bool) -> Option<Vec<Range<usize>>> {
        let mut line = self.next_line(&self.line_range(self.clamp(c)), down)?;
        if self.is_table_line(&line) && self.is_dash_row(&line) {
            line = self.next_line(&line, down)?;
        }
        self.table_cells(line.start).map(|(cells, _)| cells)
    }

    /// La sélection couvre plusieurs cellules d'un tableau (ou des `|`) : le texte de chacune
    /// des cellules qu'elle touche. `None` si elle tient dans une cellule, ou sort du tableau.
    fn selected_cells(&self) -> Option<Vec<Range<usize>>> {
        self.cells_in(self.sel.clone())
    }

    /// Comme `selected_cells`, pour la sélection `sel`.
    fn cells_in(&self, sel: Range<usize>) -> Option<Vec<Range<usize>>> {
        if sel.is_empty() {
            return None;
        }
        let (first, last) = (self.line_range(sel.start), self.line_range(sel.end));
        let (mut found, mut bars, mut line) = (Vec::new(), false, first.clone());
        loop {
            if !self.is_table_line(&line) {
                return None;
            }
            if !self.is_dash_row(&line) {
                let text = &self.content[line.clone()];
                let inside = |at: usize| (sel.start..sel.end).contains(&at);
                bars |= md::bars(text).into_iter().any(|bar| inside(line.start + bar));
                for cell in md::cells(text) {
                    let (from, to) = (line.start + cell.start, line.start + cell.end);
                    let (from, to) = (from.max(sel.start), to.min(sel.end));
                    if from < to {
                        found.push(self.off_escape(from, false)..self.off_escape(to, true));
                    }
                }
            }
            if line.end >= last.end {
                break;
            }
            line = self.line_range(line.end + 1);
        }
        (first != last || bars || found.len() > 1).then_some(found)
    }

    /// Vide les cellules de la sélection, `|` compris dans la sélection : elles gardent leur
    /// place, ce qu'effacer d'un bloc fusionnerait. Faux si la sélection n'en couvre pas plusieurs.
    fn clear_cells(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(ranges) = self.selected_cells() else {
            return false;
        };
        let at = ranges.first().map_or(self.sel.start, |r| r.start);
        if !ranges.is_empty() {
            self.push_undo(true);
            for r in ranges.iter().rev() {
                self.damaged(r);
                self.content.replace_range(r.clone(), "");
            }
        }
        self.sel = at..at;
        self.reversed = false;
        self.changed(cx);
        self.realign(cx);
        true
    }

    /// Un octet qui ne coupe pas un `\|` : la grille l'affiche d'un seul signe.
    fn off_escape(&self, t: usize, forward: bool) -> usize {
        let b = self.content.as_bytes();
        let backslashes = b[..t].iter().rev().take_while(|&&x| x == b'\\').count();
        if t < b.len() && b[t] == b'|' && backslashes % 2 == 1 {
            if forward { t + 1 } else { t - 1 }
        } else {
            t
        }
    }

    /// Déplacement horizontal dans une ligne de tableau : on saute les `|` et les espaces
    /// entre les cellules, et on passe d'une ligne à l'autre sans s'arrêter sur les tirets.
    fn table_move(&self, c: usize, m: Motion) -> Option<usize> {
        let (cells, k) = self.table_cells(c)?;
        let cell = cells[k].clone();
        let at = c.clamp(cell.start, cell.end);
        let line = self.line_range(self.clamp(c));
        let prev_cell = || match k {
            0 => self.table_neighbour(c, false).and_then(|cells| Some(cells.last()?.end)).unwrap_or(line.start.saturating_sub(1)),
            _ => cells[k - 1].end,
        };
        let next_cell = || match cells.get(k + 1) {
            Some(next) => next.start,
            None => {
                let first = self.table_neighbour(c, true).and_then(|cells| Some(cells.first()?.start));
                first.unwrap_or((line.end + 1).min(self.content.len()))
            }
        };
        let inside = &self.content[cell.clone()];
        Some(match m {
            Motion::Left if at > cell.start => {
                let t = cell.start + inside[..at - cell.start].grapheme_indices(true).next_back().map_or(0, |(i, _)| i);
                self.off_escape(t, false)
            }
            Motion::Right if at < cell.end => {
                let t = at + inside[at - cell.start..].graphemes(true).next().map_or(0, str::len);
                self.off_escape(t, true)
            }
            Motion::WordLeft if at > cell.start => cell.start
                + inside[..at - cell.start]
                    .split_word_bound_indices()
                    .rev()
                    .find(|(_, w)| !w.trim().is_empty())
                    .map_or(0, |(i, _)| i),
            Motion::WordRight if at < cell.end => at
                + inside[at - cell.start..]
                    .split_word_bound_indices()
                    .find(|(_, w)| !w.trim().is_empty())
                    .map_or(cell.end - at, |(i, w)| i + w.len()),
            Motion::Left | Motion::WordLeft => prev_cell(),
            Motion::Right | Motion::WordRight => next_cell(),
            Motion::Home => cells[0].start,
            Motion::End => cells[cells.len() - 1].end,
            _ => return None,
        })
    }

    fn target(&self, m: Motion, gx: Pixels) -> usize {
        self.table_move(self.cursor(), m).unwrap_or_else(|| self.plain_target(m, gx))
    }

    /// Où mène le déplacement `m`, en ne comptant que le texte.
    fn plain_target(&self, m: Motion, gx: Pixels) -> usize {
        let c = self.cursor();
        let lr = self.line_range(c);
        let line = &self.content[lr.clone()];
        let col = c - lr.start;
        let page = (f32::from(self.viewport.size.height) / 26.) as usize;
        match m {
            Motion::Left if col == 0 => c.saturating_sub(1),
            Motion::Left => {
                lr.start + line[..col].grapheme_indices(true).next_back().map_or(0, |(i, _)| i)
            }
            Motion::Right if c == lr.end => (c + 1).min(self.content.len()),
            Motion::Right => c + line[col..].graphemes(true).next().map_or(0, str::len),
            Motion::WordLeft => self.content[..c]
                .split_word_bound_indices()
                .rev()
                .find(|(_, w)| !w.trim().is_empty())
                .map_or(0, |(i, _)| i),
            Motion::WordRight => self.content[c..]
                .split_word_bound_indices()
                .find(|(_, w)| !w.trim().is_empty())
                .map_or(self.content.len(), |(i, w)| c + i + w.len()),
            Motion::Home => lr.start,
            Motion::End => lr.end,
            Motion::DocStart => 0,
            Motion::DocEnd => self.content.len(),
            Motion::Up => self.vertical(c, false, gx),
            Motion::Down => self.vertical(c, true, gx),
            Motion::PageUp => (0..page).fold(c, |c, _| self.vertical(c, false, gx)),
            Motion::PageDown => (0..page).fold(c, |c, _| self.vertical(c, true, gx)),
        }
    }

    fn go(&mut self, m: Motion, select: bool, cx: &mut Context<Self>) {
        // Sélecteur de tableau : les flèches règlent sa taille.
        if let Some((cols, rows)) = &mut self.grid {
            match m {
                Motion::Left => *cols = (*cols - 1).max(1),
                Motion::Right => *cols = (*cols + 1).min(GRID_MAX.0),
                Motion::Up => *rows = (*rows - 1).max(1),
                Motion::Down => *rows = (*rows + 1).min(GRID_MAX.1),
                _ => self.grid = None,
            }
            self.grid_typed.clear();
            return cx.notify();
        }
        if !select
            && matches!(m, Motion::Up | Motion::Down)
            && let Some((_, items)) = self.completion()
        {
            let n = items.len();
            let i = self.ac_index.min(n - 1);
            self.ac_index = if m == Motion::Down { (i + 1) % n } else { (i + n - 1) % n };
            cx.notify();
            return;
        }
        let c = self.cursor();
        let gx = self
            .goal_x
            .unwrap_or_else(|| self.row_at(c).map_or(px(0.), |r| {
                let row = &self.rows[r];
                row.pos(c.saturating_sub(row.start)).x
            }));
        let to = match m {
            Motion::Left if !select && !self.sel.is_empty() => self.sel.start,
            Motion::Right if !select && !self.sel.is_empty() => self.sel.end,
            _ => self.target(m, gx),
        };
        if select {
            self.select_to(to, cx)
        } else {
            self.move_to(to, cx)
        }
        if matches!(m, Motion::Up | Motion::Down | Motion::PageUp | Motion::PageDown) {
            self.goal_x = Some(gx);
        }
    }

    /// Sélecteur de tableau ouvert : `12x5` tapé au clavier donne sa taille, au-delà
    /// de la grille. Renvoie faux si `text` n'est pas une taille.
    fn type_grid(&mut self, text: &str, cx: &mut Context<Self>) -> bool {
        let Some((cols, rows)) = self.grid else {
            return false;
        };
        if !text.chars().all(|c| c.is_ascii_digit() || matches!(c, 'x' | 'X' | '×' | '*' | ' ')) {
            return false;
        }
        self.grid_typed.push_str(text);
        let mut sizes = self.grid_typed.split(|c: char| !c.is_ascii_digit()).filter_map(|n| n.parse::<usize>().ok());
        let cols = sizes.next().unwrap_or(cols).clamp(1, GRID_MAX.0);
        let rows = sizes.next().unwrap_or(rows).clamp(1, GRID_MAX.1);
        self.grid = Some((cols, rows));
        cx.notify();
        true
    }

    fn delete(&mut self, m: Motion, cx: &mut Context<Self>) {
        if self.clear_pick(cx) {
            return;
        }
        if self.grid.is_some() {
            self.grid_typed.pop();
            self.type_grid("", cx);
            return;
        }
        if self.clear_cells(cx) {
            return;
        }
        let range = if self.sel.is_empty() {
            let (c, t) = (self.cursor(), self.plain_target(m, px(0.)));
            let mut range = c.min(t)..c.max(t);
            // Dans un tableau, effacer ne passe pas d'une cellule à l'autre : cela fusionnerait
            // les colonnes. Un `\|` s'efface d'un bloc, comme il s'affiche.
            if let Some((_, k)) = self.table_cells(c) {
                let line = self.line_range(c);
                let bars = md::bars(&self.content[line.clone()]);
                let (from, to) = (line.start + bars[k] + 1, bars.get(k + 1).map_or(line.end, |b| line.start + b));
                range = range.start.max(from)..range.end.min(to);
                if range.is_empty() {
                    return;
                }
                range = self.off_escape(range.start, false)..self.off_escape(range.end, true);
            }
            range
        } else {
            self.sel.clone()
        };
        self.edit(range, "", cx);
        self.renumber(cx);
        self.realign(cx);
    }

    // ----- Édition Markdown -----

    fn newline(&mut self, cx: &mut Context<Self>) {
        if self.accept_completion(cx) {
            return;
        }
        if let Some((cols, rows)) = self.grid {
            return self.insert_table(cols, rows, cx);
        }
        // Dans un tableau : une ligne de plus sous celle du curseur ; sur une
        // dernière ligne restée vide, on la retire et on sort du tableau.
        // Un très gros tableau n'est pas relu : Entrée y ajoute une ligne vide sous la courante (sous les tirets
        // quand on est sur l'en-tête), sans toucher au reste.
        if self.sel.is_empty() && self.in_big(self.cursor()) {
            let line = self.line_range(self.cursor());
            let cells = md::cells(&self.content[line.clone()]).len().max(1);
            let after = match self.row_at(self.cursor()).and_then(|r| self.big.iter().find(|b| b.rows.contains(&r)).map(|b| r - b.rows.start)) {
                Some(0) if line.end < self.content.len() => self.line_range(line.end + 1).end,
                _ => line.end,
            };
            self.push_undo(true);
            self.edit(after..after, &format!("\n|{}", " |".repeat(cells)), cx);
            return self.move_to(after + 3, cx);
        }
        if let Some((range, mut rows, (r, _))) = self.table_at(self.cursor()) {
            if r >= 2 && r + 1 == rows.len() && rows[r].iter().all(String::is_empty) {
                rows.pop();
                let text = format!("{}\n", self.format_table(&range, &rows));
                self.push_undo(true);
                return self.edit(range, &text, cx);
            }
            let at = r.max(1) + 1;
            rows.insert(at, vec![String::new(); rows[0].len()]);
            return self.set_table(range, &rows, (at, 0), cx);
        }
        if !self.sel.is_empty() {
            return self.edit(self.sel.clone(), "\n", cx);
        }
        let c = self.cursor();
        let lr = self.line_range(c);
        let line = self.content[lr.clone()].to_string();
        let in_code = self.in_code(lr.start);
        // Ouverture d'un bloc de code : on pose aussi la clôture.
        if md::is_fence(&line)
            && !in_code
            && c == lr.end
            && self.fences_before(self.content.len()) % 2 == 1
        {
            self.edit(c..c, "\n\n```", cx);
            return self.move_to(c + 1, cx);
        }
        match md::on_enter(&line, c - lr.start, in_code) {
            Enter::Insert(text) => self.edit(c..c, &text, cx),
            Enter::Clear => self.edit(lr, "", cx),
        }
        self.renumber(cx);
    }

    fn shift_lines(&mut self, indent: bool, cx: &mut Context<Self>) {
        if indent && self.accept_completion(cx) {
            return;
        }
        if self.table_step(indent, cx) {
            return;
        }
        let first = self.line_range(self.sel.start);
        if indent && self.sel.is_empty() {
            let line = &self.content[first.clone()];
            let (kind, _) = md::classify(line, self.in_code(first.start));
            if !matches!(kind, Kind::Bullet | Kind::Ordered | Kind::Task(_)) {
                return self.edit(self.sel.clone(), md::INDENT, cx);
            }
        }
        let block = first.start..self.line_range(self.sel.end).end;
        let sel = self.sel.clone();
        let mut deltas = Vec::new();
        let shifted: Vec<String> = self.content[block.clone()]
            .split('\n')
            .map(|l| {
                let new = if indent {
                    format!("{}{l}", md::INDENT)
                } else if let Some(rest) = l.strip_prefix('\t') {
                    rest.to_string()
                } else {
                    let n = l.bytes().take(md::INDENT.len()).take_while(|&b| b == b' ').count();
                    l[n..].to_string()
                };
                deltas.push(new.len() as isize - l.len() as isize);
                new
            })
            .collect();
        self.push_undo(true);
        self.damaged(&block);
        self.content.replace_range(block.clone(), &shifted.join("\n"));
        let total: isize = deltas.iter().sum();
        let start = (sel.start as isize + deltas[0]).max(block.start as isize) as usize;
        let end = (sel.end as isize + total).max(start as isize) as usize;
        self.sel = start..end;
        self.changed(cx);
        self.renumber(cx);
    }

    /// Un modèle rempli, à la place de la sélection : le curseur va à sa marque, un seul pas d'annulation.
    pub fn insert_filled(&mut self, text: &str, cursor: usize, cx: &mut Context<Self>) {
        let at = self.sel.start;
        self.push_undo(true);
        self.edit(self.sel.clone(), text, cx);
        self.move_to(at + cursor.min(text.len()), cx);
    }

    /// Où poser ce qu'on dépose au pointeur `at` : le début d'une ligne, celle du dessous quand le pointeur est
    /// dans la moitié basse de la sienne. Jamais dans un tableau, un bloc de code ni l'en-tête YAML : au
    /// bord du bloc, vers le bas.
    pub fn drop_point(&self, at: Point<Pixels>) -> usize {
        let y = at.y - self.origin.y;
        let Some(row) = self.rows.iter().find(|r| y < r.y + r.height).or(self.rows.last()) else {
            return 0;
        };
        let line = self.line_range(row.start);
        let mut point = if y > row.y + row.height / 2. { (line.end + 1).min(self.content.len()) } else { line.start };
        let front = md::front_matter(&self.content);
        if point > 0 && point < front {
            point = front;
        }
        // Dans un tableau (sa ligne et la précédente en sont) ou dans un bloc de code : plus bas.
        while point > 0 && point < self.content.len() {
            let (here, above) = (self.line_range(point), self.line_range(point - 1));
            if (self.is_table_line(&here) && self.is_table_line(&above)) || self.in_code(point) {
                point = (here.end + 1).min(self.content.len());
            } else {
                break;
            }
        }
        point
    }

    pub fn set_drop_mark(&mut self, mark: Option<usize>, cx: &mut Context<Self>) {
        if self.drop_mark != mark {
            self.drop_mark = mark;
            cx.notify();
        }
    }

    /// Le début de la ligne du curseur.
    pub fn caret_line_start(&self) -> usize {
        self.line_range(self.cursor()).start
    }

    /// Le nombre de changements du texte : un dépôt qui a mis du temps à copier sait avant d'écrire si la note a bougé.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Des lignes posées au début de la ligne `at` (ou après la dernière), qu'on tape ou qu'on dépose :
    /// un seul pas d'annulation, le curseur à leur fin.
    pub fn insert_lines(&mut self, at: usize, lines: &[String], cx: &mut Context<Self>) {
        let at = self.clamp(at);
        let before = if at > 0 && !self.content[..at].ends_with('\n') { "\n" } else { "" };
        let after = if at < self.content.len() { "\n" } else { "" };
        let text = format!("{before}{}{after}", lines.join("\n"));
        self.push_undo(true);
        self.edit(at..at, &text, cx);
        self.move_to(at + text.len() - after.len(), cx);
    }

    /// Les lignes de la sélection (ou celle du curseur) écrites dans le style `to` : une seule étape
    /// d'annulation, la sélection garde son texte. Une ligne que la sélection effleure seulement
    /// par son début n'en fait pas partie.
    pub fn restyle(&mut self, to: ListStyle, cx: &mut Context<Self>) {
        let sel = self.sel.clone();
        let first = self.line_range(sel.start);
        let touches = !sel.is_empty() && self.content[..sel.end].ends_with('\n');
        let block = first.start..self.line_range(if touches { sel.end - 1 } else { sel.end }).end;
        if self.in_code(first.start) || block.is_empty() {
            return;
        }
        let old = self.content[block.clone()].to_string();
        let (new, spans) = md::restyle(&old, to);
        if new == old {
            return;
        }
        // La position `at` après le changement : dans le contenu d'une ligne, elle le suit ; dans son
        // marqueur, elle va au début du nouveau contenu.
        let moved = |at: usize| {
            let at = at.clamp(block.start, block.end) - block.start;
            let (mut before, mut after) = (0, 0);
            for ((was, now), &(was_at, now_at)) in old.split('\n').zip(new.split('\n')).zip(&spans) {
                if at <= before + was.len() {
                    let within = at - before;
                    return block.start + after + if within >= was_at { now_at + within - was_at } else { within.min(now_at) };
                }
                before += was.len() + 1;
                after += now.len() + 1;
            }
            block.start + new.len()
        };
        let sel = moved(sel.start)..moved(sel.end);
        self.push_undo(true);
        self.damaged(&block);
        self.content.replace_range(block, &new);
        self.sel = sel;
        self.changed(cx);
        self.renumber(cx);
    }

    /// Texte, puces, numérotée, tâches, puis texte : la ligne du curseur (la première de la
    /// sélection) dit où l'on en est, et la sélection entière passe au suivant.
    fn cycle_list(&mut self, cx: &mut Context<Self>) {
        let line = self.line_range(self.sel.start);
        let kind = md::classify(&self.content[line.clone()], self.in_code(line.start)).0;
        let next = match kind {
            Kind::Bullet => ListStyle::Ordered,
            Kind::Ordered => ListStyle::Task,
            Kind::Task(_) => ListStyle::Text,
            _ => ListStyle::Bullet,
        };
        self.restyle(next, cx);
    }

    /// Coche ou décoche la case `[ ]` qui commence à l'octet `at`.
    fn flip_box(&mut self, at: usize, cx: &mut Context<Self>) {
        let done = self.content.as_bytes()[at + 1] != b' ';
        self.push_undo(true);
        self.splice(at + 1..at + 2, if done { " " } else { "x" }, cx);
    }

    /// Case à cocher sous le pointeur : son début. Elle est sur la ligne visuelle du pointeur,
    /// entre ses deux bords, et n'est pas dans un bloc de code.
    fn checkbox_at(&self, pos: Point<Pixels>) -> Option<usize> {
        let row = &self.rows[self.row_at(self.index_at(pos))?];
        if matches!(row.kind, Kind::Code | Kind::Fence) {
            return None;
        }
        let line = &self.content[row.start..row.start + row.len];
        let local = point(pos.x - self.origin.x, pos.y - self.origin.y - row.y - row.pad);
        md::checkboxes(line, 0).into_iter().find_map(|b| {
            let (from, to) = (row.pos(b.start), row.pos(b.end));
            let inside = local.x >= from.x && local.x <= to.x && local.y >= from.y && local.y < from.y + row.lh;
            (inside && from.y == to.y).then_some(row.start + b.start)
        })
    }

    fn toggle_task(&mut self, cx: &mut Context<Self>) {
        let lr = self.line_range(self.cursor());
        let line = &self.content[lr.clone()];
        let (kind, marker) = md::classify(line, self.in_code(lr.start));
        let at = lr.start + marker;
        // Une case écrite dans le texte ou dans une cellule, sous le curseur ou dans sa cellule.
        let c = self.cursor();
        let cell = self.table_cells(c).map(|(cells, k)| cells[k].clone());
        let near = |b: &Range<usize>| match &cell {
            Some(cell) => b.start + lr.start >= cell.start && b.end + lr.start <= cell.end,
            None => (b.start + lr.start..=b.end + lr.start).contains(&c),
        };
        if !self.in_code(lr.start) && !matches!(kind, Kind::Task(_) | Kind::Bullet) {
            if let Some(b) = md::checkboxes(line, 0).into_iter().find(near) {
                return self.flip_box(lr.start + b.start, cx);
            }
            // Une cellule sans case en reçoit une.
            if let Some(cell) = cell {
                self.push_undo(true);
                let text = if cell.is_empty() { "[ ]" } else { "[ ] " };
                self.edit(cell.start..cell.start, text, cx);
                return self.realign(cx);
            }
        }
        match kind {
            Kind::Task(done) => self.splice(at - 3..at - 2, if done { " " } else { "x" }, cx),
            Kind::Bullet => {
                self.splice(at..at, "[ ] ", cx);
                if self.sel.end == at {
                    self.move_to(at + 4, cx);
                }
            }
            _ => {}
        }
    }

    /// Les passages que `mark` entoure : le texte de chaque ligne de la sélection, hors blancs de
    /// bord et puce, sans les blocs de code ; le curseur seul, hors d'un bloc de code, en est un, vide.
    fn mark_parts(&self) -> Vec<Range<usize>> {
        let sel = self.sel.clone();
        let first = self.line_range(sel.start);
        let mut code = self.in_code(first.start);
        let mut parts = Vec::new();
        if sel.is_empty() {
            if !code {
                parts.push(sel);
            }
            return parts;
        }
        let mut at = first.start;
        while at <= sel.end {
            let lr = self.line_range(at);
            let line = &self.content[lr.clone()];
            if md::is_fence(line) {
                code = !code && self.content[lr.end..].lines().any(md::is_fence);
            } else if !code {
                let (a, b) = (sel.start.max(lr.start + md::classify(line, false).1), sel.end.min(lr.end));
                if a < b {
                    let text = &self.content[a..b];
                    let lead = text.len() - text.trim_start().len();
                    let kept = text.trim().len();
                    if kept > 0 {
                        parts.push(a + lead..a + lead + kept);
                    }
                }
            }
            at = lr.end + 1;
        }
        parts
    }

    /// Les signes du style `flag` autour de `part` (octets du texte), s'il est tout entier dans un passage de ce style.
    fn cut(&self, part: &Range<usize>, flag: u16) -> Option<(Range<usize>, Range<usize>)> {
        let lr = self.line_range(part.start);
        let line = &self.content[lr.clone()];
        let flags = md::flags(line, md::classify(line, false).1);
        let (open, close) = md::unmark(line, &flags, part.start - lr.start..part.end - lr.start, flag)?;
        Some((lr.start + open.start..lr.start + open.end, lr.start + close.start..lr.start + close.end))
    }

    /// Pose le style `flag` sur la sélection, ou le retire quand tout ce qu'elle tient l'a déjà
    /// (voir `md::unmark`). Un curseur seul pose les deux signes et se place entre eux. Sur
    /// plusieurs lignes, chacune reçoit ses propres signes : un style ne passe pas d'une ligne à l'autre.
    fn mark(&mut self, flag: u16, cx: &mut Context<Self>) {
        let Some(&(open, close)) = md::delimiters(flag).first() else { return };
        let parts = self.mark_parts();
        if parts.is_empty() {
            return;
        }
        let cuts: Option<Vec<_>> = parts.iter().map(|part| self.cut(part, flag)).collect();
        self.push_undo(true);
        // Tout y est déjà : les signes partent, ceux de fermeture d'abord pour que les octets restent justes.
        if let Some(cuts) = cuts {
            for (open, close) in cuts.into_iter().rev() {
                self.splice(close, "", cx);
                self.splice(open, "", cx);
            }
            return;
        }
        self.wrap(&parts, open, close, cx);
    }

    /// Entoure chaque passage de `open` et `close` ; la sélection va du premier au dernier, signes dehors.
    fn wrap(&mut self, parts: &[Range<usize>], open: &str, close: &str, cx: &mut Context<Self>) {
        let (Some(head), Some(tail)) = (parts.first().map(|p| p.start), parts.last().map(|p| p.end)) else { return };
        for part in parts.iter().rev() {
            self.splice(part.end..part.end, close, cx);
            self.splice(part.start..part.start, open, cx);
        }
        let grown = parts.len() * open.len() + (parts.len() - 1) * close.len();
        self.sel = head + open.len()..tail + grown;
        self.reversed = false;
    }

    /// Surligne la sélection de la couleur `color` (voir `md::MARKS`) : de la même couleur qu'elle a déjà,
    /// le surlignage part ; d'une autre, il change de couleur ; sinon il se pose.
    fn paint_with(&mut self, color: usize, cx: &mut Context<Self>) {
        let (open, close) = md::mark_signs(color);
        let parts = self.mark_parts();
        if parts.is_empty() {
            return;
        }
        let cuts: Option<Vec<_>> = parts.iter().map(|part| self.cut(part, md::HIGHLIGHT)).collect();
        self.push_undo(true);
        let Some(cuts) = cuts else {
            return self.wrap(&parts, &open, close, cx);
        };
        let same = cuts.iter().all(|(o, _)| self.content[o.clone()] == open);
        for (o, c) in cuts.into_iter().rev() {
            self.splice(c, if same { "" } else { close }, cx);
            self.splice(o, if same { "" } else { &open }, cx);
        }
    }

    #[cfg(test)]
    /// Les boutons de la barre de mise en forme sont à l'écran : dans la rangée du bas, ou ouverts au-dessus du bouton qui la replie.
    pub fn bar_shown(&self) -> bool {
        match self.bar_at {
            Some(Bar::Inline(_)) => true,
            Some(Bar::Compact) => self.bar_open,
            None => false,
        }
    }

    /// La note est trop étroite pour la barre : elle se replie en un bouton.
    #[cfg(test)]
    pub fn bar_folded(&self) -> bool {
        self.bar_at == Some(Bar::Compact)
    }

    /// Où poser la barre : au milieu de la page, plus à gauche quand le compteur et les boutons du coin
    /// la pousseraient, repliée en un bouton quand elle n'a plus la place ; `None` si elle est coupée ou
    /// que ce n'est pas le pane actif, ou tant que la largeur de la note n'est pas connue.
    // ponytail: la place du compteur est estimée (7,4 px par caractère à 12 px, les espaces et les points médians comptent plus que les lettres) plutôt que mesurée.
    fn bar_place(&mut self) -> Option<Bar> {
        let width = f32::from(self.viewport.size.width);
        if !self.theme.bar || !self.bar_here || width == 0. {
            return None;
        }
        let counts = self.counts().chars().count() as f32 * 7.4 + 16.;
        let x = (width / 2. - BAR_WIDTH / 2.).min(width - (12. + 34. * self.corner as f32) - counts - 8. - BAR_WIDTH);
        Some(if x >= 8. { Bar::Inline(x) } else { Bar::Compact })
    }

    /// Ce que la barre propose pour la sélection, recalculé seulement quand le texte ou la sélection changent.
    // ponytail: rien n'est allumé pour une sélection sur plusieurs lignes (le geste, lui, les traite) :
    // la compter à chaque mouvement de souris pèserait sur une grande sélection.
    fn tools(&mut self) -> Tools {
        let key = (self.version, self.sel.clone());
        if let Some((known, tools)) = &self.tooled
            && *known == key
        {
            return *tools;
        }
        let line = self.line_range(self.sel.start);
        let one_line = self.sel.end <= line.end;
        let parts = if one_line { self.mark_parts() } else { Vec::new() };
        let styles = match parts.as_slice() {
            [part] => md::STYLES.into_iter().filter(|&flag| self.cut(part, flag).is_some()).fold(0, |all, flag| all | flag),
            _ => 0,
        };
        let list = match md::classify(&self.content[line], false).0 {
            Kind::Bullet => Some(ListStyle::Bullet),
            Kind::Ordered => Some(ListStyle::Ordered),
            Kind::Task(_) => Some(ListStyle::Task),
            _ => None,
        };
        let tools = Tools { styles, list, table: self.in_table() };
        self.tooled = Some((key, tools));
        tools
    }

    /// Les styles posés sur la sélection (`md::STYLES`), pour allumer les boutons de la barre de mise en forme.
    #[cfg(test)]
    pub fn styles(&mut self) -> u16 {
        self.tools().styles
    }

    /// Les pastilles de couleur du surlignage, au-dessus de la barre ; `aligned` : sous le bouton H.
    fn swatches(&mut self, aligned: bool, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let t = self.theme;
        let names = [tr("Yellow", "Jaune"), tr("Green", "Vert"), tr("Blue", "Bleu"), tr("Pink", "Rose"), tr("Purple", "Violet")];
        div()
            .debug_selector(|| "format-swatches".into())
            .ml(px(if aligned { 4. + 4. * 27. } else { 0. }))
            .flex_none()
            .h(px(30.))
            .px(px(7.))
            .flex()
            .items_center()
            .gap(px(5.))
            .rounded(px(8.))
            .border_1()
            .border_color(t.border)
            .bg(t.panel)
            .occlude()
            .children((0..md::MARKS.len()).map(|i| {
                div()
                    .id(("swatch", i))
                    .debug_selector(move || format!("swatch-{i}"))
                    .size(px(18.))
                    .flex_none()
                    .rounded_full()
                    .cursor_pointer()
                    .bg(highlight(i).opacity(2.))
                    .border_1()
                    .border_color(t.border)
                    .hover(|s| s.border_color(t.accent))
                    .tooltip(crate::nav::tip(names[i], String::new(), t))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        window.focus(&this.focus);
                        this.swatches = false;
                        this.each(cx, |this, cx| this.paint_with(i, cx));
                    }))
            }))
    }

    /// La barre de mise en forme : gras, italique, souligné, barré, surligné, code, lien, puis les listes
    /// et l'insertion d'un tableau, ou, dans un tableau, ses lignes et ses colonnes. Un bouton s'allume
    /// quand la sélection a déjà le style, et le retire alors.
    fn format_bar(&mut self, mode: Bar, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let t = self.theme;
        let tools = self.tools();
        let width = f32::from(self.viewport.size.width);
        let (left, wide, inline) = match mode {
            Bar::Inline(x) => (x, BAR_WIDTH, true),
            Bar::Compact => (8., BAR_WIDTH.min(width - 16.), false),
        };
        let swatches = self.swatches.then(|| self.swatches(inline, cx));
        let on = tools.styles;
        let keys = |action: &dyn gpui::Action| crate::keys::of(cx, action);
        let mut tips = vec![
            (tr("Bold", "Gras"), keys(&Bold)),
            (tr("Italic", "Italique"), keys(&Italic)),
            (tr("Underline", "Souligné"), keys(&Underline)),
            (tr("Strikethrough", "Barré"), keys(&Strike)),
            (tr("Highlight", "Surligné"), keys(&Highlight)),
            (tr("Code", "Code"), keys(&InlineCode)),
            (tr("Link", "Lien"), keys(&InsertLink)),
        ];
        let ops = [TableOp::RowBelow, TableOp::ColRight, TableOp::DeleteRow, TableOp::DeleteCol];
        if tools.table {
            tips.extend(ops.map(|op| (op.label(), String::new())));
        } else {
            tips.extend([
                (tr("Bulleted list", "Liste à puces"), keys(&ListBullet)),
                (tr("Numbered list", "Liste numérotée"), keys(&ListOrdered)),
                (tr("Task list", "Liste de tâches"), keys(&ListTask)),
                (tr("Table", "Tableau"), String::new()),
            ]);
        }
        // Un bouton : son visage, son état, son infobulle ; le geste se pose ensuite.
        let button = |i: usize, face: gpui::AnyElement, lit: bool| {
            let (label, keys) = tips[i].clone();
            div()
                .id(("format", i))
                .debug_selector(move || format!("format-{i}"))
                .size(px(26.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .cursor_pointer()
                .text_size(px(13.))
                .text_color(if lit { t.accent } else { t.soft })
                .when(lit, |d| d.bg(t.selection))
                .hover(|s| s.bg(t.border).text_color(t.text))
                .active(|s| s.bg(t.selection))
                .tooltip(crate::nav::tip(label, keys, t))
                // Ni la note ni son curseur ne bougent sous le clic : le geste porte sur la sélection.
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(face)
        };
        let icon = |path: &'static str, lit: bool| gpui::svg().path(path).size(px(16.)).flex_none().text_color(if lit { t.accent } else { t.soft }).into_any_element();
        let lit = |flag: u16| on & flag != 0;
        let styled = |i: usize, flag: u16, face: gpui::AnyElement| {
            button(i, face, lit(flag)).on_click(cx.listener(move |this, _, window, cx| {
                window.focus(&this.focus);
                this.each(cx, |this, cx| this.mark(flag, cx));
            }))
        };
        // Un genre de liste : un second clic, sur ses lignes, les remet en texte.
        let listed = |i: usize, style: ListStyle, path: &'static str| {
            let lit = tools.list == Some(style);
            button(i, icon(path, lit), lit).on_click(cx.listener(move |this, _, window, cx| {
                window.focus(&this.focus);
                this.restyle(if lit { ListStyle::Text } else { style }, cx);
            }))
        };
        let last = if tools.table {
            let paths = ["row-add.svg", "col-add.svg", "row-del.svg", "col-del.svg"];
            (0..4)
                .map(|n| {
                    button(7 + n, icon(paths[n], false), false)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            window.focus(&this.focus);
                            this.table_op(ops[n], cx);
                        }))
                        .into_any_element()
                })
                .collect::<Vec<_>>()
        } else {
            vec![
                listed(7, ListStyle::Bullet, "list-bullet.svg").into_any_element(),
                listed(8, ListStyle::Ordered, "list-number.svg").into_any_element(),
                listed(9, ListStyle::Task, "list-task.svg").into_any_element(),
                // Le choix de la taille se fait sur la grille, comme avec `/table`.
                button(10, icon("table.svg", false), false)
                    .on_click(cx.listener(|this, _, window, cx| {
                        window.focus(&this.focus);
                        this.grid = Some((3, 3));
                        this.grid_typed.clear();
                        cx.notify();
                    }))
                    .into_any_element(),
            ]
        };
        let separator = || div().w(px(1.)).h(px(14.)).mx(px(3.)).bg(t.border);
        let bar = div()
            .debug_selector(|| "format-bar".into())
            .w_full()
            .min_h(px(30.))
            .px(px(3.))
            .flex_none()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(1.))
            .rounded(px(8.))
            .border_1()
            .border_color(t.border)
            .bg(t.panel)
            .occlude()
            .child(styled(0, md::BOLD, div().font_weight(FontWeight::BOLD).child("B").into_any_element()))
            .child(styled(1, md::ITALIC, div().italic().child("I").into_any_element()))
            .child(styled(2, md::UNDER, div().underline().child("U").into_any_element()))
            .child(styled(3, md::STRIKE, div().line_through().child("S").into_any_element()))
            .child(styled(4, md::HIGHLIGHT, div().px(px(4.)).rounded(px(3.)).bg(highlight(0)).text_color(t.text).child("H").into_any_element()))
            // Le choix de la couleur : des pastilles au-dessus de la barre.
            .child(
                div()
                    .id("format-colors")
                    .debug_selector(|| "format-colors".into())
                    .w(px(14.))
                    .h(px(26.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .when(self.swatches, |d| d.bg(t.selection))
                    .hover(|s| s.bg(t.border))
                    .tooltip(crate::nav::tip(tr("Highlight color", "Couleur du surlignage"), String::new(), t))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(|this, _, window, cx| {
                        window.focus(&this.focus);
                        this.swatches = !this.swatches;
                        cx.notify();
                    }))
                    .child(gpui::svg().path("chevron-down.svg").size(px(10.)).flex_none().text_color(t.soft)),
            )
            .child(separator())
            .child(styled(5, md::CODE, icon("code.svg", lit(md::CODE))))
            .child(button(6, icon("link.svg", false), false).on_click(cx.listener(|this, _, window, cx| {
                window.focus(&this.focus);
                this.link(cx);
            })))
            .child(separator())
            .children(last);
        // Les pastilles de couleur au-dessus de la barre, le tout ancré au bas de la note.
        // Repliée, la barre s'ouvre au-dessus du bouton (30 px, 16 px du bas, 6 px d'écart).
        div().absolute().bottom(px(if inline { 16. } else { 52. })).left(px(left)).w(px(wide)).flex().flex_col().items_start().gap(px(6.)).children(swatches).child(bar)
    }

    /// Le bouton qui replie la barre : en bas à gauche, il l'ouvre au-dessus de lui et la referme.
    fn bar_toggle(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let t = self.theme;
        let open = self.bar_open;
        div()
            .id("format-toggle")
            .debug_selector(|| "format-toggle".into())
            .absolute()
            .bottom_4()
            .left_2()
            .size(px(30.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(8.))
            .border_1()
            .border_color(t.border)
            .bg(bar_fill(&t, open))
            .cursor_pointer()
            .hover(|s| s.bg(t.border))
            .occlude()
            .tooltip(crate::nav::tip(tr("Formatting", "Mise en forme"), String::new(), t))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, window, cx| {
                window.focus(&this.focus);
                this.bar_open = !this.bar_open;
                this.swatches = false;
                cx.notify();
            }))
            .child(gpui::svg().path("format.svg").size(px(16.)).flex_none().text_color(if open { t.accent } else { t.soft }))
    }

    /// Adresse que tient le presse-papiers, s'il n'y a que cela.
    fn copied_url(cx: &mut Context<Self>) -> Option<String> {
        let text = cx.read_from_clipboard()?.text()?;
        md::is_url(text.trim()).then(|| text.trim().to_string())
    }

    /// La sélection, si elle peut devenir le texte d'un lien : sur une seule ligne, hors du
    /// code, et pas déjà une adresse.
    fn linkable(&self) -> bool {
        let text = &self.content[self.sel.clone()];
        !text.contains('\n') && !md::is_url(text.trim()) && !self.in_code(self.line_range(self.sel.start).start)
    }

    /// Fait un lien `[texte](adresse)`. Sur un lien existant, son adresse est sélectionnée,
    /// prête à être remplacée. Sinon la sélection devient le texte du lien, vers l'adresse du
    /// presse-papiers s'il en tient une ; le curseur attend ce qui manque, l'adresse entre les
    /// parenthèses ou le texte entre les crochets.
    fn link(&mut self, cx: &mut Context<Self>) {
        let lr = self.line_range(self.sel.start);
        let (line, col) = (&self.content[lr.clone()], self.sel.start - lr.start);
        let on = md::links(line).into_iter().find(|(r, _)| (r.start..=r.end).contains(&col));
        if let Some((r, _)) = &on
            && let Some((_, url)) = md::web_link(&line[r.start..])
        {
            let at = lr.start + r.start;
            self.move_to(at + url.start, cx);
            return self.select_to(at + url.end, cx);
        }
        // Sur une adresse nue : c'est elle qui devient le lien.
        if let Some((r, md::Link::Url(_))) = on.filter(|_| self.sel.is_empty()) {
            self.sel = lr.start + r.start..lr.start + r.end;
        }
        let sel = self.sel.clone();
        // Une adresse sélectionnée garde sa place : il lui manque son texte.
        let picked = self.content[sel.clone()].to_string();
        if !md::is_url(picked.trim()) && !self.linkable() {
            return;
        }
        let (text, url) = match md::is_url(picked.trim()) {
            true => (String::new(), picked.trim().to_string()),
            false => (picked, Self::copied_url(cx).unwrap_or_default()),
        };
        self.push_undo(true);
        self.edit(sel.clone(), &format!("[{text}]({url})"), cx);
        let at = match (text.is_empty(), url.is_empty()) {
            (true, _) => 1,
            (false, true) => text.len() + 3,
            (false, false) => text.len() + url.len() + 4,
        };
        self.move_to(sel.start + at, cx);
    }

    /// Remplace tout le texte, comme une frappe : une étape d'annulation, la note est à enregistrer.
    pub fn rewrite(&mut self, text: &str, cx: &mut Context<Self>) {
        self.more.clear();
        // Une seule étape : `splice` en poserait une seconde pour un texte de plusieurs lignes.
        self.push_undo(true);
        self.held = true;
        self.splice(0..self.content.len(), text, cx);
        self.held = false;
    }

    /// Résout le commentaire qui commence à l'octet `at`.
    pub fn resolve_comment(&mut self, at: usize, cx: &mut Context<Self>) {
        self.move_to(self.clamp(at), cx);
        self.comment(cx);
    }

    /// Commente la sélection, ou la ligne sans elle : `{==texte==}{>>…<<}`, le curseur dans le
    /// commentaire à écrire. Sur un commentaire, le résout : le balisage part, le texte reste.
    fn comment(&mut self, cx: &mut Context<Self>) {
        let lr = self.line_range(self.sel.start);
        let (line, col) = (&self.content[lr.clone()], self.sel.start - lr.start);
        if let Some((whole, noted, _)) = md::comments(line).into_iter().find(|(whole, ..)| (whole.start..=whole.end).contains(&col)) {
            let kept = line[noted].to_string();
            self.push_undo(true);
            return self.edit(lr.start + whole.start..lr.start + whole.end, &kept, cx);
        }
        let marker = md::classify(line, false).1;
        let sel = if self.sel.is_empty() { lr.start + marker..lr.end } else { self.sel.clone() };
        if sel.is_empty() || sel.end > lr.end || self.in_code(lr.start) {
            return;
        }
        let text = self.content[sel.clone()].to_string();
        self.push_undo(true);
        self.edit(sel.clone(), &format!("{{=={text}==}}{{>><<}}"), cx);
        self.move_to(sel.start + text.len() + 9, cx);
    }

    // ----- Tableaux -----

    /// Tableau autour de l'octet `at` : son étendue, ses lignes, et la cellule
    /// (ligne, colonne) où se trouve `at`.
    /// L'octet `at` est dans un très gros tableau (voir `grid::BIG`).
    fn in_big(&self, at: usize) -> bool {
        self.row_at(at).is_some_and(|r| self.big.iter().any(|b| b.rows.contains(&r)))
    }

    fn table_at(&self, at: usize) -> Option<(Range<usize>, Vec<Vec<String>>, (usize, usize))> {
        // ponytail: un très gros tableau (des milliers de lignes) n'est pas relu ni réaligné d'un bloc à chaque
        // frappe : ni alignement des colonnes, ni ajout de ligne avec Tab ou Entrée. Tab passe à la cellule voisine.
        if self.in_big(at) {
            return None;
        }
        let is_row = |r: &Range<usize>| self.content[r.clone()].trim_start().starts_with('|');
        let line = self.line_range(at);
        if !is_row(&line) || self.in_code(line.start) {
            return None;
        }
        let (mut start, mut end) = (line.start, line.end);
        while start > 0 {
            let above = self.line_range(start - 1);
            if !is_row(&above) {
                break;
            }
            start = above.start;
        }
        while end < self.content.len() {
            let below = self.line_range(end + 1);
            if !is_row(&below) {
                break;
            }
            end = below.end;
        }
        let (rows, added) = md::table(&self.content[start..end]);
        let mut r = self.content[start..at].matches('\n').count();
        // La ligne de tirets qui manquait décale celles qui suivent l'en-tête.
        r += (added && r > 0) as usize;
        let col = at - line.start;
        let bars = md::bars(&self.content[line]).into_iter().filter(|&bar| bar < col).count();
        let c = bars.saturating_sub(1).min(rows[0].len() - 1);
        Some((start..end, rows, (r, c)))
    }

    fn format_table(&self, range: &Range<usize>, rows: &[Vec<String>]) -> String {
        let first = &self.content[range.clone()];
        md::format_table(rows, &first[..first.len() - first.trim_start().len()])
    }

    /// Réécrit le tableau `range`, colonnes alignées, et sélectionne le contenu
    /// de la cellule : taper le remplace.
    fn set_table(&mut self, range: Range<usize>, rows: &[Vec<String>], (r, c): (usize, usize), cx: &mut Context<Self>) {
        let text = self.format_table(&range, rows);
        if self.content[range.clone()] != text {
            self.push_undo(true);
            self.edit(range.clone(), &text, cx);
        }
        let line = text.split('\n').nth(r).unwrap_or_default();
        let at = range.start + text.split('\n').take(r).map(|l| l.len() + 1).sum::<usize>();
        let cell = md::cells(line).get(c).cloned().unwrap_or(0..0);
        self.move_to(at + cell.start, cx);
        self.select_to(at + cell.end, cx);
    }

    /// Après une frappe dans un tableau déjà formé (il a sa ligne de tirets) : ses
    /// colonnes se réalignent et le curseur garde sa place dans sa cellule. Comme
    /// `renumber`, sans étape d'annulation propre.
    fn realign(&mut self, cx: &mut Context<Self>) {
        let c = self.cursor();
        let line = self.line_range(c);
        if !self.sel.is_empty() || !self.content[line.clone()].trim_start().starts_with('|') {
            return;
        }
        let Some((range, rows, (r, col))) = self.table_at(c) else {
            return;
        };
        let text = self.format_table(&range, &rows);
        let formed = self.content[range.clone()].matches('\n').count() + 1 == rows.len();
        if !formed || text == self.content[range.clone()] {
            return;
        }
        let old = md::cells(&self.content[line.clone()]).get(col).map_or(c, |cell| line.start + cell.start);
        let at = range.start + text.split('\n').take(r).map(|l| l.len() + 1).sum::<usize>();
        let new = text.split('\n').nth(r).unwrap_or_default();
        // Au plus loin, juste avant le `|` qui ferme la cellule.
        let (start, end) = (md::cells(new)[col].start, md::bars(new)[col + 1]);
        let cursor = at + (start + c.saturating_sub(old)).min(end);
        self.damaged(&range);
        self.content.replace_range(range, &text);
        self.sel = cursor..cursor;
        self.changed(cx);
    }

    /// Tab et Maj+Tab dans un tableau : cellule suivante ou précédente ; au bout
    /// du tableau, une ligne s'ajoute.
    /// Tab et Maj+Tab dans un très gros tableau : la cellule voisine, sur la ligne ou sur la voisine,
    /// sans relire le tableau.
    fn table_step_big(&mut self, forward: bool, cx: &mut Context<Self>) -> bool {
        let cursor = self.cursor();
        let line = self.line_range(cursor);
        let cells = md::cells(&self.content[line.clone()]);
        let here = cells.iter().position(|c| line.start + c.end >= cursor).unwrap_or(cells.len().saturating_sub(1));
        let next = |line: &Range<usize>, first: bool| {
            let cells = md::cells(&self.content[line.clone()]);
            let cell = if first { cells.first() } else { cells.last() }?;
            Some(line.start + cell.start..line.start + cell.end)
        };
        let target = match (forward, here) {
            (true, i) if i + 1 < cells.len() => Some(line.start + cells[i + 1].start..line.start + cells[i + 1].end),
            (false, i) if i > 0 => Some(line.start + cells[i - 1].start..line.start + cells[i - 1].end),
            (true, _) if line.end < self.content.len() => {
                let below = self.line_range(line.end + 1);
                self.is_table_line(&below).then(|| next(&below, true)).flatten()
            }
            (false, _) if line.start > 0 => {
                let above = self.line_range(line.start - 1);
                self.is_table_line(&above).then(|| next(&above, false)).flatten()
            }
            _ => None,
        };
        if let Some(cell) = target {
            self.move_to(cell.start, cx);
            self.select_to(cell.end, cx);
        }
        true
    }

    fn table_step(&mut self, forward: bool, cx: &mut Context<Self>) -> bool {
        if self.in_big(self.cursor()) {
            return self.table_step_big(forward, cx);
        }
        let Some((range, mut rows, (r, c))) = self.table_at(self.cursor()) else {
            return false;
        };
        let cols = rows[0].len();
        let mut cell = r * cols + c;
        // La ligne de tirets (la deuxième) se saute.
        loop {
            cell = if forward { cell + 1 } else { cell.saturating_sub(1) };
            if cell / cols != 1 {
                break;
            }
        }
        if cell / cols == rows.len() {
            rows.push(vec![String::new(); cols]);
        }
        self.set_table(range, &rows, (cell / cols, cell % cols), cx);
        true
    }

    /// Le curseur est dans un tableau Markdown qu'on sait réécrire (pas un très gros).
    pub fn in_table(&self) -> bool {
        self.table_at(self.sel.start).is_some()
    }

    /// Un geste sur le tableau du curseur, pour les lignes et les colonnes que la sélection touche
    /// (de la cellule où elle commence à celle où elle finit). Un seul pas d'annulation ; le curseur
    /// se pose sur la cellule qui suit le geste.
    /// Les colonnes prises par leur poignée, entières, partent ; sinon les lignes de la sélection.
    fn table_delete_span(&mut self, cx: &mut Context<Self>) {
        let Some((range, rows, _)) = self.table_at(self.sel.start) else {
            return;
        };
        let cols = self.live_pick(range.start).is_some_and(|p| p.rows.start == 0 && p.rows.end >= md::visible_rows(&rows) && p.cols.len() < rows[0].len());
        self.table_op(if cols { TableOp::DeleteCol } else { TableOp::DeleteRow }, cx)
    }

    fn table_op(&mut self, op: TableOp, cx: &mut Context<Self>) {
        let Some((range, rows, (r, c))) = self.table_at(self.sel.start) else {
            return;
        };
        // Rang visible : la ligne de tirets compte pour l'en-tête.
        let seen = |r: usize| r.saturating_sub(1);
        let slot = |v: usize| if v == 0 { 0 } else { v + 1 };
        let end = self.table_at(self.sel.end).filter(|(other, ..)| *other == range).map_or((r, c), |(.., cell)| cell);
        // Ce qu'une poignée a pris compte plus que le texte sélectionné.
        let (r0, r1, c0, c1) = match self.live_pick(range.start) {
            Some(pick) => (pick.rows.start, pick.rows.end - 1, pick.cols.start, pick.cols.end - 1),
            None => (seen(r).min(seen(end.0)), seen(r).max(seen(end.0)), c.min(end.1), c.max(end.1)),
        };
        let (tall, wide) = (md::visible_rows(&rows), rows[0].len());
        let (grid, at) = match op {
            TableOp::RowAbove => (Some(md::insert_row(&rows, r0)), (slot(r0.max(1)), c0)),
            TableOp::RowBelow => (Some(md::insert_row(&rows, r1 + 1)), (slot(r1 + 1), c0)),
            TableOp::ColLeft => (Some(md::insert_col(&rows, c0)), (slot(r0), c0)),
            TableOp::ColRight => (Some(md::insert_col(&rows, c1 + 1)), (slot(r0), c1 + 1)),
            TableOp::DuplicateRow => (Some(md::duplicate_row(&rows, r0)), (slot(r0 + 1), c0)),
            TableOp::DuplicateCol => (Some(md::duplicate_col(&rows, c0)), (slot(r0), c0 + 1)),
            TableOp::MoveRowUp if r0 > 1 => (Some(md::move_row(&rows, r0 - 1, r1)), (slot(r0 - 1), c0)),
            TableOp::MoveRowDown if r0 > 0 && r1 + 1 < tall => (Some(md::move_row(&rows, r1 + 1, r0)), (slot(r0 + 1), c0)),
            TableOp::MoveColLeft if c0 > 0 => (Some(md::move_col(&rows, c0 - 1, c1)), (slot(r0), c0 - 1)),
            TableOp::MoveColRight if c1 + 1 < wide => (Some(md::move_col(&rows, c1 + 1, c0)), (slot(r0), c0 + 1)),
            TableOp::DeleteRow => (md::delete_rows(&rows, r0..r1 + 1), (slot(r0.min(tall.saturating_sub(r1 - r0 + 2))), c0)),
            TableOp::DeleteCol => (md::delete_cols(&rows, c0..c1 + 1), (slot(r0), c0.min(wide.saturating_sub(c1 - c0 + 2)))),
            TableOp::SortAsc => (Some(md::sort_rows(&rows, c0, true)), (0, c0)),
            TableOp::SortDesc => (Some(md::sort_rows(&rows, c0, false)), (0, c0)),
            TableOp::Transpose => (Some(md::transpose(&rows)), (0, 0)),
            TableOp::DeleteTable => (None, (0, 0)),
            // Un geste qui n'a nulle part où aller : l'en-tête reste en tête, le dernier rang ne descend pas.
            _ => return,
        };
        match grid {
            Some(grid) => self.set_table(range, &grid, at, cx),
            // Plus rien dans le tableau : il part avec le retour à la ligne qui le suit.
            None => {
                let stop = if self.content[range.end..].starts_with('\n') { range.end + 1 } else { range.end };
                self.push_undo(true);
                self.edit(range.start..stop, "", cx);
                self.move_to(range.start.min(self.content.len()), cx);
            }
        }
    }

    /// Aligne la colonne du curseur : `:--`, `:-:` ou `--:` sous son en-tête. La ligne de
    /// tirets ne se voit pas, c'est donc ce raccourci qui la règle.
    fn align(&mut self, dashes: &str, cx: &mut Context<Self>) {
        let Some((range, mut rows, (r, c))) = self.table_at(self.cursor()) else {
            return;
        };
        rows[1][c] = dashes.to_string();
        self.set_table(range, &rows, (r, c), cx);
    }

    /// Pose un tableau vide au curseur, sur ses propres lignes.
    fn insert_table(&mut self, cols: usize, rows: usize, cx: &mut Context<Self>) {
        let c = self.cursor();
        let line = self.line_range(c);
        // Une ligne vide le sépare de ce qui précède : collé à une citation, il en ferait partie.
        let above = self.content[..line.start].trim_end_matches(' ');
        let apart = self.content[line.start..c].trim().is_empty() && (above.is_empty() || above.ends_with("\n\n"));
        let lead = if apart { "" } else { "\n" };
        let tail = if c == line.end { "" } else { "\n" };
        let table = md::new_table(cols, rows);
        self.push_undo(true);
        self.edit(c..c, &format!("{lead}{}{tail}", md::format_table(&table, "")), cx);
        if let Some((range, rows, _)) = self.table_at(c + lead.len()) {
            self.set_table(range, &rows, (0, 0), cx);
        }
    }

    /// Ce qui se clique sous le pointeur prend la main : une poignée de tableau (ouverte, fermée
    /// quand on la tire), un trait où insérer, une case à cocher, un lien quand `Ctrl` est tenu.
    fn hand_at(&self, at: Point<Pixels>, held: gpui::Modifiers) -> Option<CursorStyle> {
        if self.moving.is_some() {
            return Some(CursorStyle::ClosedHand);
        }
        if !self.viewport.contains(&at) {
            return None;
        }
        if let Some((_, part)) = self.part_at(at) {
            return Some(match part {
                Part::JoinCol(_) | Part::JoinRow(_) => CursorStyle::PointingHand,
                Part::Col(_) | Part::Row(_) | Part::Corner => CursorStyle::OpenHand,
            });
        }
        if self.rows.is_empty() {
            return None;
        }
        if !held.secondary() && self.checkbox_at(at).is_some() {
            return Some(CursorStyle::PointingHand);
        }
        let i = self.index_at(at);
        let lr = self.line_range(i);
        let (line, col) = (&self.content[lr.clone()], i - lr.start);
        if held.secondary() && md::links(line).into_iter().any(|(r, _)| r.contains(&col)) {
            return Some(CursorStyle::PointingHand);
        }
        let (kind, marker) = md::classify(line, self.in_code(lr.start));
        (matches!(kind, Kind::Task(_)) && col + 4 >= marker && col < marker).then_some(CursorStyle::PointingHand)
    }

    fn aim_hand(&mut self, at: Point<Pixels>, held: gpui::Modifiers, cx: &mut Context<Self>) {
        let hand = self.hand_at(at, held);
        if hand != self.hand {
            self.hand = hand;
            cx.notify();
        }
    }

    /// La poignée sous le pointeur : (début du tableau, poignée). Seules celles qu'on voit se saisissent.
    fn part_at(&self, at: Point<Pixels>) -> Option<(usize, Part)> {
        self.geos.iter().find_map(|geo| {
            let (near, pick) = (geo.near(at), self.live_pick(geo.start));
            let hit = geo.parts().into_iter().find(|(part, zone)| zone.dilate(px(2.)).contains(&at) && geo.visible(*part, near, pick))?;
            Some((geo.start, hit.0))
        })
    }

    /// Ce qu'une poignée a pris dans le tableau qui commence en `table`, tant que rien n'a bougé.
    fn live_pick(&self, table: usize) -> Option<&Pick> {
        self.pick.as_ref().filter(|p| p.table == table && p.version == self.version && p.sel == self.sel)
    }

    /// Les cases `rows` × `cols` (rangs visibles) du tableau qui commence en `table` sont prises : le
    /// curseur va dans la première, et les gestes, la copie et l'effacement valent pour toutes.
    fn pick(&mut self, table: usize, rows: Range<usize>, cols: Range<usize>, cx: &mut Context<Self>) {
        let Some((range, _, _)) = self.table_at(table) else {
            return;
        };
        let line = if rows.start == 0 { 0 } else { rows.start + 1 };
        let text = &self.content[range.clone()];
        let at = range.start + text.split('\n').take(line).map(|l| l.len() + 1).sum::<usize>();
        let cell = md::cells(text.split('\n').nth(line).unwrap_or_default()).get(cols.start).cloned().unwrap_or(0..0);
        self.move_to(at + cell.start, cx);
        self.select_to(at + cell.end, cx);
        self.pick = Some(Pick { table: range.start, rows, cols, sel: self.sel.clone(), version: self.version });
        cx.notify();
    }

    /// Prend ce que désigne une poignée.
    fn pick_part(&mut self, table: usize, part: Part, cx: &mut Context<Self>) {
        let Some(geo) = self.geos.iter().find(|g| g.start == table) else {
            return;
        };
        let (tall, cols) = (geo.tall(), geo.cols());
        match part {
            Part::Col(c) => self.pick(table, 0..tall, c..c + 1, cx),
            Part::Row(r) => self.pick(table, r..r + 1, 0..cols, cx),
            Part::Corner => self.pick(table, 0..tall, 0..cols, cx),
            Part::JoinCol(_) | Part::JoinRow(_) => {}
        }
    }

    /// Les cases prises, en TSV.
    fn pick_text(&self) -> Option<String> {
        let (range, rows, _) = self.table_at(self.sel.start)?;
        let pick = self.live_pick(range.start)?;
        let line = |v: usize| if v == 0 { 0 } else { v + 1 };
        let cells = |v: usize| rows[line(v)][pick.cols.clone()].iter().map(|c| c.replace("\\|", "|")).collect::<Vec<_>>().join("\t");
        Some(pick.rows.clone().map(cells).collect::<Vec<_>>().join("\n"))
    }

    /// Vide les cases prises, sans toucher à la forme du tableau. Faux s'il n'y en a pas.
    fn clear_pick(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((range, mut rows, _)) = self.table_at(self.sel.start) else {
            return false;
        };
        let Some(pick) = self.live_pick(range.start).cloned() else {
            return false;
        };
        for v in pick.rows.clone() {
            for c in pick.cols.clone() {
                rows[if v == 0 { 0 } else { v + 1 }][c].clear();
            }
        }
        self.set_table(range.clone(), &rows, (if pick.rows.start == 0 { 0 } else { pick.rows.start + 1 }, pick.cols.start), cx);
        self.pick(range.start, pick.rows, pick.cols, cx);
        true
    }

    /// Un clic sur une poignée : « + » insère, une poignée prend sa ligne ou sa colonne (et la tire,
    /// si on la déplace), l'angle prend tout le tableau.
    fn grip_down(&mut self, table: usize, part: Part, at: Point<Pixels>, cx: &mut Context<Self>) {
        match part {
            Part::JoinCol(b) | Part::JoinRow(b) => {
                let Some((range, rows, _)) = self.table_at(table) else {
                    return;
                };
                let cols = matches!(part, Part::JoinCol(_));
                let grid = if cols { md::insert_col(&rows, b) } else { md::insert_row(&rows, b) };
                self.set_table(range, &grid, if cols { (0, b) } else { (b + 1, 0) }, cx);
            }
            _ => {
                self.pick_part(table, part, cx);
                // L'en-tête reste en tête : sa ligne se prend, elle ne se tire pas.
                if matches!(part, Part::Col(_) | Part::Row(1..)) {
                    self.moving = Some(Moving { table, part, from: at, to: None });
                }
            }
        }
    }

    /// La poignée tirée est lâchée sur le trait `to` : la ligne ou la colonne s'y pose, et reste prise.
    fn drop_part(&mut self, moving: Moving, cx: &mut Context<Self>) {
        let (Some(b), Some((range, rows, _))) = (moving.to, self.table_at(moving.table)) else {
            return;
        };
        let (from, grid) = match moving.part {
            Part::Col(n) | Part::Row(n) => (n, if b > n { b - 1 } else { b }),
            _ => return,
        };
        if from == grid {
            return;
        }
        let (cols, tall) = (rows[0].len(), md::visible_rows(&rows));
        let moved = if matches!(moving.part, Part::Col(_)) { md::move_col(&rows, from, grid) } else { md::move_row(&rows, from, grid) };
        self.set_table(range.clone(), &moved, (0, 0), cx);
        if matches!(moving.part, Part::Col(_)) {
            self.pick(range.start, 0..tall, grid..grid + 1, cx);
        } else {
            self.pick(range.start, grid..grid + 1, 0..cols, cx);
        }
    }

    /// Bouton « + » sous le pointeur : (début du tableau, 0 pour une ligne ou 1
    /// pour une colonne).
    fn plus_at(&self, at: Point<Pixels>) -> Option<(usize, usize)> {
        self.tables.iter().find_map(|(range, frame)| {
            Some((range.start, plus_bars(*frame).iter().position(|bar| bar.contains(&at))?))
        })
    }

    fn grid_size() -> Size<Pixels> {
        let side = GRID_STEP * GRID as f32 + px(12.);
        size(side, side + px(22.))
    }

    /// Case du sélecteur de tableau sous le pointeur : (colonnes, lignes).
    fn grid_at(&self, at: Point<Pixels>) -> Option<(usize, usize)> {
        self.grid?;
        let local = at - self.popup_at(Self::grid_size())? - point(px(8.), px(8.));
        let (col, row) = ((local.x / GRID_STEP).floor(), (local.y / GRID_STEP).floor());
        let inside = |n: f32| (0. ..GRID as f32).contains(&n);
        (inside(col) && inside(row)).then_some((col as usize + 1, row as usize + 1))
    }

    /// Coin d'un panneau flottant de taille `panel` : sous le curseur, ou
    /// au-dessus s'il n'y tient pas.
    fn popup_at(&self, panel: Size<Pixels>) -> Option<Point<Pixels>> {
        let c = self.cursor();
        let row = &self.rows[self.row_at(c)?];
        let p = row.pos(c - row.start);
        let top = self.origin.y + row.y + row.pad + p.y;
        let (below, above) = (top + row.lh + px(4.), top - panel.height - px(4.));
        let fits = below + panel.height <= self.viewport.bottom() || above < self.viewport.top();
        let x = (self.origin.x + p.x).min(self.viewport.right() - panel.width - px(8.));
        Some(point(x.max(self.viewport.left()), if fits { below } else { above }))
    }

    // ----- Complétion : wikiliens et commandes `/` -----

    /// Début de la requête (après `[[` ou `/`) et ce qui lui correspond.
    fn completion(&self) -> Option<(usize, Vec<Choice<'_>>)> {
        if !self.sel.is_empty() || !self.more.is_empty() {
            return None;
        }
        let c = self.cursor();
        let line = self.line_range(c).start;
        let before = &self.content[line..c];
        let (start, items) = self.names(before, c).or_else(|| self.commands(before, c, line)).or_else(|| self.days(before, c, line))?;
        (!items.is_empty() && self.ac_dismissed != Some(start)).then_some((start, items))
    }

    /// Après `[[` : les notes du coffre ; après `![[` : ses images et ses schémas.
    fn names(&self, before: &str, c: usize) -> Option<(usize, Vec<Choice<'_>>)> {
        let open = before.rfind("[[")?;
        let query = &before[open + 2..];
        if query.contains("]]") || query.contains('|') {
            return None;
        }
        let start = c - query.len();
        let query = query.to_lowercase();
        // `![[` affiche un fichier : ce sont les images et les schémas du coffre
        // qu'on propose, par leur nom de fichier ; `[[` propose les notes.
        let embed = before[..open].ends_with('!');
        let mut items: Vec<&str> = match embed {
            true => self.images.values().filter_map(|p| p.file_name()?.to_str()).collect(),
            false => self.notes.iter().map(String::as_str).collect(),
        };
        items.retain(|name| name.to_lowercase().contains(&query));
        if embed {
            items.sort_unstable_by_key(|name| name.to_lowercase());
        }
        items.truncate(6);
        Some((start, items.into_iter().map(Choice::Name).collect()))
    }

    /// Après un `/` en début de ligne ou de mot, hors du code : les commandes dont
    /// le nom commence par la requête, puis celles dont le libellé la contient.
    fn commands(&self, before: &str, c: usize, line: usize) -> Option<(usize, Vec<Choice<'_>>)> {
        let slash = before.rfind('/')?;
        let query = before[slash + 1..].to_lowercase();
        let starts_word = before[..slash].chars().next_back().is_none_or(char::is_whitespace);
        if !starts_word || !query.chars().all(char::is_alphanumeric) || self.in_code(line) {
            return None;
        }
        let mut items: Vec<&'static Command> = COMMANDS
            .iter()
            .filter(|(name, en, fr, ..)| name.starts_with(&query) || tr(en, fr).to_lowercase().contains(&query))
            .collect();
        items.sort_by_key(|(name, ..)| !name.starts_with(&query));
        Some((c - query.len(), items.into_iter().map(Choice::Command).collect()))
    }

    /// Après un `@` en début de ligne ou de mot, hors du code : les jours que désigne la suite
    /// (`@demain`, `@lundi`, `@2026-10-09`). Une adresse `nom@domaine` n'ouvre rien.
    fn days(&self, before: &str, c: usize, line: usize) -> Option<(usize, Vec<Choice<'_>>)> {
        let at = before.rfind('@')?;
        let query = &before[at + 1..];
        let starts_word = before[..at].chars().next_back().is_none_or(char::is_whitespace);
        if !starts_word || !query.chars().all(|ch| ch.is_alphanumeric() || matches!(ch, '-' | '\'' | '’')) || self.in_code(line) {
            return None;
        }
        let items = md::dates(query, crate::today()).into_iter().map(|(en, fr, date)| Choice::Date(en, fr, date));
        Some((c - query.len(), items.collect()))
    }

    fn accept_completion(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((start, items)) = self.completion() else {
            return false;
        };
        let c = self.cursor();
        match items[self.ac_index.min(items.len() - 1)] {
            Choice::Name(name) => {
                let name = name.to_string();
                let closed = self.content[c..].starts_with("]]");
                self.edit(start..c, &format!("{name}]]"), cx);
                if closed {
                    let end = self.cursor();
                    self.splice(end..end + 2, "", cx);
                }
            }
            // Le jour devient un lien vers sa note du jour, créée à sa première ouverture.
            Choice::Date(_, _, date) => {
                self.push_undo(true);
                self.edit(start - 1..c, &format!("[[{}]]", crate::date_name(date)), cx);
            }
            Choice::Command(&(name, _, _, before, after)) => {
                let slash = start - 1;
                self.push_undo(true);
                if name == "table" {
                    self.edit(slash..c, "", cx);
                    self.grid = Some((3, 3));
                    self.grid_typed.clear();
                    return true;
                }
                // L'en-tête YAML se pose en tête de note, où qu'on le demande ; s'il y est déjà,
                // le curseur y va. Dans les deux cas il attend entre les crochets des tags.
                if name == "meta" {
                    self.edit(slash..c, "", cx);
                    if md::front_matter(&self.content) == 0 {
                        self.edit(0..0, "---\ntags: []\naliases: []\n---\n", cx);
                    }
                    let block = &self.content[..md::front_matter(&self.content)];
                    let at = block.find("tags: [").map_or("---\n".len(), |i| i + "tags: [".len());
                    self.move_to(at, cx);
                    return true;
                }
                // La date du jour s'écrit là où on la demande.
                if name == "date" {
                    self.edit(slash..c, &crate::date_name(crate::today()), cx);
                    return true;
                }
                // Un lien se pose dans la phrase ; tout le reste commence sa ligne.
                let line = self.line_range(c);
                let inline = before.ends_with("[[") || self.content[line.start..slash].trim().is_empty();
                let lead = if inline { "" } else { "\n" };
                let tail = if after.is_empty() || c == line.end { "" } else { "\n" };
                self.edit(slash..c, &format!("{lead}{before}{after}{tail}"), cx);
                self.move_to(slash + lead.len() + before.len(), cx);
            }
        }
        true
    }

    // ----- Souris -----

    fn copy_bounds(&self, row: &Row) -> Bounds<Pixels> {
        let at = point(self.width - COPY_SIZE, row.y + row.pad + (row.lh - COPY_SIZE) / 2.);
        Bounds::new(self.origin + at, size(COPY_SIZE, COPY_SIZE))
    }

    /// Début de la ligne dont le bouton de copie est sous le pointeur.
    fn copy_at(&self, at: Point<Pixels>) -> Option<usize> {
        let row = self.rows.iter().find(|r| r.opens && self.copy_bounds(r).contains(&at))?;
        Some(row.start)
    }

    fn index_at(&self, pos: Point<Pixels>) -> usize {
        let y = pos.y - self.origin.y;
        let Some(row) = self.rows.iter().find(|r| y < r.y + r.height).or(self.rows.last()) else {
            return 0;
        };
        let local = point(pos.x - self.origin.x, (y - row.y - row.pad).max(px(0.)));
        self.clamp(row.start + row.index_at(local))
    }

    fn mouse_down(&mut self, e: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(find) = &mut self.find {
            find.active = false;
        }
        // Bouton de copie, à droite de l'ouverture d'un bloc de code.
        if let Some(start) = self.copy_at(e.position)
            && let Some(block) = md::code_block(&self.content, self.line_range(start).end + 1)
        {
            cx.write_to_clipboard(ClipboardItem::new_string(self.content[block].to_string()));
            self.copied = Some(start);
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(Duration::from_millis(1500)).await;
                this.update(cx, |this, cx| {
                    this.copied = None;
                    cx.notify();
                })
                .ok();
            })
            .detach();
            return cx.notify();
        }
        if let Some((cols, rows)) = self.grid_at(e.position) {
            return self.insert_table(cols, rows, cx);
        }
        self.grid = None;
        // Poignées d'un tableau : prendre une ligne, une colonne ou tout, insérer à un trait.
        if let Some((table, part)) = self.part_at(e.position) {
            return self.grip_down(table, part, e.position, cx);
        }
        // Boutons « + » d'un tableau : une ligne à la fin, ou une colonne.
        if let Some((at, bar)) = self.plus_at(e.position)
            && let Some((range, mut rows, _)) = self.table_at(at)
        {
            let cell = if bar == 0 {
                rows.push(vec![String::new(); rows[0].len()]);
                (rows.len() - 1, 0)
            } else {
                let cells = rows.iter_mut().enumerate();
                cells.for_each(|(i, row)| row.push(if i == 1 { "---" } else { "" }.to_string()));
                (0, rows[0].len() - 1)
            };
            return self.set_table(range, &rows, cell, cx);
        }
        let i = self.index_at(e.position);
        if e.modifiers.alt {
            return self.add_cursor(i, cx);
        }
        // Un clic ordinaire revient à un seul curseur.
        self.more.clear();
        let lr = self.line_range(i);
        let line = &self.content[lr.clone()];
        let col = i - lr.start;
        if e.modifiers.secondary()
            && let Some((_, link)) = md::links(line).into_iter().find(|(r, _)| r.contains(&col))
        {
            return cx.emit(if e.modifiers.shift { EditorEvent::OpenAside(link) } else { EditorEvent::Open(link) });
        }
        if !e.modifiers.secondary()
            && let Some(at) = self.checkbox_at(e.position)
        {
            return self.flip_box(at, cx);
        }
        let (kind, marker) = md::classify(line, self.in_code(lr.start));
        if matches!(kind, Kind::Task(_)) && col + 4 >= marker && col < marker {
            self.move_to(lr.end, cx);
            return self.toggle_task(cx);
        }
        match e.click_count {
            n @ (2 | 3) => {
                let unit = self.unit_at(i, n == 3);
                self.move_to(unit.start, cx);
                self.select_to(unit.end, cx);
                self.selecting = true;
                self.anchor = Some((unit, n == 3));
            }
            _ => {
                self.selecting = true;
                self.anchor = None;
                if e.modifiers.shift {
                    self.select_to(i, cx)
                } else {
                    self.move_to(i, cx)
                }
            }
        }
    }

    /// Clic droit : le menu de la note. Hors de la sélection, le curseur vient d'abord sous
    /// le pointeur, pour que « Coller » et « Lien » agissent là où l'on a cliqué.
    fn menu_down(&mut self, e: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        // Sur une poignée de tableau : elle prend sa ligne ou sa colonne, et le menu en parle.
        if let Some((table, part)) = self.part_at(e.position)
            && matches!(part, Part::Col(_) | Part::Row(_) | Part::Corner)
        {
            self.pick_part(table, part, cx);
            return cx.emit(EditorEvent::Menu(e.position, None, false));
        }
        let i = self.index_at(e.position);
        if !(self.sel.start..=self.sel.end).contains(&i) {
            self.move_to(i, cx);
        }
        let lr = self.line_range(i);
        let link = md::links(&self.content[lr.clone()]).into_iter().find(|(r, _)| (r.start..=r.end).contains(&(i - lr.start)));
        let col = i - lr.start;
        let commented = md::comments(&self.content[lr]).iter().any(|(whole, ..)| (whole.start..=whole.end).contains(&col));
        cx.emit(EditorEvent::Menu(e.position, link.map(|(_, link)| link), commented));
    }

    /// Mot (ou ligne entière) autour de l'octet `i`.
    fn unit_at(&self, i: usize, by_line: bool) -> Range<usize> {
        let lr = self.line_range(i);
        if by_line {
            return lr;
        }
        let col = i - lr.start;
        let word = self.content[lr.clone()]
            .split_word_bound_indices()
            .find(|(s, w)| (*s..=s + w.len()).contains(&col))
            .map_or(col..col, |(s, w)| s..s + w.len());
        lr.start + word.start..lr.start + word.end
    }

    /// Le glisser étend la sélection jusqu'à `i` ; après un double ou triple clic,
    /// par mots ou par lignes entières, le mot ou la ligne saisis restant sélectionnés.
    fn drag_to(&mut self, i: usize, cx: &mut Context<Self>) {
        let Some((first, by_line)) = self.anchor.clone() else {
            return self.select_to(i, cx);
        };
        let unit = self.unit_at(i, by_line);
        self.reversed = unit.start < first.start;
        self.sel = if self.reversed { unit.start..first.end } else { first.start..unit.end.max(first.end) };
        self.goal_x = None;
        self.reveal = true;
        cx.notify();
    }

    fn mouse_move(&mut self, e: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        // Un bouton relâché hors de la fenêtre n'envoie pas de `mouse_up` : on ne
        // continue pas à sélectionner pointeur libre.
        if self.selecting && e.pressed_button != Some(MouseButton::Left) {
            self.selecting = false;
        }
        if self.selecting {
            self.drag_to(self.index_at(e.position), cx);
        }
        self.aim_hand(e.position, e.modifiers, cx);
        // Une poignée tirée : le trait où elle se poserait suit le pointeur, après quelques pixels.
        if let Some(moving) = &mut self.moving {
            if e.pressed_button != Some(MouseButton::Left) {
                self.moving = None;
            } else if (e.position - moving.from).magnitude() > 4. {
                let at = self.geos.iter().find(|g| g.start == moving.table).map(|geo| geo.boundary(moving.part, e.position));
                moving.to = at;
            }
            return cx.notify();
        }
        let part = self.part_at(e.position);
        let near = self.geos.iter().map(|geo| geo.near(e.position)).collect::<Vec<_>>();
        let before = self.geos.iter().map(|geo| geo.near(self.pointer)).collect::<Vec<_>>();
        self.pointer = e.position;
        if part != self.part_hover || near != before {
            self.part_hover = part;
            cx.notify();
        }
        let hover = self.copy_at(e.position);
        if hover != self.hover {
            self.hover = hover;
            cx.notify();
        }
        if let Some(cell) = self.grid_at(e.position)
            && self.grid != Some(cell)
        {
            self.grid = Some(cell);
            self.grid_typed.clear();
            cx.notify();
        }
        // Un tableau montre ses boutons « + » dès qu'on s'en approche.
        let bar = self.plus_at(e.position);
        let near = |(range, frame): &(Range<usize>, Bounds<Pixels>)| {
            frame.dilate(px(20.)).contains(&e.position).then_some((range.start, None))
        };
        let over = bar.map(|(at, bar)| (at, Some(bar))).or_else(|| self.tables.iter().find_map(near));
        if over != self.table_hover {
            self.table_hover = over;
            cx.notify();
        }
    }

    fn mouse_up(&mut self, e: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.selecting = false;
        if let Some(moving) = self.moving.take() {
            self.drop_part(moving, cx);
            cx.notify();
        }
        self.aim_hand(e.position, e.modifiers, cx);
    }

    fn scroll(&mut self, e: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        let delta = e.delta.pixel_delta(px(26.));
        // Maj + molette, ou geste horizontal du pavé tactile : un tableau trop large défile.
        let (dx, dy) = if e.modifiers.shift { (delta.y, delta.x) } else { (delta.x, delta.y) };
        if let Some((range, _)) = self.tables.iter().find(|(_, frame)| frame.contains(&e.position)) {
            *self.table_x.entry(range.start).or_default() -= dx;
        }
        self.scroll_y -= dy;
        cx.notify();
    }

    // ----- Presse-papiers -----

    /// Cellules sélectionnées d'un tableau, en TSV : les tableurs les collent telles quelles,
    /// et dans une note, le collage reforme le tableau Markdown.
    fn cells_text(&self) -> Option<String> {
        let mut rows: Vec<Vec<String>> = Vec::new();
        let mut line = None;
        // Un « tout sélectionner » finit après le dernier retour à la ligne : hors du tableau.
        let end = self.content[..self.sel.end].trim_end_matches('\n').len().max(self.sel.start);
        for cell in self.cells_in(self.sel.start..end)? {
            let start = self.line_range(cell.start).start;
            if line != Some(start) {
                rows.push(Vec::new());
                line = Some(start);
            }
            rows.last_mut()?.push(self.content[cell].trim().replace("\\|", "|"));
        }
        Some(rows.iter().map(|row| row.join("\t")).collect::<Vec<_>>().join("\n"))
    }

    fn copy(&mut self, cut: bool, cx: &mut Context<Self>) {
        if let Some(text) = self.pick_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            if cut {
                self.clear_pick(cx);
            }
            return;
        }
        if self.sel.is_empty() {
            return;
        }
        let text = self.cells_text().unwrap_or_else(|| self.content[self.sel.clone()].to_string());
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        if cut && !self.clear_cells(cx) {
            self.edit(self.sel.clone(), "", cx);
            self.realign(cx);
        }
    }

    /// Collage dans un tableau : une cellule garde une seule ligne et ses `|`
    /// protégés ; des cellules de tableur ou de Markdown remplissent la grille à
    /// partir de la cellule du curseur, qui s'agrandit au besoin.
    fn paste_in_table(&mut self, text: &str, cx: &mut Context<Self>) -> bool {
        self.clear_cells(cx);
        let sel = self.sel.clone();
        if sel.end > self.line_range(sel.start).end {
            return false;
        }
        let Some((range, mut rows, (r, c))) = self.table_at(sel.start) else {
            return false;
        };
        let Some(grid) = md::paste_grid(text) else {
            // Avant le premier `|` ou après le dernier : dans la cellule voisine, pas sur la ligne.
            let line = self.line_range(sel.start);
            let bars = md::bars(&self.content[line.clone()]);
            let col = sel.start - line.start;
            let cell = md::cells(&self.content[line.clone()]).get(c).cloned().unwrap_or(0..0);
            let sel = match (bars.first(), bars.last()) {
                _ if !sel.is_empty() => sel,
                (Some(&first), _) if col <= first => line.start + cell.start..line.start + cell.start,
                (Some(_), Some(&last)) if col > last && self.content[line.clone()].trim_end().ends_with('|') => line.start + cell.end..line.start + cell.end,
                _ => sel,
            };
            self.push_undo(true);
            self.edit(sel, &md::cell_text(text), cx);
            self.realign(cx);
            return true;
        };
        let width = rows[0].len().max(c + grid.iter().map(Vec::len).max().unwrap_or(0));
        for (i, row) in rows.iter_mut().enumerate() {
            row.resize(width, if i == 1 { "---" } else { "" }.to_string());
        }
        // La ligne de tirets (la deuxième) se saute.
        let mut at = if r == 1 { 2 } else { r };
        let mut last = (at, c);
        for cells in &grid {
            at += (at == 1) as usize;
            if at == rows.len() {
                rows.push(vec![String::new(); width]);
            }
            rows[at][c..c + cells.len()].clone_from_slice(cells);
            last = (at, c + cells.len() - 1);
            at += 1;
        }
        self.set_table(range, &rows, last, cx);
        true
    }

    fn paste(&mut self, cx: &mut Context<Self>) {
        let Some(item) = cx.read_from_clipboard() else {
            return;
        };
        let image = item.entries().iter().find_map(|entry| match entry {
            ClipboardEntry::Image(image) => Some(image),
            _ => None,
        });
        let text = match (image, self.dirs.first()) {
            // Une image collée est enregistrée à côté de la note, qui la désigne.
            (Some(image), Some(dir)) => {
                let extension = match image.format {
                    ImageFormat::Png => "png",
                    ImageFormat::Jpeg => "jpg",
                    ImageFormat::Webp => "webp",
                    ImageFormat::Gif => "gif",
                    ImageFormat::Svg => "svg",
                    ImageFormat::Bmp => "bmp",
                    ImageFormat::Tiff => "tiff",
                };
                match vault::save_image(dir, extension, &image.bytes) {
                    Ok(name) => format!("![]({name})"),
                    Err(_) => return,
                }
            }
            _ => match item.text() {
                Some(text) => text.replace("\r\n", "\n"),
                None => return,
            },
        };
        if self.paste_in_table(&text, cx) {
            return;
        }
        // Un tableau Markdown, ou des cellules de tableur ou de page web, se posent en tableau,
        // sur leurs propres lignes et remis en forme.
        let line = self.line_range(self.sel.start);
        let (sel, table) = (self.sel.clone(), md::paste_table(&text).filter(|_| !self.in_code(line.start)));
        if let Some(table) = table {
            let alone = self.content[line.start..sel.start].trim().is_empty();
            let apart = line.start == 0 || self.content[..line.start].ends_with("\n\n");
            let lead = match (alone, apart) {
                (true, true) => "",
                (true, false) => "\n",
                (false, _) => "\n\n",
            };
            let rest = self.line_range(sel.end);
            let next = self.content[rest.end..].strip_prefix('\n').and_then(|n| n.lines().next());
            let tail = if !self.content[sel.end..rest.end].trim().is_empty() {
                "\n\n"
            } else if next.is_some_and(|n| !n.trim().is_empty()) {
                "\n"
            } else {
                ""
            };
            let from = if alone { line.start } else { sel.start };
            self.push_undo(true);
            return self.edit(from..sel.end, &format!("{lead}{table}{tail}"), cx);
        }
        self.push_undo(true);
        // Une adresse collée sur du texte en fait un lien.
        if !sel.is_empty() && md::is_url(text.trim()) && self.linkable() {
            let linked = format!("[{}]({})", &self.content[sel.clone()], text.trim());
            return self.edit(sel, &linked, cx);
        }
        self.edit(sel, &text, cx);
        // Une ligne collée sous un tableau en fait partie : ses colonnes s'alignent.
        self.realign(cx);
    }

    // ----- UTF-16 (méthodes de saisie) -----

    fn from_utf16(&self, offset: usize) -> usize {
        let mut utf16 = 0;
        for (i, ch) in self.content.char_indices() {
            if utf16 >= offset {
                return i;
            }
            utf16 += ch.len_utf16();
        }
        self.content.len()
    }

    fn to_utf16(&self, offset: usize) -> usize {
        self.content[..offset].encode_utf16().count()
    }

    fn range_to_utf16(&self, r: &Range<usize>) -> Range<usize> {
        self.to_utf16(r.start)..self.to_utf16(r.end)
    }

    fn range_from_utf16(&self, r: &Range<usize>) -> Range<usize> {
        self.from_utf16(r.start)..self.from_utf16(r.end)
    }

    // ----- Mise en page et rendu -----

    /// Les zones à remettre en page, quand le reste de la dernière mise en page peut être repris :
    /// le texte touché, la ligne où était le curseur et celle où il est, élargies aux blocs qui
    /// doivent rester entiers (tableau, citation). `None` : tout refaire.
    fn plan(&self, width: Pixels, room: Pixels, cursor: usize) -> Option<Plan> {
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
    fn signature(&self) -> Vec<(usize, usize, i64, i64, i64, bool, Kind, i64)> {
        let px = |v: Pixels| (f32::from(v) * 100.).round() as i64;
        self.rows.iter().map(|r| (r.start, r.len, px(r.y), px(r.height), px(r.pad), r.opens, r.kind, px(r.gap))).collect()
    }

    /// Chaque image : la mise en page lourde n'est refaite que si le texte, la largeur, le thème
    /// ou le curseur ont changé ; sinon, seul ce qui dépend du défilement est recalculé.
    fn layout(&mut self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
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
    fn relayout(&mut self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
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
        let look = figure::Look {
            dark: t.bg.l < 0.5,
            bg: hex(t.bg),
            fill: hex(t.code_bg),
            text: hex(t.text),
            line: hex(t.dim),
            font: sans().to_string(),
        };
        let key = |kind: u8, source: &str| {
            let mut hasher = Fast::default();
            (kind, source, &look.bg, &look.fill, &look.text, &look.line, &look.font, t.size.to_bits())
                .hash(&mut hasher);
            hasher.finish()
        };
        let style = {
            let mut hasher = Fast::default();
            (key(0, ""), hex(t.accent), mono(), has_unequal).hash(&mut hasher);
            hasher.finish()
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
                    // ponytail: un diagramme se dessine sur le thread UI (quelques dizaines
                    // de ms), donc seulement une fois le curseur sorti du bloc ; passer
                    // en tâche de fond si de gros diagrammes font attendre.
                    let editing = (start..=offset + line.len()).contains(&cursor);
                    drawing = cached(&mut old, &mut kept, key(0, &source), editing, || {
                        figure::mermaid(&source, &look)
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
                            figure::latex(&source, true, rgb(t.text), t.size * MATH_SCALE)
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
                    figure::latex(source, *display, rgb(t.text), t.size * MATH_SCALE)
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
            // Un schéma de Bref est redessiné aux couleurs du thème, et à chaque fois
            // que son fichier change : sa date fait partie de sa clé.
            if svg
                && let Some(file) = &file
                && let Ok(stamp) = fs::metadata(file).and_then(|m| m.modified())
            {
                let own = cached(&mut old, &mut kept, key(3, &format!("{}{stamp:?}", file.display())), false, || {
                    let diagram = Diagram::from_svg(&fs::read_to_string(file).ok()?)?;
                    figure::sharpen(&diagram.to_svg(crate::canvas::neutral(t)))
                });
                drawing = own.or(drawing);
            }
            let image = file.filter(|_| drawing.is_none()).and_then(|file| {
                // gpui 0.2 rend un SVG deux fois plus grand que nature, pour qu'il reste
                // net (`SMOOTH_SVG_SCALE_FACTOR`, qui n'est pas public).
                let zoom = if svg { 2. } else { 1. };
                let loaded = window.use_asset::<ImgResourceLoader>(&Resource::Path(file.into()), cx);
                loading |= loaded.is_none();
                let image = loaded?.ok()?;
                let natural = |d: gpui::DevicePixels| d.0 as f32 / zoom;
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
        self.loading = loading;
        if incremental {
            kept.extend(old);
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
    fn materialize(&mut self, bounds: Bounds<Pixels>, window: &mut Window) {
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
    fn place(&mut self, bounds: Bounds<Pixels>, window: &mut Window, _: &mut App) {
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
    }

    fn paint(&self, focused: bool, window: &mut Window, cx: &mut App) {
        let (t, o, w) = (self.theme, self.origin, self.width);
        let cursor = self.cursor();
        let sel = &self.sel;
        let label = |text: &str, color, window: &mut Window| {
            let run = TextRun {
                len: text.len(),
                font: font(sans()),
                color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            window
                .text_system()
                .shape_line(SharedString::from(text.to_string()), px(14.), &[run], None)
        };
        for row in &self.rows {
            let top = o.y + row.y;
            if top + row.height < self.viewport.top() || top > self.viewport.bottom() {
                continue;
            }
            let text_top = top + row.pad;
            let end = row.start + row.len;
            let has_cursor = (row.start..=end).contains(&cursor);
            let block = |x: Pixels, y: Pixels, w: Pixels, h: Pixels| {
                Bounds::new(point(o.x + x, y), size(w, h))
            };
            match row.kind {
                Kind::Code | Kind::Fence => {
                    window.paint_quad(fill(block(px(-12.), top, w + px(24.), row.height), t.code_bg))
                }
                Kind::Quote => {
                    if let Some(tint) = row.tint {
                        window.paint_quad(fill(block(px(-14.), top, w + px(28.), row.height), tint.opacity(0.1)));
                    }
                    window.paint_quad(fill(block(px(-14.), top, px(3.), row.height), row.tint.unwrap_or(t.border)))
                }
                Kind::Rule if !has_cursor => {
                    let mid = top + row.height / 2.;
                    window.paint_quad(fill(block(px(0.), mid, w, px(1.)), t.border));
                    continue;
                }
                _ => {}
            }
            // Un tableau plus large que la page est coupé à ses bords.
            let mask = (row.kind == Kind::Table).then(|| {
                let frame = self.tables.iter().find(|(range, _)| range.contains(&row.start)).map(|(_, frame)| *frame);
                let (left, wide) = frame.map_or((o.x, w), |f| (f.left(), f.size.width));
                ContentMask { bounds: Bounds::new(point(left - px(2.), top), size(wide + px(4.), row.height)) }
            });
            if row.hidden() {
                continue;
            }
            window.with_content_mask(mask, |window| {
                // Dans un tableau, le fond de l'en-tête passe sous la sélection.
                let table_top = point(o.x - row.dx, top);
                if let Some(grid) = &row.grid {
                    grid.paint_back(table_top, row.height - row.gap, &t, window);
                }
                // Fond d'un passage du texte sur cette ligne : la sélection, les passages trouvés.
                let mark = |range: &Range<usize>, color: Hsla, window: &mut Window| {
                    let a = row.pos(range.start.max(row.start) - row.start);
                    let b = row.pos(range.end.min(end) - row.start);
                    let tail = if range.end > end { px(6.) } else { px(0.) };
                    let mut rect = |x: Pixels, y: Pixels, w: Pixels, h: Pixels| {
                        window.paint_quad(fill(block(x, text_top + y, w, h), color))
                    };
                    if let Some(grid) = &row.grid {
                        for (x, y, w) in grid.spans(range.start.max(row.start) - row.start, range.end.min(end) - row.start, row.lh) {
                            rect(x - row.dx, y, w, row.lh);
                        }
                    } else if a.y == b.y {
                        rect(a.x, a.y, b.x - a.x + tail, row.lh);
                    } else {
                        rect(a.x, a.y, w - a.x, row.lh);
                        rect(px(0.), a.y + row.lh, w, b.y - a.y - row.lh);
                        rect(px(0.), b.y, b.x + tail, row.lh);
                    }
                };
                // Le texte surligné (`==texte==`), sous la sélection ; les trouvailles et commentaires passent dessus.
                if !matches!(row.kind, Kind::Code | Kind::Fence | Kind::Meta | Kind::Rule) {
                    for (r, color) in md::highlights(&self.content[row.start..end], row.kind == Kind::Table) {
                        mark(&(row.start + r.start..row.start + r.end), highlight(color), window);
                    }
                }
                if !sel.is_empty() && sel.start <= end && sel.end > row.start {
                    mark(sel, t.selection, window);
                }
                for (other, _) in self.more.iter().filter(|(other, _)| !other.is_empty() && other.start <= end && other.end > row.start) {
                    mark(other, t.selection, window);
                }
                // Le texte qui porte un commentaire est surligné.
                if !matches!(row.kind, Kind::Code | Kind::Fence) {
                    for (_, noted, _) in md::comments(&self.content[row.start..end]) {
                        mark(&(row.start + noted.start..row.start + noted.end), t.accent.opacity(0.22), window);
                    }
                }
                if let Some(find) = &self.find {
                    let from = find.hits.partition_point(|hit| hit.end <= row.start);
                    for (i, hit) in find.hits.iter().enumerate().skip(from).take_while(|(_, hit)| hit.start < end) {
                        mark(hit, t.accent.opacity(if i == find.at { 0.45 } else { 0.2 }), window);
                    }
                }
                if let Some(grid) = &row.grid {
                    grid.paint_front(table_top, row.height - row.gap, row.lh, &t, window, cx);
                } else if let Some(line) = &row.shaped.line {
                    line.paint(point(o.x - row.dx, text_top), row.lh, TextAlign::Left, None, window, cx)
                        .ok();
                }
                if focused && sel.is_empty() && has_cursor {
                    let p = row.pos(cursor - row.start);
                    let caret = block(p.x, text_top + p.y + row.lh * 0.14, px(2.), row.lh * 0.72);
                    window.paint_quad(fill(caret, t.accent));
                }
                // Les autres curseurs, quand la note a la saisie.
                for (other, back) in self.more.iter().filter(|_| focused && row.grid.is_none()) {
                    let at = if *back { other.start } else { other.end };
                    if other.is_empty() && (row.start..=end).contains(&at) {
                        let p = row.pos(at - row.start);
                        let caret = block(p.x, text_top + p.y + row.lh * 0.14, px(2.), row.lh * 0.72);
                        window.paint_quad(fill(caret, t.accent));
                    }
                }
            });
            if let Some((image, s)) = &row.image {
                let at = block(px(0.), top + row.height - s.height, s.width, s.height);
                window.paint_image(at, Corners::all(px(6.)), image.clone(), 0, false).ok();
            }
            // Bouton de copie du bloc : la même icône que pour copier la note.
            if row.opens {
                let bounds = self.copy_bounds(row);
                if self.hover == Some(row.start) {
                    window.paint_quad(fill(bounds, t.border).corner_radii(px(6.)));
                }
                let icon = if self.copied == Some(row.start) { "check.svg" } else { "copy.svg" };
                let inset = (COPY_SIZE - px(16.)) / 2.;
                let at = Bounds::new(bounds.origin + point(inset, inset), size(px(16.), px(16.)));
                window.paint_svg(at, icon.into(), TransformationMatrix::unit(), t.text, cx).ok();
                if let Some((_, hitbox)) = self.copy_hitboxes.iter().find(|(s, _)| *s == row.start) {
                    window.set_cursor_style(CursorStyle::PointingHand, hitbox);
                }
            }
        }
        if self.content.is_empty() {
            // Le reste de la page vide (date, touches, notes à relire) est à `Shell::render_empty`.
            label(tr("Write here…", "Écris ici…"), t.soft, window)
                .paint(point(o.x + px(8.), o.y + px(3.)), px(22.), window, cx)
                .ok();
        }
        for thumb in &self.thumbs {
            window.paint_quad(fill(*thumb, t.dim.opacity(0.5)).corner_radii(px(1.5)));
        }
        // Le trait où se poseraient des fichiers glissés sur la note.
        if let Some(mark) = self.drop_mark
            && let Some(last) = self.rows.last()
        {
            let y = match self.row_at(mark).filter(|_| mark < self.content.len()) {
                Some(r) => self.rows[r].y,
                None => last.y + last.height,
            };
            let line = Bounds::new(point(o.x - px(6.), o.y + y - px(1.)), size(w + px(12.), px(2.)));
            window.paint_quad(fill(line, t.accent).corner_radii(px(1.)));
        }
        for (n, (range, frame)) in self.tables.iter().enumerate() {
            let hover = self.table_hover.filter(|(at, _)| *at == range.start);
            if hover.is_none() && !(focused && (range.start..=range.end).contains(&cursor)) {
                continue;
            }
            for (i, bar) in plus_bars(*frame).into_iter().enumerate() {
                let hot = hover.is_some_and(|(_, over)| over == Some(i));
                window.paint_quad(fill(bar, if hot { t.selection } else { t.code_bg }).corner_radii(px(4.)));
                let icon = Bounds::new(bar.center() - point(px(5.), px(5.)), size(px(10.), px(10.)));
                let color = if hot { t.accent } else { t.soft };
                window.paint_svg(icon, "plus.svg".into(), TransformationMatrix::unit(), color, cx).ok();
                if let Some(hitbox) = self.table_hitboxes.get(n * 2 + i) {
                    window.set_cursor_style(CursorStyle::PointingHand, hitbox);
                }
            }
        }
        // Poignées d'un tableau : une pastille à trois points sur la colonne et la ligne que le pointeur
        // survole (et sur ce qu'on a pris), ce qu'elles ont pris, et le trait où l'on dépose ce qu'on tire.
        for geo in &self.geos {
            let hot = self.part_hover.filter(|(at, _)| *at == geo.start).map(|(_, part)| part);
            let (near, pick) = (geo.near(self.pointer), self.live_pick(geo.start));
            if let Some(pick) = pick {
                let area = geo.cells(&pick.rows, &pick.cols).intersect(&geo.frame);
                window.paint_quad(quad(area, px(2.), t.selection, px(1.), t.accent.opacity(0.7), BorderStyle::default()));
            }
            let held = |part: Part| {
                pick.is_some_and(|p| {
                    let (all_rows, all_cols) = (p.rows.len() == geo.tall(), p.cols.len() == geo.cols());
                    match part {
                        Part::Col(c) => all_rows && p.cols.contains(&c),
                        Part::Row(r) => all_cols && p.rows.contains(&r),
                        Part::Corner => all_rows && all_cols,
                        _ => false,
                    }
                })
            };
            for (part, zone) in geo.parts() {
                if !geo.visible(part, near, pick) {
                    continue;
                }
                if matches!(part, Part::JoinCol(_) | Part::JoinRow(_)) {
                    if hot == Some(part) {
                        let dot = Bounds::new(zone.center() - point(px(7.), px(7.)), size(px(14.), px(14.)));
                        window.paint_quad(fill(dot, t.accent).corner_radii(px(7.)));
                        let icon = Bounds::new(zone.center() - point(px(5.), px(5.)), size(px(10.), px(10.)));
                        window.paint_svg(icon, "plus.svg".into(), TransformationMatrix::unit(), t.bg, cx).ok();
                    }
                    continue;
                }
                // Au repos : un fond clair et un filet ; sous le pointeur : l'accent en transparence ; pris : l'accent.
                let (back, edge, dots) = if held(part) {
                    (t.accent, t.accent, t.bg)
                } else if hot == Some(part) {
                    (t.selection, t.accent.opacity(0.6), t.accent)
                } else {
                    (t.bg, t.border, t.soft)
                };
                window.paint_quad(quad(zone, zone.size.width.min(zone.size.height) / 2., back, px(1.), edge, BorderStyle::default()));
                // Trois points dans le sens de la poignée ; le coin n'en a pas.
                if part != Part::Corner {
                    let lengthwise = zone.size.width > zone.size.height;
                    for k in [-1., 0., 1.] {
                        let step = px(4.) * k;
                        let center = if lengthwise { zone.center() + point(step, px(0.)) } else { zone.center() + point(px(0.), step) };
                        let dot = Bounds::new(center - point(px(1.), px(1.)), size(px(2.), px(2.)));
                        window.paint_quad(fill(dot, dots).corner_radii(px(1.)));
                    }
                }
            }
            if let Some(Moving { table, part, to: Some(b), .. }) = &self.moving
                && *table == geo.start
            {
                let f = geo.frame;
                let line = match part {
                    Part::Col(_) => Bounds::new(point(geo.xs[*b] - px(1.), f.top()), size(px(2.), f.size.height)),
                    _ => Bounds::new(point(f.left(), geo.ys[*b] - px(1.)), size(f.size.width, px(2.))),
                };
                window.paint_quad(fill(line, t.accent));
            }
        }
        // Sélecteur de tableau : la grille, et la taille choisie dessous.
        if focused
            && let Some((cols, rows)) = self.grid
            && let Some(at) = self.popup_at(Self::grid_size())
        {
            let panel = Bounds::new(at, Self::grid_size());
            window.paint_quad(quad(panel, px(8.), t.panel, px(1.), t.border, BorderStyle::default()));
            for (col, row) in (0..GRID * GRID).map(|i| (i % GRID, i / GRID)) {
                let corner = at + point(px(8.) + GRID_STEP * col as f32, px(8.) + GRID_STEP * row as f32);
                let on = col < cols && row < rows;
                let (inside, edge) = if on { (t.selection, t.accent) } else { (t.code_bg, t.border) };
                let cell = Bounds::new(corner, size(px(16.), px(16.)));
                window.paint_quad(quad(cell, px(3.), inside, px(1.), edge, BorderStyle::default()));
            }
            if let Some(hitbox) = &self.grid_hitbox {
                window.set_cursor_style(CursorStyle::PointingHand, hitbox);
            }
            let size = format!("{cols} × {rows}");
            label(&size, t.text, window)
                .paint(point(at.x + px(8.), panel.bottom() - px(24.)), px(20.), window, cx)
                .ok();
            let hint = label(tr("or type 12x5", "ou tape 12x5"), t.soft, window);
            hint.paint(point(panel.right() - px(8.) - hint.width, panel.bottom() - px(24.)), px(20.), window, cx).ok();
        }
        if focused && let Some((_, items)) = self.completion() {
            let chosen = self.ac_index.min(items.len() - 1);
            // La liste suit le choix quand elle dépasse ce qu'elle montre.
            let first = chosen.saturating_sub(SHOWN - 1);
            let shown = &items[first..items.len().min(first + SHOWN)];
            let item_h = px(28.);
            let panel = size(px(280.), item_h * shown.len() as f32 + px(8.));
            let Some(at) = self.popup_at(panel) else {
                return;
            };
            window.paint_quad(quad(Bounds::new(at, panel), px(8.), t.panel, px(1.), t.border, BorderStyle::default()));
            for (i, choice) in shown.iter().enumerate() {
                let y = at.y + px(4.) + item_h * i as f32;
                if first + i == chosen {
                    let hl = Bounds::new(point(at.x + px(4.), y), size(px(272.), item_h));
                    window.paint_quad(fill(hl, t.selection).corner_radii(px(5.)));
                }
                // Le libellé à gauche, sa précision à droite, calée sur le bord : un libellé trop
                // long est coupé avant elle, jamais dessiné dessous.
                let mut write = |text: &str, detail: &str| {
                    let detail = label(detail, t.soft, window);
                    let right = at.x + panel.width - px(12.) - detail.width;
                    let room = right - at.x - px(24.);
                    let (mut cut, mut main) = (text.to_string(), label(text, t.text, window));
                    while main.width > room && cut.pop().is_some() {
                        main = label(&format!("{}…", cut.trim_end()), t.text, window);
                    }
                    main.paint(point(at.x + px(12.), y + px(4.)), px(20.), window, cx).ok();
                    detail.paint(point(right, y + px(4.)), px(20.), window, cx).ok();
                };
                match choice {
                    Choice::Name(name) => write(name, ""),
                    Choice::Command((name, en, fr, ..)) => write(tr(en, fr), &format!("/{name}")),
                    Choice::Date(en, fr, date) => write(tr(en, fr), &crate::date_name(*date)),
                }
            }
        }
    }
}

impl EntityInputHandler for Editor {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.sel),
            reversed: self.reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.as_ref().map(|r| self.range_to_utf16(r))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.find.as_ref().is_some_and(|find| find.active) {
            return self.find_type(text, cx);
        }
        // Plusieurs curseurs : chacun reçoit le texte, à la place de sa sélection.
        if !self.more.is_empty() && range_utf16.is_none() && self.marked.is_none() {
            return self.each(cx, |this, cx| this.edit(this.sel.clone(), text, cx));
        }
        if range_utf16.is_none() && self.marked.is_none() {
            self.clear_cells(cx);
        }
        let range = range_utf16
            .map(|r| self.range_from_utf16(&r))
            .or(self.marked.clone())
            .unwrap_or(self.sel.clone());
        if self.type_grid(text, cx) {
            return;
        }
        self.edit(range, text, cx);
        self.realign(cx);
        if text != " " {
            return;
        }
        // `[] ` en début de ligne devient une case à cocher.
        let c = self.cursor();
        let lr = self.line_range(c);
        let typed = self.content[lr.start..c].trim_start();
        let task = match typed {
            "[] " | "[ ] " => "- [ ] ",
            "[x] " => "- [x] ",
            _ => return,
        };
        if !self.in_code(lr.start) {
            self.edit(c - typed.len()..c, task, cx);
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        new_selected_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.find.as_ref().is_some_and(|find| find.active) {
            return;
        }
        let range = range_utf16
            .map(|r| self.range_from_utf16(&r))
            .or(self.marked.clone())
            .unwrap_or(self.sel.clone());
        let start = range.start;
        self.edit(range, text, cx);
        self.marked = (!text.is_empty()).then_some(start..start + text.len());
        if let Some(r) = new_selected_utf16 {
            let to = |n: usize| -> usize {
                text.char_indices()
                    .scan(0, |u, (i, ch)| {
                        let at = (*u, i);
                        *u += ch.len_utf16();
                        Some(at)
                    })
                    .find(|(u, _)| *u >= n)
                    .map_or(text.len(), |(_, i)| i)
            };
            self.sel = start + to(r.start)..start + to(r.end);
        }
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let i = self.from_utf16(range_utf16.start);
        let row = &self.rows[self.row_at(i)?];
        let p = row.pos(i.saturating_sub(row.start));
        Some(Bounds::new(
            point(self.origin.x + p.x, self.origin.y + row.y + row.pad + p.y),
            size(px(2.), row.lh),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.to_utf16(self.index_at(point)))
    }
}

impl Render for Editor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // À gauche des boutons que la fenêtre pose dans le coin (copie, commentaires).
        // Le compte se tronque dans une note étroite, au lieu de sortir par la gauche.
        let after = 12. + 34. * self.corner as f32;
        // La barre de mise en forme, à part du compteur et au milieu de la page.
        self.bar_at = self.bar_place();
        let folded = self.bar_at == Some(Bar::Compact);
        let bar = match self.bar_at {
            Some(Bar::Inline(_)) => self.bar_at.map(|mode| self.format_bar(mode, cx).into_any_element()),
            Some(Bar::Compact) if self.bar_open => Some(self.format_bar(Bar::Compact, cx).into_any_element()),
            _ => None,
        };
        let toggle = folded.then(|| self.bar_toggle(cx));
        // Repliée, la barre laisse la place de son bouton : le compteur se tronque avant de passer dessous.
        let counts = div().absolute().bottom_4().left(px(if folded { 46. } else { 8. })).right(px(after)).flex().justify_end().child(
            div()
                .debug_selector(|| "counts".into())
                .h(px(30.))
                .min_w_0()
                .px_2()
                .rounded(px(8.))
                .flex()
                .items_center()
                .bg(self.theme.bg)
                .text_size(px(12.))
                .text_color(self.theme.soft)
                .child(div().min_w_0().truncate().child(self.counts())),
        );
        // Avant le dessin : les passages suivent une note modifiée pendant que la barre est ouverte.
        self.refresh_find();
        let finding = self.find.as_ref().is_some_and(|find| find.active);
        let find_bar = self.find_bar(cx);
        macro_rules! motions {
            ($el:expr, $($action:ident => $motion:ident, $select:expr;)*) => {
                $el$(.on_action(cx.listener(|this, _: &$action, _, cx| {
                    this.each(cx, |this, cx| this.go(Motion::$motion, $select, cx))
                })))*
            };
        }
        let el = div()
            .size_full()
            .relative()
            // Barre de recherche en saisie : ses touches, et non celles du texte.
            .key_context(if finding { "Find" } else { "Editor" })
            .track_focus(&self.focus)
            .cursor(CursorStyle::IBeam);
        motions!(el,
            Left => Left, false; Right => Right, false; Up => Up, false; Down => Down, false;
            WordLeft => WordLeft, false; WordRight => WordRight, false;
            Home => Home, false; End => End, false;
            DocStart => DocStart, false; DocEnd => DocEnd, false;
            PageUp => PageUp, false; PageDown => PageDown, false;
            SelectLeft => Left, true; SelectRight => Right, true;
            SelectUp => Up, true; SelectDown => Down, true;
            SelectWordLeft => WordLeft, true; SelectWordRight => WordRight, true;
            SelectHome => Home, true; SelectEnd => End, true;
            SelectDocStart => DocStart, true; SelectDocEnd => DocEnd, true;
        )
        .on_action(cx.listener(|this, _: &Backspace, _, cx| this.each(cx, |this, cx| this.delete(Motion::Left, cx))))
        .on_action(cx.listener(|this, _: &Delete, _, cx| this.each(cx, |this, cx| this.delete(Motion::Right, cx))))
        .on_action(cx.listener(|this, _: &DeleteWordLeft, _, cx| this.each(cx, |this, cx| this.delete(Motion::WordLeft, cx))))
        .on_action(cx.listener(|this, _: &DeleteWordRight, _, cx| this.each(cx, |this, cx| this.delete(Motion::WordRight, cx))))
        .on_action(cx.listener(|this, _: &SelectNext, _, cx| this.select_next(cx)))
        .on_action(cx.listener(|this, _: &SelectAll, _, cx| {
            this.more.clear();
            this.move_to(0, cx);
            this.select_to(this.content.len(), cx);
        }))
        .on_action(cx.listener(|this, _: &Newline, _, cx| this.each(cx, |this, cx| this.newline(cx))))
        .on_action(cx.listener(|this, _: &Indent, _, cx| this.shift_lines(true, cx)))
        .on_action(cx.listener(|this, _: &Outdent, _, cx| this.shift_lines(false, cx)))
        .on_action(cx.listener(|this, _: &ToggleTask, _, cx| this.toggle_task(cx)))
        .on_action(cx.listener(|this, _: &ListBullet, _, cx| this.restyle(ListStyle::Bullet, cx)))
        .on_action(cx.listener(|this, _: &ListOrdered, _, cx| this.restyle(ListStyle::Ordered, cx)))
        .on_action(cx.listener(|this, _: &ListTask, _, cx| this.restyle(ListStyle::Task, cx)))
        .on_action(cx.listener(|this, _: &ListText, _, cx| this.restyle(ListStyle::Text, cx)))
        .on_action(cx.listener(|this, _: &ListCycle, _, cx| this.cycle_list(cx)))
        .on_action(cx.listener(|this, _: &TableRowAbove, _, cx| this.table_op(TableOp::RowAbove, cx)))
        .on_action(cx.listener(|this, _: &TableRowBelow, _, cx| this.table_op(TableOp::RowBelow, cx)))
        .on_action(cx.listener(|this, _: &TableColLeft, _, cx| this.table_op(TableOp::ColLeft, cx)))
        .on_action(cx.listener(|this, _: &TableColRight, _, cx| this.table_op(TableOp::ColRight, cx)))
        .on_action(cx.listener(|this, _: &TableDuplicateRow, _, cx| this.table_op(TableOp::DuplicateRow, cx)))
        .on_action(cx.listener(|this, _: &TableDuplicateCol, _, cx| this.table_op(TableOp::DuplicateCol, cx)))
        .on_action(cx.listener(|this, _: &TableMoveRowUp, _, cx| this.table_op(TableOp::MoveRowUp, cx)))
        .on_action(cx.listener(|this, _: &TableMoveRowDown, _, cx| this.table_op(TableOp::MoveRowDown, cx)))
        .on_action(cx.listener(|this, _: &TableMoveColLeft, _, cx| this.table_op(TableOp::MoveColLeft, cx)))
        .on_action(cx.listener(|this, _: &TableMoveColRight, _, cx| this.table_op(TableOp::MoveColRight, cx)))
        .on_action(cx.listener(|this, _: &TableDeleteRow, _, cx| this.table_op(TableOp::DeleteRow, cx)))
        .on_action(cx.listener(|this, _: &TableDeleteSpan, _, cx| this.table_delete_span(cx)))
        .on_action(cx.listener(|this, _: &TableDeleteCol, _, cx| this.table_op(TableOp::DeleteCol, cx)))
        .on_action(cx.listener(|this, _: &TableDelete, _, cx| this.table_op(TableOp::DeleteTable, cx)))
        .on_action(cx.listener(|this, _: &TableSortAsc, _, cx| this.table_op(TableOp::SortAsc, cx)))
        .on_action(cx.listener(|this, _: &TableSortDesc, _, cx| this.table_op(TableOp::SortDesc, cx)))
        .on_action(cx.listener(|this, _: &TableTranspose, _, cx| this.table_op(TableOp::Transpose, cx)))
        .on_action(cx.listener(|this, _: &AlignLeft, _, cx| this.align(":--", cx)))
        .on_action(cx.listener(|this, _: &AlignCenter, _, cx| this.align(":-:", cx)))
        .on_action(cx.listener(|this, _: &AlignRight, _, cx| this.align("--:", cx)))
        .on_action(cx.listener(|this, _: &Bold, _, cx| this.each(cx, |this, cx| this.mark(md::BOLD, cx))))
        .on_action(cx.listener(|this, _: &Italic, _, cx| this.each(cx, |this, cx| this.mark(md::ITALIC, cx))))
        .on_action(cx.listener(|this, _: &Underline, _, cx| this.each(cx, |this, cx| this.mark(md::UNDER, cx))))
        .on_action(cx.listener(|this, _: &Strike, _, cx| this.each(cx, |this, cx| this.mark(md::STRIKE, cx))))
        .on_action(cx.listener(|this, _: &Highlight, _, cx| this.each(cx, |this, cx| this.mark(md::HIGHLIGHT, cx))))
        .on_action(cx.listener(|this, _: &InlineCode, _, cx| this.each(cx, |this, cx| this.mark(md::CODE, cx))))
        .on_action(cx.listener(|this, _: &InsertLink, _, cx| this.link(cx)))
        .on_action(cx.listener(|this, _: &Comment, _, cx| this.comment(cx)))
        .on_action(cx.listener(|this, _: &Copy, _, cx| this.copy(false, cx)))
        .on_action(cx.listener(|this, _: &Cut, _, cx| this.copy(true, cx)))
        .on_action(cx.listener(|this, _: &Paste, _, cx| this.each(cx, |this, cx| this.paste(cx))))
        .on_action(cx.listener(|this, _: &Undo, _, cx| this.restore(false, cx)))
        .on_action(cx.listener(|this, _: &Redo, _, cx| this.restore(true, cx)))
        .on_action(cx.listener(|this, _: &Find, _, cx| this.open_find(false, cx)))
        .on_action(cx.listener(|this, _: &FindReplace, _, cx| this.open_find(true, cx)))
        .on_action(cx.listener(|this, _: &FindNext, _, cx| this.find_step(true, cx)))
        .on_action(cx.listener(|this, _: &FindEnter, _, cx| this.find_enter(cx)))
        .on_action(cx.listener(|this, _: &FindPrev, _, cx| this.find_step(false, cx)))
        .on_action(cx.listener(|this, _: &FindClose, _, cx| {
            this.find = None;
            cx.notify();
        }))
        .on_action(cx.listener(|this, _: &FindErase, _, cx| this.find_erase(cx)))
        .on_action(cx.listener(|this, _: &FindSwitch, _, cx| {
            if let Some(find) = &mut this.find {
                find.on_with = !find.on_with && find.with.is_some();
            }
            cx.notify();
        }))
        .on_action(cx.listener(|this, _: &FindCase, _, cx| {
            if let Some(find) = &mut this.find {
                find.case ^= true;
            }
            this.find_changed(cx);
        }))
        .on_action(cx.listener(|this, _: &FindWord, _, cx| {
            if let Some(find) = &mut this.find {
                find.word ^= true;
            }
            this.find_changed(cx);
        }))
        .on_action(cx.listener(|this, _: &FindRegex, _, cx| {
            if let Some(find) = &mut this.find {
                find.regex ^= true;
            }
            this.find_changed(cx);
        }))
        .on_action(cx.listener(|this, _: &FindPaste, _, cx| {
            if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                this.find_type(text.lines().next().unwrap_or_default(), cx);
            }
        }))
        .on_action(cx.listener(|this, _: &ReplaceAll, _, cx| this.replace_all(cx)))
        .on_action(cx.listener(|this, _: &ReplaceInVault, _, cx| this.replace_in_vault(cx)))
        .on_action(cx.listener(|this, _: &Cancel, _, cx| {
            // Les pastilles de couleur se referment d'abord.
            if std::mem::take(&mut this.swatches) || std::mem::take(&mut this.bar_open) {
                return cx.notify();
            }
            // Barre ouverte mais saisie dans la note : Échap la ferme d'abord.
            if this.find.take().is_some() {
                return cx.notify();
            }
            // Plusieurs curseurs : Échap revient à un seul.
            if !this.more.is_empty() {
                this.more.clear();
                return cx.notify();
            }
            // Une poignée tirée est lâchée sans rien déplacer ; ce qu'elle avait pris est relâché.
            if this.moving.take().is_some() {
                return cx.notify();
            }
            if this.pick.as_ref().is_some_and(|p| p.version == this.version && p.sel == this.sel) {
                this.pick = None;
                return cx.notify();
            }
            if this.grid.take().is_some() {
                cx.notify();
            } else if let Some((start, _)) = this.completion() {
                this.ac_dismissed = Some(start);
                cx.notify();
            } else {
                // Rien à fermer ici : à la fenêtre de voir (son menu).
                cx.propagate();
            }
        }))
        .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
        .on_mouse_down(MouseButton::Right, cx.listener(Self::menu_down))
        .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
        .on_mouse_up_out(MouseButton::Left, cx.listener(Self::mouse_up))
        .on_mouse_move(cx.listener(Self::mouse_move))
        .on_modifiers_changed(cx.listener(|this, e: &gpui::ModifiersChangedEvent, _, cx| this.aim_hand(this.pointer, e.modifiers, cx)))
        .on_scroll_wheel(cx.listener(Self::scroll))
        .child(EditorElement(cx.entity()))
        .child(counts)
        .children(toggle)
        .children(bar)
        .children(find_bar)
    }
}

struct EditorElement(Entity<Editor>);

impl IntoElement for EditorElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for EditorElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.0.update(cx, |editor, cx| {
            let before = editor.viewport.size.width;
            editor.layout(bounds, window, cx);
            // La place de la barre de mise en forme suit la largeur de la note : l'image suivante la déplace.
            if before != editor.viewport.size.width {
                cx.notify();
            }
        });
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus = self.0.read(cx).focus.clone();
        window.handle_input(&focus, ElementInputHandler::new(bounds, self.0.clone()), cx);
        let focused = focus.is_focused(window);
        self.0.update(cx, |editor, cx| {
            editor.paint(focused, window, cx);
            if let Some((hand, hitbox)) = editor.hand.zip(editor.view_hitbox.as_ref()) {
                window.set_cursor_style(hand, hitbox);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Une image du coffre se désigne par `..` (dossier frère) ; rien ne sort du coffre.
    #[test]
    fn images_stay_inside_the_vault() {
        let base = std::env::temp_dir().join(format!("bref-inside-{}", std::process::id()));
        let (vault, outside) = (base.join("vault"), base.join("outside.png"));
        fs::create_dir_all(vault.join("notes")).unwrap();
        fs::create_dir_all(vault.join("assets")).unwrap();
        fs::write(vault.join("assets/p.png"), "").unwrap();
        fs::write(&outside, "").unwrap();
        let notes = vault.join("notes");
        assert!(inside(&vault, &notes.join("../assets/p.png")));
        assert!(inside(&vault, &vault.join("assets/p.png")));
        assert!(!inside(&vault, &notes.join("../../outside.png")));
        assert!(!inside(&vault, &outside));
        assert!(!inside(&vault, &vault.join("absent.png")));
        // Un lien symbolique du coffre vers l'extérieur n'est pas suivi.
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, vault.join("lien.png")).unwrap();
            assert!(!inside(&vault, &vault.join("lien.png")));
        }
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn history_returns_each_state() {
        let states = ["", "é", "éa", "éa\nligne", "a\nligne", "a\nligné"];
        let mut history = History::default();
        for (i, state) in states.iter().enumerate() {
            history.push(state.to_string(), i..i);
        }
        // Seul le dernier état est gardé en entier.
        assert!(history.earlier.iter().all(|(_, _, own, _)| own.len() <= 2));
        for (i, state) in states.iter().enumerate().rev() {
            assert_eq!(history.pop(), Some((state.to_string(), i..i)));
        }
        assert_eq!(history.pop(), None);
    }

    #[test]
    fn tables_get_room_by_setting() {
        let (pane, text) = (px(1000.), px(720.));
        // Dans la colonne : rien ne change. Centré : toute la fenêtre moins les marges.
        assert_eq!(table_room(Wide::Column, pane, text), text);
        assert_eq!(table_room(Wide::Centered, pane, text), px(952.));
        // Fenêtre étroite : la colonne remplit déjà la place, rien ne déborde.
        let narrow = px(500.);
        assert_eq!(table_room(Wide::Centered, narrow, narrow - SIDE * 2.), narrow - SIDE * 2.);
        // Un tableau centré dépasse de la moitié de son excédent de chaque côté ; jamais à rebours.
        assert_eq!(table_lead(Wide::Centered, px(952.), text), px(116.));
        assert_eq!(table_lead(Wide::Centered, px(300.), text), px(0.));
        assert_eq!(table_lead(Wide::Column, px(952.), text), px(0.));
    }

    #[test]
    fn page_width_follows_the_text_size() {
        assert_eq!(Page::Normal.measure(16.), MAX_WIDTH);
        assert_eq!(Page::Normal.measure(32.), px(1440.));
        assert!(Page::Narrow.measure(16.) < Page::Normal.measure(16.) && Page::Normal.measure(16.) < Page::Wide.measure(16.));
        assert!(Page::Wide.measure(16.) < Page::Full.measure(16.));
        // Les clés se relisent ; une clé inconnue ou absente vaut le choix par défaut.
        assert!(Page::ALL.into_iter().all(|p| Page::of(p.key()) == p) && Wide::ALL.into_iter().all(|w| Wide::of(w.key()) == w));
        assert_eq!((Page::of(""), Page::of("?"), Wide::of(""), Wide::of("?")), (Page::Normal, Page::Normal, Wide::Centered, Wide::Centered));
    }
}
