//! Homebrew services (launchd agents) on macOS: `brew services stop`.

use super::{Resolution, ResolveCtx, StopStrategy};
use crate::engine::text::process_details;
use crate::engine::types::{Owner, Step, Supervisor};
use crate::model::{PortEntry, ProcessInfo};

/// Stops `brew services` formulae through Homebrew instead of killing the daemon.
pub struct BrewServiceStrategy;

/// Homebrew formula for a launchd-started process under `/opt/homebrew` or `/usr/local`.
fn brew_formula(p: &ProcessInfo) -> Option<String> {
    if p.ppid != Some(1) {
        return None;
    }
    let exe = p.exe.as_ref()?.to_string_lossy().into_owned();
    let formula = formula_from_exe(&exe)?;
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from)?;
    let base = formula.split('@').next().unwrap_or(&formula).to_string();
    [formula.clone(), base].into_iter().find(|f| {
        let plist = format!("homebrew.mxcl.{f}.plist");
        home.join("Library/LaunchAgents").join(&plist).exists()
            || std::path::Path::new("/Library/LaunchDaemons")
                .join(&plist)
                .exists()
    })
}

/// `/opt/homebrew/Cellar/postgresql@16/16.2/bin/postgres` → `postgresql@16`.
pub fn formula_from_exe(exe: &str) -> Option<String> {
    Some(
        exe.split("/Cellar/")
            .nth(1)
            .or_else(|| exe.split("/opt/homebrew/opt/").nth(1))
            .or_else(|| exe.split("/usr/local/opt/").nth(1))?
            .split('/')
            .next()?
            .to_string(),
    )
}

impl StopStrategy for BrewServiceStrategy {
    fn name(&self) -> &'static str {
        "brew-service"
    }

    fn resolve(&self, _ctx: &ResolveCtx, e: &PortEntry, details: &[String]) -> Option<Resolution> {
        if !cfg!(target_os = "macos") {
            return None;
        }
        let p = e.process.as_ref()?;
        let formula = brew_formula(p)?;
        let mut r = Resolution::new(
            Owner::Supervised {
                supervisor: Supervisor::BrewService { formula: formula.clone() },
                pid: p.pid,
                name: p.name.clone(),
            },
            format!("Port {} is held by {} (PID {}), started by `brew services` ({formula}). launchd would restart it if killed.", e.port, p.name, p.pid),
        );
        let mut d = details.to_vec();
        d.extend(process_details(p, e));
        r.details = d;
        r.recommendation = format!("Stop the service: `brew services stop {formula}`.");
        r.commands.push(format!("brew services stop {formula}"));
        r.steps.push(Step::RunCommand {
            program: "brew".into(),
            args: vec!["services".into(), "stop".into(), formula],
            reason: "stop the Homebrew service".into(),
        });
        Some(r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formula_parsing() {
        assert_eq!(
            formula_from_exe("/opt/homebrew/Cellar/postgresql@16/16.2/bin/postgres").as_deref(),
            Some("postgresql@16")
        );
        assert_eq!(
            formula_from_exe("/usr/local/opt/redis/bin/redis-server").as_deref(),
            Some("redis")
        );
        assert_eq!(formula_from_exe("/usr/bin/node"), None);
    }
}
