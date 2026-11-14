//! Vault health: the small problems a long-lived vault gathers without anyone seeing them
//! (links to notes never written, pictures nothing shows, empty notes, sync conflict copies,
//! two notes a link cannot tell apart…), found from the index already in memory.
//! Pure: the shell lists the findings and applies the fixes, through `.trash` and rename.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use super::markdown::{self, Link};
use super::note::Note;
use crate::lang::fold;

/// A picture heavier than this is worth a look: 5 MB.
pub const HEAVY: u64 = 5 * 1024 * 1024;

/// What a file of the vault other than a note is, for the checks that care.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FileKind {
    /// A picture: unused when no note shows or links it, heavy over `HEAVY`.
    Picture,
    /// A PDF, a video, an archive…: unused when no note links it.
    Attachment,
    /// A CSV or TSV table, or a Bref diagram (an SVG with its source): a document of its own,
    /// opened from the tree, never reported as unused.
    Document,
}

/// A file of the vault other than a note, with its size on disk.
#[derive(Clone, Debug)]
pub struct File {
    pub path: PathBuf,
    pub bytes: u64,
    pub kind: FileKind,
}

/// What the scan reads: the index of the vault as the shell keeps it.
pub struct Vault<'a> {
    pub notes: &'a [Note],
    pub files: &'a [File],
    pub dirs: &'a [PathBuf],
    /// Which notes are templates: they answer no link and are never reported.
    pub template: &'a dyn Fn(&Note) -> bool,
}

/// The kinds of finding, in the order the page lists them.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Kind {
    BrokenLink,
    TemplateLink,
    SameName,
    Conflict,
    BadFrontMatter,
    Empty,
    Unused,
    Heavy,
    Orphan,
}

/// One problem of the vault. A `row` is a line of the note, counted from 0.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Finding {
    /// `[[target]]` names no note, alias, file or folder. `closest` holds the existing names
    /// nearest to it (all at the same, smallest distance), as written.
    BrokenLink { note: PathBuf, row: usize, target: String, closest: Vec<String> },
    /// `[[target]]` names only a template, which answers no link.
    TemplateLink { note: PathBuf, row: usize, target: String, template: PathBuf },
    /// Several notes answer the same name or alias (`name`, lowercase): a link to it is ambiguous.
    SameName { name: String, notes: Vec<PathBuf> },
    /// A sync conflict copy, and the note or file it is a copy of, if still there.
    Conflict { copy: PathBuf, original: Option<PathBuf> },
    /// The front matter does not close, or a line of it is not `key: value`.
    BadFrontMatter { note: PathBuf, row: usize },
    /// Nothing written besides the front matter and a title.
    Empty { note: PathBuf },
    /// A picture or attachment that no note shows or links.
    Unused { path: PathBuf },
    /// A picture over `HEAVY` bytes.
    Heavy { path: PathBuf, bytes: u64 },
    /// A note with no link in or out (daily notes, named after their date, are left out).
    Orphan { note: PathBuf },
}

impl Finding {
    pub fn kind(&self) -> Kind {
        match self {
            Finding::BrokenLink { .. } => Kind::BrokenLink,
            Finding::TemplateLink { .. } => Kind::TemplateLink,
            Finding::SameName { .. } => Kind::SameName,
            Finding::Conflict { .. } => Kind::Conflict,
            Finding::BadFrontMatter { .. } => Kind::BadFrontMatter,
            Finding::Empty { .. } => Kind::Empty,
            Finding::Unused { .. } => Kind::Unused,
            Finding::Heavy { .. } => Kind::Heavy,
            Finding::Orphan { .. } => Kind::Orphan,
        }
    }

    /// The fix that needs no choice: a broken link with exactly one close name is relinked to it.
    /// It is what "Fix all" applies; every other fix waits for the user.
    pub fn safe_relink(&self) -> Option<(&Path, &str, &str)> {
        match self {
            Finding::BrokenLink { note, target, closest, .. } if closest.len() == 1 => Some((note.as_path(), target.as_str(), closest[0].as_str())),
            _ => None,
        }
    }
}

/// Every finding of the vault, by kind, then by path and line.
pub fn scan(vault: &Vault) -> Vec<Finding> {
    let live: Vec<&Note> = vault.notes.iter().filter(|n| !(vault.template)(n)).collect();
    let mut found = Vec::new();
    found.extend(links(vault, &live));
    found.extend(same_names(&live));
    found.extend(conflicts(vault));
    for note in &live {
        if let Some(row) = bad_front_matter(&note.body) {
            found.push(Finding::BadFrontMatter { note: note.path.clone(), row });
        }
        if is_empty(&note.body) {
            found.push(Finding::Empty { note: note.path.clone() });
        }
    }
    found.extend(files(vault, &live));
    found.extend(orphans(vault, &live));
    found.sort_by(|a, b| a.kind().cmp(&b.kind()).then_with(|| place(a).cmp(&place(b))));
    found
}

/// How many links name nothing, from the index alone (no text read): cheap enough to count each
/// time the palette opens, for the dot on the health page's command. A link written twice in a
/// note counts once, as the index keeps it.
pub fn broken_count(vault: &Vault) -> usize {
    let known = known_names(vault);
    let templates: HashSet<String> = vault.notes.iter().filter(|n| (vault.template)(n)).map(|n| n.name.to_lowercase()).collect();
    let live = vault.notes.iter().filter(|n| !(vault.template)(n));
    live.flat_map(|n| n.links.iter()).filter(|link| !known.contains(link.as_str()) && !templates.contains(link.as_str())).count()
}

/// Every name a link can answer to, lowercase: notes, aliases, files and folders.
fn known_names(vault: &Vault) -> HashSet<String> {
    let mut known: HashSet<String> = HashSet::new();
    for note in vault.notes.iter().filter(|n| !(vault.template)(n)) {
        known.insert(note.name.to_lowercase());
        known.extend(note.aliases.iter().map(|alias| alias.to_lowercase()));
    }
    known.extend(vault.files.iter().map(|file| file_key(&file.path)));
    known.extend(vault.dirs.iter().map(|dir| file_key(dir)));
    known
}

/// How many findings of each kind, in the page's order; kinds with none are left out.
pub fn counts(found: &[Finding]) -> Vec<(Kind, usize)> {
    let mut counts: Vec<(Kind, usize)> = Vec::new();
    for finding in found {
        match counts.last_mut() {
            Some((kind, n)) if *kind == finding.kind() => *n += 1,
            _ => counts.push((finding.kind(), 1)),
        }
    }
    counts
}

/// The safe relinks of `found`, grouped by note: each note is then rewritten once, as one
/// undoable step. Pairs are `(old, new)`, in the order found.
pub fn safe_relinks(found: &[Finding]) -> Vec<(PathBuf, Vec<(String, String)>)> {
    let mut by_note: Vec<(PathBuf, Vec<(String, String)>)> = Vec::new();
    for (note, old, new) in found.iter().filter_map(Finding::safe_relink) {
        let pair = (old.to_string(), new.to_string());
        match by_note.iter_mut().find(|(path, _)| path == note) {
            Some((_, pairs)) if pairs.contains(&pair) => {}
            Some((_, pairs)) => pairs.push(pair),
            None => by_note.push((note.to_path_buf(), vec![pair])),
        }
    }
    by_note
}

/// The note's text once every safe relink of it is applied; `None` if nothing changes.
pub fn apply_relinks(text: &str, pairs: &[(String, String)]) -> Option<String> {
    let mut out: Option<String> = None;
    for (old, new) in pairs {
        if let Some(changed) = markdown::relink(out.as_deref().unwrap_or(text), old, new) {
            out = Some(changed);
        }
    }
    out
}

/// The text where the `[[target]]` links of line `row` lose their brackets: `[[Name|shown]]`
/// becomes `shown`, `[[Name#Part]]` becomes `Name#Part`, `![[file]]` is left alone (a picture
/// without its brackets would show its name, not itself). `None` if no link of that line changes.
pub fn unbracket(text: &str, row: usize, target: &str) -> Option<String> {
    let wanted = target.to_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut changed = false;
    for (i, line) in text.split_inclusive('\n').enumerate() {
        if i != row {
            out.push_str(line);
            continue;
        }
        let mut done = 0;
        for (range, link) in markdown::links(line) {
            let embed = range.start > 0 && line.as_bytes()[range.start - 1] == b'!';
            if embed || !matches!(link, Link::Wiki(name) if name.to_lowercase() == wanted) {
                continue;
            }
            let inner = &line[range.start + 2..range.end - 2];
            let shown = inner.split_once('|').map_or(inner, |(_, shown)| shown);
            out.push_str(&line[done..range.start]);
            out.push_str(shown.trim());
            done = range.end;
            changed = true;
        }
        out.push_str(&line[done..]);
    }
    changed.then_some(out)
}

/// The existing names closest to `target`, ignoring case and accents: all those at the smallest
/// edit distance, if it is small enough for a typo or a rename made elsewhere.
// ponytail: edit distance against every name, cut short by length; a vault of tens of thousands
// of notes with many broken links would want an index of names by their letters.
pub fn closest<'a>(target: &str, names: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let wanted = fold(target);
    let size = wanted.chars().count();
    let limit = (size / 4).clamp(1, 3);
    let mut best = limit + 1;
    let mut found: Vec<String> = Vec::new();
    for name in names {
        let folded = fold(name);
        if folded.chars().count().abs_diff(size) > limit {
            continue;
        }
        let distance = edits(&wanted, &folded);
        if distance > limit {
            continue;
        }
        if distance < best {
            best = distance;
            found.clear();
        }
        if distance == best && !found.iter().any(|known| known.to_lowercase() == name.to_lowercase()) {
            found.push(name.to_string());
        }
    }
    found
}

/// Levenshtein distance, by characters.
fn edits(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = (above + 1).min(row[j] + 1).min(diagonal + usize::from(ca != *cb));
            diagonal = above;
        }
    }
    row[b.len()]
}

/// The path and line a finding sorts by within its kind.
fn place(finding: &Finding) -> (&Path, usize) {
    match finding {
        Finding::BrokenLink { note, row, .. } | Finding::TemplateLink { note, row, .. } | Finding::BadFrontMatter { note, row } => (note.as_path(), *row),
        Finding::SameName { notes, .. } => (notes.first().map_or(Path::new(""), PathBuf::as_path), 0),
        Finding::Conflict { copy: path, .. }
        | Finding::Empty { note: path }
        | Finding::Unused { path }
        | Finding::Heavy { path, .. }
        | Finding::Orphan { note: path } => (path.as_path(), 0),
    }
}

/// The lowercase file name of a path, the way a link names a file.
fn file_key(path: &Path) -> String {
    path.file_name().unwrap_or_default().to_string_lossy().to_lowercase()
}

/// Broken links and links to templates, line by line, outside code blocks and front matter.
fn links(vault: &Vault, live: &[&Note]) -> Vec<Finding> {
    let known = known_names(vault);
    let templates: HashMap<String, &Note> =
        vault.notes.iter().filter(|n| (vault.template)(n)).map(|n| (n.name.to_lowercase(), n)).collect();
    let names: Vec<&str> = live.iter().map(|n| n.name.as_str()).chain(live.iter().flat_map(|n| n.aliases.iter().map(String::as_str))).collect();
    let mut found = Vec::new();
    for note in live {
        // The index already lists its links: most notes are passed over without reading their text.
        if note.links.iter().all(|link| known.contains(link.as_str())) {
            continue;
        }
        let start = markdown::front_matter(&note.body);
        let skipped = note.body[..start].lines().count();
        let mut in_code = false;
        for (row, line) in note.body[start..].lines().enumerate() {
            if markdown::is_fence(line) {
                in_code = !in_code;
                continue;
            }
            if in_code {
                continue;
            }
            let mut seen: Vec<String> = Vec::new();
            for (_, link) in markdown::links(line) {
                let Link::Wiki(target) = link else { continue };
                let key = target.to_lowercase();
                if known.contains(&key) || seen.contains(&key) {
                    continue;
                }
                seen.push(key.clone());
                let (at, row) = (note.path.clone(), skipped + row);
                found.push(match templates.get(&key) {
                    Some(template) => Finding::TemplateLink { note: at, row, target, template: template.path.clone() },
                    None => {
                        let closest = closest(&target, names.iter().copied());
                        Finding::BrokenLink { note: at, row, target, closest }
                    }
                });
            }
        }
    }
    found
}

/// Names and aliases answered by more than one note.
fn same_names(live: &[&Note]) -> Vec<Finding> {
    let mut by_name: HashMap<String, Vec<PathBuf>> = HashMap::new();
    for note in live {
        let keys = std::iter::once(note.name.to_lowercase()).chain(note.aliases.iter().map(|alias| alias.to_lowercase()));
        for key in keys {
            let paths = by_name.entry(key).or_default();
            if !paths.contains(&note.path) {
                paths.push(note.path.clone());
            }
        }
    }
    let mut found: Vec<Finding> = by_name
        .into_iter()
        .filter(|(_, notes)| notes.len() > 1)
        .map(|(name, mut notes)| {
            notes.sort();
            Finding::SameName { name, notes }
        })
        .collect();
    found.sort_by(|a, b| place(a).cmp(&place(b)));
    found
}

/// What a sync conflict copy is named after: the stem before ` (conflict DATE[ HOST])`, which
/// `platform::git::conflict_path` writes, then numbers (` 2`) when that name is taken.
pub fn conflict_of(stem: &str) -> Option<&str> {
    let open = stem.rfind(" (conflict ")?;
    let rest = &stem[open + " (conflict ".len()..];
    let close = rest.find(')')?;
    let date = rest[..close].split(' ').next()?;
    let is_date = date.len() == 10
        && date.char_indices().all(|(i, c)| if i == 4 || i == 7 { c == '-' } else { c.is_ascii_digit() });
    let tail = &rest[close + 1..];
    let numbered = tail.is_empty() || tail.strip_prefix(' ').is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
    (open > 0 && is_date && numbered).then(|| &stem[..open])
}

/// Sync conflict copies among the notes and files, with what they copy.
fn conflicts(vault: &Vault) -> Vec<Finding> {
    let paths: Vec<&Path> = vault
        .notes
        .iter()
        .filter(|n| !(vault.template)(n))
        .map(|n| n.path.as_path())
        .chain(vault.files.iter().map(|f| f.path.as_path()))
        .collect();
    let present: HashSet<&Path> = paths.iter().copied().collect();
    paths
        .iter()
        .filter_map(|path| {
            let stem = path.file_stem()?.to_string_lossy();
            let base = conflict_of(&stem)?;
            let name = match path.extension() {
                Some(extension) => format!("{base}.{}", extension.to_string_lossy()),
                None => base.to_string(),
            };
            let original = path.with_file_name(name);
            let original = present.contains(original.as_path()).then_some(original);
            Some(Finding::Conflict { copy: path.to_path_buf(), original })
        })
        .collect()
}

/// The first line that keeps the front matter from being read: its opening line when it never
/// closes, otherwise a line that is neither `key: value`, a list item, a continuation, a comment
/// nor blank.
// ponytail: the same hand reading as `front_values`; a YAML library would catch more.
pub fn bad_front_matter(text: &str) -> Option<usize> {
    if !text.split_inclusive('\n').next().is_some_and(|first| first.trim_end() == "---") {
        return None;
    }
    let end = markdown::front_matter(text);
    if end == 0 {
        return Some(0);
    }
    let lines: Vec<&str> = text[..end].lines().collect();
    let inner = &lines[1..lines.len().saturating_sub(1)];
    inner
        .iter()
        .position(|line| {
            let trimmed = line.trim();
            let read = trimmed.is_empty()
                || trimmed.starts_with('#')
                || line.starts_with([' ', '\t'])
                || trimmed.starts_with("- ")
                || trimmed == "-"
                || line.split_once(':').is_some_and(|(key, _)| !key.trim().is_empty() && !key.contains(['[', '{', '"']));
            !read
        })
        .map(|i| i + 1)
}

/// Nothing written besides the front matter and a title line.
// ponytail: only truly empty bodies; "nearly empty" (a line or two) would need a threshold
// users agree on.
pub fn is_empty(text: &str) -> bool {
    let body = &text[markdown::front_matter(text)..];
    let mut lines = body.lines().map(str::trim).filter(|line| !line.is_empty());
    let Some(first) = lines.next() else {
        return true;
    };
    let hashes = first.bytes().take_while(|b| *b == b'#').count();
    let heading = (1..=6).contains(&hashes) && (first.len() == hashes || first[hashes..].starts_with(' '));
    heading && lines.next().is_none()
}

/// The files a note points to with a plain Markdown link, `[text](folder/file.pdf)`, by their lowercase
/// file name: the index keeps only wikilinks and displayed pictures. Web addresses, anchors and code
/// blocks are left out.
pub fn local_links(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut in_code = false;
    for line in text[markdown::front_matter(text)..].lines() {
        if markdown::is_fence(line) {
            in_code = !in_code;
            continue;
        }
        if in_code {
            continue;
        }
        let mut rest = line;
        while let Some(at) = rest.find("](") {
            let after = &rest[at + 2..];
            let Some(end) = after.find(')') else { break };
            // `<path with spaces>` and `path "title"` keep only the path.
            let inner = after[..end].trim();
            let target = match inner.strip_prefix('<') {
                Some(quoted) => quoted.split('>').next().unwrap_or(""),
                None => inner.split(' ').next().unwrap_or(""),
            };
            let target = target.split(['#', '?']).next().unwrap_or("").replace("%20", " ");
            let web = target.contains("://") || target.starts_with("mailto:");
            if !target.is_empty() && !web {
                let name = target.rsplit(['/', '\\']).next().unwrap_or(&target).to_lowercase();
                if !name.is_empty() && !found.contains(&name) {
                    found.push(name);
                }
            }
            rest = &after[end..];
        }
    }
    found
}

/// Pictures and attachments nothing links, and pictures over `HEAVY`.
fn files(vault: &Vault, live: &[&Note]) -> Vec<Finding> {
    let mut linked: HashSet<String> = live.iter().flat_map(|n| n.links.iter().map(|link| link.as_str().to_string())).collect();
    // A plain Markdown link to a file uses it too: only notes that point at a file are read.
    linked.extend(live.iter().filter(|n| n.body.contains("](")).flat_map(|n| local_links(&n.body)));
    let mut found = Vec::new();
    for file in vault.files {
        if file.kind != FileKind::Document && !linked.contains(&file_key(&file.path)) {
            found.push(Finding::Unused { path: file.path.clone() });
        }
        if file.kind == FileKind::Picture && file.bytes > HEAVY {
            found.push(Finding::Heavy { path: file.path.clone(), bytes: file.bytes });
        }
    }
    found
}

/// The daily note is named after its day: `2026-10-09`.
fn is_dated(name: &str) -> bool {
    name.len() == 10 && name.char_indices().all(|(i, c)| if i == 4 || i == 7 { c == '-' } else { c.is_ascii_digit() })
}

/// Notes no link reaches and that link to nothing in the vault.
// ponytail: daily notes are left out, since a journal is meant to stand alone; an inbox folder
// could be too, once a vault says which one it is.
fn orphans(vault: &Vault, live: &[&Note]) -> Vec<Finding> {
    let mut by_name: HashMap<String, usize> = HashMap::new();
    for (i, note) in live.iter().enumerate() {
        by_name.entry(note.name.to_lowercase()).or_insert(i);
    }
    for (i, note) in live.iter().enumerate() {
        for alias in note.aliases.iter() {
            by_name.entry(alias.to_lowercase()).or_insert(i);
        }
    }
    let files: HashSet<String> = vault.files.iter().map(|f| file_key(&f.path)).collect();
    let mut linked = vec![false; live.len()];
    for (i, note) in live.iter().enumerate() {
        for link in note.links.iter() {
            match by_name.get(link.as_str()) {
                Some(&j) if j != i => {
                    linked[i] = true;
                    linked[j] = true;
                }
                _ if files.contains(link.as_str()) => linked[i] = true,
                _ => {}
            }
        }
    }
    // A conflict copy or an empty note is listed under its own kind already: not twice.
    let elsewhere = |note: &Note| conflict_of(&note.name).is_some() || is_empty(&note.body);
    live.iter()
        .zip(linked)
        .filter(|(note, linked)| !linked && !is_dated(&note.name) && !elsewhere(note))
        .map(|(note, _)| Finding::Orphan { note: note.path.clone() })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::Arc, time::SystemTime};

    fn note(path: &str, body: &str) -> Note {
        let path = PathBuf::from(path);
        let name = path.file_stem().unwrap().to_string_lossy().into();
        Note::from_text(name, path, SystemTime::UNIX_EPOCH, Arc::from(body), false)
    }

    fn file(path: &str, bytes: u64, kind: FileKind) -> File {
        File { path: PathBuf::from(path), bytes, kind }
    }

    fn templates(n: &Note) -> bool {
        n.path.starts_with("/v/Templates")
    }

    fn scan_of(notes: &[Note], files: &[File], dirs: &[PathBuf]) -> Vec<Finding> {
        scan(&Vault { notes, files, dirs, template: &templates })
    }

    fn of(found: &[Finding], kind: Kind) -> Vec<Finding> {
        found.iter().filter(|f| f.kind() == kind).cloned().collect()
    }

    #[test]
    fn finds_broken_links_with_their_line_and_closest_name() {
        let notes = [
            note("/v/Recettes.md", "---\ntags: [x]\n---\n# Recettes\n\nvoir [[Recetes]] et [[Courses]]\n```\n[[Dans du code]]\n```\n[[Rien du tout|là]]\n"),
            note("/v/Courses.md", "# Courses\n[[Recettes]]\n"),
            note("/v/Thème.md", "# Thème\n[[thème sombre]] [[Projets]] [[photo.png]] [[Recettes#Desserts]]\n"),
            note("/v/Thème sombre.md", "x [[Thème]]"),
        ];
        let files = [file("/v/img/photo.png", 10, FileKind::Picture)];
        let dirs = [PathBuf::from("/v/Projets")];
        let broken = of(&scan_of(&notes, &files, &dirs), Kind::BrokenLink);
        assert_eq!(
            broken,
            [
                Finding::BrokenLink { note: "/v/Recettes.md".into(), row: 5, target: "Recetes".into(), closest: vec!["Recettes".into()] },
                Finding::BrokenLink { note: "/v/Recettes.md".into(), row: 9, target: "Rien du tout".into(), closest: vec![] },
            ]
        );
        // Name, alias, file, folder and anchor all answer; code blocks are not read.
        assert!(broken.iter().all(|f| matches!(f, Finding::BrokenLink { note, .. } if note.ends_with("Recettes.md"))));
    }

    #[test]
    fn closest_names_fold_case_and_accents_and_keep_ties() {
        let names = ["Réunion", "Reunions", "Projet", "Projets", "Journal"];
        assert_eq!(closest("reunion", names), ["Réunion"]);
        assert_eq!(closest("Projt", names), ["Projet"]);
        assert_eq!(closest("Projetz", names), ["Projet", "Projets"]);
        assert!(closest("Zèbre", names).is_empty());
        // A short name allows a single typo, not a different word.
        assert!(closest("ab", ["xy"]).is_empty());
        assert_eq!(edits("chat", "chats"), 1);
        assert_eq!(edits("kitten", "sitting"), 3);
    }

    #[test]
    fn a_link_to_a_template_is_its_own_finding() {
        let notes = [note("/v/Templates/Réunion.md", "# {{title}}"), note("/v/Lundi.md", "# Lundi\n[[Réunion]]\n")];
        let found = scan_of(&notes, &[], &[]);
        assert_eq!(
            of(&found, Kind::TemplateLink),
            [Finding::TemplateLink { note: "/v/Lundi.md".into(), row: 1, target: "Réunion".into(), template: "/v/Templates/Réunion.md".into() }]
        );
        assert!(of(&found, Kind::BrokenLink).is_empty());
        // A template is never reported for itself: not empty, not orphan.
        assert!(found.iter().all(|f| place(f).0 != Path::new("/v/Templates/Réunion.md")));
    }

    #[test]
    fn same_name_or_alias_in_several_notes() {
        let notes = [
            note("/v/a/Idées.md", "# Idées\n[[b]]"),
            note("/v/b/idées.md", "# idées\n"),
            note("/v/Appli.md", "---\naliases: [Bref, App]\n---\n# Appli\n"),
            note("/v/App.md", "# App\n"),
            note("/v/Seule.md", "---\naliases: [Seule]\n---\n"),
        ];
        assert_eq!(
            of(&scan_of(&notes, &[], &[]), Kind::SameName),
            [
                Finding::SameName { name: "app".into(), notes: vec!["/v/App.md".into(), "/v/Appli.md".into()] },
                Finding::SameName { name: "idées".into(), notes: vec!["/v/a/Idées.md".into(), "/v/b/idées.md".into()] },
            ]
        );
    }

    #[test]
    fn knows_the_conflict_copies_git_writes() {
        assert_eq!(conflict_of("Note (conflict 2026-10-09 laptop)"), Some("Note"));
        assert_eq!(conflict_of("Note (conflict 2026-10-09)"), Some("Note"));
        assert_eq!(conflict_of("Note (conflict 2026-10-09 pc) 2"), Some("Note"));
        assert_eq!(conflict_of("Mon (projet) (conflict 2026-10-09 pc)"), Some("Mon (projet)"));
        assert_eq!(conflict_of("Note (conflict hier)"), None);
        assert_eq!(conflict_of("Note (conflict 2026-10-09) bis"), None);
        assert_eq!(conflict_of(" (conflict 2026-10-09)"), None);
        let notes = [note("/v/Note.md", "# Note\n"), note("/v/Note (conflict 2026-10-09 pc).md", "# Note\n"), note("/v/Partie (conflict 2026-10-09).md", "")];
        let files = [file("/v/a (conflict 2026-10-09 pc).png", 1, FileKind::Picture)];
        assert_eq!(
            of(&scan_of(&notes, &files, &[]), Kind::Conflict),
            [
                Finding::Conflict { copy: "/v/Note (conflict 2026-10-09 pc).md".into(), original: Some("/v/Note.md".into()) },
                Finding::Conflict { copy: "/v/Partie (conflict 2026-10-09).md".into(), original: None },
                Finding::Conflict { copy: "/v/a (conflict 2026-10-09 pc).png".into(), original: None },
            ]
        );
    }

    #[test]
    fn front_matter_that_cannot_be_read() {
        assert_eq!(bad_front_matter("# Pas de front matter\n"), None);
        assert_eq!(bad_front_matter("---\ntags: [a]\naliases:\n  - b\n- c\n# note\n\n---\ntexte"), None);
        assert_eq!(bad_front_matter("---\ntags: [a]\npas de clé\n---\n"), Some(2));
        assert_eq!(bad_front_matter("---\ntags: a\n\nfin sans fermeture\n"), Some(0));
        let notes = [note("/v/Cassée.md", "---\ntitre: a\noups\n---\n# Cassée\ntexte [[Cassée]]")];
        assert_eq!(of(&scan_of(&notes, &[], &[]), Kind::BadFrontMatter), [Finding::BadFrontMatter { note: "/v/Cassée.md".into(), row: 2 }]);
    }

    #[test]
    fn empty_means_nothing_past_the_title() {
        assert!(is_empty(""));
        assert!(is_empty("\n\n  \n"));
        assert!(is_empty("# Titre\n\n"));
        assert!(is_empty("---\ntags: [a]\n---\n## Titre\n"));
        assert!(!is_empty("# Titre\nune ligne\n"));
        assert!(!is_empty("une ligne sans titre"));
        assert!(!is_empty("#tag"));
    }

    #[test]
    fn unused_and_heavy_files() {
        let notes = [
            note("/v/Album.md", "# Album\n![](img/vue%20mer.png)\n![[plan.pdf]]\n"),
            // A plain Markdown link uses its file too (the review's `rapport.pdf`).
            note("/v/Bilan.md", "# Bilan\nVoir [le rapport](docs/Rapport.pdf \"titre\") et [le site](https://x.org/a.zip).\n"),
        ];
        let files = [
            file("/v/img/vue mer.png", HEAVY + 1, FileKind::Picture),
            file("/v/img/oubliée.jpg", 10, FileKind::Picture),
            file("/v/plan.pdf", HEAVY * 10, FileKind::Attachment),
            file("/v/doc.zip", 1, FileKind::Attachment),
            file("/v/a.zip", 1, FileKind::Attachment),
            file("/v/docs/rapport.pdf", 1, FileKind::Attachment),
            file("/v/budget.csv", 1, FileKind::Document),
            // A Bref diagram is a document of its own, like a table (the review's `Schéma.svg`).
            file("/v/Schéma.svg", 1, FileKind::Document),
        ];
        let found = scan_of(&notes, &files, &[]);
        // A web address is not a file of the vault: `a.zip` stays unused.
        assert_eq!(
            of(&found, Kind::Unused),
            [Finding::Unused { path: "/v/a.zip".into() }, Finding::Unused { path: "/v/doc.zip".into() }, Finding::Unused { path: "/v/img/oubliée.jpg".into() }]
        );
        // Only pictures are heavy: a video or a PDF is expected to be.
        assert_eq!(of(&found, Kind::Heavy), [Finding::Heavy { path: "/v/img/vue mer.png".into(), bytes: HEAVY + 1 }]);
    }

    #[test]
    fn orphans_have_no_link_in_or_out() {
        let notes = [
            note("/v/A.md", "# A\n[[B]]\n"),
            note("/v/B.md", "# B\n"),
            note("/v/C.md", "# C\n![[c.png]]\n"),
            note("/v/Alias.md", "---\naliases: [Surnom]\n---\n# Alias\n"),
            note("/v/D.md", "# D\n[[surnom]] [[D]] [[nulle part]]\n"),
            note("/v/Seule.md", "# Seule\n[[Seule]]\n"),
            note("/v/2026-10-09.md", "# 2026-10-09\n"),
            // Listed as empty and as a conflict copy already: not orphans as well.
            note("/v/Vide.md", "# Vide\n"),
            note("/v/B (conflict 2026-10-09 pc).md", "# B\nautre\n"),
        ];
        let files = [file("/v/c.png", 1, FileKind::Picture)];
        assert_eq!(of(&scan_of(&notes, &files, &[]), Kind::Orphan), [Finding::Orphan { note: "/v/Seule.md".into() }]);
    }

    #[test]
    fn reads_the_files_of_plain_markdown_links() {
        let text = "---\nlien: [x](front.pdf)\n---\n[a](Dossier/Mon%20Fichier.PDF) [b](<avec espaces.zip>) [c](c.txt#part)\n```\n[d](code.pdf)\n```\n[e](https://site.org/e.pdf) [f](mailto:a@b.c) ![g](img/g.png)\n";
        assert_eq!(local_links(text), ["mon fichier.pdf", "avec espaces.zip", "c.txt", "g.png"]);
    }

    #[test]
    fn counts_broken_links_from_the_index() {
        let notes = [
            note("/v/A.md", "# A\n[[B]] [[Absente]] [[absente]] [[Modèle]] [[photo.png]]\n"),
            note("/v/B.md", "# B\n[[Autre absente]]\n"),
            note("/v/Templates/Modèle.md", "# {{title}}\n"),
        ];
        let files = [file("/v/photo.png", 1, FileKind::Picture)];
        // `Absente` once (the index lowercases it), `Autre absente`; a template is not a broken link.
        let vault = Vault { notes: &notes, files: &files, dirs: &[], template: &templates };
        assert_eq!(broken_count(&vault), 2);
        assert_eq!(broken_count(&vault), of(&scan(&vault), Kind::BrokenLink).len());
    }

    #[test]
    fn findings_come_by_kind_with_their_counts() {
        let notes = [note("/v/Vide.md", ""), note("/v/Lien.md", "# Lien\n[[Vide]] [[Absente]] [[Autre absente]]")];
        let found = scan_of(&notes, &[], &[]);
        assert_eq!(counts(&found), [(Kind::BrokenLink, 2), (Kind::Empty, 1)]);
    }

    #[test]
    fn fix_all_relinks_once_per_note() {
        let notes = [
            note("/v/Recettes.md", "# Recettes\n"),
            note("/v/Projet.md", "# Projet\n"),
            note("/v/Projets.md", "# Projets\n"),
            note("/v/Lundi.md", "# Lundi\n[[Recetes|à cuisiner]] puis [[Recetes]]\n[[Projetz]]\n[[recetes#Dessert]]\n"),
        ];
        let found = scan_of(&notes, &[], &[]);
        // `Projetz` has two candidates: not safe. Both spellings of `Recetes` are fixed.
        let relinks = safe_relinks(&found);
        assert_eq!(relinks, [(PathBuf::from("/v/Lundi.md"), vec![("Recetes".into(), "Recettes".into()), ("recetes".into(), "Recettes".into())])]);
        assert_eq!(
            apply_relinks(&notes[3].body, &relinks[0].1).as_deref(),
            Some("# Lundi\n[[Recettes|à cuisiner]] puis [[Recettes]]\n[[Projetz]]\n[[Recettes#Dessert]]\n")
        );
        assert_eq!(apply_relinks("rien", &relinks[0].1), None);
    }

    #[test]
    fn removing_the_brackets_keeps_what_was_shown() {
        let text = "# N\n[[Absente|vue ici]], [[absente#Partie]] et [[Autre]]\n![[Absente]]\n";
        assert_eq!(unbracket(text, 1, "Absente").as_deref(), Some("# N\nvue ici, absente#Partie et [[Autre]]\n![[Absente]]\n"));
        assert_eq!(unbracket(text, 2, "Absente"), None);
        assert_eq!(unbracket(text, 0, "Absente"), None);
    }
}
