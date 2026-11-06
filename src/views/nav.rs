//! Navigation: an always-visible icon rail, and a panel (vault
//! tree, recent notes, graph or tags) that shares the window with the note.

use crate::logic::dock;
use crate::platform::config;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use gpui::{
    Context, FocusHandle, Entity, Pixels, Point, UniformListScrollHandle, Window,
    actions, div, prelude::*, px,
};

use crate::{
    ui::theme::Theme, views::editor,
    views::field::Field,
    logic::markdown::{Date, Link},
    lang::tr,
    platform::vault,
};

actions!(nav, [ShowCalendar, PrevMonth, NextMonth, ShowTree, ShowRecent, ShowGraph, ShowTags, ShowLinks, ToggleFull, Prev, Next, Fold, Unfold, Open, Close, NewFolder, Rename, Duplicate, Trash]);
// The entries of a row's menu that have no other way, for the open file, from the palette.
actions!(nav, [OpenAside, SetIcon, ShowFolder, CopyLink, CopyPath, CopyRelative, Reveal, MoveTo]);

/// Width of the rail and height of a row, at the interface's scale 1 (`Theme::px`).
pub const RAIL: f32 = 40.;
pub const ROW: f32 = 28.;
/// Indent of a tree row per level of depth, at scale 1.
pub const INDENT: f32 = 16.;
pub const LIST_W: Pixels = px(crate::logic::dock::LIST_W);
/// Width the panel and the comments column always leave to the note.
pub const NOTE_MIN: Pixels = px(200.);
/// Ground between two sheets and around the window's edges (none on the rail's side).
pub const GAP: Pixels = px(8.);
/// Handle between the panel and the note: the gap itself, so that dragging it needs no thin line.
pub const DIVIDER: Pixels = GAP;

pub use crate::logic::dock::{Mode, Panel};

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

/// Narrowest detail (a note's folder) worth showing; under it, it is hidden.
pub const DETAIL_MIN: f32 = 40.;

impl Row {
    /// The name as shown and its extension: a note (its name has no `.md`) and a folder are never split.
    pub fn split(&self) -> (&str, &str) {
        match self.dir.is_some() || self.path.extension().is_some_and(|e| e == "md") {
            true => (&self.name, ""),
            false => split_ext(&self.name),
        }
    }

    /// The icon that says what the row is: a folder (shut or open), a note, a table, a diagram, a picture,
    /// any other file.
    pub fn icon(&self) -> &'static str {
        let ext = self.path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        match (self.dir, ext.as_str()) {
            (Some(true), _) => "folder-open.svg",
            (Some(false), _) => "folder.svg",
            (_, "md") => "note.svg",
            (_, "csv" | "tsv") => "table.svg",
            (_, "svg") => "diagram.svg",
            _ if vault::is_image(&self.path) => "image.svg",
            _ => "file.svg",
        }
    }
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
    /// Right click on a tab: what to close.
    pub tab: Option<usize>,
    /// Share/export menu, pinned to the document tab that opened it.
    pub share: Option<usize>,
    /// The entry the arrow keys are on.
    pub chosen: Option<usize>,
    /// What had the focus before it opened, to give it back.
    pub back: Option<FocusHandle>,
}

#[derive(Clone, Copy)]
pub enum Do {
    Share(Share),
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
    // Tab menu.
    CloseTab,
    CloseOtherTabs,
    CloseTabsInPane,
}

/// Formats available from the current document's sharing menu.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Share {
    MarkdownCopy, PlainCopy, MarkdownSave, PlainSave, SvgCopy, SvgSave, Png,
    TableCopy, TableExport, ImageSave, Pdf,
}

impl Share {
    pub fn label(self) -> &'static str {
        match self {
            Self::MarkdownCopy => tr("Copy note as Markdown", "Copier la note en Markdown"),
            Self::PlainCopy => tr("Copy note as plain text", "Copier la note en texte brut"),
            Self::MarkdownSave => tr("Save a Markdown copy…", "Enregistrer une copie Markdown…"),
            Self::Pdf => tr("Save the note as PDF…", "Enregistrer la note en PDF…"),
            Self::PlainSave => tr("Save a text copy…", "Enregistrer une copie texte…"),
            Self::SvgCopy => tr("Copy as SVG", "Copier en SVG"),
            Self::SvgSave => tr("Save an SVG copy…", "Enregistrer une copie SVG…"),
            Self::Png => tr("Export as PNG", "Exporter en PNG"),
            Self::TableCopy => tr("Copy as… (table/selection)", "Copier en… (tableau/sélection)"),
            Self::TableExport => tr("Export as… (table/selection)", "Exporter en… (tableau/sélection)"),
            Self::ImageSave => tr("Save an image copy…", "Enregistrer une copie de l’image…"),
        }
    }
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
            Do::CloseTab => of(cx, &crate::actions::ClosePane),
            Do::CloseOtherTabs => of(cx, &crate::actions::CloseOtherTabs),
            Do::CloseTabsInPane => of(cx, &crate::actions::CloseTabsInPane),
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
            .text_size(t.font_body())
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
    /// The displayed document the selection last followed: when another one shows (tab, pane,
    /// picture, table…), the selection moves to it, and is free again until the next change.
    pub followed: Option<PathBuf>,
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
    /// The folder unfolded last, when, and which unfolding it was: its rows fade in for `FADE`.
    pub unfolded: Option<(PathBuf, std::time::Instant, u32)>,
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
        let saved = dock::parse(&config::load(config::File::Layout));
        Self {
            focus,
            mode: saved.mode,
            parked: HashMap::new(),
            left_shown: saved.left_shown,
            right_w: px(saved.right_w),
            right: saved.right,
            right_shown: saved.right_shown,
            right_dragging: false,
            folded: saved.folded,
            panel: saved.panel,
            list_w: px(saved.list_w),
            graph_w: px(saved.graph_w),
            dragging: false,
            sel: None,
            followed: None,
            marked: HashSet::new(),
            sel_at: Default::default(),
            anchor: None,
            day: crate::platform::clock::today(),
            open: HashSet::new(),
            rows: Vec::new(),
            rows_from: 0,
            scroll: UniformListScrollHandle::new(),
            reveal: false,
            unfolded: None,
            native_bar: false,
            left: px(0.),
            total: px(0.),
            aside: px(0.),
        }
    }

    pub fn save(&self) {
        let layout = dock::Layout {
            mode: self.left_mode(),
            panel: self.panel,
            list_w: f32::from(self.list_w),
            graph_w: f32::from(self.graph_w),
            left_shown: self.left_shown.clone(),
            right: self.right.clone(),
            right_shown: self.right_shown.clone(),
            right_w: f32::from(self.right_w),
            folded: self.folded.clone(),
        };
        config::keep(config::File::Layout, &format!("{}\n", dock::write(&layout)));
    }

    /// A panel that took the whole window gives the note its place back.
    pub fn give_note_room(&mut self) {
        if self.panel == Panel::Full {
            self.panel = Panel::Split;
        }
    }

    /// The tools drawn in the left dock (or the right one), top to bottom: those shown there that
    /// live there and are not open as a tab (`tabs`: the tools that are).
    pub fn dock_tools(&self, right: bool, tabs: &[Mode]) -> Vec<Mode> {
        let shown = if right { &self.right_shown } else { &self.left_shown };
        shown.iter().copied().filter(|m| self.right.contains(m) == right && !tabs.contains(m)).collect()
    }

    pub fn left_open(&self, tabs: &[Mode]) -> bool {
        self.panel != Panel::Rail && !self.dock_tools(false, tabs).is_empty()
    }

    pub fn right_open(&self, tabs: &[Mode]) -> bool {
        self.panel != Panel::Full && !self.dock_tools(true, tabs).is_empty()
    }

    /// The tool is drawn in a dock that is open.
    pub fn docked(&self, mode: Mode, tabs: &[Mode]) -> bool {
        self.left_open(tabs) && self.dock_tools(false, tabs).contains(&mode) || self.right_open(tabs) && self.dock_tools(true, tabs).contains(&mode)
    }

    /// The tool's view comes into `Nav` and its dock opens on it: alone, as the panel always did,
    /// unless it is already shown there with others.
    pub fn put_tool(&mut self, mode: Mode, tabs: &[Mode]) {
        let right = self.right.contains(&mode);
        if !self.docked(mode, tabs) {
            match right {
                true => self.right_shown = vec![mode],
                false => self.left_shown = vec![mode],
            }
        }
        if !right && self.panel == Panel::Rail {
            self.panel = Panel::Split;
        }
        self.folded.remove(&mode);
        self.activate(mode);
    }

    /// The tool joins the others of its dock, at the bottom, and is the active one (the caller
    /// has checked that it is not shown).
    pub fn stack_tool(&mut self, mode: Mode, tabs: &[Mode]) {
        let right = self.right.contains(&mode);
        // A folded left dock opens on this tool alone.
        let fresh = !right && !self.left_open(tabs);
        let stack = if right { &mut self.right_shown } else { &mut self.left_shown };
        if fresh {
            stack.clear();
        }
        stack.retain(|m| *m != mode);
        stack.push(mode);
        if !right && self.panel == Panel::Rail {
            self.panel = Panel::Split;
        }
        self.folded.remove(&mode);
        self.activate(mode);
        self.reveal = true;
    }

    /// The tool leaves its dock's stack; the last one of the left dock folds the panel onto its
    /// rail instead, so that it comes back the same.
    pub fn hide_tool(&mut self, mode: Mode, tabs: &[Mode]) {
        match self.right.contains(&mode) {
            true => self.right_shown.retain(|m| *m != mode),
            false if self.dock_tools(false, tabs) == [mode] => self.panel = Panel::Rail,
            false => self.left_shown.retain(|m| *m != mode),
        }
    }

    /// The tool the keys go to when `hide_tool` left the dock `right`: the first one in sight in
    /// that dock, else in the other.
    pub fn next_docked(&self, right: bool, tabs: &[Mode]) -> Option<Mode> {
        self.dock_tools(right, tabs).first().or(self.dock_tools(!right, tabs).first()).copied().filter(|m| self.docked(*m, tabs))
    }

    /// A tool stacked in a dock folds down to its header, or unfolds.
    pub fn toggle_fold(&mut self, mode: Mode) {
        if !self.folded.remove(&mode) {
            self.folded.insert(mode);
        }
    }

    /// The right dock's border follows the pointer.
    pub fn drag_right(&mut self, x: Pixels) {
        self.right_w = (self.left + self.total - x).max(px(160.));
    }

    /// Puts `mode` at place `at` of the stack of the right dock (`right`) or the left one; it is
    /// shown there and active. The left dock, left without tools, folds onto its rail.
    pub fn place_tool(&mut self, mode: Mode, right: bool, at: usize, tabs: &[Mode]) {
        // The place counts the tools drawn; the tool itself, if it was above, leaves a gap.
        let drawn = self.dock_tools(right, tabs);
        let at = match drawn.iter().position(|m| *m == mode) {
            Some(was) if was < at => at - 1,
            _ => at,
        };
        let before = drawn.iter().filter(|m| **m != mode).nth(at).copied();
        let left_drawn = self.left_open(tabs);
        self.left_shown.retain(|m| *m != mode);
        self.right_shown.retain(|m| *m != mode);
        self.right.retain(|m| *m != mode);
        let stack = match right {
            true => {
                self.right.push(mode);
                &mut self.right_shown
            }
            false => {
                // A folded left dock opens on this tool alone.
                if !left_drawn {
                    self.left_shown.clear();
                }
                &mut self.left_shown
            }
        };
        let at = before.and_then(|b| stack.iter().position(|m| *m == b)).unwrap_or(stack.len());
        stack.insert(at, mode);
        match (right, self.dock_tools(false, tabs).is_empty()) {
            (false, _) => {
                if self.panel != Panel::Full {
                    self.panel = Panel::Split;
                }
            }
            // Its last tool gone, the left dock shows another one that lives there; none left, it folds.
            (true, true) => match Mode::ALL.into_iter().find(|m| !self.right.contains(m) && !tabs.contains(m)) {
                Some(other) => self.left_shown = vec![other],
                None => self.panel = Panel::Rail,
            },
            (true, false) => {}
        }
        self.folded.remove(&mode);
        self.activate(mode);
        self.reveal = true;
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
impl Nav {
    /// Opens a folder of the tree and remembers it, so that its rows fade in.
    pub fn unfold(&mut self, dir: PathBuf) {
        let seq = self.unfolded.as_ref().map_or(0, |u| u.2.wrapping_add(1));
        self.unfolded = Some((dir.clone(), std::time::Instant::now(), seq));
        self.open.insert(dir);
    }

    /// Whether the row at `path` came with the last unfolding and is still fading in: the id of its fade.
    pub fn fading(&self, path: &Path) -> Option<u32> {
        let (dir, at, seq) = self.unfolded.as_ref()?;
        (at.elapsed() < crate::ui::widgets::FADE && path.starts_with(dir) && path != dir).then_some(*seq)
    }
}

pub fn folder_of(sel: &Path) -> Option<PathBuf> {
    if sel.is_dir() { Some(sel.to_path_buf()) } else { sel.parent().map(Path::to_path_buf) }
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
    /// The page's scroll: only the rows near the window are built.
    pub scroll: gpui::ScrollHandle,
}

pub use crate::logic::commands::reveal_label;
pub use crate::logic::tree::{Row, tag_of, tag_rows, tree_rows};
pub use crate::logic::rows::{ellipsize, share, split_ext};

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn a_row_splits_its_extension_off_a_file_name_only() {
        let row = |file: &str, name: &str, dir| Row { path: PathBuf::from(file), name: name.into(), depth: 0, dir };
        // A note's name has no `.md` (its dot is part of the name); a folder is never split.
        assert_eq!(row("n.v2.md", "n.v2", None).split(), ("n.v2", ""));
        assert_eq!(row("Dossier.v2", "Dossier.v2", Some(false)).split(), ("Dossier.v2", ""));
        assert_eq!(row("t/a.csv", "a.csv", None).split(), ("a", ".csv"));
        // The ellipsis goes into the name, the extension stays whole.
        let width = |t: &str| t.chars().count() as f32;
        let (stem, ext) = split_ext("Très longue note de réunion.pdf");
        let cut = ellipsize(stem, 16. - width(ext), width);
        assert_eq!(format!("{cut}{ext}"), "Très longue….pdf");
        assert!(width(&format!("{cut}{ext}")) <= 16.);
    }

    #[test]
    fn every_kind_of_row_has_its_icon() {
        let row = |file: &str, dir: Option<bool>| Row { path: PathBuf::from(file), name: String::new(), depth: 0, dir };
        assert_eq!(row("a/Dossier.v2", Some(false)).icon(), "folder.svg");
        assert_eq!(row("a/Dossier.v2", Some(true)).icon(), "folder-open.svg");
        for (file, icon) in [
            ("note.md", "note.svg"), ("NOTE.MD", "note.svg"), ("table.CSV", "table.svg"), ("t.tsv", "table.svg"),
            ("schéma.svg", "diagram.svg"), ("photo.PNG", "image.svg"), ("p.jpg", "image.svg"), ("doc.pdf", "file.svg"), ("sans extension", "file.svg"),
        ] {
            assert_eq!(row(file, None).icon(), icon, "{file}");
        }
    }

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
}
