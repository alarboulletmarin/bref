//! Local date and time. The standard library only knows UTC.

/// Today's date in local time: year, month, day. The standard library only gives UTC.
#[cfg(unix)]
pub fn today() -> (i32, u32, u32) {
        // SAFETY: `time` accepts a null pointer; `localtime_r` only writes into `tm`, which is ours.
    let tm = unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&now, &mut tm);
        tm
    };
    (tm.tm_year + 1900, tm.tm_mon as u32 + 1, tm.tm_mday as u32)
}

/// Local time, `09:05`: hours and minutes.
#[cfg(unix)]
pub fn clock() -> (u32, u32) {
        // SAFETY: like `today`, `localtime_r` only writes into `tm`, which is ours.
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
        // SAFETY: `GetLocalTime` fills the structure it is given, and does not fail.
    unsafe { GetLocalTime(&mut now) };
    (now.hour as u32, now.minute as u32)
}

#[cfg(windows)]
pub fn today() -> (i32, u32, u32) {
        /// `SYSTEMTIME` of the Windows API: eight 16-bit words.
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
        // SAFETY: `GetLocalTime` fills the structure it is given, and does not fail.
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
                // The local date is the one the system gives (give or take midnight, between the two reads).
        #[cfg(unix)]
        if let Ok(out) = std::process::Command::new("date").arg("+%F").output() {
            let system = String::from_utf8_lossy(&out.stdout).trim().to_string();
            assert!(system == date_name((year, month, day)) || system == date_name(today()));
        }
    }
}
