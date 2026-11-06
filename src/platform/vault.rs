//! Vault: a folder of `.md` files, plus the app's small config.

use std::{
    collections::{HashMap, HashSet},
    env, fs,
    hash::{DefaultHasher, Hash, Hasher},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use super::signal::Signal;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};

use crate::{logic::markdown, lang::tr};
use crate::logic::note::Note;

/// Folder offered for a first vault: `Notes` in the user's Documents, or
/// in their home folder if they have no Documents.
pub fn default_vault() -> Option<PathBuf> {
    let home = env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)?;
    let documents = home.join("Documents");
    Some(if documents.is_dir() { documents } else { home }.join("Notes"))
}

/// A path as shown: the home folder shortened to `~` outside Windows.
pub fn shown(path: &Path) -> String {
    let home = env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    match home.as_deref().and_then(|home| path.strip_prefix(home).ok()) {
        Some(rest) if !cfg!(windows) => format!("~/{}", rest.display()),
        _ => path.display().to_string(),
    }
}

pub use crate::logic::files::*;

pub fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The names of the templates folder, at the vault root (ignoring case and accents).
const TEMPLATE_DIRS: [&str; 2] = ["templates", "modeles"];

fn is_template_dir(name: &str) -> bool {
    let name = crate::lang::fold(name);
    TEMPLATE_DIRS.contains(&name.as_str())
}

/// The vault's templates folder: `Templates` or `Modèles`, among its folders.
pub fn template_dir(root: &Path, dirs: &[PathBuf]) -> Option<PathBuf> {
    dirs.iter().find(|d| d.parent() == Some(root) && d.file_name().is_some_and(|n| is_template_dir(&n.to_string_lossy()))).cloned()
}

/// The note is a template: it is in the templates folder, however deep under it. A template
/// is neither in the graph nor in tags and links: its `{{words}}` are not links.
pub fn in_templates(root: &Path, path: &Path) -> bool {
    let Ok(rel) = path.strip_prefix(root) else {
        return false;
    };
    let mut parts = rel.components();
    parts.next_back().is_some() && parts.next().is_some_and(|first| is_template_dir(&first.as_os_str().to_string_lossy()))
}

/// The daily note template among the templates: the one called `Daily`, `Journal`, `Today`, `Note du jour`…
pub fn daily_template(templates: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    let named: Vec<(PathBuf, String)> = templates.into_iter().map(|path| {
        let name = stem(&path);
        (path, name)
    }).collect();
    crate::logic::daily::pick(named.iter().map(|(path, name)| (path.clone(), name.as_str())))
}

/// Path of `path` relative to the vault, with `/` even on Windows.
pub fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root).unwrap_or(path).to_string_lossy().replace('\\', "/")
}

/// True the first time this folder is entered. A symbolic link that points back up to a
/// parent folder (or two links to the same one) must neither loop nor duplicate notes.
fn first_visit(visited: &mut HashSet<PathBuf>, dir: &Path) -> bool {
    visited.insert(fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf()))
}

/// The entry is a folder, link followed. Its type comes with the folder listing: the disk
/// is queried only for a link, not once per file in the vault.
fn is_dir(entry: &fs::DirEntry) -> bool {
    entry.file_type().is_ok_and(|kind| kind.is_dir() || (kind.is_symlink() && entry.path().is_dir()))
}

/// Fingerprint of the vault's state: the paths of its notes, the other files it shows and its folders,
/// and the date of each file. It changes as soon as another program touches it, without
/// reading a single file.
pub fn fingerprint(root: &Path) -> u64 {
    let mut seen = Vec::new();
    let mut visited = HashSet::new();
    first_visit(&mut visited, root);
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for entry in fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            if is_dir(&entry) {
                if first_visit(&mut visited, &path) {
                    seen.push((path.clone(), None));
                    dirs.push(path);
                }
            } else if path.extension().is_some_and(|e| e == "md") {
                seen.push((path, entry.metadata().and_then(|m| m.modified()).ok()));
            } else if is_file_shown(&path) {
                // Another program adding an attachment, or rewriting the open table or diagram, must be seen.
                seen.push((path, entry.metadata().and_then(|m| m.modified()).ok()));
            }
        }
    }
    // The order in which a folder is read is not guaranteed.
    seen.sort();
    let mut hasher = DefaultHasher::new();
    seen.hash(&mut hasher);
    hasher.finish()
}

/// A vault change worth rereading the disk for: not a mere read,
/// and not a hidden file or folder (`.trash`, `.git`…).
fn matters(root: &Path, event: &Event) -> bool {
    let hidden = |path: &PathBuf| {
        path.strip_prefix(root)
            .is_ok_and(|rel| rel.components().any(|c| c.as_os_str().to_string_lossy().starts_with('.')))
    };
    !event.kind.is_access() && (event.paths.is_empty() || !event.paths.iter().all(hidden))
}

/// Asks the system to report any change in the vault: `changed` is raised
/// as soon as there is one. `None` if the system cannot do it (inotify limit
/// reached, network folder): the caller then rereads the vault at regular intervals.
/// Watching stops when the result is dropped.
pub fn watch(root: &Path, changed: Arc<Signal>) -> Option<RecommendedWatcher> {
    let base = root.to_path_buf();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
        // A system error: events may have been lost, better reread.
        if event.map_or(true, |e| matters(&base, &e)) {
            changed.raise();
        }
    })
    .ok()?;
    watcher.watch(root, RecursiveMode::Recursive).ok()?;
    Some(watcher)
}

/// All the vault's notes (recursive, hidden folders ignored), most recent
/// first, all its folders, even empty, and its other files: images and tables.
#[cfg(test)]
pub fn scan(root: &Path) -> (Vec<Note>, Vec<PathBuf>, Vec<PathBuf>) {
    rescan(root, &[])
}

/// Like `scan`, without rereading the notes of `known` whose date has not changed.
pub fn rescan(root: &Path, known: &[Note]) -> (Vec<Note>, Vec<PathBuf>, Vec<PathBuf>) {
    let (notes, dirs, images, _) = rescan_full(root, known);
    (notes, dirs, images)
}

/// Folders that could not be listed, and why.
pub type Unreadable = Vec<(PathBuf, io::ErrorKind)>;

/// Like `rescan`, with the folders that could not be listed and why: the root itself when the whole
/// vault cannot be read. A folder inside one of them is not reached, so not reported.
pub fn rescan_full(root: &Path, known: &[Note]) -> (Vec<Note>, Vec<PathBuf>, Vec<PathBuf>, Unreadable) {
    let mut unreadable = Vec::new();
    let known: HashMap<&Path, &Note> = known.iter().map(|n| (n.path.as_path(), n)).collect();
    let mut files = Vec::new();
    let mut images = Vec::new();
    let mut found = Vec::new();
    let mut visited = HashSet::new();
    first_visit(&mut visited, root);
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) => {
                unreadable.push((dir, e.kind()));
                continue;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            if is_dir(&entry) {
                if first_visit(&mut visited, &path) {
                    found.push(path.clone());
                    dirs.push(path);
                }
            } else if path.extension().is_some_and(|e| e == "md") {
                files.push((path, entry.metadata().and_then(|m| m.modified()).unwrap_or(UNIX_EPOCH)));
            } else if is_file_shown(&path) {
                images.push(path);
            }
        }
    }
    let note = |(path, mtime): &(PathBuf, SystemTime)| {
        let template = in_templates(root, path);
        match known.get(path.as_path()) {
            // Unchanged on disk: reused as is.
            Some(old) if old.mtime == *mtime => {
                // Shared lists, nothing reparsed; a template keeps none of them.
                let mut note = (*old).clone();
                if template {
                    (note.tags, note.links, note.aliases) = (Arc::from([]), Arc::from([]), Arc::from([]));
                }
                note
            }
            _ => {
                let text = Arc::from(fs::read_to_string(path).unwrap_or_default());
                Note::from_text(stem(path), path.clone(), *mtime, text, template)
            }
        }
    };
    let mut notes: Vec<Note> = files.iter().map(note).collect();
    notes.sort_by_key(|note| std::cmp::Reverse(note.mtime));
    (notes, found, images, unreadable)
}

/// File or folder name stripped of what a file system or
/// a wikilink refuses; `None` if nothing is left.
pub fn clean_name(name: &str) -> Option<String> {
    let name: String = name
        .chars()
        .filter(|c| !c.is_control() && !r#"/\:*?"<>|#^[]"#.contains(*c))
        .take(80)
        .collect();
    // No leading dot (hidden file) nor trailing one (refused by Windows).
    let name = name.trim().trim_matches('.').trim();
    (!name.is_empty()).then(|| name.to_string())
}

/// The entry of `dir` called `file` (a file name), ignoring case: the file systems of Windows and
/// macOS do, so a name that differs only by case is taken there, and would be a trap elsewhere.
pub fn name_taken(dir: &Path, file: &str) -> Option<PathBuf> {
    let want = file.to_lowercase();
    fs::read_dir(dir).ok()?.flatten().find(|entry| entry.file_name().to_string_lossy().to_lowercase() == want).map(|entry| entry.path())
}

/// `to` is the name of something that is not `from` (only the case of its own name may change).
pub fn taken_by_other(from: &Path, to: &Path) -> bool {
    let taken = to.parent().zip(to.file_name()).and_then(|(dir, file)| name_taken(dir, &file.to_string_lossy()));
    taken.is_some_and(|other| other != from)
}

/// Moves or renames, never overwriting what already has that name. Only the case of
pub fn rename(from: &Path, to: &Path) -> io::Result<()> {
    if taken_by_other(from, to) {
        let taken = tr("this name is already taken", "ce nom est déjà pris");
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, taken));
    }
    fs::rename(from, to)
}

/// Text of a new note called `name`: its title and a blank line to write under it.
pub fn new_note_text(name: &str) -> String {
    format!("# {name}\n\n")
}

/// Where the note goes when its title says `title`: the file follows the title only if it did
/// until now (`synced`). `None`: it keeps its name. The caller still refuses a name that is taken.
pub fn title_rename(path: &Path, title: &str, synced: bool) -> Option<PathBuf> {
    (synced && stem(path) != title).then(|| path.with_file_name(format!("{title}.md")))
}

/// Renames a note and returns its path. If its name followed its title, the
/// first line is rewritten so they stay in step; if that write fails, the rename is undone.
pub fn rename_note(path: &Path, name: &str) -> io::Result<PathBuf> {
    let to = path.with_file_name(format!("{name}.md"));
    rename(path, &to)?;
    if let Err(e) = retitle(&to, &stem(path), name) {
        rename(&to, path).ok();
        return Err(e);
    }
    Ok(to)
}

/// The note's title was `old` (its former name): its first line becomes `new`, written atomically.
/// The front matter stays as is: the title is the first line after it.
pub fn retitle(path: &Path, old: &str, new: &str) -> io::Result<()> {
    let content = fs::read_to_string(path)?;
    let (meta, body) = content.split_at(markdown::front_matter(&content));
    if old == title_of(&content)
        && let Some(first) = body.lines().find(|l| !l.trim().is_empty())
    {
        let hashes = first.trim_start().chars().take_while(|c| *c == '#').count();
        let title = if hashes > 0 { format!("{} {new}", "#".repeat(hashes)) } else { new.to_string() };
        write(path, format!("{meta}{}", body.replacen(first, &title, 1)))?;
    }
    Ok(())
}

/// Heading `was` of the note at `path` is now `now`: the other notes that link to it, `[[Note#was]]` and
/// `![[Note#was]]`, are rewritten on disk and re-indexed. Returns the notes rewritten.
pub fn follow_heading(root: Option<&Path>, notes: &mut [Note], path: &Path, was: &str, now: &str) -> Vec<PathBuf> {
    let (name, key) = (stem(path), stem(path).to_lowercase());
    let mut rewritten = Vec::new();
    for note in notes.iter_mut().filter(|n| n.path != path && n.links.contains(&key)) {
        let fresh = fs::read_to_string(&note.path)
            .ok()
            .map(|text| crate::logic::embed::rename_heading(&text, &name, was, now))
            .filter(|(_, count)| *count > 0)
            .filter(|(text, _)| write(&note.path, text).is_ok());
        if let Some((text, _)) = fresh {
            note.set_text(text.as_str().into(), root.is_some_and(|r| in_templates(r, &note.path)));
            rewritten.push(note.path.clone());
        }
    }
    rewritten
}

/// `[[old]]` becomes `[[new]]` in every note that links to it, on disk and in the index, unless
/// another note is still called `old` (the links may be meant for it). Returns the notes rewritten.
pub fn relink(root: Option<&Path>, notes: &mut [Note], old: &str, new: &str) -> Vec<PathBuf> {
    let Some(targets) = crate::logic::wiki::relink_set(notes, old, new) else {
        return Vec::new();
    };
    let paths: Vec<PathBuf> = targets.iter().map(|n| n.path.clone()).collect();
    let mut rewritten = Vec::new();
    for note in notes.iter_mut().filter(|n| paths.contains(&n.path)) {
        let fresh = fs::read_to_string(&note.path)
            .ok()
            .and_then(|text| markdown::relink(&text, old, new))
            .filter(|text| write(&note.path, text).is_ok());
        if let Some(text) = fresh {
            note.set_text(text.as_str().into(), root.is_some_and(|r| in_templates(r, &note.path)));
            rewritten.push(note.path.clone());
        }
    }
    rewritten
}

/// Moves the note or folder to the vault's trash: the hidden folder
/// `.trash`, which the index ignores. Nothing is destroyed.
pub fn trash(root: &Path, path: &Path) -> io::Result<PathBuf> {
    let bin = root.join(".trash");
    fs::create_dir_all(&bin)?;
    let free = free_name(&bin, path.file_name().unwrap_or_default(), path.is_dir());
    fs::rename(path, &free)?;
    Ok(free)
}

/// Vault file that keeps the icons: one `icon<tab>path` line per item,
/// the path relative to the vault and written with `/`. It travels with the vault.
const ICONS: &str = ".bref-icons";

/// Icons chosen for notes, files and folders of the vault, by path.
pub fn load_icons(root: &Path) -> HashMap<PathBuf, String> {
    let text = fs::read_to_string(root.join(ICONS)).unwrap_or_default();
    text.lines().filter_map(|line| line.split_once('\t')).map(|(icon, path)| (root.join(path), icon.to_string())).collect()
}

pub fn save_icons(root: &Path, icons: &HashMap<PathBuf, String>) -> io::Result<()> {
    let line = |(path, icon): (&PathBuf, &String)| {
        let inside = path.strip_prefix(root).ok()?.to_string_lossy().replace('\\', "/");
        Some(format!("{icon}\t{inside}\n"))
    };
    let mut lines: Vec<String> = icons.iter().filter_map(line).collect();
    lines.sort();
    write(&root.join(ICONS), &lines.concat())
}

/// `from` became `to` (a file, or a folder and what it holds): its icons follow. False if none did.
pub fn move_icons(icons: &mut HashMap<PathBuf, String>, from: &Path, to: &Path) -> bool {
    if !icons.keys().any(|p| p.starts_with(from)) {
        return false;
    }
    *icons = icons.drain().map(|(path, icon)| (if let Ok(rest) = path.strip_prefix(from) { to.join(rest) } else { path }, icon)).collect();
    true
}

/// The same on the vault's icon file, for what renames without the app's state.
pub fn relocate_icons(root: &Path, from: &Path, to: &Path) -> io::Result<()> {
    let mut icons = load_icons(root);
    if move_icons(&mut icons, from, to) { save_icons(root, &icons) } else { Ok(()) }
}

/// Contents of the vault's trash, most recently modified first.
pub fn trashed(root: &Path) -> Vec<PathBuf> {
    let entries = fs::read_dir(root.join(".trash")).into_iter().flatten().flatten();
    let mut found: Vec<(SystemTime, PathBuf)> =
        entries.map(|e| (e.metadata().and_then(|m| m.modified()).unwrap_or(UNIX_EPOCH), e.path())).collect();
    found.sort_by(|a, b| b.cmp(a));
    found.into_iter().map(|(_, path)| path).collect()
}

/// Takes `path` out of the trash and puts it back at the vault root (the trash does not keep
/// where it came from), under its name, numbered if taken: nothing is overwritten.
pub fn restore(root: &Path, path: &Path) -> io::Result<PathBuf> {
    let free = free_name(root, path.file_name().unwrap_or_default(), path.is_dir());
    fs::rename(path, &free)?;
    Ok(free)
}

/// Free path in `dir` for `name.extension` (no dot if `extension` is empty): the name is followed
/// by a number if taken, `name 2.extension`, `name 3.extension`… Nothing is overwritten. Case is
/// ignored (see `name_taken`), and a dangling link counts as taken.
pub fn free_path(dir: &Path, name: &str, extension: &str) -> PathBuf {
    let taken: HashSet<String> = fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.file_name().to_string_lossy().to_lowercase()).collect();
    let dot = if extension.is_empty() { "" } else { "." };
    (1..)
        .map(|n| if n == 1 { format!("{name}{dot}{extension}") } else { format!("{name} {n}{dot}{extension}") })
        .find(|file| !taken.contains(&file.to_lowercase()) && dir.join(file).symlink_metadata().is_err())
        .map(|file| dir.join(file))
        .unwrap()
}

/// `free_path`, created empty in the same step (`create_new`): two writers choosing a name at once
/// never share it, the second one gets the next number.
pub fn create_free(dir: &Path, name: &str, extension: &str) -> io::Result<(PathBuf, fs::File)> {
    loop {
        let path = free_path(dir, name, extension);
        match fs::File::create_new(&path) {
            Ok(file) => return Ok((path, file)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
}

/// File name (without extension) taken from the first non-empty line.
pub fn title_of(content: &str) -> String {
    let content = &content[markdown::front_matter(content)..];
    let first = content.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    clean_name(first.trim().trim_start_matches('#'))
        .unwrap_or_else(|| tr("Untitled", "Sans titre").to_string())
}

/// `# …` title at the top of the note, as a file name.
pub fn h1_of(content: &str) -> Option<String> {
    let content = &content[markdown::front_matter(content)..];
    let first = content.lines().find(|l| !l.trim().is_empty())?;
    clean_name(first.trim_start().strip_prefix("# ")?)
}

/// A dropped item, stored in the vault: where it is, whether it is a folder, how many files it holds.
#[derive(Debug, PartialEq)]
pub struct Imported {
    pub to: PathBuf,
    pub dir: bool,
    pub files: usize,
}

/// A free name in `dir` for a dropped item: a file keeps its extension, a folder's name is whole.
fn free_name(dir: &Path, name: &std::ffi::OsStr, folder: bool) -> PathBuf {
    let as_path = Path::new(name);
    match (folder, as_path.extension()) {
        (false, Some(ext)) => free_path(dir, &as_path.file_stem().unwrap_or(name).to_string_lossy(), &ext.to_string_lossy()),
        _ => free_path(dir, &name.to_string_lossy(), ""),
    }
}

/// Copies `from` to `to`: a file is first written under a hidden name then renamed, so an interrupted
/// copy never leaves a half file. Returns the number of files copied and of items
/// left out (symbolic links, hidden files, things other than files).
fn copy_into(from: &Path, to: &Path, skipped: &mut usize) -> io::Result<usize> {
    if from.is_dir() {
        fs::create_dir(to)?;
        let mut entries: Vec<_> = fs::read_dir(from)?.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        let mut files = 0;
        for entry in entries {
            let (path, kind) = (entry.path(), entry.file_type()?);
            if entry.file_name().to_string_lossy().starts_with('.') || kind.is_symlink() || !(kind.is_file() || kind.is_dir()) {
                *skipped += 1;
                continue;
            }
            files += copy_into(&path, &to.join(entry.file_name()), skipped)?;
        }
        return Ok(files);
    }
    let part = to.with_file_name(format!(".{}.bref-part", to.file_name().unwrap_or_default().to_string_lossy()));
    let copied = fs::copy(from, &part).and_then(|_| fs::rename(&part, to));
    if copied.is_err() {
        let _ = fs::remove_file(&part);
    }
    copied.map(|()| 1)
}

/// Stores in `dir` files and folders from elsewhere (a drop): each under a free name, a
/// folder with everything it holds except what is hidden or a symbolic link. Returns what was stored and,
/// for each thing left out, why. A folder is never copied into itself.
pub fn import(sources: &[PathBuf], dir: &Path) -> (Vec<Imported>, Vec<String>) {
    let (mut done, mut problems) = (Vec::new(), Vec::new());
    for from in sources {
        let name = from.file_name().unwrap_or_default();
        let kind = from.symlink_metadata();
        let shown = from.display();
        match kind {
            Err(e) => problems.push(format!("{shown} : {e}")),
            Ok(kind) if kind.is_symlink() || !(kind.is_file() || kind.is_dir()) => problems.push(format!("{shown} : not a file or a folder")),
            Ok(_) if name.is_empty() || name.to_string_lossy().starts_with('.') => problems.push(format!("{shown} : hidden")),
            Ok(kind) if kind.is_dir() && dir.starts_with(from) => problems.push(format!("{shown} : a folder cannot be copied into itself")),
            Ok(kind) => {
                let to = free_name(dir, name, kind.is_dir());
                let mut skipped = 0;
                match copy_into(from, &to, &mut skipped) {
                    Ok(files) => {
                        if skipped > 0 {
                            problems.push(format!("{shown} : {skipped} hidden or linked items left out"));
                        }
                        done.push(Imported { to, dir: kind.is_dir(), files });
                    }
                    Err(e) => {
                        // A half-copied folder goes entirely: nothing is left behind.
                        if kind.is_dir() {
                            let _ = fs::remove_dir_all(&to);
                        }
                        problems.push(format!("{shown} : {e}"));
                    }
                }
            }
        }
    }
    (done, problems)
}

/// Saves a pasted image in `dir`, under a free name, and returns that name.
pub fn save_image(dir: &Path, extension: &str, bytes: &[u8]) -> io::Result<String> {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (path, mut file) = create_free(dir, &format!("image-{stamp}"), extension)?;
    if let Err(e) = file.write_all(bytes) {
        let _ = fs::remove_file(&path);
        return Err(e);
    }
    Ok(path.file_name().unwrap_or_default().to_string_lossy().into_owned())
}

/// Appends the line `text` at the bottom of the note named `name` (the daily note), wherever it is
/// stored in the vault; failing that creates it at the root, titled with its name. Returns its file.
/// A capture reads only file names, never notes: it stays instant
/// in a big vault.
pub fn capture(root: &Path, name: &str, text: &str) -> io::Result<PathBuf> {
    let file = format!("{name}.md");
    let mut visited = HashSet::new();
    first_visit(&mut visited, root);
    let mut dirs = vec![root.to_path_buf()];
    let mut found = None;
    while let (None, Some(dir)) = (&found, dirs.pop()) {
        for entry in fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            if is_dir(&entry) {
                if first_visit(&mut visited, &path) {
                    dirs.push(path);
                }
            } else if entry.file_name().to_string_lossy() == file {
                found = Some(path);
            }
        }
    }
    let path = found.unwrap_or_else(|| root.join(&file));
    let mut body = match fs::read_to_string(&path) {
        Ok(body) => body,
        Err(e) if e.kind() == io::ErrorKind::NotFound => new_note_text(name),
        Err(e) => return Err(e),
    };
    if !body.ends_with('\n') {
        body.push('\n');
    }
    // A list line, unless it is already written that way (`- [ ] …`).
    let text = text.trim();
    body.push_str(&if text.starts_with("- ") { format!("{text}\n") } else { format!("- {text}\n") });
    write(&path, &body)?;
    Ok(path)
}

/// Atomic write: a crash never leaves a truncated note.
pub fn write(path: &Path, content: impl AsRef<[u8]>) -> io::Result<()> {
    let (tmp, mut file) = staging(path)?;
    let result = (|| {
        file.write_all(content.as_ref())?;
        // Without `fsync`, a power cut can leave the rename without the data.
        file.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        fs::remove_file(&tmp).ok();
    }
    result
}

/// Paths whose next writes fail, so a test can play a failed save on every platform.
#[cfg(test)]
static REFUSED: std::sync::Mutex<Vec<PathBuf>> = std::sync::Mutex::new(Vec::new());

/// Makes every write of `path` fail (`on`), or succeed again.
#[cfg(test)]
pub fn refuse_writes(path: &Path, on: bool) {
    let mut refused = REFUSED.lock().unwrap();
    refused.retain(|p| p != path);
    if on {
        refused.push(path.to_path_buf());
    }
}

/// A new hidden file beside `path`, for an atomic write. Its name is unique to this process
/// and this write: two files of the same stem (`Note.md`, `Note.csv`), or the app and
/// `bref mcp` saving the same note at once, never share it.
pub fn staging(path: &Path) -> io::Result<(PathBuf, fs::File)> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    #[cfg(test)]
    if REFUSED.lock().unwrap().iter().any(|p| p == path) {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "refused by a test"));
    }
    loop {
        // Not built from the file's name: a name near the length limit would leave no room.
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let tmp = path.with_file_name(format!(".bref-{}-{n}.tmp", std::process::id()));
        match fs::File::create_new(&tmp) {
            Ok(file) => return Ok((tmp, file)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
}

/// Saves the note and returns its path; a new note is created in
/// `dir`. If `synced` (the file name
/// already followed the title), the file is renamed when the title changes, unless
/// the new name is already taken.
pub fn save(
    dir: &Path,
    current: Option<&Path>,
    synced: bool,
    content: &str,
) -> io::Result<PathBuf> {
    let title = title_of(content);
    let target = match current {
        Some(p) => title_rename(p, &title, synced).filter(|to| !taken_by_other(p, to)).unwrap_or_else(|| p.to_path_buf()),
        None => create_free(dir, &title, "md")?.0,
    };
    if let Err(e) = write(&target, content) {
        // A name reserved for a new note is not left empty behind a failed write.
        if current.is_none() {
            let _ = fs::remove_file(&target);
        }
        return Err(e);
    }
    if let Some(old) = current
        && old != target
    {
        fs::remove_file(old)?;
    }
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_appends_a_line_to_the_note_of_the_day() {
        let root = std::env::temp_dir().join(format!("bref-capture-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("Journal")).unwrap();
        fs::create_dir_all(root.join(".trash")).unwrap();
        // No daily note: it is created at the root, titled with its name.
        let made = capture(&root, "2026-10-10", "appeler Léa").unwrap();
        assert_eq!(made, root.join("2026-10-10.md"));
        assert_eq!(fs::read_to_string(&made).unwrap(), "# 2026-10-10\n\n- appeler Léa\n");
        // The next one goes below; a line already written as a list stays as is.
        capture(&root, "2026-10-10", "- [ ] acheter du pain").unwrap();
        assert_eq!(fs::read_to_string(&made).unwrap(), "# 2026-10-10\n\n- appeler Léa\n- [ ] acheter du pain\n");
        // The daily note stored in a folder is found; the one in the trash is not.
        let filed = root.join("Journal/2026-10-11.md");
        fs::write(&filed, "# 2026-10-11\n\nsans fin de ligne").unwrap();
        fs::write(root.join(".trash/2026-10-11.md"), "jetée").unwrap();
        assert_eq!(capture(&root, "2026-10-11", "une idée").unwrap(), filed);
        assert_eq!(fs::read_to_string(&filed).unwrap(), "# 2026-10-11\n\nsans fin de ligne\n- une idée\n");
        assert_eq!(fs::read_to_string(root.join(".trash/2026-10-11.md")).unwrap(), "jetée");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_name_is_taken_whatever_its_case() {
        let root = env::temp_dir().join(format!("bref-taken-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("Dossier")).unwrap();
        fs::write(root.join("Plan.md"), "# Plan\n\n").unwrap();
        assert_eq!(name_taken(&root, "plan.MD"), Some(root.join("Plan.md")));
        assert_eq!(name_taken(&root, "dossier"), Some(root.join("Dossier")));
        assert_eq!((name_taken(&root, "Autre.md"), name_taken(&root.join("absent"), "x")), (None, None));
        // Another file refuses the name, even in other letters; the file itself may change case.
        fs::write(root.join("B.md"), "").unwrap();
        assert!(rename(&root.join("B.md"), &root.join("pLAN.md")).is_err());
        rename(&root.join("B.md"), &root.join("b.md")).unwrap();
        assert!(root.join("b.md").exists());
        // The title renames the file only while it follows it; the empty-ish names stay as they are.
        let path = root.join("Plan.md");
        assert_eq!(title_rename(&path, "Autre", true), Some(root.join("Autre.md")));
        assert_eq!((title_rename(&path, "Autre", false), title_rename(&path, "Plan", true)), (None, None));
        assert_eq!(new_note_text("Plan"), "# Plan\n\n");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn free_names_are_numbered_and_reserved() {
        let root = env::temp_dir().join(format!("bref-free-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        assert_eq!(free_path(&root, "Note", "md"), root.join("Note.md"));
        // Reserved at once: the next asker gets the next number; case is ignored.
        let (first, _) = create_free(&root, "Note", "md").unwrap();
        let (second, _) = create_free(&root, "note", "MD").unwrap();
        let (third, _) = create_free(&root, "Note", "md").unwrap();
        assert_eq!((first, second, third), (root.join("Note.md"), root.join("note 2.MD"), root.join("Note 3.md")));
        assert_eq!(free_path(&root, "Dossier", ""), root.join("Dossier"));
        // A new note takes the number, and a failed write leaves no empty file behind.
        fs::write(root.join("Idée.md"), "# Idée\n").unwrap();
        let made = save(&root, None, false, "# Idée\nautre").unwrap();
        assert_eq!(made, root.join("Idée 2.md"));
        refuse_writes(&root.join("Idée 3.md"), true);
        assert!(save(&root, None, false, "# Idée\nencore").is_err());
        refuse_writes(&root.join("Idée 3.md"), false);
        assert!(!root.join("Idée 3.md").exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn derives_titles() {
        assert_eq!(title_of("\n# Courses: lundi\nlait"), "Courses lundi");
        assert_eq!(title_of("  [[x]] / y  "), "x  y");
        assert_eq!(title_of("\n\n"), "Sans titre");
        assert_eq!(title_of("# .."), "Sans titre");
        assert_eq!(h1_of("\n# Courses: lundi\n"), Some("Courses lundi".into()));
        assert_eq!((h1_of("## Sous-titre"), h1_of("texte"), h1_of("# ")), (None, None, None));
    }

    #[test]
    fn saves_and_renames() {
        let root = env::temp_dir().join(format!("bref-test-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let a = save(&root, None, true, "# A\n").unwrap();
        assert_eq!(a, root.join("A.md"));
        // New note with the same title: no overwrite.
        assert_eq!(save(&root, None, true, "# A\n").unwrap(), root.join("A 2.md"));
        // The title changes: the file follows.
        let b = save(&root, Some(&a), true, "# B\n").unwrap();
        assert_eq!(b, root.join("B.md"));
        assert!(!a.exists());
        // Name already taken or note not synced: the file is kept.
        assert_eq!(save(&root, Some(&b), true, "# A 2\n").unwrap(), b);
        assert_eq!(save(&root, Some(&b), false, "# Z\n").unwrap(), b);
        assert_eq!(fs::read_to_string(&b).unwrap(), "# Z\n");
        assert_eq!(scan(&root).0.len(), 2);
        // Front matter: the name comes from the title after it, and renaming touches only that title.
        let meta = "---\ntitle: autre\n---\n";
        let c = save(&root, None, true, &format!("{meta}# Fiche\n---\n")).unwrap();
        assert_eq!((c.clone(), h1_of(&format!("{meta}\n# Fiche\n"))), (root.join("Fiche.md"), Some("Fiche".into())));
        let c = rename_note(&c, "Carte").unwrap();
        assert_eq!(fs::read_to_string(&c).unwrap(), format!("{meta}# Carte\n---\n"));
        fs::remove_file(&c).unwrap();
        // The vault's fingerprint follows what another program changes in it.
        let before = fingerprint(&root);
        assert_eq!(before, fingerprint(&root));
        fs::write(root.join("ailleurs.md"), "x").unwrap();
        assert_ne!(before, fingerprint(&root));
        fs::remove_file(root.join("ailleurs.md")).unwrap();
        // So does a drawing rewritten in place (an agent editing the open diagram), with no other file touched.
        let drawing = root.join("schéma.svg");
        fs::write(&drawing, "<svg/>").unwrap();
        let before = fingerprint(&root);
        let later = fs::metadata(&drawing).unwrap().modified().unwrap() + std::time::Duration::from_secs(1);
        fs::File::options().write(true).open(&drawing).unwrap().set_modified(later).unwrap();
        assert_ne!(before, fingerprint(&root));
        fs::remove_file(&drawing).unwrap();
        // So does an attachment added by another program: the tree shows it.
        let before = fingerprint(&root);
        fs::write(root.join("facture.pdf"), "%PDF").unwrap();
        assert_ne!(before, fingerprint(&root));
        fs::remove_file(root.join("facture.pdf")).unwrap();
        // Two files of the same stem never share a temporary file, and none is left behind.
        let (a, _) = staging(&root.join("Note.md")).unwrap();
        let (b, _) = staging(&root.join("Note.csv")).unwrap();
        let (c, _) = staging(&root.join("Note.md")).unwrap();
        assert!(a != b && a != c && b != c);
        for t in [a, b, c] {
            fs::remove_file(t).unwrap();
        }
        write(&root.join("Note.md"), "# Note\n").unwrap();
        assert!(!fs::read_dir(&root).unwrap().flatten().any(|e| e.file_name().to_string_lossy().ends_with(".tmp")));
        fs::remove_file(root.join("Note.md")).unwrap();

        // Renaming a note in step with its title rewrites that title; otherwise the text stays as is.
        let c = rename_note(&root.join("A 2.md"), "C").unwrap();
        assert_eq!(fs::read_to_string(&c).unwrap(), "# A\n");
        fs::write(root.join("D.md"), "\n## D\ntexte D\n").unwrap();
        let e = rename_note(&root.join("D.md"), "E").unwrap();
        assert_eq!(fs::read_to_string(&e).unwrap(), "\n## E\ntexte D\n");
        // A title that cannot be written leaves the note under its old name.
        fs::write(root.join("F.md"), "# F\n").unwrap();
        refuse_writes(&root.join("G.md"), true);
        assert!(rename_note(&root.join("F.md"), "G").is_err());
        refuse_writes(&root.join("G.md"), false);
        assert!(root.join("F.md").exists() && !root.join("G.md").exists());
        fs::remove_file(root.join("F.md")).unwrap();
        // Never overwrite.
        assert!(rename_note(&e, "C").is_err() && e.exists());

        // Trash: nothing is destroyed, and the index does not see it.
        fs::create_dir(root.join("vide")).unwrap();
        trash(&root, &c).unwrap();
        fs::write(&c, "autre").unwrap();
        trash(&root, &c).unwrap();
        assert_eq!(fs::read_to_string(root.join(".trash/C.md")).unwrap(), "# A\n");
        assert_eq!(fs::read_to_string(root.join(".trash/C 2.md")).unwrap(), "autre");
        fs::write(root.join("vide/Photo.PNG"), "").unwrap();
        let (notes, dirs, images) = scan(&root);
        assert_eq!((notes.len(), dirs), (2, vec![root.join("vide")]));
        // Each note keeps its text as is, and the same in lowercase for search.
        assert!(notes.iter().all(|n| !n.body.is_empty() && *n.lower == n.body.to_lowercase()));
        assert!(notes.iter().any(|n| n.lower != n.body));
        assert_eq!(images, [root.join("vide/Photo.PNG")]);
        assert_eq!(clean_name(" ../a:b. "), Some("ab".into()));
        assert_eq!(clean_name(" . "), None);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn keeps_icons_with_the_vault() {
        let root = env::temp_dir().join(format!("bref-icons-{}", std::process::id()));
        fs::create_dir_all(root.join("Projets")).unwrap();
        assert!(load_icons(&root).is_empty());
        let icons = HashMap::from([(root.join("Projets"), "rocket".to_string()), (root.join("Projets/Idées.md"), "idea".to_string())]);
        save_icons(&root, &icons).unwrap();
        assert_eq!(fs::read_to_string(root.join(ICONS)).unwrap(), "idea\tProjets/Idées.md\nrocket\tProjets\n");
        assert_eq!(load_icons(&root), icons);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn restores_from_the_trash() {
        let root = env::temp_dir().join(format!("bref-restore-{}", std::process::id()));
        fs::create_dir_all(root.join("Dossier")).unwrap();
        assert!(trashed(&root).is_empty());
        for (name, text) in [("A.md", "premier"), ("Dossier/b.md", "dedans")] {
            fs::write(root.join(name), text).unwrap();
        }
        trash(&root, &root.join("A.md")).unwrap();
        trash(&root, &root.join("Dossier")).unwrap();
        // The name was taken in the meantime: the restored note gets another, nothing is overwritten.
        fs::write(root.join("A.md"), "second").unwrap();
        let mut names: Vec<String> = trashed(&root).iter().map(|p| stem(p)).collect();
        names.sort();
        assert_eq!(names, ["A", "Dossier"]);
        assert_eq!(restore(&root, &root.join(".trash/A.md")).unwrap(), root.join("A 2.md"));
        assert_eq!(restore(&root, &root.join(".trash/Dossier")).unwrap(), root.join("Dossier"));
        assert_eq!(fs::read_to_string(root.join("A.md")).unwrap(), "second");
        assert_eq!(fs::read_to_string(root.join("A 2.md")).unwrap(), "premier");
        assert_eq!(fs::read_to_string(root.join("Dossier/b.md")).unwrap(), "dedans");
        assert!(trashed(&root).is_empty());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn ignores_reads_and_hidden_files() {
        use notify::{EventKind, event::{AccessKind, CreateKind}};
        let root = Path::new("/v");
        let event = |kind, path: &str| Event::new(kind).add_path(root.join(path));
        let created = EventKind::Create(CreateKind::File);
        assert!(matters(root, &event(created, "a.md")));
        assert!(matters(root, &event(created, "dossier/a.md")));
        assert!(!matters(root, &event(created, ".trash/a.md")));

        assert!(!matters(root, &event(created, ".git/objects/ab")));
        assert!(!matters(root, &event(EventKind::Access(AccessKind::Read), "a.md")));
        // Outside the vault or without a path: when in doubt, reread.
        assert!(matters(root, &Event::new(created).add_path("/ailleurs/.x".into())));
        assert!(matters(root, &Event::new(created)));
    }

    /// A symbolic link that points back up to the vault, or a second link to an already
    /// walked folder, does not loop and duplicates no note.
    #[cfg(unix)]
    #[test]
    fn scan_survives_symlink_loops() {
        let root = env::temp_dir().join(format!("bref-loop-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("a/b")).unwrap();
        fs::write(root.join("a/b/Note.md"), "# Note\n").unwrap();
        std::os::unix::fs::symlink(&root, root.join("a/b/loop")).unwrap();
        std::os::unix::fs::symlink(root.join("a"), root.join("alias")).unwrap();
        let (notes, dirs, _) = scan(&root);
        assert_eq!(notes.len(), 1, "{:?}", notes.iter().map(|n| &n.path).collect::<Vec<_>>());
        assert!(dirs.len() <= 3, "{dirs:?}");
        // The fingerprint, too, is computed without looping and does not move.
        assert_eq!(fingerprint(&root), fingerprint(&root));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn finds_the_templates_folder() {
        let root = Path::new("/v");
        let dirs: Vec<PathBuf> = ["/v/Notes", "/v/Modèles", "/v/Notes/Templates"].into_iter().map(PathBuf::from).collect();
        assert_eq!(template_dir(root, &dirs), Some(PathBuf::from("/v/Modèles")));
        // Case and accents do not count; only a folder at the root is the templates folder.
        for name in ["templates", "TEMPLATES", "Modeles", "MODÈLES", "modèles"] {
            assert_eq!(template_dir(root, &[root.join(name)]), Some(root.join(name)), "{name}");
        }
        assert_eq!(template_dir(root, &[PathBuf::from("/v/Notes/Templates"), root.join("Template"), root.join("Modèle")]), None);
        assert_eq!(template_dir(root, &[]), None);
        // A template is a file under this folder, at any depth; the folder itself, a
        // file of the same name, a note beside it or elsewhere are not.
        assert!(in_templates(root, Path::new("/v/Templates/Réunion.md")));
        assert!(in_templates(root, Path::new("/v/modèles/Projets/Plan.md")));
        assert!(!in_templates(root, Path::new("/v/Templates")));
        assert!(!in_templates(root, Path::new("/v/Notes/Templates/A.md")));
        assert!(!in_templates(root, Path::new("/v/Réunion.md")));
        assert!(!in_templates(root, Path::new("/autre/Templates/A.md")));
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bref-import-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn imports_files_and_folders_without_overwriting() {
        let base = scratch("a");
        let (vault, outside) = (base.join("coffre"), base.join("dehors"));
        fs::create_dir_all(&vault).unwrap();
        fs::create_dir_all(outside.join("Photos/été")).unwrap();
        fs::write(outside.join("Photos/a.png"), "a").unwrap();
        fs::write(outside.join("Photos/été/b.jpg"), "bé").unwrap();
        fs::write(outside.join("Photos/.cache"), "caché").unwrap();
        fs::write(outside.join("rapport.pdf"), "pdf").unwrap();
        fs::write(outside.join("vide.txt"), "").unwrap();
        fs::write(outside.join("sans-extension"), "x").unwrap();
        fs::write(outside.join("a.tar.gz"), "z").unwrap();
        fs::create_dir(outside.join("Dossier vide")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.join("rapport.pdf"), outside.join("Photos/lien.pdf")).unwrap();
        let all: Vec<PathBuf> = ["rapport.pdf", "Photos", "vide.txt", "sans-extension", "a.tar.gz", "Dossier vide"].iter().map(|n| outside.join(n)).collect();
        let (done, problems) = import(&all, &vault);
        let names: Vec<_> = done.iter().map(|d| d.to.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["rapport.pdf", "Photos", "vide.txt", "sans-extension", "a.tar.gz", "Dossier vide"]);
        assert_eq!(done.iter().map(|d| (d.dir, d.files)).collect::<Vec<_>>(), [(false, 1), (true, 2), (false, 1), (false, 1), (false, 1), (true, 0)]);
        assert_eq!(fs::read_to_string(vault.join("Photos/été/b.jpg")).unwrap(), "bé");
        // The hidden one and the symbolic link stay out, and we say so; the sources have not moved.
        assert!(!vault.join("Photos/.cache").exists() && !vault.join("Photos/lien.pdf").exists());
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(outside.join("Photos/a.png").is_file());
        // The same again: each name takes a number, files already there keep their content.
        fs::write(vault.join("rapport.pdf"), "le mien").unwrap();
        let (again, _) = import(&all, &vault);
        let names: Vec<_> = again.iter().map(|d| d.to.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["rapport 2.pdf", "Photos 2", "vide 2.txt", "sans-extension 2", "a.tar 2.gz", "Dossier vide 2"]);
        assert_eq!(fs::read_to_string(vault.join("rapport.pdf")).unwrap(), "le mien");
        let (third, _) = import(&[outside.join("rapport.pdf")], &vault);
        assert_eq!(third[0].to, vault.join("rapport 3.pdf"));
        // No half copy is left behind.
        assert!(fs::read_dir(&vault).unwrap().flatten().all(|e| !e.file_name().to_string_lossy().ends_with(".bref-part")));
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn imports_refuse_what_cannot_be_copied() {
        let base = scratch("b");
        let vault = base.join("coffre");
        fs::create_dir_all(vault.join("Sous")).unwrap();
        fs::write(base.join("f.md"), "# f").unwrap();
        // A folder does not go into itself or into one of its subfolders; a missing source, a hidden name,
        // a missing destination folder: refused with the reason, the rest goes through.
        let (done, problems) = import(&[base.join("coffre"), vault.join("Sous"), base.join("absent.png"), base.join("f.md")], &vault.join("Sous"));
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].to, vault.join("Sous/f.md"));
        assert_eq!(problems.len(), 3, "{problems:?}");
        assert!(problems[0].contains("itself") && problems[1].contains("itself"));
        let hidden = base.join(".cache");
        fs::write(&hidden, "x").unwrap();
        assert!(import(&[hidden], &vault).0.is_empty());
        let (none, problems) = import(&[base.join("f.md")], &base.join("nulle/part"));
        assert!(none.is_empty() && problems.len() == 1, "{problems:?}");
        assert_eq!(import(&[], &vault), (Vec::new(), Vec::new()));
        // Attachments are recognised by their extension, ignoring case.
        for name in ["a.PDF", "b.Mp4", "c.docx"] {
            assert!(is_attachment(Path::new(name)) && is_file_shown(Path::new(name)), "{name}");
        }
        for name in ["a.md", "b", "c.exe", ".pdf"] {
            assert!(!is_attachment(Path::new(name)), "{name}");
        }
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn watcher_reports_a_new_note() {
        let root = env::temp_dir().join(format!("bref-watch-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let changed = Arc::new(Signal::default());
        let _watcher = watch(&root, changed.clone()).expect("the system must be able to watch a folder");
        // No simulated clock here: the system takes a few milliseconds (FSEvents, a bit more).
        fs::write(root.join("neuve.md"), "x").unwrap();
        let start = std::time::Instant::now();
        let mut seen = changed.take();
        while !seen && start.elapsed() < std::time::Duration::from_secs(10) {
            std::thread::sleep(std::time::Duration::from_millis(50));
            seen = changed.take();
        }
        assert!(seen, "no event in 10 s");
        fs::remove_dir_all(&root).unwrap();
    }

    /// A rescan reuses an unchanged note as is (no reparse), reads a changed one again, and a
    /// template keeps no tags, links or aliases either way.
    #[test]
    fn rescan_reuses_unchanged_notes() {
        let root = std::env::temp_dir().join(format!("bref-reuse-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("Templates")).unwrap();
        let text = "---\naliases: [Pseudo]\n---\n#etiquette [[Cible]]\n";
        fs::write(root.join("A.md"), text).unwrap();
        fs::write(root.join("Templates/T.md"), text).unwrap();
        let (first, ..) = rescan_full(&root, &[]);
        let by = |notes: &[Note], name: &str| notes.iter().position(|n| n.name == name).unwrap();
        assert_eq!(first[by(&first, "A")].aliases[..], ["Pseudo".to_string()]);
        assert!(first[by(&first, "T")].aliases.is_empty());
        let (again, ..) = rescan_full(&root, &first);
        let (a, b) = (&first[by(&first, "A")], &again[by(&again, "A")]);
        assert!(Arc::ptr_eq(&a.body, &b.body) && Arc::ptr_eq(&a.links, &b.links) && Arc::ptr_eq(&a.aliases, &b.aliases));
        assert!(again[by(&again, "T")].tags.is_empty() && again[by(&again, "T")].aliases.is_empty());
        // A note whose mtime moved is read again.
        let known: Vec<Note> = first.iter().cloned().map(|mut n| { n.mtime = UNIX_EPOCH; n }).collect();
        let (reread, ..) = rescan_full(&root, &known);
        assert!(!Arc::ptr_eq(&a.body, &reread[by(&reread, "A")].body));
        let _ = fs::remove_dir_all(&root);
    }

    /// A folder that cannot be listed is reported, root or inner, with the reason; the rest is read as
    /// usual, and the same folder read again once the permission is back.
    #[cfg(unix)]
    #[test]
    fn reports_unreadable_folders() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("bref-unreadable-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let deep = root.join("a/b/c/d");
        fs::create_dir_all(&deep).unwrap();
        fs::create_dir_all(root.join("Fermé/Dedans")).unwrap();
        fs::write(root.join("Note.md"), "# Note").unwrap();
        fs::write(deep.join("Profonde.md"), "x").unwrap();
        fs::write(root.join("Fermé/Cachée.md"), "x").unwrap();
        let mode = |path: &Path, m: u32| fs::set_permissions(path, fs::Permissions::from_mode(m)).unwrap();
        // Root (`chmod 0`) can read anything: nothing to see there. The folder inside first: closed, the
        // outer one hides it.
        mode(&root.join("Fermé/Dedans"), 0o000);
        mode(&root.join("Fermé"), 0o000);
        if fs::read_dir(root.join("Fermé")).is_ok() {
            return eprintln!("permissions are not enforced here (root?): skipped");
        }
        // An inner folder at depth 1, one inside it, one at depth 4 (write and cross, no read: 311).
        mode(&deep, 0o311);
        let (notes, dirs, _, unreadable) = rescan_full(&root, &[]);
        let names: Vec<_> = notes.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, ["Note"]);
        assert!(dirs.contains(&root.join("Fermé")) && dirs.contains(&deep), "an unreadable folder is still a folder of the vault");
        let mut got: Vec<_> = unreadable.iter().map(|(p, kind)| (p.strip_prefix(&root).unwrap().to_path_buf(), *kind)).collect();
        got.sort();
        // The folder inside the closed one is not reached: only the closed one is said.
        assert_eq!(got, [(PathBuf::from("Fermé"), io::ErrorKind::PermissionDenied), (PathBuf::from("a/b/c/d"), io::ErrorKind::PermissionDenied)]);
        // The root: nothing is read, and it says so; given back, everything is there.
        mode(&root, 0o000);
        let (notes, _, _, unreadable) = rescan_full(&root, &[]);
        assert!(notes.is_empty() && unreadable == [(root.clone(), io::ErrorKind::PermissionDenied)]);
        for dir in [&root, &root.join("Fermé"), &root.join("Fermé/Dedans"), &deep] {
            mode(dir, 0o755);
        }
        let (notes, _, _, unreadable) = rescan_full(&root, &[]);
        assert!(unreadable.is_empty() && notes.len() == 3, "{:?}", notes.iter().map(|n| &n.name).collect::<Vec<_>>());
        // A readable but empty vault is empty, not unreadable.
        let empty = root.join("vide");
        fs::create_dir(&empty).unwrap();
        assert!(rescan_full(&empty, &[]).3.is_empty());
        fs::remove_dir_all(&root).unwrap();
    }
}
