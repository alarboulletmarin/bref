//! Dates: day numbers, months of the calendar, and the dates offered after `@`.

/// Year, month, day.
pub type Date = (i32, u32, u32);

/// Number of days between January 1st 1970 and this Gregorian calendar date.
pub fn day_number((year, month, day): Date) -> i64 {
    let y = i64::from(year) - i64::from(month <= 2);
    let era = y.div_euclid(400);
    let of_era = y.rem_euclid(400);
    let of_year = (153 * ((i64::from(month) + 9) % 12) + 2) / 5 + i64::from(day) - 1;
    era * 146_097 + of_era * 365 + of_era / 4 - of_era / 100 + of_year - 719_468
}

/// The date of day number `days`: the inverse of `day_number`.
pub fn day_of(days: i64) -> Date {
    let z = days + 719_468;
    let (era, of_era) = (z.div_euclid(146_097), z.rem_euclid(146_097));
    let year_of_era = (of_era - of_era / 1_460 + of_era / 36_524 - of_era / 146_096) / 365;
    let of_year = of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted = (5 * of_year + 2) / 153;
    let month = (shifted + 2) % 12 + 1;
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year as i32, month as u32, (of_year - (153 * shifted + 2) / 5 + 1) as u32)
}

/// The month of `year`: the rank in the week of its first day (0: the first day of the
/// week, a Monday if `monday`, otherwise a Sunday) and its number of days.
pub fn month(year: i32, month: u32, monday: bool) -> (u32, u32) {
    let first = day_number((year, month, 1));
    let next = if month == 12 { (year + 1, 1, 1) } else { (year, month + 1, 1) };
        // January 1st 1970 was a Thursday: Monday is 0.
    let weekday = (first + 3 + i64::from(!monday)).rem_euclid(7);
    (weekday as u32, (day_number(next) - first) as u32)
}

/// The same day `by` months later (or earlier), brought back into the target month.
pub fn add_months((year, month_, day): Date, by: i32) -> Date {
    let months = year * 12 + month_ as i32 - 1 + by;
    let (year, month_) = (months.div_euclid(12), months.rem_euclid(12) as u32 + 1);
    (year, month_, day.min(month(year, month_, true).1))
}

/// The days named after `@`: (English, French), Monday to Sunday after the three
/// relative days.
pub const DAYS: [(&str, &str); 10] = [
    ("today", "aujourd'hui"),
    ("tomorrow", "demain"),
    ("yesterday", "hier"),
    ("monday", "lundi"),
    ("tuesday", "mardi"),
    ("wednesday", "mercredi"),
    ("thursday", "jeudi"),
    ("friday", "vendredi"),
    ("saturday", "samedi"),
    ("sunday", "dimanche"),
];

/// The months: (English, French).
pub const MONTHS: [(&str, &str); 12] = [
    ("January", "janvier"), ("February", "février"), ("March", "mars"), ("April", "avril"), ("May", "mai"), ("June", "juin"),
    ("July", "juillet"), ("August", "août"), ("September", "septembre"), ("October", "octobre"), ("November", "novembre"), ("December", "décembre"),
];

/// The dates `query` (what follows an `@`) can name from `today`: the days whose
/// English or French name starts that way, with their date, or the written date `2026-10-09`.
/// A weekday is the next one, never today.
pub fn dates(query: &str, today: Date) -> Vec<(&'static str, &'static str, Date)> {
    let query = query.to_lowercase().replace('’', "'");
    let now = day_number(today);
    let typed = || {
        let mut parts = query.split('-').map(|p| p.parse::<u32>().ok());
        let date = (parts.next()??.try_into().ok()?, parts.next()??, parts.next()??);
                // A date that exists: it survives the round trip.
        (parts.next().is_none() && query.len() == 10 && day_of(day_number(date)) == date).then_some(("", "", date))
    };
    let named = DAYS.iter().enumerate().filter(|(_, (en, fr))| en.starts_with(&query) || fr.starts_with(&query));
    let named = named.map(|(i, &(en, fr))| {
        let ahead = match i {
            0 => 0,
            1 => 1,
            2 => -1,
                        // January 1st 1970 was a Thursday: Monday is 0.
            _ => (i as i64 - 3 - (now + 3).rem_euclid(7) - 1).rem_euclid(7) + 1,
        };
        (en, fr, day_of(now + ahead))
    });
    typed().into_iter().chain(named).collect()
}
