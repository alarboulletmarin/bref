//! Coffre : un dossier de fichiers `.md`, plus la petite config de l'app.

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

use crate::{markdown, tr};

#[derive(Clone)]
pub struct Note {
    pub name: String,
    pub path: PathBuf,
    pub tags: Vec<String>,
    /// Notes visées par ses `[[wikiliens]]`, en minuscules.
    pub links: Vec<String>,
    /// Ses autres noms (clé `aliases` de l'en-tête YAML), tels qu'écrits.
    pub aliases: Vec<String>,
    pub mtime: SystemTime,
    /// Texte entier de la note, pour la recherche plein texte.
    // ponytail: tout le texte du coffre reste en mémoire (5 000 notes de 6 Ko : 30 Mo) ;
    // passer à un index sur disque si des coffres bien plus gros se présentent.
    pub body: Arc<str>,
}

impl Note {
    /// Un lien écrit `key` (en minuscules) vise cette note : par son nom, ou par un alias.
    pub fn answers(&self, key: &str) -> bool {
        self.name.to_lowercase() == key || self.aliases.iter().any(|alias| alias.to_lowercase() == key)
    }
}

/// Dossier de configuration de l'utilisateur, selon la plateforme.
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

/// (coffre, notes ouvertes de la plus récente à la plus ancienne) : une ligne chacun.
pub fn load_config() -> (Option<PathBuf>, Vec<PathBuf>) {
    // Repli sur la config des anciens noms de l'app ; elle n'est jamais réécrite.
    let text = fs::read_to_string(config_path())
        .or_else(|_| fs::read_to_string(config_dir().join("encre/config")))
        .or_else(|_| fs::read_to_string(config_dir().join("onenote/config")))
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
    // Même repli que pour la config : les réglages laissés sous l'ancien nom sont relus.
    fs::read_to_string(config_dir().join("bref").join(name))
        .or_else(|_| fs::read_to_string(config_dir().join("encre").join(name)))
        .unwrap_or_default()
}

pub fn save_file(name: &str, text: &str) {
    let path = config_dir().join("bref").join(name);
    // Simple confort : si l'écriture échoue, l'app rouvrira avec ses réglages d'origine.
    let _ = path.parent().map_or(Ok(()), fs::create_dir_all).and_then(|_| fs::write(path, text));
}

/// Disposition du panneau de navigation, mémorisée telle quelle sur une ligne.
pub fn load_layout() -> String {
    load_file("layout")
}

pub fn save_layout(text: &str) {
    save_file("layout", text)
}

/// Apparence choisie (thème, polices, taille) : une ligne `clé=valeur` par réglage.
pub fn load_settings() -> String {
    load_file("settings")
}

pub fn save_settings(text: &str) {
    save_file("settings", text)
}

pub fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Le fichier est une image que l'app sait afficher.
pub fn is_image(path: &Path) -> bool {
    let known = ["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "tif", "tiff", "avif"];
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| known.iter().any(|k| e.eq_ignore_ascii_case(k)))
}

/// Le fichier est un tableau (CSV ou TSV) que l'app ouvre dans une grille.
pub fn is_table(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| ["csv", "tsv"].iter().any(|k| e.eq_ignore_ascii_case(k)))
}

/// Les images parmi les fichiers du coffre (le graphe et les liens `![[…]]` ignorent les tableaux).
pub fn pictures(files: &[PathBuf]) -> Vec<PathBuf> {
    files.iter().filter(|p| is_image(p)).cloned().collect()
}

/// Vrai la première fois qu'on entre dans ce dossier. Un lien symbolique qui remonte vers un
/// dossier parent (ou deux liens vers le même) ne doit ni boucler ni dupliquer les notes.
fn first_visit(visited: &mut HashSet<PathBuf>, dir: &Path) -> bool {
    visited.insert(fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf()))
}

/// Empreinte de l'état du coffre : les chemins de ses notes, images et dossiers,
/// et la date de chaque note. Elle change dès qu'un autre programme y touche, sans
/// qu'il faille lire un seul fichier.
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
            if path.is_dir() {
                if first_visit(&mut visited, &path) {
                    seen.push((path.clone(), None));
                    dirs.push(path);
                }
            } else if path.extension().is_some_and(|e| e == "md") {
                seen.push((path, entry.metadata().and_then(|m| m.modified()).ok()));
            } else if is_image(&path) {
                seen.push((path, None));
            } else if is_table(&path) {
                // Un autre programme qui réécrit le tableau ouvert doit se voir.
                seen.push((path, entry.metadata().and_then(|m| m.modified()).ok()));
            }
        }
    }
    // L'ordre de lecture d'un dossier n'est pas garanti.
    seen.sort();
    let mut hasher = DefaultHasher::new();
    seen.hash(&mut hasher);
    hasher.finish()
}

/// Un changement du coffre qui mérite de relire le disque : pas une simple lecture,
/// et pas un fichier ou un dossier caché (`.trash`, `.git`…).
fn matters(root: &Path, event: &Event) -> bool {
    let hidden = |path: &PathBuf| {
        path.strip_prefix(root)
            .is_ok_and(|rel| rel.components().any(|c| c.as_os_str().to_string_lossy().starts_with('.')))
    };
    !event.kind.is_access() && (event.paths.is_empty() || !event.paths.iter().all(hidden))
}

/// Un signal qui réveille la tâche qui l'attend, sans qu'elle ait à le guetter : le thread
/// du système le lève, la tâche dort jusque-là.
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

    /// Vrai, et le signal retombe, s'il a été levé depuis le dernier appel.
    pub fn take(&self) -> bool {
        self.raised.swap(false, Ordering::AcqRel)
    }

    /// Attend que le signal soit levé (vrai) ou que `deadline` finisse (faux).
    pub async fn wait<D: std::future::Future<Output = ()> + Unpin>(&self, mut deadline: D) -> bool {
        std::future::poll_fn(|cx| {
            if self.take() {
                return std::task::Poll::Ready(true);
            }
            *self.waker.lock().unwrap() = Some(cx.waker().clone());
            // Levé entre-temps, avant que le réveil soit en place : on ne le manque pas.
            if self.take() {
                return std::task::Poll::Ready(true);
            }
            std::pin::Pin::new(&mut deadline).poll(cx).map(|()| false)
        })
        .await
    }
}

/// Demande au système de signaler tout changement du coffre : `changed` est levé
/// dès qu'il y en a un. `None` si le système ne sait pas le faire (limite d'inotify
/// atteinte, dossier réseau) : l'appelant relit alors le coffre à intervalle régulier.
/// La surveillance s'arrête quand le résultat est lâché.
pub fn watch(root: &Path, changed: Arc<Signal>) -> Option<RecommendedWatcher> {
    let base = root.to_path_buf();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
        // Une erreur du système : des événements ont pu être perdus, mieux vaut relire.
        if event.map_or(true, |e| matters(&base, &e)) {
            changed.raise();
        }
    })
    .ok()?;
    watcher.watch(root, RecursiveMode::Recursive).ok()?;
    Some(watcher)
}

/// Toutes les notes du coffre (récursif, dossiers cachés ignorés), plus récentes
/// d'abord, tous ses dossiers, même vides, et ses autres fichiers : images et tableaux.
pub fn scan(root: &Path) -> (Vec<Note>, Vec<PathBuf>, Vec<PathBuf>) {
    rescan(root, &[])
}

/// Comme `scan`, sans relire les notes de `known` dont la date n'a pas changé.
pub fn rescan(root: &Path, known: &[Note]) -> (Vec<Note>, Vec<PathBuf>, Vec<PathBuf>) {
    let known: HashMap<&Path, &Note> = known.iter().map(|n| (n.path.as_path(), n)).collect();
    let mut notes = Vec::new();
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
            if path.is_dir() {
                if first_visit(&mut visited, &path) {
                    found.push(path.clone());
                    dirs.push(path);
                }
            } else if path.extension().is_some_and(|e| e == "md") {
                let mtime = entry.metadata().and_then(|m| m.modified()).unwrap_or(UNIX_EPOCH);
                let (tags, links, aliases, body) = match known.get(path.as_path()) {
                    Some(note) if note.mtime == mtime => (note.tags.clone(), note.links.clone(), note.aliases.clone(), note.body.clone()),
                    _ => {
                        let text = fs::read_to_string(&path).unwrap_or_default();
                        let (tags, links) = markdown::index(&text);
                        (tags, links, markdown::aliases(&text), Arc::from(text))
                    }
                };
                notes.push(Note { name: stem(&path), tags, links, aliases, mtime, path, body });
            } else if is_image(&path) || is_table(&path) {
                images.push(path);
            }
        }
    }
    notes.sort_by(|a, b| b.mtime.cmp(&a.mtime));
    (notes, found, images)
}

/// Nom de fichier ou de dossier débarrassé de ce qu'un système de fichiers ou
/// un wikilien refuse ; `None` s'il n'en reste rien.
pub fn clean_name(name: &str) -> Option<String> {
    let name: String = name
        .chars()
        .filter(|c| !c.is_control() && !r#"/\:*?"<>|#^[]"#.contains(*c))
        .take(80)
        .collect();
    // Ni point en tête (fichier caché) ni en fin (refusé par Windows).
    let name = name.trim().trim_matches('.').trim();
    (!name.is_empty()).then(|| name.to_string())
}

/// Déplace ou renomme, sans jamais écraser ce qui porte déjà ce nom.
pub fn rename(from: &Path, to: &Path) -> io::Result<()> {
    if to.exists() {
        let taken = tr("this name is already taken", "ce nom est déjà pris");
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, taken));
    }
    fs::rename(from, to)
}

/// Renomme une note et renvoie son chemin. Si son nom suivait son titre, la
/// première ligne est réécrite pour qu'ils restent accordés.
pub fn rename_note(path: &Path, name: &str) -> io::Result<PathBuf> {
    let content = fs::read_to_string(path)?;
    let to = path.with_file_name(format!("{name}.md"));
    rename(path, &to)?;
    // L'en-tête YAML reste tel quel : le titre est la première ligne qui le suit.
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

/// Met la note ou le dossier à la corbeille du coffre : le dossier caché
/// `.trash`, que l'index ignore. Rien n'est détruit.
pub fn trash(root: &Path, path: &Path) -> io::Result<()> {
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
    fs::rename(path, free)
}

/// Fichier du coffre qui retient les icônes : une ligne `icône<tabulation>chemin` par élément,
/// le chemin relatif au coffre et écrit avec des `/`. Il voyage avec le coffre.
const ICONS: &str = ".bref-icons";

/// Icônes choisies pour des notes, des fichiers et des dossiers du coffre, par chemin.
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

/// Les commandes à essayer, dans l'ordre, pour ouvrir un terminal dans `dir` sous le système
/// `os` (`std::env::consts::OS`) : le programme et ses arguments. `preferred` est le terminal
/// que l'utilisateur nomme dans `$TERMINAL`.
pub fn terminals(os: &str, dir: &str, preferred: Option<&str>) -> Vec<(String, Vec<String>)> {
    let command = |program: &str, args: &[&str]| (program.to_string(), args.iter().map(|a| a.to_string()).collect());
    match os {
        "macos" => vec![command("open", &["-a", "Terminal", dir])],
        // Windows Terminal s'il est installé, sinon la console de toujours.
        "windows" => vec![command("wt", &["-d", dir]), command("cmd", &["/c", "start", "cmd"])],
        // Linux n'a pas de réponse unique. Ceux qui ne font que prévenir une instance déjà
        // lancée ignorent le dossier courant : il leur est donné en argument.
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

/// Ouvre le terminal du système dans `dir` : le premier de `terminals` qui démarre. Faux si
/// aucun n'est installé. Les tests n'ouvrent rien.
pub fn open_terminal(dir: &Path) -> bool {
    let preferred = env::var("TERMINAL").ok().filter(|t| !t.trim().is_empty());
    let found = terminals(env::consts::OS, &dir.to_string_lossy(), preferred.as_deref());
    !cfg!(test)
        && found.into_iter().any(|(program, args)| {
            // Sans console pour le lanceur (`cmd /c start`) ; le terminal, lui, ouvre la sienne.
            let child = crate::update::command(&program).args(args).current_dir(dir).stdin(std::process::Stdio::null()).spawn();
            // Le terminal fermé, son processus est recueilli : pas de zombie tant que Bref tourne.
            child.map(|mut child| std::thread::spawn(move || child.wait())).is_ok()
        })
}

/// Touches proposées pour la capture ; libres sur un GNOME d'origine.
const CAPTURE_KEYS: &str = "<Super><Shift>n";
const GNOME_KEYS: &str = "org.gnome.settings-daemon.plugins.media-keys";
/// Un chemin à nous dans les raccourcis personnalisés de GNOME : le redemander ne double rien.
const GNOME_PATH: &str = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/bref-capture/";

/// Ce qu'a donné la demande d'un raccourci de capture.
#[derive(Debug, PartialEq)]
pub enum Shortcut {
    /// Posé dans les réglages du bureau : les touches, telles qu'on les lit.
    Bound(String),
    /// Posé sans touches, celles proposées étant prises : les voici, à choisir dans les réglages.
    Unbound(String),
    /// Ce bureau ne se règle pas d'ici : la commande à lier soi-même.
    Manual(String),
}

/// Lie `exe --capture` à un raccourci du bureau, sans jamais prendre des touches déjà
/// utilisées. Sous GNOME c'est fait d'ici ; ailleurs la commande est rendue, à lier à la main.
/// Bloquant (quelques appels à `gsettings`) : à lancer hors du thread UI. Les tests ne
/// touchent à aucun réglage.
pub fn capture_shortcut(exe: &Path) -> Shortcut {
    let exe = exe.to_string_lossy();
    let command = if exe.contains(' ') { format!("'{exe}' --capture") } else { format!("{exe} --capture") };
    let gnome = !cfg!(test) && env::var("XDG_CURRENT_DESKTOP").is_ok_and(|desktop| desktop.contains("GNOME"));
    gnome.then(|| gnome_shortcut(&command)).flatten().unwrap_or(Shortcut::Manual(command))
}

/// Ce que répond `gsettings`, s'il est là et accepte.
fn gsettings(args: &[&str]) -> Option<String> {
    let out = crate::update::command("gsettings").args(args).stdin(std::process::Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Le raccourci personnalisé « Bref : capture » de GNOME : créé avec `CAPTURE_KEYS` si rien
/// ne s'en sert, sinon sans touches. S'il existe déjà, seule sa commande est remise à jour (le
/// binaire a pu changer de place) : les touches que l'utilisateur lui a données restent.
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
    // Prises : par un raccourci du système, ou par un autre raccourci personnalisé.
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

/// La liste `['a', 'b']` de `gsettings` (ou `@as []`, vide) avec `path` en plus ; `None` s'il
/// y est déjà.
fn with_keybinding(list: &str, path: &str) -> Option<String> {
    let mut paths: Vec<&str> = list.split('\'').skip(1).step_by(2).collect();
    if paths.contains(&path) {
        return None;
    }
    paths.push(path);
    Some(format!("[{}]", paths.iter().map(|p| format!("'{p}'")).collect::<Vec<_>>().join(", ")))
}

/// `<Super><Shift>n`, comme GNOME l'écrit, tel qu'on le lit : `Super+Maj+N`.
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

/// Sauvegarde : tout le coffre (corbeille comprise) dans une archive `<coffre> <date>.tar.gz`
/// du dossier `dir`, numérotée si le nom est pris. Bloquant : à lancer hors du thread UI.
// ponytail: le `tar` du système (livré avec Linux, macOS et Windows 10) plutôt qu'une
// bibliothèque d'archives en dépendance ; écrire un `.zip` nous-mêmes si le `.tar.gz` gêne
// sous Windows.
pub fn backup(root: &Path, dir: &Path, date: &str) -> io::Result<PathBuf> {
    let (Some(parent), Some(name)) = (root.parent(), root.file_name()) else {
        return Err(io::Error::other(tr("this vault has no parent folder", "ce coffre n'a pas de dossier parent")));
    };
    // Dans le coffre, l'archive se contiendrait elle-même.
    if dir.starts_with(root) {
        return Err(io::Error::other(tr("choose a folder outside the vault", "choisir un dossier hors du coffre")));
    }
    let to = free_path(dir, &format!("{} {date}", name.to_string_lossy()), "tar.gz");
    // L'archive est nommée depuis `dir` : le `tar` de Git pour Windows prendrait `C:\…` pour
    // une machine distante.
    let status = crate::update::command("tar")
        .current_dir(dir)
        .arg("-czf")
        .arg(to.file_name().unwrap_or_default())
        .arg("-C")
        .arg(parent)
        .arg(name)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    match status {
        Ok(status) if status.success() => Ok(to),
        failed => {
            // Une archive incomplète ne doit pas passer pour une sauvegarde.
            let _ = fs::remove_file(&to);
            Err(failed.err().unwrap_or_else(|| io::Error::other(tr("tar failed", "tar a échoué"))))
        }
    }
}

/// Contenu de la corbeille du coffre, du plus récemment modifié au plus ancien.
pub fn trashed(root: &Path) -> Vec<PathBuf> {
    let entries = fs::read_dir(root.join(".trash")).into_iter().flatten().flatten();
    let mut found: Vec<(SystemTime, PathBuf)> =
        entries.map(|e| (e.metadata().and_then(|m| m.modified()).unwrap_or(UNIX_EPOCH), e.path())).collect();
    found.sort_by(|a, b| b.cmp(a));
    found.into_iter().map(|(_, path)| path).collect()
}

/// Sort `path` de la corbeille et le remet à la racine du coffre (la corbeille ne retient pas
/// d'où il venait), sous son nom, numéroté s'il est pris : rien n'est écrasé.
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

/// Chemin libre dans `dir` pour un fichier `name.extension` : le nom est suivi
/// d'un numéro s'il est déjà pris.
pub fn free_path(dir: &Path, name: &str, extension: &str) -> PathBuf {
    let numbered = (1..).map(|n| match n {
        1 => dir.join(format!("{name}.{extension}")),
        n => dir.join(format!("{name} {n}.{extension}")),
    });
    numbered.into_iter().find(|p| !p.exists()).unwrap()
}

/// Nom de fichier (sans extension) tiré de la première ligne non vide.
pub fn title_of(content: &str) -> String {
    let content = &content[markdown::front_matter(content)..];
    let first = content.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    clean_name(first.trim().trim_start_matches('#'))
        .unwrap_or_else(|| tr("Untitled", "Sans titre").to_string())
}

/// Titre `# …` en tête de note, sous la forme d'un nom de fichier.
pub fn h1_of(content: &str) -> Option<String> {
    let content = &content[markdown::front_matter(content)..];
    let first = content.lines().find(|l| !l.trim().is_empty())?;
    clean_name(first.trim_start().strip_prefix("# ")?)
}

/// Enregistre une image collée dans `dir`, sous un nom libre, et renvoie ce nom.
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

/// Ajoute la ligne `text` au bas de la note nommée `name` (la note du jour), où qu'elle soit
/// rangée dans le coffre ; à défaut la crée à la racine, titrée de son nom. Rend son fichier.
/// Une capture ne lit que les noms des fichiers, jamais les notes : elle reste immédiate
/// dans un gros coffre.
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
            if path.is_dir() {
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
    // Une ligne de liste, sauf si elle est déjà écrite ainsi (`- [ ] …`).
    let text = text.trim();
    body.push_str(&if text.starts_with("- ") { format!("{text}\n") } else { format!("- {text}\n") });
    write(&path, &body)?;
    Ok(path)
}

/// Écriture atomique : un crash ne laisse jamais une note tronquée.
pub fn write(path: &Path, content: &str) -> io::Result<()> {
    let tmp = path.with_file_name(format!(".{}.tmp", stem(path)));
    let mut file = fs::File::create(&tmp)?;
    file.write_all(content.as_bytes())?;
    // Sans `fsync`, une coupure de courant peut laisser le renommage sans les données.
    file.sync_all()?;
    fs::rename(&tmp, path)
}

/// Enregistre la note et renvoie son chemin ; une nouvelle note est créée dans
/// `dir`. Si `synced` (le nom du fichier
/// suivait déjà le titre), le fichier est renommé quand le titre change, sauf
/// si le nouveau nom est déjà pris.
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
    fn gnome_shortcut_list_gains_the_capture_once() {
        let path = "/org/gnome/x/bref-capture/";
        assert_eq!(with_keybinding("@as []", path).as_deref(), Some("['/org/gnome/x/bref-capture/']"));
        assert_eq!(with_keybinding("['/org/gnome/x/custom0/', '/org/gnome/x/custom1/']", path).as_deref(), Some("['/org/gnome/x/custom0/', '/org/gnome/x/custom1/', '/org/gnome/x/bref-capture/']"));
        // Déjà là : rien à ajouter, la demande peut se répéter sans rien doubler.
        assert_eq!(with_keybinding("['/org/gnome/x/custom0/', '/org/gnome/x/bref-capture/']", path), None);
    }

    #[test]
    fn gnome_keys_are_spelled_like_the_others() {
        assert_eq!(spell_keys("<Super><Shift>n"), "Super+Maj+N");
        assert_eq!(spell_keys("<Primary><Alt>space"), "Ctrl+Alt+Space");
        assert_eq!(spell_keys("F9"), "F9");
    }

    /// Le vrai `gsettings`, sur des réglages jetables (fichier clé-valeur dans un dossier
    /// temporaire) : `cargo test -- --ignored gnome_shortcut_is_registered`, sur un poste GNOME.
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
        // Redemandé après un déplacement du binaire : la commande suit, la touche choisie reste.
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
        // Pas de note du jour : elle est créée à la racine, titrée de son nom.
        let made = capture(&root, "2026-10-10", "appeler Léa").unwrap();
        assert_eq!(made, root.join("2026-10-10.md"));
        assert_eq!(fs::read_to_string(&made).unwrap(), "# 2026-10-10\n\n- appeler Léa\n");
        // La suivante s'ajoute dessous ; une ligne déjà écrite en liste reste telle quelle.
        capture(&root, "2026-10-10", "- [ ] acheter du pain").unwrap();
        assert_eq!(fs::read_to_string(&made).unwrap(), "# 2026-10-10\n\n- appeler Léa\n- [ ] acheter du pain\n");
        // La note du jour rangée dans un dossier est retrouvée ; celle de la corbeille, non.
        let filed = root.join("Journal/2026-10-11.md");
        fs::write(&filed, "# 2026-10-11\n\nsans fin de ligne").unwrap();
        fs::write(root.join(".trash/2026-10-11.md"), "jetée").unwrap();
        assert_eq!(capture(&root, "2026-10-11", "une idée").unwrap(), filed);
        assert_eq!(fs::read_to_string(&filed).unwrap(), "# 2026-10-11\n\nsans fin de ligne\n- une idée\n");
        assert_eq!(fs::read_to_string(root.join(".trash/2026-10-11.md")).unwrap(), "jetée");
        let _ = fs::remove_dir_all(&root);
    }

    /// Exécute un futur sur le thread courant, qui dort entre deux réveils.
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
        // Levé avant l'attente : pas de sommeil. Retombé ensuite.
        signal.raise();
        assert!(block_on(signal.wait(std::future::pending())));
        assert!(!signal.take());
        // L'échéance finie, sans signal : faux.
        assert!(!block_on(signal.wait(std::future::ready(()))));
        // Levé par un autre thread pendant que la tâche dort.
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
        // Nouvelle note de même titre : pas d'écrasement.
        assert_eq!(save(&root, None, true, "# A\n").unwrap(), root.join("A 2.md"));
        // Le titre change : le fichier suit.
        let b = save(&root, Some(&a), true, "# B\n").unwrap();
        assert_eq!(b, root.join("B.md"));
        assert!(!a.exists());
        // Nom déjà pris ou note non synchronisée : on garde le fichier.
        assert_eq!(save(&root, Some(&b), true, "# A 2\n").unwrap(), b);
        assert_eq!(save(&root, Some(&b), false, "# Z\n").unwrap(), b);
        assert_eq!(fs::read_to_string(&b).unwrap(), "# Z\n");
        assert_eq!(scan(&root).0.len(), 2);
        // En-tête YAML : le nom vient du titre qui le suit, et renommer ne touche que ce titre.
        let meta = "---\ntitle: autre\n---\n";
        let c = save(&root, None, true, &format!("{meta}# Fiche\n---\n")).unwrap();
        assert_eq!((c.clone(), h1_of(&format!("{meta}\n# Fiche\n"))), (root.join("Fiche.md"), Some("Fiche".into())));
        let c = rename_note(&c, "Carte").unwrap();
        assert_eq!(fs::read_to_string(&c).unwrap(), format!("{meta}# Carte\n---\n"));
        fs::remove_file(&c).unwrap();
        // L'empreinte du coffre suit ce qu'un autre programme y change.
        let before = fingerprint(&root);
        assert_eq!(before, fingerprint(&root));
        fs::write(root.join("ailleurs.md"), "x").unwrap();
        assert_ne!(before, fingerprint(&root));
        fs::remove_file(root.join("ailleurs.md")).unwrap();

        // Renommer une note accordée à son titre réécrit ce titre ; sinon le texte reste tel quel.
        let c = rename_note(&root.join("A 2.md"), "C").unwrap();
        assert_eq!(fs::read_to_string(&c).unwrap(), "# A\n");
        fs::write(root.join("D.md"), "\n## D\ntexte D\n").unwrap();
        let e = rename_note(&root.join("D.md"), "E").unwrap();
        assert_eq!(fs::read_to_string(&e).unwrap(), "\n## E\ntexte D\n");
        // Jamais d'écrasement.
        assert!(rename_note(&e, "C").is_err() && e.exists());

        // Corbeille : rien n'est détruit, et l'index ne la voit pas.
        fs::create_dir(root.join("vide")).unwrap();
        trash(&root, &c).unwrap();
        fs::write(&c, "autre").unwrap();
        trash(&root, &c).unwrap();
        assert_eq!(fs::read_to_string(root.join(".trash/C.md")).unwrap(), "# A\n");
        assert_eq!(fs::read_to_string(root.join(".trash/2 C.md")).unwrap(), "autre");
        fs::write(root.join("vide/Photo.PNG"), "").unwrap();
        let (notes, dirs, images) = scan(&root);
        assert_eq!((notes.len(), dirs), (2, vec![root.join("vide")]));
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
        // `$TERMINAL` passe devant ; sans lui, le choix du bureau d'abord.
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
        // Le même jour : une seconde archive, la première reste.
        assert_eq!(backup(&root, &out, "2026-10-09").unwrap(), out.join("Mon coffre 2026-10-09 2.tar.gz"));
        let listed = crate::update::command("tar").current_dir(&out).arg("-tzf").arg(first.file_name().unwrap()).output().unwrap();
        let listed = String::from_utf8_lossy(&listed.stdout).replace('\\', "/");
        assert!(listed.contains("Mon coffre/A.md") && listed.contains("Mon coffre/.trash/Vieux.md"), "{listed}");
        // Dans le coffre, l'archive se contiendrait elle-même : refusé, rien n'est écrit.
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
        // Le nom est repris entre-temps : la note restaurée en prend un autre, rien n'est écrasé.
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
        // Hors du coffre ou sans chemin : dans le doute, on relit.
        assert!(matters(root, &Event::new(created).add_path("/ailleurs/.x".into())));
        assert!(matters(root, &Event::new(created)));
    }

    /// Un lien symbolique qui remonte au coffre, ou un second lien vers un dossier déjà
    /// parcouru, ne boucle pas et ne duplique aucune note.
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
        // L'empreinte, elle aussi, se calcule sans boucler et ne bouge pas.
        assert_eq!(fingerprint(&root), fingerprint(&root));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn watcher_reports_a_new_note() {
        let root = env::temp_dir().join(format!("bref-watch-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let changed = Arc::new(Signal::default());
        let _watcher = watch(&root, changed.clone()).expect("le système doit pouvoir surveiller un dossier");
        // Pas d'horloge simulée ici : le système met quelques millisecondes (FSEvents, un peu plus).
        fs::write(root.join("neuve.md"), "x").unwrap();
        let start = std::time::Instant::now();
        while !changed.raised.load(Ordering::Acquire) && start.elapsed() < std::time::Duration::from_secs(10) {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(changed.take(), "aucun événement en 10 s");
        fs::remove_dir_all(&root).unwrap();
    }
}
