//! The empty page: what the vault brings back to view while nothing is written (pure functions).

use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use crate::logic::markdown::{self, Date};
use crate::logic::note::Note;
use crate::{lang::date_name, lang::tr};

/// A note untouched for this long to be offered for rereading.
const OLD: i64 = 30;

/// A note to reread, and its age in days.
#[derive(Clone, Debug, PartialEq)]
pub struct Recalled {
    pub path: PathBuf,
    pub name: String,
    pub days: i64,
}

/// The notes to reread `today`: the daily notes from a week, a month and a year ago, when
/// they exist, then a note untouched for a long time. That one is drawn at
/// random from the date: the same all day, another the next day.
pub fn recall(notes: &[Note], today: Date, now: SystemTime) -> Vec<Recalled> {
    let day = markdown::day_number(today);
    let past = [markdown::day_of(day - 7), markdown::add_months(today, -1), markdown::add_months(today, -12)];
    let mut found: Vec<Recalled> = past
        .iter()
        .filter_map(|&date| {
            let name = date_name(date);
            let note = notes.iter().find(|n| n.name == name)?;
            Some(Recalled { path: note.path.clone(), name, days: day - markdown::day_number(date) })
        })
        .collect();
    let age = |note: &Note| now.duration_since(note.mtime).unwrap_or(Duration::ZERO).as_secs() as i64 / 86_400;
    let draw = |note: &Note| {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        (&note.name, day).hash(&mut hasher);
        hasher.finish()
    };
    let old = notes
        .iter()
        .filter(|n| age(n) >= OLD && !n.body.trim().is_empty() && found.iter().all(|f| f.path != n.path))
        .min_by_key(|n| draw(n));
    found.extend(old.map(|n| Recalled { path: n.path.clone(), name: n.name.clone(), days: age(n) }));
    found
}

/// « il y a 3 mois » ("3 months ago"): an age in days, told in the largest unit that fits.
pub fn ago(days: i64) -> String {
    let said = |n: i64, one: (&'static str, &'static str), many: (&'static str, &'static str)| match n {
        1 => tr(one.0, one.1).to_string(),
        _ => tr(many.0, many.1).replace("{}", &n.to_string()),
    };
    match days {
        ..=0 => tr("today", "aujourd'hui").to_string(),
        1..7 => said(days, ("yesterday", "hier"), ("{} days ago", "il y a {} jours")),
        7..28 => said(days / 7, ("a week ago", "il y a une semaine"), ("{} weeks ago", "il y a {} semaines")),
        28..365 => said((days / 30).max(1), ("a month ago", "il y a un mois"), ("{} months ago", "il y a {} mois")),
        _ => said(days / 365, ("a year ago", "il y a un an"), ("{} years ago", "il y a {} ans")),
    }
}

/// « vendredi 10 octobre » ("Friday 10 October"): the date in words, without the year.
pub fn long_date(date: Date) -> String {
    let (_, month, day) = date;
    // 1 January 1970 was a Thursday: Monday is 0.
    let weekday = markdown::DAYS[3 + (markdown::day_number(date) + 3).rem_euclid(7) as usize];
    let months = markdown::MONTHS[month as usize - 1];
    if tr("en", "fr") == "fr" {
        return format!("{} {day}{} {}", weekday.1, if day == 1 { "er" } else { "" }, months.1);
    }
    let mut name = weekday.0.to_string();
    name[..1].make_ascii_uppercase();
    format!("{name}, {} {day}", months.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: Duration = Duration::from_secs(86_400);

    fn note(name: &str, age: u32, now: SystemTime) -> Note {
        Note {
            name: name.into(),
            path: PathBuf::from(format!("/coffre/{name}.md")),
            tags: Default::default(),
            links: Default::default(),
            aliases: Default::default(),
            mtime: now - DAY * age,
            body: "du texte".into(),
            lower: "du texte".into(),
        }
    }

    #[test]
    fn recall_brings_back_the_daily_notes_of_a_week_a_month_and_a_year_ago() {
        let now = SystemTime::now();
        let notes = [note("2026-10-03", 7, now), note("2025-10-10", 365, now), note("2026-09-10", 30, now), note("2026-10-09", 1, now)];
        let found = recall(&notes, (2026, 10, 10), now);
        let said: Vec<(&str, i64)> = found.iter().map(|r| (r.name.as_str(), r.days)).collect();
        assert_eq!(said, [("2026-10-03", 7), ("2026-09-10", 30), ("2025-10-10", 365)]);
    }

    #[test]
    fn recall_draws_one_old_note_and_keeps_it_for_the_day() {
        let now = SystemTime::now();
        let mut notes: Vec<Note> = (0..40).map(|i| note(&format!("Ancienne {i}"), 90 + i, now)).collect();
        // Neither a recent note nor an empty note is offered.
        notes.push(note("Récente", 3, now));
        notes.push(Note { body: "  \n".into(), ..note("Vide", 400, now) });
        let today = recall(&notes, (2026, 10, 10), now);
        assert_eq!(today.len(), 1);
        assert!(today[0].name.starts_with("Ancienne") && today[0].days >= 90, "{today:?}");
        // The same on every call of the day, whatever the order of the notes.
        notes.reverse();
        assert_eq!(recall(&notes, (2026, 10, 10), now), today);
        // From one day to the next the draw changes: over a month, several notes come out.
        let drawn: std::collections::HashSet<String> = (1..=30).map(|d| recall(&notes, (2026, 11, d), now)[0].name.clone()).collect();
        assert!(drawn.len() > 5, "{drawn:?}");
    }

    #[test]
    fn recall_is_empty_for_a_young_vault() {
        let now = SystemTime::now();
        assert!(recall(&[note("Hier", 1, now)], (2026, 10, 10), now).is_empty());
        assert!(recall(&[], (2026, 10, 10), now).is_empty());
    }

    #[test]
    fn ago_names_the_largest_unit() {
        let said: Vec<String> = [0, 1, 3, 7, 20, 28, 30, 75, 364, 365, 800].into_iter().map(ago).collect();
        assert_eq!(
            said,
            [
                "aujourd'hui", "hier", "il y a 3 jours", "il y a une semaine", "il y a 2 semaines", "il y a un mois", "il y a un mois",
                "il y a 2 mois", "il y a 12 mois", "il y a un an", "il y a 2 ans"
            ]
        );
    }

    #[test]
    fn long_date_spells_the_day() {
        assert_eq!(long_date((2026, 10, 10)), "samedi 10 octobre");
        assert_eq!(long_date((2026, 10, 1)), "jeudi 1er octobre");
        assert_eq!(long_date((1970, 1, 1)), "jeudi 1er janvier");
    }
}
