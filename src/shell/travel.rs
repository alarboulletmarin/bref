//! Back and forward through what was shown.

use super::*;

impl Shell {
    /// Ce qui est affiché à la place de la note ; rien pour une note neuve pas encore enregistrée.
    pub(super) fn shown(&self) -> Option<Shown> {
        match (&self.pane().listing, self.pane().file_path(), &self.pane().path) {
            (Some(listing), ..) => Some(Shown::List(listing.of.clone())),
            (None, Some(file), _) => Some(Shown::File(file.clone())),
            (None, None, Some(note)) => Some(Shown::Note(note.clone())),
            (None, None, None) => None,
        }
    }

    /// Retient, dans l'entrée courante de l'historique, où l'on en est dans ce qu'elle montre :
    /// à appeler tant que la vue qu'on quitte existe encore.
    pub(super) fn mark_place(&mut self, cx: &App) {
        let Some(shown) = self.history.current().map(|entry| entry.shown.clone()) else { return };
        let place = match &shown {
            Shown::Note(path) if self.pane().path.as_ref() == Some(path) => {
                let (at, scroll) = self.pane().editor.read(cx).view();
                Place { at: (at, 0), scroll: (0., scroll) }
            }
            Shown::File(path) => match self.pane().sheet().filter(|_| self.pane().file_path() == Some(path)) {
                Some(sheet) => {
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
    pub(super) fn track(&mut self, cx: &App) {
        if self.pane().peek {
            return;
        }
        let Some(shown) = self.shown() else { return };
        if self.history.current().is_none_or(|entry| entry.shown != shown) {
            self.mark_place(cx);
            self.history.visit(shown);
        }
    }

    /// Précédent ou suivant dans ce qui a été affiché ; ce qui n'existe plus est sauté.
    pub(super) fn travel(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
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
    pub(super) fn show(&mut self, entry: history::Entry, window: &mut Window, cx: &mut Context<Self>) {
        self.pane_mut().peek = false;
        if self.nav.panel == nav::Panel::Full {
            self.nav.panel = nav::Panel::Split;
        }
        match entry.shown {
            Shown::List(of) => self.open_listing(of, window, cx),
            Shown::Note(path) => {
                if self.load_note(&path, cx) {
                    self.pane_mut().preview = false;
                    self.pane().editor.update(cx, |editor, cx| editor.set_view(entry.place.at.0, entry.place.scroll.1, cx));
                }
                window.focus(&self.pane().editor.focus_handle(cx));
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
}
