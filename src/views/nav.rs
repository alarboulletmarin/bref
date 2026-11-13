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
    logic::{dates::Date, markdown::Link},
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
    mode: Mode,
    rows: Vec<Row>,
    rows_from: u64,
    scroll: UniformListScrollHandle,
    reveal: bool,
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
    focus: FocusHandle,
    mode: Mode,
    panel: Panel,
    list_w: Pixels,
    /// Width of the graph; zero: half the window.
    graph_w: Pixels,
    dragging: bool,
    /// Selected row (note or folder).
    sel: Option<PathBuf>,
    /// Multiple selection (Ctrl+click, Shift+click); empty as long as a single row
    /// is chosen. `sel` is then the row Shift+click extends from.
    marked: HashSet<PathBuf>,
    /// The displayed document the selection last followed: when another one shows (tab, pane,
    /// picture, table…), the selection moves to it, and is free again until the next change.
    followed: Option<PathBuf>,
    /// Where the selected row was drawn: the menu opened from the keys opens under it.
    sel_at: std::rc::Rc<std::cell::Cell<Option<gpui::Bounds<Pixels>>>>,
    /// Note whose backlinks the panel shows: the last opened one, not the one being
    /// browsed in preview, otherwise the list would change under the selection.
    anchor: Option<PathBuf>,
    /// Day chosen in the calendar, which shows its month.
    day: Date,
    /// Expanded folders.
    open: HashSet<PathBuf>,
    /// Rows of the last frame, and the fingerprint of what they are drawn from.
    rows: Vec<Row>,
    rows_from: u64,
    scroll: UniformListScrollHandle,
    /// Bring the selection on screen at the next frame.
    reveal: bool,
    /// The folder unfolded last, when, and which unfolding it was: its rows fade in for `FADE`.
    unfolded: Option<(PathBuf, std::time::Instant, u32)>,
    /// The system draws the title bar (macOS, Windows): the rail does not have to serve as a
    /// handle to move the window.
    native_bar: bool,
    // Left edge and width of the window content, at the last frame.
    left: Pixels,
    total: Pixels,
    /// Width reserved to the right of the note (the comments column): the panel leaves it free.
    aside: Pixels,
    /// The views of the tools other than the active one (`mode`).
    parked: HashMap<Mode, View>,
    /// The tools shown in the left dock, top to bottom (one, as the panel always was, unless more
    /// are stacked).
    left_shown: Vec<Mode>,
    /// The tools that live in the right dock (none at first), those shown there top to bottom,
    /// and its width.
    right: Vec<Mode>,
    right_shown: Vec<Mode>,
    right_w: Pixels,
    right_dragging: bool,
    /// Tools stacked in a dock and folded down to their header.
    folded: HashSet<Mode>,
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

    pub fn focus(&self) -> &FocusHandle {
        &self.focus
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn panel(&self) -> Panel {
        self.panel
    }

    pub fn set_panel(&mut self, panel: Panel) {
        self.panel = panel;
    }

    /// The panel takes the whole window, or gives the note its place back.
    pub fn toggle_full(&mut self) {
        self.panel = if self.panel == Panel::Full { Panel::Split } else { Panel::Full };
    }

    /// The panel's border dragged to `x` (from the panel's left edge), in a content `room` wide:
    /// near the rail it collapses onto it, near the other edge it takes the whole window.
    pub fn drag_to(&mut self, x: Pixels, room: Pixels) {
        self.panel = if x < px(100.) {
            Panel::Rail
        } else if x > room - px(140.) {
            Panel::Full
        } else {
            *self.width() = x;
            Panel::Split
        };
    }

    pub fn list_w(&self) -> Pixels {
        self.list_w
    }

    /// Tests size the panel this way.
    #[cfg(test)]
    pub fn set_list_w(&mut self, w: Pixels) {
        self.list_w = w;
    }

    pub fn dragging(&self) -> bool {
        self.dragging
    }

    pub fn set_dragging(&mut self, on: bool) {
        self.dragging = on;
    }

    pub fn right_dragging(&self) -> bool {
        self.right_dragging
    }

    pub fn set_right_dragging(&mut self, on: bool) {
        self.right_dragging = on;
    }

    pub fn graph_w(&self) -> Pixels {
        self.graph_w
    }

    pub fn right_w(&self) -> Pixels {
        self.right_w
    }

    pub fn set_right_w(&mut self, w: Pixels) {
        self.right_w = w;
    }

    /// A gesture ended: no border is being dragged.
    pub fn stop_dragging(&mut self) {
        (self.dragging, self.right_dragging) = (false, false);
    }

    /// The tools shown in each dock, for tests.
    #[cfg(test)]
    pub fn left_shown(&self) -> &Vec<Mode> {
        &self.left_shown
    }

    #[cfg(test)]
    pub fn right_shown(&self) -> &Vec<Mode> {
        &self.right_shown
    }

    /// The tools that live in the right dock.
    pub fn right(&self) -> &Vec<Mode> {
        &self.right
    }

    pub fn folded(&self) -> &HashSet<Mode> {
        &self.folded
    }

    /// Selected row (note or folder).
    pub fn sel(&self) -> &Option<PathBuf> {
        &self.sel
    }

    pub fn set_sel(&mut self, sel: Option<PathBuf>) {
        self.sel = sel;
    }

    pub fn marked(&self) -> &HashSet<PathBuf> {
        &self.marked
    }

    pub fn clear_marked(&mut self) {
        self.marked.clear();
    }

    /// Ctrl+click on row `ix` (`extend`: Shift+click) of the rows drawn: adds it to the multiple
    /// selection or removes it, or extends the selection from the chosen row to it.
    pub fn mark(&mut self, ix: usize, extend: bool) {
        let Some(path) = self.rows.get(ix).map(|r| r.path.clone()) else {
            return;
        };
        if extend {
            let from = self.sel.as_ref().and_then(|sel| self.rows.iter().position(|r| r.path == *sel)).unwrap_or(ix);
            self.marked = self.rows[from.min(ix)..=from.max(ix)].iter().map(|r| r.path.clone()).collect();
            self.sel.get_or_insert(path);
        } else {
            // The row already selected is part of the batch.
            if self.marked.is_empty() {
                self.marked.extend(self.sel.take());
            }
            if !self.marked.remove(&path) {
                self.marked.insert(path.clone());
            }
            // Removed, it leaves its chosen-row place to another of the batch.
            let other = self.rows.iter().map(|r| &r.path).find(|p| self.marked.contains(*p)).cloned();
            self.sel = if self.marked.contains(&path) { Some(path) } else { other.or(Some(path)) };
        }
    }

    pub fn followed(&self) -> &Option<PathBuf> {
        &self.followed
    }

    pub fn set_followed(&mut self, followed: Option<PathBuf>) {
        self.followed = followed;
    }

    pub fn sel_at(&self) -> &std::rc::Rc<std::cell::Cell<Option<gpui::Bounds<Pixels>>>> {
        &self.sel_at
    }

    pub fn anchor(&self) -> &Option<PathBuf> {
        &self.anchor
    }

    pub fn set_anchor(&mut self, anchor: Option<PathBuf>) {
        self.anchor = anchor;
    }

    pub fn day(&self) -> Date {
        self.day
    }

    pub fn set_day(&mut self, day: Date) {
        self.day = day;
    }

    /// Expanded folders.
    pub fn open(&self) -> &HashSet<PathBuf> {
        &self.open
    }

    pub fn set_open(&mut self, open: HashSet<PathBuf>) {
        self.open = open;
    }

    /// Expands a folder (no fade: see `unfold`).
    pub fn open_dir(&mut self, dir: PathBuf) {
        self.open.insert(dir);
    }

    /// Folds a folder; whether it was expanded.
    pub fn close_dir(&mut self, dir: &Path) -> bool {
        self.open.remove(dir)
    }

    pub fn close_all_dirs(&mut self) {
        self.open.clear();
    }

    /// Rows of the last frame, and the fingerprint of what they are drawn from.
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    pub fn rows_from(&self) -> u64 {
        self.rows_from
    }

    pub fn set_rows(&mut self, rows: Vec<Row>) {
        self.rows = rows;
    }

    pub fn set_rows_from(&mut self, from: u64) {
        self.rows_from = from;
    }

    pub fn scroll(&self) -> &UniformListScrollHandle {
        &self.scroll
    }

    /// The selection comes on screen at the next frame.
    pub fn needs_reveal(&self) -> bool {
        self.reveal
    }

    pub fn set_needs_reveal(&mut self, on: bool) {
        self.reveal = on;
    }

    pub fn native_bar(&self) -> bool {
        self.native_bar
    }

    pub fn set_native_bar(&mut self, on: bool) {
        self.native_bar = on;
    }

    /// Left edge and width of the window content, at the last frame; set while drawing.
    pub fn left(&self) -> Pixels {
        self.left
    }

    pub fn total(&self) -> Pixels {
        self.total
    }

    pub fn set_frame(&mut self, left: Pixels, total: Pixels) {
        (self.left, self.total) = (left, total);
    }

    pub fn aside(&self) -> Pixels {
        self.aside
    }

    pub fn set_aside(&mut self, aside: Pixels) {
        self.aside = aside;
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
    fn left_mode(&self) -> Mode {
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
    of: PathBuf,
    /// The sorting column (0: the name), and whether it is reversed.
    by: usize,
    back: bool,
    /// What is typed keeps the rows that contain it.
    filter: Entity<Field>,
    /// Cell being rewritten: the note, the key, its field.
    edit: Option<(PathBuf, String, Entity<Field>)>,
    /// The page's scroll: only the rows near the window are built.
    scroll: gpui::ScrollHandle,
}

impl Listing {
    pub fn new(of: PathBuf, filter: Entity<Field>) -> Self {
        Self { of, by: 0, back: false, filter, edit: None, scroll: gpui::ScrollHandle::new() }
    }

    pub fn of(&self) -> &Path {
        &self.of
    }

    pub fn by(&self) -> usize {
        self.by
    }

    pub fn reversed(&self) -> bool {
        self.back
    }

    pub fn filter(&self) -> &Entity<Field> {
        &self.filter
    }

    pub fn edit(&self) -> Option<&(PathBuf, String, Entity<Field>)> {
        self.edit.as_ref()
    }

    pub fn scroll(&self) -> &gpui::ScrollHandle {
        &self.scroll
    }

    /// Sorts by this column; asking again reverses the order.
    pub fn sort_by(&mut self, col: usize) {
        (self.back, self.by) = (self.by == col && !self.back, col);
    }

    pub fn set_edit(&mut self, cell: Option<(PathBuf, String, Entity<Field>)>) {
        self.edit = cell;
    }
}

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
