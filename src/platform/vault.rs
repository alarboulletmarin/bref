//! Vault: a folder of `.md` files, plus the app's small config.

use std::{
    collections::{HashMap, HashSet},
    env, fs,
    hash::{DefaultHasher, Hash, Hasher},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};

use crate::{logic::markdown, lang::tr};
use crate::logic::note::{Note, lowered};

/// The user's configuration folder, per platform.
pub fn config_dir() -> PathBuf {
    let var = |key: &str| env::var_os(key).map(PathBuf::from);
    let dir = if cfg!(windows) {
        var("APPDATA")
    } else if cfg!(target_os = "macos") {
        var("HOME").map(|home| home.join("Library/Application Support"))
    } else {
        var("XDG_CONFIG_HOME").or_else(|| var("HOME").map(|home| home.join(".config")))
    };
    dir.unwrap_or_default()
}

fn config_path() -> PathBuf {
    config_dir().join("bref/config")
}

/// (vault, open notes from most recent to oldest): one line each.
pub fn load_config() -> (Option<PathBuf>, Vec<PathBuf>) {
    // Falls back to the config of the app's old name; it is never rewritten.
    let text = fs::read_to_string(config_path())
        .or_else(|_| fs::read_to_string(config_dir().join("encre/config")))
        .unwrap_or_default();
    let mut lines = text.lines().filter(|l| !l.is_empty()).map(PathBuf::from);
    (lines.next(), lines.collect())
}

pub fn save_config(vault: &Path, recent: &[PathBuf]) {
    let path = config_path();
    let text: String = std::iter::once(vault)
        .chain(recent.iter().map(PathBuf::as_path))
        .map(|p| format!("{}\n", p.display()))
        .collect();
    let result = path
        .parent()
        .map_or(Ok(()), fs::create_dir_all)
        .and_then(|_| fs::write(&path, text));
    if let Err(e) = result {
        eprintln!("bref: {} ({}): {e}", tr("config not saved", "config non enregistrée"), path.display());
    }
}

pub fn load_file(name: &str) -> String {
    // Same fallback as for the config: settings left under the old name are read again.
    fs::read_to_string(config_dir().join("bref").join(name))
        .or_else(|_| fs::read_to_string(config_dir().join("encre").join(name)))
        .unwrap_or_default()
}

pub fn save_file(name: &str, text: &str) {
    let path = config_dir().join("bref").join(name);
    // Mere convenience: if writing fails, the app will reopen with its original settings.
    let _ = path.parent().map_or(Ok(()), fs::create_dir_all).and_then(|_| fs::write(path, text));
}

/// Layout of the navigation panel, kept as is on one line.
pub fn load_layout() -> String {
    load_file("layout")
}

pub fn save_layout(text: &str) {
    save_file("layout", text)
}

/// Chosen appearance (theme, fonts, size): one `key=value` line per setting.
pub fn load_settings() -> String {
    load_file("settings")
}

pub fn save_settings(text: &str) {
    save_file("settings", text)
}

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

/// Piece of a name for natural sort: a number compares by its value, before text.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub enum Chunk {
    Number(u128),
    Text(String),
}

/// "Natural" sort key, like file managers use: `Note 2` before
/// `Note 10`, ignoring case.
pub fn natural(name: &str) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let mut rest = name;
    while let Some(first) = rest.chars().next() {
        let digit = first.is_ascii_digit();
        let end = rest.find(|c: char| c.is_ascii_digit() != digit).unwrap_or(rest.len());
        let (part, tail) = rest.split_at(end);
        chunks.push(match part.parse() {
            Ok(n) if digit => Chunk::Number(n),
            _ => Chunk::Text(part.to_lowercase()),
        });
        rest = tail;
    }
    chunks
}

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

/// The file is an image the app can display.
pub fn is_image(path: &Path) -> bool {
    let known = ["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "tif", "tiff", "avif"];
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| known.iter().any(|k| e.eq_ignore_ascii_case(k)))
}

/// Files kept in the vault without being read: they are listed in the tree, found in
/// the palette, targeted by `[[name.pdf]]` and opened in the system's application.
const ATTACHMENTS: [&str; 34] = [
    "pdf", "mp4", "m4v", "mov", "mkv", "webm", "avi", "mp3", "m4a", "wav", "ogg", "opus", "flac", "zip", "7z", "tar", "gz", "rar",
    "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "odp", "epub", "txt", "json", "xml", "html", "rtf", "log",
];

pub fn is_attachment(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| ATTACHMENTS.iter().any(|k| e.eq_ignore_ascii_case(k)))
}

/// The file is an image, a table or an attachment: what the tree shows besides notes.
pub fn is_file_shown(path: &Path) -> bool {
    is_image(path) || is_table(path) || is_attachment(path)
}

/// The file is a table (CSV or TSV) the app opens in a grid.
pub fn is_table(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| ["csv", "tsv"].iter().any(|k| e.eq_ignore_ascii_case(k)))
}

/// The images among the vault's files (the graph and `![[…]]` links ignore tables).
pub fn pictures(files: &[PathBuf]) -> Vec<PathBuf> {
    files.iter().filter(|p| is_image(p)).cloned().collect()
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

/// Fingerprint of the vault's state: the paths of its notes, images and folders,
/// and the date of each note. It changes as soon as another program touches it, without
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
            } else if is_image(&path) {
                seen.push((path, None));
            } else if is_table(&path) {
                // Another program rewriting the open table must be seen.
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

/// A signal that wakes the task waiting on it, without it having to poll: the system's
/// thread raises it, the task sleeps until then.
#[derive(Default)]
pub struct Signal {
    raised: AtomicBool,
    waker: std::sync::Mutex<Option<std::task::Waker>>,
}

impl Signal {
    pub fn raise(&self) {
        self.raised.store(true, Ordering::Release);
        if let Some(waker) = self.waker.lock().unwrap().take() {
            waker.wake();
        }
    }

    /// True, and the signal drops, if it was raised since the last call.
    pub fn take(&self) -> bool {
        self.raised.swap(false, Ordering::AcqRel)
    }

    /// Waits until the signal is raised (true) or `deadline` passes (false).
    pub async fn wait<D: std::future::Future<Output = ()> + Unpin>(&self, mut deadline: D) -> bool {
        std::future::poll_fn(|cx| {
            if self.take() {
                return std::task::Poll::Ready(true);
            }
            *self.waker.lock().unwrap() = Some(cx.waker().clone());
            // Raised in the meantime, before the waker is in place: it is not missed.
            if self.take() {
                return std::task::Poll::Ready(true);
            }
            std::pin::Pin::new(&mut deadline).poll(cx).map(|()| false)
        })
        .await
    }
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
pub fn scan(root: &Path) -> (Vec<Note>, Vec<PathBuf>, Vec<PathBuf>) {
    rescan(root, &[])
}

/// Like `scan`, without rereading the notes of `known` whose date has not changed.
pub fn rescan(root: &Path, known: &[Note]) -> (Vec<Note>, Vec<PathBuf>, Vec<PathBuf>) {
    let known: HashMap<&Path, &Note> = known.iter().map(|n| (n.path.as_path(), n)).collect();
    let mut files = Vec::new();
    let mut images = Vec::new();
    let mut found = Vec::new();
    let mut visited = HashSet::new();
    first_visit(&mut visited, root);
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
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
        let (tags, links, aliases, body, lower) = match known.get(path.as_path()) {
            Some(note) if note.mtime == *mtime => (note.tags.clone(), note.links.clone(), note.aliases.clone(), note.body.clone(), note.lower.clone()),
            _ => {
                let text = fs::read_to_string(path).unwrap_or_default();
                let (tags, links) = markdown::index(&text);
                let body = Arc::from(text);
                let lower = lowered(&body);
                (tags.into(), links.into(), markdown::aliases(&body).into(), body, lower)
            }
        };
        // A template has no tags, links or aliases: what it writes between `{{ }}` or after `#` is not its own.
        let (tags, links, aliases) =
            if in_templates(root, path) { (Vec::new().into(), Vec::new().into(), Vec::new().into()) } else { (tags, links, aliases) };
        Note { name: stem(path), tags, links, aliases, mtime: *mtime, path: path.clone(), body, lower }
    };
    let mut notes: Vec<Note> = files.iter().map(note).collect();
    notes.sort_by_key(|note| std::cmp::Reverse(note.mtime));
    (notes, found, images)
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

/// Moves or renames, never overwriting what already has that name.
pub fn rename(from: &Path, to: &Path) -> io::Result<()> {
    if to.exists() {
        let taken = tr("this name is already taken", "ce nom est déjà pris");
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, taken));
    }
    fs::rename(from, to)
}

/// Renames a note and returns its path. If its name followed its title, the
/// first line is rewritten so they stay in step.
pub fn rename_note(path: &Path, name: &str) -> io::Result<PathBuf> {
    let content = fs::read_to_string(path)?;
    let to = path.with_file_name(format!("{name}.md"));
    rename(path, &to)?;
    // The front matter stays as is: the title is the first line after it.
    let (meta, body) = content.split_at(markdown::front_matter(&content));
    if stem(path) == title_of(&content)
        && let Some(first) = body.lines().find(|l| !l.trim().is_empty())
    {
        let hashes = first.trim_start().chars().take_while(|c| *c == '#').count();
        let title = if hashes > 0 { format!("{} {name}", "#".repeat(hashes)) } else { name.to_string() };
        fs::write(&to, format!("{meta}{}", body.replacen(first, &title, 1)))?;
    }
    Ok(to)
}

/// Moves the note or folder to the vault's trash: the hidden folder
/// `.trash`, which the index ignores. Nothing is destroyed.
pub fn trash(root: &Path, path: &Path) -> io::Result<PathBuf> {
    let bin = root.join(".trash");
    fs::create_dir_all(&bin)?;
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let free = (1..)
        .map(|n| match n {
            1 => bin.join(&*name),
            n => bin.join(format!("{n} {name}")),
        })
        .find(|p| !p.exists())
        .unwrap();
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

/// The commands to try, in order, to open a terminal in `dir` under the system
/// `os` (`std::env::consts::OS`): the program and its arguments. `preferred` is the terminal
/// the user names in `$TERMINAL`.
pub fn terminals(os: &str, dir: &str, preferred: Option<&str>) -> Vec<(String, Vec<String>)> {
    let command = |program: &str, args: &[&str]| (program.to_string(), args.iter().map(|a| a.to_string()).collect());
    match os {
        "macos" => vec![command("open", &["-a", "Terminal", dir])],
        // Windows Terminal if installed, otherwise the classic console.
        "windows" => vec![command("wt", &["-d", dir]), command("cmd", &["/c", "start", "cmd"])],
        // Linux has no single answer. Those that merely notify an already running instance
        // ignore the current folder: it is given to them as an argument.
        _ => {
            let mut all: Vec<_> = preferred.iter().map(|p| command(p, &[])).collect();
            all.extend([
                command("xdg-terminal-exec", &[]),
                command("x-terminal-emulator", &[]),
                command("gnome-terminal", &["--working-directory", dir]),
                command("ptyxis", &["--new-window", "-d", dir]),
                command("kgx", &["--working-directory", dir]),
                command("konsole", &["--workdir", dir]),
                command("kitty", &[]),
                command("alacritty", &[]),
                command("foot", &[]),
                command("wezterm", &["start", "--cwd", dir]),
                command("xterm", &[]),
            ]);
            all
        }
    }
}

/// Opens the system terminal in `dir`: the first of `terminals` that starts. False if
/// none is installed. Tests open nothing.
pub fn open_terminal(dir: &Path) -> bool {
    let preferred = env::var("TERMINAL").ok().filter(|t| !t.trim().is_empty());
    let found = terminals(env::consts::OS, &dir.to_string_lossy(), preferred.as_deref());
    !cfg!(test)
        && found.into_iter().any(|(program, args)| {
            // No console for the launcher (`cmd /c start`); the terminal opens its own.
            let child = crate::platform::command(&program).args(args).current_dir(dir).stdin(std::process::Stdio::null()).spawn();
            // Once the terminal is closed, its process is reaped: no zombie while Bref runs.
            child.map(|mut child| std::thread::spawn(move || child.wait())).is_ok()
        })
}

/// Keys offered for capture; free on a stock GNOME.
const CAPTURE_KEYS: &str = "<Super><Shift>n";
const GNOME_KEYS: &str = "org.gnome.settings-daemon.plugins.media-keys";
/// A path of our own in GNOME's custom shortcuts: asking again doubles nothing.
const GNOME_PATH: &str = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/bref-capture/";

/// What asking for a capture shortcut gave.
#[derive(Debug, PartialEq)]
pub enum Shortcut {
    /// Set in the desktop settings: the keys, as read.
    Bound(String),
    /// Set without keys, the offered ones being taken: here they are, to choose in the settings.
    Unbound(String),
    /// This desktop cannot be set from here: the command to bind yourself.
    Manual(String),
}

/// Binds `exe --capture` to a desktop shortcut, never taking keys already
/// in use. Under GNOME it is done from here; elsewhere the command is returned, to bind by hand.
/// Blocking (a few `gsettings` calls): run off the UI thread. Tests
/// touch no setting.
pub fn capture_shortcut(exe: &Path) -> Shortcut {
    let exe = exe.to_string_lossy();
    let command = if exe.contains(' ') { format!("'{exe}' --capture") } else { format!("{exe} --capture") };
    let gnome = !cfg!(test) && env::var("XDG_CURRENT_DESKTOP").is_ok_and(|desktop| desktop.contains("GNOME"));
    gnome.then(|| gnome_shortcut(&command)).flatten().unwrap_or(Shortcut::Manual(command))
}

/// What `gsettings` answers, if it is there and accepts.
fn gsettings(args: &[&str]) -> Option<String> {
    let out = crate::platform::command("gsettings").args(args).stdin(std::process::Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// GNOME's custom shortcut "Bref: capture": created with `CAPTURE_KEYS` if nothing
/// uses them, otherwise without keys. If it already exists, only its command is updated (the
/// binary may have moved): the keys the user gave it stay.
fn gnome_shortcut(command: &str) -> Option<Shortcut> {
    let entry = format!("{GNOME_KEYS}.custom-keybinding:{GNOME_PATH}");
    let list = gsettings(&["get", GNOME_KEYS, "custom-keybindings"])?;
    let quoted = |text: &str| format!("'{}'", text.replace('\'', "\\'"));
    let Some(longer) = with_keybinding(&list, GNOME_PATH) else {
        gsettings(&["set", &entry, "command", &quoted(command)])?;
        let keys = gsettings(&["get", &entry, "binding"])?;
        let keys = keys.trim_matches('\'');
        return Some(if keys.is_empty() { Shortcut::Unbound(spell_keys(CAPTURE_KEYS)) } else { Shortcut::Bound(spell_keys(keys)) });
    };
    // Taken: by a system shortcut, or by another custom shortcut.
    let wanted = format!("'{}'", CAPTURE_KEYS.to_lowercase());
    let others = list.split('\'').skip(1).step_by(2).filter_map(|path| gsettings(&["get", &format!("{GNOME_KEYS}.custom-keybinding:{path}"), "binding"]));
    let taken = gsettings(&["list-recursively"]).is_some_and(|all| all.to_lowercase().contains(&wanted))
        || others.into_iter().any(|keys| keys.to_lowercase() == wanted);
    gsettings(&["set", &entry, "name", &quoted(tr("Bref: capture", "Bref : capture"))])?;
    gsettings(&["set", &entry, "command", &quoted(command)])?;
    if !taken {
        gsettings(&["set", &entry, "binding", &quoted(CAPTURE_KEYS)])?;
    }
    gsettings(&["set", GNOME_KEYS, "custom-keybindings", &longer])?;
    Some(if taken { Shortcut::Unbound(spell_keys(CAPTURE_KEYS)) } else { Shortcut::Bound(spell_keys(CAPTURE_KEYS)) })
}

/// The `['a', 'b']` list from `gsettings` (or `@as []`, empty) with `path` added; `None` if it
/// is already there.
fn with_keybinding(list: &str, path: &str) -> Option<String> {
    let mut paths: Vec<&str> = list.split('\'').skip(1).step_by(2).collect();
    if paths.contains(&path) {
        return None;
    }
    paths.push(path);
    Some(format!("[{}]", paths.iter().map(|p| format!("'{p}'")).collect::<Vec<_>>().join(", ")))
}

/// `<Super><Shift>n`, as GNOME writes it, as read: `Super+Shift+N`.
fn spell_keys(keys: &str) -> String {
    let parts = keys.split(['<', '>']).filter(|part| !part.is_empty()).map(|part| match part {
        "Shift" => tr("Shift", "Maj").to_string(),
        "Primary" | "Control" => "Ctrl".to_string(),
        _ => {
            let mut chars = part.chars();
            chars.next().map(|first| first.to_uppercase().chain(chars).collect()).unwrap_or_default()
        }
    });
    parts.collect::<Vec<_>>().join("+")
}

/// Backup: the whole vault (trash included) in a `<vault> <date>.tar.gz` archive
/// in folder `dir`, numbered if the name is taken. Blocking: run off the UI thread.
// ponytail: the system's `tar` (shipped with Linux, macOS and Windows 10) rather than an
// archive library as a dependency; write a `.zip` ourselves if `.tar.gz` is a problem
// on Windows.
pub fn backup(root: &Path, dir: &Path, date: &str) -> io::Result<PathBuf> {
    let (Some(parent), Some(name)) = (root.parent(), root.file_name()) else {
        return Err(io::Error::other(tr("this vault has no parent folder", "ce coffre n'a pas de dossier parent")));
    };
    // Inside the vault, the archive would contain itself.
    if dir.starts_with(root) {
        return Err(io::Error::other(tr("choose a folder outside the vault", "choisir un dossier hors du coffre")));
    }
    let to = free_path(dir, &format!("{} {date}", name.to_string_lossy()), "tar.gz");
    // The archive is named from `dir`: Git for Windows' `tar` would take `C:\…` for
    // a remote machine.
    let status = crate::platform::command("tar")
        .current_dir(dir)
        .arg("-czf")
        .arg(to.file_name().unwrap_or_default())
        .arg("-C")
        .arg(parent)
        .arg(name)
        .stdin(std::process::Stdio::null())
        .output();
    match status {
        Ok(out) if out.status.success() => Ok(to),
        failed => {
            // An incomplete archive must not pass for a backup.
            let _ = fs::remove_file(&to);
            // What `tar` said, its first line: the reason is the user's to read.
            let said = failed.as_ref().ok().and_then(|out| String::from_utf8_lossy(&out.stderr).lines().next().map(str::to_string)).filter(|l| !l.is_empty());
            Err(match (failed, said) {
                (Err(e), _) => e,
                (Ok(_), Some(line)) => io::Error::other(format!("{} ({line})", tr("tar failed", "tar a échoué"))),
                (Ok(out), None) => io::Error::other(format!("{} (code {})", tr("tar failed", "tar a échoué"), out.status.code().map_or("?".into(), |c| c.to_string()))),
            })
        }
    }
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
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let extension = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let free = (1..)
        .map(|n| match n {
            1 => root.join(format!("{stem}{extension}")),
            n => root.join(format!("{stem} {n}{extension}")),
        })
        .find(|p| !p.exists())
        .unwrap();
    fs::rename(path, &free)?;
    Ok(free)
}

/// Free path in `dir` for a file `name.extension`: the name is followed
/// by a number if already taken.
pub fn free_path(dir: &Path, name: &str, extension: &str) -> PathBuf {
    let numbered = (1..).map(|n| match n {
        1 => dir.join(format!("{name}.{extension}")),
        n => dir.join(format!("{name} {n}.{extension}")),
    });
    numbered.into_iter().find(|p| !p.exists()).unwrap()
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

/// A free name in `dir`: `name.ext`, then `name 2.ext`, `name 3.ext`… Nothing is ever overwritten.
fn free_name(dir: &Path, name: &std::ffi::OsStr, folder: bool) -> PathBuf {
    let as_path = Path::new(name);
    let (stem, ext) = match (folder, as_path.extension()) {
        (false, Some(ext)) => (as_path.file_stem().unwrap_or(name).to_string_lossy().into_owned(), Some(ext.to_string_lossy().into_owned())),
        _ => (name.to_string_lossy().into_owned(), None),
    };
    (1..)
        .map(|n| {
            let numbered = if n == 1 { stem.clone() } else { format!("{stem} {n}") };
            dir.join(match &ext {
                Some(ext) => format!("{numbered}.{ext}"),
                None => numbered,
            })
        })
        .find(|path| !path.exists() && path.symlink_metadata().is_err())
        .unwrap()
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
    let name = (1..)
        .map(|n| match n {
            1 => format!("image-{stamp}.{extension}"),
            n => format!("image-{stamp}-{n}.{extension}"),
        })
        .find(|name| !dir.join(name).exists())
        .unwrap();
    fs::write(dir.join(&name), bytes)?;
    Ok(name)
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
        Err(e) if e.kind() == io::ErrorKind::NotFound => format!("# {name}\n\n"),
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
pub fn write(path: &Path, content: &str) -> io::Result<()> {
    let tmp = path.with_file_name(format!(".{}.tmp", stem(path)));
    let mut file = fs::File::create(&tmp)?;
    file.write_all(content.as_bytes())?;
    // Without `fsync`, a power cut can leave the rename without the data.
    file.sync_all()?;
    fs::rename(&tmp, path)
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
        Some(p) if !synced || stem(p) == title => p.to_path_buf(),
        Some(p) => {
            let renamed = p.with_file_name(format!("{title}.md"));
            if renamed.exists() { p.to_path_buf() } else { renamed }
        }
        None => (1..)
            .map(|n| match n {
                1 => dir.join(format!("{title}.md")),
                n => dir.join(format!("{title} {n}.md")),
            })
            .find(|p| !p.exists())
            .unwrap(),
    };
    write(&target, content)?;
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
    fn sorts_names_naturally() {
        let mut names = vec!["Note 10", "note 2", "Note 1", "Notes", "2026-10-09", "2026-9-30", "Note 1b"];
        names.sort_by_cached_key(|n| natural(n));
        assert_eq!(names, ["2026-9-30", "2026-10-09", "Note 1", "Note 1b", "note 2", "Note 10", "Notes"]);
    }

    #[test]
    fn gnome_shortcut_list_gains_the_capture_once() {
        let path = "/org/gnome/x/bref-capture/";
        assert_eq!(with_keybinding("@as []", path).as_deref(), Some("['/org/gnome/x/bref-capture/']"));
        assert_eq!(with_keybinding("['/org/gnome/x/custom0/', '/org/gnome/x/custom1/']", path).as_deref(), Some("['/org/gnome/x/custom0/', '/org/gnome/x/custom1/', '/org/gnome/x/bref-capture/']"));
        // Already there: nothing to add, the request can repeat without doubling anything.
        assert_eq!(with_keybinding("['/org/gnome/x/custom0/', '/org/gnome/x/bref-capture/']", path), None);
    }

    #[test]
    fn gnome_keys_are_spelled_like_the_others() {
        assert_eq!(spell_keys("<Super><Shift>n"), "Super+Maj+N");
        assert_eq!(spell_keys("<Primary><Alt>space"), "Ctrl+Alt+Space");
        assert_eq!(spell_keys("F9"), "F9");
    }

    /// The real `gsettings`, on throwaway settings (key-value file in a temporary
    /// folder): `cargo test -- --ignored gnome_shortcut_is_registered`, on a GNOME machine.
    #[test]
    #[ignore]
    fn gnome_shortcut_is_registered() {
        let dir = std::env::temp_dir().join(format!("bref-gsettings-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        unsafe {
            std::env::set_var("GSETTINGS_BACKEND", "keyfile");
            std::env::set_var("XDG_CONFIG_HOME", &dir);
        }
        assert_eq!(gnome_shortcut("/usr/bin/bref --capture"), Some(Shortcut::Bound("Super+Maj+N".into())));
        let saved = fs::read_to_string(dir.join("glib-2.0/settings/keyfile")).unwrap();
        assert!(saved.contains("command='/usr/bin/bref --capture'") && saved.contains("binding='<Super><Shift>n'"), "{saved}");
        // Asked again after the binary moved: the command follows, the chosen key stays.
        gsettings(&["set", &format!("{GNOME_KEYS}.custom-keybinding:{GNOME_PATH}"), "binding", "<Super>F9"]).unwrap();
        assert_eq!(gnome_shortcut("/opt/bref --capture"), Some(Shortcut::Bound("Super+F9".into())));
        let saved = fs::read_to_string(dir.join("glib-2.0/settings/keyfile")).unwrap();
        assert!(saved.contains("command='/opt/bref --capture'") && saved.matches("bref-capture").count() == 2, "{saved}");
        let _ = fs::remove_dir_all(&dir);
    }

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

    /// Runs a future on the current thread, which sleeps between wakeups.
    fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
        struct Unpark(std::thread::Thread);
        impl std::task::Wake for Unpark {
            fn wake(self: Arc<Self>) {
                self.0.unpark();
            }
        }
        let waker = Arc::new(Unpark(std::thread::current())).into();
        let mut cx = std::task::Context::from_waker(&waker);
        let mut future = std::pin::pin!(future);
        loop {
            if let std::task::Poll::Ready(value) = future.as_mut().poll(&mut cx) {
                return value;
            }
            std::thread::park_timeout(std::time::Duration::from_secs(5));
        }
    }

    #[test]
    fn signal_wakes_a_sleeping_task() {
        let signal = Arc::new(Signal::default());
        // Raised before the wait: no sleep. Dropped afterwards.
        signal.raise();
        assert!(block_on(signal.wait(std::future::pending())));
        assert!(!signal.take());
        // Deadline passed, no signal: false.
        assert!(!block_on(signal.wait(std::future::ready(()))));
        // Raised by another thread while the task sleeps.
        let other = signal.clone();
        let thread = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(100));
            other.raise();
        });
        let start = std::time::Instant::now();
        assert!(block_on(signal.wait(std::future::pending())));
        assert!(start.elapsed() < std::time::Duration::from_secs(4));
        thread.join().unwrap();
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

        // Renaming a note in step with its title rewrites that title; otherwise the text stays as is.
        let c = rename_note(&root.join("A 2.md"), "C").unwrap();
        assert_eq!(fs::read_to_string(&c).unwrap(), "# A\n");
        fs::write(root.join("D.md"), "\n## D\ntexte D\n").unwrap();
        let e = rename_note(&root.join("D.md"), "E").unwrap();
        assert_eq!(fs::read_to_string(&e).unwrap(), "\n## E\ntexte D\n");
        // Never overwrite.
        assert!(rename_note(&e, "C").is_err() && e.exists());

        // Trash: nothing is destroyed, and the index does not see it.
        fs::create_dir(root.join("vide")).unwrap();
        trash(&root, &c).unwrap();
        fs::write(&c, "autre").unwrap();
        trash(&root, &c).unwrap();
        assert_eq!(fs::read_to_string(root.join(".trash/C.md")).unwrap(), "# A\n");
        assert_eq!(fs::read_to_string(root.join(".trash/2 C.md")).unwrap(), "autre");
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
    fn picks_the_terminal_of_the_system() {
        let programs = |os, preferred| terminals(os, "/coffre", preferred).into_iter().map(|(p, _)| p).collect::<Vec<_>>();
        assert_eq!(terminals("macos", "/coffre", None), [("open".to_string(), vec!["-a".to_string(), "Terminal".into(), "/coffre".into()])]);
        assert_eq!(programs("windows", Some("kitty")), ["wt", "cmd"]);
        // `$TERMINAL` comes first; without it, the desktop's choice first.
        assert_eq!(programs("linux", Some("foot"))[..2], ["foot", "xdg-terminal-exec"]);
        assert_eq!(programs("linux", None)[0], "xdg-terminal-exec");
        let gnome = terminals("linux", "/coffre", None).into_iter().find(|(p, _)| p == "gnome-terminal").unwrap();
        assert_eq!(gnome.1, ["--working-directory", "/coffre"]);
    }

    #[test]
    fn backs_up_the_vault() {
        let base = env::temp_dir().join(format!("bref-backup-{}", std::process::id()));
        let (root, out) = (base.join("Mon coffre"), base.join("sauvegardes"));
        fs::create_dir_all(root.join(".trash")).unwrap();
        fs::create_dir_all(&out).unwrap();
        fs::write(root.join("A.md"), "# A\n").unwrap();
        fs::write(root.join(".trash/Vieux.md"), "jeté").unwrap();
        let first = backup(&root, &out, "2026-10-09").unwrap();
        assert_eq!(first, out.join("Mon coffre 2026-10-09.tar.gz"));
        // Same day: a second archive, the first one stays.
        assert_eq!(backup(&root, &out, "2026-10-09").unwrap(), out.join("Mon coffre 2026-10-09 2.tar.gz"));
        let listed = crate::platform::command("tar").current_dir(&out).arg("-tzf").arg(first.file_name().unwrap()).output().unwrap();
        let listed = String::from_utf8_lossy(&listed.stdout).replace('\\', "/");
        assert!(listed.contains("Mon coffre/A.md") && listed.contains("Mon coffre/.trash/Vieux.md"), "{listed}");
        // Inside the vault, the archive would contain itself: refused, nothing is written.
        assert!(backup(&root, &root.join(".trash"), "2026-10-09").is_err());
        assert_eq!(fs::read_dir(root.join(".trash")).unwrap().count(), 1);
        fs::remove_dir_all(&base).unwrap();
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
        while !changed.raised.load(Ordering::Acquire) && start.elapsed() < std::time::Duration::from_secs(10) {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(changed.take(), "no event in 10 s");
        fs::remove_dir_all(&root).unwrap();
    }
}
