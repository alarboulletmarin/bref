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
}

impl Prefs {
    pub const SIZE: f32 = 16.;

    pub fn parse(text: &str) -> Self {
        let mut prefs =
            Self { theme: String::new(), font: String::new(), mono: String::new(), size: Self::SIZE, updates: true, page: String::new(), wide: String::new(), bar: String::new() };
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
                _ => {}
            }
        }
        prefs.size = prefs.size.clamp(11., 32.);
        prefs
    }

    pub fn to_text(&self) -> String {
        let mut text = format!("theme={}\nfont={}\nmono={}\nsize={}\n", self.theme, self.font, self.mono, self.size);
        // Absent until turned off: the default value does not clutter the file.
        if !self.updates {
            text.push_str("updates=off\n");
        }
        for (key, value) in [("page", &self.page), ("wide", &self.wide), ("bar", &self.bar)] {
            if !value.is_empty() {
                text.push_str(&format!("{key}={value}\n"));
            }
        }
        text
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
        assert!(!prefs.to_text().contains("page=") && !prefs.to_text().contains("wide="));
    }
}
