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
