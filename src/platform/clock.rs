//! Local date and time. The standard library only knows UTC.

/// Date du jour à l'heure locale : année, mois, jour. La bibliothèque standard ne donne que l'UTC.
#[cfg(unix)]
pub fn today() -> (i32, u32, u32) {
    // SAFETY: `time` accepte un pointeur nul ; `localtime_r` n'écrit que dans `tm`, qui est à nous.
    let tm = unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&now, &mut tm);
        tm
    };
    (tm.tm_year + 1900, tm.tm_mon as u32 + 1, tm.tm_mday as u32)
}

/// L'heure locale, `09:05` : heures et minutes.
#[cfg(unix)]
pub fn clock() -> (u32, u32) {
    // SAFETY: comme `today`, `localtime_r` n'écrit que dans `tm`, qui est à nous.
    let tm = unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&now, &mut tm);
        tm
    };
    (tm.tm_hour as u32, tm.tm_min as u32)
}

#[cfg(windows)]
pub fn clock() -> (u32, u32) {
    #[repr(C)]
    #[derive(Default)]
    #[allow(dead_code)]
    struct Local {
        year: u16,
        month: u16,
        day_of_week: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        millis: u16,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetLocalTime(time: *mut Local);
    }
    let mut now = Local::default();
    // SAFETY: `GetLocalTime` remplit la structure qu'on lui donne, et n'échoue pas.
    unsafe { GetLocalTime(&mut now) };
    (now.hour as u32, now.minute as u32)
}

#[cfg(windows)]
pub fn today() -> (i32, u32, u32) {
    /// `SYSTEMTIME` de l'API Windows : huit mots de 16 bits.
    #[repr(C)]
    #[derive(Default)]
    #[allow(dead_code)]
    struct Local {
        year: u16,
        month: u16,
        day_of_week: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        millis: u16,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetLocalTime(time: *mut Local);
    }
    let mut now = Local::default();
    // SAFETY: `GetLocalTime` remplit la structure qu'on lui donne, et n'échoue pas.
    unsafe { GetLocalTime(&mut now) };
    (now.year as i32, now.month as u32, now.day as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::date_name;

    #[test]
    fn names_the_day() {
        assert_eq!(date_name((2026, 1, 2)), "2026-01-02");
        let (year, month, day) = today();
        assert!(year >= 2026 && (1..=12).contains(&month) && (1..=31).contains(&day));
        // La date locale est celle que donne le système (à minuit près, entre les deux lectures).
        #[cfg(unix)]
        if let Ok(out) = std::process::Command::new("date").arg("+%F").output() {
            let system = String::from_utf8_lossy(&out.stdout).trim().to_string();
            assert!(system == date_name((year, month, day)) || system == date_name(today()));
        }
    }
}
