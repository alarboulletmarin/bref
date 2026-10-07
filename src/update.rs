//! Nouvelle version : une requête à l'API de GitHub au plus une fois par jour, puis, sur
//! demande, téléchargement et installation. Rien d'autre ne sort de l'app, et elle ne dit
//! que « bref » comme nom de client.

use std::{
    ffi::OsStr,
    fs::{DirBuilder, File},
    io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use sha2::{Digest, Sha256};

use crate::{tr, vault};

const DAY: u64 = 24 * 3600;
/// Les fichiers d'une release arrivent un quart d'heure après elle : on revérifie plus tôt.
const HOUR: u64 = 3600;

/// Une version plus récente que celle qui tourne.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Release {
    pub version: String,
    /// Adresse du fichier à installer pour ce système et cette installation ; `None` si l'app
    /// ne peut pas se mettre à jour seule (ou si le fichier n'est pas encore publié).
    pub asset: Option<String>,
    /// Somme SHA-256 (hexadécimale) de ce fichier, telle que GitHub la publie avec la release.
    pub sha256: Option<String>,
}

/// Page où télécharger la dernière version.
pub fn releases_url() -> String {
    format!("{}/releases/latest", env!("CARGO_PKG_REPOSITORY"))
}

/// Adresse de la dernière release. `BREF_RELEASES_API` la remplace : les essais de la mise à
/// jour la pointent vers un fichier local (`file://`) qui décrit une release.
fn api_url() -> String {
    std::env::var("BREF_RELEASES_API").unwrap_or_else(|_| {
        let repo = env!("CARGO_PKG_REPOSITORY").replace("https://github.com/", "https://api.github.com/repos/");
        format!("{repo}/releases/latest")
    })
}

/// `BREF_RELEASES_API` est un réglage d'essai : il lève les contrôles de l'adresse et de la somme.
fn overridden() -> bool {
    std::env::var_os("BREF_RELEASES_API").is_some()
}

/// Le fichier à installer ne vient que des releases de ce dépôt, et en HTTPS de bout en bout
/// (`curl` suit les redirections : sans `--proto-redir`, une adresse `http://` passerait).
fn trusted(url: &str) -> bool {
    url.strip_prefix(env!("CARGO_PKG_REPOSITORY")).is_some_and(|rest| rest.starts_with("/releases/download/"))
}

fn curl() -> Command {
    let mut curl = command("curl");
    if !overridden() {
        curl.args(["--proto", "=https", "--proto-redir", "=https"]);
    }
    curl
}

/// Version plus récente que celle qui tourne, si GitHub en connaît une. Le résultat du
/// dernier contrôle est gardé dans le fichier `update` de la config : avant 24 h, il est
/// relu au lieu de refaire la requête. Bloquant : à lancer hors du thread UI.
pub fn check() -> Option<Release> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (at, known) = parse_state(&vault::load_file("update"));
    let installable = asset_name().is_some();
    let waiting_for_files = !known.version.is_empty() && known.asset.is_none() && installable;
    let latest = if at <= now && now - at < if waiting_for_files { HOUR } else { DAY } {
        known
    } else if let Some(found) = fetch() {
        vault::save_file("update", &state_text(now, &found));
        found
    } else {
        // Hors ligne, ou sans `curl` : rien n'est noté, le prochain démarrage réessaie.
        known
    };
    newer(env!("CARGO_PKG_VERSION"), &latest.version).then(|| only_if_installable(latest, installable))
}

/// Le contrôle gardé dans le fichier `update` peut venir d'une autre installation de la même
/// machine (le zip à côté de l'installeur, `bref-git` après le paquet) : son fichier à installer
/// ne vaut que si celle-ci sait s'en servir.
fn only_if_installable(release: Release, installable: bool) -> Release {
    if installable { release } else { Release { asset: None, sha256: None, ..release } }
}

/// Télécharge le fichier de la release, vérifie sa somme et l'installe. `Ok` : le nouveau
/// programme se relance de lui-même, l'app peut quitter. Bloquant : à lancer hors du thread UI.
pub fn install(url: &str, sha256: Option<&str>) -> Result<(), String> {
    // Les tests ne sortent pas sur le réseau et n'installent rien.
    if cfg!(test) {
        return Err("no installation in tests".into());
    }
    if !overridden() {
        if !trusted(url) {
            return Err(tr("untrusted download address", "adresse de téléchargement non fiable").into());
        }
        if sha256.is_none() {
            return Err(tr("this release publishes no checksum", "cette version ne publie pas de somme de contrôle").into());
        }
    }
    let dir = private_dir().map_err(|e| e.to_string())?;
    let name = url.rsplit('/').next().filter(|n| !n.is_empty()).unwrap_or("bref-update");
    let file = dir.join(name);
    let installed = run(curl().args(["-fL", "--max-time", "900", "-A", "bref", "-o"]).arg(&file).arg(url))
        .and_then(|()| sha256.map_or(Ok(()), |expected| verify(&file, expected)))
        .and_then(|()| apply(&file));
    // Un fichier refusé ou dont l'installation a échoué ne reste pas dans le dossier temporaire ;
    // après un succès, il y reste : l'installeur le lit encore quand l'app a quitté.
    if installed.is_err() {
        let _ = std::fs::remove_dir_all(&dir);
    }
    installed
}

/// Un dossier neuf, à nous seuls (`0700`), dans le dossier temporaire. Le nom n'est pas
/// prévisible et la création échoue s'il existe : sous Linux, `/tmp` est partagé, et un
/// autre utilisateur pourrait sinon remplacer le fichier avant que `pkexec` l'installe.
fn private_dir() -> io::Result<PathBuf> {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.subsec_nanos());
    for n in 0..100 {
        let dir = std::env::temp_dir().join(format!("bref-update-{}-{stamp:x}-{n}", std::process::id()));
        let mut builder = DirBuilder::new();
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
        match builder.create(&dir) {
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            made => return made.map(|()| dir),
        }
    }
    Err(io::Error::new(io::ErrorKind::AlreadyExists, tr("no free folder", "aucun dossier libre")))
}

/// La somme SHA-256 du fichier téléchargé est celle que GitHub a publiée.
fn verify(file: &Path, expected: &str) -> Result<(), String> {
    let mut hasher = Sha256::new();
    io::copy(&mut File::open(file).map_err(|e| e.to_string())?, &mut hasher).map_err(|e| e.to_string())?;
    if format!("{:x}", hasher.finalize()).eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(tr("the download does not match its checksum", "le fichier téléchargé ne correspond pas à sa somme de contrôle").into())
    }
}

// ponytail: `curl` (livré avec macOS, Windows 10 et la plupart des Linux) plutôt qu'une
// bibliothèque HTTP et TLS en dépendance ; sans lui, l'app ne signale simplement rien.
// Le fichier est vérifié par la somme SHA-256 que GitHub publie avec la release : elle protège
// le téléchargement, pas le compte GitHub lui-même. Signer les versions (clé minisign) si l'app grandit.
fn fetch() -> Option<Release> {
    let out = curl()
        .args(["-fsSL", "--max-time", "10", "-A", "bref", "-H", "Accept: application/vnd.github+json"])
        .arg(api_url())
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    release_of(&String::from_utf8_lossy(&out.stdout), asset_name())
}

/// Un processus sans fenêtre de console (l'app n'en a pas sous Windows : sinon elle clignote).
fn command(program: impl AsRef<OsStr>) -> Command {
    #[allow(unused_mut)]
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    command
}

/// Lance la commande et attend ; en cas d'échec, la dernière ligne de son message d'erreur.
fn run(command: &mut Command) -> Result<(), String> {
    let out = command.stdin(Stdio::null()).output().map_err(|e| e.to_string())?;
    if out.status.success() {
        return Ok(());
    }
    let message = String::from_utf8_lossy(&out.stderr);
    Err(message.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("failed").trim().to_string())
}

/// Relance `program` quand ce processus-ci aura quitté.
#[cfg(not(windows))]
fn relaunch(program: &str, args: &[&Path]) -> Result<(), String> {
    Command::new("sh")
        .args(["-c", r#"while kill -0 "$0" 2>/dev/null; do sleep 0.2; done; exec "$@""#])
        .arg(std::process::id().to_string())
        .arg(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(drop)
        .map_err(|e| e.to_string())
}

/// Windows : l'installeur de la version, en silencieux. Il remplace `bref.exe` (en le fermant
/// s'il tourne encore) puis relance l'app (`/RELAUNCH=1`, lu par `packaging/windows/bref.iss`).
#[cfg(windows)]
fn apply(file: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    Command::new(file)
        .args(["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/CLOSEAPPLICATIONS", "/RELAUNCH=1"])
        // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP : l'installeur survit à l'app qui l'a lancé.
        .creation_flags(0x0000_0208)
        .stdin(Stdio::null())
        .spawn()
        .map(drop)
        .map_err(|e| e.to_string())
}

/// Seule une app installée par son installeur (qui laisse `unins000.exe` à côté) peut se
/// mettre à jour ainsi, pas celle du zip ni celle de `cargo install`.
#[cfg(windows)]
fn asset_name() -> Option<&'static str> {
    let exe = std::env::current_exe().ok()?;
    exe.with_file_name("unins000.exe").is_file().then_some("bref-windows-x86_64-setup.exe")
}

/// Le bundle `.app` qui contient ce programme.
#[cfg(target_os = "macos")]
fn app_bundle() -> Option<std::path::PathBuf> {
    std::env::current_exe().ok()?.ancestors().find(|p| p.extension().is_some_and(|e| e == "app")).map(Path::to_path_buf)
}

#[cfg(target_os = "macos")]
fn asset_name() -> Option<&'static str> {
    app_bundle().map(|_| "bref-macos.dmg")
}

/// macOS : remplace `Bref.app` par celui du disque image, puis relance l'app.
#[cfg(target_os = "macos")]
fn apply(file: &Path) -> Result<(), String> {
    let app = app_bundle().ok_or("not running from an app bundle")?;
    replace_app(file, &app)?;
    relaunch("open", &[&app])
}

/// Copie `Bref.app` du disque image à côté de `app` (même volume, donc le renommage est
/// atomique), puis l'échange avec l'ancien. En cas d'échec, l'ancien reste en place. Le
/// programme en cours d'exécution continue de tourner : son fichier n'est pas supprimé tant qu'il l'ouvre.
#[cfg(target_os = "macos")]
fn replace_app(dmg: &Path, app: &Path) -> Result<(), String> {
    let dir = app.parent().ok_or("no parent folder")?;
    let (fresh, old) = (dir.join(".Bref.app.new"), dir.join(".Bref.app.old"));
    let mount = std::env::temp_dir().join(format!("bref-update-mount-{}", std::process::id()));
    std::fs::create_dir_all(&mount).map_err(|e| e.to_string())?;
    run(Command::new("hdiutil").args(["attach", "-nobrowse", "-readonly", "-noverify", "-mountpoint"]).arg(&mount).arg(dmg))?;
    let swapped = (|| {
        let _ = std::fs::remove_dir_all(&fresh);
        let _ = std::fs::remove_dir_all(&old);
        run(Command::new("ditto").arg(mount.join("Bref.app")).arg(&fresh))?;
        std::fs::rename(app, &old).map_err(|e| e.to_string())?;
        if let Err(e) = std::fs::rename(&fresh, app) {
            let _ = std::fs::rename(&old, app);
            return Err(e.to_string());
        }
        Ok(())
    })();
    let _ = run(Command::new("hdiutil").args(["detach", "-force"]).arg(&mount));
    let _ = std::fs::remove_dir(&mount);
    let _ = std::fs::remove_dir_all(&fresh);
    let _ = std::fs::remove_dir_all(&old);
    swapped
}

/// Linux : seul le paquet Arch de la release (`bref`, installé dans `/usr/bin`) est mis à jour,
/// par pacman ; les autres installations (compilées, `bref-git`) n'ont pas de fichier à installer.
#[cfg(target_os = "linux")]
fn asset_name() -> Option<&'static str> {
    let owned = || {
        let owner = Command::new("pacman").args(["-Qqo", "/usr/bin/bref"]).output().ok()?;
        (String::from_utf8_lossy(&owner.stdout).trim() == "bref").then_some(())
    };
    let here = std::env::current_exe().ok().is_some_and(|p| p == Path::new("/usr/bin/bref"));
    (cfg!(target_arch = "x86_64") && here && owned().is_some()).then_some("bref-x86_64.pkg.tar.zst")
}

/// Linux : `pkexec` demande le mot de passe dans une fenêtre du bureau, puis pacman installe.
#[cfg(target_os = "linux")]
fn apply(file: &Path) -> Result<(), String> {
    run(Command::new("pkexec").args(["pacman", "-U", "--noconfirm"]).arg(file))?;
    relaunch("/usr/bin/bref", &[])
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
fn asset_name() -> Option<&'static str> {
    None
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
fn apply(_: &Path) -> Result<(), String> {
    Err("not supported on this system".into())
}

/// La release publiée : son numéro (`v0.2.0` donne `0.2.0`) et l'adresse du fichier nommé
/// `asset`, s'il y est. Rien pour un brouillon ou une préversion.
fn release_of(json: &str, asset: Option<&str>) -> Option<Release> {
    let release: serde_json::Value = serde_json::from_str(json).ok()?;
    let flag = |key| release[key].as_bool().unwrap_or(false);
    if flag("draft") || flag("prerelease") {
        return None;
    }
    let version = release["tag_name"].as_str()?.trim_start_matches('v').to_string();
    let found = asset.and_then(|name| release["assets"].as_array()?.iter().find(|a| a["name"] == name));
    let text = |key| found.and_then(|a| a[key].as_str());
    Some(Release {
        version,
        asset: text("browser_download_url").map(String::from),
        sha256: text("digest").and_then(|d| d.strip_prefix("sha256:")).map(String::from),
    })
}

/// `latest` est un numéro `X.Y.Z` strictement plus grand que `current`.
fn newer(current: &str, latest: &str) -> bool {
    let parts = |v: &str| -> Option<Vec<u64>> {
        let parts: Vec<u64> = v.trim().split('.').map(|p| p.parse().ok()).collect::<Option<_>>()?;
        (parts.len() == 3).then_some(parts)
    };
    matches!((parts(current), parts(latest)), (Some(c), Some(l)) if l > c)
}

/// Contenu du fichier `update` : l'heure du dernier contrôle (secondes depuis 1970), la
/// version trouvée, l'adresse de son fichier, puis sa somme. Illisible ou absent : jamais contrôlé.
fn parse_state(text: &str) -> (u64, Release) {
    let mut lines = text.lines().map(str::trim);
    let at = lines.next().and_then(|l| l.parse().ok()).unwrap_or(0);
    let version = lines.next().unwrap_or_default().to_string();
    let mut optional = || lines.next().filter(|l| !l.is_empty()).map(String::from);
    let (asset, sha256) = (optional(), optional());
    (at, Release { version, asset, sha256 })
}

fn state_text(at: u64, release: &Release) -> String {
    let text = |value: &Option<String>| value.clone().unwrap_or_default();
    format!("{at}\n{}\n{}\n{}\n", release.version, text(&release.asset), text(&release.sha256))
}

#[cfg(test)]
mod tests {
    use super::*;

    const JSON: &str = r#"{"tag_name": "v0.2.0", "draft": false, "prerelease": false, "assets": [
        {"name": "bref-x86_64.pkg.tar.zst", "browser_download_url": "https://x/bref-x86_64.pkg.tar.zst"},
        {"name": "bref-macos.dmg", "browser_download_url": "https://x/bref-macos.dmg", "digest": "sha256:ab12"}]}"#;

    #[test]
    fn reads_the_published_release() {
        let release = |asset| release_of(JSON, asset).unwrap();
        assert_eq!(release(None), Release { version: "0.2.0".into(), ..Default::default() });
        let dmg = release(Some("bref-macos.dmg"));
        assert_eq!(dmg.asset.as_deref(), Some("https://x/bref-macos.dmg"));
        assert_eq!(dmg.sha256.as_deref(), Some("ab12"));
        // Pas de somme publiée : rien à comparer, l'installation sera refusée.
        assert_eq!(release(Some("bref-x86_64.pkg.tar.zst")).sha256, None);
        // Le fichier de ce système n'est pas (encore) publié.
        assert_eq!(release(Some("bref-windows-x86_64-setup.exe")).asset, None);
        assert_eq!(release_of(r#"{"tag_name": "0.3.1"}"#, Some("a")).unwrap().version, "0.3.1");
        assert_eq!(release_of(r#"{"tag_name": "v0.3.0-rc1", "prerelease": true}"#, None), None);
        assert_eq!(release_of(r#"{"tag_name": "v0.3.0", "draft": true}"#, None), None);
        // Réponse d'erreur ou charabia : rien.
        assert_eq!(release_of(r#"{"message": "Not Found"}"#, None), None);
        assert_eq!(release_of("<html>", None), None);
    }

    #[test]
    fn keeps_the_file_only_for_an_installable_app() {
        let found = Release { version: "0.2.0".into(), asset: Some("https://x/a.dmg".into()), sha256: Some("ab12".into()) };
        assert_eq!(only_if_installable(found.clone(), true), found);
        assert_eq!(only_if_installable(found, false), Release { version: "0.2.0".into(), ..Default::default() });
    }

    #[test]
    fn only_trusts_release_files_of_this_repository() {
        let repo = env!("CARGO_PKG_REPOSITORY");
        assert!(trusted(&format!("{repo}/releases/download/v0.2.2/bref-macos.dmg")));
        assert!(!trusted("http://github.com/alarboulletmarin/bref/releases/download/v1/a"));
        assert!(!trusted(&format!("{repo}-evil/releases/download/v1/a")));
        assert!(!trusted("https://github.com/autre/bref/releases/download/v1/a"));
        assert!(!trusted("file:///tmp/a.pkg.tar.zst"));
    }

    #[test]
    fn refuses_a_download_that_does_not_match_its_checksum() {
        let dir = private_dir().unwrap();
        let file = dir.join("a");
        std::fs::write(&file, "abc").unwrap();
        // SHA-256 de « abc ».
        let abc = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert!(verify(&file, abc).is_ok());
        assert!(verify(&file, &abc.to_uppercase()).is_ok());
        assert!(verify(&file, "00").is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Le dossier de téléchargement est neuf et fermé aux autres utilisateurs.
    #[cfg(unix)]
    #[test]
    fn downloads_into_a_private_folder() {
        use std::os::unix::fs::PermissionsExt;
        let (a, b) = (private_dir().unwrap(), private_dir().unwrap());
        assert_ne!(a, b);
        assert_eq!(std::fs::metadata(&a).unwrap().permissions().mode() & 0o777, 0o700);
        std::fs::remove_dir_all(&a).unwrap();
        std::fs::remove_dir_all(&b).unwrap();
    }

    #[test]
    fn compares_versions_as_numbers() {
        assert!(newer("0.1.9", "0.2.0"));
        assert!(newer("0.9.9", "0.10.0"));
        assert!(!newer("0.2.0", "0.2.0"));
        assert!(!newer("0.2.1", "0.2.0"));
        assert!(!newer("0.2.0", ""));
        assert!(!newer("0.2.0", "latest"));
        assert!(!newer("0.2.0", "0.3"));
    }

    #[test]
    fn reads_the_saved_check() {
        let found = Release { version: "0.2.0".into(), asset: Some("https://x/a.dmg".into()), sha256: Some("ab12".into()) };
        assert_eq!(parse_state(&state_text(1_730_000_000, &found)), (1_730_000_000, found));
        let bare = Release { version: "0.2.0".into(), ..Default::default() };
        assert_eq!(parse_state(&state_text(5, &bare)), (5, bare));
        // L'ancien format (sans adresse), puis vide ou illisible : jamais contrôlé.
        assert_eq!(parse_state("1730000000\n0.2.0\n").1.version, "0.2.0");
        assert_eq!(parse_state(""), (0, Release::default()));
        assert_eq!(parse_state("pas un nombre\n0.2.0").0, 0);
    }

    /// Sur un vrai macOS : le disque image remplace l'app installée, et rien ne traîne.
    #[cfg(target_os = "macos")]
    #[test]
    fn replaces_the_app_from_a_disk_image() {
        use std::fs;
        let dir = std::env::temp_dir().join(format!("bref-update-test-{}", std::process::id()));
        let program = |root: &Path| root.join("Bref.app/Contents/MacOS/bref");
        let write = |root: &Path, text: &str| {
            fs::create_dir_all(program(root).parent().unwrap()).unwrap();
            fs::write(program(root), text).unwrap();
        };
        write(&dir.join("image"), "nouvelle");
        let dmg = dir.join("new.dmg");
        run(Command::new("hdiutil").args(["create", "-volname", "Bref", "-format", "UDZO", "-srcfolder"])
            .arg(dir.join("image")).arg(&dmg))
        .unwrap();
        write(&dir.join("Applications"), "ancienne");
        let installed = dir.join("Applications/Bref.app");
        replace_app(&dmg, &installed).unwrap();
        assert_eq!(fs::read_to_string(program(&dir.join("Applications"))).unwrap(), "nouvelle");
        let left: Vec<_> = fs::read_dir(dir.join("Applications")).unwrap().flatten().collect();
        assert_eq!(left.len(), 1, "ni .new ni .old ne doivent rester");
        // Un disque image illisible : l'app installée reste telle quelle.
        fs::write(dir.join("bad.dmg"), "pas un disque").unwrap();
        assert!(replace_app(&dir.join("bad.dmg"), &installed).is_err());
        assert_eq!(fs::read_to_string(program(&dir.join("Applications"))).unwrap(), "nouvelle");
        fs::remove_dir_all(&dir).unwrap();
    }
}
