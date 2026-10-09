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
    MOD, Theme, diagram::Diagram, figure, graph, grid::{self, TableRow}, mono, sans, tr, vault,
    markdown::{self as md, Enter, Kind, Link},
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
        Bold,
        Italic,
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
const MAX_WIDTH: Pixels = px(720.);
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
            tint
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
            underline: (m || f & md::LINK != 0).then_some(UnderlineStyle {
                thickness: px(1.),
                color: Some(if f & md::LINK != 0 { t.accent } else { color }.opacity(0.5)),
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
    /// Compteur du bas de page, gardé tant que le texte et la sélection ne bougent pas.
    counted: Option<((u64, Range<usize>), String)>,
    /// Nombre de boutons que la fenêtre pose dans le coin, à droite du compteur.
    pub corner: usize,
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
            table_x: HashMap::new(),
            thumbs: Vec::new(),
            theme,
            rows: Vec::new(),
            version: 0,
            counted: None,
            corner: 1,
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
        let key = (self.version, self.sel.clone());
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
            both(md::counts(&self.content))
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

    /// Premier tableau : décalage de ses lignes, plus grand nombre de lignes
    /// visuelles d'une des siennes, largeur de son cadre et de la page.
    #[cfg(test)]
    pub fn table_state(&self) -> Option<(Pixels, i32, Pixels, Pixels)> {
        let rows = self.rows.iter().filter(|r| r.kind == Kind::Table);
        let tall = rows.clone().map(Row::visual_rows).max()?;
        Some((rows.map(|r| r.dx).next()?, tall, self.tables.first()?.1.size.width, self.width))
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
                true => div().text_color(t.dim).child(hint),
                false => div().when(fresh, |d| d.bg(t.selection)).child(text.to_string()),
            };
            let field = div().flex_1().min_w_0().h(px(26.)).px_2().rounded(px(6.)).border_1().bg(t.bg);
            let field = field.border_color(if on { t.accent } else { t.border }).flex().items_center().overflow_hidden();
            match text.is_empty() {
                true => field.children(caret).child(shown),
                false => field.child(shown).children(caret),
            }
        };
        let toggle = |label: &'static str, on: bool| {
            let toggle = div().flex_none().h(px(26.)).px_1p5().rounded(px(6.)).flex().items_center().cursor_pointer();
            toggle.text_color(if on { t.accent } else { t.dim }).when(on, |d| d.bg(t.selection)).child(label)
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
            .absolute()
            .top(px(52.))
            .right_4()
            .w(px(340.))
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
        let first = div()
            .flex()
            .items_center()
            .gap_1p5()
            .child(field(&find.query, tr("Find", "Chercher"), find.active && !find.on_with, find.fresh).on_mouse_down(MouseButton::Left, pick(false)))
            .child(div().flex_none().text_size(px(12.)).text_color(t.dim).child(count))
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
        Some(bar.child(first).children(second))
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
        self.version += 1;
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

    fn restore(&mut self, redo: bool, cx: &mut Context<Self>) {
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

    fn wrap(&mut self, delimiter: &str, cx: &mut Context<Self>) {
        let sel = self.sel.clone();
        let n = delimiter.len();
        self.splice(sel.end..sel.end, delimiter, cx);
        self.splice(sel.start..sel.start, delimiter, cx);
        self.sel = sel.start + n..sel.end + n;
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
            return cx.emit(EditorEvent::Open(link));
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

    fn mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.selecting = false;
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
    fn plan(&self, width: Pixels, cursor: usize) -> Option<Plan> {
        let laid = self.laid.as_ref()?;
        let (rows, len) = (&self.rows, self.content.len());
        if laid.epoch != self.epoch
            || laid.width != f32::from(width).to_bits()
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
        let width = (bounds.size.width - px(48.)).min(MAX_WIDTH).max(px(120.));
        let text_system = window.text_system().clone();
        // Ce qui peut être repris de la mise en page précédente, quand seules quelques lignes ont changé.
        // Les premières lignes d'un très gros tableau fixent ses colonnes : si elles bougent, on refait tout.
        let plan = self.plan(width, cursor).filter(|plan| {
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
                    && grid::geometry(&mut grid::Cache::default(), &text_system, &look, width, &lines).same(geom)
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
                    let geom = Rc::new(grid::geometry(&mut cells, &text_system, &look, width, &block[..grid::SAMPLE]));
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
                pending = grid::build(&mut cells, &text_system, &look, width, &block, cursor, marked.as_ref()).iter().cloned().collect();
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
                (style, line, kind, marker, math, lang, has_cursor, near, f32::from(width).to_bits(), header, tinted)
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
        let (cursor, width) = (self.cursor(), self.width);
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
        self.thumbs.clear();
        for meta in &self.table_meta {
            let (start, end) = (meta.range.start, meta.range.end);
            let wide = meta.wide;
            let overflow = (wide - width).max(px(0.));
            let mut dx = scrolled.get(&start).copied().unwrap_or_default().max(px(0.)).min(overflow);
            if reveal && (start..=end).contains(&cursor) {
                let at = self.row_at(cursor).unwrap_or(meta.rows.start).clamp(meta.rows.start, meta.rows.end - 1);
                let row = &self.rows[at];
                let x = row.pos(cursor - row.start).x;
                let margin = px(24.);
                if x - dx < margin {
                    dx = x - margin;
                } else if x - dx > width - margin {
                    dx = x - width + margin;
                }
                dx = dx.max(px(0.)).min(overflow);
            }
            if overflow > px(0.) {
                self.table_x.insert(start, dx);
            }
            if self.rows[meta.rows.start].dx != dx {
                self.rows[meta.rows.clone()].iter_mut().for_each(|r| r.dx = dx);
            }
            let (first, last) = (&self.rows[meta.rows.start], &self.rows[meta.rows.end - 1]);
            let top = first.y + first.pad;
            let frame = Bounds::new(
                self.origin + point(px(0.), top),
                size(wide.min(width), last.y + last.height - last.gap - top),
            );
            if overflow > px(0.) {
                let thumb = (width * (width / wide)).max(px(24.));
                let at = (width - thumb) * (dx / overflow);
                let bar = Bounds::new(point(frame.left() + at, frame.bottom() - px(5.)), size(thumb, px(3.)));
                self.thumbs.push(bar);
            }
            self.tables.push((start..end, frame));
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
            let mask = (row.kind == Kind::Table).then(|| ContentMask {
                bounds: Bounds::new(point(o.x - px(2.), top), size(w + px(4.), row.height)),
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
            let hint = format!(
                "{}   {MOD}+P : notes   {MOD}+N : {}   F1 : {}",
                tr("Write here…", "Écris ici…"),
                tr("new note", "nouvelle note"),
                tr("shortcuts", "raccourcis"),
            );
            label(&hint, t.dim, window)
                .paint(point(o.x + px(8.), o.y + px(3.)), px(22.), window, cx)
                .ok();
        }
        for thumb in &self.thumbs {
            window.paint_quad(fill(*thumb, t.dim.opacity(0.5)).corner_radii(px(1.5)));
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
                let color = if hot { t.accent } else { t.dim };
                window.paint_svg(icon, "plus.svg".into(), TransformationMatrix::unit(), color, cx).ok();
                if let Some(hitbox) = self.table_hitboxes.get(n * 2 + i) {
                    window.set_cursor_style(CursorStyle::PointingHand, hitbox);
                }
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
            let hint = label(tr("or type 12x5", "ou tape 12x5"), t.dim, window);
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
                let mut write = |text: &str, x: Pixels, color| {
                    label(text, color, window).paint(point(at.x + x, y + px(4.)), px(20.), window, cx).ok();
                };
                match choice {
                    Choice::Name(name) => write(&name.chars().take(36).collect::<String>(), px(12.), t.text),
                    Choice::Command((name, en, fr, ..)) => {
                        write(tr(en, fr), px(12.), t.text);
                        write(&format!("/{name}"), px(186.), t.dim);
                    }
                    Choice::Date(en, fr, date) => {
                        write(tr(en, fr), px(12.), t.text);
                        write(&crate::date_name(*date), px(186.), t.dim);
                    }
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
        let counts = div()
            .absolute()
            .bottom_4()
            .right(px(12. + 34. * self.corner as f32))
            .h(px(30.))
            .px_2()
            .rounded(px(8.))
            .flex()
            .items_center()
            .bg(self.theme.bg)
            .text_size(px(12.))
            .text_color(self.theme.dim)
            .child(self.counts());
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
        .on_action(cx.listener(|this, _: &AlignLeft, _, cx| this.align(":--", cx)))
        .on_action(cx.listener(|this, _: &AlignCenter, _, cx| this.align(":-:", cx)))
        .on_action(cx.listener(|this, _: &AlignRight, _, cx| this.align("--:", cx)))
        .on_action(cx.listener(|this, _: &Bold, _, cx| this.each(cx, |this, cx| this.wrap("**", cx))))
        .on_action(cx.listener(|this, _: &Italic, _, cx| this.each(cx, |this, cx| this.wrap("*", cx))))
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
            // Barre ouverte mais saisie dans la note : Échap la ferme d'abord.
            if this.find.take().is_some() {
                return cx.notify();
            }
            // Plusieurs curseurs : Échap revient à un seul.
            if !this.more.is_empty() {
                this.more.clear();
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
        .on_scroll_wheel(cx.listener(Self::scroll))
        .child(EditorElement(cx.entity()))
        .child(counts)
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
        self.0.update(cx, |editor, cx| editor.layout(bounds, window, cx));
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
        self.0.update(cx, |editor, cx| editor.paint(focused, window, cx));
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
}
