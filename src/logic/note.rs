//! A note of the vault as the app keeps it in memory: name, path, tags, links and text.

use std::{path::PathBuf, sync::Arc, time::SystemTime};

use crate::logic::markdown;

#[derive(Clone)]
pub struct Note {
    pub name: String,
    pub path: PathBuf,
    // Tags, liens et alias sont partagés : copier une note (relecture du coffre, comparaison)
    // ne recopie pas ses listes.
    pub tags: Arc<[String]>,
    /// Notes visées par ses `[[wikiliens]]`, en minuscules.
    pub links: Arc<[String]>,
    /// Ses autres noms (clé `aliases` de l'en-tête YAML), tels qu'écrits.
    pub aliases: Arc<[String]>,
    pub mtime: SystemTime,
    /// Texte entier de la note, pour la recherche plein texte.
    // ponytail: tout le texte du coffre reste en mémoire, tel quel et en minuscules (5 000 notes de 6 Ko : 60 Mo) ;
    // passer à un index sur disque si des coffres bien plus gros se présentent.
    pub body: Arc<str>,
    /// Le même en minuscules : la recherche ne le refait pas à chaque ouverture de la palette.
    pub lower: Arc<str>,
}

/// Le texte en minuscules ; le texte lui-même s'il l'est déjà, sans copie.
pub fn lowered(text: &Arc<str>) -> Arc<str> {
    let lower = text.to_lowercase();
    if lower == **text { text.clone() } else { lower.into() }
}

impl Note {
    /// Range le nouveau texte de la note, et rend l'ancien.
    pub fn set_body(&mut self, text: Arc<str>) -> Arc<str> {
        self.lower = lowered(&text);
        std::mem::replace(&mut self.body, text)
    }

    /// Tags et liens du texte d'une note, prêts à y être rangés.
    pub fn index(text: &str) -> (Arc<[String]>, Arc<[String]>) {
        let (tags, links) = markdown::index(text);
        (tags.into(), links.into())
    }

    /// Un lien écrit `key` (en minuscules) vise cette note : par son nom, ou par un alias.
    pub fn answers(&self, key: &str) -> bool {
        self.name.to_lowercase() == key || self.aliases.iter().any(|alias| alias.to_lowercase() == key)
    }
}
