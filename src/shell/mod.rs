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
use crate::logic::{self, commands, diagram, history, import, markdown, prefs::Prefs, recall, table};
use crate::platform::{self, clock::{clock, today}, git, update, vault};
use crate::ui::{self,
    assets::{Assets, FAMILIES, init_fonts, apply_fonts, logo, mono, sans},
    figure, line,
    theme::{Page, THEMES, Theme, Wide},
    widgets::{drag_window, icon_button},
};
use crate::views::{canvas, editor, graph, kanban, nav, palette, sheet, viewer};

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

use gpui::{
    Animation, AnimationExt, App, Application, Bounds, BoxShadow, ClipboardItem, Context,
    CursorStyle, Decorations, Div, Entity, FocusHandle, Focusable, KeyBinding, MouseButton,
    MouseMoveEvent, ObjectFit, PathPromptOptions, Pixels, Point, ResizeEdge, Size, TitlebarOptions,
    Transformation,
    Window, WindowBounds, WindowBackgroundAppearance, WindowDecorations,
    WindowOptions, div, ease_out_quint, hsla, img, percentage, point, prelude::*, px, rgb, size, svg,
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

struct Shell {
    /// Un pane, ou deux côte à côte ; `active` est celui qui a reçu le focus en dernier.
    panes: Vec<Pane>,
    active: usize,
    /// Part de la largeur que prend le pane de gauche quand il y en a deux.
    pane_ratio: f32,
    /// La bordure entre les deux panes suit le pointeur.
    pane_drag: bool,
    /// Place qu'occupent les panes, relevée à leur dessin.
    pane_box: std::rc::Rc<std::cell::Cell<Bounds<Pixels>>>,
    /// Pendant qu'on glisse un fichier de l'arbre : le pane survolé, et s'il s'agit de sa moitié
    /// droite (un seul pane : y lâcher le fichier ouvre le second).
    pane_aim: Option<(usize, bool)>,
    /// Des fichiers du système survolent la fenêtre : où ils se rangeraient.
    drop_aim: Option<DropAim>,
    /// Test : les pièces jointes qu'on a demandé au système d'ouvrir.
    #[cfg(test)]
    opened: Vec<PathBuf>,
    /// Test : garder l'étiquette de dépôt sans vrai glisser en cours.
    #[cfg(test)]
    keep_aim: bool,
    /// Le pane actif a changé sans que la saisie suive : le prochain rendu la lui donne.
    grab: bool,
    /// Une synchronisation git est en cours : une seule à la fois.
    syncing: bool,
    /// La disposition des panes telle qu'elle est enregistrée (fichier `panes`).
    panes_saved: String,
    focus: FocusHandle,
    nav: Nav,
    graph: Entity<Graph>,
    /// Les notes ou leurs liens ont changé depuis le dernier calcul du graphe.
    graph_stale: bool,
    palette: Option<Entity<Palette>>,
    vault: Option<PathBuf>,
    notes: Vec<Note>,
    /// Dossiers du coffre, y compris ceux qui ne contiennent aucune note.
    dirs: Vec<PathBuf>,
    /// Fichiers du coffre qui ne sont pas des notes : images et tableaux (CSV, TSV).
    images: Vec<PathBuf>,
    /// Menu contextuel de l'arbre, s'il est ouvert.
    menu: Option<nav::Menu>,
    /// Notes ouvertes, de la plus récente à la plus ancienne.
    recent: Vec<PathBuf>,
    save_gen: usize,
    /// Dernier remplacement dans le coffre, pour le défaire : chaque note réécrite, son texte
    /// d'avant et celui d'après.
    swapped: Vec<(PathBuf, Arc<str>, String)>,
    /// Icônes des notes, fichiers et dossiers du coffre (voir `vault::load_icons`).
    icons: std::collections::HashMap<PathBuf, String>,
    /// Grille de choix d'une icône : où elle s'ouvre, et pour quel élément.
    icon_pick: Option<(Point<Pixels>, PathBuf)>,
    /// Le panneau des commentaires a été refermé : il attend qu'on le redemande.
    comments_shut: bool,
    /// Messages d'état affichés, du plus ancien au plus récent.
    toasts: Vec<Toast>,
    /// Numéro du dernier message, pour retirer le bon à l'échéance.
    toasted: usize,
    /// Version plus récente que celle qui tourne, trouvée sur GitHub.
    update: Option<update::Release>,
    /// Son installation est en cours.
    updating: bool,
    title: String,
    prefs: Prefs,
    theme: Theme,
    /// Bord de fenêtre survolé (redimensionnement sans décorations système).
    edge: Option<ResizeEdge>,
    copied: bool,
    /// The help panel, open or not.
    help: Help,
    /// Ce qui a été affiché, pour « précédent » et « suivant ».
    history: History,
    /// Les liaisons en vigueur, et ce que l'utilisateur a changé aux raccourcis par défaut.
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
        // Le graphe partage le focus du panneau : les mêmes touches y naviguent.
        let graph = cx.new(|_| Graph::new(nav.focus.clone(), theme));
        cx.subscribe_in(&graph, window, |this, _, event, window, cx| match event {
            GraphEvent::Select(path) => this.select_from_graph(path, cx),
            GraphEvent::Open(path) => this.open_from_nav(path, window, cx),
        })
        .detach();

        let mut this = Self {
            panes: vec![pane],
            active: 0,
            pane_ratio: 0.5,
            pane_drag: false,
            pane_box: Default::default(),
            pane_aim: None,
            drop_aim: None,
            #[cfg(test)]
            opened: Vec::new(),
            #[cfg(test)]
            keep_aim: false,
            grab: false,
            panes_saved: String::new(),
            syncing: false,
            focus: cx.focus_handle(),
            nav,
            graph,
            graph_stale: true,
            palette: None,
            vault: None,
            notes: Vec::new(),
            dirs: Vec::new(),
            images: Vec::new(),
            menu: None,
            recent: Vec::new(),
            save_gen: 0,
            swapped: Vec::new(),
            icons: Default::default(),
            icon_pick: None,
            comments_shut: false,
            toasts: Vec::new(),
            toasted: 0,
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
                // Le second pane d'avant, s'il y en avait un et que son fichier existe encore.
                let saved = vault::load_file("panes");
                let mut lines = saved.lines();
                let ratio = lines.next().and_then(|ratio| ratio.parse::<f32>().ok()).filter(|ratio| PANE_RATIO.contains(ratio));
                this.pane_ratio = ratio.unwrap_or(0.5);
                let aside = lines.next().map(PathBuf::from).filter(|path| path.is_file() && path.starts_with(&root));
                if let Some(last) = this.recent.iter().find(|path| Some(*path) != aside.as_ref()).cloned() {
                    this.open_note(&last, cx);
                }
                if let Some(aside) = aside {
                    this.open_aside(&aside, window, cx);
                    this.focus_pane(0, window, cx);
                }
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
        // Les raccourcis changés ; une ligne fautive du fichier est écartée et signalée.
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
        let vault = vault.filter(|v| v.is_dir());
        // `bref --capture` : la seule ligne de saisie, dans une petite fenêtre sans décor.
        // Sans coffre choisi, l'app s'ouvre comme d'habitude pour en demander un.
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
                // Doit correspondre au nom du fichier .desktop pour que le bureau associe l'icône.
                app_id: Some("dev.andrea.Bref".into()),
                // Comme Zed : sous Linux l'app dessine sa barre de titre. Demander
                // `Server` ne marche pas sous GNOME/Wayland, qui n'en fournit pas
                // alors que gpui 0.2 se croit quand même décoré. macOS et Windows
                // ignorent la demande et gardent leur barre native.
                window_decorations: Some(WindowDecorations::Client),
                window_background: WindowBackgroundAppearance::Transparent,
                window_min_size: Some(size(px(360.), px(240.))),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| Shell::new(vault, recent, window, cx)),
        )
        .unwrap();
        cx.activate(true);
    });
}

#[cfg(test)]
mod tests;
