//! Navigation: an always-visible icon rail, and a panel (vault
//! tree, recent notes, graph or tags) that shares the window with the note.

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
    views::field::Field,
    logic::markdown::{Date, Link},
    lang::tr,
    platform::vault,
    logic::note::Note,
};

actions!(nav, [ShowCalendar, PrevMonth, NextMonth, ShowTree, ShowRecent, ShowGraph, ShowTags, ShowLinks, ToggleFull, Prev, Next, Fold, Unfold, Open, Close, NewFolder, Rename, Duplicate, Trash]);
// The entries of a row's menu that have no other way, for the open file, from the palette.
actions!(nav, [OpenAside, SetIcon, ShowFolder, CopyLink, CopyPath, CopyRelative, Reveal, MoveTo]);

/// Width of the rail and height of a row, at the interface's scale 1 (`Theme::px`).
pub const RAIL: f32 = 40.;
pub const ROW: f32 = 26.;
pub const LIST_W: Pixels = px(260.);
/// Width the panel and the comments column always leave to the note.
pub const NOTE_MIN: Pixels = px(200.);
/// Handle between the panel and the note.
pub const DIVIDER: Pixels = px(5.);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum Mode {
    #[default]
    Tree,
    Recent,
    Graph,
    Tags,
    /// Backlinks: the notes that lead to the open note.
    Links,
    /// Calendar: a month, showing the days that have their note.
    Calendar,
    /// The vault search: the matches of a search, note by note.
    Search,
}

impl Mode {
    pub const ALL: [Mode; 7] = [Mode::Tree, Mode::Recent, Mode::Graph, Mode::Tags, Mode::Links, Mode::Calendar, Mode::Search];

    /// As written in the `layout` file.
    pub fn name(self) -> &'static str {
        match self {
            Mode::Tree => "tree",
            Mode::Recent => "recent",
            Mode::Graph => "graph",
            Mode::Tags => "tags",
            Mode::Links => "links",
            Mode::Calendar => "calendar",
            Mode::Search => "search",
        }
    }

    pub fn named(name: &str) -> Option<Mode> {
        Mode::ALL.into_iter().find(|m| m.name() == name)
    }

    /// Only the graph and the calendar open as a tab: they need room, the lists do not.
    pub fn tabs(self) -> bool {
        matches!(self, Mode::Graph | Mode::Calendar)
    }
}

/// Where a tool's list stands. The active tool's view lives in `Nav` itself, where every gesture
/// of the panel finds it; the others wait in `Nav::parked` (`Nav::activate`).
#[derive(Default)]
pub struct View {
    pub mode: Mode,
    pub rows: Vec<Row>,
    pub rows_from: u64,
    pub scroll: UniformListScrollHandle,
    pub reveal: bool,
}

/// Place of the panel: collapsed onto its rail, next to the note, or alone.
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
    /// Folder, expanded or not; `None` for a note.
    pub dir: Option<bool>,
}

/// Context menu: where it opens and the targeted row (`None`: the vault itself).
#[derive(Default)]
pub struct Menu {
    pub at: Point<Pixels>,
    pub target: Option<PathBuf>,
    /// Right click in the note and not in the tree: the targeted link, if there is one.
    pub text: Option<Option<Link>>,
    /// This right click in the note targets a comment.
    pub commented: bool,
    /// Opened by a block's handle: the block's gestures.
    pub block: bool,
    /// The entry the arrow keys are on.
    pub chosen: Option<usize>,
    /// What had the focus before it opened, to give it back.
    pub back: Option<FocusHandle>,
}

#[derive(Clone, Copy)]
pub enum Do {
    Open,
    OpenAside,
    NewNote,
    NewDiagram,
    NewFolder,
    /// A note born from a template, in the targeted folder.
    FromTemplate,
    Rename,
    /// Into a folder chosen in a list: what dragging does, from the keys.
    MoveTo,
    Duplicate,
    Trash,
    CopyLink,
    CopyPath,
    CopyRelative,
    Reveal,
    /// Opens the icon grid for the targeted row.
    Icon,
    /// Shows the targeted folder as a list page.
    List,
    // Note menu.
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
    /// The chosen lines as a bulleted, numbered or task list, or as text.
    Bullets,
    Numbered,
    Tasks,
    Plain,
    /// A gesture on the cursor's table.
    Table(editor::TableOp),
    /// A gesture on the handle's block.
    Block(editor::BlockOp),
    OpenLink,
    CopyAddress,
}

impl Do {
    /// The shortcut that does the same as this menu entry, as currently in force;
    /// nothing when the action has none.
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
            // Not a binding, but the gesture that does the same.
            Do::OpenLink => format!("{}+{}", crate::ui::keys::MOD, tr("click", "clic")),
            _ => String::new(),
        }
    }
}

/// Tree row being dragged and dropped; drawn under the pointer.
#[derive(Clone)]
pub struct Dragged {
    /// The grabbed row, or the whole multiple selection if it is part of it.
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
            .rounded(t.small())
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
    /// Width of the graph; zero: half the window.
    pub graph_w: Pixels,
    pub dragging: bool,
    /// Selected row (note or folder).
    pub sel: Option<PathBuf>,
    /// Multiple selection (Ctrl+click, Shift+click); empty as long as a single row
    /// is chosen. `sel` is then the row Shift+click extends from.
    pub marked: HashSet<PathBuf>,
    /// Where the selected row was drawn: the menu opened from the keys opens under it.
    pub sel_at: std::rc::Rc<std::cell::Cell<Option<gpui::Bounds<Pixels>>>>,
    /// Note whose backlinks the panel shows: the last opened one, not the one being
    /// browsed in preview, otherwise the list would change under the selection.
    pub anchor: Option<PathBuf>,
    /// Day chosen in the calendar, which shows its month.
    pub day: Date,
    /// Expanded folders.
    pub open: HashSet<PathBuf>,
    /// Rows of the last frame, and the fingerprint of what they are drawn from.
    pub rows: Vec<Row>,
    pub rows_from: u64,
    pub scroll: UniformListScrollHandle,
    /// Bring the selection on screen at the next frame.
    pub reveal: bool,
    /// The system draws the title bar (macOS, Windows): the rail does not have to serve as a
    /// handle to move the window.
    pub native_bar: bool,
    // Left edge and width of the window content, at the last frame.
    pub left: Pixels,
    pub total: Pixels,
    /// Width reserved to the right of the note (the comments column): the panel leaves it free.
    pub aside: Pixels,
    /// The views of the tools other than the active one (`mode`).
    pub parked: HashMap<Mode, View>,
    /// The tools shown in the left dock, top to bottom (one, as the panel always was, unless more
    /// are stacked).
    pub left_shown: Vec<Mode>,
    /// The tools that live in the right dock (none at first), those shown there top to bottom,
    /// and its width.
    pub right: Vec<Mode>,
    pub right_shown: Vec<Mode>,
    pub right_w: Pixels,
    pub right_dragging: bool,
    /// Tools stacked in a dock and folded down to their header.
    pub folded: HashSet<Mode>,
}

impl Nav {
    /// Restores the panel as it was when the app closed.
    pub fn new(focus: FocusHandle) -> Self {
        let saved = vault::load_layout();
        let mut words = saved.split_whitespace();
        let mode = words.next().and_then(Mode::named).unwrap_or_default();
        // Then, in any order, what an older file does not have: the right dock.
        let (positional, keyed): (Vec<&str>, Vec<&str>) = words.partition(|w| !w.contains('='));
        let key = |name: &str| keyed.iter().find_map(|w| w.strip_prefix(name)?.strip_prefix('='));
        let list = |name: &str| {
            let mut tools: Vec<Mode> = Vec::new();
            for tool in key(name).unwrap_or_default().split(',').filter_map(Mode::named) {
                if !tools.contains(&tool) {
                    tools.push(tool);
                }
            }
            tools
        };
        // A tool on both sides stays on the left, where it always was.
        let mut left_shown = list("left");
        if !left_shown.contains(&mode) {
            left_shown.insert(0, mode);
        }
        let right: Vec<Mode> = list("right").into_iter().filter(|m| !left_shown.contains(m)).collect();
        let right_shown = list("show").into_iter().filter(|m| right.contains(m)).collect();
        let folded = list("fold").into_iter().collect();
        let mut words = positional.into_iter();
        Self {
            focus,
            mode,
            parked: HashMap::new(),
            left_shown,
            right_w: key("width").and_then(|w| w.parse().ok()).map_or(LIST_W, px),
            right,
            right_shown,
            right_dragging: false,
            folded,
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
            sel_at: Default::default(),
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
        let panel = match self.panel {
            Panel::Rail => "rail",
            Panel::Split => "split",
            Panel::Full => "full",
        };
        let (list, graph) = (f32::from(self.list_w), f32::from(self.graph_w));
        let names = |tools: &mut dyn Iterator<Item = &Mode>| tools.map(|m| m.name()).collect::<Vec<_>>().join(",");
        let mut line = format!("{} {panel} {list} {graph}", self.left_mode().name());
        if self.left_shown.len() > 1 {
            line += &format!(" left={}", names(&mut self.left_shown.iter()));
        }
        if !self.right.is_empty() {
            line += &format!(" right={} width={}", names(&mut self.right.iter()), f32::from(self.right_w));
            if !self.right_shown.is_empty() {
                line += &format!(" show={}", names(&mut self.right_shown.iter()));
            }
        }
        if !self.folded.is_empty() {
            let mut folded: Vec<&Mode> = self.folded.iter().collect();
            folded.sort_by_key(|m| m.name());
            line += &format!(" fold={}", names(&mut folded.into_iter()));
        }
        vault::save_layout(&format!("{line}\n"));
    }

    /// The tool's view comes into `Nav`, the active one's goes to wait.
    pub fn activate(&mut self, mode: Mode) {
        if self.mode == mode {
            return;
        }
        let mut view = self.parked.remove(&mode).unwrap_or_else(|| View { mode, ..View::default() });
        std::mem::swap(&mut self.mode, &mut view.mode);
        std::mem::swap(&mut self.rows, &mut view.rows);
        std::mem::swap(&mut self.rows_from, &mut view.rows_from);
        std::mem::swap(&mut self.scroll, &mut view.scroll);
        std::mem::swap(&mut self.reveal, &mut view.reveal);
        self.parked.insert(view.mode, view);
    }

    /// The tool alone in the left dock, its view in `Nav` (tests set the panel this way).
    #[cfg(test)]
    pub fn show_alone(&mut self, mode: Mode) {
        self.activate(mode);
        self.left_shown = vec![mode];
    }

    /// The first tool of the left dock: the panel's mode, as older files say it.
    pub fn left_mode(&self) -> Mode {
        self.left_shown.first().copied().unwrap_or(self.mode)
    }

    /// The tool that sets the left dock's width: the graph alone takes its own.
    pub fn width_mode(&self) -> Mode {
        match self.left_shown.as_slice() {
            [only] => *only,
            _ => Mode::Tree,
        }
    }

    /// Selects the note and expands the folders leading to it.
    pub fn reveal(&mut self, path: &Path) {
        self.sel = Some(path.to_path_buf());
        self.open.extend(path.ancestors().skip(1).map(Path::to_path_buf));
        self.reveal = true;
    }

    /// Width of the panel next to the note, in the shown mode.
    pub fn width(&mut self) -> &mut Pixels {
        match self.width_mode() {
            Mode::Graph => &mut self.graph_w,
            _ => &mut self.list_w,
        }
    }

    /// Folder where to create a note from the tree: the selection's.
    pub fn target_dir(&self) -> Option<PathBuf> {
        if self.panel == Panel::Rail || self.mode != Mode::Tree {
            return None;
        }
        folder_of(self.sel.as_ref()?)
    }
}

/// The folder a new file goes in when `sel` is selected in the tree: itself if it is a folder, otherwise the
/// folder it is in (a note, a picture, a diagram, a table: none of them can hold a file).
pub fn folder_of(sel: &Path) -> Option<PathBuf> {
    if sel.is_dir() { Some(sel.to_path_buf()) } else { sel.parent().map(Path::to_path_buf) }
}

/// Visible rows of the tree: folders first, then notes, in alphabetical
/// order; the content of collapsed folders is left out. `dirs` brings the
/// folders without notes.
pub fn tree_rows(
    root: &Path,
    notes: &[Note],
    dirs: &[PathBuf],
    images: &[PathBuf],
    open: &HashSet<PathBuf>,
) -> Vec<Row> {
    let mut all: HashSet<&Path> = dirs.iter().map(PathBuf::as_path).collect();
    // Per folder: its notes then its pictures, each with the shown name.
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
    // Depth-first walk: (folder, depth of its content), subfolders first.
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

/// Row of a tag in the panel: a fake path, that no file bears.
fn tag_path(tag: &str) -> PathBuf {
    PathBuf::from(format!("#{tag}"))
}

/// The tag this path points to (`#tag`), if it is one.
pub fn tag_of(path: &Path) -> Option<&str> {
    path.to_str()?.strip_prefix('#')
}

/// List page shown in place of the note.
pub struct Listing {
    /// The folder, or the tag (`#tag`), whose notes are listed.
    pub of: PathBuf,
    /// The sorting column (0: the name), and whether it is reversed.
    pub by: usize,
    pub back: bool,
    /// What is typed keeps the rows that contain it.
    pub filter: Entity<Field>,
    /// Cell being rewritten: the note, the key, its field.
    pub edit: Option<(PathBuf, String, Entity<Field>)>,
}

/// The vault's tags in alphabetical order; under an expanded tag, its notes.
// ponytail: a note with several expanded tags appears under each, and the
// selection, held by its path, marks them all; give each row its own identity
// if that gets in the way.
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

pub use crate::logic::commands::reveal_label;

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, time::SystemTime};

    #[test]
    fn a_file_selected_in_the_tree_is_not_a_folder() {
        let root = std::env::temp_dir().join(format!("bref-nav-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("Dossier.v2")).unwrap();
        for file in ["note.md", "schéma.svg", "photo.png", "table.csv", "sans extension", "Dossier.v2/dedans.md"] {
            fs::write(root.join(file), "").unwrap();
        }
        // A folder is its own target, even with a dot in its name; every file gives the folder it is in.
        assert_eq!(folder_of(&root.join("Dossier.v2")), Some(root.join("Dossier.v2")));
        assert_eq!(folder_of(&root), Some(root.clone()));
        for file in ["note.md", "schéma.svg", "photo.png", "table.csv", "sans extension"] {
            assert_eq!(folder_of(&root.join(file)), Some(root.clone()), "{file}");
        }
        assert_eq!(folder_of(&root.join("Dossier.v2/dedans.md")), Some(root.join("Dossier.v2")));
        // What is not there any more (removed outside) is looked for beside, never inside; the root of the disk is its own folder.
        assert_eq!(folder_of(&root.join("disparu.svg")), Some(root.clone()));
        assert_eq!(folder_of(Path::new("/")), Some(PathBuf::from("/")));
        fs::remove_dir_all(&root).unwrap();
    }

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
        // Folders first, case-insensitive; everything is collapsed.
        assert_eq!(shown(&[]), ["a/", "Z/", "A", "b"]);
        // A folder without notes appears too.
        // A folder's pictures follow its notes.
        assert_eq!(shown(&["a"]), ["a/", "  c/", "  vide/", "  B", "  Photo.png", "Z/", "A", "b"]);
        // A folder expanded inside a collapsed folder stays hidden.
        assert_eq!(shown(&["a/c", "Z"]), ["a/", "Z/", "  x", "A", "b"]);
        assert_eq!(shown(&["a", "a/c"]), ["a/", "  c/", "    d", "  vide/", "  B", "  Photo.png", "Z/", "A", "b"]);

        // Tags: in alphabetical order; an expanded tag shows its notes.
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
