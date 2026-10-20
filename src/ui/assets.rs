//! What ships inside the binary: the IBM Plex fonts and the icons (16 px SVG drawn as code).

use std::{
    borrow::Cow,
    sync::{OnceLock, RwLock},
};

use gpui::{App, AssetSource, SharedString, prelude::*, px, svg};

use crate::{logic::prefs::Prefs, ui::theme::Theme};

/// Installed fonts, collected at startup.
pub static FAMILIES: OnceLock<Vec<&'static str>> = OnceLock::new();

/// Fonts (text, code) in use: the chosen ones, otherwise the defaults.
static FONTS: RwLock<(&str, &str)> = RwLock::new(("IBM Plex Sans", "IBM Plex Mono"));

/// IBM Plex Sans and Plex Mono (SIL OFL license, `assets/fonts/`), in the binary: the app
/// has its fonts without installing anything. Static fonts, because gpui 0.2 does not
/// bold a variable font.
pub const FONT_FILES: [&[u8]; 8] = [
    include_bytes!("../../assets/fonts/IBMPlexSans-Regular.ttf"),
    include_bytes!("../../assets/fonts/IBMPlexSans-Italic.ttf"),
    include_bytes!("../../assets/fonts/IBMPlexSans-Bold.ttf"),
    include_bytes!("../../assets/fonts/IBMPlexSans-BoldItalic.ttf"),
    include_bytes!("../../assets/fonts/IBMPlexMono-Regular.ttf"),
    include_bytes!("../../assets/fonts/IBMPlexMono-Italic.ttf"),
    include_bytes!("../../assets/fonts/IBMPlexMono-Bold.ttf"),
    include_bytes!("../../assets/fonts/IBMPlexMono-BoldItalic.ttf"),
];

pub fn sans() -> &'static str {
    FONTS.read().unwrap().0
}

pub fn mono() -> &'static str {
    FONTS.read().unwrap().1
}

pub fn init_fonts(cx: &mut App) {
    cx.text_system().add_fonts(FONT_FILES.iter().map(|f| Cow::Borrowed(*f)).collect()).ok();
    FAMILIES.get_or_init(|| {
        let names: &'static [String] = cx.text_system().all_font_names().leak();
        names.iter().map(String::as_str).collect()
    });
}

/// Puts the settings' fonts in use (any installed font); failing that,
/// the app's.
pub fn apply_fonts(prefs: &Prefs) {
    let installed = FAMILIES.get().map_or(&[][..], Vec::as_slice);
    let pick = |chosen: &str, wanted: &[&'static str]| {
        let known = |name: &str| installed.iter().copied().find(|i| *i == name);
        known(chosen)
            .or_else(|| wanted.iter().find_map(|w| known(w)))
            .unwrap_or(wanted[0])
    };
    *FONTS.write().unwrap() = (pick(&prefs.font, &["IBM Plex Sans"]), pick(&prefs.mono, &["IBM Plex Mono"]));
}

/// Icons embedded in the binary (1.2 px strokes on a 16 px grid).
pub struct Assets;

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
            // Icons of the palette commands and of a table's bar.
            "c-bold.svg" => r#"<path d="M5 3.5H9A2.2 2.2 0 0 1 9 8H5ZM5 8H9.6A2.25 2.25 0 0 1 9.6 12.5H5Z"/>"#,
            "c-italic.svg" => r#"<path d="M7 3.5H12M4 12.5H9M9.5 3.5L6.5 12.5"/>"#,
            "c-underline.svg" => r#"<path d="M4.5 3V7.5A3.5 3.5 0 0 0 11.5 7.5V3M3.5 13.5H12.5"/>"#,
            "c-strike.svg" => r#"<path d="M11 4.5C10.5 3.6 9.4 3 8 3C6.3 3 5 3.9 5 5.2C5 6.3 5.9 6.9 7 7.3M3 8.5H13M10.2 9.5C10.7 10 11 10.5 11 11.1C11 12.4 9.7 13.2 8 13.2C6.4 13.2 5.2 12.6 4.7 11.6"/>"#,
            "c-highlight.svg" => r#"<path d="M9.5 3L13 6.5L7.5 12H4V8.5ZM3 13.5H13"/>"#,
            "c-heading.svg" => r#"<path d="M3 3.5V12.5M9 3.5V12.5M3 8H9M11.5 8.5L13 7.5V12.5"/>"#,
            "c-text.svg" => r#"<path d="M3.5 4.5V3.5H12.5V4.5M8 3.5V12.5M6.5 12.5H9.5"/>"#,
            "c-quote.svg" => r#"<path d="M3.5 3.5V12.5M6.5 5H12.5M6.5 8H12.5M6.5 11H10.5"/>"#,
            "c-up.svg" => r#"<path d="M8 12.5V3.5M4.5 7L8 3.5L11.5 7"/>"#,
            "c-down.svg" => r#"<path d="M8 3.5V12.5M4.5 9L8 12.5L11.5 9"/>"#,
            "c-left.svg" => r#"<path d="M12.5 8H3.5M7 4.5L3.5 8L7 11.5"/>"#,
            "c-right.svg" => r#"<path d="M3.5 8H12.5M9 4.5L12.5 8L9 11.5"/>"#,
            "c-ins-left.svg" => r#"<rect x="7.5" y="2.5" width="6" height="11" rx="1"/><path d="M2.5 8H5.5M4 6.5V9.5"/>"#,
            "c-ins-right.svg" => r#"<rect x="2.5" y="2.5" width="6" height="11" rx="1"/><path d="M10.5 8H13.5M12 6.5V9.5"/>"#,
            "c-ins-above.svg" => r#"<rect x="2.5" y="7.5" width="11" height="6" rx="1"/><path d="M8 2.5V5.5M6.5 4H9.5"/>"#,
            "c-ins-below.svg" => r#"<rect x="2.5" y="2.5" width="11" height="6" rx="1"/><path d="M8 10.5V13.5M6.5 12H9.5"/>"#,
            "c-sort-az.svg" => r#"<path d="M2.5 12L4.5 5L6.5 12M3.2 9.8H5.8M9 5H12.5L9 12H12.5"/>"#,
            "c-sort-za.svg" => r#"<path d="M2.5 5H6L2.5 12H6M9 12L11 5L13 12M9.7 9.8H12.3"/>"#,
            "c-transpose.svg" => r#"<path d="M3.5 3.5H9.5M3.5 3.5V9.5M12.5 6.5V12.5H6.5M11 11L5 5"/>"#,
            "c-select-row.svg" => r#"<rect x="2.5" y="6" width="11" height="4" rx="1"/><path d="M2.5 3H13.5M2.5 13H13.5"/>"#,
            "c-select-col.svg" => r#"<rect x="6" y="2.5" width="4" height="11" rx="1"/><path d="M3 2.5V13.5M13 2.5V13.5"/>"#,
            "c-cut.svg" => r#"<circle cx="4.5" cy="11.5" r="1.8"/><circle cx="11.5" cy="11.5" r="1.8"/><path d="M5.8 10.2L11.5 3M10.2 10.2L4.5 3"/>"#,
            "c-paste.svg" => r#"<rect x="3.5" y="3" width="9" height="10.5" rx="1.5"/><path d="M6 3V2.5H10V3M6 7H10M6 10H9"/>"#,
            "c-undo.svg" => r#"<path d="M5.5 4L3 6.5L5.5 9M3 6.5H9.5A3 3 0 0 1 9.5 12.5H6"/>"#,
            "c-redo.svg" => r#"<path d="M10.5 4L13 6.5L10.5 9M13 6.5H6.5A3 3 0 0 0 6.5 12.5H10"/>"#,
            "c-comment.svg" => r#"<path d="M3 4A1.5 1.5 0 0 1 4.5 2.5H11.5A1.5 1.5 0 0 1 13 4V9.5A1.5 1.5 0 0 1 11.5 11H7L4 13.5V11H4.5A1.5 1.5 0 0 1 3 9.5Z"/>"#,
            "c-outline.svg" => r#"<path d="M3 4H13M5.5 8H13M8 12H13"/>"#,
            "c-calendar.svg" => r#"<rect x="2.5" y="3.5" width="11" height="10" rx="1.5"/><path d="M2.5 6.5H13.5M5.5 2.5V4.5M10.5 2.5V4.5"/>"#,
            "c-split.svg" => r#"<rect x="2.5" y="3" width="11" height="10" rx="1.5"/><path d="M8 3V13"/>"#,
            "c-zoom-in.svg" => r#"<circle cx="7" cy="7" r="4.2"/><path d="M10.2 10.2L13 13M5.3 7H8.7M7 5.3V8.7"/>"#,
            "c-zoom-out.svg" => r#"<circle cx="7" cy="7" r="4.2"/><path d="M10.2 10.2L13 13M5.3 7H8.7"/>"#,
            "c-zoom-reset.svg" => r#"<circle cx="7" cy="7" r="4.2"/><path d="M10.2 10.2L13 13M5.5 7H8.5M5.5 7.01"/>"#,
            "c-quit.svg" => r#"<path d="M9.5 3H12.5V13H9.5M6.5 5L3.5 8L6.5 11M3.5 8H10"/>"#,
            // Three strokes: the menu of all commands.
            "menu.svg" => r#"<path d="M3 4.5H13M3 8H13M3 11.5H13"/>"#,
            "info.svg" => r#"<circle cx="8" cy="8" r="5.5"/><path d="M8 7.5V11M8 5V5.2"/>"#,
            "alert.svg" => r#"<path d="M8 2.5L14 13H2ZM8 6.5V9.5M8 11.3V11.5"/>"#,
            "spinner.svg" => r#"<path d="M8 2.5A5.5 5.5 0 1 1 2.5 8"/>"#,
            "tree.svg" => r#"<path d="M3 3.5H10M3 3.5V12H6M3 7.75H6M8.5 7.75H13M8.5 12H13"/>"#,
            "clock.svg" => r#"<circle cx="8" cy="8" r="5.5"/><path d="M8 5V8L10 9.5"/>"#,
            "search.svg" => r#"<circle cx="7" cy="7" r="3.5"/><path d="M9.7 9.7L12.5 12.5"/>"#,
            "plus.svg" => r#"<path d="M8 3.5V12.5M3.5 8H12.5"/>"#,
            // Six dots: a block's handle.
            "grip.svg" => r#"<path d="M6 3.5V3.6M10 3.5V3.6M6 8V8.1M10 8V8.1M6 12.5V12.6M10 12.5V12.6" stroke-width="2.2"/>"#,
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
            // An arrow toward a disk: a copy put somewhere safe, not an archive box.
            "backup.svg" => {
                r#"<path d="M8 2V8M5.5 5.5L8 8L10.5 5.5"/><rect x="2.5" y="9.5" width="11" height="4" rx="1"/><path d="M11 11.5H11.2"/>"#
            }
            "backlink.svg" => r#"<path d="M13 4.5H8.5A3 3 0 0 0 5.5 7.5V11.5M3 9L5.5 11.5L8 9"/>"#,
            "code.svg" => r#"<path d="M5.5 4.5L2.5 8L5.5 11.5M10.5 4.5L13.5 8L10.5 11.5"/>"#,
            "list-bullet.svg" => r#"<path d="M6.5 4H13.5M6.5 8H13.5M6.5 12H13.5M3 4H3.1M3 8H3.1M3 12H3.1"/>"#,
            "list-number.svg" => r#"<path d="M7 4H13.5M7 8H13.5M7 12H13.5M2.7 3.6L3.8 3V5.4M2.7 8.4A1 1 0 0 1 4.4 9C4.4 9.7 3 10.4 2.7 11H4.5"/>"#,
            "list-task.svg" => r#"<rect x="2.5" y="2.5" width="3.5" height="3.5" rx="0.8"/><path d="M3.2 4.3L4 5L5.4 3.4M8 4.3H13.5M8 11.7H13.5"/><rect x="2.5" y="10" width="3.5" height="3.5" rx="0.8"/>"#,
            "table.svg" => r#"<rect x="2.5" y="3" width="11" height="10" rx="1.2"/><path d="M2.5 6.5H13.5M2.5 9.8H13.5M7 3V13"/>"#,
            "row-add.svg" => r#"<rect x="2.5" y="2.5" width="11" height="6" rx="1"/><path d="M2.5 5.5H13.5M8 10.6V13.8M6.4 12.2H9.6"/>"#,
            "col-add.svg" => r#"<rect x="2.5" y="2.5" width="6" height="11" rx="1"/><path d="M5.5 2.5V13.5M12 6.4V9.6M10.4 8H13.6"/>"#,
            "row-del.svg" => r#"<rect x="2.5" y="2.5" width="11" height="6" rx="1"/><path d="M2.5 5.5H13.5M6.4 12.2H9.6"/>"#,
            "col-del.svg" => r#"<rect x="2.5" y="2.5" width="6" height="11" rx="1"/><path d="M5.5 2.5V13.5M10.4 8H13.6"/>"#,
            "format.svg" => r#"<path d="M4 11.5L8 3L12 11.5M5.4 8.4H10.6M3 14H13"/>"#,
            "link.svg" => r#"<path d="M6.9 9.1A2.4 2.4 0 0 0 10.3 9.1L12.2 7.2A2.4 2.4 0 0 0 8.8 3.8L8 4.6M9.1 6.9A2.4 2.4 0 0 0 5.7 6.9L3.8 8.8A2.4 2.4 0 0 0 7.2 12.2L8 11.4"/>"#,
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
            "d-label-start.svg" => r#"<path d="M2.5 11.5H13.5M3 3.5H7M5 3.5V8"/>"#,
            "d-label-end.svg" => r#"<path d="M2.5 11.5H13.5M9 3.5H13M11 3.5V8"/>"#,
            "d-rel-menu.svg" => r#"<path d="M2.5 11.5H8M8 11.5L11 8.5L14 11.5L11 14.5ZM3 4.5H6.5M9.5 4.5H13M13 4.5L10.5 2.5M13 4.5L10.5 6.5"/>"#,
            "d-rel-association.svg" => r#"<path d="M2.5 8H13.5"/>"#,
            "d-rel-navigable.svg" => r#"<path d="M2.5 8H13.5M10 4.8L13.5 8L10 11.2"/>"#,
            "d-rel-nonnavigable.svg" => r#"<path d="M2.5 8H13.5M8.5 5.5L11.5 10.5M11.5 5.5L8.5 10.5"/>"#,
            "d-rel-aggregation.svg" => r#"<path d="M2.5 8L5.5 5.5L8.5 8L5.5 10.5ZM8.5 8H13.5"/>"#,
            "d-rel-composition.svg" => r#"<path d="M2.5 8L5.5 5.5L8.5 8L5.5 10.5ZM8.5 8H13.5" fill="black"/>"#,
            "d-rel-generalization.svg" => r#"<path d="M2.5 8H8M14 8L8 4.8V11.2Z"/>"#,
            "d-rel-realization.svg" => r#"<path d="M2.5 8H4M5.8 8H7.4M14 8L8 4.8V11.2Z"/>"#,
            "d-rel-dependency.svg" => r#"<path d="M2.5 8H4.5M6.5 8H8.5M10.5 8H13.5M10 4.8L13.5 8L10 11.2"/>"#,
            "d-rel-assocclass.svg" => r#"<path d="M2.5 4.5H13.5M8 5V7M8 8.6V10.2M8 11.8V13"/><circle cx="8" cy="4.5" r="0.6" fill="black"/>"#,
            "d-rel-ball.svg" => r#"<path d="M2.5 8H7.5"/><circle cx="10.2" cy="8" r="2.7"/>"#,
            "d-rel-socket.svg" => r#"<path d="M2.5 8H7M11.5 4.5A4 4 0 0 0 11.5 11.5"/>"#,
            "d-rel-nary.svg" => r#"<path d="M8 3L13 8L8 13L3 8ZM8 3V1.5M3 8H1.5M13 8H14.5"/>"#,
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
            // The two cards of the app icon, filled; the one underneath is faded.
            "logo.svg" => {
                r#"<rect x="1.8" y="1.8" width="6.2" height="7.6" rx="1.4" transform="rotate(-7 5 5.5)" fill="black" stroke="none"/><rect x="8.3" y="6.6" width="6.2" height="7.6" rx="1.4" transform="rotate(7 11.4 10.4)" fill="black" stroke="none" opacity=".55"/>"#
            }
            // Icons given to notes and folders.
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

/// Icons to give a note, a file or a folder: the drawn file, and its shapes
/// in the 16-unit square of the other icons. The kept name is the file name without `i-` or `.svg`.
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

/// The app logo, discreet: the two cards, in the accent color.
pub fn logo(t: Theme) -> gpui::Svg {
    svg().path("logo.svg").size(px(15.)).flex_none().text_color(t.accent.opacity(0.85))
}
