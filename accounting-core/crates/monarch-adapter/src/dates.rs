//! Civil dates for the void window. A pull covers an observation only when
//! its own date span includes the seven days before and the seven days after
//! that observation's date.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Coverage {
    pub first: String,
    pub last: String,
}

pub fn span<'a>(dates: impl Iterator<Item = &'a str>) -> Option<Coverage> {
    let mut first: Option<&str> = None;
    let mut last: Option<&str> = None;
    for date in dates {
        first = Some(match first {
            Some(current) if current <= date => current,
            _ => date,
        });
        last = Some(match last {
            Some(current) if current >= date => current,
            _ => date,
        });
    }
    Some(Coverage {
        first: first?.to_string(),
        last: last?.to_string(),
    })
}

/// True when `coverage` includes `[occurred - 7 days, occurred + 7 days]`.
pub fn covers(coverage: &Coverage, occurred: &str) -> bool {
    let Some(start) = shift_days(occurred, -7) else {
        return false;
    };
    let Some(end) = shift_days(occurred, 7) else {
        return false;
    };
    coverage.first.as_str() <= start.as_str() && coverage.last.as_str() >= end.as_str()
}

pub fn shift_days(date: &str, delta: i32) -> Option<String> {
    let (year, month, day) = parse(date)?;
    let shifted = days_from_civil(year, month, day).checked_add(delta)?;
    let (year, month, day) = civil_from_days(shifted);
    Some(format!("{year:04}-{month:02}-{day:02}"))
}

fn parse(date: &str) -> Option<(i32, u32, u32)> {
    let bytes = date.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let year: i32 = date[0..4].parse().ok()?;
    let month: u32 = date[5..7].parse().ok()?;
    let day: u32 = date[8..10].parse().ok()?;
    if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
        return None;
    }
    Some((year, month, day))
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_from_civil(mut year: i32, month: u32, day: u32) -> i32 {
    year -= i32::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = (year - era * 400) as u32;
    let month_prime = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * month_prime + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe as i32 - 719468
}

fn civil_from_days(days: i32) -> (i32, u32, u32) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i32 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seven_days_crosses_months_and_a_leap_day() {
        assert_eq!(shift_days("2025-10-15", -7).as_deref(), Some("2025-10-08"));
        assert_eq!(shift_days("2025-10-15", 7).as_deref(), Some("2025-10-22"));
        assert_eq!(shift_days("2025-03-01", -1).as_deref(), Some("2025-02-28"));
        assert_eq!(shift_days("2024-03-01", -1).as_deref(), Some("2024-02-29"));
        assert_eq!(shift_days("2025-01-03", -7).as_deref(), Some("2024-12-27"));
    }

    #[test]
    fn a_narrow_span_does_not_cover_the_window() {
        let narrow = Coverage {
            first: "2025-10-14".to_string(),
            last: "2025-10-16".to_string(),
        };
        assert!(!covers(&narrow, "2025-10-15"));
        let wide = Coverage {
            first: "2025-10-01".to_string(),
            last: "2025-10-31".to_string(),
        };
        assert!(covers(&wide, "2025-10-15"));
        assert!(!covers(&wide, "2025-10-01"));
    }
}
