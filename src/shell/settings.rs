//! Settings: theme, fonts, text size and the other choices of the palette.

use super::*;

impl Shell {
    /// Applique les réglages d'apparence à toute la fenêtre.
    pub(super) fn restyle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
    pub(super) fn choose_setting(&mut self, setting: Setting, window: &mut Window, cx: &mut Context<Self>) {
        if self.vault.is_none() {
            return;
        }
        let default = tr("Default", "Par défaut");
        // Largeur de page et grands tableaux : une courte liste fixe, sans « Par défaut » ; le
        // choix se range sous sa clé, ses libellés suivent la langue.
        let fixed: Option<Vec<(&'static str, &'static str)>> = match setting {
            Setting::Page => Some(Page::ALL.map(|p| (p.key(), p.label())).into()),
            Setting::Wide => Some(Wide::ALL.map(|w| (w.key(), w.label())).into()),
            Setting::Bar => Some(vec![("on", tr("Shown", "Affichée")), ("off", tr("Hidden", "Masquée"))]),
            _ => None,
        };
        let (mut options, current): (Vec<String>, &str) = match (setting, &fixed) {
            (Setting::Page, Some(list)) => (list.iter().map(|o| o.1.to_string()).collect(), Page::of(&self.prefs.page).label()),
            (Setting::Wide, Some(list)) => (list.iter().map(|o| o.1.to_string()).collect(), Wide::of(&self.prefs.wide).label()),
            (Setting::Bar, Some(list)) => (list.iter().map(|o| o.1.to_string()).collect(), if self.prefs.bar == "off" { list[1].1 } else { list[0].1 }),
            (Setting::Theme, _) => (THEMES.iter().map(|t| t.0.to_string()).collect(), &self.prefs.theme),
            _ => (
                FAMILIES.get().into_iter().flatten().map(|f| f.to_string()).collect(),
                if setting == Setting::Font { &self.prefs.font } else { &self.prefs.mono },
            ),
        };
        let current = if fixed.is_some() {
            current
        } else {
            // Le choix en cours remonte en tête, juste sous « Par défaut ».
            if let Some(i) = options.iter().position(|o| o == current) {
                let chosen = options.remove(i);
                options.insert(0, chosen);
            }
            options.insert(0, default.to_string());
            if current.is_empty() { default } else { current }
        };
        let (theme, before) = (self.theme, self.prefs.clone());
        let palette = cx.new(|cx| Palette::choose(setting.label(), options, current, theme, cx));
        cx.subscribe_in(&palette, window, move |this, _, event, window, cx| {
            match event {
                PaletteEvent::Preview(name) | PaletteEvent::Submit(name) => {
                    let value = if name == default { String::new() } else { name.clone() };
                    // Le libellé choisi redevient la clé qu'on enregistre.
                    let key = fixed.iter().flatten().find(|o| o.1 == name).map_or_else(String::new, |o| o.0.to_string());
                    match setting {
                        Setting::Theme => this.prefs.theme = value,
                        Setting::Font => this.prefs.font = value,
                        Setting::Mono => this.prefs.mono = value,
                        Setting::Page => this.prefs.page = key,
                        Setting::Wide => this.prefs.wide = key,
                        Setting::Bar => this.prefs.bar = key,
                    }
                }
                _ => this.prefs = before.clone(),
            }
            if !matches!(event, PaletteEvent::Preview(_)) {
                this.palette = None;
                window.focus(&this.pane().editor.focus_handle(cx));
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
    pub(super) fn resize_text(&mut self, by: Option<f32>, window: &mut Window, cx: &mut Context<Self>) {
        let size = by.map_or(Prefs::SIZE, |by| self.prefs.size + by);
        self.prefs = Prefs::parse(&Prefs { size, ..self.prefs.clone() }.to_text());
        vault::save_settings(&self.prefs.to_text());
        self.restyle(window, cx);
    }
}
