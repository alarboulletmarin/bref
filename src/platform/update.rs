//! New version: a request to GitHub's API at most once a day, then, on
//! request, download and install. Nothing else leaves the app, and it gives
//! only "bref" as its client name.

use std::{
    fs::{DirBuilder, File},
    io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use sha2::{Digest, Sha256};

use crate::{lang::tr, platform::vault};
use crate::platform::command;

const DAY: u64 = 24 * 3600;
/// A release's files arrive a quarter of an hour after it: check again sooner.
const HOUR: u64 = 3600;

/// A version newer than the one running.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Release {
    pub version: String,
    /// URL of the file to install for this system and this installation; `None` if the app
    /// cannot update itself (or if the file is not published yet).
    pub asset: Option<String>,
    /// SHA-256 sum (hex) of this file, as GitHub publishes it with the release.
    pub sha256: Option<String>,
}

/// Page to download the latest version from.
pub fn releases_url() -> String {
    format!("{}/releases/latest", env!("CARGO_PKG_REPOSITORY"))
}

/// URL of the latest release. `BREF_RELEASES_API` replaces it: update tests
/// point it to a local file (`file://`) describing a release.
fn api_url() -> String {
    std::env::var("BREF_RELEASES_API").unwrap_or_else(|_| {
        let repo = env!("CARGO_PKG_REPOSITORY").replace("https://github.com/", "https://api.github.com/repos/");
        format!("{repo}/releases/latest")
    })
}

/// `BREF_RELEASES_API` is a test setting: it lifts the URL and sum checks.
fn overridden() -> bool {
    std::env::var_os("BREF_RELEASES_API").is_some()
}

/// The file to install only comes from this repository's releases, and over HTTPS end to end
/// (`curl` follows redirects: without `--proto-redir`, an `http://` URL would get through).
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

/// Version newer than the one running, if GitHub knows one. The result of the
/// last check is kept in the config's `update` file: within 24 h, it is
/// reread instead of redoing the request. Blocking: run off the UI thread.
pub fn check() -> Option<Release> {
    let now = now();
    let (at, known) = parse_state(&vault::load_file("update"));
    let installable = asset_name().is_some();
    let waiting_for_files = !known.version.is_empty() && known.asset.is_none() && installable;
    let latest = if at <= now && now - at < if waiting_for_files { HOUR } else { DAY } {
        known
    } else if let Some(found) = fetch() {
        vault::save_file("update", &state_text(now, &found));
        found
    } else {
        // Offline, or without `curl`: nothing is recorded, the next start tries again.
        known
    };
    is_new(&latest).then(|| only_if_installable(latest, installable))
}

/// Check asked by hand: the latest published version, without waiting for the 24 h (`None`:
/// GitHub did not answer). It is recorded like an ordinary check. Blocking: run
/// off the UI thread.
pub fn check_now() -> Option<Release> {
    let found = fetch()?;
    vault::save_file("update", &state_text(now(), &found));
    Some(found)
}

/// This published version is newer than the one running.
pub fn is_new(release: &Release) -> bool {
    newer(env!("CARGO_PKG_VERSION"), &release.version)
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// The check kept in the `update` file can come from another installation on the same
/// machine (the zip next to the installer, `bref-git` after the package): its file to install
/// only counts if this one knows how to use it.
fn only_if_installable(release: Release, installable: bool) -> Release {
    if installable { release } else { Release { asset: None, sha256: None, ..release } }
}

/// Downloads the release's file, checks its sum and installs it. `Ok`: the new
/// program relaunches itself, the app can quit. Blocking: run off the UI thread.
pub fn install(url: &str, sha256: Option<&str>) -> Result<(), String> {
    // Tests do not go on the network and install nothing.
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
    // A file refused or whose installation failed does not stay in the temporary folder;
    // after a success, it stays: the installer still reads it once the app has quit.
    if installed.is_err() {
        let _ = std::fs::remove_dir_all(&dir);
    }
    installed
}

/// A new folder, ours alone (`0700`), in the temporary folder. The name is not
/// predictable and creation fails if it exists: on Linux, `/tmp` is shared, and another
/// user could otherwise replace the file before `pkexec` installs it.
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

/// The SHA-256 sum of the downloaded file is the one GitHub published.
fn verify(file: &Path, expected: &str) -> Result<(), String> {
    let mut hasher = Sha256::new();
    io::copy(&mut File::open(file).map_err(|e| e.to_string())?, &mut hasher).map_err(|e| e.to_string())?;
    if format!("{:x}", hasher.finalize()).eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(tr("the download does not match its checksum", "le fichier téléchargé ne correspond pas à sa somme de contrôle").into())
    }
}

// ponytail: `curl` (shipped with macOS, Windows 10 and most Linuxes) rather than an
// HTTP and TLS library as a dependency; without it, the app simply reports nothing.
// The file is checked against the SHA-256 sum GitHub publishes with the release: it protects
// the download, not the GitHub account itself. Sign the versions (minisign key) if the app grows.
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

/// Runs the command and waits; on failure, the last line of its error message.
fn run(command: &mut Command) -> Result<(), String> {
    let out = command.stdin(Stdio::null()).output().map_err(|e| e.to_string())?;
    if out.status.success() {
        return Ok(());
    }
    let message = String::from_utf8_lossy(&out.stderr);
    Err(message.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("failed").trim().to_string())
}

/// Relaunches `program` once this process has quit.
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

/// Windows: the version's installer, silent. It replaces `bref.exe` (closing it
/// if still running) then relaunches the app (`/RELAUNCH=1`, read by `packaging/windows/bref.iss`).
#[cfg(windows)]
fn apply(file: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    Command::new(file)
        .args(["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/CLOSEAPPLICATIONS", "/RELAUNCH=1"])
        // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP: the installer outlives the app that launched it.
        .creation_flags(0x0000_0208)
        .stdin(Stdio::null())
        .spawn()
        .map(drop)
        .map_err(|e| e.to_string())
}

/// Only an app installed by its installer (which leaves `unins000.exe` beside it) can
/// update this way, not the zip one nor the `cargo install` one.
#[cfg(windows)]
fn asset_name() -> Option<&'static str> {
    let exe = std::env::current_exe().ok()?;
    exe.with_file_name("unins000.exe").is_file().then_some("bref-windows-x86_64-setup.exe")
}

/// The `.app` bundle that contains this program.
#[cfg(target_os = "macos")]
fn app_bundle() -> Option<std::path::PathBuf> {
    std::env::current_exe().ok()?.ancestors().find(|p| p.extension().is_some_and(|e| e == "app")).map(Path::to_path_buf)
}

#[cfg(target_os = "macos")]
fn asset_name() -> Option<&'static str> {
    app_bundle().map(|_| "bref-macos.dmg")
}

/// macOS: replaces `Bref.app` with the one from the disk image, then relaunches the app.
#[cfg(target_os = "macos")]
fn apply(file: &Path) -> Result<(), String> {
    let app = app_bundle().ok_or("not running from an app bundle")?;
    replace_app(file, &app)?;
    relaunch("open", &[&app])
}

/// Copies `Bref.app` from the disk image next to `app` (same volume, so the rename is
/// atomic), then swaps it with the old one. On failure, the old one stays in place. The
/// running program keeps running: its file is not deleted while it holds it open.
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

/// Linux: only the release's Arch package (`bref`, installed in `/usr/bin`) is updated,
/// by pacman; other installations (built, `bref-git`) have no file to install.
#[cfg(target_os = "linux")]
fn asset_name() -> Option<&'static str> {
    let owned = || {
        let owner = Command::new("pacman").args(["-Qqo", "/usr/bin/bref"]).output().ok()?;
        (String::from_utf8_lossy(&owner.stdout).trim() == "bref").then_some(())
    };
    let here = std::env::current_exe().ok().is_some_and(|p| p == Path::new("/usr/bin/bref"));
    (cfg!(target_arch = "x86_64") && here && owned().is_some()).then_some("bref-x86_64.pkg.tar.zst")
}

/// Linux: `pkexec` asks for the password in a desktop window, then pacman installs.
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

/// The published release: its number (`v0.2.0` gives `0.2.0`) and the URL of the file named
/// `asset`, if it is there. Nothing for a draft or a prerelease.
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

/// `latest` is an `X.Y.Z` number strictly greater than `current`.
fn newer(current: &str, latest: &str) -> bool {
    let parts = |v: &str| -> Option<Vec<u64>> {
        let parts: Vec<u64> = v.trim().split('.').map(|p| p.parse().ok()).collect::<Option<_>>()?;
        (parts.len() == 3).then_some(parts)
    };
    matches!((parts(current), parts(latest)), (Some(c), Some(l)) if l > c)
}

/// Content of the `update` file: the time of the last check (seconds since 1970), the
/// version found, the URL of its file, then its sum. Unreadable or missing: never checked.
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
        // No published sum: nothing to compare, installation will be refused.
        assert_eq!(release(Some("bref-x86_64.pkg.tar.zst")).sha256, None);
        // This system's file is not (yet) published.
        assert_eq!(release(Some("bref-windows-x86_64-setup.exe")).asset, None);
        assert_eq!(release_of(r#"{"tag_name": "0.3.1"}"#, Some("a")).unwrap().version, "0.3.1");
        assert_eq!(release_of(r#"{"tag_name": "v0.3.0-rc1", "prerelease": true}"#, None), None);
        assert_eq!(release_of(r#"{"tag_name": "v0.3.0", "draft": true}"#, None), None);
        // Error response or gibberish: nothing.
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
        // SHA-256 of "abc".
        let abc = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert!(verify(&file, abc).is_ok());
        assert!(verify(&file, &abc.to_uppercase()).is_ok());
        assert!(verify(&file, "00").is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The download folder is new and closed to other users.
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
        // The old format (without URL), then empty or unreadable: never checked.
        assert_eq!(parse_state("1730000000\n0.2.0\n").1.version, "0.2.0");
        assert_eq!(parse_state(""), (0, Release::default()));
        assert_eq!(parse_state("pas un nombre\n0.2.0").0, 0);
    }

    /// On a real macOS: the disk image replaces the installed app, and nothing is left behind.
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
        // An unreadable disk image: the installed app stays as it is.
        fs::write(dir.join("bad.dmg"), "pas un disque").unwrap();
        assert!(replace_app(&dir.join("bad.dmg"), &installed).is_err());
        assert_eq!(fs::read_to_string(program(&dir.join("Applications"))).unwrap(), "nouvelle");
        fs::remove_dir_all(&dir).unwrap();
    }
}
