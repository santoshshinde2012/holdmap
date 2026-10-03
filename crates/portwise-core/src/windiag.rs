//! Windows-specific diagnostics (parsers are platform-independent so they're tested everywhere).

/// An excluded (reserved) port range from `netsh interface ipv4 show excludedportrange`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExcludedRange {
    /// First port of the range.
    pub start: u16,
    /// Last port of the range (inclusive).
    pub end: u16,
    /// Marked with `*` (administered exclusion, added by an admin via netsh).
    pub administered: bool,
}

impl ExcludedRange {
    /// True when `port` is inside the range.
    pub fn contains(&self, port: u16) -> bool {
        (self.start..=self.end).contains(&port)
    }
}

/// Parse `netsh interface ipv4 show excludedportrange protocol=tcp` output.
pub fn parse_excluded_ranges(text: &str) -> Vec<ExcludedRange> {
    text.lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let start = it.next()?.parse().ok()?;
            let end = it.next()?.parse().ok()?;
            Some(ExcludedRange {
                start,
                end,
                administered: it.next() == Some("*"),
            })
        })
        .collect()
}

/// Query excluded port ranges (Windows only; empty elsewhere or on failure).
pub fn excluded_ranges(protocol: crate::model::Protocol) -> Vec<ExcludedRange> {
    if !cfg!(windows) {
        return Vec::new();
    }
    let proto = match protocol {
        crate::model::Protocol::Tcp => "protocol=tcp",
        crate::model::Protocol::Udp => "protocol=udp",
    };
    let mut out = Vec::new();
    for fam in ["ipv4", "ipv6"] {
        if let Some(o) = crate::util::run_with_timeout(
            "netsh",
            &["interface", fam, "show", "excludedportrange", proto],
            std::time::Duration::from_secs(3),
        ) {
            for r in parse_excluded_ranges(&o.stdout) {
                if !out.contains(&r) {
                    out.push(r);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_netsh_output() {
        let text = include_str!("../tests/fixtures/netsh_excludedportrange.txt");
        let r = parse_excluded_ranges(text);
        assert_eq!(r.len(), 3);
        assert_eq!(
            r[0],
            ExcludedRange {
                start: 5357,
                end: 5357,
                administered: false
            }
        );
        assert!(r[2].administered);
        assert!(r[1].contains(50010));
        assert!(!r[1].contains(49999));
    }
}
