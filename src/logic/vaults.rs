//! The vaults Bref has opened, to switch between them in a keystroke: their list (file `vaults` of the
//! config folder), and what each one had on screen when it was left (file `session-{id}`). Pure: the
//! shell reads and writes the files.

use std::path::{Path, PathBuf};

/// A vault of the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Known {
    /// Names its session file; never reused, so a vault found again under another path keeps its own.
    pub id: u32,
    pub path: PathBuf,
    /// The short name the user gave it; the folder's name otherwise.
    pub name: Option<String>,
    /// Its colour, an index in `COLORS`: the band at the top of the rail.
    pub color: Option<usize>,
    /// Last opened, in seconds since 1970; 0 when unknown.
    pub used: u64,
    /// Its notes, as counted when it was last open.
    pub notes: Option<usize>,
}

/// How many colours a vault can wear: the five hues of the highlights (yellow, green, blue, pink, purple).
pub const COLORS: usize = 5;

impl Known {
    /// What the switcher calls it: its short name, or its folder's.
    pub fn label(&self) -> String {
        match &self.name {
            Some(name) => name.clone(),
            None => self.path.file_name().map_or_else(|| self.path.display().to_string(), |n| n.to_string_lossy().into_owned()),
        }
    }
}

/// The list as written: one vault a line, in the user's order, its fields separated by tabulations:
/// `id`, path, name (may be empty), colour (may be empty), last use, notes (may be empty). A line that
/// cannot be read is dropped, and so is a second line for the same path or the same id.
pub fn read(text: &str) -> Vec<Known> {
    let mut list: Vec<Known> = Vec::new();
    for line in text.lines() {
        let mut fields = line.split('\t');
        let (Some(id), Some(path)) = (fields.next().and_then(|id| id.parse().ok()), fields.next().filter(|p| !p.is_empty())) else {
            continue;
        };
        let name = fields.next().filter(|n| !n.trim().is_empty()).map(|n| n.trim().to_string());
        let color = fields.next().and_then(|c| c.parse().ok()).filter(|c| *c < COLORS);
        let used = fields.next().and_then(|u| u.parse().ok()).unwrap_or(0);
        let notes = fields.next().and_then(|n| n.parse().ok());
        let path = PathBuf::from(path);
        if list.iter().all(|known| known.id != id && known.path != path) {
            list.push(Known { id, path, name, color, used, notes });
        }
    }
    list
}

pub fn write(list: &[Known]) -> String {
    let opt = |value: Option<String>| value.unwrap_or_default();
    list.iter()
        .map(|k| {
            // A tabulation or a line break in a name would break the line: they become spaces.
            let name = k.name.as_deref().map(|n| n.replace(['\t', '\n', '\r'], " "));
            format!("{}\t{}\t{}\t{}\t{}\t{}\n", k.id, k.path.display(), opt(name), opt(k.color.map(|c| c.to_string())), k.used, opt(k.notes.map(|n| n.to_string())))
        })
        .collect()
}

/// The vault at `path` was just opened, at `now`: its last use is updated, or it joins the end of the list
/// with a new id. Returns its place in the list.
pub fn remember(list: &mut Vec<Known>, path: &Path, now: u64) -> usize {
    if let Some(at) = list.iter().position(|k| k.path == path) {
        list[at].used = now;
        return at;
    }
    let id = list.iter().map(|k| k.id + 1).max().unwrap_or(1);
    list.push(Known { id, path: path.to_path_buf(), name: None, color: None, used: now, notes: None });
    list.len() - 1
}

/// Moves the vault at `at` one place up (`up`) or down; false when it is already at that end.
pub fn shift(list: &mut [Known], at: usize, up: bool) -> bool {
    let to = match up {
        true => at.checked_sub(1),
        false => Some(at + 1).filter(|to| *to < list.len()),
    };
    match to.filter(|_| at < list.len()) {
        Some(to) => {
            list.swap(at, to);
            true
        }
        None => false,
    }
}

/// What a vault had on screen when it was left: its panel (`Nav`'s line), its recent notes, most recent
/// first, and its panes and tabs (the shell's `panes` format, kept as is).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Session {
    pub panel: String,
    pub recent: Vec<PathBuf>,
    pub panes: String,
}

/// A session file: `panel <line>`, then one `recent <path>` line per note, then `panes` alone on its line
/// followed by the panes' own lines, up to the end.
pub fn write_session(session: &Session) -> String {
    let mut text = format!("panel {}\n", session.panel.trim());
    for path in &session.recent {
        text += &format!("recent {}\n", path.display());
    }
    text += "panes\n";
    text += &session.panes;
    text
}

pub fn read_session(text: &str) -> Session {
    let mut session = Session::default();
    let mut lines = text.split_inclusive('\n');
    for line in lines.by_ref() {
        let line = line.trim_end_matches(['\n', '\r']);
        if line == "panes" {
            break;
        }
        if let Some(panel) = line.strip_prefix("panel ") {
            session.panel = panel.to_string();
        } else if let Some(path) = line.strip_prefix("recent ").filter(|p| !p.is_empty()) {
            session.recent.push(PathBuf::from(path));
        }
    }
    session.panes = lines.collect();
    session
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known(id: u32, path: &str) -> Known {
        Known { id, path: path.into(), name: None, color: None, used: 0, notes: None }
    }

    #[test]
    fn the_list_reads_back_what_it_wrote() {
        let list = vec![
            Known { id: 3, path: "/home/a/Travail".into(), name: Some("Boulot".into()), color: Some(2), used: 1_790_000_000, notes: Some(1250) },
            known(1, "/media/clé/Perso"),
        ];
        assert_eq!(read(&write(&list)), list);
        // A tabulation in a name becomes a space; the line stays whole.
        let odd = vec![Known { name: Some("a\tb".into()), ..known(1, "/v") }];
        assert_eq!(read(&write(&odd))[0].name.as_deref(), Some("a b"));
    }

    #[test]
    fn a_damaged_line_is_dropped_and_the_rest_kept() {
        let text = "x\t/bad\n2\t/v/A\t\t\t10\t\n2\t/v/Again\n3\t/v/A\n4\t/v/B\tB\t9\tnope\t7\n\n5\t\n";
        let list = read(text);
        // Same id or same path a second time: dropped. A colour out of range is no colour.
        assert_eq!(list.iter().map(|k| k.id).collect::<Vec<_>>(), [2, 4]);
        assert_eq!((list[0].used, list[0].notes, list[0].name.clone()), (10, None, None));
        assert_eq!((list[1].color, list[1].used, list[1].notes), (None, 0, Some(7)));
    }

    #[test]
    fn remembers_without_duplicates_and_with_fresh_ids() {
        let mut list = vec![known(4, "/v/A"), known(2, "/v/B")];
        assert_eq!(remember(&mut list, Path::new("/v/B"), 50), 1);
        assert_eq!((list.len(), list[1].used), (2, 50));
        assert_eq!(remember(&mut list, Path::new("/v/C"), 60), 2);
        assert_eq!((list[2].id, list[2].used), (5, 60));
        assert_eq!(remember(&mut Vec::new(), Path::new("/v/D"), 1), 0);
    }

    #[test]
    fn shifts_within_the_list() {
        let mut list = vec![known(1, "/a"), known(2, "/b"), known(3, "/c")];
        assert!(shift(&mut list, 2, true));
        assert_eq!(list.iter().map(|k| k.id).collect::<Vec<_>>(), [1, 3, 2]);
        assert!(!shift(&mut list, 0, true) && !shift(&mut list, 2, false) && !shift(&mut list, 9, true));
        assert!(shift(&mut list, 0, false));
        assert_eq!(list.iter().map(|k| k.id).collect::<Vec<_>>(), [3, 1, 2]);
    }

    #[test]
    fn labels_by_name_then_folder() {
        assert_eq!(known(1, "/home/a/Notes perso").label(), "Notes perso");
        assert_eq!(Known { name: Some("Perso".into()), ..known(1, "/x") }.label(), "Perso");
    }

    #[test]
    fn a_session_reads_back_what_it_wrote() {
        let session = Session {
            panel: "tree split 260 0 right=graph width=300".into(),
            recent: vec!["/v/A.md".into(), "/v/sous dossier/B.md".into()],
            panes: "v3 row 0.5\n*/v/A.md\t/v/B.md\n@graph\n".into(),
        };
        assert_eq!(read_session(&write_session(&session)), session);
        assert_eq!(read_session(""), Session::default());
        // An old or hand-edited file without `panes`: what it has is read.
        assert_eq!(read_session("recent /v/A.md\r\n").recent, [PathBuf::from("/v/A.md")]);
    }
}
