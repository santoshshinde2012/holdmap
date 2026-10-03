//! Stop strategies: one small type per kind of owner (Strategy + Chain of Responsibility).
//!
//! The [`StrategyRegistry`] asks each [`StopStrategy`] in order; the first one that recognises the
//! owner of a port produces the [`Resolution`] (explanation + plan steps). New owner kinds are
//! added by registering a new strategy, without touching the engine (open/closed).

mod brew;
mod container;
mod hidden;
mod os_service;
mod other_user;
mod pm2;
mod process_tree;
mod protected;
mod systemd;

pub use brew::BrewServiceStrategy;
pub use container::ContainerStrategy;
pub use hidden::HiddenOwnerStrategy;
pub use os_service::OsServiceStrategy;
pub use other_user::OtherUserStrategy;
pub use pm2::Pm2Strategy;
pub use process_tree::ProcessTreeStrategy;
pub use protected::ProtectedStrategy;
pub use systemd::parse_list_sockets;
pub use systemd::SystemdStrategy;

use super::context::ResolveCtx;
use super::resolution::Resolution;
use crate::model::{PortEntry, ProcessInfo};
use std::sync::Arc;

/// Recognises one kind of port owner and decides how to stop it.
pub trait StopStrategy: Send + Sync {
    /// Short identifier, e.g. `"container"`.
    fn name(&self) -> &'static str;

    /// Resolve the owner of `entry`, or `None` to let the next strategy try.
    /// `details` holds facts already gathered (e.g. the exposure line).
    fn resolve(
        &self,
        ctx: &ResolveCtx,
        entry: &PortEntry,
        details: &[String],
    ) -> Option<Resolution>;
}

/// Ordered list of strategies. The last one should always answer.
#[derive(Clone)]
pub struct StrategyRegistry {
    strategies: Vec<Arc<dyn StopStrategy>>,
}

impl std::fmt::Debug for StrategyRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list()
            .entries(self.strategies.iter().map(|s| s.name()))
            .finish()
    }
}

impl Default for StrategyRegistry {
    /// Containers → hidden owners → OS services → supervisors → protection → other users →
    /// plain process trees. Order matters: e.g. a pm2 app must be stopped through pm2 even
    /// though it is also a process tree.
    fn default() -> Self {
        Self::empty()
            .with(ContainerStrategy)
            .with(HiddenOwnerStrategy)
            .with(OsServiceStrategy)
            .with(SystemdStrategy)
            .with(Pm2Strategy)
            .with(BrewServiceStrategy)
            .with(ProtectedStrategy)
            .with(OtherUserStrategy)
            .with(ProcessTreeStrategy)
    }
}

impl StrategyRegistry {
    pub fn empty() -> Self {
        Self {
            strategies: Vec::new(),
        }
    }

    /// Append a strategy (builder style).
    pub fn with(mut self, s: impl StopStrategy + 'static) -> Self {
        self.strategies.push(Arc::new(s));
        self
    }

    /// Insert a strategy ahead of the built-in ones.
    pub fn with_first(mut self, s: impl StopStrategy + 'static) -> Self {
        self.strategies.insert(0, Arc::new(s));
        self
    }

    pub fn names(&self) -> Vec<&'static str> {
        self.strategies.iter().map(|s| s.name()).collect()
    }

    pub fn resolve(&self, ctx: &ResolveCtx, entry: &PortEntry, details: &[String]) -> Resolution {
        self.strategies
            .iter()
            .find_map(|s| s.resolve(ctx, entry, details))
            .or_else(|| HiddenOwnerStrategy.resolve(ctx, entry, details))
            .or_else(|| ProcessTreeStrategy.resolve(ctx, entry, details))
            .unwrap_or_else(|| {
                Resolution::new(
                    super::types::Owner::Free,
                    format!("Port {} has no resolvable owner.", entry.port),
                )
            })
    }
}

/// Launchers whose children are part of the same dev session.
pub fn is_launcher(p: &ProcessInfo) -> bool {
    // npm/yarn/pnpm rename themselves ("npm run dev"), so look at the first word only.
    let full = p.name.to_ascii_lowercase();
    let first = full.split_whitespace().next().unwrap_or("");
    let n = first.strip_suffix(".exe").unwrap_or(first);
    const LAUNCHERS: &[&str] = &[
        "npm",
        "npx",
        "pnpm",
        "pnpx",
        "yarn",
        "bun",
        "bunx",
        "nodemon",
        "turbo",
        "nx",
        "concurrently",
        "cargo",
        "cargo-watch",
        "watchexec",
        "air",
        "reflex",
        "foreman",
        "overmind",
        "honcho",
        "make",
        "just",
        "task",
        "uv",
        "poetry",
        "pipenv",
        "rye",
        "hatch",
        "mix",
        "bundle",
        "dotnet",
        "deno",
        "tsx",
        "ts-node",
        "vite-node",
        "go",
    ];
    if LAUNCHERS.contains(&n) {
        return true;
    }
    let args: Vec<String> = p.cmdline.iter().map(|a| a.to_ascii_lowercase()).collect();
    if n.starts_with("node") {
        const JS_LAUNCHERS: &[&str] = &[
            "npm-cli.js",
            "npx-cli.js",
            "yarn.js",
            "yarn.cjs",
            "pnpm.cjs",
            "pnpm.js",
            "/bin/npm",
            "/bin/yarn",
            "/bin/pnpm",
            "nodemon",
            "concurrently",
            "turbo",
        ];
        return args
            .iter()
            .skip(1)
            .take(2)
            .any(|a| JS_LAUNCHERS.iter().any(|l| a.contains(l)));
    }
    // Non-interactive shells (`sh -c "next dev"`) spawned to run a script.
    const SHELLS: &[&str] = &[
        "sh",
        "bash",
        "zsh",
        "dash",
        "fish",
        "ash",
        "cmd",
        "powershell",
        "pwsh",
    ];
    if SHELLS.contains(&n) {
        return args
            .iter()
            .skip(1)
            .any(|a| a == "-c" || a == "/c" || a == "-command");
    }
    false
}
