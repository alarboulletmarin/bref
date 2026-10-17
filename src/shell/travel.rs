//! Back and forward through what was shown.

use super::*;

impl Shell {
    /// What is shown in place of the note; nothing for a new note not saved yet.
    pub(super) fn shown(&self) -> Option<Shown> {
        match (&self.pane().listing, self.pane().file_path(), &self.pane().path) {
            (Some(listing), ..) => Some(Shown::List(listing.of.clone())),
            (None, Some(file), _) => Some(Shown::File(file.clone())),
            (None, None, Some(note)) => Some(Shown::Note(note.clone())),
            (None, None, None) => None,
        }
    }

    /// Remembers, in the current history entry, where we are in what it shows:
    /// to call while the view being left still exists.
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

    /// The only place where history moves forward: at each render, what is shown is compared to
    /// its current entry. A preview does not count; neither does going back or forward,
    /// since the current entry is then already what is shown.
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

    /// Back or forward in what was shown; what no longer exists is skipped.
    pub(super) fn travel(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.mark_place(cx);
        // On what the history does not keep (a preview, a new note), "back"
        // returns to the current entry: that is where we came from.
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
            // Deleted by another program: the entry goes, the one before becomes current.
            self.history.remove(entry.shown.path());
            stay = !forward;
        }
        cx.notify();
    }

    /// Shows a history entry, where it was left.
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
                // A table takes back its cell and input; an image has nothing to take back.
                if let Some(sheet) = self.shown_sheet() {
                    sheet.update(cx, |sheet, cx| sheet.set_view(entry.place.at, entry.place.scroll, cx));
                    window.focus(&sheet.focus_handle(cx));
                }
            }
        }
    }
}
