//! A note of the vault as the app keeps it in memory: name, path, tags, links and text.

use std::{path::PathBuf, sync::Arc, time::SystemTime};

use crate::logic::markdown;

/// What a `[[link]]` names, folded to lowercase: the key under which a note answers, by its
/// name or by an alias. Always built by `NoteKey::of`, so two keys compare the way links match.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct NoteKey(String);

impl NoteKey {
    pub fn of(name: &str) -> NoteKey {
        NoteKey(name.to_lowercase())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Tags, links and aliases of a text.
type Indexed = (Arc<[String]>, Arc<[NoteKey]>, Arc<[String]>);

#[derive(Clone)]
pub struct Note {
    pub name: String,
    pub path: PathBuf,
        // Tags, links and aliases are shared: copying a note (vault reread, comparison)
        // does not copy its lists.
    pub tags: Arc<[String]>,
        /// Notes targeted by its `[[wikilinks]]`, lowercase.
    pub links: Arc<[NoteKey]>,
        /// Its other names (`aliases` key of the front matter), as written.
    pub aliases: Arc<[String]>,
    pub mtime: SystemTime,
        /// Whole text of the note, for full-text search: searched as written, whatever the case
        /// (`rank::Words`), so it is kept once.
        // ponytail: all the vault's text stays in memory (5,000 notes of 6 KB: 30 MB);
        // move to an on-disk index if much bigger vaults show up.
    pub body: Arc<str>,
}

impl Note {
        /// Tags, links and aliases of a note's text. A template has none: what it writes
        /// between `{{ }}` or after `#` is not its own.
    pub fn index(text: &str, template: bool) -> Indexed {
        if template {
            return (Arc::from([]), Arc::from([]), Arc::from([]));
        }
        let (tags, links) = markdown::index(text);
        (tags.into(), links.iter().map(|l| NoteKey::of(l)).collect(), markdown::aliases(text).into())
    }

        /// A note read from its text: the one way to build everything derived from it.
    pub fn from_text(name: String, path: PathBuf, mtime: SystemTime, text: Arc<str>, template: bool) -> Note {
        let (tags, links, aliases) = Note::index(&text, template);
        Note { name, path, tags, links, aliases, mtime, body: text }
    }

        /// Stores the note's new text with all that derives from it, and returns the old text.
    pub fn set_text(&mut self, text: Arc<str>, template: bool) -> Arc<str> {
        (self.tags, self.links, self.aliases) = Note::index(&text, template);
        std::mem::replace(&mut self.body, text)
    }

        /// A link written `key` targets this note: by its name, or by an alias.
    pub fn answers(&self, key: &NoteKey) -> bool {
        NoteKey::of(&self.name) == *key || self.aliases.iter().any(|alias| NoteKey::of(alias) == *key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "---\naliases: [Pseudo]\n---\n# A\n#étiquette [[Cible]]\n";

    fn note(text: &str, template: bool) -> Note {
        Note::from_text("A".into(), "/v/A.md".into(), SystemTime::UNIX_EPOCH, Arc::from(text), template)
    }

    #[test]
    fn from_text_derives_everything() {
        let n = note(TEXT, false);
        assert_eq!((&n.tags[..], &n.links[..], &n.aliases[..]), (&["étiquette".to_string()][..], &[NoteKey::of("cible")][..], &["Pseudo".to_string()][..]));
        // A template keeps the text and nothing derived from it; empty text is fine.
        let t = note(TEXT, true);
        assert!(t.tags.is_empty() && t.links.is_empty() && t.aliases.is_empty() && *t.body == *TEXT);
        let e = note("", false);
        assert!(e.tags.is_empty() && e.links.is_empty() && e.aliases.is_empty());
    }

    #[test]
    fn set_text_leaves_nothing_stale() {
        let mut n = note(TEXT, false);
        let old = n.set_text(Arc::from("# A\nMOT\n"), false);
        assert_eq!(*old, *TEXT);
        assert!(n.tags.is_empty() && n.links.is_empty() && n.aliases.is_empty());
        assert_eq!(&*n.body, "# A\nMOT\n");
        // The same note rewritten as a template drops its aliases, tags and links.
        n.set_text(Arc::from(TEXT), true);
        assert!(n.tags.is_empty() && n.links.is_empty() && n.aliases.is_empty());
        // And back: the same as a fresh read.
        n.set_text(Arc::from(TEXT), false);
        assert_eq!(n.aliases[..], ["Pseudo".to_string()]);
    }
}
