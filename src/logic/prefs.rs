//! User preferences (`settings` file): parsed from and written to `key=value` lines.

/// Apparence choisie par l'utilisateur ; un champ vide garde la valeur par défaut.
#[derive(Clone, Debug, PartialEq)]
pub struct Prefs {
    pub theme: String,
    pub font: String,
    pub mono: String,
    /// Taille du texte courant de la note, en pixels.
    pub size: f32,
    /// Chercher une nouvelle version de l'app (une requête par jour).
    pub updates: bool,
    /// Largeur de la colonne de texte et place des grands tableaux (clés de `editor::Page` et
    /// `editor::Wide`) ; vide : le choix par défaut.
    pub page: String,
    pub wide: String,
    /// Barre de mise en forme du bas de la note : `off` la cache, le reste l'affiche.
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
        // Absent tant qu'on ne l'a pas coupé : la valeur par défaut n'encombre pas le fichier.
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
        // Taille bornée, et valeur illisible ignorée.
        assert_eq!((prefs.size, Prefs::parse("size=NaN").size), (32., 16.));
        assert_eq!(Prefs::parse(&prefs.to_text()), prefs);
        // La recherche de nouvelle version est active tant qu'on ne l'a pas coupée.
        assert!(prefs.updates && !Prefs::parse("updates=off\n").updates && Prefs::parse("updates=on").updates);
        assert!(!Prefs::parse(&Prefs::parse("updates=off").to_text()).updates);
        // Largeur de page et grands tableaux : absents tant qu'on ne les a pas choisis.
        let wide = Prefs::parse("page=wide\nwide=column\n");
        assert_eq!((wide.page.as_str(), wide.wide.as_str()), ("wide", "column"));
        assert_eq!(Prefs::parse(&wide.to_text()), wide);
        assert!(!prefs.to_text().contains("page=") && !prefs.to_text().contains("wide="));
    }
}
