//! Local date and time. The standard library only knows UTC.

/// The local date and time now: year, month, day, hour, minute.
#[cfg(unix)]
fn local() -> (i32, u32, u32, u32, u32) {
    // SAFETY: `time` accepts a null pointer; `localtime_r` only writes into `tm`, which is ours.
    let tm = unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&now, &mut tm);
        tm
    };
    (tm.tm_year + 1900, tm.tm_mon as u32 + 1, tm.tm_mday as u32, tm.tm_hour as u32, tm.tm_min as u32)
}

/// The local date and time now: year, month, day, hour, minute.
#[cfg(windows)]
fn local() -> (i32, u32, u32, u32, u32) {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        /// Fills a `SYSTEMTIME`: eight 16-bit words, year, month, day of the week, day, hour, minute,
        /// second, milliseconds.
        fn GetLocalTime(time: *mut [u16; 8]);
    }
    let mut now = [0u16; 8];
    // SAFETY: `GetLocalTime` fills the structure it is given, and does not fail.
    unsafe { GetLocalTime(&mut now) };
    (now[0] as i32, now[1] as u32, now[3] as u32, now[4] as u32, now[5] as u32)
}

/// Today's date in local time: year, month, day.
pub fn today() -> (i32, u32, u32) {
    let (year, month, day, _, _) = local();
    (year, month, day)
}

/// Local time, `09:05`: hours and minutes.
pub fn clock() -> (u32, u32) {
    let (_, _, _, hour, minute) = local();
    (hour, minute)
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
        // The local date is the one the system gives (give or take midnight, between the two reads).
        #[cfg(unix)]
        if let Ok(out) = std::process::Command::new("date").arg("+%F").output() {
            let system = String::from_utf8_lossy(&out.stdout).trim().to_string();
            assert!(system == date_name((year, month, day)) || system == date_name(today()));
        }
    }
}
