//! A note of the vault as the app keeps it in memory: name, path, tags, links and text.

use std::{path::PathBuf, sync::Arc, time::SystemTime};

use crate::logic::markdown;

#[derive(Clone)]
pub struct Note {
    pub name: String,
    pub path: PathBuf,
        // Tags, links and aliases are shared: copying a note (vault reread, comparison)
        // does not copy its lists.
    pub tags: Arc<[String]>,
        /// Notes targeted by its `[[wikilinks]]`, lowercase.
    pub links: Arc<[String]>,
        /// Its other names (`aliases` key of the front matter), as written.
    pub aliases: Arc<[String]>,
    pub mtime: SystemTime,
        /// Whole text of the note, for full-text search.
        // ponytail: all the vault's text stays in memory, as is and lowercase (5,000 notes of 6 KB: 60 MB);
        // move to an on-disk index if much bigger vaults show up.
    pub body: Arc<str>,
        /// The same in lowercase: search does not redo it each time the palette opens.
    pub lower: Arc<str>,
}

/// The text in lowercase; the text itself if it already is, without a copy.
pub fn lowered(text: &Arc<str>) -> Arc<str> {
    let lower = text.to_lowercase();
    if lower == **text { text.clone() } else { lower.into() }
}

impl Note {
        /// Stores the note's new text, and returns the old one.
    pub fn set_body(&mut self, text: Arc<str>) -> Arc<str> {
        self.lower = lowered(&text);
        std::mem::replace(&mut self.body, text)
    }

        /// Tags and links of a note's text, ready to be stored in it.
    pub fn index(text: &str) -> (Arc<[String]>, Arc<[String]>) {
        let (tags, links) = markdown::index(text);
        (tags.into(), links.into())
    }

        /// A link written `key` (lowercase) targets this note: by its name, or by an alias.
    pub fn answers(&self, key: &str) -> bool {
        self.name.to_lowercase() == key || self.aliases.iter().any(|alias| alias.to_lowercase() == key)
    }
}
