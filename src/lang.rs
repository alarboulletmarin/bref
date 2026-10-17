//! Language: interface text in English or French (`tr`), how the system's language is detected,
//! and how dates, times and search keys are written. Used by every layer.

use std::sync::OnceLock;

/// Interface text: French if the system's language is, English otherwise.
pub fn tr(en: &'static str, fr: &'static str) -> &'static str {
    static FRENCH: OnceLock<bool> = OnceLock::new();
    // Tests run in French to stay deterministic.
    if *FRENCH.get_or_init(|| cfg!(test) || system_is_french()) { fr } else { en }
}

/// Locale variables first (launched from a terminal), otherwise the system:
/// an application launched from the Finder or the Start menu gets none.
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
    // A LANGID: the 10 low bits give the language (0x0c: French).
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

// ponytail: on Linux, only environment variables count (the system
// offers nothing else); on macOS the language comes from a call to `defaults` at startup.
#[cfg(not(any(windows, target_os = "macos")))]
fn os_language_is_french() -> bool {
    false
}

/// First language of the list that `defaults read -g AppleLanguages` prints:
/// `(\n    "fr-FR",\n    "en-US"\n)`, with or without quotes.
#[cfg(any(target_os = "macos", test))]
fn first_language(list: &str) -> Option<&str> {
    list.lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && *l != "(" && *l != ")")
        .map(|l| l.trim_end_matches(',').trim_matches('"'))
}

/// `09:05`: the time as written in a template.
pub fn time_name((hour, minute): (u32, u32)) -> String {
    format!("{hour:02}:{minute:02}")
}

/// `2026-10-09`: the date as written in a note and naming the daily note.
pub fn date_name((year, month, day): (i32, u32, u32)) -> String {
    format!("{year:04}-{month:02}-{day:02}")
}
/// Text reduced to what search compares: lowercase, without accents.
// ponytail: only French accents; switch to Unicode normalisation if another
// language arrives in `tr`.
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
        // What `defaults read -g AppleLanguages` prints, with and without quotes.
        assert_eq!(first_language("(\n    \"fr-FR\",\n    \"en-US\"\n)\n"), Some("fr-FR"));
        assert_eq!(first_language("(\n    en,\n    fr\n)"), Some("en"));
        assert_eq!(first_language("(\n)"), None);
        assert_eq!(first_language(""), None);
    }
}
