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

/// Disposition du panneau de navigation, mémorisée telle quelle sur une ligne.
pub fn load_layout() -> String {
    fs::read_to_string(config_dir().join("encre/layout")).unwrap_or_default()
}

pub fn save_layout(text: &str) {
    let path = config_dir().join("encre/layout");
    // Simple confort : si l'écriture échoue, l'app rouvrira panneau replié.
    let _ = path.parent().map_or(Ok(()), fs::create_dir_all).and_then(|_| fs::write(path, text));
}

pub fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Toutes les notes du coffre (récursif, dossiers cachés ignorés), plus récentes d'abord.
pub fn scan(root: &Path) -> Vec<Note> {
    let mut notes = Vec::new();
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
    notes
}

/// Nom de fichier (sans extension) tiré de la première ligne non vide.
pub fn title_of(content: &str) -> String {
    let first = content.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let title: String = first
        .trim()
        .trim_start_matches('#')
        .chars()
        .filter(|c| !c.is_control() && !r#"/\:*?"<>|#^[]"#.contains(*c))
        .take(80)
        .collect();
    // Ni point en tête (fichier caché) ni en fin (refusé par Windows).
    let title = title.trim().trim_matches('.').trim();
    if title.is_empty() {
        tr("Untitled", "Sans titre").to_string()
    } else {
        title.to_string()
    }
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
    // Écriture atomique : un crash ne laisse jamais une note tronquée.
    let tmp = target.with_file_name(format!(".{}.tmp", stem(&target)));
    fs::write(&tmp, content)?;
    fs::rename(&tmp, &target)?;
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
        assert_eq!(scan(&root).len(), 2);
        fs::remove_dir_all(&root).unwrap();
    }
}
