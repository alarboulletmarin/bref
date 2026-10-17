//! Colours and page geometry: the themes, how one is picked from the preferences and the system
//! appearance, and the contrast rules (WCAG) that keep text readable.

use gpui::{Hsla, Pixels, WindowAppearance, px, rgb};

use crate::{lang::tr, logic::prefs::Prefs};

/// Width of the text column ("Page width" setting).
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Page {
    Narrow,
    #[default]
    Normal,
    Wide,
    /// The whole room of the window.
    Full,
}

impl Page {
    pub const ALL: [Page; 4] = [Page::Narrow, Page::Normal, Page::Wide, Page::Full];

    pub fn key(self) -> &'static str {
        match self {
            Page::Narrow => "narrow",
            Page::Normal => "normal",
            Page::Wide => "wide",
            Page::Full => "full",
        }
    }

    /// An unknown or missing setting means the normal width.
    pub fn of(key: &str) -> Self {
        Self::ALL.into_iter().find(|p| p.key() == key).unwrap_or_default()
    }

    pub fn label(self) -> &'static str {
        match self {
            Page::Narrow => tr("Narrow", "Étroite"),
            Page::Normal => tr("Normal", "Normale"),
            Page::Wide => tr("Wide", "Large"),
            Page::Full => tr("Full width", "Pleine largeur"),
        }
    }

    /// The widest column, for text of `size` px: it follows the body size, so that
    /// the number of characters per line stays the same when zooming in.
    pub fn measure(self, size: f32) -> Pixels {
        let base = match self {
            Page::Narrow => 560.,
            Page::Normal => 720.,
            Page::Wide => 960.,
            Page::Full => return px(f32::MAX),
        };
        px(base * size / 16.)
    }
}

/// Where a table wider than the text column goes ("Wide tables" setting).
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Wide {
    /// It stays in the column and scrolls sideways.
    Column,
    /// It overflows on both sides, centered on the window.
    #[default]
    Centered,
}

impl Wide {
    pub const ALL: [Wide; 2] = [Wide::Column, Wide::Centered];

    pub fn key(self) -> &'static str {
        match self {
            Wide::Column => "column",
            Wide::Centered => "center",
        }
    }

    pub fn of(key: &str) -> Self {
        Self::ALL.into_iter().find(|w| w.key() == key).unwrap_or_default()
    }

    pub fn label(self) -> &'static str {
        match self {
            Wide::Column => tr("In the text column", "Dans la colonne du texte"),
            Wide::Centered => tr("Centered, wider than the text", "Centrés, plus larges que le texte"),
        }
    }
}
#[derive(Clone, Copy)]
pub struct Theme {
    pub bg: Hsla,
    pub text: Hsla,
    pub dim: Hsla,
    /// Secondary text that stays readable (4.5:1 on the background and the panel): labels, hints,
    /// counters. `dim` stays for Markdown markers, which do not need to be read.
    pub soft: Hsla,
    pub accent: Hsla,
    pub selection: Hsla,
    pub code_bg: Hsla,
    pub panel: Hsla,
    pub border: Hsla,
    /// Size of the note's body text, in pixels.
    pub size: f32,
    pub page: Page,
    pub wide: Wide,
    /// Formatting bar at the bottom of the note.
    pub bar: bool,
}

/// Offered themes: (name, [background, text, faint, accent, code background, panel, border]).
/// The first two are Bref's, dark and light, which follow the system.
pub const THEMES: &[(&str, [u32; 7])] = &[
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
    /// The chosen theme; failing that, Bref's one matching the system appearance.
    pub fn of(prefs: &Prefs, appearance: WindowAppearance) -> Self {
        let dark = matches!(appearance, WindowAppearance::Dark | WindowAppearance::VibrantDark);
        let system = &THEMES[if dark { 0 } else { 1 }];
        let (_, colors) = THEMES.iter().find(|t| t.0 == prefs.theme).unwrap_or(system);
        let [bg, text, dim, accent, code_bg, panel, border] = colors.map(|c| Hsla::from(rgb(c)));
        // The faintest grey between text and background that still reads on both backgrounds.
        let soft = (0..=20)
            .map(|i| blend(text, bg, 0.5 + i as f32 * 0.025))
            .find(|c| contrast(*c, bg) >= 4.5 && contrast(*c, panel) >= 4.5)
            .unwrap_or(text);
        Self {
            bg,
            text,
            dim,
            soft,
            // Links and tags are written in the accent: it must read like text.
            accent: legible(legible(accent, bg), panel),
            selection: accent.opacity(if bg.l < 0.5 { 0.25 } else { 0.2 }),
            code_bg,
            panel,
            border,
            size: prefs.size,
            page: Page::of(&prefs.page),
            wide: Wide::of(&prefs.wide),
            bar: prefs.bar != "off",
        }
    }
}

/// Relative luminance of a color, in the WCAG sense.
fn luminance(color: Hsla) -> f32 {
    let c = color.to_rgb();
    let linear = |x: f32| if x <= 0.03928 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) };
    0.2126 * linear(c.r) + 0.7152 * linear(c.g) + 0.0722 * linear(c.b)
}

/// Contrast ratio of two colors (WCAG): at least 4.5 for body text.
pub fn contrast(a: Hsla, b: Hsla) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// `color`, darkened on a light background or lightened on a dark one, just enough to
/// read on `bg`; it keeps its hue.
pub fn legible(color: Hsla, bg: Hsla) -> Hsla {
    let step = if luminance(bg) > 0.18 { -0.02 } else { 0.02 };
    let mut color = color;
    while contrast(color, bg) < 4.5 && (0.0..=1.0).contains(&(color.l + step)) {
        color.l += step;
    }
    color
}

/// `top` laid over `under` with opacity `share`.
pub fn blend(top: Hsla, under: Hsla, share: f32) -> Hsla {
    let (a, b) = (top.to_rgb(), under.to_rgb());
    let mix = |x: f32, y: f32| x * share + y * (1. - share);
    Hsla::from(gpui::Rgba { r: mix(a.r, b.r), g: mix(a.g, b.g), b: mix(a.b, b.b), a: 1. })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::WindowAppearance;

    /// In each theme, secondary text, the accent (links, tags) and the title of a `[!TIP]`
    /// callout read: at least 4.5:1 (WCAG AA).
    #[test]
    fn every_theme_reads() {
        for (name, _) in THEMES {
            let t = Theme::of(&Prefs::parse(&format!("theme={name}\n")), WindowAppearance::Light);
            assert!(contrast(t.soft, t.bg) >= 4.5 && contrast(t.soft, t.panel) >= 4.5, "{name}: soft");
            assert!(contrast(t.accent, t.bg) >= 4.5, "{name}: accent");
            for h in [0.38, 0.75, 0.09, 0., 0.58] {
                let tint = Hsla { h, s: 0.7, l: 0.5, a: 1. };
                let under = blend(tint, t.bg, 0.1);
                assert!(contrast(legible(tint, under), under) >= 4.5, "{name}: panel {h}");
            }
        }
    }

    #[test]
    fn page_width_follows_the_text_size() {
        assert_eq!(Page::Normal.measure(16.), px(720.));
        assert_eq!(Page::Normal.measure(32.), px(1440.));
        assert!(Page::Narrow.measure(16.) < Page::Normal.measure(16.) && Page::Normal.measure(16.) < Page::Wide.measure(16.));
        assert!(Page::Wide.measure(16.) < Page::Full.measure(16.));
        // Keys are read back; an unknown or missing key means the default choice.
        assert!(Page::ALL.into_iter().all(|p| Page::of(p.key()) == p) && Wide::ALL.into_iter().all(|w| Wide::of(w.key()) == w));
        assert_eq!((Page::of(""), Page::of("?"), Wide::of(""), Wide::of("?")), (Page::Normal, Page::Normal, Wide::Centered, Wide::Centered));
    }
}
