//! The daily note: the name of a day, and which template a vault uses for it.
//! Finding the file is the shell's; saying which names count is pure.

use crate::lang::fold;

/// File names (without extension, accents folded) of the template a daily note is born from.
pub const NAMES: [&str; 5] = ["daily", "journal", "today", "note du jour", "aujourd'hui"];

/// Whether a template file of this name is the daily note's template.
pub fn is_daily_name(stem: &str) -> bool {
    NAMES.contains(&fold(stem).as_str())
}

/// The daily template among these `(path, file name)` pairs, the first whose name counts.
pub fn pick<'a, T>(templates: impl IntoIterator<Item = (T, &'a str)>) -> Option<T> {
    templates
        .into_iter()
        .find(|(_, stem)| is_daily_name(stem))
        .map(|(item, _)| item)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_the_daily_template_by_its_name() {
        assert!(is_daily_name("Daily") && is_daily_name("Journal") && is_daily_name("Aujourd'hui"));
        assert!(is_daily_name("Note du jour") && is_daily_name("Today"));
        assert!(!is_daily_name("Réunion") && !is_daily_name("Notes du jour"));
        let paths = ["/v/Templates/Réunion.md", "/v/Templates/Journal.md"];
        let found = pick(
            paths
                .iter()
                .map(|p| (*p, p.rsplit(['/', '.']).nth(1).unwrap())),
        );
        assert_eq!(found, Some("/v/Templates/Journal.md"));
        assert!(pick([("/v/Templates/Réunion.md", "Réunion")]).is_none());
    }
}
