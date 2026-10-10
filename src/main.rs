// Pas de console derrière la fenêtre sous Windows.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod canvas;
mod diagram;
mod editor;
mod figure;
mod git;
mod graph;
mod grid;
mod history;
mod import;
mod kanban;
mod keys;
mod line;
mod markdown;
mod nav;
mod palette;
mod recall;
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
    Transformation,
    Window, WindowAppearance, WindowBounds, WindowBackgroundAppearance, WindowDecorations,
    WindowOptions, actions, div, ease_out_quint, hsla, img, percentage, point, prelude::*, px, rgb, size, svg,
};

use canvas::{Canvas, CanvasEvent};
use diagram::Diagram;
use editor::{Editor, EditorEvent};
use graph::{Graph, GraphEvent};
use history::{History, Place, Shown};
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
            "board.svg" => r#"<rect x="2.5" y="3" width="3" height="10" rx="0.8"/><rect x="6.5" y="3" width="3" height="7" rx="0.8"/><rect x="10.5" y="3" width="3" height="4.5" rx="0.8"/>"#,
            "info.svg" => r#"<circle cx="8" cy="8" r="5.5"/><path d="M8 7.5V11M8 5V5.2"/>"#,
            "alert.svg" => r#"<path d="M8 2.5L14 13H2ZM8 6.5V9.5M8 11.3V11.5"/>"#,
            "spinner.svg" => r#"<path d="M8 2.5A5.5 5.5 0 1 1 2.5 8"/>"#,
            "tree.svg" => r#"<path d="M3 3.5H10M3 3.5V12H6M3 7.75H6M8.5 7.75H13M8.5 12H13"/>"#,
            "clock.svg" => r#"<circle cx="8" cy="8" r="5.5"/><path d="M8 5V8L10 9.5"/>"#,
            "search.svg" => r#"<circle cx="7" cy="7" r="3.5"/><path d="M9.7 9.7L12.5 12.5"/>"#,
            "plus.svg" => r#"<path d="M8 3.5V12.5M3.5 8H12.5"/>"#,
            "help.svg" => {
                r#"<circle cx="8" cy="8" r="5.5"/><path d="M6.4 6.6A1.6 1.6 0 1 1 8 8.4V9.2M8 11.2V11.3"/>"#
            }
            "chevron-right.svg" => r#"<path d="M6.5 4.5L10 8L6.5 11.5"/>"#,
            "chevron-left.svg" => r#"<path d="M9.5 4.5L6 8L9.5 11.5"/>"#,
            "chevron-down.svg" => r#"<path d="M4.5 6.5L8 10L11.5 6.5"/>"#,
            "graph.svg" => {
                r#"<circle cx="4" cy="11.5" r="1.7"/><circle cx="11.5" cy="4.5" r="1.7"/><circle cx="12" cy="12" r="1.3"/><path d="M5.3 10.3L10.2 5.7M11.6 6.2L11.9 10.7"/>"#
            }
            "tag.svg" => r#"<path d="M6.5 3L5 13M11 3L9.5 13M3.5 6H13M3 10H12.5"/>"#,
            "trash.svg" => r#"<path d="M3 4.5H13M6.5 4.5V3H9.5V4.5M4.5 4.5L5 13H11L11.5 4.5M6.8 7V10.5M9.2 7V10.5"/>"#,
            // Une flèche vers un disque : une copie mise à l'abri, pas une boîte d'archives.
            "backup.svg" => {
                r#"<path d="M8 2V8M5.5 5.5L8 8L10.5 5.5"/><rect x="2.5" y="9.5" width="11" height="4" rx="1"/><path d="M11 11.5H11.2"/>"#
            }
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
            // Les icônes qu'on donne aux notes et aux dossiers.
            _ => match ICON_SET.iter().find(|(file, _)| *file == path) {
                Some((_, shapes)) => shapes,
                None => return Ok(None),
            },
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

/// Icônes à donner à une note, un fichier ou un dossier : le fichier dessiné, et ses formes
/// dans le carré de 16 des autres icônes. Le nom retenu est celui du fichier sans `i-` ni `.svg`.
pub const ICON_SET: &[(&str, &str)] = &[
    ("i-star.svg", r#"<path d="M8 2.5L9.7 6.1L13.6 6.6L10.7 9.3L11.5 13.2L8 11.3L4.5 13.2L5.3 9.3L2.4 6.6L6.3 6.1Z"/>"#),
    ("i-heart.svg", r#"<path d="M8 13C8 13 2.5 9.8 2.5 6.2A2.8 2.8 0 0 1 8 5A2.8 2.8 0 0 1 13.5 6.2C13.5 9.8 8 13 8 13Z"/>"#),
    ("i-bookmark.svg", r#"<path d="M4.5 2.5H11.5V13.5L8 11L4.5 13.5Z"/>"#),
    ("i-flag.svg", r#"<path d="M4 13.5V2.5M4 3H12L10 5.75L12 8.5H4"/>"#),
    ("i-pin.svg", r#"<path d="M8 14C8 14 3.5 9.8 3.5 6.5A4.5 4.5 0 0 1 12.5 6.5C12.5 9.8 8 14 8 14Z"/><circle cx="8" cy="6.5" r="1.5"/>"#),
    ("i-target.svg", r#"<circle cx="8" cy="8" r="5.5"/><circle cx="8" cy="8" r="2.5"/><path d="M8 8V8.1"/>"#),
    ("i-done.svg", r#"<circle cx="8" cy="8" r="5.5"/><path d="M5.5 8.2L7.3 10L10.5 6"/>"#),
    ("i-bolt.svg", r#"<path d="M9 2L4 9H8L7 14L12 7H8Z"/>"#),
    ("i-home.svg", r#"<path d="M2.5 7.5L8 3L13.5 7.5M4 6.5V13H12V6.5M6.8 13V9.5H9.2V13"/>"#),
    ("i-book.svg", r#"<path d="M3.5 3H11.5A1 1 0 0 1 12.5 4V13H4.5A1 1 0 0 1 3.5 12ZM3.5 11.5A1 1 0 0 1 4.5 10.5H12.5M6 5.5H10"/>"#),
    ("i-work.svg", r#"<rect x="2.5" y="5" width="11" height="8" rx="1"/><path d="M6 5V3.5H10V5M2.5 8.5H13.5"/>"#),
    ("i-school.svg", r#"<path d="M1.5 6.5L8 3.5L14.5 6.5L8 9.5ZM4.5 8V11C4.5 12 6 12.8 8 12.8S11.5 12 11.5 11V8"/>"#),
    ("i-calendar.svg", r#"<rect x="2.5" y="3.5" width="11" height="10" rx="1"/><path d="M2.5 6.5H13.5M5.5 2.5V4.5M10.5 2.5V4.5"/>"#),
    ("i-clock.svg", r#"<circle cx="8" cy="8" r="5.5"/><path d="M8 5V8L10 9.5"/>"#),
    ("i-inbox.svg", r#"<path d="M2.5 9L4.5 3.5H11.5L13.5 9V12.5H2.5ZM2.5 9H6A2 2 0 0 0 10 9H13.5"/>"#),
    ("i-archive.svg", r#"<path d="M2.5 3H13.5V6H2.5ZM3.5 6V13H12.5V6M6.5 8.5H9.5"/>"#),
    ("i-idea.svg", r#"<path d="M6 11.5H10M6.5 13.5H9.5M5.5 9.5A4 4 0 1 1 10.5 9.5C10 10 10 10.5 10 11.5H6C6 10.5 6 10 5.5 9.5Z"/>"#),
    ("i-rocket.svg", r#"<path d="M8 2C10.5 3.5 11.5 6 11 9.5L9.5 11H6.5L5 9.5C4.5 6 5.5 3.5 8 2ZM6.5 11L5 13.5M9.5 11L11 13.5M8 11V13.5"/><circle cx="8" cy="6.5" r="1"/>"#),
    ("i-person.svg", r#"<circle cx="8" cy="5.5" r="2.5"/><path d="M3 13.5C3 10.8 5.2 9.5 8 9.5S13 10.8 13 13.5"/>"#),
    ("i-chat.svg", r#"<path d="M2.5 3.5H13.5V10.5H7L4.5 13V10.5H2.5Z"/>"#),
    ("i-mail.svg", r#"<rect x="2.5" y="3.5" width="11" height="9" rx="1"/><path d="M2.5 4.5L8 8.5L13.5 4.5"/>"#),
    ("i-globe.svg", r#"<circle cx="8" cy="8" r="5.5"/><path d="M2.5 8H13.5M8 2.5C6 4.5 6 11.5 8 13.5M8 2.5C10 4.5 10 11.5 8 13.5"/>"#),
    ("i-plane.svg", r#"<path d="M14 2L2 7L6.5 9L8.5 14ZM6.5 9L14 2"/>"#),
    ("i-cart.svg", r#"<path d="M2 3H4L5.5 10H12L13.5 5H4.5"/><circle cx="6.5" cy="12.5" r="1"/><circle cx="11" cy="12.5" r="1"/>"#),
    ("i-money.svg", r#"<circle cx="8" cy="8" r="5.5"/><path d="M9.8 6.2C9.5 5.5 8.8 5.2 8 5.2C7 5.2 6.2 5.7 6.2 6.6C6.2 8.4 9.8 7.6 9.8 9.4C9.8 10.3 9 10.8 8 10.8C7.2 10.8 6.5 10.5 6.2 9.8M8 4V5.2M8 10.8V12"/>"#),
    ("i-chart.svg", r#"<path d="M3 13.5V2.5M3 13.5H13.5M5.5 11V8M8 11V5M10.5 11V7"/>"#),
    ("i-code.svg", r#"<path d="M5.5 4.5L2.5 8L5.5 11.5M10.5 4.5L13.5 8L10.5 11.5M9 3.5L7 12.5"/>"#),
    ("i-terminal.svg", r#"<rect x="2.5" y="3" width="11" height="10" rx="1"/><path d="M5 6.5L7 8L5 9.5M8.5 10H11"/>"#),
    ("i-data.svg", r#"<ellipse cx="8" cy="4" rx="4.5" ry="1.8"/><path d="M3.5 4V12C3.5 13 5.5 13.8 8 13.8S12.5 13 12.5 12V4M3.5 8C3.5 9 5.5 9.8 8 9.8S12.5 9 12.5 8"/>"#),
    ("i-tool.svg", r#"<path d="M13 4.8A3 3 0 0 1 9 7.6L4.6 12.5A1.2 1.2 0 0 1 3 11L7.6 6.6A3 3 0 0 1 10.6 2.6L8.9 4.3L9.3 6.2L11.2 6.6Z"/>"#),
    ("i-lock.svg", r#"<rect x="3.5" y="7" width="9" height="6.5" rx="1"/><path d="M5.5 7V5A2.5 2.5 0 0 1 10.5 5V7"/>"#),
    ("i-key.svg", r#"<circle cx="5.5" cy="10.5" r="2.5"/><path d="M7.3 8.7L13 3M11 5L12.5 6.5M9.5 6.5L10.5 7.5"/>"#),
    ("i-music.svg", r#"<path d="M6 12V4L12 3V11"/><circle cx="4.5" cy="12" r="1.5"/><circle cx="10.5" cy="11" r="1.5"/>"#),
    ("i-camera.svg", r#"<path d="M2.5 5.5H5L6 4H10L11 5.5H13.5V12.5H2.5Z"/><circle cx="8" cy="8.8" r="2"/>"#),
    ("i-gift.svg", r#"<rect x="2.5" y="6" width="11" height="2.5"/><path d="M3.5 8.5V13.5H12.5V8.5M8 6V13.5M8 6C8 6 6 6 5.5 4.5C5.2 3.4 7 2.8 8 6ZM8 6C8 6 10 6 10.5 4.5C10.8 3.4 9 2.8 8 6Z"/>"#),
    ("i-cup.svg", r#"<path d="M3.5 6H11V10.5A2.5 2.5 0 0 1 8.5 13H6A2.5 2.5 0 0 1 3.5 10.5ZM11 7H12A1.5 1.5 0 0 1 12 10H11M5.5 2.5V4M8.5 2.5V4"/>"#),
    ("i-health.svg", r#"<path d="M6.5 2.5H9.5V6.5H13.5V9.5H9.5V13.5H6.5V9.5H2.5V6.5H6.5Z"/>"#),
    ("i-leaf.svg", r#"<path d="M3 13C3 7 6 3 13 3C13 10 9 13 3 13ZM3 13L9 7"/>"#),
    ("i-sun.svg", r#"<circle cx="8" cy="8" r="2.8"/><path d="M8 1.8V3.2M8 12.8V14.2M1.8 8H3.2M12.8 8H14.2M3.6 3.6L4.6 4.6M11.4 11.4L12.4 12.4M3.6 12.4L4.6 11.4M11.4 4.6L12.4 3.6"/>"#),
    ("i-moon.svg", r#"<path d="M13 9.5A5.5 5.5 0 1 1 6.5 3A4.5 4.5 0 0 0 13 9.5Z"/>"#),
];

/// Le logo de l'app, discret : les deux fiches, à la couleur d'accent.
pub fn logo(t: Theme) -> gpui::Svg {
    svg().path("logo.svg").size(px(15.)).flex_none().text_color(t.accent.opacity(0.85))
}

actions!(app, [SyncVault, SplitPane, FocusLeft, FocusRight, ClosePane, GoBack, GoForward, SearchVault, Today, Outline, OpenPalette, NewNote, NewDiagram, OpenVault, CopyAll, ToggleHelp, CloseHelp, ChooseTheme, ZoomIn, ZoomOut, ZoomReset, Quit]);

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

/// Ton d'un message d'état : il choisit son icône, et s'il part de lui-même.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tone {
    Info,
    Done,
    Failed,
    /// Opération en cours : reste affiché jusqu'au message qui en donne l'issue.
    Busy,
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
        match (self.shown_sheet(), &self.listing, &self.drawing) {
            (Some(sheet), ..) => window.focus(&sheet.focus_handle(cx)),
            (None, Some(listing), _) => window.focus(&listing.filter.focus_handle(cx)),
            (None, None, Some((_, canvas))) if shown => window.focus(&canvas.focus_handle(cx)),
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
            window.focus(&self.editor.focus_handle(cx));
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
        let names: Vec<String> = self.notes.iter().map(|n| n.name.clone()).chain(self.notes.iter().flat_map(|n| n.aliases.clone())).collect();
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
        let mut notes: Vec<&Note> = self.notes.iter().collect();
        notes.sort_by_cached_key(|n| {
            self.recent.iter().position(|p| *p == n.path).unwrap_or(usize::MAX)
        });
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
                (note.tags, note.links) = markdown::index(&text);
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
            (note.tags, note.links) = markdown::index(&text);
            self.swapped.push((path, std::mem::replace(&mut note.body, Arc::from(text.as_str())), text));
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
                    (note.tags, note.links) = markdown::index(&old);
                    note.body = old;
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
                let old = self.path.clone();
                // Le graphe ne change que si la note est nouvelle, renommée ou liée autrement.
                let known = self.notes.iter().find(|n| Some(&n.path) == old.as_ref());
                if known.is_none_or(|n| n.path != path || n.links != links) {
                    self.graph_stale = true;
                }
                self.notes.retain(|n| Some(&n.path) != old.as_ref() && n.path != path);
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
                self.menu = Some(nav::Menu { at: *at, target: None, text: Some(link.clone()), commented: *commented });
                cx.notify();
            }
        }
    }

    /// Suit un lien de la note : l'adresse dans le navigateur, le tag dans la palette, la note.
    pub fn follow(&mut self, link: &Link, window: &mut Window, cx: &mut Context<Self>) {
        match link {
            Link::Url(url) => cx.open_url(url),
            Link::Tag(tag) => self.open_palette(&format!("#{tag}"), window, cx),
            Link::Wiki(name) => self.open_wiki(name, cx),
        }
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
        if self.vault.is_some() {
            self.open_wiki(&date_name(today()), cx);
        }
    }

    /// La note que désigne un `[[lien]]` : par son nom d'abord, sinon par un alias de son
    /// en-tête YAML.
    fn wiki_path(&self, name: &str) -> Option<PathBuf> {
        let wanted = name.to_lowercase();
        let named = self.notes.iter().find(|n| n.name.to_lowercase() == wanted);
        named.or_else(|| self.notes.iter().find(|n| n.answers(&wanted))).map(|note| note.path.clone())
    }

    fn open_wiki(&mut self, name: &str, cx: &mut Context<Self>) {
        match self.wiki_path(name) {
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
        cx.subscribe_in(&palette, window, |this, palette, event, window, cx| {
            this.palette = None;
            window.focus(&this.editor.focus_handle(cx));
            let aside = palette.read(cx).aside;
            match event {
                PaletteEvent::Open(path) if aside => this.open_aside(path, window, cx),
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
                PaletteEvent::Comments => this.open_comments(window, cx),
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
                k(&SplitPane, tr("Second pane, side by side: opens on the palette, to choose what goes there", "Second pane, côte à côte : s'ouvre sur la palette, pour choisir ce qui y va")),
                k(&FocusLeft, tr("Go to the left pane", "Aller au pane de gauche")),
                k(&FocusRight, tr("Go to the right pane", "Aller au pane de droite")),
                k(&ClosePane, tr("Close the pane that has the focus", "Fermer le pane qui a la saisie")),
                k(&palette::ConfirmAside, tr("In the palette: open the chosen note in the other pane", "Dans la palette : ouvrir la note choisie dans l'autre pane")),
                t(&format!("{MOD}+{shift}+{click}"), tr("Open a [[link]] in the other pane", "Ouvrir un [[lien]] dans l'autre pane")),
                t(tr("Drag from the tree", "Glisser depuis l'arbre"), tr("Onto a pane: open the file there; onto the right half of a single pane: open a second one", "Sur un pane : y ouvrir le fichier ; sur la moitié droite du seul pane : en ouvrir un second")),
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
                k(&sheet::InsertBelow, tr("Table: insert a row below", "Tableau : insérer une ligne dessous")),
                k(&sheet::InsertAbove, tr("Table: insert a row above", "Tableau : insérer une ligne dessus")),
                k(&sheet::DeleteRows, tr("Table: remove the selected rows", "Tableau : retirer les lignes sélectionnées")),
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
                k(&ZoomIn, tr("Bigger text", "Texte plus grand")),
                k(&ZoomOut, tr("Smaller text", "Texte plus petit")),
                k(&ZoomReset, tr("Default text size", "Taille du texte d'origine")),
            ],
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
                t(tr("Double / triple click", "Double / triple clic"), tr("Select a word / a line; drag to extend by words / lines", "Sélectionner un mot / une ligne ; glisser étend par mots / par lignes")),
                t("```mermaid", tr("Diagram, under its block", "Diagramme, sous son bloc")),
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
                k(&editor::ToggleTask, tr("Check / uncheck a task or a [ ] box; in a table cell, add one", "Cocher / décocher une tâche ou une case [ ] ; dans une cellule, en ajouter une")),
                k(&editor::Bold, tr("Bold", "Gras")),
                k(&editor::Italic, tr("Italic", "Italique")),
                k(&editor::SelectNext, tr("Several cursors: select the word, then its next occurrence (Esc: back to one)", "Plusieurs curseurs : prendre le mot, puis son occurrence suivante (Échap : un seul)")),
                t(&format!("Alt+{click}"), tr("Add a cursor", "Ajouter un curseur")),
                k(&editor::InsertLink, tr("Link: [text](address) from the selection; on a link, change its address", "Lien : [texte](adresse) depuis la sélection ; sur un lien, changer son adresse")),
                k(&editor::Comment, tr("Comment the selection or the line; on a comment, resolve it", "Commenter la sélection ou la ligne ; sur un commentaire, le résoudre")),
                p(tr("comments", "commentaires"), tr("Comments: show their panel, jump to one", "Commentaires : montrer leur panneau, aller à l'un d'eux")),
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
        keys::filter(help_sections(&self.bound, &self.changes), &query)
    }

    fn render_help(&self, cx: &mut Context<Self>) -> impl IntoElement {
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
                .text_color(t.dim)
                .hover(|s| s.bg(t.border).text_color(t.text))
                .child(text)
        };
        let hint = div().absolute().left_0().top_0().text_color(t.dim).child(tr("Search the shortcuts…", "Chercher un raccourci…"));
        let find = div()
            .flex_none()
            .px_5()
            .py_3()
            .border_b_1()
            .border_color(t.border)
            .flex()
            .items_center()
            .gap_2()
            .child(svg().path("search.svg").size(px(14.)).flex_none().text_color(t.dim))
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
                        None => div().text_size(px(12.)).text_color(t.dim).child(tr(
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
                        .child(div().flex().gap_3().child(keys).child(div().flex_1().min_w_0().text_color(t.dim).child(row.effect)))
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
                                d.child(div().text_color(t.dim).child(tr("No shortcut matches.", "Aucun raccourci ne correspond.")))
                            }),
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

impl Shell {
    /// Dessine le pane `pane` (l'appelant l'a rendu actif le temps du dessin) : ce qu'il montre,
    /// et le panneau de ses commentaires. `active` : c'est lui qui a la saisie.
    /// Les notes que le coffre remet sous les yeux aujourd'hui (voir `recall::recall`).
    // ponytail: recalculé à chaque rendu d'une page vide (un hachage par note, 5 000 notes :
    // une fraction de milliseconde) ; le garder d'un rendu à l'autre si de bien plus gros
    // coffres le font sentir.
    fn recalled(&self) -> Vec<recall::Recalled> {
        recall::recall(&self.notes, today(), std::time::SystemTime::now())
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
                .text_color(t.dim)
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
                .child(div().mb_1().text_size(px(11.)).text_color(t.dim.opacity(0.7)).child(tr("READ AGAIN", "À RELIRE")))
                .children(rows)
        });
        // Dans la colonne du texte, sous sa première ligne.
        div().absolute().top(px(76.)).left_0().right_0().flex().justify_center().child(
            div()
                .w_full()
                .max_w(editor::MAX_WIDTH + px(48.))
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
            nav::button("comments-toggle", "i-chat.svg", !self.comments_shut, t)
                .absolute()
                .bottom_4()
                .right(px(if kanban::is_board(self.editor.read(cx).text()) { 84. } else { 50. }))
                .size(px(30.))
                .rounded(px(8.))
                .occlude()
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
            nav::button("board-toggle", "board.svg", board, t)
                .absolute()
                .bottom_4()
                // Toujours à la même place, tableau ou texte : à gauche de la copie.
                .right(px(50.))
                .size(px(30.))
                .rounded(px(8.))
                .occlude()
                .on_click(cx.listener(|this, _, window, cx| this.toggle_board(window, cx)))
        });
        self.editor.update(cx, |e, _| e.corner = 1 + talk.is_some() as usize + boards as usize);
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
                        .child(nav::button(("resolve", i), "check.svg", false, t).on_click(cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.editor.update(cx, |e, cx| e.resolve_comment(at, cx));
                            window.focus(&this.editor.focus_handle(cx));
                        }))),
                )
                .child(div().text_color(if said.is_empty() { t.dim } else { t.text }).child(match said.is_empty() {
                    true => tr("(nothing written yet)", "(rien d'écrit pour l'instant)").to_string(),
                    false => said.to_string(),
                }))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.editor.update(cx, |e, cx| e.jump(inside, false, cx));
                    window.focus(&this.editor.focus_handle(cx));
                }))
        });
        let comments = (!said.is_empty() && !self.comments_shut).then(|| {
            let title = format!("{} ({})", tr("Comments", "Commentaires"), said.len());
            let shut = nav::button("comments-shut", "close.svg", false, t).on_click(cx.listener(|this, _, _, cx| {
                this.comments_shut = true;
                cx.notify();
            }));
            div()
                .id("comments")
                .w(px(270.))
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
                .child(div().flex().items_center().child(div().flex_1().text_color(t.dim).child(title)).child(shut))
                .children(cards)
        });
        let copy = icon_button("copy-all", if self.copied { "check.svg" } else { "copy.svg" }, t)
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
        }
        if self.drawing.as_ref().map(|(path, _)| path) != self.picture.as_ref() {
            self.drawing = None;
        }
        let note = div().flex_1().min_w_0().h_full().relative();
        if self.sheet.as_ref().map(|(path, _)| path) != self.picture.as_ref() {
            self.flush_sheet(cx);
            self.sheet = None;
        }
        let listing = self.listing.is_some().then(|| self.render_listing(cx));
        let note = match (&self.picture, &self.drawing) {
            _ if listing.is_some() => note.children(listing),
            _ if board => note.child(self.render_board(cx)).children(flip),
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
            .flex()
            .map(|d| if two && pane == 0 { d.flex_none().w(gpui::relative(ratio)) } else { d.flex_1() })
            // Un geste de souris dans un pane vaut pour lui, avant que ses vues ne le reçoivent.
            .capture_any_mouse_down(cx.listener(move |this, _, _, cx| {
                if this.active != pane && pane < this.panes.len() {
                    this.active = pane;
                    cx.notify();
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

/// Bouton rond à icône : contrôles de la fenêtre, copie de la note.
fn icon_button(id: &'static str, icon: &'static str, t: Theme) -> gpui::Stateful<gpui::Div> {
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
        if !cx.has_active_drag() {
            self.pane_aim = None;
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
                .child(div().text_size(px(12.)).text_color(t.dim).child(format!(
                    "{} · {}",
                    keys::of(cx, &OpenVault),
                    tr(
                        "you can create a new folder from the file dialog",
                        "un nouveau dossier peut être créé depuis la fenêtre de sélection",
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
            let banner = || card().absolute().bottom_3().left_16();
            // Messages d'état, empilés au-dessus du compteur de mots : un clic les retire.
            let toasts = self.toasts.iter().map(|toast| {
                let id = toast.id;
                let (icon, color) = match toast.tone {
                    Tone::Info => ("info.svg", t.dim),
                    Tone::Done => ("check.svg", rgb(0x3fa46a).into()),
                    Tone::Failed => ("alert.svg", rgb(0xd9483b).into()),
                    Tone::Busy => ("spinner.svg", t.dim),
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
                    .child(svg().path("close.svg").size(px(14.)).flex_none().text_color(t.dim))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            this.toasts.retain(|t| t.id != id);
                            cx.notify();
                        }),
                    )
            });
            let toasts = div().absolute().bottom(px(54.)).right_4().flex().flex_col().items_end().gap_2().children(toasts);
            body.flex()
                .child(self.render_nav(cx))
                .when(self.nav.panel != Panel::Full, |d| d.child(note).children(comments))
                .children(self.palette.clone())
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
        b("secondary-shift-l", &AlignLeft, e),
        b("secondary-shift-e", &AlignCenter, e),
        b("secondary-shift-r", &AlignRight, e),
        b("secondary-b", &Bold, e),
        b("secondary-i", &Italic, e),
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
        assert!(shell.read_with(cx, |s, _| !s.updating && s.told(Tone::Failed, "Mise à jour impossible")));
        shell.update(cx, |s, _| (s.update, s.toasts) = (None, Vec::new()));

        // Contrôle à la main : la palette le lance, puis la réponse dit que Bref est à jour,
        // que GitHub ne répond pas, ou montre la bannière.
        cx.simulate_keystrokes("secondary-p");
        cx.simulate_input("maintenant");
        cx.simulate_keystrokes("down enter");
        assert!(shell.read_with(cx, |s, _| s.told(Tone::Busy, "Recherche")));
        let version = |v: &str| Some(update::Release { version: v.into(), ..Default::default() });
        shell.update(cx, |s, cx| s.checked(version(env!("CARGO_PKG_VERSION")), cx));
        assert!(shell.read_with(cx, |s, _| s.update.is_none() && s.told(Tone::Done, "à jour") && !s.told(Tone::Busy, "")));
        shell.update(cx, |s, cx| s.checked(None, cx));
        assert!(shell.read_with(cx, |s, _| s.told(Tone::Failed, "GitHub")));
        shell.update(cx, |s, cx| s.checked(version("9.9.9"), cx));
        assert!(shell.read_with(cx, |s, _| !s.told(Tone::Busy, "") && s.update.as_ref().is_some_and(|r| r.version == "9.9.9")));
        shell.update(cx, |s, _| (s.update, s.toasts) = (None, Vec::new()));

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
        // La frappe va au champ de recherche, qui filtre les lignes : touches ou effet, sans
        // les accents.
        let rows = |cx: &mut gpui::VisualTestContext| {
            shell.read_with(cx, |s, cx| s.help_rows(cx).into_iter().flat_map(|(_, rows)| rows).collect::<Vec<_>>())
        };
        let all = rows(cx).len();
        cx.simulate_input("cOmmente");
        let found = rows(cx);
        assert!(!found.is_empty() && found.len() < all, "{found:?}");
        assert!(found.iter().all(|row| keys::fold(row.effect).contains("commente")), "{found:?}");
        cx.simulate_keystrokes("secondary-a");
        cx.simulate_input("maj+f");
        assert!(rows(cx).iter().any(|row| row.keys.ends_with("Maj+F")));
        cx.simulate_keystrokes("secondary-a");
        cx.simulate_input("zzz");
        assert!(rows(cx).is_empty());
        assert_eq!(text(cx), "# Idées\n\n", "rien n'est tapé dans la note");
        // Échap vide d'abord le champ, puis ferme le panneau.
        cx.simulate_keystrokes("escape");
        assert!(shell.read_with(cx, |s, _| s.help));
        assert_eq!(rows(cx).len(), all);
        cx.simulate_keystrokes("escape");
        assert!(!shell.read_with(cx, |s, _| s.help));
        cx.simulate_input("ok");
        assert_eq!(text(cx), "# Idées\n\nok");

        // Raccourcis modifiables : Entrée sur une ligne de l'aide attend des touches, les
        // suivantes deviennent le raccourci, tout de suite et dans le fichier `keys`.
        let (mode, panel) = shell.read_with(cx, |s, _| (s.nav.mode, s.nav.panel));
        let keys_of = |cx: &mut gpui::VisualTestContext, action: &str| {
            let row = rows(cx).into_iter().find(|row| row.action == Some(action)).unwrap();
            (row.keys, row.changed)
        };
        let editing = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, _| s.help_edit.as_ref().map(|edit| (edit.action, edit.issue.clone())));
        let graph = format!("{MOD}+G");
        cx.simulate_keystrokes("f1");
        cx.simulate_input("graphe des notes");
        assert_eq!(keys_of(cx, "nav::ShowGraph"), (graph.clone(), false));
        cx.simulate_keystrokes("down enter");
        assert_eq!(editing(cx), Some(("nav::ShowGraph", None)));
        // Refusées, avec la raison : une touche seule qui écrit, une touche réservée au texte.
        cx.simulate_keystrokes("a");
        assert!(matches!(editing(cx), Some((_, Some(KeyIssue::Refused(_))))));
        assert_eq!(shell.read_with(cx, |s, cx| s.help_find.as_ref().unwrap().read(cx).line.text.clone()), "graphe des notes", "la touche n'est pas tapée");
        cx.simulate_keystrokes("enter");
        assert!(matches!(editing(cx), Some((_, Some(KeyIssue::Refused(_))))));
        cx.simulate_keystrokes("secondary-shift-y");
        assert_eq!(editing(cx), None);
        assert_eq!(keys_of(cx, "nav::ShowGraph"), (format!("{MOD}+Maj+Y"), true));
        assert_eq!(vault::load_file("keys"), "nav::ShowGraph=secondary-shift-y\n");
        cx.simulate_keystrokes("escape escape");
        assert!(!shell.read_with(cx, |s, _| s.help));
        // Sans redémarrer : les anciennes touches ne font plus rien, les nouvelles agissent.
        cx.simulate_keystrokes("secondary-e");
        assert_eq!(shell.read_with(cx, |s, _| s.nav.mode), nav::Mode::Tree);
        cx.simulate_keystrokes("secondary-g");
        assert_eq!(shell.read_with(cx, |s, _| s.nav.mode), nav::Mode::Tree);
        cx.simulate_keystrokes("secondary-shift-y");
        assert_eq!(shell.read_with(cx, |s, _| s.nav.mode), nav::Mode::Graph);
        // Des touches déjà prises : la ligne dit par quoi, et rien n'est pris sans « remplacer ».
        cx.simulate_keystrokes("f1");
        cx.simulate_input("graphe des notes");
        cx.simulate_keystrokes("down enter secondary-p");
        assert_eq!(editing(cx), Some(("nav::ShowGraph", Some(KeyIssue::Taken(vec!["app::OpenPalette"])))));
        assert!(shell.read_with(cx, |s, _| s.palette.is_none()));
        assert_eq!(keys_of(cx, "nav::ShowGraph").0, format!("{MOD}+Maj+Y"));
        // Entrée remplace : l'autre action perd son raccourci, et la liste le montre.
        cx.simulate_keystrokes("enter");
        assert_eq!(editing(cx), None);
        assert_eq!(keys_of(cx, "nav::ShowGraph"), (format!("{MOD}+P"), true));
        cx.simulate_keystrokes("secondary-a");
        cx.simulate_input("filtrer par #tag");
        assert_eq!(keys_of(cx, "app::OpenPalette"), (String::new(), true));
        cx.simulate_keystrokes("escape escape secondary-e secondary-p");
        assert_eq!(shell.read_with(cx, |s, _| (s.nav.mode, s.palette.is_none())), (nav::Mode::Graph, true));
        // Retour arrière : aucun raccourci. Échap pendant la saisie n'annule qu'elle.
        cx.simulate_keystrokes("f1");
        cx.simulate_input("graphe des notes");
        cx.simulate_keystrokes("down enter backspace");
        assert_eq!(keys_of(cx, "nav::ShowGraph"), (String::new(), true));
        cx.simulate_keystrokes("enter escape");
        assert_eq!((editing(cx), shell.read_with(cx, |s, _| s.help)), (None, true));
        assert_eq!(rows(cx).len(), 1, "la recherche n'a pas bougé");
        // Un clic sur les touches attend aussi ; « défaut » remet celles d'origine.
        let at = cx.debug_bounds("key-nav::ShowGraph").unwrap().center();
        cx.simulate_click(at, gpui::Modifiers::none());
        assert_eq!(editing(cx), Some(("nav::ShowGraph", None)));
        cx.simulate_keystrokes("escape");
        let at = cx.debug_bounds("key-reset-nav::ShowGraph").unwrap().center();
        cx.simulate_click(at, gpui::Modifiers::none());
        assert_eq!(keys_of(cx, "nav::ShowGraph"), (graph, false));
        assert_eq!(vault::load_file("keys"), "app::OpenPalette=\n");
        // « Tout rétablir » : plus rien de changé.
        let at = cx.debug_bounds("keys-reset").unwrap().center();
        cx.simulate_click(at, gpui::Modifiers::none());
        assert!(shell.read_with(cx, |s, _| s.changes.is_empty()));
        assert_eq!(vault::load_file("keys"), "");
        cx.simulate_keystrokes("escape escape secondary-p");
        assert!(shell.read_with(cx, |s, _| s.palette.is_some()));
        cx.simulate_keystrokes("escape");
        shell.update(cx, |s, cx| {
            (s.nav.mode, s.nav.panel) = (mode, panel);
            cx.notify();
        });
        cx.run_until_parked();
        assert_eq!(text(cx), "# Idées\n\nok");

        // Thème : la liste des thèmes (icône du rail, palette), celui qu'on parcourt s'applique en
        // aperçu ; Échap revient au précédent, Entrée garde le choix et le mémorise.
        let look = |cx: &mut gpui::VisualTestContext| {
            shell.read_with(cx, |s, _| (s.prefs.theme.clone(), s.theme.bg))
        };
        let system = look(cx);
        cx.dispatch_action(ChooseTheme);
        cx.simulate_input("drac");
        assert_eq!(look(cx), ("Dracula".to_string(), Hsla::from(rgb(0x282a36))));
        cx.simulate_keystrokes("escape");
        assert_eq!(look(cx), system);
        cx.dispatch_action(ChooseTheme);
        cx.simulate_keystrokes("down down enter");
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
        // Une icône, choisie dans la grille du menu, suit la note qu'on renomme ; elle est
        // retenue dans un fichier du coffre.
        shell.update_in(cx, |s, window, cx| {
            s.menu = Some(nav::Menu { at: point(px(300.), px(200.)), target: Some(root.join("Test.md")), text: None, commented: false });
            s.menu_do(nav::Do::Icon, Some(root.join("Test.md")), window, cx)
        });
        cx.run_until_parked();
        let grid = shell.read_with(cx, |s, _| s.icon_pick.clone().unwrap().0 - point(s.nav.left, s.nav.left));
        cx.simulate_click(grid + point(px(5. + 30. + 14.), px(5. + 14.)), gpui::Modifiers::none());
        assert!(shell.read_with(cx, |s, _| s.icon_pick.is_none() && s.icons.get(&root.join("Test.md")).map(String::as_str) == Some("star")));
        shell.update(cx, |s, _| s.nav.sel = Some(root.join("Test.md")));
        cx.simulate_keystrokes("f2");
        cx.simulate_input("s");
        cx.simulate_keystrokes("enter");
        let renamed = fs::read_to_string(root.join("Tests.md")).unwrap();
        assert!(renamed.starts_with("# Tests\n1. un") && !root.join("Test.md").exists());
        assert!(shell.read_with(cx, |s, _| s.recent.contains(&root.join("Tests.md"))));
        assert_eq!(fs::read_to_string(root.join(".bref-icons")).unwrap(), "star\tTests.md\n");
        shell.update(cx, |s, cx| s.set_icon(root.join("Tests.md"), None, cx));
        assert_eq!(fs::read_to_string(root.join(".bref-icons")).unwrap(), "");
        // Les liens suivent, dans le fichier comme dans la note affichée.
        assert_eq!(text(cx), "# Courses\n\n- lait #maison\n[[Tests]]");
        assert!(fs::read_to_string(root.join("Courses.md")).unwrap().ends_with("[[Tests]]"));

        // Changer le titre renomme aussi la note : les liens ne suivent qu'une fois la
        // note quittée, pas à chaque titre intermédiaire.
        cx.simulate_keystrokes("secondary-p");
        cx.simulate_input("tests");
        // La note s'ouvre sur son corps : on remonte au titre pour le changer.
        cx.simulate_keystrokes("enter secondary-home end");
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
        assert!(shell.read_with(cx, |s, _| s.palette.is_none() && !s.told(Tone::Failed, "")));

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
        assert!(shell.read_with(cx, |s, _| s.told(Tone::Done, "Restauré : Jetée.md")));
        settle(cx);
        assert!(root.join("Jetée.md").is_file() && !root.join(".trash/Jetée.md").exists());
        assert!(shell.read_with(cx, |s, _| s.notes.iter().any(|n| n.name == "Jetée") && s.toasts.is_empty()));
        fs::remove_file(root.join("Jetée.md")).unwrap();
        settle(cx);

        // Remplacer dans tout le coffre, depuis la barre de recherche : le compte d'abord, puis
        // les notes sont réécrites ; la palette défait le tout, sauf ce qui a changé depuis.
        let here = shell.read_with(cx, |s, _| s.path.clone().unwrap());
        fs::write(root.join("Zoo A.md"), "# Zoo A\n\nle zèbre court").unwrap();
        fs::write(root.join("Zoo B.md"), "# Zoo B\n\nun zèbre, deux Zèbres").unwrap();
        settle(cx);
        shell.update(cx, |s, cx| s.open_note(&root.join("Zoo A.md"), cx));
        cx.simulate_keystrokes("secondary-h");
        cx.simulate_input("zèbre");
        cx.simulate_keystrokes("tab");
        cx.simulate_input("okapi");
        cx.simulate_keystrokes("secondary-shift-enter");
        assert!(shell.read_with(cx, |s, _| s.palette.is_some()));
        assert_eq!(fs::read_to_string(root.join("Zoo B.md")).unwrap(), "# Zoo B\n\nun zèbre, deux Zèbres");
        cx.simulate_keystrokes("enter");
        assert_eq!(fs::read_to_string(root.join("Zoo B.md")).unwrap(), "# Zoo B\n\nun okapi, deux okapis");
        assert_eq!(text(cx), "# Zoo A\n\nle okapi court");
        assert!(shell.read_with(cx, |s, _| s.palette.is_none() && s.told(Tone::Done, "Remplacés : 3 · notes : 2")));
        cx.simulate_keystrokes("escape");
        fs::write(root.join("Zoo B.md"), "# Zoo B\n\nautre chose").unwrap();
        cx.simulate_keystrokes("secondary-p");
        cx.simulate_input("annuler le remplacement");
        cx.simulate_keystrokes("down enter");
        assert_eq!(text(cx), "# Zoo A\n\nle zèbre court");
        assert_eq!(fs::read_to_string(root.join("Zoo B.md")).unwrap(), "# Zoo B\n\nautre chose");
        assert!(shell.read_with(cx, |s, _| s.told(Tone::Done, "Notes rétablies : 1 · modifiées depuis, laissées telles quelles : 1")));
        shell.update(cx, |s, cx| {
            s.toasts.clear();
            s.open_note(&here, cx)
        });
        fs::remove_file(root.join("Zoo A.md")).unwrap();
        fs::remove_file(root.join("Zoo B.md")).unwrap();
        settle(cx);

        // Page liste : un dossier ou un tag montré en table de ses notes, une colonne par clé de
        // leurs en-têtes ; un clic sur une colonne trie, la frappe filtre, un clic sur une
        // cellule la réécrit dans l'en-tête de la note, la note choisie s'ouvre.
        fs::create_dir_all(root.join("Livres")).unwrap();
        fs::write(root.join("Livres/Dune.md"), "---\nauteur: Herbert\nnote: 9\n---\n# Dune\n").unwrap();
        fs::write(root.join("Livres/Emma.md"), "---\nauteur: Austen\nnote: 10\n---\n# Emma\n\n#classique\n").unwrap();
        settle(cx);
        shell.update_in(cx, |s, window, cx| s.menu_do(nav::Do::List, Some(root.join("Livres")), window, cx));
        let listed = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.listed(cx).map(|(keys, rows)| (keys, rows.into_iter().map(|(_, cells)| cells[0].clone()).collect::<Vec<_>>())));
        assert_eq!(listed(cx), Some((vec!["Note".to_string(), "auteur".into(), "note".into()], vec!["Dune".to_string(), "Emma".into()])));
        shell.update(cx, |s, cx| s.sort_listing(1, cx));
        assert_eq!(listed(cx).unwrap().1, ["Emma", "Dune"]);
        shell.update(cx, |s, cx| s.sort_listing(2, cx));
        shell.update(cx, |s, cx| s.sort_listing(2, cx));
        assert_eq!(listed(cx).unwrap().1, ["Emma", "Dune"]);
        // Ce qu'on tape filtre, dans toutes les colonnes ou dans celle qu'on nomme ; Échap le vide.
        cx.run_until_parked();
        cx.simulate_input("auteur:her");
        assert_eq!(listed(cx).unwrap().1, ["Dune"]);
        cx.simulate_keystrokes("escape");
        cx.simulate_input("aus");
        assert_eq!(listed(cx).unwrap().1, ["Emma"]);
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert_eq!(listed(cx).unwrap().1, ["Emma", "Dune"]);
        // Un clic sur une cellule l'ouvre ; validée, seule cette clé change dans le fichier.
        let cell = cx.debug_bounds("list-cell-1-2").unwrap().center();
        cx.simulate_click(cell, gpui::Modifiers::none());
        cx.simulate_keystrokes("backspace");
        cx.simulate_input("7.5");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert_eq!(fs::read_to_string(root.join("Livres/Dune.md")).unwrap(), "---\nauteur: Herbert\nnote: 7.5\n---\n# Dune\n");
        // Triée par note à rebours : 10 avant 7.5 ; Échap dans une cellule ne change rien.
        assert_eq!(listed(cx).unwrap().1, ["Emma", "Dune"]);
        shell.update_in(cx, |s, window, cx| s.list_write(root.join("Livres/Emma.md"), "auteur".into(), window, cx));
        cx.simulate_input(" Jane");
        cx.simulate_keystrokes("escape");
        assert!(fs::read_to_string(root.join("Livres/Emma.md")).unwrap().contains("auteur: Austen\n"));
        // Un tag se liste comme un dossier.
        shell.update_in(cx, |s, window, cx| s.menu_do(nav::Do::List, Some(PathBuf::from("#classique")), window, cx));
        assert_eq!(listed(cx).unwrap().1, ["Emma"]);
        shell.update(cx, |s, cx| s.open_note(&root.join("Livres/Dune.md"), cx));
        assert!(shell.read_with(cx, |s, _| s.listing.is_none()) && text(cx).ends_with("# Dune\n"));
        shell.update(cx, |s, cx| s.open_note(&here, cx));
        fs::remove_dir_all(root.join("Livres")).unwrap();
        settle(cx);

        // Calendrier : Ctrl+Maj+J montre le mois ; les flèches changent de jour, Page bas de
        // mois, Entrée ouvre la note de ce jour, ou la crée.
        let before = shell.read_with(cx, |s, _| (s.path.clone(), s.nav.mode, s.nav.panel));
        cx.simulate_keystrokes("secondary-shift-j");
        assert!(shell.read_with(cx, |s, _| s.nav.mode == Mode::Calendar && s.nav.panel != Panel::Rail));
        shell.update(cx, |s, _| s.nav.day = (2026, 10, 30));
        cx.simulate_keystrokes("right down pagedown");
        assert_eq!(shell.read_with(cx, |s, _| s.nav.day), (2026, 12, 7));
        cx.simulate_keystrokes("pageup pageup up enter");
        assert_eq!(shell.read_with(cx, |s, _| s.nav.day), (2026, 9, 30));
        assert_eq!(text(cx), "# 2026-09-30\n\n");
        shell.update(cx, |s, cx| {
            (s.nav.mode, s.nav.panel) = (before.1, before.2);
            s.open_note(before.0.as_ref().unwrap(), cx);
        });
        // Quittée, la note du jour est enregistrée comme toute note qui a un titre.
        settle(cx);
        fs::remove_file(root.join("2026-09-30.md")).unwrap();
        settle(cx);

        // Terminal : la palette le propose ; les tests n'ouvrent aucune fenêtre, d'où le refus.
        cx.simulate_keystrokes("secondary-p");
        cx.simulate_input("terminal");
        cx.simulate_keystrokes("down enter");
        assert!(shell.read_with(cx, |s, _| s.palette.is_none() && s.told(Tone::Failed, "$TERMINAL")));
        shell.update(cx, |s, _| s.toasts.clear());

        // Raccourci de capture : la palette le propose. Hors de GNOME (et dans les tests) la
        // commande à lier est copiée, et le message dit quoi en faire.
        cx.simulate_keystrokes("secondary-p");
        cx.simulate_input("capture rapide");
        cx.simulate_keystrokes("down enter");
        cx.run_until_parked();
        assert!(shell.read_with(cx, |s, _| s.palette.is_none() && s.told(Tone::Done, "copiée")));
        assert!(cx.read_from_clipboard().and_then(|item| item.text()).is_some_and(|text| text.ends_with(" --capture")));
        shell.update(cx, |s, _| s.toasts.clear());

        // Sauvegarde : une archive datée de tout le coffre dans le dossier choisi (la fenêtre de
        // choix n'existe pas dans les tests) ; dans le coffre lui-même, elle est refusée.
        let out = root.with_file_name(format!("{}-sauvegardes", vault::stem(&root)));
        fs::create_dir_all(&out).unwrap();
        shell.update(cx, |s, cx| s.backup_to(out.clone(), cx));
        cx.run_until_parked();
        let saved = fs::read_dir(&out).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect::<Vec<_>>();
        assert!(saved.len() == 1 && saved[0].ends_with(&format!(" {}.tar.gz", date_name(today()))), "{saved:?}");
        assert!(shell.read_with(cx, |s, _| !s.told(Tone::Failed, "") && !s.told(Tone::Busy, "") && s.told(Tone::Done, "Sauvegarde enregistrée") && s.told(Tone::Done, &saved[0])));
        shell.update(cx, |s, cx| s.backup_to(root.join(".trash"), cx));
        cx.run_until_parked();
        assert!(shell.read_with(cx, |s, _| s.told(Tone::Failed, "hors du coffre")));
        shell.update(cx, |s, _| s.toasts.clear());
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
        assert!(shell.read_with(cx, |s, _| s.palette.is_none() && s.told(Tone::Info, "titres")));
        // Messages d'état : un clic retire celui qu'il vise, les autres partent seuls après quatre
        // secondes ; celui d'une opération en cours reste, jusqu'au message qui en donne l'issue.
        shell.update(cx, |s, cx| s.say(Tone::Busy, "En cours", cx));
        let view = cx.update(|window, _| window.viewport_size());
        cx.simulate_click(point(view.width - px(60.), view.height - px(70.)), gpui::Modifiers::none());
        assert!(shell.read_with(cx, |s, _| s.told(Tone::Info, "titres") && !s.told(Tone::Busy, "")));
        shell.update(cx, |s, cx| s.say(Tone::Busy, "En cours", cx));
        cx.executor().advance_clock(TOAST);
        cx.run_until_parked();
        assert!(shell.read_with(cx, |s, _| s.toasts.len() == 1 && s.told(Tone::Busy, "En cours")));
        shell.update(cx, |s, cx| s.say(Tone::Done, "Fini", cx));
        assert!(shell.read_with(cx, |s, _| s.toasts.len() == 1 && s.told(Tone::Done, "Fini")));
        cx.executor().advance_clock(TOAST);
        cx.run_until_parked();
        assert!(shell.read_with(cx, |s, _| s.toasts.is_empty()));

        // Liens : une adresse collée sur une sélection en fait un lien ; Ctrl+K sur un lien
        // sélectionne son adresse, qu'on retape ; Ctrl+K sur une sélection attend l'adresse.
        load(cx, "voir la doc ici", 5);
        cx.simulate_keystrokes("shift-right shift-right shift-right shift-right shift-right shift-right");
        cx.write_to_clipboard(ClipboardItem::new_string("https://a.b/c\n".into()));
        cx.simulate_keystrokes("secondary-v");
        assert_eq!(text(cx), "voir [la doc](https://a.b/c) ici");
        cx.simulate_keystrokes("secondary-k");
        cx.simulate_input("https://x.y");
        assert_eq!(text(cx), "voir [la doc](https://x.y) ici");
        cx.simulate_keystrokes("secondary-z secondary-z");
        assert_eq!(text(cx), "voir la doc ici");
        load(cx, "mot", 0);
        cx.write_to_clipboard(ClipboardItem::new_string("rien".into()));
        cx.simulate_keystrokes("shift-right shift-right shift-right secondary-k");
        cx.simulate_input("https://z.fr");
        assert_eq!(text(cx), "[mot](https://z.fr)");
        // Kanban : la palette crée un tableau, qui s'ouvre avec ses trois colonnes. Tout s'y
        // écrit sur place : « + » ouvre une carte, Entrée la valide et en rouvre une, Échap
        // arrête ; un double-clic réécrit une carte ou un titre. Déplacer ou cocher une carte
        // réécrit sa ligne. Rien ne fait quitter le tableau, sauf son bouton.
        // La palette est une ligne de saisie comme les autres : un mot s'y efface d'un coup.
        let word = if cfg!(target_os = "macos") { "alt" } else { "ctrl" };
        cx.simulate_keystrokes("secondary-p");
        cx.simulate_input("tableau zzz");
        cx.simulate_keystrokes(&format!("{word}-backspace"));
        cx.simulate_input("kanban");
        cx.simulate_keystrokes("down enter");
        cx.run_until_parked();
        let on_board = |cx: &mut gpui::VisualTestContext| cx.update(|window, cx| shell.read(cx).board_focus.is_focused(window));
        assert!(shell.read_with(cx, |s, cx| s.board_shown(cx)) && on_board(cx));
        assert_eq!(text(cx), "---\nkanban: true\n---\n# Tableau\n\n## À faire\n\n## En cours\n\n## Fait\n");
        shell.update_in(cx, |s, window, cx| s.board_write(kanban::Slot::New(0), window, cx));
        cx.simulate_input("Écrire");
        cx.simulate_keystrokes("enter");
        cx.simulate_input("Relire");
        cx.simulate_keystrokes("backspace enter escape");
        cx.run_until_parked();
        assert!(text(cx).contains("## À faire\n- [ ] Écrire\n- [ ] Relir\n\n## En cours") && shell.read_with(cx, |s, cx| s.board_field.is_none() && s.board_shown(cx)) && on_board(cx));
        shell.update_in(cx, |s, window, cx| s.board_write(kanban::Slot::Card(0, 1), window, cx));
        cx.simulate_input("e");
        cx.simulate_keystrokes("enter");
        shell.update_in(cx, |s, window, cx| s.board_write(kanban::Slot::Column(1), window, cx));
        cx.simulate_input(" de route");
        // Ouvrir un autre champ valide celui qu'on écrivait.
        shell.update_in(cx, |s, window, cx| s.board_write(kanban::Slot::NewColumn, window, cx));
        cx.simulate_input("Plus tard");
        cx.simulate_keystrokes("enter");
        assert!(text(cx).contains("- [ ] Relire\n\n## En cours de route\n") && text(cx).ends_with("## Fait\n\n## Plus tard\n"));
        // Une carte s'écrit dans une vraie ligne de saisie : un mot effacé d'un coup, le curseur
        // qui va au début, une sélection remplacée, tout copié puis collé à la fin.
        shell.update_in(cx, |s, window, cx| s.board_write(kanban::Slot::New(3), window, cx));
        cx.simulate_input("Un deux trois");
        cx.simulate_keystrokes(&format!("{word}-backspace home {word}-shift-right"));
        cx.simulate_input("Zéro");
        cx.simulate_keystrokes("secondary-a secondary-c end");
        cx.simulate_input("+");
        cx.simulate_keystrokes("secondary-v left delete enter escape");
        assert!(text(cx).ends_with("## Plus tard\n- [ ] Zéro deux +Zéro deux\n"), "{}", text(cx));
        shell.update(cx, |s, cx| s.board_edit(|text| kanban::retitle(text, (3, 0), ""), cx));
        // Une carte prise à la souris quitte sa place ; celle qu'elle prendrait suit le pointeur :
        // sous une colonne, son bas ; sur une carte, avant ou après elle selon la moitié visée.
        cx.run_until_parked();
        let held = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, _| s.board_drag);
        let (none, left) = (gpui::Modifiers::none(), gpui::MouseButton::Left);
        let (first, second) = (cx.debug_bounds("card-0-0").unwrap(), cx.debug_bounds("card-0-1").unwrap());
        let under = cx.debug_bounds("column-rest-2").unwrap().center();
        cx.simulate_mouse_down(first.center(), left, none);
        cx.simulate_mouse_move(first.center() + point(px(6.), px(6.)), left, none);
        cx.simulate_mouse_move(second.center() + point(px(0.), px(4.)), left, none);
        assert_eq!(held(cx), Some(((0, 0), (0, 2))));
        // La carte suivante est remontée à la place de celle qu'on tient.
        cx.simulate_mouse_move(first.center() - point(px(0.), px(4.)), left, none);
        assert_eq!(held(cx), Some(((0, 0), (0, 1))));
        cx.simulate_mouse_move(under, left, none);
        assert_eq!(held(cx), Some(((0, 0), (2, 0))));
        cx.simulate_mouse_up(under, left, none);
        cx.run_until_parked();
        assert_eq!(held(cx), None);
        // Le geste s'annule et se rétablit depuis le tableau, sans passer par le texte.
        cx.simulate_keystrokes("secondary-z");
        assert!(text(cx).contains("## À faire\n- [ ] Écrire\n- [ ] Relire\n") && on_board(cx));
        cx.simulate_keystrokes("secondary-shift-z");
        assert!(text(cx).contains("## Fait\n- [ ] Écrire\n"));
        shell.update(cx, |s, cx| s.board_edit(|text| kanban::tick(text, (2, 0)), cx));
        assert!(text(cx).contains("## À faire\n- [ ] Relire\n") && text(cx).contains("## Fait\n- [x] Écrire\n"));
        // Le bouton du coin, à gauche de la copie dans le texte comme dans le tableau.
        let corner = cx.update(|window, _| window.viewport_size());
        cx.simulate_click(point(corner.width - px(65.), corner.height - px(31.)), gpui::Modifiers::none());
        cx.run_until_parked();
        assert!(shell.read_with(cx, |s, cx| !s.board_shown(cx)) && !on_board(cx));
        // Un geste du tableau, une étape d'annulation.
        cx.simulate_keystrokes("secondary-z secondary-z");
        assert!(text(cx).contains("## À faire\n- [ ] Écrire\n- [ ] Relire\n"));
        cx.simulate_click(point(corner.width - px(65.), corner.height - px(31.)), gpui::Modifiers::none());
        cx.run_until_parked();
        assert!(shell.read_with(cx, |s, cx| s.board_shown(cx)));
        let board_file = shell.read_with(cx, |s, _| s.path.clone());
        shell.update(cx, |s, cx| s.open_note(&here, cx));
        settle(cx);
        // Écrire la clé dans le texte d'une note ne la change pas en tableau sous la frappe :
        // on finit sa ligne, et c'est le bouton du coin qui montre le tableau.
        cx.simulate_keystrokes("secondary-n");
        cx.run_until_parked();
        cx.simulate_input("---\nkanban: true\n---\n# Semaine\n\n## Lundi\n");
        cx.run_until_parked();
        assert!(shell.read_with(cx, |s, cx| !s.board_shown(cx) && kanban::is_board(s.editor.read(cx).text())));
        cx.simulate_click(point(corner.width - px(65.), corner.height - px(31.)), gpui::Modifiers::none());
        cx.run_until_parked();
        assert!(shell.read_with(cx, |s, cx| s.board_shown(cx)));
        settle(cx);
        shell.update(cx, |s, cx| s.open_note(&here, cx));
        settle(cx);
        for file in board_file.into_iter().chain([root.join("Tableau.md"), root.join("Semaine.md")]) {
            fs::remove_file(file).ok();
        }
        settle(cx);
        // Plusieurs curseurs : Ctrl+D prend le mot, puis ses occurrences suivantes ; la frappe,
        // l'effacement et les déplacements valent pour tous, un seul Ctrl+Z défait le tout,
        // Échap revient à un curseur. Alt+clic en pose un de plus.
        let cursors = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.editor.read(cx).cursors());
        load(cx, "un chat\nun chien\nun rat", 0);
        cx.simulate_keystrokes("secondary-d secondary-d secondary-d");
        assert_eq!(cursors(cx), 3);
        cx.simulate_keystrokes("secondary-d");
        assert_eq!(cursors(cx), 3);
        cx.simulate_input("le");
        assert_eq!(text(cx), "le chat\nle chien\nle rat");
        cx.simulate_keystrokes("end");
        cx.simulate_input("s !");
        cx.simulate_keystrokes("backspace backspace home delete");
        assert_eq!((text(cx), cursors(cx)), ("e chats\ne chiens\ne rats".to_string(), 3));
        cx.simulate_keystrokes("shift-end secondary-b");
        assert_eq!(text(cx), "**e chats**\n**e chiens**\n**e rats**");
        // Une étape d'annulation défait le geste pour les trois curseurs à la fois.
        cx.simulate_keystrokes("secondary-z");
        assert_eq!((text(cx), cursors(cx)), ("le chats\nle chiens\nle rats".to_string(), 1));
        load(cx, "un chat\nun chien\nun rat", 0);
        cx.simulate_keystrokes("secondary-d secondary-d escape");
        assert_eq!(cursors(cx), 1);
        cx.simulate_input("X");
        // Le dernier curseur posé reste : celui de la deuxième occurrence.
        assert_eq!(text(cx), "un chat\nX chien\nun rat");
        load(cx, "ab\ncd", 0);
        let middle = cx.update(|window, _| window.viewport_size());
        cx.simulate_mouse_down(point(middle.width / 2., middle.height / 2.), MouseButton::Left, gpui::Modifiers { alt: true, ..Default::default() });
        cx.simulate_mouse_up(point(middle.width / 2., middle.height / 2.), MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_input("X");
        assert_eq!((text(cx).matches('X').count(), cursors(cx)), (2, 2));
        cx.simulate_keystrokes("escape");
        // Commentaires : Ctrl+Maj+M commente la sélection (sans elle, la ligne, après sa puce),
        // la palette les liste et y mène, le même raccourci sur un commentaire le résout.
        load(cx, "- un mot ici\nfin", 5);
        cx.simulate_keystrokes("shift-right shift-right shift-right secondary-shift-m");
        cx.simulate_input("à revoir");
        assert_eq!(text(cx), "- un {==mot==}{>>à revoir<<} ici\nfin");
        cx.simulate_keystrokes("down secondary-shift-m");
        cx.simulate_input("ok");
        assert_eq!(text(cx), "- un {==mot==}{>>à revoir<<} ici\n{==fin==}{>>ok<<}");
        // Le panneau, à droite de la note : un clic sur une carte mène dans son commentaire,
        // sa coche le résout ; refermé, il revient quand la palette liste les commentaires.
        cx.run_until_parked();
        let view = cx.update(|window, _| window.viewport_size());
        cx.simulate_click(point(view.width - px(150.), px(125.)), gpui::Modifiers::none());
        assert!(shell.read_with(cx, |s, cx| s.editor.read(cx).position() == 17));
        shell.update(cx, |s, cx| s.editor.update(cx, |e, cx| e.resolve_comment(34, cx)));
        assert_eq!(text(cx), "- un {==mot==}{>>à revoir<<} ici\nfin");
        cx.simulate_keystrokes("secondary-z");
        // Le bouton du coin, à gauche de la copie, referme le panneau (et le rouvre).
        cx.run_until_parked();
        let corner = cx.update(|window, _| window.viewport_size());
        let closed = shell.read_with(cx, |s, _| s.comments_shut);
        cx.simulate_click(point(corner.width - px(270. + 65.), corner.height - px(31.)), gpui::Modifiers::none());
        assert!(!closed && shell.read_with(cx, |s, _| s.comments_shut));
        cx.simulate_keystrokes("secondary-p");
        cx.simulate_input("commentaires");
        cx.simulate_keystrokes("down enter");
        cx.simulate_input("revoir");
        cx.simulate_keystrokes("enter");
        assert!(shell.read_with(cx, |s, cx| s.palette.is_none() && !s.comments_shut && s.editor.read(cx).position() == 5));
        cx.simulate_keystrokes("secondary-shift-m");
        assert_eq!(text(cx), "- un mot ici\n{==fin==}{>>ok<<}");
        cx.simulate_keystrokes("secondary-z");
        assert_eq!(text(cx), "- un {==mot==}{>>à revoir<<} ici\n{==fin==}{>>ok<<}");
        load(cx, "rien", 0);
        cx.simulate_keystrokes("secondary-p");
        cx.simulate_input("commentaires");
        cx.simulate_keystrokes("down enter");
        assert!(shell.read_with(cx, |s, _| s.palette.is_none() && s.told(Tone::Info, "commentaires")));
        shell.update(cx, |s, _| s.toasts.clear());
        // `@` propose des jours : celui qu'on choisit devient un lien vers sa note du jour.
        // Une adresse e-mail n'ouvre pas la liste.
        let tomorrow = date_name(markdown::day_of(markdown::day_number(today()) + 1));
        load(cx, "", 0);
        cx.simulate_input("voir @dem");
        cx.simulate_keystrokes("enter");
        assert_eq!(text(cx), format!("voir [[{tomorrow}]]"));
        load(cx, "", 0);
        cx.simulate_input("@2026-11-02");
        cx.simulate_keystrokes("enter");
        assert_eq!(text(cx), "[[2026-11-02]]");
        load(cx, "", 0);
        cx.simulate_input("a@dem");
        cx.simulate_keystrokes("enter");
        assert_eq!(text(cx), "a@dem\n");
        // Sur une adresse nue, Ctrl+K lui fait une place pour son texte.
        load(cx, "voir https://a.b/c ici", 9);
        cx.simulate_keystrokes("secondary-k");
        cx.simulate_input("la doc");
        assert_eq!(text(cx), "voir [la doc](https://a.b/c) ici");
        // Clic droit dans la note : son menu ; Échap le referme. Ses entrées font ce que fait
        // le clavier, et celles d'un lien agissent sur le lien visé.
        let view = cx.update(|window, _| window.viewport_size());
        cx.simulate_mouse_down(point(view.width / 2., view.height / 2.), MouseButton::Right, gpui::Modifiers::none());
        assert!(shell.read_with(cx, |s, _| s.menu.as_ref().is_some_and(|m| m.text == Some(None))));
        cx.simulate_keystrokes("escape");
        assert!(shell.read_with(cx, |s, _| s.menu.is_none()));
        cx.simulate_mouse_down(point(view.width / 2., view.height / 2.), MouseButton::Right, gpui::Modifiers::none());
        cx.write_to_clipboard(ClipboardItem::new_string(" !".into()));
        shell.update_in(cx, |s, window, cx| s.menu_do(nav::Do::Paste, None, window, cx));
        cx.run_until_parked();
        // Collé là où le clic a posé le curseur.
        assert!(text(cx).contains(" !") && text(cx).replace(" !", "") == "voir [la doc](https://a.b/c) ici");
        let link = Link::Url("https://a.b/c".into());
        shell.update_in(cx, |s, window, cx| {
            s.menu = Some(nav::Menu { at: point(px(0.), px(0.)), target: None, text: Some(Some(link)), commented: false });
            s.menu_do(nav::Do::CopyAddress, None, window, cx)
        });
        assert_eq!(cx.read_from_clipboard().and_then(|c| c.text()).as_deref(), Some("https://a.b/c"));
        // Sans sélection, et une adresse dans le presse-papiers : il ne manque que le texte.
        load(cx, "", 0);
        cx.write_to_clipboard(ClipboardItem::new_string("https://a.b".into()));
        cx.simulate_keystrokes("secondary-k");
        cx.simulate_input("ici");
        assert_eq!(text(cx), "[ici](https://a.b)");

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
        // Expression régulière : Alt+R ; le remplacement reprend ses groupes.
        load(cx, "le chat, le chien", 0);
        cx.simulate_keystrokes("secondary-h");
        cx.simulate_input(r"le (\w+)");
        assert_eq!(finding(cx), Some((r"le (\w+)".into(), 0, 0)));
        cx.simulate_keystrokes("alt-r");
        assert_eq!(finding(cx), Some((r"le (\w+)".into(), 1, 2)));
        cx.simulate_keystrokes("tab");
        cx.simulate_input("$1 !");
        cx.simulate_keystrokes("enter");
        assert_eq!(text(cx), "chat !, le chien");
        cx.simulate_keystrokes("secondary-enter");
        assert_eq!(text(cx), "chat !, chien !");
        cx.simulate_keystrokes("alt-r escape");
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
        assert!(shell.read_with(cx, |s, _| s.images.contains(&file.with_extension("png")) && !s.told(Tone::Failed, "")));

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
        // La cellule qu'on écrit est une ligne de saisie : mot sélectionné et remplacé, espace effacé.
        let word = if cfg!(target_os = "macos") { "alt" } else { "ctrl" };
        cx.simulate_keystrokes("down right");
        cx.simulate_input("zéro 2");
        cx.simulate_keystrokes(&format!("home {word}-shift-right"));
        cx.simulate_input("3");
        cx.simulate_keystrokes("delete enter");
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

        // Précédent / suivant : l'historique de ce qui a été affiché, comme dans un navigateur.
        let (hist_a, hist_b, hist_csv) = (root.join("Hist A.md"), root.join("Hist B.md"), root.join("hist.csv"));
        let long: String = (0..200).map(|i| format!("ligne {i}\n")).collect();
        fs::write(&hist_a, format!("# Hist A\n\n{long}\n[[Hist B]]\n")).unwrap();
        fs::write(&hist_b, "# Hist B\n").unwrap();
        fs::write(&hist_csv, "a,b\n1,2\n3,4\n5,6\n").unwrap();
        settle(cx);
        let (back, forward) = if cfg!(target_os = "macos") { ("cmd-[", "cmd-]") } else { ("alt-left", "alt-right") };
        let seen = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, _| (s.picture.clone(), s.path.clone()));
        let on_note = |path: &Path| (None, Some(path.to_path_buf()));
        shell.update_in(cx, |s, window, cx| s.open_from_nav(&hist_a, window, cx));
        // Loin dans la note : le curseur et le défilement seront retrouvés au retour.
        cx.simulate_keystrokes("secondary-end up up up");
        let place = shell.read_with(cx, |s, cx| s.editor.read(cx).view());
        assert!(place.0 > 1000 && place.1 > 0., "{place:?}");
        // Suivre un lien, revenir, repartir.
        shell.update_in(cx, |s, window, cx| s.follow(&Link::Wiki("Hist B".into()), window, cx));
        assert_eq!(seen(cx), on_note(&hist_b));
        assert!(shell.read_with(cx, |s, _| s.history.can_back() && !s.history.can_forward()));
        cx.simulate_keystrokes(back);
        assert_eq!(seen(cx), on_note(&hist_a));
        assert_eq!(shell.read_with(cx, |s, cx| s.editor.read(cx).view()), place);
        cx.simulate_input("x");
        assert!(text(cx).contains("x"), "la saisie est dans la note retrouvée");
        cx.simulate_keystrokes("backspace");
        assert!(shell.read_with(cx, |s, _| s.history.can_forward()));
        cx.simulate_keystrokes(forward);
        assert_eq!(seen(cx), on_note(&hist_b));
        // Le bouton du rail.
        let at = cx.debug_bounds("nav-back").unwrap().center();
        cx.simulate_click(at, gpui::Modifiers::none());
        assert_eq!(seen(cx), on_note(&hist_a));
        // Ouvrir autre chose après un retour : « suivant » n'a plus où aller. Un tableau compte.
        shell.update_in(cx, |s, window, cx| s.open_from_nav(&hist_csv, window, cx));
        assert_eq!(seen(cx).0, Some(hist_csv.clone()));
        assert!(!shell.read_with(cx, |s, _| s.history.can_forward()));
        cx.simulate_keystrokes("down right");
        cx.simulate_keystrokes(forward);
        assert_eq!(seen(cx).0, Some(hist_csv.clone()));
        cx.simulate_keystrokes(back);
        assert_eq!(seen(cx), on_note(&hist_a));
        cx.simulate_keystrokes(forward);
        assert_eq!(seen(cx).0, Some(hist_csv.clone()));
        assert_eq!(shell.read_with(cx, |s, cx| s.shown_sheet().unwrap().read(cx).whereabouts().0), (1, 1), "la cellule est retrouvée");
        // Les boutons précédent et suivant de la souris.
        let middle = gpui::point(px(500.), px(400.));
        let navigate = |cx: &mut gpui::VisualTestContext, to| {
            cx.simulate_mouse_down(middle, MouseButton::Navigate(to), gpui::Modifiers::none());
            cx.simulate_mouse_up(middle, MouseButton::Navigate(to), gpui::Modifiers::none());
        };
        navigate(cx, gpui::NavigationDirection::Back);
        assert_eq!(seen(cx), on_note(&hist_a));
        // Un aperçu n'entre pas dans l'historique : « précédent » ramène d'où l'on vient, et
        // « suivant » mène toujours au tableau.
        shell.update_in(cx, |s, window, cx| {
            window.focus(&s.nav.focus);
            s.preview_note(&hist_b, cx);
        });
        assert_eq!(seen(cx), on_note(&hist_b));
        cx.simulate_keystrokes(back);
        assert_eq!(seen(cx), on_note(&hist_a));
        navigate(cx, gpui::NavigationDirection::Forward);
        assert_eq!(seen(cx).0, Some(hist_csv.clone()));
        // Une note déplacée est suivie.
        shell.update_in(cx, |s, window, cx| s.open_from_nav(&hist_b, window, cx));
        let shelf = root.join("Étagère");
        fs::create_dir_all(&shelf).unwrap();
        shell.update(cx, |s, cx| s.move_into(&hist_b, &shelf, cx));
        let moved = shelf.join("Hist B.md");
        cx.simulate_keystrokes(back);
        assert_eq!(seen(cx).0, Some(hist_csv.clone()));
        cx.simulate_keystrokes(forward);
        assert_eq!(seen(cx), on_note(&moved));
        // Un fichier mis à la corbeille quitte l'historique ; un fichier supprimé par un autre
        // programme est sauté.
        shell.update_in(cx, |s, window, cx| s.menu_do(nav::Do::Trash, Some(hist_csv.clone()), window, cx));
        cx.simulate_keystrokes(back);
        assert_eq!(seen(cx), on_note(&hist_a));
        fs::remove_file(&moved).unwrap();
        cx.simulate_keystrokes(forward);
        assert_eq!(seen(cx), on_note(&hist_a));
        assert!(!shell.read_with(cx, |s, _| s.history.can_forward()));
        // Depuis une note neuve, pas encore enregistrée : « précédent » ramène à la dernière.
        shell.update_in(cx, |s, window, cx| s.new_note_in(None, window, cx));
        assert_eq!(seen(cx), (None, None));
        cx.simulate_keystrokes(back);
        assert_eq!(seen(cx), on_note(&hist_a));

        // Les menus montrent, à côté de chaque action, le raccourci qui fait la même chose :
        // celui en vigueur, et rien pour une action qui n'en a pas.
        let keys = |cx: &mut gpui::VisualTestContext, what: nav::Do| cx.update(|_, cx| what.keys(cx));
        assert_eq!(keys(cx, nav::Do::Rename), "F2");
        assert_eq!(keys(cx, nav::Do::Duplicate), format!("{MOD}+D"));
        assert_eq!(keys(cx, nav::Do::Bold), format!("{MOD}+B"));
        assert_eq!(keys(cx, nav::Do::OpenLink), format!("{MOD}+clic"));
        assert_eq!(keys(cx, nav::Do::Icon), "");
        shell.update(cx, |s, cx| s.key_set("nav::Rename", "f6".into(), &[], cx));
        assert_eq!(keys(cx, nav::Do::Rename), "F6");
        shell.update(cx, |s, cx| s.key_set("nav::Rename", String::new(), &[], cx));
        assert_eq!(keys(cx, nav::Do::Rename), "");
        shell.update(cx, |s, cx| s.key_reset(None, cx));
        assert_eq!(keys(cx, nav::Do::Rename), "F2");

        // Deux panes côte à côte.
        let pane = |name: &str| root.join(format!("Pane {name}.md"));
        for name in ["A", "B", "D"] {
            fs::write(pane(name), format!("# Pane {name}\n\n")).unwrap();
        }
        fs::write(pane("C"), "# Pane C\n\n[[Pane A]]\n").unwrap();
        settle(cx);
        let open = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, _| (s.active, s.panes.iter().map(|pane| pane.path.clone()).collect::<Vec<_>>()));
        let both = |active: usize, left: &str, right: &str| (active, vec![Some(pane(left)), Some(pane(right))]);
        let pane_text = |cx: &mut gpui::VisualTestContext, pane: usize| shell.read_with(cx, |s, cx| s.panes[pane].editor.read(cx).text().to_string());
        shell.update_in(cx, |s, window, cx| s.open_from_nav(&pane("A"), window, cx));
        assert_eq!(open(cx), (0, vec![Some(pane("A"))]));
        // Au clavier : le second pane s'ouvre sur la palette, pour choisir ce qui y va.
        cx.simulate_keystrokes("secondary-\\");
        assert_eq!(open(cx), (1, vec![Some(pane("A")), None]));
        assert!(shell.read_with(cx, |s, _| s.palette.is_some()));
        cx.simulate_input("pane b");
        cx.simulate_keystrokes("enter");
        assert_eq!(open(cx), both(1, "A", "B"));
        // Chaque pane s'écrit et s'enregistre ; Ctrl+1 et Ctrl+2 passent de l'un à l'autre.
        cx.simulate_keystrokes("secondary-end");
        cx.simulate_input("droite");
        cx.simulate_keystrokes("secondary-1 secondary-end");
        assert_eq!(open(cx).0, 0);
        cx.simulate_input("gauche");
        assert_eq!((pane_text(cx, 0), pane_text(cx, 1)), ("# Pane A\n\ngauche".to_string(), "# Pane B\n\ndroite".to_string()));
        settle(cx);
        assert_eq!(fs::read_to_string(pane("A")).unwrap(), "# Pane A\n\ngauche");
        assert_eq!(fs::read_to_string(pane("B")).unwrap(), "# Pane B\n\ndroite");
        // Une note déjà affichée ne s'ouvre pas deux fois : la saisie passe dans son pane.
        shell.update_in(cx, |s, window, cx| s.open_from_nav(&pane("B"), window, cx));
        assert_eq!(open(cx), both(1, "A", "B"));
        cx.simulate_input("!");
        assert_eq!(pane_text(cx, 1), "# Pane B\n\ndroite!");
        // Ctrl+Entrée dans la palette : la note choisie s'ouvre dans l'autre pane.
        cx.simulate_keystrokes("secondary-p");
        cx.simulate_input("pane c");
        cx.simulate_keystrokes("secondary-enter");
        assert_eq!(open(cx), both(0, "C", "B"));
        // Ctrl+Maj+clic sur un lien : la note liée s'ouvre à côté.
        let link = shell.read_with(cx, |s, cx| s.editor.read(cx).point_of("# Pane C\n\n[[Pa".len()).unwrap());
        cx.simulate_click(link, gpui::Modifiers { shift: true, ..gpui::Modifiers::secondary_key() });
        assert_eq!(open(cx), both(1, "C", "A"));
        // « Ouvrir à côté » dans le menu de l'arbre.
        shell.update_in(cx, |s, window, cx| s.menu_do(nav::Do::OpenAside, Some(pane("D")), window, cx));
        assert_eq!(open(cx), both(0, "D", "A"));
        // Renommer ou déplacer la note de l'autre pane : il suit.
        shell.update(cx, |s, cx| s.move_into(&pane("A"), &shelf, cx));
        assert_eq!(open(cx), (0, vec![Some(pane("D")), Some(shelf.join("Pane A.md"))]));
        // Glisser une note de l'arbre sur un pane l'y ouvre ; la moitié visée est teintée.
        shell.update(cx, |s, cx| {
            (s.nav.mode, s.nav.panel) = (nav::Mode::Tree, nav::Panel::Split);
            cx.notify();
        });
        cx.run_until_parked();
        let row = cx.debug_bounds("nav-row-Pane B").unwrap().center();
        let right = cx.debug_bounds("pane-1").unwrap().center();
        cx.simulate_mouse_down(row, MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_move(row + gpui::point(px(12.), px(4.)), Some(MouseButton::Left), gpui::Modifiers::none());
        cx.simulate_mouse_move(right, Some(MouseButton::Left), gpui::Modifiers::none());
        assert_eq!(shell.read_with(cx, |s, _| s.pane_aim), Some((1, false)));
        cx.simulate_mouse_up(right, MouseButton::Left, gpui::Modifiers::none());
        assert_eq!(open(cx), both(1, "D", "B"));
        assert_eq!(shell.read_with(cx, |s, _| s.pane_aim), None);
        // Ctrl+W ferme le pane qui a la saisie ; ce qu'on y avait écrit est enregistré.
        cx.simulate_keystrokes("secondary-end");
        cx.simulate_input("?");
        cx.simulate_keystrokes("secondary-w");
        assert_eq!(open(cx), (0, vec![Some(pane("D"))]));
        assert_eq!(fs::read_to_string(pane("B")).unwrap(), "# Pane B\n\ndroite!?");
        cx.simulate_keystrokes("secondary-end");
        cx.simulate_input("seul");
        assert_eq!(text(cx), "# Pane D\n\nseul");
        cx.simulate_keystrokes("secondary-w");
        assert_eq!(open(cx).1.len(), 1, "le dernier pane ne se ferme pas");
        // Avec un seul pane, lâcher une note sur la moitié droite ouvre le second.
        let whole = cx.debug_bounds("pane-0").unwrap();
        let half = gpui::point(whole.right() - px(40.), whole.center().y);
        cx.simulate_mouse_down(row, MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_move(row + gpui::point(px(12.), px(4.)), Some(MouseButton::Left), gpui::Modifiers::none());
        cx.simulate_mouse_move(half, Some(MouseButton::Left), gpui::Modifiers::none());
        assert_eq!(shell.read_with(cx, |s, _| s.pane_aim), Some((0, true)));
        cx.simulate_mouse_up(half, MouseButton::Left, gpui::Modifiers::none());
        assert_eq!(open(cx), both(1, "D", "B"));
        // La disposition est retenue : la largeur, et ce que montre le second pane.
        assert_eq!(vault::load_file("panes"), format!("0.5\n{}\n", pane("B").display()));
        cx.simulate_keystrokes("secondary-w");
        assert_eq!(vault::load_file("panes"), "0.5\n\n");

        // Synchronisation par git : un serveur (dépôt nu) et une autre machine, dans le dossier
        // de test. Le coffre de ce parcours-ci est la première machine.
        let lab = root.join("sync");
        let (server, here, there) = (lab.join("remote.git"), lab.join("ici"), lab.join("ailleurs"));
        fs::create_dir_all(&server).unwrap();
        fs::create_dir_all(&here).unwrap();
        let git = |dir: &Path, args: &[&str]| assert!(std::process::Command::new("git").current_dir(dir).args(args).output().unwrap().status.success(), "{args:?}");
        git(&server, &["init", "-q", "--bare", "-b", "main"]);
        let url = server.to_str().unwrap().to_string();
        fs::write(here.join("Carnet.md"), "# Carnet\n\nun\ndeux\n").unwrap();
        shell.update_in(cx, |s, window, cx| s.set_vault(here.clone(), window, cx));
        cx.run_until_parked();
        shell.update(cx, |s, cx| s.open_note(&here.join("Carnet.md"), cx));
        // Un coffre sans git : « Synchroniser » demande l'adresse du dépôt, puis le relie.
        cx.simulate_keystrokes("secondary-shift-s");
        assert!(shell.read_with(cx, |s, _| s.palette.is_some()), "l'adresse est demandée");
        cx.simulate_keystrokes("escape");
        shell.update(cx, |s, cx| s.git_connect(url.clone(), cx));
        cx.run_until_parked();
        assert!(shell.read_with(cx, |s, _| s.told(Tone::Done, "relié")), "{:?}", shell.read_with(cx, |s, _| s.toasts.iter().map(|t| t.text.clone()).collect::<Vec<_>>()));
        // Une autre machine clone le coffre, écrit une note et change le carnet, puis envoie.
        git::clone(&url, &there).unwrap();
        // Elle reçoit d'abord ce que le serveur a, comme le ferait sa propre synchronisation,
        // puis écrit : ses changements partent de la dernière version commune.
        let other = |write: &dyn Fn()| {
            git::fetch(&there).unwrap();
            git::merge(&there, "2026-10-09", "ailleurs").unwrap();
            write();
            git::save(&there).unwrap();
            git::fetch(&there).unwrap();
            git::merge(&there, "2026-10-09", "ailleurs").unwrap();
            assert_eq!(git::push(&there), Ok(git::Pushed::Done));
        };
        other(&|| {
            fs::write(there.join("Venue d'ailleurs.md"), "# Venue d'ailleurs\n").unwrap();
            fs::write(there.join("Carnet.md"), "# Carnet\n\nun\ndeux\ntrois\n").unwrap();
        });
        // Ici, on tape, puis une seule action : tout est commité, reçu, fusionné, envoyé, et la
        // note ouverte montre ce qui a été reçu sans perdre ce qui vient d'être tapé.
        cx.simulate_keystrokes("secondary-home down down");
        cx.simulate_input("zéro");
        cx.simulate_keystrokes("enter");
        cx.simulate_keystrokes("secondary-shift-s");
        cx.run_until_parked();
        assert!(shell.read_with(cx, |s, _| s.told(Tone::Done, "1 envoyé") && s.told(Tone::Done, "2 reçus")), "{:?}", shell.read_with(cx, |s, _| s.toasts.iter().map(|t| t.text.clone()).collect::<Vec<_>>()));
        assert_eq!(text(cx), "# Carnet\n\nzéro\nun\ndeux\ntrois\n");
        assert!(shell.read_with(cx, |s, _| s.notes.iter().any(|note| note.name == "Venue d'ailleurs")));
        // Rien de nouveau : la commande le dit.
        cx.simulate_keystrokes("secondary-shift-s");
        cx.run_until_parked();
        assert!(shell.read_with(cx, |s, _| s.told(Tone::Done, "à jour")));
        // La même ligne changée des deux côtés : la note garde ce qu'on y a écrit, la version
        // d'ailleurs devient une note à côté, et aucun marqueur n'entre dans le texte.
        other(&|| fs::write(there.join("Carnet.md"), "# Carnet\n\nzéro\nUN (ailleurs)\ndeux\ntrois\n").unwrap());
        cx.simulate_keystrokes("secondary-home down down down end");
        cx.simulate_input(" (ici)");
        cx.simulate_keystrokes("secondary-shift-s");
        cx.run_until_parked();
        assert_eq!(text(cx), "# Carnet\n\nzéro\nun (ici)\ndeux\ntrois\n");
        assert!(shell.read_with(cx, |s, _| s.told(Tone::Done, "conflit")));
        let copy = fs::read_dir(&here).unwrap().flatten().map(|e| e.path()).find(|p| p.file_name().unwrap().to_string_lossy().starts_with("Carnet (conflict ")).unwrap();
        assert_eq!(fs::read_to_string(&copy).unwrap(), "# Carnet\n\nzéro\nUN (ailleurs)\ndeux\ntrois\n");
        assert!(shell.read_with(cx, |s, _| s.notes.iter().any(|note| note.path == copy)), "la copie est dans le coffre");
        // L'autre machine reçoit les deux versions.
        other(&|| ());
        assert_eq!(fs::read_to_string(there.join("Carnet.md")).unwrap(), "# Carnet\n\nzéro\nun (ici)\ndeux\ntrois\n");
        assert!(there.join(copy.file_name().unwrap()).is_file());
        // Un serveur injoignable : un message clair, et le coffre reste tel qu'il était.
        git(&here, &["remote", "set-url", "origin", lab.join("nulle-part.git").to_str().unwrap()]);
        cx.simulate_input("!");
        cx.simulate_keystrokes("secondary-shift-s");
        cx.run_until_parked();
        assert!(shell.read_with(cx, |s, _| s.told(Tone::Failed, "Synchronisation impossible")));
        assert_eq!(text(cx), "# Carnet\n\nzéro\nun (ici)!\ndeux\ntrois\n");
        assert!(!shell.read_with(cx, |s, _| s.syncing));
        // Cloner un coffre : le dossier est créé, et devient le coffre ouvert.
        let shelf = lab.join("clones");
        fs::create_dir_all(&shelf).unwrap();
        shell.update_in(cx, |s, window, cx| s.clone_to(url.clone(), shelf.clone(), window, cx));
        cx.run_until_parked();
        assert_eq!(shell.read_with(cx, |s, _| s.vault.clone()), Some(shelf.join("remote")));
        assert!(shelf.join("remote/Venue d'ailleurs.md").is_file());

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

    /// Les raccourcis changés sont relus au lancement ; une ligne fautive du fichier est écartée
    /// et signalée, les autres s'appliquent.
    #[gpui::test]
    fn keys_survive_a_restart(cx: &mut TestAppContext) {
        let root = std::env::temp_dir().join(format!("bref-keys-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let _config = isolated_config(&root);
        vault::save_file("keys", "nav::ShowTags=secondary-shift-y\nno::Such=f8\n");
        cx.update(bind_keys);
        cx.update(init_fonts);
        let (shell, cx) = cx.add_window_view(|window, cx| Shell::new(Some(root.clone()), Vec::new(), window, cx));
        cx.run_until_parked();
        assert_eq!(shell.read_with(cx, |s, _| s.changes.clone()), keys::Changes::from([("nav::ShowTags".to_string(), "secondary-shift-y".to_string())]));
        assert!(shell.read_with(cx, |s, _| s.told(Tone::Failed, "no::Such=f8")));
        cx.simulate_keystrokes("secondary-t");
        assert_ne!(shell.read_with(cx, |s, _| s.nav.mode), nav::Mode::Tags);
        cx.simulate_keystrokes("secondary-shift-y");
        assert_eq!(shell.read_with(cx, |s, _| s.nav.mode), nav::Mode::Tags);
        // Un fichier d'avant cette version, ou vide : les raccourcis d'origine.
        let _ = fs::remove_dir_all(&root);
    }

    /// Le second pane revient au lancement, avec sa largeur ; un fichier disparu, ou une config
    /// d'avant les panes, laisse un seul pane.
    #[gpui::test]
    fn panes_come_back(cx: &mut TestAppContext) {
        let root = std::env::temp_dir().join(format!("bref-panes-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let _config = isolated_config(&root);
        let (left, right) = (root.join("Gauche.md"), root.join("Droite.md"));
        fs::write(&left, "# Gauche\n").unwrap();
        fs::write(&right, "# Droite\n").unwrap();
        vault::save_file("panes", &format!("0.4\n{}\n", right.display()));
        cx.update(bind_keys);
        cx.update(init_fonts);
        // La note de droite est la dernière ouverte : le premier pane prend la suivante.
        let recent = vec![right.clone(), left.clone()];
        let (shell, cx) = cx.add_window_view(|window, cx| Shell::new(Some(root.clone()), recent, window, cx));
        cx.run_until_parked();
        let open = shell.read_with(cx, |s, _| (s.active, s.pane_ratio, s.panes.iter().map(|pane| pane.path.clone()).collect::<Vec<_>>()));
        assert_eq!(open, (0, 0.4, vec![Some(left.clone()), Some(right.clone())]));
        cx.simulate_input("x");
        assert_eq!(shell.read_with(cx, |s, cx| s.panes[0].editor.read(cx).text().to_string()), "# Gauche\nx");
        let _ = fs::remove_dir_all(&root);
    }

    /// Une note s'ouvre sur son corps : la première touche ne casse pas l'en-tête YAML et ne
    /// renomme pas la note par son titre, ce qui réécrirait les liens des autres notes.
    #[gpui::test]
    fn opens_on_the_body(cx: &mut TestAppContext) {
        let root = std::env::temp_dir().join(format!("bref-body-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let _config = isolated_config(&root);
        let plan = root.join("Plan.md");
        fs::write(&plan, "---\ntags: [a]\n---\n# Plan\n\nÉtapes\n").unwrap();
        fs::write(root.join("Accueil.md"), "# Accueil\n\n[[Plan]]\n").unwrap();
        cx.update(bind_keys);
        cx.update(init_fonts);
        let (_shell, cx) = cx.add_window_view(|window, cx| Shell::new(Some(root.clone()), vec![plan.clone()], window, cx));
        cx.run_until_parked();
        cx.simulate_input("x");
        cx.executor().advance_clock(Duration::from_millis(500));
        cx.run_until_parked();
        assert_eq!(fs::read_to_string(&plan).unwrap(), "---\ntags: [a]\n---\n# Plan\n\nxÉtapes\n");
        assert_eq!(fs::read_to_string(root.join("Accueil.md")).unwrap(), "# Accueil\n\n[[Plan]]\n");

        // Une nouvelle note écrite puis vidée ne laisse rien en partant.
        cx.simulate_keystrokes("secondary-n");
        cx.simulate_input("Brouillon");
        cx.executor().advance_clock(Duration::from_millis(500));
        cx.run_until_parked();
        assert!(root.join("Brouillon.md").is_file());
        cx.simulate_keystrokes("secondary-a backspace");
        cx.executor().advance_clock(Duration::from_millis(500));
        cx.run_until_parked();
        cx.simulate_keystrokes("secondary-n");
        cx.run_until_parked();
        let mut notes: Vec<_> = fs::read_dir(&root).unwrap().filter_map(|e| e.ok()).map(|e| e.file_name()).filter(|n| n.to_string_lossy().ends_with(".md")).collect();
        notes.sort();
        assert_eq!(notes, ["Accueil.md", "Plan.md"]);
        let _ = fs::remove_dir_all(&root);
    }

    /// La capture (`bref --capture`) : une ligne tapée, Entrée, elle est dans la note du jour
    /// et la fenêtre s'est refermée. Échap referme sans rien écrire.
    #[gpui::test]
    fn capture_window(cx: &mut TestAppContext) {
        let root = std::env::temp_dir().join(format!("bref-capture-window-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let _config = isolated_config(&root);
        let note = root.join(format!("{}.md", date_name(today())));
        cx.update(bind_keys);
        cx.update(init_fonts);
        let closed = std::rc::Rc::new(std::cell::Cell::new(0));
        cx.update(|cx| {
            let closed = closed.clone();
            cx.on_window_closed(move |_| closed.set(closed.get() + 1)).detach();
        });
        {
            let (capture, cx) = cx.add_window_view(|window, cx| Capture::new(root.clone(), window, cx));
            cx.run_until_parked();
            // Un champ de saisie ne propose rien, même avant la première frappe.
            assert_eq!(capture.read_with(cx, |c, cx| c.field.read(cx).listed()), 0);
            cx.simulate_input("rappeler le plombier");
            cx.simulate_keystrokes("enter");
            cx.run_until_parked();
        }
        assert_eq!(fs::read_to_string(&note).unwrap(), format!("# {}\n\n- rappeler le plombier\n", date_name(today())));
        assert_eq!(closed.get(), 1);
        {
            let (_, cx) = cx.add_window_view(|window, cx| Capture::new(root.clone(), window, cx));
            cx.run_until_parked();
            cx.simulate_input("à oublier");
            cx.simulate_keystrokes("escape");
            cx.run_until_parked();
        }
        assert!(!fs::read_to_string(&note).unwrap().contains("oublier"));
        assert_eq!(closed.get(), 2);
        // Une très longue ligne : rien n'est coupé ni refusé. Le texte défile sous le curseur,
        // qui reste dans le cadre, à la frappe comme au retour en début de ligne.
        let long = "mot ".repeat(250);
        {
            let (capture, cx) = cx.add_window_view(|window, cx| Capture::new(root.clone(), window, cx));
            cx.run_until_parked();
            let caret_inside = |cx: &mut gpui::VisualTestContext| capture.read_with(cx, |c, cx| c.field.read(cx).caret_inside());
            cx.simulate_input(&long);
            cx.run_until_parked();
            assert_eq!(caret_inside(cx), Some(true), "le curseur suit la frappe");
            cx.simulate_keystrokes("home");
            cx.run_until_parked();
            assert_eq!(caret_inside(cx), Some(true), "le curseur revient en vue au début");
            cx.simulate_keystrokes("end enter");
            cx.run_until_parked();
        }
        assert!(fs::read_to_string(&note).unwrap().ends_with(&format!("- {}\n", long.trim())));
        assert_eq!(closed.get(), 3);
        let _ = fs::remove_dir_all(&root);
    }

    /// La page vide : la date du jour, les touches utiles et les notes à relire, sous la ligne
    /// où l'on écrit. Tout tient dans une fenêtre étroite, et s'efface à la première frappe.
    #[gpui::test]
    fn empty_page(cx: &mut TestAppContext) {
        let root = std::env::temp_dir().join(format!("bref-empty-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let _config = isolated_config(&root);
        let week = date_name(markdown::day_of(markdown::day_number(today()) - 7));
        let (daily, old) = (root.join(format!("{week}.md")), root.join("Ancienne.md"));
        fs::write(&daily, format!("# {week}\n")).unwrap();
        fs::write(&old, "# Ancienne\n").unwrap();
        let long_ago = std::time::SystemTime::now() - Duration::from_secs(100 * 86_400);
        fs::File::options().write(true).open(&old).unwrap().set_modified(long_ago).unwrap();
        cx.update(bind_keys);
        cx.update(init_fonts);
        let (shell, cx) = cx.add_window_view(|window, cx| Shell::new(Some(root.clone()), Vec::new(), window, cx));
        cx.run_until_parked();
        let open = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, _| s.path.clone());
        let text_of = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.editor.read(cx).text().to_string());
        assert_eq!(open(cx), None);

        // Le logo est dans le rail, quel que soit le système.
        assert!(cx.debug_bounds("rail-logo").is_some());
        // Les notes à relire : la note du jour d'il y a une semaine, puis une note ancienne.
        assert_eq!(shell.read_with(cx, |s, _| s.recalled().iter().map(|r| r.name.clone()).collect::<Vec<_>>()), [week.clone(), "Ancienne".to_string()]);
        // Dans la fenêtre la plus étroite, rien ne dépasse du pane.
        cx.simulate_resize(size(px(360.), px(640.)));
        cx.run_until_parked();
        let pane = cx.debug_bounds("pane-0").unwrap();
        assert!(pane.size.width < px(360.), "{pane:?}");
        for part in ["empty-today", "empty-key-0", "empty-key-1", "empty-key-2", "empty-recall-0", "empty-recall-1"] {
            let bounds = cx.debug_bounds(part).unwrap_or_else(|| panic!("{part} absent"));
            assert!(bounds.left() >= pane.left() && bounds.right() <= pane.right(), "{part} dépasse : {bounds:?} hors de {pane:?}");
        }
        cx.simulate_resize(size(px(860.), px(720.)));
        cx.run_until_parked();

        // Un clic sur une note à relire l'ouvre.
        let at = cx.debug_bounds("empty-recall-1").unwrap().center();
        cx.simulate_click(at, gpui::Modifiers::none());
        assert_eq!(open(cx), Some(old.clone()));
        // Une note qui a du texte ne montre pas la page vide : là où était la date, le clic
        // tombe dans la note. (gpui garde les bornes d'un élément disparu : on clique.)
        cx.run_until_parked();
        let at = cx.debug_bounds("empty-today").unwrap().center();
        cx.simulate_click(at, gpui::Modifiers::none());
        cx.run_until_parked();
        assert_eq!(open(cx), Some(old.clone()));

        // Une touche de la page vide se clique : ici la palette.
        cx.simulate_keystrokes("secondary-n");
        cx.run_until_parked();
        let at = cx.debug_bounds("empty-key-0").unwrap().center();
        cx.simulate_click(at, gpui::Modifiers::none());
        assert!(shell.read_with(cx, |s, _| s.palette.is_some()));
        cx.simulate_keystrokes("escape");

        // La date ouvre la note du jour.
        let at = cx.debug_bounds("empty-today").unwrap().center();
        cx.simulate_click(at, gpui::Modifiers::none());
        cx.run_until_parked();
        assert_eq!(open(cx), Some(root.join(format!("{}.md", date_name(today())))));

        // La première frappe efface tout : la page est à ce qu'on écrit.
        cx.simulate_keystrokes("secondary-n");
        cx.run_until_parked();
        cx.simulate_input("x");
        cx.run_until_parked();
        cx.simulate_click(at, gpui::Modifiers::none());
        cx.run_until_parked();
        assert_eq!(text_of(cx), "x");
        let _ = fs::remove_dir_all(&root);
    }
}
