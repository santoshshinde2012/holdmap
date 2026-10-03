#![warn(missing_docs)]
//! # portwise-core
//!
//! The engine behind every portwise surface: it lists which ports are in use and by whom,
//! explains *why* a port is busy in plain English, and stops the right thing safely.
//!
//! ```no_run
//! use portwise_core::{execute, Engine, ScanOptions, StopOptions, Target};
//!
//! let engine = Engine::new(&ScanOptions::default())?;
//! for e in &engine.snapshot().entries {
//!     println!("{} {} {}", e.port, e.protocol, e.label);
//! }
//! let explanation = engine.explain(3000, &StopOptions::default());
//! println!("{}", explanation.headline);
//! let plan = engine.plan(&Target::Port(3000), &StopOptions::default());
//! if !plan.is_blocked() {
//!     let report = execute(&plan, &mut |line| eprintln!("{line}"));
//!     assert!(report.freed);
//! }
//! # Ok::<(), std::io::Error>(())
//! ```

pub mod docker;
pub mod engine;
pub mod events;
pub mod exec;
pub mod history;
pub mod model;
pub mod probe;
pub mod process;
pub mod project;
pub mod provider;
pub mod remote;
pub mod safety;
pub mod scan;
pub mod store;
pub mod sys;
pub mod topology;
pub mod tunnel;
pub mod util;
pub mod windiag;

pub use engine::{
    port_busy, tilde, ActionPlan, BlockKind, Blocked, Engine, Explanation, Owner, PortStatus,
    ProcRef, Risk, Step, StopOptions, StopReport, Supervisor, Target,
};
pub use exec::execute;
pub use model::*;
pub use probe::{ephemeral_port, probe_tcp, probe_udp, tcp_accepting, ProbeResult};
pub use process::ProcessTable;
pub use safety::{DefaultProtectionPolicy, ProtectionPolicy};
pub use scan::{parse_range, scan, Filter, Scan, ScanOptions, Scanner};

/// Version of the core library.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
