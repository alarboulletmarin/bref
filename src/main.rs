// Pas de console derrière la fenêtre sous Windows.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod canvas;
mod diagram;
mod editor;
mod figure;
mod graph;
mod import;
mod markdown;
mod nav;
mod palette;
mod vault;

use std::{
    borrow::Cow,
    fs,
    path::{Path, PathBuf},
    sync::{OnceLock, RwLock},
    time::{Duration, Instant, SystemTime},
};

use gpui::{
    Animation, AnimationExt, App, Application, AssetSource, Bounds, BoxShadow, ClipboardItem, Context,
    CursorStyle, Decorations, Entity, FocusHandle, Focusable, Hsla, KeyBinding, MouseButton,
    MouseMoveEvent, ObjectFit, PathPromptOptions, Pixels, Point, ResizeEdge, SharedString, Size, TitlebarOptions,
    Window, WindowAppearance, WindowBounds, WindowBackgroundAppearance, WindowDecorations,
    WindowOptions, actions, div, ease_out_quint, hsla, img, point, prelude::*, px, rgb, size, svg,
};

use canvas::{Canvas, CanvasEvent};
use diagram::Diagram;
use editor::{Editor, EditorEvent};
use graph::{Graph, GraphEvent};
use markdown::Link;
use nav::{Mode, Nav, Panel};
use palette::{Entry, Palette, PaletteEvent, Setting};
use vault::Note;

/// Touche des raccourcis, telle qu'affichée : Cmd sur macOS, Ctrl ailleurs.
pub const MOD: &str = if cfg!(target_os = "macos") { "Cmd" } else { "Ctrl" };

/// Texte d'interface : français si la langue du système l'est, anglais sinon.
// ponytail: langue lue dans LC_ALL / LC_MESSAGES / LANG, absentes sous Windows
// (donc anglais) ; interroger l'API de locale Windows si le besoin apparaît.
pub fn tr(en: &'static str, fr: &'static str) -> &'static str {
    static FRENCH: OnceLock<bool> = OnceLock::new();
    let french = *FRENCH.get_or_init(|| {
        // Les tests tournent en français pour rester déterministes.
        cfg!(test)
            || ["LC_ALL", "LC_MESSAGES", "LANG"]
                .iter()
                .find_map(|key| std::env::var(key).ok().filter(|v| !v.is_empty()))
                .is_some_and(|lang| lang.starts_with("fr"))
    });
    if french { fr } else { en }
}

/// Polices installées, relevées au démarrage.
static FAMILIES: OnceLock<Vec<&'static str>> = OnceLock::new();

/// Polices (texte, code) en usage : celles choisies, sinon celles par défaut.
static FONTS: RwLock<(&str, &str)> = RwLock::new(("Cantarell", "DejaVu Sans Mono"));

pub fn sans() -> &'static str {
    FONTS.read().unwrap().0
}

pub fn mono() -> &'static str {
    FONTS.read().unwrap().1
}

fn init_fonts(cx: &mut App) {
    FAMILIES.get_or_init(|| {
        let names: &'static [String] = cx.text_system().all_font_names().leak();
        names.iter().map(String::as_str).collect()
    });
}

/// Met en usage les polices des réglages ; à défaut, les premières installées
/// de chaque liste.
fn apply_fonts(prefs: &Prefs) {
    let installed = FAMILIES.get().map_or(&[][..], Vec::as_slice);
    let pick = |chosen: &str, wanted: &[&'static str]| {
        let known = |name: &str| installed.iter().copied().find(|i| *i == name);
        known(chosen)
            .or_else(|| wanted.iter().find_map(|w| known(w)))
            .unwrap_or(wanted[wanted.len() - 1])
    };
    *FONTS.write().unwrap() = (
        // Adwaita Sans et Cantarell sont des polices variables : pas de gras avec gpui 0.2.
        pick(&prefs.font, &["Inter", "Noto Sans", "Segoe UI", "Helvetica Neue", "DejaVu Sans", "Cantarell"]),
        pick(
            &prefs.mono,
            &[
                "JetBrains Mono",
                "JetBrainsMono Nerd Font",
                "Adwaita Mono",
                "Noto Sans Mono",
                "Menlo",
                "Consolas",
                "DejaVu Sans Mono",
            ],
        ),
    );
}

/// Apparence choisie par l'utilisateur ; un champ vide garde la valeur par défaut.
#[derive(Clone, Debug, PartialEq)]
struct Prefs {
    theme: String,
    font: String,
    mono: String,
    /// Taille du texte courant de la note, en pixels.
    size: f32,
}

impl Prefs {
    const SIZE: f32 = 16.;

    fn parse(text: &str) -> Self {
        let mut prefs = Self { theme: String::new(), font: String::new(), mono: String::new(), size: Self::SIZE };
        for (key, value) in text.lines().filter_map(|line| line.split_once('=')) {
            match key {
                "theme" => prefs.theme = value.into(),
                "font" => prefs.font = value.into(),
                "mono" => prefs.mono = value.into(),
                "size" => prefs.size = value.parse().ok().filter(|s: &f32| s.is_finite()).unwrap_or(Self::SIZE),
                _ => {}
            }
        }
        prefs.size = prefs.size.clamp(11., 32.);
        prefs
    }

    fn to_text(&self) -> String {
        format!("theme={}\nfont={}\nmono={}\nsize={}\n", self.theme, self.font, self.mono, self.size)
    }
}

/// Marge transparente autour de la fenêtre quand l'app dessine ses décorations.
const SHADOW: Pixels = px(10.);

/// Icônes embarquées dans le binaire (traits de 1,2 px sur une grille de 16 px).
struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        let shapes = match path {
            "minimize.svg" => r#"<path d="M4 8H12"/>"#,
            "maximize.svg" => r#"<path d="M4.5 4.5H11.5V11.5H4.5Z"/>"#,
            "restore.svg" => r#"<path d="M3.5 6.5H9.5V12.5H3.5Z"/><path d="M10 8.5H12.5V3.5H7.5V6"/>"#,
            "close.svg" => r#"<path d="M4.5 4.5L11.5 11.5M11.5 4.5L4.5 11.5"/>"#,
            "copy.svg" => {
                r#"<rect x="6" y="6" width="7.5" height="7.5" rx="1.5"/><path d="M10 4A1.5 1.5 0 0 0 8.5 2.5H4A1.5 1.5 0 0 0 2.5 4V8.5A1.5 1.5 0 0 0 4 10"/>"#
            }
            "check.svg" => r#"<path d="M4 8.5L7 11L12 5"/>"#,
            "tree.svg" => r#"<path d="M3 3.5H10M3 3.5V12H6M3 7.75H6M8.5 7.75H13M8.5 12H13"/>"#,
            "clock.svg" => r#"<circle cx="8" cy="8" r="5.5"/><path d="M8 5V8L10 9.5"/>"#,
            "search.svg" => r#"<circle cx="7" cy="7" r="3.5"/><path d="M9.7 9.7L12.5 12.5"/>"#,
            "plus.svg" => r#"<path d="M8 3.5V12.5M3.5 8H12.5"/>"#,
            "help.svg" => {
                r#"<circle cx="8" cy="8" r="5.5"/><path d="M6.4 6.6A1.6 1.6 0 1 1 8 8.4V9.2M8 11.2V11.3"/>"#
            }
            "chevron-right.svg" => r#"<path d="M6.5 4.5L10 8L6.5 11.5"/>"#,
            "chevron-down.svg" => r#"<path d="M4.5 6.5L8 10L11.5 6.5"/>"#,
            "graph.svg" => {
                r#"<circle cx="4" cy="11.5" r="1.7"/><circle cx="11.5" cy="4.5" r="1.7"/><circle cx="12" cy="12" r="1.3"/><path d="M5.3 10.3L10.2 5.7M11.6 6.2L11.9 10.7"/>"#
            }
            "tag.svg" => r#"<path d="M6.5 3L5 13M11 3L9.5 13M3.5 6H13M3 10H12.5"/>"#,
            "code.svg" => r#"<path d="M5.5 4.5L2.5 8L5.5 11.5M10.5 4.5L13.5 8L10.5 11.5"/>"#,
            "heart.svg" => {
                r#"<path d="M8 13C3.5 9.8 2.5 7.6 2.5 5.9A2.6 2.6 0 0 1 8 4.9A2.6 2.6 0 0 1 13.5 5.9C13.5 7.6 12.5 9.8 8 13Z"/>"#
            }
            "d-select.svg" => r#"<path d="M4 3V12L6.5 9.8L8.2 13.2L9.6 12.5L7.9 9.2L11 9Z"/>"#,
            "d-rect.svg" => r#"<rect x="2.5" y="4" width="11" height="8" rx="0.5"/>"#,
            "d-round.svg" => r#"<rect x="2.5" y="4" width="11" height="8" rx="3"/>"#,
            "d-ellipse.svg" => r#"<ellipse cx="8" cy="8" rx="5.5" ry="4"/>"#,
            "d-diamond.svg" => r#"<path d="M8 2.5L13.5 8L8 13.5L2.5 8Z"/>"#,
            "d-cylinder.svg" => {
                r#"<ellipse cx="8" cy="4.5" rx="4.5" ry="1.8"/><path d="M3.5 4.5V11.5A4.5 1.8 0 0 0 12.5 11.5V4.5"/>"#
            }
            "d-actor.svg" => r#"<circle cx="8" cy="4" r="1.7"/><path d="M8 5.7V10M5 7.5H11M5.5 13.5L8 10L10.5 13.5"/>"#,
            "d-note.svg" => r#"<path d="M3.5 3H10L12.5 5.5V13H3.5ZM10 3V5.5H12.5"/>"#,
            "d-text.svg" => r#"<path d="M4 4.5V3.5H12V4.5M8 3.5V12.5M6.5 12.5H9.5"/>"#,
            "d-arrow.svg" => r#"<path d="M3 13L13 3M7.5 3H13V8.5"/>"#,
            "d-line.svg" => r#"<path d="M3 13L13 3"/>"#,
            "d-fill.svg" => r#"<rect x="3" y="3" width="10" height="10" rx="1.5" fill="black" fill-opacity="0.35"/>"#,
            "d-dash.svg" => r#"<path d="M2.5 8H5M7 8H9M11 8H13.5"/>"#,
            "d-head-start.svg" => r#"<path d="M13.5 8H3M6.5 4.5L3 8L6.5 11.5"/>"#,
            "d-head-end.svg" => r#"<path d="M2.5 8H13M9.5 4.5L13 8L9.5 11.5"/>"#,
            "export.svg" => r#"<path d="M8 2.5V10M5 7L8 10L11 7M3 13H13"/>"#,
            "target.svg" => r#"<circle cx="8" cy="8" r="2"/><path d="M8 2.5V5M8 11V13.5M2.5 8H5M11 8H13.5"/>"#,
            "folder-plus.svg" => {
                r#"<path d="M2.5 4.5A1 1 0 0 1 3.5 3.5H6.5L8 5H12.5A1 1 0 0 1 13.5 6V11.5A1 1 0 0 1 12.5 12.5H3.5A1 1 0 0 1 2.5 11.5Z"/><path d="M8 7.2V10.4M6.4 8.8H9.6"/>"#
            }
            "unfold.svg" => r#"<path d="M4.5 6.5L8 3.5L11.5 6.5M4.5 9.5L8 12.5L11.5 9.5"/>"#,
            "fold.svg" => r#"<path d="M4.5 3.5L8 6.5L11.5 3.5M4.5 12.5L8 9.5L11.5 12.5"/>"#,
            "expand.svg" => r#"<path d="M9.5 3.5H12.5V6.5M6.5 12.5H3.5V9.5M12.5 3.5L9 7M3.5 12.5L7 9"/>"#,
            "shrink.svg" => r#"<path d="M12.5 6.5H9.5V3.5M3.5 9.5H6.5V12.5M9.5 6.5L13 3M6.5 9.5L3 13"/>"#,
            "theme.svg" => {
                r#"<circle cx="8" cy="8" r="5.5"/><path d="M8 2.5A5.5 5.5 0 0 1 8 13.5Z" fill="black"/>"#
            }
            // Les deux fiches de l'icône de l'app, pleines ; celle du dessous est estompée.
            "logo.svg" => {
                r#"<rect x="1.8" y="1.8" width="6.2" height="7.6" rx="1.4" transform="rotate(-7 5 5.5)" fill="black" stroke="none"/><rect x="8.3" y="6.6" width="6.2" height="7.6" rx="1.4" transform="rotate(7 11.4 10.4)" fill="black" stroke="none" opacity=".55"/>"#
            }
            _ => return Ok(None),
        };
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="black" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round">{shapes}</svg>"#
        );
        Ok(Some(Cow::Owned(svg.into_bytes())))
    }

    fn list(&self, _: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(Vec::new())
    }
}

/// Le logo de l'app, discret : les deux fiches, à la couleur d'accent.
pub fn logo(t: Theme) -> gpui::Svg {
    svg().path("logo.svg").size(px(15.)).flex_none().text_color(t.accent.opacity(0.85))
}

actions!(app, [OpenPalette, NewNote, NewDiagram, OpenVault, CopyAll, ToggleHelp, CloseHelp, ChooseTheme, ZoomIn, ZoomOut, ZoomReset, Quit]);

#[derive(Clone, Copy)]
pub struct Theme {
    pub bg: Hsla,
    pub text: Hsla,
    pub dim: Hsla,
    pub accent: Hsla,
    pub selection: Hsla,
    pub code_bg: Hsla,
    pub panel: Hsla,
    pub border: Hsla,
    /// Taille du texte courant de la note, en pixels.
    pub size: f32,
}

/// Thèmes proposés : (nom, [fond, texte, discret, accent, fond du code, panneau, bordure]).
/// Les deux premiers sont ceux de Bref, sombre et clair, qui suivent le système.
const THEMES: &[(&str, [u32; 7])] = &[
    ("Bref Dark", [0x1b1a19, 0xe4e0da, 0x77716a, 0xe0926b, 0x262423, 0x242221, 0x3a3735]),
    ("Bref Light", [0xfbfaf8, 0x26231f, 0xa8a29a, 0xb4532a, 0xf1eeea, 0xffffff, 0xe2ddd6]),
    ("Dracula", [0x282a36, 0xf8f8f2, 0x6272a4, 0xbd93f9, 0x21222c, 0x343746, 0x44475a]),
    ("One Dark", [0x282c34, 0xabb2bf, 0x5c6370, 0x61afef, 0x21252b, 0x2c313a, 0x3e4451]),
    ("Gruvbox Dark", [0x282828, 0xebdbb2, 0x928374, 0xfabd2f, 0x32302f, 0x3c3836, 0x504945]),
    ("Nord", [0x2e3440, 0xd8dee9, 0x616e88, 0x88c0d0, 0x3b4252, 0x3b4252, 0x4c566a]),
    ("Catppuccin Mocha", [0x1e1e2e, 0xcdd6f4, 0x6c7086, 0xcba6f7, 0x181825, 0x313244, 0x45475a]),
    ("Tokyo Night", [0x1a1b26, 0xc0caf5, 0x565f89, 0x7aa2f7, 0x16161e, 0x24283b, 0x3b4261]),
    ("Rosé Pine", [0x191724, 0xe0def4, 0x6e6a86, 0xebbcba, 0x1f1d2e, 0x26233a, 0x403d52]),
    ("Solarized Dark", [0x002b36, 0x93a1a1, 0x586e75, 0x268bd2, 0x073642, 0x073642, 0x0a4856]),
    ("Solarized Light", [0xfdf6e3, 0x586e75, 0x93a1a1, 0x268bd2, 0xeee8d5, 0xfffbf0, 0xd9d2c2]),
    ("Catppuccin Latte", [0xeff1f5, 0x4c4f69, 0x9ca0b0, 0x8839ef, 0xe6e9ef, 0xffffff, 0xccd0da]),
    ("Gruvbox Light", [0xfbf1c7, 0x3c3836, 0x928374, 0xb57614, 0xf2e5bc, 0xf9f5d7, 0xd5c4a1]),
];

impl Theme {
    /// Le thème choisi ; à défaut, celui de Bref accordé à l'apparence du système.
    fn of(prefs: &Prefs, appearance: WindowAppearance) -> Self {
        let dark = matches!(appearance, WindowAppearance::Dark | WindowAppearance::VibrantDark);
        let system = &THEMES[if dark { 0 } else { 1 }];
        let (_, colors) = THEMES.iter().find(|t| t.0 == prefs.theme).unwrap_or(system);
        let [bg, text, dim, accent, code_bg, panel, border] = colors.map(|c| Hsla::from(rgb(c)));
        Self {
            bg,
            text,
            dim,
            accent,
            selection: accent.opacity(if bg.l < 0.5 { 0.25 } else { 0.2 }),
            code_bg,
            panel,
            border,
            size: prefs.size,
        }
    }
}

struct Shell {
    focus: FocusHandle,
    editor: Entity<Editor>,
    nav: Nav,
    graph: Entity<Graph>,
    /// Les notes ou leurs liens ont changé depuis le dernier calcul du graphe.
    graph_stale: bool,
    palette: Option<Entity<Palette>>,
    vault: Option<PathBuf>,
    notes: Vec<Note>,
    /// Dossiers du coffre, y compris ceux qui ne contiennent aucune note.
    dirs: Vec<PathBuf>,
    /// Images du coffre.
    images: Vec<PathBuf>,
    /// Image affichée à la place de la note, choisie dans l'arbre ou le graphe.
    picture: Option<PathBuf>,
    /// Schéma ouvert dans son canevas, quand l'image affichée en est un.
    drawing: Option<(PathBuf, Entity<Canvas>)>,
    /// Menu contextuel de l'arbre, s'il est ouvert.
    menu: Option<nav::Menu>,
    /// Notes ouvertes, de la plus récente à la plus ancienne.
    recent: Vec<PathBuf>,
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
    dirty: bool,
    save_gen: usize,
    error: Option<String>,
    title: String,
    prefs: Prefs,
    theme: Theme,
    /// Bord de fenêtre survolé (redimensionnement sans décorations système).
    edge: Option<ResizeEdge>,
    copied: bool,
    help: bool,
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
        let editor = cx.new(|cx| Editor::new(theme, cx));
        cx.subscribe_in(&editor, window, Self::on_editor_event).detach();
        cx.observe_window_appearance(window, |this, window, cx| this.restyle(window, cx)).detach();
        cx.on_app_quit(|this, cx| {
            this.leave(cx);
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
            focus: cx.focus_handle(),
            editor,
            nav,
            graph,
            graph_stale: true,
            palette: None,
            vault: None,
            notes: Vec::new(),
            dirs: Vec::new(),
            images: Vec::new(),
            picture: None,
            drawing: None,
            menu: None,
            recent: Vec::new(),
            path: None,
            synced: true,
            h1: None,
            origin: None,
            preview: false,
            new_dir: None,
            dirty: false,
            save_gen: 0,
            error: None,
            title: String::new(),
            prefs,
            theme,
            edge: None,
            copied: false,
            help: false,
        };
        match vault {
            Some(root) => {
                this.set_vault(root, window, cx);
                this.recent = recent.into_iter().filter(|p| p.is_file()).collect();
                if let Some(last) = this.recent.first().cloned() {
                    this.open_note(&last, cx);
                }
                if this.nav.panel == Panel::Full {
                    window.focus(&this.nav.focus);
                }
            }
            None => window.focus(&this.focus),
        }
        this.watch(cx);
        this
    }

    /// Suit ce que d'autres programmes changent dans le coffre : notes ajoutées,
    /// renommées, supprimées, ou modifiées pendant qu'elles sont affichées.
    // ponytail: l'empreinte du coffre (noms et dates, sans lire les fichiers) est
    // recalculée toutes les 2 s, hors du thread UI, plutôt que d'écouter le système
    // de fichiers. Le parcours ne prend jamais plus de 0,5 % d'un cœur : un gros
    // coffre est donc regardé moins souvent (5 000 notes : toutes les 5 s). Passer à
    // inotify et ses équivalents (crate `notify`) si ce délai devient gênant.
    fn watch(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let mut last = None;
            let mut pause = Duration::from_secs(2);
            loop {
                cx.background_executor().timer(pause).await;
                let Ok(state) = this.update(cx, |this, _| this.vault.clone().map(|root| (root, this.save_gen)))
                else {
                    return;
                };
                let Some((root, generation)) = state else {
                    continue;
                };
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
                this.update(cx, |this, cx| {
                    // Une frappe ou un enregistrement pendant la lecture : on réessaie au prochain tour.
                    if this.vault.as_ref() == Some(&root) && this.save_gen == generation && !this.dirty {
                        this.sync(notes, dirs, images, cx);
                        last = Some((root, print));
                    }
                })
                .ok();
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
        let shape = |notes: &[Note]| {
            let mut shape: Vec<_> = notes.iter().map(|n| (n.path.clone(), n.tags.clone(), n.links.clone())).collect();
            shape.sort();
            shape
        };
        dirs.sort();
        self.dirs.sort();
        images.sort();
        self.images.sort();
        if shape(&notes) != shape(&self.notes) || dirs != self.dirs || images != self.images {
            self.notes = notes;
            self.dirs = dirs;
            self.images = images;
            self.push_names(cx);
            self.graph_stale = true;
            self.refresh_graph(cx);
            cx.notify();
        }
        // La note affichée a été réécrite ailleurs : on la relit. Sans modification
        // en attente ici (l'appelant s'en assure), rien n'est perdu.
        if let Some(path) = &self.path
            && let Ok(text) = fs::read_to_string(path)
            && text != self.editor.read(cx).text()
        {
            self.reload(cx);
        }
    }

    fn set_vault(&mut self, root: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.leave(cx);
        self.vault = Some(root.clone());
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
        self.editor.update(cx, |e, cx| e.set_theme(theme, cx));
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
        let (mut options, current): (Vec<String>, &str) = match setting {
            Setting::Theme => (THEMES.iter().map(|t| t.0.to_string()).collect(), &self.prefs.theme),
            Setting::Font | Setting::Mono => (
                FAMILIES.get().into_iter().flatten().map(|f| f.to_string()).collect(),
                if setting == Setting::Font { &self.prefs.font } else { &self.prefs.mono },
            ),
        };
        // ponytail: la liste montre 14 choix sans défiler ; le choix en cours remonte
        // en tête pour rester visible. Une liste défilante si le filtre ne suffit plus.
        if let Some(i) = options.iter().position(|o| o == current) {
            let chosen = options.remove(i);
            options.insert(0, chosen);
        }
        options.insert(0, default.to_string());
        let current = if current.is_empty() { default } else { current };
        let (theme, before) = (self.theme, self.prefs.clone());
        let palette = cx.new(|cx| Palette::choose(setting.label(), options, current, theme, cx));
        cx.subscribe_in(&palette, window, move |this, _, event, window, cx| {
            match event {
                PaletteEvent::Preview(name) | PaletteEvent::Submit(name) => {
                    let value = if name == default { String::new() } else { name.clone() };
                    match setting {
                        Setting::Theme => this.prefs.theme = value,
                        Setting::Font => this.prefs.font = value,
                        Setting::Mono => this.prefs.mono = value,
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
        if open || self.vault.is_none() {
            window.focus(&self.focus);
        } else {
            window.focus(&self.editor.focus_handle(cx));
        }
        cx.notify();
    }

    fn push_names(&mut self, cx: &mut Context<Self>) {
        let names = self.notes.iter().map(|n| n.name.clone()).collect();
        self.editor.update(cx, |e, _| {
            e.set_notes(names);
            e.set_images(&self.images);
        });
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
        let mut notes: Vec<&Note> = self.notes.iter().collect();
        notes.sort_by_cached_key(|n| {
            self.recent.iter().position(|p| *p == n.path).unwrap_or(usize::MAX)
        });
        notes
    }

    /// Charge la note dans l'éditeur ; faux si le fichier est illisible.
    fn load_note(&mut self, path: &Path, cx: &mut Context<Self>) -> bool {
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
            cx.notify();
            return false;
        }
        self.picture = None;
        self.leave(cx);
        cx.notify();
        match fs::read_to_string(path) {
            Ok(text) => {
                self.synced = vault::stem(path) == vault::title_of(&text);
                self.h1 = vault::h1_of(&text);
                self.origin = Some(vault::stem(path));
                self.path = Some(path.to_path_buf());
                self.error = None;
                self.editor.update(cx, |e, cx| e.load(text, 0, cx));
                self.push_dirs(cx);
                self.nav.reveal(path);
                true
            }
            Err(e) => {
                let what = tr("Cannot open", "Impossible d'ouvrir");
                self.error = Some(format!("{what} {} : {e}", path.display()));
                false
            }
        }
    }

    fn open_note(&mut self, path: &Path, cx: &mut Context<Self>) {
        if self.load_note(path, cx) {
            self.preview = false;
            self.touch(path);
        }
    }

    /// Affiche la note sans la compter comme ouverte.
    fn preview_note(&mut self, path: &Path, cx: &mut Context<Self>) {
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
        if std::mem::take(&mut self.preview)
            && let Some(path) = self.path.clone()
        {
            self.touch(&path);
        }
    }

    /// Nouvelle note, rangée dans le dossier sélectionné quand l'arbre est affiché.
    fn new_note_here(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.vault.is_none() {
            return;
        }
        self.new_note(String::new(), cx);
        self.new_dir = self.nav.target_dir();
        if self.nav.panel == Panel::Full {
            self.nav.panel = Panel::Split;
        }
        window.focus(&self.editor.focus_handle(cx));
        self.settle_nav(window, cx);
    }

    fn new_note(&mut self, text: String, cx: &mut Context<Self>) {
        self.leave(cx);
        self.picture = None;
        self.path = None;
        self.origin = None;
        self.synced = true;
        self.preview = false;
        self.new_dir = None;
        self.dirty = !text.is_empty();
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
        self.flush(cx);
        if let (Some(old), Some(path)) = (self.origin.take(), &self.path) {
            let new = vault::stem(path);
            self.relink(&old, &new, cx);
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
        let mut open_rewritten = false;
        for note in self.notes.iter_mut().filter(|n| n.links.contains(&key)) {
            let rewritten = fs::read_to_string(&note.path)
                .ok()
                .and_then(|text| markdown::relink(&text, old, new))
                .filter(|text| vault::write(&note.path, text).is_ok());
            if let Some(text) = rewritten {
                (note.tags, note.links) = markdown::index(&text);
                open_rewritten |= Some(&note.path) == self.path.as_ref();
                self.graph_stale = true;
            }
        }
        self.refresh_graph(cx);
        open_rewritten
    }

    /// Relit depuis le disque la note affichée, réécrite en dehors de l'éditeur.
    fn reload(&mut self, cx: &mut Context<Self>) {
        if let Some(path) = self.path.clone() {
            let preview = self.preview;
            self.load_note(&path, cx);
            self.preview = preview;
        }
    }

    /// Écrit la note sur disque si elle a changé.
    // ponytail: écriture synchrone sur le thread UI (quelques Ko, < 1 ms) ;
    // passer en tâche de fond si les notes deviennent très grosses.
    fn flush(&mut self, cx: &mut Context<Self>) {
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
                self.error = None;
                let (tags, links) = markdown::index(&content);
                // Le graphe ne change que si la note est nouvelle, renommée ou liée autrement.
                let known = self.notes.iter().find(|n| Some(&n.path) == self.path.as_ref());
                if known.is_none_or(|n| n.path != path || n.links != links) {
                    self.graph_stale = true;
                }
                self.notes
                    .retain(|n| Some(&n.path) != self.path.as_ref() && n.path != path);
                self.notes.insert(
                    0,
                    Note {
                        name: vault::stem(&path),
                        tags,
                        links,
                        mtime: SystemTime::now(),
                        path: path.clone(),
                    },
                );
                if self.path.as_ref() != Some(&path) {
                    self.recent.retain(|p| Some(p) != self.path.as_ref());
                    self.touch(&path);
                    self.nav.reveal(&path);
                    self.path = Some(path);
                    self.push_names(cx);
                    self.push_dirs(cx);
                }
                self.refresh_graph(cx);
            }
            Err(e) => {
                self.error = Some(format!("{} : {e}", tr("Note not saved", "Note non enregistrée")))
            }
        }
        cx.notify();
    }

    fn on_editor_event(
        &mut self,
        _: &Entity<Editor>,
        event: &EditorEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            EditorEvent::Changed => {
                self.keep_preview();
                self.dirty = true;
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
            EditorEvent::Open(Link::Url(url)) => cx.open_url(url),
            EditorEvent::Open(Link::Tag(tag)) => self.open_palette(&format!("#{tag}"), window, cx),
            EditorEvent::Open(Link::Wiki(name)) => self.open_wiki(name, cx),
        }
    }

    /// Le schéma est enregistré à chaque changement : rien n'attend en mémoire.
    // ponytail: écriture synchrone à chaque geste ou frappe (quelques Ko) ;
    // la différer comme celle des notes si de gros schémas font attendre.
    fn on_canvas_event(&mut self, canvas: Entity<Canvas>, event: &CanvasEvent, cx: &mut Context<Self>) {
        let Some((path, _)) = self.drawing.as_ref().filter(|(_, open)| *open == canvas) else {
            return;
        };
        let result = match event {
            CanvasEvent::Changed => vault::write(path, &canvas.read(cx).diagram().to_svg(diagram::COLORS[0])),
            CanvasEvent::Export => return self.export_png(path.clone(), &canvas, cx),
        };
        self.error = result.err().map(|e| format!("{} : {e}", tr("Diagram not saved", "Schéma non enregistré")));
        cx.notify();
    }

    /// Image PNG du schéma, à côté de lui : encre sombre sur fond blanc, pour
    /// qu'elle se colle partout. Elle est dessinée hors du thread UI.
    fn export_png(&mut self, file: PathBuf, canvas: &Entity<Canvas>, cx: &mut Context<Self>) {
        let svg = canvas.read(cx).diagram().to_svg(0x1e1e1e);
        let canvas = canvas.downgrade();
        cx.spawn(async move |this, cx| {
            let drawn = cx.background_executor().spawn(async move {
                let bytes = figure::png(&svg, sans(), 2.).ok_or_else(|| std::io::Error::other("PNG"))?;
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
                    Err(e) => this.error = Some(format!("{} : {e}", tr("Picture not saved", "Image non enregistrée"))),
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
            (Err(e), _) => self.error = Some(format!("{} : {e}", tr("Import failed", "Import impossible"))),
            _ => {}
        }
        cx.notify();
    }

    /// Enregistre `diagram` dans `dir` sous un nom libre, et l'ouvre.
    fn add_diagram(&mut self, dir: &Path, name: &str, diagram: Diagram, window: &mut Window, cx: &mut Context<Self>) {
        let path = vault::free_path(dir, name, "svg");
        if let Err(e) = vault::write(&path, &diagram.to_svg(diagram::COLORS[0])) {
            self.error = Some(format!("{} : {e}", tr("Diagram not saved", "Schéma non enregistré")));
            return cx.notify();
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
        }
        self.settle_nav(window, cx);
    }

    fn open_wiki(&mut self, name: &str, cx: &mut Context<Self>) {
        let wanted = name.to_lowercase();
        match self.notes.iter().find(|n| n.name.to_lowercase() == wanted) {
            Some(note) => self.open_note(&note.path.clone(), cx),
            None => self.new_note(format!("# {name}\n\n"), cx),
        }
    }

    fn open_palette(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
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
            })
            .collect();
        let theme = self.theme;
        let palette = cx.new(|cx| Palette::new(entries, query, theme, cx));
        cx.subscribe_in(&palette, window, |this, _, event, window, cx| {
            this.palette = None;
            window.focus(&this.editor.focus_handle(cx));
            match event {
                PaletteEvent::Open(path) => this.open_note(path, cx),
                PaletteEvent::Create(name) => this.open_wiki(name, cx),
                PaletteEvent::ChangeVault => this.choose_vault(window, cx),
                PaletteEvent::Help => this.set_help(true, window, cx),
                PaletteEvent::NewDiagram => this.new_diagram(window, cx),
                PaletteEvent::ImportDiagram => this.import_diagram(window, cx),
                PaletteEvent::Setting(setting) => this.choose_setting(*setting, window, cx),
                PaletteEvent::Dismiss | PaletteEvent::Submit(_) | PaletteEvent::Preview(_) => {}
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

/// Contenu du panneau d'aide : (titre de section, [(touches, effet)]).
fn help_sections() -> Vec<(&'static str, Vec<(String, &'static str)>)> {
    let m = |key: &str| format!("{MOD}+{key}");
    let word = if cfg!(target_os = "macos") { "Alt" } else { "Ctrl" };
    vec![
        (
            "Notes",
            vec![
                (m("P"), tr("Find, create, filter by #tag", "Chercher, créer, filtrer par #tag")),
                (m("N"), tr("New note", "Nouvelle note")),
                (m("O"), tr("Change vault", "Changer de coffre")),
                (m("Shift+C"), tr("Copy the code block, else the note", "Copier le bloc de code, sinon la note")),
                (format!("{MOD}+{}", tr("click", "clic")), tr("Open a [[link]], #tag or URL", "Ouvrir un [[lien]], #tag ou URL")),
                ("F1".into(), tr("This help", "Cette aide")),
            ],
        ),
        (
            "Navigation",
            vec![
                (format!("{} / R / G / T", m("E")), tr("Vault tree / recent notes / graph / tags", "Arbre du coffre / notes récentes / graphe / tags")),
                (tr("A picture", "Une image").into(), tr("Shown in place of the note; a square in the graph", "Affichée à la place de la note ; un carré dans le graphe")),
                (m("M"), tr("Panel on the whole window", "Panneau en pleine fenêtre")),
                (tr("Arrows / Tab", "Flèches / Tab").into(), tr("Select and preview / linked notes (graph)", "Sélectionner en aperçu / notes liées (graphe)")),
                (tr("Enter / Esc", "Entrée / Échap").into(), tr("Open the note / back to the note", "Ouvrir la note / revenir à la note")),
                (m("Shift+N"), tr("New folder", "Nouveau dossier")),
                (tr("F2 / Delete", "F2 / Suppr").into(), tr("Rename / move to the trash", "Renommer / mettre à la corbeille")),
                (tr("Drag a node", "Glisser un nœud").into(), tr("Move it in the graph, linked notes follow", "Le déplacer dans le graphe, les notes liées suivent")),
            ],
        ),
        (
            tr("Diagrams", "Schémas"),
            vec![
                (m("Shift+D"), tr("New diagram: an SVG file in the vault", "Nouveau schéma : un fichier SVG du coffre")),
                ("R U O D C P N T".into(), tr("Rectangle, rounded, ellipse, diamond, cylinder, person, note, text", "Rectangle, arrondi, ellipse, losange, cylindre, personnage, note, texte")),
                ("A / L / V".into(), tr("Arrow / line, held by the shapes they join / select", "Flèche / trait, accrochés aux formes reliées / sélection")),
                (tr("Enter, double click", "Entrée, double-clic").into(), tr("Write in the shape or on the arrow; Esc when done", "Écrire dans la forme ou sur la flèche ; Échap pour finir")),
                ("---".into(), tr("Alone on a line of a box: a compartment (UML class)", "Seul sur une ligne d'une boîte : un compartiment (classe UML)")),
                (format!("{} / {} / {}", tr("Del", "Suppr"), m("D"), m("Z")), tr("Remove / duplicate / undo", "Retirer / dupliquer / annuler")),
                (format!("{} / {MOD}+{}", tr("Wheel", "Molette"), tr("wheel", "molette")), tr("Move the view / zoom", "Déplacer la vue / zoomer")),
                ("![](Schéma.svg)".into(), tr("Show the diagram in a note", "Afficher le schéma dans une note")),
                (format!("{} › import", m("P")), tr("Bring in an Excalidraw or draw.io file", "Reprendre un fichier Excalidraw ou draw.io")),
            ],
        ),
        (
            tr("Appearance", "Apparence"),
            vec![
                (format!("{} {}", m("K"), m("T")), tr("Theme, previewed as you browse", "Thème, en aperçu pendant le choix")),
                (format!("{} › {}", m("P"), tr("font", "police")), tr("Font of the app / of the code", "Police de l'app / du code")),
                (format!("{} / {} / {}", m("+"), m("-"), m("0")), tr("Bigger / smaller / default text", "Texte plus grand / plus petit / d'origine")),
            ],
        ),
        (
            tr("Typing", "À la frappe"),
            vec![
                ("- ".into(), tr("Bullet list", "Liste à puces")),
                ("1. ".into(), tr("Numbered list", "Liste numérotée")),
                ("[] ".into(), tr("Task", "Tâche à cocher")),
                ("# ## ###".into(), tr("Headings; the first # names the file", "Titres ; le premier # nomme le fichier")),
                ("> ".into(), tr("Quote", "Citation")),
                ("```rust".into(), tr("Code block: colors, copy icon", "Bloc de code : couleurs, icône de copie")),
                ("---".into(), tr("Divider", "Séparateur")),
                ("[[".into(), tr("Link to a note", "Lien vers une note")),
                ("![](image.png)".into(), tr("Picture, under its line", "Image, sous sa ligne")),
                (m("V"), tr("Paste text, or a picture", "Coller du texte, ou une image")),
                ("```mermaid".into(), tr("Diagram, under its block", "Diagramme, sous son bloc")),
                ("$x^2$  $$…$$".into(), tr("LaTeX formula, under its line", "Formule LaTeX, sous sa ligne")),
                ("-> != <= =>".into(), tr("Shown as → ≠ ≤ ⇒", "Affichés → ≠ ≤ ⇒")),
            ],
        ),
        (
            tr("Editing", "Édition"),
            vec![
                (tr("Enter", "Entrée").into(), tr("Continue the list, or leave it on an empty item", "Continuer la liste, ou en sortir sur un item vide")),
                ("Tab / Shift+Tab".into(), tr("Indent / outdent", "Indenter / désindenter")),
                (m(tr("Enter", "Entrée")), tr("Check / uncheck a task", "Cocher / décocher une tâche")),
                (format!("{} / {}", m("B"), m("I")), tr("Bold / italic", "Gras / italique")),
                (format!("{} / {}", m("Z"), m("Shift+Z")), tr("Undo / redo", "Annuler / rétablir")),
                (format!("{word}+{}", tr("Left / Right", "Gauche / Droite")), tr("Move by word", "Se déplacer par mot")),
            ],
        ),
    ]
}

impl Shell {
    fn render_help(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        let sections = help_sections().into_iter().map(|(title, rows)| {
            div()
                .w(px(370.))
                .flex()
                .flex_col()
                .gap_1()
                .child(div().mb_1().text_color(t.accent).text_size(px(12.)).child(title))
                .children(rows.into_iter().map(|(keys, effect)| {
                    div()
                        .flex()
                        .gap_3()
                        .child(div().w(px(156.)).flex_none().font_family(mono()).text_size(px(12.)).child(keys))
                        .child(div().flex_1().min_w_0().text_color(t.dim).child(effect))
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
            .px_5()
            .py_3()
            .border_t_1()
            .border_color(t.border)
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(logo(t))
            .child(format!("Bref {}", env!("CARGO_PKG_VERSION")))
            .child(div().flex_1().min_w(px(200.)).text_color(t.dim).child(tr(
                "Fast, minimal Markdown notes, in plain files you own.",
                "Des notes Markdown rapides et minimales, dans de simples fichiers qui t'appartiennent.",
            )))
            .child(link("about-github", "code.svg", "GitHub", env!("CARGO_PKG_REPOSITORY")))
            .child(link("about-kofi", "heart.svg", tr("Support on Ko-fi", "Soutenir sur Ko-fi"), KOFI));
        div()
            .absolute()
            .inset_0()
            .occlude()
            .p_4()
            .flex()
            .items_center()
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
                            .children(sections),
                    )
                    .child(about)
                    .with_animation(
                        "help-in",
                        Animation::new(Duration::from_millis(140)).with_easing(ease_out_quint()),
                        |help, delta| help.opacity(delta),
                    ),
            )
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        if self.editor.focus_handle(cx).is_focused(window) {
            self.keep_preview();
        }
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
        self.nav.logo = !client;
        self.nav.total = window.viewport_size().width - inset * 2.;

        // Bouton icône : cercle visible au survol, comme les contrôles de fenêtre de Zed.
        let icon_button = |id: &'static str, icon: &'static str| {
            div()
                .id(id)
                .size(px(20.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .cursor_pointer()
                .hover(|s| s.bg(t.border))
                .active(|s| s.bg(t.selection))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(svg().path(icon).size(px(16.)).flex_none().text_color(t.text))
        };
        let controls = window.window_controls();
        let header = div()
            .flex_none()
            .h(px(34.))
            .flex()
            .items_center()
            .text_size(px(12.5))
            .text_color(t.dim)
            .on_mouse_down(MouseButton::Left, |e, window, _| {
                if e.click_count == 2 {
                    window.zoom_window()
                } else {
                    window.start_window_move()
                }
            })
            .on_mouse_down(MouseButton::Right, |e, window, _| window.show_window_menu(e.position))
            // Le logo, discret, à l'aplomb du rail.
            .child(div().flex_none().w(nav::RAIL).flex().justify_center().child(logo(t)))
            .child(div().flex_1().pr_3().truncate().child(self.title.clone()))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .when(controls.minimize, |d| {
                        d.child(
                            icon_button("minimize", "minimize.svg")
                                .on_click(|_, window, _| window.minimize_window()),
                        )
                    })
                    .when(controls.maximize, |d| {
                        let icon = if window.is_maximized() { "restore.svg" } else { "maximize.svg" };
                        d.child(
                            icon_button("maximize", icon).on_click(|_, window, _| window.zoom_window()),
                        )
                    })
                    .child(
                        icon_button("close", "close.svg")
                            .on_click(|_, window, _| window.remove_window()),
                    ),
            );

        let body = div().flex_1().min_h_0().relative();
        let body = if self.vault.is_none() {
            body.flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_3()
                .child(
                    div()
                        .text_size(px(26.))
                        .font_weight(gpui::FontWeight::BOLD)
                        .child("Bref"),
                )
                .child(
                    div()
                        .text_color(t.dim)
                        .child(tr(
                            "Your notes live in a folder of Markdown files: the vault.",
                            "Tes notes vivent dans un dossier de fichiers Markdown : le coffre.",
                        )),
                )
                .child(
                    div()
                        .mt_4()
                        .px_4()
                        .py_2()
                        .rounded(px(8.))
                        .bg(t.accent)
                        .text_color(t.bg)
                        .cursor_pointer()
                        .hover(|s| s.opacity(0.9))
                        .child(tr("Open or create a vault", "Ouvrir ou créer un coffre"))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, window, cx| this.choose_vault(window, cx)),
                        ),
                )
                .child(div().text_size(px(12.)).text_color(t.dim).child(format!(
                    "{MOD}+O · {}",
                    tr(
                        "you can create a new folder from the file dialog",
                        "un nouveau dossier peut être créé depuis la fenêtre de sélection",
                    )
                )))
        } else {
            // Copie de toute la note : simple icône flottante, hors de la barre de titre.
            let copy = icon_button("copy-all", if self.copied { "check.svg" } else { "copy.svg" })
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
            }
            if self.drawing.as_ref().map(|(path, _)| path) != self.picture.as_ref() {
                self.drawing = None;
            }
            let note = div().flex_1().min_w_0().h_full().relative();
            let note = match (&self.picture, &self.drawing) {
                (Some(_), Some((_, canvas))) => {
                    canvas.update(cx, |canvas, _| canvas.sync(t));
                    note.child(canvas.clone())
                }
                (Some(path), None) => note.p_6().flex().items_center().justify_center().child(
                    img(path.clone()).max_w_full().max_h_full().object_fit(ObjectFit::ScaleDown),
                ),
                (None, _) => note.child(self.editor.clone()).child(copy),
            };
            body.flex()
                .child(self.render_nav(cx))
                .when(self.nav.panel != Panel::Full, |d| d.child(note))
                .children(self.palette.clone())
                .children(self.error.clone().map(|message| {
                    div()
                        .absolute()
                        .bottom_3()
                        .left_3()
                        .px_3()
                        .py_1p5()
                        .rounded(px(6.))
                        .bg(rgb(0xb42318))
                        .text_color(rgb(0xffffff))
                        .text_size(px(13.))
                        .child(message)
                }))
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
            .on_action(cx.listener(|this, _: &nav::ToggleFull, window, cx| this.toggle_full(window, cx)))
            // Séparateur du panneau : il suit le pointeur tant que le bouton est tenu.
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, window, cx| {
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
                    if this.nav.dragging {
                        this.settle_nav(window, cx)
                    }
                }),
            )
            .on_action(cx.listener(|this, _: &OpenVault, window, cx| this.choose_vault(window, cx)))
            .on_action(cx.listener(|this, _: &CopyAll, _, cx| this.copy_all(true, cx)))
            .on_action(cx.listener(|this, _: &ChooseTheme, window, cx| {
                this.choose_setting(Setting::Theme, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ZoomIn, window, cx| this.resize_text(Some(1.), window, cx)))
            .on_action(cx.listener(|this, _: &ZoomOut, window, cx| this.resize_text(Some(-1.), window, cx)))
            .on_action(cx.listener(|this, _: &ZoomReset, window, cx| this.resize_text(None, window, cx)))
            .on_action(cx.listener(|this, _: &ToggleHelp, window, cx| {
                this.set_help(!this.help, window, cx)
            }))
            .on_action(cx.listener(|this, _: &CloseHelp, window, cx| {
                if this.help {
                    this.set_help(false, window, cx)
                }
            }))
            .key_context("Shell")
            .when(client, |d| d.child(header))
            .child(body)
            .children(self.render_menu(window, cx))
            .when(self.help, |d| d.child(self.render_help(cx)));

        // La marge transparente porte l'ombre et sert de poignée de redimensionnement.
        div()
            .size_full()
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

fn bind_keys(cx: &mut App) {
    use editor::*;
    use palette::{Confirm, DeleteChar, Dismiss, Next, Prev};
    let e = Some("Editor");
    let p = Some("Palette");
    let n = Some("Nav");
    let c = Some("Canvas");
    // `secondary` = Cmd sur macOS, Ctrl ailleurs ; les mots se parcourent avec Alt sur macOS.
    let word = if cfg!(target_os = "macos") { "alt" } else { "ctrl" };
    cx.bind_keys([
        KeyBinding::new("secondary-p", OpenPalette, None),
        KeyBinding::new("secondary-n", NewNote, None),
        KeyBinding::new("secondary-o", OpenVault, None),
        KeyBinding::new("secondary-shift-c", CopyAll, None),
        KeyBinding::new("f1", ToggleHelp, None),
        KeyBinding::new("secondary-/", ToggleHelp, None),
        KeyBinding::new("escape", CloseHelp, Some("Shell")),
        KeyBinding::new("secondary-k secondary-t", ChooseTheme, None),
        KeyBinding::new("secondary-=", ZoomIn, None),
        KeyBinding::new("secondary-+", ZoomIn, None),
        KeyBinding::new("secondary--", ZoomOut, None),
        KeyBinding::new("secondary-0", ZoomReset, None),
        KeyBinding::new("secondary-q", Quit, None),
        KeyBinding::new("secondary-shift-d", NewDiagram, Some("Shell")),
        KeyBinding::new("backspace", canvas::Erase, c),
        KeyBinding::new("delete", canvas::EraseNext, c),
        KeyBinding::new("left", canvas::Left, c),
        KeyBinding::new("right", canvas::Right, c),
        KeyBinding::new("up", canvas::Up, c),
        KeyBinding::new("down", canvas::Down, c),
        KeyBinding::new("secondary-z", canvas::Undo, c),
        KeyBinding::new("secondary-shift-z", canvas::Redo, c),
        KeyBinding::new("secondary-d", canvas::Duplicate, c),
        KeyBinding::new("secondary-a", canvas::SelectAll, c),
        KeyBinding::new("secondary-v", canvas::Paste, c),
        KeyBinding::new("enter", canvas::Confirm, c),
        KeyBinding::new("escape", canvas::Cancel, c),
        KeyBinding::new("secondary-e", nav::ShowTree, Some("Shell")),
        KeyBinding::new("secondary-r", nav::ShowRecent, Some("Shell")),
        KeyBinding::new("secondary-g", nav::ShowGraph, Some("Shell")),
        KeyBinding::new("secondary-t", nav::ShowTags, Some("Shell")),
        KeyBinding::new("secondary-m", nav::ToggleFull, Some("Shell")),
        KeyBinding::new("tab", graph::Cycle, n),
        KeyBinding::new("secondary-shift-n", nav::NewFolder, Some("Shell")),
        KeyBinding::new("f2", nav::Rename, n),
        KeyBinding::new("delete", nav::Trash, n),
        KeyBinding::new("up", nav::Prev, n),
        KeyBinding::new("down", nav::Next, n),
        KeyBinding::new("left", nav::Fold, n),
        KeyBinding::new("right", nav::Unfold, n),
        KeyBinding::new("enter", nav::Open, n),
        KeyBinding::new("escape", nav::Close, n),
        KeyBinding::new("up", Prev, p),
        KeyBinding::new("down", Next, p),
        KeyBinding::new("enter", Confirm, p),
        KeyBinding::new("escape", Dismiss, p),
        KeyBinding::new("backspace", DeleteChar, p),
        KeyBinding::new("backspace", Backspace, e),
        KeyBinding::new("delete", Delete, e),
        KeyBinding::new(&format!("{word}-backspace"), DeleteWordLeft, e),
        KeyBinding::new(&format!("{word}-delete"), DeleteWordRight, e),
        KeyBinding::new("left", Left, e),
        KeyBinding::new("right", Right, e),
        KeyBinding::new("up", Up, e),
        KeyBinding::new("down", Down, e),
        KeyBinding::new(&format!("{word}-left"), WordLeft, e),
        KeyBinding::new(&format!("{word}-right"), WordRight, e),
        KeyBinding::new("home", Home, e),
        KeyBinding::new("end", End, e),
        KeyBinding::new("secondary-home", DocStart, e),
        KeyBinding::new("secondary-end", DocEnd, e),
        KeyBinding::new("pageup", PageUp, e),
        KeyBinding::new("pagedown", PageDown, e),
        KeyBinding::new("shift-left", SelectLeft, e),
        KeyBinding::new("shift-right", SelectRight, e),
        KeyBinding::new("shift-up", SelectUp, e),
        KeyBinding::new("shift-down", SelectDown, e),
        KeyBinding::new(&format!("{word}-shift-left"), SelectWordLeft, e),
        KeyBinding::new(&format!("{word}-shift-right"), SelectWordRight, e),
        KeyBinding::new("shift-home", SelectHome, e),
        KeyBinding::new("shift-end", SelectEnd, e),
        KeyBinding::new("secondary-shift-home", SelectDocStart, e),
        KeyBinding::new("secondary-shift-end", SelectDocEnd, e),
        KeyBinding::new("secondary-a", SelectAll, e),
        KeyBinding::new("enter", Newline, e),
        KeyBinding::new("tab", Indent, e),
        KeyBinding::new("shift-tab", Outdent, e),
        KeyBinding::new("secondary-enter", ToggleTask, e),
        KeyBinding::new("secondary-b", Bold, e),
        KeyBinding::new("secondary-i", Italic, e),
        KeyBinding::new("secondary-c", Copy, e),
        KeyBinding::new("secondary-x", Cut, e),
        KeyBinding::new("secondary-v", Paste, e),
        KeyBinding::new("secondary-z", Undo, e),
        KeyBinding::new("secondary-shift-z", Redo, e),
        KeyBinding::new("secondary-y", Redo, e),
        KeyBinding::new("escape", Cancel, e),
    ]);
    // Conventions macOS : Cmd+flèches pour les extrémités de ligne et de document.
    #[cfg(target_os = "macos")]
    cx.bind_keys([
        KeyBinding::new("cmd-left", Home, e),
        KeyBinding::new("cmd-right", End, e),
        KeyBinding::new("cmd-up", DocStart, e),
        KeyBinding::new("cmd-down", DocEnd, e),
        KeyBinding::new("cmd-shift-left", SelectHome, e),
        KeyBinding::new("cmd-shift-right", SelectEnd, e),
        KeyBinding::new("cmd-shift-up", SelectDocStart, e),
        KeyBinding::new("cmd-shift-down", SelectDocEnd, e),
    ]);
}

/// Le pilote Vulkan NVIDIA (paquet `nvidia-utils`) met environ 2 s à renoncer
/// quand aucune carte NVIDIA n'est branchée : on demande au chargeur de l'ignorer.
#[cfg(target_os = "linux")]
fn skip_absent_nvidia_driver() {
    let has_nvidia = fs::read_dir("/sys/bus/pci/devices").is_ok_and(|devices| {
        devices.flatten().any(|d| {
            fs::read_to_string(d.path().join("vendor")).is_ok_and(|v| v.trim() == "0x10de")
        })
    });
    if !has_nvidia && std::env::var_os("VK_LOADER_DRIVERS_DISABLE").is_none() {
        // Sûr : appelé au tout début de `main`, avant la création du moindre thread.
        unsafe { std::env::set_var("VK_LOADER_DRIVERS_DISABLE", "*nvidia*") };
    }
}

fn main() {
    #[cfg(target_os = "linux")]
    skip_absent_nvidia_driver();
    Application::new().with_assets(Assets).run(|cx: &mut App| {
        bind_keys(cx);
        init_fonts(cx);
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.on_window_closed(|cx| cx.quit()).detach();

        let (vault, recent) = vault::load_config();
        let vault = vault.filter(|v| v.is_dir());
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
mod tests {
    use super::*;
    use gpui::TestAppContext;

    /// `secondary` = Cmd sur macOS, Ctrl ailleurs, comme dans `bind_keys`.
    /// Parcours complet au clavier : saisie, listes, enregistrement, palette, wikilien.
    #[gpui::test]
    fn end_to_end(cx: &mut TestAppContext) {
        let root = std::env::temp_dir().join(format!("bref-e2e-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("Courses.md"), "# Courses\n\n- lait #maison\n").unwrap();
        fs::create_dir(root.join("Projets")).unwrap();
        fs::write(root.join("Projets/Plan.md"), "# Plan\n\n[[Courses]]\n").unwrap();
        // La config de test ne doit pas toucher celle de l'utilisateur.
        for key in ["XDG_CONFIG_HOME", "HOME", "APPDATA"] {
            unsafe { std::env::set_var(key, root.join(".config")) };
        }
        // Ce qu'une version nommée « encre » a laissé est relu tant que rien n'existe sous le nouveau nom.
        let old = vault::config_dir().join("encre");
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join("config"), format!("{}\n", root.display())).unwrap();
        fs::write(old.join("layout"), "graph split 260 0").unwrap();
        assert_eq!(vault::load_config().0, Some(root.clone()));
        assert_eq!(vault::load_layout(), "graph split 260 0");
        fs::remove_dir_all(&old).unwrap();

        cx.update(bind_keys);
        cx.update(init_fonts);
        let (shell, cx) = cx.add_window_view({
            let root = root.clone();
            |window, cx| Shell::new(Some(root), Vec::new(), window, cx)
        });
        cx.run_until_parked();
        let text = |cx: &mut gpui::VisualTestContext| {
            shell.read_with(cx, |s, cx| s.editor.read(cx).text().to_string())
        };

        cx.simulate_input("# Test");
        cx.simulate_keystrokes("enter");
        cx.simulate_input("1. un");
        cx.simulate_keystrokes("enter");
        cx.simulate_input("deux");
        cx.simulate_keystrokes("enter enter");
        cx.simulate_input("[] tâche");
        cx.simulate_keystrokes("secondary-enter enter");
        cx.simulate_input("suite");
        cx.simulate_keystrokes("tab");
        assert_eq!(
            text(cx),
            "# Test\n1. un\n2. deux\n- [x] tâche\n    - [ ] suite"
        );
        cx.simulate_keystrokes("secondary-z");
        assert_eq!(text(cx), "# Test\n1. un\n2. deux\n- [x] tâche\n- [ ] suite");

        // Enregistrement automatique, nommé d'après le titre.
        cx.executor().advance_clock(Duration::from_millis(500));
        cx.run_until_parked();
        assert_eq!(fs::read_to_string(root.join("Test.md")).unwrap(), text(cx));

        // Palette : recherche floue puis ouverture.
        cx.simulate_keystrokes("secondary-p");
        cx.simulate_input("crs");
        cx.simulate_keystrokes("enter");
        assert_eq!(text(cx), "# Courses\n\n- lait #maison\n");

        // Récents : la dernière note ouverte d'abord, dans l'app comme dans la config.
        let recent = shell.read_with(cx, |s, _| s.recent.clone());
        assert_eq!(recent, [root.join("Courses.md"), root.join("Test.md")]);
        assert_eq!(vault::load_config(), (Some(root.clone()), recent));
        let names = shell.read_with(cx, |s, _| {
            s.by_recency().iter().map(|n| n.name.clone()).collect::<Vec<_>>()
        });
        assert_eq!(names, ["Courses", "Test", "Plan"]);

        // Wikilien complété puis nouvelle note créée depuis la palette.
        cx.simulate_keystrokes("secondary-end");
        cx.simulate_input("[[te");
        cx.simulate_keystrokes("enter");
        assert!(text(cx).ends_with("[[Test]]"));
        cx.simulate_keystrokes("secondary-p");
        cx.simulate_input("Idées");
        cx.simulate_keystrokes("enter");
        assert_eq!(text(cx), "# Idées\n\n");
        assert!(root.join("Idées.md").is_file());
        assert!(fs::read_to_string(root.join("Courses.md")).unwrap().ends_with("[[Test]]"));


        // Copie rapide de toute la note.
        cx.simulate_keystrokes("secondary-shift-c");
        let clipboard = cx.read_from_clipboard().and_then(|item| item.text());
        assert_eq!(clipboard.as_deref(), Some("# Idées\n\n"));


        // Panneau d'aide : F1 l'ouvre, Échap le ferme et rend la main à l'éditeur.
        cx.simulate_keystrokes("f1");
        assert!(shell.read_with(cx, |s, _| s.help));
        cx.simulate_keystrokes("escape");
        assert!(!shell.read_with(cx, |s, _| s.help));
        cx.simulate_input("ok");
        assert_eq!(text(cx), "# Idées\n\nok");

        // Thème : Ctrl+K Ctrl+T liste les thèmes, celui qu'on parcourt s'applique en
        // aperçu ; Échap revient au précédent, Entrée garde le choix et le mémorise.
        let look = |cx: &mut gpui::VisualTestContext| {
            shell.read_with(cx, |s, _| (s.prefs.theme.clone(), s.theme.bg))
        };
        let system = look(cx);
        cx.simulate_keystrokes("secondary-k secondary-t");
        cx.simulate_input("drac");
        assert_eq!(look(cx), ("Dracula".to_string(), Hsla::from(rgb(0x282a36))));
        cx.simulate_keystrokes("escape");
        assert_eq!(look(cx), system);
        cx.simulate_keystrokes("secondary-k secondary-t down down enter");
        assert_eq!(look(cx), ("Bref Light".to_string(), Hsla::from(rgb(0xfbfaf8))));
        // Police du code, depuis la palette ; taille du texte au clavier.
        cx.simulate_keystrokes("secondary-p");
        cx.simulate_input("police du");
        cx.simulate_keystrokes("down enter");
        cx.simulate_input("systemui");
        cx.simulate_keystrokes("enter");
        assert_eq!(mono(), ".SystemUIFont");
        cx.simulate_keystrokes("secondary-= secondary-= secondary--");
        assert_eq!(shell.read_with(cx, |s, _| s.theme.size), 17.);
        assert_eq!(vault::load_settings(), "theme=Bref Light\nfont=\nmono=.SystemUIFont\nsize=17\n");
        cx.simulate_keystrokes("secondary-0");
        assert_eq!(shell.read_with(cx, |s, _| s.theme.size), 16.);
        cx.simulate_input("!");
        assert_eq!(text(cx), "# Idées\n\nok!");

        // Arbre : Ctrl+E l'ouvre et lui donne le focus ; les flèches affichent un aperçu,
        // qui ne compte pas comme une ouverture.
        cx.executor().advance_clock(Duration::from_millis(500));
        cx.simulate_keystrokes("secondary-e up");
        assert_eq!(shell.read_with(cx, |s, _| (s.nav.mode, s.nav.panel)), (Mode::Tree, Panel::Split));
        assert_eq!(text(cx), "# Courses\n\n- lait #maison\n[[Test]]");
        assert!(shell.read_with(cx, |s, _| s.preview && s.recent[0] == root.join("Idées.md")));
        // Le dossier se déplie avec Droite ; Entrée ouvre la note et rend la main à l'éditeur.
        cx.simulate_keystrokes("up right down");
        // Le bouton de repli replie tout l'arbre, puis le déplie tout entier.
        let open = |s: &mut Shell| {
            s.fold_all();
            s.nav.open.len()
        };
        assert_eq!(shell.update(cx, |s, _| (open(s), open(s))), (0, 1));
        assert_eq!(text(cx), "# Plan\n\n[[Courses]]\n");
        cx.simulate_keystrokes("enter");
        assert!(shell.read_with(cx, |s, _| !s.preview && s.recent[0] == root.join("Projets/Plan.md")));
        assert!(cx.update(|window, cx| shell.read(cx).editor.focus_handle(cx).is_focused(window)));

        // Nouvelle note depuis l'arbre : rangée dans le dossier de la sélection.
        cx.simulate_keystrokes("secondary-n");
        cx.simulate_input("# Sous");
        cx.executor().advance_clock(Duration::from_millis(500));
        cx.run_until_parked();
        assert!(root.join("Projets/Sous.md").is_file());

        // Séparateur : il règle la largeur, et replie le panneau en bout de course.
        let drag = |cx: &mut gpui::VisualTestContext, from: f32, to: f32| {
            let at = |x: f32| point(px(x), px(300.));
            cx.simulate_mouse_down(at(from), MouseButton::Left, gpui::Modifiers::none());
            cx.simulate_mouse_move(at(to), MouseButton::Left, gpui::Modifiers::none());
            cx.simulate_mouse_up(at(to), MouseButton::Left, gpui::Modifiers::none());
        };
        drag(cx, 302., 450.);
        assert_eq!(vault::load_layout(), "tree split 410 0\n");
        drag(cx, 452., 60.);
        assert_eq!(shell.read_with(cx, |s, _| s.nav.panel), Panel::Rail);

        // Récents, plein écran (Ctrl+M), puis Échap rend la place et le focus à la note.
        cx.simulate_keystrokes("secondary-r secondary-m");
        assert_eq!(shell.read_with(cx, |s, _| (s.nav.mode, s.nav.panel)), (Mode::Recent, Panel::Full));
        cx.simulate_keystrokes("down escape");
        assert_eq!(shell.read_with(cx, |s, _| s.nav.panel), Panel::Split);
        assert_eq!(text(cx), "# Sous");
        // Redemander le mode affiché : d'abord le focus, puis le repli sur le rail.
        cx.simulate_keystrokes("secondary-r secondary-r");
        assert_eq!(vault::load_layout(), "recent rail 410 0\n");

        // Graphe : une arête par wikilien résolu. Tab parcourt les notes liées à la
        // note ouverte, en aperçu ; Entrée ouvre celle qui est sélectionnée.
        cx.simulate_keystrokes("secondary-p");
        cx.simulate_input("cou");
        cx.simulate_keystrokes("enter secondary-g");
        cx.run_until_parked();
        assert_eq!(shell.read_with(cx, |s, _| (s.nav.mode, s.nav.panel)), (Mode::Graph, Panel::Split));
        assert_eq!(shell.read_with(cx, |s, cx| s.graph.read(cx).size()), (5, 2));
        // Un nœud tiré à la souris suit le pointeur.
        let spot = |cx: &mut gpui::VisualTestContext| {
            shell.read_with(cx, |s, cx| s.graph.read(cx).spot(&root.join("Courses.md")))
        };
        let from = spot(cx);
        let to = from + point(px(40.), px(30.));
        cx.simulate_mouse_down(from, MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_move(to, MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_up(to, MouseButton::Left, gpui::Modifiers::none());
        assert!((spot(cx).x - to.x).abs() < px(1.) && (spot(cx).y - to.y).abs() < px(1.));
        let mut linked = Vec::new();
        for _ in 0..2 {
            cx.simulate_keystrokes("tab");
            linked.push(shell.read_with(cx, |s, _| vault::stem(s.path.as_ref().unwrap())));
            assert!(shell.read_with(cx, |s, _| s.preview && s.recent[0] == root.join("Courses.md")));
        }
        linked.sort();
        assert_eq!(linked, ["Plan", "Test"]);
        cx.simulate_keystrokes("enter");
        assert!(shell.read_with(cx, |s, _| !s.preview && s.recent[0] == *s.path.as_ref().unwrap()));
        assert!(cx.update(|window, cx| shell.read(cx).editor.focus_handle(cx).is_focused(window)));
        // Plein écran : la sélection ne déplace plus la note ; Entrée lui rend sa place.
        cx.simulate_keystrokes("secondary-g secondary-m tab");
        let open = shell.read_with(cx, |s, _| s.path.clone());
        assert!(shell.read_with(cx, |s, _| s.nav.panel == Panel::Full && s.nav.sel != s.path));
        cx.simulate_keystrokes("enter");
        assert!(shell.read_with(cx, |s, _| s.nav.panel == Panel::Split && s.path != open));

        // Fichiers. Ctrl+Maj+N affiche l'arbre et crée un dossier à côté de la sélection.
        cx.simulate_keystrokes("secondary-p");
        cx.simulate_input("cou");
        cx.simulate_keystrokes("enter secondary-shift-n");
        cx.simulate_input("Archives: 2026");
        cx.simulate_keystrokes("enter");
        let archives = root.join("Archives 2026");
        assert!(archives.is_dir());
        assert_eq!(shell.read_with(cx, |s, _| (s.nav.mode, s.nav.sel.clone())), (Mode::Tree, Some(archives.clone())));

        // F2 renomme : le titre d'une note accordée à son nom suit, la liste des récents aussi.
        shell.update(cx, |s, _| s.nav.sel = Some(root.join("Test.md")));
        cx.simulate_keystrokes("f2");
        cx.simulate_input("s");
        cx.simulate_keystrokes("enter");
        let renamed = fs::read_to_string(root.join("Tests.md")).unwrap();
        assert!(renamed.starts_with("# Tests\n1. un") && !root.join("Test.md").exists());
        assert!(shell.read_with(cx, |s, _| s.recent.contains(&root.join("Tests.md"))));
        // Les liens suivent, dans le fichier comme dans la note affichée.
        assert_eq!(text(cx), "# Courses\n\n- lait #maison\n[[Tests]]");
        assert!(fs::read_to_string(root.join("Courses.md")).unwrap().ends_with("[[Tests]]"));

        // Changer le titre renomme aussi la note : les liens ne suivent qu'une fois la
        // note quittée, pas à chaque titre intermédiaire.
        cx.simulate_keystrokes("secondary-p");
        cx.simulate_input("tests");
        cx.simulate_keystrokes("enter end");
        for letter in ["X", "Y"] {
            cx.simulate_input(letter);
            cx.executor().advance_clock(Duration::from_millis(500));
            cx.run_until_parked();
        }
        assert!(root.join("TestsXY.md").is_file() && !root.join("TestsX.md").exists());
        assert!(fs::read_to_string(root.join("Courses.md")).unwrap().ends_with("[[Tests]]"));
        cx.simulate_keystrokes("secondary-p");
        cx.simulate_input("cou");
        cx.simulate_keystrokes("enter secondary-e");
        assert_eq!(text(cx), "# Courses\n\n- lait #maison\n[[TestsXY]]");

        // Glisser-déposer : une note, puis un dossier entier, rangés dans un autre dossier.
        shell.update(cx, |s, cx| {
            s.move_into(&root.join("TestsXY.md"), &archives, cx);
            s.move_into(&root.join("Projets"), &archives, cx);
            // Un dossier ne se range pas dans lui-même.
            s.move_into(&archives, &archives.join("Projets"), cx);
        });
        assert!(archives.join("TestsXY.md").is_file() && archives.join("Projets/Plan.md").is_file());
        assert!(shell.read_with(cx, |s, _| {
            s.dirs.contains(&archives.join("Projets"))
                && s.recent.contains(&archives.join("Projets/Plan.md"))
                && s.notes.iter().all(|n| n.path.is_file())
        }));

        // Suppr met à la corbeille du coffre, sans rien détruire ; la note ouverte laisse
        // place à une note vide.
        shell.update(cx, |s, _| s.nav.sel = Some(archives.clone()));
        cx.simulate_keystrokes("delete");
        assert!(root.join(".trash/Archives 2026/Projets/Plan.md").is_file() && !archives.exists());
        assert_eq!(text(cx), "# Courses\n\n- lait #maison\n[[TestsXY]]");
        shell.update(cx, |s, _| s.nav.sel = Some(root.join("Courses.md")));
        cx.simulate_keystrokes("delete");
        assert!(root.join(".trash/Courses.md").is_file());
        assert_eq!(text(cx), "");
        let left = shell.read_with(cx, |s, _| {
            assert!(s.dirs.is_empty() && s.path.is_none() && s.recent.iter().all(|p| p.is_file()));
            s.notes.iter().map(|n| n.name.clone()).collect::<Vec<_>>()
        });
        assert_eq!(left, ["Idées"]);

        // Ce qu'un autre programme change dans le coffre apparaît sans rien faire : une
        // note ajoutée, puis la note affichée réécrite ailleurs.
        fs::write(root.join("Ailleurs.md"), "# Ailleurs\n").unwrap();
        fs::write(root.join("Idées.md"), "# Idées\n\nréécrite\n").unwrap();
        shell.update(cx, |s, cx| s.open_note(&root.join("Idées.md"), cx));
        fs::write(root.join("Idées.md"), "# Idées\n\nréécrite ailleurs\n").unwrap();
        cx.executor().advance_clock(Duration::from_secs(3));
        cx.run_until_parked();
        assert!(shell.read_with(cx, |s, _| s.notes.iter().any(|n| n.name == "Ailleurs")));
        assert_eq!(text(cx), "# Idées\n\nréécrite ailleurs\n");

        // Un titre `#` ajouté en tête d'une note qui n'en avait pas : le fichier prend
        // son nom. Sans titre touché, une note au nom libre garde le sien.
        fs::write(root.join("brouillon.md"), "texte\n").unwrap();
        shell.update(cx, |s, cx| s.open_note(&root.join("brouillon.md"), cx));
        cx.update(|window, cx| window.focus(&shell.read(cx).editor.focus_handle(cx)));
        cx.simulate_keystrokes("secondary-end");
        cx.simulate_input("suite");
        cx.executor().advance_clock(Duration::from_millis(500));
        cx.run_until_parked();
        assert_eq!(fs::read_to_string(root.join("brouillon.md")).unwrap(), "texte\nsuite");
        cx.simulate_keystrokes("secondary-home");
        cx.simulate_input("# Titre");
        cx.simulate_keystrokes("enter");
        cx.executor().advance_clock(Duration::from_millis(500));
        cx.run_until_parked();
        assert_eq!(fs::read_to_string(root.join("Titre.md")).unwrap(), "# Titre\ntexte\nsuite");
        assert!(!root.join("brouillon.md").exists());

        // Bloc de code : Entrée pose la clôture ; curseur dedans, la copie rapide ne
        // prend que le bloc, et le bouton « Copier » de son ouverture fait de même.
        cx.simulate_keystrokes("secondary-end enter");
        cx.simulate_input("```rust");
        cx.simulate_keystrokes("enter");
        cx.simulate_input("let x = 1;");
        assert!(text(cx).ends_with("```rust\nlet x = 1;\n```"));
        let clipboard = |cx: &mut gpui::VisualTestContext| cx.read_from_clipboard().and_then(|item| item.text());
        cx.simulate_keystrokes("secondary-shift-c");
        assert_eq!(clipboard(cx).as_deref(), Some("let x = 1;"));
        cx.simulate_keystrokes("secondary-home secondary-shift-c");
        assert!(clipboard(cx).unwrap().starts_with("# Titre"));
        let button = shell.read_with(cx, |s, cx| s.editor.read(cx).copy_button().unwrap());
        cx.simulate_click(button, gpui::Modifiers::none());
        assert_eq!(clipboard(cx).as_deref(), Some("let x = 1;"));

        // Image : la ligne qui la désigne porte l'image, trouvée à côté de la note.
        // Les signes (`->`) ne s'affichent que hors de la ligne du curseur, sans toucher au texte.
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect width="40" height="20"/></svg>"#;
        fs::write(root.join("carré.svg"), svg).unwrap();
        cx.simulate_keystrokes("secondary-end enter");
        cx.simulate_input("a -> b ![](carré.svg)");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        cx.simulate_input("fin");
        cx.run_until_parked();
        let (signs, image) = shell.read_with(cx, |s, cx| s.editor.read(cx).decorations("a -> b"));
        assert_eq!((signs, image), (1, Some((40., 20.))));
        assert!(text(cx).ends_with("a -> b ![](carré.svg)\nfin"));

        // Coller une image l'enregistre à côté de la note, qui la désigne.
        let image = gpui::Image::from_bytes(gpui::ImageFormat::Svg, svg.as_bytes().to_vec());
        cx.write_to_clipboard(ClipboardItem::new_image(&image));
        cx.simulate_keystrokes("enter secondary-v");
        let note = text(cx);
        let pasted = note.rsplit_once("![](").unwrap().1.trim_end_matches(')');
        assert!(pasted.starts_with("image-") && pasted.ends_with(".svg"));
        assert_eq!(fs::read_to_string(root.join(pasted)).unwrap(), svg);

        // Diagramme Mermaid, dessiné sous son bloc une fois le curseur sorti, et
        // formule LaTeX, sous sa ligne.
        cx.simulate_keystrokes("enter");
        cx.simulate_input("```mermaid");
        cx.simulate_keystrokes("enter");
        cx.simulate_input("flowchart LR");
        cx.simulate_keystrokes("enter");
        cx.simulate_input("A --> B");
        cx.simulate_keystrokes("secondary-end enter");
        cx.simulate_input("soit $x^2$");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        cx.simulate_input("fin");
        cx.run_until_parked();
        let figures = shell.read_with(cx, |s, cx| {
            let editor = s.editor.read(cx);
            ["![](image-", "```\nsoit", "soit $x^2$"].map(|line| editor.decorations(line).1.is_some())
        });
        assert_eq!(figures, [true, true, true]);

        // Les images du coffre sont suivies comme les notes. Celles qu'une note affiche
        // ont leur nœud dans le graphe ; en choisir une l'affiche à la place de la note.
        for _ in 0..2 {
            cx.executor().advance_clock(Duration::from_secs(3));
            cx.run_until_parked();
        }
        cx.simulate_keystrokes("secondary-g");
        cx.run_until_parked();
        let (images, notes, nodes) = shell.read_with(cx, |s, cx| {
            (s.images.len(), s.notes.len(), s.graph.read(cx).size().0)
        });
        assert_eq!((images, nodes), (2, notes + 2));
        // Une image qu'aucune note n'affiche a aussi son nœud, sans lien.
        fs::write(root.join("seule.svg"), svg).unwrap();
        for _ in 0..2 {
            cx.executor().advance_clock(Duration::from_secs(3));
            cx.run_until_parked();
        }
        assert_eq!(shell.read_with(cx, |s, cx| s.graph.read(cx).size().0), notes + 3);
        shell.update(cx, |s, cx| s.preview_note(&root.join("carré.svg"), cx));
        assert_eq!(shell.read_with(cx, |s, _| s.picture.clone()), Some(root.join("carré.svg")));
        assert!(text(cx).ends_with("fin"));
        // Revenir d'une image à la note qu'elle cachait se fait en une sélection, comme
        // d'une note à une autre.
        shell.update(cx, |s, cx| {
            let note = s.path.clone().unwrap();
            s.preview_note(&note, cx);
            assert_eq!(s.picture, None);
            s.preview_note(&root.join("carré.svg"), cx);
        });
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert_eq!(shell.read_with(cx, |s, _| s.picture.clone()), None);

        // Tags : Ctrl+T liste les tags du coffre ; Droite déplie le premier, Bas
        // affiche en aperçu une note qui le porte.
        fs::write(root.join("Tagué.md"), "# Tagué\n\n#alpha\n").unwrap();
        for _ in 0..2 {
            cx.executor().advance_clock(Duration::from_secs(3));
            cx.run_until_parked();
        }
        cx.simulate_keystrokes("secondary-t down right down");
        cx.run_until_parked();
        assert_eq!(shell.read_with(cx, |s, _| s.nav.mode), Mode::Tags);
        assert!(vault::load_layout().starts_with("tags split"));
        let (tagged, previewed) = shell.read_with(cx, |s, _| {
            let tag = s.notes.iter().flat_map(|n| &n.tags).min().unwrap().clone();
            let has = |n: &&Note| n.tags.contains(&tag);
            (s.notes.iter().filter(has).count(), s.notes.iter().filter(has).any(|n| Some(&n.path) == s.path.as_ref()))
        });
        assert!(tagged > 0 && previewed);
        // Un tag n'est pas un fichier : Suppr n'y fait rien.
        cx.simulate_keystrokes("up delete");
        assert!(shell.read_with(cx, |s, _| s.palette.is_none() && s.error.is_none()));

        // Schéma : Ctrl+Maj+D en crée un à côté de la note. Une lettre choisit la
        // forme, glisser la pose ; Entrée écrit dedans ; une flèche tirée d'une
        // forme à l'autre s'y accroche.
        cx.simulate_keystrokes("secondary-shift-d");
        cx.run_until_parked();
        let (file, canvas) = shell.read_with(cx, |s, _| s.drawing.clone().unwrap());
        assert_eq!(file.file_name().unwrap(), "Schéma.svg");
        let drag = |cx: &mut gpui::VisualTestContext, from: (f32, f32), to: (f32, f32)| {
            let (from, to) = canvas.read_with(cx, |c, _| (c.spot(from.0, from.1), c.spot(to.0, to.1)));
            cx.simulate_mouse_down(from, MouseButton::Left, gpui::Modifiers::none());
            cx.simulate_mouse_move(to, MouseButton::Left, gpui::Modifiers::none());
            cx.simulate_mouse_up(to, MouseButton::Left, gpui::Modifiers::none());
        };
        let saved = || Diagram::from_svg(&fs::read_to_string(&file).unwrap()).unwrap();
        cx.simulate_input("r");
        drag(cx, (0., 0.), (120., 60.));
        cx.simulate_keystrokes("enter");
        cx.simulate_input("Client");
        cx.simulate_keystrokes("escape");
        cx.simulate_input("o");
        drag(cx, (300., 0.), (400., 80.));
        cx.simulate_input("a");
        drag(cx, (60., 30.), (350., 40.));
        let drawn = saved();
        assert_eq!((drawn.shapes.len(), drawn.shapes[0].text.as_str(), drawn.shapes[0].w), (2, "Client", 120.));
        let ends = (diagram::End::Shape(drawn.shapes[0].id), diagram::End::Shape(drawn.shapes[1].id));
        assert_eq!((drawn.links[0].from, drawn.links[0].to, drawn.links[0].end), (ends.0, ends.1, diagram::Head::Arrow));
        // Une forme se déplace en la tirant, sur la grille ; Ctrl+Z la remet en place.
        drag(cx, (350., 40.), (350., 143.));
        assert_eq!(saved().shapes[1].y, 100.);
        cx.simulate_keystrokes("secondary-z");
        assert_eq!(saved().shapes[1].y, 0.);
        // Suppr retire la forme sélectionnée, et la flèche qui y tenait.
        drag(cx, (350., 40.), (350., 40.));
        cx.simulate_keystrokes("delete");
        assert_eq!((saved().shapes.len(), saved().links.len()), (1, 0));
        // Le schéma figure parmi les images du coffre ; Échap rend la main à la note.
        assert!(shell.read_with(cx, |s, _| s.images.contains(&file)));
        cx.simulate_keystrokes("escape escape");
        cx.run_until_parked();
        assert!(shell.read_with(cx, |s, _| s.picture.is_none() && s.drawing.is_none()));

        // Import : un fichier Excalidraw devient un schéma du coffre, sous son nom.
        // Son image PNG s'enregistre à côté de lui.
        let outside = root.join(".config/croquis.excalidraw");
        fs::write(&outside, r#"{"elements":[{"id":"a","type":"ellipse","x":0,"y":0,"width":80,"height":60}]}"#).unwrap();
        shell.update_in(cx, |s, window, cx| s.import_file(&outside, window, cx));
        cx.run_until_parked();
        let (file, canvas) = shell.read_with(cx, |s, _| s.drawing.clone().unwrap());
        assert_eq!((file.file_name().unwrap().to_str(), canvas.read_with(cx, |c, _| c.diagram().shapes.len())), (Some("croquis.svg"), 1));
        shell.update(cx, |s, cx| s.export_png(file.clone(), &canvas, cx));
        cx.run_until_parked();
        assert!(fs::read(file.with_extension("png")).unwrap().starts_with(b"\x89PNG"));
        assert!(shell.read_with(cx, |s, _| s.images.contains(&file.with_extension("png")) && s.error.is_none()));

        fs::remove_dir_all(&root).unwrap();
    }

    /// Temps par geste sur un gros coffre et une grosse note. Hors de la suite
    /// courante : `cargo test --release --locked -- --ignored --nocapture bench`.
    #[gpui::test]
    #[ignore]
    fn bench(cx: &mut TestAppContext) {
        let root = std::env::temp_dir().join(format!("bref-bench-{}", std::process::id()));
        for i in 0..2000 {
            let dir = root.join(format!("d{}", i % 40));
            fs::create_dir_all(&dir).unwrap();
            let body: String = (0..30).map(|j| format!("Ligne {j} vers [[Note {}]] #tag{}\n", (i + j) % 2000, j % 12)).collect();
            fs::write(dir.join(format!("Note {i}.md")), format!("# Note {i}\n\n{body}")).unwrap();
        }
        let big: String =
            (0..5000).map(|j| format!("Ligne {j} avec **gras**, `code`, [[Note {}]] et #tag{} -> fin.\n", j % 2000, j % 12)).collect();
        fs::write(root.join("Grosse.md"), format!("# Grosse\n\n{big}")).unwrap();
        for key in ["XDG_CONFIG_HOME", "HOME", "APPDATA"] {
            unsafe { std::env::set_var(key, root.join(".config")) };
        }
        vault::save_layout("tree split 260 0");

        let start = Instant::now();
        let scanned = vault::scan(&root);
        println!("scan du coffre : {:?}", start.elapsed());
        let start = Instant::now();
        let again = vault::rescan(&root, &scanned.0);
        println!("nouveau scan, rien n'a changé : {:?}", start.elapsed());
        assert_eq!(again.0.len(), scanned.0.len());

        cx.update(bind_keys);
        cx.update(init_fonts);
        let (shell, cx) = cx.add_window_view({
            let root = root.clone();
            |window, cx| Shell::new(Some(root), Vec::new(), window, cx)
        });
        cx.run_until_parked();
        shell.update(cx, |s, cx| s.open_note(&root.join("Grosse.md"), cx));
        cx.run_until_parked();
        let time = |what: &str, cx: &mut gpui::VisualTestContext, act: &dyn Fn(&mut gpui::VisualTestContext)| {
            let start = Instant::now();
            for _ in 0..20 {
                act(cx);
                cx.run_until_parked();
            }
            println!("{what} : {:?} par geste", start.elapsed() / 20);
        };
        time("frappe", cx, &|cx| cx.simulate_input("a"));
        time("flèche bas", cx, &|cx| cx.simulate_keystrokes("down"));
        time("molette", cx, &|cx| {
            cx.simulate_event(gpui::ScrollWheelEvent {
                position: point(px(700.), px(300.)),
                delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-40.))),
                ..Default::default()
            })
        });
        time("mot puis annulation", cx, &|cx| {
            cx.simulate_input("mot ");
            cx.simulate_keystrokes("secondary-z");
        });
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn reads_settings() {
        let prefs = Prefs::parse("theme=Dracula\nfont=Inter\nsize=99\nbogus\nmono=\n");
        assert_eq!((prefs.theme.as_str(), prefs.font.as_str(), prefs.mono.as_str()), ("Dracula", "Inter", ""));
        // Taille bornée, et valeur illisible ignorée.
        assert_eq!((prefs.size, Prefs::parse("size=NaN").size), (32., 16.));
        assert_eq!(Prefs::parse(&prefs.to_text()), prefs);
    }
}
