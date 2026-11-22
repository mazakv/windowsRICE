//! Local wall-clock time.

use std::time::Duration;

use windows::Win32::System::SystemInformation::GetLocalTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalTimeParts {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    /// 0 = Sunday.
    pub weekday: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub millisecond: u16,
}

pub fn local_time() -> LocalTimeParts {
    // SAFETY: no preconditions.
    let t = unsafe { GetLocalTime() };
    LocalTimeParts {
        year: t.wYear,
        month: t.wMonth as u8,
        day: t.wDay as u8,
        weekday: t.wDayOfWeek as u8,
        hour: t.wHour as u8,
        minute: t.wMinute as u8,
        second: t.wSecond as u8,
        millisecond: t.wMilliseconds,
    }
}

/// Time until the next minute boundary plus 20 ms, so a clock refreshes
/// exactly once per minute.
pub fn until_next_minute(t: &LocalTimeParts) -> Duration {
    let into = u64::from(t.second) * 1000 + u64::from(t.millisecond);
    Duration::from_millis(60_000 - into.min(59_999) + 20)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_minute() {
        let mut t = local_time();
        t.second = 59;
        t.millisecond = 990;
        assert_eq!(until_next_minute(&t), Duration::from_millis(30));
        t.second = 0;
        t.millisecond = 0;
        assert_eq!(until_next_minute(&t), Duration::from_millis(60_020));
    }
}

/// Current time in another Windows time zone (Phase 13 calendar), by its
/// Windows id such as `"Korea Standard Time"` or `"Pacific Standard Time"`:
/// (the zone's id, hour, minute). `None` when the zone is unknown.
/// Read-only: uses the time zone database Windows ships; nothing is set.
pub fn zone_time(id: &str) -> Option<(String, u8, u8)> {
    use windows::Win32::Foundation::{ERROR_SUCCESS, SYSTEMTIME};
    use windows::Win32::System::SystemInformation::GetSystemTime;
    use windows::Win32::System::Time::{
        DYNAMIC_TIME_ZONE_INFORMATION, EnumDynamicTimeZoneInformation,
        GetTimeZoneInformationForYear, SystemTimeToTzSpecificLocalTime, TIME_ZONE_INFORMATION,
    };
    let wanted = id.trim();
    if wanted.is_empty() || wanted.len() > 128 {
        return None;
    }
    let name_of = |w: &[u16]| {
        let n = w.iter().position(|c| *c == 0).unwrap_or(w.len());
        String::from_utf16_lossy(&w[..n])
    };
    // SAFETY: enumerates the time zone database into a local struct, then
    // converts the current UTC time with it; all buffers are locals.
    unsafe {
        let mut i = 0u32;
        loop {
            let mut d = DYNAMIC_TIME_ZONE_INFORMATION::default();
            if EnumDynamicTimeZoneInformation(i, &raw mut d) != ERROR_SUCCESS.0 {
                return None;
            }
            i += 1;
            if i > 512 {
                return None;
            }
            let key = name_of(&d.TimeZoneKeyName);
            if !key.eq_ignore_ascii_case(wanted) {
                continue;
            }
            let utc: SYSTEMTIME = GetSystemTime();
            let mut tzi = TIME_ZONE_INFORMATION::default();
            if GetTimeZoneInformationForYear(utc.wYear, Some(&raw const d), &raw mut tzi).is_err() {
                return None;
            }
            let mut local = SYSTEMTIME::default();
            SystemTimeToTzSpecificLocalTime(Some(&raw const tzi), &raw const utc, &raw mut local)
                .ok()?;
            return Some((key, local.wHour as u8, local.wMinute as u8));
        }
    }
}

#[cfg(test)]
mod zone_tests {
    /// Real Windows: a zone without daylight saving is UTC+9 (Korea).
    #[test]
    fn korea_is_nine_hours_ahead_of_utc() {
        use windows::Win32::System::SystemInformation::GetSystemTime;
        let (key, h, m) = super::zone_time("Korea Standard Time").expect("zone exists");
        assert_eq!(key, "Korea Standard Time");
        // SAFETY: no preconditions.
        let utc = unsafe { GetSystemTime() };
        let expect = (u32::from(utc.wHour) + 9) % 24;
        // Allow for the minute changing between the two reads.
        let got = u32::from(h) * 60 + u32::from(m);
        let want = expect * 60 + u32::from(utc.wMinute);
        assert!(
            got.abs_diff(want) <= 1 || got.abs_diff(want) >= 24 * 60 - 1,
            "{h}:{m}"
        );
        assert!(super::zone_time("Not A Zone").is_none());
    }
}
