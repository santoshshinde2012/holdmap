//! Whether the machine runs on battery, read from the OS without privileges. `None` when it
//! can't tell (no battery, or an unsupported system).

/// `Some(true)` on battery, `Some(false)` on mains power, `None` when unknown.
pub fn on_battery() -> Option<bool> {
    imp::on_battery()
}

/// One `/sys/class/power_supply` entry: its `type`, `online` and `status` files.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Supply {
    /// `Mains`, `Battery`, `USB`…
    pub kind: String,
    /// Whether a mains or USB supply is connected.
    pub online: Option<bool>,
    /// A battery's status, e.g. `Discharging`.
    pub status: Option<String>,
}

/// Decide from Linux supplies: any online mains means plugged in; otherwise a discharging
/// battery, or a battery with every mains supply offline, means on battery.
pub fn from_supplies(supplies: &[Supply]) -> Option<bool> {
    let mains: Vec<_> = supplies
        .iter()
        .filter(|s| s.kind == "Mains" || s.kind == "USB" || s.kind == "USB_C")
        .collect();
    if mains.iter().any(|s| s.online == Some(true)) {
        return Some(false);
    }
    let batteries: Vec<_> = supplies.iter().filter(|s| s.kind == "Battery").collect();
    if batteries.is_empty() {
        return None;
    }
    if batteries
        .iter()
        .any(|b| b.status.as_deref() == Some("Discharging"))
        || (!mains.is_empty() && mains.iter().all(|s| s.online == Some(false)))
    {
        return Some(true);
    }
    None
}

/// Decide from `pmset -g batt` output (macOS).
pub fn from_pmset(out: &str) -> Option<bool> {
    let first = out.lines().next()?;
    if first.contains("'AC Power'") {
        Some(false)
    } else if first.contains("'Battery Power'") {
        Some(true)
    } else {
        None
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use super::{from_supplies, Supply};
    use std::fs;

    pub fn on_battery() -> Option<bool> {
        let read = |p: std::path::PathBuf| fs::read_to_string(p).ok().map(|s| s.trim().to_string());
        let supplies: Vec<Supply> = fs::read_dir("/sys/class/power_supply")
            .ok()?
            .flatten()
            .take(32)
            .map(|e| {
                let dir = e.path();
                Supply {
                    kind: read(dir.join("type")).unwrap_or_default(),
                    online: read(dir.join("online")).map(|v| v == "1"),
                    status: read(dir.join("status")),
                }
            })
            .collect();
        from_supplies(&supplies)
    }
}

#[cfg(target_os = "macos")]
mod imp {
    pub fn on_battery() -> Option<bool> {
        let out = std::process::Command::new("/usr/bin/pmset")
            .args(["-g", "batt"])
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .ok()?;
        super::from_pmset(&String::from_utf8_lossy(&out.stdout))
    }
}

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};

    pub fn on_battery() -> Option<bool> {
        // SAFETY: GetSystemPowerStatus fills the zero-initialized struct we own.
        let mut status: SYSTEM_POWER_STATUS = unsafe { std::mem::zeroed() };
        if unsafe { GetSystemPowerStatus(&mut status) } == 0 {
            return None;
        }
        match status.ACLineStatus {
            0 => Some(true),
            1 => Some(false),
            _ => None,
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
mod imp {
    pub fn on_battery() -> Option<bool> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(kind: &str, online: Option<bool>, status: Option<&str>) -> Supply {
        Supply {
            kind: kind.into(),
            online,
            status: status.map(Into::into),
        }
    }

    #[test]
    fn linux_supplies() {
        assert_eq!(from_supplies(&[]), None, "desktop without supplies");
        assert_eq!(
            from_supplies(&[
                s("Mains", Some(true), None),
                s("Battery", None, Some("Discharging"))
            ]),
            Some(false)
        );
        assert_eq!(
            from_supplies(&[
                s("Mains", Some(false), None),
                s("Battery", None, Some("Full"))
            ]),
            Some(true)
        );
        assert_eq!(
            from_supplies(&[s("Battery", None, Some("Discharging"))]),
            Some(true)
        );
        assert_eq!(from_supplies(&[s("Battery", None, Some("Unknown"))]), None);
    }

    #[test]
    fn macos_pmset() {
        assert_eq!(
            from_pmset("Now drawing from 'AC Power'\n -InternalBattery-0 100%"),
            Some(false)
        );
        assert_eq!(from_pmset("Now drawing from 'Battery Power'\n"), Some(true));
        assert_eq!(from_pmset("Now drawing from 'UPS Power'"), None);
        assert_eq!(from_pmset(""), None);
    }

    #[test]
    fn never_panics_here() {
        let _ = on_battery();
    }
}
