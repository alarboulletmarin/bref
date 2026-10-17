// Pas de console derrière la fenêtre sous Windows.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod keys;
mod lang;
mod logic;
mod platform;
mod ui;
mod views;

use lang::{date_name, time_name, tr};
use logic::{commands, diagram, history, import, markdown, prefs::Prefs, recall, table};
use platform::{clock::{clock, today}, git, update, vault};
use ui::{
    assets::{Assets, FAMILIES, init_fonts, apply_fonts, logo, mono, sans},
    figure, line,
    theme::{Page, THEMES, Theme, Wide},
    widgets::{drag_window, icon_button},
};
use views::{canvas, editor, graph, kanban, nav, palette, sheet, viewer};

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

use gpui::{
    Animation, AnimationExt, App, Application, Bounds, BoxShadow, ClipboardItem, Context,
    CursorStyle, Decorations, Entity, FocusHandle, Focusable, KeyBinding, MouseButton,
    MouseMoveEvent, ObjectFit, PathPromptOptions, Pixels, Point, ResizeEdge, Size, TitlebarOptions,
    Transformation,
    Window, WindowBounds, WindowBackgroundAppearance, WindowDecorations,
    WindowOptions, actions, div, ease_out_quint, hsla, img, percentage, point, prelude::*, px, rgb, size, svg,
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

/// Touche des raccourcis, telle qu'affichée : Cmd sur macOS, Ctrl ailleurs.
pub const MOD: &str = if cfg!(target_os = "macos") { "Cmd" } else { "Ctrl" };

/// Le nombre de la pastille des commentaires : « 9+ » au-delà de neuf, pour qu'elle reste ronde.
fn badge_label(count: usize) -> String {
    if count > 9 { "9+".into() } else { count.to_string() }
}

/// Marge transparente autour de la fenêtre quand l'app dessine ses décorations.
const SHADOW: Pixels = px(10.);

actions!(app, [ToggleCommands, SyncVault, SplitPane, FocusLeft, FocusRight, ClosePane, GoBack, GoForward, SearchVault, Today, Outline, OpenPalette, NewNote, NewFromTemplate, NewDiagram, OpenVault, CopyAll, ToggleHelp, CloseHelp, ChooseTheme, ZoomIn, ZoomOut, ZoomReset, Quit]);

/// Ton d'un message d'état : il choisit son icône, et s'il part de lui-même.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tone {
    Info,
    Done,
    Failed,
    /// Opération en cours : reste affiché jusqu'au message qui en donne l'issue.
    Busy,
}

/// Où des fichiers venus du système se rangeraient, pendant qu'on les glisse sur la fenêtre.
#[derive(Clone, PartialEq, Debug)]
pub enum Aim {
    /// Dans la note du pane, entre deux lignes (l'octet où elle commence) ; `version` : le texte vu alors.
    Note { pane: usize, at: usize, version: u64 },
    /// Dans un dossier du coffre.
    Folder(PathBuf),
    /// À la racine du coffre.
    Root,
}

/// Ce qu'on glisse, et ce que le dépôt ferait : de quoi montrer le choix sous le pointeur.
#[derive(Clone, PartialEq, Debug)]
pub struct DropAim {
    pub aim: Aim,
    pub items: usize,
    pub pointer: Point<Pixels>,
}

struct Toast {
    id: usize,
    tone: Tone,
    text: String,
}

/// Durée d'affichage d'un message d'état.
const TOAST: Duration = Duration::from_secs(4);

/// Ce qui empêche des touches de devenir un raccourci telles quelles.
#[derive(Clone, Debug, PartialEq)]
pub enum KeyIssue {
    /// Déjà prises par ces actions : à remplacer, ou à laisser.
    Taken(Vec<&'static str>),
    Refused(&'static str),
}

/// Un raccourci attendu dans le panneau d'aide : la prochaine frappe le devient.
struct KeyEdit {
    action: &'static str,
    keys: String,
    issue: Option<KeyIssue>,
}

/// Largeur en dessous de laquelle deux panes ne tiennent pas à côté du panneau : il se replie.
const MIN_PANE: Pixels = px(300.);
/// Part de la largeur que peut prendre le pane de gauche.
const PANE_RATIO: std::ops::RangeInclusive<f32> = 0.2..=0.8;

/// Ce qui est ouvert dans une moitié de la fenêtre : une note et son éditeur, ce qui s'affiche
/// à sa place (image, schéma, tableau, page liste), et la vue kanban de la note.
struct Pane {
    /// Contient le focus dès qu'une des vues du pane l'a : c'est ce qui le rend actif. Il le
    /// porte lui-même quand le pane ne montre qu'une image.
    zone: FocusHandle,
    editor: Entity<Editor>,
    /// Image affichée à la place de la note, choisie dans l'arbre ou le graphe.
    picture: Option<PathBuf>,
    /// Schéma ouvert dans son canevas, quand l'image affichée en est un.
    drawing: Option<(PathBuf, Entity<Canvas>)>,
    /// SVG venu d'ailleurs, ouvert dans la visionneuse.
    viewer: Option<(PathBuf, Entity<viewer::Viewer>)>,
    /// Diagramme Mermaid de la note, vu en grand à sa place.
    figure: Option<Entity<viewer::Viewer>>,
    /// Tableau ouvert dans sa grille, quand le fichier affiché en est un.
    sheet: Option<(PathBuf, Entity<Sheet>)>,
    /// Fichier de la note ouverte ; `None` tant qu'une nouvelle note n'est pas enregistrée.
    path: Option<PathBuf>,
    /// Le nom du fichier suit le titre (première ligne) de la note.
    synced: bool,
    /// Titre `# …` de la note quand elle a été chargée : s'il change, ou s'il
    /// apparaît, le fichier prend son nom.
    h1: Option<String>,
    /// Nom de la note quand elle a été chargée ; `None` pour une nouvelle note.
    /// Si son titre change, les liens vers ce nom suivent quand on la quitte.
    origin: Option<String>,
    /// La note affichée n'est qu'un aperçu : elle ne compte comme ouverte que
    /// lorsqu'on y entre (focus ou frappe).
    preview: bool,
    /// Dossier où enregistrer la nouvelle note ; le coffre par défaut.
    new_dir: Option<PathBuf>,
    /// La note a été créée dans ce pane : laissée vide, son fichier ne reste pas dans le coffre.
    fresh: bool,
    dirty: bool,
    /// La note ouverte est un tableau kanban dont on regarde le texte : on l'a demandé, ou on
    /// vient d'en écrire la clé (taper `kanban: true` ne fait pas quitter le texte).
    board_text: bool,
    /// La note était un tableau à sa dernière frappe.
    board_was: bool,
    /// Carte qu'on déplace dans le tableau, et où elle se poserait (colonne, rang).
    board_drag: Option<((usize, usize), (usize, usize))>,
    /// Largeur d'une carte du tableau, relevée à son dessin.
    board_card: std::rc::Rc<std::cell::Cell<Pixels>>,
    board_scroll: gpui::ScrollHandle,
    /// Ce qu'on écrit dans le tableau, et son champ de saisie.
    board_field: Option<(kanban::Slot, Entity<kanban::Field>)>,
    /// Le tableau reçoit la saisie à la place de la note qu'il cache.
    board_focus: FocusHandle,
    /// Page liste affichée à la place de la note.
    listing: Option<nav::Listing>,
    /// Ce qui est affiché n'est qu'un aperçu (flèches dans l'arbre, graphe) : l'historique
    /// l'ignore tant qu'il n'est pas ouvert pour de bon.
    peek: bool,
}

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
    help: bool,
    /// Ce qui a été affiché, pour « précédent » et « suivant ».
    history: History,
    /// Les liaisons en vigueur, et ce que l'utilisateur a changé aux raccourcis par défaut.
    bound: Vec<keys::Bind>,
    changes: keys::Changes,
    /// Champ de recherche du panneau d'aide, tant qu'il est ouvert.
    help_find: Option<Entity<kanban::Field>>,
    /// La ligne de l'aide choisie au clavier, et le raccourci en cours de saisie.
    help_sel: Option<&'static str>,
    help_edit: Option<KeyEdit>,
    /// Reçoit chaque frappe avant les raccourcis, pour celle qu'on attend.
    key_tap: Option<gpui::Subscription>,
}


/// `Shell` se lit comme son pane actif : `self.path`, `self.editor`… sont ceux du pane sur lequel
/// la palette, l'arbre et les raccourcis agissent. Ce qui vaut pour tous les panes passe par
/// `each_pane`, ce qui vise un pane précis par `in_pane`.
impl std::ops::Deref for Shell {
    type Target = Pane;

    fn deref(&self) -> &Pane {
        &self.panes[self.active]
    }
}

impl std::ops::DerefMut for Shell {
    fn deref_mut(&mut self) -> &mut Pane {
        &mut self.panes[self.active]
    }
}

impl Shell {
    /// Un pane vide : une note neuve dans son éditeur.
    fn new_pane(theme: Theme, window: &mut Window, cx: &mut Context<Self>) -> Pane {
        let editor = cx.new(|cx| Editor::new(theme, cx));
        cx.subscribe_in(&editor, window, Self::on_editor_event).detach();
        Pane {
            zone: cx.focus_handle(),
            editor,
            picture: None,
            drawing: None,
            viewer: None,
            figure: None,
            sheet: None,
            path: None,
            synced: true,
            h1: None,
            origin: None,
            preview: false,
            new_dir: None,
            fresh: false,
            dirty: false,
            board_text: false,
            board_was: false,
            board_drag: None,
            board_card: Default::default(),
            board_scroll: Default::default(),
            board_field: None,
            board_focus: cx.focus_handle(),
            listing: None,
            peek: false,
        }
    }

    /// Demande une ligne de texte libre (une adresse) dans la palette, puis la passe à `then`.
    fn ask_text(&mut self, label: &'static str, window: &mut Window, cx: &mut Context<Self>, then: fn(&mut Self, String, &mut Window, &mut Context<Self>)) {
        let theme = self.theme;
        let palette = cx.new(|cx| Palette::prompt(label, "", theme, cx));
        cx.subscribe_in(&palette, window, move |this, _, event, window, cx| {
            this.palette = None;
            match this.vault {
                Some(_) => window.focus(&this.editor.focus_handle(cx)),
                None => window.focus(&this.focus),
            }
            if let PaletteEvent::Submit(text) = event
                && !text.trim().is_empty()
            {
                then(this, text.trim().to_string(), window, cx);
            }
            cx.notify();
        })
        .detach();
        window.focus(&palette.focus_handle(cx));
        self.palette = Some(palette);
        cx.notify();
    }

    /// Synchronise le coffre en une action : tout commiter, recevoir, fusionner, envoyer. Le
    /// réseau travaille en tâche de fond, la saisie reste libre ; un coffre sans dépôt relié
    /// demande d'abord l'adresse du dépôt.
    fn git_sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = self.vault.clone() else { return };
        if self.syncing {
            return;
        }
        self.flush(cx);
        match git::linked(&root) {
            Ok(true) => {}
            Ok(false) => {
                let label = tr("Address of an empty git repository", "Adresse d'un dépôt git vide");
                return self.ask_text(label, window, cx, |this, url, _, cx| this.git_connect(url, cx));
            }
            Err(e) => return self.sync_failed(&e, cx),
        }
        if let Err(e) = git::save(&root) {
            return self.sync_failed(&e, cx);
        }
        self.syncing = true;
        self.say(Tone::Busy, tr("Syncing the vault…", "Synchronisation du coffre…"), cx);
        let (date, host) = (date_name(today()), git::host());
        cx.spawn(async move |this, cx| {
            let (mut received, mut conflicts, mut sent) = (0, 0, 0);
            // Refusé parce qu'une autre machine a envoyé entre-temps : recevoir de nouveau,
            // trois fois au plus.
            let busy = tr("the server keeps receiving from another machine: try again.", "le serveur reçoit sans cesse d'une autre machine : réessaie.");
            let mut outcome = Err(git::Fail::Told(busy.to_string()));
            for _ in 0..3 {
                let fetch_root = root.clone();
                if let Err(e) = cx.background_executor().spawn(async move { git::fetch(&fetch_root) }).await {
                    outcome = Err(e);
                    break;
                }
                // La partie locale se fait d'une traite sur ce fil : rien ne peut s'intercaler
                // entre l'enregistrement de ce qui vient d'être tapé et la fusion, donc rien de
                // tapé n'est perdu et rien de reçu n'est écrasé.
                // ponytail: l'interface est figée le temps de la fusion (quelques dixièmes de
                // seconde) ; la passer en tâche de fond avec la saisie retenue si un très gros
                // coffre la rend sensible.
                let merged = this.update(cx, |this, cx| {
                    this.flush(cx);
                    let merged = git::merge(&root, &date, &host);
                    this.rescan(&root, cx);
                    merged
                });
                let merged = match merged {
                    Ok(Ok(merged)) => merged,
                    Ok(Err(e)) => {
                        outcome = Err(e);
                        break;
                    }
                    Err(_) => return,
                };
                (received, conflicts, sent) = (received + merged.received, conflicts + merged.conflicts.len(), merged.sent);
                if !merged.ahead {
                    outcome = Ok(());
                    break;
                }
                let push_root = root.clone();
                match cx.background_executor().spawn(async move { git::push(&push_root) }).await {
                    Ok(git::Pushed::Done) => {
                        outcome = Ok(());
                        break;
                    }
                    Ok(git::Pushed::Rejected) => {}
                    Err(e) => {
                        outcome = Err(e);
                        break;
                    }
                }
            }
            this.update(cx, |this, cx| {
                this.syncing = false;
                match outcome {
                    Err(e) => this.sync_failed(&e, cx),
                    Ok(()) if sent + received + conflicts == 0 => this.say(Tone::Done, tr("Already up to date", "Déjà à jour"), cx),
                    Ok(()) => {
                        let mut told = format!(
                            "{} {sent} {}, {received} {}",
                            tr("Synced:", "Synchronisé :"),
                            if sent > 1 { tr("sent", "envoyés") } else { tr("sent", "envoyé") },
                            if received > 1 { tr("received", "reçus") } else { tr("received", "reçu") },
                        );
                        if conflicts > 0 {
                            let kept = if conflicts > 1 {
                                tr("conflicts: both versions are kept, the other ones as « (conflict …) » notes", "conflits : les deux versions sont gardées, celles d'en face dans des notes « (conflict …) »")
                            } else {
                                tr("conflict: both versions are kept, the other one as a « (conflict …) » note", "conflit : les deux versions sont gardées, celle d'en face dans une note « (conflict …) »")
                            };
                            told.push_str(&format!(" · {conflicts} {kept}"));
                        }
                        this.say(Tone::Done, told, cx)
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    fn sync_failed(&mut self, why: &git::Fail, cx: &mut Context<Self>) {
        self.syncing = false;
        self.say(Tone::Failed, format!("{} : {}", tr("Sync failed", "Synchronisation impossible"), why.text()), cx);
    }

    /// Relit le coffre tout de suite, sans attendre la surveillance : ce que la fusion a écrit
    /// s'affiche, chaque note gardant son curseur et son défilement.
    fn rescan(&mut self, root: &Path, cx: &mut Context<Self>) {
        let views: Vec<(usize, f32)> = self.panes.iter().map(|pane| pane.editor.read(cx).view()).collect();
        let (notes, dirs, images) = vault::rescan(root, &self.notes);
        self.sync(notes, dirs, images, cx);
        for (pane, (at, scroll)) in self.panes.iter().zip(views) {
            if pane.editor.read(cx).view().0 != at {
                pane.editor.update(cx, |editor, cx| editor.set_view(at, scroll, cx));
            }
        }
    }

    /// Relie le coffre à un dépôt distant vide, et y envoie ce qu'il contient.
    fn git_connect(&mut self, url: String, cx: &mut Context<Self>) {
        let Some(root) = self.vault.clone() else { return };
        if self.syncing {
            return;
        }
        self.flush(cx);
        self.syncing = true;
        self.say(Tone::Busy, tr("Connecting the vault…", "Liaison du coffre…"), cx);
        cx.spawn(async move |this, cx| {
            let done = cx.background_executor().spawn(async move { git::connect(&root, &url) }).await;
            this.update(cx, |this, cx| {
                this.syncing = false;
                match done {
                    Ok(()) => this.say(Tone::Done, tr("Vault connected: it syncs with this repository from now on", "Coffre relié : il se synchronise désormais avec ce dépôt"), cx),
                    Err(e) => this.say(Tone::Failed, format!("{} : {}", tr("Not connected", "Liaison impossible"), e.text()), cx),
                }
            })
            .ok();
        })
        .detach();
    }

    /// « Cloner un coffre » : l'adresse du dépôt, puis le dossier où le poser.
    fn ask_clone(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let label = tr("Address of the git repository to clone", "Adresse du dépôt git à cloner");
        self.ask_text(label, window, cx, |_, url, window, cx| {
            let paths = cx.prompt_for_paths(PathPromptOptions {
                files: false,
                directories: true,
                multiple: false,
                prompt: Some(tr("Clone into this folder", "Cloner dans ce dossier").into()),
            });
            cx.spawn_in(window, async move |this, cx| {
                if let Ok(Ok(Some(paths))) = paths.await
                    && let Some(parent) = paths.into_iter().next()
                {
                    this.update_in(cx, |this, window, cx| this.clone_to(url, parent, window, cx)).ok();
                }
            })
            .detach();
        });
    }

    /// Clone le dépôt dans un nouveau dossier de `parent`, qui devient le coffre ouvert.
    fn clone_to(&mut self, url: String, parent: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let into = parent.join(git::folder_of(&url));
        if into.exists() {
            let taken = tr("Not cloned: this folder already exists:", "Clonage impossible : ce dossier existe déjà :");
            return self.say(Tone::Failed, format!("{taken} {}", into.display()), cx);
        }
        self.say(Tone::Busy, tr("Cloning the vault…", "Clonage du coffre…"), cx);
        cx.spawn_in(window, async move |this, cx| {
            let target = into.clone();
            let done = cx.background_executor().spawn(async move { git::clone(&url, &target) }).await;
            this.update_in(cx, |this, window, cx| match done {
                Ok(()) => {
                    this.set_vault(into, window, cx);
                    this.say(Tone::Done, tr("Vault cloned", "Coffre cloné"), cx)
                }
                Err(e) => this.say(Tone::Failed, format!("{} : {}", tr("Not cloned", "Clonage impossible"), e.text()), cx),
            })
            .ok();
        })
        .detach();
    }

    /// Le pane qui montre déjà ce fichier, comme note (même cachée par une image) ou à sa place.
    fn pane_showing(&self, path: &Path) -> Option<usize> {
        self.panes.iter().position(|pane| pane.path.as_deref() == Some(path) || pane.picture.as_deref() == Some(path))
    }

    /// Donne la saisie au pane : à sa grille, à sa liste, à sa note, sinon au pane lui-même.
    fn focus_pane(&mut self, pane: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.active = pane.min(self.panes.len() - 1);
        let shown = self.picture.is_some();
        if let Some(figure) = &self.figure {
            window.focus(&figure.focus_handle(cx));
            return cx.notify();
        }
        match (self.shown_sheet(), &self.listing, &self.drawing) {
            (Some(sheet), ..) => window.focus(&sheet.focus_handle(cx)),
            (None, Some(listing), _) => window.focus(&listing.filter.focus_handle(cx)),
            (None, None, Some((_, canvas))) if shown => window.focus(&canvas.focus_handle(cx)),
            (None, None, None) if shown && let Some((_, viewer)) = &self.viewer => window.focus(&viewer.focus_handle(cx)),
            (None, None, _) if shown => window.focus(&self.zone),
            (None, None, _) => window.focus(&self.editor.focus_handle(cx)),
        }
        cx.notify();
    }

    /// Ouvre le second pane, sur une note neuve, et lui donne la saisie.
    fn add_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Trop étroit pour deux panes : le panneau se replie sur son rail.
        let width = self.pane_box.get().size.width;
        if self.nav.panel == Panel::Split && width > px(0.) && width < MIN_PANE * 2. {
            self.nav.panel = Panel::Rail;
            self.nav.save();
        }
        if self.nav.panel == Panel::Full {
            self.nav.panel = Panel::Split;
        }
        let pane = Self::new_pane(self.theme, window, cx);
        self.panes.push(pane);
        self.active = self.panes.len() - 1;
        self.push_names(cx);
        self.push_dirs(cx);
        window.focus(&self.editor.focus_handle(cx));
        cx.notify();
    }

    /// Ctrl+\ : un second pane, ouvert sur la palette pour choisir ce qui y va. Une note ne
    /// s'affiche pas deux fois, il ne peut donc pas reprendre celle qu'on quitte.
    fn split(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.vault.is_none() {
            return;
        }
        if self.panes.len() > 1 {
            return self.focus_pane(1, window, cx);
        }
        self.add_pane(window, cx);
        self.open_palette("", window, cx);
    }

    /// Ouvre le fichier dans l'autre pane, créé au besoin ; déjà affiché, il reçoit la saisie.
    pub fn open_aside(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(pane) = self.pane_showing(path) {
            return self.focus_pane(pane, window, cx);
        }
        match self.panes.len() {
            1 => self.add_pane(window, cx),
            _ => self.active = 1 - self.active,
        }
        self.open_from_nav(path, window, cx);
    }

    /// Un fichier de l'arbre lâché sur un pane : il s'y ouvre (un dossier : sa page liste), ou
    /// dans un second pane s'il est lâché sur la moitié droite du seul pane.
    fn drop_on_pane(&mut self, pane: usize, aside: bool, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        if path.is_dir() {
            match aside {
                true => self.add_pane(window, cx),
                false => self.active = pane.min(self.panes.len() - 1),
            }
            self.open_listing(path.to_path_buf(), window, cx);
        } else if aside {
            self.open_aside(path, window, cx);
        } else {
            self.active = pane.min(self.panes.len() - 1);
            self.open_from_nav(path, window, cx);
        }
    }

    /// Ferme le pane qui a la saisie ; l'autre reprend toute la largeur. Le dernier reste.
    fn close_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.panes.len() < 2 {
            return;
        }
        self.leave(cx);
        self.panes.remove(self.active);
        self.focus_pane(0, window, cx);
    }

    /// La disposition des panes, telle qu'elle s'enregistre : la part du pane de gauche, puis le
    /// fichier du second pane (rien s'il n'y en a qu'un).
    fn panes_state(&self) -> String {
        let aside = self.panes.get(1).and_then(|pane| pane.picture.as_ref().or(pane.path.as_ref()));
        format!("{}\n{}\n", self.pane_ratio, aside.map(|path| path.display().to_string()).unwrap_or_default())
    }

    /// Fait `act` dans le pane `pane`, comme s'il était l'actif, puis revient à l'actif.
    fn in_pane<R>(&mut self, pane: usize, act: impl FnOnce(&mut Self) -> R) -> R {
        let active = std::mem::replace(&mut self.active, pane);
        let done = act(self);
        // Le pane a pu être fermé entre-temps.
        self.active = active.min(self.panes.len() - 1);
        done
    }

    /// Fait `act` dans chaque pane, tour à tour.
    fn each_pane(&mut self, mut act: impl FnMut(&mut Self)) {
        for pane in 0..self.panes.len() {
            self.in_pane(pane, &mut act);
        }
    }

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
            help: false,
            history: History::default(),
            bound: defaults(),
            changes: keys::Changes::new(),
            help_find: None,
            help_sel: None,
            help_edit: None,
            key_tap: None,
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
        this.key_tap = Some(cx.intercept_keystrokes(move |event, _, cx| {
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

    /// Cherche en tâche de fond une version plus récente (au plus une requête par jour).
    /// Les tests ne sortent pas sur le réseau.
    fn check_updates(&mut self, cx: &mut Context<Self>) {
        if !self.prefs.updates || cfg!(test) {
            return;
        }
        cx.spawn(async move |this, cx| {
            let found = cx.background_executor().spawn(async move { update::check() }).await;
            this.update(cx, |this, cx| {
                if this.prefs.updates && found.is_some() {
                    this.update = found;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Affiche un message d'état, qui part au clic ou après `TOAST`. Il remplace le message
    /// « en cours », dont il est l'issue.
    // ponytail: une seule opération « en cours » à la fois ; rendre le numéro du message à
    // l'appelant le jour où deux opérations longues peuvent se chevaucher.
    pub fn say(&mut self, tone: Tone, text: impl Into<String>, cx: &mut Context<Self>) {
        self.toasts.retain(|t| t.tone != Tone::Busy);
        self.toasted += 1;
        let id = self.toasted;
        self.toasts.push(Toast { id, tone, text: text.into() });
        if tone != Tone::Busy {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(TOAST).await;
                this.update(cx, |this, cx| {
                    this.toasts.retain(|t| t.id != id);
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
        cx.notify();
    }

    /// Une réussite retire les erreurs encore affichées.
    pub fn calm(&mut self) {
        self.toasts.retain(|t| t.tone != Tone::Failed);
    }

    /// Un message de ce ton contient `part`.
    #[cfg(test)]
    fn told(&self, tone: Tone, part: &str) -> bool {
        self.toasts.iter().any(|t| t.tone == tone && t.text.contains(part))
    }

    /// Contrôle demandé depuis la palette : interroge GitHub tout de suite, même si la recherche
    /// quotidienne est coupée, et dit ce qu'il en est. Les tests ne sortent pas sur le réseau.
    fn check_now(&mut self, cx: &mut Context<Self>) {
        self.say(Tone::Busy, tr("Checking for updates…", "Recherche d'une mise à jour…"), cx);
        if cfg!(test) {
            return;
        }
        cx.spawn(async move |this, cx| {
            let latest = cx.background_executor().spawn(async move { update::check_now() }).await;
            this.update(cx, |this, cx| this.checked(latest, cx)).ok();
        })
        .detach();
    }

    /// Suite du contrôle : la bannière si `latest` est plus récente, sinon un mot.
    fn checked(&mut self, latest: Option<update::Release>, cx: &mut Context<Self>) {
        match latest {
            None => self.say(Tone::Failed, tr("No answer from GitHub: check the connection", "GitHub ne répond pas : vérifier la connexion"), cx),
            Some(release) if update::is_new(&release) => {
                self.toasts.retain(|t| t.tone != Tone::Busy);
                self.update = Some(release);
                cx.notify();
            }
            Some(_) => self.say(Tone::Done, format!("{} ({})", tr("Bref is up to date", "Bref est à jour"), env!("CARGO_PKG_VERSION")), cx),
        }
    }

    /// Télécharge et installe la version trouvée, puis quitte : le nouveau programme se
    /// relance de lui-même. Un échec laisse l'app telle qu'elle est, avec son message.
    fn install_update(&mut self, cx: &mut Context<Self>) {
        let Some((url, sha256)) = self.update.as_ref().and_then(|r| Some((r.asset.clone()?, r.sha256.clone()))) else {
            return;
        };
        if self.updating {
            return;
        }
        self.updating = true;
        self.calm();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { update::install(&url, sha256.as_deref()) }).await;
            this.update(cx, |this, cx| match result {
                // La sortie enregistre la note en cours (`on_app_quit`).
                Ok(()) => cx.quit(),
                Err(e) => {
                    this.updating = false;
                    this.say(Tone::Failed, format!("{} : {e}", tr("Update failed", "Mise à jour impossible")), cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// Active ou coupe la recherche de nouvelle version, et retient le choix.
    fn toggle_updates(&mut self, cx: &mut Context<Self>) {
        self.prefs.updates = !self.prefs.updates;
        vault::save_settings(&self.prefs.to_text());
        self.update = None;
        self.check_updates(cx);
        cx.notify();
    }

    /// Suit ce que d'autres programmes changent dans le coffre : notes ajoutées,
    /// renommées, supprimées, ou modifiées pendant qu'elles sont affichées.
    ///
    /// Le système signale les changements (inotify, FSEvents…) : la relecture suit
    /// dans la fraction de seconde. Une relecture de contrôle a lieu quand même toutes
    /// les 30 s, au cas où un événement se perdrait. Sans surveillance possible (limite
    /// d'inotify atteinte, dossier réseau), l'empreinte du coffre est recalculée toutes
    /// les 2 s.
    // ponytail: l'empreinte (noms et dates, sans lire les fichiers) est calculée hors
    // du thread UI et ne prend jamais plus de 0,5 % d'un cœur : un gros coffre est
    // donc regardé moins souvent (5 000 notes : toutes les 5 s, ou 100 s en contrôle).
    // Les tests n'installent pas de surveillance : leur horloge est simulée.
    fn watch(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let changed = Arc::new(vault::Signal::default());
            let mut watched: Option<(PathBuf, Option<notify::RecommendedWatcher>)> = None;
            let mut last = None;
            let mut pause = Duration::from_secs(2);
            // Prochain contrôle de la surveillance.
            let mut next_check = Instant::now();
            loop {
                let live = watched.as_ref().is_some_and(|(_, w)| w.is_some());
                // Avec la surveillance, la tâche dort jusqu'au signal du système ; elle se réveille
                // quand même toutes les 2 s pour voir si on a changé de coffre.
                let notified = if live {
                    changed.wait(cx.background_executor().timer(Duration::from_secs(2))).await
                } else {
                    cx.background_executor().timer(pause).await;
                    false
                };
                let Ok(state) = this.update(cx, |this, _| this.vault.clone().map(|root| (root, this.save_gen)))
                else {
                    return;
                };
                let Some((root, generation)) = state else {
                    continue;
                };
                if watched.as_ref().map(|(r, _)| r) != Some(&root) {
                    let watcher = if cfg!(test) { None } else { vault::watch(&root, changed.clone()) };
                    watched = Some((root.clone(), watcher));
                    next_check = Instant::now();
                }
                let live = watched.as_ref().is_some_and(|(_, w)| w.is_some());
                if live {
                    if !notified && Instant::now() < next_check {
                        continue;
                    }
                    if notified {
                        // Un enregistrement ou une synchronisation touche souvent plusieurs
                        // fichiers d'affilée : on laisse la rafale se terminer.
                        cx.background_executor().timer(Duration::from_millis(250)).await;
                        changed.take();
                    }
                }
                let print_root = root.clone();
                let (took, print) = cx
                    .background_executor()
                    .spawn(async move {
                        let start = Instant::now();
                        let print = vault::fingerprint(&print_root);
                        (start.elapsed(), print)
                    })
                    .await;
                pause = Duration::from_secs(2).max(took * 200);
                next_check = Instant::now() + Duration::from_secs(30).max(took * 200);
                if last == Some((root.clone(), print)) {
                    continue;
                }
                // Seules les notes dont la date a changé sont relues.
                let Ok(known) = this.update(cx, |this, _| this.notes.clone()) else {
                    return;
                };
                let scan_root = root.clone();
                let (notes, dirs, images) = cx
                    .background_executor()
                    .spawn(async move { vault::rescan(&scan_root, &known) })
                    .await;
                let mut applied = false;
                this.update(cx, |this, cx| {
                    // Une frappe ou un enregistrement pendant la lecture : on réessaie au prochain tour.
                    if this.vault.as_ref() == Some(&root) && this.save_gen == generation && !this.panes.iter().any(|pane| pane.dirty) {
                        this.sync(notes, dirs, images, cx);
                        last = Some((root, print));
                        applied = true;
                    }
                })
                .ok();
                if !applied {
                    // Avec une surveillance, le prochain tour est celui du contrôle : le rapprocher.
                    next_check = Instant::now() + Duration::from_secs(1);
                }
            }
        })
        .detach();
    }

    /// Aligne l'app sur le coffre tel qu'il vient d'être relu.
    fn sync(
        &mut self,
        notes: Vec<Note>,
        mut dirs: Vec<PathBuf>,
        mut images: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        // Mêmes notes, mêmes tags, mêmes liens ? Chaque note relue se compare à celle qu'on avait
        // sous son chemin, sans rien copier ni trier.
        let had: std::collections::HashMap<&Path, &Note> = self.notes.iter().map(|n| (n.path.as_path(), n)).collect();
        let same = notes.len() == had.len() && notes.iter().all(|n| had.get(n.path.as_path()).is_some_and(|o| o.tags == n.tags && o.links == n.links));
        dirs.sort();
        self.dirs.sort();
        images.sort();
        self.images.sort();
        if !same || dirs != self.dirs || images != self.images {
            self.notes = notes;
            self.dirs = dirs;
            self.images = images;
            self.push_names(cx);
            self.graph_stale = true;
            self.refresh_graph(cx);
            cx.notify();
        } else {
            // Ni nom, ni tag, ni lien n'ont changé, mais le texte peut avoir : la recherche le suit.
            self.notes = notes;
        }
        self.each_pane(|this| {
            // Le tableau affiché a été réécrit ailleurs : il est relu s'il n'attend rien ici.
            if let Some((_, sheet)) = &this.sheet {
                sheet.update(cx, |sheet, cx| sheet.reload_if_changed(cx));
            }
            // La note affichée a été réécrite ailleurs : on la relit. Sans modification
            // en attente ici (l'appelant s'en assure), rien n'est perdu.
            if let Some(path) = &this.path
                && let Ok(text) = fs::read_to_string(path)
                && text != this.editor.read(cx).text()
            {
                this.reload(cx);
            }
        });
    }

    fn set_vault(&mut self, root: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.each_pane(|this| this.leave(cx));
        // Un autre coffre : un seul pane, sur une note neuve.
        self.panes.truncate(1);
        self.active = 0;
        self.vault = Some(root.clone());
        self.icons = vault::load_icons(&root);
        self.notes.clear();
        self.dirs.clear();
        self.images.clear();
        self.recent.clear();
        self.new_note(String::new(), cx);
        vault::save_config(&root, &[]);
        window.focus(&self.editor.focus_handle(cx));
        // L'index (noms, tags) se construit hors du thread UI.
        cx.spawn(async move |this, cx| {
            let scan_root = root.clone();
            let (notes, dirs, images) = cx
                .background_executor()
                .spawn(async move { vault::scan(&scan_root) })
                .await;
            this.update(cx, |this, cx| {
                if this.vault.as_ref() == Some(&root) {
                    this.notes = notes;
                    this.dirs = dirs;
                    this.images = images;
                    this.push_names(cx);
                    this.graph_stale = true;
                    this.refresh_graph(cx);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Crée le dossier du coffre s'il n'existe pas, puis l'ouvre : un dossier déjà là est
    /// ouvert tel quel, rien n'y est écrit.
    fn create_vault(&mut self, root: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        match fs::create_dir_all(&root) {
            Ok(()) => self.set_vault(root, window, cx),
            Err(e) => {
                let what = tr("Cannot create the vault", "Impossible de créer le coffre");
                self.say(Tone::Failed, format!("{what} {} : {e}", root.display()), cx);
            }
        }
    }

    fn choose_vault(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(tr("Use this vault", "Choisir ce coffre").into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = paths.await
                && let Some(root) = paths.into_iter().next()
            {
                this.update_in(cx, |this, window, cx| this.set_vault(root, window, cx))
                    .ok();
            }
        })
        .detach();
    }

    /// Applique les réglages d'apparence à toute la fenêtre.
    fn restyle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        apply_fonts(&self.prefs);
        let theme = Theme::of(&self.prefs, window.appearance());
        self.theme = theme;
        for pane in &self.panes {
            pane.editor.update(cx, |e, cx| e.set_theme(theme, cx));
        }
        if let Some(palette) = &self.palette {
            palette.update(cx, |p, cx| p.set_theme(theme, cx));
        }
        cx.notify();
    }

    /// Liste de choix d'un réglage, comme le sélecteur de thème de Zed : le choix
    /// parcouru s'applique en aperçu, Entrée le garde, Échap revient au précédent.
    pub fn choose_setting(&mut self, setting: Setting, window: &mut Window, cx: &mut Context<Self>) {
        if self.vault.is_none() {
            return;
        }
        let default = tr("Default", "Par défaut");
        // Largeur de page et grands tableaux : une courte liste fixe, sans « Par défaut » ; le
        // choix se range sous sa clé, ses libellés suivent la langue.
        let fixed: Option<Vec<(&'static str, &'static str)>> = match setting {
            Setting::Page => Some(Page::ALL.map(|p| (p.key(), p.label())).into()),
            Setting::Wide => Some(Wide::ALL.map(|w| (w.key(), w.label())).into()),
            Setting::Bar => Some(vec![("on", tr("Shown", "Affichée")), ("off", tr("Hidden", "Masquée"))]),
            _ => None,
        };
        let (mut options, current): (Vec<String>, &str) = match (setting, &fixed) {
            (Setting::Page, Some(list)) => (list.iter().map(|o| o.1.to_string()).collect(), Page::of(&self.prefs.page).label()),
            (Setting::Wide, Some(list)) => (list.iter().map(|o| o.1.to_string()).collect(), Wide::of(&self.prefs.wide).label()),
            (Setting::Bar, Some(list)) => (list.iter().map(|o| o.1.to_string()).collect(), if self.prefs.bar == "off" { list[1].1 } else { list[0].1 }),
            (Setting::Theme, _) => (THEMES.iter().map(|t| t.0.to_string()).collect(), &self.prefs.theme),
            _ => (
                FAMILIES.get().into_iter().flatten().map(|f| f.to_string()).collect(),
                if setting == Setting::Font { &self.prefs.font } else { &self.prefs.mono },
            ),
        };
        let current = if fixed.is_some() {
            current
        } else {
            // Le choix en cours remonte en tête, juste sous « Par défaut ».
            if let Some(i) = options.iter().position(|o| o == current) {
                let chosen = options.remove(i);
                options.insert(0, chosen);
            }
            options.insert(0, default.to_string());
            if current.is_empty() { default } else { current }
        };
        let (theme, before) = (self.theme, self.prefs.clone());
        let palette = cx.new(|cx| Palette::choose(setting.label(), options, current, theme, cx));
        cx.subscribe_in(&palette, window, move |this, _, event, window, cx| {
            match event {
                PaletteEvent::Preview(name) | PaletteEvent::Submit(name) => {
                    let value = if name == default { String::new() } else { name.clone() };
                    // Le libellé choisi redevient la clé qu'on enregistre.
                    let key = fixed.iter().flatten().find(|o| o.1 == name).map_or_else(String::new, |o| o.0.to_string());
                    match setting {
                        Setting::Theme => this.prefs.theme = value,
                        Setting::Font => this.prefs.font = value,
                        Setting::Mono => this.prefs.mono = value,
                        Setting::Page => this.prefs.page = key,
                        Setting::Wide => this.prefs.wide = key,
                        Setting::Bar => this.prefs.bar = key,
                    }
                }
                _ => this.prefs = before.clone(),
            }
            if !matches!(event, PaletteEvent::Preview(_)) {
                this.palette = None;
                window.focus(&this.editor.focus_handle(cx));
                vault::save_settings(&this.prefs.to_text());
            }
            this.restyle(window, cx);
        })
        .detach();
        window.focus(&palette.focus_handle(cx));
        self.palette = Some(palette);
        cx.notify();
    }

    /// Plan de la note, comme celui de Zed : la liste de ses titres, celui de la section en
    /// cours présélectionné. Le titre parcouru se montre en aperçu, Entrée y laisse le curseur,
    /// Échap le remet où il était.
    pub fn open_outline(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let heads = markdown::headings(self.editor.read(cx).text())
            .into_iter()
            .map(|(level, title, at)| (format!("{}{title}", "  ".repeat(level as usize - 1)), at))
            .collect();
        let none = tr("This note has no headings", "Cette note n'a pas de titres");
        self.open_places(tr("Outline", "Plan"), none, heads, window, cx)
    }

    /// Les commentaires de la note, dans la même liste que son plan : le texte commenté, puis
    /// ce qu'on en dit.
    pub fn open_comments(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.comments_shut = false;
        let said = markdown::all_comments(self.editor.read(cx).text())
            .into_iter()
            .map(|(noted, said, at)| (format!("{noted} — {said}"), at))
            .collect();
        let none = tr("This note has no comments", "Cette note n'a pas de commentaires");
        self.open_places(tr("Comments", "Commentaires"), none, said, window, cx)
    }

    /// Liste d'endroits de la note (libellé, octet), celui où l'on est présélectionné ; sans
    /// aucun, `none` le dit.
    fn open_places(&mut self, title: &str, none: &str, heads: Vec<(String, usize)>, window: &mut Window, cx: &mut Context<Self>) {
        if self.vault.is_none() || self.picture.is_some() {
            return;
        }
        let before = self.editor.read(cx).position();
        if heads.is_empty() {
            self.palette = None;
            self.say(Tone::Info, none, cx);
            window.focus(&self.editor.focus_handle(cx));
            return cx.notify();
        }
        let current = heads.iter().rposition(|(_, at)| *at <= before).unwrap_or(0);
        let (theme, options) = (self.theme, heads.iter().map(|(label, _)| label.clone()).collect());
        let palette = cx.new(|cx| Palette::choose(title, options, "", theme, cx).select(current));
        cx.subscribe_in(&palette, window, move |this, palette, event, window, cx| {
            let to = match event {
                PaletteEvent::Preview(_) | PaletteEvent::Submit(_) => palette.read(cx).chosen().and_then(|i| heads.get(i)),
                _ => None,
            };
            let (at, top) = to.map_or((before, false), |(_, at)| (*at, true));
            this.editor.update(cx, |e, cx| e.jump(at, top, cx));
            if !matches!(event, PaletteEvent::Preview(_)) {
                this.palette = None;
                window.focus(&this.editor.focus_handle(cx));
            }
            cx.notify();
        })
        .detach();
        window.focus(&palette.focus_handle(cx));
        self.palette = Some(palette);
        cx.notify();
    }

    /// Grossit ou réduit le texte de la note ; `None` revient à la taille d'origine.
    fn resize_text(&mut self, by: Option<f32>, window: &mut Window, cx: &mut Context<Self>) {
        let size = by.map_or(Prefs::SIZE, |by| self.prefs.size + by);
        self.prefs = Prefs::parse(&Prefs { size, ..self.prefs.clone() }.to_text());
        vault::save_settings(&self.prefs.to_text());
        self.restyle(window, cx);
    }

    /// Copie toute la note dans le presse-papiers.
    /// Copie la note ; avec `block`, seulement le bloc de code où est le curseur
    /// s'il y en a un.
    fn copy_all(&mut self, block: bool, cx: &mut Context<Self>) {
        let editor = self.editor.read(cx);
        let code = editor.code_at_cursor().filter(|_| block);
        let text = code.unwrap_or(editor.text()).to_string();
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.copied = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(1500)).await;
            this.update(cx, |this, cx| {
                this.copied = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Affiche ou masque le panneau des raccourcis ; le focus suit.
    fn set_help(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.help = open;
        self.help_find = None;
        self.help_sel = None;
        self.help_edit = None;
        if open {
            let theme = self.theme;
            let find = cx.new(|cx| kanban::Field::new("", theme, cx));
            cx.observe(&find, |_, _, cx| cx.notify()).detach();
            // Échap vide le champ, puis ferme le panneau.
            cx.subscribe_in(&find, window, |this, find, event: &kanban::FieldEvent, window, cx| {
                if let kanban::FieldEvent::Done = event {
                    // Entrée sur la ligne choisie : ses touches sont attendues.
                    if let Some(action) = this.help_sel.filter(|sel| this.help_actions(cx).contains(sel)) {
                        this.key_edit(action, cx);
                    }
                } else if let kanban::FieldEvent::Cancel = event {
                    if find.read(cx).line.text.is_empty() {
                        this.set_help(false, window, cx);
                    } else {
                        find.update(cx, |find, cx| {
                            find.line = line::Line::default();
                            cx.notify();
                        });
                    }
                }
            })
            .detach();
            window.focus(&find.focus_handle(cx));
            self.help_find = Some(find);
        } else if self.vault.is_none() {
            window.focus(&self.focus);
        } else {
            // À ce que le pane montre : rendue à la note, la saisie effacerait un tableau ou un schéma.
            self.focus_pane(self.active, window, cx);
        }
        cx.notify();
    }

    /// Ce qui est affiché à la place de la note ; rien pour une note neuve pas encore enregistrée.
    fn shown(&self) -> Option<Shown> {
        match (&self.listing, &self.picture, &self.path) {
            (Some(listing), ..) => Some(Shown::List(listing.of.clone())),
            (None, Some(file), _) => Some(Shown::File(file.clone())),
            (None, None, Some(note)) => Some(Shown::Note(note.clone())),
            (None, None, None) => None,
        }
    }

    /// Retient, dans l'entrée courante de l'historique, où l'on en est dans ce qu'elle montre :
    /// à appeler tant que la vue qu'on quitte existe encore.
    fn mark_place(&mut self, cx: &App) {
        let Some(shown) = self.history.current().map(|entry| entry.shown.clone()) else { return };
        let place = match &shown {
            Shown::Note(path) if self.path.as_ref() == Some(path) => {
                let (at, scroll) = self.editor.read(cx).view();
                Place { at: (at, 0), scroll: (0., scroll) }
            }
            Shown::File(path) => match self.sheet.as_ref().filter(|(open, _)| open == path) {
                Some((_, sheet)) => {
                    let (at, scroll) = sheet.read(cx).whereabouts();
                    Place { at, scroll }
                }
                None => return,
            },
            _ => return,
        };
        self.history.mark(&shown, place);
    }

    /// Le seul endroit où l'historique avance : à chaque rendu, ce qui est affiché est comparé à
    /// son entrée courante. Un aperçu ne compte pas ; revenir en arrière ou repartir non plus,
    /// puisque l'entrée courante est alors déjà ce qu'on affiche.
    fn track(&mut self, cx: &App) {
        if self.peek {
            return;
        }
        let Some(shown) = self.shown() else { return };
        if self.history.current().is_none_or(|entry| entry.shown != shown) {
            self.mark_place(cx);
            self.history.visit(shown);
        }
    }

    /// Précédent ou suivant dans ce qui a été affiché ; ce qui n'existe plus est sauté.
    fn travel(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.mark_place(cx);
        // Sur ce que l'historique ne retient pas (un aperçu, une note neuve), « précédent »
        // ramène à l'entrée courante : c'est de là qu'on venait.
        let mut stay = !forward && self.history.current().is_some_and(|entry| Some(&entry.shown) != self.shown().as_ref());
        loop {
            let entry = match (std::mem::take(&mut stay), forward) {
                (true, _) => self.history.current().cloned(),
                (false, true) => self.history.forward(),
                (false, false) => self.history.back(),
            };
            let Some(entry) = entry else { break };
            if Some(&entry.shown) == self.shown().as_ref() {
                break;
            }
            let there = match &entry.shown {
                Shown::List(of) => nav::tag_of(of).is_some() || of.is_dir(),
                Shown::Note(path) | Shown::File(path) => path.is_file(),
            };
            if there {
                self.show(entry, window, cx);
                break;
            }
            // Supprimé par un autre programme : l'entrée part, celle d'avant devient la courante.
            self.history.remove(entry.shown.path());
            stay = !forward;
        }
        cx.notify();
    }

    /// Affiche une entrée de l'historique, là où on l'avait laissée.
    fn show(&mut self, entry: history::Entry, window: &mut Window, cx: &mut Context<Self>) {
        self.peek = false;
        if self.nav.panel == nav::Panel::Full {
            self.nav.panel = nav::Panel::Split;
        }
        match entry.shown {
            Shown::List(of) => self.open_listing(of, window, cx),
            Shown::Note(path) => {
                if self.load_note(&path, cx) {
                    self.preview = false;
                    self.editor.update(cx, |editor, cx| editor.set_view(entry.place.at.0, entry.place.scroll.1, cx));
                }
                window.focus(&self.editor.focus_handle(cx));
            }
            Shown::File(path) => {
                self.load_note(&path, cx);
                // Un tableau reprend sa cellule et la saisie ; une image n'a rien à reprendre.
                if let Some(sheet) = self.shown_sheet() {
                    sheet.update(cx, |sheet, cx| sheet.set_view(entry.place.at, entry.place.scroll, cx));
                    window.focus(&sheet.focus_handle(cx));
                }
            }
        }
    }

    /// Les actions des lignes que l'aide montre, dans l'ordre.
    fn help_actions(&self, cx: &App) -> Vec<&'static str> {
        self.help_rows(cx).into_iter().flat_map(|(_, rows)| rows).filter_map(|row| row.action).collect()
    }

    /// Haut et bas dans l'aide : la ligne choisie, parmi celles dont les touches se changent.
    // ponytail: la liste ne défile pas jusqu'à la ligne choisie ; la recherche la ramène à
    // l'écran. Un `ScrollHandle` par section si le besoin se confirme.
    fn help_step(&mut self, down: bool, cx: &mut Context<Self>) {
        let actions = self.help_actions(cx);
        let at = self.help_sel.and_then(|sel| actions.iter().position(|action| *action == sel));
        let next = match (at, down) {
            (None, true) => 0,
            (None, false) => actions.len().saturating_sub(1),
            (Some(at), true) => (at + 1).min(actions.len().saturating_sub(1)),
            (Some(at), false) => at.saturating_sub(1),
        };
        self.help_sel = actions.get(next).copied();
        cx.notify();
    }

    /// Attend les touches du raccourci de `action`.
    fn key_edit(&mut self, action: &'static str, cx: &mut Context<Self>) {
        self.help_sel = Some(action);
        self.help_edit = Some(KeyEdit { action, keys: String::new(), issue: None });
        cx.notify();
    }

    /// Une frappe pendant qu'un raccourci est attendu : elle le devient, sauf Échap (renoncer),
    /// Retour arrière (aucun raccourci) et Entrée sur des touches déjà prises (remplacer).
    /// `false` quand rien n'est attendu : la frappe suit son cours.
    fn key_pressed(&mut self, stroke: &gpui::Keystroke, cx: &mut Context<Self>) -> bool {
        let Some(edit) = &self.help_edit else { return false };
        let (action, taken) = (edit.action, matches!(edit.issue, Some(KeyIssue::Taken(_))));
        let keys = keys::spell(stroke);
        match keys.as_str() {
            "escape" => self.help_edit = None,
            "enter" if taken => self.key_replace(cx),
            "backspace" | "delete" => self.key_set(action, String::new(), &[], cx),
            _ => {
                let open = editable();
                let others = keys::clashes(&self.bound, &defaults(), action, &keys);
                let issue = match keys::refusal(&keys) {
                    Some(keys::Why::Types) => Some(KeyIssue::Refused(tr(
                        "A key alone types text: add Ctrl, Alt or Cmd.",
                        "Une touche seule écrit du texte : ajouter Ctrl, Alt ou Cmd.",
                    ))),
                    Some(_) => Some(KeyIssue::Refused(tr("These keys cannot be a shortcut.", "Ces touches ne peuvent pas servir de raccourci."))),
                    None if others.iter().any(|other| !open.contains(other)) => Some(KeyIssue::Refused(tr(
                        "Kept for typing and moving in the text and the views.",
                        "Réservées à la saisie et au déplacement dans le texte et les vues.",
                    ))),
                    None if !others.is_empty() => Some(KeyIssue::Taken(others)),
                    None => None,
                };
                match issue {
                    None => self.key_set(action, keys, &[], cx),
                    issue => self.help_edit = Some(KeyEdit { action, keys, issue }),
                }
            }
        }
        cx.notify();
        true
    }

    /// « Remplacer » : les touches proposées vont à l'action, celles qui les avaient les perdent.
    fn key_replace(&mut self, cx: &mut Context<Self>) {
        if let Some(KeyEdit { action, keys, issue: Some(KeyIssue::Taken(others)) }) = self.help_edit.take() {
            self.key_set(action, keys, &others, cx);
        }
    }

    /// Donne `keys` à `action` (vides : aucun raccourci) et les retire à `losers`.
    fn key_set(&mut self, action: &'static str, keys: String, losers: &[&'static str], cx: &mut Context<Self>) {
        let mut usual: Vec<String> = defaults().into_iter().filter(|bind| bind.action == action).map(|bind| bind.keys).collect();
        usual.dedup();
        // Revenu à ses touches d'origine : plus rien à retenir pour cette action.
        if usual == [keys.clone()] {
            self.changes.remove(action);
        } else {
            self.changes.insert(action.to_string(), keys.clone());
        }
        for other in losers {
            self.changes.insert(other.to_string(), String::new());
        }
        self.help_edit = None;
        if keys::system(&keys) {
            self.say(Tone::Info, tr("The system may keep these keys for itself.", "Le système garde peut-être ces touches pour lui."), cx);
        }
        self.rebind(cx);
        vault::save_file("keys", &keys::write(&self.changes));
    }

    /// Remet les touches d'origine d'une action, ou de toutes.
    fn key_reset(&mut self, action: Option<&'static str>, cx: &mut Context<Self>) {
        match action {
            Some(action) => drop(self.changes.remove(action)),
            None => self.changes.clear(),
        }
        self.help_edit = None;
        self.rebind(cx);
        vault::save_file("keys", &keys::write(&self.changes));
    }

    /// Applique `changes` : les liaisons en vigueur changent tout de suite, sans redémarrer.
    fn rebind(&mut self, cx: &mut Context<Self>) {
        let open = editable();
        self.bound = keys::effective(&defaults(), &self.changes, &|action| open.contains(action));
        install(cx, &self.bound);
        cx.notify();
    }

    fn push_names(&mut self, cx: &mut Context<Self>) {
        // Les alias se proposent après `[[` comme des noms.
        let notes: Vec<&Note> = self.notes.iter().filter(|n| !self.is_template(n)).collect();
        let names: Vec<String> = notes.iter().map(|n| n.name.clone()).chain(notes.iter().flat_map(|n| n.aliases.iter().cloned())).collect();
        let pictures = vault::pictures(&self.images);
        for pane in &self.panes {
            pane.editor.update(cx, |e, _| {
                e.set_notes(names.clone());
                e.set_images(&pictures);
            });
        }
    }

    /// Indique à l'éditeur où chercher les images : à côté de la note, puis dans le coffre.
    fn push_dirs(&mut self, cx: &mut Context<Self>) {
        let note_dir = self.path.as_deref().and_then(Path::parent).map(Path::to_path_buf);
        let dirs = note_dir.into_iter().chain(self.vault.clone()).collect();
        self.editor.update(cx, |e, _| e.set_dirs(dirs));
    }

    /// Place la note en tête des récentes et mémorise la liste.
    fn touch(&mut self, path: &Path) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_path_buf());
        self.recent.truncate(200);
        if let Some(root) = &self.vault {
            vault::save_config(root, &self.recent);
        }
    }

    /// Les notes, de la plus récemment ouverte à la plus ancienne ; celles jamais
    /// ouvertes suivent, par date de modification.
    fn by_recency(&self) -> Vec<&Note> {
        // Le rang de chaque récente, trouvé d'un coup : pas de liste relue pour chaque note.
        let rank: std::collections::HashMap<&Path, usize> = self.recent.iter().enumerate().rev().map(|(i, p)| (p.as_path(), i)).collect();
        let mut notes: Vec<&Note> = self.notes.iter().collect();
        notes.sort_by_cached_key(|n| rank.get(n.path.as_path()).copied().unwrap_or(usize::MAX));
        notes
    }

    /// Charge la note dans l'éditeur ; faux si le fichier est illisible.
    fn load_note(&mut self, path: &Path, cx: &mut Context<Self>) -> bool {
        // Déjà dans l'autre pane : deux éditeurs sur un même fichier s'écraseraient l'un l'autre.
        // La saisie y passe ; un simple aperçu ne la déplace pas.
        if let Some(other) = self.pane_showing(path).filter(|other| *other != self.active) {
            if !self.peek {
                self.active = other;
                if self.path.as_deref() == Some(path) {
                    (self.picture, self.listing) = (None, None);
                }
                self.grab = true;
                cx.notify();
            }
            return false;
        }
        self.listing = None;
        // Une pièce jointe (PDF, vidéo, archive…) s'ouvre dans l'application du système ; la note reste là.
        if vault::is_attachment(path) {
            // La plateforme de test n'ouvre rien : elle note ce qu'on lui aurait demandé.
            #[cfg(test)]
            self.opened.push(path.to_path_buf());
            #[cfg(not(test))]
            cx.open_with_system(path);
            return false;
        }
        if vault::is_table(path) {
            return self.open_table(path, cx);
        }
        // Une image prend la place de la note, qui reste chargée dessous.
        if vault::is_image(path) {
            self.picture = Some(path.to_path_buf());
            // Un schéma s'ouvre dans son canevas, prêt à être repris.
            let svg = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("svg"));
            if svg && self.drawing.as_ref().is_none_or(|(open, _)| open != path) {
                let diagram = fs::read_to_string(path).ok().and_then(|svg| Diagram::from_svg(&svg));
                let theme = self.theme;
                self.drawing = diagram.map(|diagram| {
                    let canvas = cx.new(|cx| Canvas::new(diagram, theme, cx));
                    cx.subscribe(&canvas, Self::on_canvas_event).detach();
                    (path.to_path_buf(), canvas)
                });
            }
            // Un SVG d'ailleurs s'ouvre dans la visionneuse : on le zoome, on s'y déplace.
            if svg && self.drawing.is_none() && self.viewer.as_ref().is_none_or(|(open, _)| open != path) {
                let theme = self.theme;
                let source = viewer::Source::File(path.to_path_buf(), None);
                self.viewer = Some((path.to_path_buf(), cx.new(|cx| viewer::Viewer::new(source, theme, cx))));
            }
            cx.notify();
            return false;
        }
        self.picture = None;
        self.leave(cx);
        cx.notify();
        match fs::read_to_string(path) {
            Ok(text) => {
                self.synced = vault::stem(path) == vault::title_of(&text);
                self.fresh = false;
                self.h1 = vault::h1_of(&text);
                self.origin = Some(vault::stem(path));
                // Une note relue garde sa vue ; une autre s'ouvre en tableau si elle en est un.
                self.board_text &= self.path.as_deref() == Some(path);
                self.board_was = kanban::is_board(&text);
                self.path = Some(path.to_path_buf());
                self.calm();
                let cursor = markdown::body_start(&text);
                self.editor.update(cx, |e, cx| e.load(text, cursor, cx));
                self.push_dirs(cx);
                self.nav.reveal(path);
                true
            }
            Err(e) => {
                let what = tr("Cannot open", "Impossible d'ouvrir");
                self.say(Tone::Failed, format!("{what} {} : {e}", path.display()), cx);
                false
            }
        }
    }

    /// Un tableau prend la place de la note, comme une image ; la note reste chargée dessous.
    fn open_table(&mut self, path: &Path, cx: &mut Context<Self>) -> bool {
        self.picture = Some(path.to_path_buf());
        if self.sheet.as_ref().is_some_and(|(open, _)| open == path) {
            cx.notify();
            return false;
        }
        self.mark_place(cx);
        self.flush_sheet(cx);
        self.sheet = None;
        match sheet::load(path, None) {
            Ok(loaded) => {
                let theme = self.theme;
                let sheet = cx.new(|cx| Sheet::new(path.to_path_buf(), loaded, theme, cx));
                cx.subscribe(&sheet, Self::on_sheet_event).detach();
                self.sheet = Some((path.to_path_buf(), sheet));
                self.calm();
            }
            Err(e) => {
                self.picture = None;
                self.say(Tone::Failed, e, cx);
            }
        }
        cx.notify();
        false
    }

    /// Le tableau affiché, s'il y en a un.
    fn shown_sheet(&self) -> Option<Entity<Sheet>> {
        self.sheet.as_ref().filter(|(path, _)| self.picture.as_ref() == Some(path)).map(|(_, s)| s.clone())
    }

    fn flush_sheet(&mut self, cx: &mut Context<Self>) {
        if let Some((_, sheet)) = &self.sheet {
            sheet.update(cx, |sheet, cx| sheet.flush(cx));
        }
    }

    fn on_sheet_event(&mut self, _: Entity<Sheet>, event: &SheetEvent, cx: &mut Context<Self>) {
        match event {
            SheetEvent::Error(message) => self.say(Tone::Failed, message.clone(), cx),
            // Un fichier est apparu à côté du tableau : l'arbre le montre.
            SheetEvent::Exported => {
                if let Some(root) = self.vault.clone() {
                    let (notes, dirs, images) = vault::rescan(&root, &self.notes);
                    self.sync(notes, dirs, images, cx);
                }
            }
        }
        cx.notify();
    }

    /// Liste de choix d'un réglage du tableau affiché (délimiteur, encodage, copie, export).
    fn choose_table(&mut self, pick: Pick, window: &mut Window, cx: &mut Context<Self>) {
        let Some(sheet) = self.shown_sheet() else {
            return;
        };
        if pick == Pick::Header {
            return sheet.update(cx, |sheet, cx| sheet.toggle_header(cx));
        }
        let auto = tr("Detect automatically", "Détecter automatiquement");
        let format = sheet.read(cx).format();
        let styles = [
            ("TSV", Style::Tsv),
            ("CSV", Style::Csv(format.delimiter)),
            ("Markdown", Style::Markdown),
            ("JSON", Style::Json),
        ];
        let (options, current): (Vec<String>, String) = match pick {
            Pick::Delimiter => (
                std::iter::once(auto.to_string()).chain(table::DELIMITERS.map(sheet::delimiter_option)).collect(),
                sheet::delimiter_option(format.delimiter),
            ),
            Pick::Encoding => (
                std::iter::once(auto.to_string()).chain(table::Encoding::ALL.map(|e| e.label().to_string())).collect(),
                format.encoding.label().to_string(),
            ),
            _ => (styles.iter().map(|(name, _)| name.to_string()).collect(), styles[0].0.to_string()),
        };
        let theme = self.theme;
        let palette = cx.new(|cx| Palette::choose(pick.label(), options, &current, theme, cx));
        cx.subscribe_in(&palette, window, move |this, _, event, window, cx| {
            let PaletteEvent::Submit(name) = event else {
                // Pas d'aperçu pour un tableau : seul le choix validé compte.
                if !matches!(event, PaletteEvent::Preview(_)) {
                    this.palette = None;
                    window.focus(&sheet.focus_handle(cx));
                    cx.notify();
                }
                return;
            };
            this.palette = None;
            window.focus(&sheet.focus_handle(cx));
            let style = styles.iter().find(|(label, _)| label == name).map(|&(_, style)| style);
            sheet.update(cx, |sheet, cx| match pick {
                Pick::Delimiter | Pick::Encoding if name == auto => sheet.reformat(None, cx),
                Pick::Delimiter => {
                    let delimiter = table::DELIMITERS.into_iter().find(|&d| sheet::delimiter_option(d) == *name);
                    sheet.reformat(delimiter.map(|delimiter| Format { delimiter, ..format }), cx)
                }
                Pick::Encoding => {
                    let encoding = Encoding::from_label(name);
                    sheet.reformat(encoding.map(|encoding| Format { encoding, ..format }), cx)
                }
                Pick::CopyAs => style.into_iter().for_each(|style| sheet.copy_as(style, cx)),
                Pick::Export => style.into_iter().for_each(|style| sheet.export(style, cx)),
                Pick::Header => {}
            });
            cx.notify();
        })
        .detach();
        window.focus(&palette.focus_handle(cx));
        self.palette = Some(palette);
        cx.notify();
    }

    fn open_note(&mut self, path: &Path, cx: &mut Context<Self>) {
        self.peek = false;
        if self.load_note(path, cx) {
            self.preview = false;
            self.touch(path);
        }
    }

    /// Affiche la note sans la compter comme ouverte.
    fn preview_note(&mut self, path: &Path, cx: &mut Context<Self>) {
        self.peek = true;
        // La note déjà chargée, cachée par une image : il suffit de retirer l'image.
        if self.path.as_deref() == Some(path) {
            if self.picture.take().is_some() {
                cx.notify();
            }
        } else if self.load_note(path, cx) {
            self.preview = true;
        }
    }

    /// L'aperçu devient une note ouverte.
    fn keep_preview(&mut self) {
        self.peek = false;
        if std::mem::take(&mut self.preview)
            && let Some(path) = self.path.clone()
        {
            self.touch(&path);
        }
    }

    /// Nouvelle note, rangée dans le dossier sélectionné quand l'arbre est affiché.
    fn new_note_here(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.new_note_in(self.nav.target_dir(), window, cx);
    }

    /// Nouvelle note rangée dans `dir` ; `None` : à la racine du coffre.
    fn new_note_in(&mut self, dir: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        if self.vault.is_none() {
            return;
        }
        self.new_note(String::new(), cx);
        self.new_dir = dir;
        if self.nav.panel == Panel::Full {
            self.nav.panel = Panel::Split;
        }
        window.focus(&self.editor.focus_handle(cx));
        self.settle_nav(window, cx);
    }

    fn new_note(&mut self, text: String, cx: &mut Context<Self>) {
        self.peek = false;
        self.leave(cx);
        self.listing = None;
        self.picture = None;
        self.path = None;
        self.origin = None;
        self.synced = true;
        self.preview = false;
        self.new_dir = None;
        self.fresh = true;
        self.dirty = !text.is_empty();
        (self.board_text, self.board_was) = (false, kanban::is_board(&text));
        let cursor = text.len();
        self.editor.update(cx, |e, cx| e.load(text, cursor, cx));
        self.push_dirs(cx);
        self.flush(cx);
        cx.notify();
    }

    /// Avant de quitter la note affichée : elle est enregistrée et, si son titre
    /// l'a renommée, les liens vers son ancien nom suivent. Attendre ce moment
    /// évite de réécrire les liens à chaque titre intermédiaire pendant la frappe.
    fn leave(&mut self, cx: &mut Context<Self>) {
        self.mark_place(cx);
        self.flush(cx);
        if let (Some(old), Some(path)) = (self.origin.take(), &self.path) {
            let new = vault::stem(path);
            self.relink(&old, &new, cx);
        }
        self.drop_if_empty(cx);
    }

    /// Une note créée ici puis vidée ne laisse pas de fichier vide derrière elle, ni dans les
    /// récentes. Seul un fichier vide sur le disque est retiré : rien d'écrit ne se perd.
    fn drop_if_empty(&mut self, cx: &mut Context<Self>) {
        if !std::mem::take(&mut self.fresh) || !self.editor.read(cx).text().trim().is_empty() {
            return;
        }
        let Some(path) = self.path.clone() else {
            return;
        };
        if fs::read_to_string(&path).is_ok_and(|text| text.trim().is_empty()) && fs::remove_file(&path).is_ok() {
            self.notes.retain(|n| n.path != path);
            self.recent.retain(|p| *p != path);
            if let Some(root) = &self.vault {
                vault::save_config(root, &self.recent);
            }
            self.history.remove(&path);
            self.path = None;
            self.graph_stale = true;
            self.push_names(cx);
            self.refresh_graph(cx);
        }
    }

    /// Les `[[liens]]` vers `old` visent désormais `new` dans toutes les notes ;
    /// vrai si la note affichée a été réécrite. Sans effet si une autre note
    /// s'appelle encore `old` : les liens sont peut-être pour elle.
    // ponytail: réécriture synchrone, limitée aux notes que l'index dit liées à
    // `old` ; passer en tâche de fond si une note est liée depuis des centaines d'autres.
    fn relink(&mut self, old: &str, new: &str, cx: &mut Context<Self>) -> bool {
        let key = old.to_lowercase();
        if key == new.to_lowercase() || self.notes.iter().any(|n| n.name.to_lowercase() == key) {
            return false;
        }
        let mut changed = Vec::new();
        for note in self.notes.iter_mut().filter(|n| n.links.contains(&key)) {
            let rewritten = fs::read_to_string(&note.path)
                .ok()
                .and_then(|text| markdown::relink(&text, old, new))
                .filter(|text| vault::write(&note.path, text).is_ok());
            if let Some(text) = rewritten {
                (note.tags, note.links) = Note::index(&text);
                changed.push(note.path.clone());
                self.graph_stale = true;
            }
        }
        self.refresh_graph(cx);
        // La note de l'autre pane est relue ici ; celle du pane actif, l'appelant en décide
        // (il est peut-être en train de la quitter).
        let showing = |pane: &Pane| pane.path.as_ref().is_some_and(|path| changed.contains(path));
        for pane in 0..self.panes.len() {
            if pane != self.active && showing(&self.panes[pane]) {
                self.in_pane(pane, |this| this.reload(cx));
            }
        }
        showing(&self.panes[self.active])
    }

    /// Remplacement dans tout le coffre : compte d'abord, et n'écrit qu'une fois le compte
    /// accepté. Chaque note réécrite est gardée telle qu'elle était, pour tout défaire.
    fn replace_in_vault(&mut self, swap: markdown::Swap, window: &mut Window, cx: &mut Context<Self>) {
        // La note ouverte est d'abord enregistrée : le remplacement part de ce qui est à l'écran.
        self.flush(cx);
        let found: Vec<(PathBuf, String, usize)> = (self.notes.iter())
            .filter_map(|note| markdown::swap(&note.body, &swap).map(|(text, count)| (note.path.clone(), text, count)))
            .collect();
        let count: usize = found.iter().map(|(.., count)| count).sum();
        if count == 0 {
            return self.say(Tone::Info, tr("Nothing to replace in the vault", "Rien à remplacer dans le coffre"), cx);
        }
        let title = format!("{} {count} · notes : {}", tr("Matches:", "Passages :"), found.len());
        let options = vec![tr("Replace them all", "Tout remplacer").to_string(), tr("Cancel", "Annuler").to_string()];
        let theme = self.theme;
        let palette = cx.new(|cx| Palette::choose(&title, options, "", theme, cx));
        cx.subscribe_in(&palette, window, move |this, palette, event, window, cx| {
            if matches!(event, PaletteEvent::Preview(_)) {
                return;
            }
            let agreed = matches!(event, PaletteEvent::Submit(_)) && palette.read(cx).chosen() == Some(0);
            this.palette = None;
            window.focus(&this.editor.focus_handle(cx));
            if agreed {
                this.swap_notes(found.clone(), cx);
            }
            cx.notify();
        })
        .detach();
        window.focus(&palette.focus_handle(cx));
        self.palette = Some(palette);
        cx.notify();
    }

    /// Écrit le nouveau texte de chaque note, et retient l'ancien.
    fn swap_notes(&mut self, found: Vec<(PathBuf, String, usize)>, cx: &mut Context<Self>) {
        self.swapped.clear();
        let mut count = 0;
        for (path, text, hits) in found {
            let Some(note) = self.notes.iter_mut().find(|note| note.path == path) else {
                continue;
            };
            if let Err(e) = vault::write(&path, &text) {
                self.say(Tone::Failed, format!("{} {} : {e}", tr("Not rewritten:", "Non réécrite :"), path.display()), cx);
                continue;
            }
            count += hits;
            (note.tags, note.links) = Note::index(&text);
            self.swapped.push((path, note.set_body(Arc::from(text.as_str())), text));
        }
        self.after_swap(cx);
        let done = format!("{} {count} · notes : {} ({})", tr("Replaced:", "Remplacés :"), self.swapped.len(), tr("the palette can undo it", "la palette peut l'annuler"));
        self.say(Tone::Done, done, cx);
    }

    /// Défait le dernier remplacement dans le coffre : chaque note retrouve son texte d'avant,
    /// sauf celles qui ont changé depuis, laissées telles quelles.
    fn undo_swap(&mut self, cx: &mut Context<Self>) {
        self.flush(cx);
        let (mut back, mut kept) = (0, 0);
        for (path, old, new) in std::mem::take(&mut self.swapped) {
            let same = fs::read_to_string(&path).is_ok_and(|now| now == new);
            if same && vault::write(&path, &old).is_ok() {
                if let Some(note) = self.notes.iter_mut().find(|note| note.path == path) {
                    (note.tags, note.links) = Note::index(&old);
                    note.set_body(old);
                }
                back += 1;
            } else {
                kept += 1;
            }
        }
        self.after_swap(cx);
        let mut done = format!("{} {back}", tr("Notes restored:", "Notes rétablies :"));
        if kept > 0 {
            done += &format!(" · {} {kept}", tr("changed since, left as they are:", "modifiées depuis, laissées telles quelles :"));
        }
        self.say(if back + kept == 0 { Tone::Info } else { Tone::Done }, if back + kept == 0 { tr("No replacement to undo", "Aucun remplacement à annuler").to_string() } else { done }, cx);
    }

    /// Après une réécriture de notes : celle qui est ouverte est relue, le graphe suit.
    fn after_swap(&mut self, cx: &mut Context<Self>) {
        self.graph_stale = true;
        self.refresh_graph(cx);
        self.each_pane(|this| this.reload(cx));
        cx.notify();
    }

    /// Relit depuis le disque la note affichée, réécrite en dehors de l'éditeur.
    fn reload(&mut self, cx: &mut Context<Self>) {
        if let Some(path) = self.path.clone() {
            // Relire n'est pas ouvrir : l'aperçu reste un aperçu, la page liste reste affichée.
            let (preview, listing) = (self.preview, self.listing.take());
            self.load_note(&path, cx);
            (self.preview, self.listing) = (preview, listing);
        }
    }

    /// Met le disque à jour : chaque pane écrit ce qui y attend.
    fn flush(&mut self, cx: &mut Context<Self>) {
        self.each_pane(|this| this.flush_one(cx));
    }

    /// Écrit la note du pane sur disque si elle a changé.
    // ponytail: écriture synchrone sur le thread UI (quelques Ko, < 1 ms) ;
    // passer en tâche de fond si les notes deviennent très grosses.
    fn flush_one(&mut self, cx: &mut Context<Self>) {
        self.flush_sheet(cx);
        let Some(root) = self.vault.clone() else {
            return;
        };
        if !self.dirty {
            return;
        }
        let content = self.editor.read(cx).text().to_string();
        if self.path.is_none() && content.trim().is_empty() {
            self.dirty = false;
            return;
        }
        // Un titre `#` ajouté ou modifié en tête : le fichier porte désormais son nom.
        if vault::h1_of(&content).is_some_and(|h1| self.h1.as_ref() != Some(&h1)) {
            self.synced = true;
        }
        let dir = self.new_dir.as_deref().unwrap_or(&root);
        match vault::save(dir, self.path.as_deref(), self.synced, &content) {
            Ok(path) => {
                self.dirty = false;
                self.calm();
                let (tags, links) = markdown::index(&content);
                // Un modèle n'a ni tags ni liens.
                let (tags, links) = if vault::in_templates(&root, &path) { (Vec::new(), Vec::new()) } else { (tags, links) };
                let old = self.path.clone();
                // Le graphe ne change que si la note est nouvelle, renommée ou liée autrement.
                let known = self.notes.iter().find(|n| Some(&n.path) == old.as_ref());
                if known.is_none_or(|n| n.path != path || n.links[..] != links[..]) {
                    self.graph_stale = true;
                }
                self.notes.retain(|n| Some(&n.path) != old.as_ref() && n.path != path);
                let body: Arc<str> = Arc::from(content);
                self.notes.insert(
                    0,
                    Note {
                        name: vault::stem(&path),
                        tags: tags.into(),
                        links: links.into(),
                        aliases: markdown::aliases(&body).into(),
                        mtime: SystemTime::now(),
                        path: path.clone(),
                        lower: logic::note::lowered(&body),
                        body,
                    },
                );
                if old.as_ref() != Some(&path) {
                    // Renommée par son titre, la note garde son icône.
                    if let Some(icon) = old.as_ref().and_then(|old| self.icons.remove(old)) {
                        self.icons.insert(path.clone(), icon);
                        self.save_icons(cx);
                    }
                    if let Some(old) = &old {
                        self.history.rename(old, &path);
                    }
                    self.recent.retain(|p| Some(p) != old.as_ref());
                    self.touch(&path);
                    self.nav.reveal(&path);
                    self.path = Some(path);
                    self.push_names(cx);
                    self.push_dirs(cx);
                }
                self.refresh_graph(cx);
            }
            Err(e) => {
                self.say(Tone::Failed, format!("{} : {e}", tr("Note not saved", "Note non enregistrée")), cx)
            }
        }
        cx.notify();
    }

    fn on_editor_event(
        &mut self,
        editor: &Entity<Editor>,
        event: &EditorEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pane) = self.panes.iter().position(|pane| pane.editor == *editor) else { return };
        // Un geste fait dans une note (lien suivi, menu, remplacement) vaut pour son pane ; la
        // frappe, elle, peut venir d'ailleurs (une carte du tableau, une cellule de la liste).
        if !matches!(event, EditorEvent::Changed) {
            self.active = pane;
        }
        match event {
            EditorEvent::Changed => {
                self.in_pane(pane, |this| {
                    // La clé du tableau vient d'être écrite dans le texte : on y reste, le
                    // bouton du coin montre le tableau quand on le veut.
                    let board = kanban::is_board(this.editor.read(cx).text());
                    this.board_text |= board && !this.board_was;
                    this.board_was = board;
                    this.keep_preview();
                    this.dirty = true;
                });
                self.save_gen += 1;
                let generation = self.save_gen;
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(Duration::from_millis(400)).await;
                    this.update(cx, |this, cx| {
                        if this.save_gen == generation {
                            this.flush(cx);
                        }
                    })
                    .ok();
                })
                .detach();
            }
            EditorEvent::Open(link) => self.follow(link, window, cx),
            EditorEvent::OpenAside(link) => match link {
                Link::Wiki(name) => match self.wiki_path(name) {
                    Some(path) => self.open_aside(&path, window, cx),
                    None => self.follow(link, window, cx),
                },
                _ => self.follow(link, window, cx),
            },
            EditorEvent::Swap(swap) => self.replace_in_vault(swap.clone(), window, cx),
            EditorEvent::Menu(at, link, commented) => {
                self.menu = Some(nav::Menu { at: *at, target: None, text: Some(link.clone()), commented: *commented, block: false });
                cx.notify();
            }
            EditorEvent::BlockMenu(at) => {
                self.menu = Some(nav::Menu { at: *at, target: None, text: Some(None), commented: false, block: true });
                cx.notify();
            }
            // Le diagramme prend la place de la note, comme une image ; Échap y ramène.
            EditorEvent::View(source) => {
                let (source, theme) = (viewer::Source::Mermaid(source.clone()), self.theme);
                let figure = cx.new(|cx| viewer::Viewer::new(source, theme, cx));
                window.focus(&figure.focus_handle(cx));
                self.figure = Some(figure);
                cx.notify();
            }
        }
    }

    /// Suit un lien de la note : l'adresse dans le navigateur, le tag dans la palette, la note.
    pub fn follow(&mut self, link: &Link, window: &mut Window, cx: &mut Context<Self>) {
        match link {
            Link::Url(url) => cx.open_url(url),
            Link::Tag(tag) => self.open_palette(&format!("#{tag}"), window, cx),
            // Un dossier se lit comme une page liste, quand aucune note ne porte son nom.
            Link::Wiki(name) if self.wiki_path(name).is_none() && self.wiki_file(name).is_none() => match self.wiki_dir(name) {
                Some(dir) => self.open_listing(dir, window, cx),
                None => self.open_wiki(name, cx),
            },
            Link::Wiki(name) => self.open_wiki(name, cx),
        }
    }

    /// Le fichier du coffre (image, tableau, pièce jointe) que désigne `[[nom.ext]]`, sans tenir compte de la casse.
    fn wiki_file(&self, name: &str) -> Option<PathBuf> {
        let wanted = name.to_lowercase();
        self.images.iter().find(|path| path.file_name().is_some_and(|n| n.to_string_lossy().to_lowercase() == wanted)).cloned()
    }

    /// Le dossier du coffre que désigne `[[Nom]]`, sauf celui des modèles.
    fn wiki_dir(&self, name: &str) -> Option<PathBuf> {
        let (root, wanted) = (self.vault.as_ref()?, name.to_lowercase());
        self.dirs.iter().find(|dir| !vault::in_templates(root, &dir.join("x")) && dir.file_name().is_some_and(|n| n.to_string_lossy().to_lowercase() == wanted)).cloned()
    }

    /// Le schéma est enregistré à chaque changement : rien n'attend en mémoire.
    // ponytail: écriture synchrone à chaque geste ou frappe (quelques Ko) ;
    // la différer comme celle des notes si de gros schémas font attendre.
    fn on_canvas_event(&mut self, canvas: Entity<Canvas>, event: &CanvasEvent, cx: &mut Context<Self>) {
        let shows = |pane: &Pane| pane.drawing.as_ref().is_some_and(|(_, open)| *open == canvas);
        if let Some(pane) = self.panes.iter().position(shows) {
            self.in_pane(pane, |this| this.canvas_event(canvas, event, cx));
        }
    }

    fn canvas_event(&mut self, canvas: Entity<Canvas>, event: &CanvasEvent, cx: &mut Context<Self>) {
        let Some((path, _)) = self.drawing.as_ref().filter(|(_, open)| *open == canvas) else {
            return;
        };
        let result = match event {
            CanvasEvent::Changed => vault::write(path, &canvas.read(cx).diagram().to_svg(diagram::COLORS[0])),
            CanvasEvent::Export => return self.export_png(path.clone(), &canvas, cx),
        };
        match result {
            Ok(()) => self.calm(),
            Err(e) => self.say(Tone::Failed, format!("{} : {e}", tr("Diagram not saved", "Schéma non enregistré")), cx),
        }
        cx.notify();
    }

    /// Image PNG du schéma, à côté de lui : encre sombre sur fond blanc, pour
    /// qu'elle se colle partout. Elle est dessinée hors du thread UI.
    fn export_png(&mut self, file: PathBuf, canvas: &Entity<Canvas>, cx: &mut Context<Self>) {
        let svg = canvas.read(cx).diagram().to_svg(0x1e1e1e);
        let canvas = canvas.downgrade();
        cx.spawn(async move |this, cx| {
            let drawn = cx.background_executor().spawn(async move {
                let bytes = figure::png(&svg, 2.).ok_or_else(|| std::io::Error::other("PNG"))?;
                let path = vault::free_path(file.parent().unwrap_or(Path::new("")), &vault::stem(&file), "png");
                fs::write(&path, bytes).map(|_| path)
            });
            let saved = drawn.await;
            this.update(cx, |this, cx| {
                match saved {
                    Ok(path) => {
                        this.images.push(path);
                        this.push_names(cx);
                        this.graph_stale = true;
                        this.refresh_graph(cx);
                        canvas.update(cx, |canvas, cx| canvas.exported(cx)).ok();
                    }
                    Err(e) => this.say(Tone::Failed, format!("{} : {e}", tr("Picture not saved", "Image non enregistrée")), cx),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Dossier d'un nouveau schéma : celui sélectionné dans l'arbre, sinon celui de la note.
    fn diagram_dir(&self) -> Option<PathBuf> {
        let beside = self.path.as_deref().and_then(Path::parent).map(Path::to_path_buf);
        self.nav.target_dir().or(beside).or(self.vault.clone())
    }

    fn new_diagram(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(dir) = self.diagram_dir().filter(|_| self.vault.is_some()) {
            self.add_diagram(&dir, tr("Diagram", "Schéma"), Diagram::default(), window, cx);
        }
    }

    /// Demande un fichier Excalidraw ou draw.io, et en fait un schéma du coffre.
    fn import_diagram(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.vault.is_none() {
            return;
        }
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(tr("Import", "Importer").into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = paths.await
                && let Some(file) = paths.into_iter().next()
            {
                this.update_in(cx, |this, window, cx| this.import_file(&file, window, cx)).ok();
            }
        })
        .detach();
    }

    fn import_file(&mut self, file: &Path, window: &mut Window, cx: &mut Context<Self>) {
        let drawio = file.extension().is_some_and(|e| e == "drawio" || e == "xml");
        let read = fs::read_to_string(file).map_err(|e| e.to_string()).and_then(|text| match drawio {
            true => import::drawio(&text),
            false => import::excalidraw(&text),
        });
        match (read, self.diagram_dir()) {
            (Ok(diagram), Some(dir)) => self.add_diagram(&dir, &vault::stem(file), diagram, window, cx),
            (Err(e), _) => self.say(Tone::Failed, format!("{} : {e}", tr("Import failed", "Import impossible")), cx),
            _ => {}
        }
        cx.notify();
    }

    /// Enregistre `diagram` dans `dir` sous un nom libre, et l'ouvre.
    fn add_diagram(&mut self, dir: &Path, name: &str, diagram: Diagram, window: &mut Window, cx: &mut Context<Self>) {
        let path = vault::free_path(dir, name, "svg");
        if let Err(e) = vault::write(&path, &diagram.to_svg(diagram::COLORS[0])) {
            return self.say(Tone::Failed, format!("{} : {e}", tr("Diagram not saved", "Schéma non enregistré")), cx);
        }
        self.images.push(path.clone());
        self.push_names(cx);
        self.graph_stale = true;
        self.refresh_graph(cx);
        self.nav.reveal(&path);
        self.load_note(&path, cx);
        if self.nav.panel == Panel::Full {
            self.nav.panel = Panel::Split;
        }
        if let Some((_, canvas)) = &self.drawing {
            window.focus(&canvas.focus_handle(cx));
        } else if let Some((_, viewer)) = &self.viewer {
            window.focus(&viewer.focus_handle(cx));
        }
        self.settle_nav(window, cx);
    }

    /// Donne l'icône `icon` à la note, au fichier ou au dossier `path` ; `None` la lui retire.
    pub fn set_icon(&mut self, path: PathBuf, icon: Option<&str>, cx: &mut Context<Self>) {
        match icon {
            Some(icon) => self.icons.insert(path, icon.to_string()),
            None => self.icons.remove(&path),
        };
        self.save_icons(cx);
    }

    pub fn save_icons(&mut self, cx: &mut Context<Self>) {
        if let Some(root) = &self.vault
            && let Err(e) = vault::save_icons(root, &self.icons)
        {
            self.say(Tone::Failed, format!("{} : {e}", tr("Icons not saved", "Icônes non enregistrées")), cx);
        }
        cx.notify();
    }

    /// Lie la capture à un raccourci du bureau, et dit lequel. Là où Bref ne peut pas le
    /// poser lui-même, la commande à lier est copiée : il ne reste qu'à la coller.
    fn capture_shortcut(&mut self, cx: &mut Context<Self>) {
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let made = cx.background_executor().spawn(async move { vault::capture_shortcut(&exe) }).await;
            this.update(cx, |this, cx| {
                let told = match made {
                    vault::Shortcut::Bound(keys) => format!("{} : {keys}", tr("Quick capture, from anywhere", "Capture rapide, depuis n'importe où")),
                    vault::Shortcut::Unbound(keys) => format!(
                        "{keys} {}",
                        tr("is taken: choose the keys of “Bref: capture” in Settings › Keyboard", "est pris : choisir les touches de « Bref : capture » dans Réglages › Clavier")
                    ),
                    vault::Shortcut::Manual(command) => {
                        cx.write_to_clipboard(ClipboardItem::new_string(command));
                        tr("Capture command copied: paste it into a new shortcut of your system settings", "Commande de capture copiée : la coller dans un nouveau raccourci des réglages du système").to_string()
                    }
                };
                this.say(Tone::Done, told, cx);
            })
            .ok();
        })
        .detach();
    }

    /// Ouvre le terminal du système dans le coffre, ou dit qu'il n'en a pas trouvé.
    fn open_terminal(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.vault.clone() else {
            return;
        };
        if !vault::open_terminal(&root) {
            let hint = tr("No terminal found: name yours in $TERMINAL", "Aucun terminal trouvé : nommer le vôtre dans $TERMINAL");
            self.say(Tone::Failed, hint, cx);
        }
    }

    /// Sauvegarde du coffre : demande où la ranger.
    fn choose_backup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(tr("Save the backup here", "Enregistrer la sauvegarde ici").into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = paths.await
                && let Some(dir) = paths.into_iter().next()
            {
                this.update(cx, |this, cx| this.backup_to(dir, cx)).ok();
            }
        })
        .detach();
    }

    /// Écrit dans `dir`, en tâche de fond, l'archive datée de tout le coffre, puis dit où elle est.
    fn backup_to(&mut self, dir: PathBuf, cx: &mut Context<Self>) {
        let Some(root) = self.vault.clone() else {
            return;
        };
        self.flush(cx);
        self.say(Tone::Busy, tr("Backing up the vault…", "Sauvegarde du coffre…"), cx);
        let date = date_name(today());
        cx.spawn(async move |this, cx| {
            let done = cx.background_executor().spawn(async move { vault::backup(&root, &dir, &date) }).await;
            this.update(cx, |this, cx| match done {
                Ok(to) => this.say(Tone::Done, format!("{} {}", tr("Backup saved:", "Sauvegarde enregistrée :"), to.display()), cx),
                Err(e) => this.say(Tone::Failed, format!("{} : {e}", tr("Backup failed", "Sauvegarde impossible")), cx),
            })
            .ok();
        })
        .detach();
    }

    /// Corbeille du coffre : la liste de ce qu'elle contient ; Entrée remet l'élément choisi à
    /// la racine du coffre, où le suivi des fichiers le retrouve.
    fn open_trash(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = self.vault.clone() else {
            return;
        };
        let found = vault::trashed(&root);
        if found.is_empty() {
            return self.say(Tone::Info, tr("The trash is empty", "La corbeille est vide"), cx);
        }
        let names = found.iter().map(|p| p.file_name().unwrap_or_default().to_string_lossy().into_owned()).collect();
        let theme = self.theme;
        let palette = cx.new(|cx| Palette::choose(tr("Restore from the trash", "Restaurer depuis la corbeille"), names, "", theme, cx));
        cx.subscribe_in(&palette, window, move |this, palette, event, window, cx| {
            if matches!(event, PaletteEvent::Preview(_)) {
                return;
            }
            let chosen = palette.read(cx).chosen().and_then(|i| found.get(i)).filter(|_| matches!(event, PaletteEvent::Submit(_)));
            match chosen.map(|path| vault::restore(&root, path)) {
                Some(Ok(to)) => {
                    let name = to.file_name().unwrap_or_default().to_string_lossy().into_owned();
                    this.say(Tone::Done, format!("{} {name}", tr("Restored:", "Restauré :")), cx);
                }
                Some(Err(e)) => this.say(Tone::Failed, format!("{} : {e}", tr("Not restored", "Restauration impossible")), cx),
                None => {}
            }
            this.palette = None;
            window.focus(&this.editor.focus_handle(cx));
            cx.notify();
        })
        .detach();
        window.focus(&palette.focus_handle(cx));
        self.palette = Some(palette);
        cx.notify();
    }

    /// Note du jour : la note qui porte la date locale pour nom, où qu'elle soit dans le
    /// coffre ; créée avec cette date pour titre si elle n'existe pas encore.
    fn open_today(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.vault.clone() else {
            return;
        };
        let name = date_name(today());
        // Pas encore de note du jour, et un modèle `Daily` ou `Journal` : elle en naît.
        if self.wiki_path(&name).is_none()
            && let Some(template) = self.daily_template()
            && self.create_from(&template, &name, &root, cx).is_some()
        {
            return;
        }
        self.open_wiki(&name, cx);
    }

    /// Ce qu'un dépôt ferait, tant qu'il dure : l'étiquette près du pointeur, le trait dans la note.
    fn aim_drop(&mut self, aim: Option<DropAim>, cx: &mut Context<Self>) {
        if aim == self.drop_aim {
            return;
        }
        let mark = |aim: &Option<DropAim>| match aim {
            Some(DropAim { aim: Aim::Note { pane, at, .. }, .. }) => Some((*pane, *at)),
            _ => None,
        };
        let (old, new) = (mark(&self.drop_aim), mark(&aim));
        if old != new {
            for (n, pane) in self.panes.iter().enumerate() {
                let at = new.filter(|(pane, _)| *pane == n).map(|(_, at)| at);
                pane.editor.update(cx, |editor, cx| editor.set_drop_mark(at, cx));
            }
        }
        self.drop_aim = aim;
        cx.notify();
    }

    /// Des fichiers et des dossiers lâchés sur la fenêtre : copiés dans le coffre (l'original ne bouge pas, un
    /// nom pris prend un numéro), puis, sur une note, nommés par une ligne chacun là où le trait était.
    fn drop_paths(&mut self, paths: Vec<PathBuf>, aim: Aim, cx: &mut Context<Self>) {
        let Some(root) = self.vault.clone() else {
            return;
        };
        let dir = match &aim {
            Aim::Note { pane, .. } => {
                self.panes.get(*pane).and_then(|p| p.path.as_deref()).and_then(Path::parent).map(Path::to_path_buf).unwrap_or_else(|| root.clone())
            }
            Aim::Folder(dir) => dir.clone(),
            Aim::Root => root.clone(),
        };
        self.aim_drop(None, cx);
        if paths.is_empty() {
            return;
        }
        self.say(Tone::Busy, tr("Copying…", "Copie…"), cx);
        cx.spawn(async move |this, cx| {
            let copied = cx.background_executor().spawn({
                let dir = dir.clone();
                async move { vault::import(&paths, &dir) }
            });
            let (done, problems) = copied.await;
            this.update(cx, |this, cx| this.dropped(done, problems, &root, &dir, aim, cx)).ok();
        })
        .detach();
    }

    /// Ce que la copie a rangé : les nouveaux fichiers dans l'arbre, leurs liens dans la note, un mot sur ce
    /// qui a été laissé de côté.
    fn dropped(&mut self, done: Vec<vault::Imported>, problems: Vec<String>, root: &Path, dir: &Path, aim: Aim, cx: &mut Context<Self>) {
        self.toasts.retain(|t| t.tone != Tone::Busy);
        if done.is_empty() {
            let why = problems.first().cloned().unwrap_or_default();
            return self.say(Tone::Failed, format!("{} : {why}", tr("Nothing copied", "Rien n'a été copié")), cx);
        }
        self.rescan(root, cx);
        if let Some(first) = done.first() {
            self.nav.open.insert(dir.to_path_buf());
            self.nav.reveal(&first.to);
        }
        if let Aim::Note { pane, at, version } = aim {
            // Une ligne par élément : l'image s'affiche, un autre fichier se lie par son nom.
            let lines: Vec<String> = done
                .iter()
                // Un type que le coffre ne liste pas (`.exe`…) est copié, mais n'a pas de lien qui s'ouvre.
                .filter(|item| item.dir || vault::is_file_shown(&item.to) || item.to.extension().is_some_and(|e| e.eq_ignore_ascii_case("md")))
                .map(|item| {
                    let name = item.to.file_name().unwrap_or_default().to_string_lossy();
                    match (item.dir, vault::is_image(&item.to)) {
                        (false, true) => format!("![[{name}]]"),
                        (false, false) if item.to.extension().is_some_and(|e| e.eq_ignore_ascii_case("md")) => format!("[[{}]]", vault::stem(&item.to)),
                        _ => format!("[[{name}]]"),
                    }
                })
                .collect();
            if let Some(editor) = self.panes.get(pane).map(|p| p.editor.clone()).filter(|_| !lines.is_empty()) {
                // Le texte a changé pendant la copie : les lignes vont où est le curseur.
                editor.update(cx, |editor, cx| {
                    let at = if editor.revision() == version { at } else { editor.caret_line_start() };
                    editor.insert_lines(at, &lines, cx);
                });
            }
        }
        let files: usize = done.iter().map(|d| d.files.max(1)).sum();
        let place = match dir.strip_prefix(root) {
            Ok(rel) if !rel.as_os_str().is_empty() => rel.display().to_string(),
            _ => tr("the vault", "le coffre").to_string(),
        };
        let mut said = format!("{} {} {} {place}", tr("Copied", "Copié"), files, tr(if files == 1 { "file into" } else { "files into" }, if files == 1 { "fichier dans" } else { "fichiers dans" }));
        if !problems.is_empty() {
            said.push_str(&format!(" · {} {}", problems.len(), tr("left out", "laissés de côté")));
        }
        self.say(if problems.is_empty() { Tone::Done } else { Tone::Info }, said, cx);
    }

    /// Où des fichiers glissés en `at` se rangeraient dans la note du pane `pane` : seulement si une note s'y écrit.
    fn note_aim(&self, pane: usize, at: Point<Pixels>, items: usize, cx: &App) -> Option<DropAim> {
        let shown = self.panes.get(pane)?;
        let editor = shown.editor.read(cx);
        let board = kanban::is_board(editor.text()) && !shown.board_text;
        let writes = shown.picture.is_none() && shown.listing.is_none() && !board && self.vault.is_some();
        writes.then(|| DropAim { aim: Aim::Note { pane, at: editor.drop_point(at), version: editor.revision() }, items, pointer: at })
    }

    /// L'étiquette qui suit le pointeur pendant qu'on glisse des fichiers sur la fenêtre.
    fn render_drop_chip(&self, view: Size<Pixels>) -> Option<impl IntoElement + use<>> {
        let (aim, t) = (self.drop_aim.as_ref()?, self.theme);
        let name = |path: &Path| path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let (verb, place) = match &aim.aim {
            Aim::Note { pane, .. } => {
                let title = self.panes.get(*pane).and_then(|p| p.path.as_deref()).map(vault::stem).unwrap_or_else(|| tr("New note", "Nouvelle note").to_string());
                (tr("Insert in", "Insérer dans"), title)
            }
            Aim::Folder(dir) => (tr("Copy into", "Copier dans"), name(dir)),
            Aim::Root => (tr("Copy into", "Copier dans"), tr("the vault", "le coffre").to_string()),
        };
        let count = format!("{} {}", aim.items, tr(if aim.items == 1 { "item" } else { "items" }, if aim.items == 1 { "élément" } else { "éléments" }));
        // Près d'un bord, l'étiquette passe de l'autre côté du pointeur : elle reste dans la fenêtre.
        let (left, above) = (aim.pointer.x > view.width * 0.6, aim.pointer.y > view.height - px(70.));
        Some(
            div()
                .debug_selector(|| "drop-chip".into())
                .absolute()
                .map(|d| if left { d.right(view.width - aim.pointer.x + px(14.)) } else { d.left(aim.pointer.x + px(14.)) })
                .map(|d| if above { d.bottom(view.height - aim.pointer.y + px(14.)) } else { d.top(aim.pointer.y + px(18.)) })
                .max_w(px(420.))
                .truncate()
                .px_2()
                .py_1()
                .rounded(px(8.))
                .bg(t.panel)
                .border_1()
                .border_color(t.accent.opacity(0.6))
                .text_size(px(12.))
                .text_color(t.text)
                .flex()
                .gap_1()
                .child(div().text_color(t.accent).child(verb))
                .child(format!("« {place} » · {count}")),
        )
    }

    /// Les modèles du coffre, par nom.
    fn templates(&self) -> Vec<PathBuf> {
        let Some(root) = &self.vault else {
            return Vec::new();
        };
        let mut found: Vec<PathBuf> = self.notes.iter().filter(|n| vault::in_templates(root, &n.path)).map(|n| n.path.clone()).collect();
        found.sort_by_key(|path| vault::natural(&vault::stem(path)));
        found
    }

    /// Le modèle de la note du jour : celui qui s'appelle `Daily`, `Journal`, `Today`, `Note du jour`…
    fn daily_template(&self) -> Option<PathBuf> {
        const NAMES: [&str; 5] = ["daily", "journal", "today", "note du jour", "aujourd'hui"];
        self.templates().into_iter().find(|path| NAMES.contains(&lang::fold(&vault::stem(path)).as_str()))
    }

    /// Un modèle à choisir, puis, pour une nouvelle note, son nom ; ou, pour l'insérer, rien d'autre.
    fn open_templates(&mut self, act: TemplateAct, window: &mut Window, cx: &mut Context<Self>) {
        if self.vault.is_none() {
            return;
        }
        let found = self.templates();
        if found.is_empty() {
            return self.say(
                Tone::Info,
                tr(
                    "No template: put notes in a folder named Templates (or Modèles) at the root of the vault",
                    "Aucun modèle : rangez des notes dans un dossier Templates (ou Modèles) à la racine du coffre",
                ),
                cx,
            );
        }
        let names = found.iter().map(|path| vault::stem(path)).collect();
        let label = if act == TemplateAct::New { tr("New note from a template", "Nouvelle note depuis un modèle") } else { tr("Insert a template", "Insérer un modèle") };
        let theme = self.theme;
        let palette = cx.new(|cx| Palette::choose(label, names, "", theme, cx));
        cx.subscribe_in(&palette, window, move |this, palette, event, window, cx| {
            if matches!(event, PaletteEvent::Preview(_)) {
                return;
            }
            let chosen = palette.read(cx).chosen().and_then(|i| found.get(i)).cloned().filter(|_| matches!(event, PaletteEvent::Submit(_)));
            this.palette = None;
            window.focus(&this.editor.focus_handle(cx));
            match (chosen, act) {
                (Some(template), TemplateAct::New) => {
                    // La note naît dans le dossier choisi dans l'arbre, jamais dans celui des modèles.
                    let root = this.vault.clone().unwrap_or_default();
                    let dir = this.nav.target_dir().filter(|dir| !vault::in_templates(&root, &dir.join("x.md"))).unwrap_or(root);
                    this.ask(tr("Name of the note", "Nom de la note"), "", window, cx, move |this, name, window, cx| {
                        // La note née, on écrit : le curseur est à sa marque, le focus à l'éditeur.
                        if this.create_from(&template, &name, &dir, cx).is_some() {
                            window.focus(&this.editor.focus_handle(cx));
                        }
                    });
                }
                (Some(template), _) => this.insert_template(&template, cx),
                (None, _) => {}
            }
            cx.notify();
        })
        .detach();
        window.focus(&palette.focus_handle(cx));
        self.palette = Some(palette);
        cx.notify();
    }

    /// La note `name` dans `dir`, écrite d'après le modèle, ouverte, le curseur à sa marque. Rien n'est
    /// écrasé : un nom pris est refusé. `None` si elle n'a pas pu naître.
    fn create_from(&mut self, template: &Path, name: &str, dir: &Path, cx: &mut Context<Self>) -> Option<PathBuf> {
        let root = self.vault.clone()?;
        let text = match fs::read_to_string(template) {
            Ok(text) => text,
            Err(e) => {
                self.say(Tone::Failed, format!("{} : {e}", tr("Template not read", "Modèle illisible")), cx);
                return None;
            }
        };
        let path = dir.join(format!("{name}.md"));
        if path.exists() {
            self.say(Tone::Failed, format!("{} « {name} »", tr("A note already has the name", "Une note porte déjà le nom")), cx);
            return None;
        }
        let (body, cursor) = markdown::fill(&text, name, &date_name(today()), &time_name(clock()));
        if let Err(e) = vault::write(&path, &body) {
            self.say(Tone::Failed, format!("{} : {e}", tr("Note not created", "Note non créée")), cx);
            return None;
        }
        self.rescan(&root, cx);
        self.nav.reveal(&path);
        self.open_note(&path, cx);
        self.editor.update(cx, |editor, cx| editor.jump(cursor, false, cx));
        Some(path)
    }

    /// Le modèle, rempli au nom de la note ouverte, à la place du curseur.
    fn insert_template(&mut self, template: &Path, cx: &mut Context<Self>) {
        let text = match fs::read_to_string(template) {
            Ok(text) => text,
            Err(e) => return self.say(Tone::Failed, format!("{} : {e}", tr("Template not read", "Modèle illisible")), cx),
        };
        let title = self.path.as_deref().map(vault::stem).unwrap_or_default();
        let (body, cursor) = markdown::fill(&text, &title, &date_name(today()), &time_name(clock()));
        self.editor.update(cx, |editor, cx| editor.insert_filled(&body, cursor, cx));
    }

    /// La note ouverte, rangée comme modèle : son titre devient `{{title}}`.
    fn save_as_template(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.vault.is_none() || self.picture.is_some() || self.listing.is_some() {
            return;
        }
        self.flush(cx);
        let text = markdown::templatize(self.editor.read(cx).text());
        let name = self.path.as_deref().map(vault::stem).unwrap_or_default();
        self.ask(tr("Name of the template", "Nom du modèle"), &name, window, cx, move |this, name, window, cx| {
            window.focus(&this.editor.focus_handle(cx));
            let Some(root) = this.vault.clone() else { return };
            let dir = vault::template_dir(&root, &this.dirs).unwrap_or_else(|| root.join("Templates"));
            let path = dir.join(format!("{name}.md"));
            if path.exists() {
                return this.say(Tone::Failed, format!("{} « {name} »", tr("A template already has the name", "Un modèle porte déjà le nom")), cx);
            }
            match fs::create_dir_all(&dir).and_then(|()| vault::write(&path, &text)) {
                Ok(()) => {
                    this.rescan(&root, cx);
                    this.say(Tone::Done, format!("{} {name}", tr("Template saved:", "Modèle enregistré :")), cx);
                }
                Err(e) => this.say(Tone::Failed, format!("{} : {e}", tr("Template not saved", "Modèle non enregistré")), cx),
            }
        });
    }

    /// La note que désigne un `[[lien]]` : par son nom d'abord, sinon par un alias de son
    /// en-tête YAML.
    fn wiki_path(&self, name: &str) -> Option<PathBuf> {
        let wanted = name.to_lowercase();
        // Un modèle ne répond à aucun lien : `[[Réunion]]` vise la note, pas son modèle.
        let mut notes = self.notes.iter().filter(|n| !self.is_template(n));
        let named = notes.clone().find(|n| n.name.to_lowercase() == wanted);
        named.or_else(|| notes.find(|n| n.answers(&wanted))).map(|note| note.path.clone())
    }

    fn open_wiki(&mut self, name: &str, cx: &mut Context<Self>) {
        match self.wiki_path(name).or_else(|| self.wiki_file(name)) {
            Some(path) => self.open_note(&path, cx),
            None => self.new_note(format!("# {name}\n\n"), cx),
        }
    }

    fn open_palette(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.show_palette(query, false, window, cx)
    }

    /// Recherche dans le texte de tout le coffre : la palette, où chaque choix est une ligne.
    fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_palette("", true, window, cx)
    }

    fn show_palette(&mut self, query: &str, lines: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.vault.is_none() {
            return;
        }
        self.flush(cx);
        let entries = self
            .by_recency()
            .into_iter()
            .map(|n| Entry {
                name: n.name.clone(),
                path: n.path.clone(),
                tags: n.tags.clone(),
                body: n.body.clone(),
                lower: n.lower.clone(),
            })
            // Les autres fichiers du coffre (tableaux, images, schémas) se trouvent aussi par leur nom.
            .chain(self.images.iter().map(|path| Entry {
                name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                path: path.clone(),
                ..Entry::default()
            }))
            .collect();
        let theme = self.theme;
        let updates = self.prefs.updates;
        let installable = self.update.as_ref().filter(|r| r.asset.is_some()).map(|r| r.version.clone());
        let table = self.shown_sheet().is_some();
        let lists = self.picture.is_none() && self.listing.is_none() && !self.board_shown(cx);
        let cells = lists && self.editor.read(cx).in_table();
        let swap = !self.swapped.is_empty();
        let (commands, here) = self.palette_commands(window, cx);
        let context = self.here_group(cx).1;
        let palette = cx.new(|cx| match lines {
            true => Palette::search(entries, theme, cx),
            false => Palette::new(entries, query, theme, cx)
                .with_updates(updates, installable)
                .with_table(table)
                .with_lists(lists)
                .with_cells(cells)
                .with_undo_swap(swap)
                .with_commands(commands, here, context),
        });
        // Une commande choisie dans la palette va à ce qui avait la saisie avant elle.
        let back = window.focused(cx);
        cx.subscribe_in(&palette, window, move |this, palette, event, window, cx| {
            this.palette = None;
            window.focus(&this.editor.focus_handle(cx));
            let aside = palette.read(cx).aside;
            match event {
                PaletteEvent::Open(path) if aside => this.open_aside(path, window, cx),
                PaletteEvent::Open(path) => {
                    this.open_note(path, cx);
                    // Un tableau, une image ou un schéma prend la saisie : rendue à la note, elle les effacerait.
                    this.focus_pane(this.active, window, cx);
                }
                PaletteEvent::OpenAt(path, row, query) => {
                    this.open_note(path, cx);
                    this.editor.update(cx, |e, cx| e.select_in_row(*row, query, cx));
                }
                PaletteEvent::Create(name) => this.open_wiki(name, cx),
                PaletteEvent::ChangeVault => this.choose_vault(window, cx),
                PaletteEvent::Help => this.set_help(true, window, cx),
                PaletteEvent::NewDiagram => this.new_diagram(window, cx),
                PaletteEvent::NewFolder => this.new_folder(window, cx),
                PaletteEvent::Table(pick) => this.choose_table(*pick, window, cx),
                PaletteEvent::ImportDiagram => this.import_diagram(window, cx),
                PaletteEvent::Setting(setting) => this.choose_setting(*setting, window, cx),
                PaletteEvent::ToggleUpdates => this.toggle_updates(cx),
                PaletteEvent::CheckUpdate => this.check_now(cx),
                PaletteEvent::Outline => this.open_outline(window, cx),
                PaletteEvent::Comments => this.open_comments(window, cx),
                PaletteEvent::Template(TemplateAct::Save) => this.save_as_template(window, cx),
                PaletteEvent::Template(act) => this.open_templates(*act, window, cx),
                PaletteEvent::Grid(op) => {
                    // Le geste va à ce qui est affiché : la feuille d'un CSV, ou la note.
                    match this.shown_sheet() {
                        Some(sheet) => window.focus(&sheet.focus_handle(cx)),
                        None => window.focus(&this.editor.focus_handle(cx)),
                    }
                    window.dispatch_action(op.action(), cx);
                }
                PaletteEvent::Run(name) => {
                    match &back {
                        Some(back) => window.focus(back),
                        None => this.focus_pane(this.active, window, cx),
                    }
                    if let Ok(action) = cx.build_action(name, None) {
                        window.dispatch_action(action, cx);
                    }
                }
                PaletteEvent::Restyle(style) => {
                    window.focus(&this.editor.focus_handle(cx));
                    this.editor.update(cx, |e, cx| e.restyle(*style, cx));
                }
                PaletteEvent::UndoSwap => this.undo_swap(cx),
                PaletteEvent::NewBoard => this.new_note(kanban::new_board(), cx),
                PaletteEvent::Today => this.open_today(cx),
                PaletteEvent::Travel(forward) => this.travel(*forward, window, cx),
                PaletteEvent::Trash => this.open_trash(window, cx),
                PaletteEvent::Backup => this.choose_backup(window, cx),
                PaletteEvent::Sync => this.git_sync(window, cx),
                PaletteEvent::CloneVault => this.ask_clone(window, cx),
                PaletteEvent::Terminal => this.open_terminal(cx),
                PaletteEvent::CaptureKey => this.capture_shortcut(cx),
                PaletteEvent::InstallUpdate => this.install_update(cx),
                // Refermée sans choix : on retrouve ce que le pane montrait, tableau ou schéma compris.
                PaletteEvent::Dismiss => this.focus_pane(this.active, window, cx),
                PaletteEvent::Submit(_) | PaletteEvent::Preview(_) => {}
            }
            cx.notify();
        })
        .detach();
        window.focus(&palette.focus_handle(cx));
        self.palette = Some(palette);
        cx.notify();
    }
}

const KOFI: &str = "https://ko-fi.com/T6T01WC5ZC";

/// Contenu du panneau d'aide, et du même coup la liste des raccourcis qui se changent : une ligne
/// `k` par action, dont les touches sont celles en vigueur (`bound`) ; une ligne `t` pour ce qui
/// n'est pas une liaison (souris, texte tapé, commande de la palette, touches fixes).
fn help_sections(bound: &[keys::Bind], changes: &keys::Changes) -> keys::Sections {
    use gpui::Action;
    use keys::Row;
    let k = |action: &dyn Action, effect: &'static str| Row {
        keys: keys::shown(bound, action.name()),
        effect,
        action: Some(action.name()),
        changed: changes.contains_key(action.name()),
    };
    let t = |keys: &str, effect: &'static str| Row::text(keys, effect);
    // Une commande de la palette : ses touches, puis le mot à y taper.
    let p = |word: &str, effect: &'static str| {
        let palette = keys::shown(bound, OpenPalette.name());
        Row::text(format!("{} › {word}", if palette.is_empty() { "Palette" } else { &palette }), effect)
    };
    let click = tr("click", "clic");
    let shift = tr("Shift", "Maj");
    let word = if cfg!(target_os = "macos") { "Alt" } else { "Ctrl" };
    vec![
        (
            "Notes",
            vec![
                k(&OpenPalette, tr("Find by name or text, create, filter by #tag", "Chercher par nom ou texte, créer, filtrer par #tag")),
                k(&ToggleCommands, tr("Command palette: every command, found by what it does (also the first button of the rail, or > in the palette)", "Palette des commandes : toutes les commandes, trouvées par ce qu'elles font (aussi le premier bouton du rail, ou > dans la palette)")),
                k(&editor::Find, tr("Find in the note", "Chercher dans la note")),
                k(&editor::FindReplace, tr("Find and replace in the note", "Chercher et remplacer dans la note")),
                k(&SearchVault, tr("Search the text of the whole vault, line by line", "Chercher dans le texte de tout le coffre, ligne par ligne")),
                k(&editor::FindNext, tr("Search bar: next match (Enter too)", "Barre de recherche : passage suivant (Entrée aussi)")),
                k(&editor::FindPrev, tr("Search bar: previous match", "Barre de recherche : passage précédent")),
                k(&editor::FindCase, tr("Search bar: match case", "Barre de recherche : respecter la casse")),
                k(&editor::FindWord, tr("Search bar: whole words", "Barre de recherche : mots entiers")),
                k(&editor::FindRegex, tr("Search bar: regular expression ($1 in the replacement)", "Barre de recherche : expression régulière ($1 dans le remplacement)")),
                t("Tab", tr("Search bar: replace field", "Barre de recherche : champ de remplacement")),
                k(&editor::ReplaceAll, tr("Search bar: replace all (Enter: this match)", "Barre de recherche : tout remplacer (Entrée : ce passage)")),
                k(&editor::ReplaceInVault, tr("Search bar: replace in the whole vault, after showing the count", "Barre de recherche : remplacer dans tout le coffre, après en avoir montré le compte")),
                k(&Outline, tr("Outline: jump to a heading of the note", "Plan : aller à un titre de la note")),
                k(&NewNote, tr("New note", "Nouvelle note")),
                k(&NewFromTemplate, tr("New note from a template (a note of the Templates folder: {{title}} {{date}} {{time}} {{cursor}})", "Nouvelle note depuis un modèle (une note du dossier Templates ou Modèles : {{title}} {{date}} {{time}} {{cursor}})")),
                p(tr("template", "modèle"), tr("Insert a template at the cursor, or make a template of the open note", "Insérer un modèle au curseur, ou faire un modèle de la note ouverte")),
                k(&Today, tr("Today's note: open it, or create it", "Note du jour : l'ouvrir, ou la créer")),
                p("capture", tr("Quick capture: sets up a shortcut of your desktop (Super+Shift+N on GNOME) that opens a one-line window anywhere; what you type goes to today's note", "Capture rapide : crée un raccourci du bureau (Super+Maj+N sous GNOME) qui ouvre partout une fenêtre d'une ligne ; ce qu'on y tape va dans la note du jour")),
                k(&OpenVault, tr("Change vault", "Changer de coffre")),
                p("terminal", tr("Open a terminal in the vault", "Ouvrir un terminal dans le coffre")),
                k(&SyncVault, tr("Sync the vault with git: commit, receive, merge, send; a conflict keeps both versions as two notes", "Synchroniser le coffre par git : commiter, recevoir, fusionner, envoyer ; un conflit garde les deux versions en deux notes")),
                p("clone", tr("Clone a vault from a git address (also on the welcome screen)", "Cloner un coffre depuis une adresse git (aussi sur l'écran d'accueil)")),
                t(tr("A cloud folder", "Un dossier synchronisé"), tr("iCloud, Dropbox, Google Drive, Syncthing: put the vault there, nothing else; never a git repository inside one", "iCloud, Dropbox, Google Drive, Syncthing : y poser le coffre, rien d'autre ; jamais un dépôt git dans un tel dossier")),
                k(&CopyAll, tr("Copy the code block, else the note", "Copier le bloc de code, sinon la note")),
                t(&format!("{MOD}+{click}"), tr("Open a [[link]], #tag or URL", "Ouvrir un [[lien]], #tag ou URL")),
                k(&ToggleHelp, tr("This help", "Cette aide")),
                k(&Quit, tr("Quit", "Quitter")),
            ],
        ),
        (
            "Navigation",
            vec![
                k(&GoBack, tr("Back: what was shown before, at the same place (also the mouse button, and the rail)", "Précédent : ce qui était affiché avant, au même endroit (aussi le bouton de la souris, et le rail)")),
                k(&GoForward, tr("Forward: what was shown after going back", "Suivant : ce qui était affiché après un retour")),
                k(&SplitPane, tr("Second pane, side by side: opens on the palette, to choose what goes there", "Second volet, côte à côte : s'ouvre sur la palette, pour choisir ce qui y va")),
                k(&FocusLeft, tr("Go to the left pane", "Aller au volet de gauche")),
                k(&FocusRight, tr("Go to the right pane", "Aller au volet de droite")),
                k(&ClosePane, tr("Close the pane that has the focus", "Fermer le volet qui a la saisie")),
                k(&palette::ConfirmAside, tr("In the palette: open the chosen note in the other pane", "Dans la palette : ouvrir la note choisie dans l'autre volet")),
                t(&format!("{MOD}+{shift}+{click}"), tr("Open a [[link]] in the other pane", "Ouvrir un [[lien]] dans l'autre volet")),
                t(tr("Drag from the tree", "Glisser depuis l'arbre"), tr("Onto a pane: open the file there; onto the right half of a single pane: open a second one", "Sur un volet : y ouvrir le fichier ; sur la moitié droite du seul volet : en ouvrir un second")),
                k(&nav::ShowTree, tr("Vault tree", "Arbre du coffre")),
                k(&nav::ShowRecent, tr("Recent notes", "Notes récentes")),
                k(&nav::ShowGraph, tr("Graph of the notes", "Graphe des notes")),
                k(&nav::ShowTags, "Tags"),
                k(&nav::ShowLinks, tr("Backlinks: the notes that link to this one", "Rétroliens : les notes qui mènent à celle-ci")),
                k(&nav::ShowCalendar, tr("Calendar: arrows, Page Up/Down for the month, Enter opens the day", "Calendrier : flèches, Page haut/bas pour le mois, Entrée ouvre le jour")),
                t(tr("A picture", "Une image"), tr("Shown in place of the note; a square in the graph", "Affichée à la place de la note ; un carré dans le graphe")),
                t(tr("A .csv or .tsv", "Un .csv ou .tsv"), tr("A grid in place of the note; not in the graph", "Une grille à la place de la note ; absent du graphe")),
                k(&nav::ToggleFull, tr("Panel on the whole window", "Panneau en pleine fenêtre")),
                t(tr("Arrows / Tab", "Flèches / Tab"), tr("Select and preview / linked notes (graph)", "Sélectionner en aperçu / notes liées (graphe)")),
                t(tr("Enter / Esc", "Entrée / Échap"), tr("Open the note / back to the note", "Ouvrir la note / revenir à la note")),
                k(&nav::NewFolder, tr("New folder (also in the palette)", "Nouveau dossier (aussi dans la palette)")),
                t(tr("Point at an icon", "Pointer une icône"), tr("Its name and its shortcut", "Son nom et son raccourci")),
                t("Alt + drag", tr("Move the window from anywhere (Linux)", "Déplacer la fenêtre depuis n'importe où (Linux)")),
                k(&nav::Rename, tr("Panel: rename", "Panneau : renommer")),
                k(&nav::Duplicate, tr("Panel: duplicate", "Panneau : dupliquer")),
                k(&nav::Trash, tr("Panel: move to the trash", "Panneau : mettre à la corbeille")),
                t(&format!("{MOD}+{click} / {shift}+{click}"), tr("Select several rows: move, duplicate, trash them together", "Sélectionner plusieurs lignes : les déplacer, dupliquer, jeter ensemble")),
                t(tr("Right click", "Clic droit"), tr("Copy the link or the path, set an icon, show a folder or a tag as a list…", "Copier le lien ou le chemin, donner une icône, afficher un dossier ou un tag en liste…")),
                t(tr("In a list", "Dans une liste"), tr("Type to filter (key:text for one column), click a cell to rewrite it", "Taper pour filtrer (clé:texte pour une colonne), cliquer une cellule pour la réécrire")),
                t(tr("Drag a node", "Glisser un nœud"), tr("Move it in the graph, linked notes follow", "Le déplacer dans le graphe, les notes liées suivent")),
            ],
        ),
        (
            tr("Diagrams", "Schémas"),
            vec![
                k(&NewDiagram, tr("New diagram: an SVG file in the vault", "Nouveau schéma : un fichier SVG du coffre")),
                t("R U O D C P N T", tr("Rectangle, rounded, ellipse, diamond, cylinder, person, note, text", "Rectangle, arrondi, ellipse, losange, cylindre, personnage, note, texte")),
                t("A / L / V", tr("Arrow / line, held by the shapes they join / select", "Flèche / trait, accrochés aux formes reliées / sélection")),
                t(tr("Enter, double click", "Entrée, double-clic"), tr("Write in the shape or on the arrow; Esc when done", "Écrire dans la forme ou sur la flèche ; Échap pour finir")),
                t("---", tr("Alone on a line of a box: a compartment (UML class)", "Seul sur une ligne d'une boîte : un compartiment (classe UML)")),
                t(&format!("{shift}+{click} / {MOD}+{click}"), tr("Add a shape to the selection, or take it out", "Ajouter une forme à la sélection, ou l'en retirer")),
                t(tr("Delete", "Suppr"), tr("Remove the selection", "Retirer la sélection")),
                k(&canvas::Duplicate, tr("Diagram: duplicate the selection", "Schéma : dupliquer la sélection")),
                k(&canvas::Undo, tr("Diagram: undo", "Schéma : annuler")),
                k(&canvas::Redo, tr("Diagram: redo", "Schéma : rétablir")),
                t(tr("Edge of a shape", "Bord d'une forme"), tr("An arrow end dropped there stays there; in the middle, it follows the other end", "Un bout de flèche lâché là y reste ; au milieu, il suit l'autre bout")),
                t(tr("Wheel / + - 0", "Molette / + - 0"), tr("Move the view / zoom in, out, fit all", "Déplacer la vue / zoomer, dézoomer, tout cadrer")),
                t("![](Schéma.svg)", tr("Show the diagram in a note", "Afficher le schéma dans une note")),
                t(".svg", tr("An SVG from elsewhere (draw.io, Mermaid.js…) opens in a viewer: drag or wheel to move, + - 0 to zoom", "Un SVG d'ailleurs (draw.io, Mermaid.js…) s'ouvre dans une visionneuse : glisser ou molette pour se déplacer, + - 0 pour zoomer")),
                p("import", tr("Bring in an Excalidraw or draw.io file", "Reprendre un fichier Excalidraw ou draw.io")),
            ],
        ),
        (
            tr("CSV tables", "Tableaux CSV"),
            vec![
                t(".csv .tsv", tr("Opens in a grid; encoding and delimiter are detected, written straight into the file", "S'ouvre dans une grille ; encodage et délimiteur sont devinés, écrit directement dans le fichier")),
                t(tr("Arrows, Tab", "Flèches, Tab"), tr("Move; with Shift (or drag), select a range; click a row number or a header", "Se déplacer ; avec Maj (ou en glissant), sélectionner une plage ; clic sur un numéro ou un en-tête")),
                t(&format!("{MOD}+{click}"), tr("Add another selection: copy, cut, empty or remove their rows together", "Ajouter une autre sélection : les copier, couper, vider ou retirer leurs lignes ensemble")),
                t(tr("Type, Enter, F2", "Taper, Entrée, F2"), tr("Replace the cell / validate and go down / open the cell; Esc cancels", "Remplacer la cellule / valider et descendre / ouvrir la cellule ; Échap annule")),
                k(&sheet::Copy, tr("Table: copy as tab-separated text", "Tableau : copier en texte à tabulations")),
                k(&sheet::Cut, tr("Table: cut", "Tableau : couper")),
                k(&sheet::Paste, tr("Table: paste cells from a spreadsheet, Markdown or CSV", "Tableau : coller des cellules d'un tableur, de Markdown ou de CSV")),
                k(&sheet::SelectRow, tr("Table: select the whole rows of the selection", "Tableau : sélectionner les lignes entières de la sélection")),
                k(&sheet::SelectCol, tr("Table: select the whole columns of the selection", "Tableau : sélectionner les colonnes entières de la sélection")),
                k(&sheet::InsertBelow, tr("Table: insert a row below (a column to the right when whole columns are selected)", "Tableau : insérer une ligne dessous (une colonne à droite si des colonnes entières sont sélectionnées)")),
                k(&sheet::InsertAbove, tr("Table: insert a row above (a column to the left when whole columns are selected)", "Tableau : insérer une ligne dessus (une colonne à gauche si des colonnes entières sont sélectionnées)")),
                k(&sheet::DeleteRows, tr("Table: remove the selected rows, or the columns when whole columns are selected", "Tableau : retirer les lignes sélectionnées, ou les colonnes si des colonnes entières sont sélectionnées")),
                k(&sheet::Undo, tr("Table: undo", "Tableau : annuler")),
                k(&sheet::Redo, tr("Table: redo", "Tableau : rétablir")),
                p("table", tr("Delimiter, encoding, header row, copy as…, export (CSV, TSV, Markdown, JSON)", "Délimiteur, encodage, en-tête, copier en…, exporter (CSV, TSV, Markdown, JSON)")),
            ],
        ),
        (
            tr("Appearance", "Apparence"),
            vec![
                p(tr("theme", "thème"), tr("Theme, previewed as you browse", "Thème, en aperçu pendant le choix")),
                p(tr("font", "police"), tr("Font of the app / of the code", "Police de l'app / du code")),
                p(tr("page width", "largeur de page"), tr("Width of the text column: narrow, normal, wide, full", "Largeur de la colonne de texte : étroite, normale, large, pleine")),
                p(tr("wide tables", "grands tableaux"), tr("A table wider than the text: in the column, or centered and wider", "Un tableau plus large que le texte : dans la colonne, ou centré et plus large")),
                k(&ZoomIn, tr("Bigger text", "Texte plus grand")),
                k(&ZoomOut, tr("Smaller text", "Texte plus petit")),
                k(&ZoomReset, tr("Default text size", "Taille du texte d'origine")),
            ],
        ),
        (
            // Les gestes de tableau du menu : le cadre dit où ils agissent, chacun peut avoir sa touche.
            tr("Tables in a note", "Tableaux d'une note"),
            std::iter::once(t(
                tr("Grips", "Poignées"),
                tr(
                    "Over a table: a small grip above the column and left of the row under the pointer, a round one at the corner for the whole table. Click one to take the column, the row or the table (copy, cut, erase, or any gesture below), drag it to move it, Esc lets go; the + between two grips inserts there",
                    "Sur un tableau : une petite poignée au-dessus de la colonne et à gauche de la ligne sous le pointeur, une ronde au coin pour tout le tableau. Un clic prend la colonne, la ligne ou le tableau (copier, couper, effacer, ou un geste ci-dessous), tirer la déplace, Échap la lâche ; le + entre deux poignées insère à cet endroit",
                ),
            ))
            .chain(editor::TableOp::ALL.into_iter().map(|op| k(op.action().as_ref(), op.label())))
            .chain([k(
                &editor::TableDeleteSpan,
                tr("Remove the row, or the columns taken by their grip", "Supprimer la ligne, ou les colonnes prises par leur poignée"),
            )])
            .collect(),
        ),
        (
            tr("Typing", "À la frappe"),
            vec![
                t("/", tr("Components: headings, lists, panel, table, code, formula…", "Composants : titres, listes, panneau, tableau, code, formule…")),
                t("/table", tr("Table: pick its size on the grid (mouse, arrows), or type it: 12x5", "Tableau : choisir sa taille sur la grille (souris, flèches), ou la taper : 12x5")),
                t("> [!NOTE]", tr("Colored panel: NOTE, TIP, IMPORTANT, WARNING, CAUTION", "Panneau coloré : NOTE, TIP, IMPORTANT, WARNING, CAUTION")),
                t("- ", tr("Bullet list", "Liste à puces")),
                t("1. ", tr("Numbered list", "Liste numérotée")),
                t("[] ", tr("Task", "Tâche à cocher")),
                t("# ## ###", tr("Headings; the first # names the file", "Titres ; le premier # nomme le fichier")),
                t("> ", tr("Quote", "Citation")),
                t("```rust", tr("Code block: colors, copy icon", "Bloc de code : couleurs, icône de copie")),
                t("---", tr("Divider", "Séparateur")),
                t("[[", tr("Link to a note", "Lien vers une note")),
                p("kanban", tr("New kanban board: cards you type, drag and tick; double click to rewrite", "Nouveau tableau kanban : des cartes à écrire, glisser et cocher ; double-clic pour réécrire")),
                t("@", tr("Link to the note of a day: @today, @monday, @2026-10-09", "Lien vers la note d'un jour : @demain, @lundi, @2026-10-09")),
                t("![](image.png)", tr("Picture, under its line", "Image, sous sa ligne")),
                t("![[", tr("Suggests the pictures and diagrams of the vault", "Propose les images et les schémas du coffre")),
                t(tr("Drop files", "Déposer des fichiers"), tr("Drag files or folders from your file manager onto a note (a line shows where) or onto the tree (the folder under the pointer): they are copied, never overwritten, and a note gets a link per item", "Glisser des fichiers ou des dossiers du gestionnaire de fichiers sur une note (un trait montre où) ou sur l'arbre (le dossier sous le pointeur) : ils sont copiés, jamais écrasés, et la note reçoit un lien par élément")),
                t(tr("Double / triple click", "Double / triple clic"), tr("Select a word / a line; drag to extend by words / lines", "Sélectionner un mot / une ligne ; glisser étend par mots / par lignes")),
                t("```mermaid", tr("Diagram, under its block; its magnifier shows it full size", "Diagramme, sous son bloc ; sa loupe le montre en grand")),
                t("$x^2$  $$…$$", tr("LaTeX formula, under its line", "Formule LaTeX, sous sa ligne")),
                t("-> != <= =>", tr("Shown as → ≠ ≤ ⇒", "Affichés → ≠ ≤ ⇒")),
            ],
        ),
        (
            tr("Editing", "Édition"),
            vec![
                t(tr("Enter", "Entrée"), tr("Continue the list, or leave it on an empty item", "Continuer la liste, ou en sortir sur un item vide")),
                t(&format!("Tab / {shift}+Tab"), tr("Indent / outdent", "Indenter / désindenter")),
                t(tr("Tab / Enter in a table", "Tab / Entrée dans un tableau"), tr("Next cell / new row; columns stay aligned; + buttons add a row or a column", "Cellule suivante / nouvelle ligne ; les colonnes restent alignées ; les boutons + ajoutent une ligne ou une colonne")),
                k(&editor::AlignLeft, tr("Align the column of the cursor to the left", "Aligner la colonne du curseur à gauche")),
                k(&editor::AlignCenter, tr("Center the column of the cursor", "Centrer la colonne du curseur")),
                k(&editor::AlignRight, tr("Align the column of the cursor to the right", "Aligner la colonne du curseur à droite")),
                t(tr("Pasting a table", "Coller un tableau"), tr("Spreadsheet cells or Markdown fill the grid, or become a table", "Des cellules de tableur ou du Markdown remplissent la grille, ou deviennent un tableau")),
                k(&editor::ListCycle, tr("Turn the selected lines into a list, then the next kind: bullets, numbered, tasks, plain text", "Mettre les lignes choisies en liste, puis au genre suivant : puces, numérotée, tâches, texte")),
                k(&editor::ListBullet, tr("Selected lines: bulleted list (again: plain text)", "Lignes choisies : liste à puces (encore : texte)")),
                k(&editor::ListOrdered, tr("Selected lines: numbered list (again: plain text)", "Lignes choisies : liste numérotée (encore : texte)")),
                k(&editor::ListTask, tr("Selected lines: task list (again: plain text)", "Lignes choisies : liste de tâches (encore : texte)")),
                k(&editor::ListText, tr("Selected lines: plain text", "Lignes choisies : texte simple")),
                k(&editor::ToggleTask, tr("Check / uncheck a task or a [ ] box; in a table cell, add one", "Cocher / décocher une tâche ou une case [ ] ; dans une cellule, en ajouter une")),
                k(&editor::Bold, tr("Bold", "Gras")),
                k(&editor::Italic, tr("Italic", "Italique")),
                k(&editor::Underline, tr("Underline", "Souligné")),
                k(&editor::Strike, tr("Strikethrough", "Barré")),
                k(&editor::Highlight, tr("Highlight", "Surligné")),
                k(&editor::InlineCode, tr("Inline code", "Code en ligne")),
                p(tr("formatting bar", "barre de mise en forme"), tr("Under the note, centered: bold, italic, underline, strikethrough, highlight (five colors), code, link, lists, tasks, table (in a table: add or delete a row or a column); a lit button means the selection has the style, click again to remove it; only the active pane shows it; in a narrow note it folds into a button at the bottom left that opens it; hide it with the palette", "Sous la note, au centre : gras, italique, souligné, barré, surligné (cinq couleurs), code, lien, listes, tâches, tableau (dans un tableau : ajouter ou supprimer une ligne ou une colonne) ; un bouton allumé : la sélection a le style, un second clic le retire ; seul le pane actif la montre ; dans une note étroite, elle se replie en un bouton en bas à gauche qui l'ouvre ; masquable par la palette")),
                k(&editor::SelectNext, tr("Several cursors: select the word, then its next occurrence (Esc: back to one)", "Plusieurs curseurs : prendre le mot, puis son occurrence suivante (Échap : un seul)")),
                t(&format!("Alt+{click}"), tr("Add a cursor", "Ajouter un curseur")),
                k(&editor::InsertLink, tr("Link: [text](address) from the selection; on a link, change its address", "Lien : [texte](adresse) depuis la sélection ; sur un lien, changer son adresse")),
                k(&editor::Comment, tr("Comment the selection or the line; on a comment, resolve it", "Commenter la sélection ou la ligne ; sur un commentaire, le résoudre")),
                p(tr("comments", "commentaires"), tr("Comments: show their panel, jump to one (a badge on the speech bubble counts them)", "Commentaires : montrer leur panneau, aller à l'un d'eux (une pastille sur la bulle les compte)")),
                t(tr("Right click", "Clic droit"), tr("Menu of the note: link, comment, cut, copy, paste, bold, italic", "Menu de la note : lien, commentaire, couper, copier, coller, gras, italique")),
                k(&editor::Copy, tr("Copy", "Copier")),
                k(&editor::Cut, tr("Cut", "Couper")),
                k(&editor::Paste, tr("Paste text or a picture; an address over a selection makes it a link", "Coller du texte ou une image ; une adresse sur une sélection en fait un lien")),
                k(&editor::SelectAll, tr("Select all", "Tout sélectionner")),
                k(&editor::Undo, tr("Undo", "Annuler")),
                k(&editor::Redo, tr("Redo", "Rétablir")),
                t(&format!("{word}+{}", tr("Left / Right", "Gauche / Droite")), tr("Move by word", "Se déplacer par mot")),
            ],
        ),
        (
            tr("Blocks", "Blocs"),
            std::iter::once(t(
                tr("Grip", "Poignée"),
                tr(
                    "Left of the line under the pointer: drag it to move the block (a line shows where it lands), click it to select the block and open its menu; the + left of it adds a block under it and opens the / list",
                    "À gauche de la ligne sous le pointeur : la tirer déplace le bloc (un trait montre où il arrive), un clic sélectionne le bloc et ouvre son menu ; le + à sa gauche ajoute un bloc dessous et ouvre la liste /",
                ),
            ))
            .chain(editor::BlockOp::ALL.into_iter().map(|op| k(op.action().as_ref(), op.effect())))
            .collect(),
        ),
    ]
}

/// Les actions dont le raccourci se change : celles qui ont leur ligne dans l'aide.
fn editable() -> std::collections::HashSet<&'static str> {
    help_sections(&[], &keys::Changes::new()).into_iter().flat_map(|(_, rows)| rows).filter_map(|row| row.action).collect()
}

impl Shell {
    /// Les lignes de l'aide que la recherche garde.
    fn help_rows(&self, cx: &App) -> keys::Sections {
        let query = self.help_find.as_ref().map(|find| find.read(cx).line.text.clone()).unwrap_or_default();
        let mut sections = keys::filter(help_sections(&self.bound, &self.changes), &query);
        // Ouvert sur un tableau ou un schéma, le panneau commence par ses touches.
        if let Some(at) = self.here(cx).and_then(|here| sections.iter().position(|(title, _)| *title == here)) {
            let first = sections.remove(at);
            sections.insert(0, first);
        }
        sections
    }

    /// La section de l'aide qui parle de ce qui est affiché : un tableau CSV, un schéma, un tableau
    /// de la note sous le curseur ; aucune pour une note ordinaire, l'aide commence par les notes.
    fn here(&self, cx: &App) -> Option<&'static str> {
        if self.shown_sheet().is_some() {
            Some(tr("CSV tables", "Tableaux CSV"))
        } else if self.picture.is_some() && self.drawing.is_some() {
            Some(tr("Diagrams", "Schémas"))
        } else if self.picture.is_none() && self.listing.is_none() && self.editor.read(cx).in_table() {
            Some(tr("Tables in a note", "Tableaux d'une note"))
        } else {
            None
        }
    }

    /// Le groupe des commandes de ce qu'on regarde, et son nom pour le champ de la palette : un tableau
    /// CSV, un schéma, un tableau de la note sous le curseur, sinon le bloc du curseur.
    fn here_group(&self, cx: &App) -> (Option<commands::Group>, &'static str) {
        use commands::Group;
        if self.shown_sheet().is_some() {
            (Some(Group::Table), tr("CSV table", "Tableau CSV"))
        } else if self.picture.is_some() && self.drawing.is_some() {
            (Some(Group::Diagram), tr("Diagram", "Schéma"))
        } else if self.picture.is_some() || self.listing.is_some() || self.board_shown(cx) {
            (None, "")
        } else if self.editor.read(cx).in_table() {
            (Some(Group::Table), tr("Table", "Tableau"))
        } else {
            (Some(Group::Block), "Note")
        }
    }

    /// Les commandes du catalogue que la vue sait faire, avec leurs touches, celles de la vue d'abord
    /// (combien : le second nombre). Celles qu'elle ne sait pas faire ne sont pas proposées.
    fn palette_commands(&self, window: &mut Window, cx: &mut Context<Self>) -> (Vec<commands::Command>, usize) {
        use commands::Group;
        let available: std::collections::HashSet<&str> = window.available_actions(cx).iter().map(|action| action.name()).collect();
        let here = self.here_group(cx).0;
        let list = commands::catalog()
            .into_iter()
            .filter(|(action, ..)| available.contains(action))
            // Les gestes d'un bloc, d'un tableau ou d'un schéma ne valent que là : ailleurs, ils ne feraient rien.
            .filter(|&(_, _, _, group)| !matches!(group, Group::Block | Group::Table | Group::Diagram) || Some(group) == here)
            .filter_map(|(action, label, icon, group)| {
                let keys = keys::of(cx, cx.build_action(action, None).ok()?.as_ref());
                Some(commands::Command { action, label, icon, group, keys })
            })
            .collect();
        commands::arrange(list, here)
    }

    /// La palette des commandes (F10, ou le premier bouton du rail) : la palette, rien que les commandes.
    pub fn open_commands(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.palette.is_some() {
            return;
        }
        self.show_palette(">", false, window, cx)
    }

    /// `short` : la fenêtre est basse, le pied « à propos » défile avec la liste au lieu de lui prendre sa place.
    fn render_help(&self, short: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        let rows = self.help_rows(cx);
        let none = rows.is_empty();
        let asked = self.help_find.as_ref().is_some_and(|find| !find.read(cx).line.text.is_empty());
        let small = |id: String, text: &'static str| {
            div()
                .id(gpui::SharedString::from(id.clone()))
                .debug_selector(move || id.clone())
                .px_1()
                .rounded(px(4.))
                .cursor_pointer()
                .text_size(px(11.))
                .text_color(t.soft)
                .hover(|s| s.bg(t.border).text_color(t.text))
                .child(text)
        };
        let hint = div().absolute().left_0().top_0().text_color(t.soft).child(tr("Search the shortcuts…", "Chercher un raccourci…"));
        let find = div()
            .flex_none()
            .px_5()
            .py_3()
            .border_b_1()
            .border_color(t.border)
            .flex()
            .items_center()
            .gap_2()
            .child(svg().path("search.svg").size(px(14.)).flex_none().text_color(t.soft))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .relative()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .children(self.help_find.clone())
                    .when(!asked, |d| d.child(hint)),
            )
            .when(!self.changes.is_empty(), |d| {
                d.child(
                    small("keys-reset".into(), tr("Reset all", "Tout rétablir")).on_click(cx.listener(|this, _, _, cx| this.key_reset(None, cx))),
                )
            });
        let label = |action: &str| {
            help_sections(&[], &keys::Changes::new()).into_iter().flat_map(|(_, rows)| rows).find(|row| row.action == Some(action)).map_or("", |row| row.effect)
        };
        let sections = rows.into_iter().map(|(title, rows)| {
            div()
                .w(px(370.))
                .max_w_full()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().mb_1().text_color(t.accent).text_size(px(12.)).child(title))
                .children(rows.into_iter().map(|row| {
                    let edit = self.help_edit.as_ref().filter(|edit| Some(edit.action) == row.action);
                    let chosen = row.action.is_some() && row.action == self.help_sel;
                    // Les touches : un bouton quand elles se changent, du texte sinon.
                    let keys = match row.action {
                        None => div().w(px(156.)).flex_none().font_family(mono()).text_size(px(12.)).child(row.keys.clone()),
                        Some(action) => {
                            let shown = if edit.is_some() {
                                tr("Press the keys…", "Tape les touches…").to_string()
                            } else if row.keys.is_empty() {
                                "—".to_string()
                            } else {
                                row.keys.clone()
                            };
                            let key = div()
                                .id(gpui::SharedString::from(format!("key-{action}")))
                                .debug_selector(move || format!("key-{action}"))
                                .px_1()
                                .rounded(px(4.))
                                .bg(t.border.opacity(0.6))
                                .cursor_pointer()
                                .font_family(mono())
                                .text_size(px(12.))
                                .text_color(if edit.is_some() || row.changed { t.accent } else { t.text })
                                .hover(|s| s.bg(t.border))
                                .child(shown)
                                .on_click(cx.listener(move |this, _, _, cx| this.key_edit(action, cx)));
                            let reset = small(format!("key-reset-{action}"), tr("default", "défaut"))
                                .on_click(cx.listener(move |this, _, _, cx| this.key_reset(Some(action), cx)));
                            div()
                                .w(px(156.))
                                .flex_none()
                                .flex()
                                .flex_wrap()
                                .items_start()
                                .gap_1()
                                .child(key)
                                .when(row.changed && edit.is_none(), |d| d.child(reset))
                        }
                    };
                    // Sous la ligne en cours de saisie : quoi faire, ou ce qui coince.
                    let note = edit.map(|edit| match &edit.issue {
                        None => div().text_size(px(12.)).text_color(t.soft).child(tr(
                            "Backspace: no shortcut · Esc: cancel",
                            "Retour arrière : aucun raccourci · Échap : annuler",
                        )),
                        Some(KeyIssue::Refused(why)) => div().text_size(px(12.)).child(format!("{} : {why}", keys::show(&edit.keys))),
                        Some(KeyIssue::Taken(others)) => div()
                            .text_size(px(12.))
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap_1()
                            .child(div().w_full().child(format!(
                                "{} : {} {}",
                                keys::show(&edit.keys),
                                tr("already used by:", "déjà utilisé par :"),
                                others.iter().map(|other| label(other)).collect::<Vec<_>>().join(", "),
                            )))
                            .child(
                                small("key-replace".into(), tr("Replace (Enter)", "Remplacer (Entrée)"))
                                    .text_color(t.text)
                                    .on_click(cx.listener(|this, _, _, cx| this.key_replace(cx))),
                            )
                            .child(small("key-keep".into(), tr("Cancel (Esc)", "Annuler (Échap)")).on_click(cx.listener(|this, _, _, cx| {
                                this.help_edit = None;
                                cx.notify();
                            }))),
                    });
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .rounded(px(4.))
                        .when(chosen, |d| d.bg(t.selection.opacity(0.5)))
                        .child(div().flex().gap_3().child(keys).child(div().flex_1().min_w_0().text_color(t.soft).child(row.effect)))
                        .children(note)
                }))
        });
        // À propos, en pied de panneau : ce qu'est l'app, et où la trouver.
        let link = |id: &'static str, icon: &'static str, label: &'static str, url: &'static str| {
            div()
                .id(id)
                .px_2()
                .py_1()
                .flex()
                .items_center()
                .gap_1p5()
                .rounded(px(6.))
                .cursor_pointer()
                .text_color(t.accent)
                .hover(|s| s.bg(t.border))
                .child(svg().path(icon).size(px(14.)).flex_none().text_color(t.accent))
                .child(label)
                .on_mouse_down(MouseButton::Left, move |_, _, cx| cx.open_url(url))
        };
        let about = div()
            .flex_none()
            .map(|d| if short { d.w_full().pt_3() } else { d.px_5().py_3() })
            .border_t_1()
            .border_color(t.border)
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(logo(t))
            .child(format!("Bref {}", env!("CARGO_PKG_VERSION")))
            .child(div().flex_1().min_w(px(200.)).text_color(t.soft).child(tr(
                "Fast, minimal Markdown notes, in plain files you own.",
                "Des notes Markdown rapides et minimales, dans de simples fichiers qui t'appartiennent.",
            )))
            .child(link("about-github", "code.svg", "GitHub", env!("CARGO_PKG_REPOSITORY")))
            .child(link("about-kofi", "heart.svg", tr("Support on Ko-fi", "Soutenir sur Ko-fi"), KOFI));
        let (inside, below) = if short { (Some(about), None) } else { (None, Some(about)) };
        div()
            .absolute()
            .inset_0()
            .occlude()
            .p_4()
            .flex()
            // Calé en haut : le champ de recherche ne bouge pas quand la liste raccourcit.
            .items_start()
            .justify_center()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| this.set_help(false, window, cx)),
            )
            .child(
                div()
                    .id("help")
                    // Deux colonnes si la fenêtre est assez large, une seule sinon.
                    .w(px(812.))
                    .max_w_full()
                    .max_h_full()
                    .flex()
                    .flex_col()
                    .bg(t.panel)
                    .border_1()
                    .border_color(t.border)
                    .rounded(px(10.))
                    .shadow_lg()
                    .text_size(px(13.))
                    // Un clic dans le panneau ne le ferme pas.
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_action(cx.listener(|this, _: &palette::Prev, _, cx| this.help_step(false, cx)))
                    .on_action(cx.listener(|this, _: &palette::Next, _, cx| this.help_step(true, cx)))
                    .child(find)
                    .child(
                        div()
                            .id("help-keys")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .p_5()
                            .flex()
                            .flex_wrap()
                            .gap_x_6()
                            .gap_y_4()
                            .children(sections)
                            .when(none, |d| {
                                d.child(div().text_color(t.soft).child(tr("No shortcut matches.", "Aucun raccourci ne correspond.")))
                            })
                            .children(inside),
                    )
                    .children(below)
                    .with_animation(
                        "help-in",
                        Animation::new(Duration::from_millis(140)).with_easing(ease_out_quint()),
                        |help, delta| help.opacity(delta),
                    ),
            )
    }
}

impl Shell {
    /// Dessine le pane `pane` (l'appelant l'a rendu actif le temps du dessin) : ce qu'il montre,
    /// et le panneau de ses commentaires. `active` : c'est lui qui a la saisie.
    /// Les notes que le coffre remet sous les yeux aujourd'hui (voir `recall::recall`).
    // ponytail: recalculé à chaque rendu d'une page vide (un hachage par note, 5 000 notes :
    // une fraction de milliseconde) ; le garder d'un rendu à l'autre si de bien plus gros
    // coffres le font sentir.
    /// La note est un modèle du coffre.
    fn is_template(&self, note: &Note) -> bool {
        self.vault.as_ref().is_some_and(|root| vault::in_templates(root, &note.path))
    }

    fn recalled(&self) -> Vec<recall::Recalled> {
        let notes: Vec<Note> = self.notes.iter().filter(|n| !self.is_template(n)).cloned().collect();
        recall::recall(&notes, today(), std::time::SystemTime::now())
    }

    /// La page vide, sous la ligne où l'on écrit : le jour (qui ouvre sa note), les touches
    /// utiles, les notes à relire. Tout se clique, passe à la ligne dans un pane étroit et
    /// disparaît à la première frappe.
    fn render_empty(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let t = self.theme;
        // `fill` : la ligne se teinte au survol, comme une ligne de liste.
        let row = |id: (&'static str, usize), fill: bool| {
            div()
                .id(id)
                .debug_selector(move || if id.0 == "empty-today" { id.0.to_string() } else { format!("{}-{}", id.0, id.1) })
                .flex()
                .items_center()
                .gap_2()
                .rounded(px(6.))
                .cursor_pointer()
                .text_color(t.soft)
                .hover(|s| if fill { s.text_color(t.text).bg(t.border) } else { s.text_color(t.text) })
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        };
        // Les touches en vigueur ; une action laissée sans raccourci n'est pas annoncée.
        let actions: [(Box<dyn gpui::Action>, &str); 3] = [
            (Box::new(OpenPalette), "notes"),
            (Box::new(NewNote), tr("new note", "nouvelle note")),
            (Box::new(ToggleHelp), tr("shortcuts", "raccourcis")),
        ];
        let keys = actions.into_iter().enumerate().filter_map(|(i, (action, what))| {
            let keys = keys::of(cx, action.as_ref());
            let cap = div().px(px(5.)).rounded(px(4.)).border_1().border_color(t.border).bg(t.panel).text_size(px(11.)).child(keys.clone());
            (!keys.is_empty()).then(|| {
                row(("empty-key", i), false)
                    .gap(px(6.))
                    .child(cap)
                    .child(what)
                    .on_click(move |_, window, cx| window.dispatch_action(action.boxed_clone(), cx))
            })
        });
        let day = self.vault.is_some().then(|| {
            row(("empty-today", 0), false)
                .child(logo(t))
                .child(recall::long_date(today()))
                .on_click(cx.listener(|this, _, _, cx| this.open_today(cx)))
        });
        let recalled = if self.vault.is_some() { self.recalled() } else { Vec::new() };
        let again = (!recalled.is_empty()).then(|| {
            let rows = recalled.into_iter().enumerate().map(|(i, note)| {
                row(("empty-recall", i), true)
                    .mx(px(-6.))
                    .px(px(6.))
                    .py(px(2.))
                    .child(div().flex_1().min_w_0().truncate().child(note.name))
                    .child(div().flex_none().text_size(px(12.)).child(recall::ago(note.days)))
                    .on_click(cx.listener(move |this, _, _, cx| this.open_note(&note.path, cx)))
            });
            div()
                .max_w(px(380.))
                .flex()
                .flex_col()
                .child(div().mb_1().text_size(px(11.)).text_color(t.soft).child(tr("READ AGAIN", "À RELIRE")))
                .children(rows)
        });
        // Dans la colonne du texte, sous sa première ligne.
        div().absolute().top(px(76.)).left_0().right_0().flex().justify_center().child(
            div()
                .w_full()
                .max_w(self.theme.page.measure(self.theme.size) + px(48.))
                .px(px(32.))
                .flex()
                .flex_col()
                .gap_4()
                .text_size(px(13.))
                .children(day)
                .child(div().flex().flex_wrap().gap_x_4().gap_y_1().children(keys))
                .children(again),
        )
    }

    fn render_pane(&mut self, pane: usize, active: bool, client: bool, window: &mut Window, cx: &mut Context<Self>) -> (gpui::AnyElement, Option<gpui::AnyElement>) {
        let t = self.theme;
        let two = self.panes.len() == 2;
        // Copie de toute la note : simple icône flottante, hors de la barre de titre.
        // Commentaires de la note, à sa droite : le texte commenté, puis ce qu'on en dit. Un
        // clic mène au commentaire, prêt à être retouché ; la coche le résout.
        // ponytail: la note est relue à chaque rendu (une recherche de `{==`) ; garder la
        // liste d'une version du texte à l'autre si de très longues notes en pâtissent.
        let said: Vec<(String, String, usize)> = match self.picture.is_none() && self.listing.is_none() && !self.board_shown(cx) {
            true => markdown::all_comments(self.editor.read(cx).text()).into_iter().map(|(noted, said, at)| (noted.into(), said.into(), at)).collect(),
            false => Vec::new(),
        };
        // Leur bouton, à gauche de celui de la copie, ouvre et referme le panneau ; le
        // compteur de mots de la note lui laisse la place.
        let talk = (!said.is_empty()).then(|| {
            ui::widgets::button("comments-toggle", "i-chat.svg", !self.comments_shut, t)
                .debug_selector(|| "comments-toggle".into())
                .tooltip(ui::widgets::tip(tr("Comments of the note", "Commentaires de la note"), String::new(), t))
                .absolute()
                .bottom_4()
                .right(px(if kanban::is_board(self.editor.read(cx).text()) { 84. } else { 50. }))
                .size(px(30.))
                .rounded(px(8.))
                .occlude()
                // Une pastille et leur nombre, que le panneau soit ouvert ou refermé.
                .child(
                    div()
                        .debug_selector(|| "comments-badge".into())
                        .absolute()
                        .top(px(-3.))
                        .right(px(-3.))
                        .min_w(px(15.))
                        .h(px(15.))
                        .px(px(4.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(t.accent)
                        .text_color(t.bg)
                        .text_size(px(10.))
                        .font_weight(gpui::FontWeight::BOLD)
                        .child(badge_label(said.len())),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.comments_shut = !this.comments_shut;
                    cx.notify();
                }))
        });
        // Une note qui est un tableau kanban : son bouton passe du tableau au texte. Le
        // tableau cache la note, qui ne doit plus recevoir la frappe.
        let board = self.board_shown(cx);
        if board && self.editor.focus_handle(cx).is_focused(window) {
            window.focus(&self.board_focus);
        }
        if !cx.has_active_drag() {
            self.board_drag = None;
        }
        // La page liste refermée avec la saisie dans son filtre ou une de ses cellules : plus
        // rien ne la reçoit, elle revient à la note.
        if active && self.listing.is_none() && window.focused(cx).is_none() {
            window.focus(&self.editor.focus_handle(cx));
        }
        if !board {
            self.board_field = None;
            // Le tableau quitté (une autre note s'ouvre), la saisie revient à la note.
            if self.board_focus.is_focused(window) {
                window.focus(&self.editor.focus_handle(cx));
            }
        }
        let boards = kanban::is_board(self.editor.read(cx).text()) && self.picture.is_none() && self.listing.is_none();
        let flip = boards.then(|| {
            ui::widgets::button("board-toggle", "board.svg", board, t)
                .tooltip(ui::widgets::tip(if board { tr("Show the text", "Voir le texte") } else { tr("Show the board", "Voir le tableau") }, String::new(), t))
                .absolute()
                .bottom_4()
                // Toujours à la même place, tableau ou texte : à gauche de la copie.
                .right(px(50.))
                .size(px(30.))
                .rounded(px(8.))
                .occlude()
                .on_click(cx.listener(|this, _, window, cx| this.toggle_board(window, cx)))
        });
        self.editor.update(cx, |e, _| {
            e.corner = 1 + talk.is_some() as usize + boards as usize;
            e.bar_here = active;
        });
        // La colonne des commentaires prend ce que le panneau et la note lui laissent, 270 px au
        // plus ; faute de 140 px, elle ne se montre pas (son bouton reste, la fenêtre élargie la rend).
        let column = (active && !said.is_empty() && !self.comments_shut)
            .then(|| {
                self.nav.aside = px(270.);
                let panel = if self.nav.panel == Panel::Split { self.split_width() + nav::DIVIDER } else { px(0.) };
                (self.nav.total - nav::RAIL - panel - nav::NOTE_MIN).min(px(270.))
            })
            .filter(|width| *width >= px(140.));
        if active && column.is_none() {
            self.nav.aside = px(0.);
        }
        let cards = said.iter().filter(|_| !self.comments_shut).enumerate().map(|(i, (noted, said, at))| {
            let at = *at;
            let inside = at + noted.len() + 9;
            div()
                .id(("comment", i))
                .p_2()
                .rounded(px(6.))
                .border_1()
                .border_color(t.border)
                .bg(t.bg)
                .flex()
                .flex_col()
                .gap_1()
                .cursor_pointer()
                .hover(|s| s.border_color(t.accent.opacity(0.6)))
                .child(
                    div()
                        .flex()
                        .items_start()
                        .gap_1()
                        .child(div().flex_1().min_w_0().px_1().rounded(px(3.)).bg(t.accent.opacity(0.22)).line_clamp(2).child(noted.to_string()))
                        .child(ui::widgets::button(("resolve", i), "check.svg", false, t).on_click(cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.editor.update(cx, |e, cx| e.resolve_comment(at, cx));
                            window.focus(&this.editor.focus_handle(cx));
                        }))),
                )
                .child(div().text_color(if said.is_empty() { t.soft } else { t.text }).child(match said.is_empty() {
                    true => tr("(nothing written yet)", "(rien d'écrit pour l'instant)").to_string(),
                    false => said.to_string(),
                }))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.editor.update(cx, |e, cx| e.jump(inside, false, cx));
                    window.focus(&this.editor.focus_handle(cx));
                }))
        });
        let comments = column.map(|column| {
            let title = format!("{} ({})", tr("Comments", "Commentaires"), said.len());
            let shut = ui::widgets::button("comments-shut", "close.svg", false, t).tooltip(ui::widgets::tip(tr("Close", "Fermer"), String::new(), t)).on_click(cx.listener(|this, _, _, cx| {
                this.comments_shut = true;
                cx.notify();
            }));
            div()
                .id("comments")
                .debug_selector(|| "comments".into())
                .w(column)
                .flex_none()
                .h_full()
                .pt(px(48.))
                .px_3()
                .pb_3()
                .border_l_1()
                .border_color(t.border)
                .bg(t.panel)
                .text_size(px(13.))
                .flex()
                .flex_col()
                .gap_2()
                .overflow_y_scroll()
                .child(div().flex().items_center().child(div().flex_1().text_color(t.soft).child(title)).child(shut))
                .children(cards)
        });
        let copy = icon_button("copy-all", if self.copied { "check.svg" } else { "copy.svg" }, t)
            .tooltip(ui::widgets::tip(tr("Copy the note", "Copier la note"), keys::of(cx, &CopyAll), t))
            .absolute()
            .bottom_4()
            .right_4()
            .size(px(30.))
            .rounded(px(8.))
            .occlude()
            .on_click(cx.listener(|this, _, _, cx| this.copy_all(false, cx)));
        // L'image choisie dans le panneau ; elle s'efface dès que la note reprend la main.
        if self.editor.focus_handle(cx).is_focused(window) {
            self.picture = None;
            self.listing = None;
            self.figure = None;
        }
        if self.drawing.as_ref().map(|(path, _)| path) != self.picture.as_ref() {
            self.drawing = None;
        }
        if self.viewer.as_ref().map(|(path, _)| path) != self.picture.as_ref() {
            self.viewer = None;
        }
        let note = div().flex_1().min_w_0().h_full().relative();
        if self.sheet.as_ref().map(|(path, _)| path) != self.picture.as_ref() {
            self.flush_sheet(cx);
            self.sheet = None;
        }
        let listing = self.listing.is_some().then(|| self.render_listing(cx));
        let note = match (&self.picture, &self.drawing) {
            _ if listing.is_some() => note.children(listing),
            _ if let Some(figure) = self.figure.clone() => {
                figure.update(cx, |figure, _| figure.sync(t));
                note.child(figure)
            }
            _ if board => note.child(self.render_board(cx)).children(flip),
            (Some(_), _) if self.sheet.is_some() => {
                let (_, sheet) = self.sheet.clone().unwrap();
                sheet.update(cx, |sheet, _| sheet.sync(t, client));
                note.child(sheet)
            }
            (Some(_), Some((_, canvas))) => {
                canvas.update(cx, |canvas, _| canvas.sync(t, client));
                note.child(canvas.clone())
            }
            (Some(path), None) if self.viewer.as_ref().is_some_and(|(open, _)| open == path) => {
                let (_, viewer) = self.viewer.clone().unwrap();
                viewer.update(cx, |viewer, _| viewer.sync(t));
                note.child(viewer)
            }
            (Some(path), None) => note.p_6().flex().items_center().justify_center().child(
                img(path.clone()).max_w_full().max_h_full().object_fit(ObjectFit::ScaleDown),
            ),
            (None, _) => {
                let empty = self.editor.read(cx).text().is_empty().then(|| self.render_empty(cx));
                note.child(self.editor.clone()).children(empty).child(copy).children(talk).children(flip)
            }
        };
        // Pendant qu'on glisse un fichier de l'arbre, la zone qui le recevrait est teintée.
        let tint = self.pane_aim.filter(|(over, _)| *over == pane).map(|(_, half)| {
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .right_0()
                .map(|d| if half { d.w(gpui::relative(0.5)) } else { d.left_0() })
                .bg(t.accent.opacity(0.12))
                .border_1()
                .border_color(t.accent.opacity(0.5))
        });
        let ratio = self.pane_ratio;
        let area = div()
            .id(("pane", pane))
            .debug_selector(move || format!("pane-{pane}"))
            .track_focus(&self.zone)
            .relative()
            .h_full()
            .min_w_0()
            // Rien d'un pane ne se dessine chez son voisin, si étroit soit-il.
            .overflow_hidden()
            .flex()
            .map(|d| if two && pane == 0 { d.flex_none().w(gpui::relative(ratio)) } else { d.flex_1() })
            // Un geste de souris dans un pane vaut pour lui, avant que ses vues ne le reçoivent.
            .capture_any_mouse_down(cx.listener(move |this, _, _, cx| {
                if this.active != pane && pane < this.panes.len() {
                    this.active = pane;
                    cx.notify();
                }
            }))
            // Des fichiers du système sur la note : un trait entre deux lignes dit où ils se rangeraient.
            .on_drag_move(cx.listener(move |this, e: &gpui::DragMoveEvent<gpui::ExternalPaths>, _, cx| {
                let mine = matches!(&this.drop_aim, Some(DropAim { aim: Aim::Note { pane: p, .. }, .. }) if *p == pane);
                let at = e.event.position;
                match e.bounds.contains(&at).then(|| this.note_aim(pane, at, e.drag(cx).paths().len(), cx)).flatten() {
                    Some(aim) => this.aim_drop(Some(aim), cx),
                    None if mine => this.aim_drop(None, cx),
                    None => {}
                }
            }))
            .on_drop(cx.listener(move |this, paths: &gpui::ExternalPaths, _, cx| {
                if let Some(DropAim { aim: aim @ Aim::Note { pane: p, .. }, .. }) = this.drop_aim.clone()
                    && p == pane
                {
                    this.drop_paths(paths.paths().to_vec(), aim, cx);
                }
            }))
            .on_drag_move(cx.listener(move |this, e: &gpui::DragMoveEvent<nav::Dragged>, _, cx| {
                let at = e.event.position;
                let aim = e.bounds.contains(&at).then(|| (pane, this.panes.len() == 1 && at.x > e.bounds.center().x));
                let was = this.pane_aim.filter(|(over, _)| *over == pane);
                if aim != was {
                    this.pane_aim = aim;
                    cx.notify();
                }
            }))
            .on_drop(cx.listener(move |this, dragged: &nav::Dragged, window, cx| {
                let aside = this.pane_aim.take().is_some_and(|(_, half)| half);
                if let Some(path) = dragged.paths.first().cloned() {
                    this.drop_on_pane(pane, aside, &path, window, cx);
                }
            }))
            .child(note)
            // Avec deux panes, un filet marque celui qui a la saisie.
            .when(two && active, |d| d.child(div().absolute().top_0().left_0().right_0().h(px(2.)).bg(t.accent.opacity(0.7))))
            .children(tint);
        (area.into_any_element(), comments.map(IntoElement::into_any_element))
    }
}

/// La fenêtre de capture (`bref --capture`, à lier à un raccourci du bureau) : une ligne de
/// saisie, rien d'autre. Entrée l'ajoute à la note du jour et referme ; Échap referme. Le coffre
/// n'est pas lu, la fenêtre est là aussi vite que l'app.
// ponytail: si Bref est ouvert sur la note du jour, c'est sa surveillance du coffre qui la relit ;
// une frappe en attente là-bas est enregistrée 400 ms après (bien avant qu'on ait tapé ici).
// Parler à l'instance ouverte si ce délai devait un jour s'allonger.
struct Capture {
    root: PathBuf,
    theme: Theme,
    field: Entity<Palette>,
}

impl Capture {
    fn new(root: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let prefs = Prefs::parse(&vault::load_settings());
        apply_fonts(&prefs);
        let theme = Theme::of(&prefs, window.appearance());
        let field = Self::ask(tr("Add to today's note", "Ajouter à la note du jour").into(), "", theme, window, cx);
        Self { root, theme, field }
    }

    /// Le champ de saisie : celui de la palette, qui annonce `label` et part de `text`.
    fn ask(label: String, text: &str, theme: Theme, window: &mut Window, cx: &mut Context<Self>) -> Entity<Palette> {
        let field = cx.new(|cx| Palette::prompt(label, text, theme, cx).at_top());
        cx.subscribe_in(&field, window, |this, _, event, window, cx| match event {
            PaletteEvent::Submit(text) if !text.is_empty() => match vault::capture(&this.root, &date_name(today()), text) {
                Ok(_) => window.remove_window(),
                // Rien n'est perdu : la ligne reste là, avec la raison.
                Err(e) => {
                    let label = format!("{} : {e}", tr("Not saved", "Non enregistré"));
                    this.field = Self::ask(label, text, this.theme, window, cx);
                    cx.notify();
                }
            },
            PaletteEvent::Submit(_) | PaletteEvent::Dismiss => window.remove_window(),
            _ => {}
        })
        .detach();
        window.focus(&field.focus_handle(cx));
        field
    }
}

impl Render for Capture {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().font_family(sans()).text_color(self.theme.text).child(self.field.clone())
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        // Le pane qui contient le focus est l'actif ; quand le code en a changé sans déplacer la
        // saisie (une note déjà ouverte dans l'autre pane), elle suit.
        if std::mem::take(&mut self.grab) {
            self.focus_pane(self.active, window, cx);
        } else if let Some(pane) = (0..self.panes.len()).find(|&pane| self.panes[pane].zone.contains_focused(window, cx)) {
            self.active = pane;
        }
        #[cfg(test)]
        let dragging = cx.has_active_drag() || self.keep_aim;
        #[cfg(not(test))]
        let dragging = cx.has_active_drag();
        if !dragging {
            self.pane_aim = None;
            // Le glisser est fini (lâché, ou sorti de la fenêtre) : plus d'étiquette ni de trait.
            if self.drop_aim.is_some() {
                self.aim_drop(None, cx);
            }
        }
        if self.editor.focus_handle(cx).is_focused(window) {
            self.keep_preview();
        }
        self.track(cx);
        let title = match (self.picture.as_ref().or(self.path.as_ref()), &self.vault) {
            (Some(p), _) => vault::stem(p),
            (None, Some(_)) => tr("New note", "Nouvelle note").to_string(),
            (None, None) => "Bref".to_string(),
        };
        if title != self.title {
            window.set_window_title(&title);
            self.title = title;
        }

        // Sous Linux l'app fournit barre de titre, coins arrondis et ombre ; sous
        // macOS et Windows, gpui annonce `Server` et le système s'en charge.
        let tiling = match window.window_decorations() {
            Decorations::Client { tiling } => Some(tiling),
            Decorations::Server => None,
        };
        let client = tiling.is_some();
        let framed = tiling.is_some_and(|t| !t.is_tiled())
            && !window.is_maximized()
            && !window.is_fullscreen();
        let inset = if framed { SHADOW } else { px(0.) };
        window.set_client_inset(inset);
        self.nav.left = inset;
        self.nav.native_bar = !client;
        self.nav.total = window.viewport_size().width - inset * 2.;

        // Bouton icône : cercle visible au survol, comme les contrôles de fenêtre de Zed.
        let controls = window.window_controls();
        // Pastille flottante en haut à droite de la note : un fond et un bord fins la
        // distinguent du texte, ni flou ni transparence. Elle sert aussi de poignée de
        // déplacement, comme l'en-tête du panneau, le vide du rail et la barre du tableau ;
        // Alt + glisser déplace depuis n'importe où. ponytail: pas de bande transparente
        // sur le haut de la note, elle gênerait les premières lignes.
        let pill = div()
            .absolute()
            .top_2()
            .right_2()
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .py_1()
            .rounded(px(10.))
            .bg(t.panel)
            .border_1()
            .border_color(t.border)
            .shadow(vec![BoxShadow {
                color: hsla(0., 0., 0., 0.18),
                offset: point(px(0.), px(1.)),
                blur_radius: px(6.),
                spread_radius: px(0.),
            }])
            .occlude()
            .map(drag_window)
            .when(controls.minimize, |d| {
                d.child(icon_button("minimize", "minimize.svg", t).on_click(|_, window, _| window.minimize_window()))
            })
            .when(controls.maximize, |d| {
                let icon = if window.is_maximized() { "restore.svg" } else { "maximize.svg" };
                d.child(icon_button("maximize", icon, t).on_click(|_, window, _| window.zoom_window()))
            })
            .child(icon_button("close", "close.svg", t).on_click(|_, window, _| window.remove_window()));

        let body = div().flex_1().min_h_0().relative();
        let body = if self.vault.is_none() {
            // Sans coffre, un dossier lâché sur l'accueil devient le coffre.
            body.drag_over::<gpui::ExternalPaths>(move |style, _, _, _| style.bg(t.selection))
                .on_drop(cx.listener(|this, paths: &gpui::ExternalPaths, window, cx| {
                    if let Some(dir) = paths.paths().iter().find(|p| p.is_dir()) {
                        this.set_vault(dir.clone(), window, cx);
                    }
                }))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_3()
                .px_4()
                .text_center()
                .child(
                    div()
                        .text_size(px(26.))
                        .font_weight(gpui::FontWeight::BOLD)
                        .child("Bref"),
                )
                .child(
                    div()
                        .text_color(t.soft)
                        .child(tr(
                            "Your notes live in a folder of Markdown files: the vault.",
                            "Tes notes vivent dans un dossier de fichiers Markdown : le coffre.",
                        )),
                )
                // Le plus court chemin d'abord : un coffre tout prêt, d'un clic, sans fenêtre de
                // sélection ni dossier à créer soi-même.
                .children(vault::default_vault().map(|place| {
                    div()
                        .id("welcome-create")
                        .debug_selector(|| "welcome-create".into())
                        .mt_4()
                        .px_4()
                        .py_2()
                        .rounded(px(8.))
                        .bg(t.accent)
                        .text_color(t.bg)
                        .cursor_pointer()
                        .hover(|s| s.opacity(0.9))
                        .child(format!("{} {}", tr("Start in", "Commencer dans"), vault::shown(&place)))
                        .on_click(cx.listener(move |this, _, window, cx| this.create_vault(place.clone(), window, cx)))
                }))
                .child(
                    div()
                        .id("welcome-open")
                        .px_3()
                        .py_1()
                        .rounded(px(6.))
                        .cursor_pointer()
                        .text_color(t.accent)
                        .hover(|s| s.bg(t.border))
                        .child(tr("Open another folder…", "Ouvrir un autre dossier…"))
                        .on_click(cx.listener(|this, _, window, cx| this.choose_vault(window, cx))),
                )
                .child(
                    div()
                        .id("welcome-clone")
                        .px_3()
                        .py_1()
                        .rounded(px(6.))
                        .cursor_pointer()
                        .text_color(t.accent)
                        .hover(|s| s.bg(t.border))
                        .child(tr("Clone a vault from a git address…", "Cloner un coffre depuis une adresse git…"))
                        .on_click(cx.listener(|this, _, window, cx| this.ask_clone(window, cx))),
                )
                .child(div().text_size(px(12.)).text_color(t.soft).child(format!(
                    "{} · {}",
                    keys::of(cx, &OpenVault),
                    tr(
                        "an existing Obsidian vault works as it is",
                        "un coffre Obsidian existant fonctionne tel quel",
                    )
                )))
                // L'adresse à cloner se demande ici aussi, sans coffre.
                .children(self.palette.clone())
        } else {
            // Un pane, ou deux côte à côte : chacun se dessine comme s'il était l'actif. Le panneau
            // des commentaires, à droite de tout, est celui du pane actif.
            let (two, active) = (self.panes.len() == 2, self.active);
            let (mut areas, mut comments) = (Vec::new(), None);
            for pane in 0..self.panes.len() {
                let (area, said) = self.in_pane(pane, |this| this.render_pane(pane, pane == active, client, window, cx));
                areas.push(area);
                if pane == active {
                    comments = said;
                }
            }
            let mut areas = areas.into_iter();
            // La bordure entre les deux se tire ; un double clic la remet au milieu.
            let divider = two.then(|| {
                div()
                    .id("pane-divider")
                    .group("pane-divider")
                    .w(px(5.))
                    .h_full()
                    .flex_none()
                    .cursor(CursorStyle::ResizeLeftRight)
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .w(px(1.))
                            .h_full()
                            .bg(if self.pane_drag { t.accent } else { t.border })
                            .group_hover("pane-divider", |s| s.bg(t.accent)),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, e: &gpui::MouseDownEvent, _, cx| {
                            match e.click_count >= 2 {
                                true => this.pane_ratio = 0.5,
                                false => this.pane_drag = true,
                            }
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
            });
            let area = self.pane_box.clone();
            let note = div()
                .flex_1()
                .min_w_0()
                .h_full()
                .relative()
                .flex()
                .child(gpui::canvas(move |bounds, _, _| area.set(bounds), |_, _, _, _| ()).absolute().size_full())
                .children(areas.next())
                .children(divider)
                .children(areas.next());
            // La disposition des panes est retenue dès qu'elle change.
            let state = self.panes_state();
            if state != self.panes_saved {
                vault::save_file("panes", &state);
                self.panes_saved = state;
            }
            let card = || {
                div()
                    .pl_3()
                    .pr_2()
                    .py_1p5()
                    .rounded(px(6.))
                    .bg(t.panel)
                    .border_1()
                    .border_color(t.border)
                    .text_size(px(13.))
                    .flex()
                    .items_center()
                    .gap_3()
            };
            // Messages d'état, empilés au-dessus du compteur de mots : un clic les retire.
            let toasts = self.toasts.iter().map(|toast| {
                let id = toast.id;
                let (icon, color) = match toast.tone {
                    Tone::Info => ("info.svg", t.soft),
                    Tone::Done => ("check.svg", rgb(0x3fa46a).into()),
                    Tone::Failed => ("alert.svg", rgb(0xd9483b).into()),
                    Tone::Busy => ("spinner.svg", t.soft),
                };
                let icon = svg().path(icon).size(px(14.)).flex_none().text_color(color);
                let icon = match toast.tone {
                    Tone::Busy => icon
                        .with_animation(("toast-spin", id), Animation::new(Duration::from_secs(1)).repeat(), |icon, delta| {
                            icon.with_transformation(Transformation::rotate(percentage(delta)))
                        })
                        .into_any_element(),
                    _ => icon.into_any_element(),
                };
                card()
                    .max_w(px(440.))
                    .shadow_lg()
                    .cursor_pointer()
                    .child(icon)
                    .child(div().min_w_0().child(toast.text.clone()))
                    .child(svg().path("close.svg").size(px(14.)).flex_none().text_color(t.soft))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            this.toasts.retain(|t| t.id != id);
                            cx.notify();
                        }),
                    )
            });
            let banner = self.update.clone().map(|release| {
                let banner = card().max_w_full();
                if self.updating {
                    return banner.pr_3().child(tr("Updating, Bref restarts…", "Mise à jour, Bref redémarre…"));
                }
                // « Mettre à jour » quand l'app sait le faire seule, sinon la page de téléchargement.
                let (action, install) = match release.asset {
                    Some(_) => (tr("Update", "Mettre à jour"), true),
                    None => (tr("Download", "Télécharger"), false),
                };
                banner
                    .child(format!("{} {}", tr("New version:", "Nouvelle version :"), release.version))
                    .child(
                        div()
                            .cursor_pointer()
                            .text_color(t.accent)
                            .hover(|s| s.underline())
                            .child(action)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _, _, cx| {
                                    if install {
                                        this.install_update(cx)
                                    } else {
                                        cx.open_url(&update::releases_url())
                                    }
                                }),
                            ),
                    )
                    .child(
                        svg()
                            .path("close.svg")
                            .size(px(14.))
                            .flex_none()
                            .text_color(t.soft)
                            .cursor_pointer()
                            .hover(|s| s.text_color(t.text))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    this.update = None;
                                    cx.notify();
                                }),
                            ),
                    )
            });
            // La bannière tient à gauche du compteur de mots ; dans une fenêtre étroite, où elle le
            // couvrirait, elle rejoint la pile des messages, au-dessus de lui.
            let narrow = self.nav.total < px(640.);
            let (low, stacked) = if narrow { (None, banner) } else { (banner, None) };
            let toasts = div().absolute().bottom(px(54.)).left_4().right_4().flex().flex_col().items_end().gap_2().children(stacked).children(toasts);
            body.flex()
                .child(self.render_nav(cx))
                .when(self.nav.panel != Panel::Full, |d| d.child(note).children(comments))
                .children(self.palette.clone())
                .children(low.map(|banner| banner.absolute().bottom_3().left_16()))
                .child(toasts)
        };

        let content = div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(t.bg)
            .text_color(t.text)
            .font_family(sans())
            .when(framed, |d| {
                d.rounded(px(12.)).border_1().border_color(t.border).shadow(vec![BoxShadow {
                    color: hsla(0., 0., 0., 0.35),
                    offset: point(px(0.), px(2.)),
                    blur_radius: SHADOW / 2.,
                    spread_radius: px(0.),
                }])
            })
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &OpenPalette, window, cx| {
                this.open_palette("", window, cx)
            }))
            .on_action(cx.listener(|this, _: &NewNote, window, cx| this.new_note_here(window, cx)))
            .on_action(cx.listener(|this, _: &NewFromTemplate, window, cx| this.open_templates(TemplateAct::New, window, cx)))
            .on_action(cx.listener(|this, _: &NewDiagram, window, cx| this.new_diagram(window, cx)))
            // Échap dans un schéma où plus rien n'est en cours : retour à la note.
            .on_action(cx.listener(|this, _: &canvas::Cancel, window, cx| {
                window.focus(&this.editor.focus_handle(cx));
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &nav::ShowTree, window, cx| {
                this.show_nav(Mode::Tree, false, window, cx)
            }))
            .on_action(cx.listener(|this, _: &nav::ShowRecent, window, cx| {
                this.show_nav(Mode::Recent, false, window, cx)
            }))
            .on_action(cx.listener(|this, _: &nav::NewFolder, window, cx| this.new_folder(window, cx)))
            .on_action(cx.listener(|this, _: &nav::ShowGraph, window, cx| {
                this.show_nav(Mode::Graph, false, window, cx)
            }))
            .on_action(cx.listener(|this, _: &nav::ShowTags, window, cx| {
                this.show_nav(Mode::Tags, false, window, cx)
            }))
            .on_action(cx.listener(|this, _: &nav::ShowCalendar, window, cx| {
                this.show_nav(Mode::Calendar, false, window, cx)
            }))
            .on_action(cx.listener(|this, _: &nav::ShowLinks, window, cx| {
                this.show_nav(Mode::Links, false, window, cx)
            }))
            .on_action(cx.listener(|this, _: &nav::ToggleFull, window, cx| this.toggle_full(window, cx)))
            // Séparateur du panneau : il suit le pointeur tant que le bouton est tenu.
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, window, cx| {
                // La bordure entre les deux panes, de même.
                if this.pane_drag {
                    let area = this.pane_box.get();
                    if e.pressed_button == Some(MouseButton::Left) && area.size.width > px(0.) {
                        let ratio = (e.position.x - area.left()) / area.size.width;
                        this.pane_ratio = ratio.clamp(*PANE_RATIO.start(), *PANE_RATIO.end());
                    } else {
                        this.pane_drag = false;
                    }
                    cx.notify();
                }
                if !this.nav.dragging {
                } else if e.pressed_button == Some(MouseButton::Left) {
                    this.drag_nav(e.position.x, cx)
                } else {
                    this.settle_nav(window, cx)
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    if std::mem::take(&mut this.pane_drag) {
                        cx.notify();
                    }
                    if this.nav.dragging {
                        this.settle_nav(window, cx)
                    }
                }),
            )
            .on_action(cx.listener(|this, _: &sheet::PickDelimiter, window, cx| this.choose_table(Pick::Delimiter, window, cx)))
            .on_action(cx.listener(|this, _: &sheet::PickEncoding, window, cx| this.choose_table(Pick::Encoding, window, cx)))
            .on_action(cx.listener(|this, _: &sheet::PickHeader, window, cx| this.choose_table(Pick::Header, window, cx)))
            .on_action(cx.listener(|this, _: &sheet::PickCopyAs, window, cx| this.choose_table(Pick::CopyAs, window, cx)))
            .on_action(cx.listener(|this, _: &sheet::PickExport, window, cx| this.choose_table(Pick::Export, window, cx)))
            .on_action(cx.listener(|this, _: &OpenVault, window, cx| this.choose_vault(window, cx)))
            .on_action(cx.listener(|this, _: &CopyAll, _, cx| this.copy_all(true, cx)))
            .on_action(cx.listener(|this, _: &Outline, window, cx| this.open_outline(window, cx)))
            .on_action(cx.listener(|this, _: &Today, _, cx| this.open_today(cx)))
            .on_action(cx.listener(|this, _: &SyncVault, window, cx| this.git_sync(window, cx)))
            .on_action(cx.listener(|this, _: &SplitPane, window, cx| this.split(window, cx)))
            .on_action(cx.listener(|this, _: &FocusLeft, window, cx| this.focus_pane(0, window, cx)))
            .on_action(cx.listener(|this, _: &FocusRight, window, cx| this.focus_pane(1, window, cx)))
            .on_action(cx.listener(|this, _: &ClosePane, window, cx| this.close_pane(window, cx)))
            .on_action(cx.listener(|this, _: &GoBack, window, cx| this.travel(false, window, cx)))
            .on_action(cx.listener(|this, _: &GoForward, window, cx| this.travel(true, window, cx)))
            // Les boutons précédent et suivant de la souris.
            .on_mouse_down(
                MouseButton::Navigate(gpui::NavigationDirection::Back),
                cx.listener(|this, _, window, cx| this.travel(false, window, cx)),
            )
            .on_mouse_down(
                MouseButton::Navigate(gpui::NavigationDirection::Forward),
                cx.listener(|this, _, window, cx| this.travel(true, window, cx)),
            )
            .on_action(cx.listener(|this, _: &SearchVault, window, cx| this.open_search(window, cx)))
            .on_action(cx.listener(|this, _: &ChooseTheme, window, cx| {
                this.choose_setting(Setting::Theme, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ZoomIn, window, cx| this.resize_text(Some(1.), window, cx)))
            .on_action(cx.listener(|this, _: &ZoomOut, window, cx| this.resize_text(Some(-1.), window, cx)))
            .on_action(cx.listener(|this, _: &ZoomReset, window, cx| this.resize_text(None, window, cx)))
            .on_action(cx.listener(|this, _: &ToggleHelp, window, cx| {
                this.set_help(!this.help, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ToggleCommands, window, cx| this.open_commands(window, cx)))
            // Échap dans la note, quand elle n'a rien à fermer : le menu du clic droit.
            .on_action(cx.listener(|this, _: &editor::Cancel, _, cx| {
                this.icon_pick = None;
                this.menu = None;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &CloseHelp, window, cx| {
                if this.help {
                    this.set_help(false, window, cx)
                } else if this.zone.is_focused(window) {
                    // Un pane qui ne montre qu'une image : Échap revient à sa note.
                    window.focus(&this.editor.focus_handle(cx));
                }
            }))
            .key_context("Shell")
            .child(body)
            .when(client, |d| d.child(pill))
            .children(self.render_menu(window, cx))
            .children(self.render_icons(window, cx))
            .children(self.render_drop_chip(window.viewport_size()))
            .when(self.help, |d| d.child(self.render_help(window.viewport_size().height < px(420.), cx)));

        // La marge transparente porte l'ombre et sert de poignée de redimensionnement.
        div()
            .size_full()
            // Alt + glisser, n'importe où : déplace la fenêtre (la phase de capture passe
            // avant l'éditeur, qui n'utilise pas Alt avec la souris).
            .when(client, |d| {
                d.capture_any_mouse_down(|e, window, cx| {
                    if e.button == MouseButton::Left && e.modifiers.alt {
                        window.start_window_move();
                        cx.stop_propagation();
                    }
                })
            })
            .when(framed, |d| {
                d.p(SHADOW)
                    .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, window, cx| {
                        let edge = resize_edge(e.position, window.viewport_size());
                        if edge != this.edge {
                            this.edge = edge;
                            cx.notify();
                        }
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, _| {
                            if let Some(edge) = this.edge {
                                window.start_window_resize(edge)
                            }
                        }),
                    )
                    .when_some(self.edge, |d, edge| {
                        d.cursor(match edge {
                            ResizeEdge::Top | ResizeEdge::Bottom => CursorStyle::ResizeUpDown,
                            ResizeEdge::Left | ResizeEdge::Right => CursorStyle::ResizeLeftRight,
                            ResizeEdge::TopLeft | ResizeEdge::BottomRight => {
                                CursorStyle::ResizeUpLeftDownRight
                            }
                            ResizeEdge::TopRight | ResizeEdge::BottomLeft => {
                                CursorStyle::ResizeUpRightDownLeft
                            }
                        })
                    })
            })
            .child(content)
    }
}

/// Bord ou coin de fenêtre sous le pointeur, dans une marge de quelques pixels.
fn resize_edge(pos: Point<Pixels>, size: Size<Pixels>) -> Option<ResizeEdge> {
    let m = SHADOW;
    let (left, right) = (pos.x < m, pos.x > size.width - m);
    let (top, bottom) = (pos.y < m, pos.y > size.height - m);
    Some(match (top, bottom, left, right) {
        (true, _, true, _) => ResizeEdge::TopLeft,
        (true, _, _, true) => ResizeEdge::TopRight,
        (_, true, true, _) => ResizeEdge::BottomLeft,
        (_, true, _, true) => ResizeEdge::BottomRight,
        (true, ..) => ResizeEdge::Top,
        (_, true, ..) => ResizeEdge::Bottom,
        (_, _, true, _) => ResizeEdge::Left,
        (_, _, _, true) => ResizeEdge::Right,
        _ => return None,
    })
}

/// La table des raccourcis par défaut. Son ordre compte : à contexte égal, la dernière liaison
/// l'emporte.
fn defaults() -> Vec<keys::Bind> {
    use editor::*;
    use keys::bind as b;
    use palette::{Confirm, Dismiss, Next, Prev};
    let e = Some("Editor");
    let p = Some("Palette");
    let f = Some("Find");
    let n = Some("Nav");
    let c = Some("Canvas");
    let sh = Some("Sheet");
    // `secondary` = Cmd sur macOS, Ctrl ailleurs ; les mots se parcourent avec Alt sur macOS.
    let word = if cfg!(target_os = "macos") { "alt" } else { "ctrl" };
    let mut all = vec![
        b("secondary-p", &OpenPalette, None),
        b("secondary-n", &NewNote, None),
        b("secondary-o", &OpenVault, None),
        b("secondary-shift-c", &CopyAll, None),
        b("f1", &ToggleHelp, None),
        b("secondary-/", &ToggleHelp, None),
        b("escape", &CloseHelp, Some("Shell")),
        b("secondary-shift-o", &Outline, None),
        b("secondary-j", &Today, None),
        b("secondary-shift-s", &SyncVault, None),
        b("secondary-\\", &SplitPane, None),
        b("secondary-1", &FocusLeft, None),
        b("secondary-2", &FocusRight, None),
        b("secondary-w", &ClosePane, None),
        // Sous macOS, Alt+flèche parcourt les mots : les crochets, comme dans un navigateur.
        b(if cfg!(target_os = "macos") { "cmd-[" } else { "alt-left" }, &GoBack, None),
        b(if cfg!(target_os = "macos") { "cmd-]" } else { "alt-right" }, &GoForward, None),
        b("secondary-shift-f", &SearchVault, None),
        b("secondary-=", &ZoomIn, None),
        b("secondary-+", &ZoomIn, None),
        b("secondary--", &ZoomOut, None),
        b("secondary-0", &ZoomReset, None),
        b("secondary-q", &Quit, None),
        // La palette des commandes : F10, comme la barre de menus d'une app de bureau.
        b("f10", &ToggleCommands, None),
        b("secondary-shift-d", &NewDiagram, Some("Shell")),
        b("secondary-=", &canvas::ZoomIn, c),
        b("secondary-+", &canvas::ZoomIn, c),
        b("secondary--", &canvas::ZoomOut, c),
        b("secondary-0", &canvas::ZoomFit, c),
        b("backspace", &canvas::Erase, c),
        b("delete", &canvas::EraseNext, c),
        b("left", &canvas::Left, c),
        b("right", &canvas::Right, c),
        b("up", &canvas::Up, c),
        b("down", &canvas::Down, c),
        b("secondary-z", &canvas::Undo, c),
        b("secondary-shift-z", &canvas::Redo, c),
        b("secondary-d", &canvas::Duplicate, c),
        b("secondary-a", &canvas::SelectAll, c),
        b("secondary-v", &canvas::Paste, c),
        b("enter", &canvas::Confirm, c),
        b("escape", &canvas::Cancel, c),
        // Tableau (CSV, TSV) : flèches, sélection avec Maj, saisie, presse-papiers, annuler.
        b("left", &sheet::Left, sh),
        b("right", &sheet::Right, sh),
        b("up", &sheet::Up, sh),
        b("down", &sheet::Down, sh),
        b("shift-left", &sheet::ExtendLeft, sh),
        b("shift-right", &sheet::ExtendRight, sh),
        b("shift-up", &sheet::ExtendUp, sh),
        b("shift-down", &sheet::ExtendDown, sh),
        b("pageup", &sheet::PageUp, sh),
        b("pagedown", &sheet::PageDown, sh),
        b("home", &sheet::RowStart, sh),
        b("end", &sheet::RowEnd, sh),
        b("secondary-home", &sheet::Origin, sh),
        b("secondary-end", &sheet::Corner, sh),
        b("secondary-up", &sheet::FirstRow, sh),
        b("secondary-down", &sheet::LastRow, sh),
        b("tab", &sheet::Next, sh),
        b("shift-tab", &sheet::Previous, sh),
        b("enter", &sheet::Enter, sh),
        b("f2", &sheet::Edit, sh),
        b("escape", &sheet::Cancel, sh),
        b("backspace", &sheet::Backspace, sh),
        b("delete", &sheet::Delete, sh),
        b("secondary-c", &sheet::Copy, sh),
        b("secondary-x", &sheet::Cut, sh),
        b("secondary-v", &sheet::Paste, sh),
        b("secondary-a", &sheet::SelectAll, sh),
        b("secondary-z", &sheet::Undo, sh),
        b("secondary-shift-z", &sheet::Redo, sh),
        b("secondary-y", &sheet::Redo, sh),
        b("secondary-enter", &sheet::InsertBelow, sh),
        b("secondary-shift-enter", &sheet::InsertAbove, sh),
        b("secondary-delete", &sheet::DeleteRows, sh),
        // Comme dans un tableur : la ligne, la colonne de la sélection entières.
        b("shift-space", &sheet::SelectRow, sh),
        b("ctrl-space", &sheet::SelectCol, sh),
        b("secondary-shift-backspace", &TableDeleteSpan, sh),
        b("secondary-e", &nav::ShowTree, Some("Shell")),
        b("secondary-r", &nav::ShowRecent, Some("Shell")),
        b("secondary-g", &nav::ShowGraph, Some("Shell")),
        b("secondary-t", &nav::ShowTags, Some("Shell")),
        b("secondary-l", &nav::ShowLinks, Some("Shell")),
        b("secondary-shift-j", &nav::ShowCalendar, Some("Shell")),
        b("secondary-m", &nav::ToggleFull, Some("Shell")),
        b("tab", &graph::Cycle, n),
        b("secondary-shift-n", &nav::NewFolder, Some("Shell")),
        b("f2", &nav::Rename, n),
        b("secondary-d", &nav::Duplicate, n),
        b("delete", &nav::Trash, n),
        b("up", &nav::Prev, n),
        b("down", &nav::Next, n),
        b("left", &nav::Fold, n),
        b("right", &nav::Unfold, n),
        b("enter", &nav::Open, n),
        b("pageup", &nav::PrevMonth, n),
        b("pagedown", &nav::NextMonth, n),
        b("escape", &nav::Close, n),
        b("up", &Prev, p),
        b("down", &Next, p),
        b("enter", &Confirm, p),
        b("secondary-enter", &palette::ConfirmAside, p),
        b("escape", &Dismiss, p),
        b("backspace", &Backspace, e),
        b("delete", &Delete, e),
        b(&format!("{word}-backspace"), &DeleteWordLeft, e),
        b(&format!("{word}-delete"), &DeleteWordRight, e),
        b("left", &Left, e),
        b("right", &Right, e),
        b("up", &Up, e),
        b("down", &Down, e),
        b(&format!("{word}-left"), &WordLeft, e),
        b(&format!("{word}-right"), &WordRight, e),
        b("home", &Home, e),
        b("end", &End, e),
        b("secondary-home", &DocStart, e),
        b("secondary-end", &DocEnd, e),
        b("pageup", &PageUp, e),
        b("pagedown", &PageDown, e),
        b("shift-left", &SelectLeft, e),
        b("shift-right", &SelectRight, e),
        b("shift-up", &SelectUp, e),
        b("shift-down", &SelectDown, e),
        b(&format!("{word}-shift-left"), &SelectWordLeft, e),
        b(&format!("{word}-shift-right"), &SelectWordRight, e),
        b("shift-home", &SelectHome, e),
        b("shift-end", &SelectEnd, e),
        b("secondary-shift-home", &SelectDocStart, e),
        b("secondary-shift-end", &SelectDocEnd, e),
        b("secondary-a", &SelectAll, e),
        // Recherche dans la note : la barre a son propre contexte tant qu'elle reçoit la saisie.
        b("secondary-f", &Find, e),
        b("secondary-f", &Find, f),
        b("secondary-h", &FindReplace, e),
        b("secondary-h", &FindReplace, f),
        b("f3", &FindNext, e),
        b("shift-f3", &FindPrev, e),
        b("f3", &FindNext, f),
        b("shift-f3", &FindPrev, f),
        b("enter", &FindEnter, f),
        b("shift-enter", &FindPrev, f),
        b("secondary-enter", &ReplaceAll, f),
        b("secondary-shift-enter", &ReplaceInVault, f),
        b("escape", &FindClose, f),
        b("backspace", &FindErase, f),
        b("tab", &FindSwitch, f),
        // Aussi quand la barre est ouverte mais que la saisie est revenue à la note.
        b("alt-c", &FindCase, f),
        b("alt-w", &FindWord, f),
        b("alt-r", &FindRegex, f),
        b("alt-r", &FindRegex, e),
        b("alt-c", &FindCase, e),
        b("alt-w", &FindWord, e),
        b("secondary-v", &FindPaste, f),
        b("enter", &Newline, e),
        b("tab", &Indent, e),
        b("shift-tab", &Outdent, e),
        b("secondary-enter", &ToggleTask, e),
        // Les quatre choix directs n'ont pas de touche : le menu et la palette les offrent, et l'aide les accorde.
        b("secondary-shift-b", &ListCycle, e),
        // Un tableau : monter ou descendre la ligne, la supprimer ; le reste par le menu, la palette ou l'aide.
        // Hors d'un tableau, Alt+flèche monte ou descend le bloc du curseur ; dans un tableau, sa ligne.
        b("alt-up", &BlockUp, e),
        b("alt-up", &TableMoveRowUp, sh),
        b("alt-down", &TableMoveRowDown, sh),
        b("alt-down", &BlockDown, e),
        b("secondary-shift-backspace", &TableDeleteSpan, e),
        b("secondary-shift-l", &AlignLeft, e),
        b("secondary-shift-e", &AlignCenter, e),
        b("secondary-shift-r", &AlignRight, e),
        b("secondary-b", &Bold, e),
        b("secondary-i", &Italic, e),
        b("secondary-u", &Underline, e),
        b("secondary-shift-x", &Strike, e),
        b("secondary-shift-h", &Highlight, e),
        b("secondary-shift-k", &InlineCode, e),
        b("secondary-k", &InsertLink, e),
        b("secondary-d", &SelectNext, e),
        b("secondary-shift-m", &Comment, e),
        b("secondary-c", &Copy, e),
        b("secondary-x", &Cut, e),
        b("secondary-v", &Paste, e),
        b("secondary-z", &Undo, e),
        b("secondary-shift-z", &Redo, e),
        b("secondary-y", &Redo, e),
        // Un geste fait sur le tableau kanban s'annule depuis le tableau.
        b("secondary-z", &Undo, Some("Board")),
        b("secondary-shift-z", &Redo, Some("Board")),
        b("secondary-y", &Redo, Some("Board")),
        b("escape", &Cancel, e),
    ];
    // Après les champs qui s'en servent (palette, grille) : ses touches passent avant les leurs.
    all.extend(line::bindings(word));
    // Conventions macOS : Cmd+flèches pour les extrémités de ligne et de document.
    #[cfg(target_os = "macos")]
    all.extend([
        b("cmd-left", &Home, e),
        b("cmd-right", &End, e),
        b("cmd-up", &DocStart, e),
        b("cmd-down", &DocEnd, e),
        b("cmd-shift-left", &SelectHome, e),
        b("cmd-shift-right", &SelectEnd, e),
        b("cmd-shift-up", &SelectDocStart, e),
        b("cmd-shift-down", &SelectDocEnd, e),
    ]);
    all
}

/// Remplace toutes les liaisons de l'app par celles-ci.
fn install(cx: &mut App, binds: &[keys::Bind]) {
    let binds: Vec<KeyBinding> = binds
        .iter()
        .filter_map(|bind| {
            let action = cx.build_action(bind.action, None).ok()?;
            let context = bind.context.map(|context| gpui::KeyBindingContextPredicate::parse(context).unwrap().into());
            KeyBinding::load(&bind.keys, action, context, false, None, &gpui::DummyKeyboardMapper).ok()
        })
        .collect();
    cx.clear_key_bindings();
    cx.bind_keys(binds);
}

fn bind_keys(cx: &mut App) {
    install(cx, &defaults());
}

fn main() {
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
