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
}

impl Prefs {
    pub const SIZE: f32 = 16.;
    pub const OPACITY: f32 = 0.9;
    /// The lowest opacity of the ground: below it, some theme's text no longer reads on it over a white or a
    /// black desktop (`ui::theme`, `a_translucent_ground_still_reads`).
    pub const FLOOR: f32 = 0.75;

    pub fn parse(text: &str) -> Self {
        let mut prefs =
            Self { theme: String::new(), font: String::new(), mono: String::new(), size: Self::SIZE, updates: true, page: String::new(), wide: String::new(), bar: String::new(), motion: String::new(), background: String::new(), opacity: Self::OPACITY };
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
        for (key, value) in [("page", &self.page), ("wide", &self.wide), ("bar", &self.bar), ("motion", &self.motion), ("background", &self.background)] {
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
}
