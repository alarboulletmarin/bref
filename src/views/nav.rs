//! Navigation : un rail d'icônes toujours visible, et un panneau (arbre du
//! coffre, notes récentes, graphe ou tags) qui partage la fenêtre avec la note.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    hash::Hash,
    path::{Path, PathBuf},
};

use gpui::{
    Context, FocusHandle, Entity, Pixels, Point, UniformListScrollHandle, Window,
    actions, div, prelude::*, px,
};

use crate::{
    ui::theme::Theme, views::editor, views::graph,
    views::kanban::Field,
    logic::markdown::{Date, Link},
    lang::tr,
    platform::vault,
    logic::note::Note,
};

actions!(nav, [ShowCalendar, PrevMonth, NextMonth, ShowTree, ShowRecent, ShowGraph, ShowTags, ShowLinks, ToggleFull, Prev, Next, Fold, Unfold, Open, Close, NewFolder, Rename, Duplicate, Trash]);

pub const RAIL: Pixels = px(40.);
pub const ROW: Pixels = px(26.);
pub const LIST_W: Pixels = px(260.);
/// Largeur que le panneau et la colonne des commentaires laissent toujours à la note.
pub const NOTE_MIN: Pixels = px(200.);
/// Poignée entre le panneau et la note.
pub const DIVIDER: Pixels = px(5.);

#[derive(Clone, Copy, PartialEq, Hash, Debug)]
pub enum Mode {
    Tree,
    Recent,
    Graph,
    Tags,
    /// Rétroliens : les notes qui mènent à la note ouverte.
    Links,
    /// Calendrier : un mois, où se voient les jours qui ont leur note.
    Calendar,
}

/// Place du panneau : replié sur son rail, à côté de la note, ou seul.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Panel {
    Rail,
    Split,
    Full,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Row {
    pub path: PathBuf,
    pub name: String,
    pub depth: usize,
    /// Dossier, déplié ou non ; `None` pour une note.
    pub dir: Option<bool>,
}

/// Menu contextuel : où il s'ouvre et la ligne visée (`None` : le coffre lui-même).
pub struct Menu {
    pub at: Point<Pixels>,
    pub target: Option<PathBuf>,
    /// Clic droit dans la note et non dans l'arbre : le lien visé, s'il y en a un.
    pub text: Option<Option<Link>>,
    /// Ce clic droit dans la note vise un commentaire.
    pub commented: bool,
    /// Ouvert par la poignée d'un bloc : les gestes du bloc.
    pub block: bool,
}

#[derive(Clone, Copy)]
pub enum Do {
    Open,
    OpenAside,
    NewNote,
    NewDiagram,
    NewFolder,
    /// Une note née d'un modèle, dans le dossier visé.
    FromTemplate,
    Rename,
    Duplicate,
    Trash,
    CopyLink,
    CopyPath,
    CopyRelative,
    Reveal,
    /// Ouvre la grille des icônes pour la ligne visée.
    Icon,
    /// Montre le dossier visé en page liste.
    List,
    // Menu de la note.
    Cut,
    Copy,
    Paste,
    Bold,
    Italic,
    Underline,
    Strike,
    Highlight,
    Code,
    Link,
    Comment,
    /// Les lignes choisies en liste à puces, numérotée, de tâches, ou en texte.
    Bullets,
    Numbered,
    Tasks,
    Plain,
    /// Un geste sur le tableau du curseur.
    Table(editor::TableOp),
    /// Un geste sur le bloc de la poignée.
    Block(editor::BlockOp),
    OpenLink,
    CopyAddress,
}

impl Do {
    /// Le raccourci qui fait la même chose que cette entrée de menu, tel qu'il est en vigueur ;
    /// rien quand l'action n'en a pas.
    pub fn keys(self, cx: &gpui::App) -> String {
        use crate::{views::editor, ui::keys::of};
        match self {
            Do::Open => of(cx, &Open),
            Do::NewNote => of(cx, &crate::actions::NewNote),
            Do::NewDiagram => of(cx, &crate::actions::NewDiagram),
            Do::NewFolder => of(cx, &NewFolder),
            Do::FromTemplate => of(cx, &crate::actions::NewFromTemplate),
            Do::Rename => of(cx, &Rename),
            Do::Duplicate => of(cx, &Duplicate),
            Do::Trash => of(cx, &Trash),
            Do::Cut => of(cx, &editor::Cut),
            Do::Copy => of(cx, &editor::Copy),
            Do::Paste => of(cx, &editor::Paste),
            Do::Bold => of(cx, &editor::Bold),
            Do::Italic => of(cx, &editor::Italic),
            Do::Underline => of(cx, &editor::Underline),
            Do::Strike => of(cx, &editor::Strike),
            Do::Highlight => of(cx, &editor::Highlight),
            Do::Code => of(cx, &editor::InlineCode),
            Do::Link => of(cx, &editor::InsertLink),
            Do::Comment => of(cx, &editor::Comment),
            Do::Bullets => of(cx, &editor::ListBullet),
            Do::Numbered => of(cx, &editor::ListOrdered),
            Do::Tasks => of(cx, &editor::ListTask),
            Do::Plain => of(cx, &editor::ListText),
            Do::Table(op) => of(cx, op.action().as_ref()),
            Do::Block(op) => of(cx, op.action().as_ref()),
            // Pas une liaison, mais le geste qui fait la même chose.
            Do::OpenLink => format!("{}+{}", crate::ui::keys::MOD, tr("click", "clic")),
            _ => String::new(),
        }
    }
}

/// Ligne de l'arbre en cours de glisser-déposer ; dessinée sous le pointeur.
#[derive(Clone)]
pub struct Dragged {
    /// La ligne saisie, ou toute la sélection multiple si elle en fait partie.
    pub paths: Vec<PathBuf>,
    pub name: String,
    pub theme: Theme,
}

impl Render for Dragged {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        div()
            .px_2()
            .py_1()
            .rounded(px(5.))
            .bg(t.panel)
            .border_1()
            .border_color(t.border)
            .text_size(px(13.))
            .text_color(t.text)
            .child(self.name.clone())
    }
}

pub struct Nav {
    pub focus: FocusHandle,
    pub mode: Mode,
    pub panel: Panel,
    pub list_w: Pixels,
    /// Largeur du graphe ; zéro : la moitié de la fenêtre.
    pub graph_w: Pixels,
    pub dragging: bool,
    /// Ligne sélectionnée (note ou dossier).
    pub sel: Option<PathBuf>,
    /// Sélection multiple (Ctrl+clic, Maj+clic) ; vide tant qu'une seule ligne
    /// est choisie. `sel` est alors la ligne d'où Maj+clic étend.
    pub marked: HashSet<PathBuf>,
    /// Note dont le panneau montre les rétroliens : la dernière ouverte, pas celle qu'on
    /// parcourt en aperçu, sinon la liste changerait sous la sélection.
    pub anchor: Option<PathBuf>,
    /// Jour choisi dans le calendrier, qui montre son mois.
    pub day: Date,
    /// Dossiers dépliés.
    pub open: HashSet<PathBuf>,
    /// Lignes de la dernière frame, et l'empreinte de ce dont elles sont tirées.
    pub rows: Vec<Row>,
    pub rows_from: u64,
    pub scroll: UniformListScrollHandle,
    /// Amener la sélection à l'écran à la prochaine frame.
    pub reveal: bool,
    /// Le système dessine la barre de titre (macOS, Windows) : le rail n'a pas à servir de
    /// poignée pour déplacer la fenêtre.
    pub native_bar: bool,
    // Bord gauche et largeur du contenu de la fenêtre, à la dernière frame.
    pub left: Pixels,
    pub total: Pixels,
    /// Largeur réservée à droite de la note (la colonne des commentaires) : le panneau la lui laisse.
    pub aside: Pixels,
}

impl Nav {
    /// Reprend le panneau tel qu'il était à la fermeture de l'app.
    pub fn new(focus: FocusHandle) -> Self {
        let saved = vault::load_layout();
        let mut words = saved.split_whitespace();
        Self {
            focus,
            mode: match words.next() {
                Some("recent") => Mode::Recent,
                Some("graph") => Mode::Graph,
                Some("tags") => Mode::Tags,
                Some("links") => Mode::Links,
                Some("calendar") => Mode::Calendar,
                _ => Mode::Tree,
            },
            panel: match words.next() {
                Some("split") => Panel::Split,
                Some("full") => Panel::Full,
                _ => Panel::Rail,
            },
            list_w: words.next().and_then(|w| w.parse().ok()).map_or(LIST_W, px),
            graph_w: words.next().and_then(|w| w.parse().ok()).map_or(px(0.), px),
            dragging: false,
            sel: None,
            marked: HashSet::new(),
            anchor: None,
            day: crate::platform::clock::today(),
            open: HashSet::new(),
            rows: Vec::new(),
            rows_from: 0,
            scroll: UniformListScrollHandle::new(),
            reveal: false,
            native_bar: false,
            left: px(0.),
            total: px(0.),
            aside: px(0.),
        }
    }

    pub fn save(&self) {
        let mode = match self.mode {
            Mode::Tree => "tree",
            Mode::Recent => "recent",
            Mode::Graph => "graph",
            Mode::Tags => "tags",
            Mode::Links => "links",
            Mode::Calendar => "calendar",
        };
        let panel = match self.panel {
            Panel::Rail => "rail",
            Panel::Split => "split",
            Panel::Full => "full",
        };
        let (list, graph) = (f32::from(self.list_w), f32::from(self.graph_w));
        vault::save_layout(&format!("{mode} {panel} {list} {graph}\n"));
    }

    /// Sélectionne la note et déplie les dossiers qui y mènent.
    pub fn reveal(&mut self, path: &Path) {
        self.sel = Some(path.to_path_buf());
        self.open.extend(path.ancestors().skip(1).map(Path::to_path_buf));
        self.reveal = true;
    }

    /// Largeur du panneau à côté de la note, dans le mode affiché.
    pub fn width(&mut self) -> &mut Pixels {
        match self.mode {
            Mode::Graph => &mut self.graph_w,
            _ => &mut self.list_w,
        }
    }

    /// Dossier où créer une note depuis l'arbre : celui de la sélection.
    pub fn target_dir(&self) -> Option<PathBuf> {
        if self.panel == Panel::Rail || self.mode != Mode::Tree {
            return None;
        }
        let sel = self.sel.as_ref()?;
        if sel.extension().is_some_and(|e| e == "md") {
            sel.parent().map(Path::to_path_buf)
        } else {
            Some(sel.clone())
        }
    }
}

/// Lignes visibles de l'arbre : dossiers d'abord, puis notes, par ordre
/// alphabétique ; le contenu des dossiers repliés est omis. `dirs` apporte les
/// dossiers sans note.
pub fn tree_rows(
    root: &Path,
    notes: &[Note],
    dirs: &[PathBuf],
    images: &[PathBuf],
    open: &HashSet<PathBuf>,
) -> Vec<Row> {
    let mut all: HashSet<&Path> = dirs.iter().map(PathBuf::as_path).collect();
    // Par dossier : ses notes puis ses images, chacune avec le nom affiché.
    let mut files: HashMap<&Path, Vec<(&Path, String)>> = HashMap::new();
    let named = notes.iter().map(|n| (n.path.as_path(), n.name.clone()));
    let pictures = images.iter().map(|p| (p.as_path(), graph::image_name(p)));
    for (path, name) in named.chain(pictures) {
        let parents = path.ancestors().skip(1).take_while(|a| *a != root && a.starts_with(root));
        all.extend(parents);
        files.entry(path.parent().unwrap_or(root)).or_default().push((path, name));
    }
    let mut folders: HashMap<&Path, Vec<&Path>> = HashMap::new();
    for dir in all {
        folders.entry(dir.parent().unwrap_or(root)).or_default().push(dir);
    }
    let label = |dir: &Path| dir.file_name().unwrap_or_default().to_string_lossy().into_owned();
    folders.values_mut().for_each(|f| f.sort_by_cached_key(|d| vault::natural(&label(d))));
    files
        .values_mut()
        .for_each(|f| f.sort_by_cached_key(|(path, name)| (vault::is_file_shown(path), vault::natural(name))));

    let mut rows = Vec::new();
    // Parcours en profondeur : (dossier, profondeur de son contenu), sous-dossiers d'abord.
    fn walk(
        dir: &Path,
        depth: usize,
        folders: &HashMap<&Path, Vec<&Path>>,
        files: &HashMap<&Path, Vec<(&Path, String)>>,
        open: &HashSet<PathBuf>,
        rows: &mut Vec<Row>,
    ) {
        for sub in folders.get(dir).into_iter().flatten() {
            let unfolded = open.contains(*sub);
            rows.push(Row {
                path: sub.to_path_buf(),
                name: sub.file_name().unwrap_or_default().to_string_lossy().into_owned(),
                depth,
                dir: Some(unfolded),
            });
            if unfolded {
                walk(sub, depth + 1, folders, files, open, rows);
            }
        }
        for (path, name) in files.get(dir).into_iter().flatten() {
            rows.push(Row {
                path: path.to_path_buf(),
                name: name.clone(),
                depth,
                dir: None,
            });
        }
    }
    walk(root, 0, &folders, &files, open, &mut rows);
    rows
}

/// Ligne d'un tag dans le panneau : un faux chemin, qu'aucun fichier ne porte.
fn tag_path(tag: &str) -> PathBuf {
    PathBuf::from(format!("#{tag}"))
}

/// Le tag que désigne ce chemin (`#tag`), si c'en est un.
pub fn tag_of(path: &Path) -> Option<&str> {
    path.to_str()?.strip_prefix('#')
}

/// Page liste affichée à la place de la note.
pub struct Listing {
    /// Le dossier, ou le tag (`#tag`), dont on liste les notes.
    pub of: PathBuf,
    /// La colonne qui trie (0 : le nom), et si c'est à rebours.
    pub by: usize,
    pub back: bool,
    /// Ce qu'on tape garde les lignes qui le contiennent.
    pub filter: Entity<Field>,
    /// Cellule qu'on réécrit : la note, la clé, son champ.
    pub edit: Option<(PathBuf, String, Entity<Field>)>,
}

/// Les tags du coffre par ordre alphabétique ; sous un tag déplié, ses notes.
// ponytail: une note qui porte plusieurs tags dépliés apparaît sous chacun, et la
// sélection, tenue par son chemin, les marque toutes ; donner une identité propre
// à chaque ligne si cela gêne.
pub fn tag_rows(notes: &[Note], open: &HashSet<PathBuf>) -> Vec<Row> {
    let mut tagged: BTreeMap<&str, Vec<&Note>> = BTreeMap::new();
    for note in notes {
        for tag in note.tags.iter() {
            tagged.entry(tag).or_default().push(note);
        }
    }
    let mut rows = Vec::new();
    for (tag, mut notes) in tagged {
        let path = tag_path(tag);
        let unfolded = open.contains(&path);
        rows.push(Row { path, name: format!("#{tag}"), depth: 0, dir: Some(unfolded) });
        if unfolded {
            notes.sort_by_cached_key(|n| vault::natural(&n.name));
            rows.extend(notes.iter().map(|n| Row { path: n.path.clone(), name: n.name.clone(), depth: 1, dir: None }));
        }
    }
    rows
}

/// L'entrée du menu qui montre un fichier dans le gestionnaire de fichiers, sous son nom de chaque système.
pub fn reveal_label() -> &'static str {
    if cfg!(target_os = "macos") {
        tr("Reveal in Finder", "Afficher dans le Finder")
    } else if cfg!(windows) {
        tr("Show in Explorer", "Afficher dans l'Explorateur")
    } else {
        tr("Show in the file manager", "Afficher dans le gestionnaire de fichiers")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    #[test]
    fn builds_tree_rows() {
        let root = Path::new("/v");
        let note = |rel: &str| Note {
            name: vault::stem(Path::new(rel)),
            path: root.join(rel),
            tags: Default::default(),
            links: Default::default(),
            aliases: Default::default(),
            mtime: SystemTime::UNIX_EPOCH,
            body: "".into(),
            lower: "".into(),
        };
        let notes = [note("b.md"), note("Z/x.md"), note("a/c/d.md"), note("a/B.md"), note("A.md")];
        let shown = |open: &[&str]| -> Vec<String> {
            let open = open.iter().map(|d| root.join(d)).collect();
            tree_rows(root, &notes, &[root.join("a/vide"), root.join("a")], &[root.join("a/Photo.png")], &open)
                .iter()
                .map(|r| format!("{}{}{}", "  ".repeat(r.depth), r.name, if r.dir.is_some() { "/" } else { "" }))
                .collect()
        };
        // Dossiers d'abord, sans tenir compte de la casse ; tout est replié.
        assert_eq!(shown(&[]), ["a/", "Z/", "A", "b"]);
        // Un dossier sans note apparaît aussi.
        // Les images d'un dossier suivent ses notes.
        assert_eq!(shown(&["a"]), ["a/", "  c/", "  vide/", "  B", "  Photo.png", "Z/", "A", "b"]);
        // Un dossier déplié dans un dossier replié reste caché.
        assert_eq!(shown(&["a/c", "Z"]), ["a/", "Z/", "  x", "A", "b"]);
        assert_eq!(shown(&["a", "a/c"]), ["a/", "  c/", "    d", "  vide/", "  B", "  Photo.png", "Z/", "A", "b"]);

        // Tags : par ordre alphabétique ; un tag déplié montre ses notes.
        let tagged = |rel: &str, tags: &[&str]| Note { tags: tags.iter().map(|t| t.to_string()).collect(), ..note(rel) };
        let notes = [tagged("b.md", &["zoo", "ami"]), tagged("A.md", &["zoo"]), note("sans.md")];
        let shown = |open: &[&str]| -> Vec<String> {
            let open = open.iter().map(|t| tag_path(t)).collect();
            tag_rows(&notes, &open).iter().map(|r| format!("{}{}", "  ".repeat(r.depth), r.name)).collect()
        };
        assert_eq!(shown(&[]), ["#ami", "#zoo"]);
        assert_eq!(shown(&["zoo"]), ["#ami", "#zoo", "  A", "  b"]);
    }
}
