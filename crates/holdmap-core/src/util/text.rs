//! Time and human-readable text helpers.

#[allow(unused_imports)]
use super::*;
#[allow(unused_imports)]
use std::io::Read;
#[allow(unused_imports)]
use std::process::{Command, Stdio};
#[allow(unused_imports)]
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Current Unix time in milliseconds.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Current Unix time in seconds.
pub fn now_secs() -> u64 {
    now_ms() / 1000
}

/// "3h 12m", "45s", "2d 4h".
pub fn human_duration(secs: u64) -> String {
    let (d, h, m, s) = (
        secs / 86_400,
        (secs % 86_400) / 3600,
        (secs % 3600) / 60,
        secs % 60,
    );
    if d > 0 {
        format!("{d}d {h}h")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else if m > 0 {
        format!("{m}m {s}s")
    } else {
        format!("{s}s")
    }
}

/// `1 process`, `2 processes`: a count with the right noun form.
pub fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// "12.3 MB".
pub fn human_bytes(b: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{b} B")
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

/// The URL a listener is most likely served on (`http://localhost:PORT`, https for 443/8443).
pub fn local_url(port: u16) -> String {
    let scheme = if matches!(port, 443 | 8443) {
        "https"
    } else {
        "http"
    };
    format!("{scheme}://localhost:{port}")
}

/// Text from outside holdmap (process names, command lines, HTTP titles and headers,
/// container names) made safe to print: control characters such as ESC can't reach the
/// terminal, so a process or a local web page can't rewrite the screen or the title bar.
/// Whitespace controls become spaces; anything else becomes `�`.
pub fn printable(s: &str) -> std::borrow::Cow<'_, str> {
    if !s.chars().any(char::is_control) {
        return std::borrow::Cow::Borrowed(s);
    }
    std::borrow::Cow::Owned(
        s.chars()
            .map(|c| match c {
                '\t' | '\n' | '\r' => ' ',
                c if c.is_control() => '\u{FFFD}',
                c => c,
            })
            .collect(),
    )
}

/// Validate an exact systemd unit argument, preserving canonical `\xNN` name escapes while
/// refusing option prefixes and glob patterns that could select unrelated units.
pub(crate) fn exact_systemd_unit(unit: &str, suffix: &str) -> bool {
    if unit.len() > 255
        || unit.starts_with('-')
        || !unit.ends_with(suffix)
        || unit.len() <= suffix.len()
    {
        return false;
    }
    let bytes = unit.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\' {
            if bytes.get(index + 1) != Some(&b'x')
                || !bytes.get(index + 2).is_some_and(u8::is_ascii_hexdigit)
                || !bytes.get(index + 3).is_some_and(u8::is_ascii_hexdigit)
            {
                return false;
            }
            index += 4;
        } else if bytes[index].is_ascii_alphanumeric() || b"_-.:@".contains(&bytes[index]) {
            index += 1;
        } else {
            return false;
        }
    }
    true
}

/// `YYYY-MM-DD HH:MM` in local time (UTC with a `Z` suffix where local time isn't available).
pub fn local_datetime(epoch_secs: u64) -> String {
    #[cfg(unix)]
    {
        #[allow(deprecated)]
        let t = epoch_secs as libc::time_t;
        // SAFETY: localtime_r writes into the provided, properly sized `tm`.
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        if !unsafe { libc::localtime_r(&t, &mut tm) }.is_null() {
            return format!(
                "{:04}-{:02}-{:02} {:02}:{:02}",
                tm.tm_year + 1900,
                tm.tm_mon + 1,
                tm.tm_mday,
                tm.tm_hour,
                tm.tm_min
            );
        }
    }
    // Civil date from days since the epoch (Howard Hinnant's algorithm).
    let days = (epoch_secs / 86_400) as i64;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    let s = epoch_secs % 86_400;
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}Z", s / 3600, (s / 60) % 60)
}

/// The local hour (0–23) of a Unix time (UTC where local time isn't available).
pub fn local_hour(epoch_secs: u64) -> u8 {
    local_datetime(epoch_secs)
        .get(11..13)
        .and_then(|h| h.parse().ok())
        .unwrap_or(0)
}

/// The last millisecond of a `YYYY-MM-DD` date in local time (UTC where local time isn't
/// available), as Unix epoch ms. `None` for anything that isn't a real date.
pub fn local_date_end_ms(date: &str) -> Option<u64> {
    let mut parts = date.trim().splitn(3, '-');
    let (y, m, d): (i64, i64, i64) = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    );
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days_in = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if !(1970..=9999).contains(&y)
        || !(1..=12).contains(&m)
        || d < 1
        || d > days_in[(m - 1) as usize]
    {
        return None;
    }
    #[cfg(unix)]
    {
        // SAFETY: mktime reads and normalizes the zero-initialized `tm` we own.
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        tm.tm_year = (y - 1900) as libc::c_int;
        tm.tm_mon = (m - 1) as libc::c_int;
        tm.tm_mday = d as libc::c_int;
        tm.tm_hour = 23;
        tm.tm_min = 59;
        tm.tm_sec = 59;
        tm.tm_isdst = -1;
        let t = unsafe { libc::mktime(&mut tm) };
        if t >= 0 {
            return Some(t as u64 * 1000 + 999);
        }
    }
    // Days since the epoch (Howard Hinnant's algorithm), UTC.
    let (y2, m2) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let doy = (153 * m2 + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some((days as u64 + 1) * 86_400_000 - 1)
}

/// `HH:MM:SS` in local time (UTC with a `Z` suffix where local time isn't available).
pub fn local_hms(epoch_secs: u64) -> String {
    #[cfg(unix)]
    {
        // musl deprecates the `time_t` alias (it is moving to 64 bits); it is still exactly the
        // type `localtime_r` takes on every libc.
        #[allow(deprecated)]
        let t = epoch_secs as libc::time_t;
        // SAFETY: localtime_r writes into the provided, properly sized `tm`.
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        if !unsafe { libc::localtime_r(&t, &mut tm) }.is_null() {
            return format!("{:02}:{:02}:{:02}", tm.tm_hour, tm.tm_min, tm.tm_sec);
        }
    }
    let s = epoch_secs % 86_400;
    format!("{:02}:{:02}:{:02}Z", s / 3600, (s / 60) % 60, s % 60)
}

#[cfg(test)]
mod date_tests {
    use super::local_date_end_ms;

    #[test]
    fn parses_real_dates_only() {
        assert!(local_date_end_ms("2026-12-31").is_some());
        assert!(local_date_end_ms("2028-02-29").is_some());
        for bad in [
            "2026-02-29",
            "2026-13-01",
            "2026-1",
            "soon",
            "1969-12-31",
            "2026-00-10",
        ] {
            assert_eq!(local_date_end_ms(bad), None, "{bad}");
        }
        let a = local_date_end_ms("2026-12-30").unwrap();
        let b = local_date_end_ms("2026-12-31").unwrap();
        assert_eq!(b - a, 86_400_000);
    }
}
