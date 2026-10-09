//! Navigation : un rail d'icônes toujours visible, et un panneau (arbre du
//! coffre, notes récentes, graphe ou tags) qui partage la fenêtre avec la note.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    hash::{BuildHasher, BuildHasherDefault, DefaultHasher, Hash, Hasher},
    ops::Range,
    path::{Path, PathBuf},
    time::Duration,
};

use gpui::{
    Animation, AnimationExt, ClickEvent, ClipboardItem, Context, CursorStyle, Div, FocusHandle, Focusable, MouseButton,
    MouseDownEvent, Pixels, Point, ScrollStrategy, Stateful, UniformListScrollHandle, Window,
    actions, div, ease_out_quint, point, prelude::*, px, svg, uniform_list,
};

use crate::{
    Shell, Theme, graph,
    palette::{Palette, PaletteEvent, Setting},
    tr,
    vault::{self, Note},
};

actions!(nav, [ShowTree, ShowRecent, ShowGraph, ShowTags, ToggleFull, Prev, Next, Fold, Unfold, Open, Close, NewFolder, Rename, Duplicate, Trash]);

pub const RAIL: Pixels = px(40.);
const ROW: Pixels = px(26.);
const LIST_W: Pixels = px(260.);

#[derive(Clone, Copy, PartialEq, Hash, Debug)]
pub enum Mode {
    Tree,
    Recent,
    Graph,
    Tags,
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
    at: Point<Pixels>,
    target: Option<PathBuf>,
}

#[derive(Clone, Copy)]
pub enum Do {
    Open,
    NewNote,
    NewDiagram,
    NewFolder,
    Rename,
    Duplicate,
    Trash,
    CopyLink,
    CopyPath,
    CopyRelative,
    Reveal,
}

/// Ligne de l'arbre en cours de glisser-déposer ; dessinée sous le pointeur.
#[derive(Clone)]
struct Dragged {
    /// La ligne saisie, ou toute la sélection multiple si elle en fait partie.
    paths: Vec<PathBuf>,
    name: String,
    theme: Theme,
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
    list_w: Pixels,
    /// Largeur du graphe ; zéro : la moitié de la fenêtre.
    graph_w: Pixels,
    pub dragging: bool,
    /// Ligne sélectionnée (note ou dossier).
    pub sel: Option<PathBuf>,
    /// Sélection multiple (Ctrl+clic, Maj+clic) ; vide tant qu'une seule ligne
    /// est choisie. `sel` est alors la ligne d'où Maj+clic étend.
    pub marked: HashSet<PathBuf>,
    /// Dossiers dépliés.
    pub open: HashSet<PathBuf>,
    /// Lignes de la dernière frame, et l'empreinte de ce dont elles sont tirées.
    pub rows: Vec<Row>,
    rows_from: u64,
    scroll: UniformListScrollHandle,
    /// Amener la sélection à l'écran à la prochaine frame.
    reveal: bool,
    /// Le logo se place en tête du rail quand l'app ne dessine pas la barre de
    /// titre (macOS, Windows).
    pub logo: bool,
    // Bord gauche et largeur du contenu de la fenêtre, à la dernière frame.
    pub left: Pixels,
    pub total: Pixels,
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
            open: HashSet::new(),
            rows: Vec::new(),
            rows_from: 0,
            scroll: UniformListScrollHandle::new(),
            reveal: false,
            logo: false,
            left: px(0.),
            total: px(0.),
        }
    }

    fn save(&self) {
        let mode = match self.mode {
            Mode::Tree => "tree",
            Mode::Recent => "recent",
            Mode::Graph => "graph",
            Mode::Tags => "tags",
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
    fn width(&mut self) -> &mut Pixels {
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
    folders.values_mut().for_each(|f| f.sort_by_cached_key(|d| label(d).to_lowercase()));
    files
        .values_mut()
        .for_each(|f| f.sort_by_cached_key(|(path, name)| (vault::is_image(path), name.to_lowercase())));

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

/// Les tags du coffre par ordre alphabétique ; sous un tag déplié, ses notes.
// ponytail: une note qui porte plusieurs tags dépliés apparaît sous chacun, et la
// sélection, tenue par son chemin, les marque toutes ; donner une identité propre
// à chaque ligne si cela gêne.
pub fn tag_rows(notes: &[Note], open: &HashSet<PathBuf>) -> Vec<Row> {
    let mut tagged: BTreeMap<&str, Vec<&Note>> = BTreeMap::new();
    for note in notes {
        for tag in &note.tags {
            tagged.entry(tag).or_default().push(note);
        }
    }
    let mut rows = Vec::new();
    for (tag, mut notes) in tagged {
        let path = tag_path(tag);
        let unfolded = open.contains(&path);
        rows.push(Row { path, name: format!("#{tag}"), depth: 0, dir: Some(unfolded) });
        if unfolded {
            notes.sort_by_cached_key(|n| n.name.to_lowercase());
            rows.extend(notes.iter().map(|n| Row { path: n.path.clone(), name: n.name.clone(), depth: 1, dir: None }));
        }
    }
    rows
}

/// Bouton icône du rail et des en-têtes de panneau.
pub fn button(id: &'static str, icon: &'static str, active: bool, t: Theme) -> Stateful<Div> {
    div()
        .id(id)
        .size(px(28.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.))
        .cursor_pointer()
        .hover(|s| s.bg(t.border))
        .active(|s| s.bg(t.selection))
        .when(active, |d| d.bg(t.selection))
        .child(
            svg()
                .path(icon)
                .size(px(16.))
                .flex_none()
                .text_color(if active { t.accent } else { t.dim }),
        )
}

impl Shell {
    /// Affiche le panneau dans ce mode et lui donne le focus. Redemander le mode
    /// affiché replie le panneau (au clavier : seulement s'il a déjà le focus).
    pub fn show_nav(&mut self, mode: Mode, toggle: bool, window: &mut Window, cx: &mut Context<Self>) {
        let shown = self.nav.panel != Panel::Rail && self.nav.mode == mode;
        if shown && (toggle || self.nav.focus.contains_focused(window, cx)) {
            self.nav.panel = Panel::Rail;
        } else {
            self.nav.mode = mode;
            if self.nav.panel == Panel::Rail {
                self.nav.panel = Panel::Split;
            }
            self.nav.reveal = true;
            window.focus(&self.nav.focus);
            self.refresh_graph(cx);
        }
        self.settle_nav(window, cx);
    }

    /// Met le graphe à jour s'il est affiché et que les notes ou leurs liens ont changé.
    pub fn refresh_graph(&mut self, cx: &mut Context<Self>) {
        if self.graph_stale && self.nav.panel != Panel::Rail && self.nav.mode == Mode::Graph {
            self.graph_stale = false;
            self.graph.update(cx, |graph, cx| graph.set_notes(&self.notes, &vault::pictures(&self.images), cx));
        }
    }

    /// Un nœud du graphe est sélectionné : aperçu de la note si elle a la place.
    pub fn select_from_graph(&mut self, path: &Path, cx: &mut Context<Self>) {
        if self.nav.panel == Panel::Split {
            self.preview_note(path, cx);
        }
        self.nav.sel = Some(path.to_path_buf());
        cx.notify();
    }

    pub fn toggle_full(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.nav.panel = match self.nav.panel {
            Panel::Full => Panel::Split,
            _ => Panel::Full,
        };
        self.settle_nav(window, cx);
    }

    /// Après un changement de panneau : le focus quitte ce qui n'est plus
    /// affiché, et la disposition est mémorisée.
    pub fn settle_nav(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.nav.dragging = false;
        let editor = self.editor.focus_handle(cx);
        match self.nav.panel {
            Panel::Rail if self.nav.focus.contains_focused(window, cx) => window.focus(&editor),
            Panel::Full if editor.is_focused(window) => window.focus(&self.nav.focus),
            _ => {}
        }
        self.nav.save();
        cx.notify();
    }

    /// Le séparateur suit le pointeur ; aux extrémités, un côté se replie.
    pub fn drag_nav(&mut self, x: Pixels, cx: &mut Context<Self>) {
        let x = x - self.nav.left - RAIL;
        let room = self.nav.total - RAIL;
        self.nav.panel = if x < px(100.) {
            Panel::Rail
        } else if x > room - px(140.) {
            Panel::Full
        } else {
            *self.nav.width() = x;
            Panel::Split
        };
        cx.notify();
    }

    /// Empreinte de tout ce dont les lignes du panneau dépendent : les recalculer
    /// coûte dix fois plus cher que de vérifier qu'elles sont encore justes.
    fn rows_source(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        let open = self.nav.open.iter().map(|dir| BuildHasherDefault::<DefaultHasher>::default().hash_one(dir)).fold(0u64, u64::wrapping_add);
        (self.nav.mode, self.nav.panel == Panel::Rail, &self.vault, &self.dirs, &self.images, open).hash(&mut hasher);
        if self.nav.mode == Mode::Recent {
            self.recent.hash(&mut hasher);
        }
        for note in &self.notes {
            (&note.path, &note.name).hash(&mut hasher);
            if self.nav.mode == Mode::Tags {
                note.tags.hash(&mut hasher);
            }
        }
        hasher.finish()
    }

    fn nav_rows(&self) -> Vec<Row> {
        let Some(root) = &self.vault else {
            return Vec::new();
        };
        match self.nav.mode {
            Mode::Tree => tree_rows(root, &self.notes, &self.dirs, &self.images, &self.nav.open),
            Mode::Graph => Vec::new(),
            Mode::Tags => tag_rows(&self.notes, &self.nav.open),
            Mode::Recent => self
                .by_recency()
                .into_iter()
                .map(|n| Row {
                    path: n.path.clone(),
                    name: n.name.clone(),
                    depth: 0,
                    dir: None,
                })
                .collect(),
        }
    }

    fn selected_row(&self) -> Option<usize> {
        let sel = self.nav.sel.as_ref()?;
        self.nav.rows.iter().position(|r| r.path == *sel)
    }

    /// Sélectionne la ligne ; une note s'affiche en aperçu si elle a la place.
    fn nav_select(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(row) = self.nav.rows.get(ix) else {
            return;
        };
        let (path, is_note) = (row.path.clone(), row.dir.is_none());
        self.nav.marked.clear();
        self.nav.scroll.scroll_to_item(ix, ScrollStrategy::Center);
        if is_note && self.nav.panel == Panel::Split {
            self.preview_note(&path, cx);
        }
        self.nav.sel = Some(path);
        self.nav.reveal = false;
        cx.notify();
    }

    /// Ctrl+clic ajoute la ligne à la sélection ou l'en retire ; Maj+clic
    /// sélectionne tout depuis la dernière ligne choisie.
    pub fn nav_mark(&mut self, ix: usize, extend: bool, cx: &mut Context<Self>) {
        let Some(path) = self.nav.rows.get(ix).map(|r| r.path.clone()) else {
            return;
        };
        let marked = &mut self.nav.marked;
        if extend {
            let from = self.nav.sel.as_ref().and_then(|sel| self.nav.rows.iter().position(|r| r.path == *sel)).unwrap_or(ix);
            *marked = self.nav.rows[from.min(ix)..=from.max(ix)].iter().map(|r| r.path.clone()).collect();
            self.nav.sel.get_or_insert(path);
        } else {
            // La ligne déjà sélectionnée fait partie du lot.
            if marked.is_empty() {
                marked.extend(self.nav.sel.take());
            }
            if !marked.remove(&path) {
                marked.insert(path.clone());
            }
            // Retirée, elle laisse sa place de ligne choisie à une autre du lot.
            let other = self.nav.rows.iter().map(|r| &r.path).find(|p| marked.contains(*p)).cloned();
            self.nav.sel = if marked.contains(&path) { Some(path) } else { other.or(Some(path)) };
        }
        cx.notify();
    }

    /// Ce qu'une action sur `target` vise : toute la sélection multiple s'il en
    /// fait partie, dans l'ordre du panneau, sans ce qu'un dossier visé contient déjà.
    fn targets(&self, target: &Path) -> Vec<PathBuf> {
        let marked = &self.nav.marked;
        if !marked.contains(target) {
            return vec![target.to_path_buf()];
        }
        let mut seen = HashSet::new();
        let rows = self.nav.rows.iter().map(|r| &r.path);
        rows.filter(|p| marked.contains(*p) && !p.ancestors().skip(1).any(|a| marked.contains(a)) && seen.insert(*p))
            .cloned()
            .collect()
    }

    fn nav_step(&mut self, down: bool, cx: &mut Context<Self>) {
        let last = self.nav.rows.len().saturating_sub(1);
        let ix = match self.selected_row() {
            Some(ix) if down => (ix + 1).min(last),
            Some(ix) => ix.saturating_sub(1),
            None => 0,
        };
        self.nav_select(ix, cx);
    }

    /// Entrée ou double-clic : ouvre la note pour de bon, ou bascule le dossier.
    fn nav_activate(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.nav.rows.get(ix) else {
            return;
        };
        let path = row.path.clone();
        self.nav.marked.clear();
        if row.dir.is_some() {
            if !self.nav.open.remove(&path) {
                self.nav.open.insert(path.clone());
            }
            self.nav.sel = Some(path);
            cx.notify();
        } else {
            self.open_from_nav(&path, window, cx);
        }
    }

    /// Ouvre la note et y place le focus ; la note reprend sa place si le
    /// panneau occupait toute la fenêtre.
    pub fn open_from_nav(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        self.open_note(path, cx);
        if self.nav.panel == Panel::Full {
            self.nav.panel = Panel::Split;
        }
        // Un tableau prend le focus pour qu'on y écrive ; une image s'affiche à la place
        // de la note, et le panneau garde la main.
        match self.shown_sheet() {
            Some(sheet) => window.focus(&sheet.focus_handle(cx)),
            None if self.picture.is_none() => window.focus(&self.editor.focus_handle(cx)),
            None => {}
        }
        self.settle_nav(window, cx);
    }

    /// Gauche/droite dans l'arbre : replier ou déplier, sinon remonter ou descendre.
    fn nav_fold(&mut self, unfold: bool, cx: &mut Context<Self>) {
        let Some(ix) = self.selected_row() else {
            return self.nav_step(true, cx);
        };
        let row = self.nav.rows[ix].clone();
        match (row.dir, unfold) {
            (Some(false), true) => {
                self.nav.open.insert(row.path);
            }
            (Some(true), false) => {
                self.nav.open.remove(&row.path);
            }
            (_, true) => return self.nav_step(true, cx),
            (_, false) => {
                let parent = row.path.parent();
                if let Some(up) = self.nav.rows.iter().position(|r| Some(r.path.as_path()) == parent) {
                    return self.nav_select(up, cx);
                }
            }
        }
        cx.notify();
    }

    /// Ctrl+Maj+N : nouveau dossier dans l'arbre, à côté de la sélection.
    pub fn new_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.vault.is_none() {
            return;
        }
        if self.nav.panel == Panel::Rail || self.nav.mode != Mode::Tree {
            self.show_nav(Mode::Tree, false, window, cx);
        }
        self.menu_do(Do::NewFolder, self.nav.sel.clone(), window, cx);
    }

    /// Exécute une entrée du menu contextuel sur `target` (`None` : le coffre).
    pub fn menu_do(&mut self, what: Do, target: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        self.menu = None;
        cx.notify();
        let Some(root) = self.vault.clone() else {
            return;
        };
        // Un tag n'est ni une note ni un dossier : rien à renommer ni à jeter.
        if target.as_ref().is_some_and(|t| !t.starts_with(&root)) {
            return;
        }
        let is_dir = target.as_ref().is_none_or(|t| self.dirs.contains(t));
        // Dossier visé : la cible elle-même, ou celui qui contient la note.
        let dir = match &target {
            Some(t) if is_dir => t.clone(),
            Some(t) => t.parent().map_or(root.clone(), Path::to_path_buf),
            None => root.clone(),
        };
        // Corbeille, copies et duplication portent sur toute la sélection multiple.
        let all = target.as_deref().map_or_else(|| vec![root.clone()], |t| self.targets(t));
        let copy = |lines: Vec<String>, cx: &mut Context<Self>| {
            cx.write_to_clipboard(ClipboardItem::new_string(lines.join("\n")));
        };
        match (what, target) {
            (Do::Open, Some(path)) => self.open_from_nav(&path, window, cx),
            (Do::NewNote, _) => {
                self.nav.open.insert(dir.clone());
                self.nav.sel = Some(dir);
                self.new_note_here(window, cx);
            }
            (Do::NewDiagram, _) => {
                self.nav.open.insert(dir.clone());
                self.nav.sel = Some(dir);
                self.new_diagram(window, cx);
            }
            (Do::NewFolder, _) => {
                // Le dossier est créé dans celui de la sélection : le champ le dit.
                let label = match dir.file_name().filter(|_| dir != root) {
                    Some(name) => format!("{} « {} »", tr("Folder name, in", "Nom du dossier, dans"), name.to_string_lossy()),
                    None => tr("Folder name", "Nom du dossier").to_string(),
                };
                self.ask(label, "", window, cx, move |this, name, _| {
                    let new = dir.join(name);
                    match fs::create_dir(&new) {
                        Ok(()) => {
                            this.nav.open.insert(dir.clone());
                            this.dirs.push(new.clone());
                            this.nav.reveal(&new);
                        }
                        Err(e) => this.fail(tr("Folder not created", "Dossier non créé"), e),
                    }
                });
            }
            (Do::Rename, Some(path)) => {
                let label = tr("Rename: new name", "Renommer : nouveau nom");
                let current = if is_dir { path.file_name().unwrap_or_default().to_string_lossy().into_owned() } else { vault::stem(&path) };
                self.ask(label, &current.clone(), window, cx, move |this, name, cx| {
                    if name == current {
                        return;
                    }
                    this.flush(cx);
                    let image = vault::is_image(&path);
                    let renamed = if is_dir {
                        let to = path.with_file_name(&name);
                        vault::rename(&path, &to).map(|()| to)
                    } else if image || vault::is_table(&path) {
                        // Une image ou un tableau garde son extension.
                        let extension = path.extension().unwrap_or_default().to_string_lossy();
                        let to = path.with_file_name(format!("{name}.{extension}"));
                        vault::rename(&path, &to).map(|()| to)
                    } else {
                        vault::rename_note(&path, &name)
                    };
                    match renamed {
                        Ok(to) => {
                            // Le titre d'une note renommée a pu changer : on la relit.
                            this.relocate(&path, &to, !is_dir, cx);
                            // Les liens suivent : par nom de note, ou par nom de fichier pour une image.
                            let (old, new) = if image {
                                (graph::image_name(&path), graph::image_name(&to))
                            } else {
                                (current.clone(), name)
                            };
                            if !is_dir && this.relink(&old, &new, cx) {
                                this.reload(cx);
                            }
                        }
                        Err(e) => this.fail(tr("Not renamed", "Renommage impossible"), e),
                    }
                });
            }
            (Do::Trash, Some(_)) => {
                all.iter().for_each(|path| self.trash(&root, path, cx));
                self.nav.marked.clear();
            }
            (Do::CopyLink, Some(_)) => {
                let files = all.iter().filter(|p| !self.dirs.contains(p));
                let link = |path: &PathBuf| match (vault::is_image(path), vault::is_table(path)) {
                    (true, _) => format!("![[{}]]", graph::image_name(path)),
                    // Un tableau n'a pas de lien : son nom de fichier.
                    (_, true) => graph::image_name(path),
                    _ => format!("[[{}]]", vault::stem(path)),
                };
                copy(files.map(link).collect(), cx);
            }
            (Do::CopyPath, _) => copy(all.iter().map(|p| p.display().to_string()).collect(), cx),
            (Do::CopyRelative, _) => {
                let relative = |p: &PathBuf| p.strip_prefix(&root).unwrap_or(p).display().to_string();
                copy(all.iter().map(relative).collect(), cx);
            }
            (Do::Reveal, _) => cx.reveal_path(&all[0]),
            // ponytail: seuls les fichiers se dupliquent ; copier un dossier entier
            // demande un parcours récursif, à écrire si le besoin se présente.
            (Do::Duplicate, Some(_)) => {
                // La copie part de ce qui est à l'écran, pas d'un fichier en retard.
                self.flush(cx);
                let mut last = None;
                for path in all.iter().filter(|p| p.is_file()) {
                    let extension = path.extension().unwrap_or_default().to_string_lossy();
                    let twin = vault::free_path(path.parent().unwrap_or(&root), &vault::stem(path), &extension);
                    match fs::copy(path, &twin) {
                        Ok(_) => last = Some(twin),
                        Err(e) => return self.fail(tr("Not duplicated", "Duplication impossible"), e),
                    }
                }
                if let Some(twin) = last {
                    let (notes, dirs, images) = vault::rescan(&root, &self.notes);
                    self.sync(notes, dirs, images, cx);
                    self.nav.marked.clear();
                    self.nav.reveal(&twin);
                }
            }
            _ => {}
        }
    }

    /// Replie tous les dossiers de l'arbre ; s'ils le sont déjà, les déplie tous.
    pub fn fold_all(&mut self) {
        if self.all_folded() {
            self.nav.open = self.dirs.iter().cloned().collect();
        } else {
            self.nav.open.clear();
        }
    }

    /// Aucun dossier du coffre n'est déplié. `open` garde aussi les parents du
    /// coffre lui-même, d'où le test dossier par dossier.
    fn all_folded(&self) -> bool {
        !self.dirs.iter().any(|d| self.nav.open.contains(d))
    }

    fn fail(&mut self, what: &str, e: std::io::Error) {
        self.error = Some(format!("{what} : {e}"));
    }

    /// Demande un nom dans un champ de saisie, puis appelle `then` avec ce nom nettoyé.
    fn ask(
        &mut self,
        label: impl Into<String>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
        then: impl Fn(&mut Self, String, &mut Context<Self>) + 'static,
    ) {
        let theme = self.theme;
        let prompt = cx.new(|cx| Palette::prompt(label, text, theme, cx));
        cx.subscribe_in(&prompt, window, move |this, _, event, window, cx| {
            this.palette = None;
            window.focus(&this.nav.focus);
            if let PaletteEvent::Submit(text) = event
                && let Some(name) = vault::clean_name(text)
            {
                then(this, name, cx);
            }
            cx.notify();
        })
        .detach();
        window.focus(&prompt.focus_handle(cx));
        self.palette = Some(prompt);
        cx.notify();
    }

    /// Range dans le dossier `dir` tout ce qui a été glissé.
    fn move_all(&mut self, from: &[PathBuf], dir: &Path, cx: &mut Context<Self>) {
        from.iter().for_each(|path| self.move_into(path, dir, cx));
        self.nav.marked.clear();
    }

    /// Range la note ou le dossier `from` dans le dossier `dir`.
    pub fn move_into(&mut self, from: &Path, dir: &Path, cx: &mut Context<Self>) {
        // Déjà dans ce dossier, ou dossier déposé dans lui-même : rien à faire.
        if from.parent() == Some(dir) || dir.starts_with(from) {
            return;
        }
        let Some(name) = from.file_name() else {
            return;
        };
        let to = dir.join(name);
        self.flush(cx);
        match vault::rename(from, &to) {
            Ok(()) => {
                self.nav.open.insert(dir.to_path_buf());
                self.relocate(from, &to, false, cx);
            }
            Err(e) => self.fail(tr("Not moved", "Déplacement impossible"), e),
        }
    }

    /// `from` est devenu `to` sur le disque : tout ce que l'app en sait suit.
    fn relocate(&mut self, from: &Path, to: &Path, reload: bool, cx: &mut Context<Self>) {
        let shift = |p: &PathBuf| match p.strip_prefix(from) {
            Ok(rest) if rest.as_os_str().is_empty() => to.to_path_buf(),
            Ok(rest) => to.join(rest),
            Err(_) => p.clone(),
        };
        for note in &mut self.notes {
            note.path = shift(&note.path);
            note.name = vault::stem(&note.path);
        }
        self.dirs = self.dirs.iter().map(shift).collect();
        self.images = self.images.iter().map(shift).collect();
        self.picture = self.picture.as_ref().map(shift);
        if let Some((path, _)) = &mut self.drawing {
            *path = shift(path);
        }
        if let Some((path, sheet)) = &mut self.sheet {
            *path = shift(path);
            let moved = path.clone();
            sheet.update(cx, |sheet, _| sheet.moved(moved));
        }
        self.recent = self.recent.iter().map(shift).collect();
        self.nav.open = self.nav.open.iter().map(shift).collect();
        self.new_dir = self.new_dir.as_ref().map(shift);
        self.path = self.path.as_ref().map(shift);
        if reload && self.path.as_deref() == Some(to) {
            self.reload(cx);
        }
        self.nav.reveal(to);
        self.files_changed(cx);
    }

    /// Met à la corbeille du coffre (`.trash`) la note ou le dossier.
    fn trash(&mut self, root: &Path, path: &Path, cx: &mut Context<Self>) {
        self.flush(cx);
        if let Err(e) = vault::trash(root, path) {
            return self.fail(tr("Not moved to the trash", "Mise à la corbeille impossible"), e);
        }
        let gone = |p: &PathBuf| p.starts_with(path);
        self.notes.retain(|n| !gone(&n.path));
        self.dirs.retain(|d| !gone(d));
        self.images.retain(|p| !gone(p));
        self.recent.retain(|p| !gone(p));
        if self.picture.as_ref().is_some_and(gone) {
            self.picture = None;
        }
        self.nav.sel = None;
        if self.path.as_ref().is_some_and(gone) {
            self.path = None;
            self.new_note(String::new(), cx);
        }
        self.files_changed(cx);
    }

    /// Après un déplacement ou une suppression : config, complétion et graphe à jour.
    fn files_changed(&mut self, cx: &mut Context<Self>) {
        if let Some(root) = &self.vault {
            vault::save_config(root, &self.recent);
        }
        self.error = None;
        self.push_names(cx);
        self.graph_stale = true;
        self.refresh_graph(cx);
        cx.notify();
    }

    /// Menu contextuel, par-dessus toute la fenêtre : un clic ailleurs le ferme.
    pub fn render_menu(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let menu = self.menu.as_ref()?;
        let t = self.theme;
        let is_dir = menu.target.as_ref().is_none_or(|p| self.dirs.contains(p));
        let tree = self.nav.mode == Mode::Tree;
        let many = menu.target.as_ref().is_some_and(|p| self.nav.marked.contains(p)) && self.nav.marked.len() > 1;
        // Groupes d'entrées ; un trait sépare ceux qui en ont.
        let mut groups: Vec<Vec<(&'static str, Do)>> = vec![Vec::new(); 4];
        if !is_dir && !many {
            groups[0].push((tr("Open", "Ouvrir"), Do::Open));
        }
        if tree && !many {
            groups[1].push((tr("New note here", "Nouvelle note ici"), Do::NewNote));
            groups[1].push((tr("New diagram here", "Nouveau schéma ici"), Do::NewDiagram));
            groups[1].push((tr("New folder", "Nouveau dossier"), Do::NewFolder));
        }
        if !is_dir || many {
            groups[2].push((tr("Copy the [[link]]", "Copier le [[lien]]"), Do::CopyLink));
        }
        groups[2].push((tr("Copy path", "Copier le chemin"), Do::CopyPath));
        if menu.target.is_some() {
            groups[2].push((tr("Copy relative path", "Copier le chemin relatif"), Do::CopyRelative));
        }
        if !many {
            groups[2].push((tr("Reveal in file explorer", "Afficher dans l'explorateur"), Do::Reveal));
        }
        if menu.target.is_some() {
            if !is_dir || many {
                groups[3].push((tr("Duplicate", "Dupliquer"), Do::Duplicate));
            }
            if !many {
                groups[3].push((tr("Rename", "Renommer"), Do::Rename));
            }
            groups[3].push((tr("Move to the trash", "Mettre à la corbeille"), Do::Trash));
        }
        groups.retain(|g| !g.is_empty());
        let count: usize = groups.iter().map(Vec::len).sum();
        let lines = (groups.len() - 1) as f32;
        // Le menu reste dans la fenêtre, même ouvert près d'un bord.
        let view = window.viewport_size();
        let (width, height) = (px(230.), px(28. * count as f32 + 9. * lines + 10.));
        let at = point(
            (menu.at.x - self.nav.left).min(view.width - self.nav.left * 2. - width - px(8.)),
            (menu.at.y - self.nav.left).min(view.height - self.nav.left * 2. - height - px(8.)),
        );
        let target = menu.target.clone();
        let entry = |(label, what): (&'static str, Do), cx: &mut Context<Self>| {
            let target = target.clone();
            div()
                .id(label)
                .h(px(28.))
                .mx_1()
                .px_2()
                .flex()
                .items_center()
                .rounded(px(5.))
                .hover(|s| s.bg(t.selection))
                .when(matches!(what, Do::Trash), |d| d.text_color(t.accent))
                .child(label)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.menu_do(what, target.clone(), window, cx)
                    }),
                )
                .into_any_element()
        };
        let mut rows = Vec::new();
        for (i, group) in groups.into_iter().enumerate() {
            if i > 0 {
                rows.push(div().h(px(1.)).my(px(4.)).bg(t.border).into_any_element());
            }
            rows.extend(group.into_iter().map(|item| entry(item, cx)));
        }
        let close = || {
            cx.listener(|this: &mut Self, _: &MouseDownEvent, _, cx| {
                this.menu = None;
                cx.notify();
            })
        };
        Some(
            div()
                .absolute()
                .inset_0()
                .occlude()
                .on_mouse_down(MouseButton::Left, close())
                .on_mouse_down(MouseButton::Right, close())
                .child(
                    div()
                        .absolute()
                        .left(at.x)
                        .top(at.y)
                        .w(width)
                        .py(px(5.))
                        .bg(t.panel)
                        .border_1()
                        .border_color(t.border)
                        .rounded(px(8.))
                        .shadow_lg()
                        .text_size(px(13.))
                        .children(rows)
                        .with_animation(
                            "menu-in",
                            Animation::new(Duration::from_millis(110)).with_easing(ease_out_quint()),
                            |menu, delta| menu.opacity(delta),
                        ),
                ),
        )
    }

    fn render_rows(
        &mut self,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Stateful<Div>> {
        let t = self.theme;
        let focused = self.nav.focus.is_focused(window);
        let tree = self.nav.mode == Mode::Tree;
        let marked = &self.nav.marked;
        // Glisser une ligne de la sélection multiple emporte tout le lot.
        let lot = marked.iter().next().map(|any| self.targets(any)).unwrap_or_default();
        range
            .filter_map(|ix| {
                let row = self.nav.rows.get(ix)?;
                let selected = match marked.is_empty() {
                    true => self.nav.sel.as_ref() == Some(&row.path),
                    false => marked.contains(&row.path),
                };
                let current = self.path.as_ref() == Some(&row.path);
                // Dans la liste des récents, le dossier de la note sert de détail.
                let detail = match (&self.vault, row.path.parent()) {
                    (Some(root), Some(dir)) if !tree => dir
                        .strip_prefix(root)
                        .map(|d| d.display().to_string())
                        .unwrap_or_default(),
                    _ => String::new(),
                };
                // Sous un tag, le nombre de notes qui le portent.
                let detail = match row.dir {
                    Some(_) if !tree => {
                        let tag = &row.name[1..];
                        self.notes.iter().filter(|n| n.tags.iter().any(|t| t == tag)).count().to_string()
                    }
                    _ => detail,
                };
                let foldable = self.nav.mode != Mode::Recent;
                let chevron = match row.dir {
                    Some(true) => "chevron-down.svg",
                    Some(false) => "chevron-right.svg",
                    None => "",
                };
                let line = div()
                    .size_full()
                    .pr_2()
                        .pl(px(8. + 14. * row.depth as f32))
                        .flex()
                        .items_center()
                        .gap_1()
                        .rounded(px(5.))
                        .text_size(px(13.))
                        .when(!selected, |d| d.hover(|s| s.bg(t.code_bg)))
                        .when(selected, |d| d.bg(if focused { t.selection } else { t.border }))
                        .when(foldable, |d| {
                            d.child(div().size(px(14.)).flex_none().when(row.dir.is_some(), |d| {
                                d.child(svg().path(chevron).size(px(14.)).text_color(t.dim))
                            }))
                        })
                        .child(
                            div()
                                .flex_1()
                                .truncate()
                                .when(current, |d| d.text_color(t.accent))
                                .child(row.name.clone()),
                        )
                        .child(
                            div()
                                .flex_none()
                                .max_w(px(110.))
                                .truncate()
                                .text_size(px(11.5))
                                .text_color(t.dim)
                                .child(detail),
                        );
                let (path, name) = (row.path.clone(), row.name.clone());
                // Déposer sur une note range dans le dossier de cette note.
                let into = match row.dir {
                    Some(_) => Some(path.clone()),
                    None => path.parent().map(Path::to_path_buf),
                };
                let target = path.clone();
                let (paths, name) = match marked.contains(&path) {
                    true => (lot.clone(), format!("{} {}", lot.len(), tr("items", "éléments"))),
                    false => (vec![path], name),
                };
                Some(
                    div()
                        .id(ix)
                        .w_full()
                        .h(ROW)
                        .px_1()
                        .child(line)
                        .on_click(cx.listener(move |this, e: &ClickEvent, window, cx| {
                            let held = e.modifiers();
                            if held.secondary() || held.shift {
                                return this.nav_mark(ix, held.shift, cx);
                            }
                            let folder = this.nav.rows.get(ix).is_some_and(|r| r.dir.is_some());
                            if folder || e.click_count() >= 2 {
                                this.nav_activate(ix, window, cx)
                            } else {
                                this.nav_select(ix, cx)
                            }
                        }))
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |this, e: &MouseDownEvent, window, cx| {
                                // Hors de la sélection multiple, le clic droit la remplace.
                                if !this.nav.marked.contains(&target) {
                                    this.nav.marked.clear();
                                    this.nav.sel = Some(target.clone());
                                }
                                // Un tag n'a pas de menu.
                                if this.vault.as_ref().is_some_and(|root| target.starts_with(root)) {
                                    this.menu = Some(Menu { at: e.position, target: Some(target.clone()) });
                                }
                                window.focus(&this.nav.focus);
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        )
                        .when(tree, |d| {
                            d.on_drag(Dragged { paths, name, theme: t }, |dragged, _, _, cx| {
                                cx.new(|_| dragged.clone())
                            })
                            .when(row.dir.is_some(), |d| {
                                d.drag_over::<Dragged>(move |style, _, _, _| style.bg(t.selection))
                            })
                            .on_drop(cx.listener(move |this, dragged: &Dragged, _, cx| {
                                if let Some(into) = &into {
                                    this.move_all(&dragged.paths, into, cx)
                                }
                            }))
                        }),
                )
            })
            .collect()
    }

    /// Rail, puis panneau et séparateur s'ils sont dépliés.
    pub fn render_nav(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let t = self.theme;
        let (mode, panel) = (self.nav.mode, self.nav.panel);
        let unfolded = panel != Panel::Rail;
        let source = self.rows_source();
        if source != self.nav.rows_from {
            self.nav.rows = if unfolded { self.nav_rows() } else { Vec::new() };
            self.nav.rows_from = source;
        }
        let (current, selected) = (self.path.clone(), self.nav.sel.clone());
        self.graph.update(cx, |graph, _| graph.sync(t, current.as_deref(), selected.as_deref()));
        if std::mem::take(&mut self.nav.reveal)
            && let Some(ix) = self.selected_row()
        {
            self.nav.scroll.scroll_to_item(ix, ScrollStrategy::Center);
        }

        let mode_button = |id, icon, m: Mode| {
            button(id, icon, unfolded && mode == m, t)
                .on_click(cx.listener(move |this, _, window, cx| this.show_nav(m, true, window, cx)))
        };
        let rail = div()
            .flex_none()
            .w(RAIL)
            .h_full()
            .py_2()
            .flex()
            .flex_col()
            .items_center()
            .gap_1()
            .when(!unfolded, |d| d.border_r_1().border_color(t.border))
            .when(self.nav.logo, |d| d.child(crate::logo(t).mb_1()))
            .child(mode_button("nav-tree", "tree.svg", Mode::Tree))
            .child(mode_button("nav-recent", "clock.svg", Mode::Recent))
            .child(mode_button("nav-graph", "graph.svg", Mode::Graph))
            .child(mode_button("nav-tags", "tag.svg", Mode::Tags))
            .child(div().w(px(16.)).h(px(1.)).my_1().bg(t.border))
            .child(
                button("nav-search", "search.svg", false, t)
                    .on_click(cx.listener(|this, _, window, cx| this.open_palette("", window, cx))),
            )
            .child(
                button("nav-new", "plus.svg", false, t)
                    .on_click(cx.listener(|this, _, window, cx| this.new_note_here(window, cx))),
            )
            .child(div().flex_1().w_full().when(!self.nav.logo, |d| d.map(crate::drag_window)))
            .child(button("nav-theme", "theme.svg", false, t).on_click(cx.listener(
                |this, _, window, cx| this.choose_setting(Setting::Theme, window, cx),
            )))
            .child(
                button("nav-help", "help.svg", false, t)
                    .on_click(cx.listener(|this, _, window, cx| this.set_help(true, window, cx))),
            );

        let wrapper = div().h_full().flex().min_w_0();
        if !unfolded {
            return wrapper.flex_none().child(rail);
        }

        let full = panel == Panel::Full;
        let folded = self.all_folded();
        let title = match (mode, &self.vault) {
            (Mode::Tree, Some(root)) => vault::stem(root),
            (Mode::Graph, _) => tr("Graph", "Graphe").to_string(),
            (Mode::Tags, _) => "Tags".to_string(),
            _ => tr("Recent", "Récents").to_string(),
        };
        let header = div()
            .flex_none()
            .h(px(44.))
            .pl_2()
            .pr_1()
            .flex()
            .items_center()
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .items_center()
                    .truncate()
                    .text_size(px(12.))
                    .text_color(t.dim)
                    .when(!self.nav.logo, |d| d.map(crate::drag_window))
                    .child(title),
            )
            .when(mode == Mode::Graph, |d| {
                d.child(button("nav-center", "target.svg", false, t).on_click(cx.listener(
                    |this, _, _, cx| this.graph.update(cx, |graph, cx| graph.recenter(cx)),
                )))
            })
            .when(mode == Mode::Tree, |d| {
                // Une note à la racine du coffre, quelle que soit la sélection.
                d.child(button("nav-file", "file-plus.svg", false, t).on_click(cx.listener(
                    |this, _, window, cx| this.new_note_in(None, window, cx),
                )))
                .child(button("nav-folder", "folder-plus.svg", false, t).on_click(cx.listener(
                    |this, _, window, cx| this.menu_do(Do::NewFolder, this.nav.sel.clone(), window, cx),
                )))
                // Tout replier ; si tout l'est déjà, tout déplier.
                .child(button("nav-fold", if folded { "unfold.svg" } else { "fold.svg" }, false, t).on_click(
                    cx.listener(|this, _, _, cx| {
                        this.fold_all();
                        cx.notify();
                    }),
                ))
            })
            .child(
                button("nav-full", if full { "shrink.svg" } else { "expand.svg" }, false, t)
                    .on_click(cx.listener(|this, _, window, cx| this.toggle_full(window, cx))),
            );
        let list = uniform_list("nav-rows", self.nav.rows.len(), cx.processor(Self::render_rows))
            .track_scroll(self.nav.scroll.clone())
            .flex_1()
            .min_h_0()
            // Hors de toute ligne : déposer range à la racine, le clic droit vise le coffre.
            .when(mode == Mode::Tree, |d| {
                d.on_drop(cx.listener(|this, dragged: &Dragged, _, cx| {
                    if let Some(root) = this.vault.clone() {
                        this.move_all(&dragged.paths, &root, cx)
                    }
                }))
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, e: &MouseDownEvent, _, cx| {
                        this.menu = Some(Menu { at: e.position, target: None });
                        cx.notify();
                    }),
                )
            });
        let room = self.nav.total - RAIL;
        let width = match *self.nav.width() {
            w if w == px(0.) => room / 2.,
            w => w.min(room - px(200.)).max(px(120.)),
        };
        let content = div()
            .h_full()
            .min_w_0()
            .flex()
            .flex_col()
            .map(|d| if full { d.flex_1() } else { d.flex_none().w(width) })
            .key_context("Nav")
            .track_focus(&self.nav.focus)
            .on_action(cx.listener(|this, _: &Prev, _, cx| this.nav_step(false, cx)))
            .on_action(cx.listener(|this, _: &Next, _, cx| this.nav_step(true, cx)))
            .on_action(cx.listener(|this, _: &Fold, _, cx| this.nav_fold(false, cx)))
            .on_action(cx.listener(|this, _: &Unfold, _, cx| this.nav_fold(true, cx)))
            .on_action(cx.listener(|this, _: &Open, window, cx| {
                if let Some(ix) = this.selected_row() {
                    this.nav_activate(ix, window, cx)
                }
            }))
            .when(mode != Mode::Graph, |d| {
                d.on_action(cx.listener(|this, _: &Rename, window, cx| {
                    this.menu_do(Do::Rename, this.nav.sel.clone(), window, cx)
                }))
                .on_action(cx.listener(|this, _: &Duplicate, window, cx| {
                    this.menu_do(Do::Duplicate, this.nav.sel.clone(), window, cx)
                }))
                .on_action(cx.listener(|this, _: &Trash, window, cx| {
                    this.menu_do(Do::Trash, this.nav.sel.clone(), window, cx)
                }))
            })
            .on_action(cx.listener(|this, _: &Close, window, cx| {
                if this.menu.take().is_some() {
                    return cx.notify();
                }
                if this.nav.panel == Panel::Full {
                    this.nav.panel = Panel::Split;
                }
                window.focus(&this.editor.focus_handle(cx));
                this.settle_nav(window, cx);
            }))
            .child(header)
            .map(|d| match mode {
                Mode::Graph => d.child(div().flex_1().min_h_0().child(self.graph.clone())),
                _ => d.child(list),
            });
        // Poignée de 5 px autour d'un trait de 1 px ; double-clic : largeur d'origine.
        let divider = div()
            .id("nav-divider")
            .flex_none()
            .w(px(5.))
            .h_full()
            .flex()
            .justify_center()
            .cursor(CursorStyle::ResizeLeftRight)
            .group("nav-divider")
            .child(
                div()
                    .w(px(1.))
                    .h_full()
                    .bg(if self.nav.dragging { t.accent } else { t.border })
                    .group_hover("nav-divider", |s| s.bg(t.accent)),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, window, cx| {
                    if e.click_count >= 2 {
                        *this.nav.width() = if this.nav.mode == Mode::Graph { px(0.) } else { LIST_W };
                        this.nav.panel = Panel::Split;
                        this.settle_nav(window, cx);
                    } else {
                        this.nav.dragging = true;
                    }
                    cx.stop_propagation();
                }),
            );
        wrapper
            .map(|d| if full { d.flex_1() } else { d.flex_none() })
            .child(rail)
            .child(content)
            .child(divider)
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
            tags: Vec::new(),
            links: Vec::new(),
            mtime: SystemTime::UNIX_EPOCH,
            body: "".into(),
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
