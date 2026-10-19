//! Live-styled Markdown editor: the text stays raw Markdown, only the
//! rendering changes (sizes, weights, colors).


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
mod notes;
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
pub use notes::*;
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
    ui::theme::Theme, logic::block, logic::diagram::Diagram, ui::figure, views::graph, views::viewer::Source, ui::svg, ui::assets::mono, ui::assets::sans, lang::tr, platform::vault,
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

/// Band kept at the bottom of the note for the counter and the formatting bar or its button:
/// the text scrolls behind it, and the cursor and the popups stay above it.
pub(super) const FOOTER: Pixels = px(56.);
const TOP: Pixels = px(20.);
pub const MAX_WIDTH: Pixels = px(720.);
/// Margin kept on each side of the page, narrow or full.
const SIDE: Pixels = px(24.);

/// Side of the copy button, right of the opening line of a code block.
const COPY_SIZE: Pixels = px(22.);
/// Size of formulas relative to the text: the math font looks small at equal size.
const MATH_SCALE: f32 = 1.2;
/// Space between a line and the image it points to.
const IMAGE_GAP: Pixels = px(8.);
/// `file` is inside `root`, once `..` and symbolic links are resolved: a received
/// note must not display a file from the disk outside the vault.
// ponytail: an image folder linked by a symbolic link outside the vault is
// not followed; allow the link's target if someone needs it.
fn inside(root: &Path, file: &Path) -> bool {
    let (Ok(root), Ok(file)) = (fs::canonicalize(root), fs::canonicalize(file)) else {
        return false;
    };
    file.starts_with(root)
}

pub enum EditorEvent {
    Changed,
    Open(Link),
    /// The same, with Shift: to open in the other pane.
    OpenAside(Link),
    /// Replace across the whole vault what the search bar looks for.
    Swap(md::Swap),
    /// Right click in the note: where, the link found there, and whether it targets a comment.
    Menu(Point<Pixels>, Option<Link>, bool),
    /// Click on a block handle: its menu, at that spot.
    BlockMenu(Point<Pixels>),
    /// A diagram, formula or picture to view large.
    View(Source),
}

/// What the footer counter depends on: the text version and the selection.
type CountKey = (u64, Option<Range<usize>>);

pub struct Editor {
    focus: FocusHandle,
    content: String,
    sel: Range<usize>,
    reversed: bool,
    /// The other cursors (Alt+click, next occurrence): their selection, and whether it is
    /// backwards. What the main cursor does, they do too (see `each`).
    more: Vec<(Range<usize>, bool)>,
    /// The following cursors replay the first one's gesture: a single undo step.
    held: bool,
    marked: Option<Range<usize>>,
    selecting: bool,
    /// Word or line grabbed by a double or triple click (and whether it is a line):
    /// dragging extends it word by word or line by line.
    anchor: Option<(Range<usize>, bool)>,
    goal_x: Option<Pixels>,
    undo: History,
    redo: History,
    last_edit: Option<Instant>,
    notes: Vec<String>,
    /// Folders to look for images in: the note's, then the vault.
    dirs: Vec<PathBuf>,
    /// Vault images by file name (lowercase): wherever they are
    /// stored, a note finds them.
    images: HashMap<String, PathBuf>,
    /// Start of the line whose code block was just copied.
    copied: Option<usize>,
    /// Start of the line whose copy button is hovered.
    hover: Option<usize>,
    /// Line whose "full size" button is hovered.
    view_hover: Option<usize>,
    /// Line whose picture, formula or figure is under the pointer (its "enlarge" button shows).
    big_hover: Option<usize>,
    /// Areas of the copy buttons, by line start: they turn the pointer into a hand.
    copy_hitboxes: Vec<(usize, Hitbox)>,
    view_hitboxes: Vec<(usize, Hitbox)>,
    big_hitboxes: Vec<(usize, Hitbox)>,
    /// Diagrams and formulas of the last frame, by hash of their source.
    figures: Figures,
    /// Figures being drawn.
    waiting: Vec<Job>,
    /// Lines styled in the last frame, by hash of their text and
    /// of their context.
    shaped: FastMap<u64, (Rc<Shaped>, u32)>,
    /// Number of the last layout: a line is marked with it when it is used.
    frame: u32,
    /// Table cells styled in the last frame.
    cells: grid::Cache,
    ac_index: usize,
    ac_dismissed: Option<usize>,
    /// Table picker open under the cursor: chosen columns and rows.
    grid: Option<(usize, usize)>,
    /// Size typed while it is open (`12x5`): it goes beyond the grid.
    grid_typed: String,
    /// Its area: the pointer turns into a hand there.
    grid_hitbox: Option<Hitbox>,
    /// Extent and frame of each table in the last frame, then the areas of
    /// their "+" buttons, two per table.
    tables: Vec<(Range<usize>, Bounds<Pixels>)>,
    table_hitboxes: Vec<Hitbox>,
    /// Hovered table (its start), and which of its buttons is.
    table_hover: Option<(usize, Option<usize>)>,
    /// Where the lines of each displayed table are, in the last frame.
    geos: Vec<Geo>,
    /// The commented parts of each table, for one version of the text and one cursor.
    marks: Option<Marks>,
    /// The handle under the pointer: (table start, handle).
    part_hover: Option<(usize, Part)>,
    /// Where the pointer was at its last move.
    pointer: Point<Pixels>,
    /// The pointer shape over what can be clicked in the text (handle, checkbox, link under `Ctrl`), and the
    /// area of the note where it applies.
    hand: Option<CursorStyle>,
    view_hitbox: Option<Hitbox>,
    /// Block handles: the note's blocks for one version of the text, the block under the pointer
    /// (at its version), the one being dragged, the one the handle menu targets, and the areas of the
    /// handle and of the "+".
    blocks: Option<(u64, Rc<Vec<Range<usize>>>)>,
    handles: BlockHandles,
    /// Where dragged files would land: a line between two lines.
    drop_mark: Option<usize>,
    pick: Option<Pick>,
    moving: Option<Moving>,
    /// Horizontal scroll of tables wider than the page, by table start.
    table_x: HashMap<usize, Pixels>,
    /// Scrollbar of each too-wide table, in the last frame.
    thumbs: Vec<Bounds<Pixels>>,
    theme: Theme,
    // Layout of the last frame.
    rows: Vec<Row>,
    /// Changes on every edit of the text or of what is used to lay it out
    /// (theme, images): as long as it and the rest of `LayoutKey` do not move, the
    /// layout is reused as is, for a plain scroll.
    version: u64,
    /// Counts text changes, and only them: an image that loads or a theme that changes does not move it.
    revision: u64,
    /// Footer counter, kept as long as the text and the selection do not move.
    counted: Option<(CountKey, String)>,
    /// The text counted last, its version and its counts: on a keystroke, only the changed
    /// lines are recounted (`md::recount`).
    tally: Option<(u64, String, (usize, usize))>,
    /// Number of buttons the window puts in the corner, right of the counter.
    pub corner: usize,
    /// The formatting bar: what it offers, where it sits, folded or open.
    bar: BarState,
    /// The note is the active pane's: a single formatting bar for the whole window.
    pub bar_here: bool,
    /// Changes when everything must be redone (other note, theme, images): nothing is reused.
    epoch: u64,
    dmg: Option<Damage>,
    laid: Option<Laid>,
    /// Tests redo the whole layout every time, to compare it with the reused one.
    force_full: bool,
    /// Test: compare each reused layout with a full layout.
    #[cfg(test)]
    compare: bool,
    /// Test: full layouts and reused layouts.
    #[cfg(test)]
    counts: (usize, usize, usize),
    laid_out: Option<LayoutKey>,
    /// An image or a diagram is still loading: the layout will be redone.
    loading: bool,
    /// Total height of the lines, and indices of the lines that carry a copy button.
    total_y: Pixels,
    opening: Vec<usize>,
    /// Each table of the note: its extent, its rows and its width.
    table_meta: Vec<TableMeta>,
    big: Vec<BigBlock>,
    /// Hash of the theme and fonts of the last layout.
    big_style: u64,
    origin: Point<Pixels>,
    width: Pixels,
    /// Width offered to a table: the text's, or more if the settings let it overflow.
    room: Pixels,
    viewport: Bounds<Pixels>,
    scroll_y: Pixels,
    reveal: bool,
    /// With `reveal`: the cursor's line goes to the top of the view (jump to a heading).
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
            big_hover: None,
            copy_hitboxes: Vec::new(),
            view_hitboxes: Vec::new(),
            big_hitboxes: Vec::new(),
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
            marks: None,
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

    /// Replaces the whole content (note change); cursor at byte `cursor`.
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
        // Another note: the bar stays, input goes back to the text.
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

    /// Names of the vault's notes, to complete `[[wikilinks]]`.
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

    /// Content of the code block the cursor is in.
    pub fn code_at_cursor(&self) -> Option<&str> {
        md::code_block(&self.content, self.cursor()).map(|r| &self.content[r])
    }

    /// Center of the first "Copy" button shown.
    #[cfg(test)]
    pub fn copy_button(&self) -> Option<Point<Pixels>> {
        Some(self.copy_bounds(self.rows.iter().find(|r| r.opens)?).center())
    }

    /// Number of characters and size of the image shown on the line starting with `prefix`.
    #[cfg(test)]
    pub fn decorations(&self, prefix: &str) -> (usize, Option<(f32, f32)>) {
        let row = self.rows.iter().find(|r| self.content[r.start..].starts_with(prefix)).unwrap();
        let image = row.image.as_ref().map(|(_, s)| (s.width.into(), s.height.into()));
        (row.shaped.subs.len(), image)
    }

    /// Center of the "full size" button of the Mermaid block starting with `prefix`.
    #[cfg(test)]
    pub fn view_button(&self, prefix: &str) -> Option<Point<Pixels>> {
        let row = self.rows.iter().find(|r| self.views(r) && self.content[r.start..].starts_with(prefix))?;
        Some(self.view_bounds(row).center())
    }

    /// Every picture, formula or figure that can be enlarged: its line's start, its bounds in the
    /// window, its button's, and whether the button is shown.
    #[cfg(test)]
    pub fn enlargeable(&self) -> Vec<(usize, Bounds<Pixels>, Bounds<Pixels>, bool)> {
        let rows = self.rows.iter();
        rows.filter_map(|r| Some((r.start, self.image_bounds(r)?, self.big_bounds(r)?, self.big_hover == Some(r.start)))).collect()
    }

    /// Kept figures, and figures being drawn.
    #[cfg(test)]
    pub fn figure_count(&self) -> (usize, usize) {
        (self.figures.len(), self.waiting.len())
    }

    /// Center of the "+" buttons (row, column) of the first table shown.
    #[cfg(test)]
    pub fn plus_buttons(&self) -> Option<[Point<Pixels>; 2]> {
        Some(plus_bars(self.tables.first()?.1).map(|bar| bar.center()))
    }

    /// Footer counter: words and characters of the note, or of the selection. Recounted only when the text or the selection change.
    pub fn counts(&mut self) -> String {
        // Without a selection, moving the cursor changes nothing in the count.
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
                // ponytail: a copy of the text for the next recount; keep the counts in `splice` if it weighs.
                self.tally = Some((self.version, self.content.clone(), whole));
            }
            both(whole)
        } else {
            format!("{} {}", tr("Selection:", "Sélection :"), both(md::counts(&self.content[self.sel.clone()])))
        };
        self.counted = Some((key, label.clone()));
        label
    }

    /// Type of the line holding byte `i`, in the last layout.
    #[cfg(test)]
    pub fn kind_at(&self, i: usize) -> Option<Kind> {
        Some(self.rows[self.row_at(i)?].kind)
    }

    /// Cursor position in the text.
    pub fn position(&self) -> usize {
        self.cursor()
    }

    /// Where we are in the note: the cursor and the scroll, to come back to it.
    pub fn view(&self) -> (usize, f32) {
        (self.cursor(), self.scroll_y.into())
    }

    /// Goes back to where we were: the scroll is restored as is, not recomputed from the
    /// cursor.
    pub fn set_view(&mut self, at: usize, scroll: f32, cx: &mut Context<Self>) {
        self.more.clear();
        self.move_to(self.clamp(at), cx);
        self.scroll_y = px(scroll);
        self.reveal = false;
    }

    /// Table dash lines that are not shown.
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

    /// A ``` without a closing fence does not open a block, as in `layout`.
    fn in_code(&self, line_start: usize) -> bool {
        self.fences_before(line_start) % 2 == 1
            && self.content[line_start..].lines().any(md::is_fence)
    }

    // ----- Search in the note -----

    // ----- Multiple cursors -----

    // ----- Moves -----

    // ----- Markdown editing -----

    // ----- Tables -----

    // ----- Block handles -----

    // ----- Completion: wikilinks and `/` commands -----

    // ----- Mouse -----

    // ----- Clipboard -----

    // ----- UTF-16 (input methods) -----

    // ----- Layout and rendering -----

}

#[cfg(test)]
mod tests {
    use super::*;

    /// A vault image is reached through `..` (sibling folder); nothing leaves the vault.
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
        // A symbolic link from the vault to the outside is not followed.
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
        // Only the last state is kept in full.
        assert!(history.earlier.iter().all(|(_, _, own, _)| own.len() <= 2));
        for (i, state) in states.iter().enumerate().rev() {
            assert_eq!(history.pop(), Some((state.to_string(), i..i)));
        }
        assert_eq!(history.pop(), None);
    }

    #[test]
    fn tables_get_room_by_setting() {
        let (pane, text) = (px(1000.), px(720.));
        // In the column: nothing changes. Centered: the whole window minus the margins.
        assert_eq!(table_room(Wide::Column, pane, text), text);
        assert_eq!(table_room(Wide::Centered, pane, text), px(952.));
        // Narrow window: the column already fills the space, nothing overflows.
        let narrow = px(500.);
        assert_eq!(table_room(Wide::Centered, narrow, narrow - SIDE * 2.), narrow - SIDE * 2.);
        // A centered table overflows by half its excess on each side; never backwards.
        assert_eq!(table_lead(Wide::Centered, px(952.), text), px(116.));
        assert_eq!(table_lead(Wide::Centered, px(300.), text), px(0.));
        assert_eq!(table_lead(Wide::Column, px(952.), text), px(0.));
    }

}
