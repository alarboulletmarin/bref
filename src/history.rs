//! Historique de ce qui a été affiché à la place de la note, pour « précédent » et « suivant »
//! (fonctions pures). En mémoire seulement : les récents retiennent déjà le reste d'un
//! lancement à l'autre.

use std::path::{Path, PathBuf};

/// Ce qui prend la place de la note.
#[derive(Clone, Debug, PartialEq)]
pub enum Shown {
    Note(PathBuf),
    /// Une image, un schéma ou un tableau.
    File(PathBuf),
    /// La page liste d'un dossier ou d'un tag.
    List(PathBuf),
}

impl Shown {
    pub fn path(&self) -> &Path {
        let (Shown::Note(path) | Shown::File(path) | Shown::List(path)) = self;
        path
    }

    fn path_mut(&mut self) -> &mut PathBuf {
        let (Shown::Note(path) | Shown::File(path) | Shown::List(path)) = self;
        path
    }
}

/// Où l'on en était : le curseur (octet dans une note ; ligne et colonne dans un tableau) et le
/// défilement.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Place {
    pub at: (usize, usize),
    pub scroll: (f32, f32),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub shown: Shown,
    pub place: Place,
}

/// Nombre d'entrées gardées : au-delà, les plus anciennes partent.
const KEPT: usize = 100;

/// Une liste et une position, comme dans un navigateur.
#[derive(Default)]
pub struct History {
    list: Vec<Entry>,
    at: usize,
}

impl History {
    pub fn current(&self) -> Option<&Entry> {
        self.list.get(self.at)
    }

    /// Ce qui vient d'être affiché ; ce qui suivait dans la liste est oublié.
    pub fn visit(&mut self, shown: Shown) {
        if self.current().is_some_and(|entry| entry.shown == shown) {
            return;
        }
        self.list.truncate(self.at + 1);
        self.list.push(Entry { shown, place: Place::default() });
        if self.list.len() > KEPT {
            self.list.remove(0);
        }
        self.at = self.list.len() - 1;
    }

    /// Retient où l'on en est dans `shown`, si c'est bien l'entrée courante.
    pub fn mark(&mut self, shown: &Shown, place: Place) {
        if let Some(entry) = self.list.get_mut(self.at).filter(|entry| entry.shown == *shown) {
            entry.place = place;
        }
    }

    pub fn can_back(&self) -> bool {
        self.at > 0
    }

    pub fn can_forward(&self) -> bool {
        self.at + 1 < self.list.len()
    }

    pub fn back(&mut self) -> Option<Entry> {
        self.can_back().then(|| {
            self.at -= 1;
            self.list[self.at].clone()
        })
    }

    pub fn forward(&mut self) -> Option<Entry> {
        self.can_forward().then(|| {
            self.at += 1;
            self.list[self.at].clone()
        })
    }

    /// `from` est devenu `to` (un fichier, ou un dossier avec ce qu'il contient).
    pub fn rename(&mut self, from: &Path, to: &Path) {
        for entry in &mut self.list {
            let path = entry.shown.path_mut();
            if let Ok(rest) = path.strip_prefix(from) {
                *path = if rest.as_os_str().is_empty() { to.to_path_buf() } else { to.join(rest) };
            }
        }
        self.keep(|_| true);
    }

    /// `gone` n'existe plus, ni rien de ce qu'il contenait.
    pub fn remove(&mut self, gone: &Path) {
        self.keep(|entry| !entry.shown.path().starts_with(gone));
    }

    /// Ne garde que les entrées voulues, sans deux voisines identiques ; la position reste sur
    /// l'entrée courante, sinon sur la dernière gardée avant elle.
    fn keep(&mut self, wanted: impl Fn(&Entry) -> bool) {
        let (mut list, mut at): (Vec<Entry>, usize) = (Vec::new(), 0);
        for (i, entry) in std::mem::take(&mut self.list).into_iter().enumerate() {
            if wanted(&entry) && list.last().is_none_or(|last| last.shown != entry.shown) {
                list.push(entry);
            }
            if i == self.at {
                at = list.len().saturating_sub(1);
            }
        }
        (self.list, self.at) = (list, at);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(name: &str) -> Shown {
        Shown::Note(PathBuf::from(name))
    }

    fn names(history: &History) -> Vec<String> {
        history.list.iter().map(|entry| entry.shown.path().display().to_string()).collect()
    }

    fn at(cursor: usize) -> Place {
        Place { at: (cursor, 0), scroll: (0., cursor as f32) }
    }

    #[test]
    fn walks_back_and_forward_like_a_browser() {
        let mut h = History::default();
        assert!(!h.can_back() && !h.can_forward() && h.back().is_none());
        for name in ["a", "b", "c"] {
            h.visit(note(name));
        }
        // Revoir ce qui est déjà là n'ajoute rien.
        h.visit(note("c"));
        assert_eq!(names(&h), ["a", "b", "c"]);
        assert!(h.can_back() && !h.can_forward());
        assert_eq!(h.back().map(|e| e.shown), Some(note("b")));
        assert_eq!(h.back().map(|e| e.shown), Some(note("a")));
        assert!(h.back().is_none() && h.can_forward());
        assert_eq!(h.forward().map(|e| e.shown), Some(note("b")));
        // Ouvrir autre chose après un retour : ce qui suivait est oublié.
        h.visit(Shown::File("d.png".into()));
        assert_eq!(names(&h), ["a", "b", "d.png"]);
        assert!(!h.can_forward() && h.forward().is_none());
        // La même note, puis une liste du même chemin : deux choses différentes.
        h.visit(Shown::List("a".into()));
        h.visit(note("a"));
        assert_eq!(names(&h).len(), 5);
    }

    #[test]
    fn remembers_where_one_was() {
        let mut h = History::default();
        h.visit(note("a"));
        // Seule l'entrée courante reçoit la position, et seulement si c'est bien elle qu'on quitte.
        h.mark(&note("b"), at(9));
        h.mark(&note("a"), at(42));
        h.visit(note("b"));
        h.mark(&note("b"), at(7));
        let back = h.back().unwrap();
        assert_eq!((back.shown, back.place), (note("a"), at(42)));
        assert_eq!(h.forward().unwrap().place, at(7));
    }

    #[test]
    fn keeps_a_hundred() {
        let mut h = History::default();
        for i in 0..150 {
            h.visit(note(&i.to_string()));
        }
        assert_eq!(h.list.len(), KEPT);
        assert_eq!(h.current().map(|e| e.shown.clone()), Some(note("149")));
        assert_eq!(names(&h)[0], "50");
    }

    #[test]
    fn follows_renames_and_forgets_what_is_gone() {
        let mut h = History::default();
        for shown in [note("dir/a.md"), note("b.md"), Shown::File("dir/t.csv".into()), Shown::List("dir".into()), note("b.md")] {
            h.visit(shown);
        }
        // Un dossier renommé emporte tout ce qu'il contient, et sa propre liste.
        h.rename(Path::new("dir"), Path::new("new"));
        assert_eq!(names(&h), ["new/a.md", "b.md", "new/t.csv", "new", "b.md"]);
        h.rename(Path::new("b.md"), Path::new("c.md"));
        assert_eq!(names(&h), ["new/a.md", "c.md", "new/t.csv", "new", "c.md"]);
        // Supprimé : les entrées partent, deux voisines devenues identiques n'en font qu'une, et
        // l'on reste sur ce qui est affiché.
        h.remove(Path::new("new"));
        assert_eq!(names(&h), ["c.md"]);
        assert_eq!(h.current().map(|e| e.shown.clone()), Some(note("c.md")));
        assert!(!h.can_back() && !h.can_forward());
        // L'entrée courante disparaît : la précédente la remplace.
        let mut h = History::default();
        for name in ["a", "b", "c"] {
            h.visit(note(name));
        }
        h.back();
        h.remove(Path::new("b"));
        assert_eq!(names(&h), ["a", "c"]);
        assert_eq!(h.current().map(|e| e.shown.clone()), Some(note("a")));
        assert_eq!(h.forward().map(|e| e.shown), Some(note("c")));
    }
}
