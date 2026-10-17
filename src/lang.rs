//! Language: interface text in English or French (`tr`), how the system's language is detected,
//! and how dates, times and search keys are written. Used by every layer.

use std::sync::OnceLock;

/// Texte d'interface : français si la langue du système l'est, anglais sinon.
pub fn tr(en: &'static str, fr: &'static str) -> &'static str {
    static FRENCH: OnceLock<bool> = OnceLock::new();
    // Les tests tournent en français pour rester déterministes.
    if *FRENCH.get_or_init(|| cfg!(test) || system_is_french()) { fr } else { en }
}

/// Les variables de locale d'abord (lancement depuis un terminal), sinon le système :
/// une application lancée depuis le Finder ou le menu Démarrer n'en reçoit aucune.
fn system_is_french() -> bool {
    match ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|key| std::env::var(key).ok().filter(|v| !v.is_empty()))
    {
        Some(lang) => lang.starts_with("fr"),
        None => os_language_is_french(),
    }
}

#[cfg(windows)]
fn os_language_is_french() -> bool {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetUserDefaultUILanguage() -> u16;
    }
    // Un LANGID : les 10 bits de poids faible donnent la langue (0x0c : français).
    (unsafe { GetUserDefaultUILanguage() } & 0x3ff) == 0x0c
}

#[cfg(target_os = "macos")]
fn os_language_is_french() -> bool {
    std::process::Command::new("defaults")
        .args(["read", "-g", "AppleLanguages"])
        .output()
        .ok()
        .and_then(|out| first_language(&String::from_utf8_lossy(&out.stdout)).map(|l| l.starts_with("fr")))
        .unwrap_or(false)
}

// ponytail: sous Linux, seules les variables d'environnement comptent (le système
// n'en offre pas d'autre) ; sous macOS la langue vient d'un appel à `defaults` au démarrage.
#[cfg(not(any(windows, target_os = "macos")))]
fn os_language_is_french() -> bool {
    false
}

/// Première langue de la liste que `defaults read -g AppleLanguages` imprime :
/// `(\n    "fr-FR",\n    "en-US"\n)`, avec ou sans guillemets.
#[cfg(any(target_os = "macos", test))]
fn first_language(list: &str) -> Option<&str> {
    list.lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && *l != "(" && *l != ")")
        .map(|l| l.trim_end_matches(',').trim_matches('"'))
}

/// `09:05` : l'heure telle qu'elle s'écrit dans un modèle.
pub fn time_name((hour, minute): (u32, u32)) -> String {
    format!("{hour:02}:{minute:02}")
}

/// `2026-10-09` : la date telle qu'elle s'écrit dans une note et nomme la note du jour.
pub fn date_name((year, month, day): (i32, u32, u32)) -> String {
    format!("{year:04}-{month:02}-{day:02}")
}
/// Texte ramené à ce que la recherche compare : minuscules, sans accents.
// ponytail: les seuls accents du français ; passer à une normalisation Unicode si une autre
// langue arrive dans `tr`.
pub fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars().flat_map(char::to_lowercase) {
        match c {
            'à' | 'â' | 'ä' => out.push('a'),
            'é' | 'è' | 'ê' | 'ë' => out.push('e'),
            'î' | 'ï' => out.push('i'),
            'ô' | 'ö' => out.push('o'),
            'ù' | 'û' | 'ü' => out.push('u'),
            'ç' => out.push('c'),
            'œ' => out.push_str("oe"),
            '’' => out.push('\''),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_first_system_language() {
        // Ce que `defaults read -g AppleLanguages` imprime, avec et sans guillemets.
        assert_eq!(first_language("(\n    \"fr-FR\",\n    \"en-US\"\n)\n"), Some("fr-FR"));
        assert_eq!(first_language("(\n    en,\n    fr\n)"), Some("en"));
        assert_eq!(first_language("(\n)"), None);
        assert_eq!(first_language(""), None);
    }
}
