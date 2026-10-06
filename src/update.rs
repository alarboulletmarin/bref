//! Nouvelle version : une requête à l'API de GitHub au plus une fois par jour, la seule
//! que l'app fasse. Elle ne dit rien d'autre que « bref » comme nom de client.

use std::{
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::vault;

const DAY: u64 = 24 * 3600;

/// Page où télécharger la dernière version.
pub fn releases_url() -> String {
    format!("{}/releases/latest", env!("CARGO_PKG_REPOSITORY"))
}

/// Version plus récente que celle qui tourne, si GitHub en connaît une. Le résultat du
/// dernier contrôle (heure, version) est gardé dans le fichier `update` de la config :
/// avant 24 h, il est relu au lieu de refaire la requête. Bloquant : à lancer hors du thread UI.
pub fn check() -> Option<String> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (at, known) = parse_state(&vault::load_file("update"));
    let latest = if at <= now && now - at < DAY {
        known
    } else if let Some(found) = fetch() {
        vault::save_file("update", &format!("{now}\n{found}\n"));
        found
    } else {
        // Hors ligne, ou sans `curl` : rien n'est noté, le prochain démarrage réessaie.
        known
    };
    newer(env!("CARGO_PKG_VERSION"), &latest).then_some(latest)
}

// ponytail: `curl` (livré avec macOS, Windows 10 et la plupart des Linux) plutôt qu'une
// bibliothèque HTTP et TLS en dépendance ; sans lui, l'app ne signale simplement rien.
fn fetch() -> Option<String> {
    let url = env!("CARGO_PKG_REPOSITORY").replace("https://github.com/", "https://api.github.com/repos/");
    let mut curl = Command::new("curl");
    curl.args(["-fsSL", "--max-time", "10", "-A", "bref", "-H", "Accept: application/vnd.github+json"])
        .arg(format!("{url}/releases/latest"))
        .stdin(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        // Sans cela, une fenêtre de console clignote : l'app n'en a pas.
        use std::os::windows::process::CommandExt;
        curl.creation_flags(0x0800_0000);
    }
    let out = curl.output().ok().filter(|o| o.status.success())?;
    tag_of(&String::from_utf8_lossy(&out.stdout))
}

/// Numéro de la version publiée (`v0.2.0` donne `0.2.0`) ; rien pour un brouillon ou une préversion.
fn tag_of(json: &str) -> Option<String> {
    let release: serde_json::Value = serde_json::from_str(json).ok()?;
    let flag = |key| release[key].as_bool().unwrap_or(false);
    if flag("draft") || flag("prerelease") {
        return None;
    }
    Some(release["tag_name"].as_str()?.trim_start_matches('v').to_string())
}

/// `latest` est un numéro `X.Y.Z` strictement plus grand que `current`.
fn newer(current: &str, latest: &str) -> bool {
    let parts = |v: &str| -> Option<Vec<u64>> {
        let parts: Vec<u64> = v.trim().split('.').map(|p| p.parse().ok()).collect::<Option<_>>()?;
        (parts.len() == 3).then_some(parts)
    };
    matches!((parts(current), parts(latest)), (Some(c), Some(l)) if l > c)
}

/// Contenu du fichier `update` : l'heure du dernier contrôle (secondes depuis 1970),
/// puis la version trouvée. Illisible ou absent : jamais contrôlé.
fn parse_state(text: &str) -> (u64, String) {
    let mut lines = text.lines();
    let at = lines.next().and_then(|l| l.trim().parse().ok()).unwrap_or(0);
    (at, lines.next().unwrap_or_default().trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_published_version() {
        assert_eq!(tag_of(r#"{"tag_name": "v0.2.0", "draft": false, "prerelease": false}"#), Some("0.2.0".into()));
        assert_eq!(tag_of(r#"{"tag_name": "0.3.1"}"#), Some("0.3.1".into()));
        assert_eq!(tag_of(r#"{"tag_name": "v0.3.0-rc1", "prerelease": true}"#), None);
        assert_eq!(tag_of(r#"{"tag_name": "v0.3.0", "draft": true}"#), None);
        // Réponse d'erreur ou charabia : rien.
        assert_eq!(tag_of(r#"{"message": "Not Found"}"#), None);
        assert_eq!(tag_of("<html>"), None);
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
        assert_eq!(parse_state("1730000000\n0.2.0\n"), (1_730_000_000, "0.2.0".into()));
        assert_eq!(parse_state("1730000000\n"), (1_730_000_000, String::new()));
        assert_eq!(parse_state(""), (0, String::new()));
        assert_eq!(parse_state("pas un nombre\n0.2.0"), (0, "0.2.0".into()));
    }
}
