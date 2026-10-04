//! Coffre : un dossier de fichiers `.md`, plus la petite config de l'app.

use std::{
    env, fs, io,
    path::{Path, PathBuf},
    time::SystemTime,
};

use crate::{markdown, tr};

pub struct Note {
    pub name: String,
    pub path: PathBuf,
    pub tags: Vec<String>,
    /// Notes visées par ses `[[wikiliens]]`, en minuscules.
    pub links: Vec<String>,
    pub mtime: SystemTime,
}

/// Dossier de configuration de l'utilisateur, selon la plateforme.
fn config_dir() -> PathBuf {
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
    config_dir().join("encre/config")
}

/// (coffre, notes ouvertes de la plus récente à la plus ancienne) : une ligne chacun.
pub fn load_config() -> (Option<PathBuf>, Vec<PathBuf>) {
    // Repli sur la config de l'ancien nom de l'app ; elle n'est jamais réécrite.
    let text = fs::read_to_string(config_path())
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
        eprintln!("encre: {} ({}): {e}", tr("config not saved", "config non enregistrée"), path.display());
    }
}

fn load_file(name: &str) -> String {
    fs::read_to_string(config_dir().join("encre").join(name)).unwrap_or_default()
}

fn save_file(name: &str, text: &str) {
    let path = config_dir().join("encre").join(name);
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

/// Toutes les notes du coffre (récursif, dossiers cachés ignorés), plus récentes
/// d'abord, et tous ses dossiers, même vides.
pub fn scan(root: &Path) -> (Vec<Note>, Vec<PathBuf>) {
    let mut notes = Vec::new();
    let mut found = Vec::new();
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
                found.push(path.clone());
                dirs.push(path);
            } else if path.extension().is_some_and(|e| e == "md") {
                let (tags, links) = fs::read_to_string(&path)
                    .map(|t| markdown::index(&t))
                    .unwrap_or_default();
                notes.push(Note {
                    name: stem(&path),
                    tags,
                    links,
                    mtime: entry
                        .metadata()
                        .and_then(|m| m.modified())
                        .unwrap_or(SystemTime::UNIX_EPOCH),
                    path,
                });
            }
        }
    }
    notes.sort_by(|a, b| b.mtime.cmp(&a.mtime));
    (notes, found)
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

/// Nom de fichier (sans extension) tiré de la première ligne non vide.
pub fn title_of(content: &str) -> String {
    let first = content.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    clean_name(first.trim().trim_start_matches('#'))
        .unwrap_or_else(|| tr("Untitled", "Sans titre").to_string())
}

/// Écriture atomique : un crash ne laisse jamais une note tronquée.
pub fn write(path: &Path, content: &str) -> io::Result<()> {
    let tmp = path.with_file_name(format!(".{}.tmp", stem(path)));
    fs::write(&tmp, content)?;
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
    fn derives_titles() {
        assert_eq!(title_of("\n# Courses: lundi\nlait"), "Courses lundi");
        assert_eq!(title_of("  [[x]] / y  "), "x  y");
        assert_eq!(title_of("\n\n"), "Sans titre");
        assert_eq!(title_of("# .."), "Sans titre");
    }

    #[test]
    fn saves_and_renames() {
        let root = env::temp_dir().join(format!("encre-test-{}", std::process::id()));
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
        let (notes, dirs) = scan(&root);
        assert_eq!((notes.len(), dirs), (2, vec![root.join("vide")]));
        assert_eq!(clean_name(" ../a:b. "), Some("ab".into()));
        assert_eq!(clean_name(" . "), None);
        fs::remove_dir_all(&root).unwrap();
    }
}
