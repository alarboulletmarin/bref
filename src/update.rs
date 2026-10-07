//! Nouvelle version : une requête à l'API de GitHub au plus une fois par jour, puis, sur
//! demande, téléchargement et installation. Rien d'autre ne sort de l'app, et elle ne dit
//! que « bref » comme nom de client.

use std::{
    ffi::OsStr,
    path::Path,
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::vault;

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
    Release { asset: release.asset.filter(|_| installable), ..release }
}

/// Télécharge le fichier de la release et l'installe. `Ok` : le nouveau programme se
/// relance de lui-même, l'app peut quitter. Bloquant : à lancer hors du thread UI.
pub fn install(url: &str) -> Result<(), String> {
    // Les tests ne sortent pas sur le réseau et n'installent rien.
    if cfg!(test) {
        return Err("no installation in tests".into());
    }
    let dir = std::env::temp_dir().join("bref-update");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let name = url.rsplit('/').next().filter(|n| !n.is_empty()).unwrap_or("bref-update");
    let file = dir.join(name);
    run(command("curl").args(["-fL", "--max-time", "900", "-A", "bref", "-o"]).arg(&file).arg(url))?;
    apply(&file)
}

// ponytail: `curl` (livré avec macOS, Windows 10 et la plupart des Linux) plutôt qu'une
// bibliothèque HTTP et TLS en dépendance ; sans lui, l'app ne signale simplement rien.
// Aucune somme de contrôle ni signature : on fait confiance à HTTPS et au compte GitHub,
// comme pour un téléchargement à la main. Signer les versions (clé minisign) si l'app grandit.
fn fetch() -> Option<Release> {
    let out = command("curl")
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
    let asset = asset.and_then(|name| {
        release["assets"]
            .as_array()?
            .iter()
            .find(|a| a["name"] == name)
            .and_then(|a| a["browser_download_url"].as_str())
            .map(String::from)
    });
    Some(Release { version, asset })
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
/// version trouvée, puis l'adresse de son fichier. Illisible ou absent : jamais contrôlé.
fn parse_state(text: &str) -> (u64, Release) {
    let mut lines = text.lines().map(str::trim);
    let at = lines.next().and_then(|l| l.parse().ok()).unwrap_or(0);
    let version = lines.next().unwrap_or_default().to_string();
    let asset = lines.next().filter(|l| !l.is_empty()).map(String::from);
    (at, Release { version, asset })
}

fn state_text(at: u64, release: &Release) -> String {
    format!("{at}\n{}\n{}\n", release.version, release.asset.as_deref().unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    const JSON: &str = r#"{"tag_name": "v0.2.0", "draft": false, "prerelease": false, "assets": [
        {"name": "bref-x86_64.pkg.tar.zst", "browser_download_url": "https://x/bref-x86_64.pkg.tar.zst"},
        {"name": "bref-macos.dmg", "browser_download_url": "https://x/bref-macos.dmg"}]}"#;

    #[test]
    fn reads_the_published_release() {
        let release = |asset| release_of(JSON, asset).unwrap();
        assert_eq!(release(None), Release { version: "0.2.0".into(), asset: None });
        assert_eq!(release(Some("bref-macos.dmg")).asset.as_deref(), Some("https://x/bref-macos.dmg"));
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
        let found = Release { version: "0.2.0".into(), asset: Some("https://x/a.dmg".into()) };
        assert_eq!(only_if_installable(found.clone(), true), found);
        assert_eq!(only_if_installable(found, false), Release { version: "0.2.0".into(), asset: None });
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
        let found = Release { version: "0.2.0".into(), asset: Some("https://x/a.dmg".into()) };
        assert_eq!(parse_state(&state_text(1_730_000_000, &found)), (1_730_000_000, found));
        let bare = Release { version: "0.2.0".into(), asset: None };
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
