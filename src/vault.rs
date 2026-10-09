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
    pub mtime: SystemTime,
    /// Texte entier de la note, pour la recherche plein texte.
    // ponytail: tout le texte du coffre reste en mémoire (5 000 notes de 6 Ko : 30 Mo) ;
    // passer à un index sur disque si des coffres bien plus gros se présentent.
    pub body: Arc<str>,
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
                let (tags, links, body) = match known.get(path.as_path()) {
                    Some(note) if note.mtime == mtime => (note.tags.clone(), note.links.clone(), note.body.clone()),
                    _ => {
                        let text = fs::read_to_string(&path).unwrap_or_default();
                        let (tags, links) = markdown::index(&text);
                        (tags, links, Arc::from(text))
                    }
                };
                notes.push(Note { name: stem(&path), tags, links, mtime, path, body });
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
    if stem(path) == title_of(&content)
        && let Some(first) = content.lines().find(|l| !l.trim().is_empty())
    {
        let hashes = first.trim_start().chars().take_while(|c| *c == '#').count();
        let title = if hashes > 0 { format!("{} {name}", "#".repeat(hashes)) } else { name.to_string() };
        fs::write(&to, content.replacen(first, &title, 1))?;
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
    let first = content.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    clean_name(first.trim().trim_start_matches('#'))
        .unwrap_or_else(|| tr("Untitled", "Sans titre").to_string())
}

/// Titre `# …` en tête de note, sous la forme d'un nom de fichier.
pub fn h1_of(content: &str) -> Option<String> {
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
