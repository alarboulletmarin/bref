//! Colours and page geometry: the themes, how one is picked from the preferences and the system
//! appearance, and the contrast rules (WCAG) that keep text readable.

use gpui::{BoxShadow, Hsla, Pixels, WindowAppearance, hsla, point, px, rgb};

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
#[derive(Clone, Copy, PartialEq)]
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
    /// Strings and numbers of a code block.
    pub string: Hsla,
    pub number: Hsla,
    /// Lines that are not text but must be seen (3:1): graph edges, diagram strokes.
    pub line: Hsla,
    /// The surface of a hovered button or row: a little of the text over what is under it, so that it
    /// reads on `bg`, `panel`, `code_bg` and `selection` alike, and is never the colour of a line.
    pub hover: Hsla,
    /// The same, a little stronger, while the pointer is down (`.active`).
    pub pressed: Hsla,
    /// Size of the note's body text, in pixels.
    pub size: f32,
    /// Scale of the interface around the note (panel, menus, help, palette): it follows the note's size,
    /// 1 at 16, from 0.85 to 1.5 so that the panel stays usable.
    pub ui: f32,
    pub page: Page,
    pub wide: Wide,
    /// Formatting bar at the bottom of the note.
    pub bar: bool,
    /// Short fades when a panel, the format bar or a folder opens; off: everything is instant.
    pub motion: bool,
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
        // The selection, as strong as the text over it allows.
        let share = (0..=15)
            .map(|i| if bg.l < 0.5 { 0.25 } else { 0.2 } - i as f32 * 0.01)
            .find(|&a| [bg, panel].iter().all(|b| contrast(text, blend(accent, *b, a)) >= 4.5))
            .unwrap_or(0.12);
        let picked = [bg, panel, blend(accent, bg, share), blend(accent, panel, share)];
        // Some themes' own text is under 4.5:1 on their code background or selected (Solarized).
        let text = picked.iter().chain([&code_bg]).fold(text, |c, b| legible(c, *b));
        // The faintest grey between text and background that still reads on the page, the panel and code.
        let soft = (0..=20)
            .map(|i| blend(text, bg, 0.5 + i as f32 * 0.025))
            .find(|c| [bg, panel, code_bg].iter().all(|b| contrast(*c, *b) >= 4.5))
            .unwrap_or(text);
        // Links and tags are written in the accent: it must read like text, selected too.
        let accent_read = picked.iter().fold(accent, |c, b| legible(c, *b));
        // Strings and numbers take the accent shifted in hue, to match each theme without a palette of their own.
        let code = |by: f32| [bg, code_bg].iter().fold(Hsla { h: (accent_read.h + by).fract(), ..accent_read }, |c, b| legible(c, *b));
        Self {
            bg,
            text,
            dim,
            soft,
            accent: accent_read,
            string: code(0.33),
            number: code(0.6),
            line: reach(reach(dim, bg, 3.), code_bg, 3.),
            selection: accent.opacity(share),
            hover: text.opacity(0.07),
            pressed: text.opacity(0.13),
            code_bg,
            panel,
            border,
            size: prefs.size,
            ui: (prefs.size / 16.).clamp(0.85, 1.5),
            page: Page::of(&prefs.page),
            wide: Wide::of(&prefs.wide),
            bar: prefs.bar != "off",
            motion: prefs.motion != "off",
        }
    }
}

impl Theme {
    /// `v` pixels of the interface at its scale.
    pub fn px(&self, v: f32) -> Pixels {
        px(v * self.ui)
    }

    /// The small radius of the interface: fields, buttons, pills, rows.
    pub fn small(&self) -> Pixels {
        self.px(6.)
    }

    /// The large radius: cards, bars, panels, blocks of the note.
    pub fn large(&self) -> Pixels {
        self.px(8.)
    }

    /// What sits on the page: tooltips, the dragged card, the bars over a canvas.
    pub fn lifted(&self) -> Vec<BoxShadow> {
        vec![BoxShadow { color: hsla(0., 0., 0., 0.18), offset: point(px(0.), self.px(1.)), blur_radius: self.px(6.), spread_radius: px(0.) }]
    }

    /// What covers the page: palette, menus, help, toasts.
    pub fn floating(&self) -> Vec<BoxShadow> {
        vec![BoxShadow { color: hsla(0., 0., 0., 0.28), offset: point(px(0.), self.px(6.)), blur_radius: self.px(20.), spread_radius: px(0.) }]
    }

    /// Background of a highlight (`==text==`) of color `i` (see `md::MARKS`), opaque.
    pub fn highlight(&self, i: usize) -> Hsla {
        let tint = highlight_tint(i);
        // As strong as the text over it allows.
        (0..=30).map(|k| blend(tint, self.bg, 0.4 - k as f32 * 0.01)).find(|c| contrast(self.text, *c) >= 4.5).unwrap_or(self.bg)
    }

    /// One of the five hues of the recipe (`HUES`), readable on `under`: icons of the messages, pills.
    pub fn hue(&self, h: f32, under: Hsla) -> Hsla {
        legible(Hsla { h, s: 0.7, l: 0.5, a: 1. }, under)
    }

    /// Hover of the close button: the red of the callout recipe, translucent like `hover`, so it lies on any surface.
    pub fn danger_hover(&self) -> Hsla {
        Hsla { a: 0.2, ..Hsla { h: RED, s: 0.7, l: 0.5, a: 1. } }
    }

    /// The label of a callout of hue `tint`: legible on the callout's tint (10 % over the page), then drawn
    /// a tenth of the way toward the text so that it does not outshine the body text. With no side band, the
    /// label and the tint alone say the type.
    pub fn callout_label(&self, tint: Hsla) -> Hsla {
        blend(legible(tint, blend(tint, self.bg, 0.1)), self.text, 0.1)
    }
}

/// Radius of the window itself when the app draws its frame (Linux): it matches the system's corners, so it is
/// no part of the two radii of the interface.
pub fn frame_radius() -> Pixels {
    px(12.)
}

/// The shadow around the framed window, inside the transparent margin.
pub fn frame_shadow(blur: Pixels) -> Vec<BoxShadow> {
    vec![BoxShadow { color: hsla(0., 0., 0., 0.35), offset: point(px(0.), px(2.)), blur_radius: blur, spread_radius: px(0.) }]
}

/// The pure color of highlight `i`, for its badge in the formatting bar.
pub fn highlight_tint(i: usize) -> Hsla {
    Hsla { h: HIGHLIGHTS[i.min(HIGHLIGHTS.len() - 1)], s: 0.9, l: 0.55, a: 1. }
}

/// Hues of the highlights, in the order of `md::MARKS`: yellow, green, blue, pink, purple.
const HIGHLIGHTS: [f32; 5] = [0.14, 0.36, 0.58, 0.93, 0.76];

/// The five hues of the callout recipe: green, purple, orange, red, blue.
#[cfg(test)]
const HUES: [f32; 5] = [GREEN, 0.75, 0.09, RED, 0.58];
pub const GREEN: f32 = 0.38;
pub const RED: f32 = 0.;

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
    reach(color, bg, 4.5)
}

/// `color`, moved like `legible` until its contrast with `bg` reaches `ratio`.
pub fn reach(color: Hsla, bg: Hsla, ratio: f32) -> Hsla {
    let step = if luminance(bg) > 0.18 { -0.02 } else { 0.02 };
    let mut color = color;
    while contrast(color, bg) < ratio && (0.0..=1.0).contains(&(color.l + step)) {
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
            // The window is cut into zones by `bg` and `panel` alone, with no line between them.
            assert!(contrast(t.bg, t.panel) >= 1.03, "{name}: bg and panel too close to tell the zones apart");
            assert!(contrast(t.soft, t.bg) >= 4.5 && contrast(t.soft, t.panel) >= 4.5, "{name}: soft");
            assert!(contrast(t.accent, t.bg) >= 4.5, "{name}: accent");
            for h in HUES {
                let tint = Hsla { h, s: 0.7, l: 0.5, a: 1. };
                let under = blend(tint, t.bg, 0.1);
                assert!(contrast(legible(tint, under), under) >= 4.5, "{name}: panel {h}");
                // The label of a callout reads, and stays within a step of the body text.
                let label = contrast(t.callout_label(tint), under);
                assert!(label >= 4.5 && label <= contrast(t.text, under).max(4.5) + 1., "{name}: callout label {h} {label:.2}");
            }
        }
    }

    /// A kanban column (`code_bg`) stands out from the page, and a card (`bg`) from its column, in every theme.
    #[test]
    fn board_columns_stand_out() {
        let mut fails = Vec::new();
        for (name, _) in THEMES {
            for look in [WindowAppearance::Light, WindowAppearance::Dark] {
                let t = Theme::of(&Prefs::parse(&format!("theme={name}\n")), look);
                let (page, card) = (contrast(t.code_bg, t.bg), contrast(t.border, t.code_bg));
                if page < 1.05 || card < 1.1 {
                    fails.push(format!("{name}: column on page {page:.2}, card edge on column {card:.2}"));
                }
            }
        }
        assert!(fails.is_empty(), "{}", fails.join("\n"));
    }

    /// `color` with its opacity, laid over `under`.
    fn over(color: Hsla, under: Hsla) -> Hsla {
        blend(Hsla { a: 1., ..color }, under, color.a)
    }

    /// Every pair of colors the views draw one on the other reads, in each theme: text at 4.5:1,
    /// lines that must be seen at 3:1. A view that puts a color on another background adds its pair here.
    #[test]
    fn every_pair_reads() {
        let mut fails = Vec::new();
        for (name, _) in THEMES {
            let t = Theme::of(&Prefs::parse(&format!("theme={name}\n")), WindowAppearance::Light);
            let (picked, picked_panel) = (over(t.selection, t.bg), over(t.selection, t.panel));
            let mut pairs = vec![
                // Secondary text, and content that steps back (front matter, addresses, comments) in a code block too.
                ("soft/bg", t.soft, t.bg, 4.5),
                ("soft/panel", t.soft, t.panel, 4.5),
                ("soft/code", t.soft, t.code_bg, 4.5),
                // A selected row, a found passage, a commented passage: their text, and a key cap on the row.
                ("text/selection", t.text, picked, 4.5),
                ("text/selection panel", t.text, picked_panel, 4.5),
                ("text/key cap", t.text, over(t.panel.opacity(0.7), picked_panel), 4.5),
                // Links, tags, active icons: on the page, the panel and the selection.
                ("accent/bg", t.accent, t.bg, 4.5),
                ("accent/panel", t.accent, t.panel, 4.5),
                ("accent/selection", t.accent, picked, 4.5),
                ("accent/selection panel", t.accent, picked_panel, 4.5),
                // The selected row of the panel when it has not the focus.
                ("text/selection half", t.text, over(t.selection.opacity(0.5), t.panel), 4.5),
                ("accent/selection half", t.accent, over(t.selection.opacity(0.5), t.panel), 4.5),
                ("string/bg", t.string, t.bg, 4.5),
                ("string/code", t.string, t.code_bg, 4.5),
                ("number/bg", t.number, t.bg, 4.5),
                ("number/code", t.number, t.code_bg, 4.5),
                ("line/bg", t.line, t.bg, 3.),
                ("line/code", t.line, t.code_bg, 3.),
            ];
            for i in 0..HIGHLIGHTS.len() {
                pairs.push(("text/highlight", t.text, t.highlight(i), 4.5));
            }
            for h in HUES {
                pairs.push(("hue/panel", t.hue(h, t.panel), t.panel, 4.5));
            }
            for c in crate::logic::diagram::COLORS {
                pairs.push(("diagram/bg", legible(rgb(c).into(), t.bg), t.bg, 4.5));
            }
            for (what, fg, bg, min) in pairs {
                let got = contrast(fg, bg);
                if got < min {
                    fails.push(format!("{name}: {what} {got:.2}"));
                }
            }
        }
        assert!(fails.is_empty(), "{} pairs under the bar:\n{}", fails.len(), fails.join("\n"));
    }

    /// In each theme, a hovered surface shows on every background it can be over, is never the colour of a
    /// 1 px line, and a selected row hovered then pressed shows three different states.
    #[test]
    fn hover_and_pressed_show() {
        let mut fails = Vec::new();
        for (name, _) in THEMES {
            let t = Theme::of(&Prefs::parse(&format!("theme={name}\n")), WindowAppearance::Light);
            let picked = over(t.selection, t.panel);
            for (what, under) in [("bg", t.bg), ("panel", t.panel), ("code", t.code_bg), ("selection", picked)] {
                let (hover, pressed) = (over(t.hover, under), over(t.pressed, under));
                if contrast(hover, under) < 1.05 || contrast(pressed, hover) < 1.03 {
                    fails.push(format!("{name}: hover or pressed invisible on {what}"));
                }
                if hover == t.border || pressed == t.border {
                    fails.push(format!("{name}: hover is the colour of a line on {what}"));
                }
            }
        }
        assert!(fails.is_empty(), "{}", fails.join("\n"));
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
