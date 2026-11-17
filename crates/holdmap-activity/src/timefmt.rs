//! Duration and UTC timestamp formatting without the unmaintained `humantime` crate.
//!
//! The accepted duration text is the same set the CLI already documents: a number and a unit
//! (`ns`, `us`, `ms`, `s`, `m`, `h`, `d`), repeated, with or without spaces (`5s`, `1m 30s`).

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Parse a duration such as `200ms` or `1h 2m`.
pub fn parse_duration(input: &str) -> Result<Duration, String> {
    let s = input.trim();
    if s.is_empty() {
        return Err("empty duration".into());
    }
    let mut rest = s;
    let mut total: u128 = 0;
    let mut saw = false;
    while !rest.is_empty() {
        rest = rest.trim_start();
        if rest.is_empty() {
            break;
        }
        let digits = rest.chars().take_while(char::is_ascii_digit).count();
        if digits == 0 {
            return Err(format!("invalid duration: {input}"));
        }
        let n: u128 = rest[..digits]
            .parse()
            .map_err(|_| format!("invalid duration: {input}"))?;
        rest = &rest[digits..];
        let unit = if rest.starts_with("µs") || rest.starts_with("μs") {
            rest = &rest[rest.chars().next().map(char::len_utf8).unwrap_or(0) + 1..];
            "us"
        } else {
            let letters = rest.chars().take_while(char::is_ascii_alphabetic).count();
            if letters == 0 {
                return Err(format!("invalid duration: {input}"));
            }
            let unit = &rest[..letters];
            rest = &rest[letters..];
            unit
        };
        let mult: u128 = match unit {
            "ns" => 1,
            "us" | "µs" | "μs" => 1_000,
            "ms" => 1_000_000,
            "s" => 1_000_000_000,
            "m" => 60 * 1_000_000_000,
            "h" => 3_600 * 1_000_000_000,
            "d" => 86_400 * 1_000_000_000,
            _ => return Err(format!("invalid duration: {input}")),
        };
        total = total
            .checked_add(
                n.checked_mul(mult)
                    .ok_or_else(|| format!("invalid duration: {input}"))?,
            )
            .ok_or_else(|| format!("invalid duration: {input}"))?;
        saw = true;
    }
    if !saw {
        return Err(format!("invalid duration: {input}"));
    }
    u64::try_from(total)
        .map(Duration::from_nanos)
        .map_err(|_| format!("invalid duration: {input}"))
}

/// `5s`, `200ms`, `1m 30s`. Zero is `0s`.
pub fn format_duration(d: Duration) -> String {
    let mut ns = d.as_nanos();
    if ns == 0 {
        return "0s".into();
    }
    const UNITS: &[(&str, u128)] = &[
        ("d", 86_400 * 1_000_000_000),
        ("h", 3_600 * 1_000_000_000),
        ("m", 60 * 1_000_000_000),
        ("s", 1_000_000_000),
        ("ms", 1_000_000),
        ("us", 1_000),
        ("ns", 1),
    ];
    let mut parts = Vec::new();
    for (name, size) in UNITS {
        if ns >= *size {
            let n = ns / size;
            ns %= size;
            parts.push(format!("{n}{name}"));
        }
    }
    parts.join(" ")
}

/// `YYYY-MM-DDTHH:MM:SSZ`, UTC, whole seconds.
pub fn format_rfc3339_seconds(t: SystemTime) -> String {
    let secs = t
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (y, m, d, hh, mm, ss) = civil_utc(secs);
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
}

/// Parse an RFC3339 timestamp (`Z` or a numeric offset, optional fraction) into UTC.
pub fn parse_rfc3339(input: &str) -> Result<SystemTime, String> {
    let s = input.trim();
    let (date, rest) = s
        .split_once(['T', 't', ' '])
        .ok_or_else(|| format!("invalid timestamp: {input}"))?;
    let mut date_parts = date.split('-');
    let year: i32 = date_parts
        .next()
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| format!("invalid timestamp: {input}"))?;
    let month: u32 = date_parts
        .next()
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| format!("invalid timestamp: {input}"))?;
    let day: u32 = date_parts
        .next()
        .and_then(|v| v.parse().ok())
        .filter(|_| date_parts.next().is_none())
        .ok_or_else(|| format!("invalid timestamp: {input}"))?;
    let (time, offset) = split_offset(rest).ok_or_else(|| format!("invalid timestamp: {input}"))?;
    let (hms, frac) = time
        .split_once('.')
        .map(|(a, b)| (a, Some(b)))
        .unwrap_or((time, None));
    let mut time_parts = hms.split(':');
    let hour: u32 = time_parts
        .next()
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| format!("invalid timestamp: {input}"))?;
    let minute: u32 = time_parts
        .next()
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| format!("invalid timestamp: {input}"))?;
    let second: u32 = time_parts
        .next()
        .and_then(|v| v.parse().ok())
        .filter(|_| time_parts.next().is_none())
        .ok_or_else(|| format!("invalid timestamp: {input}"))?;
    let mut nanos: u32 = 0;
    if let Some(frac) = frac {
        if frac.is_empty() || !frac.chars().all(|c| c.is_ascii_digit()) || frac.len() > 9 {
            return Err(format!("invalid timestamp: {input}"));
        }
        nanos = frac.parse::<u32>().unwrap_or(0) * 10u32.pow(9 - frac.len() as u32);
    }
    if !(1..=12).contains(&month)
        || day == 0
        || day > days_in_month(year, month)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return Err(format!("invalid timestamp: {input}"));
    }
    let days = days_from_civil(year, month, day);
    let mut secs = days * 86_400
        + i64::from(hour) * 3_600
        + i64::from(minute) * 60
        + i64::from(second.min(59));
    secs -= offset;
    if secs < 0 {
        return Err(format!("invalid timestamp: {input}"));
    }
    UNIX_EPOCH
        .checked_add(Duration::new(secs as u64, nanos))
        .ok_or_else(|| format!("invalid timestamp: {input}"))
}

fn split_offset(rest: &str) -> Option<(&str, i64)> {
    if let Some(time) = rest.strip_suffix(['Z', 'z']) {
        return Some((time, 0));
    }
    let (time, sign, off) = if let Some(i) = rest.rfind('+') {
        (&rest[..i], 1, &rest[i + 1..])
    } else {
        let i = rest.rfind('-')?;
        (&rest[..i], -1, &rest[i + 1..])
    };
    let (hh, mm) = off
        .split_once(':')
        .or_else(|| (off.len() == 4).then(|| (&off[..2], &off[2..])))?;
    let hours: i64 = hh.parse().ok()?;
    let mins: i64 = mm.parse().ok()?;
    if hours > 23 || mins > 59 {
        return None;
    }
    Some((time, sign * (hours * 3_600 + mins * 60)))
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap(year) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

fn is_leap(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

/// Days since 1970-01-01 for a civil UTC date (Howard Hinnant).
fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let m = i64::from(month);
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe as i64 * 365 + yoe as i64 / 4 - yoe as i64 / 100 + doy;
    i64::from(era) * 146_097 + doe - 719_468
}

fn civil_utc(secs: i64) -> (i32, u32, u32, u32, u32, u32) {
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400) as u32;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i32 + era as i32 * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m as u32, d as u32, tod / 3600, (tod / 60) % 60, tod % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_round_trip_the_cli_forms() {
        assert_eq!(parse_duration("5s").unwrap(), Duration::from_secs(5));
        assert_eq!(parse_duration("200ms").unwrap(), Duration::from_millis(200));
        assert_eq!(parse_duration("1m 30s").unwrap(), Duration::from_secs(90));
        assert_eq!(format_duration(Duration::from_secs(5)), "5s");
        assert_eq!(format_duration(Duration::from_millis(200)), "200ms");
        assert_eq!(format_duration(Duration::from_secs(90)), "1m 30s");
        assert!(parse_duration("soon").is_err());
    }

    #[test]
    fn rfc3339_keeps_fractional_seconds_and_offsets() {
        let t = parse_rfc3339("2026-10-11T12:00:00.123Z").unwrap();
        let ms = t.duration_since(UNIX_EPOCH).unwrap().as_millis();
        assert_eq!(ms % 1000, 123);
        assert_eq!(format_rfc3339_seconds(t), "2026-10-11T12:00:00Z");
        let shifted = parse_rfc3339("2026-10-11T14:00:00+02:00").unwrap();
        assert_eq!(format_rfc3339_seconds(shifted), "2026-10-11T12:00:00Z");
    }
}
