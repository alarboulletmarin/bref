//! The rows of the panel's lists: the folder tree and the tags. Pure functions over the notes and
//! paths already scanned; the panel state is `views/nav.rs`, its drawing `shell/sidebar.rs`.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path, PathBuf},
};

use crate::logic::{files, graph::image_name, note::Note};

#[derive(Clone, PartialEq, Debug)]
pub struct Row {
    pub path: PathBuf,
    pub name: String,
    pub depth: usize,
    /// Folder, expanded or not; `None` for a note.
    pub dir: Option<bool>,
}

/// Visible rows of the tree: folders first, then notes, in alphabetical
/// order; the content of collapsed folders is left out. `dirs` brings the
/// folders without notes.
pub fn tree_rows(
    root: &Path,
    notes: &[Note],
    dirs: &[PathBuf],
    images: &[PathBuf],
    open: &HashSet<PathBuf>,
) -> Vec<Row> {
    let mut all: HashSet<&Path> = dirs.iter().map(PathBuf::as_path).collect();
    // Per folder: its notes then its pictures, each with the shown name.
    let mut files: HashMap<&Path, Vec<(&Path, String)>> = HashMap::new();
    let named = notes.iter().map(|n| (n.path.as_path(), n.name.clone()));
    let pictures = images.iter().map(|p| (p.as_path(), image_name(p)));
    for (path, name) in named.chain(pictures) {
        let parents = path.ancestors().skip(1).take_while(|a| *a != root && a.starts_with(root));
        all.extend(parents);
        files.entry(path.parent().unwrap_or(root)).or_default().push((path, name));
    }
    let mut folders: HashMap<&Path, Vec<&Path>> = HashMap::new();
    for dir in all {
        folders.entry(dir.parent().unwrap_or(root)).or_default().push(dir);
    }
    let label = |dir: &Path| dir.file_name().unwrap_or_default().to_string_lossy().into_owned();
    folders.values_mut().for_each(|f| f.sort_by_cached_key(|d| files::natural(&label(d))));
    files
        .values_mut()
        .for_each(|f| f.sort_by_cached_key(|(path, name)| (files::is_file_shown(path), files::natural(name))));

    let mut rows = Vec::new();
    // Depth-first walk: (folder, depth of its content), subfolders first.
    fn walk(
        dir: &Path,
        depth: usize,
        folders: &HashMap<&Path, Vec<&Path>>,
        files: &HashMap<&Path, Vec<(&Path, String)>>,
        open: &HashSet<PathBuf>,
        rows: &mut Vec<Row>,
    ) {
        for sub in folders.get(dir).into_iter().flatten() {
            let unfolded = open.contains(*sub);
            rows.push(Row {
                path: sub.to_path_buf(),
                name: sub.file_name().unwrap_or_default().to_string_lossy().into_owned(),
                depth,
                dir: Some(unfolded),
            });
            if unfolded {
                walk(sub, depth + 1, folders, files, open, rows);
            }
        }
        for (path, name) in files.get(dir).into_iter().flatten() {
            rows.push(Row {
                path: path.to_path_buf(),
                name: name.clone(),
                depth,
                dir: None,
            });
        }
    }
    walk(root, 0, &folders, &files, open, &mut rows);
    rows
}

/// Row of a tag in the panel: a fake path, that no file bears.
fn tag_path(tag: &str) -> PathBuf {
    PathBuf::from(format!("#{tag}"))
}

/// The tag this path points to (`#tag`), if it is one.
pub fn tag_of(path: &Path) -> Option<&str> {
    path.to_str()?.strip_prefix('#')
}

/// The vault's tags in alphabetical order; under an expanded tag, its notes.
// ponytail: a note with several expanded tags appears under each, and the
// selection, held by its path, marks them all; give each row its own identity
// if that gets in the way.
pub fn tag_rows(notes: &[Note], open: &HashSet<PathBuf>) -> Vec<Row> {
    let mut tagged: BTreeMap<&str, Vec<&Note>> = BTreeMap::new();
    for note in notes {
        for tag in note.tags.iter() {
            tagged.entry(tag).or_default().push(note);
        }
    }
    let mut rows = Vec::new();
    for (tag, mut notes) in tagged {
        let path = tag_path(tag);
        let unfolded = open.contains(&path);
        rows.push(Row { path, name: format!("#{tag}"), depth: 0, dir: Some(unfolded) });
        if unfolded {
            notes.sort_by_cached_key(|n| files::natural(&n.name));
            rows.extend(notes.iter().map(|n| Row { path: n.path.clone(), name: n.name.clone(), depth: 1, dir: None }));
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    #[test]
    fn builds_tree_rows() {
        let root = Path::new("/v");
        let note = |rel: &str| Note {
            name: Path::new(rel).file_stem().unwrap().to_string_lossy().into_owned(),
            path: root.join(rel),
            tags: Default::default(),
            links: Default::default(),
            aliases: Default::default(),
            mtime: SystemTime::UNIX_EPOCH,
            body: "".into(),
            lower: "".into(),
        };
        let notes = [note("b.md"), note("Z/x.md"), note("a/c/d.md"), note("a/B.md"), note("A.md")];
        let shown = |open: &[&str]| -> Vec<String> {
            let open = open.iter().map(|d| root.join(d)).collect();
            tree_rows(root, &notes, &[root.join("a/vide"), root.join("a")], &[root.join("a/Photo.png")], &open)
                .iter()
                .map(|r| format!("{}{}{}", "  ".repeat(r.depth), r.name, if r.dir.is_some() { "/" } else { "" }))
                .collect()
        };
        // Folders first, case-insensitive; everything is collapsed.
        assert_eq!(shown(&[]), ["a/", "Z/", "A", "b"]);
        // A folder without notes appears too.
        // A folder's pictures follow its notes.
        assert_eq!(shown(&["a"]), ["a/", "  c/", "  vide/", "  B", "  Photo.png", "Z/", "A", "b"]);
        // A folder expanded inside a collapsed folder stays hidden.
        assert_eq!(shown(&["a/c", "Z"]), ["a/", "Z/", "  x", "A", "b"]);
        assert_eq!(shown(&["a", "a/c"]), ["a/", "  c/", "    d", "  vide/", "  B", "  Photo.png", "Z/", "A", "b"]);

        // Tags: in alphabetical order; an expanded tag shows its notes.
        let tagged = |rel: &str, tags: &[&str]| Note { tags: tags.iter().map(|t| t.to_string()).collect(), ..note(rel) };
        let notes = [tagged("b.md", &["zoo", "ami"]), tagged("A.md", &["zoo"]), note("sans.md")];
        let shown = |open: &[&str]| -> Vec<String> {
            let open = open.iter().map(|t| tag_path(t)).collect();
            tag_rows(&notes, &open).iter().map(|r| format!("{}{}", "  ".repeat(r.depth), r.name)).collect()
        };
        assert_eq!(shown(&[]), ["#ami", "#zoo"]);
        assert_eq!(shown(&["zoo"]), ["#ami", "#zoo", "  A", "  b"]);
    }
}
