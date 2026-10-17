//! Éditeur Markdown stylé en direct : le texte reste du Markdown brut, seul le
//! rendu change (tailles, graisses, couleurs).


mod blocks;
mod clipboard;
mod completion;
mod cursor;
mod edit;
mod find;
mod format_bar;
mod input;
mod layout;
mod links;
mod mouse;
mod paint;
mod render;
mod style;
mod table;
pub mod grid;

use grid::TableRow;

pub use blocks::*;
use completion::*;
use cursor::*;
use edit::*;
use find::*;
pub use format_bar::*;
pub use layout::*;
use mouse::*;
use paint::*;
pub use table::*;

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
    ui::theme::Wide,
    ui::theme::Theme, logic::block, logic::diagram::Diagram, ui::figure, views::graph, ui::svg, ui::assets::mono, ui::assets::sans, lang::tr, platform::vault,
    logic::markdown::{self as md, Enter, Kind, Link, ListStyle},
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
        BlockText,
        BlockHeading1,
        BlockHeading2,
        BlockHeading3,
        BlockBullet,
        BlockOrdered,
        BlockTask,
        BlockQuote,
        BlockCode,
        BlockDuplicate,
        BlockUp,
        BlockDown,
        BlockDelete,
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

const TOP: Pixels = px(20.);
pub const MAX_WIDTH: Pixels = px(720.);
/// Marge gardée de chaque côté de la page, qu'elle soit étroite ou pleine.
const SIDE: Pixels = px(24.);

/// Côté du bouton de copie, à droite de l'ouverture d'un bloc de code.
const COPY_SIZE: Pixels = px(22.);
/// Corps des formules, par rapport au texte : la police mathématique paraît petite à taille égale.
const MATH_SCALE: f32 = 1.2;
/// Espace entre une ligne et l'image qu'elle désigne.
const IMAGE_GAP: Pixels = px(8.);
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

pub enum EditorEvent {
    Changed,
    Open(Link),
    /// Le même, avec Maj : à ouvrir dans l'autre pane.
    OpenAside(Link),
    /// Remplacer dans tout le coffre ce que la barre de recherche cherche.
    Swap(md::Swap),
    /// Clic droit dans la note : où, le lien qui s'y trouve, et s'il vise un commentaire.
    Menu(Point<Pixels>, Option<Link>, bool),
    /// Clic sur la poignée d'un bloc : son menu, à cet endroit.
    BlockMenu(Point<Pixels>),
    /// Un diagramme Mermaid à voir en grand : son source.
    View(String),
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
    /// Ligne dont le bouton « en grand » est survolé.
    view_hover: Option<usize>,
    /// Zones des boutons de copie, par début de ligne : elles portent le curseur en main.
    copy_hitboxes: Vec<(usize, Hitbox)>,
    view_hitboxes: Vec<(usize, Hitbox)>,
    /// Diagrammes et formules de la dernière frame, par empreinte de leur source.
    figures: Figures,
    /// Figures en cours de dessin.
    waiting: Vec<Job>,
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
    /// Poignées de bloc : les blocs de la note pour une version du texte, le bloc sous le pointeur
    /// (à sa version), celui qu'on tire, celui que vise le menu de la poignée, et les zones de la
    /// poignée et du « + ».
    blocks: Option<(u64, Rc<Vec<Range<usize>>>)>,
    handles: BlockHandles,
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
    /// The formatting bar: what it offers, where it sits, folded or open.
    bar: BarState,
    /// La note est celle du pane actif : une seule barre de mise en forme pour toute la fenêtre.
    pub bar_here: bool,
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
            view_hover: None,
            copy_hitboxes: Vec::new(),
            view_hitboxes: Vec::new(),
            figures: FastMap::default(),
            waiting: Vec::new(),
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
            blocks: None,
            handles: BlockHandles::default(),
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
            bar: BarState::default(),
            bar_here: true,
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

    /// Centre du bouton « en grand » du bloc Mermaid qui commence par `prefix`.
    #[cfg(test)]
    pub fn view_button(&self, prefix: &str) -> Option<Point<Pixels>> {
        let row = self.rows.iter().find(|r| self.views(r) && self.content[r.start..].starts_with(prefix))?;
        Some(self.view_bounds(row).center())
    }

    /// Figures gardées, et figures en cours de dessin.
    #[cfg(test)]
    pub fn figure_count(&self) -> (usize, usize) {
        (self.figures.len(), self.waiting.len())
    }

    /// Centre des boutons « + » (ligne, colonne) du premier tableau affiché.
    #[cfg(test)]
    pub fn plus_buttons(&self) -> Option<[Point<Pixels>; 2]> {
        Some(plus_bars(self.tables.first()?.1).map(|bar| bar.center()))
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

    fn fences_before(&self, at: usize) -> usize {
        self.content[..at].lines().filter(|l| md::is_fence(l)).count()
    }

    /// Un ``` sans clôture n'ouvre pas de bloc, comme dans `layout`.
    fn in_code(&self, line_start: usize) -> bool {
        self.fences_before(line_start) % 2 == 1
            && self.content[line_start..].lines().any(md::is_fence)
    }

    // ----- Recherche dans la note -----

    // ----- Plusieurs curseurs -----

    // ----- Déplacements -----

    // ----- Édition Markdown -----

    // ----- Tableaux -----

    // ----- Poignées de bloc -----

    // ----- Complétion : wikiliens et commandes `/` -----

    // ----- Souris -----

    // ----- Presse-papiers -----

    // ----- UTF-16 (méthodes de saisie) -----

    // ----- Mise en page et rendu -----

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

}
