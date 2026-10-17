//! Replace in the whole vault, and undoing it.

use super::*;

impl Shell {
    /// Remplacement dans tout le coffre : compte d'abord, et n'écrit qu'une fois le compte
    /// accepté. Chaque note réécrite est gardée telle qu'elle était, pour tout défaire.
    pub(super) fn replace_in_vault(&mut self, swap: markdown::Swap, window: &mut Window, cx: &mut Context<Self>) {
        // La note ouverte est d'abord enregistrée : le remplacement part de ce qui est à l'écran.
        self.flush(cx);
        let found: Vec<(PathBuf, String, usize)> = (self.notes.iter())
            .filter_map(|note| markdown::swap(&note.body, &swap).map(|(text, count)| (note.path.clone(), text, count)))
            .collect();
        let count: usize = found.iter().map(|(.., count)| count).sum();
        if count == 0 {
            return self.say(Tone::Info, tr("Nothing to replace in the vault", "Rien à remplacer dans le coffre"), cx);
        }
        let title = format!("{} {count} · notes : {}", tr("Matches:", "Passages :"), found.len());
        let options = vec![tr("Replace them all", "Tout remplacer").to_string(), tr("Cancel", "Annuler").to_string()];
        let theme = self.theme;
        let palette = cx.new(|cx| Palette::choose(&title, options, "", theme, cx));
        cx.subscribe_in(&palette, window, move |this, palette, event, window, cx| {
            if matches!(event, PaletteEvent::Preview(_)) {
                return;
            }
            let agreed = matches!(event, PaletteEvent::Submit(_)) && palette.read(cx).chosen() == Some(0);
            this.palette = None;
            window.focus(&this.pane().editor.focus_handle(cx));
            if agreed {
                this.swap_notes(found.clone(), cx);
            }
            cx.notify();
        })
        .detach();
        window.focus(&palette.focus_handle(cx));
        self.palette = Some(palette);
        cx.notify();
    }

    /// Écrit le nouveau texte de chaque note, et retient l'ancien.
    pub(super) fn swap_notes(&mut self, found: Vec<(PathBuf, String, usize)>, cx: &mut Context<Self>) {
        self.swapped.clear();
        let mut count = 0;
        for (path, text, hits) in found {
            let Some(note) = self.notes.iter_mut().find(|note| note.path == path) else {
                continue;
            };
            if let Err(e) = vault::write(&path, &text) {
                self.say(Tone::Failed, format!("{} {} : {e}", tr("Not rewritten:", "Non réécrite :"), path.display()), cx);
                continue;
            }
            count += hits;
            (note.tags, note.links) = Note::index(&text);
            self.swapped.push((path, note.set_body(Arc::from(text.as_str())), text));
        }
        self.after_swap(cx);
        let done = format!("{} {count} · notes : {} ({})", tr("Replaced:", "Remplacés :"), self.swapped.len(), tr("the palette can undo it", "la palette peut l'annuler"));
        self.say(Tone::Done, done, cx);
    }

    /// Défait le dernier remplacement dans le coffre : chaque note retrouve son texte d'avant,
    /// sauf celles qui ont changé depuis, laissées telles quelles.
    pub(super) fn undo_swap(&mut self, cx: &mut Context<Self>) {
        self.flush(cx);
        let (mut back, mut kept) = (0, 0);
        for (path, old, new) in std::mem::take(&mut self.swapped) {
            let same = fs::read_to_string(&path).is_ok_and(|now| now == new);
            if same && vault::write(&path, &old).is_ok() {
                if let Some(note) = self.notes.iter_mut().find(|note| note.path == path) {
                    (note.tags, note.links) = Note::index(&old);
                    note.set_body(old);
                }
                back += 1;
            } else {
                kept += 1;
            }
        }
        self.after_swap(cx);
        let mut done = format!("{} {back}", tr("Notes restored:", "Notes rétablies :"));
        if kept > 0 {
            done += &format!(" · {} {kept}", tr("changed since, left as they are:", "modifiées depuis, laissées telles quelles :"));
        }
        self.say(if back + kept == 0 { Tone::Info } else { Tone::Done }, if back + kept == 0 { tr("No replacement to undo", "Aucun remplacement à annuler").to_string() } else { done }, cx);
    }

    /// Après une réécriture de notes : celle qui est ouverte est relue, le graphe suit.
    pub(super) fn after_swap(&mut self, cx: &mut Context<Self>) {
        self.graph_stale = true;
        self.refresh_graph(cx);
        self.each_pane(|this| this.reload(cx));
        cx.notify();
    }
}
