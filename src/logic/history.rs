//! History of what was shown in place of the note, for "back" and "forward"
//! (pure functions). In memory only: the recent notes already keep the rest from one
//! launch to the next.

use std::path::{Path, PathBuf};

/// What takes the note's place.
#[derive(Clone, Debug, PartialEq)]
pub enum Shown {
    Note(PathBuf),
    /// An image, a diagram or a table.
    File(PathBuf),
    /// The list page of a folder or a tag.
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

/// Where one was: the cursor (byte in a note; row and column in a table) and the
/// scroll.
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

/// Number of entries kept: beyond that, the oldest go.
const KEPT: usize = 100;

/// A list and a position, as in a browser.
#[derive(Default)]
pub struct History {
    list: Vec<Entry>,
    at: usize,
}

impl History {
    pub fn current(&self) -> Option<&Entry> {
        self.list.get(self.at)
    }

    /// What was just shown; what followed in the list is forgotten.
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

    /// Remembers where one is in `shown`, if it is indeed the current entry.
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

    /// `from` became `to` (a file, or a folder with what it contains).
    pub fn rename(&mut self, from: &Path, to: &Path) {
        for entry in &mut self.list {
            let path = entry.shown.path_mut();
            if let Ok(rest) = path.strip_prefix(from) {
                *path = if rest.as_os_str().is_empty() { to.to_path_buf() } else { to.join(rest) };
            }
        }
        self.keep(|_| true);
    }

    /// `gone` no longer exists, nor anything it contained.
    pub fn remove(&mut self, gone: &Path) {
        self.keep(|entry| !entry.shown.path().starts_with(gone));
    }

    /// Keeps only the wanted entries, without two identical neighbours; the position stays on
    /// the current entry, otherwise on the last one kept before it.
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
        // On Windows, a rebuilt path is written with `\\`.
        history.list.iter().map(|entry| entry.shown.path().display().to_string().replace('\\', "/")).collect()
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
        // Showing again what is already there adds nothing.
        h.visit(note("c"));
        assert_eq!(names(&h), ["a", "b", "c"]);
        assert!(h.can_back() && !h.can_forward());
        assert_eq!(h.back().map(|e| e.shown), Some(note("b")));
        assert_eq!(h.back().map(|e| e.shown), Some(note("a")));
        assert!(h.back().is_none() && h.can_forward());
        assert_eq!(h.forward().map(|e| e.shown), Some(note("b")));
        // Opening something else after going back: what followed is forgotten.
        h.visit(Shown::File("d.png".into()));
        assert_eq!(names(&h), ["a", "b", "d.png"]);
        assert!(!h.can_forward() && h.forward().is_none());
        // The same note, then a list of the same path: two different things.
        h.visit(Shown::List("a".into()));
        h.visit(note("a"));
        assert_eq!(names(&h).len(), 5);
    }

    #[test]
    fn remembers_where_one_was() {
        let mut h = History::default();
        h.visit(note("a"));
        // Only the current entry gets the position, and only if it is indeed the one being left.
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
        // A renamed folder carries everything it contains, and its own list.
        h.rename(Path::new("dir"), Path::new("new"));
        assert_eq!(names(&h), ["new/a.md", "b.md", "new/t.csv", "new", "b.md"]);
        h.rename(Path::new("b.md"), Path::new("c.md"));
        assert_eq!(names(&h), ["new/a.md", "c.md", "new/t.csv", "new", "c.md"]);
        // Deleted: the entries go, two neighbours that became identical make only one, and
        // we stay on what is shown.
        h.remove(Path::new("new"));
        assert_eq!(names(&h), ["c.md"]);
        assert_eq!(h.current().map(|e| e.shown.clone()), Some(note("c.md")));
        assert!(!h.can_back() && !h.can_forward());
        // The current entry disappears: the previous one replaces it.
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
