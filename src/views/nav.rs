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
/// Width the panel and the comments column always leave to the note.
pub const NOTE_MIN: Pixels = px(200.);
/// Handle between the panel and the note.
pub const DIVIDER: Pixels = px(5.);

#[derive(Clone, Copy, PartialEq, Hash, Debug)]
pub enum Mode {
    Tree,
    Recent,
    Graph,
    Tags,
    /// Backlinks: the notes that lead to the open note.
    Links,
    /// Calendar: a month, showing the days that have their note.
    Calendar,
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
pub struct Menu {
    pub at: Point<Pixels>,
    pub target: Option<PathBuf>,
    /// Right click in the note and not in the tree: the targeted link, if there is one.
    pub text: Option<Option<Link>>,
    /// This right click in the note targets a comment.
    pub commented: bool,
    /// Opened by a block's handle: the block's gestures.
    pub block: bool,
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
    /// Width of the graph; zero: half the window.
    pub graph_w: Pixels,
    pub dragging: bool,
    /// Selected row (note or folder).
    pub sel: Option<PathBuf>,
    /// Multiple selection (Ctrl+click, Shift+click); empty as long as a single row
    /// is chosen. `sel` is then the row Shift+click extends from.
    pub marked: HashSet<PathBuf>,
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
}

impl Nav {
    /// Restores the panel as it was when the app closed.
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

    /// Selects the note and expands the folders leading to it.
    pub fn reveal(&mut self, path: &Path) {
        self.sel = Some(path.to_path_buf());
        self.open.extend(path.ancestors().skip(1).map(Path::to_path_buf));
        self.reveal = true;
    }

    /// Width of the panel next to the note, in the shown mode.
    pub fn width(&mut self) -> &mut Pixels {
        match self.mode {
            Mode::Graph => &mut self.graph_w,
            _ => &mut self.list_w,
        }
    }

    /// Folder where to create a note from the tree: the selection's.
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

/// The menu entry that shows a file in the file manager, under its name on each system.
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
