//! What a `[[link]]` means in a vault: which note it names, which notes point back at one,
//! and when a rename must leave the links alone. Pure: the shell does the writing.

use std::path::Path;

use super::markdown::{self, Link};
use super::note::Note;

/// The note a `[[link]]` points to: by its name first, otherwise by an alias.
/// A template answers no link (`template` says which notes are templates).
pub fn resolve<'a>(
    notes: &'a [Note],
    name: &str,
    template: impl Fn(&Note) -> bool,
) -> Option<&'a Note> {
    let wanted = name.to_lowercase();
    let live = |n: &&Note| !template(n);
    notes
        .iter()
        .filter(live)
        .find(|n| n.name.to_lowercase() == wanted)
        .or_else(|| notes.iter().filter(live).find(|n| n.answers(&wanted)))
}

/// Backlinks: the notes whose `[[wikilink]]` targets note `target`, by name order. When two
/// notes have the same name, the link targets the first, and the other has no backlinks.
pub fn backlinks<'a>(notes: &'a [Note], target: &Path) -> Vec<&'a Note> {
    let Some(aimed) = notes.iter().find(|note| note.path == target) else {
        return Vec::new();
    };
    let key = aimed.name.to_lowercase();
    if notes
        .iter()
        .find(|note| note.name.to_lowercase() == key)
        .is_some_and(|first| first.path != target)
    {
        return Vec::new();
    }
    let reaches =
        |note: &&Note| note.path != target && note.links.iter().any(|link| aimed.answers(link));
    let mut found: Vec<&Note> = notes.iter().filter(reaches).collect();
    found.sort_by_cached_key(|note| note.name.to_lowercase());
    found
}

/// First line of `body` with a wikilink to note `aimed` (its name or an alias).
pub fn link_line<'a>(body: &'a str, aimed: &Note) -> Option<&'a str> {
    body.lines().find(|line| {
        let links = markdown::links(line);
        links.iter().any(
            |(_, link)| matches!(link, Link::Wiki(name) if aimed.answers(&name.to_lowercase())),
        )
    })
}

/// The notes whose links must be rewritten when `old` is renamed `new`.
/// `None` when the links stay as they are: the two names match ignoring case, or another
/// note is still called `old` (the links may be meant for it). The renamed note itself must
/// already bear `new` in `notes`.
pub fn relink_set<'a>(notes: &'a [Note], old: &str, new: &str) -> Option<Vec<&'a Note>> {
    let key = old.to_lowercase();
    if key == new.to_lowercase() || notes.iter().any(|n| n.name.to_lowercase() == key) {
        return None;
    }
    Some(notes.iter().filter(|n| n.links.contains(&key)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{path::PathBuf, sync::Arc, time::SystemTime};

    fn note(name: &str, links: &[&str]) -> Note {
        Note {
            name: name.into(),
            path: PathBuf::from(format!("/v/{name}.md")),
            tags: Arc::from([]),
            links: links.iter().map(|l| (*l).to_string()).collect(),
            aliases: Arc::from([]),
            mtime: SystemTime::UNIX_EPOCH,
            body: "".into(),
            lower: "".into(),
        }
    }

    #[test]
    fn lists_backlinks() {
        let mut notes = vec![
            note("Cible", &[]),
            note("b", &["cible"]),
            note("A", &["cible", "b"]),
            note("Seule", &["cible"]),
        ];
        notes[3].path = PathBuf::from("/v/sous/Cible.md");
        notes[3].name = "Cible".into();
        let names = |target: &str| {
            backlinks(&notes, Path::new(target))
                .iter()
                .map(|n| n.name.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(names("/v/Cible.md"), ["A", "b", "Cible"]);
        assert_eq!(names("/v/b.md"), ["A"]);
        // A second note with the same name: links target the first.
        assert!(names("/v/sous/Cible.md").is_empty() && names("/v/absente.md").is_empty());
        let body = "# A\n[[autre]] et [[Cible]]x\n| t | [[cible|alias]] |\n";
        assert_eq!(link_line(body, &notes[0]), Some("[[autre]] et [[Cible]]x"));
        assert_eq!(
            link_line("un [[Cible|alias]] là", &notes[0]),
            Some("un [[Cible|alias]] là")
        );
        assert_eq!(link_line(body, &notes[1]), None);
        // A front matter alias counts as the name.
        let mut notes = vec![note("Appli", &[]), note("Journal", &["bref app"])];
        notes[0].aliases = vec!["Bref App".into()].into();
        assert_eq!(backlinks(&notes, Path::new("/v/Appli.md")).len(), 1);
        assert_eq!(
            link_line("vu [[bref app]]", &notes[0]),
            Some("vu [[bref app]]")
        );
    }

    #[test]
    fn resolves_name_then_alias_and_skips_templates() {
        let mut notes = vec![
            note("Réunion", &[]),
            note("Journal", &[]),
            note("Modèle", &[]),
        ];
        notes[2].path = PathBuf::from("/v/Templates/Réunion.md");
        notes[2].name = "Réunion".into();
        notes[2].aliases = vec!["modele".into()].into();
        let template = |n: &Note| n.path.starts_with("/v/Templates");
        // The note wins over a template of the same name. An alias of the template answers nothing.
        assert_eq!(
            resolve(&notes, "réunion", template).map(|n| n.path.clone()),
            Some(PathBuf::from("/v/Réunion.md"))
        );
        assert!(resolve(&notes, "modele", template).is_none());
        notes[1].aliases = vec!["carnet".into()].into();
        assert_eq!(resolve(&notes, "carnet", template).unwrap().name, "Journal");
        // The name wins over an alias of another note.
        notes[0].aliases = vec!["autre".into()].into();
        assert_eq!(
            resolve(&notes, "Réunion", template).unwrap().name,
            "Réunion"
        );
        assert_eq!(resolve(&notes, "autre", |_| false).unwrap().name, "Réunion");
        assert!(resolve(&notes, "absente", template).is_none());
        // Alone, a template of that name answers no link.
        let only = vec![notes.pop().unwrap()];
        assert!(resolve(&only, "Réunion", template).is_none());
    }

    #[test]
    fn leaves_links_when_the_old_name_remains() {
        let notes = vec![
            note("Nouveau", &[]),
            note("Ancien", &[]),
            note("Autre", &["ancien"]),
        ];
        assert!(relink_set(&notes, "Ancien", "Nouveau").is_none());
        assert!(relink_set(&notes, "Nouveau", "nouveau").is_none());
        let notes = vec![note("Nouveau", &[]), note("Autre", &["ancien"])];
        let set = relink_set(&notes, "Ancien", "Nouveau").unwrap();
        assert_eq!(set.len(), 1);
        assert_eq!(set[0].name, "Autre");
    }
}
