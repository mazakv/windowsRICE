//! Formatting of clock, date, percentages and byte sizes.

use crate::strings::Lang;

/// Local wall-clock time (supplied by the platform layer).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    /// 0 = Sunday … 6 = Saturday.
    pub weekday: u8,
    pub hour: u8,
    pub minute: u8,
}

const WEEKDAYS_KO_SHORT: [&str; 7] = ["일", "월", "화", "수", "목", "금", "토"];
const WEEKDAYS_KO: [&str; 7] = [
    "일요일",
    "월요일",
    "화요일",
    "수요일",
    "목요일",
    "금요일",
    "토요일",
];
const WEEKDAYS_EN_SHORT: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const WEEKDAYS_EN: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];
const MONTHS_EN_SHORT: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const MONTHS_EN: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

fn wd(t: &LocalTime) -> usize {
    usize::from(t.weekday % 7)
}

fn mi(t: &LocalTime) -> usize {
    usize::from(t.month.clamp(1, 12) - 1)
}

/// Displayed time, 24-hour ("15:42").
pub fn time_text(t: &LocalTime) -> String {
    format!("{:02}:{:02}", t.hour, t.minute)
}

/// 12-hour time ("3:42 PM" / "오후 3:42").
pub fn time_text_12(t: &LocalTime, lang: Lang) -> String {
    let h = match t.hour % 12 {
        0 => 12,
        h => h,
    };
    let pm = t.hour >= 12;
    match lang {
        Lang::En => format!("{h}:{:02} {}", t.minute, if pm { "PM" } else { "AM" }),
        Lang::Ko => format!("{} {h}:{:02}", if pm { "오후" } else { "오전" }, t.minute),
    }
}

/// Displayed date ("11월 19일 (수)" / "Wed, Nov 19").
pub fn date_text(t: &LocalTime, lang: Lang) -> String {
    match lang {
        Lang::Ko => format!("{}월 {}일 ({})", t.month, t.day, WEEKDAYS_KO_SHORT[wd(t)]),
        Lang::En => format!(
            "{}, {} {}",
            WEEKDAYS_EN_SHORT[wd(t)],
            MONTHS_EN_SHORT[mi(t)],
            t.day
        ),
    }
}

/// Spoken form for assistive technology, without abbreviations.
pub fn clock_accessible(t: &LocalTime, lang: Lang) -> String {
    match lang {
        Lang::Ko => format!(
            "시계: {}시 {}분, {}년 {}월 {}일 {}",
            t.hour,
            t.minute,
            t.year,
            t.month,
            t.day,
            WEEKDAYS_KO[wd(t)]
        ),
        Lang::En => format!(
            "Clock: {:02}:{:02}, {}, {} {}, {}",
            t.hour,
            t.minute,
            WEEKDAYS_EN[wd(t)],
            MONTHS_EN[mi(t)],
            t.day,
            t.year
        ),
    }
}

/// Rounds a 0–100 percentage for display.
pub fn percent(v: f32) -> u8 {
    v.clamp(0.0, 100.0).round() as u8
}

/// Gibibytes with one decimal, labelled "GB" as Windows does.
pub fn gigabytes(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: LocalTime = LocalTime {
        year: 2026,
        month: 11,
        day: 19,
        weekday: 4,
        hour: 9,
        minute: 5,
    };

    #[test]
    fn korean() {
        assert_eq!(time_text(&T), "09:05");
        assert_eq!(date_text(&T, Lang::Ko), "11월 19일 (목)");
        assert_eq!(
            clock_accessible(&T, Lang::Ko),
            "시계: 9시 5분, 2026년 11월 19일 목요일"
        );
    }

    #[test]
    fn english() {
        assert_eq!(date_text(&T, Lang::En), "Thu, Nov 19");
        assert_eq!(
            clock_accessible(&T, Lang::En),
            "Clock: 09:05, Thursday, November 19, 2026"
        );
    }

    #[test]
    fn numbers() {
        assert_eq!(percent(12.5), 13);
        assert_eq!(percent(-3.0), 0);
        assert_eq!(percent(250.0), 100);
        assert_eq!(gigabytes(34_359_738_368), "32.0 GB");
    }
}
