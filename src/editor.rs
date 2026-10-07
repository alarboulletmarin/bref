//! Éditeur Markdown stylé en direct : le texte reste du Markdown brut, seul le
//! rendu change (tailles, graisses, couleurs).

use std::{
    borrow::Cow,
    collections::{HashMap, VecDeque},
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    ops::Range,
    path::PathBuf,
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
        Copy,
        Cut,
        Paste,
        Undo,
        Redo,
        Cancel,
    ]
);

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

/// Commande `/nom` : (nom, libellé anglais, libellé français, texte posé avant
/// le curseur, texte posé après).
type Command = (&'static str, &'static str, &'static str, &'static str, &'static str);

// ponytail: pas de `/date` : la bibliothèque standard ne connaît pas le fuseau
// horaire ; l'ajouter si une dépendance de dates entre un jour dans le projet.
const COMMANDS: &[Command] = &[
    ("h1", "Heading 1", "Titre 1", "# ", ""),
    ("h2", "Heading 2", "Titre 2", "## ", ""),
    ("h3", "Heading 3", "Titre 3", "### ", ""),
    ("list", "Bullet list", "Liste à puces", "- ", ""),
    ("num", "Numbered list", "Liste numérotée", "1. ", ""),
    ("todo", "Task", "Tâche à cocher", "- [ ] ", ""),
    ("table", "Table", "Tableau", "", ""),
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
    grid: Option<TableRow>,
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
        self.grid.as_ref().is_some_and(TableRow::hidden)
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
    old: &mut HashMap<u64, Drawing>,
    kept: &mut HashMap<u64, Drawing>,
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
    figures: HashMap<u64, Drawing>,
    /// Lignes mises en forme à la dernière frame, par empreinte de leur texte et
    /// de leur contexte.
    shaped: HashMap<u64, Rc<Shaped>>,
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
    origin: Point<Pixels>,
    width: Pixels,
    viewport: Bounds<Pixels>,
    scroll_y: Pixels,
    reveal: bool,
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
            figures: HashMap::new(),
            shaped: HashMap::new(),
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
            origin: Point::default(),
            width: MAX_WIDTH,
            viewport: Bounds::default(),
            scroll_y: px(0.),
            reveal: false,
        }
    }

    pub fn text(&self) -> &str {
        &self.content
    }

    /// Remplace tout le contenu (changement de note) ; curseur à l'octet `cursor`.
    pub fn load(&mut self, text: String, cursor: usize, cx: &mut Context<Self>) {
        let cursor = cursor.min(text.len());
        self.content = text;
        self.sel = cursor..cursor;
        self.reversed = false;
        self.marked = None;
        self.undo = History::default();
        self.redo = History::default();
        self.last_edit = None;
        self.scroll_y = px(0.);
        self.table_x.clear();
        self.selecting = false;
        self.reveal = true;
        cx.notify();
    }

    pub fn set_theme(&mut self, theme: Theme, cx: &mut Context<Self>) {
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
    }

    pub fn set_dirs(&mut self, dirs: Vec<PathBuf>) {
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

    // ponytail: chaque étape d'annulation (200 max) recopie une fois la note pour
    // en tirer la différence ; passer à un journal d'opérations si les notes
    // dépassent le Mo.
    fn push_undo(&mut self, boundary: bool) {
        let stale = self
            .last_edit
            .is_none_or(|t| t.elapsed() > Duration::from_millis(600));
        if boundary || stale {
            self.undo.push(self.content.clone(), self.sel.clone());
        }
        self.redo = History::default();
        self.last_edit = Some(Instant::now());
    }

    fn changed(&mut self, cx: &mut Context<Self>) {
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
        self.changed(cx);
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
            self.content = text;
            self.sel = cursor..cursor;
            self.reversed = false;
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
        to.push(std::mem::replace(&mut self.content, text), self.sel.clone());
        self.sel = sel;
        self.reversed = false;
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
        if self.sel.is_empty() {
            return None;
        }
        let (first, last) = (self.line_range(self.sel.start), self.line_range(self.sel.end));
        let (mut found, mut bars, mut line) = (Vec::new(), false, first.clone());
        loop {
            if !self.is_table_line(&line) {
                return None;
            }
            if !self.is_dash_row(&line) {
                let text = &self.content[line.clone()];
                let inside = |at: usize| (self.sel.start..self.sel.end).contains(&at);
                bars |= md::bars(text).into_iter().any(|bar| inside(line.start + bar));
                for cell in md::cells(text) {
                    let (from, to) = (line.start + cell.start, line.start + cell.end);
                    let (from, to) = (from.max(self.sel.start), to.min(self.sel.end));
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
            ranges.iter().rev().for_each(|r| self.content.replace_range(r.clone(), ""));
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

    // ----- Tableaux -----

    /// Tableau autour de l'octet `at` : son étendue, ses lignes, et la cellule
    /// (ligne, colonne) où se trouve `at`.
    fn table_at(&self, at: usize) -> Option<(Range<usize>, Vec<Vec<String>>, (usize, usize))> {
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
        self.content.replace_range(range, &text);
        self.sel = cursor..cursor;
        self.changed(cx);
    }

    /// Tab et Maj+Tab dans un tableau : cellule suivante ou précédente ; au bout
    /// du tableau, une ligne s'ajoute.
    fn table_step(&mut self, forward: bool, cx: &mut Context<Self>) -> bool {
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
        if !self.sel.is_empty() {
            return None;
        }
        let c = self.cursor();
        let line = self.line_range(c).start;
        let before = &self.content[line..c];
        let (start, items) = self.names(before, c).or_else(|| self.commands(before, c, line))?;
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
            Choice::Command(&(name, _, _, before, after)) => {
                let slash = start - 1;
                self.push_undo(true);
                if name == "table" {
                    self.edit(slash..c, "", cx);
                    self.grid = Some((3, 3));
                    self.grid_typed.clear();
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

    fn copy(&mut self, cut: bool, cx: &mut Context<Self>) {
        if self.sel.is_empty() {
            return;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(
            self.content[self.sel.clone()].to_string(),
        ));
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

    // ponytail: chaque frame parcourt toutes les lignes (classement, empreinte) et
    // garde leur mise en forme en mémoire, même hors de l'écran. Ne traiter que le
    // visible si une note de plusieurs dizaines de milliers de lignes devient lente.
    fn layout(&mut self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        let t = self.theme;
        let cursor = self.cursor();
        // Un ``` sans clôture n'ouvre pas de bloc : sinon chaque ``` tapé ferait
        // basculer en code, donc remettre en forme, toute la suite de la note. S'il
        // y en a un de trop, c'est celui qu'on est en train de taper, sinon le dernier.
        let mut fences = self.content.split('\n').filter(|l| md::is_fence(l)).count();
        let typed = self.line_range(cursor);
        let orphan = (fences % 2 == 1 && md::is_fence(&self.content[typed.clone()]))
            .then_some(typed.start);
        fences -= orphan.is_some() as usize;
        // Une police sans `≠` le compose d'un `=` et d'une barre mal placée : autant
        // laisser `!=`.
        let text_system = window.text_system().clone();
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
            let mut hasher = DefaultHasher::new();
            (kind, source, &look.bg, &look.fill, &look.text, &look.line, &look.font, t.size.to_bits())
                .hash(&mut hasher);
            hasher.finish()
        };
        let style = {
            let mut hasher = DefaultHasher::new();
            (key(0, ""), hex(t.accent), mono(), has_unequal).hash(&mut hasher);
            hasher.finish()
        };
        let mut cells = std::mem::take(&mut self.cells);
        cells.next_frame();
        // Lignes d'un tableau déjà mises en page, en attente de leur tour dans la boucle.
        let mut pending: VecDeque<Option<TableRow>> = VecDeque::new();
        let empty = Rc::new(Shaped { line: None, subs: Vec::new(), formula: None, picture: None });
        let mut stale = std::mem::take(&mut self.shaped);
        let mut shaped = HashMap::new();
        let mut old = std::mem::take(&mut self.figures);
        let mut kept = HashMap::new();
        // Bloc Mermaid ou `$$` en cours : son début et son source.
        let mut block: Option<(usize, String)> = None;
        let mut dollars = self.content.split('\n').filter(|l| l.trim() == "$$").count();
        let width = (bounds.size.width - px(48.)).min(MAX_WIDTH).max(px(120.));
        let marked = self.marked.clone();
        let mut rows = Vec::new();
        let mut y = px(0.);
        let mut offset = 0;
        let mut in_code = false;
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
        let mut tint = None;
        for line in self.content.split('\n') {
            let (mut kind, mut marker) = md::classify(line, in_code);
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
            let code = matches!(kind, Kind::Code | Kind::Fence);
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
                    .take_while(|l| md::classify(l, false).0 == Kind::Table)
                    .map(|l| {
                        at += l.len() + 1;
                        (at - l.len() - 1, l)
                    })
                    .collect();
                let look = grid::Look { theme: &t, style };
                pending = grid::build(&mut cells, &text_system, &look, width, &block, cursor, marked.as_ref()).into();
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
                Kind::Code | Kind::Fence | Kind::Table => (px(14.), px(0.)),
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
                let mut hasher = DefaultHasher::new();
                (style, line, kind, marker, math, lang, has_cursor, near, f32::from(width).to_bits(), header, tinted)
                    .hash(&mut hasher);
                hasher.finish()
            };
            let lh = (font_size * 1.65).round();
            let made = if grid_row.is_some() { empty.clone() } else { stale.remove(&id).or_else(|| shaped.get(&id).cloned()).unwrap_or_else(|| {
                let mut formula = None;
                let mut flags = vec![0u16; line.len()];
                let list = matches!(kind, Kind::Bullet | Kind::Ordered | Kind::Task(_));
                flags[..marker].fill(if list { md::MARK } else { md::DIM });
                if math {
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
            if grid_row.is_none() {
                shaped.insert(id, made.clone());
            }
            if let Some((source, display)) = &made.formula {
                drawing = cached(&mut old, &mut kept, key(1 + *display as u8, source), false, || {
                    figure::latex(source, *display, rgb(t.text), t.size * MATH_SCALE)
                });
            }
            // ponytail: une image par ligne, cherchée dans le dossier de la note, à la
            // racine du coffre, puis par son nom dans tout le coffre ; les images en
            // ligne (http) ne sont pas chargées.
            let file = made.picture.as_ref().and_then(|path| {
                self.dirs.iter().map(|d| d.join(path)).find(|p| p.is_file()).or_else(|| {
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
                let image = window
                    .use_asset::<ImgResourceLoader>(&Resource::Path(file.into()), cx)?
                    .ok()?;
                let natural = |d: gpui::DevicePixels| d.0 as f32 / zoom;
                let (w, h) = (natural(image.size(0).width), natural(image.size(0).height));
                let scale = (f32::from(width) / w.max(1.)).min(1.);
                Some((image, size(px(w * scale), px(h * scale))))
            });
            let image = image.or_else(|| {
                let (image, s) = drawing?;
                let image = image.use_render_image(window, cx)?;
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
        if let Some(last) = rows.last_mut().filter(|r| r.kind == Kind::Table) {
            last.gap = TABLE_GAP;
            last.height += TABLE_GAP;
            y += TABLE_GAP;
        }
        self.rows = rows;
        self.figures = kept;
        self.shaped = shaped;
        self.cells = cells;

        let reveal = std::mem::take(&mut self.reveal);
        if reveal {
            let c = self.cursor();
            if let Some(r) = self.row_at(c) {
                let row = &self.rows[r];
                let top = TOP + row.y + row.pad + row.pos(c - row.start).y;
                let margin = px(24.);
                if top - self.scroll_y < margin {
                    self.scroll_y = top - margin;
                } else if top + row.lh - self.scroll_y > bounds.size.height - margin {
                    self.scroll_y = top + row.lh - bounds.size.height + margin;
                }
            }
        }
        let max_scroll = (TOP + y - bounds.size.height * 0.4).max(px(0.));
        self.scroll_y = self.scroll_y.max(px(0.)).min(max_scroll);
        self.origin = point(
            bounds.left() + (bounds.size.width - width) / 2.,
            bounds.top() + TOP - self.scroll_y,
        );
        self.width = width;
        self.viewport = bounds;
        let opening = self.rows.iter().filter(|r| r.opens);
        self.copy_hitboxes = opening
            .map(|r| (r.start, window.insert_hitbox(self.copy_bounds(r), HitboxBehavior::Normal)))
            .collect();
        let grid = self.grid.and_then(|_| self.popup_at(Self::grid_size()));
        self.grid_hitbox = grid.map(|at| window.insert_hitbox(Bounds::new(at, Self::grid_size()), HitboxBehavior::Normal));
        // Cadre de chaque tableau : ses lignes, à la largeur de la plus longue sans
        // dépasser la page ; au-delà, il défile et suit le curseur.
        let scrolled = std::mem::take(&mut self.table_x);
        self.tables.clear();
        self.thumbs.clear();
        let mut i = 0;
        while i < self.rows.len() {
            let n = self.rows[i..].iter().take_while(|r| r.kind == Kind::Table).count();
            if n > 0 {
                let wide = self.rows[i..i + n].iter().map(Row::content_width).fold(px(0.), |a, b| a.max(b));
                let overflow = (wide - width).max(px(0.));
                let (start, end) = (self.rows[i].start, self.rows[i + n - 1].start + self.rows[i + n - 1].len);
                let mut dx = scrolled.get(&start).copied().unwrap_or_default().max(px(0.)).min(overflow);
                if reveal && (start..=end).contains(&cursor) {
                    let row = self.rows[i..i + n].iter().find(|r| r.start + r.len >= cursor).unwrap_or(&self.rows[i]);
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
                self.rows[i..i + n].iter_mut().for_each(|r| r.dx = dx);
                let (first, last) = (&self.rows[i], &self.rows[i + n - 1]);
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
            i += n.max(1);
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
                if !sel.is_empty() && sel.start <= end && sel.end > row.start {
                    let a = row.pos(sel.start.max(row.start) - row.start);
                    let b = row.pos(sel.end.min(end) - row.start);
                    let tail = if sel.end > end { px(6.) } else { px(0.) };
                    let mut rect = |x: Pixels, y: Pixels, w: Pixels, h: Pixels| {
                        window.paint_quad(fill(block(x, text_top + y, w, h), t.selection))
                    };
                    if let Some(grid) = &row.grid {
                        for (x, y, w) in grid.spans(sel.start.max(row.start) - row.start, sel.end.min(end) - row.start, row.lh) {
                            rect(x - row.dx, y, w, row.lh);
                        }
                    } else if a.y == b.y {
                        rect(a.x, a.y, b.x - a.x + tail, row.lh);
                    } else {
                        rect(a.x, a.y, w - a.x, row.lh);
                        rect(px(0.), a.y + row.lh, w, b.y - a.y - row.lh);
                        rect(px(0.), b.y, b.x + tail, row.lh);
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
        macro_rules! motions {
            ($el:expr, $($action:ident => $motion:ident, $select:expr;)*) => {
                $el$(.on_action(cx.listener(|this, _: &$action, _, cx| {
                    this.go(Motion::$motion, $select, cx)
                })))*
            };
        }
        let el = div()
            .size_full()
            .key_context("Editor")
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
        .on_action(cx.listener(|this, _: &Backspace, _, cx| this.delete(Motion::Left, cx)))
        .on_action(cx.listener(|this, _: &Delete, _, cx| this.delete(Motion::Right, cx)))
        .on_action(cx.listener(|this, _: &DeleteWordLeft, _, cx| this.delete(Motion::WordLeft, cx)))
        .on_action(cx.listener(|this, _: &DeleteWordRight, _, cx| this.delete(Motion::WordRight, cx)))
        .on_action(cx.listener(|this, _: &SelectAll, _, cx| {
            this.move_to(0, cx);
            this.select_to(this.content.len(), cx);
        }))
        .on_action(cx.listener(|this, _: &Newline, _, cx| this.newline(cx)))
        .on_action(cx.listener(|this, _: &Indent, _, cx| this.shift_lines(true, cx)))
        .on_action(cx.listener(|this, _: &Outdent, _, cx| this.shift_lines(false, cx)))
        .on_action(cx.listener(|this, _: &ToggleTask, _, cx| this.toggle_task(cx)))
        .on_action(cx.listener(|this, _: &AlignLeft, _, cx| this.align(":--", cx)))
        .on_action(cx.listener(|this, _: &AlignCenter, _, cx| this.align(":-:", cx)))
        .on_action(cx.listener(|this, _: &AlignRight, _, cx| this.align("--:", cx)))
        .on_action(cx.listener(|this, _: &Bold, _, cx| this.wrap("**", cx)))
        .on_action(cx.listener(|this, _: &Italic, _, cx| this.wrap("*", cx)))
        .on_action(cx.listener(|this, _: &Copy, _, cx| this.copy(false, cx)))
        .on_action(cx.listener(|this, _: &Cut, _, cx| this.copy(true, cx)))
        .on_action(cx.listener(|this, _: &Paste, _, cx| this.paste(cx)))
        .on_action(cx.listener(|this, _: &Undo, _, cx| this.restore(false, cx)))
        .on_action(cx.listener(|this, _: &Redo, _, cx| this.restore(true, cx)))
        .on_action(cx.listener(|this, _: &Cancel, _, cx| {
            if this.grid.take().is_some() {
                cx.notify();
            } else if let Some((start, _)) = this.completion() {
                this.ac_dismissed = Some(start);
                cx.notify();
            }
        }))
        .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
        .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
        .on_mouse_up_out(MouseButton::Left, cx.listener(Self::mouse_up))
        .on_mouse_move(cx.listener(Self::mouse_move))
        .on_scroll_wheel(cx.listener(Self::scroll))
        .child(EditorElement(cx.entity()))
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
