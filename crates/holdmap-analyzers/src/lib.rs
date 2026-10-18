//! Opt-in analyzers for holdmap.
//!
//! An analyzer is a scanner the user picks (a local CLI like SkillSpector, or the built-in OSV
//! lookup). Nothing runs until the user approves it on a permissions sheet that shows exactly
//! what it gets; approval is pinned to the manifest and program, and any change asks again.
//! Runs get a staged copy of the subject, a cleared environment and hard limits; output is
//! reduced to capped, secret-masked findings. holdmap never starts MCP servers on its own: an
//! MCP analyzer the user registered starts its server only for a run the user started.

pub mod catalog;
pub mod cli_runner;
pub mod consent;
pub mod finding;
pub mod input;
#[cfg(target_os = "linux")]
pub mod landlock;
pub mod manifest;
#[cfg(feature = "mcp-http")]
pub mod mcp_http;
pub mod mcp_map;
pub mod mcp_runner;
pub mod mcp_tools;
pub mod needs;
pub mod osv;
pub mod parse;
pub mod probe;
pub mod runner;
pub mod sandbox;
pub mod secrets;
pub mod service;
pub mod store;

pub use consent::{
    ConsentStore, Grant, MemoryStore, ResultStore, Review, ReviewStore, SecretIndex,
};
pub use finding::{
    AnalyzerResult, Detail, Finding, FindingState, Location, RunStatus, Severity, Subject,
    SubjectKind,
};
pub use input::{AnalyzerInput, InputProjector, InventoryProjector, PackageCoord};
pub use manifest::{AnalyzerManifest, ManifestError};
pub use parse::OutputParser;
pub use runner::{AnalyzerRunner, BasicSandbox, Containment, Sandbox, SandboxLevel};
pub use service::{
    AnalyzerService, AnalyzerState, AnalyzerView, Insights, Preview, SeverityCounts, Stores,
};
pub use store::FileStore;
