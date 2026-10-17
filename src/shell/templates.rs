//! Templates: the folder, the daily note template, creating from and saving as a template.

use super::*;

impl Shell {
    /// Les modèles du coffre, par nom.
    pub(super) fn templates(&self) -> Vec<PathBuf> {
        let Some(root) = &self.vault else {
            return Vec::new();
        };
        let mut found: Vec<PathBuf> = self.notes.iter().filter(|n| vault::in_templates(root, &n.path)).map(|n| n.path.clone()).collect();
        found.sort_by_key(|path| vault::natural(&vault::stem(path)));
        found
    }

    /// Le modèle de la note du jour : celui qui s'appelle `Daily`, `Journal`, `Today`, `Note du jour`…
    pub(super) fn daily_template(&self) -> Option<PathBuf> {
        const NAMES: [&str; 5] = ["daily", "journal", "today", "note du jour", "aujourd'hui"];
        self.templates().into_iter().find(|path| NAMES.contains(&lang::fold(&vault::stem(path)).as_str()))
    }

    /// Un modèle à choisir, puis, pour une nouvelle note, son nom ; ou, pour l'insérer, rien d'autre.
    pub(super) fn open_templates(&mut self, act: TemplateAct, window: &mut Window, cx: &mut Context<Self>) {
        if self.vault.is_none() {
            return;
        }
        let found = self.templates();
        if found.is_empty() {
            return self.say(
                Tone::Info,
                tr(
                    "No template: put notes in a folder named Templates (or Modèles) at the root of the vault",
                    "Aucun modèle : rangez des notes dans un dossier Templates (ou Modèles) à la racine du coffre",
                ),
                cx,
            );
        }
        let names = found.iter().map(|path| vault::stem(path)).collect();
        let label = if act == TemplateAct::New { tr("New note from a template", "Nouvelle note depuis un modèle") } else { tr("Insert a template", "Insérer un modèle") };
        let theme = self.theme;
        let palette = cx.new(|cx| Palette::choose(label, names, "", theme, cx));
        cx.subscribe_in(&palette, window, move |this, palette, event, window, cx| {
            if matches!(event, PaletteEvent::Preview(_)) {
                return;
            }
            let chosen = palette.read(cx).chosen().and_then(|i| found.get(i)).cloned().filter(|_| matches!(event, PaletteEvent::Submit(_)));
            this.palette = None;
            window.focus(&this.pane().editor.focus_handle(cx));
            match (chosen, act) {
                (Some(template), TemplateAct::New) => {
                    // La note naît dans le dossier choisi dans l'arbre, jamais dans celui des modèles.
                    let root = this.vault.clone().unwrap_or_default();
                    let dir = this.nav.target_dir().filter(|dir| !vault::in_templates(&root, &dir.join("x.md"))).unwrap_or(root);
                    this.ask(tr("Name of the note", "Nom de la note"), "", window, cx, move |this, name, window, cx| {
                        // La note née, on écrit : le curseur est à sa marque, le focus à l'éditeur.
                        if this.create_from(&template, &name, &dir, cx).is_some() {
                            window.focus(&this.pane().editor.focus_handle(cx));
                        }
                    });
                }
                (Some(template), _) => this.insert_template(&template, cx),
                (None, _) => {}
            }
            cx.notify();
        })
        .detach();
        window.focus(&palette.focus_handle(cx));
        self.palette = Some(palette);
        cx.notify();
    }

    /// La note `name` dans `dir`, écrite d'après le modèle, ouverte, le curseur à sa marque. Rien n'est
    /// écrasé : un nom pris est refusé. `None` si elle n'a pas pu naître.
    pub(super) fn create_from(&mut self, template: &Path, name: &str, dir: &Path, cx: &mut Context<Self>) -> Option<PathBuf> {
        let root = self.vault.clone()?;
        let text = match fs::read_to_string(template) {
            Ok(text) => text,
            Err(e) => {
                self.say(Tone::Failed, format!("{} : {e}", tr("Template not read", "Modèle illisible")), cx);
                return None;
            }
        };
        let path = dir.join(format!("{name}.md"));
        if path.exists() {
            self.say(Tone::Failed, format!("{} « {name} »", tr("A note already has the name", "Une note porte déjà le nom")), cx);
            return None;
        }
        let (body, cursor) = markdown::fill(&text, name, &date_name(today()), &time_name(clock()));
        if let Err(e) = vault::write(&path, &body) {
            self.say(Tone::Failed, format!("{} : {e}", tr("Note not created", "Note non créée")), cx);
            return None;
        }
        self.rescan(&root, cx);
        self.nav.reveal(&path);
        self.open_note(&path, cx);
        self.pane().editor.update(cx, |editor, cx| editor.jump(cursor, false, cx));
        Some(path)
    }

    /// Le modèle, rempli au nom de la note ouverte, à la place du curseur.
    pub(super) fn insert_template(&mut self, template: &Path, cx: &mut Context<Self>) {
        let text = match fs::read_to_string(template) {
            Ok(text) => text,
            Err(e) => return self.say(Tone::Failed, format!("{} : {e}", tr("Template not read", "Modèle illisible")), cx),
        };
        let title = self.pane().path.as_deref().map(vault::stem).unwrap_or_default();
        let (body, cursor) = markdown::fill(&text, &title, &date_name(today()), &time_name(clock()));
        self.pane().editor.update(cx, |editor, cx| editor.insert_filled(&body, cursor, cx));
    }

    /// La note ouverte, rangée comme modèle : son titre devient `{{title}}`.
    pub(super) fn save_as_template(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.vault.is_none() || self.pane().file.is_some() || self.pane().listing.is_some() {
            return;
        }
        self.flush(cx);
        let text = markdown::templatize(self.pane().editor.read(cx).text());
        let name = self.pane().path.as_deref().map(vault::stem).unwrap_or_default();
        self.ask(tr("Name of the template", "Nom du modèle"), &name, window, cx, move |this, name, window, cx| {
            window.focus(&this.pane().editor.focus_handle(cx));
            let Some(root) = this.vault.clone() else { return };
            let dir = vault::template_dir(&root, &this.dirs).unwrap_or_else(|| root.join("Templates"));
            let path = dir.join(format!("{name}.md"));
            if path.exists() {
                return this.say(Tone::Failed, format!("{} « {name} »", tr("A template already has the name", "Un modèle porte déjà le nom")), cx);
            }
            match fs::create_dir_all(&dir).and_then(|()| vault::write(&path, &text)) {
                Ok(()) => {
                    this.rescan(&root, cx);
                    this.say(Tone::Done, format!("{} {name}", tr("Template saved:", "Modèle enregistré :")), cx);
                }
                Err(e) => this.say(Tone::Failed, format!("{} : {e}", tr("Template not saved", "Modèle non enregistré")), cx),
            }
        });
    }

    /// Dessine le pane `pane` (l'appelant l'a rendu actif le temps du dessin) : ce qu'il montre,
    /// et le panneau de ses commentaires. `active` : c'est lui qui a la saisie.
    /// Les notes que le coffre remet sous les yeux aujourd'hui (voir `recall::recall`).
    // ponytail: recalculé à chaque rendu d'une page vide (un hachage par note, 5 000 notes :
    // une fraction de milliseconde) ; le garder d'un rendu à l'autre si de bien plus gros
    // coffres le font sentir.
    /// La note est un modèle du coffre.
    pub(super) fn is_template(&self, note: &Note) -> bool {
        self.vault.as_ref().is_some_and(|root| vault::in_templates(root, &note.path))
    }
}
