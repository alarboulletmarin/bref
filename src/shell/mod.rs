//! The window: it owns the panes and the views, routes their events, runs the commands, and
//! keeps the vault, the settings and the history. One `impl Shell` per subject, in the files of
//! this folder.

mod board;
mod capture;
mod diagrams;
mod drops;
mod files;
mod help;
mod keymap;
mod library;
mod listing;
mod menu;
mod notes;
mod palette_open;
mod pane;
mod render;
mod search;
mod settings;
mod sidebar;
mod sync;
mod tables;
mod tabs;
mod templates;
mod toasts;
mod travel;
mod updates;

use capture::*;
use drops::*;
use help::*;
use keymap::*;
use pane::*;
use toasts::*;

use crate::actions::*;
use crate::ui::keys::{self, MOD};
use crate::lang::{self, date_name, time_name, tr};
use crate::logic::{self, commands, diagram, embed, history, import, layout::{self, Node, Side}, markdown, prefs::Prefs, recall, table};
use crate::platform::{self, clock::{clock, today}, git, update, vault};
use crate::ui::{self,
    assets::{Assets, FAMILIES, init_fonts, apply_fonts, logo, mono, sans},
    figure, line,
    theme::{Page, THEMES, Theme, Wide},
    widgets::{drag_window, icon_button, keycaps},
};
use crate::views::{canvas, editor, field, graph, kanban, nav, palette, sheet, viewer};

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

use gpui::{
    Animation, AnimationExt, App, Application, Bounds, BoxShadow, ClipboardItem, Context,
    CursorStyle, Decorations, Div, Entity, FocusHandle, Focusable, KeyBinding, MouseButton,
    MouseMoveEvent, ObjectFit, Stateful, PathPromptOptions, Pixels, Point, ResizeEdge, Size, TitlebarOptions,
    Transformation,
    Window, WindowBounds, WindowBackgroundAppearance, WindowDecorations,
    WindowOptions, div, ease_out_quint, hsla, img, percentage, point, prelude::*, px, size, svg,
};

use canvas::{Canvas, CanvasEvent};
use diagram::Diagram;
use editor::{Editor, EditorEvent};
use graph::{Graph, GraphEvent};
use history::{History, Place, Shown};
use markdown::Link;
use nav::{Mode, Nav, Panel};
use palette::{Entry, Palette, PaletteEvent, Setting, TemplateAct};
use sheet::{Pick, Sheet, SheetEvent};
use table::{Encoding, Format, Style};
use logic::note::Note;

/// The icon grid: where it opens, for which item, the cell the keys are on (0: no icon), and what
/// had the focus before it.
struct IconPick {
    at: Point<Pixels>,
    path: PathBuf,
    chosen: usize,
    back: Option<FocusHandle>,
}

struct Shell {
    /// Every tab of the window, shown or not; `active` is the one that last got focus.
    panes: Vec<Pane>,
    active: usize,
    /// The panes as the user sees them: one, or two side by side, each with its tabs.
    groups: Vec<Group>,
    /// Files of the tabs closed, to reopen them (most recent last).
    closed: Vec<PathBuf>,
    /// The tab under the pointer: it shows its cross.
    tab_hover: Option<usize>,
    /// How the panes (`groups`, by index) are arranged: rows and columns with their shares.
    layout: Node,
    /// The border being dragged: the split (its path in `layout`) and the child it follows.
    pane_drag: Option<(Vec<usize>, usize)>,
    /// The pane on the whole window; the others keep what they hold, out of sight.
    zoomed: Option<usize>,
    /// Space the panes take, recorded when they are drawn.
    pane_box: std::rc::Rc<std::cell::Cell<Bounds<Pixels>>>,
    /// While a file from the tree is dragged: the hovered pane, and whether it is its right
    /// half (single pane: dropping the file there opens the second).
    pane_aim: Option<(usize, bool)>,
    /// System files hover over the window: where they would go.
    drop_aim: Option<DropAim>,
    /// Test: the attachments the system was asked to open.
    #[cfg(test)]
    opened: Vec<PathBuf>,
    /// Test: keep the drop label without a real drag in progress.
    #[cfg(test)]
    keep_aim: bool,
    /// The vault the config points to when it is no longer there: the welcome page says so, until a vault is opened.
    lost: Option<PathBuf>,
    /// Test: the window buttons are drawn by the app, as on Linux, whatever the test platform says.
    #[cfg(test)]
    client: bool,
    /// The active pane changed without input following: the next render gives it focus.
    grab: bool,
    /// A git sync is in progress: only one at a time.
    syncing: bool,
    /// The last sync failed: the rail's sync button says so until one succeeds.
    sync_broken: bool,

    /// The pane arrangement as saved (file `panes`).
    panes_saved: String,
    focus: FocusHandle,
    nav: Nav,
    graph: Entity<Graph>,
    /// Notes or their links changed since the graph was last computed.
    graph_stale: bool,
    palette: Option<Entity<Palette>>,
    vault: Option<PathBuf>,
    notes: Vec<Note>,
    /// Vault folders, including those that contain no note.
    dirs: Vec<PathBuf>,
    /// Vault files that are not notes: images and tables (CSV, TSV).
    images: Vec<PathBuf>,
    /// Folders of the vault that could not be read at the last scan, and why; the root itself
    /// when the whole vault cannot be read.
    unreadable: vault::Unreadable,
    /// Tree context menu, if open.
    menu: Option<nav::Menu>,
    /// The open menu has the keys: arrows, Enter, Escape.
    menu_focus: FocusHandle,
    /// Open notes, most recent first.
    recent: Vec<PathBuf>,
    save_gen: usize,
    /// Last replace across the vault, to undo it: each rewritten note, its text
    /// before and after.
    swapped: Vec<(PathBuf, Arc<str>, String)>,
    /// The vault search of the panel, once opened.
    vault_find: Option<search::VaultFind>,
    /// Icons of the vault's notes, files and folders (see `vault::load_icons`).
    icons: std::collections::HashMap<PathBuf, String>,
    /// Icon picker grid, if open.
    icon_pick: Option<IconPick>,
    /// The comments panel was closed: it waits to be asked for again.
    comments_shut: bool,
    /// Status messages shown, oldest to newest.
    toasts: Vec<Toast>,
    /// Number of the last message, to remove the right one when it expires.
    toasted: usize,
    /// A starting point asked for from the library of a canvas, to open when the window is drawn.
    template: Option<usize>,
    /// Newer version than the running one, found on GitHub.
    update: Option<update::Release>,
    /// Its install is in progress.
    updating: bool,
    title: String,
    prefs: Prefs,
    theme: Theme,
    /// Hovered window edge (resizing without system decorations).
    edge: Option<ResizeEdge>,
    copied: bool,
    /// The help panel, open or not.
    help: Help,
    /// What was shown, for "back" and "forward".
    history: History,
    /// The bindings in effect, and what the user changed in the default shortcuts.
    bound: Vec<keys::Bind>,
    changes: keys::Changes,
}

/// The active pane is the one the palette, the tree and the shortcuts act on (`pane`,
/// `pane_mut`). What holds for every pane goes through `each_pane`, what targets one through
/// `in_pane`.
impl Shell {
    fn pane(&self) -> &Pane {
        &self.panes[self.active]
    }

    fn pane_mut(&mut self) -> &mut Pane {
        &mut self.panes[self.active]
    }
}

impl Shell {
    fn new(
        vault: Option<PathBuf>,
        recent: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let prefs = Prefs::parse(&vault::load_settings());
        apply_fonts(&prefs);
        let theme = Theme::of(&prefs, window.appearance());
        let pane = Self::new_pane(theme, window, cx);
        cx.observe_window_appearance(window, |this, window, cx| this.restyle(window, cx)).detach();
        cx.on_app_quit(|this, cx| {
            this.each_pane(|this| this.leave(cx));
            async {}
        })
        .detach();

        let nav = Nav::new(cx.focus_handle());
        // The graph shares the panel's focus: the same keys navigate it.
        let graph = cx.new(|_| Graph::new(nav.focus.clone(), theme));
        cx.subscribe_in(&graph, window, |this, _, event, window, cx| match event {
            GraphEvent::Select(path) => this.select_from_graph(path, window, cx),
            GraphEvent::Open(path) => this.open_from_nav(path, window, cx),
        })
        .detach();

        let mut this = Self {
            panes: vec![pane],
            active: 0,
            groups: vec![Group::of(0)],
            closed: Vec::new(),
            tab_hover: None,
            layout: Node::default(),
            pane_drag: None,
            zoomed: None,
            pane_box: Default::default(),
            pane_aim: None,
            drop_aim: None,
            #[cfg(test)]
            opened: Vec::new(),
            #[cfg(test)]
            keep_aim: false,
            lost: None,
            #[cfg(test)]
            client: false,
            grab: false,
            panes_saved: String::new(),
            syncing: false,
            sync_broken: false,

            focus: cx.focus_handle(),
            menu_focus: cx.focus_handle(),
            nav,
            graph,
            graph_stale: true,
            palette: None,
            vault: None,
            notes: Vec::new(),
            dirs: Vec::new(),
            images: Vec::new(),
            unreadable: Vec::new(),
            menu: None,
            recent: Vec::new(),
            save_gen: 0,
            swapped: Vec::new(),
            vault_find: None,
            icons: Default::default(),
            icon_pick: None,
            comments_shut: false,
            toasts: Vec::new(),
            toasted: 0,
            template: None,
            update: None,
            updating: false,
            title: String::new(),
            prefs,
            theme,
            edge: None,
            copied: false,
            help: Help::default(),
            history: History::default(),
            bound: defaults(),
            changes: keys::Changes::new(),
        };
        match vault {
            Some(root) => {
                this.set_vault(root.clone(), window, cx);
                this.recent = recent.into_iter().filter(|p| p.is_file()).collect();
                // The tabs of last time, those whose file is still there; the older formats left
                // the first pane to the most recent note.
                let (tree, saved) = read_panes(&vault::load_file("panes"));
                this.restore_panes(tree, saved, &root, window, cx);
                this.panes_saved = this.panes_state();
                if this.nav.panel == Panel::Full {
                    window.focus(&this.nav.focus);
                }
            }
            None => window.focus(&this.focus),
        }
        this.watch(cx);
        this.check_updates(cx);
        let weak = cx.weak_entity();
        this.help.key_tap = Some(cx.intercept_keystrokes(move |event, _, cx| {
            if weak.update(cx, |this, cx| this.key_pressed(&event.keystroke, cx)).unwrap_or(false) {
                cx.stop_propagation();
            }
        }));
        // Changed shortcuts; a faulty line of the file is dropped and reported.
        let open = editable();
        let (changes, faults) = keys::read(&vault::load_file("keys"), &this.bound, &|action| open.contains(action));
        if !changes.is_empty() {
            this.changes = changes;
            this.rebind(cx);
        }
        if let Some(first) = faults.first() {
            let more = if faults.len() > 1 { format!(" (+{})", faults.len() - 1) } else { String::new() };
            this.say(Tone::Failed, format!("{} {first}{more}", tr("Shortcut ignored in the keys file:", "Raccourci ignoré dans le fichier keys :")), cx);
        }
        this
    }

}

/// Opens the main window, or the quick-capture window with `--capture`.
pub fn run() {
    #[cfg(target_os = "linux")]
    platform::skip_absent_nvidia_driver();
    Application::new().with_assets(Assets).run(|cx: &mut App| {
        bind_keys(cx);
        init_fonts(cx);
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.on_window_closed(|cx| cx.quit()).detach();

        let (vault, recent) = vault::load_config();
        let lost = vault.clone().filter(|v| !v.is_dir());
        let vault = vault.filter(|v| v.is_dir());
        // `bref --capture`: the input line alone, in a small undecorated window.
        // Without a chosen vault, the app opens as usual to ask for one.
        if let Some(root) = vault.clone().filter(|_| std::env::args().any(|arg| arg == "--capture")) {
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(560.), px(96.)), cx))),
                    titlebar: None,
                    app_id: Some("dev.andrea.Bref".into()),
                    window_decorations: Some(WindowDecorations::Client),
                    window_background: WindowBackgroundAppearance::Transparent,
                    is_resizable: false,
                    ..Default::default()
                },
                |window, cx| cx.new(|cx| Capture::new(root, window, cx)),
            )
            .unwrap();
            return cx.activate(true);
        }
        let bounds = Bounds::centered(None, size(px(860.), px(720.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("Bref".into()),
                    ..Default::default()
                }),
                // Must match the .desktop file name for the desktop to associate the icon.
                app_id: Some("dev.andrea.Bref".into()),
                // On Linux the app draws its title bar. Asking for
                // `Server` does not work on GNOME/Wayland, which provides none
                // while gpui 0.2 still believes it is decorated. macOS and Windows
                // ignore the request and keep their native bar.
                window_decorations: Some(WindowDecorations::Client),
                window_background: WindowBackgroundAppearance::Transparent,
                window_min_size: Some(size(px(360.), px(240.))),
                ..Default::default()
            },
            |window, cx| {
                cx.new(|cx| {
                    let mut shell = Shell::new(vault, recent, window, cx);
                    shell.lost = lost;
                    shell
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}

#[cfg(test)]
mod tests;
