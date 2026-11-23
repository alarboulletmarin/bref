//! User preferences (`settings` file): parsed from and written to `key=value` lines.

/// Appearance chosen by the user; an empty field keeps the default value.
#[derive(Clone, Debug, PartialEq)]
pub struct Prefs {
    pub theme: String,
    pub font: String,
    pub mono: String,
    /// Size of the note's body text, in pixels.
    pub size: f32,
    /// Look for a new version of the app (one request per day).
    pub updates: bool,
    /// Width of the text column and placement of wide tables (keys of `editor::Page` and
    /// `editor::Wide`); empty: the default choice.
    pub page: String,
    pub wide: String,
    /// Formatting bar at the bottom of the note: `off` hides it, anything else shows it.
    pub bar: String,
    /// Short fades when something opens: `off` makes everything instant, anything else keeps them.
    pub motion: String,
    /// The window's ground, behind the sheets: `translucent` or `blurred` lets the desktop through
    /// at `opacity`; anything else keeps it opaque.
    pub background: String,
    pub opacity: f32,
    /// The accent over the theme's, as `#rrggbb`; empty: the theme's own.
    pub accent: String,
}

impl Prefs {
    pub const SIZE: f32 = 16.;
    pub const OPACITY: f32 = 0.9;
    /// The lowest opacity of the ground: below it, some theme's text no longer reads on it over a white or a
    /// black desktop (`ui::theme`, `a_translucent_ground_still_reads`).
    pub const FLOOR: f32 = 0.75;

    pub fn parse(text: &str) -> Self {
        let mut prefs =
            Self { theme: String::new(), font: String::new(), mono: String::new(), size: Self::SIZE, updates: true, page: String::new(), wide: String::new(), bar: String::new(), motion: String::new(), background: String::new(), opacity: Self::OPACITY, accent: String::new() };
        for (key, value) in text.lines().filter_map(|line| line.split_once('=')) {
            match key {
                "theme" => prefs.theme = value.into(),
                "font" => prefs.font = value.into(),
                "mono" => prefs.mono = value.into(),
                "size" => prefs.size = value.parse().ok().filter(|s: &f32| s.is_finite()).unwrap_or(Self::SIZE),
                "updates" => prefs.updates = value != "off",
                "page" => prefs.page = value.into(),
                "wide" => prefs.wide = value.into(),
                "bar" => prefs.bar = value.into(),
                "motion" => prefs.motion = value.into(),
                "background" => prefs.background = value.into(),
                "opacity" => prefs.opacity = value.parse().ok().filter(|o: &f32| o.is_finite()).unwrap_or(Self::OPACITY),
                // A line that is no colour keeps the theme's accent.
                "accent" => prefs.accent = hex_color(value).map_or_else(String::new, write_hex),
                _ => {}
            }
        }
        prefs.size = prefs.size.clamp(11., 32.);
        // Kept to the hundredth: steps of 5 % add up without drifting (0.85 + 0.05 is not 0.9 in f32).
        prefs.opacity = (prefs.opacity.clamp(Self::FLOOR, 1.) * 100.).round() / 100.;
        prefs
    }

    pub fn to_text(&self) -> String {
        let mut text = format!("theme={}\nfont={}\nmono={}\nsize={}\n", self.theme, self.font, self.mono, self.size);
        // Absent until turned off: the default value does not clutter the file.
        if !self.updates {
            text.push_str("updates=off\n");
        }
        for (key, value) in [("page", &self.page), ("wide", &self.wide), ("bar", &self.bar), ("motion", &self.motion), ("background", &self.background), ("accent", &self.accent)] {
            if !value.is_empty() {
                text.push_str(&format!("{key}={value}\n"));
            }
        }
        if self.opacity != Self::OPACITY {
            text.push_str(&format!("opacity={}\n", self.opacity));
        }
        text
    }

    /// The opacity of the window's ground: 1 when it is opaque.
    pub fn ground_alpha(&self) -> f32 {
        if matches!(self.background.as_str(), "translucent" | "blurred") { self.opacity } else { 1. }
    }

    /// The system's blur asked for behind the window.
    pub fn blurred(&self) -> bool {
        self.background == "blurred"
    }

    /// The accent chosen over the theme's, as `0xrrggbb`; `None`: the theme's own.
    pub fn accent_rgb(&self) -> Option<u32> {
        hex_color(&self.accent)
    }
}

/// A colour typed as `#rrggbb` or `#rgb` (the `#` may be left out, any case), as `0xrrggbb`; anything else
/// (`red`, `#12`, `#gggggg`) is `None`.
pub fn hex_color(text: &str) -> Option<u32> {
    let digits = text.trim();
    let digits = digits.strip_prefix('#').unwrap_or(digits);
    if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    match digits.len() {
        6 => u32::from_str_radix(digits, 16).ok(),
        // Each digit doubled: `#f80` is `#ff8800`.
        3 => u32::from_str_radix(&digits.chars().flat_map(|c| [c, c]).collect::<String>(), 16).ok(),
        _ => None,
    }
}

/// `0xrrggbb` written as the settings file keeps it: `#rrggbb`, lowercase.
pub fn write_hex(rgb: u32) -> String {
    format!("#{:06x}", rgb & 0xff_ffff)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_settings() {
        let prefs = Prefs::parse("theme=Dracula\nfont=Inter\nsize=99\nbogus\nmono=\n");
        assert_eq!((prefs.theme.as_str(), prefs.font.as_str(), prefs.mono.as_str()), ("Dracula", "Inter", ""));
        // Size clamped, and unreadable value ignored.
        assert_eq!((prefs.size, Prefs::parse("size=NaN").size), (32., 16.));
        assert_eq!(Prefs::parse(&prefs.to_text()), prefs);
        // The new-version check is on until turned off.
        assert!(prefs.updates && !Prefs::parse("updates=off\n").updates && Prefs::parse("updates=on").updates);
        assert!(!Prefs::parse(&Prefs::parse("updates=off").to_text()).updates);
        // Page width and wide tables: absent until chosen.
        let wide = Prefs::parse("page=wide\nwide=column\n");
        assert_eq!((wide.page.as_str(), wide.wide.as_str()), ("wide", "column"));
        assert_eq!(Prefs::parse(&wide.to_text()), wide);
        // Background: opaque until chosen, an unknown value too; the opacity only counts once translucent.
        assert_eq!((Prefs::parse("").ground_alpha(), Prefs::parse("background=glass\nopacity=0.8").ground_alpha()), (1., 1.));
        let glass = Prefs::parse("background=blurred\nopacity=0.8\n");
        assert_eq!((glass.ground_alpha(), glass.blurred()), (0.8, true));
        assert_eq!(Prefs::parse(&glass.to_text()), glass);
        assert!(!Prefs::parse("").to_text().contains("opacity") && !Prefs::parse("").to_text().contains("background"));
        // Opacity: kept between the floor and 1, an unreadable or empty value gives the default.
        let alpha = |text: &str| Prefs::parse(&format!("background=translucent\n{text}")).ground_alpha();
        assert_eq!([alpha("opacity=0"), alpha("opacity=-1"), alpha("opacity=2"), alpha("opacity=NaN"), alpha("opacity="), alpha("opacity=0.854")], [Prefs::FLOOR, Prefs::FLOOR, 1., Prefs::OPACITY, Prefs::OPACITY, 0.85]);
        assert_eq!(Prefs::parse(&format!("opacity={}", 0.85f32 + 0.05)).opacity, 0.9);
        // Motion: absent until chosen, anything but `off` keeps the fades.
        let still = Prefs::parse("motion=off\n");
        assert_eq!(still.motion, "off");
        assert_eq!(Prefs::parse(&still.to_text()), still);
        assert!(!prefs.to_text().contains("motion="));
        assert!(!prefs.to_text().contains("page=") && !prefs.to_text().contains("wide="));
    }

    #[test]
    fn reads_and_writes_the_accent() {
        // Absent until chosen: the theme's own.
        let none = Prefs::parse("theme=Nord\n");
        assert_eq!((none.accent.as_str(), none.accent_rgb()), ("", None));
        assert!(!none.to_text().contains("accent"));
        // Six digits or three, with or without `#`, any case: written back long and lowercase.
        for (typed, kept) in [("#3E63DD", "#3e63dd"), ("3e63dd", "#3e63dd"), ("#f80", "#ff8800"), (" #ABC ", "#aabbcc"), ("#000000", "#000000")] {
            let prefs = Prefs::parse(&format!("accent={typed}\n"));
            assert_eq!(prefs.accent, kept, "{typed}");
            assert!(prefs.to_text().contains(&format!("accent={kept}\n")));
            assert_eq!(Prefs::parse(&prefs.to_text()), prefs);
        }
        assert_eq!(Prefs::parse("accent=#ff8800").accent_rgb(), Some(0xff8800));
        // What is no colour is refused, and a broken line keeps the theme's accent.
        for bad in ["#12", "red", "#gggggg", "#1234567", "#", "", "##123456", "#12 345"] {
            assert_eq!(hex_color(bad), None, "{bad}");
            assert_eq!(Prefs::parse(&format!("accent={bad}\n")).accent, "", "{bad}");
        }
        assert_eq!((write_hex(0xabc), write_hex(0x1ff_ffff)), ("#000abc".to_string(), "#ffffff".to_string()));
    }
}
