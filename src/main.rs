// Pas de console derrière la fenêtre sous Windows.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod canvas;
mod diagram;
mod editor;
mod figure;
mod graph;
mod grid;
mod import;
mod markdown;
mod nav;
mod palette;
mod sheet;
mod table;
mod update;
mod vault;

use std::{
    borrow::Cow,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock, RwLock},
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
use sheet::{Pick, Sheet, SheetEvent};
use table::{Encoding, Format, Style};
use vault::Note;

/// Touche des raccourcis, telle qu'affichée : Cmd sur macOS, Ctrl ailleurs.
pub const MOD: &str = if cfg!(target_os = "macos") { "Cmd" } else { "Ctrl" };

/// Texte d'interface : français si la langue du système l'est, anglais sinon.
pub fn tr(en: &'static str, fr: &'static str) -> &'static str {
    static FRENCH: OnceLock<bool> = OnceLock::new();
    // Les tests tournent en français pour rester déterministes.
    if *FRENCH.get_or_init(|| cfg!(test) || system_is_french()) { fr } else { en }
}

/// Les variables de locale d'abord (lancement depuis un terminal), sinon le système :
/// une application lancée depuis le Finder ou le menu Démarrer n'en reçoit aucune.
fn system_is_french() -> bool {
    match ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|key| std::env::var(key).ok().filter(|v| !v.is_empty()))
    {
        Some(lang) => lang.starts_with("fr"),
        None => os_language_is_french(),
    }
}

#[cfg(windows)]
fn os_language_is_french() -> bool {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetUserDefaultUILanguage() -> u16;
    }
    // Un LANGID : les 10 bits de poids faible donnent la langue (0x0c : français).
    (unsafe { GetUserDefaultUILanguage() } & 0x3ff) == 0x0c
}

#[cfg(target_os = "macos")]
fn os_language_is_french() -> bool {
    std::process::Command::new("defaults")
        .args(["read", "-g", "AppleLanguages"])
        .output()
        .ok()
        .and_then(|out| first_language(&String::from_utf8_lossy(&out.stdout)).map(|l| l.starts_with("fr")))
        .unwrap_or(false)
}

// ponytail: sous Linux, seules les variables d'environnement comptent (le système
// n'en offre pas d'autre) ; sous macOS la langue vient d'un appel à `defaults` au démarrage.
#[cfg(not(any(windows, target_os = "macos")))]
fn os_language_is_french() -> bool {
    false
}

/// Première langue de la liste que `defaults read -g AppleLanguages` imprime :
/// `(\n    "fr-FR",\n    "en-US"\n)`, avec ou sans guillemets.
#[cfg(any(target_os = "macos", test))]
fn first_language(list: &str) -> Option<&str> {
    list.lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && *l != "(" && *l != ")")
        .map(|l| l.trim_end_matches(',').trim_matches('"'))
}

/// Date du jour à l'heure locale : année, mois, jour. La bibliothèque standard ne donne que l'UTC.
#[cfg(unix)]
pub fn today() -> (i32, u32, u32) {
    // SAFETY: `time` accepte un pointeur nul ; `localtime_r` n'écrit que dans `tm`, qui est à nous.
    let tm = unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&now, &mut tm);
        tm
    };
    (tm.tm_year + 1900, tm.tm_mon as u32 + 1, tm.tm_mday as u32)
}

#[cfg(windows)]
pub fn today() -> (i32, u32, u32) {
    /// `SYSTEMTIME` de l'API Windows : huit mots de 16 bits.
    #[repr(C)]
    #[derive(Default)]
    #[allow(dead_code)]
    struct Local {
        year: u16,
        month: u16,
        day_of_week: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        millis: u16,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetLocalTime(time: *mut Local);
    }
    let mut now = Local::default();
    // SAFETY: `GetLocalTime` remplit la structure qu'on lui donne, et n'échoue pas.
    unsafe { GetLocalTime(&mut now) };
    (now.year as i32, now.month as u32, now.day as u32)
}

/// `2026-10-09` : la date telle qu'elle s'écrit dans une note et nomme la note du jour.
pub fn date_name((year, month, day): (i32, u32, u32)) -> String {
    format!("{year:04}-{month:02}-{day:02}")
}

/// Polices installées, relevées au démarrage.
static FAMILIES: OnceLock<Vec<&'static str>> = OnceLock::new();

/// Polices (texte, code) en usage : celles choisies, sinon celles par défaut.
static FONTS: RwLock<(&str, &str)> = RwLock::new(("IBM Plex Sans", "IBM Plex Mono"));

/// IBM Plex Sans et Plex Mono (licence SIL OFL, `assets/fonts/`), dans le binaire : l'app
/// a ses polices sans rien installer. Des polices statiques, car gpui 0.2 ne met pas en
/// gras une police variable.
pub const FONT_FILES: [&[u8]; 8] = [
    include_bytes!("../assets/fonts/IBMPlexSans-Regular.ttf"),
    include_bytes!("../assets/fonts/IBMPlexSans-Italic.ttf"),
    include_bytes!("../assets/fonts/IBMPlexSans-Bold.ttf"),
    include_bytes!("../assets/fonts/IBMPlexSans-BoldItalic.ttf"),
    include_bytes!("../assets/fonts/IBMPlexMono-Regular.ttf"),
    include_bytes!("../assets/fonts/IBMPlexMono-Italic.ttf"),
    include_bytes!("../assets/fonts/IBMPlexMono-Bold.ttf"),
    include_bytes!("../assets/fonts/IBMPlexMono-BoldItalic.ttf"),
];

pub fn sans() -> &'static str {
    FONTS.read().unwrap().0
}

pub fn mono() -> &'static str {
    FONTS.read().unwrap().1
}

fn init_fonts(cx: &mut App) {
    cx.text_system().add_fonts(FONT_FILES.iter().map(|f| Cow::Borrowed(*f)).collect()).ok();
    FAMILIES.get_or_init(|| {
        let names: &'static [String] = cx.text_system().all_font_names().leak();
        names.iter().map(String::as_str).collect()
    });
}

/// Met en usage les polices des réglages (n'importe quelle police installée) ; à
/// défaut, celles de l'app.
fn apply_fonts(prefs: &Prefs) {
    let installed = FAMILIES.get().map_or(&[][..], Vec::as_slice);
    let pick = |chosen: &str, wanted: &[&'static str]| {
        let known = |name: &str| installed.iter().copied().find(|i| *i == name);
        known(chosen)
            .or_else(|| wanted.iter().find_map(|w| known(w)))
            .unwrap_or(wanted[0])
    };
    *FONTS.write().unwrap() = (pick(&prefs.font, &["IBM Plex Sans"]), pick(&prefs.mono, &["IBM Plex Mono"]));
}

/// Apparence choisie par l'utilisateur ; un champ vide garde la valeur par défaut.
#[derive(Clone, Debug, PartialEq)]
struct Prefs {
    theme: String,
    font: String,
    mono: String,
    /// Taille du texte courant de la note, en pixels.
    size: f32,
    /// Chercher une nouvelle version de l'app (une requête par jour).
    updates: bool,
}

impl Prefs {
    const SIZE: f32 = 16.;

    fn parse(text: &str) -> Self {
        let mut prefs =
            Self { theme: String::new(), font: String::new(), mono: String::new(), size: Self::SIZE, updates: true };
        for (key, value) in text.lines().filter_map(|line| line.split_once('=')) {
            match key {
                "theme" => prefs.theme = value.into(),
                "font" => prefs.font = value.into(),
                "mono" => prefs.mono = value.into(),
                "size" => prefs.size = value.parse().ok().filter(|s: &f32| s.is_finite()).unwrap_or(Self::SIZE),
                "updates" => prefs.updates = value != "off",
                _ => {}
            }
        }
        prefs.size = prefs.size.clamp(11., 32.);
        prefs
    }

    fn to_text(&self) -> String {
        let mut text = format!("theme={}\nfont={}\nmono={}\nsize={}\n", self.theme, self.font, self.mono, self.size);
        // Absent tant qu'on ne l'a pas coupé : la valeur par défaut n'encombre pas le fichier.
        if !self.updates {
            text.push_str("updates=off\n");
        }
        text
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
            "trash.svg" => r#"<path d="M3 4.5H13M6.5 4.5V3H9.5V4.5M4.5 4.5L5 13H11L11.5 4.5M6.8 7V10.5M9.2 7V10.5"/>"#,
            "archive.svg" => r#"<path d="M2.5 3H13.5V6H2.5ZM3.5 6V13H12.5V6M6.5 8.5H9.5"/>"#,
            "backlink.svg" => r#"<path d="M13 4.5H8.5A3 3 0 0 0 5.5 7.5V11.5M3 9L5.5 11.5L8 9"/>"#,
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
            "d-route.svg" => r#"<path d="M2.5 12.5H8V3.5H13.5"/>"#,
            "d-fill.svg" => r#"<rect x="3" y="3" width="10" height="10" rx="1.5" fill="black" fill-opacity="0.35"/>"#,
            "d-dash.svg" => r#"<path d="M2.5 8H5M7 8H9M11 8H13.5"/>"#,
            "d-head-start.svg" => r#"<path d="M13.5 8H3M6.5 4.5L3 8L6.5 11.5"/>"#,
            "d-head-end.svg" => r#"<path d="M2.5 8H13M9.5 4.5L13 8L9.5 11.5"/>"#,
            "export.svg" => r#"<path d="M8 2.5V10M5 7L8 10L11 7M3 13H13"/>"#,
            "target.svg" => r#"<circle cx="8" cy="8" r="2"/><path d="M8 2.5V5M8 11V13.5M2.5 8H5M11 8H13.5"/>"#,
            "file-plus.svg" => {
                r#"<path d="M4 2.5H9L12.5 6V12.5A1 1 0 0 1 11.5 13.5H4.5A1 1 0 0 1 3.5 12.5V3.5A1 1 0 0 1 4.5 2.5Z"/><path d="M8 7.7V11M6.4 9.35H9.6"/>"#
            }
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

actions!(app, [SearchVault, Today, Outline, OpenPalette, NewNote, NewDiagram, OpenVault, CopyAll, ToggleHelp, CloseHelp, ChooseTheme, ZoomIn, ZoomOut, ZoomReset, Quit]);

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
    /// Fichiers du coffre qui ne sont pas des notes : images et tableaux (CSV, TSV).
    images: Vec<PathBuf>,
    /// Image affichée à la place de la note, choisie dans l'arbre ou le graphe.
    picture: Option<PathBuf>,
    /// Schéma ouvert dans son canevas, quand l'image affichée en est un.
    drawing: Option<(PathBuf, Entity<Canvas>)>,
    /// Tableau ouvert dans sa grille, quand le fichier affiché en est un.
    sheet: Option<(PathBuf, Entity<Sheet>)>,
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
    /// Version plus récente que celle qui tourne, trouvée sur GitHub.
    update: Option<update::Release>,
    /// Son installation est en cours.
    updating: bool,
    /// Réponse à un contrôle demandé à la main (« Bref est à jour »…), à la place de la bannière.
    notice: Option<String>,
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
            sheet: None,
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
            update: None,
            updating: false,
            notice: None,
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
        this.check_updates(cx);
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

    /// Contrôle demandé depuis la palette : interroge GitHub tout de suite, même si la recherche
    /// quotidienne est coupée, et dit ce qu'il en est. Les tests ne sortent pas sur le réseau.
    fn check_now(&mut self, cx: &mut Context<Self>) {
        self.notice = Some(tr("Checking for updates…", "Recherche d'une mise à jour…").into());
        cx.notify();
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
        self.notice = match latest {
            None => Some(tr("No answer from GitHub: check the connection", "GitHub ne répond pas : vérifier la connexion").into()),
            Some(release) if update::is_new(&release) => {
                self.update = Some(release);
                None
            }
            Some(_) => Some(format!("{} ({})", tr("Bref is up to date", "Bref est à jour"), env!("CARGO_PKG_VERSION"))),
        };
        cx.notify();
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
        self.error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { update::install(&url, sha256.as_deref()) }).await;
            this.update(cx, |this, cx| match result {
                // La sortie enregistre la note en cours (`on_app_quit`).
                Ok(()) => cx.quit(),
                Err(e) => {
                    this.updating = false;
                    this.error = Some(format!("{} : {e}", tr("Update failed", "Mise à jour impossible")));
                    cx.notify();
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
                    if this.vault.as_ref() == Some(&root) && this.save_gen == generation && !this.dirty {
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
        } else {
            // Ni nom, ni tag, ni lien n'ont changé, mais le texte peut avoir : la recherche le suit.
            self.notes = notes;
        }
        // Le tableau affiché a été réécrit ailleurs : il est relu s'il n'attend rien ici.
        if let Some((_, sheet)) = &self.sheet {
            sheet.update(cx, |sheet, cx| sheet.reload_if_changed(cx));
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
        // Le choix en cours remonte en tête, juste sous « Par défaut ».
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

    /// Plan de la note, comme celui de Zed : la liste de ses titres, celui de la section en
    /// cours présélectionné. Le titre parcouru se montre en aperçu, Entrée y laisse le curseur,
    /// Échap le remet où il était.
    pub fn open_outline(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.vault.is_none() || self.picture.is_some() {
            return;
        }
        let editor = self.editor.read(cx);
        let before = editor.position();
        let heads: Vec<(String, usize)> = markdown::headings(editor.text())
            .into_iter()
            .map(|(level, title, at)| (format!("{}{title}", "  ".repeat(level as usize - 1)), at))
            .collect();
        if heads.is_empty() {
            self.palette = None;
            self.notice = Some(tr("This note has no headings", "Cette note n'a pas de titres").into());
            window.focus(&self.editor.focus_handle(cx));
            return cx.notify();
        }
        let current = heads.iter().rposition(|(_, at)| *at <= before).unwrap_or(0);
        let (theme, options) = (self.theme, heads.iter().map(|(label, _)| label.clone()).collect());
        let palette = cx.new(|cx| Palette::choose(tr("Outline", "Plan"), options, "", theme, cx).select(current));
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
        if open || self.vault.is_none() {
            window.focus(&self.focus);
        } else {
            window.focus(&self.editor.focus_handle(cx));
        }
        cx.notify();
    }

    fn push_names(&mut self, cx: &mut Context<Self>) {
        // Les alias se proposent après `[[` comme des noms.
        let names = self.notes.iter().map(|n| n.name.clone()).chain(self.notes.iter().flat_map(|n| n.aliases.clone())).collect();
        self.editor.update(cx, |e, _| {
            e.set_notes(names);
            e.set_images(&vault::pictures(&self.images));
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

    /// Un tableau prend la place de la note, comme une image ; la note reste chargée dessous.
    fn open_table(&mut self, path: &Path, cx: &mut Context<Self>) -> bool {
        self.picture = Some(path.to_path_buf());
        if self.sheet.as_ref().is_some_and(|(open, _)| open == path) {
            cx.notify();
            return false;
        }
        self.flush_sheet(cx);
        self.sheet = None;
        match sheet::load(path, None) {
            Ok(loaded) => {
                let theme = self.theme;
                let sheet = cx.new(|cx| Sheet::new(path.to_path_buf(), loaded, theme, cx));
                cx.subscribe(&sheet, Self::on_sheet_event).detach();
                self.sheet = Some((path.to_path_buf(), sheet));
                self.error = None;
            }
            Err(e) => {
                self.picture = None;
                self.error = Some(e);
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
            SheetEvent::Error(message) => self.error = Some(message.clone()),
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
                        aliases: markdown::aliases(&content),
                        mtime: SystemTime::now(),
                        path: path.clone(),
                        body: Arc::from(content),
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
        self.notice = Some(tr("Backing up the vault…", "Sauvegarde du coffre…").into());
        cx.notify();
        let date = date_name(today());
        cx.spawn(async move |this, cx| {
            let done = cx.background_executor().spawn(async move { vault::backup(&root, &dir, &date) }).await;
            this.update(cx, |this, cx| {
                this.notice = None;
                match done {
                    Ok(to) => this.notice = Some(format!("{} {}", tr("Backup saved:", "Sauvegarde enregistrée :"), to.display())),
                    Err(e) => this.error = Some(format!("{} : {e}", tr("Backup failed", "Sauvegarde impossible"))),
                }
                cx.notify();
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
            self.notice = Some(tr("The trash is empty", "La corbeille est vide").into());
            return cx.notify();
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
                    this.notice = Some(format!("{} {name}", tr("Restored:", "Restauré :")));
                }
                Some(Err(e)) => this.error = Some(format!("{} : {e}", tr("Not restored", "Restauration impossible"))),
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
        if self.vault.is_some() {
            self.open_wiki(&date_name(today()), cx);
        }
    }

    fn open_wiki(&mut self, name: &str, cx: &mut Context<Self>) {
        let wanted = name.to_lowercase();
        // Par son nom d'abord, sinon par un alias de son en-tête YAML.
        let named = self.notes.iter().find(|n| n.name.to_lowercase() == wanted);
        match named.or_else(|| self.notes.iter().find(|n| n.answers(&wanted))) {
            Some(note) => self.open_note(&note.path.clone(), cx),
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
                // ponytail: mis en minuscules à chaque ouverture (5 000 notes : environ 50 ms) ;
                // à garder dans `Note` si cela se remarque.
                lower: n.body.to_lowercase(),
            })
            .collect();
        let theme = self.theme;
        let updates = self.prefs.updates;
        let installable = self.update.as_ref().filter(|r| r.asset.is_some()).map(|r| r.version.clone());
        let table = self.shown_sheet().is_some();
        let palette = cx.new(|cx| match lines {
            true => Palette::search(entries, theme, cx),
            false => Palette::new(entries, query, theme, cx).with_updates(updates, installable).with_table(table),
        });
        cx.subscribe_in(&palette, window, |this, _, event, window, cx| {
            this.palette = None;
            window.focus(&this.editor.focus_handle(cx));
            match event {
                PaletteEvent::Open(path) => this.open_note(path, cx),
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
                PaletteEvent::Today => this.open_today(cx),
                PaletteEvent::Trash => this.open_trash(window, cx),
                PaletteEvent::Backup => this.choose_backup(window, cx),
                PaletteEvent::InstallUpdate => this.install_update(cx),
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
                (m("P"), tr("Find by name or text, create, filter by #tag", "Chercher par nom ou texte, créer, filtrer par #tag")),
                (format!("{} / {}", m("F"), m("H")), tr("Find in the note / find and replace", "Chercher dans la note / chercher et remplacer")),
                (m("Shift+F"), tr("Search the text of the whole vault, line by line", "Chercher dans le texte de tout le coffre, ligne par ligne")),
                (tr("Enter / Shift+Enter", "Entrée / Maj+Entrée").into(), tr("Search bar: next / previous match (F3 too)", "Barre de recherche : passage suivant / précédent (F3 aussi)")),
                ("Alt+C / Alt+W / Tab".into(), tr("Search bar: match case / whole words / replace field", "Barre de recherche : casse / mots entiers / champ de remplacement")),
                (m(tr("Enter", "Entrée")), tr("Search bar: replace all (Enter: this match)", "Barre de recherche : tout remplacer (Entrée : ce passage)")),
                (m("Shift+O"), tr("Outline: jump to a heading of the note", "Plan : aller à un titre de la note")),
                (m("N"), tr("New note", "Nouvelle note")),
                (m("J"), tr("Today's note: open it, or create it", "Note du jour : l'ouvrir, ou la créer")),
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
                (m("L"), tr("Backlinks: the notes that link to this one", "Rétroliens : les notes qui mènent à celle-ci")),
                (tr("A picture", "Une image").into(), tr("Shown in place of the note; a square in the graph", "Affichée à la place de la note ; un carré dans le graphe")),
                (tr("A .csv or .tsv", "Un .csv ou .tsv").into(), tr("A grid in place of the note; not in the graph", "Une grille à la place de la note ; absent du graphe")),
                (m("M"), tr("Panel on the whole window", "Panneau en pleine fenêtre")),
                (tr("Arrows / Tab", "Flèches / Tab").into(), tr("Select and preview / linked notes (graph)", "Sélectionner en aperçu / notes liées (graphe)")),
                (tr("Enter / Esc", "Entrée / Échap").into(), tr("Open the note / back to the note", "Ouvrir la note / revenir à la note")),
                (m("Shift+N"), tr("New folder (also in the palette)", "Nouveau dossier (aussi dans la palette)")),
                ("Alt + drag".into(), tr("Move the window from anywhere (Linux)", "Déplacer la fenêtre depuis n'importe où (Linux)")),
                (format!("F2 / {} / {}", m("D"), tr("Delete", "Suppr")), tr("Rename / duplicate / move to the trash", "Renommer / dupliquer / mettre à la corbeille")),
                (format!("{} / Shift+{}", m(tr("click", "clic")), tr("click", "clic")), tr("Select several rows: move, duplicate, trash them together", "Sélectionner plusieurs lignes : les déplacer, dupliquer, jeter ensemble")),
                (tr("Right click", "Clic droit").into(), tr("Copy the link or the path, reveal in the file explorer…", "Copier le lien ou le chemin, afficher dans l'explorateur…")),
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
                (format!("Shift+{0} / {1}", tr("click", "clic"), m(tr("click", "clic"))), tr("Add a shape to the selection, or take it out", "Ajouter une forme à la sélection, ou l'en retirer")),
                (format!("{} / {} / {}", tr("Del", "Suppr"), m("D"), m("Z")), tr("Remove / duplicate / undo", "Retirer / dupliquer / annuler")),
                (tr("Edge of a shape", "Bord d'une forme").into(), tr("An arrow end dropped there stays there; in the middle, it follows the other end", "Un bout de flèche lâché là y reste ; au milieu, il suit l'autre bout")),
                (tr("Wheel / + - 0", "Molette / + - 0").into(), tr("Move the view / zoom in, out, fit all", "Déplacer la vue / zoomer, dézoomer, tout cadrer")),
                ("![](Schéma.svg)".into(), tr("Show the diagram in a note", "Afficher le schéma dans une note")),
                (format!("{} › import", m("P")), tr("Bring in an Excalidraw or draw.io file", "Reprendre un fichier Excalidraw ou draw.io")),
            ],
        ),
        (
            tr("CSV tables", "Tableaux CSV"),
            vec![
                (".csv .tsv".into(), tr("Opens in a grid; encoding and delimiter are detected, written straight into the file", "S'ouvre dans une grille ; encodage et délimiteur sont devinés, écrit directement dans le fichier")),
                (tr("Arrows, Tab", "Flèches, Tab").into(), tr("Move; with Shift (or drag), select a range; click a row number or a header", "Se déplacer ; avec Maj (ou en glissant), sélectionner une plage ; clic sur un numéro ou un en-tête")),
                (m(tr("click", "clic")), tr("Add another selection: copy, cut, empty or remove their rows together", "Ajouter une autre sélection : les copier, couper, vider ou retirer leurs lignes ensemble")),
                (tr("Type, Enter, F2", "Taper, Entrée, F2").into(), tr("Replace the cell / validate and go down / open the cell; Esc cancels", "Remplacer la cellule / valider et descendre / ouvrir la cellule ; Échap annule")),
                (format!("{} / {} / {}", m("C"), m("X"), m("V")), tr("Copy as tab-separated text / cut / paste cells from a spreadsheet, Markdown or CSV", "Copier en texte à tabulations / couper / coller des cellules d'un tableur, de Markdown ou de CSV")),
                (format!("{} / {}", m("Enter"), m("Delete")), tr("Insert a row (with Shift: above) / remove the selected rows", "Insérer une ligne (avec Maj : au-dessus) / retirer les lignes sélectionnées")),
                (format!("{} › table", m("P")), tr("Delimiter, encoding, header row, copy as…, export (CSV, TSV, Markdown, JSON)", "Délimiteur, encodage, en-tête, copier en…, exporter (CSV, TSV, Markdown, JSON)")),
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
                ("/".into(), tr("Components: headings, lists, panel, table, code, formula…", "Composants : titres, listes, panneau, tableau, code, formule…")),
                ("/table".into(), tr("Table: pick its size on the grid (mouse, arrows), or type it: 12x5", "Tableau : choisir sa taille sur la grille (souris, flèches), ou la taper : 12x5")),
                ("> [!NOTE]".into(), tr("Colored panel: NOTE, TIP, IMPORTANT, WARNING, CAUTION", "Panneau coloré : NOTE, TIP, IMPORTANT, WARNING, CAUTION")),
                ("- ".into(), tr("Bullet list", "Liste à puces")),
                ("1. ".into(), tr("Numbered list", "Liste numérotée")),
                ("[] ".into(), tr("Task", "Tâche à cocher")),
                ("# ## ###".into(), tr("Headings; the first # names the file", "Titres ; le premier # nomme le fichier")),
                ("> ".into(), tr("Quote", "Citation")),
                ("```rust".into(), tr("Code block: colors, copy icon", "Bloc de code : couleurs, icône de copie")),
                ("---".into(), tr("Divider", "Séparateur")),
                ("[[".into(), tr("Link to a note", "Lien vers une note")),
                ("![](image.png)".into(), tr("Picture, under its line", "Image, sous sa ligne")),
                ("![[".into(), tr("Suggests the pictures and diagrams of the vault", "Propose les images et les schémas du coffre")),
                (m("V"), tr("Paste text, or a picture", "Coller du texte, ou une image")),
                (tr("Double / triple click", "Double / triple clic").into(), tr("Select a word / a line; drag to extend by words / lines", "Sélectionner un mot / une ligne ; glisser étend par mots / par lignes")),
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
                (tr("Tab / Enter in a table", "Tab / Entrée dans un tableau").into(), tr("Next cell / new row; columns stay aligned; + buttons add a row or a column", "Cellule suivante / nouvelle ligne ; les colonnes restent alignées ; les boutons + ajoutent ligne ou colonne")),
                (m("Shift+L / E / R"), tr("Align the column of the cursor: left, centered, right", "Aligner la colonne du curseur : à gauche, centrée, à droite")),
                (tr("Pasting a table", "Coller un tableau").into(), tr("Spreadsheet cells or Markdown fill the grid, or become a table", "Des cellules de tableur ou du Markdown remplissent la grille, ou deviennent un tableau")),
                (m(tr("Enter", "Entrée")), tr("Check / uncheck a task or a [ ] box; in a table cell, add one", "Cocher / décocher une tâche ou une case [ ] ; dans une cellule, en ajouter une")),
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
                d.child(icon_button("minimize", "minimize.svg").on_click(|_, window, _| window.minimize_window()))
            })
            .when(controls.maximize, |d| {
                let icon = if window.is_maximized() { "restore.svg" } else { "maximize.svg" };
                d.child(icon_button("maximize", icon).on_click(|_, window, _| window.zoom_window()))
            })
            .child(icon_button("close", "close.svg").on_click(|_, window, _| window.remove_window()));

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
            if self.sheet.as_ref().map(|(path, _)| path) != self.picture.as_ref() {
                self.flush_sheet(cx);
                self.sheet = None;
            }
            let note = match (&self.picture, &self.drawing) {
                (Some(_), _) if self.sheet.is_some() => {
                    let (_, sheet) = self.sheet.clone().unwrap();
                    sheet.update(cx, |sheet, _| sheet.sync(t, client));
                    note.child(sheet)
                }
                (Some(_), Some((_, canvas))) => {
                    canvas.update(cx, |canvas, _| canvas.sync(t));
                    note.child(canvas.clone())
                }
                (Some(path), None) => note.p_6().flex().items_center().justify_center().child(
                    img(path.clone()).max_w_full().max_h_full().object_fit(ObjectFit::ScaleDown),
                ),
                (None, _) => note.child(self.editor.clone()).child(copy),
            };
            let banner = || {
                div()
                    .absolute()
                    .bottom_3()
                    .left_16()
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
            body.flex()
                .child(self.render_nav(cx))
                .when(self.nav.panel != Panel::Full, |d| d.child(note))
                .children(self.palette.clone())
                // Réponse à un contrôle demandé à la main ; un clic la referme.
                .children(self.notice.clone().filter(|_| self.update.is_none()).map(|message| {
                    banner().pr_3().cursor_pointer().child(message).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.notice = None;
                            cx.notify();
                        }),
                    )
                }))
                .children(self.update.clone().map(|release| {
                    let banner = banner();
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
                                .text_color(t.dim)
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
                }))
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
            .on_action(cx.listener(|this, _: &nav::ShowLinks, window, cx| {
                this.show_nav(Mode::Links, false, window, cx)
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
            .on_action(cx.listener(|this, _: &sheet::PickDelimiter, window, cx| this.choose_table(Pick::Delimiter, window, cx)))
            .on_action(cx.listener(|this, _: &sheet::PickEncoding, window, cx| this.choose_table(Pick::Encoding, window, cx)))
            .on_action(cx.listener(|this, _: &sheet::PickHeader, window, cx| this.choose_table(Pick::Header, window, cx)))
            .on_action(cx.listener(|this, _: &sheet::PickCopyAs, window, cx| this.choose_table(Pick::CopyAs, window, cx)))
            .on_action(cx.listener(|this, _: &sheet::PickExport, window, cx| this.choose_table(Pick::Export, window, cx)))
            .on_action(cx.listener(|this, _: &OpenVault, window, cx| this.choose_vault(window, cx)))
            .on_action(cx.listener(|this, _: &CopyAll, _, cx| this.copy_all(true, cx)))
            .on_action(cx.listener(|this, _: &Outline, window, cx| this.open_outline(window, cx)))
            .on_action(cx.listener(|this, _: &Today, _, cx| this.open_today(cx)))
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
            .on_action(cx.listener(|this, _: &CloseHelp, window, cx| {
                if this.help {
                    this.set_help(false, window, cx)
                }
            }))
            .key_context("Shell")
            .child(body)
            .when(client, |d| d.child(pill))
            .children(self.render_menu(window, cx))
            .when(self.help, |d| d.child(self.render_help(cx)));

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

/// Fait d'un élément une poignée de la fenêtre, là où l'app dessine sa barre de titre
/// (Linux) : glisser la déplace, double-clic l'agrandit, clic droit ouvre le menu du système.
pub fn drag_window<E: InteractiveElement>(element: E) -> E {
    element
        .on_mouse_down(MouseButton::Left, |e, window, _| {
            if e.click_count == 2 {
                window.zoom_window()
            } else {
                window.start_window_move()
            }
        })
        .on_mouse_down(MouseButton::Right, |e, window, _| window.show_window_menu(e.position))
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
    let f = Some("Find");
    let n = Some("Nav");
    let c = Some("Canvas");
    let sh = Some("Sheet");
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
        KeyBinding::new("secondary-shift-o", Outline, None),
        KeyBinding::new("secondary-j", Today, None),
        KeyBinding::new("secondary-shift-f", SearchVault, None),
        KeyBinding::new("secondary-=", ZoomIn, None),
        KeyBinding::new("secondary-+", ZoomIn, None),
        KeyBinding::new("secondary--", ZoomOut, None),
        KeyBinding::new("secondary-0", ZoomReset, None),
        KeyBinding::new("secondary-q", Quit, None),
        KeyBinding::new("secondary-shift-d", NewDiagram, Some("Shell")),
        KeyBinding::new("secondary-=", canvas::ZoomIn, c),
        KeyBinding::new("secondary-+", canvas::ZoomIn, c),
        KeyBinding::new("secondary--", canvas::ZoomOut, c),
        KeyBinding::new("secondary-0", canvas::ZoomFit, c),
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
        // Tableau (CSV, TSV) : flèches, sélection avec Maj, saisie, presse-papiers, annuler.
        KeyBinding::new("left", sheet::Left, sh),
        KeyBinding::new("right", sheet::Right, sh),
        KeyBinding::new("up", sheet::Up, sh),
        KeyBinding::new("down", sheet::Down, sh),
        KeyBinding::new("shift-left", sheet::ExtendLeft, sh),
        KeyBinding::new("shift-right", sheet::ExtendRight, sh),
        KeyBinding::new("shift-up", sheet::ExtendUp, sh),
        KeyBinding::new("shift-down", sheet::ExtendDown, sh),
        KeyBinding::new("pageup", sheet::PageUp, sh),
        KeyBinding::new("pagedown", sheet::PageDown, sh),
        KeyBinding::new("home", sheet::RowStart, sh),
        KeyBinding::new("end", sheet::RowEnd, sh),
        KeyBinding::new("secondary-home", sheet::Origin, sh),
        KeyBinding::new("secondary-end", sheet::Corner, sh),
        KeyBinding::new("secondary-up", sheet::FirstRow, sh),
        KeyBinding::new("secondary-down", sheet::LastRow, sh),
        KeyBinding::new("tab", sheet::Next, sh),
        KeyBinding::new("shift-tab", sheet::Previous, sh),
        KeyBinding::new("enter", sheet::Enter, sh),
        KeyBinding::new("f2", sheet::Edit, sh),
        KeyBinding::new("escape", sheet::Cancel, sh),
        KeyBinding::new("backspace", sheet::Backspace, sh),
        KeyBinding::new("delete", sheet::Delete, sh),
        KeyBinding::new("secondary-c", sheet::Copy, sh),
        KeyBinding::new("secondary-x", sheet::Cut, sh),
        KeyBinding::new("secondary-v", sheet::Paste, sh),
        KeyBinding::new("secondary-a", sheet::SelectAll, sh),
        KeyBinding::new("secondary-z", sheet::Undo, sh),
        KeyBinding::new("secondary-shift-z", sheet::Redo, sh),
        KeyBinding::new("secondary-y", sheet::Redo, sh),
        KeyBinding::new("secondary-enter", sheet::InsertBelow, sh),
        KeyBinding::new("secondary-shift-enter", sheet::InsertAbove, sh),
        KeyBinding::new("secondary-delete", sheet::DeleteRows, sh),
        KeyBinding::new("secondary-e", nav::ShowTree, Some("Shell")),
        KeyBinding::new("secondary-r", nav::ShowRecent, Some("Shell")),
        KeyBinding::new("secondary-g", nav::ShowGraph, Some("Shell")),
        KeyBinding::new("secondary-t", nav::ShowTags, Some("Shell")),
        KeyBinding::new("secondary-l", nav::ShowLinks, Some("Shell")),
        KeyBinding::new("secondary-m", nav::ToggleFull, Some("Shell")),
        KeyBinding::new("tab", graph::Cycle, n),
        KeyBinding::new("secondary-shift-n", nav::NewFolder, Some("Shell")),
        KeyBinding::new("f2", nav::Rename, n),
        KeyBinding::new("secondary-d", nav::Duplicate, n),
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
        // Recherche dans la note : la barre a son propre contexte tant qu'elle reçoit la saisie.
        KeyBinding::new("secondary-f", Find, e),
        KeyBinding::new("secondary-f", Find, f),
        KeyBinding::new("secondary-h", FindReplace, e),
        KeyBinding::new("secondary-h", FindReplace, f),
        KeyBinding::new("f3", FindNext, e),
        KeyBinding::new("shift-f3", FindPrev, e),
        KeyBinding::new("f3", FindNext, f),
        KeyBinding::new("shift-f3", FindPrev, f),
        KeyBinding::new("enter", FindEnter, f),
        KeyBinding::new("shift-enter", FindPrev, f),
        KeyBinding::new("secondary-enter", ReplaceAll, f),
        KeyBinding::new("escape", FindClose, f),
        KeyBinding::new("backspace", FindErase, f),
        KeyBinding::new("tab", FindSwitch, f),
        // Aussi quand la barre est ouverte mais que la saisie est revenue à la note.
        KeyBinding::new("alt-c", FindCase, f),
        KeyBinding::new("alt-w", FindWord, f),
        KeyBinding::new("alt-c", FindCase, e),
        KeyBinding::new("alt-w", FindWord, e),
        KeyBinding::new("secondary-v", FindPaste, f),
        KeyBinding::new("enter", Newline, e),
        KeyBinding::new("tab", Indent, e),
        KeyBinding::new("shift-tab", Outdent, e),
        KeyBinding::new("secondary-enter", ToggleTask, e),
        KeyBinding::new("secondary-shift-l", AlignLeft, e),
        KeyBinding::new("secondary-shift-e", AlignCenter, e),
        KeyBinding::new("secondary-shift-r", AlignRight, e),
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

    static CONFIG: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Le dossier de config est global au processus (variables d'environnement) : un test qui le déplace sous
    /// `root` garde le verrou jusqu'à sa fin, pour qu'un autre ne le déplace pas pendant qu'il s'en sert.
    fn isolated_config(root: &Path) -> std::sync::MutexGuard<'static, ()> {
        let guard = CONFIG.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        for key in ["XDG_CONFIG_HOME", "HOME", "APPDATA"] {
            unsafe { std::env::set_var(key, root.join(".config")) };
        }
        guard
    }

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
        let _config = isolated_config(&root);
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
        // Bas de page : le compteur suit la frappe, puis ne compte que la sélection.
        let counts = |cx: &mut gpui::VisualTestContext| shell.update(cx, |s, cx| s.editor.update(cx, |e, _| e.counts()));
        assert_eq!(counts(cx), "1 mot · 6 caractères");
        cx.simulate_keystrokes("shift-left shift-left");
        assert_eq!(counts(cx), "Sélection : 1 mot · 2 caractères");
        cx.simulate_keystrokes("right");
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

        // Palette : le texte des notes se cherche aussi (« deux » n'est dans aucun nom),
        // et tous les mots de la requête doivent s'y trouver.
        cx.simulate_keystrokes("secondary-p");
        cx.simulate_input("deux");
        cx.simulate_keystrokes("enter");
        assert!(text(cx).starts_with("# Test\n1. un\n2. deux"));
        cx.simulate_keystrokes("secondary-p");
        cx.simulate_input("lait maison");
        cx.simulate_keystrokes("enter");
        assert_eq!(text(cx), "# Courses\n\n- lait #maison\n");

        // Mises à jour : la palette coupe la recherche (et la bannière), puis la rétablit.
        shell.update(cx, |s, _| s.update = Some(update::Release { version: "9.9.9".into(), ..Default::default() }));
        cx.simulate_keystrokes("secondary-p");
        cx.simulate_input("mises à jour");
        cx.simulate_keystrokes("down enter");
        assert!(shell.read_with(cx, |s, _| !s.prefs.updates && s.update.is_none()));
        assert!(vault::load_settings().ends_with("updates=off\n"));
        cx.simulate_keystrokes("secondary-p");
        cx.simulate_input("mises à jour");
        cx.simulate_keystrokes("down enter");
        assert!(shell.read_with(cx, |s, _| s.prefs.updates));
        assert!(!vault::load_settings().contains("updates"));

        // Installer la nouvelle version : la palette le propose quand l'app sait le faire seule.
        // Les tests n'installent rien : l'échec laisse l'app en place, avec son message.
        let release = update::Release { version: "9.9.9".into(), asset: Some("file:///absent".into()), ..Default::default() };
        shell.update(cx, |s, _| s.update = Some(release));
        cx.simulate_keystrokes("secondary-p");
        cx.simulate_input("mettre à jour");
        cx.simulate_keystrokes("down enter");
        cx.run_until_parked();
        assert!(shell.read_with(cx, |s, _| !s.updating && s.error.as_deref().is_some_and(|e| e.contains("Mise à jour impossible"))));
        shell.update(cx, |s, _| (s.update, s.error) = (None, None));

        // Contrôle à la main : la palette le lance, puis la réponse dit que Bref est à jour,
        // que GitHub ne répond pas, ou montre la bannière.
        cx.simulate_keystrokes("secondary-p");
        cx.simulate_input("maintenant");
        cx.simulate_keystrokes("down enter");
        assert!(shell.read_with(cx, |s, _| s.notice.as_deref().is_some_and(|n| n.contains("Recherche"))));
        let version = |v: &str| Some(update::Release { version: v.into(), ..Default::default() });
        shell.update(cx, |s, cx| s.checked(version(env!("CARGO_PKG_VERSION")), cx));
        assert!(shell.read_with(cx, |s, _| s.update.is_none() && s.notice.as_deref().is_some_and(|n| n.contains("à jour"))));
        shell.update(cx, |s, cx| s.checked(None, cx));
        assert!(shell.read_with(cx, |s, _| s.notice.as_deref().is_some_and(|n| n.contains("GitHub"))));
        shell.update(cx, |s, cx| s.checked(version("9.9.9"), cx));
        assert!(shell.read_with(cx, |s, _| s.notice.is_none() && s.update.as_ref().is_some_and(|r| r.version == "9.9.9")));
        shell.update(cx, |s, _| s.update = None);

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

        // Sélection multiple : Ctrl+clic ajoute une ligne, Maj+clic étend depuis la
        // ligne choisie ; dupliquer, copier les chemins et jeter portent sur le lot.
        cx.run_until_parked();
        let mark = |cx: &mut gpui::VisualTestContext, from: &str, to: &str, extend: bool| {
            shell.update(cx, |s, cx| {
                let row = |s: &Shell, name: &str| s.nav.rows.iter().position(|r| r.path == root.join(name)).unwrap();
                s.nav.marked.clear();
                s.nav.sel = Some(root.join(from));
                s.nav_mark(row(s, to), extend, cx);
            });
        };
        mark(cx, "Courses.md", "Idées.md", false);
        cx.simulate_keystrokes("secondary-d");
        assert!(root.join("Courses 2.md").is_file() && root.join("Idées 2.md").is_file());
        assert_eq!(fs::read_to_string(root.join("Courses 2.md")).unwrap(), text(cx));
        cx.run_until_parked();
        mark(cx, "Courses.md", "Idées.md", true);
        assert_eq!(shell.read_with(cx, |s, _| s.nav.marked.len()), 3);
        mark(cx, "Courses 2.md", "Idées 2.md", false);
        shell.update_in(cx, |s, window, cx| s.menu_do(nav::Do::CopyRelative, s.nav.sel.clone(), window, cx));
        assert_eq!(cx.read_from_clipboard().and_then(|item| item.text()).as_deref(), Some("Courses 2.md\nIdées 2.md"));
        cx.simulate_keystrokes("delete");
        assert!(root.join(".trash/Courses 2.md").is_file() && root.join(".trash/Idées 2.md").is_file());
        assert!(root.join("Courses.md").is_file() && shell.read_with(cx, |s, _| s.nav.marked.is_empty()));

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

        // Rétroliens : Ctrl+L liste les notes qui mènent à la note ouverte (lien avec alias, lien
        // dans un tableau). Bas en montre une en aperçu sans que la liste change ; elle suit un
        // lien retiré ; Entrée ouvre la note, et la liste devient la sienne.
        let was = shell.read_with(cx, |s, _| s.path.clone()).unwrap();
        fs::write(root.join("Cible.md"), "# Cible\n").unwrap();
        fs::write(root.join("Source A.md"), "# Source A\n\nvoir [[Cible|la cible]] ici\n").unwrap();
        fs::write(root.join("Source B.md"), "# Source B\n\n| a | [[cible]] |\n").unwrap();
        let settle = |cx: &mut gpui::VisualTestContext| {
            for _ in 0..2 {
                cx.executor().advance_clock(Duration::from_secs(3));
                cx.run_until_parked();
            }
        };
        settle(cx);
        shell.update(cx, |s, cx| s.open_note(&root.join("Cible.md"), cx));
        cx.simulate_keystrokes("secondary-l");
        cx.run_until_parked();
        let rows = |cx: &mut gpui::VisualTestContext| {
            shell.read_with(cx, |s, _| s.nav.rows.iter().map(|r| r.name.clone()).collect::<Vec<_>>())
        };
        assert_eq!(rows(cx), ["Source A", "Source B"]);
        assert!(vault::load_layout().starts_with("links split"));
        cx.simulate_keystrokes("down");
        cx.run_until_parked();
        assert!(shell.read_with(cx, |s, _| s.path.as_ref().is_some_and(|p| p.to_string_lossy().contains("Source"))));
        assert_eq!(rows(cx), ["Source A", "Source B"]);
        fs::write(root.join("Source B.md"), "# Source B\n").unwrap();
        settle(cx);
        assert_eq!(rows(cx), ["Source A"]);
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert!(shell.read_with(cx, |s, _| s.path.as_deref() == Some(&*root.join("Source A.md"))) && rows(cx).is_empty());
        for name in ["Cible.md", "Source A.md", "Source B.md"] {
            fs::remove_file(root.join(name)).unwrap();
        }

        // En-tête YAML (front matter) : ses lignes ne sont ni un titre ni une règle, le nom du
        // fichier suit le titre placé après lui, et ni la frappe, ni l'enregistrement, ni le
        // renommage, ni la réécriture des liens n'en changent un octet.
        let meta = "---\ntags: [projet]\n# pas un titre\nvoir: \"[[Fiche]]\"\n---\n";
        fs::write(root.join("Fiche.md"), format!("{meta}# Fiche\n\ntexte\n")).unwrap();
        fs::write(root.join("Renvoi.md"), format!("{meta}# Renvoi\n\n[[Fiche]]\n")).unwrap();
        settle(cx);
        shell.update(cx, |s, cx| s.open_note(&root.join("Fiche.md"), cx));
        cx.run_until_parked();
        let kind = |cx: &mut gpui::VisualTestContext, at: usize| shell.read_with(cx, |s, cx| s.editor.read(cx).kind_at(at));
        assert_eq!((kind(cx, 0), kind(cx, meta.find("# pas").unwrap())), (Some(markdown::Kind::Meta), Some(markdown::Kind::Meta)));
        assert_eq!(kind(cx, meta.len()), Some(markdown::Kind::Heading(1)));
        shell.update(cx, |s, cx| s.editor.update(cx, |e, cx| e.jump(meta.len() + "# Fiche".len(), false, cx)));
        cx.simulate_input(" 2");
        settle(cx);
        assert_eq!(fs::read_to_string(root.join("Fiche 2.md")).unwrap(), format!("{meta}# Fiche 2\n\ntexte\n"));
        assert!(!root.join("Fiche.md").exists());
        // Les liens suivent quand on quitte la note renommée.
        shell.update(cx, |s, cx| s.open_note(&root.join("Renvoi.md"), cx));
        settle(cx);
        assert_eq!(fs::read_to_string(root.join("Renvoi.md")).unwrap(), format!("{meta}# Renvoi\n\n[[Fiche 2]]\n"));
        // Ses tags valent ceux du texte ; ses alias se proposent après `[[`, et un lien écrit
        // avec l'un d'eux ouvre la note au lieu d'en créer une.
        fs::write(root.join("Appli.md"), "---\ntags:\n  - enyaml\naliases: [Bref app]\n---\n# Appli\n").unwrap();
        settle(cx);
        assert!(shell.read_with(cx, |s, _| s.notes.iter().any(|n| n.name == "Appli" && n.tags == ["enyaml"] && n.aliases == ["Bref app"])));
        let count = shell.read_with(cx, |s, _| s.notes.len());
        cx.simulate_keystrokes("secondary-end enter");
        cx.simulate_input("[[bref a");
        cx.simulate_keystrokes("enter");
        assert!(text(cx).ends_with("[[Bref app]]"));
        shell.update(cx, |s, cx| s.open_wiki("bref APP", cx));
        settle(cx);
        assert!(shell.read_with(cx, |s, _| s.path.as_deref() == Some(&*root.join("Appli.md")) && s.notes.len() == count));
        fs::remove_file(root.join("Appli.md")).unwrap();
        shell.update(cx, |s, cx| s.open_note(&root.join("Renvoi.md"), cx));
        shell.update(cx, |s, cx| s.open_note(&was, cx));
        settle(cx);
        for name in ["Fiche 2.md", "Renvoi.md"] {
            fs::remove_file(root.join(name)).unwrap();
        }
        settle(cx);

        // Corbeille : la palette liste ce qu'elle contient ; Entrée remet la note dans le coffre,
        // où elle reparaît parmi les notes.
        fs::write(root.join("Jetée.md"), "# Jetée\n").unwrap();
        settle(cx);
        vault::trash(&root, &root.join("Jetée.md")).unwrap();
        settle(cx);
        assert!(shell.read_with(cx, |s, _| s.notes.iter().all(|n| n.name != "Jetée")));
        cx.simulate_keystrokes("secondary-p");
        cx.simulate_input("corbeille");
        cx.simulate_keystrokes("down enter");
        cx.simulate_input("jet");
        cx.simulate_keystrokes("enter");
        settle(cx);
        assert!(root.join("Jetée.md").is_file() && !root.join(".trash/Jetée.md").exists());
        assert!(shell.read_with(cx, |s, _| s.notes.iter().any(|n| n.name == "Jetée") && s.notice.as_deref() == Some("Restauré : Jetée.md")));
        shell.update(cx, |s, _| s.notice = None);
        fs::remove_file(root.join("Jetée.md")).unwrap();
        settle(cx);

        // Sauvegarde : une archive datée de tout le coffre dans le dossier choisi (la fenêtre de
        // choix n'existe pas dans les tests) ; dans le coffre lui-même, elle est refusée.
        let out = root.with_file_name(format!("{}-sauvegardes", vault::stem(&root)));
        fs::create_dir_all(&out).unwrap();
        shell.update(cx, |s, cx| s.backup_to(out.clone(), cx));
        cx.run_until_parked();
        let saved = fs::read_dir(&out).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect::<Vec<_>>();
        assert!(saved.len() == 1 && saved[0].ends_with(&format!(" {}.tar.gz", date_name(today()))), "{saved:?}");
        assert!(shell.read_with(cx, |s, _| s.error.is_none() && s.notice.as_deref().is_some_and(|n| n.starts_with("Sauvegarde enregistrée") && n.ends_with(&saved[0]))));
        shell.update(cx, |s, cx| s.backup_to(root.join(".trash"), cx));
        cx.run_until_parked();
        assert!(shell.read_with(cx, |s, _| s.error.as_deref().is_some_and(|e| e.contains("hors du coffre"))));
        shell.update(cx, |s, _| (s.notice, s.error) = (None, None));
        fs::remove_dir_all(&out).unwrap();

        // Recherche dans le coffre : Ctrl+Maj+F liste chaque ligne où figure le texte tapé, sans
        // la casse ; Entrée ouvre la note, le passage sélectionné.
        fs::write(root.join("Recette.md"), "# Recette\n\nfarine\n\ndu Quinoa rouge, quinoa\n").unwrap();
        fs::write(root.join("Liste.md"), "# Liste\n- quinoa\n").unwrap();
        settle(cx);
        cx.simulate_keystrokes("secondary-shift-f");
        cx.simulate_input("QUINOA");
        cx.simulate_keystrokes("down enter");
        settle(cx);
        let found = shell.read_with(cx, |s, cx| (s.path.clone().unwrap(), s.editor.read(cx).selected().to_lowercase()));
        assert!(found.0 == root.join("Recette.md") || found.0 == root.join("Liste.md"), "{found:?}");
        assert_eq!(found.1, "quinoa");
        cx.simulate_keystrokes("secondary-shift-f");
        cx.simulate_input("quinoa r");
        cx.simulate_keystrokes("enter");
        settle(cx);
        assert_eq!(shell.read_with(cx, |s, cx| (s.path.clone().unwrap(), s.editor.read(cx).selected().to_string())), (root.join("Recette.md"), "Quinoa r".to_string()));
        shell.update(cx, |s, cx| s.open_note(&was, cx));
        for name in ["Recette.md", "Liste.md"] {
            fs::remove_file(root.join(name)).unwrap();
        }
        settle(cx);

        // Note du jour : Ctrl+J la crée, nommée et titrée de la date locale ; la seconde fois
        // (ici depuis la palette) c'est la même note qui s'ouvre. `/date` écrit la date.
        let day = date_name(today());
        cx.simulate_keystrokes("secondary-j");
        settle(cx);
        assert_eq!(fs::read_to_string(root.join(format!("{day}.md"))).unwrap(), format!("# {day}\n\n"));
        cx.simulate_input("vu le /date");
        cx.simulate_keystrokes("enter");
        assert_eq!(text(cx), format!("# {day}\n\nvu le {day}"));
        settle(cx);
        shell.update(cx, |s, cx| s.open_note(&was, cx));
        cx.simulate_keystrokes("secondary-p");
        cx.simulate_input("note du jour");
        cx.simulate_keystrokes("down enter");
        settle(cx);
        assert_eq!(text(cx), format!("# {day}\n\nvu le {day}"));
        assert!(!root.join(format!("{day} 2.md")).exists());
        shell.update(cx, |s, cx| s.open_note(&was, cx));
        fs::remove_file(root.join(format!("{day}.md"))).unwrap();
        settle(cx);
        shell.update(cx, |s, cx| s.open_note(&was, cx));
        settle(cx);
        cx.simulate_keystrokes("secondary-t");
        cx.run_until_parked();

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
        // Tirée du bord d'une forme, la flèche s'y fixe ; elle est coudée d'office.
        cx.simulate_input("a");
        drag(cx, (115., 30.), (350., 40.));
        let pinned = saved().links[1].clone();
        assert_eq!((pinned.from, pinned.route), (diagram::End::Pin(drawn.shapes[0].id, 1., 0.5), diagram::Route::Elbow));
        // « + » et « - » zooment, « 0 » recadre tout.
        let span = |cx: &mut gpui::VisualTestContext| canvas.read_with(cx, |c, _| f32::from(c.spot(100., 0.).x - c.spot(0., 0.).x));
        cx.simulate_input("+");
        assert_eq!(span(cx), 125.);
        cx.simulate_input("-");
        cx.simulate_input("0");
        cx.run_until_parked();
        assert!(span(cx) <= 100.);
        // Une forme se déplace en la tirant, sur la grille ; Ctrl+Z la remet en place.
        drag(cx, (350., 40.), (350., 143.));
        assert_eq!(saved().shapes[1].y, 100.);
        cx.simulate_keystrokes("secondary-z");
        assert_eq!(saved().shapes[1].y, 0.);
        // Ctrl+clic ajoute une forme à la sélection ; un second l'en retire.
        let click = |cx: &mut gpui::VisualTestContext, at: (f32, f32), held: gpui::Modifiers| {
            let at = canvas.read_with(cx, |c, _| c.spot(at.0, at.1));
            cx.simulate_click(at, held);
        };
        click(cx, (20., 50.), gpui::Modifiers::none());
        click(cx, (350., 40.), gpui::Modifiers::secondary_key());
        assert_eq!(canvas.read_with(cx, |c, _| c.picked()), 2);
        click(cx, (350., 40.), gpui::Modifiers::secondary_key());
        assert_eq!(canvas.read_with(cx, |c, _| c.picked()), 1);
        // Suppr retire la forme sélectionnée, et la flèche qui y tenait.
        drag(cx, (350., 40.), (350., 40.));
        cx.simulate_keystrokes("delete");
        assert_eq!((saved().shapes.len(), saved().links.len()), (1, 0));
        // Le schéma figure parmi les images du coffre ; Échap rend la main à la note.
        assert!(shell.read_with(cx, |s, _| s.images.contains(&file)));
        cx.simulate_keystrokes("escape escape");
        cx.run_until_parked();
        assert!(shell.read_with(cx, |s, _| s.picture.is_none() && s.drawing.is_none()));

        // `![[` propose les images et les schémas du coffre ; Entrée complète le nom.
        cx.simulate_keystrokes("secondary-end enter");
        cx.simulate_input("![[sch");
        cx.simulate_keystrokes("enter");
        assert!(text(cx).ends_with("![[Schéma.svg]]"));
        cx.executor().advance_clock(Duration::from_millis(500));
        cx.run_until_parked();

        // Commandes `/` : la liste se filtre par nom ou par libellé ; Entrée pose le
        // composant. Ici un panneau, dont la citation continue à la ligne.
        cx.simulate_keystrokes("enter");
        cx.simulate_input("/pan");
        cx.simulate_keystrokes("down enter");
        cx.simulate_input("vu");
        assert!(text(cx).ends_with("\n> [!TIP]\n> vu"));
        cx.simulate_keystrokes("enter enter");
        // `/tableau` ouvre la grille : les flèches règlent colonnes et lignes.
        cx.simulate_input("/tableau");
        cx.simulate_keystrokes("enter left down enter");
        cx.simulate_input("Nom");
        cx.simulate_keystrokes("tab");
        cx.simulate_input("Âge");
        // Tab passe à la cellule suivante, par-dessus les tirets, et réaligne les colonnes.
        cx.simulate_keystrokes("tab");
        cx.simulate_input("Élodie");
        cx.simulate_keystrokes("tab");
        cx.simulate_input("31");
        cx.simulate_keystrokes("tab");
        let empty = "|        |     |";
        let table = format!("| Nom    | Âge |\n| ------ | --- |\n| Élodie | 31  |\n{empty}\n{empty}");
        assert!(text(cx).ends_with(&format!("\n\n{table}")), "{}", text(cx));
        // Entrée ajoute une ligne sous celle du curseur ; les boutons « + » une
        // colonne à droite et une ligne en bas.
        cx.simulate_keystrokes("enter");
        assert!(text(cx).ends_with(&format!("{table}\n{empty}")));
        cx.run_until_parked();
        let [_, column] = shell.read_with(cx, |s, cx| s.editor.read(cx).plus_buttons().unwrap());
        cx.simulate_click(column, gpui::Modifiers::none());
        cx.simulate_input("Ville");
        // Chaque frappe réaligne les colonnes ; effacer aussi.
        assert!(text(cx).contains("| Nom    | Âge | Ville |\n| ------ | --- | ----- |\n| Élodie | 31  |       |"), "{}", text(cx));
        cx.simulate_input("s x");
        cx.simulate_keystrokes("backspace backspace backspace");
        assert!(text(cx).contains("| Nom    | Âge | Ville |\n| ------ | --- | ----- |\n| Élodie | 31  |       |"), "{}", text(cx));
        cx.run_until_parked();
        let [row, _] = shell.read_with(cx, |s, cx| s.editor.read(cx).plus_buttons().unwrap());
        cx.simulate_click(row, gpui::Modifiers::none());
        // Entrée sur une dernière ligne vide la retire et sort du tableau.
        cx.simulate_keystrokes("enter");
        cx.simulate_input("fin");
        let empty = "|        |     |       |";
        let table = format!("| Nom    | Âge | Ville |\n| ------ | --- | ----- |\n| Élodie | 31  |       |\n{empty}\n{empty}\n{empty}");
        assert!(text(cx).ends_with(&format!("\n\n{table}\nfin")), "{}", text(cx));
        // À la souris : la case survolée de la grille donne la taille du tableau.
        cx.simulate_keystrokes("enter");
        cx.simulate_input("/table");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        let cell = shell.read_with(cx, |s, cx| s.editor.read(cx).grid_cell(2, 1).unwrap());
        cx.simulate_click(cell, gpui::Modifiers::none());
        assert!(text(cx).ends_with("fin\n\n|     |     |\n| --- | --- |"), "{}", text(cx));
        // Au clavier : une taille tapée dans la grille, au-delà de ses cases.
        cx.simulate_keystrokes("secondary-end enter enter");
        cx.simulate_input("/table");
        cx.simulate_keystrokes("enter");
        cx.simulate_input("12x9");
        cx.simulate_keystrokes("backspace");
        cx.simulate_input("1");
        cx.simulate_keystrokes("enter");
        let wide = format!("|{}\n|{}", "     |".repeat(12), " --- |".repeat(12));
        assert!(text(cx).ends_with(&format!("| --- | --- |\n\n{wide}")), "{}", text(cx));
        cx.executor().advance_clock(Duration::from_millis(500));
        cx.run_until_parked();

        // Un tableau plus large que la page : ses cellules passent à la ligne, il garde la largeur
        // de la page, et le curseur reste dans ses cellules.
        let long = "mot ".repeat(120);
        let note = format!("avant\n\n| A | B |\n| --- | --- |\n| {long}| fin |\n\napres\n");
        let load = |cx: &mut gpui::VisualTestContext, text: &str, cursor: usize| {
            shell.update(cx, |s, cx| s.editor.update(cx, |e, cx| e.load(text.to_string(), cursor, cx)));
            cx.run_until_parked();
        };
        let state = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.editor.read(cx).table_state().unwrap());
        load(cx, &note, note.find("fin").unwrap());
        let (dx, tall, frame, page) = state(cx);
        assert!(dx == px(0.) && tall > 3 && frame <= page, "{dx:?} {tall} {frame:?} {page:?}");
        // Trop de colonnes pour tenir même à leur largeur minimale : le tableau défile, et suit
        // le curseur ; la molette horizontale le fait défiler aussi.
        let cols = 30;
        let head = format!("|{}\n|{}\n|{}\n", " colonne |".repeat(cols), " --- |".repeat(cols), " texte |".repeat(cols));
        load(cx, &head, head.rfind("texte").unwrap());
        let (dx, tall, frame, page) = state(cx);
        assert!(dx > px(0.) && frame <= page, "{dx:?} {tall} {frame:?} {page:?}");
        load(cx, &head, 0);
        assert_eq!(state(cx).0, px(0.));
        let on_table = shell.read_with(cx, |s, cx| s.editor.read(cx).point_of(head.find("colonne").unwrap()).unwrap());
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: on_table,
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(-90.), px(0.))),
            modifiers: gpui::Modifiers::none(),
            touch_phase: gpui::TouchPhase::Moved,
        });
        cx.run_until_parked();
        assert_eq!(state(cx).0, px(90.));

        // Coller dans un tableau : une cellule garde une seule ligne et ses `|` ; des
        // cellules de tableur remplissent la grille ; le tableau reste aligné.
        let small = "| Nom | Âge |\n| --- | --- |\n| Léa | 31 |\n";
        let paste = |cx: &mut gpui::VisualTestContext, clip: &str| {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(clip.to_string()));
            cx.simulate_keystrokes("secondary-v");
        };
        let cell = small.find("Léa").unwrap();
        load(cx, small, cell);
        paste(cx, "Zoé\nBob |x");
        assert_eq!(text(cx), "| Nom            | Âge |\n| -------------- | --- |\n| Zoé Bob \\|xLéa | 31  |\n");
        load(cx, small, cell);
        paste(cx, "x\ty\nz\tw\n");
        assert_eq!(text(cx), "| Nom | Âge |\n| --- | --- |\n| x   | y   |\n| z   | w   |\n");
        load(cx, small, small.find("31").unwrap());
        paste(cx, "1\t2\t3");
        assert_eq!(text(cx), "| Nom | Âge |     |     |\n| --- | --- | --- | --- |\n| Léa | 1   | 2   | 3   |\n");
        // Un tableau Markdown copié ailleurs se recolle cellule par cellule.
        load(cx, small, cell);
        paste(cx, "| a | b |\n| :-- | --: |\n| c | d |");
        assert_eq!(text(cx), "| Nom | Âge |\n| --- | --- |\n| a   | b   |\n| c   | d   |\n");
        // Couper dans une cellule réaligne aussi.
        load(cx, small, cell);
        cx.simulate_keystrokes("shift-right shift-right secondary-x");
        assert_eq!(text(cx), "| Nom | Âge |\n| --- | --- |\n| a   | 31  |\n");

        // Coller n'importe quoi n'importe où dans un tableau : il reste une grille régulière.
        let clips = [
            "", "x", "a|b", "a\nb", "a\tb", "a\tb\nc\td\ne\tf", "| x | y |\n| - | - |\n| 1 | 2 |", "日本語\t😀", "  \n",
            "\t\t", "ligne1\r\nligne2", &"très long ".repeat(12), "[[a|b]]", "`x|y`", "\\|", "|", "||", "| ", "a\tb\t\n", "\n\n\n",
            "| a | b | c | d |", "# titre\n- liste\n> citation",
        ];
        let grid = "| Nom | Âge |\n| --- | --- |\n| Léa | 31 |\n| Max |  |\n";
        let mut cases = 0;
        for at in (0..grid.len()).step_by(2).filter(|&i| grid.is_char_boundary(i)) {
            for clip in clips {
                load(cx, grid, at);
                paste(cx, clip);
                let table = text(cx);
                let counts: Vec<usize> = table.lines().map(|l| markdown::cells(l).len()).collect();
                assert!(
                    table.lines().all(|l| l.starts_with('|')) && counts.iter().all(|&n| n == counts[0]),
                    "{clip:?} collé à {at} : {table:?}"
                );
                cases += 1;
            }
        }
        assert!(cases > 500);

        // Le curseur va de cellule en cellule sans s'arrêter sur les `|`, saute la ligne de
        // tirets, et effacer ne fusionne jamais deux cellules.
        let caret = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.editor.read(cx).caret());
        let t = "| Nom | Âge  |\n| --- | ---- |\n| Léa | a\\|b |\n| Max | 7    |\n";
        let at = |needle: &str| t.find(needle).unwrap();
        load(cx, t, at("Léa") + "Léa".len());
        cx.simulate_keystrokes("right");
        assert_eq!(caret(cx), at("a\\|b"));
        // `\|` s'affiche `|` : une flèche le franchit d'un coup.
        cx.simulate_keystrokes("right right");
        assert_eq!(caret(cx), at("a\\|b") + 3);
        cx.simulate_keystrokes("left left");
        assert_eq!(caret(cx), at("a\\|b"));
        cx.simulate_keystrokes("end");
        assert_eq!(caret(cx), at("a\\|b") + 4);
        // Au bout d'une ligne, la première cellule de la suivante ; au bout du tableau, la ligne d'après.
        cx.simulate_keystrokes("right");
        assert_eq!(caret(cx), at("Max"));
        cx.simulate_keystrokes("home left");
        assert_eq!(caret(cx), at("a\\|b") + 4);
        load(cx, t, at("Léa"));
        cx.simulate_keystrokes("left");
        assert_eq!(caret(cx), at("Âge") + "Âge".len(), "la ligne de tirets se saute");
        cx.simulate_keystrokes("down");
        assert!((at("a\\|b")..=at("a\\|b") + 4).contains(&caret(cx)), "descend dans la colonne, tirets sautés : {}", caret(cx));
        load(cx, t, t.len() - 1);
        cx.simulate_keystrokes("end right");
        assert_eq!(caret(cx), t.len());
        // Effacer s'arrête au bord de la cellule ; un `\|` part d'un bloc.
        load(cx, t, at("Léa"));
        cx.simulate_keystrokes("backspace");
        assert_eq!(text(cx), t);
        load(cx, t, at("a\\|b") + 3);
        cx.simulate_keystrokes("backspace");
        assert_eq!(text(cx), "| Nom | Âge |\n| --- | --- |\n| Léa | ab  |\n| Max | 7   |\n");
        load(cx, t, at("Léa") + "Léa".len());
        cx.simulate_keystrokes("delete");
        assert_eq!(text(cx), t);

        // Une cellule trop longue passe à la ligne : les flèches la parcourent ligne par ligne.
        let long = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau upsilon phi chi psi omega ".repeat(3);
        let wrapped = format!("| A | B |\n| --- | --- |\n| {long}| fin |\n| z | z |\n");
        let start = wrapped.find("alpha").unwrap();
        load(cx, &wrapped, start);
        let rows_of = |cx: &mut gpui::VisualTestContext, i: usize| shell.read_with(cx, |s, cx| s.editor.read(cx).point_of(i).unwrap());
        let (top, below) = (rows_of(cx, start).y, rows_of(cx, start + 140).y);
        assert!(below > top + px(30.), "la cellule passe à la ligne : {top:?} {below:?}");
        cx.simulate_keystrokes("down");
        let first = caret(cx);
        assert!(first > start && first < start + 80 && rows_of(cx, first).y > top, "{first}");
        cx.simulate_keystrokes("up");
        let back = caret(cx);
        assert!(back < start + 40 && rows_of(cx, back).y == top);
        // Un clic sur une ligne coupée place le curseur à cet endroit.
        let spot = rows_of(cx, start + 140) + gpui::point(px(2.), px(0.));
        cx.simulate_click(spot, gpui::Modifiers::none());
        assert!(caret(cx).abs_diff(start + 140) <= 1, "{} {}", caret(cx), start + 140);
        // Sélectionner d'une cellule à l'autre ne prend que le texte des cellules.
        load(cx, t, at("Léa"));
        cx.simulate_keystrokes("shift-right shift-right shift-right shift-right shift-right");
        assert_eq!(shell.read_with(cx, |s, cx| s.editor.read(cx).selected().to_string()), "Léa | a");

        // La ligne de tirets ne se voit pas, sauf sous le curseur ; un raccourci règle l'alignement.
        let hidden = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.editor.read(cx).hidden_rows());
        let small = "| Nom | Âge |\n| --- | --- |\n| Léa | 31  |\n";
        load(cx, small, 0);
        assert_eq!(hidden(cx), 1);
        load(cx, small, small.find("---").unwrap());
        assert_eq!(hidden(cx), 0);
        load(cx, small, small.find("Âge").unwrap());
        cx.simulate_keystrokes("secondary-shift-r");
        assert_eq!(text(cx), "| Nom | Âge |\n| --- | --: |\n| Léa | 31  |\n");
        cx.simulate_keystrokes("secondary-shift-e");
        assert_eq!(text(cx), "| Nom | Âge |\n| --- | :-: |\n| Léa | 31  |\n");
        cx.simulate_keystrokes("secondary-shift-l");
        assert_eq!(text(cx), "| Nom | Âge |\n| --- | :-- |\n| Léa | 31  |\n");

        // Largeur d'affichage : un idéogramme compte pour deux colonnes dans le texte aligné.
        load(cx, "| a |\n| --- |\n| 日本語 |\n", 3);
        cx.simulate_input("x");
        assert_eq!(text(cx), "| ax     |\n| ------ |\n| 日本語 |\n");

        // Coller un tableau Markdown, ou des cellules de page web ou de tableur, hors d'un tableau.
        let clip = "| a | b |\n|:--|--:|\n| longue cellule | 2 |";
        let table = "| a              | b   |\n| :------------- | --: |\n| longue cellule | 2   |";
        load(cx, "avant\n\napres\n", 7);
        paste(cx, clip);
        assert_eq!(text(cx), format!("avant\n\n{table}\n\napres\n"), "{}", text(cx));
        // Des cellules séparées par des tabulations : la première ligne est l'en-tête.
        load(cx, "avant", 5);
        paste(cx, "Nom\tÂge\nLéa\t31\n");
        assert_eq!(text(cx), "avant\n\n| Nom | Âge |\n| --- | --- |\n| Léa | 31  |", "{}", text(cx));
        // Du code indenté par des tabulations, ou une seule ligne, reste du texte.
        load(cx, "", 0);
        paste(cx, "\tfn a()\n\t\tx");
        assert_eq!(text(cx), "\tfn a()\n\t\tx");

        // Des centaines de gestes au hasard (graine fixe) dans des tableaux de toute sorte : aucun
        // plantage, le curseur reste sur un bord de caractère, et un tableau en forme reste régulier.
        // `BREF_FUZZ_SEED=n cargo test end_to_end` essaie une autre suite de gestes.
        let mut seed = std::env::var("BREF_FUZZ_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(0x9E3779B97F4A7C15u64);
        let mut roll = move |n: usize| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % n as u64) as usize
        };
        let starts = [
            "| a | b |\n| --- | --- |\n| c | d |\n",
            "avant\n\n| 日本語 | 😀 émoji |\n| :-: | --: |\n| a \\| b | `x` **y** |\n\napres",
            "|\n| a\n| a | b | c\n",
            "| seul |\n",
            "| H1 | H2 |\n| --- | --- |\n| texte très long qui passe à la ligne dans sa cellule étroite | 1 |\n| | |\n",
            "```\n| pas | un tableau |\n```\n| mais | si |\n",
        ];
        let keys = [
            "left", "right", "up", "down", "home", "end", "backspace", "delete", "tab", "shift-tab", "enter",
            "shift-left", "shift-right", "shift-down", "shift-up", "secondary-left", "secondary-right", "secondary-z",
            "secondary-shift-z", "secondary-backspace", "secondary-home", "secondary-end", "pageup", "pagedown",
        ];
        let typed = ["a", "é", "日本", "😀", " ", "|", "\\|", "x y z", "-", ":", "[[n|a]]", "`c|d`"];
        let mut gestures = 0;
        for start in starts {
            for round in 0..3 {
                load(cx, start, if round % 2 == 0 { 0 } else { start.len() / 2 });
                for _ in 0..30 {
                    let before = text(cx);
                    // Une sélection qui sort du tableau remplace aussi ce qu'elle touche hors de lui.
                    let span = shell.read_with(cx, |s, cx| s.editor.read(cx).span());
                    let leaves = {
                        let (a, b) = (before[..span.start].rfind('\n').map_or(0, |i| i + 1), before[span.end..].find('\n').map_or(before.len(), |i| span.end + i));
                        !span.is_empty() && before[a..b].split('\n').any(|l| !l.trim_start().starts_with('|'))
                    };
                    let step = match roll(10) {
                        0..=4 => {
                            let key = keys[roll(keys.len())];
                            cx.simulate_keystrokes(key);
                            format!("touche {key}")
                        }
                        5..=7 => {
                            let input = typed[roll(typed.len())];
                            cx.simulate_input(input);
                            format!("saisie {input:?}")
                        }
                        _ => {
                            let clip = clips[roll(clips.len())];
                            paste(cx, clip);
                            format!("collage {clip:?}")
                        }
                    };
                    gestures += 1;
                    let (now, at) = (text(cx), caret(cx));
                    assert!(at <= now.len() && now.is_char_boundary(at), "curseur {at} dans {now:?}");
                    cx.run_until_parked();
                    // Chaque tableau qui était déjà en forme (ligne de tirets en deuxième) garde le même
                    // nombre de cellules partout : taper ou coller n'en défait aucun.
                    let formed = |text: &str| -> Vec<(usize, Vec<usize>)> {
                        let lines: Vec<&str> = text.lines().collect();
                        let is_row = |l: &str| l.trim_start().starts_with('|');
                        let dashes = |l: &str| markdown::cells(l).iter().all(|r| l[r.clone()].contains('-') && l[r.clone()].chars().all(|c| matches!(c, '-' | ':')));
                        let mut fence = false;
                        let mut found = Vec::new();
                        for (i, l) in lines.iter().enumerate() {
                            fence ^= l.trim_start().starts_with("```");
                            let top = !fence && is_row(l) && (i == 0 || !is_row(lines[i - 1]));
                            if top && lines.get(i + 1).is_some_and(|d| is_row(d) && dashes(d)) {
                                let counts = lines[i..].iter().take_while(|l| is_row(l)).map(|l| markdown::cells(l).len()).filter(|&n| n > 0);
                                found.push((i, counts.collect()));
                            }
                        }
                        found
                    };
                    let steady = |counts: &[usize]| counts.iter().all(|&n| n == counts[0]);
                    let was: Vec<usize> = formed(&before).into_iter().filter(|(_, c)| steady(c)).map(|(i, _)| i).collect();
                    for (i, counts) in formed(&now) {
                        assert!(steady(&counts) || !was.contains(&i) || leaves || step.ends_with("-z"), "{counts:?} après {step} (curseur {at}, sélection {span:?}) : {before:?} -> {now:?}");
                    }
                }
            }
        }
        assert!(gestures > 500);

        // `/meta` (ou `/tags`, `/alias`) pose l'en-tête YAML en tête de note, où qu'on soit, le
        // curseur entre les crochets des tags ; redemandé, il y ramène sans en poser un second.
        load(cx, "# Titre\n\ntexte ", "# Titre\n\ntexte ".len());
        cx.simulate_input("/tags");
        cx.simulate_keystrokes("enter");
        cx.simulate_input("projet");
        assert_eq!(text(cx), "---\ntags: [projet]\naliases: []\n---\n# Titre\n\ntexte ");
        cx.simulate_keystrokes("secondary-end");
        cx.simulate_input("/meta");
        cx.simulate_keystrokes("enter");
        cx.simulate_input("x");
        assert_eq!(text(cx), "---\ntags: [xprojet]\naliases: []\n---\n# Titre\n\ntexte ");

        // En-tête YAML ouvert au clavier : tant qu'il n'est pas fermé, sa première ligne reste une règle.
        load(cx, "---\na: 1\n", 9);
        assert_eq!(kind(cx, 0), Some(markdown::Kind::Rule));
        cx.simulate_input("---");
        cx.run_until_parked();
        assert_eq!((kind(cx, 0), kind(cx, 4), kind(cx, 9)), (Some(markdown::Kind::Meta), Some(markdown::Kind::Meta), Some(markdown::Kind::Meta)));

        // Plan de la note : Ctrl+Maj+O liste les titres, celui de la section en cours
        // présélectionné. Le titre parcouru reçoit le curseur en aperçu (deux titres de même nom
        // restent distincts, celui du bloc de code n'en est pas un), Échap le rend, Entrée l'y laisse.
        let plan = "# Un\n\ntexte\n\n## Deux\n\n```\n# code\n```\n\n## Deux\n\nfin\n";
        load(cx, plan, plan.find("texte").unwrap());
        let caret = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.editor.read(cx).position());
        cx.simulate_keystrokes("secondary-shift-o down");
        assert_eq!(caret(cx), plan.find("## Deux").unwrap());
        cx.simulate_keystrokes("down");
        assert_eq!(caret(cx), plan.rfind("## Deux").unwrap());
        cx.simulate_keystrokes("down");
        assert_eq!(caret(cx), 0);
        cx.simulate_keystrokes("escape");
        assert_eq!(caret(cx), plan.find("texte").unwrap());
        cx.simulate_keystrokes("secondary-shift-o");
        cx.simulate_input("deux");
        cx.simulate_keystrokes("enter");
        assert!(caret(cx) == plan.find("## Deux").unwrap() && shell.read_with(cx, |s, _| s.palette.is_none()));
        load(cx, "sans titre", 0);
        cx.simulate_keystrokes("secondary-shift-o");
        assert!(shell.read_with(cx, |s, _| s.palette.is_none() && s.notice.as_deref().is_some_and(|n| n.contains("titres"))));
        shell.update(cx, |s, _| s.notice = None);

        // Recherche dans la note : Ctrl+F, la frappe montre le premier passage, Entrée et
        // Maj+Entrée tournent en boucle, Alt+C tient compte de la casse, Alt+W des mots entiers.
        load(cx, "Été, été.\n\n| a | étés |\n| - | ---- |\n| b | ÉTÉ  |\n", 0);
        let finding = |cx: &mut gpui::VisualTestContext| {
            shell.read_with(cx, |s, cx| s.editor.read(cx).finding().map(|(query, at, n)| (query.to_string(), at, n)))
        };
        let span = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.editor.read(cx).span());
        cx.simulate_keystrokes("secondary-f");
        cx.simulate_input("étéx");
        assert_eq!(finding(cx), Some(("étéx".into(), 0, 0)));
        cx.simulate_keystrokes("backspace");
        assert_eq!((finding(cx), span(cx)), (Some(("été".into(), 1, 4)), 0.."Été".len()));
        cx.simulate_keystrokes("enter enter enter enter");
        assert_eq!(finding(cx), Some(("été".into(), 1, 4)));
        cx.simulate_keystrokes("shift-enter");
        assert_eq!(finding(cx), Some(("été".into(), 4, 4)));
        cx.simulate_keystrokes("alt-c");
        assert_eq!(finding(cx), Some(("été".into(), 1, 2)));
        cx.simulate_keystrokes("alt-w");
        assert_eq!((finding(cx), span(cx)), (Some(("été".into(), 1, 1)), "Été, ".len().."Été, été".len()));
        cx.simulate_keystrokes("alt-c alt-w");
        assert_eq!(finding(cx), Some(("été".into(), 2, 4)));
        // Remplacer : Ctrl+H ouvre le second champ, Tab y passe, Entrée remplace le passage
        // courant, Ctrl+Entrée tous les autres ; Échap ferme, et un seul Ctrl+Z les rend tous.
        cx.simulate_keystrokes("secondary-h tab");
        cx.simulate_input("hiver");
        cx.simulate_keystrokes("enter");
        assert!(text(cx).starts_with("Été, hiver.\n"));
        assert_eq!(finding(cx), Some(("été".into(), 2, 3)));
        cx.simulate_keystrokes("secondary-enter");
        assert!(text(cx).starts_with("hiver, hiver.\n") && text(cx).contains("hivers") && !text(cx).to_lowercase().contains("été"));
        assert_eq!(finding(cx), Some(("été".into(), 0, 0)));
        cx.simulate_keystrokes("escape");
        assert_eq!(finding(cx), None);
        cx.simulate_keystrokes("secondary-z");
        assert!(text(cx).starts_with("Été, hiver.\n") && text(cx).contains("ÉTÉ"));
        // La sélection (ici le passage que l'annulation a rendu) devient la recherche ; la
        // première frappe la remplace. Barre ouverte, saisie rendue à la note : les passages
        // suivent le texte, Échap ferme.
        cx.simulate_keystrokes("secondary-f");
        assert!(finding(cx).is_some_and(|(query, ..)| query.to_lowercase().starts_with("été")));
        cx.simulate_input("hiver");
        assert_eq!(finding(cx), Some(("hiver".into(), 1, 1)));
        shell.update(cx, |s, cx| s.editor.update(cx, |e, cx| e.load("hiver hiver".into(), 0, cx)));
        cx.run_until_parked();
        assert_eq!(finding(cx), Some(("hiver".into(), 1, 2)));
        cx.simulate_input("x");
        assert_eq!((text(cx), finding(cx)), ("xhiver hiver".into(), Some(("hiver".into(), 1, 2))));
        // La saisie est dans la note : F3 et les réglages de la barre répondent quand même.
        cx.simulate_keystrokes("f3");
        assert_eq!(finding(cx), Some(("hiver".into(), 2, 2)));
        cx.simulate_keystrokes("alt-w");
        assert_eq!(finding(cx), Some(("hiver".into(), 1, 1)));
        cx.simulate_keystrokes("alt-w alt-c alt-c escape");
        assert_eq!(finding(cx), None);
        // Sans barre, F3 l'ouvre.
        cx.simulate_keystrokes("f3");
        assert!(finding(cx).is_some());
        cx.simulate_keystrokes("escape");
        assert_eq!(finding(cx), None);

        // Cases à cocher : dans le texte comme dans un tableau, un clic les coche, Ctrl+Entrée aussi.
        let boxed = "avant [ ] fait [x] fin\n\n| Tâche | Fait |\n| ----- | ---- |\n| a     | [ ]  |\n| b     |      |\n";
        load(cx, boxed, boxed.len());
        let tick = |cx: &mut gpui::VisualTestContext, needle: &str| {
            let at = boxed.find(needle).unwrap() + 1;
            let spot = shell.read_with(cx, |s, cx| s.editor.read(cx).point_of(at).unwrap()) + gpui::point(px(3.), px(0.));
            cx.simulate_click(spot, gpui::Modifiers::none());
        };
        tick(cx, "[ ] fait");
        assert!(text(cx).starts_with("avant [x] fait [x] fin"), "{}", text(cx));
        tick(cx, "[x] fin");
        assert!(text(cx).starts_with("avant [x] fait [ ] fin"), "{}", text(cx));
        tick(cx, "[ ]  |");
        assert!(text(cx).ends_with("| a     | [x]  |\n| b     |      |\n"), "{}", text(cx));
        cx.simulate_keystrokes("secondary-z");
        assert!(text(cx).ends_with("| a     | [ ]  |\n| b     |      |\n"), "{}", text(cx));
        // Une case dans le code reste du texte.
        load(cx, "`[ ]` et\n\n```\n[ ]\n```\n", 0);
        let code = shell.read_with(cx, |s, cx| s.editor.read(cx).point_of(2).unwrap()) + gpui::point(px(3.), px(0.));
        cx.simulate_click(code, gpui::Modifiers::none());
        assert!(text(cx).starts_with("`[ ]` et"));
        // Ctrl+Entrée dans une cellule : elle reçoit une case, puis la coche.
        load(cx, boxed, boxed.find("| b").unwrap() + "| b     | ".len());
        cx.simulate_keystrokes("secondary-enter");
        assert!(text(cx).ends_with("| b     | [ ]  |\n"), "{}", text(cx));
        cx.simulate_keystrokes("secondary-enter");
        assert!(text(cx).ends_with("| b     | [x]  |\n"), "{}", text(cx));

        // Double clic puis glisser : la sélection s'étend de mot en mot ; le triple, de ligne en ligne.
        let words = "alpha beta gamma delta\nsecond\ntroisième\n";
        load(cx, words, 0);
        let spot = |cx: &mut gpui::VisualTestContext, i: usize| {
            shell.read_with(cx, |s, cx| s.editor.read(cx).point_of(i).unwrap()) + gpui::point(px(3.), px(0.))
        };
        let selected = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.editor.read(cx).selected().to_string());
        let (beta, gamma) = (spot(cx, 7), spot(cx, 13));
        let click = |count| gpui::MouseDownEvent {
            position: beta,
            modifiers: gpui::Modifiers::none(),
            button: gpui::MouseButton::Left,
            click_count: count,
            first_mouse: false,
        };
        cx.simulate_event(click(2));
        assert_eq!(selected(cx), "beta");
        cx.simulate_mouse_move(gamma, gpui::MouseButton::Left, gpui::Modifiers::none());
        assert_eq!(selected(cx), "beta gamma");
        let alpha = spot(cx, 1);
        cx.simulate_mouse_move(alpha, gpui::MouseButton::Left, gpui::Modifiers::none());
        assert_eq!(selected(cx), "alpha beta");
        cx.simulate_mouse_up(alpha, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_move(gamma, None, gpui::Modifiers::none());
        assert_eq!(selected(cx), "alpha beta");
        cx.simulate_event(click(3));
        let third = spot(cx, words.find("troi").unwrap() + 2);
        cx.simulate_mouse_move(third, gpui::MouseButton::Left, gpui::Modifiers::none());
        assert_eq!(selected(cx), "alpha beta gamma delta\nsecond\ntroisième");
        cx.simulate_mouse_up(third, gpui::MouseButton::Left, gpui::Modifiers::none());

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

        // Le bouton « nouvelle note » de l'arbre range à la racine du coffre, même avec un dossier sélectionné.
        shell.update_in(cx, |s, _, cx| {
            s.nav.panel = Panel::Split;
            s.nav.mode = Mode::Tree;
            s.nav.sel = Some(root.join("Projets"));
            s.flush(cx);
        });
        shell.update_in(cx, |s, window, cx| s.new_note_in(None, window, cx));
        cx.simulate_input("# Racine");
        shell.update(cx, |s, cx| s.flush(cx));
        assert!(root.join("Racine.md").is_file() && !root.join("Projets/Racine.md").exists());

        // Ctrl+P : « Nouveau dossier » ouvre le champ de saisie du nom, le dossier naît dans le coffre.
        shell.update(cx, |s, _| s.nav.sel = None);
        cx.simulate_keystrokes("secondary-p");
        cx.simulate_input("nouveau dossier");
        cx.simulate_keystrokes("down enter");
        cx.simulate_input("Archives");
        cx.simulate_keystrokes("enter");
        assert!(root.join("Archives").is_dir(), "dossier créé depuis la palette");

        // Tableau : un CSV en Windows-1252 séparé par « ; » s'ouvre dans une grille, s'édite et se réécrit
        // dans le même encodage, les lignes intactes octet pour octet.
        let csv = root.join("data.csv");
        fs::write(&csv, b"nom;age\nAna\xefs;31\nBob;27\n").unwrap();
        shell.update(cx, |s, cx| {
            let (notes, dirs, images) = vault::scan(&root);
            s.sync(notes, dirs, images, cx);
            s.open_note(&csv, cx);
        });
        let sheet = shell.read_with(cx, |s, _| s.shown_sheet().expect("le tableau est affiché"));
        let format = sheet.read_with(cx, |sheet, _| sheet.format());
        assert_eq!((format.delimiter, format.encoding, format.header), (b';', table::Encoding::Windows1252, true));
        assert_eq!(sheet.read_with(cx, |sheet, _| sheet.table().cell(1, 0).to_string()), "Anaïs");
        // Au premier affichage la vue n'est pas encore mesurée, et GPUI ne redessine pas pour un `notify`
        // fait pendant le dessin : la grille doit déjà montrer toute la fenêtre, pas quelques cellules.
        let (lines, columns) = sheet.update(cx, |sheet, _| sheet.unmeasured());
        assert_eq!((lines, columns), (1..3, 0..2));
        shell.update_in(cx, |s, window, cx| {
            let sheet = s.shown_sheet().unwrap();
            window.focus(&sheet.focus_handle(cx));
        });
        let cell = |cx: &mut gpui::VisualTestContext, r, c| sheet.read_with(cx, |sheet, _| sheet.table().cell(r, c).to_string());
        // Taper remplace la cellule, Entrée valide et descend.
        cx.simulate_keystrokes("down right");
        cx.simulate_input("32");
        cx.simulate_keystrokes("enter");
        assert_eq!(cell(cx, 1, 1), "32");
        // Maj + flèches sélectionne une plage ; Ctrl+C la copie en TSV, Ctrl+V la colle ailleurs.
        cx.simulate_keystrokes("up shift-left shift-down");
        cx.simulate_keystrokes("secondary-c");
        assert_eq!(cx.read_from_clipboard().and_then(|c| c.text()).as_deref(), Some("Anaïs\t32\nBob\t27"));
        cx.simulate_keystrokes("down");
        cx.simulate_keystrokes("secondary-v");
        // Collée au coin de la sélection (ligne 2), la grille ajoute la ligne qui manque.
        assert_eq!((cell(cx, 2, 0), cell(cx, 3, 1)), ("Anaïs".into(), "27".into()));
        assert_eq!(sheet.read_with(cx, |sheet, _| sheet.table().rows()), 4);
        // Annuler défait le collage d'un coup, lignes ajoutées comprises.
        cx.simulate_keystrokes("secondary-z");
        assert_eq!(sheet.read_with(cx, |sheet, _| sheet.table().rows()), 3);
        // L'écriture suit après un court délai ; seule la ligne touchée a changé.
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        assert_eq!(fs::read(&csv).unwrap(), b"nom;age\nAna\xefs;32\nBob;27\n");
        // Une copie en Markdown se colle dans une note ; l'export crée un fichier à côté du tableau.
        // Sur une seule cellule, ils prennent tout le tableau.
        cx.simulate_keystrokes("secondary-home");
        sheet.update(cx, |sheet, cx| sheet.copy_as(table::Style::Markdown, cx));
        assert_eq!(
            cx.read_from_clipboard().and_then(|c| c.text()).as_deref().map(|t| t.lines().next().unwrap_or("").to_string()),
            Some("| nom | age |".to_string())
        );
        sheet.update(cx, |sheet, cx| sheet.export(table::Style::Json, cx));
        assert!(root.join("data.json").is_file());
        // Ctrl+clic : une seconde sélection s'ajoute à la première. Copier prend les deux, l'une
        // sous l'autre ; effacer aussi, en une seule annulation ; un clic simple ne garde que la sienne.
        cx.run_until_parked();
        let spot = |cx: &mut gpui::VisualTestContext, r, c| sheet.read_with(cx, |sheet, _| sheet.spot(r, c));
        let picked = |cx: &mut gpui::VisualTestContext| sheet.read_with(cx, |sheet, _| sheet.picked());
        let (first, second) = (spot(cx, 1, 0), spot(cx, 2, 1));
        cx.simulate_click(first, gpui::Modifiers::none());
        cx.simulate_click(second, gpui::Modifiers::secondary_key());
        assert_eq!(picked(cx), [(1..2, 0..1), (2..3, 1..2)]);
        cx.simulate_keystrokes("secondary-c");
        assert_eq!(cx.read_from_clipboard().and_then(|c| c.text()).as_deref(), Some("Anaïs\n27"));
        cx.simulate_keystrokes("delete");
        assert_eq!((cell(cx, 1, 0), cell(cx, 2, 1), cell(cx, 1, 1)), (String::new(), String::new(), "32".to_string()));
        cx.simulate_keystrokes("secondary-z");
        assert_eq!((cell(cx, 1, 0), cell(cx, 2, 1)), ("Anaïs".to_string(), "27".to_string()));
        cx.simulate_click(first, gpui::Modifiers::none());
        assert_eq!(picked(cx), [(1..2, 0..1)]);
        // Sélectionner une colonne, une ligne ou tout ne fait pas défiler la vue (elle ne saute pas à la dernière ligne).
        let long = root.join("long.csv");
        fs::write(&long, (0..400).map(|i| format!("{i};a{i};b{i};c{i}\n")).collect::<String>()).unwrap();
        shell.update(cx, |s, cx| {
            let (notes, dirs, images) = vault::scan(&root);
            s.sync(notes, dirs, images, cx);
            s.open_note(&long, cx);
        });
        cx.run_until_parked();
        let sheet = shell.read_with(cx, |s, _| s.shown_sheet().unwrap());
        sheet.update(cx, |sheet, _| sheet.scroll_by(0., 2000.));
        let before = sheet.read_with(cx, |sheet, _| sheet.view_state().0);
        assert!(before.1 > 1000., "la vue est bien descendue : {before:?}");
        for (what, n, cells) in [("col", 2, (0..400, 2..3)), ("row", 300, (300..301, 0..4)), ("all", 0, (0..400, 0..4))] {
            sheet.update(cx, |sheet, cx| sheet.pick(what, n, cx));
            let (scroll, selected) = sheet.read_with(cx, |sheet, _| sheet.view_state());
            assert_eq!((scroll, selected), (before, cells), "{what}");
        }
        shell.update(cx, |s, cx| s.open_note(&csv, cx));
        cx.run_until_parked();
        let sheet = shell.read_with(cx, |s, _| s.shown_sheet().unwrap());

        // Changer de délimiteur relit le fichier ainsi, et le choix est retenu.
        sheet.update(cx, |sheet, cx| sheet.reformat(Some(table::Format { delimiter: b',', ..sheet.format() }), cx));
        assert_eq!(sheet.read_with(cx, |sheet, _| sheet.table().cols()), 1);
        assert_eq!(sheet.read_with(cx, |sheet, _| sheet.table().cell(0, 0).to_string()), "nom;age");
        shell.update(cx, |s, cx| {
            s.open_note(&root.join("Racine.md"), cx);
        });
        shell.update(cx, |s, cx| s.open_note(&csv, cx));
        let delimiter = shell.read_with(cx, |s, cx| s.shown_sheet().unwrap().read(cx).format().delimiter);
        assert_eq!(delimiter, b',');

        // Dans une note, des cellules de tableau sélectionnées se copient en TSV, que les tableurs collent.
        shell.update_in(cx, |s, window, cx| {
            s.new_note("| a | b |\n| --- | --- |\n| 1 | 2 |\n".into(), cx);
            window.focus(&s.editor.focus_handle(cx));
        });
        cx.simulate_keystrokes("secondary-a secondary-c");
        assert_eq!(cx.read_from_clipboard().and_then(|c| c.text()).as_deref(), Some("a\tb\n1\t2"));

        fs::remove_dir_all(&root).unwrap();
    }

    /// Ouvre chaque fichier du dossier `BREF_CSV_DIR` dans le tableau (copié, rien n'est modifié chez
    /// l'utilisateur), chronomètre l'ouverture, puis enchaîne des gestes au hasard : un plantage se voit ici.
    /// `BREF_CSV_DIR=<dossier> cargo test --release --locked sheet_fuzz -- --ignored --nocapture`.
    #[gpui::test]
    #[ignore]
    fn sheet_fuzz(cx: &mut TestAppContext) {
        let Ok(from) = std::env::var("BREF_CSV_DIR") else { return };
        let root = std::env::temp_dir().join(format!("bref-fuzz-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let _config = isolated_config(&root);
        let mut files = Vec::new();
        for entry in fs::read_dir(&from).unwrap().flatten() {
            let to = root.join(entry.file_name());
            fs::copy(entry.path(), &to).unwrap();
            if vault::is_table(&to) {
                files.push(to);
            }
        }
        files.sort();
        cx.update(bind_keys);
        cx.update(init_fonts);
        let (shell, cx) = cx.add_window_view({
            let root = root.clone();
            |window, cx| Shell::new(Some(root), Vec::new(), window, cx)
        });
        cx.run_until_parked();
        let mut seed = 0x2545F4914F6CDD1Du64;
        let mut next = move |n: usize| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % n as u64) as usize
        };
        let keys = "left right up down shift-left shift-right shift-up shift-down pageup pagedown home end secondary-home secondary-end secondary-up secondary-down tab shift-tab enter f2 escape backspace delete secondary-c secondary-x secondary-v secondary-a secondary-z secondary-shift-z secondary-enter secondary-shift-enter secondary-delete".split(' ').collect::<Vec<_>>();
        let texts = ["a", "é", "x y", "€", "☃", "😀e\u{301}", ",", "\"", "12"];
        for file in files {
            let start = Instant::now();
            shell.update_in(cx, |s, window, cx| {
                s.open_note(&file, cx);
                if let Some(sheet) = s.shown_sheet() {
                    window.focus(&sheet.focus_handle(cx));
                }
            });
            cx.run_until_parked();
            println!("{:?} : ouverture + affichage {:?}", file.file_name().unwrap(), start.elapsed());
            let sheet = shell.read_with(cx, |s, _| s.shown_sheet().expect("tableau affiché"));
            let mut worst = Duration::ZERO;
            for step in 0..600 {
                let begin = Instant::now();
                match next(10) {
                    0..=4 => cx.simulate_keystrokes(keys[next(keys.len())]),
                    5 | 6 => cx.simulate_input(texts[next(texts.len())]),
                    7 => {
                        let at = point(px(330. + next(520) as f32), px(60. + next(640) as f32));
                        let mods = if next(2) == 0 { gpui::Modifiers::none() } else { gpui::Modifiers::shift() };
                        cx.simulate_mouse_down(at, gpui::MouseButton::Left, mods);
                        let to = point(px(330. + next(520) as f32), px(60. + next(640) as f32));
                        cx.simulate_mouse_move(to, gpui::MouseButton::Left, mods);
                        cx.simulate_mouse_up(to, gpui::MouseButton::Left, mods);
                    }
                    8 => cx.simulate_event(gpui::ScrollWheelEvent {
                        position: point(px(700.), px(300.)),
                        delta: gpui::ScrollDelta::Pixels(point(px(next(200) as f32 - 100.), px(next(4000) as f32 - 2000.))),
                        ..Default::default()
                    }),
                    _ => match next(5) {
                        0 => sheet.update(cx, |s, cx| s.toggle_header(cx)),
                        1 => sheet.update(cx, |s, cx| s.copy_as(table::Style::Json, cx)),
                        2 => sheet.update(cx, |s, cx| s.reformat(Some(table::Format { delimiter: table::DELIMITERS[next(6)], ..s.format() }), cx)),
                        3 => sheet.update(cx, |s, cx| s.export(table::Style::Markdown, cx)),
                        _ => sheet.update(cx, |s, cx| s.reformat(Some(table::Format { encoding: table::Encoding::ALL[next(6)], ..s.format() }), cx)),
                    },
                }
                cx.run_until_parked();
                worst = worst.max(begin.elapsed());
                assert!(step < 10_000);
            }
            println!("   geste le plus lent : {worst:?}");
        }
        fs::remove_dir_all(&root).unwrap();
    }

    /// Un gros CSV transformé : copie et export dans chaque format, ouverture de la note Markdown obtenue,
    /// collage du texte dans une note. `BREF_CSV=<fichier> cargo test --release --locked sheet_convert -- --ignored --nocapture`.
    #[gpui::test]
    #[ignore]
    fn sheet_convert(cx: &mut TestAppContext) {
        let Ok(from) = std::env::var("BREF_CSV") else { return };
        let root = std::env::temp_dir().join(format!("bref-convert-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let _config = isolated_config(&root);
        let file = root.join("gros.csv");
        fs::copy(&from, &file).unwrap();
        cx.update(bind_keys);
        cx.update(init_fonts);
        let (shell, cx) = cx.add_window_view({
            let root = root.clone();
            |window, cx| Shell::new(Some(root), Vec::new(), window, cx)
        });
        cx.run_until_parked();
        shell.update_in(cx, |s, window, cx| {
            s.open_note(&file, cx);
            window.focus(&s.shown_sheet().unwrap().focus_handle(cx));
        });
        cx.run_until_parked();
        let sheet = shell.read_with(cx, |s, _| s.shown_sheet().unwrap());
        cx.simulate_keystrokes("secondary-a");
        for (name, style) in [("TSV", table::Style::Tsv), ("CSV", table::Style::Csv(b',')), ("Markdown", table::Style::Markdown), ("JSON", table::Style::Json)] {
            let start = Instant::now();
            sheet.update(cx, |s, cx| s.copy_as(style, cx));
            let copied = start.elapsed();
            let len = cx.read_from_clipboard().and_then(|c| c.text()).map_or(0, |t| t.len());
            let start = Instant::now();
            sheet.update(cx, |s, cx| s.export(style, cx));
            cx.run_until_parked();
            println!("{name:9} copie {copied:?} ({} Mo) · export {:?}", len >> 20, start.elapsed());
        }
        let exported = root.join(format!("gros{}.md", tr(" (selection)", " (sélection)")));
        println!("note exportée : {} Mo", fs::metadata(&exported).map_or(0, |m| m.len()) >> 20);
        let start = Instant::now();
        shell.update(cx, |s, cx| {
            let (notes, dirs, images) = vault::scan(&root);
            s.sync(notes, dirs, images, cx);
        });
        println!("rescan du coffre avec cette note : {:?}", start.elapsed());
        let start = Instant::now();
        shell.update_in(cx, |s, window, cx| {
            s.open_note(&exported, cx);
            window.focus(&s.editor.focus_handle(cx));
        });
        cx.run_until_parked();
        println!("ouverture de la note Markdown : {:?}", start.elapsed());
        let start = Instant::now();
        cx.simulate_input("a");
        cx.run_until_parked();
        println!("une frappe dedans : {:?}", start.elapsed());
        let start = Instant::now();
        cx.simulate_keystrokes("down");
        cx.run_until_parked();
        println!("flèche bas dedans : {:?}", start.elapsed());
        // Coller le TSV d'un gros tableau dans une note neuve.
        let tsv: String = (0..300_000).map(|i| format!("{i}\tProduit {}\t{}\n", i % 977, i % 99)).collect();
        cx.write_to_clipboard(ClipboardItem::new_string(tsv));
        shell.update_in(cx, |s, window, cx| {
            s.new_note(String::new(), cx);
            window.focus(&s.editor.focus_handle(cx));
        });
        let start = Instant::now();
        cx.simulate_keystrokes("secondary-v");
        cx.run_until_parked();
        println!("coller 300 000 lignes de TSV dans une note : {:?}", start.elapsed());
        for what in ["a", "down", "up", "enter"] {
            let start = Instant::now();
            if what.len() == 1 { cx.simulate_input(what) } else { cx.simulate_keystrokes(what) }
            cx.run_until_parked();
            println!("   dans la note collée, {what:6} : {:?}", start.elapsed());
        }
        fs::remove_dir_all(&root).unwrap();
    }

    /// Des milliers de modifications au hasard sur des notes variées : chaque mise en page reprise est
    /// comparée (dans `Editor::layout`, en test) à une mise en page complète.
    #[gpui::test]
    fn layout_reuse_matches_full_layout(cx: &mut TestAppContext) {
        let root = std::env::temp_dir().join(format!("bref-reuse-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let _config = isolated_config(&root);
        cx.update(bind_keys);
        cx.update(init_fonts);
        let (shell, cx) = cx.add_window_view({
            let root = root.clone();
            |window, cx| Shell::new(Some(root), Vec::new(), window, cx)
        });
        cx.run_until_parked();
        let mut seed = std::env::var("BREF_FUZZ_SEED").ok().and_then(|n| n.parse().ok()).unwrap_or(0x9E3779B97F4A7C15u64);
        let steps: usize = std::env::var("BREF_FUZZ_STEPS").ok().and_then(|n| n.parse().ok()).unwrap_or(250);
        let mut next = move |n: usize| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % n as u64) as usize
        };
        let table = |rows: usize| {
            let body: String = (0..rows).map(|i| format!("| {i} | produit {} | note, {i} |\n", i % 7)).collect();
            format!("| id | produit | note |\n| --- | --- | --- |\n{body}")
        };
        let docs = [
            format!("# Titre\n\nUn paragraphe.\n\n- liste un\n- liste deux\n\n> [!NOTE]\n> une note\n> suite\n\n{}\ntexte après\n\n1. un\n2. deux\n", table(6)),
            format!("# Gros\n\navant\n\n{}\naprès\n\n> citation\n\n- fin\n", table(grid::BIG + 40)),
            "# Code\n\n```rust\nfn main() {}\n```\n\ntexte\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n".to_string(),
            "texte seul\n\n---\n\n## deux\n\n- [ ] tâche\n- [x] faite\n".to_string(),
        ];
        let pieces = ["a", "é", " ", "| x | y |", "> [!NOTE]", "> q", "```", "- ", "1. ", "# ", "$$", "---", "![](x.png)", "|", "texte", "\n", "\n\n"];
        let keys = ["enter", "backspace", "delete", "left", "right", "up", "down", "home", "end", "tab"];
        let (mut full, mut reused, mut inside) = (0, 0, 0);
        for (n, doc) in docs.into_iter().enumerate() {
            let len = doc.len();
            // Un ``` ou un `$$` tapé change le contexte de tout ce qui suit : sur le gros tableau, on les évite
            // pour que le test passe par l'intérieur du tableau.
            // Sur le gros tableau, on reste à l'intérieur de ses lignes : de quoi ne pas le couper en deux.
            let pieces: Vec<&str> = if n == 1 { vec!["a", "é", " ", "texte", "x y", "12", ","] } else { pieces.to_vec() };
            let keys: Vec<&str> = if n == 1 { vec!["left", "right", "up", "down", "end", "backspace"] } else { keys.to_vec() };
            shell.update(cx, |s, cx| s.editor.update(cx, |e, cx| e.load(doc, len / 2, cx)));
            cx.run_until_parked();
            shell.update_in(cx, |s, window, cx| window.focus(&s.editor.focus_handle(cx)));
            for _ in 0..steps {
                let size = shell.read_with(cx, |s, cx| s.editor.read(cx).text().len());
                match next(8) {
                    0 => {
                        let at = next(size + 1);
                        shell.update(cx, |s, cx| s.editor.update(cx, |e, cx| e.place_cursor(at, cx)));
                    }
                    1 | 2 => cx.simulate_keystrokes(keys[next(keys.len())]),
                    _ => cx.simulate_input(pieces[next(pieces.len())]),
                }
                cx.run_until_parked();
            }
            (full, reused, inside) = shell.read_with(cx, |s, cx| s.editor.read(cx).layouts());
        }
        println!("mises en page : {full} complètes, {reused} reprises, dont {inside} dans un très gros tableau");
        assert!(inside > 20 || steps < 200, "peu de reprises dans un très gros tableau : {inside}");
        fs::remove_dir_all(&root).unwrap();
    }

    /// Coût d'une note qui contient un tableau Markdown de n lignes.
    #[gpui::test]
    #[ignore]
    fn md_table_scale(cx: &mut TestAppContext) {
        let root = std::env::temp_dir().join(format!("bref-mdscale-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let _config = isolated_config(&root);
        cx.update(bind_keys);
        cx.update(init_fonts);
        let (shell, cx) = cx.add_window_view({
            let root = root.clone();
            |window, cx| Shell::new(Some(root), Vec::new(), window, cx)
        });
        cx.run_until_parked();
        for n in std::env::var("BREF_ROWS").ok().map_or(vec![500usize, 1000, 2000, 5000, 10000, 20000, 50000], |v| v.split(',').filter_map(|n| n.parse().ok()).collect()) {
            let rows: String = (0..n).map(|i| format!("| {i} | Produit {} | {} | FR | note, {i} |\n", i % 977, i % 99)).collect();
            let file = root.join(format!("t{n}.md"));
            fs::write(&file, format!("| id | produit | qte | pays | commentaire |\n| --- | --- | --- | --- | --- |\n{rows}")).unwrap();
            let start = Instant::now();
            shell.update_in(cx, |s, window, cx| {
                s.open_note(&file, cx);
                window.focus(&s.editor.focus_handle(cx));
            });
            cx.run_until_parked();
            let opened = start.elapsed();
            // Au milieu du tableau, loin des lignes qui fixent ses colonnes.
            shell.update(cx, |s, cx| s.editor.update(cx, |e, cx| {
                let at = e.text().len() / 2;
                e.place_cursor(at, cx)
            }));
            cx.run_until_parked();
            let start = Instant::now();
            cx.simulate_input("a");
            cx.run_until_parked();
            println!("{n:6} lignes : ouverture {opened:?} · une frappe {:?}", start.elapsed());
            let start = Instant::now();
            for _ in 0..20 {
                cx.simulate_event(gpui::ScrollWheelEvent {
                    position: point(px(700.), px(300.)),
                    delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-40.))),
                    ..Default::default()
                });
                cx.run_until_parked();
            }
            println!("         défilement : {:?} par geste", start.elapsed() / 20);
            if std::env::var("BREF_TYPING").is_ok() {
                let start = Instant::now();
                for _ in 0..30 {
                    cx.simulate_input("a");
                    cx.run_until_parked();
                }
                println!("         30 frappes de suite : {:?} par frappe", start.elapsed() / 30);
                let start = Instant::now();
                for _ in 0..30 {
                    cx.simulate_input(" ");
                    cx.run_until_parked();
                }
                println!("         30 espaces de suite : {:?} par espace", start.elapsed() / 30);
                let start = Instant::now();
                cx.simulate_keystrokes("secondary-z");
                cx.run_until_parked();
                println!("         annuler : {:?}", start.elapsed());
                let start = Instant::now();
                shell.update(cx, |s, cx| s.flush(cx));
                println!("         enregistrement de la note ({} Mo) : {:?}", fs::metadata(&file).map_or(0, |m| m.len()) >> 20, start.elapsed());
            }
        }
        fs::remove_dir_all(&root).unwrap();
    }

    /// Frappes dans une grosse note, pour un profileur : `cargo test --release --locked -- --ignored profile_typing`.
    #[gpui::test]
    #[ignore]
    fn profile_typing(cx: &mut TestAppContext) {
        let root = std::env::temp_dir().join(format!("bref-profile-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let _config = isolated_config(&root);
        cx.update(bind_keys);
        cx.update(init_fonts);
        let (shell, cx) = cx.add_window_view({
            let root = root.clone();
            |window, cx| Shell::new(Some(root), Vec::new(), window, cx)
        });
        cx.run_until_parked();
        // `BREF_PROFILE_LINES=n` : longueur de la note (5000 par défaut).
        let lines = std::env::var("BREF_PROFILE_LINES").ok().and_then(|n| n.parse().ok()).unwrap_or(5000);
        let big: String = if std::env::var("BREF_PROFILE_TABLE").is_ok() {
            // Un tableau de `lines` lignes.
            let rows: String = (0..lines)
                .map(|j| format!("| Ligne {j} | texte moyen avec **gras** et `code` {j} | {}| [[Note {}]] | x | y |\n", "mot ".repeat(j % 30), j))
                .collect();
            format!("| A | B | C | D | E | F |\n| --- | --- | --- | --- | --- | --- |\n{rows}")
        } else {
            (0..lines).map(|j| format!("Ligne {j} avec **gras**, `code`, [[Note {}]] et #tag{} -> fin.\n", j % 2000, j % 12)).collect()
        };
        shell.update(cx, |s, cx| s.editor.update(cx, |e, cx| e.load(format!("# Grosse\n\n{big}"), 100, cx)));
        cx.run_until_parked();
        for _ in 0..300 {
            cx.simulate_input("a");
            cx.run_until_parked();
        }
        fs::remove_dir_all(&root).unwrap();
    }

    /// Coffre synthétique, toujours le même : `notes` notes de 30 lignes réparties dans 40
    /// dossiers, liées entre elles et étiquetées, plus `Grosse.md`, une note de 5 000 lignes.
    fn synthetic_vault(root: &Path, notes: usize) {
        for i in 0..notes {
            let dir = root.join(format!("d{}", i % 40));
            fs::create_dir_all(&dir).unwrap();
            let body: String = (0..30).map(|j| format!("Ligne {j} vers [[Note {}]] #tag{}\n", (i + j) % notes, j % 12)).collect();
            fs::write(dir.join(format!("Note {i}.md")), format!("# Note {i}\n\n{body}")).unwrap();
        }
        let big: String =
            (0..5000).map(|j| format!("Ligne {j} avec **gras**, `code`, [[Note {}]] et #tag{} -> fin.\n", j % notes, j % 12)).collect();
        fs::write(root.join("Grosse.md"), format!("# Grosse\n\n{big}")).unwrap();
    }

    /// Mémoire résidente du processus en Mo (Linux seulement).
    fn resident_mb() -> Option<u64> {
        let status = fs::read_to_string("/proc/self/status").ok()?;
        let kb: u64 = status.lines().find_map(|l| l.strip_prefix("VmRSS:"))?.trim().trim_end_matches("kB").trim().parse().ok()?;
        Some(kb / 1024)
    }

    /// Temps pour un coffre de 1 000, 5 000 puis 20 000 notes : scan, nouveau scan sans
    /// changement, empreinte, démarrage de l'app, ouverture et frappe dans une note de
    /// 5 000 lignes. `BREF_BENCH_NOTES=500,3000` change les tailles. Hors de la suite
    /// courante : `cargo test --release --locked -- --ignored --nocapture bench_scale`.
    #[gpui::test]
    #[ignore]
    fn bench_scale(cx: &mut TestAppContext) {
        let sizes: Vec<usize> = std::env::var("BREF_BENCH_NOTES")
            .map(|v| v.split(',').filter_map(|n| n.trim().parse().ok()).collect())
            .unwrap_or_else(|_| vec![1000, 5000, 20000]);
        cx.update(bind_keys);
        cx.update(init_fonts);
        let ms = |d: Duration| format!("{:.1}", d.as_secs_f64() * 1000.);
        let mut rows = Vec::new();
        for notes in sizes {
            let root = std::env::temp_dir().join(format!("bref-scale-{notes}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            synthetic_vault(&root, notes);
            let _config = isolated_config(&root);
            vault::save_layout("tree split 260 0");

            let start = Instant::now();
            let scanned = vault::scan(&root);
            let scan = start.elapsed();
            let start = Instant::now();
            let again = vault::rescan(&root, &scanned.0);
            let rescan = start.elapsed();
            assert_eq!(again.0.len(), notes + 1);
            let start = Instant::now();
            vault::fingerprint(&root);
            let fingerprint = start.elapsed();

            let start = Instant::now();
            let (shell, cx) = cx.add_window_view({
                let root = root.clone();
                |window, cx| Shell::new(Some(root), Vec::new(), window, cx)
            });
            cx.run_until_parked();
            let startup = start.elapsed();
            let start = Instant::now();
            shell.update(cx, |s, cx| s.open_note(&root.join("Grosse.md"), cx));
            cx.run_until_parked();
            let open = start.elapsed();
            let start = Instant::now();
            for _ in 0..20 {
                cx.simulate_input("a");
                cx.run_until_parked();
            }
            let typing = start.elapsed() / 20;
            let memory = resident_mb().map_or("?".into(), |m| m.to_string());
            rows.push(format!("| {notes} | {} | {} | {} | {} | {} | {} | {memory} |", ms(scan), ms(rescan), ms(fingerprint), ms(startup), ms(open), ms(typing)));
            // La fenêtre et son watcher partent avant le coffre suivant.
            cx.update(|_, cx| cx.windows().iter().for_each(|w| { w.update(cx, |_, window, _| window.remove_window()).ok(); }));
            cx.run_until_parked();
            fs::remove_dir_all(&root).unwrap();
        }
        println!("\n| notes | scan (ms) | rescan (ms) | empreinte (ms) | démarrage (ms) | ouverture 5000 l. (ms) | frappe 5000 l. (ms) | mémoire (Mo) |");
        println!("|---|---|---|---|---|---|---|---|");
        rows.iter().for_each(|row| println!("{row}"));
    }

    /// Temps par geste sur un gros coffre et une grosse note. Hors de la suite
    /// courante : `cargo test --release --locked -- --ignored --nocapture bench`.
    #[gpui::test]
    #[ignore]
    fn bench(cx: &mut TestAppContext) {
        let root = std::env::temp_dir().join(format!("bref-bench-{}", std::process::id()));
        synthetic_vault(&root, 2000);
        let _config = isolated_config(&root);
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
        // Selon la longueur de la note : le prix d'une frappe est un prix fixe, plus un peu par ligne.
        for lines in [300, 1000] {
            let body: String = (0..lines).map(|j| format!("Ligne {j} avec **gras**, `code`, [[Note {}]] et #tag{} -> fin.\n", j % 2000, j % 12)).collect();
            shell.update(cx, |s, cx| s.editor.update(cx, |e, cx| e.load(format!("# Note\n\n{body}"), 100, cx)));
            cx.run_until_parked();
            time(&format!("frappe dans une note de {lines} lignes"), cx, &|cx| cx.simulate_input("a"));
        }
        // Le même geste sur une note courte : ce qui reste est le prix fixe d'une frappe.
        shell.update(cx, |s, cx| s.editor.update(cx, |e, cx| e.load("# Courte\n\nun peu de texte\n".into(), 20, cx)));
        cx.run_until_parked();
        time("frappe dans une note courte", cx, &|cx| cx.simulate_input("a"));
        // Un grand tableau, dont les cellules passent à la ligne.
        let rows: String = (0..400)
            .map(|j| format!("| Ligne {j} | texte moyen avec **gras** et `code` {j} | {}| [[Note {}]] | x | y |\n", "mot ".repeat(j % 30), j))
            .collect();
        let table = format!("# Tableau\n\n| A | B | C | D | E | F |\n| --- | --- | --- | --- | --- | --- |\n{rows}");
        shell.update(cx, |s, cx| s.editor.update(cx, |e, cx| e.load(table, 120, cx)));
        cx.run_until_parked();
        time("frappe dans un tableau de 400 lignes", cx, &|cx| cx.simulate_input("a"));
        time("flèche bas dans ce tableau", cx, &|cx| cx.simulate_keystrokes("down"));
        time("molette sur ce tableau", cx, &|cx| {
            cx.simulate_event(gpui::ScrollWheelEvent {
                position: point(px(700.), px(300.)),
                delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-40.))),
                ..Default::default()
            })
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
        // La recherche de nouvelle version est active tant qu'on ne l'a pas coupée.
        assert!(prefs.updates && !Prefs::parse("updates=off\n").updates && Prefs::parse("updates=on").updates);
        assert!(!Prefs::parse(&Prefs::parse("updates=off").to_text()).updates);
    }

    #[test]
    fn names_the_day() {
        assert_eq!(date_name((2026, 1, 2)), "2026-01-02");
        let (year, month, day) = today();
        assert!(year >= 2026 && (1..=12).contains(&month) && (1..=31).contains(&day));
        // La date locale est celle que donne le système (à minuit près, entre les deux lectures).
        #[cfg(unix)]
        if let Ok(out) = std::process::Command::new("date").arg("+%F").output() {
            let system = String::from_utf8_lossy(&out.stdout).trim().to_string();
            assert!(system == date_name((year, month, day)) || system == date_name(today()));
        }
    }

    #[test]
    fn reads_the_first_system_language() {
        // Ce que `defaults read -g AppleLanguages` imprime, avec et sans guillemets.
        assert_eq!(first_language("(\n    \"fr-FR\",\n    \"en-US\"\n)\n"), Some("fr-FR"));
        assert_eq!(first_language("(\n    en,\n    fr\n)"), Some("en"));
        assert_eq!(first_language("(\n)"), None);
        assert_eq!(first_language(""), None);
    }
}
